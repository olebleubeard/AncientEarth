//! Original MIT. Unregistered fixed-index outer-attempt scheduling diagnostic.
//! Every field/contact operation inside an original midpoint attempt remains
//! in its original textual order. Suspension is only between whole attempts.
//! State is bitcast to storage; no normalization, geometry reuse or new solve.
use super::fixture;
use serde_json::json;

pub struct WaveStep {
    pub entry: &'static str,
    pub dispatch: [u32; 3],
    pub repetitions: u32,
}
pub struct WaveFixture {
    pub scene: fixture::Fixture,
    pub state_bytes: u64,
    pub progress_bytes: u64,
    pub steps: Vec<WaveStep>,
}
fn span(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let close = closing(source, open);
    start..close + 1
}
fn closing(source: &str, open: usize) -> usize {
    let mut depth = 0u32;
    for (index, b) in source.as_bytes()[open..].iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        }
        if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                return open + index;
            }
        }
    }
    panic!("Unterminated source block")
}
fn replace(source: &mut String, old: &str, new: &str, count: usize) {
    assert_eq!(
        source.matches(old).count(),
        count,
        "Wavefront source drift: {old}"
    );
    *source = source.replace(old, new);
}
fn returns(source: &str, entry: &str, attempt: &str) -> (String, usize) {
    let mut out = String::new();
    let mut cursor = 0;
    let mut count = 0;
    while let Some(relative) = source[cursor..].find("return ") {
        let start = cursor + relative;
        let end = start
            + source[start..]
                .find(';')
                .expect("Return must be a single expression");
        let expression = &source[start + 7..end];
        assert!(
            expression.starts_with("trace_result(") || expression.starts_with("Trace("),
            "Unexpected trace return expression: {expression}"
        );
        out.push_str(&source[cursor..start]);
        out.push_str(&format!(
            "wave_complete(id,{expression},state,path,proposed,{entry},{attempt});return;"
        ));
        cursor = end + 1;
        count += 1;
    }
    out.push_str(&source[cursor..]);
    (out, count)
}

pub fn build_counted_reference(selected: bool) -> fixture::Fixture {
    let mut scene = fixture::build_reference(selected);
    let range = span(&scene.shader, "trace");
    let mut body = scene.shader[range.clone()].to_owned();
    replace(
        &mut body,
        "fn trace(direction:vec3<f32>)->Trace{",
        "fn trace(direction:vec3<f32>)->Trace{wave_reference_attempts=0u;",
        1,
    );
    replace(
        &mut body,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=0;i<4096;i+=1){wave_reference_attempts+=1u;",
        1,
    );
    scene.shader.replace_range(range, &body);
    let range = span(&scene.shader, "normal_scene");
    let mut body = scene.shader[range.clone()].to_owned();
    replace(
        &mut body,
        "terminals[detail_index]=vec4(0u);",
        "terminals[detail_index]=vec4(wave_reference_attempts,0u,0u,0u);",
        2,
    );
    // A masked pixel returns before trace(), hence explicitly reset its private
    // counter at this entry rather than depending on initialization semantics.
    replace(
        &mut body,
        "fn normal_scene(@builtin(global_invocation_id) id:vec3<u32>){",
        "fn normal_scene(@builtin(global_invocation_id) id:vec3<u32>){wave_reference_attempts=0u;",
        1,
    );
    scene.shader.replace_range(range, &body);
    scene
        .shader
        .push_str("\nvar<private> wave_reference_attempts:u32;\n");
    scene.cases["wavefront_counted_reference"] = json!({"scope":"Original full trace with an unsigned counter incremented once at each outer-attempt entry. Initialization and masked terminals count0; both retry paths count. No numerical expression changed; compiler-context terminal parity remains required.","source":"scripts/optical_midpoint_wavefront_fixture.rs","diagnostic_word0":"completed/terminal outer-attempt count"});
    scene
}

pub fn build(selected: bool, slice_attempts: u32) -> WaveFixture {
    assert!([1, 8, 32, 128].contains(&slice_attempts));
    let mut scene = fixture::build_reference(selected);
    let count = scene.record_count;
    let trace_range = span(&scene.shader, "trace");
    let trace = scene.shader[trace_range.clone()].to_owned();
    let open = trace.find('{').unwrap();
    let loop_signature = "for(var i=0;i<4096;i+=1)";
    assert_eq!(trace.matches(loop_signature).count(), 1);
    let loop_start = trace.find(loop_signature).unwrap();
    let loop_open = loop_start + trace[loop_start..].find('{').unwrap();
    let loop_close = closing(&trace, loop_open);
    let prefix = &trace[open + 1..loop_start];
    let attempt_body = &trace[loop_open + 1..loop_close];
    assert_eq!(attempt_body.matches("continue;").count(), 2);
    assert_eq!(
        trace[loop_close + 1..].replace("\r\n", "\n").trim(),
        "return trace_result(state.p,state.u,path,2u);\n}"
    );
    let (init_body, init_returns) = returns(prefix, "path", "0u");
    assert_eq!(init_returns, 3);
    let (slice_body, slice_returns) = returns(attempt_body, "entry", "attempt");
    assert_eq!(slice_returns, 11);
    // There is no per-ray mutable global in the original trace graph. The
    // entire cross-attempt state is the following twelve exact words.
    let functions = format!(
        r#"
fn wave_initialize_trace(direction:vec3<f32>,id:u32){{
{init_body}
    wave_store(id,state,path,proposed,entry,0u,0u);
}}
@compute @workgroup_size(32)
fn wave_slice(@builtin(global_invocation_id) invocation:vec3<u32>){{
    let id=invocation.x;if id>={count}u{{return;}}
    let metadata=wave_states[3u*id+2u];if metadata.w!=0u{{return;}}
    let position=bitcast<vec4<f32>>(wave_states[3u*id]);let direction=bitcast<vec4<f32>>(wave_states[3u*id+1u]);
    var state=State(position.xyz,direction.xyz);var path=position.w;var proposed=direction.w;
    let entry=bitcast<f32>(metadata.x);var attempt=metadata.y;let curvature=settings.budget.z;
    for(var local=0u;local<{slice_attempts}u && attempt<4096u;local+=1u){{
        attempt+=1u;
{slice_body}
    }}
    if attempt==4096u{{wave_complete(id,trace_result(state.p,state.u,path,2u),state,path,proposed,entry,attempt);return;}}
    wave_store(id,state,path,proposed,entry,attempt,0u);
}}
"#
    );
    scene.shader.replace_range(trace_range, &functions);
    let entry_range = span(&scene.shader, "normal_scene");
    let entry = scene.shader[entry_range.clone()].to_owned();
    let invoke = "let ray=trace(direction);";
    assert_eq!(entry.matches(invoke).count(), 1);
    let mut initialize = entry[..entry.find(invoke).unwrap()].to_owned();
    replace(
        &mut initialize,
        "fn normal_scene(",
        "fn wave_initialize(",
        1,
    );
    replace(
        &mut initialize,
        "terminals[0]=settings.control;",
        "terminals[0]=vec4(0u);",
        1,
    );
    replace(
        &mut initialize,
        "terminals[detail_index+1u]=vec4(0u);return;",
        "terminals[detail_index+1u]=vec4(0u);wave_masked(id.x);return;",
        1,
    );
    initialize
        .push_str("atomicAdd(&wave_progress[0],1u);wave_initialize_trace(direction,id.x);\n}\n");
    scene.shader.replace_range(entry_range, &initialize);
    scene
        .shader
        .push_str(&COMMON.replace("RECORD_COUNT", &format!("{count}u")));
    scene.cases["wavefront"] = json!({
        "scope":"Unregistered exact-state outer-attempt scheduling diagnostic of the original midpoint trace. Initial vacuum/readiness arithmetic and every complete attempt retain source operation order. Bitcast persistence only; compiler-context parity remains mandatory. Fixed-index slices include idle lanes and empty rounds. No compaction or production speed claim.",
        "source":"scripts/optical_midpoint_wavefront_fixture.rs","slice_attempts":slice_attempts,"maximum_attempts":4096,"slice_repetitions":4096/slice_attempts,
        "state_record_words":12,"state_layout":["p.xyz/path f32 bits","u.xyz/proposed f32 bits","entry f32 bits/completedattempts/outputrecordID/phase0live1terminal"],
        "progress_layout":["live atomic count","terminal count including masked pixels","maximum recorded attempt count","reserved0"],
        "terminal_header":"Initialization writes zero; finalizer publishes exact current token only if live0 and completed==record_count. Host clears progress before init and holds all input buffers immutable through finalize.",
        "diagnostic_word0":"completed/terminal outer-attempt count", "original_initializer_return_sites":init_returns,"original_attempt_return_sites":slice_returns
    });
    let dispatch = scene.dispatch;
    WaveFixture {
        state_bytes: 48 * u64::from(count),
        progress_bytes: 16,
        scene,
        steps: vec![
            WaveStep {
                entry: "wave_initialize",
                dispatch,
                repetitions: 1,
            },
            WaveStep {
                entry: "wave_slice",
                dispatch,
                repetitions: 4096 / slice_attempts,
            },
            WaveStep {
                entry: "wave_finalize",
                dispatch: [1, 1, 1],
                repetitions: 1,
            },
        ],
    }
}
const COMMON: &str = r#"
@group(0) @binding(6) var<storage,read_write> wave_states:array<vec4<u32>>;
@group(0) @binding(7) var<storage,read_write> wave_progress:array<atomic<u32>>;
fn wave_store(id:u32,state:State,path:f32,proposed:f32,entry:f32,attempt:u32,phase:u32){
    wave_states[3u*id]=bitcast<vec4<u32>>(vec4(state.p,path));
    wave_states[3u*id+1u]=bitcast<vec4<u32>>(vec4(state.u,proposed));
    wave_states[3u*id+2u]=vec4(bitcast<u32>(entry),attempt,id,phase);
}
fn wave_masked(id:u32){
    wave_store(id,State(vec3(0.0),vec3(0.0)),0.0,0.0,0.0,0u,1u);
    atomicAdd(&wave_progress[1],1u);
}
fn wave_complete(id:u32,ray:Trace,state:State,path:f32,proposed:f32,entry:f32,attempt:u32){
    let index=1u+3u*id;let detail_index=1u+3u*RECORD_COUNT+2u*id;
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);
    terminals[detail_index]=vec4(attempt,0u,0u,0u);
    wave_store(id,state,path,proposed,entry,attempt,1u);
    atomicSub(&wave_progress[0],1u);atomicAdd(&wave_progress[1],1u);atomicMax(&wave_progress[2],attempt);
}
@compute @workgroup_size(1)
fn wave_finalize(){
    if atomicLoad(&wave_progress[0])==0u&&atomicLoad(&wave_progress[1])==RECORD_COUNT{terminals[0]=settings.control;}
}
"#;
