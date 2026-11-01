//! App transactions for one physical camera with automatic orbital annotations.
use ancient_earth::{navigation::Explorer, orbit::OrbitOverview, simulation::AstronomicalScene};
use earth_math::{Geodetic, Observer, TerrainScene};

#[test]
fn one_camera_tracks_time_on_both_sides_of_automatic_annotation_visibility() {
    let surface = TerrainScene::new(vec![]);
    for follow in [true, false] {
        let mut explorer = Explorer {
            observer: Observer::new(Geodetic::new(29.0, -95.0, 100.0).unwrap()),
            follow_earth: follow,
            // Exercise unrestricted full-triad transport across all distances.
            horizon_lock_enabled: false,
            ..Explorer::default()
        };
        explorer.observer.heading_deg = 117.0;
        explorer.observer.pitch_deg = -21.0;
        explorer.observer.roll_deg = 32.0;
        explorer.observer.set_eye_height(1.1);
        let mut astronomy = AstronomicalScene::bundled(explorer.observer).unwrap();
        astronomy.calendar_input = "2026-10-21 18:00:00".into();
        astronomy.apply_calendar();
        astronomy.reset_tracking(explorer.observer, follow).unwrap();
        astronomy.sample(explorer.observer, follow);
        astronomy.clock.running = true;
        let lens = explorer.lens();
        let mut overview = OrbitOverview::default();
        for (height, rate) in [
            (100.0, 1.0),
            (3e6, 1.0),
            (20e6, 1.0),
            (2.0 * earth_math::ASTRONOMICAL_UNIT_M, -1.0),
            (20e6, -1.0),
            (3e6, -1.0),
            (100.0, -1.0),
        ] {
            explorer.observer.feet = explorer.observer.feet.with_height(height).unwrap();
            let before = explorer.observer;
            let old_rotation = astronomy.frame.unwrap().icrf_to_ecef;
            let held_eye = old_rotation.transpose().transform(before.eye_position());
            let held_up = old_rotation.transpose().transform(before.frame().up);
            let old_epoch = astronomy.clock.instant();
            astronomy.clock.rate = rate;
            astronomy.advance(1.0 / 60.0, &mut explorer, &surface);
            assert_eq!(
                astronomy.clock.instant(),
                old_epoch.shifted(rate / 60.0).unwrap()
            );
            assert!(astronomy.error.is_none(), "{:?}", astronomy.error);
            assert_eq!(explorer.follow_earth, follow);
            if follow {
                assert_eq!(explorer.observer, before);
            } else {
                let inverse = astronomy.frame.unwrap().icrf_to_ecef.transpose();
                assert!(
                    inverse
                        .transform(explorer.observer.eye_position())
                        .distance(held_eye)
                        < 3e-8 + 32.0 * f64::EPSILON * held_eye.length()
                );
                assert!(
                    inverse
                        .transform(explorer.observer.frame().up)
                        .distance(held_up)
                        < 5e-14
                );
            }
            let current = explorer.observer;
            astronomy.refresh_overview(&mut overview, current, lens);
            assert_eq!(explorer.observer, current);
            assert_eq!(explorer.lens(), lens);
            assert_eq!(
                overview.visible,
                earth_math::rendering::continuous::space_annotations_visible(current, lens)
            );
            assert_eq!(overview.frame.is_some(), overview.visible);
            let frame = astronomy.frame.unwrap();
            assert_eq!(frame.sample_instant, astronomy.clock.instant());
            assert_eq!(frame.sample_observer_ecef_m, current.eye_position());
            assert_eq!(frame.sample_follow_earth, follow);
        }
    }
}

#[test]
fn dated_annotation_cache_has_no_camera_and_explicit_fit_is_transactional() {
    let mut explorer = Explorer {
        observer: Observer::new(Geodetic::new(29.0, -95.0, 20e6).unwrap()),
        follow_earth: false,
        ..Explorer::default()
    };
    let mut astronomy = AstronomicalScene::bundled(explorer.observer).unwrap();
    astronomy.calendar_input = "2026-10-21 12:00:00".into();
    astronomy.apply_calendar();
    astronomy.reset_tracking(explorer.observer, false).unwrap();
    astronomy.sample(explorer.observer, false);
    let lens = explorer.lens();
    let initial = explorer.observer;
    let mut overview = OrbitOverview::default();
    astronomy.refresh_overview(&mut overview, initial, lens);
    let first = overview.frame.unwrap();
    let pointer = overview.trail.as_ref().unwrap().points.as_ptr();
    let start = overview.trail.as_ref().unwrap().start;
    let end = overview.trail.as_ref().unwrap().end;
    for delta in [0.000_000_1, 86400.0, -86400.0] {
        let next = first.instant.shifted(delta).unwrap();
        astronomy.clock.set_instant(next);
        astronomy.refresh_overview(&mut overview, initial, lens);
        assert_eq!(overview.frame.unwrap().instant, next);
        assert_eq!(overview.trail.as_ref().unwrap().points.as_ptr(), pointer);
        assert_eq!(overview.trail.as_ref().unwrap().start, start);
        assert_eq!(overview.trail.as_ref().unwrap().end, end);
        assert_eq!(explorer.observer, initial);
    }
    let near = Observer::new(Geodetic::new(29.0, -95.0, 1.0).unwrap());
    astronomy.refresh_overview(&mut overview, near, lens);
    assert!(!overview.visible && overview.frame.is_none());
    assert_eq!(overview.trail.as_ref().unwrap().points.as_ptr(), pointer);
    let jumped = end.shifted(1.0).unwrap();
    astronomy.clock.set_instant(jumped);
    astronomy.refresh_overview(&mut overview, initial, lens);
    assert!(overview.trail.as_ref().unwrap().covers(jumped));
    assert_ne!(overview.trail.as_ref().unwrap().start, start);
    for size in [(0, 1080), (1920, 0)] {
        assert!(
            astronomy
                .fit_orbit(&mut explorer, &mut overview, size)
                .is_err()
        );
        assert_eq!(explorer.observer, initial);
        assert_eq!(astronomy.clock.instant(), jumped);
    }
    explorer.normal_lens =
        earth_math::Lens::new(earth_math::Projection::Rectilinear, earth_math::MIN_FOV_DEG);
    assert!(
        astronomy
            .fit_orbit(&mut explorer, &mut overview, (1920, 1080))
            .is_err()
    );
    assert_eq!(explorer.observer, initial);
    explorer.normal_lens = lens;
    astronomy
        .fit_orbit(&mut explorer, &mut overview, (1920, 1080))
        .unwrap();
    assert_eq!(explorer.lens(), lens);
    assert!(!explorer.follow_earth);
    assert!(
        explorer
            .observer
            .eye_position()
            .distance(initial.eye_position())
            > 1e11
    );
    let fitted = explorer.observer;
    astronomy.advance(0.0, &mut explorer, &TerrainScene::new(vec![]));
    assert_eq!(explorer.observer, fitted);
    assert_eq!(
        astronomy.frame.unwrap().sample_observer_ecef_m,
        fitted.eye_position()
    );
    assert_eq!(
        astronomy.frame.unwrap().sample_instant,
        astronomy.clock.instant()
    );
    assert!(overview.visible && overview.error.is_none());
}
