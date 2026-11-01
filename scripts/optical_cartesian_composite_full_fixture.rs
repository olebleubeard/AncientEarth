//! Original MIT. One full-view decision using the unchanged composite solver.
use super::{suffix, suffix::SuffixFixture, suffix_f64};
use serde_json::json;

fn once(source: &mut String, old: &str, new: &str) {
    assert_eq!(source.matches(old).count(), 1, "Full adapter source drift");
    *source = source.replacen(old, new, 1);
}

pub fn build_full() -> SuffixFixture {
    // Existing full preparation owns camera launches, all five uploads,
    // addressing, prefix/fallback controllers and the complete five-step plan.
    let mut result = suffix::build(false);
    let selected = suffix_f64::build("selected");
    let count = result.scene.record_count;
    assert_eq!(
        (result.scene.width, result.scene.height, count),
        (1920, 1080, 2_073_600)
    );
    assert_eq!(result.scene.sample_pixels.len(), 23);
    assert_eq!(result.scene.sample_indices.len(), 23);
    assert_eq!(result.state_words, 24);
    assert_eq!(result.state_bytes, 96 * u64::from(count));
    assert_eq!(result.scene.output_bytes, 16 + 80 * u64::from(count));
    assert_eq!(result.progress_bytes, 16);
    assert_eq!(
        result.modules.iter().map(|m| m.name).collect::<Vec<_>>(),
        ["prefix", "suffix", "fallback"]
    );
    assert_eq!(result.steps.len(), 5);

    // These are record-layout operands, not the 23-bit exponent shifts in the
    // interval helpers. Every numerical function remains byte-for-byte intact.
    let mut promoted = selected.modules[1].shader.clone();
    once(
        &mut promoted,
        "let id=invocation.x;if id>=23u{return;}",
        &format!("let id=invocation.x;if id>={count}u{{return;}}"),
    );
    once(
        &mut promoted,
        "let index=1u+3u*id;let detail_index=1u+3u*23u+2u*id;",
        &format!("let index=1u+3u*id;let detail_index=1u+3u*{count}u+2u*id;"),
    );
    result.modules[1].shader = promoted;

    // Only this invocation resets progress. No prefix/suffix/fallback path
    // accesses it again until the later validator dispatch. The host omits its
    // outside-timestamp clear, so every required reset is measured.
    once(
        &mut result.modules[0].shader,
        "if id.x==0u{terminals[0]=vec4(0u);}",
        "if id.x==0u{terminals[0]=vec4(0u);atomicStore(&cs_progress[0],0u);atomicStore(&cs_progress[1],0u);atomicStore(&cs_progress[2],0u);atomicStore(&cs_progress[3],0u);}",
    );
    result.scene.shader = result.modules[0].shader.clone();
    result.scene.cases["cartesian_composite_full"] = json!({
        "source":"scripts/optical_cartesian_composite_full_fixture.rs",
        "policy":"docs/science/cartesian-composite-full-decision.md",
        "selected":false,"dimensions":[1920,1080],"record_count":count,
        "numerical_source":"scripts/optical_cartesian_suffix_f64_fixture.rs",
        "factor":0.25,"maximum_candidate_attempts":4096,
        "state_words_per_record":24,"state_bytes":result.state_bytes,
        "output_bytes":result.scene.output_bytes,"progress_bytes":16,
        "source_transform":"Existing full prefix/fallback and packet preparation. Promoted suffix changes only its record bound and diagnostic offset. Prefix invocation zero adds four progress resets. No numerical expression changes.",
        "progress_reset_inside_timed_sequence":true,
        "host_progress_clear_required":false,
        "scope":"One independently gated full-output and complete-chain decision after composite matrix closure. Every original route remains exact; all classes/launches and frozen actual-view sample metrics remain mandatory. Prior rejected experiments are unchanged. No production or moving-flight claim.",
        "timing_scope":"Initialization, prefix, suffix, discarded work, original-eye fallback, validation and publication. Readback and pipeline compilation excluded."
    });
    result
}
