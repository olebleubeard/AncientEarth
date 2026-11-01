//! Source fixtures come from independent GDAL and PROJ decoding. Mathematical
//! review of the decoder and these references is performed by the other agent.
use earth_math::{
    atmosphere::weather::{WeatherGrid, hrrr::*},
    *,
};
#[path = "fixtures/weather_hrrr_reference.rs"]
mod reference;
const SOURCE: &[u8] = include_bytes!("../../../assets/weather/hrrr-reference.grib2");

fn request() -> HrrrRequest {
    request_candidates(29.305549, -94.767015, 1_792_918_800.0)
        .unwrap()
        .remove(1)
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} within {tol}");
}

#[test]
fn independent_proj_post_selection_and_hourly_request_identity() {
    for &[lat, lon, i, j, selected_lat, selected_lon] in reference::SELECTIONS {
        let actual = grid_point(lat, lon).unwrap();
        assert_eq!((actual.0, actual.1), (i as u32, j as u32));
        close(actual.2, selected_lat, 2e-12);
        close(actual.3, selected_lon, 2e-12);
        let candidates = request_candidates(lat, lon, 1_792_918_800.0).unwrap();
        assert_eq!(candidates.len(), 3);
        for (offset, r) in candidates.iter().enumerate() {
            assert_eq!(r.cycle_unix_seconds, 1_792_911_600 - offset as i64 * 3600);
            assert_eq!(r.valid_unix_seconds, 1_792_918_800);
            assert_eq!(r.forecast_hour, 2 + offset as u32);
            assert!(r.url.contains("wrfsfcf"));
            assert!(r.url.contains("lev_2_m_above_ground=on"));
            assert!(!r.url.contains("wrfprs"));
            assert!(r.matches_location(Geodetic::new(lat, lon, 0.0).unwrap()));
        }
    }
    for (lat, lon) in [
        (0.0, 0.0),
        (90.0, 0.0),
        (-90.0, 0.0),
        (f64::NAN, 0.0),
        (29.0, 181.0),
    ] {
        assert!(grid_point(lat, lon).is_err());
    }
    for time in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(request_candidates(29.3, -94.7, time).is_err());
    }
}

#[test]
fn every_original_native_post_matches_gdal_fields_without_inventing_vertical_levels() {
    for &[i, j, lat, lon, p, h, t, q] in reference::POSTS {
        let mut selected = request();
        selected.grid_x = i as u32;
        selected.grid_y = j as u32;
        selected.latitude_deg = lat;
        selected.longitude_deg = lon;
        let column = decode_hrrr(SOURCE, &selected).unwrap();
        assert_eq!(column.samples.len(), 2);
        assert_eq!(column.samples[0].height_m, 0.0);
        assert_eq!(column.samples[1].height_m, 2.0);
        close(column.metadata.pressure_surface_pa, p, 1e-9);
        close(column.metadata.provider_surface_geopotential_m, h, 1e-9);
        close(column.metadata.temperature_2m_k, t, 1e-9);
        close(column.metadata.specific_humidity_2m, q, 1e-12);
        assert!(column.metadata.temperature_80m_k.is_none());
        assert!(column.metadata.pressure_80m_pa.is_none());
        assert!(column.metadata.specific_humidity_80m.is_none());
        assert!(
            matches!(column.metadata.source_grid,WeatherGrid::Hrrr{grid_x,grid_y} if grid_x==i as u32 && grid_y==j as u32)
        );
        let p2 = p * (-9.80665 * 2.0 / (287.05 * t * (1.0 + 0.6078 * q))).exp();
        close(column.samples[1].pressure_pa, p2, 1e-9);
        assert!(
            column
                .metadata
                .usable_at(Geodetic::new(lat, lon, 0.0).unwrap(), 1_792_918_800.0)
        );
    }
}

#[test]
fn malformed_grid_cycle_missing_duplicate_and_request_identity_are_rejected() {
    let selected = request();
    assert!(decode_hrrr(SOURCE, &selected).is_ok());
    let mut forged = selected.clone();
    forged.grid_x += 1;
    assert!(decode_hrrr(SOURCE, &forged).is_err());
    forged = selected.clone();
    forged.valid_unix_seconds += 1;
    assert!(decode_hrrr(SOURCE, &forged).is_err());
    let mut starts = Vec::new();
    let mut at = 0;
    while at < SOURCE.len() {
        starts.push(at);
        at += u64::from_be_bytes(SOURCE[at + 8..at + 16].try_into().unwrap()) as usize;
    }
    assert_eq!(starts.len(), 5);
    let mut duplicate = SOURCE.to_vec();
    duplicate.extend_from_slice(&SOURCE[..starts[1]]);
    assert!(decode_hrrr(&duplicate, &selected).is_err());
    assert!(decode_hrrr(&SOURCE[..starts[4]], &selected).is_err()); // missing2mSPFH
    for end in [0, 4, 15, SOURCE.len() - 3] {
        assert!(decode_hrrr(&SOURCE[..end], &selected).is_err());
    }
    // Modify first required field's template30 scan/sphere/spacing, retaining
    // a syntactically framed GRIB message so registration guards are exercised.
    let first_length = starts[1];
    let mut position = 16;
    while position < first_length - 4 {
        let length =
            u32::from_be_bytes(SOURCE[position..position + 4].try_into().unwrap()) as usize;
        if SOURCE[position + 4] == 3 {
            for offset in [14, 55, 64] {
                let mut changed = SOURCE.to_vec();
                changed[position + offset] ^= 1;
                assert!(decode_hrrr(&changed, &selected).is_err());
            }
            break;
        }
        position += length;
    }
}

#[test]
fn regional_global_splice_preserves_provider_heights_and_fallback_is_explicit() {
    use earth_math::atmosphere::{
        phase_index,
        weather::{WeatherRequest, decode_weather, request_candidates as global_candidates},
    };
    let hrrr = reference_request();
    let gfs = global_candidates(29.305549, -94.767015, 1_792_918_800.0)
        .unwrap()
        .remove(0);
    let request = WeatherRequest {
        key: "independent-composite".into(),
        url: gfs.url.clone(),
        gfs,
        hrrr: Some(hrrr.clone()),
    };
    let global_bytes = include_bytes!("../../../assets/weather/gfs-regional-reference.grib2");
    let decoded = decode_weather(global_bytes, Some(SOURCE), &request).unwrap();
    assert!(
        decoded.regional_fallback.is_none(),
        "{:?}",
        decoded.regional_fallback
    );
    let raw = reference::POSTS
        .iter()
        .find(|r| r[0] as u32 == hrrr.grid_x && r[1] as u32 == hrrr.grid_y)
        .unwrap();
    let [gp, gh, gt, gq, t80, p80, q80] = reference::GFS_SURFACE;
    let z = |h: f64| 6_356_766.0 * h / (6_356_766.0 - h);
    let p2 = gp * (-9.80665 * 2.0 / (287.05 * gt * (1.0 + 0.6078 * gq))).exp();
    // GFS ground/2m are synthetic lower-column support competing with HRRR's
    // lower conditions, so the splice uses only actual upper source samples.
    let mut inputs = vec![(80.0, t80, p80, q80)];
    for &[p, h, t, q] in reference::GFS_LEVELS {
        let height = z(h) - z(gh);
        if p < p2 && height > 2.01 && (height - 80.0).abs() > 0.01 {
            inputs.push((height, t, p, q));
        }
    }
    inputs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let offset = z(gh) - z(raw[5]);
    let expected = inputs
        .into_iter()
        .map(|(h, t, p, q)| (h + offset, t, p, q))
        .filter(|r| r.0 > 2.01)
        .collect::<Vec<_>>();
    assert_eq!(decoded.profile.layers().len(), expected.len() + 3); // HRRR0/2 plus vacuum
    for (&(height, t, p, q), actual) in expected.iter().zip(&decoded.profile.layers()[2..]) {
        close(actual.height_m, height, 0.012);
        close(
            actual.refractivity,
            phase_index(t, p, q, 550.0).unwrap() - 1.0,
            1e-10,
        );
    }
    assert!(decoded.metadata.secondary_source_url.is_some());
    let join = decoded.metadata.cross_model_join.unwrap();
    close(join.provider_ground_offset_m, offset, 1e-9);
    close(join.gfs_surface_geopotential_m, gh, 1e-9);
    assert_eq!(join.gfs_cycle_unix_seconds, 1_792_886_400);
    close(join.first_global_height_m, 80.0 + offset, 1e-9);
    close(join.join_span_m, 78.0 + offset, 1e-9);
    let fallback = decode_weather(global_bytes, None, &request).unwrap();
    assert!(fallback.regional_fallback.is_some());
    assert!(matches!(fallback.metadata.source_grid, WeatherGrid::Gfs));
    let mut wrong = request.clone();
    wrong.hrrr.as_mut().unwrap().valid_unix_seconds += 3600;
    assert!(
        decode_weather(global_bytes, Some(SOURCE), &wrong)
            .unwrap()
            .regional_fallback
            .is_some()
    );
}

#[test]
fn requested_surface_anchor_is_fixed_and_preserves_source_relative_knots() {
    use earth_math::{TerrainPatch, TerrainScene, atmosphere::weather::decode_gfs};
    let location = Geodetic::new(29.305549, -94.767015, 12345.0).unwrap();
    let request = earth_math::atmosphere::weather::request_candidates(
        location.lat_deg,
        location.lon_deg,
        1_792_918_800.0,
    )
    .unwrap()
    .remove(0);
    let raw = decode_gfs(
        include_bytes!("../../../assets/weather/gfs-regional-reference.grib2"),
        &request,
    )
    .unwrap()
    .profile;
    // About 22m wide: requested location has source terrain while the nearest
    // quarter-degree NOAA post is outside it. Binding at the source post would
    // incorrectly leave this mountain column at reference height zero.
    for height in [-30.0, 3500.0] {
        let patch = TerrainPatch::from_grid(
            2,
            2,
            location.lon_deg - 0.0001,
            location.lat_deg + 0.0001,
            0.0002,
            0.0002,
            vec![height; 4],
        )
        .unwrap();
        let scene = TerrainScene::new(vec![patch]);
        near_anchor(
            scene.ground(location).unwrap().hit.geodetic.height_m,
            height,
            0.001,
        );
        let anchored = raw.anchored_to_scene_at(&scene, location).unwrap();
        near_anchor(anchored.base_height_m(), height, 0.001);
        let identity = anchored.anchor().unwrap();
        assert_eq!(identity.location.lat_deg, location.lat_deg);
        assert_eq!(identity.location.lon_deg, location.lon_deg);
        near_anchor(identity.location.height_m, anchored.base_height_m(), 0.0);
        for layer in raw.layers().iter().skip(1) {
            let actual = anchored.index_gradient(anchored.base_height_m() + layer.height_m);
            let expected = raw.index_gradient(layer.height_m);
            near_anchor(actual.0, expected.0, 2e-15);
        }
        let below = anchored.index_gradient(anchored.base_height_m() - 20.0);
        near_anchor(below.0, 1.0 + raw.layers()[0].refractivity, 0.0);
        assert_eq!(below.1, 0.0);
        let distant = Geodetic::new(location.lat_deg + 1.0, location.lon_deg, 0.0).unwrap();
        let _ = scene.ground(distant).unwrap();
        assert_eq!(anchored.anchor().unwrap().location, identity.location);
        // The compatibility source-post anchor remains h=0, demonstrating the
        // distinction instead of merely comparing two calls of the new API.
        near_anchor(
            raw.anchored_to_scene(&scene).unwrap().base_height_m(),
            0.0,
            1e-6,
        );
    }
}

fn near_anchor(a: f64, b: f64, t: f64) {
    assert!((a - b).abs() <= t, "{a} != {b} within {t}");
}
