// Original MIT. Same hybrid acceptance equations with one shared scene-query
// call site. No field/tableau/error/root helper is changed by this controller.
// Both parameterizations publish the unchanged accepted physical polynomial
// before any state/source ownership commit, then consume the same event gate.
fn nc_trace_scene(direction:vec3<f32>)->NcResult{
    let axis=vec3(0.0,nc_settings.pose.x,-nc_settings.pose.y);nc_axis=axis*inverseSqrt(dot(axis,axis));
    nc_eye_height=nc_settings.pose.zw;nc_anchor=nc_eye_height;nc_index=nc_settings.initial.xy;nc_root_count=0u;
    nc_lower_gap=nc_settings.initial_lower.xyz;nc_upper_gap=nc_settings.initial_upper.xyz;
    var owner=u32(nc_settings.initial.z);let count=u32(nc_settings.initial.w);
    nc_scene_anchor_residual=nc_certificates[4].xy;nc_scene_normal=vec3(0.0);nc_scene_material=0u;nc_scene_owner=0u;nc_scene_visits=0u;
    var state=NcState(vec3(0.0),direction);var path=0.0;var proposed=min(2000.0,nc_settings.limits.y);
    var height_proposed=proposed*abs(direction.y);var height_retries=0u;nh_attempts=0u;nh_accepted=0u;
    if count<2u||count>128u||owner>=count{return nc_result(state,path,2u,0u,1u);}
    for(var attempt=1u;attempt<=4096u;attempt+=1u){
        let position=nc_position(state.q);let tolerance=max(nc_settings.limits.w,4e-7*length(position));
        nc_debug0=vec4(state.q.x,proposed,tolerance,f32(owner));
        if path>nc_settings.limits.z{return nc_result(state,path,2u,attempt,2u);}
        let spacing_band=max(1e-5,4e-7*length(position));
        for(var adjacent=max(0,i32(owner)-2);adjacent<=min(i32(count)-2,i32(owner));adjacent+=1){
            let gap=nc_settings.gaps[u32(adjacent)];
            let spacing=nc_f_add(nc_f_add(nc_interval_exact(gap.x),nc_interval_exact(gap.y)),vec2(-gap.z,gap.z));
            if !nc_interval_valid(spacing)||spacing.x<4.0*spacing_band{return nc_result(state,path,2u,attempt,7u);}
        }
        let adaptive=min(proposed,min(nc_settings.limits.y,0.49/max(abs(nc_index.y),1e-30)));
        let height=nh_attempt(state,height_proposed,adaptive,tolerance);
        var is_height=false;var next=state;var delta_path=0.0;var crossing=0;
        var accepted_error=0.0;var attempted_step=0.0;var accepted_curve=nc_scene_invalid();
        if height.kind==2u{
            accepted_curve=nc_scene_from_height(state,height.curve,height.charge);
            if accepted_curve.valid==0u||accepted_curve.numerical_error>tolerance{height_proposed=height.attempted_height*0.5;height_retries+=1u;continue;}
            is_height=true;next=height.state;delta_path=height.path;crossing=height.crossing;accepted_error=height.curve.data.error;
        }else{
            if height.kind==1u&&height_retries<8u&&height.attempted_height>0.000000002{height_proposed=height.attempted_height*0.5;height_retries+=1u;continue;}
            let step=nc_source_proposal(state,adaptive);attempted_step=step;
            if !(step>=0.000001){return nc_result(state,path,2u,attempt,3u);}
            let curve=nc_trial(state,step);nc_debug4=vec4(curve.error,nc_anchor,0.0);
            if curve.valid==0u||!(curve.error<tolerance){proposed=step*0.5;continue;}
            let lower_kind=nc_boundary(state,curve,step,nc_lower_gap,true);let upper_kind=nc_boundary(state,curve,step,nc_upper_gap,false);
            nc_debug4.w=f32(lower_kind*10u+upper_kind);
            if lower_kind==0u||upper_kind==0u{proposed=step*0.5;continue;}
            var contact=NcRoot(1.0,0.0,0.0,1u);let allocation=(tolerance-curve.error)*0.125;
            if lower_kind==2u{contact=nc_height_root(state,curve,step,nc_lower_gap,allocation);crossing=-1;}
            if upper_kind==2u{
                let root=nc_height_root(state,curve,step,nc_upper_gap,allocation);
                if root.valid==0u{contact=root;}else if crossing==0||root.theta<contact.theta{contact=root;crossing=1;}
            }
            if contact.valid==0u{proposed=step*0.5;continue;}
            next=NcState(nc_curve_q(state,curve,contact.theta),normalize(nc_curve_u(state,curve,contact.theta)));
            if !nc_finite3(next.q)||!nc_finite3(next.u){return nc_result(state,path,2u,attempt,4u);}
            accepted_curve=nc_scene_from_path(state,curve,step,contact.theta,contact.charge);
            if accepted_curve.valid==0u||accepted_curve.numerical_error>tolerance{proposed=step*0.5;continue;}
            // Same already-certified source correction, before owner commit.
            // accepted_curve carries the identical outward total allowance.
            accepted_error=curve.error;delta_path=step*contact.theta;
        }
        let event=nc_scene_query(accepted_curve,tolerance);
        if event.kind==2u{
            let floor=max(0.001,9.536743e-7*max(length(position),length(nc_position(next.q))));
            if delta_path<=floor{return nc_result(state,path,2u,attempt,8u);}
            if is_height{height_proposed=height.attempted_height*0.5;proposed=min(adaptive,height.path*0.5);height_retries+=1u;}
            else{proposed=delta_path*0.5;height_proposed=min(height_proposed,abs(dot(state.u,nc_normal(state.q)))*proposed);}
            continue;
        }
        if event.kind==1u{return nc_scene_terminal(accepted_curve,event,path,attempt);}
        if is_height{nc_debug4=vec4(accepted_error,nc_anchor,f32(nh_accepted));}
        if crossing!=0{
            if crossing<0&&owner==0u{return nc_result(state,path,2u,attempt,5u);}
            let target_index=select(owner,owner-1u,crossing<0);let node=nc_settings.layers[target_index];
            owner=select(owner+1u,owner-1u,crossing<0);nc_anchor=node.xy;
            nc_scene_anchor_residual=nc_certificates[bitcast<u32>(nc_certificates[1].y)+target_index].xy;
            next.q.x=0.0;nc_index=vec2(node.z,0.0);if owner>0u&&owner<count{nc_index.y=nc_settings.layers[owner-1u].w;}
            if crossing>0{
                nc_lower_gap=vec3(0.0);let gap=nc_settings.gaps[target_index];nc_upper_gap=vec3(-gap.xy,gap.z);
            }else{
                nc_upper_gap=vec3(0.0);let gap=nc_settings.gaps[select(target_index-1u,count-1u,target_index==0u)];nc_lower_gap=gap.xyz;
            }
        }
        state=next;path+=delta_path;
        if is_height{nh_accepted+=1u;height_retries=0u;}
        if owner==count{return nc_result(state,path,0u,attempt,0u);}
        if is_height{
            let growth=select(1.0,2.0,accepted_error<tolerance*0.03125);
            height_proposed=min(max(height_proposed,height.attempted_height)*growth,nc_settings.limits.y);
            proposed=min(max(adaptive,height.path)*growth,nc_settings.limits.y);
        }else{
            proposed=attempted_step;if accepted_error<tolerance*0.03125{proposed=min(adaptive*2.0,nc_settings.limits.y);}
            height_proposed=max(0.000000001,abs(dot(state.u,nc_normal(state.q)))*proposed);height_retries=0u;
        }
    }
    return nc_result(state,path,2u,4096u,6u);
}
