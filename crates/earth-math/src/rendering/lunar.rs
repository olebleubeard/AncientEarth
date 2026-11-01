//! Source-map registration and explicitly enlarged lunar direction annotations.
use super::{
    RenderMesh,
    orbit::{OrbitAnnotations, OrbitStroke, OrbitText},
};
use crate::{
    Lens, Observer, Projection, Ray, TerrainScene, Vec3d,
    astro::{AstronomyFrame, MOON_RADIUS_M},
    survey::project_chord_segments,
};

/// Equirectangular pixel-edge UV: east longitude [-pi,pi), north at v=0.
/// Pixel centers are ((column+.5)/width,(row+.5)/height).
pub fn map_uv(body_direction: Vec3d) -> Option<[f64; 2]> {
    let n = body_direction.normalized()?;
    Some([
        (n.y.atan2(n.x) / std::f64::consts::TAU + 0.5).rem_euclid(1.0),
        0.5 - n.z.clamp(-1.0, 1.0).asin() / std::f64::consts::PI,
    ])
}

/// Normal to p=(R+h)n. Heights and derivatives are metres and metres/radian.
/// The tiny |cos(latitude)|<1e-12 pole band uses a radial fallback; no exaggeration.
pub fn normal_from_height_gradient(
    latitude: f64,
    longitude: f64,
    height: f64,
    dh_dlon: f64,
    dh_dlat: f64,
) -> Option<Vec3d> {
    if ![latitude, longitude, height, dh_dlon, dh_dlat]
        .into_iter()
        .all(f64::is_finite)
        || latitude.abs() > std::f64::consts::FRAC_PI_2
        || MOON_RADIUS_M + height <= 0.0
    {
        return None;
    }
    let (sp, cp) = latitude.sin_cos();
    let (sl, cl) = longitude.sin_cos();
    let n = Vec3d::new(cp * cl, cp * sl, sp);
    if cp.abs() < 1e-12 {
        return Some(n);
    }
    let east = Vec3d::new(-sl, cl, 0.0);
    let north = Vec3d::new(-sp * cl, -sp * sl, cp);
    (n - east * (dh_dlon / ((MOON_RADIUS_M + height) * cp))
        - north * (dh_dlat / (MOON_RADIUS_M + height)))
        .normalized()
}

/// Uncalibrated, full-phase-normalized single-scattering disk function.
pub fn lommel_seeliger(mu0: f64, mu: f64) -> Option<f64> {
    if !mu0.is_finite() || !mu.is_finite() {
        return None;
    }
    let a = mu0.clamp(0.0, 1.0);
    let b = mu.clamp(0.0, 1.0);
    Some(if a == 0.0 || b == 0.0 {
        0.0
    } else {
        2.0 * a / (a + b)
    })
}

/// Mean-radius display sphere. Geography is sampled in the fragment shader.
pub fn mesh() -> RenderMesh {
    let mut mesh = RenderMesh::default();
    let columns = 384usize;
    let rows = 192usize;
    for row in 0..=rows {
        let latitude =
            std::f64::consts::FRAC_PI_2 - std::f64::consts::PI * row as f64 / rows as f64;
        for column in 0..=columns {
            let longitude = std::f64::consts::TAU * column as f64 / columns as f64;
            let n = [
                (latitude.cos() * longitude.cos()) as f32,
                (latitude.cos() * longitude.sin()) as f32,
                latitude.sin() as f32,
            ];
            mesh.positions.push(n);
            mesh.normals.push(n);
            mesh.colors.push([1.0; 4]);
        }
    }
    for row in 0..rows {
        for column in 0..columns {
            let nw = (row * (columns + 1) + column) as u32;
            let sw = nw + columns as u32 + 1;
            mesh.indices.extend([nw, sw, sw + 1, nw, sw + 1, nw + 1]);
        }
    }
    mesh
}

/// Straight-view direction diagram. Caller omits it during refracted viewing;
/// these strokes are screen annotations, not lunar geometry or measured lengths.
pub fn annotations(
    observer: Observer,
    lens: Lens,
    frame: &AstronomyFrame,
    scene: &TerrainScene,
    display: [f32; 2],
    scale: [f32; 2],
) -> OrbitAnnotations {
    let mut out = OrbitAnnotations::default();
    if display
        .into_iter()
        .chain(scale)
        .any(|x| !x.is_finite() || x <= 0.0)
    {
        return out;
    }
    let physical = [
        f64::from(display[0]) * f64::from(scale[0]),
        f64::from(display[1]) * f64::from(scale[1]),
    ];
    if physical.into_iter().any(|x| x > 100000.0) {
        return out;
    }
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let view = observer.frame();
    if frame.sample_observer_ecef_m != view.origin {
        return out;
    }
    let center = frame.moon.apparent_center_ecef_m;
    let delta = center - view.origin;
    if scene
        .intersect(Ray {
            origin: view.origin,
            direction: delta,
        })
        .is_some_and(|hit| hit.hit.distance_m < delta.length() - frame.moon.radius_m)
    {
        return out;
    }
    let Some(projected) = lens.project(view, delta, physical[0] / physical[1]) else {
        return out;
    };
    let position = [
        ((projected[0] + 1.0) * 0.5 * f64::from(display[0])) as f32,
        ((1.0 - projected[1]) * 0.5 * f64::from(display[1])) as f32,
    ];
    let half = lens.vertical_fov_deg.to_radians() * 0.5;
    let focal = physical[1]
        / (2.0
            * match lens.projection {
                Projection::Rectilinear => half.tan(),
                Projection::EquidistantFisheye => half,
            });
    // Centered angular display scale; enlarged annotations have no distance meaning.
    let radius = (MOON_RADIUS_M * 1.35).max(delta.length() * 45.0 / focal);
    out.axis_exaggerated = radius > MOON_RADIUS_M * 1.350001;
    let stroke = |lines: &mut Vec<OrbitStroke>, a, b| {
        if let Some(segments) = project_chord_segments(a, b, view, lens, display, scale) {
            for [a, b] in segments {
                let p = |v: [f64; 2]| {
                    [
                        (v[0] / f64::from(scale[0])) as f32,
                        (v[1] / f64::from(scale[1])) as f32,
                    ]
                };
                lines.push(OrbitStroke { a: p(a), b: p(b) });
            }
        }
    };
    let orientation = frame.lunar_orientation;
    for i in 0..96 {
        let point = |i: usize| {
            let t = i as f64 * std::f64::consts::TAU / 96.0;
            center
                + (orientation.orbital_radial_ecef * t.cos()
                    + orientation.orbital_tangent_ecef * t.sin())
                    * radius
        };
        stroke(&mut out.plane, point(i), point(i + 1));
    }
    stroke(
        &mut out.axis,
        center - orientation.axis_ecef * (radius * 1.3),
        center + orientation.axis_ecef * (radius * 1.3),
    );
    stroke(
        &mut out.normal,
        center,
        center + orientation.orbital_normal_ecef * (radius * 1.3),
    );
    for (lines, text) in [
        (
            &out.axis,
            if out.axis_exaggerated {
                "Moon north pole: text PCK (enlarged direction guide)"
            } else {
                "Moon north pole: text PCK (direction guide)"
            },
        ),
        (&out.normal, "Lunar orbit normal (emission epoch)"),
    ] {
        if let Some(line) = lines.last() {
            out.labels.push(OrbitText {
                position: [line.b[0] + 8.0, line.b[1] - 8.0],
                anchor: line.b,
                connect: true,
                text: text.into(),
            });
        }
    }
    out.labels.push(OrbitText {
        position: [position[0] + 8.0, position[1] + 24.0],
        anchor: position,
        connect: true,
        text: format!(
            "Moon tilt: {:.3} deg to orbit normal / {:.3} deg to plane",
            orientation.tilt_deg,
            (90.0 - orientation.tilt_deg).abs()
        ),
    });
    out
}
