//! Finite, one-image inverse projection of the physical A-to-B tape chord.
//! This is a sampled annotation, not an enumeration of all ducted images.
use super::{AtmosphereError, AtmosphereProfile};
use crate::{
    Lens, Observer, TerrainScene, Vec3d,
    survey::{SurveyMarker, TapeAnnotation, TapePoint},
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

/// Project the unchanged measured ECEF chord through the same optical field.
/// Every displayed sample uses inverse shooting and canonical scene occlusion.
/// Sixteen initial intervals are refined at visible transitions or when their
/// midpoint differs from the screen chord by >0.2 physical pixel. The finite
/// 1025-sample/six-level policy cannot certify arbitrarily narrow visibility
/// windows or discover every mirage branch. An unresolved solve is an error;
/// it is never replaced by a straight projected point.
#[allow(clippy::too_many_arguments)]
pub fn tape_annotation(
    profile: &AtmosphereProfile,
    scene: &TerrainScene,
    markers: &[SurveyMarker],
    a: Option<TapePoint>,
    b: Option<TapePoint>,
    observer: Observer,
    lens: Lens,
    viewport: [f32; 2],
    framebuffer_scale: [f32; 2],
    cancelled: &AtomicBool,
) -> Result<Option<TapeAnnotation>, AtmosphereError> {
    let Some(a) = a else { return Ok(None) };
    let size = [
        f64::from(viewport[0]) * f64::from(framebuffer_scale[0]),
        f64::from(viewport[1]) * f64::from(framebuffer_scale[1]),
    ];
    if viewport
        .into_iter()
        .chain(framebuffer_scale)
        .any(|v| !v.is_finite() || v <= 0.0)
        || size.into_iter().any(|v| !v.is_finite() || v > 100_000.0)
    {
        return Err(AtmosphereError::InvalidSample);
    }
    let frame = observer.frame();
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let project = |target: Vec3d| -> Result<Option<[f64; 2]>, AtmosphereError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(AtmosphereError::NoConvergence);
        }
        let (ray, _) = profile.direction_to_point(frame.origin, target, cancelled)?;
        let Some(ndc) = lens.project(frame, ray.direction, size[0] / size[1]) else {
            return Ok(None);
        };
        let normal = (target - frame.origin)
            .normalized()
            .ok_or(AtmosphereError::InvalidSample)?;
        let (hit, plane) =
            match profile.trace_inner(ray, scene, markers, Some((target, normal)), cancelled) {
                Ok(result) => result,
                Err(AtmosphereError::NoConvergence) if !cancelled.load(Ordering::Relaxed) => {
                    // Clipping an integration step at a target on a shared face
                    // edge can leave its contact unresolved. An independent full
                    // trace may establish the actual FIRST physical contact with
                    // that same endpoint. Accept only that existing contact band;
                    // a sky result or a distant hit proves no endpoint visibility.
                    let hit = profile.trace_scene_cancellable(ray, scene, markers, cancelled)?;
                    if (hit.surface.is_none() && hit.marker.is_none())
                        || hit.exit_ray.origin.distance(target) > 0.005
                    {
                        return Err(AtmosphereError::NoConvergence);
                    }
                    (hit, None)
                }
                Err(error) => return Err(error),
            };
        // The inverse stops within 2 mm of the target plane; a picked endpoint
        // may touch its own surface. This 5 mm endpoint band is numerical, not
        // a claim that the whole integrated path is accurate to that distance.
        if plane.is_none() && hit.exit_ray.origin.distance(target) > 0.005 {
            return Ok(None);
        }
        Ok(Some([
            (ndc[0] + 1.0) * size[0] * 0.5,
            (1.0 - ndc[1]) * size[1] * 0.5,
        ]))
    };
    let logical = |p: [f64; 2]| {
        [
            (p[0] / f64::from(framebuffer_scale[0])) as f32,
            (p[1] / f64::from(framebuffer_scale[1])) as f32,
        ]
    };
    let endpoint_a = project(a.position_ecef_m)?;
    let Some(b) = b else {
        return Ok(endpoint_a.map(|p| TapeAnnotation {
            quads: Vec::new(),
            label_position: logical(p),
            endpoint_positions: [Some(logical(p)), None],
        }));
    };
    let endpoint_b = project(b.position_ecef_m)?;
    let mut cache = HashMap::from([(0u32, endpoint_a), (1024u32, endpoint_b)]);
    let mut sample = |index: u32| -> Result<Option<[f64; 2]>, AtmosphereError> {
        if let Some(value) = cache.get(&index) {
            return Ok(*value);
        }
        if cache.len() >= 1025 {
            return Err(AtmosphereError::NoConvergence);
        }
        let p = a.position_ecef_m
            + (b.position_ecef_m - a.position_ecef_m) * (f64::from(index) / 1024.0);
        let value = project(p)?;
        cache.insert(index, value);
        Ok(value)
    };
    let mut stack: Vec<_> = (0..16u32).rev().map(|i| (i * 64, (i + 1) * 64)).collect();
    let mut segments = Vec::new();
    while let Some((lo, hi)) = stack.pop() {
        let p = sample(lo)?;
        let q = sample(hi)?;
        if hi - lo <= 1 {
            if let (Some(p), Some(q)) = (p, q) {
                segments.push([p, q]);
            }
            continue;
        }
        let middle = (lo + hi) / 2;
        let r = sample(middle)?;
        let split = match (p, q, r) {
            (Some(p), Some(q), Some(r)) => {
                let d = [q[0] - p[0], q[1] - p[1]];
                let norm = d[0] * d[0] + d[1] * d[1];
                let t = if norm > 0.0 {
                    (((r[0] - p[0]) * d[0] + (r[1] - p[1]) * d[1]) / norm).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (r[0] - p[0] - d[0] * t).hypot(r[1] - p[1] - d[1] * t) > 0.2
            }
            (None, None, None) => false,
            _ => true,
        };
        if split {
            stack.push((middle, hi));
            stack.push((lo, middle));
        } else if let (Some(p), Some(q)) = (p, q) {
            segments.push([p, q]);
        }
    }
    let mut quads = Vec::new();
    let mut label = endpoint_a.or(endpoint_b);
    let total: f64 = segments
        .iter()
        .map(|[p, q]| (q[0] - p[0]).hypot(q[1] - p[1]))
        .sum();
    let mut remaining = total * 0.5;
    for [p, q] in segments {
        let length = (q[0] - p[0]).hypot(q[1] - p[1]);
        if length <= 1e-8 {
            continue;
        }
        if remaining >= 0.0 && remaining <= length {
            let t = remaining / length;
            label = Some([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            remaining = -1.0;
        } else if remaining >= 0.0 {
            remaining -= length;
        }
        let n = [-(q[1] - p[1]) / length * 0.5, (q[0] - p[0]) / length * 0.5];
        quads.push(
            [
                [p[0] + n[0], p[1] + n[1]],
                [q[0] + n[0], q[1] + n[1]],
                [q[0] - n[0], q[1] - n[1]],
                [p[0] - n[0], p[1] - n[1]],
            ]
            .map(|v| logical([v[0].clamp(0.0, size[0]), v[1].clamp(0.0, size[1])])),
        );
    }
    Ok(label.map(|label| TapeAnnotation {
        quads,
        label_position: logical(label),
        endpoint_positions: [endpoint_a.map(logical), endpoint_b.map(logical)],
    }))
}
