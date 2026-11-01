//! Application sequencing around the independently reviewed pointing geometry.
use ancient_earth::{navigation::Explorer, simulation::AstronomicalScene};
use earth_math::{
    astro::aim::ViewBody,
    navigation::{FlightController, FlightSettings},
    *,
};

fn fixture(follow: bool) -> (Explorer, AstronomicalScene, TerrainScene) {
    let mut explorer = Explorer {
        observer: Observer::new(Geodetic::new(31.0, -94.0, 1_000_000.0).unwrap()),
        follow_earth: follow,
        ..Explorer::default()
    };
    explorer.observer.roll_deg = 47.0;
    let mut astronomy = AstronomicalScene::bundled(explorer.observer).unwrap();
    astronomy.calendar_input = "2026-10-21 18:00:00".into();
    astronomy.apply_calendar();
    astronomy.reset_tracking(explorer.observer, follow).unwrap();
    astronomy.sample(explorer.observer, follow);
    (
        explorer,
        astronomy,
        TerrainScene::boundary_only(vec![]).unwrap(),
    )
}

fn centered(explorer: &Explorer, astronomy: &AstronomicalScene, body: ViewBody) {
    let frame = astronomy.frame.as_ref().unwrap();
    assert_eq!(frame.sample_instant, astronomy.clock.instant());
    assert_eq!(
        frame.sample_observer_ecef_m,
        explorer.observer.eye_position()
    );
    assert_eq!(frame.sample_follow_earth, explorer.follow_earth);
    let target = match body {
        ViewBody::Earth => Vec3d::ZERO,
        ViewBody::Moon => frame.moon.apparent_center_ecef_m,
        ViewBody::Sun => frame.sun.apparent_center_ecef_m,
    };
    assert!(
        explorer.observer.frame().forward.distance(
            (target - explorer.observer.eye_position())
                .normalized()
                .unwrap()
        ) < 3e-14
    );
    assert!(astronomy.error.is_none(), "{:?}", astronomy.error);
}

#[test]
fn current_post_flight_posture_and_clock_sample_drives_tracking_without_teleport_or_zoom() {
    let (mut explorer, mut astronomy, surface) = fixture(true);
    let lens = explorer.lens();
    explorer
        .toggle_body_tracking(
            ViewBody::Moon,
            astronomy.frame.as_ref(),
            astronomy.clock.instant(),
        )
        .unwrap();
    for direction in [1.0, -1.0] {
        explorer.observer.speed_mps = 100.0;
        let report = FlightController::default().step_body(
            &surface,
            &mut explorer.observer,
            astronomy.frame.as_ref(),
            FlightSettings {
                adaptive_speed: false,
            },
            [0.0, direction, 0.0],
            0.2,
            false,
            false,
        );
        assert!(!report.blocked);
        explorer.update_crouch(direction > 0.0);
        let edited = explorer.observer;
        astronomy.step_seconds = 3600.0;
        astronomy.step_count = if direction > 0.0 { 1 } else { -1 };
        astronomy.step();
        astronomy.advance(0.0, &mut explorer, &surface);
        assert_eq!(explorer.observer.feet, edited.feet);
        assert_eq!(explorer.observer.eye_position(), edited.eye_position());
        assert_eq!(explorer.observer.eye_height_m, edited.eye_height_m);
        assert_eq!(explorer.lens(), lens);
        centered(&explorer, &astronomy, ViewBody::Moon);
        assert_eq!(explorer.body_tracking.target(), Some(ViewBody::Moon));
    }
}

#[test]
fn tracking_reanchors_f9_orientation_and_release_or_ground_does_not_restore_old_aim() {
    let (mut explorer, mut astronomy, surface) = fixture(false);
    let held_eye = astronomy
        .frame
        .unwrap()
        .icrf_to_ecef
        .transpose()
        .transform(explorer.observer.eye_position());
    explorer
        .toggle_body_tracking(
            ViewBody::Sun,
            astronomy.frame.as_ref(),
            astronomy.clock.instant(),
        )
        .unwrap();
    astronomy.step_seconds = 3600.0;
    astronomy.step_count = 1;
    astronomy.step();
    astronomy.advance(0.0, &mut explorer, &surface);
    centered(&explorer, &astronomy, ViewBody::Sun);
    assert!(!explorer.follow_earth);
    let inverse = astronomy.frame.unwrap().icrf_to_ecef.transpose();
    assert!(
        inverse
            .transform(explorer.observer.eye_position())
            .distance(held_eye)
            < 5e-8
    );
    let tracked = explorer.observer.frame();
    astronomy.advance(0.0, &mut explorer, &surface);
    assert!(explorer.observer.frame().forward.distance(tracked.forward) < 3e-14);
    assert!(explorer.observer.frame().up.distance(tracked.up) < 3e-14);

    let held_forward = inverse.transform(explorer.observer.frame().forward);
    let held_up = inverse.transform(explorer.observer.frame().up);
    explorer
        .toggle_body_tracking(ViewBody::Sun, None, astronomy.clock.instant())
        .unwrap();
    astronomy.step_seconds = 60.0;
    astronomy.step_count = -1;
    astronomy.step();
    astronomy.advance(0.0, &mut explorer, &surface);
    let inverse = astronomy.frame.unwrap().icrf_to_ecef.transpose();
    assert!(
        inverse
            .transform(explorer.observer.frame().forward)
            .distance(held_forward)
            < 5e-14
    );
    assert!(
        inverse
            .transform(explorer.observer.frame().up)
            .distance(held_up)
            < 5e-14
    );
    assert_eq!(explorer.body_tracking.target(), None);

    explorer
        .toggle_body_tracking(ViewBody::Earth, None, astronomy.clock.instant())
        .unwrap();
    explorer.ground_on_surface(&surface);
    assert_eq!(explorer.body_tracking.target(), None);
    assert_eq!(explorer.observer.pitch_deg, 0.0);
    assert_eq!(explorer.observer.roll_deg, 0.0);
    explorer
        .toggle_body_tracking(ViewBody::Earth, None, astronomy.clock.instant())
        .unwrap();
    explorer.latitude = 12.0;
    explorer.longitude = 13.0;
    explorer.altitude_m = 100.0;
    explorer.visit_coordinates_on_surface(&surface);
    assert_eq!(explorer.body_tracking.target(), None);
    assert_eq!(
        explorer.observer.feet,
        Geodetic::new(12.0, 13.0, 100.0).unwrap()
    );
}
