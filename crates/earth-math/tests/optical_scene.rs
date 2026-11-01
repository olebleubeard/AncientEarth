//! Author regression cases; independent optical/source review is separate.
use earth_math::{atmosphere::*, survey::*, *};
fn vacuum() -> AtmosphereProfile {
    AtmosphereProfile::from_layers(vec![
        AtmosphereLayer {
            height_m: 0.0,
            refractivity: 0.0,
        },
        AtmosphereLayer {
            height_m: 120000.0,
            refractivity: 0.0,
        },
    ])
    .unwrap()
}
fn patch(height: f64) -> TerrainPatch {
    TerrainPatch::from_grid(3, 3, -0.01, 0.01, 0.01, 0.01, vec![height; 9]).unwrap()
}
#[test]
fn zero_gradient_matches_canonical_terrain_and_negative_coverage() {
    for height in [-30.0, 0.0, 400.0] {
        let scene = TerrainScene::new(vec![patch(height)]);
        let eye = geodetic_to_ecef(Geodetic::new(0.001, 0.002, height + 80.0).unwrap());
        let ray = Ray::new(eye, -EnuBasis::at(ecef_to_geodetic(eye).unwrap()).up).unwrap();
        let expected = scene.intersect(ray).unwrap();
        let trace = vacuum().trace(ray, &scene).unwrap();
        let hit = trace.surface.unwrap();
        assert!(matches!(hit.kind, SurfaceKind::Terrain(0)));
        assert!(
            hit.hit.position.distance(expected.hit.position) < 0.00001,
            "{height}: {:?}",
            trace
        );
        assert!((trace.geometric_path_m - expected.hit.distance_m).abs() < 0.00001);
        assert!((trace.optical_path_m - trace.geometric_path_m).abs() < 1e-9);
    }
}
#[test]
fn thin_marker_is_hit_before_surface_and_order_is_distance_not_array_order() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    let eye = geodetic_to_ecef(Geodetic::new(0.0, 0.0, 10.0).unwrap());
    let basis = EnuBasis::at(Geodetic::new(0.0, 0.0, 0.0).unwrap());
    let marker = |id, distance| SurveyMarker {
        id,
        base_ecef_m: eye + basis.east * distance - basis.up * 10.0,
        right: basis.east,
        forward: basis.north,
        up: basis.up,
        width_m: 0.02,
        depth_m: 1.0,
        height_m: 20.0,
        along_distance_m: distance,
    };
    let near = marker(4, 100.0);
    let far = marker(9, 500.0);
    let ray = Ray::new(eye, basis.east).unwrap();
    let expected = near.intersect(ray).unwrap();
    let trace = vacuum().trace_scene(ray, &scene, &[far, near]).unwrap();
    assert_eq!(trace.marker.unwrap().marker_id, 4);
    assert!(trace.surface.is_none());
    assert!((trace.geometric_path_m - expected).abs() < 1e-5);
}
#[test]
fn fixed_weather_anchor_does_not_follow_each_ray() {
    let request = weather::reference_request();
    let raw = weather::decode_gfs(
        include_bytes!("../../../assets/weather/gfs-reference.grib2"),
        &request,
    )
    .unwrap()
    .profile;
    let ground = TerrainPatch::from_grid(3, 3, -94.76, 29.26, 0.01, 0.01, vec![-20.0; 9]).unwrap();
    let scene = TerrainScene::new(vec![ground]);
    let mapped = raw.anchored_to_scene(&scene).unwrap();
    assert!((mapped.base_height_m() + 20.0).abs() < 0.2);
    assert_eq!(mapped.layers(), raw.layers());
    let base = mapped.base_height_m();
    assert_eq!(
        mapped.index_gradient(base - 100.0),
        (1.0 + mapped.layers()[0].refractivity, 0.0)
    );
    assert!((mapped.top_height_m() - base - raw.top_height_m()).abs() < 1e-8);
    assert_eq!(raw.base_height_m(), 0.0);
    assert!(mapped.metadata.unwrap().temperature_80m_k.is_some());
}
#[test]
fn requested_anchor_resolves_small_mountain_patch_outside_weather_grid_post() {
    let raw = weather::decode_gfs(
        include_bytes!("../../../assets/weather/gfs-reference.grib2"),
        &weather::reference_request(),
    )
    .unwrap()
    .profile;
    let observer = Geodetic::new(29.305549, -94.767015, 4800.0).unwrap();
    let patch = TerrainPatch::from_grid(
        3,
        3,
        observer.lon_deg - 0.001,
        observer.lat_deg + 0.001,
        0.001,
        0.001,
        vec![4500.0; 9],
    )
    .unwrap();
    let scene = TerrainScene::new(vec![patch]);
    let old = raw.anchored_to_scene(&scene).unwrap();
    assert!(
        old.base_height_m().abs() < 1e-7,
        "source grid point is outside the small terrain crop"
    );
    let bound = raw.anchored_to_scene_at(&scene, observer).unwrap();
    assert!((bound.base_height_m() - 4500.0).abs() < 0.01);
    let anchor = bound.anchor().unwrap();
    assert_eq!(anchor.location.lat_deg, observer.lat_deg);
    assert_eq!(anchor.location.lon_deg, observer.lon_deg);
    assert_eq!(bound.layers(), raw.layers());
    let before = bound.index_gradient(bound.base_height_m() + 2.0);
    for height in [0.0, 4500.0, 4600.0] {
        let _ = bound.index_gradient(height);
    }
    assert_eq!(bound.anchor(), Some(anchor));
    assert_eq!(bound.index_gradient(bound.base_height_m() + 2.0), before);
    assert_eq!(bound.metadata.as_ref().unwrap().latitude_deg, 29.25);
    let smooth = bound
        .anchored_to_scene_at(&TerrainScene::boundary_only(vec![]).unwrap(), observer)
        .unwrap();
    assert!(smooth.base_height_m().abs() < 1e-7);
    assert!(
        (bound.base_height_m() - 4500.0).abs() < 0.01,
        "replacement must not mutate old profile"
    );
}
#[test]
fn inverse_vacuum_target_and_centerline_visibility_match_geometric_definition() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    let observer = Observer::new(Geodetic::new(20.0, 30.0, 0.0).unwrap());
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let markers = place_markers(&scene, observer, MarkerSettings::default(), &cancelled).unwrap();
    let marker = &markers[10];
    let target = marker.centerline(marker.height_m);
    let (ray, path) = vacuum()
        .direction_to_point(observer.eye_position(), target, &cancelled)
        .unwrap();
    assert!(
        ray.direction
            .distance((target - observer.eye_position()).normalized().unwrap())
            < 1e-14
    );
    assert!((path - observer.eye_position().distance(target)).abs() < 2e-6);
    let optical = vacuum()
        .marker_occlusion(&scene, observer.eye_position(), marker, &cancelled)
        .unwrap();
    let geometric = marker_occlusion(&scene, observer.eye_position(), marker, &cancelled).unwrap();
    assert!((optical.hidden_from_base_m - geometric.hidden_from_base_m).abs() < 0.002);
    assert_eq!(optical.method, OcclusionMethod::SampledRefractedBranch);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        vacuum()
            .trace_scene_cancellable(ray, &scene, &markers, &cancelled)
            .is_err()
    );
    assert!(
        vacuum()
            .direction_to_point(observer.eye_position(), target, &cancelled)
            .is_err()
    );
}
