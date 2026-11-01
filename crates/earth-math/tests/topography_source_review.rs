//! Explicit cached-source audit; source bytes/coordinates are independently
//! pinned by scripts/verify_copernicus_import.py and decoded with GDAL there.
use earth_math::{topography::*, *};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
#[path = "fixtures/copernicus_source.rs"]
mod reference;

#[test]
#[ignore = "requires reviewed N29W095/N29W096/N35E138 TIFFs in .cache/terrain; run explicitly with --ignored"]
fn cached_official_native_posts_survive_decode_decimation_and_h_plus_n() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let boundaries = Arc::new(TerrainScene::boundary_only(vec![]).unwrap());
    let geoid = GeoidGrid::from_grid(4, 3, vec![-30.0; 12]).unwrap();
    for name in ["N29W095", "N35E138"] {
        let rows: Vec<_> = reference::POSTS.iter().filter(|x| x.0 == name).collect();
        let first = rows[0];
        let region = DownloadRegion::new(
            Geodetic::new(f64::from(first.1) + 0.5, f64::from(first.2) + 0.5, 0.0).unwrap(),
            1.0,
        )
        .unwrap();
        let plan = copernicus_tiles(&region).unwrap();
        let tile = plan.iter().find(|tile| tile.core).unwrap().clone();
        let path = root.join(format!(".cache/terrain/{name}.tif"));
        assert!(path.exists(), "Missing reviewed source: {}", path.display());
        let (scene, summary) = import_sources(
            &region,
            &[(tile, path)],
            &geoid,
            boundaries.clone(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(summary.imported_tiles, 1);
        assert_eq!(summary.missing_tiles, 3);
        assert_eq!(summary.retained_spacing_arc_seconds, 8.0);
        let patch = &scene.patches()[0];
        let (width, height, vertices) = patch.presentation_grid();
        assert_eq!((width, height), (450, 450));
        assert_eq!(patch.canonical_vertices().len(), summary.retained_vertices);
        for &&(_, _, _, row, column, latitude, longitude, h) in &rows {
            let actual = ecef_to_geodetic(vertices[(row / 8) * width + column / 8]).unwrap();
            assert!((actual.lat_deg - latitude).abs() < 2e-12);
            assert!((actual.lon_deg - longitude).abs() < 2e-12);
            assert!(
                (actual.height_m - (h - 30.0)).abs() < 4e-8,
                "{name} [{row},{column}] {actual:?}"
            );
        }
        // Missing east/south neighbor posts leave an explicit fallback strip;
        // they must not be fabricated by duplicating the last source sample.
        let missing = Geodetic::new(
            f64::from(first.1) + 0.0001,
            f64::from(first.2) + 0.9999,
            0.0,
        )
        .unwrap();
        assert_eq!(
            scene.ground(missing).unwrap().kind,
            SurfaceKind::ReferenceEllipsoid
        );
        // Input cancellation is transactional and cannot publish partial tiles.
        assert!(matches!(
            import_sources(
                &region,
                &[],
                &geoid,
                boundaries.clone(),
                &AtomicBool::new(true)
            ),
            Err(TopographyError::Cancelled)
        ));
    }
    let seam_region = DownloadRegion::new(Geodetic::new(29.5, -95.0, 0.0).unwrap(), 1.0).unwrap();
    let sources: Vec<_> = copernicus_tiles(&seam_region)
        .unwrap()
        .into_iter()
        .filter(|tile| tile.core)
        .map(|tile| {
            let name = if tile.west_deg == -96 {
                "N29W096"
            } else {
                "N29W095"
            };
            (tile, root.join(format!(".cache/terrain/{name}.tif")))
        })
        .collect();
    let (scene, _) = import_sources(
        &seam_region,
        &sources,
        &geoid,
        boundaries,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(scene.patches().len(), 2);
    let (ww, wh, west) = scene.patches()[0].presentation_grid();
    let (ew, eh, east) = scene.patches()[1].presentation_grid();
    assert_eq!(wh, eh);
    assert_eq!(ww, ew + 1);
    for row in 0..wh {
        // Bit-identical source posts restored from the adjacent tile, not
        // independently extrapolated duplicate edge elevations.
        assert_eq!(west[row * ww + ww - 1], east[row * ew]);
    }
    let all_seam = |patch: &TerrainPatch| -> std::collections::BTreeSet<[u64; 3]> {
        patch
            .canonical_vertices()
            .iter()
            .filter_map(|&p| {
                let g = ecef_to_geodetic(p).unwrap();
                ((g.lon_deg + 95.0).abs() < 1e-11).then_some([
                    p.x.to_bits(),
                    p.y.to_bits(),
                    p.z.to_bits(),
                ])
            })
            .collect()
    };
    let complete_west = all_seam(&scene.patches()[0]);
    assert_eq!(complete_west, all_seam(&scene.patches()[1]));
    assert!(
        complete_west.len() > wh,
        "fixture exercises added fine boundary posts too"
    );
}

#[test]
fn tile_plans_wrap_the_dateline_include_support_and_bound_resource_requests() {
    let region = DownloadRegion::new(Geodetic::new(10.5, 179.999, 0.0).unwrap(), 1000.0).unwrap();
    let plan = copernicus_tiles(&region).unwrap();
    assert!(plan.iter().any(|x| x.core && x.west_deg == 179));
    assert!(plan.iter().any(|x| x.core && x.west_deg == -180));
    for tile in plan.iter().filter(|tile| tile.core) {
        let east = if tile.west_deg == 179 {
            -180
        } else {
            tile.west_deg + 1
        };
        assert!(
            plan.iter()
                .any(|x| x.south_deg == tile.south_deg && x.west_deg == east)
        );
        assert!(
            plan.iter()
                .any(|x| x.south_deg == tile.south_deg - 1 && x.west_deg == tile.west_deg)
        );
        assert!(tile.dem_url.ends_with(&format!("{}.tif", tile.id)));
    }
    let mut oversized = region.clone();
    oversized.radius_m = 1e9;
    assert!(matches!(
        copernicus_tiles(&oversized),
        Err(TopographyError::InvalidRadius)
    ));
    let polar =
        DownloadRegion::new(Geodetic::new(90.0, 0.0, 0.0).unwrap(), DEFAULT_RADIUS_M).unwrap();
    assert!(matches!(
        copernicus_tiles(&polar),
        Err(TopographyError::TooManyTiles)
    ));
}

#[test]
fn imported_display_retains_all_canonical_triangles_at_ground_and_orbit() {
    use earth_math::rendering::{LocalFrame, imported_scene_meshes};
    let west = TerrainPatch::from_grid(
        3,
        3,
        0.0,
        0.01,
        0.005,
        0.005,
        vec![-30.0, 10.0, 40.0, -20.0, 20.0, 50.0, -10.0, 30.0, 60.0],
    )
    .unwrap();
    let east = TerrainPatch::from_grid(
        3,
        3,
        0.01,
        0.01,
        0.005,
        0.005,
        vec![40.0, 10.0, 0.0, 50.0, 20.0, 10.0, 60.0, 30.0, 20.0],
    )
    .unwrap();
    let scene = TerrainScene::with_terrain(
        vec![west, east],
        Arc::new(TerrainScene::boundary_only(vec![]).unwrap()),
    )
    .unwrap();
    for height in [100.0, 8_000_000.0] {
        let observer = Observer::new(Geodetic::new(0.005, 0.01, height).unwrap());
        let local = LocalFrame::at(observer);
        let (_, mesh) = imported_scene_meshes(&scene, local, 500.0);
        assert_eq!(mesh.indices.len(), 16 * 3);
        let key = |p: [f32; 3]| p.map(f32::to_bits);
        let mut expected = std::collections::BTreeSet::new();
        for patch in scene.patches() {
            let (width, height, vertices) = patch.presentation_grid();
            for row in 0..height - 1 {
                for col in 0..width - 1 {
                    let nw = row * width + col;
                    let ne = nw + 1;
                    let sw = nw + width;
                    let se = sw + 1;
                    for indices in [[nw, sw, se], [nw, se, ne]] {
                        expected.insert(indices.map(|i| key(local.position(vertices[i]))));
                    }
                }
            }
        }
        let actual: std::collections::BTreeSet<_> = mesh
            .indices
            .chunks_exact(3)
            .map(|face| {
                [
                    key(mesh.positions[face[0] as usize]),
                    key(mesh.positions[face[1] as usize]),
                    key(mesh.positions[face[2] as usize]),
                ]
            })
            .collect();
        assert_eq!(actual, expected);
    }
    let location = Geodetic::new(0.0075, 0.001, 0.0).unwrap();
    let ground = scene.ground(location).unwrap();
    assert!(ground.hit.geodetic.height_m < 0.0);
    let mut observer = Observer::new(location);
    scene
        .try_teleport(
            &mut observer,
            location
                .with_height(ground.hit.geodetic.height_m + 0.5)
                .unwrap(),
        )
        .unwrap();
    let smooth = TerrainScene::boundary_only(vec![]).unwrap();
    let before = observer;
    let lift = smooth.ensure_observer_above_surface(&mut observer).unwrap();
    assert!(lift > 0.0);
    assert_eq!(observer.feet.lat_deg, before.feet.lat_deg);
    assert_eq!(observer.feet.lon_deg, before.feet.lon_deg);
    assert_eq!(observer.feet.height_m, 0.0);
    assert_eq!(observer.frame().forward, before.frame().forward);
}

#[test]
fn inspection_pose_uses_actual_retained_peak_and_imported_ground() {
    for base_height in [20.0, 3500.0] {
        let peak_height = base_height + 130.0;
        let patch = TerrainPatch::from_grid(5, 5, -0.01, 0.01, 0.005, 0.005, {
            let mut heights = vec![base_height; 25];
            heights[12] = peak_height;
            heights
        })
        .unwrap();
        // This taller patch begins >3.3km east in the equatorial plane. It
        // must not displace the nearby target merely because its height wins.
        let distant =
            TerrainPatch::from_grid(3, 3, 0.03, 0.005, 0.005, 0.005, vec![9000.0; 9]).unwrap();
        let scene = TerrainScene::with_terrain(
            vec![patch, distant],
            Arc::new(TerrainScene::boundary_only(vec![]).unwrap()),
        )
        .unwrap();
        let center = Geodetic::new(0.0, 0.0, 0.0).unwrap();
        let observer = inspection_observer(&scene, center).unwrap();
        let ground = scene.imported_ground(observer.feet).unwrap();
        assert!((observer.feet.height_m - ground.hit.geodetic.height_m - 70.0).abs() < 1e-8);
        let target = geodetic_to_ecef(center.with_height(peak_height).unwrap());
        let expected = (target - observer.eye_position()).normalized().unwrap();
        assert!(observer.frame().forward.distance(expected) < 3e-15);
        if base_height > 2000.0 {
            assert!(
                target.distance(geodetic_to_ecef(center)) > 2000.0,
                "mountain fixture distinguishes geographic radius from old 3D radius"
            );
        }
        assert!(matches!(
            inspection_observer(&scene, Geodetic::new(10.0, 10.0, 0.0).unwrap()),
            Err(TopographyError::MissingData)
        ));
    }
}

/// Explicit opt-in: reads the completed user cache without changing it. The
/// independent GDAL/PROJ verifier consumes the exact imported Cartesian mesh.
#[test]
#[ignore = "requires ANCIENT_EARTH_TOPOGRAPHY_CACHE pointing at the reviewed completed 100mi source cache"]
fn cached_hundred_mile_selection_exports_refinement_for_independent_source_and_topology_audit() {
    use std::{collections::BTreeMap, io::Write};
    let cache = PathBuf::from(
        std::env::var_os("ANCIENT_EARTH_TOPOGRAPHY_CACHE")
            .expect("explicit read-only source cache"),
    );
    let center = Geodetic::new(29.305549, -94.767015, 0.0).unwrap();
    let region = DownloadRegion::new(center, DEFAULT_RADIUS_M).unwrap();
    let sources: Vec<_> = copernicus_tiles(&region)
        .unwrap()
        .into_iter()
        .filter_map(|tile| {
            let path = cache.join("assets").join(format!("{}.tif", tile.id));
            path.exists().then_some((tile, path))
        })
        .collect();
    assert!(!sources.is_empty());
    // Isolate source decoding/registration from the already independently
    // tested native geoid. Every resulting h must equal original H minus30m.
    let geoid = GeoidGrid::from_grid(4, 3, vec![-30.0; 12]).unwrap();
    let (scene, summary) = import_sources(
        &region,
        &sources,
        &geoid,
        Arc::new(TerrainScene::boundary_only(vec![]).unwrap()),
        &AtomicBool::new(false),
    )
    .unwrap();
    println!("{}", summary.description());
    assert_eq!(summary.retained_spacing_arc_seconds, 30.0);
    assert_eq!(summary.local_spacing_arc_seconds, 1.0);
    assert!(summary.local_detail_radius_m > 0.0 && summary.local_detail_radius_m <= 5000.0);
    assert!(summary.local_added_vertices > 0 && summary.local_added_vertices <= 200_000);
    assert!(summary.retained_vertices <= MAX_RETAINED_VERTICES);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/topography-refinement-review.bin");
    let mut output = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    output.write_all(b"REFAUD01").unwrap();
    output
        .write_all(&(scene.patches().len() as u32).to_le_bytes())
        .unwrap();
    let key = |p: Vec3d| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    let mut selected_peak = None;
    for patch in scene.patches() {
        let (width, height, _) = patch.presentation_grid();
        let vertices = patch.canonical_vertices();
        let indices: BTreeMap<_, _> = vertices
            .iter()
            .enumerate()
            .map(|(i, &p)| (key(p), i as u32))
            .collect();
        assert_eq!(indices.len(), vertices.len());
        let faces: Vec<_> = patch
            .presentation_triangles()
            .map(|(p, _)| p.map(|p| indices[&key(p)]))
            .collect();
        for count in [width, height, vertices.len(), faces.len()] {
            output.write_all(&(count as u32).to_le_bytes()).unwrap();
        }
        for &p in vertices {
            for x in [p.x, p.y, p.z] {
                output.write_all(&x.to_le_bytes()).unwrap();
            }
            let g = ecef_to_geodetic(p).unwrap();
            if (g.lat_deg - (30.0 - 2495.0 / 3600.0)).abs() < 1e-11
                && (g.lon_deg - (-95.0 + 820.0 / 3600.0)).abs() < 1e-11
            {
                // GDAL-decoded source native post omitted by the former30arcsec grid.
                assert!((g.height_m - (17.913875579833984 - 30.0)).abs() < 4e-8);
                selected_peak = Some(g);
            }
        }
        for face in faces {
            for i in face {
                output.write_all(&i.to_le_bytes()).unwrap();
            }
        }
    }
    output.flush().unwrap();
    let peak = selected_peak
        .expect("local refinement retains the independently identified native relief post");
    let ground = scene.imported_ground(peak).unwrap();
    assert!((ground.hit.geodetic.height_m - peak.height_m).abs() < 2e-7);
    println!(
        "Retained independent peak: {peak:?}; audit {}",
        path.display()
    );
}
