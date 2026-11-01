//! Independent distant-coordinate and uninterrupted-flight checks. The
//! high-precision fixture is evaluated separately, not a forward/inverse loop.
use earth_math::{astro::*, navigation::*, *};

#[path = "fixtures/space_coordinates.rs"]
mod reference;

fn close_vector(actual: Vec3d, expected: Vec3d, tolerance: f64) {
    assert!(
        actual.is_finite() && actual.distance(expected) <= tolerance,
        "{actual:?} != {expected:?}; error {} > {tolerance}",
        actual.distance(expected)
    );
}

fn axes_match(actual: CameraFrame, expected: CameraFrame, tolerance: f64) {
    close_vector(actual.forward, expected.forward, tolerance);
    close_vector(actual.right, expected.right, tolerance);
    close_vector(actual.up, expected.up, tolerance);
}

#[test]
fn high_precision_coordinates_cover_four_au_without_weakening_exact_input_limits() {
    assert_eq!(MAX_HEIGHT_M, 4.0 * 149_597_870_700.0);
    let mut maximum_forward: f64 = 0.0;
    let mut maximum_inverse_height: f64 = 0.0;
    for &(latitude, longitude, height, xyz) in reference::CASES {
        let geographic = Geodetic::new(latitude, longitude, height).unwrap();
        let expected = Vec3d::new(xyz[0], xyz[1], xyz[2]);
        let actual = geodetic_to_ecef(geographic);
        let tolerance = 3e-9 + 8.0 * f64::EPSILON * expected.length();
        close_vector(actual, expected, tolerance);
        maximum_forward = maximum_forward.max(actual.distance(expected));
        // Invert the independently rounded Cartesian fixture. At exact poles
        // longitude is undetermined; verify the recovered physical point.
        let inverse = ecef_to_geodetic(expected).unwrap();
        assert!((inverse.lat_deg - latitude).abs() <= 3e-13);
        if latitude.abs() != 90.0 {
            let error = (inverse.lon_deg - longitude + 180.0).rem_euclid(360.0) - 180.0;
            assert!(error.abs() <= 1e-12);
        }
        assert!((inverse.height_m - height).abs() <= tolerance);
        maximum_inverse_height = maximum_inverse_height.max((inverse.height_m - height).abs());
        close_vector(geodetic_to_ecef(inverse), expected, 2.0 * tolerance);
        let basis = EnuBasis::at(inverse);
        close_vector(basis.east.cross(basis.north), basis.up, 4e-15);
        assert!(basis.up.dot(basis.north).abs() < 2e-15);
    }
    for height in [MAX_HEIGHT_M.next_up(), f64::INFINITY, f64::NAN] {
        assert!(Geodetic::new(0.0, 0.0, height).is_err());
    }
    for (latitude, longitude) in [(0.0, 0.0), (47.4, -122.3), (-90.0, 137.0)] {
        let mut observer = Observer::new(Geodetic::new(latitude, longitude, MAX_HEIGHT_M).unwrap());
        observer.set_eye_height(STATURE_M);
        assert_eq!(observer.eye_geodetic().height_m, MAX_HEIGHT_M);
        assert!(ecef_to_geodetic(observer.eye_position()).is_ok());
        let before = observer;
        assert!(
            observer
                .try_teleport(Geodetic::new(latitude, longitude, MAX_HEIGHT_M).unwrap())
                .is_err()
        );
        assert_eq!(observer, before);
    }
    // Equatorial rays reduce independently to a circle: impact parameter
    // b=d*sin(theta), near range=d*cos(theta)-sqrt(a²-b²).
    for height in [1e9, ASTRONOMICAL_UNIT_M, MAX_HEIGHT_M] {
        let distance = WGS84_A_M + height;
        let origin = Vec3d::new(distance, 0.0, 0.0);
        let axial = intersect_wgs84(Ray::new(origin, Vec3d::new(-1.0, 0.0, 0.0)).unwrap()).unwrap();
        assert!((axial.distance_m - height).abs() <= 8.0 * f64::EPSILON * distance);
        for factor in [1.0 - 1e-6, 1.0 + 1e-6] {
            let impact = WGS84_A_M * factor;
            let sine = impact / distance;
            let cosine = (1.0 - sine * sine).sqrt();
            let hit = intersect_wgs84(Ray::new(origin, Vec3d::new(-cosine, sine, 0.0)).unwrap());
            if factor < 1.0 {
                let expected = distance * cosine - (WGS84_A_M.powi(2) - impact.powi(2)).sqrt();
                assert!((hit.unwrap().distance_m - expected).abs() < 0.01);
            } else {
                assert!(hit.is_none());
            }
        }
    }
    println!(
        "Distant coordinate fixtures: max forward {maximum_forward} m; max inverse height {maximum_inverse_height} m"
    );
}

#[test]
fn distant_nonrotating_tracking_preserves_the_same_geocentric_position_and_full_axes() {
    let scene = TerrainScene::new(vec![]);
    let instant = SimulationClock::from_calendar("2026-10-21 18:00:00")
        .unwrap()
        .instant();
    // Explicit Cartesian quarter turn, independent of the orientation routines.
    let quarter = Mat3d {
        rows: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
    };
    let rotated = |v: Vec3d| Vec3d::new(v.x, -v.z, v.y);
    for height in [2_000_000.0, 149_597_870_700.0, MAX_HEIGHT_M * 0.9] {
        for (latitude, longitude) in [(0.0, 179.9999), (78.0, -95.0), (-90.0, 37.0)] {
            let mut observer = Observer::new(Geodetic::new(latitude, longitude, height).unwrap());
            observer.heading_deg = 137.0;
            observer.pitch_deg = -17.0;
            observer.roll_deg = 29.0;
            observer.set_eye_height(1.1);
            let initial = observer;
            let frame = observer.frame();
            let tolerance = 2e-8 + 32.0 * f64::EPSILON * frame.origin.length();
            let mut tracker = ObserverTracking::new(observer, instant, Mat3d::identity()).unwrap();
            for _ in 0..8 {
                tracker
                    .update(
                        &mut observer,
                        false,
                        instant.shifted(86400.0).unwrap(),
                        quarter,
                        &scene,
                    )
                    .unwrap();
                close_vector(observer.eye_position(), rotated(frame.origin), tolerance);
                close_vector(observer.frame().forward, rotated(frame.forward), 3e-14);
                close_vector(observer.frame().up, rotated(frame.up), 3e-14);
                tracker
                    .update(&mut observer, false, instant, Mat3d::identity(), &scene)
                    .unwrap();
                close_vector(observer.eye_position(), frame.origin, tolerance);
                axes_match(observer.frame(), frame, 3e-14);
                assert_eq!(observer.eye_height_m, initial.eye_height_m);
            }
            let before = observer;
            tracker
                .update(&mut observer, true, instant, Mat3d::identity(), &scene)
                .unwrap();
            assert_eq!(observer, before);
            tracker
                .update(
                    &mut observer,
                    true,
                    instant.shifted(86400.0).unwrap(),
                    quarter,
                    &scene,
                )
                .unwrap();
            assert_eq!(observer, before);
            assert!(tracker.follows_earth());
        }
    }
}

#[test]
fn adaptive_radial_flight_matches_exponential_oracle_at_multiple_frame_rates_both_ways() {
    let scene = TerrainScene::new(vec![]);
    let duration = 12.0_f64;
    let initial_height = 100.0;
    let base_speed = 50.0;
    let expected_height = (initial_height + 50.0) * (base_speed * duration / 50.0).exp() - 50.0;
    let mut final_heights = Vec::new();
    for hz in [30, 60, 144] {
        let mut observer = Observer::new(Geodetic::new(0.0, 0.0, initial_height).unwrap());
        observer.pitch_deg = 90.0;
        observer.roll_deg = 37.0;
        observer.speed_mps = base_speed;
        let initial = observer.frame();
        let mut controller = FlightController::default();
        let mut outward_total = 0.0;
        for _ in 0..hz * 12 {
            let report = controller.step(
                &scene,
                &mut observer,
                None,
                FlightSettings::default(),
                [1.0, 0.0, 0.0],
                1.0 / f64::from(hz),
                false,
                false,
            );
            assert!(
                !report.blocked,
                "unexpected obstruction at {}m",
                observer.feet.height_m
            );
            assert!(report.travelled_m > 0.0);
            outward_total += report.travelled_m;
            axes_match(observer.frame(), initial, 2e-13);
        }
        assert!((observer.feet.height_m - expected_height).abs() < 0.30);
        assert!((outward_total - (expected_height - initial_height)).abs() < 0.30);
        assert!(observer.feet.height_m > 20_000_000.0);
        final_heights.push(observer.feet.height_m);
        for _ in 0..hz * 12 {
            let report = controller.step(
                &scene,
                &mut observer,
                None,
                FlightSettings::default(),
                [-1.0, 0.0, 0.0],
                1.0 / f64::from(hz),
                false,
                false,
            );
            assert!(!report.blocked);
            assert!(report.travelled_m > 0.0);
            axes_match(observer.frame(), initial, 3e-13);
        }
        // dc/dt changes sign when the held direction reverses. Its exact
        // exponential solution returns to c0 without a view reset or teleport.
        assert!((observer.feet.height_m - initial_height).abs() < 0.001);
        close_vector(observer.frame().origin, initial.origin, 0.001);
    }
    let spread = final_heights
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        - final_heights.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(spread < 0.30);
    println!("30/60/144Hz radial ascent heights: {final_heights:?}; spread {spread}m");
}

#[test]
fn free_look_turns_through_zenith_and_nadir_without_changing_position_or_lens() {
    for (latitude, longitude, height) in [
        (0.0, 0.0, 2.0),
        (89.9999, 180.0, 3_000_000.0),
        (-90.0, 73.0, 149_597_870_700.0),
    ] {
        for lens in [
            Lens::default(),
            Lens::new(Projection::EquidistantFisheye, 140.0),
            Lens::new(Projection::Rectilinear, MIN_FOV_DEG),
        ] {
            let mut observer = Observer::new(Geodetic::new(latitude, longitude, height).unwrap());
            observer.heading_deg = 123.0;
            observer.pitch_deg = 17.0;
            observer.roll_deg = 39.0;
            let initial = observer.frame();
            let feet = observer.feet;
            let quarter_pixels = 90.0 / (0.12 * lens.vertical_fov_deg / DEFAULT_FOV_DEG);
            observer.apply_free_look(quarter_pixels, 0.0, lens);
            close_vector(observer.frame().forward, initial.right, 5e-14);
            close_vector(observer.frame().up, initial.up, 5e-14);
            let yawed = observer.frame();
            observer.apply_free_look(0.0, -quarter_pixels, lens);
            close_vector(observer.frame().forward, yawed.up, 5e-14);
            close_vector(observer.frame().up, -yawed.forward, 5e-14);
            observer.apply_free_look(0.0, -quarter_pixels, lens);
            close_vector(observer.frame().forward, -yawed.forward, 5e-14);
            close_vector(observer.frame().up, -yawed.up, 5e-14);
            observer.apply_free_look(0.0, -2.0 * quarter_pixels, lens);
            axes_match(observer.frame(), yawed, 1e-13);
            observer.apply_free_look(-quarter_pixels, 0.0, lens);
            axes_match(observer.frame(), initial, 1e-13);
            assert_eq!(observer.feet, feet);
            for (x, y) in [(f64::NAN, 0.0), (0.0, f64::INFINITY)] {
                let before = observer;
                observer.apply_free_look(x, y, lens);
                assert_eq!(observer, before);
            }
        }
    }
}

fn plane_patch(height: f64) -> TerrainPatch {
    let mut heights = Vec::new();
    for row in 0..3 {
        for column in 0..3 {
            let latitude = (0.1 - f64::from(row) * 0.1).to_radians();
            let longitude = (-0.1 + f64::from(column) * 0.1).to_radians();
            let n = WGS84_A_M / (1.0 - WGS84_E2 * latitude.sin().powi(2)).sqrt();
            heights.push((WGS84_A_M + height) / (latitude.cos() * longitude.cos()) - n);
        }
    }
    TerrainPatch::from_grid(3, 3, -0.1, 0.1, 0.1, 0.1, heights).unwrap()
}

#[test]
fn swept_fast_descent_stops_on_the_first_canonical_surface_and_exact_ceiling() {
    let settings = FlightSettings {
        adaptive_speed: false,
    };
    let mut controller = FlightController::default();
    for ground_height in [-500.0, 0.0, 120.0] {
        let scene = TerrainScene::new(vec![plane_patch(ground_height)]);
        let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 20_000_000_000.0).unwrap());
        observer.pitch_deg = -90.0;
        observer.speed_mps = MAX_SPEED_MPS;
        let frame = observer.frame();
        let report = controller.step(
            &scene,
            &mut observer,
            None,
            settings,
            [1.0, 0.0, 0.0],
            0.25,
            false,
            false,
        );
        assert!(report.blocked);
        assert!(report.travelled_m > 19_999_000_000.0);
        assert!(observer.feet.height_m >= ground_height - 1e-6);
        assert!((observer.feet.height_m - ground_height).abs() < 0.005);
        axes_match(observer.frame(), frame, 5e-14);
        let stopped = observer;
        let report = controller.step(
            &scene,
            &mut observer,
            None,
            settings,
            [1.0, 0.0, 0.0],
            0.25,
            false,
            false,
        );
        assert!(report.blocked);
        assert!(observer.eye_position().distance(stopped.eye_position()) < 0.005);
    }
    let scene = TerrainScene::new(vec![]);
    let mut observer = Observer::new(Geodetic::new(37.0, -94.0, MAX_HEIGHT_M - 1000.0).unwrap());
    observer.pitch_deg = 90.0;
    observer.speed_mps = MAX_SPEED_MPS;
    let frame = observer.frame();
    let report = controller.step(
        &scene,
        &mut observer,
        None,
        settings,
        [1.0, 0.0, 0.0],
        0.25,
        false,
        false,
    );
    assert!(report.blocked);
    assert!(report.travelled_m > 900.0 && report.travelled_m < 1000.1);
    assert!((observer.eye_geodetic().height_m - MAX_HEIGHT_M).abs() < 0.005);
    assert!((observer.feet.lat_deg - 37.0).abs() < 1e-12);
    assert!((observer.feet.lon_deg + 94.0).abs() < 1e-12);
    axes_match(observer.frame(), frame, 5e-14);
}

#[test]
fn uncovered_finite_terrain_underpass_is_refused_without_inventing_a_vertical_wall() {
    let scene = TerrainScene::new(vec![plane_patch(120.0)]);
    let settings = FlightSettings {
        adaptive_speed: false,
    };
    for (height, expected_blocked) in [(50.0, true), (500.0, false)] {
        let feet = Vec3d::new(WGS84_A_M + height, -20_000.0, 0.0);
        let mut observer = Observer::new(ecef_to_geodetic(feet).unwrap());
        observer
            .set_view_axes(Vec3d::new(0.0, 1.0, 0.0), Vec3d::new(1.0, 0.0, 0.0))
            .unwrap();
        observer.speed_mps = 160_000.0;
        let before = observer;
        // The whole segment is parallel to the raised planar top, with both
        // endpoints beyond the finite patch. No top-triangle ray hit can catch
        // the below-top crossing; the conservative coverage guard must refuse it.
        assert!(
            scene
                .intersect(Ray::new(feet, Vec3d::new(0.0, 1.0, 0.0)).unwrap())
                .is_none()
        );
        let report = FlightController::default().step(
            &scene,
            &mut observer,
            None,
            settings,
            [1.0, 0.0, 0.0],
            0.25,
            false,
            false,
        );
        assert_eq!(report.blocked, expected_blocked);
        if expected_blocked {
            assert_eq!(observer, before);
            assert_eq!(report.travelled_m, 0.0);
        } else {
            assert!((report.travelled_m - 40_000.0).abs() < 1e-6);
            close_vector(
                geodetic_to_ecef(observer.feet),
                feet + Vec3d::new(0.0, 1.0, 0.0) * 40_000.0,
                0.03,
            );
        }
    }
}

#[test]
fn distant_astronomy_uses_separate_local_surface_and_eye_fields_with_finite_domains() {
    let ephemeris =
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap();
    let instant = AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap();
    let geocentric = ephemeris.sample(instant, None, Vec3d::ZERO).unwrap();
    for distance in [
        1e9_f64.next_down(),
        1e9,
        1e9_f64.next_up(),
        ASTRONOMICAL_UNIT_M,
        MAX_HEIGHT_M + WGS84_A_M,
    ] {
        let eye = Vec3d::new(1.0, 0.0, 0.0) * distance;
        for follow in [true, false] {
            let frame = ephemeris
                .sample_with_tracking(instant, None, eye, follow)
                .unwrap();
            assert_eq!(frame.sample_observer_ecef_m, eye);
            assert_eq!(frame.sample_follow_earth, follow);
            let (sun, moon) = frame.observer_eclipse.apparent_vectors(eye).unwrap();
            close_vector(sun, frame.sun.apparent_center_ecef_m - eye, 0.001);
            close_vector(moon, frame.moon.apparent_center_ecef_m - eye, 0.001);
            for axis in [
                Vec3d::new(1.0, 0.0, 0.0),
                -Vec3d::new(1.0, 0.0, 0.0),
                Vec3d::new(0.0, 1.0, 0.0),
                -Vec3d::new(0.0, 1.0, 0.0),
                Vec3d::new(0.0, 0.0, 1.0),
                -Vec3d::new(0.0, 0.0, 1.0),
            ] {
                let ground = axis * if axis.z == 0.0 { WGS84_A_M } else { WGS84_B_M };
                assert!(frame.eclipse.apparent_vectors(ground).is_ok());
            }
            if distance > 1e9 {
                assert_eq!(frame.eclipse.anchor_ecef_m, Vec3d::ZERO);
                // Fresh Earth-center light-time solutions must agree with the
                // independently requested Earth-center sample, not merely move
                // the eye-relative field's anchor across astronomical distances.
                close_vector(
                    frame.eclipse.sun_relative_ecef_m,
                    geocentric.eclipse.sun_relative_ecef_m,
                    0.001,
                );
                close_vector(
                    frame.eclipse.moon_relative_ecef_m,
                    geocentric.eclipse.moon_relative_ecef_m,
                    0.001,
                );
            } else {
                assert_eq!(frame.eclipse.anchor_ecef_m, eye);
            }
            assert!(
                frame
                    .observer_eclipse
                    .apparent_vectors(eye + Vec3d::new(0.0, 1.0, 0.0) * 1.100001e9)
                    .is_err()
            );
        }
    }
    for invalid in [
        Vec3d::new(f64::MAX, f64::MAX, f64::MAX),
        Vec3d::new(1.0, 0.0, 0.0) * (MAX_HEIGHT_M + WGS84_A_M + 1.0),
    ] {
        assert!(ephemeris.sample(instant, None, invalid).is_err());
        assert!(geocentric.eclipse.apparent_vectors(invalid).is_err());
    }
    let invalid_field = SolarEclipseField {
        observer_velocity_ecef_mps: Vec3d::new(1.0, 0.0, 0.0) * LIGHT_SPEED_MPS,
        ..geocentric.eclipse
    };
    assert!(invalid_field.apparent_vectors(Vec3d::ZERO).is_err());
}

#[test]
fn inspection_aberration_uses_a_bounded_surface_reference_without_changing_reception_geometry() {
    let ephemeris =
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap();
    let instant = SimulationClock::from_calendar("2026-10-21 18:00:00")
        .unwrap()
        .instant();
    let tdb = instant.tt_seconds_j2000() + tdb_minus_tt_seconds(instant.tt_seconds_j2000());
    let earth_velocity = ephemeris.state(399, tdb).unwrap().velocity_mps;
    let omega = std::f64::consts::TAU * 1.0027378119113546 / 86400.0;
    // Independent decomposition of a Lorentz boost into components parallel
    // and perpendicular to beta, rather than production's vector numerator.
    let boosted = |direction: Vec3d, velocity: Vec3d| {
        let beta = velocity.length() / LIGHT_SPEED_MPS;
        let axis = velocity.normalized().unwrap();
        let mu = axis.dot(direction);
        axis * ((mu + beta) / (1.0 + beta * mu))
            + (direction - axis * mu) * ((1.0 - beta * beta).sqrt() / (1.0 + beta * mu))
    };
    let mut maximum_ground_angle: f64 = 0.0;
    for (latitude, longitude) in [(0.0, 0.0), (67.0, -94.0), (-90.0, 179.0)] {
        let foot = geodetic_to_ecef(Geodetic::new(latitude, longitude, 0.0).unwrap());
        let reference_spin = Vec3d::new(-omega * foot.y, omega * foot.x, 0.0);
        assert!(reference_spin.length() <= 466.0);
        let mut reference_velocity = None;
        for height in [
            1.72,
            3000.0,
            999_999.999,
            1_000_000.001,
            3e6,
            ASTRONOMICAL_UNIT_M,
            MAX_HEIGHT_M,
        ] {
            let eye = geodetic_to_ecef(Geodetic::new(latitude, longitude, height).unwrap());
            for follow in [true, false] {
                let raw = ephemeris
                    .sample_with_tracking(instant, None, eye, follow)
                    .unwrap();
                let inspection = ephemeris
                    .sample_for_inspection(instant, None, eye, follow)
                    .unwrap();
                let velocity = inspection.icrf_to_ecef.transform(earth_velocity)
                    + if follow { reference_spin } else { Vec3d::ZERO };
                close_vector(
                    inspection.observer_eclipse.observer_velocity_ecef_mps,
                    velocity,
                    2e-10,
                );
                assert_eq!(
                    inspection.observer_eclipse.spin_ecef_radians_per_second,
                    Vec3d::ZERO
                );
                assert_eq!(
                    inspection.velocity_convention,
                    if follow {
                        ApparentVelocityConvention::SurfaceReference
                    } else {
                        ApparentVelocityConvention::EarthTranslation
                    }
                );
                if follow {
                    if let Some(previous) = reference_velocity {
                        close_vector(velocity, previous, 2e-10);
                    }
                    reference_velocity = Some(velocity);
                }
                for (a, b, relative) in [
                    (
                        raw.sun,
                        inspection.sun,
                        raw.observer_eclipse.sun_relative_ecef_m,
                    ),
                    (
                        raw.moon,
                        inspection.moon,
                        raw.observer_eclipse.moon_relative_ecef_m,
                    ),
                ] {
                    assert_eq!(a.center_ecef_m, b.center_ecef_m);
                    assert_eq!(a.light_time_seconds, b.light_time_seconds);
                    assert_eq!(a.body_to_ecef, b.body_to_ecef);
                    assert_eq!(a.radius_m, b.radius_m);
                    let vector = b.apparent_center_ecef_m - eye;
                    close_vector(
                        vector,
                        boosted(relative.normalized().unwrap(), velocity) * relative.length(),
                        0.001 + 8.0 * f64::EPSILON * eye.length(),
                    );
                    if !follow {
                        assert_eq!(a.apparent_center_ecef_m, b.apparent_center_ecef_m);
                    }
                    if height <= 3000.0 && follow {
                        let n = (a.apparent_center_ecef_m - eye).normalized().unwrap();
                        let m = vector.normalized().unwrap();
                        let angle = n.cross(m).length().atan2(n.dot(m));
                        // Small-velocity finite bound includes Earth's beta
                        // through a1.001 factor; no equality is claimed.
                        assert!(angle <= omega * height / LIGHT_SPEED_MPS * 1.001 + 2e-14);
                        if height == 1.72 {
                            maximum_ground_angle = maximum_ground_angle.max(angle);
                        }
                    }
                }
                // The physical surface field keeps its independently defined
                // corotation and must not inherit the eye's bounded reference.
                for ground in [foot, -foot] {
                    let (a, b) = raw.eclipse.apparent_vectors(ground).unwrap();
                    let (c, d) = inspection.eclipse.apparent_vectors(ground).unwrap();
                    assert_eq!(a, c);
                    assert_eq!(b, d);
                }
                let (sun, moon) = inspection.observer_eclipse.apparent_vectors(eye).unwrap();
                close_vector(sun, inspection.sun.apparent_center_ecef_m - eye, 0.001);
                close_vector(moon, inspection.moon.apparent_center_ecef_m - eye, 0.001);
            }
        }
    }
    let orbit = ephemeris.orbit_frame(instant, None).unwrap();
    let trail = ephemeris.earth_orbit_trail(instant).unwrap();
    let lens = Lens::default();
    let fitted = rendering::continuous::fit_orbit_observer(
        Observer::new(Geodetic::new(29.305549, -94.767015, 0.0).unwrap()),
        lens,
        &orbit,
        Some(&trail),
        (1920, 1080),
    )
    .unwrap();
    let raw = ephemeris
        .sample_with_tracking(instant, None, fitted.eye_position(), true)
        .unwrap();
    let inspection = ephemeris
        .sample_for_inspection(instant, None, fitted.eye_position(), true)
        .unwrap();
    let project = |p| {
        lens.project(fitted.frame(), p - fitted.eye_position(), 1920.0 / 1080.0)
            .unwrap()
    };
    let earth = project(Vec3d::ZERO);
    let separation = |p| {
        let q = project(p);
        ((q[0] - earth[0]) * 960.0).hypot((q[1] - earth[1]) * 540.0)
    };
    assert!(separation(raw.moon.apparent_center_ecef_m) > 20.0);
    assert!(separation(inspection.moon.apparent_center_ecef_m) < 2.0);
    assert!(
        (separation(inspection.moon.apparent_center_ecef_m) - separation(raw.moon.center_ecef_m))
            .abs()
            < 0.01
    );
    println!(
        "Inspection reference: smooth ground max difference {} arcsec; whole-orbit Earth/Moon pixel separation raw {} vs inspection {} vs geometric {}",
        maximum_ground_angle.to_degrees() * 3600.0,
        separation(raw.moon.apparent_center_ecef_m),
        separation(inspection.moon.apparent_center_ecef_m),
        separation(raw.moon.center_ecef_m)
    );
}
