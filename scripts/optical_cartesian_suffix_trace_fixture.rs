//! Original MIT. Accepted-step telemetry only, separate from the frozen solver.
//! No arithmetic change or numerical acceptance follows from log completion.
use super::suffix::{self, SuffixFixture};
use serde_json::json;

pub struct TraceFixture {
    pub suffix: SuffixFixture,
    pub auxiliary_bytes: u64,
    pub log_words_per_record: u32,
    pub max_steps: u32,
}

fn once(source: &mut String, old: &str, new: &str) {
    assert_eq!(source.matches(old).count(), 1, "Path telemetry source drift: {old}");
    *source = source.replacen(old, new, 1);
}

pub fn build() -> TraceFixture {
    let mut suffix = suffix::build(true);
    assert_eq!(suffix.scene.record_count, 23);
    assert_eq!(suffix.state_words, 24);
    assert_eq!(suffix.modules.iter().map(|m| m.name).collect::<Vec<_>>(), ["prefix", "suffix", "fallback"]);
    let max_steps = 4096_u32;
    let log_words_per_record = 4 * (max_steps + 1);
    let auxiliary_bytes = u64::from(suffix.scene.record_count) * u64::from(log_words_per_record) * 4;
    let prefix = &mut suffix.modules[0].shader;
    prefix.push_str(DECLARATION);
    once(prefix, "fn cs_initialize_record(id:u32){",
        "fn cs_initialize_record(id:u32){cst_log[id*4097u]=vec4(0u,0u,id,settings.control.z);");

    let shader = &mut suffix.modules[1].shader;
    shader.push_str(DECLARATION);
    shader.push_str(LOGGER);
    // Record only the already accepted scene-clear prefix. The original
    // arithmetic expression, path variable, branch and source commit remain
    // byte-identical following the inserted integer/bitcast storage call.
    once(shader,
        "if contact>0 && contact_height==settings.budget.y{return trace_result(state.p+accepted.delta.p,accepted.delta.u,path+step,0u);}",
        "if contact>0 && contact_height==settings.budget.y{cst_store(id,step,path,1u);return trace_result(state.p+accepted.delta.p,accepted.delta.u,path+step,0u);}");
    once(shader,
        "state=State(state.p+accepted.delta.p,accepted.delta.u);g=accepted.geometry;path+=step;",
        "cst_store(id,step,path,0u);state=State(state.p+accepted.delta.p,accepted.delta.u);g=accepted.geometry;path+=step;");
    suffix.scene.shader = suffix.modules[0].shader.clone();
    suffix.scene.cases["cartesian_suffix_path_telemetry"] = json!({
        "source":"scripts/optical_cartesian_suffix_trace_fixture.rs",
        "scope":"Selected-only accepted suffix step-word logging. All original expressions, numerical branches and terminal/state records are retained. Require every old raw terminal/detail/state/progress word to match the frozen first suffix run before interpreting the logs. Not a corrected solver, accuracy acceptance or timing measurement.",
        "original_candidate":"artifacts/optical-cartesian-suffix-selected-first",
        "binding":8,"record_count":23,"max_steps":max_steps,
        "log_words_per_record":log_words_per_record,"auxiliary_bytes":auxiliary_bytes,
        "layout":["one vec4u header: committed log count/overflow/output-record-ID/current control.z",
            "up to4096 vec4u: accepted step f32 bits/path before addition f32 bits/suffix outer attempt ordinal/kind0ordinary1top-exit"],
        "initialization":"Prefix overwrites each log header, including masked/unhanded records. Only count records are interpreted; stale unused tail words have no authority.",
        "interpretation":"Exact rational handoff-path plus every recorded accepted step isolates accumulation from accepted-step/model error. Compare rounded path-before values with the same exact cumulative sums. Include the final source-exit step; rejected trials and failed scene-clearance steps are excluded.",
        "overflow_policy":"A count at4096 sets overflow and suppresses further log writes without altering solver decisions. Overflow invalidates telemetry interpretation; it does not change a terminal.",
        "first_candidate_preserved":true
    });
    TraceFixture { suffix, auxiliary_bytes, log_words_per_record, max_steps }
}

const DECLARATION: &str = r#"
@group(0) @binding(8) var<storage,read_write> cst_log:array<vec4<u32>>;
"#;
const LOGGER: &str = r#"
fn cst_store(id:u32,step:f32,path:f32,kind:u32){
    let base=id*4097u;let count=cst_log[base].x;
    if count>=4096u{cst_log[base].y=1u;return;}
    cst_log[base+1u+count]=vec4(bitcast<u32>(step),bitcast<u32>(path),cs_suffix_attempts,kind);
    cst_log[base].x=count+1u;
}
"#;
