//! Original MIT. Separate composite-solver matrix; numerical shaders unchanged.
//! The rejected required-route experiment and its constructor remain intact.
use super::{suffix::SuffixFixture, suffix_branch, suffix_f64, suffix_trace::TraceFixture};
#[path = "optical_cartesian_suffix_atmosphere_fixture.rs"]
mod atmosphere;
#[path = "optical_cartesian_suffix_turning_fixture.rs"]
mod turning;
use serde_json::json;

pub const CASES: [&str; 11] = [
    "readiness",
    "fisheye",
    "vacuum-outward",
    "vacuum-inward",
    "atmosphere-minus20-vacuum",
    "atmosphere-minus20-gradient",
    "atmosphere-100-vacuum",
    "atmosphere-100-gradient",
    "atmosphere-5000-vacuum",
    "atmosphere-5000-gradient",
    "returning-gradient",
];

fn remove_handoff(source: &mut String) {
    // This is the exact optional block removed by the existing branch control.
    // No original trace expression, guard, attempt or terminal changes.
    let block = r#"        if can_handoff {
            let owner=cs_initial_owner(state,g);
            if owner>=-1 && g.w!=0.0 && cs_point_scene_clear(state.p,max(0.002,4e-7*length(state.p))){
                cs_handoff(id,state,path,entry,g,owner,u32(i));return;
            }
            can_handoff=false;
        }
"#;
    assert_eq!(
        source.matches(block).count(),
        1,
        "Composite reference handoff drift"
    );
    *source = source.replacen(block, "", 1);
}

pub fn build(case: &str, reference: bool) -> SuffixFixture {
    assert!(CASES.contains(&case), "Unknown composite matrix case");
    assert!(
        !reference || case == turning::CASE,
        "Only returning-gradient needs a new original control"
    );
    // The original selected data are reused from retained raw output, never
    // rerun through this constructor. This is solely a base for packet edits.
    let base = suffix_f64::build("selected");
    let exact_suffix = base.modules[1].shader.clone();
    let original_primary = base.scene.primary_uniform.clone();
    let mut wrapper = TraceFixture {
        suffix: base,
        auxiliary_bytes: 0,
        log_words_per_record: 0,
        max_steps: 4096,
    };
    if suffix_branch::CASES.contains(&case) {
        // Reuse the already reviewed, coherent branch upload preparation.
        // Its logged f32 shaders are not copied into this unlogged candidate.
        let packet = suffix_branch::build(case, false).suffix.scene;
        assert_eq!(packet.record_count, wrapper.suffix.scene.record_count);
        assert_eq!(packet.sample_pixels, wrapper.suffix.scene.sample_pixels);
        assert_eq!(packet.sample_indices, wrapper.suffix.scene.sample_indices);
        assert_eq!(packet.control, wrapper.suffix.scene.control);
        assert_eq!(packet.field_uniform, wrapper.suffix.scene.field_uniform);
        assert_eq!(packet.certificates, wrapper.suffix.scene.certificates);
        wrapper.suffix.scene.primary_uniform = packet.primary_uniform;
        wrapper.suffix.scene.geometry = packet.geometry;
        wrapper.suffix.scene.frames = packet.frames;
        for key in ["wavefront_branch_matrix", "cartesian_suffix_branch"] {
            wrapper.suffix.scene.cases[key] = packet.cases[key].clone();
        }
    } else if atmosphere::CASES.contains(&case) {
        // This modifies only the original upload/launch and explicitly scoped
        // atmosphere no-contact/covered-h0 functions in prefix/fallback.
        atmosphere::apply(case, &mut wrapper);
    } else {
        assert_eq!(case, turning::CASE);
        turning::apply(&mut wrapper);
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
    assert_eq!(
        result.modules[1].shader, exact_suffix,
        "Packet override changed the promoted solver"
    );
    for module in &result.modules {
        assert!(
            !module.shader.contains("cst_log"),
            "Composite matrix must stay unlogged"
        );
    }
    if reference {
        remove_handoff(&mut result.modules[0].shader);
    }
    result.scene.shader = result.modules[0].shader.clone();
    let changed_words: Vec<_> = original_primary
        .chunks_exact(4)
        .zip(result.scene.primary_uniform.chunks_exact(4))
        .enumerate()
        .filter_map(|(i, (old, new))| (old != new).then_some(i))
        .collect();
    result.scene.cases["cartesian_suffix_f64"]["scope_override"] = json!(
        "Historical required-route metadata describes the rejected initial experiment. This separate composite matrix uses its prospectively frozen policy; the numerical controller and all final accuracy thresholds are unchanged."
    );
    result.scene.cases["cartesian_composite_f64"] = json!({
        "source":"scripts/optical_cartesian_composite_fixture.rs",
        "policy":"docs/science/cartesian-composite-f64-policy.md",
        "case":case,"reference":reference,"selected":true,"timing_permitted":false,
        "base_numerical_source":"scripts/optical_cartesian_suffix_f64_fixture.rs",
        "common_suffix_byte_exact":true,"factor":0.25,
        "primary_changed_word_indices":changed_words,
        "state_words_per_record":24,"auxiliary_log_present":false,
        "scope":"Separately authorized complete composite solver evaluation. All fallback/unhanded outputs must match the case original exactly; every final applicable no-worse and absolute accuracy gate remains frozen. Route populations describe acceleration coverage and do not supply accuracy evidence for discarded suffixes. No timing or production approval.",
        "original_controls":"Reuse ten previously frozen branch/atmosphere originals. Only returning-gradient reference removes the exact optional handoff block to prepare its prospective full-trace original baseline.",
        "selected_reuse":"The rejected selected run remains rejected under its original required-route policy and is reused without another GPU dispatch. This constructor does not accept selected.",
        "returning_scope":"Physical uncovered-h0 turning control retains its distinct observed turn, handoff/fallback, class and exact-original requirements; the old prescribed-path endpoint is unused.",
        "full_chain_gate":"After matrix closure only: one matched complete-chain timing includes initialization, prefix, suffix, discarded work, original fallback and validation; unchanged at-least20percent effort gate and separate native moving-view gate."
    });
    result
}
