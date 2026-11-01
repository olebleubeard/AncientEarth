//! Independent apparent-sphere contact policy and fixed-speed regressions.
//! The synthetic spheres test navigation constraints, not celestial dynamics.
use earth_math::{astro::*, navigation::*, *};

const X: Vec3d = Vec3d {
    x: 1.0,
    y: 0.0,
    z: 0.0,
};
const Y: Vec3d = Vec3d {
    x: 0.0,
    y: 1.0,
    z: 0.0,
};
const Z: Vec3d = Vec3d {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

fn packet() -> AstronomyFrame {
    Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp"))
        .unwrap()
        .sample(
            AstroInstant::from_tt_seconds_j2000(0.0).unwrap(),
            None,
            Vec3d::ZERO,
        )
        .unwrap()
}

fn spheres(mut packet: AstronomyFrame, center: Vec3d, radius: f64) -> AstronomyFrame {
    packet.sun.apparent_center_ecef_m = center;
    packet.sun.center_ecef_m = center;
    packet.sun.radius_m = radius;
    packet.moon.apparent_center_ecef_m = center + Z * 1e9;
    packet.moon.center_ecef_m = packet.moon.apparent_center_ecef_m;
    packet.moon.radius_m = 1.0;
    packet
}

fn fly(
    observer: &mut Observer,
    frame: &AstronomyFrame,
    direction: Vec3d,
    distance: f64,
) -> FlightReport {
    observer.set_view_axes(direction, Z).unwrap();
    observer.speed_mps = distance * 4.0;
    FlightController::default().step(
        &TerrainScene::boundary_only(vec![]).unwrap(),
        observer,
        Some(frame),
        FlightSettings {
            adaptive_speed: false,
        },
        [1.0, 0.0, 0.0],
        0.25,
        false,
        false,
    )
}

#[test]
fn contact_and_inside_escape_distinguish_navigation_from_measurement_exit_roots() {
    let template = packet();
    for height in [10_000_000.0, ASTRONOMICAL_UNIT_M, MAX_HEIGHT_M * 0.5] {
        let start = Observer::new(Geodetic::new(0.0, 0.0, height).unwrap());
        let eye = start.eye_position();
        let tolerance = 1e-5_f64.max(16.0 * f64::EPSILON * eye.length());
        // Equatorial radial feet and eye have the same displacement, giving
        // an exact independent one-dimensional geometric oracle.
        for (radial, direction, distance, permitted) in [
            (100.0, X, 20.0, true),
            (100.0, -X, 20.0, false),
            (25.0, X, 200.0, true),
            (25.0, -X, 200.0, false),
            (0.0, X, 200.0, true),
            (100.0, Y, 20.0, true),
            (100.0, -Y, 20.0, true),
            (100.0 - 0.5 * tolerance, X, 20.0, true),
            (100.0 + 0.5 * tolerance, -X, 20.0, false),
        ] {
            let frame = spheres(template, eye - X * radial, 100.0);
            let mut observer = start;
            let report = fly(&mut observer, &frame, direction, distance);
            if permitted {
                assert!(
                    !report.blocked,
                    "height={height} radial={radial} direction={direction:?}"
                );
                assert!((report.travelled_m - distance).abs() <= 4.0 * tolerance);
                assert!(
                    observer.eye_position().distance(eye + direction * distance) <= 5.0 * tolerance
                );
                assert!(
                    observer
                        .eye_position()
                        .distance(frame.sun.apparent_center_ecef_m)
                        + tolerance
                        >= radial
                );
            } else {
                assert!(report.blocked);
                assert_eq!(report.travelled_m, 0.0);
                assert_eq!(observer.feet, start.feet);
            }
        }
        // Measuring an exit remains meaningful even though moving inward is
        // forbidden; outward contact measurement is still exactly zero.
        let inside = spheres(template, eye - X * 25.0, 100.0);
        for (direction, expected) in [(X, 75.0), (-X, 125.0)] {
            let hit = inside
                .nearest_celestial(Ray::new(eye, direction).unwrap(), None)
                .unwrap();
            assert!((hit.distance_m - expected).abs() < 1e-10);
        }
        let contact = spheres(template, eye - X * 100.0, 100.0);
        assert_eq!(
            contact
                .nearest_celestial(Ray::new(eye, X).unwrap(), None)
                .unwrap()
                .distance_m,
            0.0
        );
    }
}

#[test]
fn exterior_entry_tangent_and_second_sphere_remain_independent_constraints() {
    let template = packet();
    let start = Observer::new(Geodetic::new(0.0, 0.0, 10_000_000.0).unwrap());
    let eye = start.eye_position();
    // Circle oracle: center lies d along X and b perpendicular; first root
    // d-sqrt(R²-b²), including the declared exterior tangent touch-stop.
    for (radius, impact) in [(10.0_f64, 0.0_f64), (10.0, 6.0), (10.0, 10.0)] {
        let frame = spheres(template, eye + X * 100.0 + Y * impact, radius);
        let expected = 100.0 - (radius * radius - impact * impact).sqrt();
        let measured = frame
            .nearest_celestial(Ray::new(eye, X).unwrap(), None)
            .unwrap();
        assert!((measured.distance_m - expected).abs() < 1e-10);
        let mut observer = start;
        let report = fly(&mut observer, &frame, X, 200.0);
        assert!(report.blocked);
        assert!((report.travelled_m - expected).abs() < 0.00005);
        assert!(report.travelled_m <= expected + 1e-8);
    }
    for reverse_order in [false, true] {
        let mut frame = spheres(template, eye - X * 100.0, 100.0);
        frame.moon.apparent_center_ecef_m = eye + X * 100.0;
        frame.moon.radius_m = 10.0;
        if reverse_order {
            std::mem::swap(&mut frame.sun, &mut frame.moon);
        }
        let mut observer = start;
        let report = fly(&mut observer, &frame, X, 200.0);
        assert!(report.blocked);
        assert!((report.travelled_m - 90.0).abs() < 0.00005);
    }
    // The contact allowance admits only roundoff-sized initial radial
    // decrease. The independent minimum on s>=0 is sqrt(r²-m²) for m<0.
    for coefficient in [-0.5, -2.0] {
        let frame = spheres(template, eye - X * 100.0, 100.0);
        let delta = 1e-5;
        let radial_component = coefficient * delta / 100.0;
        let direction =
            X * radial_component + Y * (1.0 - radial_component * radial_component).sqrt();
        let m = 100.0 * radial_component;
        let minimum = (10000.0 - m * m).sqrt();
        let mut observer = start;
        let report = fly(&mut observer, &frame, direction, 20.0);
        if coefficient == -0.5 {
            assert!(!report.blocked);
            assert!(100.0 - minimum <= delta);
        } else {
            assert!(report.blocked);
            assert_eq!(report.travelled_m, 0.0);
        }
    }
}

#[test]
fn fixed_speed_matches_unit_and_modifier_rules_independent_of_clearance() {
    let smooth = TerrainScene::boundary_only(vec![]).unwrap();
    let raised = TerrainScene::new(vec![
        TerrainPatch::from_grid(2, 2, -0.001, 0.001, 0.002, 0.002, vec![900.0; 4]).unwrap(),
    ]);
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 1000.0).unwrap());
    let close_body = spheres(packet(), observer.eye_position() + X * 10.0, 1.0);
    let controller = FlightController::default();
    for base in [
        -1.0,
        0.0,
        MIN_SPEED_MPS,
        10.0,
        MAX_SPEED_MPS / 16.0,
        MAX_SPEED_MPS,
        MAX_SPEED_MPS * 2.0,
    ] {
        observer.speed_mps = base;
        for (boost, precision, factor) in [
            (false, false, 1.0),
            (true, false, 8.0),
            (false, true, 0.1),
            (true, true, 0.8),
        ] {
            let expected = (base.clamp(MIN_SPEED_MPS, MAX_SPEED_MPS) * factor).min(MAX_SPEED_MPS);
            for scene in [&smooth, &raised] {
                for frame in [None, Some(&close_body)] {
                    let actual = controller.effective_speed(
                        scene,
                        observer,
                        frame,
                        FlightSettings {
                            adaptive_speed: false,
                        },
                        boost,
                        precision,
                    );
                    assert!((actual - expected).abs() <= 4.0 * f64::EPSILON * expected.max(1.0));
                }
            }
        }
    }
    for base in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        observer.speed_mps = base;
        assert_eq!(
            controller.effective_speed(
                &smooth,
                observer,
                None,
                FlightSettings {
                    adaptive_speed: false
                },
                false,
                false
            ),
            0.0
        );
    }
    observer.speed_mps = 10.0;
    let adaptive = FlightSettings {
        adaptive_speed: true,
    };
    assert!(
        (controller.effective_speed(&smooth, observer, None, adaptive, false, false) - 210.0).abs()
            < 1e-10
    );
    assert!(controller.effective_speed(&raised, observer, None, adaptive, false, false) < 31.0);
    assert!(
        (controller.effective_speed(&smooth, observer, Some(&close_body), adaptive, false, false)
            - 11.8)
            .abs()
            < 1e-10
    );
}
