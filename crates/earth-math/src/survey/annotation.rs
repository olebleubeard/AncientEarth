//! A screen annotation of the actual ECEF chord. It deliberately ignores depth
//! occlusion; clipping the drawing never changes the measured endpoint length.
use super::TapePoint;
use crate::{CameraFrame, Lens, Projection, Vec3d};

#[derive(Clone, Debug)]
pub struct TapeAnnotation {
    /// Logical-coordinate polygons, one physical pixel perpendicular to each
    /// segment. Fill without line antialiasing; diagonal raster coverage varies.
    pub quads: Vec<[[f32; 2]; 4]>,
    pub label_position: [f32; 2],
    pub endpoint_positions: [Option<[f32; 2]>; 2],
}

/// Visible endpoint label location, including the single-A selection state.
/// This uses the same front guard, physical aspect and viewport as the line.
pub fn tape_endpoint_position(
    point: TapePoint,
    frame: CameraFrame,
    lens: Lens,
    viewport: [f32; 2],
    framebuffer_scale: [f32; 2],
) -> Option<[f32; 2]> {
    if viewport
        .into_iter()
        .chain(framebuffer_scale)
        .any(|v| !v.is_finite() || v <= 0.0)
    {
        return None;
    }
    let size = [
        f64::from(viewport[0]) * f64::from(framebuffer_scale[0]),
        f64::from(viewport[1]) * f64::from(framebuffer_scale[1]),
    ];
    if size.into_iter().any(|v| !v.is_finite() || v > 100_000.0) {
        return None;
    }
    let direction = point.position_ecef_m - frame.origin;
    if !direction.is_finite() || direction.dot(frame.forward) < 1e-6 {
        return None;
    }
    let ndc = lens.project(frame, direction, size[0] / size[1])?;
    let pixels = physical(ndc, size);
    Some([
        (pixels[0] / f64::from(framebuffer_scale[0])) as f32,
        (pixels[1] / f64::from(framebuffer_scale[1])) as f32,
    ])
}

fn clip_linear(interval: &mut [f64; 2], a: f64, b: f64) -> bool {
    if a < 0.0 && b < 0.0 {
        return false;
    }
    if a < 0.0 {
        interval[0] = interval[0].max(a / (a - b));
    }
    if b < 0.0 {
        interval[1] = interval[1].min(a / (a - b));
    }
    interval[0] <= interval[1]
}
fn ndc(p: Vec3d, lens: Lens, aspect: f64) -> [f64; 2] {
    let half = lens.vertical_fov_deg.to_radians() * 0.5;
    match lens.projection {
        Projection::Rectilinear => [p.x / (p.z * half.tan() * aspect), p.y / (p.z * half.tan())],
        Projection::EquidistantFisheye => {
            let r = p.x.hypot(p.y);
            if r == 0.0 {
                [0.0, 0.0]
            } else {
                let scale = r.atan2(p.z) / (half * r);
                [p.x * scale / aspect, p.y * scale]
            }
        }
    }
}
fn physical(ndc: [f64; 2], size: [f64; 2]) -> [f64; 2] {
    [
        (ndc[0] + 1.0) * size[0] * 0.5,
        (1.0 - ndc[1]) * size[1] * 0.5,
    ]
}
fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn length(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn segment_clip(a: [f64; 2], b: [f64; 2], size: [f64; 2]) -> Option<[[f64; 2]; 2]> {
    let mut range = [0.0, 1.0];
    for axis in 0..2 {
        if !clip_linear(&mut range, a[axis], b[axis])
            || !clip_linear(&mut range, size[axis] - a[axis], size[axis] - b[axis])
        {
            return None;
        }
    }
    Some([lerp(a, b, range[0]), lerp(a, b, range[1])])
}

/// Project the actual A–B chord as an always-visible measurement annotation.
/// Clips at a one-micrometre camera-front plane, the optical aperture and the
/// viewport. Fisheye curvature is adaptively sampled to a 0.1 physical-pixel
/// midpoint target, with quarter probes and bounded subdivision; this is a
/// finite display criterion, not a geodetic or global pixel-error guarantee.
pub fn project_chord_segments(
    a: Vec3d,
    b: Vec3d,
    frame: CameraFrame,
    lens: Lens,
    viewport: [f32; 2],
    framebuffer_scale: [f32; 2],
) -> Option<Vec<[[f64; 2]; 2]>> {
    if viewport
        .into_iter()
        .chain(framebuffer_scale)
        .any(|v| !v.is_finite() || v <= 0.0)
    {
        return None;
    }
    let size = [
        f64::from(viewport[0]) * f64::from(framebuffer_scale[0]),
        f64::from(viewport[1]) * f64::from(framebuffer_scale[1]),
    ];
    if size.into_iter().any(|v| !v.is_finite() || v > 100_000.0) {
        return None;
    }
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let aspect = size[0] / size[1];
    let local = |p: Vec3d| {
        let d = p - frame.origin;
        Vec3d::new(d.dot(frame.right), d.dot(frame.up), d.dot(frame.forward))
    };
    let original = [local(a), local(b)];
    if original.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let [a, b] = original;
    let delta = b - a;
    let mut range = [0.0, 1.0];
    if !clip_linear(&mut range, a.z - 1e-6, b.z - 1e-6) {
        return None;
    }
    let half = lens.vertical_fov_deg.to_radians() * 0.5;
    match lens.projection {
        Projection::Rectilinear => {
            for (x, y) in [
                (
                    a.z * half.tan() * aspect - a.x,
                    b.z * half.tan() * aspect - b.x,
                ),
                (
                    a.z * half.tan() * aspect + a.x,
                    b.z * half.tan() * aspect + b.x,
                ),
                (a.z * half.tan() - a.y, b.z * half.tan() - b.y),
                (a.z * half.tan() + a.y, b.z * half.tan() + b.y),
            ] {
                if !clip_linear(&mut range, x, y) {
                    return None;
                }
            }
        }
        Projection::EquidistantFisheye => {
            // A forward circular cone is convex. The maximum z/|P| on a
            // segment is at an endpoint or this sole stationary parameter.
            let inside = |t: f64| {
                let p = a + delta * t;
                p.x.hypot(p.y).atan2(p.z) <= half + 8.0 * f64::EPSILON
            };
            let denominator = delta.z * a.dot(delta) - a.z * delta.dot(delta);
            let stationary = -(delta.z * a.dot(a) - a.z * a.dot(delta)) / denominator;
            let middle = stationary.clamp(range[0], range[1]);
            let seed = [range[0], range[1], middle]
                .into_iter()
                .find(|t| t.is_finite() && inside(*t))?;
            if !inside(range[0]) {
                let (mut lo, mut hi) = (range[0], seed);
                for _ in 0..64 {
                    let mid = (lo + hi) * 0.5;
                    if inside(mid) { hi = mid } else { lo = mid }
                }
                range[0] = hi;
            }
            if !inside(range[1]) {
                let (mut lo, mut hi) = (seed, range[1]);
                for _ in 0..64 {
                    let mid = (lo + hi) * 0.5;
                    if inside(mid) { lo = mid } else { hi = mid }
                }
                range[1] = lo;
            }
        }
    }
    let project = |t: f64| physical(ndc(a + delta * t, lens, aspect), size);
    let mut stack = vec![(range[0], range[1], 0u8)];
    let mut segments = Vec::new();
    while let Some((lo, hi, depth)) = stack.pop() {
        let p = project(lo);
        let q = project(hi);
        let needs_split = lens.projection == Projection::EquidistantFisheye
            && [0.25, 0.5, 0.75].into_iter().any(|t| {
                let actual = project(lo + (hi - lo) * t);
                let expected = lerp(p, q, t);
                // Distance to the chord, not parameter speed along that chord.
                let d = [q[0] - p[0], q[1] - p[1]];
                let norm = d[0] * d[0] + d[1] * d[1];
                let closest = if norm > 0.0 {
                    lerp(
                        p,
                        q,
                        (((actual[0] - p[0]) * d[0] + (actual[1] - p[1]) * d[1]) / norm)
                            .clamp(0.0, 1.0),
                    )
                } else {
                    expected
                };
                length(actual, closest) > 0.1
            });
        if needs_split {
            if depth >= 24 || stack.len() + segments.len() > 4096 {
                return None;
            }
            let mid = (lo + hi) * 0.5;
            stack.push((mid, hi, depth + 1));
            stack.push((lo, mid, depth + 1));
        } else if let Some(segment) = segment_clip(p, q, size)
            && length(segment[0], segment[1]) > 1e-8
        {
            segments.push(segment);
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments)
}

/// Tape drawing built from the shared physical-pixel chord projection. The
/// finite clipping/sampling policy is identical for tape and orbital strokes.
pub fn tape_annotation(
    a: TapePoint,
    b: TapePoint,
    frame: CameraFrame,
    lens: Lens,
    viewport: [f32; 2],
    framebuffer_scale: [f32; 2],
) -> Option<TapeAnnotation> {
    let segments = project_chord_segments(
        a.position_ecef_m,
        b.position_ecef_m,
        frame,
        lens,
        viewport,
        framebuffer_scale,
    )?;
    let size = [
        f64::from(viewport[0]) * f64::from(framebuffer_scale[0]),
        f64::from(viewport[1]) * f64::from(framebuffer_scale[1]),
    ];
    let endpoints =
        [a, b].map(|point| tape_endpoint_position(point, frame, lens, viewport, framebuffer_scale));
    let total: f64 = segments.iter().map(|s| length(s[0], s[1])).sum();
    let mut remaining = total * 0.5;
    let mut label = segments[0][0];
    let mut quads = Vec::with_capacity(segments.len());
    for [p, q] in segments {
        let len = length(p, q);
        if remaining >= 0.0 && remaining <= len {
            label = lerp(p, q, remaining / len);
            remaining = -1.0;
        } else if remaining >= 0.0 {
            remaining -= len;
        }
        let n = [-(q[1] - p[1]) / len * 0.5, (q[0] - p[0]) / len * 0.5];
        let convert = |v: [f64; 2]| {
            [
                (v[0].clamp(0.0, size[0]) / f64::from(framebuffer_scale[0])) as f32,
                (v[1].clamp(0.0, size[1]) / f64::from(framebuffer_scale[1])) as f32,
            ]
        };
        quads.push(
            [
                [p[0] + n[0], p[1] + n[1]],
                [q[0] + n[0], q[1] + n[1]],
                [q[0] - n[0], q[1] - n[1]],
                [p[0] - n[0], p[1] - n[1]],
            ]
            .map(convert),
        );
    }
    Some(TapeAnnotation {
        quads,
        label_position: [
            ((label[0] + 8.0).min(size[0]) / f64::from(framebuffer_scale[0])) as f32,
            ((label[1] + 8.0).min(size[1]) / f64::from(framebuffer_scale[1])) as f32,
        ],
        endpoint_positions: endpoints,
    })
}
