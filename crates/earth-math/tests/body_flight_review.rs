//! Independent control geometry: explicit quarter turns, Cartesian endpoints,
//! planar collision surfaces and inertial-frame rotations.
use earth_math::{astro::*, navigation::*, *};

fn close(a: Vec3d, b: Vec3d, tolerance: f64) {
    assert!((a - b).length() <= tolerance, "{a:?} != {b:?}");
}

fn axes_close(a: CameraFrame, b: CameraFrame) {
    close(a.forward, b.forward, 8e-14);
    close(a.right, b.right, 8e-14);
    close(a.up, b.up, 8e-14);
}

fn fixed() -> FlightSettings {
    FlightSettings {
        adaptive_speed: false,
    }
}

#[test]
fn body_roll_has_signed_quarter_turns_preserves_pose_settings_and_rejects_invalid_input() {
    for latitude in [-90.0, -31.0, 0.0, 89.99999, 90.0] {
        for pitch in [-90.0, -27.0, 0.0, 63.0, 90.0] {
            let mut observer = Observer::new(Geodetic::new(latitude, 137.0, 1000.0).unwrap());
            observer.heading_deg = 73.0;
            observer.pitch_deg = pitch;
            observer.roll_deg = 41.0;
            observer.set_eye_height(1.13);
            observer.speed_mps = 21.0;
            let original = observer;
            let first = observer.frame();
            for _ in 0..4 {
                observer.apply_body_roll(1.0, 0.25);
            }
            close(observer.frame().forward, first.forward, 8e-14);
            close(observer.frame().right, -first.up, 8e-14);
            close(observer.frame().up, first.right, 8e-14);
            assert_eq!(observer.feet, original.feet);
            assert_eq!(observer.eye_height_m, original.eye_height_m);
            assert_eq!(observer.speed_mps, original.speed_mps);
            for _ in 0..4 {
                observer.apply_body_roll(-1.0, 0.25);
            }
            axes_close(observer.frame(), first);
            let mut clamped = original;
            clamped.apply_body_roll(2.0, 9.0);
            let mut bounded = original;
            bounded.apply_body_roll(1.0, 0.25);
            axes_close(clamped.frame(), bounded.frame());
            for (input, seconds) in [
                (f64::NAN, 0.1),
                (f64::INFINITY, 0.1),
                (1.0, f64::NAN),
                (1.0, f64::INFINITY),
                (1.0, -1.0),
                (1.0, 0.0),
                (0.0, 0.1),
            ] {
                let mut invalid = original;
                invalid.apply_body_roll(input, seconds);
                assert_eq!(invalid, original);
            }
        }
    }
}

#[test]
fn body_translation_uses_pitched_banked_triad_and_normalizes_all_diagonals() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    for (latitude, longitude) in [(0.0, 0.0), (37.0, -106.0), (89.999, 170.0), (-90.0, 73.0)] {
        for (pitch, roll) in [(45.0, 67.0), (-73.0, -151.0), (90.0, 90.0)] {
            let mut initial = Observer::new(Geodetic::new(latitude, longitude, 200_000.0).unwrap());
            initial.heading_deg = 131.0;
            initial.pitch_deg = pitch;
            initial.roll_deg = roll;
            initial.speed_mps = 10.0;
            let frame = initial.frame();
            let origin = geodetic_to_ecef(initial.feet);
            for input in [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
                [1.0, -1.0, 1.0],
            ] {
                let mut observer = initial;
                // This is an independent endpoint identity at constant speed.
                let expected = origin
                    + (frame.forward * input[0] + frame.right * input[1] + frame.up * input[2])
                        .normalized()
                        .unwrap()
                        * 2.0;
                let report = FlightController::default().step_body(
                    &scene,
                    &mut observer,
                    None,
                    fixed(),
                    input,
                    0.2,
                    false,
                    false,
                );
                assert!(!report.blocked);
                close(geodetic_to_ecef(observer.feet), expected, 1e-8);
                assert!((report.travelled_m - 2.0).abs() < 1e-8);
                axes_close(observer.frame(), frame);
            }
            for input in [[0.0; 3], [f64::NAN, 0.0, 0.0]] {
                let mut observer = initial;
                let report = FlightController::default().step_body(
                    &scene,
                    &mut observer,
                    None,
                    fixed(),
                    input,
                    0.2,
                    false,
                    false,
                );
                assert_eq!(observer, initial);
                assert_eq!(report.travelled_m, 0.0);
            }
            // Deliberately geodetic QA input is expressed in this body's basis.
            for sign in [-1.0, 1.0] {
                let body = geodetic_vertical_body_axes(initial, sign);
                let reconstructed =
                    frame.forward * body[0] + frame.right * body[1] + frame.up * body[2];
                close(reconstructed, EnuBasis::at(initial.feet).up * sign, 2e-14);
                let mut legacy = initial;
                let mut mapped = initial;
                FlightController::default().step(
                    &scene,
                    &mut legacy,
                    None,
                    fixed(),
                    [0.0, 0.0, sign],
                    0.2,
                    false,
                    false,
                );
                FlightController::default().step_body(
                    &scene,
                    &mut mapped,
                    None,
                    fixed(),
                    body,
                    0.2,
                    false,
                    false,
                );
                close(
                    geodetic_to_ecef(mapped.feet),
                    geodetic_to_ecef(legacy.feet),
                    1e-8,
                );
                axes_close(mapped.frame(), legacy.frame());
            }
        }
    }
}

fn plane(height: f64) -> TerrainScene {
    let mut heights = Vec::new();
    // The generated source points lie on the explicit Cartesian plane x=a+H.
    for latitude in [0.1_f64, 0.0, -0.1] {
        for longitude in [-0.1_f64, 0.0, 0.1] {
            let lat = latitude.to_radians();
            let lon = longitude.to_radians();
            let prime_vertical = WGS84_A_M / (1.0 - WGS84_E2 * lat.sin().powi(2)).sqrt();
            heights.push((WGS84_A_M + height) / (lat.cos() * lon.cos()) - prime_vertical);
        }
    }
    TerrainScene::new(vec![
        TerrainPatch::from_grid(3, 3, -0.1, 0.1, 0.1, 0.1, heights).unwrap(),
    ])
}

#[test]
fn inverted_body_up_stops_at_canonical_positive_and_negative_ground_then_can_escape() {
    for ground in [-500.0, 0.0, 120.0] {
        let scene = plane(ground);
        let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 1000.0).unwrap());
        observer.feet.height_m = ground + 10.0;
        // Forward is +Y, camera-up is -X: Space points into this terrain plane.
        observer
            .set_view_axes(Vec3d::new(0.0, 1.0, 0.0), Vec3d::new(-1.0, 0.0, 0.0))
            .unwrap();
        observer.speed_mps = 1000.0;
        let frame = observer.frame();
        let mut controller = FlightController::default();
        let report = controller.step_body(
            &scene,
            &mut observer,
            None,
            fixed(),
            [0.0, 0.0, 1.0],
            0.1,
            false,
            false,
        );
        assert!(report.blocked);
        assert!((observer.feet.height_m - ground).abs() < 0.0001);
        assert!(observer.feet.height_m >= ground - 1e-8);
        axes_close(observer.frame(), frame);
        let stopped = observer;
        let repeated = controller.step_body(
            &scene,
            &mut observer,
            None,
            fixed(),
            [0.0, 0.0, 1.0],
            0.1,
            false,
            false,
        );
        assert!(repeated.blocked);
        close(observer.eye_position(), stopped.eye_position(), 0.0001);
        let escaping = controller.step_body(
            &scene,
            &mut observer,
            None,
            fixed(),
            [0.0, 0.0, -1.0],
            0.1,
            false,
            false,
        );
        assert!(!escaping.blocked);
        assert!((observer.feet.height_m - ground - 100.0).abs() < 0.0001);
    }
}

#[test]
fn landing_is_level_upright_preserves_actual_azimuth_and_is_transactional() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    for latitude in [-90.0, -37.0, 0.0, 89.99999, 90.0] {
        for pitch in [-130.0, -90.0, -89.999, -27.0, 90.0, 130.0] {
            let mut initial = Observer::new(Geodetic::new(latitude, 137.0, 1000.0).unwrap());
            initial.heading_deg = 271.0;
            initial.pitch_deg = pitch;
            initial.roll_deg = 113.0;
            initial.set_eye_height(1.13);
            let enu = EnuBasis::at(initial.feet);
            let before = initial.frame();
            let tangent = before.forward - enu.up * before.forward.dot(enu.up);
            let expected = if pitch.abs() == 90.0 {
                // Exactly vertical has no sight azimuth: retain the stored one.
                let angle = initial.heading_deg.to_radians();
                enu.north * angle.cos() + enu.east * angle.sin()
            } else {
                tangent.normalized().unwrap()
            };
            for direct in [false, true] {
                let mut observer = initial;
                if direct {
                    observer.ground();
                } else {
                    scene.place_on_ground(&mut observer, initial.feet).unwrap();
                }
                assert_eq!(observer.pitch_deg, 0.0);
                assert_eq!(observer.roll_deg, 0.0);
                assert_eq!(observer.eye_height_m, initial.eye_height_m);
                assert!(observer.feet.height_m.abs() < 1e-7);
                close(observer.frame().up, enu.up, 2e-14);
                close(observer.frame().forward, expected, 2e-11);
            }
            let mut failed = initial;
            assert!(
                scene
                    .place_on_ground(
                        &mut failed,
                        Geodetic {
                            lat_deg: 91.0,
                            ..initial.feet
                        }
                    )
                    .is_err()
            );
            assert_eq!(failed, initial);
        }
    }
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 500.0).unwrap());
    observer.pitch_deg = 87.0;
    observer.roll_deg = -74.0;
    let location = observer.feet;
    plane(-500.0)
        .place_on_ground(&mut observer, location)
        .unwrap();
    assert!((observer.feet.height_m + 500.0).abs() < 1e-7);
    assert_eq!(observer.pitch_deg, 0.0);
    assert_eq!(observer.roll_deg, 0.0);
}

#[test]
fn unlocked_tracking_reanchors_body_edits_and_landing_then_transports_the_complete_pose() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    let instant = SimulationClock::from_calendar("2024-04-08 18:40:00")
        .unwrap()
        .instant();
    let mut observer = Observer::new(Geodetic::new(23.0, 47.0, 200_000.0).unwrap());
    observer.heading_deg = 31.0;
    observer.pitch_deg = 17.0;
    observer.speed_mps = 10.0;
    let mut tracking = ObserverTracking::new(observer, instant, Mat3d::identity()).unwrap();
    tracking
        .update(&mut observer, false, instant, Mat3d::identity(), &scene)
        .unwrap();
    observer.apply_body_roll(1.0, 0.2);
    FlightController::default().step_body(
        &scene,
        &mut observer,
        None,
        fixed(),
        [0.0, 0.0, 1.0],
        0.2,
        false,
        false,
    );
    let edited = observer;
    tracking
        .update(&mut observer, false, instant, Mat3d::identity(), &scene)
        .unwrap();
    assert_eq!(observer, edited);
    let quarter = Mat3d {
        rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let half = Mat3d {
        rows: [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let turn = |v: Vec3d| Vec3d::new(-v.y, v.x, v.z);
    let later = instant.shifted(3600.0).unwrap();
    tracking
        .update(&mut observer, false, later, quarter, &scene)
        .unwrap();
    close(observer.eye_position(), turn(edited.eye_position()), 2e-8);
    close(
        observer.frame().forward,
        turn(edited.frame().forward),
        3e-14,
    );
    close(observer.frame().up, turn(edited.frame().up), 3e-14);
    let location = observer.feet;
    scene.place_on_ground(&mut observer, location).unwrap();
    let landed = observer;
    tracking
        .update(&mut observer, false, later, quarter, &scene)
        .unwrap();
    assert_eq!(observer, landed);
    tracking
        .update(
            &mut observer,
            false,
            later.shifted(0.5).unwrap(),
            half,
            &scene,
        )
        .unwrap();
    close(observer.eye_position(), turn(landed.eye_position()), 2e-8);
    close(
        observer.frame().forward,
        turn(landed.frame().forward),
        3e-14,
    );
    close(observer.frame().up, turn(landed.frame().up), 3e-14);
    assert!(!tracking.follows_earth());
}
