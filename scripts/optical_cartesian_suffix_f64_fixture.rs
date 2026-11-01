//! Original MIT. Separately authorized, selected-only f64 suffix feasibility.
//! The original prefix/fallback and all previously rejected fixtures are kept.
use super::suffix::{self, SuffixFixture};
use super::suffix_trace::TraceFixture;
#[path = "optical_cartesian_suffix_atmosphere_fixture.rs"]
mod atmosphere;
#[path = "optical_shader_reachability.rs"]
mod reachability;
use serde_json::json;

fn close(source: &str, open: usize) -> usize {
    let mut depth = 0;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        if *byte == b'{' {
            depth += 1;
        }
        if *byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return open + offset;
            }
        }
    }
    panic!("Unclosed source block")
}
fn function(source: &str, name: &str) -> String {
    let marker = format!("fn {name}(");
    assert_eq!(source.matches(&marker).count(), 1, "Function drift: {name}");
    let start = source.find(&marker).unwrap();
    let open = start + source[start..].find('{').unwrap();
    source[start..=close(source, open)].to_owned()
}
fn structure(source: &str, name: &str) -> String {
    let marker = format!("struct {name}");
    assert_eq!(
        source.matches(&marker).count(),
        1,
        "Structure drift: {name}"
    );
    let start = source.find(&marker).unwrap();
    let open = start + source[start..].find('{').unwrap();
    format!("{};", &source[start..=close(source, open)])
}
fn once(source: &mut String, old: &str, new: &str) {
    assert_eq!(source.matches(old).count(), 1, "F64 source drift: {old}");
    *source = source.replacen(old, new, 1);
}
fn function_names(source: &str) -> Vec<String> {
    source
        .split("fn ")
        .skip(1)
        .map(|tail| {
            let end = tail.find('(').unwrap();
            let name = &tail[..end];
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            name.to_owned()
        })
        .collect()
}

fn widen_settings(source: &str) -> String {
    // Upload values remain exactly their original f32 values. Widen on use;
    // no per-ray private copy of the complete 128-layer uniform is created.
    let mut result = source.to_owned();
    while let Some(start) = result.find("settings.layers[") {
        let index = start + "settings.layers[".len();
        let end = index + result[index..].find(']').unwrap();
        let expression = result[index..end].to_owned();
        assert!(!expression.contains('['));
        result.replace_range(start..=end, &format!("cf64_layer({expression})"));
    }
    for field in ["q0", "q1", "q2", "origin", "budget", "scene"] {
        result = result.replace(&format!("settings.{field}"), &format!("cf64_{field}()"));
    }
    assert!(
        !result.contains("settings."),
        "Unreviewed promoted uniform member"
    );
    result
}

fn promote(source: &str) -> String {
    // Identifier tokens only. Rational literal spellings/order are unchanged.
    // geometry is a function AND a DpStep member, so only calls are renamed.
    let source = source.replace("geometry(", "cf64_geometry(");
    let mut result = String::new();
    let mut offset = 0;
    let bytes = source.as_bytes();
    while offset < bytes.len() {
        if bytes[offset].is_ascii_alphabetic() || bytes[offset] == b'_' {
            let end = (offset + 1..bytes.len())
                .find(|&i| !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_'))
                .unwrap_or(bytes.len());
            let word = &source[offset..end];
            if word == "f32" {
                result.push_str("f64");
            } else if word.starts_with("dp_")
                || [
                    "State",
                    "Trace",
                    "DpStep",
                    "DpContact",
                    "qmul",
                    "knot_bound",
                    "trace_result",
                    "dopri_delta",
                    "cs_suffix_trace",
                ]
                .contains(&word)
            {
                result.push_str("cf64_");
                result.push_str(word);
            } else {
                result.push_str(word);
            }
            offset = end;
        } else {
            result.push(char::from(bytes[offset]));
            offset += 1;
        }
    }
    result = result
        .replace("vec2(", "vec2<f64>(")
        .replace("vec3(", "vec3<f64>(")
        .replace("vec4(", "vec4<f64>(")
        .replace("bitcast<vec3<u32>>", "bitcast<vec3<u64>>")
        .replace("var h=0.0", "var h:f64=0.0")
        .replace("var low=0.0", "var low:f64=0.0")
        .replace("var allocation=0.0", "var allocation:f64=0.0")
        .replace("var contact_height=0.0", "var contact_height:f64=0.0");
    result
}

fn promoted_suffix(original: &str) -> String {
    let mut body = function(original, "cs_suffix_trace");
    let begin = body
        .find("    for(var i=i32(prefix_attempts);i<4096;i+=1){cs_suffix_attempts+=1u;")
        .unwrap();
    body.replace_range(..begin, "fn cs_suffix_trace(id:u32)->Trace{\n");
    once(
        &mut body,
        "let tolerance=max(0.002,4e-7*length(state.p));",
        "let tolerance=0.25*max(0.002,4e-7*length(state.p));",
    );
    let old = r#"        let scene_error=nc_interval_add(nc_interval_exact(accepted.error),nc_interval_exact(allocation));
        if !nc_interval_valid(scene_error) || !(scene_error.y<=tolerance) || !cs_curve_scene_clear(state,step,scene_error.y){
            cs_failure_reason=3u;return trace_result(state.p,state.u,path,2u);
        }"#;
    let new = r#"        if !cf64_curve_scene_clear(state.p,dp_curve.delta.p,dp_curve.r3,dp_curve.r4,dp_curve.r5,step,dp_curve_step,accepted.error,allocation,tolerance){
            cs_failure_reason=3u;return trace_result(state.p,state.u,path,2u);
        }"#;
    once(&mut body, old, new);
    let mut promoted = promote(&widen_settings(&body));
    once(
        &mut promoted,
        "fn cf64_cs_suffix_trace(id:u32)->cf64_Trace{",
        INITIALIZE,
    );
    once(
        &mut promoted,
        "for(var i=i32(prefix_attempts);i<4096;i+=1){cs_suffix_attempts+=1u;",
        "for(var i=i32(prefix_attempts);i<4096;i+=1){cs_suffix_attempts+=1u;\n        if i==i32(prefix_attempts) && !cf64_initial_owner(state,g,cf64_dp_layer){cs_failure_reason=2u;return cf64_trace_result(state.p,state.u,path,2u);}",
    );
    assert!(!promoted.contains("nc_interval_"));
    assert!(!promoted.contains("cs_curve_scene_clear("));
    assert!(!promoted.contains("f32"));
    // The f32 bitcasts in the handoff loader are the only intended exception.
    promoted
}

pub fn build(case: &str) -> SuffixFixture {
    assert!(["selected", "atmosphere-100-gradient"].contains(&case));
    let base = suffix::build(true);
    let mut wrapper = TraceFixture {
        suffix: base,
        auxiliary_bytes: 0,
        log_words_per_record: 0,
        max_steps: 4096,
    };
    if case != "selected" {
        atmosphere::apply(case, &mut wrapper);
    }
    let mut result = wrapper.suffix;
    assert_eq!(result.scene.record_count, 23);
    assert_eq!(
        (
            result.state_words,
            result.state_bytes,
            result.progress_bytes
        ),
        (24, 23 * 96, 16)
    );
    assert_eq!(
        result.modules.iter().map(|m| m.name).collect::<Vec<_>>(),
        ["prefix", "suffix", "fallback"]
    );
    let prefix = result.modules[0].shader.clone();
    let fallback = result.modules[2].shader.clone();
    let old = result.modules[1].shader.clone();
    assert!(!old.contains("cst_log"));
    let mut numeric = String::new();
    for name in ["State", "Trace", "DpStep", "DpContact"] {
        numeric.push_str(&structure(&old, name));
        numeric.push('\n');
    }
    for name in function_names(&old) {
        if name.starts_with("dp_")
            || [
                "qmul",
                "geometry",
                "knot_bound",
                "trace_result",
                "dopri_delta",
            ]
            .contains(&name.as_str())
        {
            numeric.push_str(&function(&old, &name));
            numeric.push('\n');
        }
    }
    numeric.push_str("var<private> dp_layer:i32;\nvar<private> dp_curve:DpStep;\nvar<private> dp_curve_step:f32;\n");
    let promoted = promote(&widen_settings(&numeric));
    let controller = promoted_suffix(&old);
    let bridge = std::fs::read_to_string("scripts/optical_cartesian_f64_clearance.wgsl")
        .unwrap()
        .replace("\r\n", "\n");
    let mut shader = old;
    let old_entry = function(&shader, "cs_suffix");
    once(&mut shader, &old_entry, ENTRY);
    shader.push_str(&format!(
        "\n{promoted}\n{ACCESSORS}\n{controller}\n{bridge}\n"
    ));
    let (shader, packaging) = reachability::prune(&shader);
    result.modules[1].shader = shader;
    assert_eq!(result.modules[0].shader, prefix);
    assert_eq!(result.modules[2].shader, fallback);
    result.scene.shader = prefix;
    result.scene.cases["cartesian_suffix_f64"] = json!({
        "source":"scripts/optical_cartesian_suffix_f64_fixture.rs",
        "case":case,"selected":true,"timing_permitted":false,
        "design":"docs/science/cartesian-f64-suffix-design.md",
        "clearance_source":"scripts/optical_cartesian_f64_clearance.wgsl",
        "scope":"Separately authorized coherent f64 controller feasibility. Prior component failures remain failures. Exact unlogged legacy prefix/fallback, uploaded f32 constants widened on access, raw initial u, fixed quarter-tolerance, unchanged source/scene/cap policy. No production registration or timing claim.",
        "initialization":"Exact f32 handoff p/u/path/entry widen once; g recomputed by promoted original geometry. Saved g remains provenance. Strict owner/domain agreement checked inside first counted iteration. Failure restarts original-eye fallback.",
        "precision_boundary":"All suffix state, stage, field, dense, source-root, error, proposal, commit and path arithmetic is f64. Immutable rows/layers widen on access; no private whole-profile copy. Only successful sky terminal fields cast to f32 once. Scene exclusion receives outward enclosures, never rounded singletons.",
        "factor":0.25,"maximum_candidate_attempts":4096,
        "required_suffix_records":if case=="selected"{vec![3,9,10,11,12,14,15,17,18,20]}else{(0..23).collect::<Vec<_>>()},
        "acceptance":"Exact inputs/launches/handoffs and all original terminals; all frozen same-ray metrics, plus absolute atmosphere limits for that packet. A lost required suffix route is failure, not concealed by fallback. Both selected cases precede full branch/atmosphere/turning matrix; timing remains disabled.",
        "state_words_per_record":24,"auxiliary_log_present":false,"reachability":packaging
    });
    result
}

const INITIALIZE: &str = r#"fn cf64_cs_suffix_trace(id:u32)->cf64_Trace{
    let position=cf64_saved_position(id);let direction=cf64_saved_direction(id);
    var state=cf64_State(position.xyz,direction.xyz);var path=position.w;let entry=direction.w;
    var proposed:f64=100000.0;var g=cf64_geometry(state.p);
    cf64_dp_layer=bitcast<i32>(cs_states[6u*id+5u].x);var departing=0;
    let physical=true;cs_suffix_attempts=0u;cs_failure_reason=2u;
    let prefix_attempts=cs_states[6u*id+3u].x;
"#;

const ACCESSORS: &str = r#"
fn cf64_q0()->vec4<f64>{return vec4<f64>(settings.q0);}
fn cf64_q1()->vec4<f64>{return vec4<f64>(settings.q1);}
fn cf64_q2()->vec4<f64>{return vec4<f64>(settings.q2);}
fn cf64_origin()->vec4<f64>{return vec4<f64>(settings.origin);}
fn cf64_budget()->vec4<f64>{return vec4<f64>(settings.budget);}
fn cf64_scene()->vec4<f64>{return vec4<f64>(settings.scene);}
fn cf64_layer(index:u32)->vec4<f64>{return vec4<f64>(settings.layers[index]);}
fn cf64_saved_position(id:u32)->vec4<f64>{return vec4<f64>(bitcast<vec4<f32>>(cs_states[6u*id]));}
fn cf64_saved_direction(id:u32)->vec4<f64>{return vec4<f64>(bitcast<vec4<f32>>(cs_states[6u*id+1u]));}
fn cf64_initial_owner(state:cf64_State,g:vec4<f64>,owner:i32)->bool{
    let budget=cf64_budget();let scene=cf64_scene();
    if !cf64_dp_finite(budget)||!cf64_dp_finite(scene)||!cf64_dp_finite(g)||!cf64_dp_finite(vec4<f64>(state.p,0.0))||!cf64_dp_finite(vec4<f64>(state.u,0.0)){return false;}
    if !(budget.x>=2.0&&budget.x<=128.0&&floor(budget.x)==budget.x){return false;}
    let count=u32(budget.x);
    if owner< -1||owner>=i32(count)-1||scene.y!=cf64_layer(0u).x||budget.y!=cf64_layer(count-1u).x{return false;}
    if !(g.w>scene.z&&g.w<budget.y)||g.w==0.0||!(dot(state.u,state.u)>0.0){return false;}
    for(var i=0u;i<count-1u;i+=1u){
        let a=cf64_layer(i);let b=cf64_layer(i+1u);
        if !cf64_dp_finite(a)||!cf64_dp_finite(b)||!(a.x<b.x){return false;}
    }
    if owner<0{return g.w<scene.y;}
    return g.w>cf64_layer(u32(owner)).x&&g.w<cf64_layer(u32(owner)+1u).x;
}
"#;

const ENTRY: &str = r#"fn cs_suffix(@builtin(global_invocation_id) invocation:vec3<u32>){
    let id=invocation.x;if id>=23u{return;}
    if cs_states[6u*id+2u].w!=1u{return;}
    let ray=cf64_cs_suffix_trace(id);cs_states[6u*id+3u].y=cs_suffix_attempts;
    if ray.status==0u&&cf64_dp_finite(vec4<f64>(ray.p,ray.path))&&cf64_dp_finite(vec4<f64>(ray.u,0.0)){
        cs_states[6u*id+3u].w=0u;
        cs_publish(id,trace_result(vec3<f32>(ray.p),vec3<f32>(ray.u),f32(ray.path),0u),1u);return;
    }
    cs_states[6u*id+3u].w=cs_failure_reason;cs_states[6u*id+2u].w=2u;
}"#;
