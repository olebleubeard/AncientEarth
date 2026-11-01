// Original MIT. Unregistered selected-ray continuation ABI. All float payloads
// persist by bitcast; immutable field/scene inputs remain fixed across passes.
// 32 uvec4 per ray; one 20-uvec4 Cartesian piece per ray in a separate buffer.
@group(0) @binding(6) var<storage,read_write> ns_records:array<vec4<u32>>;
@group(0) @binding(7) var<storage,read_write> ns_pieces:array<vec4<u32>>;
@group(0) @binding(8) var<storage,read_write> ns_progress:array<u32>;
struct NsRecord {
    state:NcState,path:f32,proposed:f32,height_proposed:f32,
    owner:u32,attempt:u32,height_retries:u32,phase:u32,
    next:NcState,delta_path:f32,accepted_error:f32,attempted_step:f32,
    adaptive:f32,height_attempted:f32,height_path:f32,is_height:u32,crossing:i32,
    curve:NcSceneCurve,pieces:u32,piece_index:u32,query_count:u32,certificate_count:u32,
    tolerance:f32,event:NcSceneEvent,id:u32,reason:u32,
};
var<private> ns:NsRecord;
fn ns_read_f(base:u32,offset:u32)->vec4<f32>{return bitcast<vec4<f32>>(ns_records[base+offset]);}
fn ns_load(id:u32){
    let b=id*32u;let a=ns_read_f(b,0u);let u=ns_read_f(b,1u);
    ns.state=NcState(a.xyz,u.xyz);ns.path=a.w;ns.proposed=u.w;
    let axes=ns_read_f(b,2u);nc_axis=axes.xyz;ns.height_proposed=axes.w;
    let anchors=ns_read_f(b,3u);nc_eye_height=anchors.xy;nc_anchor=anchors.zw;
    let field=ns_read_f(b,4u);nc_index=field.xy;nc_scene_anchor_residual=field.zw;
    nc_lower_gap=ns_read_f(b,5u).xyz;nc_upper_gap=ns_read_f(b,6u).xyz;
    let ids=ns_records[b+7u];ns.owner=ids.x;ns.attempt=ids.y;ns.height_retries=ids.z;ns.phase=ids.w;
    let counts=ns_records[b+8u];nc_root_count=counts.x;nh_attempts=counts.y;nh_accepted=counts.z;nc_scene_visits=counts.w;
    let q=ns_read_f(b,9u);let v=ns_read_f(b,10u);ns.next=NcState(q.xyz,v.xyz);ns.delta_path=q.w;ns.accepted_error=v.w;
    let control=ns_read_f(b,11u);ns.attempted_step=control.x;ns.adaptive=control.y;ns.height_attempted=control.z;ns.height_path=control.w;
    let modes=ns_records[b+12u];ns.is_height=modes.x;ns.crossing=bitcast<i32>(modes.y);ns.id=modes.z;ns.reason=modes.w;
    let q2=ns_read_f(b,13u);let q3=ns_read_f(b,14u);let q4=ns_read_f(b,15u);let q5=ns_read_f(b,16u);
    let u2=ns_read_f(b,17u);let u3=ns_read_f(b,18u);let u4=ns_read_f(b,19u);let u5=ns_read_f(b,20u);
    let physical=NcCurve(q2.xyz,q3.xyz,q4.xyz,q5.xyz,u2.xyz,u3.xyz,u4.xyz,u5.xyz,ns_read_f(b,21u),q2.w,ns_records[b+16u].w);
    ns.curve=NcSceneCurve(ns.state,physical,vec4(u2.w,u3.w,u4.w,u5.w),q3.w,q4.w,ns_records[b+16u].w);
    let piece=ns_records[b+22u];ns.pieces=piece.x;ns.piece_index=piece.y;ns.query_count=piece.z;ns.certificate_count=piece.w;
    ns.tolerance=ns_read_f(b,23u).x;let event=ns_records[b+24u];let theta=ns_read_f(b,25u);
    ns.event=NcSceneEvent(event.x,theta.x,theta.y,theta.z,ns_read_f(b,26u).xyz,event.y,event.z);
}
fn ns_store(){
    let b=ns.id*32u;
    ns_records[b]=bitcast<vec4<u32>>(vec4(ns.state.q,ns.path));ns_records[b+1u]=bitcast<vec4<u32>>(vec4(ns.state.u,ns.proposed));
    ns_records[b+2u]=bitcast<vec4<u32>>(vec4(nc_axis,ns.height_proposed));ns_records[b+3u]=bitcast<vec4<u32>>(vec4(nc_eye_height,nc_anchor));
    ns_records[b+4u]=bitcast<vec4<u32>>(vec4(nc_index,nc_scene_anchor_residual));
    ns_records[b+5u]=bitcast<vec4<u32>>(vec4(nc_lower_gap,0.0));ns_records[b+6u]=bitcast<vec4<u32>>(vec4(nc_upper_gap,0.0));
    ns_records[b+7u]=vec4(ns.owner,ns.attempt,ns.height_retries,ns.phase);ns_records[b+8u]=vec4(nc_root_count,nh_attempts,nh_accepted,nc_scene_visits);
    ns_records[b+9u]=bitcast<vec4<u32>>(vec4(ns.next.q,ns.delta_path));ns_records[b+10u]=bitcast<vec4<u32>>(vec4(ns.next.u,ns.accepted_error));
    ns_records[b+11u]=bitcast<vec4<u32>>(vec4(ns.attempted_step,ns.adaptive,ns.height_attempted,ns.height_path));
    ns_records[b+12u]=vec4(ns.is_height,bitcast<u32>(ns.crossing),ns.id,ns.reason);
    let c=ns.curve.physical;
    ns_records[b+13u]=bitcast<vec4<u32>>(vec4(c.q2,c.error));ns_records[b+14u]=bitcast<vec4<u32>>(vec4(c.q3,ns.curve.theta_end));
    ns_records[b+15u]=bitcast<vec4<u32>>(vec4(c.q4,ns.curve.numerical_error));ns_records[b+16u]=vec4(bitcast<vec3<u32>>(c.q5),ns.curve.valid);
    ns_records[b+17u]=bitcast<vec4<u32>>(vec4(c.u2,ns.curve.path.x));ns_records[b+18u]=bitcast<vec4<u32>>(vec4(c.u3,ns.curve.path.y));
    ns_records[b+19u]=bitcast<vec4<u32>>(vec4(c.u4,ns.curve.path.z));ns_records[b+20u]=bitcast<vec4<u32>>(vec4(c.u5,ns.curve.path.w));
    ns_records[b+21u]=bitcast<vec4<u32>>(c.optical);ns_records[b+22u]=vec4(ns.pieces,ns.piece_index,ns.query_count,ns.certificate_count);
    ns_records[b+23u]=bitcast<vec4<u32>>(vec4(ns.tolerance,0.0,0.0,0.0));ns_records[b+24u]=vec4(ns.event.kind,ns.event.material,ns.event.owner,0u);
    ns_records[b+25u]=bitcast<vec4<u32>>(vec4(ns.event.theta,ns.event.lower,ns.event.upper,0.0));ns_records[b+26u]=bitcast<vec4<u32>>(vec4(ns.event.normal,0.0));
}
fn ns_write_vector(address:u32,value:NcMapVec){
    ns_pieces[address]=bitcast<vec4<u32>>(vec4(value.x.x,value.y.x,value.z.x,0.0));
    ns_pieces[address+1u]=bitcast<vec4<u32>>(vec4(value.x.y,value.y.y,value.z.y,0.0));
}
fn ns_read_vector(address:u32)->NcMapVec{
    let a=bitcast<vec4<f32>>(ns_pieces[address]);let b=bitcast<vec4<f32>>(ns_pieces[address+1u]);
    return NcMapVec(vec2(a.x,b.x),vec2(a.y,b.y),vec2(a.z,b.z));
}
fn ns_write_piece(piece:NcCartesianPiece){
    let b=ns.id*20u;ns_pieces[b]=bitcast<vec4<u32>>(piece.parameter);
    ns_pieces[b+1u]=bitcast<vec4<u32>>(vec4(piece.origin_hi,0.0));ns_pieces[b+2u]=bitcast<vec4<u32>>(vec4(piece.origin_lo,0.0));
    ns_write_vector(b+3u,piece.p0);ns_write_vector(b+5u,piece.p1);ns_write_vector(b+7u,piece.p2);ns_write_vector(b+9u,piece.p3);
    ns_write_vector(b+11u,piece.d0);ns_write_vector(b+13u,piece.d1);ns_write_vector(b+15u,piece.d2);
    ns_pieces[b+17u]=bitcast<vec4<u32>>(vec4(piece.position_remainder,0.0));ns_pieces[b+18u]=bitcast<vec4<u32>>(vec4(piece.derivative_remainder,0.0));
    ns_pieces[b+19u]=vec4(ns.id,ns.piece_index,ns.pieces,piece.valid);
}
fn ns_read_piece()->NcCartesianPiece{
    let b=ns.id*20u;
    return NcCartesianPiece(bitcast<vec4<f32>>(ns_pieces[b+1u]).xyz,bitcast<vec4<f32>>(ns_pieces[b+2u]).xyz,
        ns_read_vector(b+3u),ns_read_vector(b+5u),ns_read_vector(b+7u),ns_read_vector(b+9u),
        ns_read_vector(b+11u),ns_read_vector(b+13u),ns_read_vector(b+15u),
        bitcast<vec4<f32>>(ns_pieces[b+17u]).xyz,bitcast<vec4<f32>>(ns_pieces[b+18u]).xyz,bitcast<vec4<f32>>(ns_pieces[b]),ns_pieces[b+19u].w);
}
fn ns_publish(ray:NcResult){
    let index=1u+3u*ns.id;let detail=1u+3u*NS_COUNT+2u*ns.id;
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(nc_scene_normal),(nc_scene_material<<24u)|nc_scene_owner);
    terminals[detail]=vec4(ray.attempts,ray.roots,ray.reason,nc_scene_visits);ns.attempt=ray.attempts;ns.reason=ray.reason;ns.phase=4u;ns_store();
}
