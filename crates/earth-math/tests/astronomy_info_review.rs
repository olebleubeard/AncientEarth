//! Scalar information checks independent of the report's derivation/display code.
use earth_math::astro::info::{
    AstronomyInfo, BodyInfo, BodyObservation, SphereGeometry, format_paired_length,
};
use earth_math::astro::*;
use earth_math::{EyePosture, Geodetic, Observer, Vec3d};
use std::path::PathBuf;
use std::sync::OnceLock;

#[allow(dead_code)]
#[path = "fixtures/astronomy.rs"]
mod astronomy_reference;
#[path = "fixtures/eclipses.rs"]
mod topocentric_reference;

fn close(value: f64, expected: f64, tolerance: f64) {
    assert!(
        value.is_finite() && (value - expected).abs() <= tolerance,
        "{value:.17e} != {expected:.17e} within {tolerance}"
    );
}
fn norm(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}
fn sources() -> &'static (Ephemeris, EarthOrientationData) {
    static SOURCES: OnceLock<(Ephemeris, EarthOrientationData)> = OnceLock::new();
    SOURCES.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/astronomy");
        (
            Ephemeris::from_bytes(&std::fs::read(root.join("de440s.bsp")).unwrap()).unwrap(),
            EarthOrientationData::from_bytes(&std::fs::read(root.join("finals2000A.all")).unwrap())
                .unwrap(),
        )
    })
}
fn instant(label: &str) -> AstroInstant {
    AstroInstant::from_calendar(CalendarDateTime::parse(label).unwrap()).unwrap()
}
fn row<'a>(body: &'a BodyInfo, label: &str) -> &'a str {
    &body.rows.iter().find(|r| r.label == label).unwrap().value
}
fn paired_metres(value: &str) -> f64 {
    value.split_whitespace().nth(3).unwrap().parse().unwrap()
}

#[test]
fn adopted_sizes_units_and_ellipsoid_meridian_have_independent_references() {
    assert_eq!(SUN_RADIUS_M, 695_700_000.0); // IAU 2015 Resolution B3 nominal constant.
    assert_eq!(MOON_RADIUS_M, 1_737_400.0); // NASA/JPL adopted mean radius, not exact physical limb.
    for (radius, diameter) in [
        (SUN_RADIUS_M, 1_391_400_000.0),
        (MOON_RADIUS_M, 3_474_800.0),
    ] {
        let sphere = SphereGeometry::new(radius).unwrap();
        assert_eq!(sphere.radius_m, radius);
        assert_eq!(sphere.diameter_m, diameter);
        // Circumference lies between regular inscribed/circumscribed polygons.
        let n = 65_536.0;
        let angle = std::f64::consts::PI / n;
        assert!(sphere.circumference_m > n * diameter * angle.sin());
        assert!(sphere.circumference_m < n * diameter * angle.tan());
    }
    assert_eq!(
        format_paired_length(1609.344),
        "1.609 km / 1609.344 m | 1.000 mi / 5280.000 ft"
    );
    assert_eq!(
        format_paired_length(-0.3048),
        "-0.000 km / -0.305 m | -0.000 mi / -1.000 ft"
    );
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(SphereGeometry::new(invalid).is_none());
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        assert_eq!(format_paired_length(invalid), "Unavailable");
    }
    let info = AstronomyInfo::new(
        instant("2024-04-08 18:40:00"),
        Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap()),
        true,
        None,
    );
    assert!(!info.live_available);
    assert_eq!(
        info.bodies.each_ref().map(|b| b.name),
        ["Earth", "Sun", "Moon"]
    );
    let earth = &info.bodies[0];
    close(
        paired_metres(row(earth, "Equatorial radius a (defining WGS84)")),
        6_378_137.0,
        0.0,
    );
    close(
        paired_metres(row(earth, "Polar radius b (derived WGS84)")),
        6_356_752.314_245_179,
        0.0005,
    );
    // Integrate the independent ellipsoid parameterization (a cos t, b sin t),
    // using composite Simpson; no production inverse-geodesic call is involved.
    let a = 6_378_137.0_f64;
    let b = a * (1.0 - 1.0 / 298.257_223_563);
    let steps = 8192;
    let h = std::f64::consts::FRAC_PI_2 / f64::from(steps);
    let speed = |t: f64| (a * t.sin()).hypot(b * t.cos());
    let mut integral = speed(0.0) + speed(std::f64::consts::FRAC_PI_2);
    for i in 1..steps {
        integral += if i % 2 == 0 { 2.0 } else { 4.0 } * speed(f64::from(i) * h);
    }
    let meridian = 4.0 * h / 3.0 * integral;
    close(
        paired_metres(row(earth, "Meridional circumference")),
        meridian,
        0.0005,
    );
    assert!((meridian - 2.0 * std::f64::consts::PI * b).abs() > 60_000.0);
    close(
        paired_metres(row(earth, "Eyes to Earth's center")),
        6_378_138.72,
        0.0005,
    );
    close(
        paired_metres(row(earth, "Eye height above WGS84 ellipsoid")),
        1.72,
        0.0005,
    );
}

#[test]
fn observation_keeps_three_distances_distinct_and_sphere_domain_explicit() {
    let mut body = CelestialBody {
        center_ecef_m: Vec3d::new(3.0, 4.0, 12.0),
        apparent_center_ecef_m: Vec3d::new(3.0, 4.0, 6.0),
        radius_m: 3.0,
        body_to_ecef: Mat3d::identity(),
        light_time_seconds: 0.125,
    };
    let eye = Vec3d::new(3.0, 4.0, 0.0);
    let observation = BodyObservation::new(body, eye).unwrap();
    assert_eq!(observation.geometric_geocentric_distance_m, 13.0);
    assert_eq!(observation.geometric_eye_distance_m, 12.0);
    assert_eq!(observation.apparent_eye_distance_m, 6.0);
    close(
        observation.apparent_angular_diameter_deg.unwrap(),
        60.0,
        2e-14,
    );
    assert_eq!(observation.light_time_seconds, 0.125);
    for distance in [0.0, 2.0, 3.0] {
        body.apparent_center_ecef_m = eye + Vec3d::new(0.0, 0.0, distance);
        let sample = BodyObservation::new(body, eye).unwrap();
        assert_eq!(sample.apparent_eye_distance_m, distance);
        assert!(sample.apparent_angular_diameter_deg.is_none());
    }
    body.apparent_center_ecef_m = eye + Vec3d::new(0.0, 0.0, 3.000_000_001);
    let near_surface = BodyObservation::new(body, eye).unwrap();
    assert!(near_surface.apparent_angular_diameter_deg.unwrap() < 180.0);
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        let mut bad = body;
        bad.light_time_seconds = value;
        assert!(BodyObservation::new(bad, eye).is_none());
    }
    for value in [f64::NAN, f64::INFINITY, f64::MAX] {
        let mut bad = body;
        bad.center_ecef_m = Vec3d::new(value, value, value);
        assert!(BodyObservation::new(bad, eye).is_none());
        let mut bad = body;
        bad.apparent_center_ecef_m = Vec3d::new(value, value, value);
        assert!(BodyObservation::new(bad, eye).is_none());
        assert!(BodyObservation::new(body, Vec3d::new(value, value, value)).is_none());
    }
}

#[test]
fn current_body_distances_and_angles_agree_with_existing_cspice_erfa_references() {
    let (ephemeris, eop) = sources();
    for &(tt, id, geometric, apparent, light_time) in astronomy_reference::SAMPLE_BODIES {
        let epoch = AstroInstant::from_tt_seconds_j2000(tt).unwrap();
        let frame = ephemeris.sample(epoch, None, Vec3d::ZERO).unwrap();
        assert_eq!(frame.sample_instant, epoch);
        assert_eq!(frame.sample_observer_ecef_m, Vec3d::ZERO);
        assert!(frame.sample_follow_earth);
        let body = if id == 10 { frame.sun } else { frame.moon };
        let sample = BodyObservation::new(body, Vec3d::ZERO).unwrap();
        close(
            sample.geometric_geocentric_distance_m,
            norm(geometric),
            0.001,
        );
        close(sample.geometric_eye_distance_m, norm(geometric), 0.001);
        close(sample.apparent_eye_distance_m, norm(apparent), 0.001);
        close(sample.light_time_seconds, light_time, 1e-9);
    }
    for &(label, geo, _ecef, sun, moon, angles) in topocentric_reference::CASES {
        let mut observer = Observer::new(Geodetic::new(geo[0], geo[1], 0.0).unwrap());
        // The archived reference stores the receiver eye, not the feet. The
        // constructor's initial h>=0 policy is distinct from valid terrain poses.
        observer.feet = Geodetic::new(geo[0], geo[1], geo[2] - 1.72).unwrap();
        let epoch = instant(label);
        let frame = ephemeris
            .sample(epoch, Some(eop), observer.eye_position())
            .unwrap();
        let info = AstronomyInfo::new(epoch, observer, true, Some(&frame));
        assert!(info.live_available);
        for (index, body, vector) in [(0, frame.sun, sun), (1, frame.moon, moon)] {
            let sample = BodyObservation::new(body, observer.eye_position()).unwrap();
            // Existing split-date ERFA/CSPICE vectors have independent epoch rounding;
            // use their reviewed vector tolerances, not a claimed physical error bound.
            close(
                sample.apparent_eye_distance_m,
                norm(vector),
                if index == 0 { 2.0 } else { 0.006 },
            );
            close(
                sample.apparent_angular_diameter_deg.unwrap(),
                (2.0 * angles[index]).to_degrees(),
                1.2e-10,
            );
            close(
                sample.apparent_eye_distance_m / LIGHT_SPEED_MPS,
                sample.light_time_seconds,
                1e-9,
            );
            let displayed = row(&info.bodies[index + 1], "Eyes to apparent model center");
            close(
                paired_metres(displayed),
                sample.apparent_eye_distance_m,
                0.00051,
            );
        }
    }
}

#[test]
fn frame_identity_prevents_stale_time_eye_or_motion_values_and_invalid_data_fail_atomically() {
    let epoch = instant("2024-04-08 18:40:00");
    let observer = Observer::new(Geodetic::new(29.305549, -94.767015, 10.0).unwrap());
    let frame = sources()
        .0
        .sample_with_tracking(epoch, Some(&sources().1), observer.eye_position(), true)
        .unwrap();
    let valid = AstronomyInfo::new(epoch, observer, true, Some(&frame));
    assert!(valid.live_available);
    let assert_unavailable = |info: AstronomyInfo| {
        assert!(!info.live_available);
        for body in &info.bodies[1..] {
            assert!(!body.rows[0].value.contains("Unavailable"));
            for field in [
                "Eyes to apparent model center",
                "Eyes to body center (simultaneous)",
                "Earth center to body center (simultaneous)",
                "Center light travel time (reception solution)",
            ] {
                assert!(row(body, field).contains("Unavailable"));
            }
        }
    };
    assert_unavailable(AstronomyInfo::new(epoch, observer, true, None));
    let shifted =
        AstroInstant::from_tt_seconds_j2000(f64::from_bits(epoch.tt_seconds_j2000().to_bits() + 1))
            .unwrap();
    assert_ne!(epoch, shifted);
    assert_eq!(
        epoch.tt_jd(),
        shifted.tt_jd(),
        "fixture needs aliased full Julian dates"
    );
    assert_unavailable(AstronomyInfo::new(shifted, observer, true, Some(&frame)));
    assert_unavailable(AstronomyInfo::new(epoch, observer, false, Some(&frame)));
    let mut looked = observer;
    looked.heading_deg += 60.0;
    looked.pitch_deg = 45.0;
    looked.roll_deg = 38.0;
    assert!(AstronomyInfo::new(epoch, looked, true, Some(&frame)).live_available);
    let mut crouched = observer;
    EyePosture {
        crouched: true,
        ..EyePosture::default()
    }
    .apply(&mut crouched)
    .unwrap();
    assert_unavailable(AstronomyInfo::new(epoch, crouched, true, Some(&frame)));
    let mut moved = observer;
    moved.feet.lon_deg += 0.000001;
    assert_unavailable(AstronomyInfo::new(epoch, moved, true, Some(&frame)));
    for broken in 0..7 {
        let mut bad = frame;
        match broken {
            0 => bad.sun.center_ecef_m.x = f64::NAN,
            1 => bad.moon.apparent_center_ecef_m.y = f64::INFINITY,
            2 => bad.sun.light_time_seconds = -1.0,
            3 => bad.moon.radius_m += 1.0,
            4 => bad.earth_rotation_angle_rad = f64::NAN,
            5 => bad.moon_illuminated_fraction = f64::NAN,
            _ => bad.moon_illuminated_fraction = 1.01,
        }
        assert_unavailable(AstronomyInfo::new(epoch, observer, true, Some(&bad)));
    }
    for bad_value in [
        f64::NAN,
        f64::INFINITY,
        -1.0,
        0.05,
        earth_math::STATURE_M + 0.1,
    ] {
        let mut bad = observer;
        bad.eye_height_m = bad_value;
        assert_unavailable(AstronomyInfo::new(epoch, bad, true, Some(&frame)));
    }
    // An invalid feet height must not become valid merely by adding the eye
    // offset. Match the sample identity explicitly so that this tests domain
    // validation rather than incidental staleness.
    let mut invalid_feet = observer;
    invalid_feet.feet.height_m = -10_000.05;
    invalid_feet.eye_height_m = 0.1;
    assert!(
        Geodetic::new(
            invalid_feet.eye_geodetic().lat_deg,
            invalid_feet.eye_geodetic().lon_deg,
            invalid_feet.eye_geodetic().height_m
        )
        .is_ok()
    );
    let mut matching = frame;
    matching.sample_observer_ecef_m = invalid_feet.eye_position();
    assert_unavailable(AstronomyInfo::new(
        epoch,
        invalid_feet,
        true,
        Some(&matching),
    ));
    let unlocked = sources()
        .0
        .sample_with_tracking(epoch, Some(&sources().1), observer.eye_position(), false)
        .unwrap();
    assert!(!unlocked.sample_follow_earth);
    assert!(AstronomyInfo::new(epoch, observer, false, Some(&unlocked)).live_available);
    assert!(
        AstronomyInfo::new(instant("1900-01-01 12:00:00"), observer, true, None)
            .epoch
            .contains("modeled UT1")
    );
    assert!(
        AstronomyInfo::new(instant("2016-12-31 23:59:60"), observer, true, None)
            .epoch
            .contains("23:59:60")
    );
}
