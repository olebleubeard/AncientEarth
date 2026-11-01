//! Original MIT. One selected, prospectively fixed tighter-control diagnostic.
//! No first-candidate or telemetry source is modified by this wrapper.
use super::suffix_trace::{self, TraceFixture};
use serde_json::json;

pub fn build() -> TraceFixture {
    let mut fixture = suffix_trace::build();
    assert_eq!(fixture.suffix.scene.record_count, 23);
    assert_eq!(fixture.suffix.state_words, 24);
    assert_eq!(fixture.max_steps, 4096);
    assert_eq!(
        fixture
            .suffix
            .modules
            .iter()
            .map(|m| m.name)
            .collect::<Vec<_>>(),
        ["prefix", "suffix", "fallback"]
    );
    let shader = &mut fixture.suffix.modules[1].shader;
    assert_eq!(
        shader.matches("fn cs_suffix_trace(id:u32)->Trace{").count(),
        1
    );
    let original = "let tolerance=max(0.002,4e-7*length(state.p));";
    let tightened = "let tolerance=0.25*max(0.002,4e-7*length(state.p));";
    assert_eq!(
        shader.matches(original).count(),
        1,
        "Tighter-control source drift"
    );
    assert!(!shader.contains(tightened));
    *shader = shader.replacen(original, tightened, 1);
    // Prefix/fallback sources, uploads, dispatches, state and log layouts are
    // untouched. Only the suffix's existing local tolerance is contracted.
    fixture.suffix.scene.cases["cartesian_suffix"]["scope"] = json!(
        "Unregistered selected-only tighter-control experiment. Same physical field, exact original prefix/handoff/fallback, and unchanged source/scene guards. The suffix local tolerance alone is multiplied by0.25. All original per-ray accuracy gates remain fixed; no timing or production claim."
    );
    fixture.suffix.scene.cases["cartesian_suffix_path_telemetry"]["scope"] = json!(
        "Accepted-step logging is unchanged from the bit-identical telemetry control, but this variant deliberately tightens the suffix local tolerance. Terminal parity is required for original routes; suffix outputs are judged against the frozen per-ray oracle gates, with exact original handoff identity required before interpretation."
    );
    fixture.suffix.scene.cases["cartesian_suffix_tight_control"] = json!({
        "source":"scripts/optical_cartesian_suffix_tight_fixture.rs",
        "contract":"docs/science/cartesian-suffix-tight-control.md",
        "factor":0.25,
        "original_declaration":original,"candidate_declaration":tightened,
        "basis":"artifacts/optical-cartesian-suffix-trace-selected-first",
        "same_suffix_record_indices":[3,9,10,11,12,14,15,17,18,20],
        "required_identity":"Five uploaded buffers, prefix/fallback module bytes, launch and exact handoff p/u/path/entry/g/owner and prefix attempt count, dispatch order, all state/log layouts. All13 original-route terminal records remain exact.",
        "acceptance":"Same10 suffix completions, all30 frozen same-ray endpoint/path/direction gates, unchanged finite/error/source/scene checks and4096 attempt cap; valid accepted-step log. No new fallback may conceal a failed suffix.",
        "stop":"One fixed factor only. Any lost suffix route, identity failure, invalid log or metric failure rejects this refinement branch before full-frame timing. No factor sweep or gate adjustment.",
        "interpretation":"Finite sensitivity to stricter local integration and source-contact controls. Changed f32 schedules and roundoff remain included; this does not isolate truncation order or prove global physical accuracy."
    });
    fixture
}
