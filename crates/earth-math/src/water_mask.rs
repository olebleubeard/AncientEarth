//! Independent geographic categories. They never supply physical heights.
//! Native WBM posts are preserved exactly in row run-length encoding.
use crate::{
    Geodetic,
    topography::{CopernicusTile, TopographyError},
};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tiff::{
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};
#[cfg(test)]
mod review;
pub mod vector;

pub const MAX_MASK_RUNS: usize = 16_000_000;
const ROWS: usize = 3600;
fn invalid(message: impl ToString) -> TopographyError {
    TopographyError::InvalidRaster(message.to_string())
}
fn cancelled(cancel: &AtomicBool) -> Result<(), TopographyError> {
    if cancel.load(Ordering::Relaxed) {
        Err(TopographyError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct WaterMaskTile {
    pub south_deg: i32,
    pub west_deg: i32,
    pub width: usize,
    offsets: Vec<u32>,
    runs: Vec<u32>,
}
impl WaterMaskTile {
    pub fn row_offsets(&self) -> &[u32] {
        &self.offsets
    }
    /// Each run packs `(exclusive_end_column << 2) | source_code`.
    pub fn runs(&self) -> &[u32] {
        &self.runs
    }
    pub fn sample(&self, row: usize, column: usize) -> Option<u8> {
        sample(&self.offsets, &self.runs, self.width, ROWS, row, column)
    }
    fn decode(
        path: &Path,
        tile: &CopernicusTile,
        cancel: &AtomicBool,
    ) -> Result<Self, TopographyError> {
        cancelled(cancel)?;
        let file = File::open(path).map_err(invalid)?;
        if file.metadata().map_err(invalid)?.len() > 64 * 1024 * 1024 {
            return Err(invalid("WBM exceeds bounded source size"));
        }
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 16 * 1024 * 1024;
        limits.intermediate_buffer_size = 16 * 1024 * 1024;
        limits.ifd_value_size = 1024 * 1024;
        let mut decoder = Decoder::new(BufReader::new(file))
            .map_err(invalid)?
            .with_limits(limits);
        let (width, height) = decoder.dimensions().map_err(invalid)?;
        if width as usize != crate::topography::copernicus::native_width(tile.south_deg)
            || height != ROWS as u32
        {
            return Err(invalid("WBM dimensions differ from native geographic tile"));
        }
        for (tag, expected) in [
            (Tag::BitsPerSample, 8),
            (Tag::SamplesPerPixel, 1),
            (Tag::SampleFormat, 1),
            (Tag::Compression, 8),
            (Tag::Predictor, 2),
            (Tag::PhotometricInterpretation, 1),
        ] {
            if decoder.get_tag_u32(tag).map_err(invalid)? != expected {
                return Err(invalid(format!("unsupported WBM {tag:?}")));
            }
        }
        for tag in [Tag::Orientation, Tag::PlanarConfiguration] {
            if decoder
                .find_tag_unsigned::<u32>(tag)
                .map_err(invalid)?
                .is_some_and(|v| v != 1)
            {
                return Err(invalid("unsupported WBM orientation/planes"));
            }
        }
        if decoder
            .find_tag(Tag::ModelTransformationTag)
            .map_err(invalid)?
            .is_some()
        {
            return Err(invalid("unsupported transformed WBM"));
        }
        let scale = decoder
            .get_tag_f64_vec(Tag::ModelPixelScaleTag)
            .map_err(invalid)?;
        let tie = decoder
            .get_tag_f64_vec(Tag::ModelTiepointTag)
            .map_err(invalid)?;
        let close = |a: f64, b: f64| a.is_finite() && (a - b).abs() <= 1e-12;
        if scale.len() != 3
            || tie.len() != 6
            || !close(scale[0], 1.0 / f64::from(width))
            || !close(scale[1], 1.0 / 3600.0)
            || !close(scale[2], 0.0)
            || !tie[..3].iter().all(|v| close(*v, 0.0))
            || !close(tie[3], f64::from(tile.west_deg))
            || !close(tie[4], f64::from(tile.south_deg + 1))
            || !close(tie[5], 0.0)
        {
            return Err(invalid("WBM source registration mismatch"));
        }
        let keys = decoder
            .get_tag_u16_vec(Tag::GeoKeyDirectoryTag)
            .map_err(invalid)?;
        if keys.len() < 4 || keys[0] != 1 || keys.len() != 4 + usize::from(keys[3]) * 4 {
            return Err(invalid("malformed WBM GeoTIFF keys"));
        }
        for (key, value) in [(1024, 2), (1025, 2), (2048, 4326), (2054, 9102)] {
            let entries: Vec<_> = keys[4..].chunks_exact(4).filter(|k| k[0] == key).collect();
            if entries.len() != 1 || entries[0][1..] != [0, 1, value] {
                return Err(invalid("WBM requires EPSG:4326 degrees and PixelIsPoint"));
            }
        }
        // Known categorical nodata must never masquerade as a valid source class.
        let nodata = decoder
            .find_tag(Tag::GdalNodata)
            .map_err(invalid)?
            .map(|v| v.into_string().map_err(invalid))
            .transpose()?;
        let nodata = nodata
            .map(|v| {
                v.trim_matches(char::from(0))
                    .trim()
                    .parse::<u8>()
                    .map_err(invalid)
            })
            .transpose()?;
        let mut result = DecodingResult::U8(Vec::new());
        decoder.read_image_to_buffer(&mut result).map_err(invalid)?;
        let DecodingResult::U8(values) = result else {
            return Err(invalid("WBM requires original unsigned byte categories"));
        };
        if values.len() != width as usize * ROWS
            || values.iter().any(|&v| v > 3 || Some(v) == nodata)
        {
            return Err(invalid("invalid or missing WBM category"));
        }
        let mut offsets = Vec::with_capacity(ROWS + 1);
        let mut runs = Vec::new();
        offsets.push(0);
        for row in values.chunks_exact(width as usize) {
            cancelled(cancel)?;
            let mut start = 0;
            while start < row.len() {
                let code = row[start];
                let mut end = start + 1;
                while end < row.len() && row[end] == code {
                    end += 1;
                }
                runs.push(((end as u32) << 2) | u32::from(code));
                start = end;
            }
            if runs.len() > MAX_MASK_RUNS {
                return Err(invalid("native category run budget exceeded"));
            }
            offsets.push(runs.len() as u32);
        }
        Ok(Self {
            south_deg: tile.south_deg,
            west_deg: tile.west_deg,
            width: width as usize,
            offsets,
            runs,
        })
    }
}
fn sample(
    offsets: &[u32],
    runs: &[u32],
    width: usize,
    height: usize,
    row: usize,
    column: usize,
) -> Option<u8> {
    if row >= height || column >= width {
        return None;
    }
    let range = &runs[offsets[row] as usize..offsets[row + 1] as usize];
    let index = range.partition_point(|run| (*run >> 2) as usize <= column);
    range.get(index).map(|run| (run & 3) as u8)
}

#[derive(Clone, Debug)]
pub struct WaterMask {
    tiles: Vec<WaterMaskTile>,
    index: Vec<i32>,
}
impl WaterMask {
    pub fn from_sources(
        sources: &[(CopernicusTile, PathBuf)],
        cancel: &AtomicBool,
    ) -> Result<Self, TopographyError> {
        cancelled(cancel)?;
        if sources.len() > crate::topography::MAX_SOURCE_TILES {
            return Err(TopographyError::TooManyTiles);
        }
        let mut tiles = Vec::new();
        let mut index = vec![-1; 180 * 360];
        let mut runs = 0;
        for (tile, path) in sources {
            cancelled(cancel)?;
            if !(-90..90).contains(&tile.south_deg) || !(-180..180).contains(&tile.west_deg) {
                return Err(invalid("invalid WBM tile coordinates"));
            }
            let slot = (tile.south_deg + 90) as usize * 360 + (tile.west_deg + 180) as usize;
            if index[slot] >= 0 {
                return Err(invalid("duplicate WBM source tile"));
            }
            let decoded = WaterMaskTile::decode(path, tile, cancel)?;
            runs += decoded.runs.len();
            if runs > MAX_MASK_RUNS {
                return Err(invalid("combined WBM run budget exceeded"));
            }
            index[slot] = tiles.len() as i32;
            tiles.push(decoded);
        }
        Ok(Self { tiles, index })
    }
    pub fn tiles(&self) -> &[WaterMaskTile] {
        &self.tiles
    }
    /// South-to-north, west-to-east degree-cell table; -1 denotes missing data.
    pub fn tile_indices(&self) -> &[i32] {
        &self.index
    }
    /// Nearest angular post; latitude first defines the actual source row owner.
    /// Exact half-post ties select north/east. Missing owners remain unknown.
    pub fn classify(&self, location: Geodetic) -> Option<u8> {
        let location = Geodetic::new(location.lat_deg, location.lon_deg, 0.0).ok()?;
        let latitude = (location.lat_deg * 3600.0 + 0.5).floor() as i32;
        let south = (latitude - 1).div_euclid(3600);
        if !(-90..90).contains(&south) {
            return None;
        }
        let width = crate::topography::copernicus::native_width(south);
        let step = 7200 / width as i32;
        let longitude = ((location.lon_deg * 7200.0 / f64::from(step) + 0.5).floor() as i32 * step
            + 180 * 7200)
            .rem_euclid(360 * 7200)
            - 180 * 7200;
        let west = longitude.div_euclid(7200);
        let id = self.index[(south + 90) as usize * 360 + (west + 180) as usize];
        if id < 0 {
            return None;
        }
        self.tiles[id as usize].sample(
            ((south + 1) * 3600 - latitude) as usize,
            ((longitude - west * 7200) / step) as usize,
        )
    }
}

/// Coarse source-derived cartographic context. A cell is not a terrain sample.
#[derive(Clone, Debug)]
pub struct GlobalLandMask {
    pub width: usize,
    pub height: usize,
    offsets: Vec<u32>,
    runs: Vec<u32>,
    vector: Option<vector::GlobalLandVector>,
}
impl GlobalLandMask {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TopographyError> {
        if bytes.starts_with(b"AEGLND02") {
            return Ok(Self {
                width: vector::ROOT_WIDTH,
                height: vector::ROOT_HEIGHT,
                offsets: vec![],
                runs: vec![],
                vector: Some(vector::GlobalLandVector::from_bytes(bytes)?),
            });
        }
        if bytes.len() < 20 || &bytes[..8] != b"AEGLND01" {
            return Err(invalid("invalid global land-mask format"));
        }
        let get = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        let (width, height, count) = (get(8), get(12), get(16));
        if width == 0
            || height == 0
            || width > 36000
            || height > 18000
            || width != 2 * height
            || count > MAX_MASK_RUNS
            || 20usize.checked_add((height + 1 + count) * 4) != Some(bytes.len())
        {
            return Err(invalid("invalid global land-mask dimensions"));
        }
        let offsets: Vec<_> = (0..=height).map(|i| get(20 + 4 * i) as u32).collect();
        let runs: Vec<_> = (0..count)
            .map(|i| get(20 + 4 * (height + 1 + i)) as u32)
            .collect();
        if offsets[0] != 0 || offsets[height] as usize != count {
            return Err(invalid("invalid global row offsets"));
        }
        for row in offsets.windows(2) {
            if row[0] >= row[1] || row[1] as usize > count {
                return Err(invalid("invalid global row interval"));
            }
            let mut prior = 0;
            for &run in &runs[row[0] as usize..row[1] as usize] {
                let end = run >> 2;
                if end <= prior || end as usize > width || (run & 3) > 2 {
                    return Err(invalid("invalid global category run"));
                }
                prior = end;
            }
            if prior as usize != width {
                return Err(invalid("incomplete global category row"));
            }
        }
        Ok(Self {
            width,
            height,
            offsets,
            runs,
            vector: None,
        })
    }
    /// Current original-vector context. The raster format remains readable only
    /// for archived regressions; it is not the bundled runtime coastline.
    pub fn vector(&self) -> Option<&vector::GlobalLandVector> {
        self.vector.as_ref()
    }
    pub fn row_offsets(&self) -> &[u32] {
        &self.offsets
    }
    pub fn runs(&self) -> &[u32] {
        &self.runs
    }
    pub fn classify(&self, location: Geodetic) -> Option<u8> {
        if let Some(vector) = &self.vector {
            return vector.classify(location);
        }
        let location = Geodetic::new(location.lat_deg, location.lon_deg, 0.0).ok()?;
        let lon = (location.lon_deg + 180.0).rem_euclid(360.0);
        let column = (lon / 360.0 * self.width as f64).floor() as usize;
        let row = ((90.0 - location.lat_deg) / 180.0 * self.height as f64).floor() as usize;
        sample(
            &self.offsets,
            &self.runs,
            self.width,
            self.height,
            row.min(self.height - 1),
            column.min(self.width - 1),
        )
    }
}
