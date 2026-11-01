//! Original MIT. Exact source composition shared by isolated trial and ray diagnostics.
/// Copy the exact stage and certificate text, rather than reimplementing it.
/// The original complete nh_trial remains present and is the detail reference.
pub fn split_trial(shader: &str) -> String {
    let signature = "fn nh_trial(start:NcState,step:f32)->NhCurve{";
    let begin = shader.find(signature).unwrap();
    assert_eq!(shader.matches(signature).count(), 1);
    // Match the function's braces, independent of CRLF/LF file boundaries.
    // All current function comments contain no braces; assert that a future
    // extraction never silently pulls in another helper declaration.
    let mut depth = 1usize;
    let end = shader[begin + signature.len()..]
        .char_indices()
        .find_map(|(offset, ch)| {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(begin + signature.len() + offset + 1)
        })
        .expect("balanced original trial function");
    let original = &shader[begin..end];
    assert_eq!(original.matches("fn ").count(), 1);
    let boundary = "    let chart_variation=nc_f_add(nc_f_add(nc_l1(q2.yz),nc_l1(q3.yz)),nc_f_add(nc_l1(q4.yz),nc_l1(q5.yz)));";
    assert_eq!(original.matches(boundary).count(), 1);
    let split = original.find(boundary).unwrap();
    let mut stages = original[..split].replace(
        signature,
        "fn nh_stages(start:NcState,step:f32)->NhStage{\n    var stage_index=0u;",
    );
    stages = stages.replace(
        "return nh_invalid();",
        "return nh_stage_invalid(stage_index);",
    );
    for i in 0..=6 {
        let guard = format!("if k{i}.valid==0u{{return nh_stage_invalid(stage_index);}}");
        assert_eq!(stages.matches(&guard).count(), 1);
        stages = stages.replace(&guard, &format!("{guard}stage_index={}u;", i + 1));
    }
    stages.push_str("    return NhStage(NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),0.0,1u),eq,eu,en,8u);\n}\n");
    let mut bounds = String::from(
        r#"
fn nh_bounds(start:NcState,step:f32,stage:NhStage)->NhCurve{
    if stage.index!=8u{return nh_invalid();}
    let orientation=select(-1.0,1.0,step>0.0);
    let q2=stage.data.q2;let q3=stage.data.q3;let q4=stage.data.q4;let q5=stage.data.q5;
    let u2=stage.data.u2;let u3=stage.data.u3;let u4=stage.data.u4;let u5=stage.data.u5;
    let n2=stage.data.optical.x;let n3=stage.data.optical.y;let n4=stage.data.optical.z;let n5=stage.data.optical.w;
    let eq=stage.eq;let eu=stage.eu;let en=stage.en;
"#,
    );
    bounds.push_str(&original[split..]);
    format!("{STAGE_TYPE}\n{stages}\n{bounds}")
}

const STAGE_TYPE: &str = r#"
struct NhStage { data:NcCurve,eq:vec3<f32>,eu:vec3<f32>,en:f32,index:u32 };
fn nh_stage_invalid(index:u32)->NhStage{
    return NhStage(NcCurve(vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec4(0.0),0.0,0u),vec3(0.0),vec3(0.0),0.0,index);
}
"#;

/// The original function remains an explicit reference. Only the diagnostic
/// nh_trial entry selects the new certificate; physical stage text is copied.
pub fn inject_fast(mut shader: String) -> String {
    let split = split_trial(&shader);
    let signature = "fn nh_trial(start:NcState,step:f32)->NhCurve{";
    assert_eq!(shader.matches(signature).count(), 1);
    shader = shader.replace(
        signature,
        "fn nh_trial_reference(start:NcState,step:f32)->NhCurve{",
    );
    let mut helpers = split;
    helpers.push_str(&std::fs::read_to_string("scripts/optical_height_bounds_fast.wgsl").unwrap());
    helpers.push_str("\nfn nh_trial(start:NcState,step:f32)->NhCurve{return nh_bounds_fast(start,step,nh_stages(start,step));}\n");
    let bindings = shader.find("\n@group(0) @binding(1)").unwrap();
    shader.insert_str(bindings, &helpers);
    shader
}
