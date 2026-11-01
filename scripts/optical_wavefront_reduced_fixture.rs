//! Original MIT. Separate completion-accounting experiment. The original
//! wavefront producer remains reproducible; only global atomic bookkeeping is
//! replaced by a bounded read-only reduction of completed persistent records.
use super::wavefront::{self, WaveFixture, WaveStep};
use serde_json::json;

pub struct ReducedFixture {
    pub wave: WaveFixture,
    pub partial_bytes: u64,
    pub block_count: u32,
}
fn replace(source: &mut String, old: &str, new: &str, expected: usize) {
    assert_eq!(
        source.matches(old).count(),
        expected,
        "Reduction source drift: {old}"
    );
    *source = source.replace(old, new);
}
pub fn build(selected: bool, slice_attempts: u32) -> ReducedFixture {
    apply(wavefront::build(selected, slice_attempts))
}
pub fn apply(mut wave: WaveFixture) -> ReducedFixture {
    let count = wave.scene.record_count;
    assert!((1..=2_073_600).contains(&count));
    let blocks = count.div_ceil(256);
    let source = &mut wave.scene.shader;
    // These are the complete counter-only expressions. No numerical store or
    // initialization/attempt control-flow statement is included in a match.
    replace(source, "atomicAdd(&wave_progress[0],1u);", "", 1);
    replace(source, "atomicAdd(&wave_progress[1],1u);", "", 2);
    replace(source, "atomicSub(&wave_progress[0],1u);", "", 1);
    replace(source, "atomicMax(&wave_progress[2],attempt);", "", 1);
    let old_finalizer = format!(
        "@compute @workgroup_size(1)\nfn wave_finalize(){{\n    if atomicLoad(&wave_progress[0])==0u&&atomicLoad(&wave_progress[1])=={count}u{{terminals[0]=settings.control;}}\n}}"
    );
    replace(source, &old_finalizer, "", 1);
    assert!(!source.contains("atomicAdd("));
    assert!(!source.contains("atomicSub("));
    assert!(!source.contains("atomicMax("));
    source.push_str(
        &REDUCTION
            .replace("RECORD_COUNT", &format!("{count}u"))
            .replace("BLOCK_COUNT", &format!("{blocks}u")),
    );
    assert_eq!(wave.steps.len(), 3);
    assert_eq!(wave.steps[0].entry, "wave_initialize");
    assert_eq!(wave.steps[1].entry, "wave_slice");
    assert_eq!(wave.steps[2].entry, "wave_finalize");
    wave.steps.pop();
    wave.steps.push(WaveStep {
        entry: "wave_validate",
        dispatch: [blocks.div_ceil(32), 1, 1],
        repetitions: 1,
    });
    wave.steps.push(WaveStep {
        entry: "wave_finalize_reduced",
        dispatch: [1, 1, 1],
        repetitions: 1,
    });
    wave.scene.cases["wavefront_reduction"] = json!({
        "scope":"Completion-accounting-only variant. Per-ray state/terminal stores and every numerical attempt are unchanged. No shared counter atomics in init/slices. A separate metadata validation pass and final reduction authorize the current header only after all output records are valid and terminal. Raw parity is still required across compiler contexts.",
        "source":"scripts/optical_wavefront_reduced_fixture.rs",
        "binding":8,"block_records":256,"block_count":blocks,
        "partial_words":["live count","terminal count","maximum attempts","invalid metadata count"],
        "validation":"metadata.z==stable output-record ID, attempts<=4096, phase exactly0 or1; no alteration of state, attempt count, or terminal records.",
        "maximum_record_count":2_073_600,
        "header_gate":"live==0 && terminal==record_count && bad==0",
        "memory_scope":"All state records are read after the final slice; each partial block is overwritten before the finalizer reads it. No intermediate live count or compaction is used."
    });
    wave.scene.cases["wavefront"]["progress_layout"] = json!([
        "live records",
        "terminal records including masked pixels",
        "maximum recorded attempts",
        "invalid metadata count"
    ]);
    wave.scene.cases["wavefront"]["terminal_header"] = json!(
        "Initialization writes zero. Final reduction publishes exact current token only if every record has matching output ID, attempts<=4096 and terminal phase1. Inputs remain immutable through finalize."
    );
    ReducedFixture {
        wave,
        partial_bytes: u64::from(blocks) * 16,
        block_count: blocks,
    }
}

const REDUCTION: &str = r#"
@group(0) @binding(8) var<storage,read_write> wave_partials:array<vec4<u32>>;
@compute @workgroup_size(32)
fn wave_validate(@builtin(global_invocation_id) invocation:vec3<u32>){
    let block=invocation.x;if block>=BLOCK_COUNT{return;}
    let first=block*256u;let stop=min(first+256u,RECORD_COUNT);
    var live=0u;var completed=0u;var attempts=0u;var bad=0u;
    for(var id=first;id<stop;id+=1u){
        let metadata=wave_states[3u*id+2u];
        attempts=max(attempts,metadata.y);
        if metadata.w==0u{live+=1u;}else if metadata.w==1u{completed+=1u;}
        if metadata.z!=id||metadata.y>4096u||metadata.w>1u{bad+=1u;}
    }
    wave_partials[block]=vec4(live,completed,attempts,bad);
}
@compute @workgroup_size(1)
fn wave_finalize_reduced(){
    var live=0u;var completed=0u;var attempts=0u;var bad=0u;
    for(var block=0u;block<BLOCK_COUNT;block+=1u){
        let part=wave_partials[block];
        live+=part.x;completed+=part.y;attempts=max(attempts,part.z);bad+=part.w;
    }
    atomicStore(&wave_progress[0],live);atomicStore(&wave_progress[1],completed);
    atomicStore(&wave_progress[2],attempts);atomicStore(&wave_progress[3],bad);
    if live==0u&&completed==RECORD_COUNT&&bad==0u{terminals[0]=settings.control;}
}
"#;
