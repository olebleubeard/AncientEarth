//! Independent canonical-surface and curved-event checks. Synthetic columns
//! below isolate geometry; they are not weather forecasts or accuracy claims.
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile},
    survey::SurveyMarker,
    *,
};

fn profile(knots: &[(f64, f64)]) -> AtmosphereProfile {
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
fn plate(height: f64) -> TerrainScene {
    TerrainScene::new(vec![
        TerrainPatch::from_grid(2, 2, -0.001, 0.001, 0.002, 0.002, vec![height; 4]).unwrap(),
    ])
}
fn ray(height: f64, longitude: f64) -> Ray {
    let eye = Geodetic::new(0.0, longitude, height).unwrap();
    Ray::new(geodetic_to_ecef(eye), -EnuBasis::at(eye).up).unwrap()
}
fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} within {tolerance}"
    );
}

#[test]
fn zero_gradient_retains_exact_source_faces_and_masks_reference_only_under_coverage() {
    let vacuum = profile(&[(0.0, 0.0), (120000.0, 0.0)]);
    for height in [-30.0, 0.0, 250.0, 3500.0] {
        let scene = plate(height);
        for longitude in [0.0, 0.0003, 0.01] {
            let launch = ray(5000.0, longitude);
            let straight = scene.intersect(launch).unwrap();
            let trace = vacuum.trace(launch, &scene).unwrap();
            let curved = trace.surface.unwrap();
            near(trace.geometric_path_m, straight.hit.distance_m, 3e-5);
            near(trace.optical_path_m, trace.geometric_path_m, 3e-8);
            assert!(curved.hit.position.distance(straight.hit.position) < 3e-5);
            assert_eq!(curved.kind, straight.kind);
            // Independent plane oracle: all four symmetric source posts have
            // the same x, so the central inward ray hits exactly that plane.
            if longitude == 0.0 {
                let vertex = geodetic_to_ecef(Geodetic::new(0.001, 0.001, height).unwrap());
                near(curved.hit.position.x, vertex.x, 3e-5);
                if height < 0.0 {
                    assert!(curved.hit.geodetic.height_m < -29.0);
                }
            } else if longitude == 0.01 {
                assert!(matches!(curved.kind, SurfaceKind::ReferenceEllipsoid));
                near(trace.geometric_path_m, 5000.0, 3e-5);
            }
        }
    }
    // Canonical triangle rays reject exactly parallel/coplanar face travel:
    // no isolated plane crossing exists. The zero-gradient optical path keeps
    // that convention; it does not promise to report every on-face contact.
    let scene = plate(100.0);
    let x = scene.patches()[0].canonical_vertices()[0].x;
    let coplanar = Ray::new(Vec3d::new(x, 0.0, 0.0), Vec3d::new(0.0, 1.0, 0.0)).unwrap();
    assert!(scene.intersect(coplanar).is_none());
    assert!(vacuum.trace(coplanar, &scene).unwrap().surface.is_none());
}

fn circular_marker(distance: f64, radial_height: f64, thickness: f64, id: usize) -> SurveyMarker {
    let radius = WGS84_A_M + 1000.0;
    let angle = distance / radius;
    let up = Vec3d::new(angle.cos(), angle.sin(), 0.0);
    SurveyMarker {
        id,
        base_ecef_m: up * (radius - radial_height * 0.5),
        right: Vec3d::new(0.0, 0.0, -1.0),
        forward: Vec3d::new(-angle.sin(), angle.cos(), 0.0),
        up,
        width_m: 10.0,
        depth_m: thickness,
        height_m: radial_height,
        along_distance_m: distance,
    }
}

#[test]
fn curved_ray_finds_thin_marker_that_its_long_chord_misses_and_orders_both_boxes() {
    // At the equator, n'(r)=-n(r0)/r0 gives an exact circular solution
    // r(s)=r0*(cos(s/r0),sin(s/r0),0) for a tangential launch at r0.
    let radius = WGS84_A_M + 1000.0;
    let index = 1.0003;
    let slope = -index / radius;
    let column = profile(&[
        (0.0, 0.0003 - slope * 1000.0),
        (2000.0, 0.0003 + slope * 1000.0),
        (120000.0, 0.0),
    ]);
    let scene = TerrainScene::new(vec![]);
    let launch = Ray::new(Vec3d::new(radius, 0.0, 0.0), Vec3d::new(0.0, 1.0, 0.0)).unwrap();
    let near_box = circular_marker(1000.0, 0.01, 0.002, 47);
    let far_box = circular_marker(1500.0, 0.01, 0.002, 12);
    // Full 2km endpoint chord misses the entire 1cm radial slab at 1km.
    let end = Vec3d::new(
        radius * (2000.0 / radius).cos(),
        radius * (2000.0 / radius).sin(),
        0.0,
    );
    let chord = Ray::new(launch.origin, end - launch.origin).unwrap();
    assert!(near_box.intersect(chord).is_none());
    for boxes in [[near_box, far_box], [far_box, near_box]] {
        let trace = column.trace_scene(launch, &scene, &boxes).unwrap();
        assert!(trace.surface.is_none());
        let hit = trace.marker.unwrap();
        assert_eq!(hit.marker_id, 47);
        let expected = 1000.0 - radius * (0.001 / radius).asin();
        near(trace.geometric_path_m, expected, 0.0003);
        near(trace.optical_path_m, index * expected, 0.0003);
        let theta = expected / radius;
        let expected_position = Vec3d::new(radius * theta.cos(), radius * theta.sin(), 0.0);
        assert!(hit.position_ecef_m.distance(expected_position) < 0.0003);
    }
}

#[test]
fn bvh_candidate_queries_cover_closed_chords_without_changing_source_triangles() {
    let scene = plate(100.0);
    let patch = &scene.patches()[0];
    let mesh = patch.optical_mesh();
    assert_eq!(mesh.triangles.len(), 2);
    let source = mesh
        .triangles
        .iter()
        .map(|indices| indices.map(|i| mesh.vertices[i as usize]))
        .collect::<Vec<_>>();
    let launch = ray(1000.0, 0.0);
    let end = launch.origin + launch.direction * 1500.0;
    let candidates = scene.optical_candidates(launch.origin, end, 0.0).unwrap();
    assert_eq!(candidates.len(), 2); // closed shared diagonal belongs to both
    for face in candidates {
        assert!(source.contains(&face.vertices));
    }
    let vertex = source[0][0];
    assert!(
        !scene
            .optical_candidates(vertex, vertex, 0.0)
            .unwrap()
            .is_empty()
    );
    assert!(
        scene
            .optical_candidates(launch.origin, launch.origin, 0.1)
            .unwrap()
            .is_empty()
    );
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(scene.optical_candidates(launch.origin, end, bad).is_err());
    }
}

#[test]
fn reference_contacts_vacuum_marker_segments_and_vertical_inverse_have_explicit_results() {
    use std::sync::atomic::AtomicBool;
    let vacuum = profile(&[(0.0, 0.0), (1000.0, 0.0)]);
    let smooth = TerrainScene::new(vec![]);
    let contact = vacuum.trace(ray(0.0, 0.0), &smooth).unwrap();
    assert!(contact.surface.is_some());
    near(contact.geometric_path_m, 0.0, 1e-6);
    assert!(vacuum.trace(ray(-1.0, 0.0), &smooth).is_err());
    let below = vacuum.trace(ray(-1.0, 0.0), &plate(-30.0)).unwrap();
    assert!(below.surface.unwrap().hit.geodetic.height_m < -29.0);
    for (eye_height, base_height, upward, expected) in [
        (5000.0, 3000.0, false, 1970.0),
        (5000.0, 6000.0, true, 1000.0),
        (500.0, 2000.0, true, 1500.0),
    ] {
        let mut marker = circular_marker(0.0, 30.0, 3.0, 9);
        marker.base_ecef_m = Vec3d::new(WGS84_A_M + base_height, 0.0, 0.0);
        let direction = Vec3d::new(if upward { 1.0 } else { -1.0 }, 0.0, 0.0);
        let launch = Ray::new(Vec3d::new(WGS84_A_M + eye_height, 0.0, 0.0), direction).unwrap();
        let result = vacuum.trace_scene(launch, &smooth, &[marker]).unwrap();
        assert_eq!(result.marker.unwrap().marker_id, 9);
        near(result.geometric_path_m, expected, 3e-5);
    }
    let eye = Vec3d::new(WGS84_A_M + 1000.0, 0.0, 0.0);
    for target_height in [600.0, 1200.0] {
        let target = Vec3d::new(WGS84_A_M + target_height, 0.0, 0.0);
        let (launch, path) = vacuum
            .direction_to_point(eye, target, &AtomicBool::new(false))
            .unwrap();
        near(path, eye.distance(target), 2e-6);
        assert!(
            launch
                .direction
                .distance((target - eye).normalized().unwrap())
                < 1e-14
        );
        assert!(
            vacuum
                .direction_to_point(eye, target, &AtomicBool::new(true))
                .is_err()
        );
    }
}
