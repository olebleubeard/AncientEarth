//! Finite curved-event isolation against canonical faces and marker prisms.
use super::*;
use crate::{
    survey::SurveyMarker,
    terrain::optical::{OpticalTriangle, contains_point, segment_triangle_distance},
};

pub const EVENT_CURVE_TOLERANCE_M: f64 = 0.0001;
pub const EVENT_INTERVAL_M: f64 = 0.000001;
const MAX_EVENT_NODES: u32 = 512;

#[derive(Clone, Copy)]
struct Face {
    vertices: [Vec3d; 3],
    normal: Vec3d,
    terrain: Option<OpticalTriangle>,
    marker: Option<usize>,
}
pub(super) struct SceneEvent {
    pub state: State,
    pub distance: f64,
    pub surface: Option<SurfaceHit>,
    pub marker: Option<OpticalMarkerHit>,
}
type VacuumEvent = (f64, Option<SurfaceHit>, Option<OpticalMarkerHit>);
pub(super) fn vacuum_event(
    ray: Ray,
    scene: &TerrainScene,
    markers: &[SurveyMarker],
    limit: f64,
) -> Result<Option<VacuumEvent>, AtmosphereError> {
    let mut result = scene
        .intersect(ray)
        .filter(|h| h.hit.distance_m <= limit)
        .map(|h| (h.hit.distance_m, Some(h), None));
    for marker in markers {
        if !marker.valid() {
            return Err(AtmosphereError::InvalidSample);
        }
        if let Some(distance) = marker.intersect(ray)
            && distance <= limit
            && result.is_none_or(|r| distance < r.0)
        {
            let position = ray.origin + ray.direction * distance;
            let relative = position - marker.base_ecef_m;
            let normal = [
                (
                    -marker.right,
                    (relative.dot(marker.right) + marker.width_m * 0.5).abs(),
                ),
                (
                    marker.right,
                    (relative.dot(marker.right) - marker.width_m * 0.5).abs(),
                ),
                (
                    -marker.forward,
                    (relative.dot(marker.forward) + marker.depth_m * 0.5).abs(),
                ),
                (
                    marker.forward,
                    (relative.dot(marker.forward) - marker.depth_m * 0.5).abs(),
                ),
                (-marker.up, relative.dot(marker.up).abs()),
                (marker.up, (relative.dot(marker.up) - marker.height_m).abs()),
            ]
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap()
            .0;
            result = Some((
                distance,
                None,
                Some(OpticalMarkerHit {
                    marker_id: marker.id,
                    position_ecef_m: position,
                    normal,
                }),
            ));
        }
    }
    Ok(result)
}

fn resolved_event(
    scene: &TerrainScene,
    face: Face,
    state: State,
    distance: f64,
    origin: Vec3d,
) -> Result<SceneEvent, AtmosphereError> {
    let surface = if let Some(triangle) = face.terrain {
        Some(
            scene
                .optical_surface(triangle, state.p, origin)
                .ok_or(AtmosphereError::NoConvergence)?,
        )
    } else {
        None
    };
    Ok(SceneEvent {
        state,
        distance,
        surface,
        marker: face.marker.map(|marker_id| OpticalMarkerHit {
            marker_id,
            position_ecef_m: state.p,
            normal: face.normal,
        }),
    })
}

impl AtmosphereProfile {
    #[allow(clippy::too_many_arguments)] // Explicit step, physical scene, and cancellation inputs.
    pub(super) fn scene_event(
        &self,
        start: State,
        end: State,
        step: f64,
        scene: &TerrainScene,
        markers: &[SurveyMarker],
        origin: Vec3d,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<SceneEvent>, AtmosphereError> {
        let mut budget = MAX_EVENT_NODES;
        self.scene_event_inner(
            start,
            end,
            step,
            scene,
            markers,
            origin,
            &mut budget,
            cancelled,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn scene_event_inner(
        &self,
        start: State,
        end: State,
        step: f64,
        scene: &TerrainScene,
        markers: &[SurveyMarker],
        origin: Vec3d,
        budget: &mut u32,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<SceneEvent>, AtmosphereError> {
        if *budget == 0 || cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AtmosphereError::NoConvergence);
        }
        *budget -= 1;
        // For a unit-speed curve with |r''|<=k, its distance from the endpoint
        // chord is <=k*L²/8. The integration allowance is a local estimator,
        // explicitly not a certified accumulated trajectory error bound.
        let deviation = self.maximum_gradient * step * step / 8.0;
        let coarse = self.midpoint(start, step)?;
        let estimator = coarse
            .p
            .distance(end.p)
            .max((coarse.u - end.u).length() * step)
            / 3.0;
        let roundoff = 1e-8 + 32.0 * f64::EPSILON * start.p.length().max(end.p.length());
        let radius = deviation + estimator + roundoff;
        let mut faces: Vec<Face> = scene
            .optical_candidates(start.p, end.p, radius)
            .map_err(|_| AtmosphereError::NoConvergence)?
            .into_iter()
            .map(|t| Face {
                vertices: t.vertices,
                normal: t.normal,
                terrain: Some(t),
                marker: None,
            })
            .collect();
        const BOX_FACES: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        for marker in markers {
            if !marker.valid() {
                return Err(AtmosphereError::InvalidSample);
            }
            let chord = end.p - start.p;
            let relative = start.p - marker.base_ecef_m;
            let mut near = 0.0_f64;
            let mut far = 1.0_f64;
            for (axis, lo, hi) in [
                (marker.right, -marker.width_m * 0.5, marker.width_m * 0.5),
                (marker.forward, -marker.depth_m * 0.5, marker.depth_m * 0.5),
                (marker.up, 0.0, marker.height_m),
            ] {
                let o = axis.dot(relative);
                let d = axis.dot(chord);
                if d == 0.0 {
                    if o < lo - radius || o > hi + radius {
                        far = -1.0;
                        break;
                    }
                } else {
                    let x = (lo - radius - o) / d;
                    let y = (hi + radius - o) / d;
                    near = near.max(x.min(y));
                    far = far.min(x.max(y));
                }
            }
            if near > far {
                continue;
            }
            let corners = marker.corners();
            for indices in BOX_FACES {
                let vertices = indices.map(|i| corners[i]);
                if segment_triangle_distance(start.p, end.p, vertices) > radius {
                    continue;
                }
                faces.push(Face {
                    vertices,
                    normal: (vertices[1] - vertices[0])
                        .cross(vertices[2] - vertices[0])
                        .normalized()
                        .ok_or(AtmosphereError::InvalidSample)?,
                    terrain: None,
                    marker: Some(marker.id),
                });
            }
        }
        if faces.is_empty() {
            return Ok(None);
        }
        let mut best: Option<(f64, State, Face)> = None;
        let mut ambiguous = false;
        if deviation <= EVENT_CURVE_TOLERANCE_M {
            for face in faces {
                let before = face.normal.dot(start.p - face.vertices[0]);
                let after = face.normal.dot(end.p - face.vertices[0]);
                if before * after > 0.0 || (before == 0.0 && after == 0.0) {
                    ambiguous = true;
                    continue;
                }
                let mut low = 0.0;
                let mut high = step;
                for _ in 0..40 {
                    if high - low <= EVENT_INTERVAL_M {
                        break;
                    }
                    let middle = (low + high) * 0.5;
                    let value = face
                        .normal
                        .dot(self.advance(start, middle)?.p - face.vertices[0]);
                    if value * before > 0.0 {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                let distance = (low + high) * 0.5;
                let state = self.advance(start, distance)?;
                if contains_point(face.vertices, state.p, 1e-10) {
                    if best.is_none_or(|b| distance < b.0) {
                        best = Some((distance, state, face));
                    }
                } else {
                    ambiguous = true;
                }
            }
            if !ambiguous && let Some((distance, state, face)) = best {
                return resolved_event(scene, face, state, distance, origin).map(Some);
            }
        }
        if deviation == 0.0 && best.is_none() {
            return Ok(None);
        }
        if step <= EVENT_INTERVAL_M {
            // At a shared edge one neighboring face may be inconclusive while
            // another brackets a verified root. The endpoint uncertainty is
            // retained by the local event tolerance; otherwise fail closed.
            if let Some((distance, state, face)) = best {
                return resolved_event(scene, face, state, distance, origin).map(Some);
            }
            return Err(AtmosphereError::NoConvergence);
        }
        let half_step = step * 0.5;
        let middle = self.advance(start, half_step)?;
        if let Some(event) = self.scene_event_inner(
            start, middle, half_step, scene, markers, origin, budget, cancelled,
        )? {
            return Ok(Some(event));
        }
        let refined_end = self.advance(middle, half_step)?;
        self.scene_event_inner(
            middle,
            refined_end,
            half_step,
            scene,
            markers,
            origin,
            budget,
            cancelled,
        )
        .map(|event| {
            event.map(|mut e| {
                e.distance += half_step;
                e
            })
        })
    }
}
