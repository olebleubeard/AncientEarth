//! Independent inverse-image and finite tape-annotation checks. Synthetic
//! columns isolate the equations; no meteorological accuracy is inferred.
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile, projection::tape_annotation},
    survey::{SurveyMarker, SurveyTarget, TapePoint},
    *,
};
use std::sync::atomic::AtomicBool;

fn column(knots: &[(f64, f64)]) -> AtmosphereProfile {
    AtmosphereProfile::from_layers(
        knots
            .iter()
            .map(|&(height_m, refractivity)| AtmosphereLayer {
                height_m,
                refractivity,
            })
            .collect(),
    )
    .unwrap()
}
fn vacuum() -> AtmosphereProfile {
    column(&[(0.0, 0.0), (1000.0, 0.0)])
}
fn marker(base: Vec3d, height: f64, id: usize) -> SurveyMarker {
    SurveyMarker {
        id,
        base_ecef_m: base,
        right: Vec3d::new(0.0, 0.0, -1.0),
        forward: Vec3d::new(0.0, 1.0, 0.0),
        up: Vec3d::new(1.0, 0.0, 0.0),
        width_m: 10.0,
        depth_m: 10.0,
        height_m: height,
        along_distance_m: 0.0,
    }
}
fn point(position_ecef_m: Vec3d) -> TapePoint {
    TapePoint {
        position_ecef_m,
        target: SurveyTarget::Marker(0),
    }
}
fn near(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b} within {t}");
}

#[test]
fn inverse_recovers_exact_circular_ray_and_vacuum_tangent_hidden_height() {
    let scene = TerrainScene::new(vec![]);
    let cancel = AtomicBool::new(false);
    let radius = WGS84_A_M + 1000.0;
    let n = 1.0003;
    let slope = -n / radius;
    let circular = column(&[
        (0.0, 0.0003 - slope * 1000.0),
        (2000.0, 0.0003 + slope * 1000.0),
        (120000.0, 0.0),
    ]);
    let eye = Vec3d::new(radius, 0.0, 0.0);
    for length in [100.0_f64, 1000.0, 3000.0] {
        let angle = length / radius;
        let target = Vec3d::new(radius * angle.cos(), radius * angle.sin(), 0.0);
        let (ray, path) = circular.direction_to_point(eye, target, &cancel).unwrap();
        near(path, length, 0.003);
        assert!(ray.direction.distance(Vec3d::new(0.0, 1.0, 0.0)) < 2.1e-5);
        if length >= 1000.0 {
            assert!(ray.direction.distance((target - eye).normalized().unwrap()) > 5e-5);
        }
        let hit = circular
            .trace_to_plane(ray, &scene, target, (target - eye).normalized().unwrap())
            .unwrap()
            .unwrap();
        assert!(hit.distance(target) <= 0.0021);
    }
    // Equatorial WGS84 is exactly a circle of radius a. Its near-side tangent
    // from height e has alpha=acos(a/(a+e)); a radial target at theta emerges
    // at height a/cos(theta-alpha)-a, independently of the inverse solver.
    let theta = 0.002_f64;
    let eye_height = 2.0;
    let up = Vec3d::new(theta.cos(), theta.sin(), 0.0);
    let mut target = marker(up * WGS84_A_M, 100.0, 7);
    target.up = up;
    target.forward = Vec3d::new(-theta.sin(), theta.cos(), 0.0);
    let eye = Vec3d::new(WGS84_A_M + eye_height, 0.0, 0.0);
    let expected =
        WGS84_A_M / (theta - (WGS84_A_M / (WGS84_A_M + eye_height)).acos()).cos() - WGS84_A_M;
    let hidden = vacuum()
        .marker_occlusion(&scene, eye, &target, &cancel)
        .unwrap();
    assert_eq!(hidden.hidden_intervals_m.len(), 1);
    near(hidden.hidden_from_base_m, expected, 0.003);
    near(hidden.sampled_total_hidden_m, expected, 0.003);
    near(hidden.sampling_step_m, 100.0 / 64.0, 0.0);
    assert!(
        vacuum()
            .marker_occlusion(&scene, eye, &target, &AtomicBool::new(true))
            .is_err()
    );
    // A floating finite plateau hides a middle band, not a monotone prefix.
    // Its two z=0 edge points give independent 2D line-intersection heights.
    let west = 0.0048;
    let east = 0.0052;
    let latitude = 0.0002;
    let patch = TerrainPatch::from_grid(
        2,
        2,
        west,
        latitude,
        east - west,
        2.0 * latitude,
        vec![60.0; 4],
    )
    .unwrap();
    let edges = [west, east].map(|lon| {
        (geodetic_to_ecef(Geodetic::new(latitude, lon, 60.0).unwrap())
            + geodetic_to_ecef(Geodetic::new(-latitude, lon, 60.0).unwrap()))
            * 0.5
    });
    let scene = TerrainScene::new(vec![patch]);
    let theta = 0.01_f64.to_radians();
    let up = Vec3d::new(theta.cos(), theta.sin(), 0.0);
    let mut target = marker(up * WGS84_A_M, 30.48, 11);
    target.up = up;
    target.forward = Vec3d::new(-theta.sin(), theta.cos(), 0.0);
    let eye = Vec3d::new(WGS84_A_M + 100.0, 0.0, 0.0);
    let cross = |a: Vec3d, b: Vec3d| a.x * b.y - a.y * b.x;
    let mut limits =
        edges.map(|p| -cross(p - eye, target.base_ecef_m - eye) / cross(p - eye, target.up));
    limits.sort_by(f64::total_cmp);
    assert!(limits[0] > 1.0 && limits[1] < 29.0);
    let hidden = vacuum()
        .marker_occlusion(&scene, eye, &target, &cancel)
        .unwrap();
    assert_eq!(hidden.hidden_from_base_m, 0.0);
    assert_eq!(hidden.hidden_intervals_m.len(), 1);
    near(hidden.hidden_intervals_m[0][0], limits[0], 0.0011);
    near(hidden.hidden_intervals_m[0][1], limits[1], 0.0011);
    near(hidden.sampled_total_hidden_m, limits[1] - limits[0], 0.0022);
}

#[test]
fn inverse_plane_visibility_checks_objects_in_every_vacuum_segment() {
    let scene = TerrainScene::new(vec![]);
    let field = vacuum();
    for (eye_height, obstacle_height, target_height) in [
        (5000.0, 6000.0, 7000.0),
        (500.0, 2000.0, 3000.0),
        (5000.0, 3000.0, 500.0),
    ] {
        let eye = Vec3d::new(WGS84_A_M + eye_height, 0.0, 0.0);
        let target = Vec3d::new(WGS84_A_M + target_height, 0.0, 0.0);
        let obstacle = marker(Vec3d::new(WGS84_A_M + obstacle_height, 0.0, 0.0), 30.0, 1);
        let direction = (target - eye).normalized().unwrap();
        let ray = Ray::new(eye, direction).unwrap();
        assert!(
            field
                .trace_to_plane_scene(ray, &scene, &[], target, direction)
                .unwrap()
                .is_some()
        );
        assert!(
            field
                .trace_to_plane_scene(ray, &scene, &[obstacle], target, direction)
                .unwrap()
                .is_none(),
            "vacuum ordering failed at eye {eye_height}"
        );
        let closer = eye + direction * 100.0;
        assert!(
            field
                .trace_to_plane_scene(ray, &scene, &[obstacle], closer, direction)
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn inverse_tape_preserves_physical_pixel_width_projection_and_occlusion() {
    let scene = TerrainScene::new(vec![]);
    let field = vacuum();
    let cancel = AtomicBool::new(false);
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 100.0).unwrap());
    observer.heading_deg = 90.0;
    observer.pitch_deg = 0.0;
    observer.roll_deg = 31.0;
    let frame = observer.frame();
    let a = point(frame.origin + frame.forward * 100.0 - frame.right * 20.0 + frame.up * 5.0);
    let b = point(frame.origin + frame.forward * 100.0 + frame.right * 20.0 - frame.up * 5.0);
    for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
        let lens = Lens::new(projection, 60.0);
        for scale in [[1.0_f32, 1.0], [1.5, 1.5], [1.25, 2.0]] {
            let view = [800.0_f32, 600.0];
            let annotation = tape_annotation(
                &field,
                &scene,
                &[],
                Some(a),
                Some(b),
                observer,
                lens,
                view,
                scale,
                &cancel,
            )
            .unwrap()
            .unwrap();
            assert!(!annotation.quads.is_empty());
            let w = f64::from(view[0] * scale[0]);
            let h = f64::from(view[1] * scale[1]);
            let focal = h / (2.0 * (30.0_f64.to_radians()));
            for (source, display) in [a, b].into_iter().zip(annotation.endpoint_positions) {
                let d = source.position_ecef_m - frame.origin;
                let x = d.dot(frame.right);
                let y = d.dot(frame.up);
                let z = d.dot(frame.forward);
                let (px, py) = match projection {
                    Projection::Rectilinear => {
                        let f = h / (2.0 * 30.0_f64.to_radians().tan());
                        (w * 0.5 + f * x / z, h * 0.5 - f * y / z)
                    }
                    Projection::EquidistantFisheye => {
                        let r = x.hypot(y);
                        let k = focal * r.atan2(z) / r;
                        (w * 0.5 + k * x, h * 0.5 - k * y)
                    }
                };
                let actual = display.unwrap();
                near(f64::from(actual[0]) * f64::from(scale[0]), px, 0.00015);
                near(f64::from(actual[1]) * f64::from(scale[1]), py, 0.00015);
            }
            for q in annotation.quads {
                let dx = f64::from(q[0][0] - q[3][0]) * f64::from(scale[0]);
                let dy = f64::from(q[0][1] - q[3][1]) * f64::from(scale[1]);
                near(dx.hypot(dy), 1.0, 0.00015);
            }
            assert!(
                tape_annotation(
                    &field,
                    &scene,
                    &[],
                    Some(a),
                    None,
                    observer,
                    lens,
                    view,
                    scale,
                    &cancel
                )
                .unwrap()
                .unwrap()
                .endpoint_positions[0]
                    .is_some()
            );
        }
    }
    // The blocker lies halfway between the eye and both endpoint images.
    let mut blocker = marker(
        frame.origin + frame.forward * 50.0 - Vec3d::new(20.0, 0.0, 0.0),
        40.0,
        3,
    );
    blocker.width_m = 100.0;
    assert!(
        tape_annotation(
            &field,
            &scene,
            &[blocker],
            Some(a),
            Some(b),
            observer,
            Lens::default(),
            [800.0, 600.0],
            [1.0, 1.0],
            &cancel
        )
        .unwrap()
        .is_none()
    );
    assert!(
        tape_annotation(
            &field,
            &scene,
            &[],
            Some(a),
            Some(b),
            observer,
            Lens::default(),
            [800.0, 600.0],
            [1.0, 1.0],
            &AtomicBool::new(true)
        )
        .is_err()
    );
}
