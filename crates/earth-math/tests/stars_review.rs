//! Independent invariants for the original illustrative star field.
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile},
    rendering::stars::*,
    *,
};
use std::collections::BTreeSet;

fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b} (tolerance {tolerance})"
    );
}

#[test]
fn catalog_and_packed_bins_preserve_each_stable_identity_once() {
    let data = packed_catalogue();
    assert_eq!(catalogue().len(), 3000);
    assert_eq!(data.len(), 6 * STAR_GRID * STAR_GRID + 2 * STAR_COUNT);
    let mut ids = BTreeSet::new();
    for header in &data[..6 * STAR_GRID * STAR_GRID] {
        let start = header[0] as usize;
        let count = header[1] as usize;
        assert!(start >= 6 * STAR_GRID * STAR_GRID);
        for pair in data[start..start + count * 2].chunks_exact(2) {
            let id = pair[1][3] as usize;
            assert!(id < STAR_COUNT && ids.insert(id));
            let star = catalogue()[id];
            close(star.direction.length(), 1.0, 3.0 * f64::EPSILON);
            assert!((0.8..=1.1).contains(&star.relative_brightness));
            assert!(star.color.into_iter().all(|v| (0.0..=1.0).contains(&v)));
            let quantized = Vec3d::new(pair[0][0].into(), pair[0][1].into(), pair[0][2].into());
            assert!(quantized.distance(star.direction) < 5.2e-8);
            assert_eq!(&pair[1][..3], &star.color);
        }
    }
    assert_eq!(ids.len(), STAR_COUNT);
    assert_eq!(data, packed_catalogue());
}

// The shader's f32 spatial query, independent of the CPU catalogue packer.
fn candidates(data: &[[f32; 4]], direction: Vec3d) -> BTreeSet<usize> {
    let [x, y, z] = [direction.x as f32, direction.y as f32, direction.z as f32];
    let faces = [
        [y, z, x],
        [y, z, -x],
        [x, z, y],
        [x, z, -y],
        [x, y, z],
        [x, y, -z],
    ];
    let mut ids = BTreeSet::new();
    for (face, [u, v, depth]) in faces.into_iter().enumerate() {
        if depth < 0.5 {
            continue;
        }
        let uv = [u / depth, v / depth];
        if uv.into_iter().any(|q| q.abs() > 1.06) {
            continue;
        }
        let cell = uv.map(|q| (((q + 1.0) * 16.0).floor() as i32).clamp(0, 31));
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (ix, iy) = (cell[0] + dx, cell[1] + dy);
                if !(0..32).contains(&ix) || !(0..32).contains(&iy) {
                    continue;
                }
                let header = data[face * 1024 + iy as usize * 32 + ix as usize];
                for index in 0..header[1] as usize {
                    let id = data[header[0] as usize + 2 * index + 1][3] as usize;
                    assert!(ids.insert(id), "star duplicated across queried bins: {id}");
                }
            }
        }
    }
    ids
}

#[test]
fn bounded_cube_queries_cover_direction_neighborhoods_and_face_seams() {
    let data = packed_catalogue();
    // A square four-sigma support has a sqrt(2) larger corner radius than
    // its axial radius. Prove the query margin using the worst permitted sigma.
    let delta = (4.0_f64 * 2.0_f64.sqrt() * 0.003).atan();
    let star_depth = 1.0 / 3.0_f64.sqrt();
    let ray_depth = star_depth * delta.cos() - (2.0_f64 / 3.0).sqrt() * delta.sin();
    let coordinate_change = delta.sin() / (star_depth * ray_depth);
    assert!(ray_depth > 0.5 && coordinate_change < 0.0625 && coordinate_change < 0.06);
    // Exercise every actual packed star at centre and all support corners.
    for (id, star) in catalogue().iter().enumerate() {
        let n = Vec3d::new(
            star.direction.x as f32 as f64,
            star.direction.y as f32 as f64,
            star.direction.z as f32 as f64,
        )
        .normalized()
        .unwrap();
        let axis = if n.z.abs() > 0.9 {
            Vec3d::new(1.0, 0.0, 0.0)
        } else {
            Vec3d::new(0.0, 0.0, 1.0)
        };
        let right = n.cross(axis).normalized().unwrap();
        let up = right.cross(n).normalized().unwrap();
        for (u, v) in [
            (0.0, 0.0),
            (-3.999, -3.999),
            (-3.999, 3.999),
            (3.999, -3.999),
            (3.999, 3.999),
        ] {
            let ray = (n + right * (0.003 * u) + up * (0.003 * v))
                .normalized()
                .unwrap();
            assert!(
                candidates(&data, ray).contains(&id),
                "missing support star {id}"
            );
        }
    }
    // An independent all-star angular superset check at cube edges/corners.
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for z in [-1.0, 0.0, 1.0] {
                let Some(ray) = Vec3d::new(x, y, z).normalized() else {
                    continue;
                };
                let selected = candidates(&data, ray);
                for (id, star) in catalogue().iter().enumerate() {
                    if ray.dot(star.direction) > delta.cos() {
                        assert!(selected.contains(&id));
                    }
                }
            }
        }
    }
}

#[test]
fn pixel_owner_is_single_physical_cell_across_lenses_roll_and_hidpi_sizes() {
    for size in [[1920, 1080], [2880, 1620], [900, 1600], [1, 1]] {
        for roll in [0.0, 37.0, 90.0] {
            let mut observer = Observer::new(Geodetic::new(43.0, 179.0, 10.0).unwrap());
            observer.roll_deg = roll;
            observer.pitch_deg = 23.0;
            let frame = observer.frame();
            for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
                for fov in [0.2, 60.0, 140.0] {
                    let lens = Lens::new(projection, fov);
                    let half = fov.to_radians() * 0.5;
                    for (sx, sy) in [(0.5, 0.5), (0.4, 0.6), (0.6, 0.4)] {
                        let x = f64::from(size[0]) * sx;
                        let y = f64::from(size[1]) * sy;
                        let u = (x - f64::from(size[0]) * 0.5) * 2.0 / f64::from(size[1]);
                        let v = (f64::from(size[1]) * 0.5 - y) * 2.0 / f64::from(size[1]);
                        if projection == Projection::EquidistantFisheye && u.hypot(v) > 1.0 {
                            continue;
                        }
                        let direction = if projection == Projection::Rectilinear {
                            frame.forward
                                + frame.right * (u * half.tan())
                                + frame.up * (v * half.tan())
                        } else {
                            let radius = u.hypot(v);
                            let theta = radius * half;
                            frame.forward * theta.cos()
                                + (frame.right * u + frame.up * v)
                                    * if radius == 0.0 {
                                        half
                                    } else {
                                        theta.sin() / radius
                                    }
                        };
                        // Avoid a floating-point integer-boundary tie in this
                        // generic oracle; exact axis tie is tested separately.
                        let owner = pixel::owner(lens, frame, direction, size).unwrap();
                        assert!((f64::from(owner[0]) + 0.5 - x).abs() <= 0.500001);
                        assert!((f64::from(owner[1]) + 0.5 - y).abs() <= 0.500001);
                    }
                    assert!(pixel::owner(lens, frame, -frame.forward, size).is_none());
                }
            }
        }
    }
    let frame = CameraFrame {
        origin: Vec3d::ZERO,
        forward: Vec3d::new(0.0, 0.0, 1.0),
        right: Vec3d::new(1.0, 0.0, 0.0),
        up: Vec3d::new(0.0, 1.0, 0.0),
    };
    assert_eq!(
        pixel::owner(Lens::default(), frame, frame.forward, [1920, 1080]),
        Some([960, 540])
    );
    assert!(pixel::owner(Lens::default(), frame, frame.forward, [0, 1080]).is_none());
}

#[test]
fn optical_inverse_has_constant_index_identity_and_verified_exit_residual() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    let observer = Observer::new(Geodetic::new(29.25, -94.75, 6.0).unwrap());
    let basis = EnuBasis::at(observer.eye_geodetic());
    for slope in [0.0, 0.00028] {
        let profile = AtmosphereProfile::from_layers(vec![
            AtmosphereLayer {
                height_m: 0.0,
                refractivity: slope,
            },
            AtmosphereLayer {
                height_m: 120000.0,
                refractivity: 0.0,
            },
        ])
        .unwrap();
        for elevation in [3.0_f64, 15.0, 60.0, 90.0] {
            let target =
                basis.up * elevation.to_radians().sin() + basis.east * elevation.to_radians().cos();
            let launch =
                pixel::apparent_direction(&profile, &scene, observer, target, 1e-10).unwrap();
            let trace = profile
                .trace(
                    Ray {
                        origin: observer.eye_position(),
                        direction: launch,
                    },
                    &scene,
                )
                .unwrap();
            assert!(trace.surface.is_none());
            assert!(trace.exit_ray.direction.distance(target) < 1.01e-10);
            if slope == 0.0 {
                assert!(launch.distance(target) < 1e-10);
            }
        }
        assert!(pixel::apparent_direction(&profile, &scene, observer, -basis.up, 1e-10).is_none());
        assert!(pixel::apparent_direction(&profile, &scene, observer, basis.up, 0.0).is_none());
    }
}

#[test]
fn star_identity_and_phase_are_independent_of_camera_and_never_frame_random() {
    let original = catalogue().to_vec();
    for time in [0.0, 0.4, 1e8, -f64::MAX, f64::MAX] {
        let phases = display_phases(time, 0.017);
        assert!(
            phases
                .into_iter()
                .flatten()
                .all(|x| x.is_finite() && x.abs() <= 1.0)
        );
        for (id, star) in catalogue().iter().enumerate() {
            assert_eq!(star.direction, original[id].direction);
            assert_eq!(star.color, original[id].color);
        }
    }
    assert_ne!(star_phase(0, 0), star_phase(1, 0));
    assert_ne!(star_phase(0, 0), star_phase(0, 1));
}
