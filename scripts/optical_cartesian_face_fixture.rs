//! Original MIT. Single-face compiler/finite-query isolation only. The same
//! canonical face predicate and complete contact charge remain; BVH traversal,
//! reference-height coverage and event reduction are intentionally absent.
use super::CurvesFixture;
use serde_json::json;

pub fn build() -> CurvesFixture {
    let mut fixture = super::build_cartesian_curves();
    let old = "event=nc_scene_query_cached(curve,tolerance);";
    assert_eq!(fixture.scene.shader.matches(old).count(), 1);
    fixture.scene.shader =
        fixture
            .scene
            .shader
            .replacen(old, "event=nc_cartesian_one_face(curve,tolerance);", 1);
    fixture.scene.shader.push_str(QUERY);
    let context = fixture.scene.cases.clone();
    fixture.scene.cases = json!({
        "scope":"Isolated canonical face query on the same546 certified Cartesian pieces. Each invocation tests only prism face0 of marker0. Status4 excludes that one face only, status1 is a certified contact with that face only, status2 preserves uncertainty. No BVH, h0 coverage, other-face ordering, complete scene clearance, complete ray or performance claim. Original strict face/root/edge/complete-state charge is unchanged.",
        "single_face":true,"face_selection":{"marker":0,"face":0,"material":1},
        "cartesian_fixture":context,
        "record_layout":["16-byte control header","48-byte per-face terminal-shaped result","32-byte event/visit/capture-ID and root/tolerance details"]
    });
    fixture
}

const QUERY: &str = r#"
fn nc_cartesian_one_face(curve:NcSceneCurve,tolerance:f32)->NcSceneEvent{
    if curve.valid==0u||!(tolerance>0.0){return nc_scene_uncertain();}
    let parameters=nc_scene_parameters();
    let start=nc_cartesian_local_point(nc_cartesian_piece,vec2(nc_cartesian_piece.parameter.x));
    let finish=nc_cartesian_local_point(nc_cartesian_piece,vec2(curve.theta_end));
    let acceleration=nc_scene_acceleration_upper(curve,nc_interval_abs_upper(parameters.anchor_residual));
    let speed=nc_root_speed(curve.start,curve.physical,1.0);
    let width_interval=nc_f_div(nc_f_mul(vec2(tolerance),vec2(0.01)),vec2(max(1.0,speed.y)));
    if start.valid==0u||finish.valid==0u||!nc_interval_valid(acceleration)
        ||!nc_interval_valid(speed)||!nc_interval_valid(width_interval)||!(width_interval.x>0.0){return nc_scene_uncertain();}
    let marker_count=bitcast<u32>(nc_certificates[0].y);
    if marker_count==0u{return nc_scene_uncertain();}
    let marker_first=bitcast<u32>(nc_certificates[1].x);
    let face_address=bitcast<u32>(nc_certificates[marker_first].x);
    let context=NcFaceContext(start,finish,acceleration);
    return nc_scene_face_event_cached(curve,parameters,face_address,1u,0u,min(1.0,width_interval.x),tolerance,context);
}
"#;
