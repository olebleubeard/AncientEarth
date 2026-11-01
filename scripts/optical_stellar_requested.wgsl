// Original MIT. Unregistered requested-star scheduling diagnostic.
// Shared extraction supplies settings, rays (packed catalogue), output,
// inverse_star and the unchanged star_face function.
// Settings appends lens:vec4<f32> after the existing control field.
// Terminals have the exact native primary 16-byte header + 3*uvec4/pixel.
@group(0) @binding(3) var<storage,read> primary_terminals:array<vec4<u32>>;
@group(0) @binding(4) var<storage,read_write> stellar_requests:array<atomic<u32>>;

@compute @workgroup_size(32)
fn clear_stellar_requests(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=3000u{return;}
    atomicStore(&stellar_requests[id.x],0u);
    output[1u+id.x]=vec4(0u);
    if id.x==0u{output[0]=vec4(0u);}
}

@compute @workgroup_size(32)
fn gather_stellar_requests(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=settings.control.x*settings.control.y{return;}
    if any(primary_terminals[0]!=settings.control){return;}
    let terminal=primary_terminals[1u+3u*id.x+1u];
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
    if id.x==0u{output[0]=settings.control;}
    let star=rays[6144u+2u*id.x];let tint=rays[6144u+2u*id.x+1u];
    let stable_id=u32(tint.w+0.5);
    if atomicLoad(&stellar_requests[stable_id])==0u{output[1u+stable_id]=vec4(0u);return;}
    let normal=normalize(star.xyz);
    let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
    let arrival=inverse_star(stellar_direction);
    output[1u+stable_id]=vec4(bitcast<vec3<u32>>(arrival.xyz),select(1u,2u,arrival.w!=0.0));
}

// The final consumer must first require output[0]==settings.control. This
// helper's unrequested case deliberately executes the original solve, so a
// gather/fragment finite-rounding disagreement cannot silently erase a star.
fn requested_arrival(star:vec4<f32>,tint:vec4<f32>)->vec4<f32>{
    let record=output[1u+u32(tint.w+0.5)];
    if record.w==1u{return vec4(0.0);}
    if record.w==2u{return vec4(bitcast<vec3<f32>>(record.xyz),1.0);}
    let normal=normalize(star.xyz);
    let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
    return inverse_star(stellar_direction);
}

// Untimed semantics checks use 3 records per star: returned arrival, original
// direct solve, and the raw cached record. A mismatched header writes explicit
// pending sentinels before any inverse evaluation. This is not display output.
@group(0) @binding(5) var<storage,read_write> stellar_consumer_output:array<vec4<u32>>;
@compute @workgroup_size(32)
fn stellar_consumer_checks(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=3000u{return;}
    let star=rays[6144u+2u*id.x];let tint=rays[6144u+2u*id.x+1u];
    let stable_id=u32(tint.w+0.5);let base=3u*stable_id;
    stellar_consumer_output[base+2u]=output[1u+stable_id];
    if any(output[0]!=settings.control)||any(primary_terminals[0]!=settings.control){
        stellar_consumer_output[base]=vec4(0xffffffffu);stellar_consumer_output[base+1u]=vec4(0xffffffffu);return;
    }
    let result=requested_arrival(star,tint);
    let normal=normalize(star.xyz);
    let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
    let direct=inverse_star(stellar_direction);
    stellar_consumer_output[base]=bitcast<vec4<u32>>(result);
    stellar_consumer_output[base+1u]=bitcast<vec4<u32>>(direct);
}
