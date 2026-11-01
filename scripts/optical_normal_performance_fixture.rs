//! Original MIT. Math-owned full native-resolution atmospheric isolation.
//! This deliberately omits scene/h0 contacts. It is not runtime FPS or a proof
//! that adding geometry has a monotone timing cost on a different GPU program.
#[path = "optical_normal_trace_fixture.rs"]
mod clear;
#[path = "optical_height_rhs_fixture.rs"]
mod height_rhs_fixture;
use serde_json::{json, Value};

pub struct Fixture {
    pub shader: String,
    pub uniform: Vec<u8>,
    pub view: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub record_count: u32,
    pub record_words: u32,
    pub dispatch: [u32; 3],
    pub output_bytes: u64,
    pub sample_pixels: Vec<[u32; 2]>,
    pub sample_indices: Vec<u32>,
    pub cases: Value,
}
pub const VIEW_SOURCE: &str = "artifacts/optical-requested-stellar-sparse-chain-view.bin";
/// Execution-layout experiment only. Every invocation retains its original
/// global id, input ray and output record. No numerical source is rewritten.
/// Apply AFTER any selected-ray specialization, whose default entry is32.
pub fn with_workgroup_size(mut fixture: Fixture, group_size: u32) -> Fixture {
    assert!(matches!(group_size, 32 | 64 | 128 | 256));
    assert!(fixture.record_count > 0);
    let original = "@compute @workgroup_size(32)\nfn normal_performance";
    assert_eq!(fixture.shader.matches(original).count(), 1);
    let replacement = format!("@compute @workgroup_size({group_size})\nfn normal_performance");
    fixture.shader = fixture.shader.replacen(original, &replacement, 1);
    fixture.dispatch = [fixture.record_count.div_ceil(group_size), 1, 1];
    fixture.cases["workgroup_size"] = json!(group_size);
    fixture.cases["workgroup_specialization"] = json!(
        "Only the normal_performance workgroup declaration and ceil(record_count/group_size) dispatch differ. Global invocation id, per-ray source, inputs and output order are unchanged; all-word GPU comparison is required."
    );
    fixture
}
pub fn build() -> Fixture {
    build_mode(false, false)
}
pub fn build_fast() -> Fixture {
    build_mode(true, false)
}
pub fn build_metric_fast() -> Fixture {
    build_mode(true, true)
}
pub fn build_projected() -> Fixture {
    let mut fixture=build_metric_fast();
    fixture.shader=height_rhs_fixture::inject(fixture.shader);
    fixture.cases["projected_height_rhs"]=json!(true);
    fixture
}
fn build_mode(fast: bool, metric: bool) -> Fixture {
    let source = if metric {
        clear::build_metric_fast()
    } else if fast {
        clear::build_fast()
    } else {
        clear::build()
    };
    let mut shader = source.shader;
    let marker = "@group(0) @binding(1) var<storage,read> rays:";
    assert_eq!(shader.matches(marker).count(), 1);
    shader.truncate(shader.find(marker).unwrap());
    let view = std::fs::read(VIEW_SOURCE).unwrap();
    assert_eq!(view.len(), 80);
    let values: Vec<f32> = view
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let width = values[16] as u32;
    let height = values[17] as u32;
    assert_eq!((width, height), (1920, 1080));
    shader.push_str(r#"
struct NcView { values:array<vec4<f32>,5> };
@group(0) @binding(1) var<uniform> nc_view:NcView;
@group(0) @binding(2) var<storage,read_write> output:array<vec4<u32>>;
@compute @workgroup_size(32)
fn normal_performance(@builtin(global_invocation_id) id:vec3<u32>){
    let dimensions=vec2<u32>(nc_view.values[4].xy);if id.x>=dimensions.x*dimensions.y{return;}
    let pixel=vec2(id.x%dimensions.x,id.x/dimensions.x);let uv=(vec2<f32>(pixel)+vec2(0.5))/vec2<f32>(dimensions);
    let lens=nc_view.values[3];let xy=vec2((2.0*uv.x-1.0)*lens.x,1.0-2.0*uv.y);
    var optical=normalize(vec3(xy*tan(lens.y),1.0));
    if lens.z>0.5{let radius=length(xy);if radius>1.0{output[id.x*3u]=vec4(0u);output[id.x*3u+1u]=vec4(0u,0u,0u,3u);output[id.x*3u+2u]=vec4(0u);return;}let theta=radius*lens.y;let scale=select(lens.y,sin(theta)/max(radius,1e-8),radius>1e-8);optical=vec3(xy*scale,cos(theta));}
    let direction=normalize(nc_view.values[0].xyz*optical.x+nc_view.values[1].xyz*optical.y+nc_view.values[2].xyz*optical.z);
    let r=nc_trace(direction);
    output[id.x*3u]=bitcast<vec4<u32>>(vec4(r.p,r.path));
    output[id.x*3u+1u]=vec4(bitcast<vec3<u32>>(r.u),r.status);
    output[id.x*3u+2u]=vec4(r.attempts,r.roots,r.reason,0u);
}
"#);
    let sample_pixels: Vec<[u32; 2]> = [180, 540, 900]
        .into_iter()
        .flat_map(|y| [320, 960, 1600].into_iter().map(move |x| [x, y]))
        .collect();
    let sample_indices = sample_pixels.iter().map(|p| p[1] * width + p[0]).collect();
    Fixture {
        shader,
        uniform: source.uniform,
        view,
        width,
        height,
        output_bytes: u64::from(width) * u64::from(height) * 48,
        record_count: width * height,
        record_words: 12,
        dispatch: [(width * height).div_ceil(32), 1, 1],
        sample_pixels,
        sample_indices,
        cases: json!({"scope":"Full native-resolution normal-coordinate atmosphere-only isolation. Scene, h0 and vacuum-entry behavior are absent. No native FPS or monotone-cost lower-bound claim.",
            "view_source":VIEW_SOURCE,"view_values":values,
            "clear_ray_fixture":source.cases,
            "record_layout":["local p.xyz/path float bits","local u.xyz float bits/status u32","attempts/roots/reason/zero u32"]}),
    }
}

pub const SELECTION_SOURCE: &str = "artifacts/optical-height-performance-first-tail-selection.json";
pub fn build_selected() -> Fixture {
    select(build(), SELECTION_SOURCE)
}
pub fn build_selected_fast() -> Fixture {
    select(build_fast(), SELECTION_SOURCE)
}
pub fn build_selected_fast_from(selection_source: &str) -> Fixture {
    select(build_fast(), selection_source)
}
pub fn build_selected_metric_fast_from(selection_source: &str) -> Fixture {
    select(build_metric_fast(), selection_source)
}
pub fn build_selected_projected_from(selection_source: &str) -> Fixture {
    select(build_projected(),selection_source)
}
fn select(mut fixture: Fixture, selection_source: &str) -> Fixture {
    let selection: Value =
        serde_json::from_slice(&std::fs::read(selection_source).unwrap()).unwrap();
    let samples = selection["samples"].as_array().unwrap();
    let pixels: Vec<[u32; 2]> = samples
        .iter()
        .map(|s| {
            [
                u32::try_from(s["pixel"][0].as_u64().unwrap()).unwrap(),
                u32::try_from(s["pixel"][1].as_u64().unwrap()).unwrap(),
            ]
        })
        .collect();
    assert!(!pixels.is_empty() && pixels.len() <= 32);
    assert!(pixels
        .iter()
        .all(|p| p[0] < fixture.width && p[1] < fixture.height));
    let literal = pixels
        .iter()
        .map(|p| format!("vec2({}u,{}u)", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(",");
    let declaration = format!(
        "\nconst NH_SELECTED:array<vec2<u32>,{}>=array<vec2<u32>,{}>({literal});\n",
        pixels.len(),
        pixels.len()
    );
    let entry = fixture
        .shader
        .find("@compute @workgroup_size(32)\nfn normal_performance")
        .unwrap();
    fixture.shader.insert_str(entry, &declaration);
    assert_eq!(
        fixture
            .shader
            .matches("if id.x>=dimensions.x*dimensions.y{return;}")
            .count(),
        1
    );
    assert_eq!(
        fixture
            .shader
            .matches("let pixel=vec2(id.x%dimensions.x,id.x/dimensions.x);")
            .count(),
        1
    );
    fixture.shader = fixture.shader.replace(
        "if id.x>=dimensions.x*dimensions.y{return;}",
        &format!("if id.x>={}u{{return;}}", pixels.len()),
    );
    fixture.shader = fixture.shader.replace(
        "let pixel=vec2(id.x%dimensions.x,id.x/dimensions.x);",
        "let pixel=NH_SELECTED[id.x];",
    );
    fixture.shader = fixture.shader.replace("id.x*3u", "id.x*9u");
    let marker = "    output[id.x*9u+2u]=vec4(r.attempts,r.roots,r.reason,0u);";
    assert_eq!(fixture.shader.matches(marker).count(), 1);
    fixture.shader=fixture.shader.replace(marker,&format!("{marker}\n    output[id.x*9u+3u]=bitcast<vec4<u32>>(vec4(direction,0.0));\n    output[id.x*9u+4u]=bitcast<vec4<u32>>(nc_debug0);\n    output[id.x*9u+5u]=bitcast<vec4<u32>>(nc_debug1);\n    output[id.x*9u+6u]=bitcast<vec4<u32>>(nc_debug2);\n    output[id.x*9u+7u]=bitcast<vec4<u32>>(nc_debug3);\n    output[id.x*9u+8u]=bitcast<vec4<u32>>(nc_debug4);"));
    let height_attempt = "        let height=nh_attempt(state,height_proposed,adaptive,tolerance);";
    assert_eq!(fixture.shader.matches(height_attempt).count(), 1);
    fixture.shader=fixture.shader.replace(height_attempt,&format!("{height_attempt}\n        nc_debug1=vec4(state.q.yz,height_proposed,f32(nh_attempts));\n        nc_debug3=vec4(dot(state.u,nc_normal(state.q)),height.attempted_height,f32(height.kind),f32(nh_accepted));"));
    fixture.record_count = pixels.len() as u32;
    fixture.record_words = 36;
    fixture.dispatch = [fixture.record_count.div_ceil(32), 1, 1];
    fixture.output_bytes = u64::from(fixture.record_count) * 144;
    fixture.sample_indices = (0..fixture.record_count).collect();
    fixture.sample_pixels = pixels;
    fixture.cases["scope"] = json!(
        "Selected full-frame diagnostic rays: same retained camera, pixel-center expression and physical solver; instrumentation/compiler context must be compared to the retained first12 words. No timing/performance acceptance."
    );
    fixture.cases["selection"] = selection;
    fixture.cases["selection_source"] = json!(selection_source);
    fixture.cases["record_layout"] = json!([
        "first12 original terminal words",
        "exact GPU launch and zero",
        "offset/path proposal/tolerance/owner",
        "shared: last H attempt chart a/b/height proposal/height attempts OR later path boundary before/after intervals",
        "shared: last H residual/metric/charge OR later path root residual/path error/charge",
        "shared: last H attempt vertical/attempted H/kind/accepted H count OR later path root theta/step/allocation/stage",
        "shared: last H accepted error/anchor high/low/preincrement accepted H count OR later path curve error/anchor/boundary flags"
    ]);
    fixture
}
