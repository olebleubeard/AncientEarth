//! Independent full-frame pointing transport and transactional tracking checks.
use earth_math::{
    astro::{aim::*, *},
    *,
};
use std::sync::OnceLock;

fn ephemeris() -> &'static Ephemeris {
    static SOURCE: OnceLock<Ephemeris> = OnceLock::new();
    SOURCE.get_or_init(|| {
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap()
    })
}
fn instant() -> AstroInstant {
    AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap()
}
fn packet(observer: Observer, time: AstroInstant, follow: bool) -> AstronomyFrame {
    ephemeris()
        .sample_for_inspection(time, None, observer.eye_position(), follow)
        .unwrap()
}
fn close(a: Vec3d, b: Vec3d, tolerance: f64) {
    assert!(
        a.distance(b) <= tolerance,
        "{a:?} != {b:?}, tolerance {tolerance}"
    );
}
fn unchanged_position(a: Observer, b: Observer) {
    assert_eq!(a.feet, b.feet);
    assert_eq!(a.eye_height_m, b.eye_height_m);
    assert_eq!(a.speed_mps, b.speed_mps);
    assert_eq!(a.eye_position(), b.eye_position());
}
fn checked_axes(observer: Observer) {
    let f = observer.frame();
    for axis in [f.forward, f.up, f.right] {
        assert!((axis.length() - 1.0).abs() < 2e-14);
    }
    assert!(f.forward.dot(f.up).abs() < 2e-14);
    close(f.forward.cross(f.up), f.right, 2e-14);
}

#[test]
fn aim_transports_the_complete_frame_by_the_shortest_rotation_including_poles_and_antipodes() {
    let mut tracker = BodyTracking::default();
    for (lat, lon, height) in [
        (0.0, 0.0, 0.0),
        (90.0, 17.0, 20.0),
        (-89.999999, -179.9999, 1e6),
        (29.3, -94.7, 3.0 * 149_597_870_700.0),
    ] {
        let mut original = Observer::new(Geodetic::new(lat, lon, height).unwrap());
        original.heading_deg = 173.0;
        original.pitch_deg = 63.0;
        original.roll_deg = -71.0;
        let old = original.frame();
        let mut astronomy = packet(original, instant(), true);
        for angle in [
            0.0_f64,
            0.1,
            1.0,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI - 1e-5,
            std::f64::consts::PI,
        ] {
            let direction = old.forward * angle.cos() + old.right * angle.sin();
            astronomy.moon.apparent_center_ecef_m = original.eye_position() + direction * 1e9;
            // The product of reflections H_(f+t) H_f maps f to t by the
            // shortest proper rotation. Using the actual represented target
            // also captures bank changes from rounding near an antipode.
            let target = (astronomy.moon.apparent_center_ecef_m - original.eye_position())
                .normalized()
                .unwrap();
            let transported = if old.forward.cross(target).length() > 64.0 * f64::EPSILON {
                let first = old.up - old.forward * (2.0 * old.forward.dot(old.up));
                let bisector = (old.forward + target).normalized().unwrap();
                first - bisector * (2.0 * bisector.dot(first))
            } else {
                old.up
            };
            let expected_up = (transported - target * transported.dot(target))
                .normalized()
                .unwrap();
            let mut observer = original;
            tracker
                .aim_once(
                    ViewBody::Moon,
                    &mut observer,
                    Some(&astronomy),
                    instant(),
                    true,
                )
                .unwrap();
            unchanged_position(observer, original);
            close(observer.frame().forward, target, 2e-12);
            close(observer.frame().up, expected_up, 2e-9);
            checked_axes(observer);
            assert_eq!(tracker.target(), None);
        }
        let mut observer = original;
        tracker
            .aim_once(ViewBody::Earth, &mut observer, None, instant(), false)
            .unwrap();
        close(
            observer.frame().forward,
            (-original.eye_position()).normalized().unwrap(),
            2e-14,
        );
        unchanged_position(observer, original);
    }
}

#[test]
fn tracking_selection_and_pose_are_transactional_for_stale_missing_and_invalid_data() {
    let mut observer = Observer::new(Geodetic::new(29.3, -94.7, 100.0).unwrap());
    let time = instant();
    let frame = packet(observer, time, true);
    let mut tracking = BodyTracking::default();
    tracking
        .toggle(ViewBody::Moon, &mut observer, Some(&frame), time, true)
        .unwrap();
    let aimed = observer;
    assert_eq!(tracking.target(), Some(ViewBody::Moon));
    for absent in [None, Some(&frame)] {
        let wrong_time =
            AstroInstant::from_tt_seconds_j2000(time.tt_seconds_j2000() + 0.0000001).unwrap();
        assert!(
            tracking
                .update(&mut observer, absent, wrong_time, true)
                .is_err()
        );
        assert_eq!(observer, aimed);
        assert_eq!(tracking.target(), Some(ViewBody::Moon));
    }
    assert_eq!(
        tracking.update(&mut observer, Some(&frame), time, false),
        Err(BodyAimError::StaleSample)
    );
    let mut bad = frame;
    bad.sample_observer_ecef_m.x += 1.0;
    assert_eq!(
        tracking.update(&mut observer, Some(&bad), time, true),
        Err(BodyAimError::StaleSample)
    );
    for center in [
        observer.eye_position(),
        Vec3d::new(f64::NAN, 0.0, 0.0),
        Vec3d::new(f64::MAX, f64::MAX, 0.0),
    ] {
        bad = frame;
        bad.sun.apparent_center_ecef_m = center;
        assert!(
            tracking
                .toggle(ViewBody::Sun, &mut observer, Some(&bad), time, true)
                .is_err()
        );
        assert_eq!(observer, aimed);
        assert_eq!(tracking.target(), Some(ViewBody::Moon));
    }
    // Repeating an active target releases even when the current sample is absent.
    tracking
        .toggle(ViewBody::Moon, &mut observer, None, time, true)
        .unwrap();
    assert_eq!(observer, aimed);
    assert_eq!(tracking.target(), None);
    // An inside-body view may aim at the center; this is not a collision query.
    bad = frame;
    bad.sun.apparent_center_ecef_m = observer.eye_position() + observer.frame().up * 10.0;
    bad.sun.radius_m = 100.0;
    tracking
        .toggle(ViewBody::Sun, &mut observer, Some(&bad), time, true)
        .unwrap();
    close(
        observer.frame().forward,
        (bad.sun.apparent_center_ecef_m - observer.eye_position())
            .normalized()
            .unwrap(),
        2e-14,
    );
    tracking
        .aim_once(ViewBody::Earth, &mut observer, None, time, true)
        .unwrap();
    assert_eq!(tracking.target(), None);
    for kind in 0..5 {
        let mut invalid = observer;
        match kind {
            0 => invalid.feet.lat_deg = f64::NAN,
            1 => invalid.feet.height_m = f64::INFINITY,
            2 => invalid.eye_height_m = -1.0,
            3 => invalid.roll_deg = f64::NAN,
            _ => invalid.speed_mps = f64::NAN,
        }
        assert_eq!(
            tracking.aim_once(ViewBody::Earth, &mut invalid, None, time, true),
            Err(BodyAimError::InvalidObserver)
        );
    }
}

#[test]
fn successive_time_location_posture_and_follow_samples_keep_selected_body_centered() {
    let mut observer = Observer::new(Geodetic::new(67.0, 179.8, 1e6).unwrap());
    observer.roll_deg = 47.0;
    let lenses = [
        Lens::default(),
        Lens::new(Projection::EquidistantFisheye, 140.0),
        Lens::new(Projection::Rectilinear, 0.2),
    ];
    for body in [ViewBody::Earth, ViewBody::Moon, ViewBody::Sun] {
        let mut tracking = BodyTracking::default();
        let frame = packet(observer, instant(), true);
        tracking
            .toggle(body, &mut observer, Some(&frame), instant(), true)
            .unwrap();
        for index in 0..24 {
            let time = AstroInstant::from_tt_seconds_j2000(
                instant().tt_seconds_j2000() + f64::from(index - 12) * 10000.0,
            )
            .unwrap();
            observer.feet = Geodetic::new(
                67.0 + f64::from(index) * 0.01,
                (179.8 + f64::from(index) * 0.05 + 180.0).rem_euclid(360.0) - 180.0,
                1e6 + f64::from(index) * 100.0,
            )
            .unwrap();
            observer.eye_height_m = if index % 2 == 0 { 1.0 } else { 1.72 };
            let follow = index % 3 != 0;
            let frame = packet(observer, time, follow);
            let before = observer;
            tracking
                .update(&mut observer, Some(&frame), time, follow)
                .unwrap();
            unchanged_position(observer, before);
            checked_axes(observer);
            let center = match body {
                ViewBody::Earth => Vec3d::ZERO,
                ViewBody::Moon => frame.moon.apparent_center_ecef_m,
                ViewBody::Sun => frame.sun.apparent_center_ecef_m,
            };
            let direction = (center - observer.eye_position()).normalized().unwrap();
            close(observer.frame().forward, direction, 2e-14);
            for lens in lenses {
                let projected = lens
                    .project(observer.frame(), direction, 16.0 / 9.0)
                    .unwrap();
                assert!(projected[0].abs() < 1e-9 && projected[1].abs() < 1e-9);
            }
            assert_eq!(tracking.target(), Some(body));
        }
    }
}
