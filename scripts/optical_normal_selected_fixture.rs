//! Original MIT. Unregistered selected real-ray producer/scene continuation.
//! The original accepted-trial/controller expressions are extracted verbatim;
//! persistent names and the suspension boundary are the only adaptations.
use crate::fixture;
use serde_json::json;
#[path = "optical_normal_selected_split.rs"]
mod split_certificate;
pub struct SelectedFixture {
    pub scene: fixture::Fixture,
    pub producer_shader: String,
    pub consumer_shader: String,
    pub certificate_shader: Option<String>,
    pub state_bytes: u64,
    pub piece_bytes: u64,
    pub progress_bytes: u64,
    pub rounds: u32,
}
fn function(source: &str, name: &str) -> std::ops::Range<usize> {
    let key = format!("fn {name}(");
    assert_eq!(source.matches(&key).count(), 1);
    let start = source.find(&key).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, b) in source.as_bytes()[open..].iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        }
        if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                return start..open + i + 1;
            }
        }
    }
    panic!("Unterminated function {name}")
}
fn replace(source: &mut String, old: &str, new: &str) {
    assert_eq!(
        source.matches(old).count(),
        1,
        "Selected source drift: {old}"
    );
    *source = source.replacen(old, new, 1);
}
fn remove_entry(source: &mut String, name: &str) {
    let mut span = function(source, name);
    let prefix = &source[..span.start];
    let at = prefix.rfind("@compute").unwrap();
    assert!(prefix[at..].starts_with("@compute @workgroup_size("));
    assert!(!prefix[at..].contains('}'));
    span.start = at;
    source.replace_range(span, "");
}
fn persistent(source: &str) -> String {
    let names = [
        "state",
        "path",
        "proposed",
        "height_proposed",
        "height_retries",
        "owner",
    ];
    let bytes = source.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let name = &source[start..i];
            if names.contains(&name) && (start == 0 || bytes[start - 1] != b'.') {
                out.push_str("ns.");
            }
            out.push_str(name);
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}
fn controller() -> String {
    let source = std::fs::read_to_string("scripts/optical_normal_scene_trace.wgsl")
        .unwrap()
        .replace("\r\n", "\n");
    let loop_start = source
        .find("    for(var attempt=1u;attempt<=4096u;attempt+=1u){")
        .unwrap();
    let body_start = loop_start + source[loop_start..].find('{').unwrap() + 1;
    let query = "        let event=nc_scene_query(accepted_curve,tolerance);";
    assert_eq!(source.matches(query).count(), 1);
    let query_start = source.find(query).unwrap();
    let final_end = source.rfind("    }\n    return nc_result").unwrap();
    let prefix = persistent(&source[body_start..query_start]);
    let mut suffix = source[query_start + query.len()..final_end].to_owned();
    suffix = suffix
        .replace("height.attempted_height", "ns.height_attempted")
        .replace("height.path", "ns.height_path");
    assert_eq!(suffix.matches("continue;").count(), 1);
    suffix = suffix.replace("continue;", "return ns_signal(4u);");
    suffix = persistent(&suffix);
    format!(
        r#"
fn ns_signal(kind:u32)->NcResult{{return NcResult(vec3(0.0),vec3(0.0),0.0,kind,ns.attempt,nc_root_count,0u);}}
fn ns_commit(event:NcSceneEvent)->NcResult{{
    let count=u32(nc_settings.initial.w);let attempt=ns.attempt;let position=nc_position(ns.state.q);let tolerance=ns.tolerance;
    let accepted_curve=ns.curve;let adaptive=ns.adaptive;let is_height=ns.is_height!=0u;
    var next=ns.next;let delta_path=ns.delta_path;let crossing=ns.crossing;
    let accepted_error=ns.accepted_error;let attempted_step=ns.attempted_step;
{suffix}
    return ns_signal(4u);
}}
fn ns_attempt()->NcResult{{
    let count=u32(nc_settings.initial.w);
    for(var attempt=ns.attempt+1u;attempt<=4096u;attempt+=1u){{
        ns.attempt=attempt;
{prefix}
        ns.curve=accepted_curve;ns.next=next;ns.delta_path=delta_path;ns.crossing=crossing;
        ns.accepted_error=accepted_error;ns.attempted_step=attempted_step;ns.adaptive=adaptive;
        ns.height_attempted=height.attempted_height;ns.height_path=height.path;ns.is_height=select(0u,1u,is_height);ns.tolerance=tolerance;
        if nc_scene_canonical_above_ceiling(accepted_curve,nc_scene_anchor_residual,nc_certificates[4].z){{
            let result=ns_commit(nc_scene_none());if result.status!=4u{{return result;}}continue;
        }}
        ns.phase=1u;return ns_signal(5u);
    }}
    return nc_result(ns.state,ns.path,2u,4096u,6u);
}}
"#
    )
}
fn initialization(scene: &fixture::Fixture) -> String {
    let declaration = scene
        .sample_pixels
        .iter()
        .map(|p| format!("vec2({}u,{}u)", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(",");
    let original = &scene.shader[function(&scene.shader, "normal_scene")];
    let begin = original.find("    let pixel=").unwrap();
    let end = original
        .find("    let ray=nc_trace_scene(direction);")
        .unwrap();
    let mut lens = original[begin..end].to_owned();
    lens = lens.replace(
        "terminals[detail_index+1u]=vec4(0u);return;",
        "terminals[detail_index+1u]=vec4(0u);ns.phase=4u;ns_store();return;",
    );
    assert!(lens.contains("ns.phase=4u;"));
    format!(
        r#"
const NC_SCENE_PIXELS:array<vec2<u32>,{count}>=array<vec2<u32>,{count}>({declaration});
@compute @workgroup_size(32)
fn normal_selected_init(@builtin(global_invocation_id) id:vec3<u32>){{
    if id.x>=NS_COUNT{{return;}}if id.x==0u{{terminals[0]=vec4(0u);}}ns.id=id.x;
    let index=1u+3u*id.x;let detail_index=1u+3u*NS_COUNT+2u*id.x;
{lens}
    let axis=vec3(0.0,nc_settings.pose.x,-nc_settings.pose.y);nc_axis=axis*inverseSqrt(dot(axis,axis));
    nc_eye_height=nc_settings.pose.zw;nc_anchor=nc_eye_height;nc_index=nc_settings.initial.xy;
    nc_lower_gap=nc_settings.initial_lower.xyz;nc_upper_gap=nc_settings.initial_upper.xyz;
    nc_scene_anchor_residual=nc_certificates[4].xy;ns.owner=u32(nc_settings.initial.z);
    ns.state=NcState(vec3(0.0),direction);ns.proposed=min(2000.0,nc_settings.limits.y);ns.height_proposed=ns.proposed*abs(direction.y);
    let count=u32(nc_settings.initial.w);
    if count<2u||count>128u||ns.owner>=count{{ns_publish(nc_result(ns.state,0.0,2u,0u,1u));return;}}
    ns_store();
}}
"#,
        count = scene.record_count
    )
}
pub fn build() -> SelectedFixture {
    let mut scene = fixture::build(true);
    assert_eq!(scene.record_count, 23);
    let mut shader = fixture::build_cartesian_covered().scene.shader;
    remove_entry(&mut shader, "normal_scene");
    for name in ["nc_cartesian_input_vector", "nc_cartesian_load"] {
        let range = function(&shader, name);
        shader.replace_range(range, "");
    }
    replace(
        &mut shader,
        "@group(0) @binding(6) var<storage,read> nc_curve_inputs:array<vec4<u32>>;",
        "",
    );
    replace(
        &mut shader,
        "fn nc_scene_face_event_cached(curve:NcSceneCurve,parameters:NcMapParameters,address:u32,material:u32,owner:u32,width:f32,tolerance:f32,context:NcFaceContext)->NcSceneEvent{",
        "fn nc_scene_face_event_cached(curve:NcSceneCurve,parameters:NcMapParameters,address:u32,material:u32,owner:u32,width:f32,tolerance:f32,context:NcFaceContext,best:NcSceneEvent)->NcSceneEvent{",
    );
    replace(
        &mut shader,
        "let face=nc_scene_face(address);let contact=nc_face_contact_cached(curve,parameters,face,width,context);",
        "let face=nc_scene_face(address);if nc_cartesian_face_is_later(curve,face,context,best){return nc_scene_none();}let contact=nc_face_contact_cached(curve,parameters,face,width,context);",
    );
    replace(
        &mut shader,
        "nc_scene_face_event_cached(curve,parameters,face_address,material,owner,width,tolerance,context)",
        "nc_scene_face_event_cached(curve,parameters,face_address,material,owner,width,tolerance,context,result)",
    );
    shader.push_str(&format!("\nconst NS_COUNT:u32={}u;\n", scene.record_count));
    for path in [
        "scripts/optical_cartesian_chronology.wgsl",
        "scripts/optical_cartesian_producer.wgsl",
        "scripts/optical_normal_selected_state.wgsl",
    ] {
        shader.push('\n');
        shader.push_str(&std::fs::read_to_string(path).unwrap());
    }
    shader.push_str(&controller());
    shader.push_str(&initialization(&scene));
    shader.push_str(
        &std::fs::read_to_string("scripts/optical_normal_selected_control.wgsl").unwrap(),
    );
    let mut producer_shader = shader.clone();
    remove_entry(&mut producer_shader, "normal_selected_consume");
    let mut consumer_shader = shader;
    for name in [
        "normal_selected_init",
        "normal_selected_produce",
        "normal_selected_finalize",
    ] {
        remove_entry(&mut consumer_shader, name);
    }
    let n = u64::from(scene.record_count);
    let rounds = 1024;
    scene.cases["selected_continuation"] = json!({
        "scope":"Unregistered selected complete-ray canonical continuation. Same accepted H/path trial and source/scene commit policy; producer advances certified above-ceiling intervals internally. Other intervals require finite Cartesian certificates and chronological scene results before commit. Fixed operational rounds do not reset4096 attempts and never publish a valid header for unfinished rays. This is not native or full-view performance acceptance.",
        "state_vec4_per_ray":32,"piece_vec4_per_ray":20,"rounds":rounds,
        "phases":{"0":"attempt","1":"certificate needed","2":"scene input ready","3":"scene result ready","4":"terminal"},
        "packing":"All continuation float payloads bitcast. Original curve.theta_end remains stored; consumer queries min(original prefix,piece end). Mathematical full-quartic certificate extension grants no physical continuation.",
        "limits":"Current fixed in-atmosphere pose only; vacuum/exact-node matrix and runtime immutable patch preparation remain separate gates. All uncertainty retained. No fallback falsely labeled canonical success."
    });
    SelectedFixture {
        scene,
        producer_shader,
        consumer_shader,
        certificate_shader: None,
        state_bytes: n * 32 * 16,
        piece_bytes: n * 20 * 16,
        progress_bytes: 16,
        rounds,
    }
}

pub fn build_split_certificate() -> SelectedFixture {
    split_certificate::build()
}
