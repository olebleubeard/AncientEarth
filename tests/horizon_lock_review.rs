//! Independent application sequencing of the local-roll constraint.
use ancient_earth::{navigation::Explorer, simulation::AstronomicalScene};
use earth_math::{astro::aim::ViewBody, *};

fn fixture(enabled: bool, follow: bool, feet_height: f64) -> (Explorer, AstronomicalScene) {
    let mut state = Explorer {
        observer: Observer::new(Geodetic::new(35.0, -106.0, feet_height).unwrap()),
        horizon_lock_enabled: enabled,
        follow_earth: follow,
        ..Explorer::default()
    };
    state.observer.heading_deg = 113.0;
    state.observer.pitch_deg = 31.0;
    state.observer.roll_deg = 47.0;
    let mut sky = AstronomicalScene::bundled(state.observer).unwrap();
    sky.calendar_input = "2026-10-21 18:00:00".into();
    sky.apply_calendar();
    sky.reset_tracking(state.observer, follow).unwrap();
    sky.sample(state.observer, follow);
    (state, sky)
}

fn assert_centered(state: &Explorer, sky: &AstronomicalScene, body: ViewBody) {
    let frame = sky.frame.unwrap();
    assert_eq!(frame.sample_observer_ecef_m, state.observer.eye_position());
    assert_eq!(frame.sample_instant, sky.clock.instant());
    assert_eq!(frame.sample_follow_earth, state.follow_earth);
    let center = match body {
        ViewBody::Earth => Vec3d::ZERO,
        ViewBody::Moon => frame.moon.apparent_center_ecef_m,
        ViewBody::Sun => frame.sun.apparent_center_ecef_m,
    };
    let direction = (center - state.observer.eye_position())
        .normalized()
        .unwrap();
    assert!(state.observer.frame().forward.distance(direction) < 4e-14);
    assert_eq!(state.observer.roll_deg, 0.0);
    assert!(sky.error.is_none(), "{:?}", sky.error);
}

#[test]
fn default_toggle_posture_and_reentry_preserve_the_current_target_position_and_lens() {
    assert!(Explorer::default().horizon_lock_enabled);
    let (mut state, mut sky) = fixture(true, true, 99_998.5);
    let surface = TerrainScene::boundary_only(vec![]).unwrap();
    assert!(!state.horizon_lock_active());
    let before = state.observer;
    let lens = state.lens();
    state.update_crouch(true);
    assert!(state.horizon_lock_active());
    assert_eq!(state.observer.roll_deg, 0.0);
    assert_eq!(state.observer.feet, before.feet);
    assert_eq!(state.observer.heading_deg, before.heading_deg);
    assert_eq!(state.observer.pitch_deg, before.pitch_deg);
    assert_eq!(state.lens(), lens);
    state.update_crouch(false);
    assert!(!state.horizon_lock_active());
    state.observer.roll_deg = -62.0;
    assert!(!state.apply_horizon_lock());
    state.observer.feet.height_m = 20_000.0;
    sky.sample(state.observer, true);
    state
        .toggle_body_tracking(ViewBody::Moon, sky.frame.as_ref(), sky.clock.instant())
        .unwrap();
    let aimed = state.observer;
    assert_eq!(aimed.roll_deg, 0.0);
    assert_eq!(state.body_tracking.target(), Some(ViewBody::Moon));
    assert_centered(&state, &sky, ViewBody::Moon);
    state.set_horizon_lock(false);
    assert_eq!(state.observer, aimed);
    state.observer.roll_deg = 29.0;
    let before = state.observer;
    state.set_horizon_lock(true);
    assert_eq!(state.observer.frame().forward, before.frame().forward);
    assert_eq!(state.observer.eye_position(), before.eye_position());
    assert_eq!(state.body_tracking.target(), Some(ViewBody::Moon));
    assert_eq!(state.lens(), lens);
    // Each live target update may transport bank; the final displayed pose must be leveled.
    for seconds in [600.0_f64, -300.0, 3600.0] {
        sky.step_count = if seconds < 0.0 { -1 } else { 1 };
        sky.step_seconds = seconds.abs();
        sky.step();
        sky.advance(0.0, &mut state, &surface);
        assert_centered(&state, &sky, ViewBody::Moon);
        assert_eq!(state.observer.feet, before.feet);
        assert_eq!(state.lens(), lens);
    }
}

#[test]
fn f9_preserves_eye_and_forward_while_lock_constrains_bank_and_reanchors_final_axes() {
    let surface = TerrainScene::boundary_only(vec![]).unwrap();
    let (mut locked, mut locked_sky) = fixture(true, false, 50_000.0);
    let (mut free, mut free_sky) = fixture(false, false, 50_000.0);
    let rotation = locked_sky.frame.unwrap().icrf_to_ecef;
    let initial_eye = rotation
        .transpose()
        .transform(locked.observer.eye_position());
    let initial_forward = rotation
        .transpose()
        .transform(locked.observer.frame().forward);
    let initial_up = rotation.transpose().transform(free.observer.frame().up);
    for seconds in [0.0_f64, 60.0, 600.0, -120.0, 1800.0] {
        for (state, sky) in [(&mut locked, &mut locked_sky), (&mut free, &mut free_sky)] {
            sky.step_count = if seconds < 0.0 { -1 } else { 1 };
            sky.step_seconds = seconds.abs();
            if seconds != 0.0 {
                sky.step();
            }
            sky.advance(0.0, state, &surface);
            assert!(!state.follow_earth);
            assert!(sky.error.is_none(), "{:?}", sky.error);
            let inverse = sky.frame.unwrap().icrf_to_ecef.transpose();
            assert!(
                inverse
                    .transform(state.observer.eye_position())
                    .distance(initial_eye)
                    < 1e-7
            );
            assert!(
                inverse
                    .transform(state.observer.frame().forward)
                    .distance(initial_forward)
                    < 8e-14
            );
        }
        assert_eq!(locked.observer.roll_deg, 0.0);
        assert!(
            locked
                .observer
                .eye_position()
                .distance(free.observer.eye_position())
                < 1e-7
        );
        let inverse = free_sky.frame.unwrap().icrf_to_ecef.transpose();
        assert!(
            inverse
                .transform(free.observer.frame().up)
                .distance(initial_up)
                < 8e-14
        );
    }
    // Disabling the constraint keeps the last accepted bank; later F9 updates
    // must transport that frame, rather than restore pre-leveling axes.
    let inverse = locked_sky.frame.unwrap().icrf_to_ecef.transpose();
    let released_up = inverse.transform(locked.observer.frame().up);
    locked.set_horizon_lock(false);
    locked_sky.step_seconds = 600.0;
    locked_sky.step_count = 1;
    locked_sky.step();
    locked_sky.advance(0.0, &mut locked, &surface);
    let inverse = locked_sky.frame.unwrap().icrf_to_ecef.transpose();
    assert!(
        inverse
            .transform(locked.observer.frame().up)
            .distance(released_up)
            < 8e-14
    );
    assert!(!locked.follow_earth);
}
