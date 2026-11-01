//! Original cached WBM source values were independently decoded with GDAL.
use earth_math::{topography::*, water_mask::*, *};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[path = "fixtures/wbm_source.rs"]
mod reference;

fn planned(south: i32, west: i32) -> CopernicusTile {
    let center = Geodetic::new(f64::from(south) + 0.5, f64::from(west) + 0.5, 0.0).unwrap();
    copernicus_tiles(&DownloadRegion::new(center, 1.0).unwrap())
        .unwrap()
        .into_iter()
        .find(|tile| tile.south_deg == south && tile.west_deg == west)
        .unwrap()
}

fn crc_table() -> [u32; 256] {
    std::array::from_fn(|value| {
        let mut crc = value as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
        crc
    })
}

#[test]
#[ignore = "requires the four previously licensed native WBM COGs in .cache/water"]
fn cached_native_water_categories_match_independent_gdal_for_every_source_post() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources: Vec<_> = reference::RASTERS
        .iter()
        .map(|&(name, south, west, _, _, _)| {
            (
                planned(south, west),
                root.join(format!(".cache/water/{name}-WBM.tif")),
            )
        })
        .collect();
    let mask = WaterMask::from_sources(&sources, &AtomicBool::new(false)).unwrap();
    let table = crc_table();
    for (tile, &(_, _, _, expected_counts, expected_crc, expected_runs)) in
        mask.tiles().iter().zip(reference::RASTERS)
    {
        assert_eq!(tile.row_offsets().len(), 3601);
        assert_eq!(tile.runs().len(), expected_runs);
        let mut crc = u32::MAX;
        let mut counts = [0_u64; 4];
        for row in 0..3600 {
            let (start, end) = (
                tile.row_offsets()[row] as usize,
                tile.row_offsets()[row + 1] as usize,
            );
            let mut column = 0;
            for &run in &tile.runs()[start..end] {
                let stop = (run >> 2) as usize;
                let code = (run & 3) as u8;
                assert!(stop > column && stop <= tile.width);
                assert_eq!(tile.sample(row, column), Some(code));
                assert_eq!(tile.sample(row, stop - 1), Some(code));
                counts[code as usize] += (stop - column) as u64;
                for _ in column..stop {
                    crc = (crc >> 8) ^ table[((crc ^ u32::from(code)) & 255) as usize];
                }
                column = stop;
            }
            assert_eq!(column, tile.width);
        }
        assert_eq!(counts, expected_counts);
        assert_eq!(!crc, expected_crc);
        assert_eq!(tile.sample(3600, 0), None);
        assert_eq!(tile.sample(0, tile.width), None);
    }
    for &(source, row, column, latitude, longitude, code) in reference::POSTS {
        assert_eq!(mask.tiles()[source].sample(row, column), Some(code));
        assert_eq!(
            mask.classify(Geodetic::new(latitude, longitude, 1234.0).unwrap()),
            Some(code)
        );
    }
    // Exact same bytes cannot be attached at a different geographic tile.
    let mismatch = [(planned(30, -95), sources[0].1.clone())];
    assert!(WaterMask::from_sources(&mismatch, &AtomicBool::new(false)).is_err());
    let duplicate = [sources[0].clone(), sources[0].clone()];
    assert!(WaterMask::from_sources(&duplicate, &AtomicBool::new(false)).is_err());
}

fn source_coordinate(path: &Path) -> (i32, i32) {
    let name = path.file_name().unwrap().to_str().unwrap();
    let parts: Vec<_> = name.split('_').collect();
    let latitude = parts[4];
    let longitude = parts[6];
    let lat: i32 = latitude[1..].parse().unwrap();
    let lon: i32 = longitude[1..].parse().unwrap();
    (
        if latitude.starts_with('S') { -lat } else { lat },
        if longitude.starts_with('W') {
            -lon
        } else {
            lon
        },
    )
}

#[test]
#[ignore = "read-only audit of the explicitly selected isolated downloaded WBM cache"]
fn cached_downloaded_water_runs_export_for_independent_whole_raster_audit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cache = std::env::var_os("ANCIENT_EARTH_WBM_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".cache/topography-wide-area-qa"));
    let mut paths: Vec<_> = std::fs::read_dir(cache.join("assets"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("_WBM.tif")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    let sources: Vec<_> = paths
        .iter()
        .map(|path| {
            let (s, w) = source_coordinate(path);
            (planned(s, w), path.clone())
        })
        .collect();
    let field = WaterMask::from_sources(&sources, &AtomicBool::new(false)).unwrap();
    let mut bytes = b"AEWBMT01".to_vec();
    bytes.extend((field.tiles().len() as u32).to_le_bytes());
    let mut posts = 0;
    let mut runs = 0;
    for tile in field.tiles() {
        bytes.extend(tile.south_deg.to_le_bytes());
        bytes.extend(tile.west_deg.to_le_bytes());
        for value in [
            tile.width,
            3600,
            tile.row_offsets().len(),
            tile.runs().len(),
        ] {
            bytes.extend((value as u32).to_le_bytes());
        }
        for &value in tile.row_offsets().iter().chain(tile.runs()) {
            bytes.extend(value.to_le_bytes());
        }
        posts += tile.width * 3600;
        runs += tile.runs().len();
    }
    std::fs::write(root.join(".cache/wbm-field-review.bin"), bytes).unwrap();
    println!(
        "Exported {} original source masks: {posts} native posts, {runs} lossless row runs",
        field.tiles().len()
    );
}

#[test]
fn global_context_stays_categorical_at_poles_dateline_and_missing_dem() {
    let global =
        GlobalLandMask::from_bytes(include_bytes!("../../../assets/boundaries/global-land.ael"))
            .unwrap();
    for (lat, lon, expected) in [
        (0.0, -140.0, 1),
        (40.0, -100.0, 0),
        (42.0, 50.0, 2),
        (-89.0, 0.0, 0),
        (89.0, 0.0, 1),
    ] {
        let location = Geodetic::new(lat, lon, 0.0).unwrap();
        assert_eq!(global.classify(location), Some(expected));
    }
    for latitude in [-90.0, -60.0, 0.0, 60.0, 90.0] {
        assert_eq!(
            global.classify(Geodetic::new(latitude, 180.0, 0.0).unwrap()),
            global.classify(Geodetic::new(latitude, -180.0, 0.0).unwrap())
        );
    }
    let scene = TerrainScene::boundary_only(vec![])
        .unwrap()
        .with_global_land_mask(std::sync::Arc::new(global));
    for point in [
        Geodetic::new(42.0, 50.0, 0.0).unwrap(),
        Geodetic::new(0.0, -140.0, 0.0).unwrap(),
    ] {
        assert_eq!(scene.ground(point).unwrap().hit.geodetic.height_m, 0.0);
        assert!(scene.imported_ground(point).is_none());
    }
}
