//! Independent geometric/projection checks for the separate orbital camera.
use earth_math::astro::orbit::{OrbitFrame, OrbitPoint, OrbitTrail};
use earth_math::astro::{AstroInstant, Ephemeris, Mat3d};
use earth_math::rendering::orbit::*;
use earth_math::{CameraFrame, Ray, Vec3d, WGS84_A_M, WGS84_B_M};
use std::sync::OnceLock;

fn scene() -> OrbitFrame {
    static VALUE: OnceLock<OrbitFrame> = OnceLock::new();
    *VALUE.get_or_init(|| {
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp"))
            .unwrap()
            .orbit_frame(
                AstroInstant::from_tt_seconds_j2000(845_856_123.456).unwrap(),
                None,
            )
            .unwrap()
    })
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        a.is_finite() && (a - b).abs() <= tolerance,
        "{a:.17e} vs {b:.17e}; {tolerance}"
    );
}
fn basis(frame: CameraFrame) {
    for axis in [frame.right, frame.up, frame.forward] {
        close(axis.length(), 1.0, 5e-15);
    }
    close(frame.right.dot(frame.up), 0.0, 5e-15);
    close(frame.forward.dot(frame.up), 0.0, 5e-15);
    close(
        frame.forward.cross(frame.up).distance(frame.right),
        0.0,
        5e-15,
    );
}

#[test]
fn independent_camera_retains_icrf_axes_and_aim_preserves_eye_with_bounded_physical_scale() {
    let scene = scene();
    let mut camera = OrbitCamera::default();
    camera.fit_earth(&scene);
    let first = camera.frame(&scene).unwrap();
    basis(first);
    close(
        first.origin.distance(scene.earth_position_m),
        3.5 * WGS84_A_M,
        4e-5,
    );
    assert!(first.origin.distance(Vec3d::ZERO) < scene.earth_position_m.length());
    let mut moved = scene;
    moved.earth_position_m = moved.earth_position_m + Vec3d::new(1e8, -2e8, 3e8);
    moved.orbital_normal_icrf = Vec3d::new(1.0, 0.0, 0.0);
    let later = camera.frame(&moved).unwrap();
    assert_eq!(later.forward, first.forward);
    assert_eq!(later.up, first.up);
    close(
        (later.origin - first.origin).distance(moved.earth_position_m - scene.earth_position_m),
        0.0,
        4e-5,
    );
    for body in [OrbitBody::Sun, OrbitBody::Moon, OrbitBody::Earth] {
        let previous = camera.frame(&scene).unwrap();
        camera.aim(body, &scene);
        let current = camera.frame(&scene).unwrap();
        basis(current);
        assert_eq!(camera.focus(), body);
        close(current.origin.distance(previous.origin), 0.0, 0.0002);
        close(
            current
                .forward
                .distance((body.center(&scene) - current.origin).normalized().unwrap()),
            0.0,
            1e-11,
        );
        let unchanged = camera;
        camera.dolly(f64::NAN);
        camera.orbit(f64::INFINITY, 0.0);
        assert_eq!(camera, unchanged);
    }
    for body in [OrbitBody::Earth, OrbitBody::Sun, OrbitBody::Moon] {
        camera.fit_orbit(&scene);
        camera.aim(body, &scene);
        for _ in 0..5 {
            camera.dolly(100.0);
        }
        close(camera.distance_m(), 1.05 * body.radius(), 0.0);
        for _ in 0..5 {
            camera.dolly(-100.0);
        }
        close(camera.distance_m(), 4.0 * 149_597_870_700.0, 0.0);
        for (dx, dy) in [(1e300, -1e300), (-725.0, 10000.0), (35.0, -10000.0)] {
            camera.orbit(dx, dy);
            basis(camera.frame(&scene).unwrap());
        }
    }
    // Retargeting to an object that contains the eye is rejected transactionally.
    camera.fit_earth(&scene);
    let mut containing = scene;
    containing.moon_position_m = camera.frame(&scene).unwrap().origin;
    let previous = camera;
    camera.aim(OrbitBody::Moon, &containing);
    assert_eq!(camera, previous);
    // Retargeting can put distance-to-new-focus beyond the ordinary wheel cap;
    // it must preserve the eye and not jump/reverse on the next wheel input.
    camera.fit_orbit(&scene);
    camera.orbit(900.0, -0.8_f64.asin().to_degrees() / 0.2);
    camera.dolly(-100.0);
    let old_eye = camera.frame(&scene).unwrap().origin;
    assert!(camera.aim_earth(&scene));
    assert!(camera.distance_m() > MAX_DISTANCE_M);
    close(
        camera.frame(&scene).unwrap().origin.distance(old_eye),
        0.0,
        0.0002,
    );
    let far_distance = camera.distance_m();
    camera.dolly(-1.0);
    assert_eq!(camera.distance_m(), far_distance);
    camera.dolly(1.0);
    close(
        camera.distance_m(),
        far_distance * (-0.18_f64).exp(),
        0.0002,
    );

    // Earth exterior is defined by the oriented ellipsoid, not its enclosing
    // a-radius sphere. This eye is just outside the pole but inside that sphere.
    camera.fit_orbit(&scene);
    let old_eye = camera.frame(&scene).unwrap().origin;
    let mut polar_target = scene;
    let north = scene.earth_to_icrf.transform(Vec3d::new(0.0, 0.0, 1.0));
    polar_target.earth_position_m = old_eye - north * (1.001 * WGS84_B_M);
    assert!(camera.aim_earth(&polar_target));
    assert!(camera.distance_m() < WGS84_A_M);
    close(
        camera
            .frame(&polar_target)
            .unwrap()
            .origin
            .distance(old_eye),
        0.0,
        0.0002,
    );
    let close_distance = camera.distance_m();
    camera.dolly(1.0);
    assert_eq!(camera.distance_m(), close_distance);
    camera.dolly(-1.0);
    assert!(camera.distance_m() > close_distance);
}

#[test]
fn physical_body_intersections_match_axis_and_implicit_quadratic_oracles() {
    let mut scene = scene();
    // A nontrivial proper rotation and large translation test the actual body frame.
    let rotation = Mat3d::rotation_z(0.37).mul(Mat3d::rotation_y(-0.91));
    scene.earth_to_icrf = rotation;
    scene.sun_to_icrf = rotation;
    scene.moon_to_icrf = rotation;
    for body in [OrbitBody::Earth, OrbitBody::Sun, OrbitBody::Moon] {
        let a = body.radius();
        let b = if body == OrbitBody::Earth {
            WGS84_B_M
        } else {
            a
        };
        let center = body.center(&scene);
        for (axis, radius) in [
            (Vec3d::new(1.0, 0.0, 0.0), a),
            (Vec3d::new(0.0, 0.0, 1.0), b),
        ] {
            for distance in [1.05 * a, 3.5 * a, MAX_DISTANCE_M] {
                let world_axis = rotation.transform(axis);
                let ray = Ray {
                    origin: center + world_axis * distance,
                    direction: world_axis * -7.0,
                };
                close(
                    body_intersection(body, &scene, ray).unwrap(),
                    distance - radius,
                    0.002,
                );
            }
            let ray = Ray {
                origin: center,
                direction: rotation.transform(axis),
            };
            close(body_intersection(body, &scene, ray).unwrap(), radius, 1e-6);
        }
        for y in [-0.7, -0.2, 0.0, 0.8, 1.2] {
            let local_o = Vec3d::new(4.0 * a, y * a, 0.2 * b);
            let local_d = Vec3d::new(-1.0, 0.02, -0.01).normalized().unwrap();
            let q = |v: Vec3d| Vec3d::new(v.x / a, v.y / a, v.z / b);
            let o = q(local_o);
            let d = q(local_d);
            let aa = d.dot(d);
            let bb = 2.0 * o.dot(d);
            let cc = o.dot(o) - 1.0;
            let discriminant = bb * bb - 4.0 * aa * cc;
            let actual = body_intersection(
                body,
                &scene,
                Ray {
                    origin: center + rotation.transform(local_o),
                    direction: rotation.transform(local_d),
                },
            );
            if discriminant < 0.0 {
                assert!(actual.is_none());
            } else {
                close(
                    actual.unwrap(),
                    (-bb - discriminant.sqrt()) / (2.0 * aa),
                    0.0002,
                );
            }
        }
        for direction in [Vec3d::ZERO, Vec3d::new(f64::NAN, 1.0, 0.0)] {
            assert!(
                body_intersection(
                    body,
                    &scene,
                    Ray {
                        origin: center,
                        direction
                    }
                )
                .is_none()
            );
        }
        assert!(
            body_intersection(
                body,
                &scene,
                Ray {
                    origin: center + rotation.transform(Vec3d::new(2.0 * a, 0.0, 0.0)),
                    direction: rotation.transform(Vec3d::new(1.0, 0.0, 0.0))
                }
            )
            .is_none()
        );
    }
}

fn trail(scene: &OrbitFrame, points: &[Vec3d]) -> OrbitTrail {
    OrbitTrail {
        start: scene.instant,
        end: scene.instant.shifted(1.0).unwrap(),
        points: points
            .iter()
            .map(|p| OrbitPoint {
                instant: scene.instant,
                earth_position_m: *p,
            })
            .collect(),
        clipped_to_calendar: false,
    }
}

#[test]
fn annotation_projection_clips_crossing_segments_and_matches_physical_aspect_at_hidpi() {
    let scene = scene();
    let camera = OrbitCamera::default();
    let frame = camera.frame(&scene).unwrap();
    let world = |x, y, z| frame.origin + frame.right * x + frame.up * y + frame.forward * z;
    for scale in [[1.0, 1.0], [1.5, 1.5], [2.0, 0.75]] {
        let display = [960.0, 640.0];
        let width = f64::from(display[0] * scale[0]);
        let height = f64::from(display[1] * scale[1]);
        let focal = height / (2.0 * 30_f64.to_radians().tan());
        let path = trail(
            &scene,
            &[world(-100.0, 50.0, 1000.0), world(200.0, -80.0, 1000.0)],
        );
        let measured = annotations(camera, &scene, Some(&path), display, scale, true, false);
        assert_eq!(measured.trail.len(), 1);
        for (point, (x, y)) in [measured.trail[0].a, measured.trail[0].b]
            .iter()
            .zip([(-100.0, 50.0), (200.0, -80.0)])
        {
            close(
                f64::from(point[0]),
                (width / 2.0 + focal * x / 1000.0) / f64::from(scale[0]),
                0.0001,
            );
            close(
                f64::from(point[1]),
                (height / 2.0 - focal * y / 1000.0) / f64::from(scale[1]),
                0.0001,
            );
        }
        let crossing = trail(&scene, &[world(-1e6, 0.0, 1000.0), world(1e6, 0.0, 1000.0)]);
        let out = annotations(camera, &scene, Some(&crossing), display, scale, true, false);
        assert_eq!(out.trail.len(), 1);
        close(f64::from(out.trail[0].a[0]), 0.0, 0.0001);
        close(f64::from(out.trail[0].b[0]), 960.0, 0.0001);
        let behind = trail(
            &scene,
            &[world(-100.0, 0.0, -1000.0), world(100.0, 0.0, -1000.0)],
        );
        assert!(
            annotations(camera, &scene, Some(&behind), display, scale, true, false)
                .trail
                .is_empty()
        );
        let through_front = trail(
            &scene,
            &[world(0.0, 0.0, -100.0), world(100.0, 50.0, 1000.0)],
        );
        let visible = annotations(
            camera,
            &scene,
            Some(&through_front),
            display,
            scale,
            true,
            false,
        );
        assert_eq!(visible.trail.len(), 1);
        for p in [visible.trail[0].a, visible.trail[0].b] {
            assert!(p[0] >= 0.0 && p[0] <= display[0] && p[1] >= 0.0 && p[1] <= display[1]);
        }
    }
    for scale in [[f32::NAN, 1.0], [1.0, 0.0], [1.0, -1.0]] {
        let out = annotations(camera, &scene, None, [960.0, 640.0], scale, true, true);
        assert!(out.axis.is_empty() && out.bodies.is_empty());
    }
    // The dated samples' straight chord deliberately misses the actual Earth.
    // An extra sample at the exact live epoch must join at the true center,
    // without changing either recorded endpoint or closing the path.
    let start = scene.instant.shifted(-100.0).unwrap();
    let end = scene.instant.shifted(100.0).unwrap();
    let dated = OrbitTrail {
        start,
        end,
        points: vec![
            OrbitPoint {
                instant: start,
                earth_position_m: world(-1e6, 1e6, 10e6),
            },
            OrbitPoint {
                instant: end,
                earth_position_m: world(1e6, 1e6, 10e6),
            },
        ],
        clipped_to_calendar: false,
    };
    let live = annotations(
        camera,
        &scene,
        Some(&dated),
        [960.0, 640.0],
        [1.0; 2],
        true,
        false,
    );
    assert_eq!(live.trail.len(), 2);
    assert_eq!(live.trail[0].b, live.trail[1].a);
    close(f64::from(live.trail[0].b[0]), 480.0, 1e-4);
    close(f64::from(live.trail[0].b[1]), 320.0, 1e-4);
    for outside in [start, end, end.shifted(1.0).unwrap()] {
        let mut at_boundary = scene;
        at_boundary.instant = outside;
        let out = annotations(
            camera,
            &at_boundary,
            Some(&dated),
            [960.0, 640.0],
            [1.0; 2],
            true,
            false,
        );
        assert_eq!(out.trail.len(), 1);
    }
}

#[test]
fn distant_annotations_are_labeled_and_do_not_change_physical_body_or_camera() {
    let scene = scene();
    let mut camera = OrbitCamera::default();
    camera.fit_earth(&scene);
    let close_up = annotations(
        camera,
        &scene,
        None,
        [1920.0, 1080.0],
        [1.0; 2],
        false,
        true,
    );
    assert!(!close_up.axis_exaggerated);
    assert_eq!(close_up.axis.len(), 1);
    assert!(close_up.labels.iter().any(|v| v.text.contains("CIP")));
    camera.fit_orbit(&scene);
    camera.aim_earth(&scene);
    let original = camera;
    let out = annotations(
        camera,
        &scene,
        None,
        [1920.0, 1080.0],
        [1.0; 2],
        false,
        true,
    );
    assert!(out.axis_exaggerated);
    assert!(out.labels.iter().any(|v| v.text.contains("enlarged")));
    assert!(
        out.bodies
            .iter()
            .find(|v| v.body == OrbitBody::Earth)
            .unwrap()
            .small_body
    );
    assert_eq!(camera, original);
    assert_eq!(OrbitBody::Earth.radius(), WGS84_A_M);
    let off = annotations(
        camera,
        &scene,
        None,
        [1920.0, 1080.0],
        [1.0; 2],
        false,
        false,
    );
    assert!(off.axis.is_empty() && off.normal.is_empty() && off.plane.is_empty());
    assert_eq!(
        bounded_label([1900.0, -10.0], [180.0, 30.0], [1920.0, 1080.0]),
        [1740.0, 0.0]
    );
    assert_eq!(
        bounded_label([20.0, 20.0], [3000.0, 2000.0], [1920.0, 1080.0]),
        [0.0, 0.0]
    );
}

#[test]
fn measured_caption_rectangles_share_one_nonoverlapping_hidpi_layout_and_fail_closed() {
    let separated = |a: earth_math::ScreenRect, b: earth_math::ScreenRect| {
        a.max[0] + 2.0 <= b.min[0]
            || b.max[0] + 2.0 <= a.min[0]
            || a.max[1] + 2.0 <= b.min[1]
            || b.max[1] + 2.0 <= a.min[1]
    };
    // Independent text rectangles representing the seven native caption kinds.
    // Coordinates are logical; the same physical viewport is tested at each scale.
    for scale in [[1.0_f32, 1.0], [1.5, 1.5], [2.0, 2.0], [2.0, 0.75]] {
        let display = [1920.0 / scale[0], 1080.0 / scale[1]];
        for fraction in [[0.5, 0.82], [0.02, 0.02], [0.98, 0.98]] {
            let anchor = [display[0] * fraction[0], display[1] * fraction[1]];
            let requests: Vec<_> = [110.0, 100.0, 110.0, 130.0, 170.0, 510.0, 720.0]
                .into_iter()
                .enumerate()
                .map(|(i, width)| OrbitCaption {
                    anchor,
                    preferred: [anchor[0] + 8.0, anchor[1] + i as f32 * 3.0],
                    text_size: [width, 13.0],
                    connect: i < 5,
                })
                .collect();
            let first = caption_layout(&requests, display);
            let second = caption_layout(&requests, display);
            assert_eq!(first.len(), requests.len());
            assert!(
                first.iter().all(Option::is_some),
                "display {display:?} anchor {anchor:?}"
            );
            for (i, ((request, actual), repeat)) in
                requests.iter().zip(&first).zip(&second).enumerate()
            {
                let p = actual.unwrap();
                assert_eq!(p.text_position, repeat.unwrap().text_position);
                assert_eq!(p.rect.min, p.text_position.map(|x| x - 4.0));
                assert_eq!(
                    p.rect.max,
                    [
                        p.text_position[0] + request.text_size[0] + 4.0,
                        p.text_position[1] + request.text_size[1] + 4.0
                    ]
                );
                for (axis, extent) in display.into_iter().enumerate() {
                    assert!(p.rect.min[axis] >= 2.0 && p.rect.max[axis] <= extent - 2.0);
                    // The actual UI's final six-pixel text clamp is a no-op.
                    assert_eq!(
                        p.text_position[axis],
                        p.text_position[axis].clamp(6.0, extent - request.text_size[axis] - 6.0)
                    );
                }
                for other in first.iter().take(i).flatten() {
                    assert!(separated(p.rect, other.rect));
                }
                for reserved in requests.iter().filter(|r| r.connect) {
                    let area = earth_math::ScreenRect {
                        min: reserved.anchor.map(|v| v - 4.0),
                        max: reserved.anchor.map(|v| v + 4.0),
                    };
                    assert!(separated(p.rect, area));
                }
                if request.connect {
                    let line = p.leader.unwrap();
                    assert_eq!(line.a, request.anchor);
                    assert_eq!(
                        line.b,
                        [
                            request.anchor[0].clamp(p.rect.min[0], p.rect.max[0]),
                            request.anchor[1].clamp(p.rect.min[1], p.rect.max[1])
                        ]
                    );
                } else {
                    assert!(p.leader.is_none());
                }
            }
        }
    }
    let valid = OrbitCaption {
        anchor: [20.0, 20.0],
        preferred: [6.0, 6.0],
        text_size: [10.0, 13.0],
        connect: false,
    };
    let fits = caption_layout(&[valid], [22.0, 25.0]);
    assert_eq!(fits[0].unwrap().text_position, [6.0, 6.0]);
    let crowded = caption_layout(&[valid, valid], [22.0, 25.0]);
    assert!(crowded[0].is_some() && crowded[1].is_none());
    for display in [
        [3.0, 100.0],
        [100.0, 0.0],
        [f32::NAN, 100.0],
        [4_194_305.0, 100.0],
    ] {
        assert!(caption_layout(&[valid], display)[0].is_none());
    }
    for invalid in [
        OrbitCaption {
            text_size: [-1.0, 13.0],
            ..valid
        },
        OrbitCaption {
            anchor: [f32::NAN, 0.0],
            ..valid
        },
        OrbitCaption {
            preferred: [0.0, f32::INFINITY],
            ..valid
        },
    ] {
        assert!(caption_layout(&[invalid], [1920.0, 1080.0])[0].is_none());
    }
    // f32(text_width + 12) rounds down to the viewport boundary here, but
    // the true backing + margins do not fit. It must omit rather than panic.
    let round_boundary = OrbitCaption {
        text_size: [4_194_292.0 + 0.25, 13.0],
        ..valid
    };
    assert!(caption_layout(&[round_boundary], [4_194_304.0, 100.0])[0].is_none());
}
