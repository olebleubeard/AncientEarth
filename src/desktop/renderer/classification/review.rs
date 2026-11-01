//! Scalar f32 transcription of water_mask.wesl. Actual GPU transcendental,
//! contraction and geometric-normal errors are deliberately outside this test.
#![allow(clippy::excessive_precision)] // Preserve the shader's reviewed literals.
use super::*;
use earth_math::{topography::*, water_mask::*};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

fn geo(lat: f64, lon: f64) -> Geodetic {
    Geodetic::new(lat, lon, 0.0).unwrap()
}
fn angles(origin: Geodetic, target: Geodetic) -> [f32; 2] {
    [
        ((target.lat_deg - origin.lat_deg).to_radians()) as f32,
        ((target.lon_deg - origin.lon_deg + 180.0).rem_euclid(360.0) - 180.0).to_radians() as f32,
    ]
}
fn run_lookup(buffer: &[u32], mut low: usize, mut high: usize, column: u32) -> u32 {
    let end = high;
    for _ in 0..16 {
        if low >= high {
            break;
        }
        let mid = low + (high - low) / 2;
        if column < (buffer[mid] >> 2) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    if low >= end { 255 } else { buffer[low] & 3 }
}
fn native(buffer: &[u32], settings: Settings, angle: [f32; 2]) -> u32 {
    let base = settings.native_origin;
    let latitude =
        base.x as i32 + (base.y + angle[0] * 206264.80624709636_f32 + 0.5).floor() as i32;
    let south = (latitude - 1).div_euclid(3600);
    if !(-90..90).contains(&south) {
        return 255;
    }
    let row = ((south + 1) * 3600 - latitude) as usize;
    let width = match (2 * south + 1).abs() {
        0..100 => 3600,
        100..120 => 2400,
        120..140 => 1800,
        140..160 => 1200,
        160..170 => 720,
        _ => 360,
    };
    let step = 7200 / width;
    let quotient = (base.z as i32).div_euclid(step);
    let remainder = base.z as i32 - quotient * step;
    let k = quotient
        + ((remainder as f32 + base.w + angle[1] * 412529.6124941927_f32) / step as f32 + 0.5)
            .floor() as i32;
    let units = (k * step + 1296000).rem_euclid(2592000) - 1296000;
    let west = units.div_euclid(7200);
    let column = ((units - west * 7200) / step) as u32;
    let header = buffer[((south + 90) * 360 + west + 180) as usize] as usize;
    if header == 0 || buffer[header] != width as u32 {
        return 255;
    }
    run_lookup(
        buffer,
        buffer[header + 1 + row] as usize,
        buffer[header + 2 + row] as usize,
        column,
    )
}
fn global(buffer: &[u32], settings: Settings, angle: [f32; 2]) -> u32 {
    if buffer[0] == VECTOR_TAG {
        return vector(buffer, settings, angle);
    }
    let (width, height) = (buffer[0], buffer[1]);
    if width == 0 || height == 0 {
        return 255;
    }
    let base = settings.global_origin;
    let row = (base.x as i32
        + (base.y - angle[0] * height as f32 / std::f32::consts::PI).floor() as i32)
        .clamp(0, height as i32 - 1) as usize;
    let column = (base.z as i32
        + (base.w + angle[1] * width as f32 / std::f32::consts::TAU).floor() as i32)
        .rem_euclid(width as i32) as u32;
    run_lookup(
        buffer,
        buffer[2 + row] as usize,
        buffer[3 + row] as usize,
        column,
    )
}
fn vector(buffer: &[u32], settings: Settings, angle: [f32; 2]) -> u32 {
    let base = settings.global_origin;
    let residual = [
        base.w + angle[1] * 57.29577951308232_f32,
        base.y + angle[0] * 57.29577951308232_f32,
    ];
    let whole = residual.map(f32::floor);
    let column = (base.z as i32 + whole[0] as i32).rem_euclid(360) as usize;
    let mut row = base.x as i32 + whole[1] as i32;
    let mut point = [residual[0] - whole[0], residual[1] - whole[1]];
    if row >= 180 {
        row = 179;
        point[1] = 1.0;
    }
    if row < 0 {
        row = 0;
        point[1] = 0.0;
    }
    let mut node = row as usize * 360 + column;
    let dot = |a: [f32; 2], b: [f32; 2]| a[0] * b[0] + a[1] * b[1];
    let cross = |a: [f32; 2], b: [f32; 2]| a[0] * b[1] - a[1] * b[0];
    for _ in 0..=20 {
        let record = 3 + node * 8;
        let child = buffer[record] as usize;
        if child != 0 {
            let quadrant = point.map(|value| usize::from(value >= 0.5));
            point = [
                point[0] * 2.0 - quadrant[0] as f32,
                point[1] * 2.0 - quadrant[1] as f32,
            ];
            node = child + quadrant[0] + 2 * quadrant[1];
            continue;
        }
        let mut winding = buffer[record + 3] as i32;
        let mut holes = buffer[record + 4] as i32;
        let seed = [
            f32::from_bits(buffer[record + 5]),
            f32::from_bits(buffer[record + 6]),
        ];
        let direction = [point[0] - seed[0], point[1] - seed[1]];
        let extent = dot(direction, direction);
        if extent > 0.0 {
            for i in 0..buffer[record + 2] as usize {
                let edge = buffer[record + 1] as usize + i * 5;
                let a = [
                    f32::from_bits(buffer[edge]) - seed[0],
                    f32::from_bits(buffer[edge + 1]) - seed[1],
                ];
                let b = [
                    f32::from_bits(buffer[edge + 2]) - seed[0],
                    f32::from_bits(buffer[edge + 3]) - seed[1],
                ];
                let ya = cross(direction, a);
                let yb = cross(direction, b);
                if (ya > 0.0) != (yb > 0.0) {
                    let t = (dot(direction, a) * yb - dot(direction, b) * ya) / (yb - ya);
                    if t > 0.0 && t < extent {
                        let change = if ya > yb { 1 } else { -1 };
                        winding += change;
                        if buffer[edge + 4] != 0 {
                            holes += change;
                        }
                    }
                }
            }
        }
        return if winding < 0 {
            0
        } else if holes > 0 {
            2
        } else {
            1
        };
    }
    255
}

fn vector_scene() -> TerrainScene {
    TerrainScene::boundary_only(vec![])
        .unwrap()
        .with_global_land_mask(Arc::new(
            GlobalLandMask::from_bytes(include_bytes!(
                "../../../../assets/boundaries/global-land.aev"
            ))
            .unwrap(),
        ))
}

#[test]
fn actual_vector_packing_preserves_all_records_and_separated_source_seeds() {
    let scene = vector_scene();
    let packed = Packed::from_scene(&scene);
    let source = scene.global_land_mask().unwrap().vector().unwrap();
    let edge_base = 3 + source.nodes().len() * 8;
    assert_eq!(
        &packed.global[..3],
        &[VECTOR_TAG, source.nodes().len() as u32, edge_base as u32]
    );
    for (i, node) in source.nodes().iter().enumerate() {
        let record = &packed.global[3 + i * 8..3 + (i + 1) * 8];
        assert_eq!(
            record,
            &[
                node.children,
                edge_base as u32 + node.edge_start * 5,
                node.edge_count,
                node.winding as u32,
                node.hole_winding as u32,
                (node.seed[0] as f32).to_bits(),
                (node.seed[1] as f32).to_bits(),
                0
            ]
        );
    }
    for (i, edge) in source.edges().iter().enumerate() {
        assert_eq!(
            &packed.global[edge_base + i * 5..edge_base + (i + 1) * 5],
            &[
                (edge.a[0] as f32).to_bits(),
                (edge.a[1] as f32).to_bits(),
                (edge.b[0] as f32).to_bits(),
                (edge.b[1] as f32).to_bits(),
                u32::from(edge.hole)
            ]
        );
    }
    let origins = [
        geo(29.30624384697, -94.76779608468),
        geo(89.999, 179.999),
        geo(-89.999, -179.999),
    ];
    let mut stack: Vec<_> = (0..180)
        .flat_map(|r| (0..360).map(move |c| (r * 360 + c, c as f64 - 180.0, r as f64 - 90.0, 1.0)))
        .collect();
    let mut comparisons = 0;
    while let Some((i, west, south, size)) = stack.pop() {
        let node = source.nodes()[i];
        if node.children != 0 {
            for child in 0..4 {
                stack.push((
                    node.children as usize + child,
                    west + (child & 1) as f64 * size / 2.0,
                    south + (child >> 1) as f64 * size / 2.0,
                    size / 2.0,
                ));
            }
            continue;
        }
        let point = geo(south + node.seed[1] * size, west + node.seed[0] * size);
        let expected = scene.global_land_mask().unwrap().classify(point).unwrap() as u32;
        for origin in origins {
            let settings = Settings::at(&scene, origin, true);
            assert_eq!(
                global(&packed.global, settings, angles(origin, point)),
                expected,
                "origin={origin:?}, point={point:?}, leaf={i}"
            );
            comparisons += 1;
        }
    }
    println!("Packed vector scalar f32: {comparisons} separated leaf-seed queries passed");
    for point in [
        geo(42.0, 50.0),
        geo(40.0, -100.0),
        geo(0.0, 180.0),
        geo(90.0, 0.0),
    ] {
        let settings = Settings::at(&scene, point, true);
        assert_eq!(
            global(&packed.global, settings, [0.0; 2]),
            scene.global_land_mask().unwrap().classify(point).unwrap() as u32,
            "point={point:?}"
        );
        assert!(!additional(
            &packed,
            Settings::at(&scene, point, false),
            [0.0; 2]
        ));
    }
    // The source closes Antarctica along the map-domain pole edge. Exact-edge
    // ownership is a numerical convention, not a CPU/GPU category guarantee.
    let pole = geo(-90.0, 0.0);
    assert!(global(&packed.global, Settings::at(&scene, pole, true), [0.0; 2]) <= 2);
}

fn source_clearance_degrees(source: &vector::GlobalLandVector, point: Geodetic) -> f64 {
    let x = (point.lon_deg + 180.0).rem_euclid(360.0);
    let y = point.lat_deg + 90.0;
    let column = x.floor() as usize;
    let row = (y.floor() as usize).min(179);
    let mut q = [x - column as f64, y - row as f64];
    let mut index = row * 360 + column;
    let mut size = 1.0;
    loop {
        let node = source.nodes()[index];
        if node.children != 0 {
            let east = usize::from(q[0] >= 0.5);
            let north = usize::from(q[1] >= 0.5);
            q = [q[0] * 2.0 - east as f64, q[1] * 2.0 - north as f64];
            index = node.children as usize + east + 2 * north;
            size *= 0.5;
            continue;
        }
        return source.edges()
            [node.edge_start as usize..(node.edge_start + node.edge_count) as usize]
            .iter()
            .map(|edge| {
                let d = [edge.b[0] - edge.a[0], edge.b[1] - edge.a[1]];
                let v = [q[0] - edge.a[0], q[1] - edge.a[1]];
                let t = ((v[0] * d[0] + v[1] * d[1]) / (d[0] * d[0] + d[1] * d[1])).clamp(0.0, 1.0);
                (v[0] - t * d[0]).hypot(v[1] - t * d[1]) * size
            })
            .fold(f64::INFINITY, f64::min);
    }
}

#[test]
#[ignore = "requires original-OGR query fixture from scripts/verify_global_vector.py"]
fn original_vector_source_queries_match_packed_f32_at_local_origins() {
    let scene = vector_scene();
    let packed = Packed::from_scene(&scene);
    let raw = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/global-vector-query-reference.bin"),
    )
    .unwrap();
    assert_eq!(&raw[..8], b"AEGVQ001");
    let count = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
    assert_eq!(raw.len(), 12 + count * 17);
    let mut boundary_differences = 0;
    let mut maximum_difference_clearance = 0.0_f64;
    for entry in raw[12..].chunks_exact(17) {
        let point = geo(
            f64::from_le_bytes(entry[..8].try_into().unwrap()),
            f64::from_le_bytes(entry[8..16].try_into().unwrap()),
        );
        let actual = global(&packed.global, Settings::at(&scene, point, true), [0.0; 2]);
        if actual != entry[16] as u32 {
            let clearance = source_clearance_degrees(
                scene.global_land_mask().unwrap().vector().unwrap(),
                point,
            );
            assert!(
                clearance <= 1e-6,
                "point={point:?}, actual={actual}, expected={}, source-edge clearance={clearance}",
                entry[16]
            );
            boundary_differences += 1;
            maximum_difference_clearance = maximum_difference_clearance.max(clearance);
        }
    }
    println!(
        "Packed vector scalar f32: {count} independent OGR queries; {boundary_differences} finite source-edge differences, maximum clearance {maximum_difference_clearance:e} degrees"
    );
}
fn additional(packed: &Packed, settings: Settings, angle: [f32; 2]) -> bool {
    if settings.control.x == 0 {
        return false;
    }
    let code = native(&packed.native, settings, angle);
    let code = if code == 255 {
        global(&packed.global, settings, angle)
    } else {
        code
    };
    (1..=3).contains(&code)
}

fn global_scene() -> TerrainScene {
    TerrainScene::boundary_only(vec![])
        .unwrap()
        .with_global_land_mask(Arc::new(
            GlobalLandMask::from_bytes(include_bytes!(
                "../../../../assets/boundaries/global-land.ael"
            ))
            .unwrap(),
        ))
}

#[test]
fn actual_global_buffer_packing_and_split_origins_preserve_cell_centers_and_disable() {
    let scene = global_scene();
    let packed = Packed::from_scene(&scene);
    let source = scene.global_land_mask().unwrap();
    let offset = 2 + source.row_offsets().len();
    assert_eq!(&packed.global[offset..], source.runs());
    for (actual, &expected) in packed.global[2..offset].iter().zip(source.row_offsets()) {
        assert_eq!(*actual, expected + offset as u32);
    }
    let origins = [
        geo(29.3062438469716, -94.7677960846823),
        geo(89.999, 179.999),
        geo(-89.999, -179.999),
        geo(0.0, 0.0),
    ];
    let mut comparisons = 0;
    for origin in origins {
        let settings = Settings::at(&scene, origin, true);
        assert!(
            settings.native_origin.x.abs() < 16_777_216.0
                && settings.native_origin.z.abs() < 16_777_216.0
        );
        for i in 0..4096 {
            // Deterministic source-cell centers keep an independent half-cell
            // margin; this does not claim equality at f32 boundary ambiguity.
            let row = (i * 3571 + 79) % 18000;
            let column = (i * 7919 + 123) % 36000;
            let target = geo(
                90.0 - (row as f64 + 0.5) / 100.0,
                -180.0 + (column as f64 + 0.5) / 100.0,
            );
            let angle = angles(origin, target);
            let expected = u32::from(source.classify(target).unwrap());
            assert_eq!(
                global(&packed.global, settings, angle),
                expected,
                "origin{origin:?}, target{target:?}"
            );
            assert_eq!(
                additional(&packed, settings, angle),
                (1..=3).contains(&expected)
            );
            assert!(!additional(
                &packed,
                Settings::at(&scene, origin, false),
                angle
            ));
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 16384);
    let empty = Packed::empty();
    assert_eq!(native(&empty.native, Settings::default(), [0.0, 0.0]), 255);
    assert_eq!(global(&empty.global, Settings::default(), [0.0, 0.0]), 255);
}

// Independent synthetic source layout: four stripes at known row/column posts.
fn synthetic_native() -> Packed {
    let mut packed = Packed::empty();
    for (south, west, width) in [
        (49, 0, 3600),
        (50, 0, 2400),
        (-51, 0, 2400),
        (-50, 0, 3600),
        (59, 0, 2400),
        (60, 0, 1800),
        (0, 179, 3600),
        (0, -180, 3600),
        (0, 0, 3600),
        (-1, 0, 3600),
        (89, 0, 360),
    ] {
        let header = packed.native.len();
        let run_base = header + 3602;
        packed.native[((south + 90) * 360 + west + 180) as usize] = header as u32;
        packed.native.push(width);
        for row in 0..=3600 {
            packed.native.push((run_base + row * 4) as u32);
        }
        for row in 0..3600 {
            for (end, code) in [
                (1, row % 4),
                (2, (row + 1) % 4),
                (width - 1, 2),
                (width, (row + 3) % 4),
            ] {
                packed.native.push((end << 2) | code);
            }
        }
    }
    packed
}

#[test]
fn scalar_native_shader_uses_real_latitude_band_neighbor_and_dateline_ownership() {
    let scene = global_scene();
    let packed = synthetic_native();
    let arc = 1.0 / 3600.0;
    let cases = [
        (geo(50.0 + 0.4 * arc, 0.6 * arc), 1),
        (geo(50.0 + 0.6 * arc, 0.6 * arc), 3),
        (geo(-50.0 + 0.4 * arc, 0.8 * arc), 1),
        (geo(-50.0 + 0.6 * arc, 0.6 * arc), 0),
        (geo(60.0 + 0.4 * arc, 0.9 * arc), 1),
        (geo(60.0 + 0.6 * arc, 0.9 * arc), 3),
        (geo(1.0, 180.0), 0),
        (geo(1.0, -180.0), 0),
        (geo(1.0, 180.0 - 0.4 * arc), 0),
        (geo(1.0, 180.0 - 0.6 * arc), 3),
        (geo(0.4 * arc, 0.0), 0),
        (geo(0.6 * arc, 0.0), 3),
        (geo(1.0, 1.0 - 0.4 * arc), 255),
        (geo(-90.0, 0.0), 255),
        (geo(90.0, 0.0), 0),
    ];
    for origin in [
        geo(0.0, 0.0),
        geo(29.3062438469716, -94.7677960846823),
        geo(-50.25, 179.0),
    ] {
        for &(point, expected) in &cases {
            assert_eq!(
                native(
                    &packed.native,
                    Settings::at(&scene, origin, true),
                    angles(origin, point)
                ),
                expected,
                "{origin:?} -> {point:?}"
            );
        }
    }
    // At the anchor, exact dyadic half-post fractions retain north/east ties.
    for (point, expected) in [
        (geo(0.5 * arc, 0.5 * arc), 0),
        (geo(-0.5 * arc, -0.5 * arc), 0),
    ] {
        assert_eq!(
            native(
                &packed.native,
                Settings::at(&scene, point, true),
                [0.0, 0.0]
            ),
            expected
        );
    }
    // Native land must override global ocean, and native water must override
    // global land. The hierarchy is not a union of independently wet pixels.
    let mut packed = packed;
    for (point, global_code, expected) in
        [(geo(1.0, 0.0), 1, false), (geo(1.0, 2.0 * arc), 0, true)]
    {
        packed.global = vec![2, 1, 4, 5, (2 << 2) | global_code];
        let settings = Settings {
            global_origin: Vec4::ZERO,
            ..Settings::at(&scene, point, true)
        };
        assert_eq!(additional(&packed, settings, [0.0, 0.0]), expected);
    }
}

#[test]
#[ignore = "requires reviewed cached original WBM files; independent GDAL whole-source checks are in earth-math"]
fn actual_native_packed_addresses_and_f32_queries_match_all_source_run_endpoints() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    for (name, south, west) in [
        ("N29_00_W095_00", 29, -95),
        ("N29_00_W096_00", 29, -96),
        ("N35_00_E138_00", 35, 138),
        ("N35_00_E139_00", 35, 139),
    ] {
        let center = geo(f64::from(south) + 0.5, f64::from(west) + 0.5);
        let tile = copernicus_tiles(&DownloadRegion::new(center, 1.0).unwrap())
            .unwrap()
            .into_iter()
            .find(|t| t.core)
            .unwrap();
        sources.push((tile, root.join(format!(".cache/water/{name}-WBM.tif"))));
    }
    let masks = WaterMask::from_sources(&sources, &AtomicBool::new(false)).unwrap();
    let scene = global_scene().with_water_masks(Arc::new(masks));
    let packed = Packed::from_scene(&scene);
    let origin = geo(29.3062438469716, -94.7677960846823);
    let settings = Settings::at(&scene, origin, true);
    let mut checks = 0;
    for tile in scene.water_masks().unwrap().tiles() {
        let header =
            packed.native[((tile.south_deg + 90) * 360 + tile.west_deg + 180) as usize] as usize;
        let run_base = header + 1 + tile.row_offsets().len();
        assert_eq!(
            &packed.native[run_base..run_base + tile.runs().len()],
            tile.runs()
        );
        for (actual, &expected) in packed.native[header + 1..run_base]
            .iter()
            .zip(tile.row_offsets())
        {
            assert_eq!(*actual, expected + run_base as u32);
        }
        for row in 0..3600 {
            let mut column = 0;
            for &run in
                &tile.runs()[tile.row_offsets()[row] as usize..tile.row_offsets()[row + 1] as usize]
            {
                let end = run >> 2;
                let expected = run & 3;
                for c in [column, end - 1] {
                    let point = geo(
                        f64::from(tile.south_deg + 1) - row as f64 / 3600.0,
                        f64::from(tile.west_deg) + f64::from(c) / tile.width as f64,
                    );
                    assert_eq!(
                        native(&packed.native, settings, angles(origin, point)),
                        expected,
                        "{point:?}"
                    );
                    checks += 1;
                }
                column = end;
            }
        }
    }
    assert_eq!(checks, 527678);
}
