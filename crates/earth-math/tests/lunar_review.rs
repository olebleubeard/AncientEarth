//! Independent emission-event references, source registration and surface calculus.
use earth_math::{
    astro::{lunar::LunarOrientation, *},
    rendering::lunar,
    *,
};
use std::{
    f64::consts::{FRAC_PI_2, PI},
    sync::OnceLock,
};

#[path = "fixtures/lunar_reference.rs"]
mod reference;

fn ephemeris() -> &'static Ephemeris {
    static SOURCE: OnceLock<Ephemeris> = OnceLock::new();
    SOURCE.get_or_init(|| {
        Ephemeris::from_bytes(include_bytes!("../../../assets/astronomy/de440s.bsp")).unwrap()
    })
}
fn v(a: [f64; 3]) -> Vec3d {
    Vec3d::new(a[0], a[1], a[2])
}
fn close(a: f64, b: f64, e: f64) {
    assert!(
        a.is_finite() && (a - b).abs() <= e,
        "{a:.17e} != {b:.17e}; {e}"
    );
}
fn direction(p: f64, l: f64) -> Vec3d {
    Vec3d::new(p.cos() * l.cos(), p.cos() * l.sin(), p.sin())
}

#[test]
fn lunar_emission_pole_and_plane_match_original_cspice() {
    for &(tt, emission, light_time, pole, normal, radial, tangent, tilt) in reference::CASES {
        let time = AstroInstant::from_tt_seconds_j2000(tt).unwrap();
        let frame = ephemeris().sample(time, None, Vec3d::ZERO).unwrap();
        let q = frame.lunar_orientation;
        let inverse = terrestrial_rotation(time, None).transpose();
        close(q.emission_tdb_seconds_j2000, emission, 1e-6);
        close(frame.moon.light_time_seconds, light_time, 2e-10);
        for (actual, expected) in [
            (q.axis_ecef, pole),
            (q.orbital_normal_ecef, normal),
            (q.orbital_radial_ecef, radial),
            (q.orbital_tangent_ecef, tangent),
        ] {
            close(inverse.transform(actual).distance(v(expected)), 0.0, 2e-11);
        }
        close(q.tilt_deg, tilt, 2e-9);
        close(
            q.orbital_radial_ecef
                .cross(q.orbital_tangent_ecef)
                .distance(q.orbital_normal_ecef),
            0.0,
            3e-15,
        );
        close(
            q.axis_ecef
                .distance(frame.moon.body_to_ecef.transform(Vec3d::new(0.0, 0.0, 1.0))),
            0.0,
            2e-15,
        );
    }
    let identity = Mat3d {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let state = CartesianState {
        position_m: Vec3d::new(3.0, 4.0, 0.0),
        velocity_mps: Vec3d::new(-4.0, 3.0, 2.0),
    };
    let normal = Vec3d::new(8.0, -6.0, 25.0) / 725.0_f64.sqrt();
    let q = LunarOrientation::from_state(123.0, state, identity, identity).unwrap();
    close(q.orbital_normal_ecef.distance(normal), 0.0, 2e-16);
    close(
        q.tilt_deg,
        (25.0 / 725.0_f64.sqrt()).acos().to_degrees(),
        2e-14,
    );
    for invalid in [
        Mat3d {
            rows: [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
        Mat3d {
            rows: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
        Mat3d {
            rows: [[1.0, 0.1, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
    ] {
        assert!(LunarOrientation::from_state(0.0, state, invalid, identity).is_err());
        assert!(LunarOrientation::from_state(0.0, state, identity, invalid).is_err());
    }
    assert!(LunarOrientation::from_state(f64::NAN, state, identity, identity).is_err());
    assert!(
        LunarOrientation::from_state(
            0.0,
            CartesianState {
                velocity_mps: state.position_m,
                ..state
            },
            identity,
            identity
        )
        .is_err()
    );
}

#[test]
fn mapped_coordinates_and_normals_match_cartesian_surface_differentiation() {
    for (n, uv) in [
        (Vec3d::new(1.0, 0.0, 0.0), [0.5, 0.5]),
        (Vec3d::new(0.0, 1.0, 0.0), [0.75, 0.5]),
        (Vec3d::new(-1.0, 0.0, 0.0), [0.0, 0.5]),
        (Vec3d::new(0.0, -1.0, 0.0), [0.25, 0.5]),
    ] {
        assert_eq!(lunar::map_uv(n).unwrap(), uv);
    }
    for row in [0, 1, 1439, 1440, 2878, 2879] {
        for col in [0, 1, 2879, 2880, 5758, 5759] {
            let p = FRAC_PI_2 - (row as f64 + 0.5) * PI / 2880.0;
            let l = -PI + (col as f64 + 0.5) * 2.0 * PI / 5760.0;
            let uv = lunar::map_uv(direction(p, l)).unwrap();
            close(uv[0], (col as f64 + 0.5) / 5760.0, 2e-15);
            close(uv[1], (row as f64 + 0.5) / 2880.0, 3e-14);
        }
    }
    // Differentiate the actual Cartesian surface by symmetric perturbations;
    // this oracle does not assemble the production east/north gradient formula.
    let height =
        |p: f64, l: f64| 8000.0 * (3.0 * l).sin() * p.cos().powi(2) + 1200.0 * (2.0 * p).sin();
    let point = |p: f64, l: f64| direction(p, l) * (MOON_RADIUS_M + height(p, l));
    for p in [-1.4_f64, -0.8, 0.0, 0.7, 1.4] {
        for l in [-PI, -2.0, 0.0, 1.7, PI] {
            let h = height(p, l);
            let hl = 24000.0 * (3.0 * l).cos() * p.cos().powi(2);
            let hp = -16000.0 * (3.0 * l).sin() * p.cos() * p.sin() + 2400.0 * (2.0 * p).cos();
            let step = 1e-5;
            let dl = point(p, l + step) - point(p, l - step);
            let dp = point(p + step, l) - point(p - step, l);
            let expected = dl.cross(dp).normalized().unwrap();
            close(
                lunar::normal_from_height_gradient(p, l, h, hl, hp)
                    .unwrap()
                    .distance(expected),
                0.0,
                4e-10,
            );
        }
    }
    for p in [-FRAC_PI_2, FRAC_PI_2] {
        close(
            lunar::normal_from_height_gradient(p, 1.0, 0.0, 1e6, 1e6)
                .unwrap()
                .distance(direction(p, 1.0)),
            0.0,
            0.0,
        );
    }
    assert!(lunar::map_uv(Vec3d::ZERO).is_none());
    assert!(lunar::normal_from_height_gradient(0.0, 0.0, -MOON_RADIUS_M, 0.0, 0.0).is_none());
    assert!(lunar::normal_from_height_gradient(PI, 0.0, 0.0, 0.0, 0.0).is_none());
    assert!(lunar::normal_from_height_gradient(0.0, 0.0, 0.0, f64::NAN, 0.0).is_none());
}

#[test]
fn mean_sphere_and_disk_function_preserve_the_declared_geometry() {
    let mesh = lunar::mesh();
    assert_eq!(mesh.positions.len(), 385 * 193);
    assert_eq!(mesh.indices.len(), 384 * 192 * 6);
    assert_eq!(mesh.positions, mesh.normals);
    for (&p, &color) in mesh.positions.iter().zip(&mesh.colors) {
        close(v(p.map(f64::from)).length(), 1.0, 5e-8);
        assert_eq!(color, [1.0; 4]);
    }
    for face in mesh.indices.chunks_exact(3) {
        let a = v(mesh.positions[face[0] as usize].map(f64::from));
        let b = v(mesh.positions[face[1] as usize].map(f64::from));
        let c = v(mesh.positions[face[2] as usize].map(f64::from));
        assert!((b - a).cross(c - a).dot(a + b + c) >= -1e-18);
    }
    for a in [1e-10, 0.1, 0.5, 1.0] {
        close(lunar::lommel_seeliger(a, a).unwrap(), 1.0, 0.0);
        for b in [1e-10, 0.1, 0.5, 1.0] {
            let f = lunar::lommel_seeliger(a, b).unwrap();
            assert!((0.0..=2.0).contains(&f));
            close(f * b, lunar::lommel_seeliger(b, a).unwrap() * a, 2e-16);
        }
    }
    assert_eq!(lunar::lommel_seeliger(0.0, 1.0), Some(0.0));
    assert_eq!(lunar::lommel_seeliger(1.0, 0.0), Some(0.0));
    assert!(lunar::lommel_seeliger(f64::NAN, 1.0).is_none());
}

#[test]
fn lunar_annotations_use_actual_camera_aspect_and_preserve_direction_anchors() {
    let mut observer = Observer::new(Geodetic::new(29.3, -94.7, 1e9).unwrap());
    observer.roll_deg = 43.0;
    let time = AstroInstant::from_tt_seconds_j2000(845856123.456).unwrap();
    let frame = ephemeris()
        .sample_for_inspection(time, None, observer.eye_position(), true)
        .unwrap();
    astro::aim::BodyTracking::default()
        .aim_once(
            astro::aim::ViewBody::Moon,
            &mut observer,
            Some(&frame),
            time,
            true,
        )
        .unwrap();
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    for lens in [
        Lens::new(Projection::Rectilinear, 60.0),
        Lens::new(Projection::Rectilinear, 1.0),
        Lens::new(Projection::EquidistantFisheye, 140.0),
    ] {
        for scale in [[1.0, 1.0], [1.5, 1.5], [2.0, 1.25]] {
            let display = [1280.0, 720.0];
            let diagram = lunar::annotations(observer, lens, &frame, &scene, display, scale);
            assert!(
                !diagram.plane.is_empty() && !diagram.axis.is_empty() && !diagram.normal.is_empty()
            );
            close(f64::from(diagram.normal[0].a[0]), 640.0, 1e-3);
            close(f64::from(diagram.normal[0].a[1]), 360.0, 1e-3);
            // The finite stroke is checked through its actual tangent direction,
            // including roll; its launch at the lunar center is unchanged by DPI.
            let line = &diagram.normal[0];
            let view = observer.frame();
            let n = frame.lunar_orientation.orbital_normal_ecef;
            let dx = f64::from(line.b[0] - line.a[0]);
            let dy = f64::from(line.b[1] - line.a[1]);
            assert!(dx * n.dot(view.right) >= -1e-6);
            assert!(dy * n.dot(view.up) <= 1e-6);
            for line in diagram
                .plane
                .iter()
                .chain(&diagram.axis)
                .chain(&diagram.normal)
            {
                for point in [line.a, line.b] {
                    assert!(
                        point[0] >= -0.01
                            && point[0] <= 1280.01
                            && point[1] >= -0.01
                            && point[1] <= 720.01
                    );
                }
            }
            assert_eq!(diagram.axis_exaggerated, lens.vertical_fov_deg > 1.0);
            assert!(diagram.labels.iter().all(|label| label.connect));
            let pole = diagram
                .labels
                .iter()
                .find(|label| label.text.contains("Moon north pole"))
                .unwrap();
            assert!(pole.text.contains("text PCK"));
            assert_eq!(
                pole.text.contains("enlarged direction guide"),
                diagram.axis_exaggerated
            );
        }
    }
    assert!(
        lunar::annotations(
            observer,
            Lens::default(),
            &frame,
            &scene,
            [0.0, 720.0],
            [1.0, 1.0]
        )
        .axis
        .is_empty()
    );
    let mut stale = frame;
    stale.sample_observer_ecef_m.x += 1.0;
    assert!(
        lunar::annotations(
            observer,
            Lens::default(),
            &stale,
            &scene,
            [1280.0, 720.0],
            [1.0, 1.0]
        )
        .axis
        .is_empty()
    );
    let mut occluded = frame;
    occluded.moon.apparent_center_ecef_m = -observer.eye_position();
    astro::aim::BodyTracking::default()
        .aim_once(
            astro::aim::ViewBody::Moon,
            &mut observer,
            Some(&occluded),
            time,
            true,
        )
        .unwrap();
    let hidden = lunar::annotations(
        observer,
        Lens::default(),
        &occluded,
        &scene,
        [1280.0, 720.0],
        [1.0, 1.0],
    );
    assert!(hidden.axis.is_empty() && hidden.plane.is_empty() && hidden.labels.is_empty());
}
