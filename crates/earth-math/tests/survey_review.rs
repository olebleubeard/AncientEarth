//! Independent placement, visibility and presentation checks. PROJ direct
//! geodesics are a related-algorithm comparison; the tangent and plate-edge
//! visibility oracles below are independently derived Cartesian geometry.
use earth_math::rendering::{LocalFrame, survey as display};
use earth_math::survey::*;
use earth_math::*;
use std::sync::atomic::AtomicBool;

#[path = "fixtures/survey_reference.rs"]
mod reference;

fn geo(lat: f64, lon: f64, h: f64) -> Geodetic {
    Geodetic::new(lat, lon, h).unwrap()
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "actual={actual:.17e}, expected={expected:.17e}, tolerance={tolerance:.2e}"
    );
}
fn vector(v: [f32; 3]) -> Vec3d {
    Vec3d::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}
fn marker_at(g: Geodetic) -> SurveyMarker {
    let basis = EnuBasis::at(g);
    SurveyMarker {
        id: 0,
        base_ecef_m: geodetic_to_ecef(g),
        right: basis.east,
        forward: basis.north,
        up: basis.up,
        width_m: 3.048,
        depth_m: 3.048,
        height_m: 30.48,
        along_distance_m: 0.0,
    }
}

#[test]
fn startup_and_geodesic_row_match_exact_units_and_independent_direct_fixtures() {
    let default = earth_math::boundary::default_observer();
    assert_eq!(default.feet, geo(29.305549, -94.767015, 0.0));
    assert_eq!(default.heading_deg, 90.0);
    assert_eq!(default.pitch_deg, 0.0);
    let settings = MarkerSettings::default();
    assert_eq!(settings.validate().unwrap(), 81);
    assert_eq!(settings.spacing_m, 804.672);
    assert_eq!(settings.reach_m, 64_373.76);
    assert_eq!(settings.left_offset_m, 30.48);
    let scene = TerrainScene::new(Vec::new());
    let near = nearby_demo(&scene).unwrap();
    assert_eq!(near.observer.feet.lat_deg, default.feet.lat_deg);
    assert_eq!(near.observer.feet.lon_deg, default.feet.lon_deg);
    assert!(near.observer.feet.height_m.abs() < 1e-8);
    assert_eq!(near.marker_id, 0);
    assert_eq!(near.markers[0].along_distance_m, 0.0);
    let expected = (near.markers[0].centerline(15.24) - near.observer.eye_position())
        .normalized()
        .unwrap();
    assert!(near.observer.frame().forward.distance(expected) < 1e-14);
    for &(start, station, expected) in reference::ROWS {
        let mut observer = Observer::new(geo(start[0], start[1], 500.0));
        observer.heading_deg = start[2];
        observer.pitch_deg = 71.0; // Row uses horizontal azimuth, not view pitch.
        observer.roll_deg = -34.0;
        let markers = place_markers(&scene, observer, settings, &AtomicBool::new(false)).unwrap();
        let marker = markers[station];
        assert_eq!(marker.id, station);
        assert_eq!(marker.along_distance_m, station as f64 * 804.672);
        let actual = ecef_to_geodetic(marker.base_ecef_m).unwrap();
        close(actual.lat_deg, expected[0], 3e-12);
        close(actual.lon_deg, expected[1], 3e-12);
        close(actual.height_m, 0.0, 8e-9);
        let basis = EnuBasis::at(actual);
        let heading = marker
            .forward
            .dot(basis.east)
            .atan2(marker.forward.dot(basis.north))
            .to_degrees();
        close(
            (heading - expected[2] + 180.0).rem_euclid(360.0) - 180.0,
            0.0,
            3e-10,
        );
        close(
            marker.right.cross(marker.forward).distance(marker.up),
            0.0,
            4e-15,
        );
        let c = marker.corners();
        close(c[0].distance(c[1]), 3.048, 2e-9);
        close(c[1].distance(c[2]), 3.048, 2e-9);
        close(c[0].distance(c[4]), 30.48, 2e-9);
        close(
            (c[..4].iter().copied().fold(Vec3d::ZERO, |a, b| a + b) / 4.0)
                .distance(marker.base_ecef_m),
            0.0,
            2e-9,
        );
    }
    for bad in [
        MarkerSettings {
            spacing_m: 0.0,
            ..settings
        },
        MarkerSettings {
            reach_m: f64::NAN,
            ..settings
        },
        MarkerSettings {
            left_offset_m: -1.0,
            ..settings
        },
        MarkerSettings {
            spacing_m: 3.048,
            ..settings
        },
    ] {
        assert!(bad.validate().is_err());
    }
    assert!(matches!(
        place_markers(&scene, default, settings, &AtomicBool::new(true)),
        Err(SurveyError::Cancelled)
    ));
}

#[test]
fn prism_slab_distances_inside_exits_and_earth_occlusion_are_geometric() {
    let scene = TerrainScene::new(Vec::new());
    let marker = marker_at(geo(0.0, 0.0, 0.0));
    let origin = marker.centerline(10.0) + marker.right * 20.0;
    for scale in [1e-200, 0.01, 1.0, 53.0, 1e200] {
        let ray = Ray {
            origin,
            direction: -marker.right * scale,
        };
        close(marker.intersect(ray).unwrap(), 20.0 - 1.524, 2e-12);
        let hit = pick_marker(&scene, &[marker], ray).unwrap();
        assert_eq!(hit.point.target, SurveyTarget::Marker(0));
        close(hit.distance_m, 18.476, 2e-12);
        close(
            hit.point
                .position_ecef_m
                .distance(marker.centerline(10.0) + marker.right * 1.524),
            0.0,
            2e-12,
        );
    }
    close(
        marker
            .intersect(Ray {
                origin: marker.centerline(10.0),
                direction: marker.up * 7.0,
            })
            .unwrap(),
        20.48,
        1e-9,
    );
    assert!(
        marker
            .intersect(Ray {
                origin,
                direction: marker.up
            })
            .is_none()
    );
    assert!(
        marker
            .intersect(Ray {
                origin,
                direction: marker.right
            })
            .is_none()
    );
    assert!(
        marker
            .intersect(Ray {
                origin,
                direction: Vec3d::ZERO
            })
            .is_none()
    );
    let mut other = marker;
    other.id = 7;
    other.base_ecef_m = marker.base_ecef_m + marker.right * 8.0;
    let ray = Ray {
        origin,
        direction: -marker.right,
    };
    assert_eq!(
        pick_marker(&scene, &[marker, other], ray)
            .unwrap()
            .point
            .target,
        SurveyTarget::Marker(7)
    );
    let antipodal = marker_at(geo(0.0, 180.0, 0.0));
    let through_earth = Ray {
        origin: marker.centerline(100.0),
        direction: antipodal.centerline(10.0) - marker.centerline(100.0),
    };
    assert!(antipodal.intersect(through_earth).is_some());
    assert!(pick_marker(&scene, &[antipodal], through_earth).is_none());
    assert_eq!(
        pick(&scene, &[antipodal], through_earth)
            .unwrap()
            .point
            .target,
        SurveyTarget::Surface
    );
    let mut invalid = marker;
    invalid.up = -invalid.up;
    assert!(invalid.intersect(ray).is_none());
}

#[test]
fn tape_chord_and_projected_surface_arc_are_distinct_and_symmetric() {
    let make = |g| TapePoint {
        position_ecef_m: geodetic_to_ecef(g),
        target: SurveyTarget::Surface,
    };
    let a = make(geo(0.0, 0.0, 50.0));
    let b = make(geo(0.0, 60.0, 50.0));
    let measurement = TapeMeasurement::between(a, b).unwrap();
    close(measurement.straight_distance_m, WGS84_A_M + 50.0, 2e-9);
    close(
        measurement.surface_distance_m,
        WGS84_A_M * std::f64::consts::PI / 3.0,
        3e-9,
    );
    assert_eq!(measurement, TapeMeasurement::between(b, a).unwrap());
    let elevated = make(geo(0.0, 0.0, 150.0));
    let vertical = TapeMeasurement::between(a, elevated).unwrap();
    close(vertical.straight_distance_m, 100.0, 0.0);
    close(vertical.surface_distance_m, 0.0, 0.0);
    assert_eq!(
        TapeMeasurement::between(a, a).unwrap().straight_distance_m,
        0.0
    );
    assert!(
        TapeMeasurement::between(
            a,
            TapePoint {
                position_ecef_m: Vec3d::ZERO,
                target: SurveyTarget::Surface
            }
        )
        .is_err()
    );
}

#[test]
fn smooth_hidden_centerline_matches_independent_high_precision_tangent_solution() {
    let scene = TerrainScene::new(Vec::new());
    let eye = Vec3d::new(WGS84_A_M + 1.72, 0.0, 0.0);
    for &(arc, expected) in reference::HIDDEN {
        let marker = marker_at(geo(0.0, (arc / WGS84_A_M).to_degrees(), 0.0));
        let result = marker_occlusion(&scene, eye, &marker, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.method, OcclusionMethod::SmoothEllipsoidBisection);
        assert_eq!(result.sampling_step_m, 0.0);
        close(result.hidden_from_base_m, expected, 3e-5);
        close(result.sampled_total_hidden_m, expected, 3e-5);
        if expected > 0.0 {
            assert_eq!(result.hidden_intervals_m[0][0], 0.0);
        }
        let raised = marker_occlusion(
            &scene,
            eye + Vec3d::new(100.0, 0.0, 0.0),
            &marker,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(raised.hidden_from_base_m <= result.hidden_from_base_m);
    }
    let marker = marker_at(geo(0.0, 0.05, 0.0));
    assert!(matches!(
        marker_occlusion(&scene, eye, &marker, &AtomicBool::new(true)),
        Err(SurveyError::Cancelled)
    ));
    assert!(
        marker_occlusion(
            &scene,
            Vec3d::new(WGS84_A_M - 1.0, 0.0, 0.0),
            &marker,
            &AtomicBool::new(false)
        )
        .is_err()
    );
}

#[test]
fn sampled_terrain_reports_nonmonotone_hidden_band_with_independent_plate_edges() {
    let west = 0.0048;
    let east = 0.0052;
    let lat = 0.0002;
    let ground_height = 60.0;
    let patch = TerrainPatch::from_grid(
        2,
        2,
        west,
        lat,
        east - west,
        2.0 * lat,
        vec![ground_height; 4],
    )
    .unwrap();
    // Four equal-height posts symmetric about the equator are coplanar.
    // Their east/west edges meet z=0 at the averages below. Rays from eye to
    // marker centerline intersect this finite plate only between those edges.
    let edges = [west, east].map(|lon| {
        (geodetic_to_ecef(geo(lat, lon, ground_height))
            + geodetic_to_ecef(geo(-lat, lon, ground_height)))
            * 0.5
    });
    let scene = TerrainScene::new(vec![patch]);
    let eye = Vec3d::new(WGS84_A_M + 100.0, 0.0, 0.0);
    let marker = marker_at(geo(0.0, 0.01, 0.0));
    let cross2 = |a: Vec3d, b: Vec3d| a.x * b.y - a.y * b.x;
    let mut limits = edges
        .map(|edge| -cross2(edge - eye, marker.base_ecef_m - eye) / cross2(edge - eye, marker.up));
    limits.sort_by(f64::total_cmp);
    assert!(limits[0] > 1.0 && limits[1] < 29.0);
    let result = marker_occlusion(&scene, eye, &marker, &AtomicBool::new(false)).unwrap();
    assert_eq!(result.method, OcclusionMethod::SampledTerrain);
    assert_eq!(result.hidden_from_base_m, 0.0);
    assert_eq!(result.hidden_intervals_m.len(), 1);
    close(result.hidden_intervals_m[0][0], limits[0], 1.1e-5);
    close(result.hidden_intervals_m[0][1], limits[1], 1.1e-5);
    close(result.sampled_total_hidden_m, limits[1] - limits[0], 2.2e-5);
    close(result.sampling_step_m, 30.48 / 1024.0, 0.0);
}

#[test]
fn render_prisms_retain_corners_outward_faces_and_physical_band_coordinates() {
    let marker = marker_at(geo(29.305549, -94.767015, 12.0));
    let corners = marker.corners();
    for observer in [
        Observer::new(geo(29.305549, -94.767015, 1.0)),
        Observer::new(geo(29.31, -94.7, 30.0)),
    ] {
        let local = LocalFrame::at(observer);
        let rendered = display::marker_mesh(&marker, local);
        assert_eq!(rendered.mesh.positions.len(), 24);
        assert_eq!(rendered.mesh.indices.len(), 36);
        let center = vector(local.position(marker.centerline(15.24)));
        for (index, point) in rendered.mesh.positions.iter().enumerate() {
            assert!(
                corners
                    .iter()
                    .any(|corner| local.position(*corner) == *point)
            );
            let uv = rendered.band_coordinates[index][0];
            if index < 4 {
                assert_eq!(uv, 0.5);
            } else if index < 8 {
                assert_eq!(uv, 99.5);
            } else {
                assert!(uv == 0.0 || uv == 100.0);
            }
        }
        for face in rendered.mesh.indices.chunks_exact(3) {
            let [a, b, c] =
                [face[0], face[1], face[2]].map(|i| vector(rendered.mesh.positions[i as usize]));
            let normal = (b - a).cross(c - a);
            assert!(normal.dot((a + b + c) / 3.0 - center) > 0.0);
            assert!(normal.dot(vector(rendered.mesh.normals[face[0] as usize])) > 0.0);
        }
    }
}

fn interval_red_fraction(t: f64, width: f64) -> f64 {
    let lo = t - width * 0.5;
    let hi = t + width * 0.5;
    let mut red = 0.0;
    for period in ((lo / 2.0).floor() as i32 - 1)..=((hi / 2.0).ceil() as i32 + 1) {
        let begin = 2.0 * f64::from(period);
        red += (hi.min(begin + 1.0) - lo.max(begin)).max(0.0);
    }
    red / width
}
fn shader_band(t: f32, width: f32) -> f32 {
    let width = width.max(0.0001);
    if width >= 1024.0 {
        return 0.5;
    }
    let phase = t - 2.0 * (t / 2.0).floor();
    let lower = phase - width * 0.5;
    let upper = phase + width * 0.5;
    if lower >= phase.floor() && upper <= phase.floor() + 1.0 {
        return if phase < 1.0 { 1.0 } else { 0.0 };
    }
    let primitive = |v: f32| {
        let p = (v / 2.0).floor();
        p + (v - 2.0 * p).min(1.0)
    };
    ((primitive(upper) - primitive(lower)) / width).clamp(0.0, 1.0)
}

#[test]
fn band_filter_matches_interval_overlap_and_is_bounded_at_transitions() {
    let mut maximum_shader_error = 0.0_f64;
    for width in [
        0.0001, 0.003, 0.1, 0.8, 1.0, 2.0, 5.7, 63.1, 1023.0, 1024.0, 2048.0,
    ] {
        for t in [
            -4.0, -1.3, 0.0, 0.5, 0.99998, 1.0, 1.00002, 1.5, 2.0, 98.5, 99.0, 99.5, 100.0,
        ] {
            let expected = interval_red_fraction(t, width);
            let actual = display::red_band_fraction(t, width).unwrap();
            close(
                actual,
                expected,
                if width >= 1024.0 { 1.0 / width } else { 2e-10 },
            );
            let tf = t as f32;
            let wf = width as f32;
            let float_expected = interval_red_fraction(f64::from(tf), f64::from(wf.max(0.0001)));
            let gpu = f64::from(shader_band(tf, wf));
            assert!((0.0..=1.0).contains(&gpu));
            if width < 1024.0 {
                maximum_shader_error = maximum_shader_error.max((gpu - float_expected).abs());
            }
        }
    }
    assert!(
        maximum_shader_error < 0.0015,
        "f32 transition error {maximum_shader_error}"
    );
    for t in [0.5, 98.5] {
        assert_eq!(shader_band(t, 0.0001), 1.0);
    }
    for t in [1.5, 99.5] {
        assert_eq!(shader_band(t, 0.0001), 0.0);
    }
    close(display::red_band_fraction(1e300, 0.1).unwrap(), 0.5, 1e-15);
    assert!(display::red_band_fraction(f64::NAN, 1.0).is_none());
    assert!(display::red_band_fraction(0.0, -1.0).is_none());
    assert!(display::marker_precedes_grass(0.0, None));
    assert!(display::marker_precedes_grass(3.0, Some(3.0)));
    assert!(!display::marker_precedes_grass(3.0, Some(2.0)));
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(!display::marker_precedes_grass(bad, None));
        assert!(!display::marker_precedes_grass(1.0, Some(bad)));
    }
}

#[test]
fn tape_annotations_preserve_endpoints_with_explicit_finite_radii() {
    let local = LocalFrame::at(Observer::new(geo(29.3, -94.7, 100.0)));
    let a = TapePoint {
        position_ecef_m: local.origin + local.basis.east * 3.0,
        target: SurveyTarget::Surface,
    };
    let b = TapePoint {
        position_ecef_m: a.position_ecef_m + local.basis.north * 10.0 + local.basis.up * 2.0,
        target: SurveyTarget::Marker(1),
    };
    let distance = TapeMeasurement::between(a, b).unwrap();
    let rendered = display::tape_mesh(Some(&a), Some(&b), local);
    assert_eq!(rendered.positions.len(), 24 + 48);
    assert_eq!(rendered.indices.len(), (24 + 16) * 3);
    let axis = (b.position_ecef_m - a.position_ecef_m)
        .normalized()
        .unwrap();
    let axis_local = vector(local.direction(axis));
    for (index, point) in rendered.positions[..24].iter().enumerate() {
        let center = vector(local.position(if index % 2 == 0 {
            a.position_ecef_m
        } else {
            b.position_ecef_m
        }));
        let displacement = vector(*point) - center;
        close(displacement.length(), 0.015, 9e-7);
        close(displacement.dot(axis_local), 0.0, 9e-7);
    }
    for (index, point) in rendered.positions[24..].iter().enumerate() {
        let center = vector(local.position(if index < 24 {
            a.position_ecef_m
        } else {
            b.position_ecef_m
        }));
        close((vector(*point) - center).length(), 0.15, 9e-7);
    }
    assert_eq!(TapeMeasurement::between(a, b).unwrap(), distance);
    assert!(display::tape_mesh(None, None, local).positions.is_empty());
    assert_eq!(
        display::tape_mesh(Some(&a), Some(&a), local)
            .positions
            .len(),
        48
    );
}
