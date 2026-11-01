//! Repeatable CPU wall-time probe. The hash covers every GPU attribute/index bit;
//! it checks that an implementation change did not alter the finite mesh.
use earth_math::{Geodetic, Observer, TerrainScene, rendering::*};
use std::time::Instant;

fn mesh_hash(mesh: &RenderMesh) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for word in mesh
        .positions
        .iter()
        .flatten()
        .map(|v| v.to_bits())
        .chain(mesh.normals.iter().flatten().map(|v| v.to_bits()))
        .chain(mesh.colors.iter().flatten().map(|v| v.to_bits()))
        .chain(mesh.indices.iter().copied())
    {
        for byte in word.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn main() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    for (name, height, focal) in [
        ("ground-normal", 0.0, focal_pixels(60.0, 1080)),
        ("ground-telescope", 0.0, 4_000.0),
        ("space-normal", 2_000_000.0, focal_pixels(60.0, 1080)),
        ("space-telescope", 2_000_000.0, 4_000.0),
    ] {
        let observer = Observer::new(Geodetic::new(29.305549, -94.767015, height).unwrap());
        let plan = continuous::DisplayPlan::at(observer, false);
        for repetition in 0..3 {
            let started = Instant::now();
            let mesh =
                boundary_scene_meshes_with_detail(&scene, plan.local, focal, plan.detail).reference;
            let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{name} repeat={repetition} ms={milliseconds:.6} vertices={} triangles={} hash={:016x}",
                mesh.positions.len(),
                mesh.indices.len() / 3,
                mesh_hash(&mesh)
            );
            std::hint::black_box(mesh);
        }
    }
}
