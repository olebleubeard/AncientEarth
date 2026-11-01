// Original MIT. Unregistered selected pipeline orchestration. Phases:
// 0 try a complete accepted interval, 1 needs a Cartesian certificate,
// 2 ready for scene query, 3 scene result available, 4 terminal.
// No source/physical state commit occurs between phases1..3 until every
// chronological piece is clear. A hit or uncertainty retains old policies.
fn ns_piece_range(index:u32,count:u32)->vec2<f32>{
    // count is an authored power of two in[1,64], index in[0,count-1].
    // Construct its exact normal reciprocal; approximate WGSL division must
    // not introduce gaps into the shared dyadic partition.
    let exponent=(bitcast<u32>(f32(count))>>23u)&255u;
    let inverse_count=bitcast<f32>((254u-exponent)<<23u);
    return vec2(f32(index),f32(index+1u))*inverse_count;
}
fn ns_piece_above_scene(range:vec2<f32>)->bool{
    let theta=vec2(range.x,min(range.y,ns.curve.theta_end));
    if !nc_interval_valid(theta){return false;}
    let c=ns.curve.physical;
    let q=nc_m_poly(ns.curve.start.q,c.q2,c.q3,c.q4,c.q5,theta);
    let height=nc_f_add(nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),q.x),nc_scene_anchor_residual);
    let lower=nc_f_sub(height,nc_interval_exact(ns.curve.numerical_error));
    return nc_m_valid(q)&&nc_interval_valid(lower)&&lower.x>max(0.0,nc_certificates[4].z);
}
fn ns_prepare_piece(first:bool)->bool{
    let remaining=nc_f_sub(nc_interval_exact(ns.tolerance),nc_interval_exact(ns.curve.numerical_error));
    let quarter=nc_f_mul(remaining,vec2(0.25));
    if !nc_interval_valid(quarter)||!(quarter.x>0.0){return false;}
    let parameters=nc_scene_parameters();
    if first{
        var count=1u;var admitted=false;
        loop{
            var complete=true;
            for(var index=0u;index<count;index+=1u){
                let range=ns_piece_range(index,count);if range.x>=ns.curve.theta_end{break;}
                if ns_piece_above_scene(range){continue;}
                let piece=nc_cartesian_certify(ns.curve,parameters,range,quarter.x);ns.certificate_count+=1u;
                if piece.valid==0u{complete=false;break;}
            }
            if complete{ns.pieces=count;admitted=true;break;}
            if count==64u{break;}count*=2u;
        }
        if !admitted{return false;}ns.piece_index=0u;
    }
    // Clear suffixes (or other individually proven clear pieces) need no map
    // certificate. No physical state/source commit occurs while skipping.
    loop{
        if ns.piece_index>=ns.pieces||ns_piece_range(ns.piece_index,ns.pieces).x>=ns.curve.theta_end{
            ns.event=nc_scene_none();ns.phase=3u;return true;
        }
        if !ns_piece_above_scene(ns_piece_range(ns.piece_index,ns.pieces)){break;}
        ns.piece_index+=1u;
    }
    let range=ns_piece_range(ns.piece_index,ns.pieces);
    let piece=nc_cartesian_certify(ns.curve,parameters,range,quarter.x);ns.certificate_count+=1u;
    if piece.valid==0u{return false;}
    ns_write_piece(piece);ns.phase=2u;return true;
}

@compute @workgroup_size(32)
fn normal_selected_produce(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=NS_COUNT{return;}ns_load(id.x);if ns.phase==4u||ns.phase==2u{return;}
    // Every retry consumes the original4096 attempt budget in ns_attempt.
    // This local loop only handles ready scene results and certificate failure.
    for(var resumed=0u;resumed<=4096u;resumed+=1u){
        if ns.phase==3u{
            if ns.event.kind==0u{
                let next_piece=ns.piece_index+1u;
                if next_piece<ns.pieces&&ns_piece_range(next_piece,ns.pieces).x<ns.curve.theta_end{
                    ns.piece_index=next_piece;
                    if ns_prepare_piece(false){ns_store();return;}
                    ns.event=nc_scene_uncertain();
                }
            }
            let result=ns_commit(ns.event);
            if result.status!=4u{ns_publish(result);return;}
            ns.phase=0u;
        }
        if ns.phase==0u{
            let result=ns_attempt();
            if result.status!=5u{ns_publish(result);return;}
        }
        if ns.phase==1u{
            if ns_prepare_piece(true){ns_store();return;}
            ns.event=nc_scene_uncertain();ns.phase=3u;
        }
    }
    // No fabricated terminal or source commit on an operational scheduling
    // limit. Finalize leaves the packet invalid until all rays are terminal.
    ns_store();
}

@compute @workgroup_size(32)
fn normal_selected_consume(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=NS_COUNT{return;}ns_load(id.x);if ns.phase!=2u{return;}
    nc_cartesian_piece=ns_read_piece();
    // Certification may enclose the full dyadic piece beyond a source root;
    // the scene query always stops at the ORIGINAL accepted physical prefix.
    var query=ns.curve;query.theta_end=min(ns.curve.theta_end,nc_cartesian_piece.parameter.y);
    ns.event=nc_scene_query_cached(query,ns.tolerance);ns.query_count+=1u;ns.phase=3u;ns_store();
}

@compute @workgroup_size(1)
fn normal_selected_finalize(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x!=0u{return;}var pending=0u;var completed=0u;var maximum=0u;var bad=0u;
    for(var i=0u;i<NS_COUNT;i+=1u){
        let ids=ns_records[32u*i+7u];let record=ns_records[32u*i+12u];
        if record.z!=i||ids.y>4096u||ids.w>4u{bad+=1u;}
        if ids.w==4u{completed+=1u;}else{pending+=1u;}maximum=max(maximum,ids.y);
    }
    ns_progress[0]=pending;ns_progress[1]=completed;ns_progress[2]=maximum;ns_progress[3]=bad;
    if pending==0u&&completed==NS_COUNT&&bad==0u{terminals[0]=settings.control;}
}
