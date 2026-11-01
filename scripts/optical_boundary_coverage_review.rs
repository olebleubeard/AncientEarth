//! Independent finite fixtures for the unused boundary-degree certificate.
#[allow(dead_code)]
#[path = "optical_boundary_coverage.rs"]
mod implementation;

use earth_math::Vec3d;
use implementation::BoundaryChain;

#[test]
fn supplied_patch_frame_and_gpu_axis_permutation_preserve_plane_certificates() {
    let mut certified = 0;
    for (yaw, tilt) in [(0_f64, 0_f64), (0.7, -0.4), (-2.1, 1.1)] {
        let (sy, cy) = yaw.sin_cos();
        let (st, ct) = tilt.sin_cos();
        let east = Vec3d::new(cy * ct, sy * ct, -st);
        let north = Vec3d::new(-sy, cy, 0.);
        let up = Vec3d::new(cy * st, sy * st, ct);
        let anchor = Vec3d::new(6e6, -2e6, 3e6);
        let world = |x, y, z| anchor + east * x - north * y + up * z;
        let vertices = [(-100., -60.), (100., -60.), (100., 60.), (-100., 60.)]
            .map(|(x, y)| world(x, y, -27.));
        let chain = BoundaryChain::in_frame(
            &vertices,
            &[[0, 1, 2], [0, 2, 3]],
            anchor,
            [east, -north, up],
        )
        .unwrap();
        let core = chain.interior_core().unwrap();
        for x in [-120., -70., 0., 70., 120.] {
            for y in [-80., -35., 0., 35., 80.] {
                for vertical in [0.9, 1., 1.1] {
                    let point = world(x, y, 10.);
                    let direction = east * 0.01 - north * 0.02 + up * vertical;
                    let actual_t = -37. / vertical;
                    let actual_x = x + actual_t * 0.01;
                    let actual_y = y + actual_t * 0.02;
                    let local = point - anchor;
                    let gpu_point = Vec3d::new(local.dot(east), local.dot(up), -local.dot(north));
                    let gpu_direction = Vec3d::new(
                        direction.dot(east),
                        direction.dot(up),
                        -direction.dot(north),
                    );
                    let gpu_center = Vec3d::new(core.center.x, core.center.z, core.center.y);
                    // Independent real-arithmetic cheap sufficient predicate:
                    // L1 bounds the direction difference and rotation reach.
                    let l1 = |v: Vec3d| v.x.abs() + v.y.abs() + v.z.abs();
                    let e = l1(gpu_direction - Vec3d::new(0., 1., 0.)) + 1e-7;
                    let erosion = 2. * e / (1. - e)
                        * (core.boundary_radius + l1(gpu_point - gpu_center))
                        + 0.002;
                    let sufficient = e < 1.
                        && (gpu_point.x - gpu_center.x).abs() + erosion < core.half_extent[0]
                        && (gpu_point.z - gpu_center.z).abs() + erosion < core.half_extent[1];
                    if sufficient {
                        certified += 1;
                        assert!(actual_x.abs() < 100. && actual_y.abs() < 60.);
                        assert!(
                            chain
                                .certify_core(
                                    core,
                                    point,
                                    direction,
                                    0.001,
                                    1e-7,
                                    0.001,
                                    [-100., 100.]
                                )
                                .is_some()
                        );
                    }
                    if let Some(answer) = chain.certify_core(
                        core,
                        point,
                        direction,
                        0.001,
                        1e-7,
                        0.001,
                        [-100., 100.],
                    ) {
                        assert!(actual_x.abs() < 100. && actual_y.abs() < 60.);
                        assert!(answer.line_parameter_interval_m[0] <= actual_t);
                        assert!(answer.line_parameter_interval_m[1] >= actual_t);
                    }
                }
            }
        }
        assert!(
            BoundaryChain::in_frame(&vertices, &[[0, 1, 2]], anchor, [east * 1.001, -north, up])
                .is_none()
        );
        assert!(
            BoundaryChain::in_frame(&vertices, &[[0, 1, 2]], anchor, [east, east, up]).is_none()
        );
    }
    assert!(certified > 15);
}

#[test]
fn constant_work_core_retains_holes_and_only_certifies_actual_plane_intersections() {
    let altitude = 6_300_000.;
    let points = [(-100., -60.), (100., -60.), (100., 60.), (-100., 60.)]
        .map(|(x, y)| Vec3d::new(x, y, altitude));
    let chain = BoundaryChain::build(&points, &[[0, 1, 2], [0, 2, 3]]).unwrap();
    let core = chain.interior_core().unwrap();
    let mut certificates = 0;
    for height in [-20., 0., 20.] {
        for x in [-130., -100., -70., 0., 70., 100., 130.] {
            for y in [-80., -60., -35., 0., 35., 60., 80.] {
                for direction in [
                    Vec3d::new(0., 0., 1.),
                    Vec3d::new(0.02, -0.03, 2.),
                    Vec3d::new(-0.1, 0.2, 1.),
                ] {
                    let p = Vec3d::new(x, y, altitude + height);
                    let answer =
                        chain.certify_core(core, p, direction, 0.001, 1e-7, 0.001, [-100., 100.]);
                    // Independent Cartesian plane solution; neither winding
                    // nor the core's reference projection supplies this oracle.
                    let t = -height / direction.z;
                    let hit = p + direction * t;
                    if let Some(answer) = answer {
                        certificates += 1;
                        assert!(hit.x.abs() < 100. && hit.y.abs() < 60.);
                        assert!(answer.line_parameter_interval_m[0] <= t);
                        assert!(answer.line_parameter_interval_m[1] >= t);
                        assert!(
                            chain
                                .certify_core(core, p, direction, 0., 0., 0., [t + 1., t + 2.])
                                .is_none()
                        );
                    }
                }
            }
        }
    }
    assert!(
        certificates >= 50,
        "fixture must exercise positive certificates"
    );

    // A hole away from the bounding-box center limits the sufficient rectangle.
    // It must not be filled by the O(1) certificate even though the center is in
    // the surface and therefore can receive a nonzero-degree core.
    let ring = [
        (-10., -10.),
        (10., -10.),
        (10., 10.),
        (-10., 10.),
        (2., -1.),
        (4., -1.),
        (4., 1.),
        (2., 1.),
    ]
    .map(|(x, y)| Vec3d::new(x, y, altitude));
    let mut faces = Vec::new();
    for i in 0..4u32 {
        let next = (i + 1) % 4;
        faces.extend([[i, next, 4 + next], [i, 4 + next, 4 + i]]);
    }
    let ring = BoundaryChain::build(&ring, &faces).unwrap();
    let core = ring.interior_core().unwrap();
    let query = |x, y| {
        ring.certify_core(
            core,
            Vec3d::new(x, y, altitude + 2.),
            Vec3d::new(0., 0., 1.),
            0.001,
            1e-8,
            0.001,
            [-10., 10.],
        )
    };
    assert!(query(0., 0.).is_some());
    for (x, y) in [(2., 0.), (3., 0.), (4., 0.), (10., 0.), (11., 0.)] {
        assert!(query(x, y).is_none(), "hole/boundary/exterior must defer");
    }
}

#[test]
fn rectangle_plane_oracle_covers_internal_seams_after_translation_and_ray_scaling() {
    for anchor in [
        Vec3d::ZERO,
        Vec3d::new(6e6, -2e6, 4e6),
        Vec3d::new(-1e9, 1e9, 3e8),
    ] {
        let vertices = [(-10., -10.), (10., -10.), (10., 10.), (-10., 10.)]
            .map(|(x, y)| anchor + Vec3d::new(x, y, -27.));
        let chain = BoundaryChain::build(&vertices, &[[0, 1, 2], [0, 2, 3]]).unwrap();
        for v in [
            Vec3d::new(0., 0., 1.),
            Vec3d::new(0.2, -0.1, 2.),
            Vec3d::new(-0.3, 0.2, -3.),
        ] {
            let exact_t = -27. / v.z;
            for x in [-12., -10., -9., 0., 9., 10., 12.] {
                for y in [-12., -10., -9., 0., 9., 10., 12.] {
                    // Construct the launch by the independent plane solution:
                    // p+t*v has exactly the prescribed x/y on z=-27.
                    let point = anchor + Vec3d::new(x - exact_t * v.x, y - exact_t * v.y, 0.);
                    let result = chain.certify(point, v, 0.001, 1e-7, 0.002, [-100., 100.]);
                    let strict_inside = x.abs() < 10. && y.abs() < 10.;
                    assert_eq!(
                        result.is_some(),
                        strict_inside,
                        "anchor={anchor:?} v={v:?} hit=({x},{y})"
                    );
                    if let Some(result) = result {
                        assert_eq!(result.degree, if v.z > 0. { 1 } else { -1 });
                        assert!(result.line_parameter_interval_m[0] <= exact_t);
                        assert!(result.line_parameter_interval_m[1] >= exact_t);
                        assert!(
                            chain
                                .certify(point, v, 0.001, 1e-7, 0.002, [exact_t + 1., exact_t + 2.])
                                .is_none()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn annular_topology_preserves_the_hole_and_signed_chain_multiplicity() {
    let vertices = [
        (-4., -4.),
        (4., -4.),
        (4., 4.),
        (-4., 4.),
        (-1., -1.),
        (1., -1.),
        (1., 1.),
        (-1., 1.),
    ]
    .map(|(x, y)| Vec3d::new(x, y, -3.));
    let mut triangles = Vec::new();
    for i in 0..4u32 {
        let next = (i + 1) % 4;
        triangles.extend([[i, next, 4 + next], [i, 4 + next, 4 + i]]);
    }
    for multiplicity in [1, 2, -1] {
        let mut faces = triangles.clone();
        if multiplicity == 2 {
            faces.extend(triangles.iter().copied());
        }
        if multiplicity == -1 {
            for t in &mut faces {
                t.swap(0, 2);
            }
        }
        let chain = BoundaryChain::build(&vertices, &faces).unwrap();
        let query = |x, y| {
            chain.certify(
                Vec3d::new(x, y, 0.),
                Vec3d::new(0., 0., 1.),
                1e-4,
                1e-8,
                1e-4,
                [-10., 10.],
            )
        };
        assert_eq!(query(2., 0.).unwrap().degree, multiplicity);
        assert_eq!(query(0., -2.).unwrap().degree, multiplicity);
        for (x, y) in [(0., 0.), (0.9, 0.), (1., 0.), (4., 0.), (5., 0.)] {
            assert!(
                query(x, y).is_none(),
                "hole/exterior/boundary must retain fallback"
            );
        }
    }
}

#[test]
fn folded_projection_degree_matches_independent_signed_triangle_preimages() {
    let vertices = [
        Vec3d::new(-2., -2., 0.),
        Vec3d::new(2., -2., 0.),
        Vec3d::new(2., 2., 0.),
        Vec3d::new(-2., 2., 0.),
        Vec3d::new(5., 0., 3.),
    ];
    let faces = [[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
    let point = Vec3d::new(0.3, 0.2, 10.);
    let cross2 = |a: Vec3d, b: Vec3d| a.x * b.y - a.y * b.x;
    let mut degree = 0;
    let mut forward_faces = 0;
    let mut backward_faces = 0;
    for face in faces {
        let [a, b, c] = face.map(|i| vertices[i as usize]);
        let determinant = cross2(b - a, c - a);
        forward_faces += usize::from(determinant > 0.);
        backward_faces += usize::from(determinant < 0.);
        let s = cross2(point - a, c - a) / determinant;
        let t = cross2(b - a, point - a) / determinant;
        if s > 0. && t > 0. && s + t < 1. {
            degree += if determinant > 0. { 1 } else { -1 };
        }
    }
    assert!(forward_faces > 0 && backward_faces > 0);
    assert_eq!(degree, 1);
    let chain = BoundaryChain::build(&vertices, &faces).unwrap();
    let result = chain
        .certify(
            point,
            Vec3d::new(0., 0., 1.),
            0.001,
            1e-6,
            0.001,
            [-100., 100.],
        )
        .unwrap();
    assert_eq!(result.degree, degree);
    // The folded flap also has physical preimages outside the outer boundary:
    // their opposite signs cancel. Zero degree must defer, not assert no hit.
    let flap_point = Vec3d::new(3., 0.2, 10.);
    let mut flap_hits = 0;
    let mut flap_degree = 0;
    for face in faces {
        let [a, b, c] = face.map(|i| vertices[i as usize]);
        let determinant = cross2(b - a, c - a);
        let s = cross2(flap_point - a, c - a) / determinant;
        let t = cross2(b - a, flap_point - a) / determinant;
        if s > 0. && t > 0. && s + t < 1. {
            flap_hits += 1;
            flap_degree += if determinant > 0. { 1 } else { -1 };
        }
    }
    assert_eq!(flap_hits, 2);
    assert_eq!(flap_degree, 0);
    assert!(
        chain
            .certify(
                flap_point,
                Vec3d::new(0., 0., 1.),
                0.001,
                1e-6,
                0.001,
                [-100., 100.]
            )
            .is_none()
    );
    assert!(
        chain
            .certify(
                point,
                Vec3d::new(1., 0., 1e-12),
                0.001,
                1e-6,
                0.001,
                [-100., 100.]
            )
            .is_none()
    );
    assert!(
        chain
            .certify(point, Vec3d::new(0., 0., 1.), 0., 1., 0., [-100., 100.])
            .is_none()
    );
    assert!(
        chain
            .certify(
                point,
                Vec3d::new(0., 0., 1.),
                f64::NAN,
                0.,
                0.,
                [-100., 100.]
            )
            .is_none()
    );
}
