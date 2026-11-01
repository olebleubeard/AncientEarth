//! Independent original-library state/pole references and orbital-view contracts.
use earth_math::Vec3d;
use earth_math::astro::orbit::*;
use earth_math::astro::*;
use std::path::PathBuf;
use std::sync::OnceLock;

#[path = "fixtures/orbital_reference.rs"]
mod reference;

fn ephemeris() -> &'static Ephemeris {
    static SOURCE: OnceLock<Ephemeris> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/astronomy/de440s.bsp");
        Ephemeris::from_bytes(&std::fs::read(path).unwrap()).unwrap()
    })
}
fn v(a: [f64; 3]) -> Vec3d {
    Vec3d::new(a[0], a[1], a[2])
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "actual={actual:.17e}, expected={expected:.17e}, tolerance={tolerance}"
    );
}
fn instant(label: &str) -> AstroInstant {
    AstroInstant::from_calendar(CalendarDateTime::parse(label).unwrap()).unwrap()
}

#[test]
fn heliocentric_states_cip_and_tilt_match_original_cspice_erfa() {
    for &(tt, earth_p, earth_v, moon_p, moon_v, pole, normal, tilt) in reference::CASES {
        let time = AstroInstant::from_tt_seconds_j2000(tt).unwrap();
        let earth = ephemeris().heliocentric_state(399, time).unwrap();
        let moon = ephemeris().heliocentric_state(301, time).unwrap();
        close(earth.position_m.distance(v(earth_p)), 0.0, 0.001);
        close(earth.velocity_mps.distance(v(earth_v)), 0.0, 1e-8);
        close(moon.position_m.distance(v(moon_p)), 0.0, 0.001);
        close(moon.velocity_mps.distance(v(moon_v)), 0.0, 1e-8);
        let frame = ephemeris().orbit_frame(time, None).unwrap();
        assert_eq!(frame.instant, time);
        close(frame.earth_position_m.distance(v(earth_p)), 0.0, 0.001);
        close(frame.moon_position_m.distance(v(moon_p)), 0.0, 0.001);
        close(frame.axis_icrf.distance(v(pole)), 0.0, 2e-12);
        close(frame.orbital_normal_icrf.distance(v(normal)), 0.0, 2e-12);
        close(frame.axial_tilt_deg, tilt.to_degrees(), 2e-10);
        close(
            frame.orbital_radial_icrf.dot(frame.orbital_normal_icrf),
            0.0,
            1e-15,
        );
        close(
            frame
                .orbital_radial_icrf
                .cross(frame.orbital_tangent_icrf)
                .distance(frame.orbital_normal_icrf),
            0.0,
            5e-15,
        );
        assert!(frame.orbital_tangent_icrf.dot(frame.earth_velocity_mps) > 0.0);
        let inverse = terrestrial_rotation(time, None).transpose();
        for (a, b) in frame
            .earth_to_icrf
            .rows
            .iter()
            .flatten()
            .zip(inverse.rows.iter().flatten())
        {
            assert_eq!(a, b);
        }
        // The geometric Sun seen from Earth's center is the negative of the
        // heliocentric Earth, expressed in the same current celestial axes.
        let earth_view = ephemeris().sample(time, None, Vec3d::ZERO).unwrap();
        close(
            frame
                .earth_to_icrf
                .transform(earth_view.sun.center_ecef_m)
                .distance(-frame.earth_position_m),
            0.0,
            0.0002,
        );
        let origin = ephemeris().heliocentric_state(10, time).unwrap();
        assert_eq!(origin.position_m, Vec3d::ZERO);
        assert_eq!(origin.velocity_mps, Vec3d::ZERO);
    }
    assert!(
        ephemeris()
            .heliocentric_state(123456, instant("2026-10-21 18:00:00"))
            .is_err()
    );
}

#[test]
fn plane_and_tilt_are_oriented_scale_invariant_and_fail_on_degenerate_states() {
    for scale in [1e-200, 1.0, 1e200] {
        let state = CartesianState {
            position_m: Vec3d::new(3.0, 4.0, 0.0) * scale,
            velocity_mps: Vec3d::new(-4.0, 3.0, 2.0) * scale,
        };
        // Unscaled exact cross product is (8,-6,25), independently expanded.
        let expected = Vec3d::new(8.0, -6.0, 25.0) / 725.0_f64.sqrt();
        close(
            osculating_normal(state).unwrap().distance(expected),
            0.0,
            3e-16,
        );
        close(
            axis_tilt_degrees(Vec3d::new(0.0, 0.0, scale), Vec3d::new(0.0, scale, 0.0)).unwrap(),
            90.0,
            1e-14,
        );
    }
    for (normal, expected) in [
        (Vec3d::new(0.0, 0.0, 1.0), 0.0),
        (Vec3d::new(0.0, 0.0, -1.0), 180.0),
        (Vec3d::new(3.0_f64.sqrt(), 0.0, -1.0), 120.0),
    ] {
        close(
            axis_tilt_degrees(Vec3d::new(0.0, 0.0, 1.0), normal).unwrap(),
            expected,
            3e-14,
        );
    }
    for (p, velocity) in [
        (Vec3d::ZERO, Vec3d::new(0.0, 1.0, 0.0)),
        (Vec3d::new(1.0, 0.0, 0.0), Vec3d::ZERO),
        (Vec3d::new(1.0, 2.0, 3.0), Vec3d::new(-2.0, -4.0, -6.0)),
        (Vec3d::new(f64::NAN, 1.0, 0.0), Vec3d::new(0.0, 1.0, 0.0)),
        (
            Vec3d::new(1.0, 0.0, 0.0),
            Vec3d::new(0.0, f64::INFINITY, 0.0),
        ),
    ] {
        assert!(
            osculating_normal(CartesianState {
                position_m: p,
                velocity_mps: velocity
            })
            .is_err()
        );
    }
    for bad in [
        Vec3d::ZERO,
        Vec3d::new(f64::NAN, 0.0, 0.0),
        Vec3d::new(f64::INFINITY, 0.0, 0.0),
    ] {
        assert!(axis_tilt_degrees(bad, Vec3d::new(0.0, 0.0, 1.0)).is_err());
        assert!(axis_tilt_degrees(Vec3d::new(0.0, 0.0, 1.0), bad).is_err());
    }
}

#[test]
fn dated_trail_matches_independent_samples_and_is_not_a_closed_circle() {
    let center = AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap();
    let trail = ephemeris().earth_orbit_trail(center).unwrap();
    assert!(!trail.clipped_to_calendar);
    assert_eq!(trail.points.len(), 1462);
    assert_eq!(
        trail.end.tt_seconds_j2000() - trail.start.tt_seconds_j2000(),
        31_557_600.0
    );
    assert_eq!(trail.points[0].instant, trail.start);
    assert_eq!(trail.points.last().unwrap().instant, trail.end);
    for pair in trail.points.windows(2) {
        assert_eq!(
            pair[1].instant.tt_seconds_j2000() - pair[0].instant.tt_seconds_j2000(),
            21_600.0
        );
    }
    for &(tt, point) in reference::TRAIL {
        let sample = trail
            .points
            .iter()
            .find(|p| p.instant.tt_seconds_j2000() == tt)
            .unwrap();
        close(sample.earth_position_m.distance(v(point)), 0.0, 0.001);
    }
    let (minimum, maximum) = trail
        .points
        .iter()
        .map(|p| p.earth_position_m.length())
        .fold((f64::INFINITY, 0.0_f64), |(lo, hi), r| {
            (lo.min(r), hi.max(r))
        });
    assert!(
        maximum - minimum > 4e9,
        "a fitted constant-radius circle is not the ephemeris path"
    );
    assert!(
        trail.points[0]
            .earth_position_m
            .distance(trail.points.last().unwrap().earth_position_m)
            > 1e6
    );
    assert!(trail.covers(center) && trail.covers(trail.start) && trail.covers(trail.end));
    for tt in [
        trail.start.tt_seconds_j2000() - 1.0,
        trail.end.tt_seconds_j2000() + 1.0,
    ] {
        assert!(!trail.covers(AstroInstant::from_tt_seconds_j2000(tt).unwrap()));
    }
}

#[test]
fn trajectory_clips_only_at_actual_supported_calendar_bounds_in_both_time_directions() {
    for center in [
        instant("1850-01-01 00:00:00"),
        instant("1850-02-01 00:00:00"),
        instant("2149-11-30 12:00:00"),
        instant("2149-12-31 23:59:59"),
    ] {
        let trail = ephemeris().earth_orbit_trail(center).unwrap();
        assert!(trail.clipped_to_calendar && trail.covers(center));
        assert!(trail.points.len() >= 731 && trail.points.len() <= 1462);
        assert_eq!(trail.points[0].instant, trail.start);
        assert_eq!(trail.points.last().unwrap().instant, trail.end);
        for point in &trail.points {
            assert!((1850..=2149).contains(&point.instant.calendar().year));
            assert!(point.earth_position_m.is_finite());
        }
        for pair in trail.points.windows(2) {
            let dt = pair[1].instant.tt_seconds_j2000() - pair[0].instant.tt_seconds_j2000();
            assert!(dt > 0.0 && dt <= 21_600.0);
        }
    }
    let start = ephemeris()
        .earth_orbit_trail(instant("1850-01-01 00:00:00"))
        .unwrap()
        .start;
    assert_eq!(start.calendar_string(), "1850-01-01 00:00:00.000");
    let end = ephemeris()
        .earth_orbit_trail(instant("2149-12-31 23:59:59"))
        .unwrap()
        .end;
    assert_eq!(end.calendar().year, 2149);
    assert_eq!(end.calendar().month, 12);
    assert_eq!(end.calendar().day, 31);
    assert!(end.tt_seconds_j2000() - instant("2149-12-31 23:59:59").tt_seconds_j2000() > 0.99999);
}
