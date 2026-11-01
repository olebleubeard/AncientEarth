//! Conservative finite-segment candidates for numerical optical paths.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct OpticalNode {
    pub min: Vec3d,
    pub max: Vec3d,
    pub left: usize,
    pub right: usize,
    pub start: usize,
    pub count: usize,
}
pub struct OpticalMesh<'a> {
    pub vertices: &'a [Vec3d],
    /// Same triangle order as the exported BVH leaves.
    pub triangles: Vec<[u32; 3]>,
    pub nodes: Vec<OpticalNode>,
}
#[derive(Clone, Copy, Debug)]
pub struct OpticalTriangle {
    pub vertices: [Vec3d; 3],
    pub normal: Vec3d,
    pub kind: SurfaceKind,
    pub grass_allowed: bool,
}
impl TerrainPatch {
    pub fn optical_mesh(&self) -> OpticalMesh<'_> {
        OpticalMesh {
            vertices: &self.vertices,
            triangles: self
                .triangles
                .iter()
                .map(|t| t.vertices.map(|i| i as u32))
                .collect(),
            nodes: self
                .nodes
                .iter()
                .map(|n| OpticalNode {
                    min: n.bounds.min,
                    max: n.bounds.max,
                    left: n.left,
                    right: n.right,
                    start: n.start,
                    count: n.count,
                })
                .collect(),
        }
    }
}

/// Euclidean point-to-triangle distance; the minimizer is either the plane
/// projection within the face or a point on one of its three closed edges.
fn point_triangle(p: Vec3d, v: [Vec3d; 3]) -> f64 {
    let [a, b, c] = v;
    let normal = (b - a).cross(c - a).normalized().unwrap();
    let projected = p - normal * normal.dot(p - a);
    if contains_point(v, projected, 1e-12) {
        return normal.dot(p - a).abs();
    }
    [(a, b), (b, c), (c, a)]
        .into_iter()
        .map(|(a, b)| {
            let edge = b - a;
            p.distance(a + edge * ((p - a).dot(edge) / edge.dot(edge)).clamp(0.0, 1.0))
        })
        .fold(f64::INFINITY, f64::min)
}
fn segment_segment(a: Vec3d, b: Vec3d, c: Vec3d, d: Vec3d) -> f64 {
    let u = b - a;
    let v = d - c;
    let w = a - c;
    let aa = u.dot(u);
    let bb = u.dot(v);
    let cc = v.dot(v);
    let dd = u.dot(w);
    let ee = v.dot(w);
    if aa == 0.0 {
        return a.distance(c + v * (ee / cc).clamp(0.0, 1.0));
    }
    if cc == 0.0 {
        return c.distance(a + u * (-dd / aa).clamp(0.0, 1.0));
    }
    let det = aa * cc - bb * bb;
    if det <= f64::EPSILON * aa * cc {
        // This helper rejects optical candidates. Near-parallel cancellation
        // cannot justify substituting the first endpoint's distance: a closer
        // interior crossing may exist. Zero is a safe distance lower bound.
        return 0.0;
    }
    let mut s = ((bb * ee - cc * dd) / det).clamp(0.0, 1.0);
    let mut t = (bb * s + ee) / cc;
    if t < 0.0 {
        t = 0.0;
        s = (-dd / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((bb - dd) / aa).clamp(0.0, 1.0);
    }
    (a + u * s).distance(c + v * t)
}
pub(crate) fn contains_point(v: [Vec3d; 3], p: Vec3d, tolerance: f64) -> bool {
    let [a, b, c] = v;
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let aa = ab.dot(ab);
    let bb = ab.dot(ac);
    let cc = ac.dot(ac);
    let det = aa * cc - bb * bb;
    if det <= 0.0 {
        return false;
    }
    let u = (cc * ap.dot(ab) - bb * ap.dot(ac)) / det;
    let w = (aa * ap.dot(ac) - bb * ap.dot(ab)) / det;
    u >= -tolerance && w >= -tolerance && u + w <= 1.0 + tolerance
}
/// Candidate distance, conservative in an ill-conditioned near-parallel case;
/// not an exact metrology result for arbitrary degenerate segment pairs.
pub(crate) fn segment_triangle_distance(a: Vec3d, b: Vec3d, v: [Vec3d; 3]) -> f64 {
    let normal = (v[1] - v[0]).cross(v[2] - v[0]).normalized().unwrap();
    let x = normal.dot(a - v[0]);
    let y = normal.dot(b - v[0]);
    if x * y <= 0.0 && x != y {
        let p = a + (b - a) * (x / (x - y));
        if contains_point(v, p, 1e-12) {
            return 0.0;
        }
    }
    let mut best = point_triangle(a, v).min(point_triangle(b, v));
    for (c, d) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
        best = best.min(segment_segment(a, b, c, d));
    }
    // Distance to the supporting plane is another lower bound. Retain it so
    // the conservative near-parallel edge case need not admit distant planes.
    let plane_gap = if x * y > 0.0 {
        x.abs().min(y.abs())
    } else {
        0.0
    };
    best.max(plane_gap)
}

impl TerrainScene {
    /// Conservative physical mesh candidates near the closed chord. The
    /// radius is supplied by the optical curvature/error policy. No source
    /// thinning or elevation interpolation is performed here.
    pub fn optical_candidates(
        &self,
        a: Vec3d,
        b: Vec3d,
        radius: f64,
    ) -> Result<Vec<OpticalTriangle>, TerrainError> {
        if !a.is_finite() || !b.is_finite() || !radius.is_finite() || radius < 0.0 {
            return Err(TerrainError::InvalidGrid);
        }
        let mut found = Vec::new();
        if self.boundary_only {
            return Ok(found);
        }
        let length = a.distance(b);
        let direction = if length > 0.0 {
            (b - a) / length
        } else {
            Vec3d::new(1.0, 0.0, 0.0)
        };
        for (patch_id, patch) in self.patches.iter().enumerate() {
            let mut stack = vec![0usize];
            while let Some(index) = stack.pop() {
                let node = patch.nodes[index];
                let delta = Vec3d::new(radius, radius, radius);
                let bounds = Bounds {
                    min: node.bounds.min - delta,
                    max: node.bounds.max + delta,
                };
                if bounds.entry(a, direction, length).is_none() {
                    continue;
                }
                if node.count == 0 {
                    stack.push(node.right);
                    stack.push(node.left);
                    continue;
                }
                for triangle in &patch.triangles[node.start..node.start + node.count] {
                    let vertices = triangle.vertices.map(|i| patch.vertices[i]);
                    if segment_triangle_distance(a, b, vertices) > radius + 1e-7 {
                        continue;
                    }
                    if found.len() >= 4096 {
                        return Err(TerrainError::InvalidGrid);
                    }
                    found.push(OpticalTriangle {
                        vertices,
                        normal: triangle.normal,
                        kind: WaterKind::from_code(triangle.water_code).map_or(
                            SurfaceKind::Terrain(patch_id),
                            |kind| SurfaceKind::Water {
                                patch: patch_id,
                                kind,
                            },
                        ),
                        grass_allowed: triangle.water_code == 0,
                    });
                }
            }
        }
        Ok(found)
    }
    pub(crate) fn optical_surface(
        &self,
        triangle: OpticalTriangle,
        position: Vec3d,
        origin: Vec3d,
    ) -> Option<SurfaceHit> {
        Some(self.classify_physical_hit(SurfaceHit {
            hit: RayHit {
                position,
                normal: triangle.normal,
                distance_m: origin.distance(position),
                geodetic: ecef_to_geodetic(position).ok()?,
            },
            kind: triangle.kind,
            grass_allowed: triangle.grass_allowed,
        }))
    }
    pub(crate) fn optical_reference_surface(
        &self,
        position: Vec3d,
        origin: Vec3d,
    ) -> Option<SurfaceHit> {
        let geodetic = ecef_to_geodetic(position).ok()?;
        if !self.boundary_only && self.terrain_ground(geodetic).is_some() {
            return None;
        }
        Some(self.classify_physical_hit(SurfaceHit {
            hit: RayHit {
                position,
                normal: EnuBasis::at(geodetic).up,
                distance_m: origin.distance(position),
                geodetic,
            },
            kind: SurfaceKind::ReferenceEllipsoid,
            grass_allowed: true,
        }))
    }
}

#[cfg(test)]
mod distance_review_regression {
    use super::*;
    #[test]
    fn nearly_parallel_candidate_distance_never_substitutes_endpoint_gap() {
        // Every point on the chord has z=gap, and its midpoint projects into
        // the triangle. The exact segment-triangle distance is therefore gap.
        // Both chord endpoints project outside the slender triangle.
        for (width, gap) in [(0.1, 0.001), (1e-6, 1e-9)] {
            let a = Vec3d::new(-1000.0, width, gap);
            let b = Vec3d::new(1000.0, -width, gap);
            let triangle = [
                Vec3d::new(-1000.0, 0.0, 0.0),
                Vec3d::new(1000.0, 0.0, 0.0),
                Vec3d::new(0.0, -2.0 * width, 0.0),
            ];
            let lower = segment_triangle_distance(a, b, triangle);
            assert!(
                lower <= gap + 1e-12,
                "candidate lower bound {lower} exceeds actual {gap}"
            );
            assert!(
                lower >= gap - 1e-12,
                "supporting-plane separation should remain usable"
            );
        }
        let a = Vec3d::new(-1000.0, 1e-6, 5.0);
        let b = Vec3d::new(1000.0, -1e-6, 5.0);
        let triangle = [
            Vec3d::new(-1000.0, 0.0, 0.0),
            Vec3d::new(1000.0, 0.0, 0.0),
            Vec3d::new(0.0, -2e-6, 0.0),
        ];
        assert_eq!(segment_triangle_distance(a, b, triangle), 5.0);
    }
}
