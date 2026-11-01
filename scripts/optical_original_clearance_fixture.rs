//! Original MIT. Unregistered original-arithmetic clearance/fallback control.
//! Only original empty face events may complete the fast trace. Every other
//! face requests a fresh original trace from the exact saved launch bits.
use super::{
    fixture,
    suffix::{Module, Step, SuffixFixture},
};
#[path = "optical_shader_reachability.rs"]
mod reachability;
use serde_json::json;

fn span(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(
        source.matches(&signature).count(),
        1,
        "Function drift: {name}"
    );
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0_u32;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        if *byte == b'{' {
            depth += 1;
        }
        if *byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return start..open + offset + 1;
            }
        }
    }
    panic!("Unclosed function: {name}")
}
fn replace(source: &mut String, old: &str, new: &str, count: usize) {
    assert_eq!(
        source.matches(old).count(),
        count,
        "Clearance source drift: {old}"
    );
    *source = source.replace(old, new);
}
fn fast_source(base: &str) -> String {
    let mut source = base.to_owned();
    let face_span = span(&source, "face_event");
    let original_face = &source[face_span.clone()];
    let rejection = "if !triangle_candidate(p,q,a,b,c,n,tube+band){return result;}";
    assert_eq!(original_face.matches(rejection).count(), 1);
    let end = original_face.find(rejection).unwrap() + rejection.len();
    // Retain every original expression through both sufficient empty returns.
    // The placeholder below is never interpreted by trace when its flag is set.
    let face = format!(
        "{}\n    oc_pending=1u;return result;\n}}",
        &original_face[..end]
    );
    assert!(!face.contains("advance_delta("));
    source.replace_range(face_span, &face);
    for name in ["terrain_events", "marker_events"] {
        let range = span(&source, name);
        let mut traversal = source[range.clone()].to_owned();
        replace(
            &mut traversal,
            "var result=empty_event();",
            "var result=empty_event();if oc_pending!=0u{return result;}",
            1,
        );
        let needle = "result=combine_event(result,face_event(";
        assert_eq!(traversal.matches(needle).count(), 1);
        let begin = traversal.find(needle).unwrap();
        let end = begin + traversal[begin..].find(';').unwrap() + 1;
        traversal.insert_str(end, "if oc_pending!=0u{return result;}");
        source.replace_range(range, &traversal);
    }
    let call = "let event=scene_event(state,event_end,event_delta,event_step,event_error);";
    replace(
        &mut source,
        call,
        &format!(
            "{call}\n        if oc_pending!=0u{{return trace_result(state.p,state.u,path,2u);}}"
        ),
        1,
    );
    source
}

pub fn build(selected: bool) -> SuffixFixture {
    let mut scene = fixture::build_reference(selected);
    scene.shader = scene.shader.replace("\r\n", "\n");
    let count = scene.record_count;
    let entry_span = span(&scene.shader, "normal_scene");
    let entry = scene.shader[entry_span.clone()].to_owned();
    let attribute = "@compute @workgroup_size(32)\n";
    assert!(scene.shader[..entry_span.start].ends_with(attribute));
    let mut base = scene.shader.clone();
    base.replace_range(entry_span.start - attribute.len()..entry_span.end, "");
    let trace_span = span(&base, "trace");
    let mut counted = base[trace_span.clone()].to_owned();
    replace(
        &mut counted,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=0;i<4096;i+=1){oc_attempts+=1u;",
        1,
    );
    base.replace_range(trace_span, &counted);

    let invoke = "let ray=trace(direction);";
    assert_eq!(entry.matches(invoke).count(), 1);
    let mut fast_entry = entry[..entry.find(invoke).unwrap()].to_owned();
    replace(&mut fast_entry, "fn normal_scene(", "fn oc_fast(", 1);
    replace(
        &mut fast_entry,
        "terminals[0]=settings.control;",
        "terminals[0]=vec4(0u);for(var word=0u;word<4u;word+=1u){atomicStore(&oc_progress[word],0u);}",
        1,
    );
    replace(
        &mut fast_entry,
        "let index=1u+id.x*3u;",
        "oc_initialize(id.x);let index=1u+id.x*3u;",
        1,
    );
    replace(
        &mut fast_entry,
        "terminals[detail_index+1u]=vec4(0u);return;",
        "terminals[detail_index+1u]=vec4(0u);oc_masked(id.x);return;",
        1,
    );
    fast_entry.push_str(
        r#"
    oc_pending=0u;oc_attempts=0u;
    let ray=trace(direction);oc_states[id.x].y=oc_attempts;
    if oc_pending!=0u{
        oc_states[id.x].x=1u;oc_states[id.x].w=1u;
        terminals[detail_index]=vec4(oc_attempts,0u,1u,1u);return;
    }
    oc_publish(id.x,ray,0u);
}
"#,
    );
    let common = COMMON.replace("RECORD_COUNT", &format!("{count}u"));
    let sources = [
        (
            "fast",
            format!("{}\n{common}\n{attribute}{fast_entry}", fast_source(&base)),
        ),
        ("fallback", format!("{base}\n{common}\n{FINAL_ENTRIES}")),
    ];
    let mut modules = Vec::new();
    let mut packaging = Vec::new();
    for (name, source) in sources {
        let source = source.replace("RECORD_COUNT", &format!("{count}u"));
        let (shader, report) = reachability::prune(&source);
        packaging.push(json!({"module":name,"reachability":report}));
        modules.push(Module { name, shader });
    }
    scene.shader = modules[0].shader.clone();
    scene.cases["original_clearance"] = json!({
        "scope":"Original midpoint field, controller, h0 coverage, scene rejection and terminal expressions. Fast trace keeps original face expressions through degenerate/candidate rejection. Any unrejected face discards the fast prefix and invokes the complete original trace from the exact saved launch. No numerical solver, contact, tolerance or runtime registration change.",
        "source":"scripts/optical_original_clearance_fixture.rs",
        "proof":"docs/science/original-clearance-control.md",
        "state_words_per_record":4,
        "state_layout":["phase0complete1fallback2initial","fast attempt count","original fallback attempt count","route0fast1fallback2masked3initial"],
        "diagnostic_layout":["fast attempts/fallback attempts/route/pending flag (zero on final publication)","exact original GPU launch.xyz bits/0"],
        "progress_layout":["incomplete records","complete records","maximum fast+fallback work","invalid records"],
        "attempt_limits":{"fast":4096,"fallback":4096,"maximum_work":8192},
        "initialization":"Timed fast dispatch id0 clears header and all4 progress words. Every row resets all4 state,12 terminal and8 diagnostic words, including masked rows. No global progress updates occur until validation; final header requires every row complete and valid.",
        "acceptance":"Selected then full original terminal/launch bit parity; positive fast and fallback populations required. Complete-chain cost includes discarded prefixes, routing, validation and initialization. No speed claim before measurement.",
        "packaging":packaging
    });
    let dispatch = scene.dispatch;
    SuffixFixture {
        scene,
        modules,
        state_words: 4,
        state_bytes: 16 * u64::from(count),
        progress_bytes: 16,
        steps: vec![
            Step {
                module_index: 0,
                entry: "oc_fast",
                dispatch,
                repetitions: 1,
            },
            Step {
                module_index: 1,
                entry: "oc_fallback",
                dispatch,
                repetitions: 1,
            },
            Step {
                module_index: 1,
                entry: "oc_validate",
                dispatch: [count.div_ceil(256).div_ceil(32), 1, 1],
                repetitions: 1,
            },
            Step {
                module_index: 1,
                entry: "oc_finalize",
                dispatch: [1, 1, 1],
                repetitions: 1,
            },
        ],
    }
}

const COMMON: &str = r#"
@group(0) @binding(6) var<storage,read_write> oc_states:array<vec4<u32>>;
@group(0) @binding(7) var<storage,read_write> oc_progress:array<atomic<u32>>;
var<private> oc_attempts:u32;
var<private> oc_pending:u32;
fn oc_initialize(id:u32){
    oc_states[id]=vec4(2u,0u,0u,3u);
    let index=1u+3u*id;let detail_index=1u+3u*RECORD_COUNT+2u*id;
    terminals[index]=vec4(0u);terminals[index+1u]=vec4(0u);terminals[index+2u]=vec4(0u);
    terminals[detail_index]=vec4(0u);terminals[detail_index+1u]=vec4(0u);
}
fn oc_masked(id:u32){
    oc_states[id]=vec4(0u,0u,0u,2u);
    terminals[1u+3u*RECORD_COUNT+2u*id]=vec4(0u,0u,2u,0u);
}
fn oc_publish(id:u32,ray:Trace,route:u32){
    let index=1u+3u*id;let detail_index=1u+3u*RECORD_COUNT+2u*id;
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);
    oc_states[id].x=0u;oc_states[id].w=route;
    terminals[detail_index]=vec4(oc_states[id].yz,route,0u);
}
"#;
const FINAL_ENTRIES: &str = r#"
@compute @workgroup_size(32)
fn oc_fallback(@builtin(global_invocation_id) invocation:vec3<u32>){
    let id=invocation.x;if id>=RECORD_COUNT{return;}
    if oc_states[id].x!=1u{return;}
    let detail_index=1u+3u*RECORD_COUNT+2u*id;
    let direction=bitcast<vec3<f32>>(terminals[detail_index+1u].xyz);
    oc_attempts=0u;let ray=trace(direction);oc_states[id].z=oc_attempts;
    oc_publish(id,ray,1u);
}
@compute @workgroup_size(32)
fn oc_validate(@builtin(global_invocation_id) invocation:vec3<u32>){
    let first=invocation.x*256u;if first>=RECORD_COUNT{return;}
    let end=min(first+256u,RECORD_COUNT);var pending=0u;var complete=0u;var bad=0u;var work=0u;
    for(var id=first;id<end;id+=1u){
        let state=oc_states[id];let detail_index=1u+3u*RECORD_COUNT+2u*id;
        let detail=terminals[detail_index];let status=terminals[2u+3u*id].w;
        if state.x==0u{complete+=1u;}else{pending+=1u;}
        if state.x>2u || state.w>2u || state.y>4096u || state.z>4096u || status>3u{bad+=1u;}
        if any(detail!=vec4(state.yz,state.w,0u)){bad+=1u;}
        if (state.w==0u && (state.z!=0u || status==3u)) || (state.w==1u && (state.y==0u || state.z==0u || status==3u)) || (state.w==2u && (state.y!=0u || state.z!=0u || status!=3u || any(terminals[detail_index+1u]!=vec4(0u)))){bad+=1u;}
        work=max(work,state.y+state.z);
    }
    atomicAdd(&oc_progress[0],pending);atomicAdd(&oc_progress[1],complete);
    atomicMax(&oc_progress[2],work);atomicAdd(&oc_progress[3],bad);
}
@compute @workgroup_size(1)
fn oc_finalize(){
    if atomicLoad(&oc_progress[0])==0u && atomicLoad(&oc_progress[1])==RECORD_COUNT && atomicLoad(&oc_progress[3])==0u{terminals[0]=settings.control;}
}
"#;
