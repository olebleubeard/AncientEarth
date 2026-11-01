// Original MIT. UNUSED shared-contact-call variant of canonical scene queries.
// Terrain precedes markers, with the same face order and predicates. The packet owns every
// source/frame enclosure; no display-band proximity is promoted to a hit.
@group(0) @binding(1) var<storage,read> optical_geometry:array<vec4<f32>>;
@group(0) @binding(2) var<storage,read> optical_frames:array<vec4<f32>>;
@group(0) @binding(5) var<storage,read> nc_certificates:array<vec4<f32>>;
var<private> nc_scene_anchor_residual:vec2<f32>;
var<private> nc_scene_normal:vec3<f32>;
var<private> nc_scene_material:u32;
var<private> nc_scene_owner:u32;
var<private> nc_scene_visits:u32;
struct NcSceneEvent{kind:u32,theta:f32,lower:f32,upper:f32,normal:vec3<f32>,material:u32,owner:u32};
fn nc_scene_none()->NcSceneEvent{return NcSceneEvent(0u,0.0,0.0,0.0,vec3(0.0),0u,0u);}
fn nc_scene_uncertain()->NcSceneEvent{return NcSceneEvent(2u,0.0,0.0,0.0,vec3(0.0),0u,0u);}
fn nc_scene_vector(address:u32)->NcMapVec{
    let a=nc_certificates[address].xyz;let b=nc_certificates[address+1u].xyz;
    return NcMapVec(vec2(a.x,b.x),vec2(a.y,b.y),vec2(a.z,b.z));
}
fn nc_scene_parameters()->NcMapParameters{
    let a=nc_certificates[2];let b=nc_certificates[3];
    return NcMapParameters(NcMapVec(vec2(a.x,b.x),vec2(a.y,b.y),vec2(a.z,b.z)),vec2(a.w,b.w),nc_scene_anchor_residual,nc_certificates[4].xy);
}
fn nc_scene_face(address:u32)->NcFaceFrame{
    return NcFaceFrame(nc_m_exact(vec3(1.0,0.0,0.0)),nc_m_exact(vec3(0.0,1.0,0.0)),nc_m_exact(vec3(0.0,0.0,1.0)),
        nc_scene_vector(address),nc_scene_vector(address+2u),nc_scene_vector(address+4u));
}
fn nc_scene_patch_vector(base:u32,v:NcMapVec)->NcMapVec{
    return NcMapVec(nc_m_dot(nc_scene_vector(base+3u),v),nc_m_dot(nc_scene_vector(base+5u),v),nc_m_dot(nc_scene_vector(base+7u),v));
}
fn nc_scene_patch_point(base:u32,p:NcMapVec)->NcMapVec{return nc_m_add(nc_scene_vector(base+1u),nc_scene_patch_vector(base,p));}
fn nc_scene_pad(p:NcMapVec,r:f32)->NcMapVec{
    if !(r>=0.0)||!nc_finite(r){return nc_m_invalid();}let e=vec2(-r,r);return nc_m_add(p,NcMapVec(e,e,e));
}
fn nc_scene_hull(a:NcMapVec,b:NcMapVec)->NcMapVec{
    if !nc_m_valid(a)||!nc_m_valid(b){return nc_m_invalid();}
    return NcMapVec(vec2(min(a.x.x,b.x.x),max(a.x.y,b.x.y)),vec2(min(a.y.x,b.y.x),max(a.y.y,b.y.y)),vec2(min(a.z.x,b.z.x),max(a.z.y,b.z.y)));
}
fn nc_scene_box_misses(p:NcMapVec,lo:vec3<f32>,hi:vec3<f32>,padding:f32)->bool{
    if !nc_m_valid(p)||!nc_finite(padding)||padding<0.0{return false;}
    let e=vec2(-padding,padding);let x=nc_f_add(vec2(lo.x,hi.x),e);let y=nc_f_add(vec2(lo.y,hi.y),e);let z=nc_f_add(vec2(lo.z,hi.z),e);
    if !nc_interval_valid(x)||!nc_interval_valid(y)||!nc_interval_valid(z){return false;}
    return p.x.y<x.x||p.x.x>x.y||p.y.y<y.x||p.y.x>y.y||p.z.y<z.x||p.z.x>z.y;
}
fn nc_scene_midpoint(v:NcMapVec)->vec3<f32>{return vec3(v.x.x+(v.x.y-v.x.x)*0.5,v.y.x+(v.y.y-v.y.x)*0.5,v.z.x+(v.z.y-v.z.x)*0.5);}
fn nc_scene_vector_radius(v:NcMapVec,p:vec3<f32>)->vec2<f32>{return nc_face_l1(nc_m_sub(v,nc_m_exact(p)));}
fn nc_scene_path_interval(curve:NcSceneCurve,theta:vec2<f32>)->vec2<f32>{
    let c=curve.path;let z=nc_f_sub(vec2(1.0),theta);
    return nc_f_mul(theta,nc_f_add(vec2(c.x),nc_f_mul(z,nc_f_add(vec2(c.y),nc_f_mul(theta,nc_f_add(vec2(c.z),nc_f_mul(z,vec2(c.w))))))));
}
fn nc_scene_direction_interval(curve:NcSceneCurve,theta:vec2<f32>)->NcMapVec{
    let c=curve.physical;let raw=nc_m_poly(curve.start.u,c.u2,c.u3,c.u4,c.u5,theta);
    let squared=nc_f_add(nc_f_add(nc_m_square(raw.x),nc_m_square(raw.y)),nc_m_square(raw.z));
    return nc_m_scale(raw,nc_m_rsqrt(squared));
}
fn nc_scene_contact_charge(curve:NcSceneCurve,parameters:NcMapParameters,theta:f32,enclosure:vec2<f32>)->vec2<f32>{
    let at=nc_map_curve_point(curve,enclosure,parameters);if at.valid==0u{return nc_interval_invalid();}
    let position=nc_position(nc_scene_q(curve,theta));let direction=nc_scene_direction(curve,theta);let path=nc_scene_path(curve,theta);
    if !nc_finite3(position)||!nc_finite3(direction)||!nc_finite(path){return nc_interval_invalid();}
    let pe=nc_map_point_radius(at,position);
    let de=nc_scene_vector_radius(nc_scene_direction_interval(curve,enclosure),direction);
    let path_interval=nc_scene_path_interval(curve,enclosure);
    let se=nc_f_sub(path_interval,vec2(path));let length_bound=nc_scene_path_interval(curve,vec2(curve.theta_end));
    let angular=nc_f_mul(de,vec2(nc_interval_abs_upper(length_bound)));
    if !nc_interval_valid(pe)||!nc_interval_valid(de)||!nc_interval_valid(se)||!nc_interval_valid(length_bound)||!nc_interval_valid(angular){return nc_interval_invalid();}
    return nc_f_add(vec2(max(pe.y,max(angular.y,nc_interval_abs_upper(se)))),vec2(curve.numerical_error));
}
fn nc_scene_combine(a:NcSceneEvent,b:NcSceneEvent)->NcSceneEvent{
    // An unresolved candidate can contain the first contact. It is not erased
    // merely because another face of the same material/owner resolved.
    if a.kind==2u||b.kind==2u{return nc_scene_uncertain();}
    if a.kind==0u{return b;}if b.kind==0u{return a;}
    if a.upper<b.lower{return a;}if b.upper<a.lower{return b;}
    return nc_scene_uncertain();
}
fn nc_scene_face_event_cached(curve:NcSceneCurve,parameters:NcMapParameters,address:u32,material:u32,owner:u32,width:f32,tolerance:f32,context:NcFaceContext)->NcSceneEvent{
    let face=nc_scene_face(address);let contact=nc_face_contact_cached(curve,parameters,face,width,context);
    if contact.kind==0u{return nc_scene_none();}if contact.kind==2u{return nc_scene_uncertain();}
    let charge=nc_scene_contact_charge(curve,parameters,contact.theta,vec2(contact.lower,contact.upper));
    if !nc_interval_valid(charge)||charge.y>tolerance{return nc_scene_uncertain();}
    let n=nc_scene_midpoint(nc_face_cross(face.b,face.c));let norm2=dot(n,n);
    if !nc_finite3(n)||!nc_finite(norm2)||!(norm2>1.17549435e-38){return nc_scene_uncertain();}
    return NcSceneEvent(1u,contact.theta,contact.lower,contact.upper,n*inverseSqrt(norm2),material,owner);
}
// Canonical normal-line coverage. Strict interior proves covered; an interval
// edge/parallel case remains uncertain while other faces may still prove it.
fn nc_scene_line_face(point:NcMapVec,normal:NcMapVec,address:u32)->u32{
    let face=nc_scene_face(address);let p=nc_m_add(face.shift,point);let n=nc_face_cross(face.b,face.c);
    let denominator=nc_m_dot(n,normal);if !nc_interval_valid(denominator)||nc_interval_sign(denominator)==0{return 2u;}
    let t=nc_f_div(nc_f_sub(nc_interval_zero(),nc_m_dot(n,p)),denominator);
    if !nc_interval_valid(t){return 2u;}if t.y< -10001.0||t.x>101000.0{return 0u;}
    if t.x< -10001.0||t.y>101000.0{return 2u;}
    let hit=nc_m_add(p,nc_m_scale(normal,t));let e0=nc_face_edge(face.b,hit,n);
    let e1=nc_face_edge(nc_m_sub(face.c,face.b),nc_m_sub(hit,face.b),n);
    let e2=nc_face_edge(nc_m_scale(face.c,vec2(-1.0)),nc_m_sub(hit,face.c),n);
    if !nc_interval_valid(e0)||!nc_interval_valid(e1)||!nc_interval_valid(e2){return 2u;}
    if e0.y<0.0||e1.y<0.0||e2.y<0.0{return 0u;}
    if e0.x>0.0&&e1.x>0.0&&e2.x>0.0{return 1u;}return 2u;
}
fn nc_scene_coverage(curve:NcSceneCurve,parameters:NcMapParameters,theta:vec2<f32>)->u32{
    let at=nc_map_curve_point(curve,theta,parameters);if at.valid==0u{return 2u;}
    let c=curve.physical;let q=nc_m_poly(curve.start.q,c.q2,c.q3,c.q4,c.q5,theta);
    let squared=nc_f_add(vec2(1.0),nc_f_add(nc_m_square(q.y),nc_m_square(q.z)));
    let normal=nc_m_scale(NcMapVec(q.y,vec2(1.0),vec2(-q.z.y,-q.z.x)),nc_m_rsqrt(squared));
    let point=nc_scene_pad(at.position,curve.numerical_error);if !nc_m_valid(normal)||!nc_m_valid(point){return 2u;}
    let a=nc_m_add(point,nc_m_scale(normal,vec2(-10001.0)));let b=nc_m_add(point,nc_m_scale(normal,vec2(101000.0)));
    let count=bitcast<u32>(nc_certificates[0].x);let first=bitcast<u32>(nc_certificates[0].w);var uncertain=false;
    for(var region=0u;region<count;region+=1u){
        let base=first+region*9u;let box=nc_scene_hull(nc_scene_patch_point(base,a),nc_scene_patch_point(base,b));
        let header=nc_certificates[base];var node=bitcast<u32>(header.y);let stop=bitcast<u32>(header.z);
        loop{if node>=stop{break;}nc_scene_visits+=1u;if nc_scene_visits>65536u{return 2u;}
            let lo=optical_geometry[node];let hi=optical_geometry[node+1u];let next=bitcast<u32>(lo.w);let leaf=bitcast<u32>(hi.w);
            if nc_scene_box_misses(box,lo.xyz,hi.xyz,header.x){node=next;continue;}
            if (leaf&0x80000000u)==0u{node+=2u;continue;}
            let address=leaf&0x00ffffffu;let size=((leaf>>24u)&7u)+1u;
            for(var j=0u;j<size;j+=1u){let value=nc_scene_line_face(point,normal,bitcast<u32>(optical_geometry[address+3u*j].w));if value==1u{return 1u;}if value==2u{uncertain=true;}}
            node=next;
        }
    }
    return select(0u,2u,uncertain);
}
fn nc_scene_height(curve:NcSceneCurve,theta:vec2<f32>,parameters:NcMapParameters)->vec2<f32>{
    let c=curve.physical;let q=nc_m_poly(curve.start.q,c.q2,c.q3,c.q4,c.q5,theta);
    return nc_f_add(nc_f_add(nc_f_add(vec2(nc_anchor.x),vec2(nc_anchor.y)),q.x),parameters.anchor_residual);
}
fn nc_scene_h0(curve:NcSceneCurve,parameters:NcMapParameters,width:f32,tolerance:f32)->NcSceneEvent{
    let c=curve.physical;let before=nc_scene_height(curve,vec2(0.0),parameters);let after=nc_scene_height(curve,vec2(curve.theta_end),parameters);
    let a=nc_interval_sign(before);let b=nc_interval_sign(after);
    let accel=nc_f_add(nc_f_add(nc_f_mul(vec2(2.0),vec2(abs(c.q3.x))),nc_f_mul(vec2(4.0),vec2(abs(c.q4.x)))),nc_f_mul(vec2(2.0),vec2(abs(c.q5.x))));
    let derivative0=nc_f_add(vec2(c.q2.x),vec2(c.q3.x));let bend=nc_f_mul(accel,vec2(curve.theta_end));
    let derivative=nc_f_add(derivative0,vec2(-bend.y,bend.y));
    if !nc_interval_valid(before)||!nc_interval_valid(after)||!nc_interval_valid(accel)||!nc_interval_valid(derivative){return nc_scene_uncertain();}
    if a!=0&&a==b{
        if nc_interval_sign(derivative)!=0{return nc_scene_none();}
        let sag=nc_f_mul(nc_f_mul(accel,nc_f_mul(vec2(curve.theta_end),vec2(curve.theta_end))),vec2(0.125));
        let margin=select(min(-before.y,-after.y),min(before.x,after.x),a>0);
        if nc_interval_valid(sag)&&margin>sag.y{return nc_scene_none();}return nc_scene_uncertain();
    }
    if a==0||b==0||nc_interval_sign(derivative)!=b{return nc_scene_uncertain();}
    var enclosure=vec2(0.0,curve.theta_end);
    for(var iteration=0u;iteration<32u;iteration+=1u){
        let mid=enclosure.x+(enclosure.y-enclosure.x)*0.5;
        let remaining=nc_f_sub(vec2(enclosure.y),vec2(enclosure.x));
        if !nc_interval_valid(remaining){return nc_scene_uncertain();}
        // A sufficient coverage proof over the entire current root enclosure
        // needs no source snap or physical state mutation.
        if remaining.y<=width{
            let coverage=nc_scene_coverage(curve,parameters,enclosure);
            if coverage==1u{return nc_scene_none();}if coverage==2u{return nc_scene_uncertain();}
            let charge=nc_scene_contact_charge(curve,parameters,mid,enclosure);if !nc_interval_valid(charge)||charge.y>tolerance{return nc_scene_uncertain();}
            return NcSceneEvent(1u,mid,enclosure.x,enclosure.y,nc_normal(nc_scene_q(curve,mid)),0u,0u);
        }
        let correction=nc_f_div(nc_scene_height(curve,vec2(mid),parameters),derivative);let proposed=nc_f_sub(vec2(mid),correction);
        let next=vec2(max(enclosure.x,proposed.x),min(enclosure.y,proposed.y));
        if !nc_interval_valid(proposed)||!nc_interval_valid(next)||all(next==enclosure){return nc_scene_uncertain();}enclosure=next;
    }return nc_scene_uncertain();
}
fn nc_scene_query_cached(curve:NcSceneCurve,tolerance:f32)->NcSceneEvent{
    if curve.valid==0u||!(tolerance>0.0){return nc_scene_uncertain();}
    let parameters=nc_scene_parameters();
    if nc_scene_canonical_above_ceiling(curve,parameters.anchor_residual,nc_certificates[4].z){return nc_scene_none();}
    let start=nc_map_curve_point(curve,vec2(0.0),parameters);let end=nc_map_curve_point(curve,vec2(curve.theta_end),parameters);
    let acceleration=nc_scene_acceleration_upper(curve,nc_interval_abs_upper(parameters.anchor_residual));
    let sag=nc_f_mul(nc_f_mul(acceleration,nc_f_mul(vec2(curve.theta_end),vec2(curve.theta_end))),vec2(0.125));
    let padding=nc_f_add(sag,vec2(curve.numerical_error));
    if start.valid==0u||end.valid==0u||!nc_interval_valid(padding){return nc_scene_uncertain();}
    let box=nc_scene_pad(nc_scene_hull(start.position,end.position),padding.y);
    let speed=nc_root_speed(curve.start,curve.physical,1.0);
    let width_interval=nc_f_div(nc_f_mul(vec2(tolerance),vec2(0.01)),vec2(max(1.0,speed.y)));
    if !nc_interval_valid(speed)||!nc_interval_valid(width_interval)||!(width_interval.x>0.0){return nc_scene_uncertain();}
    let width=min(1.0,width_interval.x);var result=nc_scene_h0(curve,parameters,width,tolerance);
    if result.kind==2u{return result;}
    let context=NcFaceContext(start,end,acceleration);
    let count=bitcast<u32>(nc_certificates[0].x);let first=bitcast<u32>(nc_certificates[0].w);
    let marker_count=bitcast<u32>(nc_certificates[0].y);let marker_first=bitcast<u32>(nc_certificates[1].x);
    // The common face loop consumes the same terrain leaf batches followed
    // by the same fourteen-face marker batches. Only their addressing differs.
    for(var region=0u;region<count+marker_count;region+=1u){
        let is_marker=region>=count;
        var transformed=box;var header=vec4(0.0);var node=0u;var stop=1u;var marker_address=0u;
        if !is_marker{
            let base=first+region*9u;transformed=nc_scene_patch_point(base,box);header=nc_certificates[base];
            node=bitcast<u32>(header.y);stop=bitcast<u32>(header.z);
        }else{
            let marker=region-count;let base=marker_first+marker*3u;header=nc_certificates[base];
            if nc_scene_box_misses(box,nc_certificates[base+1u].xyz,nc_certificates[base+2u].xyz,0.0){continue;}
            marker_address=bitcast<u32>(header.x);
        }
        loop{
            if node>=stop{break;}
            var address=marker_address;var size=14u;
            if !is_marker{
                nc_scene_visits+=1u;if nc_scene_visits>65536u{return nc_scene_uncertain();}
                let lo=optical_geometry[node];let hi=optical_geometry[node+1u];let next=bitcast<u32>(lo.w);let leaf=bitcast<u32>(hi.w);
                if nc_scene_box_misses(transformed,lo.xyz,hi.xyz,header.x){node=next;continue;}
                if (leaf&0x80000000u)==0u{node+=2u;continue;}
                address=leaf&0x00ffffffu;size=((leaf>>24u)&7u)+1u;node=next;
            }else{node=stop;}
            for(var j=0u;j<size;j+=1u){
                var face_address=0u;var material=0u;var owner=region;
                if is_marker{face_address=address+6u*j;material=select(1u,2u,j>=12u);owner=region-count;}
                else{face_address=bitcast<u32>(optical_geometry[address+3u*j].w);}
                result=nc_scene_combine(result,nc_scene_face_event_cached(curve,parameters,face_address,material,owner,width,tolerance,context));
                if result.kind==2u{return result;}
            }
        }
    }
    return result;
}
fn nc_scene_terminal(curve:NcSceneCurve,event:NcSceneEvent,path:f32,attempt:u32)->NcResult{
    nc_scene_normal=event.normal;nc_scene_material=event.material;nc_scene_owner=event.owner;
    return NcResult(nc_position(nc_scene_q(curve,event.theta)),nc_scene_direction(curve,event.theta),path+nc_scene_path(curve,event.theta),1u,attempt,nc_root_count,0u);
}
