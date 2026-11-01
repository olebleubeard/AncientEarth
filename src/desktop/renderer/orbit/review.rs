//! Independent checks of the actual uploaded orbital body data and scalar f32 geometry.
//! This does not reproduce native GPU contraction/transcendental/derivative rules.
use super::*;
use earth_math::astro::{AstroInstant, Ephemeris};
use earth_math::rendering::orbit::{AU_M, body_intersection};
use earth_math::{Geodetic, Ray, geodetic_to_ecef};
use std::sync::OnceLock;

fn source() -> &'static OrbitFrame {
    static FRAME: OnceLock<OrbitFrame> = OnceLock::new();
    FRAME.get_or_init(|| {
        Ephemeris::from_bytes(include_bytes!("../../../../assets/astronomy/de440s.bsp"))
            .unwrap()
            .orbit_frame(
                AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap(),
                None,
            )
            .unwrap()
    })
}
fn scalar(body: Body, ray: Vec3d) -> Option<(f64, Vec3)> {
    let direction = vector(ray);
    let flatten = Vec3::new(1.0, 1.0, 1.0 / body.row2.w);
    let origin = body.eye.truncate() * flatten;
    let raw = Vec3::new(
        body.row0.truncate().dot(direction),
        body.row1.truncate().dot(direction),
        body.row2.truncate().dot(direction),
    );
    let d = raw * flatten;
    let speed = d.length();
    let u = d / speed;
    let cross = origin.cross(u);
    let disc = 1.0 - cross.dot(cross);
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let along = origin.dot(u);
    let near = (-along - root) / speed;
    let far = (-along + root) / speed;
    let distance = if near < 0.0 { far } else { near };
    if distance < 0.0 {
        return None;
    }
    Some((
        f64::from(distance * body.eye.w),
        body.eye.truncate() + raw * distance,
    ))
}
fn d(v: Vec3) -> Vec3d {
    Vec3d::new(f64::from(v.x), f64::from(v.y), f64::from(v.z))
}

#[test]
fn actual_uploaded_body_frames_match_geometry_with_finite_f32_distance_and_normal_errors() {
    let scene = source();
    let outward = Vec3d::new(0.6, -0.3, 0.74).normalized().unwrap();
    let right = outward
        .cross(Vec3d::new(0.0, 0.0, 1.0))
        .normalized()
        .unwrap();
    let up = right.cross(outward).normalized().unwrap();
    let mut maxima = [[0.0_f64; 2]; 2];
    let mut count = 0;
    for body in [OrbitBody::Earth, OrbitBody::Sun, OrbitBody::Moon] {
        let center = body.center(scene);
        let attitude = match body {
            OrbitBody::Earth => scene.earth_to_icrf,
            OrbitBody::Sun => scene.sun_to_icrf,
            OrbitBody::Moon => scene.moon_to_icrf,
        };
        for (range_group, range) in [
            (0, 1.05 * body.radius()),
            (0, 3.5 * body.radius()),
            (1, AU_M),
            (1, 4.0 * AU_M),
        ] {
            let eye = center + outward * range;
            let packed = Body::at(body, scene, eye);
            assert_eq!(packed.eye.w, body.radius() as f32);
            assert_eq!(
                packed.row2.w,
                if body == OrbitBody::Earth {
                    (WGS84_B_M / WGS84_A_M) as f32
                } else {
                    1.0
                }
            );
            for offset in [-0.8, -0.4, 0.0, 0.4, 0.8] {
                let direction =
                    (center + right * (offset * body.radius()) + up * (0.1 * body.radius()) - eye)
                        .normalized()
                        .unwrap();
                let truth = body_intersection(
                    body,
                    scene,
                    Ray {
                        origin: eye,
                        direction,
                    },
                )
                .unwrap();
                let (actual, point) = scalar(packed, direction).unwrap();
                let error = (actual - truth).abs();
                maxima[range_group][0] = maxima[range_group][0].max(error);
                let exact_point = attitude
                    .transpose()
                    .transform(eye + direction * truth - center)
                    / body.radius();
                let factor = if body == OrbitBody::Earth {
                    (WGS84_A_M / WGS84_B_M).powi(2)
                } else {
                    1.0
                };
                let exact_normal = Vec3d::new(exact_point.x, exact_point.y, exact_point.z * factor)
                    .normalized()
                    .unwrap();
                let gpu_normal = d((point
                    * Vec3::new(1.0, 1.0, 1.0 / (packed.row2.w * packed.row2.w)))
                .normalize());
                maxima[range_group][1] =
                    maxima[range_group][1].max(exact_normal.distance(gpu_normal));
                count += 1;
            }
        }
        // A valid camera around another focus can be inside this object.
        let packed = Body::at(body, scene, center);
        let direction = attitude.transform(Vec3d::new(1.0, 0.0, 0.0));
        let (exit, _) = scalar(packed, direction).unwrap();
        assert!((exit - body.radius()).abs() < body.radius() * 3e-7);
    }
    // Finite fixture guards, deliberately not all-view or ephemeris bounds.
    assert!(maxima[0][0] < 2_000.0 && maxima[0][1] < 2e-5, "{maxima:?}");
    assert!(maxima[1][0] < 300_000.0 && maxima[1][1] < 0.2, "{maxima:?}");
    println!(
        "Orbital scalar f32 {count} rays: near distance/normal-vector errors {:?}; 1–4 AU errors {:?}",
        maxima[0], maxima[1]
    );
}

#[test]
fn uploaded_earth_normal_uses_geodetic_latitude_and_body_attitude_for_global_categories() {
    let scene = source();
    let mut max_angle = 0.0_f64;
    for (lat, lon) in [
        (0.0, 0.0),
        (29.305549, -94.767015),
        (-43.0, 170.0),
        (75.0, -179.0),
        (-89.0, 179.0),
    ] {
        let ground = Geodetic::new(lat, lon, 0.0).unwrap();
        let normal = Vec3d::new(
            lat.to_radians().cos() * lon.to_radians().cos(),
            lat.to_radians().cos() * lon.to_radians().sin(),
            lat.to_radians().sin(),
        );
        let target =
            scene.earth_position_m + scene.earth_to_icrf.transform(geodetic_to_ecef(ground));
        let direction = -scene.earth_to_icrf.transform(normal);
        let eye = target - direction * 500_000.0;
        let body = Body::at(OrbitBody::Earth, scene, eye);
        let (_, point) = scalar(body, direction).unwrap();
        let normal32 = (point * Vec3::new(1.0, 1.0, 1.0 / (body.row2.w * body.row2.w))).normalize();
        let actual_lat = f64::from(normal32.z.clamp(-1.0, 1.0).asin()).to_degrees();
        let actual_lon = f64::from(normal32.y.atan2(normal32.x)).to_degrees();
        let error = (actual_lat - lat).abs().max((actual_lon - lon).abs());
        max_angle = max_angle.max(error);
        assert!(
            error < 0.0002,
            "lat/lon {lat},{lon}: {actual_lat},{actual_lon}"
        );
    }
    println!("Orbital scalar f32 finite geodetic-angle fixture maximum {max_angle} degrees");
}
