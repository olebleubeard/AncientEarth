//! Independent checks of the single camera and display-coordinate choices.
use earth_math::{astro::*, rendering::continuous::*, *};

fn close(a: Vec3d, b: Vec3d, tolerance: f64) {
    assert!(
        a.is_finite() && a.distance(b) <= tolerance,
        "{} > {tolerance}",
        a.distance(b)
    );
}

#[test]
fn coordinate_anchor_hysteresis_never_changes_the_canonical_camera_or_lens() {
    // The short shadow cascades cannot distinguish metre-scale corners when
    // added to AU-sized f32 camera coordinates. Disable them by actual eye
    // height, independently of the delayed mesh/anchor state, and restore below.
    let far_coordinate = ASTRONOMICAL_UNIT_M as f32;
    assert_eq!(far_coordinate + 12.0, far_coordinate);
    for eye_height in [
        999_999.5,
        1_000_000.0,
        1_000_000.5,
        MAX_HEIGHT_M - 1.0,
        999_999.5,
    ] {
        let observer =
            Observer::new(Geodetic::new(0.0, 0.0, eye_height - DEFAULT_EYE_HEIGHT_M).unwrap());
        assert_eq!(nearby_shadow_maps(observer), eye_height < 1_000_000.0);
    }
    for (latitude, longitude) in [(0.0, 0.0), (72.0, 179.999), (-90.0, -95.0)] {
        let mut was_space = false;
        for (height, expected_space) in [
            (0.0, false),
            (1_999_999.0, false),
            (2_000_000.0, true),
            (3_000_000.0, true),
            (30_000_000.0, true),
            (ASTRONOMICAL_UNIT_M, true),
            (MAX_HEIGHT_M - 2.0, true),
            (1_500_000.0, true),
            (999_999.0, false),
            (0.0, false),
        ] {
            let mut observer = Observer::new(
                Geodetic::new(latitude, longitude, height - DEFAULT_EYE_HEIGHT_M).unwrap(),
            );
            observer.heading_deg = 123.0;
            observer.pitch_deg = -17.0;
            observer.roll_deg = 39.0;
            let before = observer;
            let plan = DisplayPlan::at(observer, was_space);
            assert_eq!(plan.space, expected_space);
            assert_eq!(observer, before);
            assert_eq!(plan.detail.eye, observer.eye_position());
            if expected_space {
                close(
                    plan.local.origin,
                    Vec3d::new(WGS84_A_M + DEFAULT_EYE_HEIGHT_M, 0.0, 0.0),
                    0.0,
                );
            } else {
                assert_eq!(plan.coordinate_observer, observer);
                assert_eq!(plan.local.origin, observer.eye_position());
            }
            let frame = observer.frame();
            let (position, forward, up) = plan.local.camera(frame);
            let expand = |v: [f32; 3]| {
                plan.local.basis.east * f64::from(v[0]) + plan.local.basis.up * f64::from(v[1])
                    - plan.local.basis.north * f64::from(v[2])
            };
            close(
                plan.local.origin + expand(position),
                frame.origin,
                0.001 + 2.0 * f64::from(f32::EPSILON) * frame.origin.distance(plan.local.origin),
            );
            close(expand(forward), frame.forward, 1e-7);
            close(expand(up), frame.up, 1e-7);
            // Earth coordinates remain Earth-scale under the far anchor even
            // though the camera translation is necessarily lower precision.
            if expected_space {
                for (lat, lon) in [(-90.0, 0.0), (0.0, 180.0), (90.0, 0.0), (45.0, 73.0)] {
                    let p = geodetic_to_ecef(Geodetic::new(lat, lon, 0.0).unwrap());
                    close(plan.local.origin + expand(plan.local.position(p)), p, 1.0);
                }
            }
            was_space = plan.space;
        }
    }
}

#[test]
fn cached_reference_cell_distances_are_lower_bounds_throughout_the_eye_ball() {
    for (lat, lon, height) in [
        (0.0, 0.0, 2e6),
        (80.0, 179.0, 5e6),
        (-90.0, 40.0, ASTRONOMICAL_UNIT_M),
    ] {
        let observer = Observer::new(Geodetic::new(lat, lon, height).unwrap());
        let detail = ReferenceDetail::space(observer);
        assert!(detail.valid_for(observer, 16000.0, 16000.0));
        assert!(!detail.valid_for(observer, 16000.0_f64.next_up(), 16000.0));
        assert!(!detail.valid_for(observer, f64::NAN, 16000.0));
        for latitude in [-90.0, -70.0, -20.0, 0.0, 45.0, 85.0, 90.0] {
            for longitude in [-180.0, -120.0, -30.0, 0.0, 75.0, 179.0] {
                let p = geodetic_to_ecef(Geodetic::new(latitude, longitude, 0.0).unwrap());
                // Synthetic containing ball with known member p; the oracle
                // measures actual distance to p, independently of cell rules.
                let center = p + Vec3d::new(1000.0, -2000.0, 3000.0);
                let radius = p.distance(center);
                let bound = detail.cell_distance(center, radius);
                for direction in [
                    Vec3d::new(1.0, 0.0, 0.0),
                    Vec3d::new(0.0, -1.0, 0.0),
                    Vec3d::new(0.0, 0.0, 1.0),
                    (p - detail.eye).normalized().unwrap(),
                ] {
                    for fraction in [0.0, 0.5, 1.0] {
                        let moved =
                            detail.eye + direction * (fraction * detail.movement_allowance_m);
                        assert!(bound <= moved.distance(p) + 0.001);
                    }
                }
            }
        }
        let moved_eye =
            detail.eye + EnuBasis::at(observer.feet).up * (detail.movement_allowance_m * 1.001);
        let moved = navigation::observer_from_ecef_pose(
            observer,
            moved_eye,
            observer.frame().forward,
            observer.frame().up,
        )
        .unwrap();
        assert!(!detail.valid_for(moved, 100.0, 16000.0));
    }
}

#[test]
fn annotations_use_the_existing_lens_and_apparent_centers_at_every_supported_scale() {
    let ephemeris =
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap();
    let instant = AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap();
    let orbit = ephemeris.orbit_frame(instant, None).unwrap();
    for height in [20e6, ASTRONOMICAL_UNIT_M, MAX_HEIGHT_M - 2.0] {
        let mut observer = Observer::new(Geodetic::new(37.0, -94.0, height).unwrap());
        let astronomy = ephemeris
            .sample(instant, None, observer.eye_position())
            .unwrap();
        for (body, point) in [
            (rendering::orbit::OrbitBody::Earth, Vec3d::ZERO),
            (
                rendering::orbit::OrbitBody::Sun,
                astronomy.sun.apparent_center_ecef_m,
            ),
            (
                rendering::orbit::OrbitBody::Moon,
                astronomy.moon.apparent_center_ecef_m,
            ),
        ] {
            observer.aim_at(point).unwrap();
            for lens in [
                Lens::default(),
                Lens::new(Projection::EquidistantFisheye, 140.0),
                Lens::new(Projection::Rectilinear, 0.2),
            ] {
                let view = celestial_view(observer, &orbit);
                for (x, y) in [(0.0, 0.0), (-0.5, 0.3), (0.75, -0.25)] {
                    if let Some(ray) = lens.ray(observer.frame(), x, y, 1.6) {
                        close(
                            lens.ray(view, x, y, 1.6).unwrap(),
                            orbit.earth_to_icrf.transform(ray),
                            1e-14,
                        );
                    }
                }
                let display = [1000.0, 800.0];
                let scale = [1.5, 1.25];
                let annotations = continuous_annotations(
                    observer, lens, &orbit, &astronomy, None, display, scale, false, false,
                );
                if space_annotations_visible(observer, lens) {
                    let label = annotations.bodies.iter().find(|x| x.body == body).unwrap();
                    let projected = lens
                        .project(observer.frame(), point - observer.eye_position(), 1.5)
                        .unwrap();
                    assert!(
                        (f64::from(label.position[0]) - (projected[0] + 1.0) * 500.0).abs() < 0.002
                    );
                    assert!(
                        (f64::from(label.position[1]) - (1.0 - projected[1]) * 400.0).abs() < 0.002
                    );
                } else {
                    assert!(annotations.bodies.is_empty());
                }
            }
        }
    }
}

#[test]
fn explicit_orbit_fit_keeps_optics_and_encloses_the_dated_geometry() {
    let ephemeris =
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap();
    let instant = AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap();
    let orbit = ephemeris.orbit_frame(instant, None).unwrap();
    let trail = ephemeris.earth_orbit_trail(instant).unwrap();
    let observer = Observer::new(Geodetic::new(29.0, -95.0, 100.0).unwrap());
    for (lens, dimensions) in [
        (Lens::default(), (1920, 1080)),
        (
            Lens::new(Projection::EquidistantFisheye, 140.0),
            (900, 1200),
        ),
    ] {
        let fitted = fit_orbit_observer(observer, lens, &orbit, Some(&trail), dimensions).unwrap();
        let view = celestial_view(fitted, &orbit);
        let aspect = f64::from(dimensions.0) / f64::from(dimensions.1);
        for point in &trail.points {
            let relative = point.earth_position_m - orbit.earth_position_m - view.origin;
            let screen = lens.project(view, relative, aspect).unwrap();
            assert!(screen[0].abs() < 1.0 && screen[1].abs() < 1.0);
            // Distance from the viewing axis includes physical Earth radius,
            // so this verifies body extent as well as center-line containment.
            let separation = view
                .forward
                .cross(relative)
                .length()
                .atan2(view.forward.dot(relative));
            let extent = (WGS84_A_M / relative.length()).asin();
            assert!(separation + extent < lens.vertical_fov_deg.to_radians() * 0.5);
        }
        assert_eq!(fitted.eye_height_m, observer.eye_height_m);
        assert_eq!(fitted.speed_mps, observer.speed_mps);
    }
    assert!(fit_orbit_observer(observer, Lens::default(), &orbit, None, (0, 1080)).is_err());
    assert!(
        fit_orbit_observer(
            observer,
            Lens::new(Projection::Rectilinear, MIN_FOV_DEG),
            &orbit,
            Some(&trail),
            (1920, 1080)
        )
        .is_err()
    );
}
