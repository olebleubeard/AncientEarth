// Original MIT. Matched per-pixel stellar-only compositor diagnostic.
// Both entries use the same fixed physical terminals, camera, catalogue,
// candidate gates, pixel ownership, reference scintillation and brightness.
// The fixture has identity catalogue rotation, full night weight and no bodies.
@compute @workgroup_size(32)
fn direct_stellar_pixels(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=settings.control.x*settings.control.y{return;}
    if any(primary_terminals[0]!=settings.control){stellar_consumer_output[id.x]=vec4(0xffffffffu);return;}
    let terminal=primary_terminals[1u+3u*id.x+1u];
    if terminal.w!=0u{stellar_consumer_output[id.x]=vec4(0u);return;}
    let outgoing=bitcast<vec3<f32>>(terminal.xyz);
    let inertial=normalize(vec3(dot(outgoing,settings.icrf0.xyz),dot(outgoing,settings.icrf1.xyz),dot(outgoing,settings.icrf2.xyz)));
    let pixel=vec2<f32>(vec2<u32>(id.x%settings.control.x,id.x/settings.control.x));
    let color=direct_catalogue_stars(inertial,pixel)*settings.environment.z;
    stellar_consumer_output[id.x]=bitcast<vec4<u32>>(vec4(color,1.0));
}
@compute @workgroup_size(32)
fn requested_stellar_pixels(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=settings.control.x*settings.control.y{return;}
    if any(primary_terminals[0]!=settings.control)||any(output[0]!=settings.control){stellar_consumer_output[id.x]=vec4(0xffffffffu);return;}
    let terminal=primary_terminals[1u+3u*id.x+1u];
    if terminal.w!=0u{stellar_consumer_output[id.x]=vec4(0u);return;}
    let outgoing=bitcast<vec3<f32>>(terminal.xyz);
    let inertial=normalize(vec3(dot(outgoing,settings.icrf0.xyz),dot(outgoing,settings.icrf1.xyz),dot(outgoing,settings.icrf2.xyz)));
    let pixel=vec2<f32>(vec2<u32>(id.x%settings.control.x,id.x/settings.control.x));
    let color=requested_catalogue_stars(inertial,pixel)*settings.environment.z;
    stellar_consumer_output[id.x]=bitcast<vec4<u32>>(vec4(color,1.0));
}
