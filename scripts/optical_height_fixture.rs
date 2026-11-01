//! Original MIT. Isolated height/path shader composition; no host packet change.
pub fn inject(mut shader: String) -> String {
    let begin = shader
        .find("fn nc_trace(direction:")
        .expect("trace controller");
    let end = shader[begin..]
        .find("\n@group(0) @binding(1)")
        .map(|n| begin + n)
        .expect("ray bindings");
    let mut hybrid = shader[begin..end].to_owned();
    // Helpers follow the original controller before the entry-point bindings.
    // Select only its actual function, rather than duplicating those helpers.
    let function_end = hybrid.find("\n}\n").expect("controller end") + 3;
    hybrid.truncate(function_end);
    let initialization = "    var state=NcState(vec3(0.0),direction);var path=0.0;var proposed=min(2000.0,nc_settings.limits.y);";
    assert_eq!(hybrid.matches(initialization).count(), 1);
    hybrid=hybrid.replace(initialization,&format!("{initialization}\n    var height_proposed=proposed*abs(direction.y);var height_retries=0u;nh_attempts=0u;nh_accepted=0u;"));
    let insertion = "        let step=nc_source_proposal(state,adaptive);";
    assert_eq!(hybrid.matches(insertion).count(), 1);
    hybrid = hybrid.replace(insertion, &format!("{}\n{insertion}", HEIGHT_ATTEMPT));
    let finish = "        if curve.error<tolerance*0.03125{proposed=min(adaptive*2.0,nc_settings.limits.y);}";
    assert_eq!(hybrid.matches(finish).count(), 1);
    hybrid=hybrid.replace(finish,&format!("{finish}\n        height_proposed=max(0.000000001,abs(dot(state.u,nc_normal(state.q)))*proposed);height_retries=0u;"));
    shader.replace_range(
        begin..begin + "fn nc_trace".len(),
        "fn nc_trace_path_reference",
    );
    let mut helpers = std::fs::read_to_string("scripts/optical_height_step.wgsl").unwrap();
    helpers.push_str(&std::fs::read_to_string("scripts/optical_height_trace.wgsl").unwrap());
    helpers.push_str(&hybrid);
    let bindings = shader.find("\n@group(0) @binding(1)").unwrap();
    shader.insert_str(bindings, &helpers);
    shader
}

const HEIGHT_ATTEMPT: &str = r#"
        let height=nh_attempt(state,height_proposed,adaptive,tolerance);
        if height.kind==2u{
            var next=height.state;
            nc_debug4=vec4(height.curve.data.error,nc_anchor,f32(nh_accepted));
            if height.crossing!=0{
                if height.crossing<0 && owner==0u{return nc_result(state,path,2u,attempt,5u);}
                let target_index=select(owner,owner-1u,height.crossing<0);let node=nc_settings.layers[target_index];
                owner=select(owner+1u,owner-1u,height.crossing<0);nc_anchor=node.xy;next.q.x=0.0;
                nc_index=vec2(node.z,0.0);if owner>0u && owner<count{nc_index.y=nc_settings.layers[owner-1u].w;}
                if height.crossing>0{
                    nc_lower_gap=vec3(0.0);let gap=nc_settings.gaps[target_index];nc_upper_gap=vec3(-gap.xy,gap.z);
                }else{
                    nc_upper_gap=vec3(0.0);let gap=nc_settings.gaps[select(target_index-1u,count-1u,target_index==0u)];nc_lower_gap=gap.xyz;
                }
            }
            state=next;path+=height.path;nh_accepted+=1u;height_retries=0u;
            if owner==count{return nc_result(state,path,0u,attempt,0u);}
            let growth=select(1.0,2.0,height.curve.data.error<tolerance*0.03125);
            height_proposed=min(max(height_proposed,height.attempted_height)*growth,nc_settings.limits.y);
            proposed=min(max(adaptive,height.path)*growth,nc_settings.limits.y);
            continue;
        }
        if height.kind==1u && height_retries<8u && height.attempted_height>0.000000002{
            height_proposed=height.attempted_height*0.5;height_retries+=1u;continue;
        }
        // Every accepted path interval can enable height stepping again.
        // Refusal/failure never permanently assigns the whole ray to a method.
"#;
