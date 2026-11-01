//! Diagnostic sufficient normal-line coverage certificate. No runtime change.
//! The oriented boundary of the canonical triangle chain cancels shared
//! interior edges. Nonzero projected degree proves at least one intersection;
//! zero or any finite ambiguity always leaves the existing face query in charge.
use earth_math::Vec3d;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct BoundaryChain {
    pub anchor: Vec3d,
    pub axes: [Vec3d; 3],
    pub vertices: Vec<Vec3d>,
    /// Canonical endpoint IDs and signed multiplicity, never set deduplication.
    pub edges: Vec<(u32, u32, i64)>,
    pub min: Vec3d,
    pub max: Vec3d,
}
#[derive(Clone, Copy, Debug)]
pub struct CoverageCertificate {
    pub degree: i64,
    pub minimum_boundary_margin_m: f64,
    pub line_parameter_interval_m: [f64; 2],
}

/// A sufficient interior region of the projected canonical boundary chain.
/// This is a preparation-time certificate, not a replacement footprint.
#[derive(Clone, Copy, Debug)]
pub struct InteriorCore {
    pub center: Vec3d,
    pub half_extent: [f64; 2],
    pub boundary_radius: f64,
    pub degree: i64,
}

// Outward binary64 intervals are used only in preparation. In particular,
// a numerically estimated maximal rectangle is NEVER itself a certificate:
// every external segment must pass this independent separating-axis test.
#[derive(Clone, Copy)]
struct I(f64, f64);
impl I {
    fn exact(x: f64) -> Self {
        Self(x, x)
    }
    fn add(self, b: Self) -> Self {
        Self((self.0 + b.0).next_down(), (self.1 + b.1).next_up())
    }
    fn sub(self, b: Self) -> Self {
        Self((self.0 - b.1).next_down(), (self.1 - b.0).next_up())
    }
    fn mul(self, b: Self) -> Self {
        let p = [self.0 * b.0, self.0 * b.1, self.1 * b.0, self.1 * b.1];
        Self(
            p.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            p.into_iter().fold(f64::NEG_INFINITY, f64::max).next_up(),
        )
    }
    fn abs_upper(self) -> f64 {
        self.0.abs().max(self.1.abs())
    }
}

fn rectangle_separates(a: [I; 2], b: [I; 2], half: [f64; 2]) -> bool {
    for j in 0..2 {
        if a[j].0.min(b[j].0) > half[j] || a[j].1.max(b[j].1) < -half[j] {
            return true;
        }
    }
    let d = [b[0].sub(a[0]), b[1].sub(a[1])];
    let cross = a[0].mul(b[1]).sub(a[1].mul(b[0]));
    let support = I::exact(half[0])
        .mul(I::exact(d[1].abs_upper()))
        .add(I::exact(half[1]).mul(I::exact(d[0].abs_upper())))
        .1;
    cross.0 > support || cross.1 < -support
}

impl BoundaryChain {
    pub fn build(vertices: &[Vec3d], triangles: &[[u32; 3]]) -> Option<Self> {
        let anchor = *vertices.first()?;
        if !anchor.is_finite() {
            return None;
        }
        // A near-surface frame keeps the whole-patch height bounds tight.
        // This is only a rigid basis choice; the canonical vertices do not move.
        let up = anchor.normalized().unwrap_or(Vec3d::new(0.0, 0.0, 1.0));
        let candidate = if up.z.abs() < 0.8 {
            Vec3d::new(0.0, 0.0, 1.0)
        } else {
            Vec3d::new(1.0, 0.0, 0.0)
        };
        let east = up.cross(candidate).normalized()?;
        let north = up.cross(east);
        let axes = [east, north, up];
        Self::in_frame(vertices, triangles, anchor, axes)
    }

    /// Prepare in a supplied near-orthonormal rigid patch basis. This lets a
    /// diagnostic use the existing GPU patch frame (east, negative-north, up)
    /// without another per-query transform. Its finite basis/translation
    /// discrepancy remains part of the caller's point/direction/vertex errors.
    pub fn in_frame(
        vertices: &[Vec3d],
        triangles: &[[u32; 3]],
        anchor: Vec3d,
        axes: [Vec3d; 3],
    ) -> Option<Self> {
        if !anchor.is_finite() || axes.iter().any(|v| !v.is_finite()) {
            return None;
        }
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                if (axes[i].dot(axes[j]) - expected).abs() > 1e-12 {
                    return None;
                }
            }
        }
        let project = |v: Vec3d| {
            Vec3d::new(axes[0].dot(v), axes[1].dot(v), axes[2].dot(v))
        };
        let vertices: Vec<_> = vertices.iter().map(|&v| project(v - anchor)).collect();
        if vertices.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let mut counts = BTreeMap::<(u32, u32), i64>::new();
        for &[a, b, c] in triangles {
            if [a, b, c].iter().any(|&i| i as usize >= vertices.len()) {
                return None;
            }
            for (x, y) in [(a, b), (b, c), (c, a)] {
                if x == y {
                    return None;
                }
                let (key, sign) = if x < y { ((x, y), 1) } else { ((y, x), -1) };
                *counts.entry(key).or_default() += sign;
            }
        }
        let edges = counts
            .into_iter()
            .filter_map(|((a, b), count)| (count != 0).then_some((a, b, count)))
            .collect();
        let mut min = Vec3d::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3d::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for v in &vertices {
            min = Vec3d::new(min.x.min(v.x), min.y.min(v.y), min.z.min(v.z));
            max = Vec3d::new(max.x.max(v.x), max.y.max(v.y), max.z.max(v.z));
        }
        Some(Self {
            anchor,
            axes,
            vertices,
            edges,
            min,
            max,
        })
    }
    pub fn vertex_ecef(&self, index: u32) -> Vec3d {
        let v = self.vertices[index as usize];
        self.anchor + self.axes[0] * v.x + self.axes[1] * v.y + self.axes[2] * v.z
    }

    /// Prepare one connected rectangle of nonzero degree in the reference XY
    /// plane. Holes, folds and signed multiplicities are retained. If the box
    /// center has zero degree, no core is offered; ordinary queries remain.
    pub fn interior_core(&self) -> Option<InteriorCore> {
        let center = (self.min + self.max) * 0.5;
        let extent = (self.max - self.min) * 0.5;
        let radius = [extent.x, extent.y];
        if radius.iter().any(|x| !x.is_finite() || *x <= 0.0) {
            return None;
        }
        let mut scale = 1.0_f64;
        let mut degree = 0_i64;
        for &(ai, bi, multiplicity) in &self.edges {
            let a = self.vertices[ai as usize] - center;
            let b = self.vertices[bi as usize] - center;
            let x = [a.x / radius[0], a.y / radius[1]];
            let y = [b.x / radius[0], b.y / radius[1]];
            let d = [y[0] - x[0], y[1] - x[1]];
            let value = |t: f64| (x[0] + t * d[0]).abs().max((x[1] + t * d[1]).abs());
            let mut closest = value(0.0).min(value(1.0));
            // The convex piecewise-linear L-infinity distance has its minimum
            // at an endpoint or one of these absolute-value/maximum kinks.
            for (n, q) in [
                (x[0], d[0]),
                (x[1], d[1]),
                (x[0] - x[1], d[0] - d[1]),
                (x[0] + x[1], d[0] + d[1]),
            ] {
                if q != 0.0 {
                    let t = -n / q;
                    if (0.0..=1.0).contains(&t) {
                        closest = closest.min(value(t));
                    }
                }
            }
            scale = scale.min(closest);
            let av = self.vertices[ai as usize];
            let bv = self.vertices[bi as usize];
            let ia = [
                I::exact(av.x).sub(I::exact(center.x)),
                I::exact(av.y).sub(I::exact(center.y)),
            ];
            let ib = [
                I::exact(bv.x).sub(I::exact(center.x)),
                I::exact(bv.y).sub(I::exact(center.y)),
            ];
            let cross = ia[0].mul(ib[1]).sub(ia[1].mul(ib[0]));
            if av.y <= center.y && bv.y > center.y {
                if cross.0 > 0.0 {
                    degree = degree.checked_add(multiplicity)?;
                } else if cross.1 >= 0.0 {
                    return None;
                }
            }
            if bv.y <= center.y && av.y > center.y {
                if cross.1 < 0.0 {
                    degree = degree.checked_sub(multiplicity)?;
                } else if cross.0 <= 0.0 {
                    return None;
                }
            }
        }
        if degree == 0 || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        // Only a numerical proposal. The outward SAT check below proves the
        // exact represented rectangle is disjoint from every external edge.
        let half_extent = radius.map(|r| (r * scale * (1.0 - 1e-10)).next_down());
        if half_extent.iter().any(|x| *x <= 0.0) {
            return None;
        }
        for &(ai, bi, _) in &self.edges {
            let point = |i: u32| {
                let v = self.vertices[i as usize];
                [
                    I::exact(v.x).sub(I::exact(center.x)),
                    I::exact(v.y).sub(I::exact(center.y)),
                ]
            };
            if !rectangle_separates(point(ai), point(bi), half_extent) {
                return None;
            }
        }
        let component_bound = [self.min.x, self.min.y, self.min.z]
            .into_iter()
            .zip([self.max.x, self.max.y, self.max.z])
            .zip([center.x, center.y, center.z])
            .map(|((low, high), c)| {
                I::exact(low)
                    .sub(I::exact(c))
                    .abs_upper()
                    .max(I::exact(high).sub(I::exact(c)).abs_upper())
            });
        let squared = component_bound.fold(I::exact(0.0), |sum, r| {
            sum.add(I::exact(r).mul(I::exact(r)))
        });
        let boundary_radius = squared.1.sqrt().next_up();
        Some(InteriorCore {
            center,
            half_extent,
            boundary_radius,
            degree,
        })
    }

    /// Constant-work sufficient coverage. A proper rotation aligning the
    /// query normal with reference Z moves any vector by at most delta*|x|.
    /// Eroding the certified core by both boundary and query displacement
    /// preserves its nonzero winding degree. No test here asserts noncoverage.
    pub fn certify_core(
        &self,
        core: InteriorCore,
        point: Vec3d,
        v: Vec3d,
        point_error: f64,
        direction_error: f64,
        vertex_error: f64,
        interval: [f64; 2],
    ) -> Option<CoverageCertificate> {
        if !point.is_finite()
            || !v.is_finite()
            || [point_error, direction_error, vertex_error]
                .iter()
                .any(|x| !x.is_finite() || *x < 0.0)
            || !interval.iter().all(|x| x.is_finite())
            || interval[0] >= interval[1]
        {
            return None;
        }
        let project = |x: Vec3d| {
            Vec3d::new(
                self.axes[0].dot(x),
                self.axes[1].dot(x),
                self.axes[2].dot(x),
            )
        };
        let p = project(point - self.anchor);
        let v = project(v);
        let vn = v.length();
        if !(vn > direction_error && vn > 0.0) {
            return None;
        }
        let relative = p - core.center;
        let delta = (v / vn - Vec3d::new(0.0, 0.0, 1.0)).length()
            + 2.0 * direction_error / (vn - direction_error);
        let arithmetic =
            512.0 * f64::EPSILON * (1.0 + core.boundary_radius + relative.length() + vn);
        let erosion = delta * (core.boundary_radius + relative.length())
            + point_error
            + vertex_error
            + arithmetic;
        let margin = (core.half_extent[0] - relative.x.abs())
            .min(core.half_extent[1] - relative.y.abs())
            - erosion;
        if !(margin > 0.0) {
            return None;
        }
        let extent = (self.max - self.min) * 0.5;
        let center = (self.min + self.max) * 0.5 - p;
        let reach = v.x.abs() * extent.x + v.y.abs() * extent.y + v.z.abs() * extent.z;
        let vv = vn * vn;
        let middle = v.dot(center) / vv;
        let nominal = [middle - reach / vv, middle + reach / vv];
        let rmax = center.length() + extent.length();
        let relative_error = point_error + vertex_error;
        let denominator_error = 2.0 * vn * direction_error + direction_error * direction_error;
        let denominator = vv - denominator_error;
        if !(denominator > 0.0) {
            return None;
        }
        let error = (vn * relative_error
            + direction_error * (rmax + relative_error)
            + nominal[0].abs().max(nominal[1].abs()) * denominator_error)
            / denominator
            + arithmetic;
        let all_t = [nominal[0] - error, nominal[1] + error];
        if all_t[0] < interval[0] || all_t[1] > interval[1] {
            return None;
        }
        Some(CoverageCertificate {
            degree: core.degree,
            minimum_boundary_margin_m: margin,
            line_parameter_interval_m: all_t,
        })
    }

    /// All input errors are Euclidean bounds in the same coherent patch frame.
    /// They must include source/transform uncertainty if testing source geometry.
    /// Parameter t belongs to p+t*v, retaining the supplied v's actual norm.
    pub fn certify(
        &self,
        point: Vec3d,
        v: Vec3d,
        point_error: f64,
        direction_error: f64,
        vertex_error: f64,
        interval: [f64; 2],
    ) -> Option<CoverageCertificate> {
        if !point.is_finite()
            || !v.is_finite()
            || [point_error, direction_error, vertex_error]
                .iter()
                .any(|e| !e.is_finite() || *e < 0.0)
            || !interval.iter().all(|x| x.is_finite())
            || interval[0] >= interval[1]
        {
            return None;
        }
        let project = |v: Vec3d| {
            Vec3d::new(
                self.axes[0].dot(v),
                self.axes[1].dot(v),
                self.axes[2].dot(v),
            )
        };
        let p = project(point - self.anchor);
        let v = project(v);
        let vn = v.length();
        if !(vn > direction_error && vn > 0.0) {
            return None;
        }
        let normal = v / vn;
        let axis = if normal.x.abs() <= normal.y.abs() && normal.x.abs() <= normal.z.abs() {
            Vec3d::new(1.0, 0.0, 0.0)
        } else if normal.y.abs() <= normal.z.abs() {
            Vec3d::new(0.0, 1.0, 0.0)
        } else {
            Vec3d::new(0.0, 0.0, 1.0)
        };
        let e0 = normal.cross(axis).normalized()?;
        let e1 = normal.cross(e0);
        let normal_error = 2.0 * direction_error / (vn - direction_error);
        let relative_error = point_error + vertex_error;
        let center = (self.min + self.max) * 0.5 - p;
        let extent = (self.max - self.min) * 0.5;
        let reach = v.x.abs() * extent.x + v.y.abs() * extent.y + v.z.abs() * extent.z;
        let vv = vn * vn;
        let middle = v.dot(center) / vv;
        let nominal = [middle - reach / vv, middle + reach / vv];
        let rmax = center.length() + extent.length();
        let denominator_error = 2.0 * vn * direction_error + direction_error * direction_error;
        let denominator = vv - denominator_error;
        if !(denominator > 0.0) {
            return None;
        }
        let arithmetic = 128.0 * f64::EPSILON * (1.0 + rmax + vn);
        let t_error = (vn * relative_error
            + direction_error * (rmax + relative_error)
            + nominal[0].abs().max(nominal[1].abs()) * denominator_error)
            / denominator
            + arithmetic;
        let all_t = [nominal[0] - t_error, nominal[1] + t_error];
        if all_t[0] < interval[0] || all_t[1] > interval[1] {
            return None;
        }
        let project = |i: u32| {
            let r = self.vertices[i as usize] - p;
            (
                [e0.dot(r), e1.dot(r)],
                relative_error + normal_error * (r.length() + relative_error) + arithmetic,
            )
        };
        let mut degree = 0_i64;
        let mut margin = f64::INFINITY;
        for &(a, b, multiplicity) in &self.edges {
            let (a, ea) = project(a);
            let (b, eb) = project(b);
            let d = [b[0] - a[0], b[1] - a[1]];
            let dd = d[0] * d[0] + d[1] * d[1];
            let t = if dd > 0.0 {
                (-(a[0] * d[0] + a[1] * d[1]) / dd).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let x = a[0] + t * d[0];
            let y = a[1] + t * d[1];
            let separation = (x * x + y * y).sqrt() - ea.max(eb) - arithmetic;
            if !(separation > 0.0) {
                return None;
            }
            margin = margin.min(separation);
            let cross = a[0] * b[1] - a[1] * b[0];
            // Half-open crossings count each boundary vertex once. Boundary
            // distance above certifies invariance under the finite homotopy.
            if a[1] <= 0.0 && b[1] > 0.0 && cross > 0.0 {
                degree = degree.checked_add(multiplicity)?;
            }
            if b[1] <= 0.0 && a[1] > 0.0 && cross < 0.0 {
                degree = degree.checked_sub(multiplicity)?;
            }
        }
        (degree != 0).then_some(CoverageCertificate {
            degree,
            minimum_boundary_margin_m: margin,
            line_parameter_interval_m: all_t,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_core_proves_only_strict_interior_with_direction_uncertainty() {
        let a = Vec3d::new(0.0, 0.0, 6e6);
        let v = [[-100., -50.], [100., -50.], [100., 50.], [-100., 50.]]
            .map(|p| a + Vec3d::new(p[0], p[1], -27.));
        let chain = BoundaryChain::build(&v, &[[0, 1, 2], [0, 2, 3]]).unwrap();
        let core = chain.interior_core().unwrap();
        for x in [-110., -100., -80., 0., 80., 100., 110.] {
            for y in [-60., -50., -30., 0., 30., 50., 60.] {
                let point = a + Vec3d::new(x, y, 0.0);
                let certificate = chain.certify_core(
                    core,
                    point,
                    Vec3d::new(0.001, -0.002, 1.0),
                    0.01,
                    1e-6,
                    0.003,
                    [-100., 100.],
                );
                if let Some(c) = certificate {
                    let t = -27.;
                    assert!((x + 0.001 * t).abs() < 100. && (y - 0.002 * t).abs() < 50.);
                    assert!(c.minimum_boundary_margin_m > 0.);
                }
                if x.abs() < 81. && y.abs() < 31. {
                    assert!(certificate.is_some());
                }
                if x.abs() >= 100. || y.abs() >= 50. {
                    assert!(certificate.is_none());
                }
            }
        }
        assert!(
            chain
                .certify_core(core, a, Vec3d::new(0., 0., 1.), 0., 1., 0., [-100., 100.])
                .is_none()
        );
        assert!(
            chain
                .certify_core(core, a, Vec3d::new(0., 0., 1.), 0., 0., 0., [1., 2.])
                .is_none()
        );
    }
    #[test]
    fn internal_seam_cancels_but_external_boundary_never_dilates() {
        let v = [
            Vec3d::new(-10.0, -10.0, -27.0),
            Vec3d::new(10.0, -10.0, -27.0),
            Vec3d::new(10.0, 10.0, -27.0),
            Vec3d::new(-10.0, 10.0, -27.0),
        ];
        let c = BoundaryChain::build(&v, &[[0, 1, 2], [0, 2, 3]]).unwrap();
        assert_eq!(c.edges.len(), 4);
        let query = |p| {
            c.certify(
                p,
                Vec3d::new(0.0, 0.0, 1.0),
                0.003,
                1e-6,
                0.002,
                [-10001.0, 101000.0],
            )
        };
        let hit = query(Vec3d::ZERO).unwrap();
        assert_eq!(hit.degree, 1);
        assert!(hit.minimum_boundary_margin_m > 9.99);
        assert!(query(Vec3d::new(10.0, 0.0, 0.0)).is_none());
        assert!(query(Vec3d::new(10.01, 0.0, 0.0)).is_none());
        assert!(query(Vec3d::new(0.0, 0.0, 200000.0)).is_none());
    }
    #[test]
    fn directed_multiplicity_and_holes_remain_visible_to_fallback() {
        let v = [
            Vec3d::new(-1.0, -1.0, 0.0),
            Vec3d::new(1.0, -1.0, 0.0),
            Vec3d::new(1.0, 1.0, 0.0),
            Vec3d::new(-1.0, 1.0, 0.0),
        ];
        let c = BoundaryChain::build(&v, &[[0, 1, 2], [0, 2, 3], [0, 1, 2], [0, 2, 3]]).unwrap();
        assert_eq!(
            c.certify(
                Vec3d::new(0.0, 0.0, 1.0),
                Vec3d::new(0.0, 0.0, 1.0),
                0.0,
                0.0,
                0.0,
                [-10001.0, 101000.0]
            )
            .unwrap()
            .degree,
            2
        );
        let canceled =
            BoundaryChain::build(&v, &[[0, 1, 2], [0, 2, 3], [2, 1, 0], [3, 2, 0]]).unwrap();
        assert!(canceled.edges.is_empty());
        assert!(
            canceled
                .certify(
                    Vec3d::new(0.0, 0.0, 1.0),
                    Vec3d::new(0.0, 0.0, 1.0),
                    0.0,
                    0.0,
                    0.0,
                    [-10001.0, 101000.0]
                )
                .is_none()
        );
    }
}
