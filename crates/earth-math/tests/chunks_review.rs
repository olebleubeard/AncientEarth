//! Independent invariants for exact draw partitioning, not a rendering benchmark.
use earth_math::rendering::{
    DRAW_CHUNK_TRIANGLES, RenderMesh, display_bounds, spatial_draw_chunks,
};
use std::collections::BTreeMap;

fn face_counts(mesh: &RenderMesh) -> BTreeMap<Vec<u32>, usize> {
    let mut counts = BTreeMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let mut face = Vec::new();
        for &index in triangle {
            let index = index as usize;
            face.extend(mesh.positions[index].map(f32::to_bits));
            face.extend(mesh.normals[index].map(f32::to_bits));
            face.extend(mesh.colors[index].map(f32::to_bits));
        }
        *counts.entry(face).or_default() += 1;
    }
    counts
}

fn fixture() -> RenderMesh {
    let mut mesh = RenderMesh::default();
    // Equal populations of four distant islands force useful spatial splits.
    // Faces deliberately share vertices; alternate winding and duplicate faces
    // ensure that checking positions or triangle count alone would be inadequate.
    for island in 0..4 {
        let base = mesh.positions.len() as u32;
        for post in 0..129 {
            mesh.positions.push([
                (island as f32 - 1.5) * 1_000_000.0 + post as f32,
                (post % 7) as f32 * 0.25,
                if post % 2 == 0 { -0.0 } else { 0.0 },
            ]);
            mesh.normals.push([post as f32 / 129.0, 1.0, -0.0]);
            mesh.colors
                .push([island as f32 / 4.0, 0.3, post as f32 / 129.0, 1.0]);
        }
        for face in 0..DRAW_CHUNK_TRIANGLES {
            let first = (face % 127) as u32;
            let order = if face % 2 == 0 { [0, 1, 2] } else { [2, 1, 0] };
            mesh.indices
                .extend(order.map(|offset| base + first + offset));
        }
    }
    mesh
}

#[test]
fn spatial_chunks_preserve_the_complete_oriented_attribute_multiset() {
    let expected = face_counts(&fixture());
    let chunks = spatial_draw_chunks(fixture());
    assert_eq!(chunks.len(), 4);
    let mut actual = BTreeMap::new();
    for chunk in &chunks {
        assert!(chunk.indices.len() / 3 <= DRAW_CHUNK_TRIANGLES);
        assert_eq!(chunk.positions.len(), chunk.normals.len());
        assert_eq!(chunk.positions.len(), chunk.colors.len());
        let (center, half) = display_bounds(&chunk.positions);
        // A single world-spanning draw would have a 1.5 Mm half-extent.
        assert!(half[0] < 100.0);
        assert!(center[0].abs() > 400_000.0);
        for (face, count) in face_counts(chunk) {
            *actual.entry(face).or_default() += count;
        }
    }
    assert_eq!(actual, expected);
    let again = spatial_draw_chunks(fixture());
    for (a, b) in chunks.iter().zip(again) {
        assert_eq!(a.indices, b.indices);
        assert_eq!(
            a.positions
                .iter()
                .map(|v| v.map(f32::to_bits))
                .collect::<Vec<_>>(),
            b.positions
                .iter()
                .map(|v| v.map(f32::to_bits))
                .collect::<Vec<_>>()
        );
    }
    assert!(spatial_draw_chunks(RenderMesh::default()).is_empty());
}

#[test]
fn rounded_display_bounds_enclose_vertices_and_their_convex_hull() {
    // Unequal neighboring floats make the rounded center asymmetric. Test the
    // final center and half extents independently in exact f32-to-f64 arithmetic.
    for scale in [0.0_f32, 1e-38, 0.003, 1.0, 6_378_137.0, 1e9] {
        for sign in [-1.0_f32, 1.0] {
            let a = sign * scale;
            let points = [
                [a, a.next_up(), -a],
                [a.next_up().next_up().next_up(), -a, a.next_down()],
                [a.next_down(), a.next_up().next_up(), a],
            ];
            let (center, half) = display_bounds(&points);
            for weights in [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.2, 0.3, 0.5],
                [0.5, 0.5, 0.0],
            ] {
                for axis in 0..3 {
                    let value: f64 = (0..3)
                        .map(|i| f64::from(points[i][axis]) * weights[i])
                        .sum();
                    assert!(
                        (value - f64::from(center[axis])).abs() <= f64::from(half[axis]),
                        "scale={scale}, center={center:?}, half={half:?}, value={value}"
                    );
                    assert!(half[axis].is_finite() && half[axis] > 0.0);
                }
            }
        }
    }
    let (center, half) = display_bounds(&[]);
    assert_eq!(center, [0.0; 3]);
    assert!(half.iter().all(|v| v.is_finite() && *v > 0.0));
}
