//! Independent categorical reconstruction and physical-invariance checks.
use super::*;
use crate::{EnuBasis, Ray, SurfaceKind, TerrainPatch, TerrainScene, geodetic_to_ecef};
use std::sync::Arc;

fn geo(lat: f64, lon: f64) -> Geodetic {
    Geodetic::new(lat, lon, 0.0).unwrap()
}

// Four known source stripes, with row-dependent edge codes. Interior stripe2
// is constant. Values are synthetic source observations, never interpolated.
fn tile(south: i32, west: i32) -> WaterMaskTile {
    let width = crate::topography::copernicus::native_width(south);
    let mut runs = Vec::new();
    let mut offsets = vec![0];
    for row in 0..3600 {
        for (end, code) in [
            (1, row % 4),
            (2, (row + 1) % 4),
            (width - 1, 2),
            (width, (row + 3) % 4),
        ] {
            runs.push(((end as u32) << 2) | code as u32);
        }
        offsets.push(runs.len() as u32);
    }
    WaterMaskTile {
        south_deg: south,
        west_deg: west,
        width,
        offsets,
        runs,
    }
}

fn mask(positions: &[(i32, i32)]) -> WaterMask {
    let tiles: Vec<_> = positions.iter().map(|&(s, w)| tile(s, w)).collect();
    let mut index = vec![-1; 180 * 360];
    for (i, tile) in tiles.iter().enumerate() {
        index[(tile.south_deg + 90) as usize * 360 + (tile.west_deg + 180) as usize] = i as i32;
    }
    WaterMask { tiles, index }
}

fn global_bytes(width: u32, rows: &[Vec<u8>]) -> Vec<u8> {
    let mut result = b"AEGLND01".to_vec();
    result.extend(width.to_le_bytes());
    result.extend((rows.len() as u32).to_le_bytes());
    result.extend((width * rows.len() as u32).to_le_bytes());
    for row in 0..=rows.len() {
        result.extend((row as u32 * width).to_le_bytes());
    }
    for row in rows {
        assert_eq!(row.len(), width as usize);
        for (column, &code) in row.iter().enumerate() {
            result.extend(((((column + 1) as u32) << 2) | u32::from(code)).to_le_bytes());
        }
    }
    result
}

#[test]
fn native_post_ownership_is_latitude_first_at_band_seams_and_missing_neighbors() {
    let field = mask(&[
        (49, 0),
        (50, 0),
        (-51, 0),
        (-50, 0),
        (59, 0),
        (60, 0),
        (0, 179),
        (0, -180),
        (0, 0),
        (-1, 0),
    ]);
    let arc = 1.0 / 3600.0;
    // At50° the nearest latitude post still belongs to the SOUTHERN row's
    // 1" longitude lattice; 0.6" longitude therefore selects column1.
    assert_eq!(field.classify(geo(50.0 + 0.4 * arc, 0.6 * arc)), Some(1));
    // One row north belongs to the 1.5" lattice: column0, last source row.
    assert_eq!(field.classify(geo(50.0 + 0.6 * arc, 0.6 * arc)), Some(3));
    assert_eq!(field.classify(geo(-50.0 + 0.4 * arc, 0.8 * arc)), Some(1));
    assert_eq!(field.classify(geo(-50.0 + 0.6 * arc, 0.6 * arc)), Some(0));
    assert_eq!(field.classify(geo(60.0 + 0.4 * arc, 0.9 * arc)), Some(1));
    assert_eq!(field.classify(geo(60.0 + 0.6 * arc, 0.9 * arc)), Some(3));
    // Exact ties choose north/east, including negative geographic coordinates.
    assert_eq!(field.classify(geo(0.5 * arc, 0.5 * arc)), Some(0));
    assert_eq!(field.classify(geo(-0.5 * arc, -0.5 * arc)), Some(0));
    assert_eq!(field.classify(geo(0.0, 0.0)), Some(0));
    assert_eq!(field.classify(geo(0.4 * arc, 0.0)), Some(0));
    assert_eq!(field.classify(geo(0.6 * arc, 0.0)), Some(3));
    // Round through the dateline to the actual eastern neighbor's first post.
    for longitude in [180.0, -180.0, 180.0 - 0.4 * arc] {
        assert_eq!(field.classify(geo(1.0, longitude)), Some(0));
    }
    assert_eq!(field.classify(geo(1.0, 180.0 - 0.6 * arc)), Some(3));
    let lone = mask(&[(0, 0), (89, 0), (-90, 0)]);
    assert_eq!(lone.classify(geo(1.0, 1.0 - 0.4 * arc)), None);
    assert_eq!(lone.classify(geo(0.4 * arc, 0.0)), None);
    assert_eq!(lone.classify(geo(0.6 * arc, 0.0)), Some(3));
    assert_eq!(lone.classify(geo(-90.0, 0.0)), None);
    assert_eq!(lone.classify(geo(90.0, 0.0)), Some(0));
    assert_eq!(
        lone.classify(Geodetic {
            lat_deg: f64::NAN,
            lon_deg: 0.0,
            height_m: 0.0
        }),
        None
    );
    assert!(matches!(
        WaterMask::from_sources(&[], &AtomicBool::new(true)),
        Err(TopographyError::Cancelled)
    ));
}

#[test]
fn global_context_has_explicit_cell_ownership_and_rejects_malformed_run_streams() {
    let bytes = global_bytes(4, &[vec![0, 1, 2, 0], vec![2, 0, 1, 2]]);
    let field = GlobalLandMask::from_bytes(&bytes).unwrap();
    for (lat, expected) in [(45.0, [0, 1, 2, 0]), (-45.0, [2, 0, 1, 2])] {
        for (lon, code) in [-135.0, -45.0, 45.0, 135.0].into_iter().zip(expected) {
            assert_eq!(field.classify(geo(lat, lon)), Some(code));
        }
    }
    assert_eq!(field.classify(geo(90.0, 180.0)), Some(0));
    assert_eq!(field.classify(geo(-90.0, -180.0)), Some(2));
    assert_eq!(field.classify(geo(0.0, 0.0)), Some(1));
    for length in [0, 7, 19, bytes.len() - 1] {
        assert!(GlobalLandMask::from_bytes(&bytes[..length]).is_err());
    }
    // Width, count, offsets, category and cumulative run ends are all checked.
    for (offset, value) in [
        (8, 0_u32),
        (8, 6),
        (16, u32::MAX),
        (20, 1),
        (24, 0),
        (24, 9),
        (32, 7),
        (36, 4),
        (44, 12),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            GlobalLandMask::from_bytes(&bad).is_err(),
            "offset{offset}, value{value}"
        );
    }
}

fn unresolved_boundary() -> TerrainPatch {
    let mut bytes = b"AEBNDR01".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    bytes.extend(2_u32.to_le_bytes());
    for value in [0.4_f64, 0.6, 0.2, 0.2] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(4_u32.to_le_bytes());
    bytes.extend(2_u32.to_le_bytes());
    for [row, column] in [[0.0_f64, 0.0], [0.0, 3.0], [3.0, 0.0], [3.0, 3.0]] {
        bytes.extend(row.to_le_bytes());
        bytes.extend(column.to_le_bytes());
    }
    for indices in [[0_u32, 2, 3], [0, 3, 1]] {
        for index in indices {
            bytes.extend(index.to_le_bytes());
        }
        bytes.push(4);
    }
    TerrainPatch::from_boundary_bytes(&bytes).unwrap()
}

#[test]
fn category_precedence_changes_neither_analytic_nor_imported_physical_geometry() {
    let base = TerrainScene::boundary_only(vec![unresolved_boundary()]).unwrap();
    let enriched = base
        .clone()
        .with_water_masks(Arc::new(mask(&[(0, 0)])))
        .with_global_land_mask(Arc::new(
            GlobalLandMask::from_bytes(&global_bytes(2, &[vec![0, 0]])).unwrap(),
        ));
    for (point, code) in [(geo(0.5, 0.5), 4), (geo(0.75, 0.5), 2), (geo(1.5, 0.5), 0)] {
        assert_eq!(enriched.category_code(point), Some(code));
        let a = base.ground(point).unwrap();
        let b = enriched.ground(point).unwrap();
        assert_eq!(a.hit.position, b.hit.position);
        assert_eq!(b.hit.geodetic.height_m, 0.0);
        assert_eq!(b.grass_allowed, code == 0);
        let normal = EnuBasis::at(point).up;
        let ray = Ray {
            origin: geodetic_to_ecef(point) + normal * 1000.0,
            direction: -normal * 2.0,
        };
        let a = base.intersect(ray).unwrap().hit;
        let b = enriched.intersect(ray).unwrap().hit;
        assert_eq!(a.distance_m, b.distance_m);
        assert_eq!(a.position, b.position);
        assert_eq!(a.normal, b.normal);
    }
    let patch =
        TerrainPatch::from_grid(2, 2, 0.49, 0.76, 0.02, 0.02, vec![-35.0, 100.0, 20.0, 65.0])
            .unwrap();
    let original = patch.canonical_vertices().to_vec();
    let plain = TerrainScene::with_terrain(vec![patch.clone()], Arc::new(base)).unwrap();
    let classified = TerrainScene::with_terrain(vec![patch], Arc::new(enriched)).unwrap();
    assert_eq!(classified.patches()[0].canonical_vertices(), original);
    for point in [geo(0.749, 0.5), geo(0.755, 0.493), geo(2.0, 0.5)] {
        let a = plain.ground(point).unwrap();
        let b = classified.ground(point).unwrap();
        assert_eq!(a.hit.position, b.hit.position);
        assert_eq!(a.hit.geodetic.height_m, b.hit.geodetic.height_m);
        if point.lat_deg == 2.0 {
            assert_eq!(b.kind, SurfaceKind::ReferenceEllipsoid);
            assert_eq!(b.hit.geodetic.height_m, 0.0);
            assert!(classified.imported_ground(point).is_none());
        } else {
            assert!(matches!(b.kind, SurfaceKind::Water { .. }));
        }
    }
}
