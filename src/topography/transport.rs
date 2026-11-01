//! Bounded downloads from the reviewed Copernicus GLO-30 distribution.
//! Coordinate planning and raster interpretation belong to earth-math.
use earth_math::topography::CopernicusTile;
use ring::digest::{Context, SHA256, digest};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const PREFIX: &str = "https://copernicus-dem-30m.s3.amazonaws.com/";
const LICENSE_SHA: &str = "32049914c37f14e7d53b48d13d74a49e77c030236e2acc7a0df426f9344feba2";
const LICENSE: &[u8] = include_bytes!("../../licenses/Copernicus-WorldDEM-30-eula_F.pdf");
const MAX_TILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_REQUEST_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RasterKind {
    Elevation,
    Water,
}

impl RasterKind {
    fn id(self, tile: &CopernicusTile) -> String {
        match self {
            Self::Elevation => tile.id.clone(),
            Self::Water => tile.id.replace("_DEM", "_WBM"),
        }
    }

    fn url(self, tile: &CopernicusTile) -> &str {
        match self {
            Self::Elevation => &tile.dem_url,
            Self::Water => &tile.water_mask_url,
        }
    }
}

#[derive(Default)]
pub(super) struct Sources {
    pub elevations: Vec<(CopernicusTile, PathBuf)>,
    pub water: Vec<(CopernicusTile, PathBuf)>,
}

impl Sources {
    fn push(&mut self, item: &PlannedTile, root: &Path) {
        let source = (
            item.tile.clone(),
            root.join("assets")
                .join(format!("{}.tif", item.kind.id(&item.tile))),
        );
        match item.kind {
            RasterKind::Elevation => self.elevations.push(source),
            RasterKind::Water => self.water.push(source),
        }
    }
}

#[derive(Clone)]
pub(super) struct PlannedTile {
    pub tile: CopernicusTile,
    kind: RasterKind,
    pub bytes: u64,
    pub cached: bool,
}

#[derive(Clone)]
pub(super) struct Plan {
    pub available: Vec<PlannedTile>,
    pub missing: usize,
    pub missing_water: usize,
    pub water_tiles: usize,
    pub total_bytes: u64,
    pub download_bytes: u64,
}

pub(super) fn cache_directory() -> Option<PathBuf> {
    std::env::var_os("ANCIENT_EARTH_TOPOGRAPHY_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
                .map(|path| PathBuf::from(path).join("AncientEarth").join("topography"))
        })
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled. Completed source files remain cached.".into())
    } else {
        Ok(())
    }
}

fn validate_tile(tile: &CopernicusTile) -> Result<(), String> {
    if tile.id.is_empty()
        || !tile
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Invalid source tile identifier".into());
    }
    let directory = format!("{PREFIX}{}/", tile.id);
    if tile.dem_url != format!("{directory}{}.tif", tile.id)
        || tile.water_mask_url != format!("{directory}AUXFILES/{}.tif", RasterKind::Water.id(tile))
        || tile.license_url != format!("{directory}INFO/eula_F.pdf")
        || !tile.metadata_url.starts_with(&directory)
        || tile.metadata_url.contains("..")
    {
        return Err("Source is outside the reviewed Copernicus distribution".into());
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(300)))
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut context = Context::new(&SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
    }
    Ok(hex(context.finish().as_ref()))
}

fn read_json(path: &Path) -> Option<Value> {
    if fs::metadata(path).ok()?.len() > MAX_METADATA_BYTES {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn cached_bytes(
    root: &Path,
    tile: &CopernicusTile,
    kind: RasterKind,
    cancel: &AtomicBool,
) -> Option<u64> {
    let id = kind.id(tile);
    let metadata_path = root.join("assets").join(format!("{id}.json"));
    let metadata = read_json(&metadata_path)?;
    if metadata["source_url"] != kind.url(tile) || metadata["license_sha256"] != LICENSE_SHA {
        return None;
    }
    if hash_file(
        &root.join("licenses/Copernicus-WorldDEM-30-eula_F.pdf"),
        cancel,
    )
    .ok()?
        != LICENSE_SHA
    {
        return None;
    }
    let inventory = read_json(&root.join("licenses/assets.json"))?;
    let assets = inventory["assets"].as_array()?;
    let raster = assets
        .iter()
        .find(|a| a["path"] == format!("assets/{id}.tif"))?;
    if raster["sha256"] != metadata["sha256"]
        || raster["provenance_sha256"].as_str()? != hash_file(&metadata_path, cancel).ok()?
    {
        return None;
    }
    let xml = assets
        .iter()
        .find(|a| a["path"] == format!("assets/{id}.xml"))?;
    if xml["sha256"].as_str()?
        != hash_file(&root.join("assets").join(format!("{id}.xml")), cancel).ok()?
    {
        return None;
    }
    let path = root.join("assets").join(format!("{id}.tif"));
    let size = fs::metadata(&path).ok()?.len();
    if size == 0 || size > MAX_TILE_BYTES || metadata["bytes"].as_u64()? != size {
        return None;
    }
    if metadata["sha256"].as_str()? != hash_file(&path, cancel).ok()? {
        return None;
    }
    Some(size)
}

pub(super) fn plan(
    tiles: &[CopernicusTile],
    root: &Path,
    cancel: &AtomicBool,
    offline: bool,
    mut report: impl FnMut(String),
) -> Result<Plan, String> {
    let agent = agent();
    let mut result = Plan {
        available: Vec::new(),
        missing: 0,
        missing_water: 0,
        water_tiles: 0,
        total_bytes: 0,
        download_bytes: 0,
    };
    for (index, tile) in tiles.iter().enumerate() {
        check_cancel(cancel)?;
        validate_tile(tile)?;
        report(format!(
            "Checking source sizes and cache: {} / {}",
            index + 1,
            tiles.len()
        ));
        for kind in [RasterKind::Elevation, RasterKind::Water] {
            let missing = match kind {
                RasterKind::Elevation => &mut result.missing,
                RasterKind::Water => &mut result.missing_water,
            };
            let (bytes, cached) = if let Some(bytes) = cached_bytes(root, tile, kind, cancel) {
                (bytes, true)
            } else if offline {
                *missing += 1;
                continue;
            } else {
                let response = agent
                    .head(kind.url(tile))
                    .call()
                    .map_err(|e| format!("Size request failed: {e}"))?;
                if response.status() == 404 {
                    *missing += 1;
                    continue;
                }
                if response.status() != 200 {
                    return Err(format!("Source returned HTTP {}", response.status()));
                }
                let bytes = response
                    .headers()
                    .get("content-length")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| *n > 0 && *n <= MAX_TILE_BYTES)
                    .ok_or("Source did not provide an acceptable file size")?;
                (bytes, false)
            };
            result.total_bytes = result
                .total_bytes
                .checked_add(bytes)
                .ok_or("Source size overflow")?;
            if result.total_bytes > MAX_REQUEST_BYTES {
                return Err(
                    "This request exceeds the 4 GiB download budget. Reduce the radius.".into(),
                );
            }
            if !cached {
                result.download_bytes += bytes;
            }
            result.available.push(PlannedTile {
                tile: tile.clone(),
                kind,
                bytes,
                cached,
            });
            if kind == RasterKind::Water {
                result.water_tiles += 1;
            }
        }
    }
    if !result
        .available
        .iter()
        .any(|item| item.kind == RasterKind::Elevation)
    {
        return Err(if offline { "No verified cached source tiles for this area. Download them first." } else { "No public GLO-30 tiles found for this area. Ocean-only and restricted regions may have no tiles." }.into());
    }
    Ok(result)
}

fn small_get(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    if response.status() != 200 {
        return Err(format!(
            "Source metadata returned HTTP {}",
            response.status()
        ));
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

fn install_notices(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join("licenses")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("licenses/Copernicus-WorldDEM-30-eula_F.pdf"),
        LICENSE,
    )
    .map_err(|e| e.to_string())?;
    fs::write(root.join("NOTICE.md"), include_bytes!("../../NOTICE.md"))
        .map_err(|e| e.to_string())?;
    fs::write(root.join("LICENSE"), include_bytes!("../../LICENSE")).map_err(|e| e.to_string())?;
    Ok(())
}

fn register_tile(
    root: &Path,
    tile: &CopernicusTile,
    kind: RasterKind,
    bytes: u64,
    sha: String,
    xml: &[u8],
) -> Result<(), String> {
    let id = kind.id(tile);
    let relative = format!("assets/{id}.tif");
    let metadata_path = format!("assets/{id}.json");
    let xml_path = format!("assets/{id}.xml");
    fs::write(root.join(&xml_path), xml).map_err(|e| e.to_string())?;
    let modifications = match kind {
        RasterKind::Elevation => {
            "Original source GeoTIFF bytes retained. Application derives bounded source-post meshes and converts EGM2008 heights to WGS84 using the bundled NGA geoid; derived meshes remain in memory."
        }
        RasterKind::Water => {
            "Original categorical water-body-mask GeoTIFF bytes retained. Application reads native category posts, stores exact row runs in memory, and uses nearest-post classification outside the bundled reconstructed coastlines. This layer changes land/water categories only, never elevations. No imagery."
        }
    };
    let record = json!({"schema":"ancientearth.topography-source.v1", "source_url":kind.url(tile),
        "license_url":tile.license_url,"metadata_url":tile.metadata_url,"license_sha256":LICENSE_SHA,
        "bytes":bytes,"sha256":sha,"output_sha256":sha,
        "source":"Copernicus GLO-30 Public / 2021 AWS distribution",
        "modifications":modifications,
        "credit":"Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved"});
    write_json(&root.join(&metadata_path), &record)?;
    let manifest_path = root.join("licenses/assets.json");
    let mut manifest = if manifest_path.exists() {
        read_json(&manifest_path).ok_or("Cache asset inventory is invalid; preserve it for inspection and use a new cache folder")?
    } else {
        let bundled: Value = serde_json::from_str(include_str!("../../licenses/assets.json"))
            .expect("reviewed bundled inventory");
        json!({"schema":"ancientearth.asset-license-review.v1","reviewed_date":"2026-10-22",
            "policy":"Only the exact source license reviewed with this executable is accepted. Preserve notices and complete source terms when redistributing this cache.",
            "sources":{"copernicus":bundled["sources"]["copernicus"]},"assets":[]})
    };
    let assets = manifest["assets"]
        .as_array_mut()
        .ok_or("Invalid cache asset inventory")?;
    assets.retain(|asset| asset["path"] != relative && asset["path"] != xml_path);
    assets.push(
        json!({"path":relative,"sha256":sha,"sources":["copernicus"],"provenance":metadata_path,
        "provenance_sha256":hash_file(&root.join(&metadata_path), &AtomicBool::new(false))?}),
    );
    assets.push(json!({"path":xml_path,"sha256":hex(digest(&SHA256, xml).as_ref()),"sources":["copernicus"],"description":"Unmodified provider product metadata; source URL in the companion provenance."}));
    write_json(&manifest_path, &manifest)
}

pub(super) fn fetch(
    plan: &Plan,
    root: &Path,
    cancel: &AtomicBool,
    mut report: impl FnMut(String, u64),
) -> Result<Sources, String> {
    check_cancel(cancel)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let cache_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("download.lock"))
        .map_err(|e| e.to_string())?;
    cache_lock.try_lock().map_err(|_| "Another Ancient Earth process is updating this cache. Try again when its download finishes.")?;
    install_notices(root)?;
    let agent = agent();
    let mut downloaded = 0;
    let mut sources = Sources::default();
    for (index, item) in plan.available.iter().enumerate() {
        check_cancel(cancel)?;
        let tile = &item.tile;
        validate_tile(tile)?;
        let id = item.kind.id(tile);
        let path = root.join("assets").join(format!("{id}.tif"));
        if item.cached && cached_bytes(root, tile, item.kind, cancel).is_some() {
            sources.push(item, root);
            continue;
        }
        // Public access is not a license. Reject revised/unknown tile terms.
        let terms = small_get(&agent, &tile.license_url)?;
        if hex(digest(&SHA256, &terms).as_ref()) != LICENSE_SHA {
            return Err(format!(
                "{} has unreviewed license terms; no terrain was downloaded for it.",
                tile.id
            ));
        }
        check_cancel(cancel)?;
        let xml = small_get(&agent, &tile.metadata_url)?;
        let mut response = agent
            .get(item.kind.url(tile))
            .call()
            .map_err(|e| e.to_string())?;
        if response.status() != 200 {
            return Err(format!(
                "Source download returned HTTP {}",
                response.status()
            ));
        }
        let part = root.join("assets").join(format!("{id}.part"));
        let transfer = (|| {
            let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
            let mut reader = response.body_mut().as_reader();
            let mut buffer = [0u8; 64 * 1024];
            let mut count = 0u64;
            loop {
                check_cancel(cancel)?;
                let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > item.bytes || count > MAX_TILE_BYTES {
                    return Err("Source file exceeded its advertised size".into());
                }
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                downloaded += n as u64;
                report(
                    format!(
                        "Downloading elevation / water sources, please wait: file {} / {}",
                        index + 1,
                        plan.available.len()
                    ),
                    downloaded,
                );
            }
            if count != item.bytes {
                return Err("Source transfer was incomplete".into());
            }
            file.sync_all().map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = transfer {
            let _ = fs::remove_file(&part);
            return Err(error);
        }
        let sha = match hash_file(&part, cancel) {
            Ok(sha) => sha,
            Err(error) => {
                let _ = fs::remove_file(&part);
                return Err(error);
            }
        };
        // Windows rename does not replace a file: only remove the same named,
        // invalid/incomplete cache entry after a full replacement is available.
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        fs::rename(&part, &path).map_err(|e| e.to_string())?;
        register_tile(root, tile, item.kind, item.bytes, sha, &xml)?;
        sources.push(item, root);
    }
    Ok(sources)
}

pub(super) fn cached_sources(plan: &Plan, root: &Path) -> Sources {
    let mut sources = Sources::default();
    for item in plan.available.iter().filter(|item| item.cached) {
        sources.push(item, root);
    }
    sources
}

pub(super) fn save_selection(
    root: &Path,
    region: &earth_math::topography::DownloadRegion,
) -> Result<(), String> {
    write_json(
        &root.join("selection.json"),
        &json!({"latitude":region.center.lat_deg,"longitude":region.center.lon_deg,"radius_m":region.radius_m}),
    )
}

pub(super) fn saved_selection(root: &Path) -> Option<earth_math::topography::DownloadRegion> {
    let record = read_json(&root.join("selection.json"))?;
    let center = earth_math::Geodetic::new(
        record["latitude"].as_f64()?,
        record["longitude"].as_f64()?,
        0.0,
    )
    .ok()?;
    earth_math::topography::DownloadRegion::new(center, record["radius_m"].as_f64()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::{
        Geodetic,
        topography::{DownloadRegion, copernicus_tiles},
    };

    fn tile() -> CopernicusTile {
        copernicus_tiles(
            &DownloadRegion::new(Geodetic::new(29.7, -95.3, 0.0).unwrap(), 1000.0).unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|t| t.core)
        .unwrap()
    }

    #[test]
    fn approved_endpoint_and_cancellation_fail_closed() {
        let source = tile();
        validate_tile(&source).unwrap();
        let mut changed = source.clone();
        changed.dem_url = "https://example.com/map.tif".into();
        assert!(validate_tile(&changed).is_err());
        changed = source.clone();
        changed.water_mask_url = "https://example.com/water.tif".into();
        assert!(validate_tile(&changed).is_err());
        changed = source;
        changed.id = "../outside".into();
        assert!(validate_tile(&changed).is_err());
        assert_eq!(hex(digest(&SHA256, LICENSE).as_ref()), LICENSE_SHA);
        let plan = Plan {
            available: vec![],
            missing: 0,
            missing_water: 0,
            water_tiles: 0,
            total_bytes: 0,
            download_bytes: 0,
        };
        assert!(
            fetch(
                &plan,
                Path::new("not-created"),
                &AtomicBool::new(true),
                |_, _| {}
            )
            .is_err()
        );
    }

    #[test]
    fn cache_hash_license_and_provenance_are_required() {
        let root = std::env::temp_dir().join(format!(
            "ancient-earth-topography-test-{}",
            std::process::id()
        ));
        let source = tile();
        install_notices(&root).unwrap();
        let payload = b"test-only raster payload";
        let path = root.join("assets").join(format!("{}.tif", source.id));
        fs::write(&path, payload).unwrap();
        register_tile(
            &root,
            &source,
            RasterKind::Elevation,
            payload.len() as u64,
            hex(digest(&SHA256, payload).as_ref()),
            b"<test-only-metadata/>",
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            cached_bytes(&root, &source, RasterKind::Elevation, &cancel),
            Some(payload.len() as u64)
        );
        let planned = plan(std::slice::from_ref(&source), &root, &cancel, true, |_| {}).unwrap();
        assert_eq!(planned.download_bytes, 0);
        assert_eq!(planned.total_bytes, payload.len() as u64);
        assert_eq!(planned.missing_water, 1);
        assert!(cached_sources(&planned, &root).water.is_empty());
        let inventory = read_json(&root.join("licenses/assets.json")).unwrap();
        assert_eq!(inventory["assets"].as_array().unwrap().len(), 2);
        // WBM uses a distinct verified cache entry, never the similarly named DEM.
        assert!(cached_bytes(&root, &source, RasterKind::Water, &cancel).is_none());
        let water_path = root
            .join("assets")
            .join(format!("{}.tif", RasterKind::Water.id(&source)));
        fs::write(&water_path, payload).unwrap();
        register_tile(
            &root,
            &source,
            RasterKind::Water,
            payload.len() as u64,
            hex(digest(&SHA256, payload).as_ref()),
            b"<test-only-metadata/>",
        )
        .unwrap();
        let planned = plan(std::slice::from_ref(&source), &root, &cancel, true, |_| {}).unwrap();
        assert_eq!(planned.water_tiles, 1);
        assert_eq!(planned.missing_water, 0);
        assert_eq!(cached_sources(&planned, &root).water[0].1, water_path);
        fs::write(&water_path, b"corrupt water mask").unwrap();
        assert!(cached_bytes(&root, &source, RasterKind::Water, &cancel).is_none());
        assert!(cached_bytes(&root, &source, RasterKind::Elevation, &cancel).is_some());
        let record_path = root.join("assets").join(format!("{}.json", source.id));
        let mut record = read_json(&record_path).unwrap();
        record["license_sha256"] = json!("unreviewed");
        write_json(&record_path, &record).unwrap();
        assert!(cached_bytes(&root, &source, RasterKind::Elevation, &cancel).is_none());
        record["license_sha256"] = json!(LICENSE_SHA);
        write_json(&record_path, &record).unwrap();
        fs::write(&path, b"corrupt cache").unwrap();
        assert!(cached_bytes(&root, &source, RasterKind::Elevation, &cancel).is_none());
        // Only remove this test's fixed files, never recursive shell deletion.
        for dir in [root.join("assets"), root.join("licenses")] {
            for entry in fs::read_dir(&dir).unwrap() {
                fs::remove_file(entry.unwrap().path()).unwrap();
            }
            fs::remove_dir(dir).unwrap();
        }
        fs::remove_file(root.join("NOTICE.md")).unwrap();
        fs::remove_file(root.join("LICENSE")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
