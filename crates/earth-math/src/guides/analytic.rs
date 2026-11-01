//! WGS84 tangent circle, clipped in the current optical projection.
//! Rectilinear clipping/extrema are analytic trigonometric roots. Fisheye
//! aperture roots are isolated quartics; its projection extrema use bounded
//! one-dimensional derivative root refinement, never Earth ray integration.
use super::*;
use crate::{Projection, Vec3d, WGS84_A_M, WGS84_B_M};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy)]
struct Trig([f64; 3]);
impl Trig {
    fn at(self, t: f64) -> f64 {
        let (s, c) = t.sin_cos();
        self.0[0] + self.0[1] * c + self.0[2] * s
    }
    fn derivative(self, t: f64) -> f64 {
        let (s, c) = t.sin_cos();
        -self.0[1] * s + self.0[2] * c
    }
    fn minus(self, other: Self, factor: f64) -> Self {
        Self(std::array::from_fn(|i| self.0[i] - factor * other.0[i]))
    }
    fn roots(self) -> Vec<f64> {
        let [a, b, c] = self.0;
        let r = b.hypot(c);
        if r == 0.0 || a.abs() > r * (1.0 + 16.0 * f64::EPSILON) {
            return vec![];
        }
        let phase = c.atan2(b);
        let angle = (-a / r).clamp(-1.0, 1.0).acos();
        vec![
            (phase - angle).rem_euclid(TAU),
            (phase + angle).rem_euclid(TAU),
        ]
    }
    fn ratio_extrema(self, denominator: Self) -> Vec<f64> {
        let [a, b, c] = self.0;
        let [d, e, f] = denominator.0;
        Trig([c * e - b * f, c * d - a * f, a * e - b * d]).roots()
    }
}

pub(super) struct TangentContour {
    frame: CameraFrame,
    lens: Lens,
    viewport: GuideViewport,
    center: Vec3d,
    cosine: Vec3d,
    sine: Vec3d,
    direction_constant: Vec3d,
    components: [Trig; 3],
}
impl TangentContour {
    pub(super) fn new(frame: CameraFrame, lens: Lens, viewport: GuideViewport) -> Option<Self> {
        let scaled = Vec3d::new(
            frame.origin.x / WGS84_A_M,
            frame.origin.y / WGS84_A_M,
            frame.origin.z / WGS84_B_M,
        );
        let radius = scaled.length();
        if !radius.is_finite() || radius <= 1.0 {
            return None;
        }
        let axis = scaled / radius;
        let reference = if axis.z.abs() < 0.8 {
            Vec3d::new(0.0, 0.0, 1.0)
        } else {
            Vec3d::new(1.0, 0.0, 0.0)
        };
        let e = axis.cross(reference).normalized()?;
        let f = axis.cross(e);
        let excess = (radius - 1.0) * (radius + 1.0);
        let circle_radius = excess.sqrt() / radius;
        let unscale = |p: Vec3d| Vec3d::new(p.x * WGS84_A_M, p.y * WGS84_A_M, p.z * WGS84_B_M);
        let center = frame.origin / (radius * radius);
        let cosine = unscale(e) * circle_radius;
        let sine = unscale(f) * circle_radius;
        // Avoid subtracting nearly equal Earth-radius vectors near ground.
        let direction_constant = frame.origin * (-excess / (radius * radius));
        let components = [frame.right, frame.up, frame.forward].map(|basis| {
            Trig([
                direction_constant.dot(basis),
                cosine.dot(basis),
                sine.dot(basis),
            ])
        });
        Some(Self {
            frame,
            lens: Lens::new(lens.projection, lens.vertical_fov_deg),
            viewport,
            center,
            cosine,
            sine,
            direction_constant,
            components,
        })
    }
    fn direction(&self, t: f64) -> Vec3d {
        let (s, c) = t.sin_cos();
        self.direction_constant + self.cosine * c + self.sine * s
    }
    fn projection(&self, t: f64) -> Option<[f64; 2]> {
        let [x, y, z] = self.components.map(|q| q.at(t));
        let half = self.lens.vertical_fov_deg.to_radians() * 0.5;
        let factor = self.viewport.pixels[1] * 0.5;
        let offsets = match self.lens.projection {
            Projection::Rectilinear => {
                if z <= 0.0 {
                    return None;
                }
                [factor * x / (z * half.tan()), factor * y / (z * half.tan())]
            }
            Projection::EquidistantFisheye => {
                let r = x.hypot(y);
                let angle = r.atan2(z);
                if angle > half + 64.0 * f64::EPSILON {
                    return None;
                }
                if r == 0.0 {
                    [0.0, 0.0]
                } else {
                    [
                        factor * angle * x / (half * r),
                        factor * angle * y / (half * r),
                    ]
                }
            }
        };
        Some([
            self.viewport.pixels[0] * 0.5 + offsets[0],
            self.viewport.pixels[1] * 0.5 - offsets[1],
        ])
    }
    fn point(&self, scene: &TerrainScene, t: f64) -> Option<HorizonPoint> {
        let mut pixel = self.projection(t)?;
        for (i, coordinate) in pixel.iter_mut().enumerate() {
            if *coordinate < -1e-6 || *coordinate > self.viewport.pixels[i] + 1e-6 {
                return None;
            }
            *coordinate = coordinate.clamp(0.0, self.viewport.pixels[i]);
        }
        let (s, c) = t.sin_cos();
        let position = self.center + self.cosine * c + self.sine * s;
        let normal = Vec3d::new(
            position.x / WGS84_A_M.powi(2),
            position.y / WGS84_A_M.powi(2),
            position.z / WGS84_B_M.powi(2),
        )
        .normalized()?;
        let location = Geodetic::new(
            normal.z.atan2(normal.x.hypot(normal.y)).to_degrees(),
            position.y.atan2(position.x).to_degrees(),
            0.0,
        )
        .ok()?;
        let mut hit = scene.ground(location).ok()?;
        hit.hit = crate::RayHit {
            position,
            normal,
            geodetic: location,
            distance_m: self.direction(t).length(),
        };
        Some(HorizonPoint {
            pixel,
            hit,
            bracket_pixels: 0.0,
        })
    }
    fn rect_candidates(&self, column: Option<f64>) -> Vec<f64> {
        let [x, y, z] = self.components;
        let half = (self.lens.vertical_fov_deg.to_radians() * 0.5).tan();
        let aspect = self.viewport.pixels[0] / self.viewport.pixels[1];
        if let Some(column) = column {
            return x.minus(z, (2.0 * column - 1.0) * half * aspect).roots();
        }
        let mut candidates = Vec::new();
        for sign in [-1.0, 1.0] {
            candidates.extend(x.minus(z, sign * half * aspect).roots());
            candidates.extend(y.minus(z, sign * half).roots());
        }
        candidates.extend(x.ratio_extrema(z));
        candidates.extend(y.ratio_extrema(z));
        candidates
    }
    // Aperture boundaries have a quartic on each bounded tan(t/2) chart.
    // Isolating its derivative roots retains arbitrarily narrow visible arcs,
    // unlike a sampled full-circle or full-screen occupancy lattice.
    fn fisheye_intervals(&self) -> Vec<[f64; 2]> {
        let half = self.lens.vertical_fov_deg.to_radians() * 0.5;
        let cos2 = half.cos().powi(2);
        let sin2 = half.sin().powi(2);
        let chart_edge = (PI / 8.0).tan();
        let mut cuts = vec![0.0, TAU];
        // Center a chart near the optical axis before forming coefficients.
        // Otherwise a telescope-sized aperture is the tiny difference between
        // O(1) polynomial terms near a repeated root in an arbitrary chart.
        let chart_center = self.components[0]
            .roots()
            .into_iter()
            .chain(self.components[1].roots())
            .min_by(|a, b| {
                let angle = |t: f64| {
                    let [x, y, z] = self.components.map(|q| q.at(t));
                    x.hypot(y).atan2(z)
                };
                angle(*a).total_cmp(&angle(*b))
            })
            .unwrap_or(PI / 4.0);
        for quadrant in 0..4 {
            let center = chart_center + quadrant as f64 * FRAC_PI_2;
            let (s, c) = center.sin_cos();
            let rotated_cos = self.cosine * c + self.sine * s;
            let rotated_sin = self.sine * c - self.cosine * s;
            let coefficients = [
                self.direction_constant + rotated_cos,
                rotated_sin * 2.0,
                self.direction_constant - rotated_cos,
            ];
            let z = coefficients.map(|p| p.dot(self.frame.forward));
            let x = coefficients.map(|p| p.dot(self.frame.right));
            let y = coefficients.map(|p| p.dot(self.frame.up));
            let mut polynomial = [0.0; 5];
            for i in 0..3 {
                for j in 0..3 {
                    // Transverse form avoids subtracting almost equal longitudinal
                    // squares when the fisheye field of view is very narrow.
                    polynomial[i + j] += sin2 * z[i] * z[j] - cos2 * (x[i] * x[j] + y[i] * y[j]);
                }
            }
            for root in polynomial_roots(&polynomial, -chart_edge, chart_edge) {
                let t = (center + 2.0 * root.atan()).rem_euclid(TAU);
                if self.components[2].at(t) >= -1e-7 {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-13);
        cuts.windows(2)
            .filter_map(|p| self.projection((p[0] + p[1]) * 0.5).map(|_| [p[0], p[1]]))
            .collect()
    }
    fn fisheye_derivative(&self, t: f64, axis: usize) -> f64 {
        let [x, y, z] = self.components.map(|q| q.at(t));
        let [dx, dy, dz] = self.components.map(|q| q.derivative(t));
        let r = x.hypot(y);
        if r < 1e-12 {
            return if axis == 0 { dx / z } else { dy / z };
        }
        let dr = (x * dx + y * dy) / r;
        let theta = r.atan2(z);
        let dt = (z * dr - r * dz) / (r * r + z * z);
        let (v, dv) = if axis == 0 { (x, dx) } else { (y, dy) };
        dt * v / r + theta * (dv / r - v * dr / (r * r))
    }
    fn fisheye_candidates(&self, column: Option<f64>) -> Vec<f64> {
        let mut candidates = Vec::new();
        for [a, b] in self.fisheye_intervals() {
            let mut cuts = vec![a, b];
            // Finite extrema isolation on a smooth one-dimensional projected
            // analytic contour; this is not a screen-ray sampling approximation.
            for axis in 0..2 {
                cuts.extend(numeric_roots(
                    |t| self.fisheye_derivative(t, axis),
                    a,
                    b,
                    64,
                ));
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-13);
            if column.is_none() {
                candidates.extend(cuts.iter().copied());
            }
            for interval in cuts.windows(2) {
                let [a, b] = [interval[0], interval[1]];
                for axis in 0..2 {
                    let targets = if let Some(column) = column {
                        if axis == 1 {
                            continue;
                        }
                        vec![column * self.viewport.pixels[0]]
                    } else {
                        vec![0.0, self.viewport.pixels[axis]]
                    };
                    for target in targets {
                        candidates.extend(numeric_roots(
                            |t| self.projection(t).map_or(f64::NAN, |p| p[axis] - target),
                            a,
                            b,
                            1,
                        ));
                    }
                }
            }
        }
        candidates
    }
    pub(super) fn points(&self, scene: &TerrainScene, column: Option<f64>) -> Vec<HorizonPoint> {
        let candidates = match self.lens.projection {
            Projection::Rectilinear => self.rect_candidates(column),
            Projection::EquidistantFisheye => self.fisheye_candidates(column),
        };
        candidates
            .into_iter()
            .filter_map(|t| self.point(scene, t))
            .collect()
    }
    /// At tangency, moving the ray toward the interior means a negative
    /// component along the outward surface normal. Image-down reverses local y.
    pub(super) fn sky_to_earth(&self, point: HorizonPoint) -> bool {
        let derivative = match self.lens.projection {
            Projection::Rectilinear => self.frame.up,
            Projection::EquidistantFisheye => {
                let half = self.lens.vertical_fov_deg.to_radians() * 0.5;
                let x = (2.0 * point.pixel[0] - self.viewport.pixels[0]) / self.viewport.pixels[1];
                let y = 1.0 - 2.0 * point.pixel[1] / self.viewport.pixels[1];
                let radius = x.hypot(y);
                if radius < 1e-8 {
                    self.frame.up * half
                } else {
                    let theta = radius * half;
                    let (s, c) = theta.sin_cos();
                    self.frame.forward * (-s * half * y / radius)
                        + self.frame.up * (s / radius)
                        + (self.frame.right * x + self.frame.up * y)
                            * (y * (half * c * radius - s) / radius.powi(3))
                }
            }
        };
        point.hit.hit.normal.dot(derivative) > 0.0
    }
}

fn numeric_roots(f: impl Fn(f64) -> f64, a: f64, b: f64, intervals: usize) -> Vec<f64> {
    let mut result = Vec::new();
    let mut left = a;
    let mut fl = f(left);
    for i in 1..=intervals {
        let right = a + (b - a) * i as f64 / intervals as f64;
        let fr = f(right);
        if fl == 0.0 {
            result.push(left);
        }
        if fr == 0.0 {
            result.push(right);
        }
        if fl.is_finite() && fr.is_finite() && fl.signum() != fr.signum() && fl != 0.0 && fr != 0.0
        {
            let (mut lo, mut hi) = (left, right);
            for _ in 0..55 {
                let mid = (lo + hi) * 0.5;
                if f(mid).signum() == fl.signum() {
                    lo = mid
                } else {
                    hi = mid
                }
            }
            result.push((lo + hi) * 0.5);
        }
        left = right;
        fl = fr;
    }
    result
}

fn polynomial_roots(coefficients: &[f64], a: f64, b: f64) -> Vec<f64> {
    let scale = coefficients.iter().map(|v| v.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return vec![];
    }
    let mut c: Vec<f64> = coefficients.iter().map(|v| v / scale).collect();
    while c.len() > 1 && c.last() == Some(&0.0) {
        c.pop();
    }
    if c.len() == 1 {
        return vec![];
    }
    if c.len() == 2 {
        let root = -c[0] / c[1];
        return if (a..=b).contains(&root) {
            vec![root]
        } else {
            vec![]
        };
    }
    let derivative: Vec<f64> = c
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, v)| i as f64 * v)
        .collect();
    let mut cuts = vec![a];
    cuts.extend(polynomial_roots(&derivative, a, b));
    cuts.push(b);
    cuts.sort_by(f64::total_cmp);
    let evaluate = |x: f64| c.iter().rev().fold(0.0, |sum, v| sum * x + v);
    let mut result: Vec<f64> = cuts
        .iter()
        .copied()
        .filter(|t| evaluate(*t).abs() < 64.0 * f64::EPSILON)
        .collect();
    for interval in cuts.windows(2) {
        result.extend(numeric_roots(evaluate, interval[0], interval[1], 1));
    }
    result
}
