//! Lossless source categories, independent of finite presentation geometry.
//! Integer RLE indices avoid f32 address limits and padded raster allocations.
use bevy::{prelude::*, render::render_resource::ShaderType};
use earth_math::{Geodetic, Observer, TerrainScene};

pub(super) const TILE_COLUMNS: usize = 360;
pub(super) const TILE_ROWS: usize = 180;
pub(super) const VECTOR_TAG: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub(super) struct Settings {
    /// Integer latitude arcseconds/fraction; integer longitude half-arcseconds/fraction.
    pub native_origin: Vec4,
    /// Integer/fractional global raster row, followed by column.
    pub global_origin: Vec4,
    pub control: UVec4,
}
impl Settings {
    pub fn at(scene: &TerrainScene, eye: Geodetic, enabled: bool) -> Self {
        let split = |value: f64| (value.floor() as f32, (value - value.floor()) as f32);
        let (lat, lat_fraction) = split(eye.lat_deg * 3600.0);
        let (lon, lon_fraction) = split(eye.lon_deg * 7200.0);
        let global_origin = scene.global_land_mask().map_or(Vec4::ZERO, |mask| {
            let (row, row_fraction) = if mask.vector().is_some() {
                split(eye.lat_deg + 90.0)
            } else {
                split((90.0 - eye.lat_deg) * mask.height as f64 / 180.0)
            };
            let (column, column_fraction) =
                split((eye.lon_deg + 180.0) * mask.width as f64 / 360.0);
            Vec4::new(row, row_fraction, column, column_fraction)
        });
        Self {
            native_origin: Vec4::new(lat, lat_fraction, lon, lon_fraction),
            global_origin,
            control: UVec4::new(u32::from(enabled), 0, 0, 0),
        }
    }
}

pub(super) struct Packed {
    pub native: Vec<u32>,
    pub global: Vec<u32>,
}
impl Packed {
    pub fn empty() -> Self {
        Self {
            native: vec![0; TILE_COLUMNS * TILE_ROWS],
            global: vec![0; 3],
        }
    }
    pub fn from_scene(scene: &TerrainScene) -> Self {
        let mut packed = Self::empty();
        if let Some(mask) = scene.water_masks() {
            for tile in mask.tiles() {
                let header = packed.native.len();
                let run_base = header + 1 + tile.row_offsets().len();
                let index =
                    (tile.south_deg + 90) as usize * TILE_COLUMNS + (tile.west_deg + 180) as usize;
                packed.native[index] =
                    u32::try_from(header).expect("bounded native mask allocation");
                packed.native.push(tile.width as u32);
                packed.native.extend(
                    tile.row_offsets()
                        .iter()
                        .map(|offset| run_base as u32 + offset),
                );
                packed.native.extend_from_slice(tile.runs());
            }
        }
        if let Some(mask) = scene.global_land_mask() {
            if let Some(vector) = mask.vector() {
                let edge_base = 3 + vector.nodes().len() * 8;
                packed.global = Vec::with_capacity(edge_base + vector.edges().len() * 5);
                packed
                    .global
                    .extend([VECTOR_TAG, vector.nodes().len() as u32, edge_base as u32]);
                for node in vector.nodes() {
                    packed.global.extend([
                        node.children,
                        edge_base as u32 + node.edge_start * 5,
                        node.edge_count,
                        node.winding as u32,
                        node.hole_winding as u32,
                        (node.seed[0] as f32).to_bits(),
                        (node.seed[1] as f32).to_bits(),
                        0,
                    ]);
                }
                for edge in vector.edges() {
                    packed.global.extend([
                        (edge.a[0] as f32).to_bits(),
                        (edge.a[1] as f32).to_bits(),
                        (edge.b[0] as f32).to_bits(),
                        (edge.b[1] as f32).to_bits(),
                        u32::from(edge.hole),
                    ]);
                }
            } else {
                // Retained only for archived raster-format fixtures.
                let run_base = 2 + mask.row_offsets().len();
                packed.global = vec![mask.width as u32, mask.height as u32];
                packed.global.extend(
                    mask.row_offsets()
                        .iter()
                        .map(|offset| run_base as u32 + offset),
                );
                packed.global.extend_from_slice(mask.runs());
            }
        }
        packed
    }
}

/// Reproduce the user's reported nadir inspection. The HUD eye height is
/// rounded to 0.001 international feet; this is a QA pose, not a new measurement.
pub fn inspection_observer(center: Geodetic) -> Observer {
    let mut observer = Observer::new(
        Geodetic::new(center.lat_deg, center.lon_deg, 0.0).expect("validated center"),
    );
    observer.feet.height_m = 173_436.598 * 0.3048 - observer.eye_height_m;
    observer.heading_deg = 268.80;
    observer.pitch_deg = -90.0;
    observer
}

/// Supplied coordinates, smooth h=0 and the normal default heading/pitch.
pub fn rockies_cost_observer() -> Observer {
    Observer::new(Geodetic::new(38.829149, -106.415415, 0.0).expect("supplied valid coordinates"))
}

#[cfg(test)]
mod review;
