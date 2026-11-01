//! Independent analytic shapes and original-OGR query fixtures for the vector index.
use earth_math::{Geodetic, water_mask::vector::GlobalLandVector};

const ROOTS: usize = 360 * 180;
const TARGET: usize = 90 * 360 + 180;

#[test]
fn bundled_original_vector_has_known_land_ocean_hole_and_polar_context() {
    let mask =
        GlobalLandVector::from_bytes(include_bytes!("../../../assets/boundaries/global-land.aev"))
            .unwrap();
    for (latitude, longitude, category) in [
        (40.0, -100.0, 0),
        (0.0, -140.0, 1),
        (42.0, 50.0, 2),
        (-89.0, 0.0, 0),
        (89.0, 0.0, 1),
        (0.0, 179.9, 1),
        (0.0, -179.9, 1),
        (38.829149, -106.415415, 0),
    ] {
        assert_eq!(
            mask.classify(Geodetic::new(latitude, longitude, 0.0).unwrap()),
            Some(category)
        );
    }
    assert_eq!(
        mask.classify(Geodetic::new(0.0, -180.0, 0.0).unwrap()),
        mask.classify(Geodetic::new(0.0, 180.0, 0.0).unwrap())
    );
}

fn synthetic() -> Vec<u8> {
    // Clockwise diamond, counterclockwise square hole, clockwise island, and
    // overlapping clockwise land. Winding must implement union, not XOR.
    let rings = [
        (false, vec![[0.1, 0.5], [0.5, 0.9], [0.9, 0.5], [0.5, 0.1]]),
        (true, vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]]),
        (
            false,
            vec![[0.47, 0.47], [0.47, 0.53], [0.53, 0.53], [0.53, 0.47]],
        ),
        (false, vec![[0.7, 0.4], [0.7, 0.6], [0.9, 0.6], [0.9, 0.4]]),
    ];
    let mut bytes = b"AEGLND02".to_vec();
    for value in [360_u32, 180, ROOTS as u32, 16, 32, 20] {
        bytes.extend(value.to_le_bytes());
    }
    for index in 0..ROOTS {
        for value in [0_u32, 0, if index == TARGET { 16 } else { 0 }, 0, 0] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [0.125_f64, 0.125] {
            bytes.extend(value.to_le_bytes());
        }
    }
    for (hole, ring) in rings {
        for i in 0..ring.len() {
            for value in ring[i].into_iter().chain(ring[(i + 1) % ring.len()]) {
                bytes.extend(f64::to_le_bytes(value));
            }
            bytes.extend(u32::from(hole).to_le_bytes());
        }
    }
    bytes
}

#[test]
fn source_winding_preserves_oblique_edges_holes_islands_and_overlapping_land() {
    let mask = GlobalLandVector::from_bytes(&synthetic()).unwrap();
    for i in 0..101 {
        for j in 0..103 {
            let x = (i as f64 + 0.371) / 101.0;
            let y = (j as f64 + 0.193) / 103.0;
            let diamond = (x - 0.5).abs() + (y - 0.5).abs() < 0.4;
            let hole = (0.4..0.6).contains(&x) && (0.4..0.6).contains(&y);
            let island = (0.47..0.53).contains(&x) && (0.47..0.53).contains(&y);
            let overlap = (0.7..0.9).contains(&x) && (0.4..0.6).contains(&y);
            let expected = if (diamond && !hole) || island || overlap {
                0
            } else if hole {
                2
            } else {
                1
            };
            assert_eq!(
                mask.classify(Geodetic::new(y, x, 0.0).unwrap()),
                Some(expected),
                "x={x}, y={y}"
            );
        }
    }
    for (lat, lon) in [
        (-90.0, 180.0),
        (90.0, -180.0),
        (0.5, 179.999999),
        (0.5, -179.999999),
    ] {
        assert_eq!(
            mask.classify(Geodetic::new(lat, lon, 0.0).unwrap()),
            Some(1)
        );
    }
}

#[test]
fn vector_loader_rejects_bad_topology_bounds_flags_and_seed_contacts() {
    let original = synthetic();
    let changes: Vec<(usize, Vec<u8>)> = vec![
        (0, b"X".to_vec()),
        (24, 33_u32.to_le_bytes().to_vec()),
        (32 + TARGET * 36, (TARGET as u32).to_le_bytes().to_vec()),
        (32 + TARGET * 36 + 8, 33_u32.to_le_bytes().to_vec()),
        (32 + TARGET * 36 + 12, 1_i32.to_le_bytes().to_vec()),
        (32 + TARGET * 36 + 20, f64::NAN.to_le_bytes().to_vec()),
        (32 + ROOTS * 36, 1.01_f64.to_le_bytes().to_vec()),
        (32 + ROOTS * 36 + 32, 2_u32.to_le_bytes().to_vec()),
    ];
    for (offset, replacement) in changes {
        let mut invalid = original.clone();
        invalid[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            GlobalLandVector::from_bytes(&invalid).is_err(),
            "offset={offset}"
        );
    }
    let mut touching = original.clone();
    touching[32 + TARGET * 36 + 20..32 + TARGET * 36 + 28].copy_from_slice(&0.1_f64.to_le_bytes());
    touching[32 + TARGET * 36 + 28..32 + TARGET * 36 + 36].copy_from_slice(&0.5_f64.to_le_bytes());
    assert!(GlobalLandVector::from_bytes(&touching).is_err());
    assert!(GlobalLandVector::from_bytes(&original[..original.len() - 1]).is_err());
}

#[test]
#[ignore = "requires scripts/verify_global_vector.py original-OGR reference generation"]
fn original_ogr_scanlines_match_cpu_queries_across_all_index_leaves() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mask = GlobalLandVector::from_bytes(
        &std::fs::read(root.join("assets/boundaries/global-land.aev")).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(root.join(".cache/global-vector-query-reference.bin")).unwrap();
    assert_eq!(&bytes[..8], b"AEGVQ001");
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 12 + count * 17);
    for record in bytes[12..].chunks_exact(17) {
        let latitude = f64::from_le_bytes(record[..8].try_into().unwrap());
        let longitude = f64::from_le_bytes(record[8..16].try_into().unwrap());
        let actual = mask.classify(Geodetic::new(latitude, longitude, 0.0).unwrap());
        assert_eq!(
            actual,
            Some(record[16]),
            "latitude={latitude}, longitude={longitude}"
        );
    }
    println!("Original OGR scanline reference: {count} CPU queries passed");
}
