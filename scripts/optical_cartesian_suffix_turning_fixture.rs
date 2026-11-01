//! Original MIT. Retained returning-gradient input, with ordinary full-trace
//! terminal semantics. The old prescribed-path reference endpoint is unused.
use super::TraceFixture;
use earth_math::{
    Geodetic, MIN_HEIGHT_M, Observer, TerrainScene, Vec3d, WGS84_A_M, WGS84_E2,
    atmosphere::{
        AtmosphereLayer, AtmosphereProfile,
        gpu::{GpuEllipsoid, scene::PackedScene},
    },
    rendering::LocalFrame,
};
use serde_json::{Value, json};

pub const CASE: &str = "returning-gradient";
const INPUT: &str = "artifacts/optical-bouguer-reference.json";

// The enclosing document is validated by serde_json first. These bounded
// slices preserve numeric spellings; serde_json::Value's default fast float
// parser can differ by one binary64 ULP from Rust's correctly rounded parser.
fn raw_value_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    assert!(start < bytes.len());
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for (index, &byte) in bytes.iter().enumerate().skip(start) {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
                if depth == 0 {
                    return index + 1;
                }
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        return index + 1;
                    }
                }
                b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t' if depth == 0 => {
                    return index;
                }
                _ => {}
            }
        }
    }
    assert!(!quoted && depth == 0);
    bytes.len()
}

fn raw_member<'a>(text: &'a str, name: &str) -> &'a str {
    let mut rest = text.trim();
    assert!(rest.starts_with('{') && rest.ends_with('}'));
    rest = &rest[1..];
    let mut found = None;
    loop {
        rest = rest.trim_start();
        if rest.starts_with('}') {
            break;
        }
        assert!(rest.starts_with('"'));
        let end = raw_value_end(rest, 0);
        let key: String = serde_json::from_str(&rest[..end]).unwrap();
        rest = rest[end..].trim_start();
        rest = rest.strip_prefix(':').unwrap().trim_start();
        let end = raw_value_end(rest, 0);
        if key == name {
            assert!(found.is_none(), "Duplicate retained member");
            found = Some(&rest[..end]);
        }
        rest = rest[end..].trim_start();
        if rest.starts_with('}') {
            break;
        }
        rest = rest.strip_prefix(',').unwrap();
    }
    found.expect("Missing retained member")
}

fn raw_items(text: &str) -> Vec<&str> {
    let mut rest = text.trim();
    assert!(rest.starts_with('[') && rest.ends_with(']'));
    rest = &rest[1..];
    let mut values = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            break;
        }
        let end = raw_value_end(rest, 0);
        assert!(end > 0);
        values.push(&rest[..end]);
        rest = rest[end..].trim_start();
        if rest.starts_with(']') {
            break;
        }
        rest = rest.strip_prefix(',').unwrap();
    }
    values
}

fn record(bytes: &mut [u8], index: usize, value: [f32; 4]) {
    for (component, value) in value.into_iter().enumerate() {
        bytes[index * 16 + component * 4..index * 16 + component * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
}
fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

pub fn apply(fixture: &mut TraceFixture) {
    assert!(
        fixture
            .suffix
            .scene
            .cases
            .get("cartesian_suffix_atmosphere")
            .is_none(),
        "Returning physical control must not inherit atmosphere-only stubs"
    );
    assert!(
        fixture
            .suffix
            .scene
            .cases
            .get("cartesian_suffix_turning")
            .is_none(),
        "Returning control applied twice"
    );
    let retained_text = std::fs::read_to_string(INPUT).unwrap();
    let retained: Value = serde_json::from_str(&retained_text).unwrap();
    let cases = retained["cases"].as_array().unwrap();
    let matching: Vec<_> = cases
        .iter()
        .filter(|c| c["name"] == "outward turn and return to initial height")
        .collect();
    assert_eq!(matching.len(), 1);
    let input = matching[0];
    assert_eq!(input["profile_base_height_m"].as_f64().unwrap(), 0.0);
    assert_eq!(input["origin_ecef_m"], json!([6379137.0, 0.0, 0.0]));
    let expected = [
        [0.0, 0.0005],
        [1000.0, 0.0003],
        [2000.0, 0.0001],
        [3000.0, 0.0],
        [120000.0, 0.0],
    ];
    let values = input["layers"].as_array().unwrap();
    assert_eq!(values.len(), expected.len());
    let layers: Vec<_> = values
        .iter()
        .zip(expected)
        .map(|(value, expected)| {
            let h = value["height_m"].as_f64().unwrap();
            let n = value["refractivity"].as_f64().unwrap();
            assert_eq!([h, n], expected);
            AtmosphereLayer {
                height_m: h,
                refractivity: n,
            }
        })
        .collect();
    let profile = AtmosphereProfile::from_layers(layers).unwrap();
    let layers = profile.layers();
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap());
    observer.feet.height_m = 1000.0 - observer.eye_height_m;
    assert_eq!(observer.eye_geodetic().height_m, 1000.0);
    let raw_cases = raw_items(raw_member(&retained_text, "cases"));
    assert_eq!(raw_cases.len(), cases.len());
    let selected_index = cases.iter().position(|case| case == input).unwrap();
    let raw_case = raw_cases[selected_index];
    assert_eq!(serde_json::from_str::<Value>(raw_case).unwrap(), *input);
    let direction_tokens = raw_items(raw_member(raw_case, "initial_direction"));
    assert_eq!(direction_tokens.len(), 3);
    let direction: Vec<f64> = direction_tokens
        .iter()
        .map(|token| token.parse::<f64>().unwrap())
        .collect();
    let ray = Vec3d::new(direction[0], direction[1], direction[2]);
    assert_eq!(
        [ray.x, ray.y, ray.z],
        [0.0034906514152237326, 0.9999939076577904, 0.0]
    );
    let local = LocalFrame::at(observer);
    let words = local.direction(ray).map(f32::to_bits);
    assert_eq!(words, [1065353114, 996459370, 2147483648]);
    let legacy_ray = Vec3d::new(
        input["initial_direction"][0].as_f64().unwrap(),
        input["initial_direction"][1].as_f64().unwrap(),
        input["initial_direction"][2].as_f64().unwrap(),
    );
    assert_eq!(local.direction(legacy_ray).map(f32::to_bits), words);
    let gpu = GpuEllipsoid::at(observer);
    let gradient = layers
        .windows(2)
        .map(|p| (p[1].refractivity - p[0].refractivity).abs() / (p[1].height_m - p[0].height_m))
        .fold(0.0_f64, f64::max);
    let scene = &mut fixture.suffix.scene;
    assert_eq!(scene.record_count, 23);
    assert_eq!(scene.primary_uniform.len(), 2240);
    for (i, row) in gpu.rows.into_iter().enumerate() {
        record(&mut scene.primary_uniform, i, row);
    }
    record(&mut scene.primary_uniform, 3, gpu.origin);
    record(&mut scene.primary_uniform, 4, gpu.center);
    record(
        &mut scene.primary_uniform,
        9,
        [
            layers.len() as f32,
            profile.top_height_m() as f32,
            (gradient + 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M)) as f32,
            1.0,
        ],
    );
    record(
        &mut scene.primary_uniform,
        10,
        [gradient as f32, 0.0, MIN_HEIGHT_M as f32, 0.0],
    );
    for i in 0..128 {
        let value = if i < layers.len() {
            let l = layers[i];
            let slope = layers.get(i + 1).map_or(0.0, |n| {
                (n.refractivity - l.refractivity) / (n.height_m - l.height_m)
            });
            [l.height_m as f32, l.refractivity as f32, slope as f32, 0.0]
        } else {
            [0.0; 4]
        };
        record(&mut scene.primary_uniform, 12 + i, value);
    }
    assert_eq!(
        f32::from_le_bytes(scene.primary_uniform[136..140].try_into().unwrap()),
        0.0
    );
    let old = "let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);";
    let new = format!(
        "let direction=normalize(bitcast<vec3<f32>>(vec3<u32>({}u,{}u,{}u)));",
        words[0], words[1], words[2]
    );
    let prefix = &mut fixture.suffix.modules[0].shader;
    assert_eq!(
        prefix.matches(old).count(),
        1,
        "Returning launch source drift"
    );
    *prefix = prefix.replacen(old, &new, 1);
    for module in &fixture.suffix.modules {
        assert!(!module.shader.contains("nc_settings."));
    }
    // Keep actual scene_event and terrain_coverage implementations. There are
    // no triangles; h0 is therefore an uncovered physical first surface.
    let empty = PackedScene::prepare(&TerrainScene::new(vec![])).unwrap();
    let frames = empty.frame_data(local, &[]).unwrap();
    assert_eq!(empty.geometry.len(), 1);
    assert_eq!(frames.len(), 6);
    assert_eq!(frames[0].map(f32::to_bits), [0; 4]);
    assert_eq!(frames[1][0].to_bits(), 6);
    assert_eq!(frames[2][3].to_bits(), 0);
    scene.geometry = bytes(&empty.geometry);
    scene.frames = bytes(&frames);
    scene.shader = fixture.suffix.modules[0].shader.clone();
    scene.cases["cartesian_suffix_turning"] = json!({
        "case":CASE,"source":"scripts/optical_cartesian_suffix_turning_fixture.rs",
        "retained_input":INPUT,"retained_name":input["name"],
        "eye_geodetic":[0.0,0.0,1000.0],"origin_ecef_m":[6379137.0,0.0,0.0],
        "base_height_m":0.0,"layers":expected,"raw_local_launch_words":words,
        "direction_ingestion":"Bounded raw cases/member slices tied to the unique matched Value; Rust f64::from_str parses only its three direction tokens. Exact canonical assertion retained and uploaded local f32 words match the prior Value route.",
        "canonical_direction_f64_bits":direction.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "serde_value_direction_f64_bits":[legacy_ray.x.to_bits(),legacy_ray.y.to_bits(),legacy_ray.z.to_bits()],
        "launch_mapping":"One unique retained equatorial launch repeated across23 records; normalize once on GPU. Pixel labels are not a camera projection.",
        "scope":"Strong negative index-gradient returning-ray control. Full original stopping semantics, uncovered h0 and coherent empty scene; no prescribed-path stop, no positive-slope or meteorological temperature-inversion claim.",
        "unused_old_endpoint":"The retained Bouguer case stops at first return to its initial height. Its old target_path_m and endpoint are never used as this full-trace reference.",
        "prospective_gate":"Freeze original full trace and independent source-aware final-event reference, including an observed radial turn, before candidate readback. Require observed candidate handoff then original-eye fallback and exact original terminal, with no sky downgrade; otherwise this case does not close returning-ray/fallback coverage.",
        "factor":0.25,"unique_rays":1,"duplicate_records":22
    });
}
