//! Original MIT. Canonical source construction for the host-only ABI probe.
//! The mechanical probe declares `normal` at crate root using the unregistered
//! renderer module; this fixture owns all source/coordinate interpretation.
use crate::normal::FieldSettings;
use earth_math::{atmosphere::AtmosphereLayer, ecef_to_geodetic, Vec3d};
use serde_json::{json, Value};

pub struct Fixture {
    pub settings: FieldSettings,
    pub expected: Vec<u8>,
    pub metadata: Value,
}

pub fn build() -> Fixture {
    // Use precisely the immutable canonical inputs that authored the GPU
    // fixture. Fresh weather interpolation is a different source route: the
    // retained first ABI attempt records its adjacent-f64 mismatch.
    let data: Value = serde_json::from_slice(
        &std::fs::read("artifacts/optical-normal-coordinate-current.json").unwrap(),
    )
    .unwrap();
    let input = &data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["maximum_step_m"].as_f64() == Some(100000.0))
        .unwrap()["inputs"];
    let layers = input["layers"].as_array().unwrap();
    let nodes: Vec<_> = layers
        .iter()
        .map(|node| AtmosphereLayer {
            height_m: node["height_m"].as_f64().unwrap(),
            refractivity: node["refractivity"].as_f64().unwrap(),
        })
        .collect();
    let base = input["profile_base_height_m"].as_f64().unwrap();
    let origin = &input["origin_ecef_m"];
    let origin = Vec3d::new(
        origin[0].as_f64().unwrap(),
        origin[1].as_f64().unwrap(),
        origin[2].as_f64().unwrap(),
    );
    // This is precisely the initialization convention of the retained clear
    // fixture. It is not an assertion that arbitrary eye construction routes
    // are bit-identical before the common canonical eye is chosen.
    let eye = ecef_to_geodetic(origin).unwrap();
    let settings = FieldSettings::from_canonical_parts(eye, base, &nodes).unwrap();
    let expected_path = "artifacts/optical-height-trace-fast-metric-uniform.bin";
    let expected = std::fs::read(expected_path).unwrap();
    assert_eq!(expected.len() as u64, crate::normal::FIELD_BYTES);
    Fixture {
        settings,
        expected,
        metadata: json!({
            "scope":"Exact retained canonical profile/eye through the common validated host packer, compared to independently reviewed GPU uniform bytes. ABI/packing only; no new solver or scene acceptance. Fresh source-route equality is not asserted.",
            "canonical_input_source":"artifacts/optical-normal-coordinate-current.json",
            "expected_uniform":expected_path,
            "eye_geodetic":[eye.lat_deg,eye.lon_deg,eye.height_m],
            "profile_base_height_m":base,
            "prior_source_route_mismatch":"artifacts/optical-normal-packet-encase-first.stderr.log",
            "source_nodes":layers.len(),
            "field_vec4_count":crate::normal::FIELD_VEC4_COUNT,
        }),
    }
}
