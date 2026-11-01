//! One canonical view from the ground to space. Coordinate anchoring and
//! tessellation preparation are display choices, never camera movement.
use super::{
    LocalFrame,
    orbit::{OrbitAnnotations, OrbitBody, OrbitLabel, OrbitStroke, OrbitText},
};
use crate::{
    CameraFrame, Geodetic, GeometryError, Lens, Observer, Projection, Vec3d, WGS84_A_M,
    astro::{
        AstronomyFrame,
        orbit::{OrbitFrame, OrbitTrail},
    },
    navigation::observer_from_ecef_pose,
    survey::project_chord_segments,
};

pub const SPACE_ANCHOR_ENTER_M: f64 = 2_000_000.0;
pub const SPACE_ANCHOR_LEAVE_M: f64 = 1_000_000.0;

/// Local 12 km shadow cascades are a near-Earth display effect. At remote f32
/// translations their first 12 m frustum can collapse along the light axis,
/// producing a zero depth interval. Geometry and eclipse attenuation remain
/// active; this finite presentation policy restores maps below 1,000 km.
pub fn nearby_shadow_maps(observer: Observer) -> bool {
    observer.eye_geodetic().height_m < 1_000_000.0
}

#[derive(Clone, Copy, Debug)]
pub struct ReferenceDetail {
    pub eye: Vec3d,
    pub movement_allowance_m: f64,
    pub minimum_distance_m: f64,
}
impl ReferenceDetail {
    pub fn near(eye: Vec3d) -> Self {
        Self {
            eye,
            movement_allowance_m: 0.0,
            minimum_distance_m: 0.05,
        }
    }
    /// The normal foot on the convex WGS84 ellipsoid is the nearest point.
    /// Its distance h and the distance-to-set Lipschitz bound give h-delta
    /// throughout this prepared eye neighborhood. Canonical source triangles
    /// are not simplified by this reference-ellipsoid presentation budget.
    pub fn space(observer: Observer) -> Self {
        let height = observer.eye_geodetic().height_m;
        Self {
            eye: observer.eye_position(),
            movement_allowance_m: height * 0.25,
            minimum_distance_m: height * 0.75,
        }
    }
    pub fn valid_for(self, observer: Observer, focal: f64, prepared_focal: f64) -> bool {
        focal.is_finite()
            && prepared_focal.is_finite()
            && focal > 0.0
            && focal <= prepared_focal
            && observer.eye_position().distance(self.eye) <= self.movement_allowance_m
    }
    pub fn cell_distance(self, center: Vec3d, radius: f64) -> f64 {
        (self.eye.distance(center) - radius - self.movement_allowance_m)
            .max(self.minimum_distance_m)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayPlan {
    pub coordinate_observer: Observer,
    pub local: LocalFrame,
    pub detail: ReferenceDetail,
    pub space: bool,
}
impl DisplayPlan {
    pub fn at(observer: Observer, was_space: bool) -> Self {
        let height = observer.eye_geodetic().height_m;
        let space = height >= SPACE_ANCHOR_ENTER_M || was_space && height >= SPACE_ANCHOR_LEAVE_M;
        let coordinate_observer = if space {
            Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap())
        } else {
            observer
        };
        Self {
            coordinate_observer,
            local: LocalFrame::at(coordinate_observer),
            detail: if space {
                ReferenceDetail::space(observer)
            } else {
                ReferenceDetail::near(observer.eye_position())
            },
            space,
        }
    }
}

/// Direction annotations become useful once the complete enclosing Earth
/// sphere fits inside 90% of the current vertical angular half-field. This
/// only controls annotations; it never changes camera, lens or renderer.
pub fn space_annotations_visible(observer: Observer, lens: Lens) -> bool {
    let distance = observer.eye_position().length();
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    distance > WGS84_A_M
        && (WGS84_A_M / distance).asin() <= 0.9 * lens.vertical_fov_deg.to_radians() * 0.5
}

/// Earth-relative ICRF view: rotate the canonical eye/axes without adding the
/// ~AU heliocentric Earth translation and subtracting it again near the ground.
pub fn celestial_view(observer: Observer, orbit: &OrbitFrame) -> CameraFrame {
    let view = observer.frame();
    let rotation = orbit.earth_to_icrf;
    CameraFrame {
        origin: rotation.transform(view.origin),
        right: rotation.transform(view.right),
        up: rotation.transform(view.up),
        forward: rotation.transform(view.forward),
    }
}

/// Optional explicit repositioning, not an automatic renderer/camera handoff.
/// Enclose the dated path and current bodies in a Sun-centered sphere, then
/// fit that sphere inside 90% of the limiting angular field. The active lens
/// remains unchanged; an unrepresentable narrow-lens fit returns an error.
pub fn fit_orbit_observer(
    observer: Observer,
    lens: Lens,
    orbit: &OrbitFrame,
    trail: Option<&OrbitTrail>,
    dimensions: (u32, u32),
) -> Result<Observer, GeometryError> {
    if dimensions.0 == 0 || dimensions.1 == 0 {
        return Err(GeometryError::InvalidDirection);
    }
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let half = lens.vertical_fov_deg.to_radians() * 0.5;
    let aspect = f64::from(dimensions.0) / f64::from(dimensions.1);
    let limiting = match lens.projection {
        Projection::Rectilinear => half.min((aspect * half.tan()).atan()),
        Projection::EquidistantFisheye => half * aspect.min(1.0),
    } * 0.9;
    let mut radius = (orbit.earth_position_m.length() + WGS84_A_M)
        .max(orbit.moon_position_m.length() + crate::astro::MOON_RADIUS_M)
        .max(crate::astro::SUN_RADIUS_M);
    if let Some(trail) = trail {
        for point in &trail.points {
            radius = radius.max(point.earth_position_m.length() + WGS84_A_M);
        }
    }
    let distance = radius / limiting.sin();
    let offset = (orbit.orbital_radial_icrf * 0.6 + orbit.orbital_normal_icrf * 0.8)
        .normalized()
        .ok_or(GeometryError::InvalidDirection)?;
    let inverse = orbit.earth_to_icrf.transpose();
    let eye = inverse.transform(offset * distance - orbit.earth_position_m);
    observer_from_ecef_pose(
        observer,
        eye,
        inverse.transform(-offset),
        inverse.transform(orbit.orbital_normal_icrf),
    )
}

/// Orbital direction/path annotations projected through the current canonical
/// view and actual optics. The body labels use the same apparent Sun/Moon
/// centres as the single world renderer; dated path/CIP are geometric context.
#[allow(clippy::too_many_arguments)]
pub fn continuous_annotations(
    observer: Observer,
    lens: Lens,
    orbit: &OrbitFrame,
    astronomy: &AstronomyFrame,
    trail: Option<&OrbitTrail>,
    display: [f32; 2],
    scale: [f32; 2],
    show_orbit: bool,
    show_axis: bool,
) -> OrbitAnnotations {
    let mut out = OrbitAnnotations::default();
    if !space_annotations_visible(observer, lens)
        || display
            .into_iter()
            .chain(scale)
            .any(|v| !v.is_finite() || v <= 0.0)
    {
        return out;
    }
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let physical = [
        f64::from(display[0]) * f64::from(scale[0]),
        f64::from(display[1]) * f64::from(scale[1]),
    ];
    if physical.into_iter().any(|v| v > 100_000.0) {
        return out;
    }
    let view = celestial_view(observer, orbit);
    let project = |point: Vec3d| -> Option<[f32; 2]> {
        let p = lens.project(view, point - view.origin, physical[0] / physical[1])?;
        Some([
            ((p[0] + 1.0) * 0.5 * physical[0] / f64::from(scale[0])) as f32,
            ((1.0 - p[1]) * 0.5 * physical[1] / f64::from(scale[1])) as f32,
        ])
    };
    let stroke = |output: &mut Vec<OrbitStroke>, a: Vec3d, b: Vec3d| {
        if output.len() >= 16_384 {
            return;
        }
        if let Some(segments) = project_chord_segments(a, b, view, lens, display, scale) {
            for [a, b] in segments.into_iter().take(16_384 - output.len()) {
                let logical = |p: [f64; 2]| {
                    [
                        (p[0] / f64::from(scale[0])) as f32,
                        (p[1] / f64::from(scale[1])) as f32,
                    ]
                };
                output.push(OrbitStroke {
                    a: logical(a),
                    b: logical(b),
                });
            }
        }
    };
    if show_orbit && let Some(trail) = trail {
        let epoch = orbit.instant.tt_seconds_j2000();
        for pair in trail.points.windows(2) {
            let a = pair[0].earth_position_m - orbit.earth_position_m;
            let b = pair[1].earth_position_m - orbit.earth_position_m;
            if pair[0].instant.tt_seconds_j2000() < epoch
                && epoch < pair[1].instant.tt_seconds_j2000()
            {
                stroke(&mut out.trail, a, Vec3d::ZERO);
                stroke(&mut out.trail, Vec3d::ZERO, b);
            } else {
                stroke(&mut out.trail, a, b);
            }
        }
    }
    let half = lens.vertical_fov_deg.to_radians() * 0.5;
    let focal = physical[1]
        / (2.0
            * match lens.projection {
                Projection::Rectilinear => half.tan(),
                Projection::EquidistantFisheye => half,
            });
    for (body, center) in [
        (OrbitBody::Earth, Vec3d::ZERO),
        (
            OrbitBody::Sun,
            orbit
                .earth_to_icrf
                .transform(astronomy.sun.apparent_center_ecef_m),
        ),
        (
            OrbitBody::Moon,
            orbit
                .earth_to_icrf
                .transform(astronomy.moon.apparent_center_ecef_m),
        ),
    ] {
        if let Some(position) = project(center) {
            out.bodies.push(OrbitLabel {
                position,
                label_position: [position[0] + 8.0, position[1] - 24.0],
                body,
                small_body: body.radius() * focal / center.distance(view.origin) < 4.0,
            });
        }
    }
    if show_axis {
        let depth = (-view.origin).dot(view.forward);
        if depth > 0.0 {
            let radius = (WGS84_A_M * 1.5).max(depth * 50.0 / focal);
            out.axis_exaggerated = radius > WGS84_A_M * 1.500001;
            for i in 0..96 {
                let point = |i: usize| {
                    let t = i as f64 * std::f64::consts::TAU / 96.0;
                    (orbit.orbital_radial_icrf * t.cos() + orbit.orbital_tangent_icrf * t.sin())
                        * radius
                };
                stroke(&mut out.plane, point(i), point(i + 1));
            }
            stroke(
                &mut out.axis,
                -orbit.axis_icrf * (radius * 1.3),
                orbit.axis_icrf * (radius * 1.3),
            );
            stroke(
                &mut out.normal,
                Vec3d::ZERO,
                orbit.orbital_normal_icrf * (radius * 1.3),
            );
            for (lines, text) in [
                (&out.axis, "Earth axis: CIP"),
                (&out.normal, "Orbital-plane normal"),
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
            if let Some(center) = project(Vec3d::ZERO) {
                out.labels.push(OrbitText {
                    position: [center[0] + 8.0, center[1] + 24.0],
                    anchor: center,
                    connect: true,
                    text: format!(
                        "Tilt: {:.3} deg to normal / {:.3} deg to plane",
                        orbit.axial_tilt_deg,
                        (90.0 - orbit.axial_tilt_deg).abs()
                    ),
                });
            }
            out.labels.push(OrbitText {
                position: [12.0, 36.0],
                anchor: [12.0, 36.0],
                connect: false,
                text: if out.axis_exaggerated {
                    "CIP axis / instantaneous orbital plane: enlarged direction annotations"
                } else {
                    "CIP axis / instantaneous orbital plane: direction annotations"
                }
                .into(),
            });
        }
    }
    out
}
