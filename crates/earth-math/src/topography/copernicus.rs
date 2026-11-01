//! Copernicus GLO-30 2021 source-post import. No averaged TIFF overviews.
use super::{DownloadRegion, TopographyError};
use crate::{GeoidGrid, TerrainPatch, TerrainScene};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tiff::{
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};

/// Interactive resource limits, not geographic or source-accuracy statements.
pub const MAX_SOURCE_TILES: usize = 128;
pub const MAX_RETAINED_VERTICES: usize = 500_000;
const MAX_BACKGROUND_VERTICES: usize = 300_000;
const MAX_LOCAL_VERTICES: usize = 200_000;
const UNITS_PER_DEGREE: usize = 7200; // exact half arcseconds
const BUCKET: &str = "https://copernicus-dem-30m.s3.amazonaws.com";
mod refinement;
#[cfg(test)]
mod review;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopernicusTile {
    pub id: String,
    pub south_deg: i32,
    pub west_deg: i32,
    /// Core tiles intersect a query rectangle; support tiles supply omitted
    /// east/south duplicate posts, and do not independently enlarge the mesh.
    pub core: bool,
    pub dem_url: String,
    pub license_url: String,
    pub metadata_url: String,
    pub water_mask_url: String,
}
impl CopernicusTile {
    fn new(south: i32, west: i32, core: bool) -> Self {
        let latitude = format!("{}{:02}", if south < 0 { 'S' } else { 'N' }, south.abs());
        let longitude = format!("{}{:03}", if west < 0 { 'W' } else { 'E' }, west.abs());
        let geographic = format!("{latitude}_00_{longitude}_00");
        let directory = format!("{BUCKET}/Copernicus_DSM_COG_10_{geographic}_DEM");
        Self {
            id: format!("Copernicus_DSM_COG_10_{geographic}_DEM"),
            south_deg: south,
            west_deg: west,
            core,
            dem_url: format!("{directory}/Copernicus_DSM_COG_10_{geographic}_DEM.tif"),
            license_url: format!("{directory}/INFO/eula_F.pdf"),
            metadata_url: format!("{directory}/Copernicus_DSM_10_{geographic}.xml"),
            water_mask_url: format!(
                "{directory}/AUXFILES/Copernicus_DSM_COG_10_{geographic}_WBM.tif"
            ),
        }
    }
}
fn east(west: i32) -> i32 {
    if west == 179 { -180 } else { west + 1 }
}
/// Conservative whole-tile selection. Complete tiles can extend beyond the
/// requested radius. Source absence is NOT interpreted as ocean or zero height.
pub fn copernicus_tiles(region: &DownloadRegion) -> Result<Vec<CopernicusTile>, TopographyError> {
    // Recompute from validated input: callers cannot smuggle arbitrary boxes.
    let region = DownloadRegion::new(region.center, region.radius_m)?;
    let mut core = BTreeSet::new();
    for bounds in region.query_boxes {
        for south in
            (bounds.south_deg.floor() as i32).max(-90)..(bounds.north_deg.ceil() as i32).min(90)
        {
            for west in
                (bounds.west_deg.floor() as i32).max(-180)..(bounds.east_deg.ceil() as i32).min(180)
            {
                core.insert((south, west));
                if core.len() > MAX_SOURCE_TILES {
                    return Err(TopographyError::TooManyTiles);
                }
            }
        }
    }
    let mut all = core.clone();
    for &(south, west) in &core {
        all.insert((south, east(west)));
        if south > -90 {
            all.insert((south - 1, west));
            all.insert((south - 1, east(west)));
        }
    }
    if all.len() > MAX_SOURCE_TILES {
        return Err(TopographyError::TooManyTiles);
    }
    Ok(all
        .into_iter()
        .map(|(s, w)| CopernicusTile::new(s, w, core.contains(&(s, w))))
        .collect())
}
pub(crate) fn native_width(south: i32) -> usize {
    let latitude = (f64::from(south) + 0.5).abs();
    if latitude < 50.0 {
        3600
    } else if latitude < 60.0 {
        2400
    } else if latitude < 70.0 {
        1800
    } else if latitude < 80.0 {
        1200
    } else if latitude < 85.0 {
        720
    } else {
        360
    }
}
fn retained_step(tiles: &[CopernicusTile]) -> Result<usize, TopographyError> {
    let count = tiles.iter().filter(|tile| tile.core).count();
    (2..=UNITS_PER_DEGREE / 2)
        .find(|step| {
            UNITS_PER_DEGREE.is_multiple_of(*step)
                && step.is_multiple_of(2)
                && tiles.iter().all(|tile| {
                    step.is_multiple_of(UNITS_PER_DEGREE / native_width(tile.south_deg))
                })
                && count * (UNITS_PER_DEGREE / step + 1).pow(2) <= MAX_BACKGROUND_VERTICES
        })
        .ok_or(TopographyError::TooManyTiles)
}
#[derive(Clone, Debug)]
pub struct ImportSummary {
    pub imported_tiles: usize,
    /// Missing requested core/support files; availability is not fabricated.
    pub missing_tiles: usize,
    pub retained_vertices: usize,
    pub source_spacing_arc_seconds: String,
    pub retained_spacing_arc_seconds: f64,
    /// A conservative radius enclosure, centered on the original request;
    /// individual source-aligned refined cells can extend beyond it.
    pub local_detail_radius_m: f64,
    pub local_spacing_arc_seconds: f64,
    pub local_added_vertices: usize,
    /// Range of retained original EGM2008 source heights, before H+N.
    pub retained_source_height_range_m: [f64; 2],
}
impl ImportSummary {
    pub fn description(&self) -> String {
        format!(
            "{} terrain tiles; {} retained source posts; {:.3} arcsec background lattice (native {} arcsec). Local detail: {:.3} arcsec inside the {:.0} m request-center enclosure, {} additional posts. Retained source H range {:.3} to {:.3} m; heights are not exaggerated. {} unavailable source/support tiles. Whole-tile coverage can exceed the requested radius. Missing coverage uses WGS84 h=0.",
            self.imported_tiles,
            self.retained_vertices,
            self.retained_spacing_arc_seconds,
            self.source_spacing_arc_seconds,
            self.local_spacing_arc_seconds,
            self.local_detail_radius_m,
            self.local_added_vertices,
            self.retained_source_height_range_m[0],
            self.retained_source_height_range_m[1],
            self.missing_tiles
        )
    }
}
pub fn format_download_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1e9)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1e6)
    } else if bytes >= 1000 {
        format!("{:.1} kB", bytes as f64 / 1000.0)
    } else {
        format!("{bytes} bytes")
    }
}
fn cancelled(cancel: &AtomicBool) -> Result<(), TopographyError> {
    if cancel.load(Ordering::Relaxed) {
        Err(TopographyError::Cancelled)
    } else {
        Ok(())
    }
}
fn invalid(message: impl ToString) -> TopographyError {
    TopographyError::InvalidRaster(message.to_string())
}
struct Samples {
    heights: BTreeMap<(usize, usize), f64>,
}
fn decode_source(
    path: &Path,
    tile: &CopernicusTile,
    requested: &BTreeSet<(usize, usize)>,
    cancel: &AtomicBool,
) -> Result<Samples, TopographyError> {
    cancelled(cancel)?;
    let file = File::open(path).map_err(invalid)?;
    if file.metadata().map_err(invalid)?.len() > 256 * 1024 * 1024 {
        return Err(invalid("source file exceeds bounded decoder size"));
    }
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 64 * 1024 * 1024;
    limits.intermediate_buffer_size = 32 * 1024 * 1024;
    limits.ifd_value_size = 1024 * 1024;
    let mut decoder = Decoder::new(BufReader::new(file))
        .map_err(invalid)?
        .with_limits(limits);
    let (width, height) = decoder.dimensions().map_err(invalid)?;
    if width as usize != native_width(tile.south_deg) || height != 3600 {
        return Err(invalid("unexpected native grid dimensions or overview"));
    }
    for (tag, expected) in [
        (Tag::BitsPerSample, 32),
        (Tag::SamplesPerPixel, 1),
        (Tag::SampleFormat, 3),
        (Tag::Compression, 8),
        (Tag::Predictor, 3),
        (Tag::PhotometricInterpretation, 1),
    ] {
        if decoder.get_tag_u32(tag).map_err(invalid)? != expected {
            return Err(invalid(format!("unexpected {tag:?}")));
        }
    }
    for tag in [Tag::Orientation, Tag::PlanarConfiguration] {
        if decoder
            .find_tag_unsigned::<u32>(tag)
            .map_err(invalid)?
            .is_some_and(|value| value != 1)
        {
            return Err(invalid("requires top-left, single-plane source layout"));
        }
    }
    if decoder
        .find_tag(Tag::ModelTransformationTag)
        .map_err(invalid)?
        .is_some()
    {
        return Err(invalid("transformed source grid is unsupported"));
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
        || !tie[..3].iter().all(|&v| close(v, 0.0))
        || !close(tie[3], f64::from(tile.west_deg))
        || !close(tie[4], f64::from(tile.south_deg + 1))
        || !close(tie[5], 0.0)
    {
        return Err(invalid(
            "source post registration does not match its official tile",
        ));
    }
    let keys = decoder
        .get_tag_u16_vec(Tag::GeoKeyDirectoryTag)
        .map_err(invalid)?;
    if keys.len() < 4 || keys[0] != 1 || keys.len() != 4 + usize::from(keys[3]) * 4 {
        return Err(invalid("malformed GeoTIFF key directory"));
    }
    for (key, value) in [(1024, 2), (1025, 2), (2048, 4326), (2054, 9102)] {
        let matches: Vec<_> = keys[4..]
            .chunks_exact(4)
            .filter(|entry| entry[0] == key)
            .collect();
        if matches.len() != 1 || matches[0][1..] != [0, 1, value] {
            return Err(invalid(
                "requires WGS84 geographic degrees and PixelIsPoint",
            ));
        }
    }
    let nodata = decoder
        .find_tag(Tag::GdalNodata)
        .map_err(invalid)?
        .map(|v| v.into_string().map_err(invalid))
        .transpose()?;
    let nodata = nodata
        .map(|v| {
            v.trim_matches(char::from(0))
                .trim()
                .parse::<f32>()
                .map_err(invalid)
        })
        .transpose()?;
    let mut result = DecodingResult::F32(Vec::new());
    decoder.read_image_to_buffer(&mut result).map_err(invalid)?;
    cancelled(cancel)?;
    let DecodingResult::F32(values) = result else {
        return Err(invalid("expected Float32 metre elevations"));
    };
    if values.len() != width as usize * height as usize {
        return Err(invalid("incomplete raster"));
    }
    let mut heights = BTreeMap::new();
    for &(row, column) in requested {
        cancelled(cancel)?;
        let x_step = UNITS_PER_DEGREE / width as usize;
        if row >= UNITS_PER_DEGREE
            || column >= UNITS_PER_DEGREE
            || !row.is_multiple_of(2)
            || !column.is_multiple_of(x_step)
        {
            return Err(invalid(
                "requested point is not an original native source post",
            ));
        }
        let index = (row / 2) * width as usize + column / x_step;
        let value = values[index];
        if !value.is_finite() || !(-10_000.0..=100_000.0).contains(&value) || nodata == Some(value)
        {
            return Err(invalid("missing or invalid retained source elevation"));
        }
        heights.insert((row, column), f64::from(value));
    }
    Ok(Samples { heights })
}
/// Import only reviewed official source files supplied by the transport layer.
/// Every retained height is an original first-IFD post, converted H+N using the
/// bundled native EGM2008 grid. Category masks do not alter these heights.
pub fn import_sources(
    region: &DownloadRegion,
    sources: &[(CopernicusTile, PathBuf)],
    geoid: &GeoidGrid,
    boundaries: Arc<TerrainScene>,
    cancel: &AtomicBool,
) -> Result<(TerrainScene, ImportSummary), TopographyError> {
    let planned = copernicus_tiles(region)?;
    let step = retained_step(&planned)?;
    let mut available = BTreeSet::new();
    for (tile, _) in sources {
        if !planned.contains(tile) || !available.insert((tile.south_deg, tile.west_deg)) {
            return Err(invalid("unplanned or duplicate source tile"));
        }
    }
    cancelled(cancel)?;
    let (plans, detail) = refinement::plans(&planned, &available, region, step, cancel)?;
    let mut requests: BTreeMap<(i32, i32), BTreeSet<(usize, usize)>> = BTreeMap::new();
    for (&tile, plan) in &plans {
        for &point in &plan.points {
            let (key, source) = refinement::source_point(tile, point);
            requests.entry(key).or_default().insert(source);
        }
    }
    let mut decoded = BTreeMap::new();
    for (tile, path) in sources {
        cancelled(cancel)?;
        let requested = requests
            .remove(&(tile.south_deg, tile.west_deg))
            .unwrap_or_default();
        decoded.insert(
            (tile.south_deg, tile.west_deg),
            decode_source(path, tile, &requested, cancel)?,
        );
    }
    if !requests.is_empty() {
        return Err(TopographyError::MissingData);
    }
    let mut patches = Vec::new();
    let mut vertices = 0;
    let mut added_vertices = 0;
    let mut source_range = [f64::INFINITY, f64::NEG_INFINITY];
    for (key, plan) in plans {
        cancelled(cancel)?;
        let base_count = plan.width * plan.height;
        let mut heights = Vec::with_capacity(base_count);
        let mut added = Vec::with_capacity(plan.points.len() - base_count);
        for (index, &point) in plan.points.iter().enumerate() {
            cancelled(cancel)?;
            let (source_tile, source_point) = refinement::source_point(key, point);
            let h = *decoded
                .get(&source_tile)
                .and_then(|source| source.heights.get(&source_point))
                .ok_or(TopographyError::MissingData)?;
            source_range[0] = source_range[0].min(h);
            source_range[1] = source_range[1].max(h);
            let mut position = refinement::position(key, point, h)?;
            position.height_m += geoid.undulation_m(position.lat_deg, position.lon_deg)?;
            if index < base_count {
                heights.push(position.height_m);
            } else {
                added.push(position);
            }
        }
        vertices += plan.points.len();
        added_vertices += added.len();
        let patch = TerrainPatch::from_grid(
            plan.width,
            plan.height,
            f64::from(key.1),
            f64::from(key.0 + 1) - plan.first_row as f64 * step as f64 / UNITS_PER_DEGREE as f64,
            step as f64 / UNITS_PER_DEGREE as f64,
            step as f64 / UNITS_PER_DEGREE as f64,
            heights,
        )?;
        patches.push(if plan.refined {
            patch.with_source_refinement(added, plan.faces)?
        } else {
            patch
        });
    }
    if patches.is_empty() {
        return Err(TopographyError::MissingData);
    }
    cancelled(cancel)?;
    let native_max = planned
        .iter()
        .map(|tile| 3600.0 / native_width(tile.south_deg) as f64)
        .fold(1.0, f64::max);
    let summary = ImportSummary {
        imported_tiles: patches.len(),
        missing_tiles: planned.len() - decoded.len(),
        retained_vertices: vertices,
        source_spacing_arc_seconds: format!("1 latitude; 1–{native_max:.1} longitude"),
        retained_spacing_arc_seconds: step as f64 / 2.0,
        local_detail_radius_m: detail.radius_m,
        local_spacing_arc_seconds: detail.step as f64 / 2.0,
        local_added_vertices: added_vertices,
        retained_source_height_range_m: source_range,
    };
    Ok((TerrainScene::with_terrain(patches, boundaries)?, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Geodetic, Observer, SurfaceKind};
    #[test]
    fn tile_plan_wraps_dateline_and_supports_exact_native_lattices() {
        for (lat, lon) in [(29.3, -94.8), (50.0, 179.9), (-60.0, -179.9), (79.0, 12.0)] {
            let region = DownloadRegion::new(
                Geodetic::new(lat, lon, 0.0).unwrap(),
                super::super::DEFAULT_RADIUS_M,
            )
            .unwrap();
            let tiles = copernicus_tiles(&region).unwrap();
            let step = retained_step(&tiles).unwrap();
            let core = tiles.iter().filter(|tile| tile.core).count();
            assert!(core * (7200 / step + 1).pow(2) <= MAX_RETAINED_VERTICES);
            for tile in &tiles {
                assert!((-180..180).contains(&tile.west_deg));
                assert!(step.is_multiple_of(7200 / native_width(tile.south_deg)));
                assert!(tile.dem_url.ends_with(&format!("{}.tif", tile.id)));
                if tile.core {
                    assert!(tiles.iter().any(|other| other.south_deg == tile.south_deg
                        && other.west_deg == east(tile.west_deg)));
                }
            }
        }
        // Public fields must not bypass the radius policy at the planner.
        let mut forged = DownloadRegion::new(
            Geodetic::new(0.0, 0.0, 0.0).unwrap(),
            super::super::MAX_RADIUS_M,
        )
        .unwrap();
        forged.radius_m = 1e9;
        assert_eq!(
            copernicus_tiles(&forged),
            Err(TopographyError::InvalidRadius)
        );
        // A permitted radius can still exceed the independent source budget
        // near a pole because its enclosure spans every longitude.
        let polar = DownloadRegion::new(
            Geodetic::new(90.0, 0.0, 0.0).unwrap(),
            super::super::MAX_RADIUS_M,
        )
        .unwrap();
        assert_eq!(copernicus_tiles(&polar), Err(TopographyError::TooManyTiles));
    }
    #[test]
    fn imported_surface_toggle_is_explicit_and_negative_heights_override_reference() {
        let boundaries = Arc::new(TerrainScene::boundary_only(vec![]).unwrap());
        let make = |height| {
            TerrainPatch::from_grid(3, 3, -0.001, 0.001, 0.001, 0.001, vec![height; 9]).unwrap()
        };
        let positive =
            TerrainScene::with_terrain(vec![make(250.0)], Arc::clone(&boundaries)).unwrap();
        let negative =
            TerrainScene::with_terrain(vec![make(-30.0)], Arc::clone(&boundaries)).unwrap();
        let location = Geodetic::new(0.0, 0.0, 0.0).unwrap();
        let mut observer = Observer::new(location);
        let lift = positive
            .ensure_observer_above_surface(&mut observer)
            .unwrap();
        assert!((lift - 250.0).abs() < 1e-7);
        assert!((observer.feet.height_m - 250.0).abs() < 1e-7);
        assert_eq!(
            negative
                .ensure_observer_above_surface(&mut observer)
                .unwrap(),
            0.0
        );
        negative
            .try_teleport(&mut observer, location.with_height(-29.0).unwrap())
            .unwrap();
        let hit = negative.ground(location).unwrap();
        assert!((hit.hit.geodetic.height_m + 30.0).abs() < 1e-7);
        assert!(matches!(hit.kind, SurfaceKind::Terrain(0)));
        assert!(!hit.grass_allowed); // no category data is not evidence of land
        let lift = boundaries
            .ensure_observer_above_surface(&mut observer)
            .unwrap();
        assert_eq!(lift, 29.0);
        assert_eq!(observer.feet.height_m, 0.0);
        let stopped = AtomicBool::new(true);
        assert!(positive.skyline_cancellable(observer, &stopped).is_none());
    }
    #[test]
    fn invalid_or_cancelled_import_does_not_fabricate_elevations() {
        let region = DownloadRegion::new(Geodetic::new(29.3, -94.8, 0.0).unwrap(), 1000.0).unwrap();
        let geoid = GeoidGrid::from_grid(2, 2, vec![0.0; 4]).unwrap();
        let boundaries = Arc::new(TerrainScene::boundary_only(vec![]).unwrap());
        assert!(matches!(
            import_sources(
                &region,
                &[],
                &geoid,
                Arc::clone(&boundaries),
                &AtomicBool::new(false)
            ),
            Err(TopographyError::MissingData)
        ));
        assert!(matches!(
            import_sources(&region, &[], &geoid, boundaries, &AtomicBool::new(true)),
            Err(TopographyError::Cancelled)
        ));
        assert_eq!(format_download_size(1_000_000), "1.0 MB");
        assert_eq!(format_download_size(1_500_000_000), "1.50 GB");
    }
}
