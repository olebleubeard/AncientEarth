//! Exact spatial draw partitioning; no vertex or physical surface is simplified.
use super::RenderMesh;
use std::collections::HashMap;

/// Upper triangle count per ordinary regional/imported draw chunk.
pub const DRAW_CHUNK_TRIANGLES: usize = 16_384;

/// Outward-rounded bounds for the already quantized display vertices.
pub fn display_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    if positions.is_empty() {
        return ([0.0; 3], [0.0001; 3]);
    }
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for point in positions {
        for axis in 0..3 {
            let value = f64::from(point[axis]);
            assert!(value.is_finite(), "finite generated display position");
            minimum[axis] = minimum[axis].min(value);
            maximum[axis] = maximum[axis].max(value);
        }
    }
    let center = std::array::from_fn(|axis| ((minimum[axis] + maximum[axis]) * 0.5) as f32);
    let half = std::array::from_fn(|axis| {
        let c = f64::from(center[axis]);
        let extent = (maximum[axis] - c).max(c - minimum[axis]);
        (extent as f32).max(0.0001).next_up()
    });
    (center, half)
}

/// Partition a valid generated mesh by median triangle centroid, along the
/// largest centroid span. Every indexed face occurs once, with its original
/// winding and bit-identical vertex attributes. Duplicated shared vertices
/// across draw chunks are copied, never recomputed.
pub fn spatial_draw_chunks(mesh: RenderMesh) -> Vec<RenderMesh> {
    assert_eq!(mesh.indices.len() % 3, 0);
    if mesh.indices.is_empty() {
        return Vec::new();
    }
    let centroids: Vec<[f64; 3]> = mesh
        .indices
        .chunks_exact(3)
        .map(|face| {
            std::array::from_fn(|axis| {
                face.iter()
                    .map(|&i| f64::from(mesh.positions[i as usize][axis]))
                    .sum::<f64>()
                    / 3.0
            })
        })
        .collect();
    let mut faces: Vec<usize> = (0..centroids.len()).collect();
    let mut output = Vec::new();
    partition(&mesh, &centroids, &mut faces, &mut output);
    output
}

fn partition(
    source: &RenderMesh,
    centroids: &[[f64; 3]],
    faces: &mut [usize],
    output: &mut Vec<RenderMesh>,
) {
    if faces.len() > DRAW_CHUNK_TRIANGLES {
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for &face in faces.iter() {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(centroids[face][axis]);
                maximum[axis] = maximum[axis].max(centroids[face][axis]);
            }
        }
        let axis = (0..3)
            .max_by(|&a, &b| (maximum[a] - minimum[a]).total_cmp(&(maximum[b] - minimum[b])))
            .unwrap();
        let middle = faces.len() / 2;
        faces.select_nth_unstable_by(middle, |&a, &b| {
            centroids[a][axis]
                .total_cmp(&centroids[b][axis])
                .then(a.cmp(&b))
        });
        let (left, right) = faces.split_at_mut(middle);
        partition(source, centroids, left, output);
        partition(source, centroids, right, output);
        return;
    }
    let mut chunk = RenderMesh::default();
    let mut remap = HashMap::with_capacity(faces.len() * 2);
    chunk.indices.reserve(faces.len() * 3);
    for &face in faces.iter() {
        for &old in &source.indices[face * 3..face * 3 + 3] {
            let index = *remap.entry(old).or_insert_with(|| {
                let new = chunk.positions.len() as u32;
                chunk.positions.push(source.positions[old as usize]);
                chunk.normals.push(source.normals[old as usize]);
                chunk.colors.push(source.colors[old as usize]);
                new
            });
            chunk.indices.push(index);
        }
    }
    output.push(chunk);
}
