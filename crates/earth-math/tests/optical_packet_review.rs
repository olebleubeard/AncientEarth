//! Independent finite f32 packet checks. These test actual packing and source
//! geometry, not shader execution or a global optical error bound.
use earth_math::{atmosphere::gpu::scene::*, rendering::LocalFrame, survey::SurveyMarker, *};
use std::collections::BTreeMap;

fn coordinates(frame: LocalFrame, p: Vec3d) -> [f64; 3] {
    let lat = ecef_to_geodetic(frame.origin).unwrap().lat_deg.to_radians();
    let lon = ecef_to_geodetic(frame.origin).unwrap().lon_deg.to_radians();
    let d = p - frame.origin;
    [
        -lon.sin() * d.x + lon.cos() * d.y,
        lat.cos() * lon.cos() * d.x + lat.cos() * lon.sin() * d.y + lat.sin() * d.z,
        lat.sin() * lon.cos() * d.x + lat.sin() * lon.sin() * d.y - lat.cos() * d.z,
    ]
}
fn scene() -> TerrainScene {
    TerrainScene::new(vec![
        TerrainPatch::from_grid(
            3,
            3,
            -95.0,
            29.01,
            0.005,
            0.005,
            vec![0., 20., -30., 100., 35., 5., 9., 18., 4.],
        )
        .unwrap(),
        TerrainPatch::from_grid(
            3,
            3,
            138.0,
            35.01,
            0.005,
            0.005,
            vec![
                3500., 3510., 3490., 3550., 3560., 3500., 3540., 3510., 3520.,
            ],
        )
        .unwrap(),
    ])
}
fn vertex_bits(p: [f32; 4]) -> [u32; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

#[test]
fn stackless_local_bvhs_retain_all_oriented_faces_and_outward_bounds() {
    let source = scene();
    let packet = PackedScene::prepare(&source).unwrap();
    assert_eq!(packet.patches.len(), 2);
    assert_eq!(packet.triangle_count, 16);
    for (patch, packed) in source.patches().iter().zip(&packet.patches) {
        let mesh = patch.optical_mesh();
        let mut expected = BTreeMap::new();
        for face in &mesh.triangles {
            let key = face.map(|v| {
                let source = mesh.vertices[v as usize];
                let independent = coordinates(packed.frame, source);
                let delta = source - packed.frame.origin;
                let exact = [
                    delta.dot(packed.frame.basis.east),
                    delta.dot(packed.frame.basis.up),
                    -delta.dot(packed.frame.basis.north),
                ];
                for axis in 0..3 {
                    assert!((independent[axis] - exact[axis]).abs() < 1e-8);
                }
                // Compare exact packed face multiplicity with the stored frame;
                // independent trigonometric axes differ in tiny near-zero bits.
                exact.map(|x| (x as f32).to_bits())
            });
            *expected.entry(key).or_insert(0usize) += 1;
        }
        let mut actual = BTreeMap::new();
        let mut index = packed.root as usize;
        let end = packet.geometry[index][3].to_bits() as usize;
        while index < end {
            let low = packet.geometry[index];
            let high = packet.geometry[index + 1];
            let escape = low[3].to_bits() as usize;
            assert!(escape > index && escape <= end);
            let payload = high[3].to_bits();
            if payload & LEAF_FLAG == 0 {
                index += 2;
                continue;
            }
            let count = ((payload >> 24) & 7) + 1;
            let start = (payload & ADDRESS_MASK) as usize;
            for face in packet.geometry[start..start + count as usize * 3].chunks_exact(3) {
                for vertex in face {
                    for axis in 0..3 {
                        assert!(low[axis] < vertex[axis] && vertex[axis] < high[axis]);
                    }
                }
                let key = [
                    vertex_bits(face[0]),
                    vertex_bits(face[1]),
                    vertex_bits(face[2]),
                ];
                *actual.entry(key).or_insert(0usize) += 1;
            }
            index = escape;
        }
        assert_eq!(actual, expected);
        let mut maximum = 0.0_f64;
        for &point in mesh.vertices {
            let exact = coordinates(packed.frame, point);
            let error = exact
                .iter()
                .map(|&v| (v - f64::from(v as f32)).powi(2))
                .sum::<f64>()
                .sqrt();
            maximum = maximum.max(error);
        }
        // Independent trigonometric axes can differ by a few f64 ulps from
        // the stored ENU basis, well below the finite f32 source error.
        assert!((maximum - packed.source_position_error_m).abs() < 1e-8);
    }
}

#[test]
fn dynamic_transforms_preserve_patch_coordinates_across_distant_eye_frames() {
    let source = scene();
    let packet = PackedScene::prepare(&source).unwrap();
    for (lat, lon, h) in [
        (29.3, -94.7, 5.0),
        (35.0, 138.0, 5000.0),
        (0.0, 179.9, 20000.0),
        (-89.0, -90.0, 10.0),
    ] {
        let eye = LocalFrame::at(Observer::new(Geodetic::new(lat, lon, h).unwrap()));
        let data = packet.frame_data(eye, &[]).unwrap();
        assert_eq!(data[0][1].to_bits(), 2);
        for (i, patch) in source.patches().iter().enumerate() {
            let packed = &packet.patches[i];
            let offset = FRAME_HEADER_WORDS + i * PATCH_WORDS;
            assert_eq!(data[offset][3].to_bits(), packed.root);
            for &point in patch.canonical_vertices() {
                let eye_position = coordinates(eye, point).map(|x| x as f32);
                let actual: [f32; 3] = std::array::from_fn(|row| {
                    let coefficients = data[offset + 1 + row];
                    data[offset][row]
                        + coefficients[0] * eye_position[0]
                        + coefficients[1] * eye_position[1]
                        + coefficients[2] * eye_position[2]
                });
                let expected = coordinates(packed.frame, point);
                let error = actual
                    .iter()
                    .zip(expected)
                    .map(|(&a, b)| (f64::from(a) - b).powi(2))
                    .sum::<f64>()
                    .sqrt();
                let budget = 0.001
                    + 4e-7
                        * (eye.origin.distance(packed.frame.origin) + eye.origin.distance(point));
                assert!(error <= budget, "{error} > {budget}");
            }
        }
    }
}

#[test]
fn marker_packet_preserves_ids_dimensions_and_rejects_unsupported_counts() {
    let source = TerrainScene::new(vec![]);
    let packet = PackedScene::prepare(&source).unwrap();
    let geo = Geodetic::new(29.0, -95.0, 0.0).unwrap();
    let enu = EnuBasis::at(geo);
    let marker = SurveyMarker {
        id: 37,
        base_ecef_m: geodetic_to_ecef(geo),
        right: enu.east,
        forward: enu.north,
        up: enu.up,
        width_m: 3.048,
        depth_m: 3.048,
        height_m: 30.48,
        along_distance_m: 1000.0,
    };
    let eye = LocalFrame::at(Observer::new(Geodetic::new(29.001, -95.0, 10.0).unwrap()));
    let data = packet.frame_data(eye, &[marker]).unwrap();
    assert_eq!(data[0][3].to_bits(), 1);
    let start = data[1][0].to_bits() as usize;
    assert_eq!(data[start][3].to_bits(), 37);
    assert_eq!(data[start + 1][3], (marker.width_m * 0.5) as f32);
    assert_eq!(data[start + 2][3], (marker.depth_m * 0.5) as f32);
    assert_eq!(data[start + 3][3], marker.height_m as f32);
    let expected = coordinates(eye, marker.base_ecef_m);
    for (axis, value) in expected.into_iter().enumerate() {
        assert!((f64::from(data[start][axis]) - value).abs() < 1e-4);
    }
    assert!(
        packet
            .frame_data(eye, &vec![marker; MAX_MARKERS + 1])
            .is_err()
    );
    let mut invalid = marker;
    invalid.up = Vec3d::ZERO;
    assert!(packet.frame_data(eye, &[invalid]).is_err());
}

#[test]
fn triangle_halfspaces_preserve_known_tube_witnesses_and_reject_separated_planes() {
    // Scalar f32 operation order of the production candidate predicate.
    // Known triangle/segment witness points supply an independent oracle;
    // no closest-point or halfspace algorithm generates those expectations.
    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| a[i] - b[i])
    }
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn candidate(p: [f32; 3], q: [f32; 3], v: [[f32; 3]; 3], radius: f32) -> bool {
        let normal = cross(sub(v[1], v[0]), sub(v[2], v[0]));
        let length = dot(normal, normal).sqrt();
        let n = normal.map(|x| x / length);
        let before = dot(n, sub(p, v[0]));
        let after = dot(n, sub(q, v[0]));
        if before * after > 0.0 && before.abs().min(after.abs()) > radius {
            return false;
        }
        for i in 0..3 {
            let edge = sub(v[(i + 1) % 3], v[i]);
            let x = dot(cross(edge, sub(p, v[i])), n);
            let y = dot(cross(edge, sub(q, v[i])), n);
            if x.max(y) < -radius * dot(edge, edge).sqrt() {
                return false;
            }
        }
        true
    }
    for lateral in [0.1_f32, 0.01, 0.000001] {
        for gap in [0.001_f32, 0.000001] {
            let a = [-1000.0, lateral, gap];
            let b = [1000.0, -lateral, gap];
            let v = [
                [-1000.0, 0.0, 0.0],
                [1000.0, 0.0, 0.0],
                [0.0, -2.0 * lateral, 0.0],
            ];
            // Every chord point has z=gap, and its midpoint projects onto
            // the triangle. Thus the exact distance is gap, even when the
            // old near-parallel endpoint substitution reported a larger gap.
            for (p, q) in [(a, b), (b, a)] {
                for vertices in [v, [v[0], v[2], v[1]]] {
                    assert!(candidate(p, q, vertices, gap * 1.01));
                    assert!(!candidate(p, q, vertices, gap * 0.5));
                }
            }
        }
    }
    let triangle = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]];
    // Binary-exact equality remains a candidate at face and edge boundaries.
    for p in [[1.0, 1.0, 0.25], [1.0, -0.25, 0.0], [-0.25, 1.0, 0.0]] {
        assert!(candidate(p, p, triangle, 0.25));
        assert!(!candidate(p, p, triangle, 0.125));
    }
    assert!(!candidate([3.0, 3.0, 0.0], [4.0, 4.0, 0.0], triangle, 0.25));
    assert!(candidate([1.0, 1.0, -2.0], [1.0, 1.0, 2.0], triangle, 0.0));
    let f32_point = |p: Vec3d| [p.x as f32, p.y as f32, p.z as f32];
    for scale in [0.001, 1.0, 1000.0, 1e6] {
        for angle in [0.0_f64, 0.3, 1.7, 2.9] {
            let u = Vec3d::new(angle.cos(), angle.sin(), 0.0);
            let beta = angle * 0.7;
            let v = Vec3d::new(
                -angle.sin() * beta.cos(),
                angle.cos() * beta.cos(),
                beta.sin(),
            );
            let vertices = [Vec3d::ZERO, u * (4.0 * scale), v * (3.0 * scale)];
            let direction = (u * 0.2 + v * 0.3 + u.cross(v)).normalized().unwrap();
            let radius = scale * 0.001;
            let offset = Vec3d::new(0.3, -0.4, 0.5).normalized().unwrap() * (radius * 0.25);
            for (x, y) in [
                (0.0, 0.0),
                (1.0, 0.0),
                (0.0, 1.0),
                (0.5, 0.0),
                (0.5, 0.5),
                (0.2, 0.3),
            ] {
                let on_triangle = vertices[1] * x + vertices[2] * y;
                let witness = on_triangle + offset;
                for t in [0.0, 0.25, 0.5, 1.0] {
                    let p = witness - direction * (20.0 * scale * t);
                    let q = witness + direction * (20.0 * scale * (1.0 - t));
                    // A specified convex combination of the endpoints lies
                    // within radius/4 of a specified triangle point. The
                    // generous remainder covers this finite f32 fixture.
                    assert!(candidate(
                        f32_point(p),
                        f32_point(q),
                        vertices.map(f32_point),
                        radius as f32
                    ));
                }
            }
        }
    }
}
