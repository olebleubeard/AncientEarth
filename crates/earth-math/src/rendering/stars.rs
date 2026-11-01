//! Original fixed stars and conditional reference-turbulence scintillation.
//! Directions are deterministic, not an observed stellar catalogue. The
//! turbulence column and temporal spectrum are assumed, not local measurements.
use crate::Vec3d;
pub mod pixel;
pub mod scintillation;
use std::sync::OnceLock;

pub const STAR_COUNT: usize = 3_000;
pub const STAR_GRID: usize = 32;

#[derive(Clone, Copy, Debug)]
pub struct Star {
    pub direction: Vec3d,
    pub color: [f32; 3],
    /// Original illustrative intrinsic brightness, independent of pixel footprint.
    pub relative_brightness: f64,
}

pub fn catalogue() -> &'static [Star] {
    static STARS: OnceLock<Vec<Star>> = OnceLock::new();
    STARS.get_or_init(|| {
        let mut random = 0x5747_5338_3445_4152_u64;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            (random >> 11) as f64 / ((1_u64 << 53) as f64)
        };
        (0..STAR_COUNT)
            .map(|_| {
                let z = 2.0 * next() - 1.0;
                let longitude = next() * std::f64::consts::TAU;
                let radius = (1.0 - z * z).sqrt();
                Star {
                    direction: Vec3d::new(radius * longitude.cos(), radius * longitude.sin(), z),
                    relative_brightness: 0.8 + 0.3 * next(),
                    color: [
                        0.78 + 0.22 * next() as f32,
                        0.82 + 0.18 * next() as f32,
                        1.0,
                    ],
                }
            })
            .collect()
    })
}

pub fn phase_hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}
pub fn star_phase(id: u32, mode: u32) -> f64 {
    f64::from(phase_hash(id.wrapping_add(mode.wrapping_mul(0x9e37_79b9))) >> 8) / 16_777_216.0
        * std::f64::consts::TAU
}
/// Cube planes share this orientation in CPU packing and the optical shader.
/// Returns transverse coordinates and positive/negative face depth.
pub fn face_coordinates(direction: Vec3d, face: usize) -> [f64; 3] {
    match face {
        0 => [direction.y, direction.z, direction.x],
        1 => [direction.y, direction.z, -direction.x],
        2 => [direction.x, direction.z, direction.y],
        3 => [direction.x, direction.z, -direction.y],
        4 => [direction.x, direction.y, direction.z],
        _ => [direction.x, direction.y, -direction.z],
    }
}

/// Bin headers [offset,count,0,0], followed by pairs [direction,relative_brightness]
/// and [linear_RGB,stable_id]. Each star belongs to exactly one cube face.
pub fn packed_catalogue() -> Vec<[f32; 4]> {
    let mut bins = vec![Vec::new(); 6 * STAR_GRID * STAR_GRID];
    for (id, star) in catalogue().iter().enumerate() {
        let face = (0..6)
            .max_by(|a, b| {
                face_coordinates(star.direction, *a)[2]
                    .total_cmp(&face_coordinates(star.direction, *b)[2])
            })
            .unwrap();
        let [x, y, z] = face_coordinates(star.direction, face);
        let cell =
            |v: f64| (((v / z + 1.0) * 0.5 * STAR_GRID as f64).floor() as usize).min(STAR_GRID - 1);
        bins[face * STAR_GRID * STAR_GRID + cell(y) * STAR_GRID + cell(x)].push(id);
    }
    let mut data = vec![[0.0; 4]; bins.len()];
    for (index, ids) in bins.into_iter().enumerate() {
        data[index] = [data.len() as f32, ids.len() as f32, 0.0, 0.0];
        for id in ids {
            let star = catalogue()[id];
            data.push([
                star.direction.x as f32,
                star.direction.y as f32,
                star.direction.z as f32,
                star.relative_brightness as f32,
            ]);
            data.push([star.color[0], star.color[1], star.color[2], id as f32]);
        }
    }
    data
}

/// Reduced, unaveraged midpoint phases for the reference-turbulence display.
/// Exposure averaging of the four-factor process is performed jointly.
pub fn display_phases(seconds: f64, exposure_seconds: f64) -> [[f32; 4]; 2] {
    let phase = scintillation::frame_midpoint_phases(seconds, exposure_seconds).unwrap_or([0.0; 4]);
    [phase.map(|x| x.sin() as f32), phase.map(|x| x.cos() as f32)]
}
