//! Original MIT. Fixed-control branch packets; no solver arithmetic changes.
//! Reference runs remove only the optional handoff from the original prefix.
use super::{suffix_tight, suffix_trace::TraceFixture, wavefront};
#[path = "optical_cartesian_suffix_atmosphere_fixture.rs"]
mod atmosphere;
#[path = "optical_wavefront_branch_fixture.rs"]
mod packets;
use earth_math::{
    Geodetic, Observer, TerrainScene, atmosphere::gpu::scene::PackedScene, rendering::LocalFrame,
};
use serde_json::json;

pub const CASES: [&str; 4] = ["readiness", "fisheye", "vacuum-outward", "vacuum-inward"];

fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect()
}

fn remove_handoff(source: &mut String) {
    // This is the optional block inserted by the reviewed suffix adapter.
    // Original initialization, terminal guards, all attempts and publication
    // remain; can_handoff assignments are harmless diagnostic bookkeeping.
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
        "Branch reference handoff drift"
    );
    *source = source.replacen(block, "", 1);
}

pub fn build(case: &str, reference: bool) -> TraceFixture {
    assert!(
        CASES.contains(&case) || atmosphere::CASES.contains(&case),
        "Unknown fixed suffix branch case"
    );
    let mut result = suffix_tight::build();
    assert_eq!(result.suffix.scene.record_count, 23);
    let original_uniform = result.suffix.scene.primary_uniform.clone();
    if CASES.contains(&case) {
        let packet = packets::build(case, 32, true).scene;
        assert_eq!(packet.record_count, result.suffix.scene.record_count);
        assert_eq!(packet.sample_pixels, result.suffix.scene.sample_pixels);
        assert_eq!(packet.control, result.suffix.scene.control);
        assert_eq!(packet.primary_uniform.len(), 2240);
        assert_eq!(packet.geometry, result.suffix.scene.geometry);
        result.suffix.scene.primary_uniform = packet.primary_uniform;
        result.suffix.scene.frames = packet.frames;
        result.suffix.scene.cases["wavefront_branch_matrix"] =
            packet.cases["wavefront_branch_matrix"].clone();
    } else {
        atmosphere::apply(case, &mut result);
    }

    if case.starts_with("vacuum-") {
        // The earlier scheduling-only fixture zeroed roots in a retained
        // frame buffer. The stricter clearance reader additionally checks
        // all header addresses. Prepare an actual coherent empty packet;
        // this changes no geometry and is identical for reference/candidate.
        let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 2000.0).unwrap());
        observer.feet.height_m = 2000.0 - observer.eye_height_m;
        assert_eq!(observer.eye_geodetic().height_m, 2000.0);
        let empty = PackedScene::prepare(&TerrainScene::new(vec![])).unwrap();
        let frames = empty.frame_data(LocalFrame::at(observer), &[]).unwrap();
        assert_eq!(empty.geometry.len(), 1);
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[0].map(f32::to_bits), [0; 4]);
        assert_eq!(frames[1][0].to_bits(), 6);
        assert_eq!(frames[2][3].to_bits(), 0);
        result.suffix.scene.geometry = bytes(&empty.geometry);
        result.suffix.scene.frames = bytes(&frames);
        result.suffix.scene.cases["wavefront_branch_matrix"]["geometry_buffer"] = json!(
            "Superseded for this suffix branch pair: freshly prepared empty PackedScene, one geometry vec4 and six frame vec4s; all root/count/address fields coherent. The original h0 and vacuum-entry code still runs unchanged."
        );
    }

    if reference {
        remove_handoff(&mut result.suffix.modules[0].shader);
    }
    result.suffix.scene.shader = result.suffix.modules[0].shader.clone();
    let changed_words: Vec<_> = original_uniform
        .chunks_exact(4)
        .zip(result.suffix.scene.primary_uniform.chunks_exact(4))
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect();
    result.suffix.scene.cases["cartesian_suffix_branch"] = json!({
        "case":case,"reference":reference,
        "source":"scripts/optical_cartesian_suffix_branch_fixture.rs",
        "packet_source":if CASES.contains(&case){"scripts/optical_wavefront_branch_fixture.rs"}else{"scripts/optical_cartesian_suffix_atmosphere_fixture.rs"},
        "factor":0.25,
        "scope":"Selected-only initialization and atmosphere matrix. Original baseline removes only the optional accepted-state handoff; candidate retains the reviewed fixed0.25 suffix. Every packet baseline and numerical policy must be retained before its candidate readback. Atmosphere-only stubs and fixed launch controls, when present, are separately labeled. No timing or production claim.",
        "primary_changed_word_indices":changed_words,
        "reference_source_change":"Delete exactly the optional can_handoff block; original trace expressions, attempt count and terminal publication remain unchanged.",
        "packet_identity":"Baseline/candidate have identical five uploaded buffers and selected sample mapping. Synthetic vacuum uses the original GpuEllipsoid at2000m, two zero-gradient source layers0/1000m and an actual empty PackedScene with a coherent6vec4 frame header.",
        "live_physics":"All three modules use only PrimarySettings for field/geometry. The retained canonical field and certificate buffers are unused by these pruned modules; they are not asserted to describe the synthetic eye/profile.",
        "required_original_routes":"Every unhanded/fallback/masked terminal must match its case original exactly; accepted suffix skies require prospective same-ray reference checks. A fallback must be observed, not inferred from the preset name.",
        "missing_scope":"No altered iteration cutoff, forced solver decline or manufactured4096 exhaustion. Initialization branches, atmosphere-only accuracy, and inversion/turning coverage are separate gates; this metadata does not imply their acceptance."
    });
    // Old metadata describes the retained actual23 experiment. Keep its
    // historical facts nested, and give this packet's intended scope priority.
    result.suffix.scene.cases["cartesian_suffix_tight_control"]["scope_override"] = json!(
        "The prior same10routes/13originals list describes the actual-scene control only. This separate branch packet uses its own original baseline and prospective policy; the numerical factor remains0.25."
    );
    result
}
