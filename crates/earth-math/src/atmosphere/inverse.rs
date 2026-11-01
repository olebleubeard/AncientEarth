//! Bounded inverse shooting for survey-centerline visibility. This follows
//! one image branch seeded by the straight chord; it does not enumerate ducts.
use super::*;
use crate::survey::{MarkerOcclusion, OcclusionMethod, SurveyMarker};
use std::sync::atomic::{AtomicBool, Ordering};

impl AtmosphereProfile {
    fn field_to_plane(
        &self,
        ray: Ray,
        point: Vec3d,
        normal: Vec3d,
        cancelled: &AtomicBool,
    ) -> Result<(Vec3d, f64), AtmosphereError> {
        let mut state = State {
            p: ray.origin,
            u: ray.direction,
            optical: 0.0,
        };
        let mut path = 0.0;
        let mut proposed = MAX_STEP_M;
        let curvature = self.maximum_gradient + 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M);
        for _ in 0..MAX_TRACE_STEPS {
            if cancelled.load(Ordering::Relaxed) || path > MAX_ATMOSPHERIC_PATH_M {
                return Err(AtmosphereError::NoConvergence);
            }
            let (height, up) = Self::geometry(state.p)?;
            let signed = normal.dot(state.p - point);
            let toward = -signed.signum() * normal.dot(state.u);
            let root = (toward * toward + 2.0 * self.maximum_gradient * signed.abs()).sqrt();
            let bound = if self.maximum_gradient == 0.0 {
                if toward > 0.0 {
                    signed.abs() / toward
                } else {
                    MAX_STEP_M
                }
            } else if toward >= 0.0 {
                2.0 * signed.abs() / (root + toward)
            } else {
                (root - toward) / self.maximum_gradient
            };
            let step = proposed
                .min(MAX_STEP_M)
                .min(self.layer_step(height, state.u.dot(up), curvature))
                .min((bound * 1.001).max(0.01));
            let full = self.midpoint(state, step)?;
            let half = self.advance(state, step)?;
            let error = full
                .p
                .distance(half.p)
                .max((full.u - half.u).length() * step)
                .max((full.optical - half.optical).abs())
                / 3.0;
            if !error.is_finite() {
                return Err(AtmosphereError::NoConvergence);
            }
            if error > POSITION_TOLERANCE_M {
                if step <= 0.01000001 {
                    return Err(AtmosphereError::NoConvergence);
                }
                proposed = step * 0.5;
                continue;
            }
            let after = normal.dot(half.p - point);
            if signed * after <= 0.0 {
                let mut lo = 0.0;
                let mut hi = step;
                for _ in 0..40 {
                    if hi - lo < 1e-6 {
                        break;
                    }
                    let mid = (lo + hi) * 0.5;
                    let value = normal.dot(self.advance(state, mid)?.p - point);
                    if value * signed > 0.0 {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let distance = (lo + hi) * 0.5;
                return Ok((self.advance(state, distance)?.p, path + distance));
            }
            state = half;
            path += step;
            proposed = if error < POSITION_TOLERANCE_M * 0.125 {
                step * 2.0
            } else {
                step
            };
        }
        Err(AtmosphereError::NoConvergence)
    }
    /// Numerical inverse on one continuously updated image branch. The point
    /// residual is bounded by 2 mm when this returns; accumulated integration
    /// error and weather/model uncertainty are not thereby bounded to 2 mm.
    pub fn direction_to_point(
        &self,
        eye: Vec3d,
        target: Vec3d,
        cancelled: &AtomicBool,
    ) -> Result<(Ray, f64), AtmosphereError> {
        let normal = (target - eye)
            .normalized()
            .ok_or(AtmosphereError::InvalidSample)?;
        let up =
            EnuBasis::at(ecef_to_geodetic(eye).map_err(|_| AtmosphereError::InvalidSample)?).up;
        let fallback = if normal.x.abs() <= normal.y.abs() && normal.x.abs() <= normal.z.abs() {
            Vec3d::new(1.0, 0.0, 0.0)
        } else if normal.y.abs() <= normal.z.abs() {
            Vec3d::new(0.0, 1.0, 0.0)
        } else {
            Vec3d::new(0.0, 0.0, 1.0)
        };
        let right = normal
            .cross(up)
            .normalized()
            .or_else(|| normal.cross(fallback).normalized())
            .ok_or(AtmosphereError::InvalidSample)?;
        let vertical = right.cross(normal).normalized().unwrap();
        let mut direction = normal;
        for _ in 0..16 {
            if cancelled.load(Ordering::Relaxed) {
                return Err(AtmosphereError::NoConvergence);
            }
            let ray = Ray::new(eye, direction).ok_or(AtmosphereError::InvalidSample)?;
            let (point, path) = self.field_to_plane(ray, target, normal, cancelled)?;
            let residual = point - target;
            if residual.length() <= 0.002 {
                return Ok((ray, path));
            }
            let epsilon = 1e-6;
            let sample = |axis: Vec3d| -> Result<Vec3d, AtmosphereError> {
                let ray = Ray::new(eye, direction + axis * epsilon)
                    .ok_or(AtmosphereError::InvalidSample)?;
                Ok((self.field_to_plane(ray, target, normal, cancelled)?.0 - point) / epsilon)
            };
            let x = sample(right)?;
            let y = sample(vertical)?;
            let a = x.dot(right);
            let b = y.dot(right);
            let c = x.dot(vertical);
            let d = y.dot(vertical);
            let determinant = a * d - b * c;
            if !determinant.is_finite() || determinant.abs() < 1e-12 {
                return Err(AtmosphereError::NoConvergence);
            }
            let rx = residual.dot(right);
            let ry = residual.dot(vertical);
            let dx = (d * rx - b * ry) / determinant;
            let dy = (a * ry - c * rx) / determinant;
            let length = dx.hypot(dy);
            let scale = if length > 0.05 { 0.05 / length } else { 1.0 };
            direction = (direction - (right * dx + vertical * dy) * scale)
                .normalized()
                .ok_or(AtmosphereError::NoConvergence)?;
        }
        Err(AtmosphereError::NoConvergence)
    }
    pub fn marker_occlusion(
        &self,
        scene: &TerrainScene,
        eye: Vec3d,
        marker: &SurveyMarker,
        cancelled: &AtomicBool,
    ) -> Result<MarkerOcclusion, AtmosphereError> {
        if !marker.valid() {
            return Err(AtmosphereError::InvalidSample);
        }
        const INTERVALS: usize = 64;
        const HEIGHT_BRACKET: f64 = 0.001;
        let hidden = |height: f64| -> Result<bool, AtmosphereError> {
            let target = marker.centerline(height);
            let (ray, path) = self.direction_to_point(eye, target, cancelled)?;
            let normal = (target - eye)
                .normalized()
                .ok_or(AtmosphereError::InvalidSample)?;
            let (trace, plane) =
                self.trace_inner(ray, scene, &[], Some((target, normal)), cancelled)?;
            Ok(plane.is_none() && trace.surface.is_some() && trace.geometric_path_m < path - 0.005)
        };
        let mut previous = hidden(0.0)?;
        let mut start = 0.0;
        let mut intervals = Vec::new();
        let spacing = marker.height_m / INTERVALS as f64;
        for index in 1..=INTERVALS {
            let height = index as f64 * spacing;
            let current = hidden(height)?;
            if current != previous {
                let mut lo = (index - 1) as f64 * spacing;
                let mut hi = height;
                for _ in 0..32 {
                    if hi - lo <= HEIGHT_BRACKET {
                        break;
                    }
                    let middle = (lo + hi) * 0.5;
                    if hidden(middle)? == previous {
                        lo = middle;
                    } else {
                        hi = middle;
                    }
                }
                let crossing = (lo + hi) * 0.5;
                if previous {
                    intervals.push([start, crossing]);
                } else {
                    start = crossing;
                }
            }
            previous = current;
        }
        if previous {
            intervals.push([start, marker.height_m]);
        }
        Ok(MarkerOcclusion {
            hidden_from_base_m: intervals
                .first()
                .filter(|r| r[0] == 0.0)
                .map_or(0.0, |r| r[1]),
            sampled_total_hidden_m: intervals.iter().map(|r| r[1] - r[0]).sum(),
            sampling_step_m: spacing,
            boundary_tolerance_m: HEIGHT_BRACKET,
            method: OcclusionMethod::SampledRefractedBranch,
            hidden_intervals_m: intervals,
        })
    }
}
