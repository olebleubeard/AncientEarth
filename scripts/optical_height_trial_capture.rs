//! Original MIT. Instrumentation only; no solver decisions are changed.
use super::Fixture;
use serde_json::json;

pub fn inject(mut fixture: Fixture) -> Fixture {
    const CAPACITY: u32 = 128;
    const VECTORS_PER_TRIAL: u32 = 27;
    const RECORD_VECTORS: u32 = 10 + CAPACITY * VECTORS_PER_TRIAL;
    let entry = "    let launch=normalize(rays[id.x].xyz);let ray=nc_trace(launch);";
    assert_eq!(fixture.shader.matches(entry).count(), 1);
    fixture.shader = fixture.shader.replace(entry, &format!(
        "    nh_capture_base={RECORD_VECTORS}u*id.x+10u;nh_capture_count=0u;\n{entry}\n    result[{RECORD_VECTORS}u*id.x+9u]=vec4(nh_capture_count,{CAPACITY}u,select(0u,1u,nh_capture_count>{CAPACITY}u),0u);"));
    fixture.shader = fixture
        .shader
        .replace("9u*id.x", &format!("{RECORD_VECTORS}u*id.x"));
    let trial = "    let curve=nh_trial(start,step);result.curve=curve;";
    assert_eq!(fixture.shader.matches(trial).count(), 1);
    fixture.shader = fixture.shader.replace(
        trial,
        &format!("{trial}\n    nh_capture_trial(start,step,curve);"),
    );
    let attempt = "        let height=nh_attempt(state,height_proposed,adaptive,tolerance);";
    assert_eq!(fixture.shader.matches(attempt).count(), 1);
    fixture.shader = fixture.shader.replace(attempt, &format!(
        "        let capture_before=nh_capture_count;\n{attempt}\n        if nh_capture_count>capture_before && nh_capture_count<=128u{{result[nh_capture_base+27u*(nh_capture_count-1u)+24u]=vec4(height.kind,bitcast<u32>(height.charge),bitcast<u32>(tolerance),bitcast<u32>(adaptive));}}"));
    let entry = "fn nh_trial(start:NcState,step:f32)->NhCurve{";
    assert_eq!(fixture.shader.matches(entry).count(), 1);
    fixture.shader = fixture.shader.replace(entry, &format!(
        "{entry}\n    nh_capture_stage=NcCurve(vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec4(0.0),0.0,0u);nh_capture_eq=vec3(0.0);nh_capture_eu=vec3(0.0);nh_capture_en=0.0;nh_capture_stage_index=0u;"));
    // Each successful RHS evaluation is recorded before proceeding. Index 8
    // means all seven stages AND coefficient/embedded finite checks passed.
    for i in 0..=6 {
        let guard = format!("if k{i}.valid==0u{{return nh_invalid();}}");
        assert_eq!(fixture.shader.matches(&guard).count(), 1);
        fixture.shader = fixture.shader.replace(
            &guard,
            &format!("{guard}nh_capture_stage_index={}u;", i + 1),
        );
    }
    let bounds = "    let chart_variation=nc_f_add(nc_f_add(nc_l1(q2.yz),nc_l1(q3.yz)),nc_f_add(nc_l1(q4.yz),nc_l1(q5.yz)));";
    assert_eq!(fixture.shader.matches(bounds).count(), 1);
    fixture.shader = fixture.shader.replace(bounds, &format!(
        "    nh_capture_stage=NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),0.0,1u);nh_capture_eq=eq;nh_capture_eu=eu;nh_capture_en=en;nh_capture_stage_index=8u;\n{bounds}"));
    fixture.shader.push_str(CAPTURE);
    fixture.record_words = RECORD_VECTORS * 4;
    fixture.output_bytes = u64::from(fixture.count) * u64::from(fixture.record_words) * 4;
    fixture.cases["scope"] = json!("Actual seven-ray height-trial capture, including failed stages, rejected certificates and controller rejection. First36 terminal/debug words require checkpoint comparison. No timing or runtime acceptance.");
    fixture.cases["trial_capture"] = json!({
        "capacity":CAPACITY,"vectors_per_trial":VECTORS_PER_TRIAL,"header_word":36,
        "first_trial_word":40,"record_vectors":RECORD_VECTORS,
        "input_records":["start.q.xyz/height step","start.u.xyz/zero","anchor high/low/refractivity/slope"],
        "returned_curve_records":"3..12: q2..q5,u2..u5,optical,error/valid/vertical_lower/path_upper",
        "stage_records":"13..21: q2..q5,u2..u5,optical before bounds; zeros if stages failed",
        "embedded_records":"22: eq.xyz/en; 23: eu.xyz/raw stage index (8 means finite complete stages)",
        "controller_record":"24: kind u32/charge f32/tolerance f32/path-limit f32",
        "field_context_records":"25: exact runtime nc_axis.xyz/zero; 26: nc_eye_height.xy/zero/zero; full immutable NcSettings also retained"
    });
    fixture
}

const CAPTURE: &str = r#"
var<private> nh_capture_base:u32;
var<private> nh_capture_count:u32;
var<private> nh_capture_stage:NcCurve;
var<private> nh_capture_eq:vec3<f32>;
var<private> nh_capture_eu:vec3<f32>;
var<private> nh_capture_en:f32;
var<private> nh_capture_stage_index:u32;
fn nh_capture_trial(start:NcState,step:f32,curve:NhCurve){
    let index=nh_capture_count;nh_capture_count+=1u;if index>=128u{return;}
    let base=nh_capture_base+27u*index;let c=curve.data;let d=nh_capture_stage;
    result[base]=bitcast<vec4<u32>>(vec4(start.q,step));
    result[base+1u]=bitcast<vec4<u32>>(vec4(start.u,0.0));
    result[base+2u]=bitcast<vec4<u32>>(vec4(nc_anchor,nc_index));
    result[base+3u]=bitcast<vec4<u32>>(vec4(c.q2,0.0));
    result[base+4u]=bitcast<vec4<u32>>(vec4(c.q3,0.0));
    result[base+5u]=bitcast<vec4<u32>>(vec4(c.q4,0.0));
    result[base+6u]=bitcast<vec4<u32>>(vec4(c.q5,0.0));
    result[base+7u]=bitcast<vec4<u32>>(vec4(c.u2,0.0));
    result[base+8u]=bitcast<vec4<u32>>(vec4(c.u3,0.0));
    result[base+9u]=bitcast<vec4<u32>>(vec4(c.u4,0.0));
    result[base+10u]=bitcast<vec4<u32>>(vec4(c.u5,0.0));
    result[base+11u]=bitcast<vec4<u32>>(c.optical);
    result[base+12u]=vec4(bitcast<u32>(c.error),c.valid,bitcast<u32>(curve.vertical_lower),bitcast<u32>(curve.path_upper));
    result[base+13u]=bitcast<vec4<u32>>(vec4(d.q2,0.0));
    result[base+14u]=bitcast<vec4<u32>>(vec4(d.q3,0.0));
    result[base+15u]=bitcast<vec4<u32>>(vec4(d.q4,0.0));
    result[base+16u]=bitcast<vec4<u32>>(vec4(d.q5,0.0));
    result[base+17u]=bitcast<vec4<u32>>(vec4(d.u2,0.0));
    result[base+18u]=bitcast<vec4<u32>>(vec4(d.u3,0.0));
    result[base+19u]=bitcast<vec4<u32>>(vec4(d.u4,0.0));
    result[base+20u]=bitcast<vec4<u32>>(vec4(d.u5,0.0));
    result[base+21u]=bitcast<vec4<u32>>(d.optical);
    result[base+22u]=bitcast<vec4<u32>>(vec4(nh_capture_eq,nh_capture_en));
    result[base+23u]=vec4(bitcast<vec3<u32>>(nh_capture_eu),nh_capture_stage_index);
    result[base+24u]=vec4(0u);
    result[base+25u]=bitcast<vec4<u32>>(vec4(nc_axis,0.0));
    result[base+26u]=bitcast<vec4<u32>>(vec4(nc_eye_height,0.0,0.0));
}
"#;
