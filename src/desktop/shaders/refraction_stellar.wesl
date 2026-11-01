// Original MIT. Same-frame sparse request sharing; ordered clear, gather, solve.
// All arrivals are cleared before the current header is published. Unrequested
// records retain state0 for the original per-pixel inverse fallback.
@compute @workgroup_size(32)
fn clear_stellar_requests(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=3000u{return;}
    atomicStore(&stellar_requests[id.x],0u);
    output[1u+id.x]=vec4(0u);
    if id.x==0u{output[0]=vec4(0u);}
}

@compute @workgroup_size(32)
fn gather_stellar_requests(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=settings.control.x || id.y>=settings.control.y{return;}
    if settings.environment.z<=0.0{return;}
    let pixel_index=id.y*settings.control.x+id.x;
    if any(primary_terminals[0]!=settings.control){return;}
    let terminal=primary_terminals[1u+3u*pixel_index+1u];
    if terminal.w!=0u{return;}
    let outgoing=bitcast<vec3<f32>>(terminal.xyz);
    let direction=normalize(vec3(dot(outgoing,settings.icrf0.xyz),dot(outgoing,settings.icrf1.xyz),dot(outgoing,settings.icrf2.xyz)));
    // Same catalogue bin and angular predicates as catalogue_stars. Sun/Moon
    // pixel occlusion is left to the final compositor; this can only request
    // extra work, never remove a physically visible candidate.
    for(var face=0u;face<6u;face+=1u){
        let projected=star_face(direction,face);
        if projected.z<0.5 {continue;}
        let coord=projected.xy/projected.z;
        if max(abs(coord.x),abs(coord.y))>1.06 {continue;}
        let cell=clamp(vec2<i32>(floor((coord+1.0)*16.0)),vec2<i32>(0),vec2<i32>(31));
        for(var dy=-1;dy<=1;dy+=1){for(var dx=-1;dx<=1;dx+=1){
            let c=cell+vec2(dx,dy);
            if any(c<vec2<i32>(0))||any(c>vec2<i32>(31)){continue;}
            let bin=rays[face*1024u+u32(c.y)*32u+u32(c.x)];
            for(var index=0u;index<u32(bin.y);index+=1u){
                let star=rays[u32(bin.x)+2u*index];
                let tint=rays[u32(bin.x)+2u*index+1u];
                let normal=normalize(star.xyz);
                if length(cross(direction,normal))>max(3.0/max(settings.lens.w,1.0),0.00003){continue;}
                atomicOr(&stellar_requests[u32(tint.w+0.5)],1u);
            }
        }}
    }
}

@compute @workgroup_size(32)
fn requested_stellar_table(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=3000u{return;}
    // All records are overwritten before a later compositor can consume this
    // dispatch. Header is a same-queue dependency token, not a completion fence.
    if id.x==0u{output[0]=select(vec4(0u),settings.control,all(primary_terminals[0]==settings.control));}
    if settings.environment.z<=0.0 || any(primary_terminals[0]!=settings.control){return;}
    let star=rays[6144u+2u*id.x];let tint=rays[6144u+2u*id.x+1u];
    let stable_id=u32(tint.w+0.5);
    if atomicLoad(&stellar_requests[stable_id])==0u{output[1u+stable_id]=vec4(0u);return;}
    let normal=normalize(star.xyz);
    let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
    let arrival=inverse_star(stellar_direction);
    output[1u+stable_id]=vec4(bitcast<vec3<u32>>(arrival.xyz),select(1u,2u,arrival.w!=0.0));
}

