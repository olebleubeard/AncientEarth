use earth_math::atmosphere::weather::{self, CustomWeatherDraft};
use earth_math::atmosphere::{AtmosphereLayer, AtmosphereProfile, phase_index};
use earth_math::{Geodetic, TerrainPatch, TerrainScene, UnitSystem};

fn source() -> AtmosphereProfile {
    weather::decode_gfs(
        include_bytes!("../../../assets/weather/gfs-reference.grib2"),
        &weather::reference_request(),
    )
    .unwrap()
    .profile
}

fn assert_same_physics(a: &AtmosphereProfile, b: &AtmosphereProfile) {
    assert_eq!(a.layers(), b.layers());
    for (a, b) in a.layers().iter().zip(b.layers()) {
        assert_eq!(a.height_m.to_bits(), b.height_m.to_bits());
        assert_eq!(a.refractivity.to_bits(), b.refractivity.to_bits());
    }
    assert_eq!(a.weather_samples(), b.weather_samples());
    assert_eq!(a.base_height_m().to_bits(), b.base_height_m().to_bits());
    assert_eq!(a.anchor(), b.anchor());
    assert_eq!(
        a.maximum_index_gradient().to_bits(),
        b.maximum_index_gradient().to_bits()
    );
}

#[test]
fn unedited_apply_and_unit_switches_preserve_original_column_bits() {
    let original = source();
    for units in [UnitSystem::Metric, UnitSystem::Imperial] {
        let mut draft = CustomWeatherDraft::from_profile(&original, units).unwrap();
        assert_eq!(draft.rows.len() + 1, original.layers().len());
        assert_same_physics(&original, &draft.build_profile().unwrap());
        for u in [UnitSystem::Imperial, UnitSystem::Metric, units] {
            draft.set_units(u).unwrap();
            assert_same_physics(&original, &draft.build_profile().unwrap());
        }
    }
}

#[test]
fn edits_use_explicit_temperature_pressure_and_specific_humidity_units() {
    let original = source();
    let mut c = CustomWeatherDraft::from_profile(&original, UnitSystem::Metric).unwrap();
    let mut f = CustomWeatherDraft::from_profile(&original, UnitSystem::Imperial).unwrap();
    c.rows[0].temperature = "20".into();
    f.rows[0].temperature = "68".into();
    c.rows[0].pressure_hpa = "1100".into();
    f.rows[0].pressure_hpa = "1100".into();
    c.rows[0].specific_humidity_g_kg = "8".into();
    f.rows[0].specific_humidity_g_kg = "8".into();
    let changed = c.build_profile().unwrap();
    assert_same_physics(&changed, &f.build_profile().unwrap());
    let actual = changed.weather_samples().unwrap()[0];
    assert_eq!(actual.temperature_k, 293.15);
    assert_eq!(actual.pressure_pa, 110000.0);
    assert_eq!(actual.specific_humidity, 0.008);
    assert_eq!(
        changed.layers()[0].refractivity,
        phase_index(293.15, 110000.0, 0.008, 550.0).unwrap() - 1.0
    );
    // The 2 m row is independently editable; ground changes do not silently
    // replace it with a hydrostatic reconstruction or overwrite other rows.
    assert_eq!(
        changed.weather_samples().unwrap()[1..],
        original.weather_samples().unwrap()[1..]
    );
    c.set_units(UnitSystem::Imperial).unwrap();
    assert_same_physics(&changed, &c.build_profile().unwrap());
}

#[test]
fn invalid_values_and_pressure_order_are_refused_without_mutation() {
    let original = source();
    let clean = CustomWeatherDraft::from_profile(&original, UnitSystem::Metric).unwrap();
    for bad in ["NaN", "inf", "", "not a number", "127", "-174"] {
        let mut draft = clean.clone();
        draft.rows[0].temperature = bad.into();
        assert!(draft.build_profile().is_err());
        let rows = draft.rows.clone();
        assert!(draft.set_units(UnitSystem::Imperial).is_err());
        assert_eq!(draft.units(), UnitSystem::Metric);
        assert_eq!(draft.rows, rows);
    }
    for bad in ["-0.1", "100.1", "NaN"] {
        let mut draft = clean.clone();
        draft.rows[0].specific_humidity_g_kg = bad.into();
        assert!(draft.build_profile().is_err());
    }
    for bad in ["-1", "1200.1", "inf"] {
        let mut draft = clean.clone();
        draft.rows[0].pressure_hpa = bad.into();
        assert!(draft.build_profile().is_err());
    }
    let mut reordered = clean.clone();
    reordered.rows[1].pressure_hpa = reordered.rows[0].pressure_hpa.clone();
    assert!(reordered.build_profile().is_err());
    let mut incomplete = clean.clone();
    incomplete.rows.pop();
    assert!(incomplete.build_profile().is_err());
    assert_same_physics(&original, &clean.build_profile().unwrap());
}

#[test]
fn declared_temperature_endpoints_are_inclusive_without_nearby_clamping() {
    for (units, inputs) in [
        (UnitSystem::Metric, ["-173.15", "126.85"]),
        (UnitSystem::Imperial, ["-279.67", "260.33"]),
    ] {
        for (text, expected) in inputs.into_iter().zip([100.0_f64, 400.0]) {
            let mut draft = CustomWeatherDraft::from_profile(&source(), units).unwrap();
            draft.rows[0].temperature = text.into();
            assert_eq!(
                draft.build_profile().unwrap().weather_samples().unwrap()[0]
                    .temperature_k
                    .to_bits(),
                expected.to_bits()
            );
        }
        let mut draft = CustomWeatherDraft::from_profile(&source(), units).unwrap();
        draft.rows[0].temperature = if units == UnitSystem::Metric {
            "-173.150001"
        } else {
            "-279.670001"
        }
        .into();
        assert!(draft.build_profile().is_err());
    }
}

#[test]
fn custom_provenance_is_explicit_and_reediting_does_not_nest_it() {
    let original = source();
    let custom = CustomWeatherDraft::from_profile(&original, UnitSystem::Metric)
        .unwrap()
        .build_profile()
        .unwrap();
    let metadata = custom.metadata.as_ref().unwrap();
    let seed = metadata.custom_origin.as_ref().unwrap();
    assert_eq!(metadata.source, "Custom weather (user-edited column)");
    assert!(metadata.model.starts_with("Custom column"));
    assert!(
        metadata
            .surface_conditions_label(UnitSystem::Metric)
            .starts_with("Custom column:")
    );
    assert_eq!(
        seed.source_url,
        original.metadata.as_ref().unwrap().source_url
    );
    assert_eq!(
        seed.valid_unix_seconds,
        original.metadata.as_ref().unwrap().valid_unix_seconds
    );
    assert!(seed.custom_origin.is_none());
    let repeated = CustomWeatherDraft::from_profile(&custom, UnitSystem::Imperial)
        .unwrap()
        .build_profile()
        .unwrap();
    assert_same_physics(&custom, &repeated);
    assert!(
        repeated
            .metadata
            .unwrap()
            .custom_origin
            .unwrap()
            .custom_origin
            .is_none()
    );
    assert!(original.metadata.as_ref().unwrap().custom_origin.is_none());
}

#[test]
fn registered_composite_heights_and_bound_scene_anchor_are_retained() {
    let original = weather::decode_weather(
        include_bytes!("../../../assets/weather/gfs-regional-reference.grib2"),
        Some(include_bytes!(
            "../../../assets/weather/hrrr-reference.grib2"
        )),
        &weather::regional_reference_request(),
    )
    .unwrap()
    .profile;
    let eye = Geodetic::new(29.305549, -94.767015, 10.0).unwrap();
    let patch = TerrainPatch::from_grid(
        3,
        3,
        eye.lon_deg - 0.001,
        eye.lat_deg + 0.001,
        0.001,
        0.001,
        vec![-20.0; 9],
    )
    .unwrap();
    let bound = original
        .anchored_to_scene_at(&TerrainScene::new(vec![patch]), eye)
        .unwrap();
    let changed = CustomWeatherDraft::from_profile(&bound, UnitSystem::Imperial)
        .unwrap()
        .build_profile()
        .unwrap();
    assert_same_physics(&bound, &changed);
    assert!(changed.base_height_m() < -19.0);
    assert_eq!(
        changed.layers().last().unwrap(),
        &AtmosphereLayer {
            height_m: 120000.0,
            refractivity: 0.0
        }
    );
    assert!(
        changed
            .metadata
            .as_ref()
            .unwrap()
            .custom_origin
            .as_ref()
            .unwrap()
            .cross_model_join
            .is_some()
    );
    let synthetic = AtmosphereProfile::from_layers(vec![
        AtmosphereLayer {
            height_m: 0.0,
            refractivity: 0.0,
        },
        AtmosphereLayer {
            height_m: 120000.0,
            refractivity: 0.0,
        },
    ])
    .unwrap();
    assert!(CustomWeatherDraft::from_profile(&synthetic, UnitSystem::Metric).is_err());
}
