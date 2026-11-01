//! Original MIT. Mathematical inputs and exact shader/output contract for the
//! first complete clear-ray normal-chart diagnostic. No application mutation.
use earth_math::{ecef_to_geodetic, EnuBasis, Vec3d};
use serde_json::{json, Value};
#[path = "optical_height_fixture.rs"]
mod height_fixture;
#[path = "optical_height_trial_capture.rs"]
mod height_trial_capture;
#[path = "optical_height_trial_split.rs"]
mod height_trial_split;
#[path = "optical_height_rhs_fixture.rs"]
mod height_rhs_fixture;

pub struct Fixture {
    pub shader: String,
    pub uniform: Vec<u8>,
    pub rays: Vec<u8>,
    pub cases: Value,
    pub count: u32,
    pub record_words: u32,
    pub output_bytes: u64,
}
pub fn build_projected() -> Fixture {
    let mut fixture=build_metric_fast();
    fixture.shader=height_rhs_fixture::inject(fixture.shader);
    fixture.cases["projected_height_rhs"]=json!(true);
    fixture.cases["projected_height_rhs_scope"]=json!("Same fast-metric stages, source/contact controls and error budgets; algebraically projected height derivative with original-evaluator fallback. Finite outputs need canonical-reference validation; bit parity is not asserted.");
    fixture
}
fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect()
}
fn split(value: f64) -> [f32; 2] {
    let high = value as f32;
    [high, (value - f64::from(high)) as f32]
}
// Bound the difference of the actual uploaded split heights, including host
// arithmetic. The third component encloses the residual after the f32 split.
fn encoded_gap(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    let av = f64::from(a[0]) + f64::from(a[1]);
    let bv = f64::from(b[0]) + f64::from(b[1]);
    let lower = (av.next_down() - bv.next_up()).next_down();
    let upper = (av.next_up() - bv.next_down()).next_up();
    let gap = av - bv;
    let pair = split(gap);
    let encoded = f64::from(pair[0]) + f64::from(pair[1]);
    let error = (encoded.next_up() - lower)
        .max(upper - encoded.next_down())
        .max(0.)
        .next_up();
    [pair[0], pair[1], (error as f32).next_up(), 0.]
}
fn v3(value: &Value) -> Vec3d {
    Vec3d::new(
        value[0].as_f64().unwrap(),
        value[1].as_f64().unwrap(),
        value[2].as_f64().unwrap(),
    )
}
pub fn build() -> Fixture {
    let data: Value = serde_json::from_slice(
        &std::fs::read("artifacts/optical-normal-coordinate-current.json").unwrap(),
    )
    .unwrap();
    let cases: Vec<_> = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["maximum_step_m"].as_f64() == Some(100000.))
        .collect();
    assert_eq!(cases.len(), 7);
    let input = &cases[0]["inputs"];
    let origin = v3(&input["origin_ecef_m"]);
    let eye = ecef_to_geodetic(origin).unwrap();
    let basis = EnuBasis::at(eye);
    let base = input["profile_base_height_m"].as_f64().unwrap();
    let knots: Vec<_> = input["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                base + row["height_m"].as_f64().unwrap(),
                row["refractivity"].as_f64().unwrap(),
            )
        })
        .collect();
    assert!((2..=128).contains(&knots.len()));
    let slopes: Vec<_> = knots
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0))
        .collect();
    let owner = knots.partition_point(|&(h, _)| h < eye.height_m);
    assert!(owner < knots.len());
    // This first fixture is strictly inside a source interval, so all seven
    // launch directions share exact initial branch ownership.
    assert!(knots.iter().all(|&(h, _)| (h - eye.height_m).abs() > 0.001));
    let (index, slope) = if owner == 0 {
        (knots[0].1, 0.)
    } else {
        let lower = knots[owner - 1];
        (
            lower.1 + slopes[owner - 1] * (eye.height_m - lower.0),
            slopes[owner - 1],
        )
    };
    let (sin, cos) = eye.lat_deg.to_radians().sin_cos();
    let height = split(eye.height_m);
    let encoded_heights: Vec<_> = knots.iter().map(|&(h, _)| split(h)).collect();
    let initial_lower = encoded_gap(
        height,
        if owner == 0 {
            [-10000., 0.]
        } else {
            encoded_heights[owner - 1]
        },
    );
    let initial_upper = encoded_gap(height, encoded_heights[owner]);
    let mut uniform = vec![
        [sin as f32, cos as f32, height[0], height[1]],
        [index as f32, slope as f32, owner as f32, knots.len() as f32],
        [-10000., 100000., 2000000., 0.002],
        initial_lower,
        initial_upper,
    ];
    for (i, &(h, n)) in knots.iter().enumerate() {
        let h = split(h);
        uniform.push([
            h[0],
            h[1],
            n as f32,
            slopes.get(i).copied().unwrap_or(0.) as f32,
        ]);
    }
    uniform.resize(133, [0.; 4]);
    let mut gaps: Vec<_> = encoded_heights
        .windows(2)
        .map(|pair| encoded_gap(pair[1], pair[0]))
        .collect();
    // The top node has no next interval; its unused gap slot holds the
    // below-base domain width for a downward departure from the first knot.
    gaps.push(encoded_gap(encoded_heights[0], [-10000., 0.]));
    gaps.resize(128, [0.; 4]);
    uniform.extend_from_slice(&gaps);
    let mut rays = Vec::new();
    let mut records = Vec::new();
    for case in cases {
        assert_eq!(case["inputs"]["origin_ecef_m"], input["origin_ecef_m"]);
        assert_eq!(case["inputs"]["layers"], input["layers"]);
        let direction = v3(&case["inputs"]["initial_direction"]);
        let local = [
            direction.dot(basis.east) as f32,
            direction.dot(basis.up) as f32,
            -direction.dot(basis.north) as f32,
            0.,
        ];
        rays.push(local);
        records.push(json!({"name":case["name"],"canonical_inputs":case["inputs"],"reference":case["reference"],"encoded_launch":local}));
    }
    let mut shader = std::fs::read_to_string("scripts/optical_normal_step.wgsl").unwrap();
    shader.push_str(&std::fs::read_to_string("scripts/optical_normal_interval.wgsl").unwrap());
    let controller = std::fs::read_to_string("scripts/optical_normal_trace.wgsl").unwrap();
    assert_eq!(controller.matches("fn nc_residual(").count(), 1);
    // Retain the complete checked evaluator as a domain fallback. Only its
    // consumers use the separately reviewed bounded arithmetic specialization.
    let mut controller = controller
        .replace("fn nc_residual(", "fn nc_residual_reference(")
        .replace("nc_residual(", "nc_fast_residual(");
    let reference_start = controller.find("fn nc_residual_reference(").unwrap();
    let reference_end = controller.find("fn nc_height_speed(").unwrap();
    let reference = controller[reference_start..reference_end].to_string();
    const REFERENCE_SLOT: &str = "// RETAINED CHECKED RESIDUAL REFERENCE\n";
    controller.replace_range(reference_start..reference_end, REFERENCE_SLOT);
    for operation in ["add", "sub", "mul", "div"] {
        controller = controller.replace(
            &format!("nc_interval_{operation}("),
            &format!("nc_f_{operation}("),
        );
    }
    controller = controller.replace(REFERENCE_SLOT, &reference);
    shader.push_str(&controller);
    let fast = std::fs::read_to_string("scripts/optical_normal_residual_fast.wgsl").unwrap();
    assert_eq!(fast.matches("return nc_residual(").count(), 1);
    shader.push_str(&fast.replace("return nc_residual(", "return nc_residual_reference("));
    shader.push_str(&std::fs::read_to_string("scripts/optical_normal_interval_fast.wgsl").unwrap());
    shader.push_str(
        r#"
@group(0) @binding(1) var<storage,read> rays:array<vec4<f32>>;
@group(0) @binding(2) var<storage,read_write> result:array<vec4<u32>>;
@compute @workgroup_size(32)
fn normal_trace(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=arrayLength(&rays){return;}
    let launch=normalize(rays[id.x].xyz);let ray=nc_trace(launch);
    result[9u*id.x]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    result[9u*id.x+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    result[9u*id.x+2u]=vec4(ray.attempts,ray.roots,ray.reason,0u);
    result[9u*id.x+3u]=bitcast<vec4<u32>>(vec4(launch,0.0));
    result[9u*id.x+4u]=bitcast<vec4<u32>>(nc_debug0);
    result[9u*id.x+5u]=bitcast<vec4<u32>>(nc_debug1);
    result[9u*id.x+6u]=bitcast<vec4<u32>>(nc_debug2);
    result[9u*id.x+7u]=bitcast<vec4<u32>>(nc_debug3);
    result[9u*id.x+8u]=bitcast<vec4<u32>>(nc_debug4);
}


"#,
    );
    Fixture {
        shader: height_fixture::inject(shader),
        uniform: bytes(&uniform),
        rays: bytes(&rays),
        count: rays.len() as u32,
        record_words: 36,
        output_bytes: rays.len() as u64 * 144,
        cases: json!({"scope":"Seven canonical clear-ray inputs, whole atmosphere only. Native canonical scene contacts not included. Source/axis/launch packing and root failures remain visible; no runtime acceptance or timing claim.",
            "telemetry_records":["offset/proposed/tolerance/owner","before interval/after interval","last root residual/path error/charge","last root trial/step/allocation/stage","curve error/anchor high/anchor low/boundary flags"],
            "bounded_residual_specialization":true,
            "bounded_controller_interval_specialization":true,
            "hybrid_height_parameter":true,
            "source_gap_records":gaps,"initial_gap_records":[initial_lower,initial_upper],
            "initial_eye_ecef_m":input["origin_ecef_m"],"encoded_uniform":uniform,"initialized_refractivity":index,"initialized_slope":slope,"initialized_owner":owner,"eye_height_m":eye.height_m,"cases":records}),
    }
}

/// Actual attempted height trials; the first 36 terminal words stay intact.
pub fn build_capture() -> Fixture {
    height_trial_capture::inject(build())
}

/// Separate admitted-domain bound candidate; original reference stays in WGSL.
pub fn build_fast() -> Fixture {
    let mut fixture = build();
    fixture.shader = height_trial_split::inject_fast(fixture.shader);
    fixture.cases["upper_only_height_bounds"] = json!(true);
    fixture
}

/// Separately named endpoint-metric candidate. The original generic metric
/// remains available through one fallback and the old build_fast is unchanged.
pub fn build_metric_fast() -> Fixture {
    let mut fixture = build_fast();
    let original = "let metric=nh_state_per_height(start,curve);";
    assert_eq!(fixture.shader.matches(original).count(), 1);
    fixture.shader = fixture.shader.replace(
        original,
        "let metric=nh_state_per_height_fast(start,curve);",
    );
    let helpers = std::fs::read_to_string("scripts/optical_height_metric_fast.wgsl").unwrap();
    let bindings = fixture.shader.find("\n@group(0) @binding(1)").unwrap();
    fixture.shader.insert_str(bindings, &helpers);
    fixture.cases["upper_only_height_endpoint_metric"] = json!(true);
    fixture
}
