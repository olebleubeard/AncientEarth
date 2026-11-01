//! Independent horizon-lock policy and local-normal geometry checks.
use earth_math::{
    navigation::{HORIZON_LOCK_MAX_HEIGHT_M, enforce_horizon_lock, horizon_lock_active},
    *,
};

fn normal(latitude: f64, longitude: f64) -> Vec3d {
    let latitude = latitude.to_radians();
    let longitude = longitude.to_radians();
    Vec3d::new(
        latitude.cos() * longitude.cos(),
        latitude.cos() * longitude.sin(),
        latitude.sin(),
    )
}

#[test]
fn policy_uses_inclusive_ellipsoidal_eye_height_and_rejects_forged_poses() {
    assert_eq!(HORIZON_LOCK_MAX_HEIGHT_M, 100_000.0);
    let mut demo = earth_math::navigation::horizon_lock_demo_observer();
    assert!(horizon_lock_active(demo, true));
    let demo_ray = demo.frame().center_ray();
    assert_eq!(demo.feet.height_m, 50_000.0);
    assert_eq!(demo.roll_deg, 30.0);
    assert!(enforce_horizon_lock(&mut demo, true));
    assert_eq!(demo.frame().forward, demo_ray.direction);
    assert_eq!(demo.frame().origin, demo_ray.origin);
    let mut observer = Observer::new(Geodetic::new(29.0, -95.0, 99_999.0).unwrap());
    observer.set_eye_height(1.0);
    observer.roll_deg = 37.0;
    assert_eq!(observer.eye_geodetic().height_m, 100_000.0);
    assert!(horizon_lock_active(observer, true));
    let original = observer;
    assert!(!enforce_horizon_lock(&mut observer, false));
    assert_eq!(observer, original);
    observer.feet.height_m = 99_999.0_f64.next_up();
    assert!(observer.eye_geodetic().height_m > 100_000.0);
    assert!(!horizon_lock_active(observer, true));
    let above = observer;
    assert!(!enforce_horizon_lock(&mut observer, true));
    assert_eq!(observer, above);
    observer.feet.height_m = 99_999.0_f64.next_down();
    assert!(horizon_lock_active(observer, true));
    observer.feet.height_m = -300.0;
    assert!(horizon_lock_active(observer, true));
    assert!(enforce_horizon_lock(&mut observer, true));
    assert_eq!(observer.roll_deg, 0.0);
    assert!(!enforce_horizon_lock(&mut observer, true));

    for field in 0..9 {
        let mut invalid = original;
        match field {
            0 => invalid.feet.lat_deg = 91.0,
            1 => invalid.feet.lon_deg = f64::NAN,
            2 => invalid.feet.height_m = MIN_HEIGHT_M - 1.0,
            3 => invalid.eye_height_m = 0.0,
            4 => invalid.eye_height_m = STATURE_M.next_up(),
            5 => invalid.heading_deg = f64::INFINITY,
            6 => invalid.pitch_deg = 90.0_f64.next_up(),
            7 => invalid.roll_deg = f64::NAN,
            _ => invalid.eye_height_m = f64::INFINITY,
        }
        let before = format!("{invalid:?}");
        assert!(!horizon_lock_active(invalid, true), "field {field}");
        assert!(!enforce_horizon_lock(&mut invalid, true), "field {field}");
        assert_eq!(format!("{invalid:?}"), before);
    }
}

#[test]
fn leveling_preserves_forward_and_uses_geodetic_normal_including_poles_and_vertical() {
    for latitude in [-90.0, -89.999_999, -45.0, 0.0, 29.0, 89.999_999, 90.0] {
        for longitude in [-180.0, -94.75, 0.0, 123.0, 180.0] {
            for pitch in [-90.0, -89.999, -31.0, 0.0, 67.0, 89.999, 90.0] {
                for roll in [-180.0, -73.0, 0.0, 47.0, 180.0] {
                    let mut observer =
                        Observer::new(Geodetic::new(latitude, longitude, 20_000.0).unwrap());
                    observer.heading_deg = 127.0;
                    observer.pitch_deg = pitch;
                    observer.roll_deg = roll;
                    let before = observer;
                    let ray = before.frame().center_ray();
                    assert_eq!(enforce_horizon_lock(&mut observer, true), roll != 0.0);
                    let mut expected = before;
                    expected.roll_deg = 0.0;
                    assert_eq!(observer, expected);
                    let frame = observer.frame();
                    assert_eq!(frame.origin, ray.origin);
                    assert_eq!(frame.forward, ray.direction);
                    let up = normal(latitude, longitude);
                    assert!(frame.right.dot(up).abs() < 5e-16);
                    assert!(frame.up.dot(up) >= -5e-16);
                    assert!((frame.forward.cross(frame.up).dot(frame.right) - 1.0).abs() < 1e-15);
                    if pitch.abs() < 89.0 {
                        let projected = (up - frame.forward * up.dot(frame.forward))
                            .normalized()
                            .unwrap();
                        assert!(frame.up.distance(projected) < 2e-15);
                    } else if pitch.abs() == 90.0 {
                        // The vertical singularity uses the stored heading, not an arbitrary pole normal.
                        let h = before.heading_deg.to_radians();
                        let lon = longitude.to_radians();
                        let lat = latitude.to_radians();
                        let east = Vec3d::new(-lon.sin(), lon.cos(), 0.0);
                        let north =
                            Vec3d::new(-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos());
                        assert!(frame.right.distance(east * h.cos() - north * h.sin()) < 5e-16);
                    }
                }
            }
        }
    }
    // Free look can cross zenith. Leveling changes bank only after the full look update.
    let mut observer = Observer::new(Geodetic::new(43.0, 19.0, 100.0).unwrap());
    observer.pitch_deg = 89.0;
    observer.heading_deg = 73.0;
    let before = observer.frame();
    observer.apply_free_look(0.0, -20.0, Lens::default());
    let after_look = observer;
    let angle = 2.4_f64.to_radians();
    let oracle = before.forward * angle.cos() + before.up * angle.sin();
    assert!(after_look.frame().forward.distance(oracle) < 2e-15);
    assert!(enforce_horizon_lock(&mut observer, true));
    assert_eq!(observer.frame().forward, after_look.frame().forward);
    assert_eq!(observer.pitch_deg, after_look.pitch_deg);
    assert_eq!(observer.heading_deg, after_look.heading_deg);
}
