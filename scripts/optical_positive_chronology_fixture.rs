//! Original MIT. Isolated conservative later-face exclusion for the six
//! manufactured geometric-curve cases. Existing contact predicates, complete
//! state charges, budgets, traversal order and all packet bytes are preserved.
use super::CurvesFixture;
use serde_json::json;

fn replace(source: &mut String, old: &str, new: &str) {
    assert_eq!(
        source.matches(old).count(),
        1,
        "Chronology source drift: {old}"
    );
    *source = source.replace(old, new);
}

pub fn build() -> CurvesFixture {
    let mut fixture = super::build_positive_contacts();
    assert_eq!(fixture.scene.record_count, 6);
    assert_eq!(fixture.curves.len(), 6 * (64 + 80) * 4);
    for case in 0..6 {
        let at = (case * 64 + 7) * 4;
        let path_scale = f32::from_le_bytes(fixture.curves[at..at + 4].try_into().unwrap());
        assert!(path_scale.is_finite() && path_scale > 0.0);
    }
    assert!(
        fixture
            .scene
            .cases
            .get("h0_coverage_before_width")
            .is_some()
    );
    assert!(fixture.scene.cases.get("stage_diagnostics").is_none());
    let shader = &mut fixture.scene.shader;
    replace(
        shader,
        "fn nc_scene_face_event_cached(curve:NcSceneCurve,parameters:NcMapParameters,address:u32,material:u32,owner:u32,width:f32,tolerance:f32,context:NcFaceContext)->NcSceneEvent{",
        "fn nc_scene_face_event_cached(curve:NcSceneCurve,parameters:NcMapParameters,address:u32,material:u32,owner:u32,width:f32,tolerance:f32,context:NcFaceContext,best:NcSceneEvent)->NcSceneEvent{",
    );
    replace(
        shader,
        "let face=nc_scene_face(address);let contact=nc_face_contact_cached(curve,parameters,face,width,context);",
        "let face=nc_scene_face(address);if nc_cartesian_face_is_later(curve,face,context,best){return nc_scene_none();}let contact=nc_face_contact_cached(curve,parameters,face,width,context);",
    );
    replace(
        shader,
        "nc_scene_face_event_cached(curve,parameters,face_address,material,owner,width,tolerance,context)",
        "nc_scene_face_event_cached(curve,parameters,face_address,material,owner,width,tolerance,context,result)",
    );
    let helper = std::fs::read_to_string("scripts/optical_cartesian_chronology.wgsl").unwrap();
    assert_eq!(helper.matches("fn nc_cartesian_face_is_later(").count(), 1);
    shader.push('\n');
    shader.push_str(&helper);
    fixture.scene.cases["chronological_face_exclusion"] = json!({
        "source":"scripts/optical_cartesian_chronology.wgsl",
        "adapter":"scripts/optical_positive_chronology_fixture.rs",
        "scope":"A sufficient whole-piece plane-crossing enclosure may exclude a face only when its outward earliest possible theta strictly exceeds an already certified first-contact upper theta. All invalid, earlier or overlapping candidates execute the original face predicate. No new contact acceptance, uniqueness claim or tolerance allocation; unchanged positive affine path gives chronological theta ordering.",
        "unchanged":"All six uploaded buffers, terminal/detail ABI, original strict face predicates, complete contact charge, h0 coverage, traversal order and ambiguity combiner. The base six-case fixture remains independently reproducible."
    });
    fixture
}
