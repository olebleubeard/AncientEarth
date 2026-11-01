//! Original MIT. Unregistered one-handoff Cartesian atmospheric diagnostic.
//! Legacy prefix and original-eye fallback preserve the original expressions.
//! The separately reviewed clearance helper can only exclude scene contacts.
use super::fixture;
#[path = "optical_shader_reachability.rs"]
mod reachability;
use serde_json::json;

pub struct Module {
    pub name: &'static str,
    pub shader: String,
}
pub struct Step {
    pub module_index: usize,
    pub entry: &'static str,
    pub dispatch: [u32; 3],
    pub repetitions: u32,
}
pub struct SuffixFixture {
    pub scene: fixture::Fixture,
    pub modules: Vec<Module>,
    pub steps: Vec<Step>,
    pub state_words: u32,
    pub state_bytes: u64,
    pub progress_bytes: u64,
}
fn close(source: &str, open: usize) -> usize {
    let mut depth = 0_u32;
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
fn span(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(
        source.matches(&signature).count(),
        1,
        "Function source drift: {name}"
    );
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    start..close(source, open) + 1
}
fn replace(source: &mut String, old: &str, new: &str, count: usize) {
    assert_eq!(
        source.matches(old).count(),
        count,
        "Suffix source drift: {old}"
    );
    *source = source.replace(old, new);
}
fn prefix_returns(source: &str) -> (String, usize) {
    let mut output = String::new();
    let mut cursor = 0;
    let mut count = 0;
    while let Some(relative) = source[cursor..].find("return ") {
        let start = cursor + relative;
        let end = start + source[start..].find(';').unwrap();
        let expression = &source[start + 7..end];
        assert!(expression.starts_with("trace_result(") || expression.starts_with("Trace("));
        output.push_str(&source[cursor..start]);
        output.push_str(&format!("cs_prefix_done(id,{expression});return;"));
        cursor = end + 1;
        count += 1;
    }
    output.push_str(&source[cursor..]);
    (output, count)
}
fn prefix_trace(original: &str) -> String {
    let (mut source, count) = prefix_returns(original);
    assert_eq!(count, 15); // Three initializer, eleven attempt, final exhaustion.
    replace(
        &mut source,
        "fn trace(direction:vec3<f32>)->Trace{",
        "fn cs_prefix_trace(direction:vec3<f32>,id:u32){cs_prefix_attempts=0u;var can_handoff=false;",
        1,
    );
    replace(
        &mut source,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=0;i<4096;i+=1){cs_prefix_attempts=u32(i)+1u;",
        1,
    );
    // Only a previously accepted state is eligible. Original terminal checks
    // above this insertion execute first. A handoff counts prior COMPLETE
    // attempts, not this iteration's preliminary terminal/eligibility checks.
    replace(
        &mut source,
        "        // Retain the h0 event bound even where terrain masks that surface:",
        r#"
        if can_handoff {
            let owner=cs_initial_owner(state,g);
            if owner>=-1 && g.w!=0.0 && cs_point_scene_clear(state.p,max(0.002,4e-7*length(state.p))){
                cs_handoff(id,state,path,entry,g,owner,u32(i));return;
            }
            can_handoff=false;
        }
        // Retain the h0 event bound even where terrain masks that surface:"#,
        1,
    );
    replace(
        &mut source,
        "state=half;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);",
        "state=half;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);can_handoff=true;",
        1,
    );
    source
}
fn suffix_trace(original: &str) -> String {
    let function = span(original, "dp_trace");
    let mut source = original[function].to_owned();
    let loop_start = source.find("    for(var i=0;i<4096;i+=1){").unwrap();
    source.replace_range(
        ..loop_start,
        r#"fn cs_suffix_trace(id:u32)->Trace{
    let position=bitcast<vec4<f32>>(cs_states[6u*id]);
    let direction=bitcast<vec4<f32>>(cs_states[6u*id+1u]);
    var state=State(position.xyz,direction.xyz);var path=position.w;let entry=direction.w;
    var proposed=100000.0;var g=bitcast<vec4<f32>>(cs_states[6u*id+4u]);
    dp_layer=bitcast<i32>(cs_states[6u*id+5u].x);var departing=0;
    let physical=true;cs_suffix_attempts=0u;cs_failure_reason=2u;
    let prefix_attempts=cs_states[6u*id+3u].x;
"#,
    );
    replace(
        &mut source,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=i32(prefix_attempts);i<4096;i+=1){cs_suffix_attempts+=1u;",
        1,
    );
    let h0 = "        if physical && g.w==0.0 && vertical<0.0{";
    let start = source.find(h0).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let end = close(&source, open) + 1;
    source.replace_range(
        start..end,
        "        if g.w==0.0{cs_failure_reason=4u;return trace_result(state.p,state.u,path,2u);}",
    );
    // An uncertain possible h0 contact requests the unchanged original trace;
    // no suffix surface or coverage decision is ever published.
    replace(
        &mut source,
        "        if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe || !reference_safe{",
        "        if !reference_safe{cs_failure_reason=4u;return trace_result(state.p,state.u,path,2u);}\n        if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe || !reference_safe{",
        1,
    );
    let begin = source
        .find("        var reference_distance=-1.0;var reference=accepted;")
        .unwrap();
    let end = source[begin..]
        .find("        if contact>0 && contact_height==settings.budget.y")
        .unwrap()
        + begin;
    source.replace_range(begin..end, r#"
        if (g.w>0.0 && accepted.geometry.w<=0.0)||(g.w<0.0 && accepted.geometry.w>=0.0){
            cs_failure_reason=4u;return trace_result(state.p,state.u,path,2u);
        }
        // dp_curve and dp_curve_step still describe the ORIGINAL full trial.
        // Source clipping changed only accepted/step; the helper encloses that
        // original polynomial over the outward clipped parameter interval.
        let scene_error=nc_interval_add(nc_interval_exact(accepted.error),nc_interval_exact(allocation));
        if !nc_interval_valid(scene_error) || !(scene_error.y<=tolerance) || !cs_curve_scene_clear(state,step,scene_error.y){
            cs_failure_reason=3u;return trace_result(state.p,state.u,path,2u);
        }
"#);
    let tail = "    return trace_result(state.p,state.u,path,2u);\n}";
    assert!(source.ends_with(tail));
    let start = source.len() - tail.len();
    source.replace_range(
        start..,
        "    cs_failure_reason=5u;return trace_result(state.p,state.u,path,2u);\n}",
    );
    assert!(!source.contains("scene_event("));
    assert!(!source.contains("terrain_coverage("));
    source
}

pub fn build(selected: bool) -> SuffixFixture {
    let mut scene = fixture::build_reference(selected);
    scene.shader = scene.shader.replace("\r\n", "\n");
    let count = scene.record_count;
    let original = scene.shader[span(&scene.shader, "trace")].to_owned();
    let entry_span = span(&scene.shader, "normal_scene");
    let entry = scene.shader[entry_span.clone()].to_owned();
    let entry_attribute = "@compute @workgroup_size(32)\n";
    assert!(scene.shader[..entry_span.start].ends_with(entry_attribute));
    let mut base = scene.shader.clone();
    base.replace_range(entry_span.start - entry_attribute.len()..entry_span.end, "");
    let mut counted = original.clone();
    replace(
        &mut counted,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=0;i<4096;i+=1){cs_fallback_attempts+=1u;",
        1,
    );
    base.replace_range(span(&base, "trace"), &counted);
    let dp = std::fs::read_to_string("src/desktop/shaders/refraction_dopri_hermite.wesl")
        .unwrap()
        .replace("\r\n", "\n");
    let controller =
        std::fs::read_to_string("src/desktop/shaders/refraction_dopri_hermite_trace.wesl")
            .unwrap()
            .replace("\r\n", "\n");
    let intervals = std::fs::read_to_string("scripts/optical_normal_interval.wgsl").unwrap();
    let clearance = std::fs::read_to_string("scripts/optical_cartesian_clearance.wgsl").unwrap();
    let common = COMMON.replace("RECORD_COUNT", &format!("{count}u"));
    let shared = format!("{base}\n{dp}\n{controller}\n{intervals}\n{clearance}\n{common}\n");
    let invoke = "let ray=trace(direction);";
    assert_eq!(entry.matches(invoke).count(), 1);
    let mut prefix_entry = entry[..entry.find(invoke).unwrap()].to_owned();
    replace(&mut prefix_entry, "fn normal_scene(", "fn cs_prefix(", 1);
    replace(
        &mut prefix_entry,
        "terminals[0]=settings.control;",
        "terminals[0]=vec4(0u);",
        1,
    );
    replace(
        &mut prefix_entry,
        "let index=1u+id.x*3u;",
        "cs_initialize_record(id.x);let index=1u+id.x*3u;",
        1,
    );
    replace(
        &mut prefix_entry,
        "terminals[detail_index+1u]=vec4(0u);return;",
        "terminals[detail_index+1u]=vec4(0u);cs_masked(id.x);return;",
        1,
    );
    prefix_entry.push_str("cs_states[6u*id.x+2u]=vec4(bitcast<vec3<u32>>(direction),3u);cs_prefix_trace(direction,id.x);\n}\n");
    let prefix_source = format!(
        "{shared}\n{}\n@compute @workgroup_size(32)\n{prefix_entry}",
        prefix_trace(&original)
    );
    let suffix_source = format!("{shared}\n{}\n{SUFFIX_ENTRY}", suffix_trace(&controller));
    let fallback_source = format!("{shared}\n{FINAL_ENTRIES}");
    let mut modules = Vec::new();
    let mut packaging = Vec::new();
    for (name, source) in [
        ("prefix", prefix_source),
        ("suffix", suffix_source),
        ("fallback", fallback_source),
    ] {
        let source = source.replace("RECORD_COUNT", &format!("{count}u"));
        let (shader, report) = reachability::prune(&source);
        packaging.push(json!({"module":name,"reachability":report}));
        modules.push(Module { name, shader });
    }
    scene.shader = modules[0].shader.clone();
    scene.cases["cartesian_suffix"] = json!({
        "scope":"Unregistered single accepted-state handoff. Original midpoint handles near-scene contacts; suffix only publishes certified scene-free atmospheric exits. Any suffix failure discards all suffix progress and reruns original trace from the original launch. No source, contact budget or default runtime registration change.",
        "source":"scripts/optical_cartesian_suffix_fixture.rs","clearance_source":"scripts/optical_cartesian_clearance.wgsl",
        "maximum_candidate_attempts":4096,"state_words_per_record":24,
        "state_layout":["handoff p.xyz/path bits","handoff u.xyz/entry bits","original launch.xyz bits/phase0complete1suffix2fallback3initial","prefix attempts/suffix attempts/fallback attempts/failure reason","handoff geometry.xyzw bits","source owner i32 bits/output record ID/route0prefix1suffix2fallback3masked/reserved0"],
        "diagnostic_layout":["prefix attempts/suffix attempts/fallback attempts/route | reason<<8","unchanged exact original GPU launch.xyz/0"],
        "failure_reasons":{"0":"none","2":"suffix domain, source, or controller rejection","3":"whole-prefix scene exclusion declined","4":"possible h0 contact","5":"candidate 4096-attempt exhaustion"},
        "progress_layout":["incomplete records","complete records","maximum prefix+suffix+fallback attempt work","invalid record count"],
        "initialization":"Every sequence overwrites all24 state words and both detail vec4s, including masked pixels; header is zero until final validated publication.",
        "packaging":packaging
    });
    let dispatch = scene.dispatch;
    SuffixFixture {
        state_words: 24,
        state_bytes: 96 * u64::from(count),
        progress_bytes: 16,
        scene,
        modules,
        steps: vec![
            Step {
                module_index: 0,
                entry: "cs_prefix",
                dispatch,
                repetitions: 1,
            },
            Step {
                module_index: 1,
                entry: "cs_suffix",
                dispatch,
                repetitions: 1,
            },
            Step {
                module_index: 2,
                entry: "cs_fallback",
                dispatch,
                repetitions: 1,
            },
            Step {
                module_index: 2,
                entry: "cs_validate",
                dispatch: [count.div_ceil(256).div_ceil(32), 1, 1],
                repetitions: 1,
            },
            Step {
                module_index: 2,
                entry: "cs_finalize",
                dispatch: [1, 1, 1],
                repetitions: 1,
            },
        ],
    }
}

const COMMON: &str = r#"
@group(0) @binding(6) var<storage,read_write> cs_states:array<vec4<u32>>;
@group(0) @binding(7) var<storage,read_write> cs_progress:array<atomic<u32>>;
var<private> cs_prefix_attempts:u32;
var<private> cs_suffix_attempts:u32;
var<private> cs_fallback_attempts:u32;
var<private> cs_failure_reason:u32;
fn cs_initialize_record(id:u32){
    for(var word=0u;word<6u;word+=1u){cs_states[6u*id+word]=vec4(0u);}
    cs_states[6u*id+2u].w=3u;
    cs_states[6u*id+5u]=vec4(bitcast<u32>(-2i),id,4u,0u);
    let detail_index=1u+3u*RECORD_COUNT+2u*id;
    terminals[detail_index]=vec4(0u);terminals[detail_index+1u]=vec4(0u);
}
fn cs_masked(id:u32){
    cs_states[6u*id+2u].w=0u;cs_states[6u*id+5u].z=3u;
    terminals[1u+3u*RECORD_COUNT+2u*id]=vec4(0u,0u,0u,3u);
}
fn cs_publish(id:u32,ray:Trace,route:u32){
    let index=1u+3u*id;let detail_index=1u+3u*RECORD_COUNT+2u*id;
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);
    let counts=cs_states[6u*id+3u];
    terminals[detail_index]=vec4(counts.xyz,route|(counts.w<<8u));
    cs_states[6u*id+2u].w=0u;cs_states[6u*id+5u].z=route;
}
fn cs_prefix_done(id:u32,ray:Trace){cs_states[6u*id+3u].x=cs_prefix_attempts;cs_publish(id,ray,0u);}
fn cs_handoff(id:u32,state:State,path:f32,entry:f32,g:vec4<f32>,owner:i32,attempt:u32){
    cs_states[6u*id]=bitcast<vec4<u32>>(vec4(state.p,path));
    cs_states[6u*id+1u]=bitcast<vec4<u32>>(vec4(state.u,entry));
    cs_states[6u*id+2u].w=1u;cs_states[6u*id+3u]=vec4(attempt,0u,0u,0u);
    cs_states[6u*id+4u]=bitcast<vec4<u32>>(g);
    cs_states[6u*id+5u].x=bitcast<u32>(owner);cs_states[6u*id+5u].z=4u;
}
"#;
const SUFFIX_ENTRY: &str = r#"
@compute @workgroup_size(32)
fn cs_suffix(@builtin(global_invocation_id) invocation:vec3<u32>){
    let id=invocation.x;if id>=RECORD_COUNT{return;}
    if cs_states[6u*id+2u].w!=1u{return;}
    let ray=cs_suffix_trace(id);cs_states[6u*id+3u].y=cs_suffix_attempts;
    if ray.status==0u{cs_states[6u*id+3u].w=0u;cs_publish(id,ray,1u);return;}
    cs_states[6u*id+3u].w=cs_failure_reason;cs_states[6u*id+2u].w=2u;
}
"#;
const FINAL_ENTRIES: &str = r#"
@compute @workgroup_size(32)
fn cs_fallback(@builtin(global_invocation_id) invocation:vec3<u32>){
    let id=invocation.x;if id>=RECORD_COUNT{return;}
    if cs_states[6u*id+2u].w!=2u{return;}
    cs_fallback_attempts=0u;
    let direction=bitcast<vec3<f32>>(cs_states[6u*id+2u].xyz);
    let ray=trace(direction);cs_states[6u*id+3u].z=cs_fallback_attempts;
    cs_publish(id,ray,2u);
}
@compute @workgroup_size(32)
fn cs_validate(@builtin(global_invocation_id) invocation:vec3<u32>){
    let first=invocation.x*256u;if first>=RECORD_COUNT{return;}
    let end=min(first+256u,RECORD_COUNT);var pending=0u;var complete=0u;var bad=0u;var work=0u;
    for(var id=first;id<end;id+=1u){
        let phase=cs_states[6u*id+2u].w;let counts=cs_states[6u*id+3u];let metadata=cs_states[6u*id+5u];
        if phase==0u{complete+=1u;}else{pending+=1u;}
        if phase>3u || metadata.y!=id || metadata.z>3u || counts.x>4096u || counts.y>4096u || counts.z>4096u || counts.x+counts.y>4096u || counts.w>5u{bad+=1u;}
        let status=terminals[2u+3u*id].w;
        if (metadata.z==0u && (counts.y!=0u || counts.z!=0u || counts.w!=0u)) || (metadata.z==1u && (status!=0u || counts.y==0u || counts.z!=0u || counts.w!=0u)) || (metadata.z==2u && (counts.y==0u || counts.w==0u)) || (metadata.z==3u && (status!=3u || any(counts!=vec4(0u)))){bad+=1u;}
        work=max(work,counts.x+counts.y+counts.z);
    }
    atomicAdd(&cs_progress[0],pending);atomicAdd(&cs_progress[1],complete);
    atomicMax(&cs_progress[2],work);atomicAdd(&cs_progress[3],bad);
}
@compute @workgroup_size(1)
fn cs_finalize(){
    if atomicLoad(&cs_progress[0])==0u && atomicLoad(&cs_progress[1])==RECORD_COUNT && atomicLoad(&cs_progress[3])==0u{terminals[0]=settings.control;}
}
"#;
