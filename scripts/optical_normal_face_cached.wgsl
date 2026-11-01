// Original MIT. Unused cached equivalent of the strict canonical face predicate.
// Context values are the unchanged query endpoint maps and acceleration; no
// approximation or weakened predicate is introduced by retaining them.
// Requires optical_normal_map_interval.wgsl and reviewed interval primitives.
// All frame/vertex inputs enclose the canonical f64 source geometry. The
// caller must construct those enclosures; this file supplies no guessed band.
// A result of 2 means refine the interval or preserve explicit uncertainty.
struct NcFaceContext {start:NcMapPoint,finish:NcMapPoint,acceleration:vec2<f32>};
struct NcFaceFrame {
    row0:NcMapVec,row1:NcMapVec,row2:NcMapVec,
    shift:NcMapVec, // canonical eye minus vertex A, in this face's patch frame
    b:NcMapVec,c:NcMapVec, // canonical B-A and C-A, same frame
};
struct NcFaceContact { kind:u32,theta:f32,lower:f32,upper:f32 };
fn nc_face_uncertain()->NcFaceContact{return NcFaceContact(2u,0.0,0.0,0.0);}
fn nc_face_miss()->NcFaceContact{return NcFaceContact(0u,0.0,0.0,0.0);}
fn nc_face_cross(a:NcMapVec,b:NcMapVec)->NcMapVec{
    return NcMapVec(nc_f_sub(nc_f_mul(a.y,b.z),nc_f_mul(a.z,b.y)),
        nc_f_sub(nc_f_mul(a.z,b.x),nc_f_mul(a.x,b.z)),
        nc_f_sub(nc_f_mul(a.x,b.y),nc_f_mul(a.y,b.x)));
}
fn nc_face_l1(a:NcMapVec)->vec2<f32>{
    if !nc_m_valid(a){return nc_interval_invalid();}
    return nc_f_add(nc_f_add(vec2(nc_interval_abs_upper(a.x)),vec2(nc_interval_abs_upper(a.y))),vec2(nc_interval_abs_upper(a.z)));
}
fn nc_face_transform(frame:NcFaceFrame,v:NcMapVec)->NcMapVec{
    return NcMapVec(nc_m_dot(frame.row0,v),nc_m_dot(frame.row1,v),nc_m_dot(frame.row2,v));
}
fn nc_face_point(frame:NcFaceFrame,point:NcMapPoint,padding:f32)->NcMapVec{
    if point.valid==0u||!nc_finite(padding)||padding<0.0{return nc_m_invalid();}
    // Numerical curve error is expressed in the original eye's Cartesian
    // frame. Transform its component enclosure together with the point.
    let error=vec2(-padding,padding);
    let padded=nc_m_add(point.position,NcMapVec(error,error,error));
    return nc_m_add(frame.shift,nc_face_transform(frame,padded));
}
fn nc_face_edge(edge:NcMapVec,point:NcMapVec,normal:NcMapVec)->vec2<f32>{
    return nc_m_dot(nc_face_cross(edge,point),normal);
}
fn nc_face_chord_outside(before:vec2<f32>,after:vec2<f32>,sag:vec2<f32>)->bool{
    if !nc_interval_valid(before)||!nc_interval_valid(after)||!nc_interval_valid(sag){return false;}
    return max(before.y,after.y)< -sag.y;
}

// `parameter_width` is derived by the caller from the unchanged complete
// state/path contact allocation. It is a theta width, not a height band.
fn nc_face_contact_cached(curve:NcSceneCurve,parameters:NcMapParameters,frame:NcFaceFrame,parameter_width:f32,context:NcFaceContext)->NcFaceContact{
    if curve.valid==0u||!(parameter_width>0.0 && parameter_width<=1.0)
        ||!nc_m_valid(frame.row0)||!nc_m_valid(frame.row1)||!nc_m_valid(frame.row2)
        ||!nc_m_valid(frame.shift)||!nc_m_valid(frame.b)||!nc_m_valid(frame.c){return nc_face_uncertain();}
    let normal=nc_face_cross(frame.b,frame.c);
    let normal_squared=nc_m_dot(normal,normal);
    if !nc_m_valid(normal)||!nc_interval_valid(normal_squared)||!(normal_squared.x>0.0){return nc_face_uncertain();}
    let start=context.start;let finish=context.finish;
    let p=nc_face_point(frame,start,curve.numerical_error);
    let q=nc_face_point(frame,finish,curve.numerical_error);
    if !nc_m_valid(p)||!nc_m_valid(q){return nc_face_uncertain();}
    let before=nc_m_dot(normal,p);let after=nc_m_dot(normal,q);
    let acceleration=context.acceleration;
    let frame_norm=nc_f_add(nc_f_add(nc_face_l1(frame.row0),nc_face_l1(frame.row1)),nc_face_l1(frame.row2));
    let plane_acceleration=nc_f_mul(nc_f_mul(nc_face_l1(normal),frame_norm),acceleration);
    let duration=vec2(curve.theta_end);
    let sag=nc_f_mul(nc_f_mul(plane_acceleration,nc_f_mul(duration,duration)),vec2(0.125));
    if !nc_interval_valid(before)||!nc_interval_valid(after)||!nc_interval_valid(plane_acceleration)||!nc_interval_valid(sag){return nc_face_uncertain();}
    // Plane/edge chord tubes are only necessary candidate conditions. An
    // inconclusive same-sign plane interval is never promoted to a hit.
    if min(before.x,after.x)>sag.y||max(before.y,after.y)< -sag.y{return nc_face_miss();}
    let edge0=frame.b;let edge1=nc_m_sub(frame.c,frame.b);let edge2=nc_m_scale(frame.c,vec2(-1.0));
    let edge_sag0=nc_f_mul(nc_face_l1(edge0),sag);
    let edge_sag1=nc_f_mul(nc_face_l1(edge1),sag);
    let edge_sag2=nc_f_mul(nc_face_l1(edge2),sag);
    if nc_face_chord_outside(nc_face_edge(edge0,p,normal),nc_face_edge(edge0,q,normal),edge_sag0)
        ||nc_face_chord_outside(nc_face_edge(edge1,nc_m_sub(p,frame.b),normal),nc_face_edge(edge1,nc_m_sub(q,frame.b),normal),edge_sag1)
        ||nc_face_chord_outside(nc_face_edge(edge2,nc_m_sub(p,frame.c),normal),nc_face_edge(edge2,nc_m_sub(q,frame.c),normal),edge_sag2){return nc_face_miss();}
    let before_sign=nc_interval_sign(before);let after_sign=nc_interval_sign(after);
    if before_sign==0||after_sign==0||before_sign==after_sign{return nc_face_uncertain();}
    let initial_derivative=nc_m_dot(normal,nc_face_transform(frame,start.velocity));
    let derivative_change=nc_f_mul(plane_acceleration,duration);
    let derivative=nc_f_add(initial_derivative,vec2(-derivative_change.y,derivative_change.y));
    if !nc_interval_valid(derivative)||nc_interval_sign(derivative)!=after_sign{return nc_face_uncertain();}
    // The original strict sign bracket and transverse bound prove existence
    // and uniqueness. Interval Newton only intersects a certified enclosure;
    // a rounded zero residual is never an acceptance criterion.
    var enclosure=vec2(0.0,curve.theta_end);
    for(var iteration=0u;iteration<32u;iteration+=1u){
        let width=nc_f_sub(vec2(enclosure.y),vec2(enclosure.x));
        if !nc_interval_valid(width){return nc_face_uncertain();}
        let narrow=width.y<=parameter_width;
        let middle=enclosure.x+(enclosure.y-enclosure.x)*0.5;
        if !narrow && !(middle>enclosure.x && middle<enclosure.y){return nc_face_uncertain();}
        let evaluation_theta=select(vec2(middle),enclosure,narrow);
        let at=nc_map_curve_point(curve,evaluation_theta,parameters);
        let point=nc_face_point(frame,at,curve.numerical_error);
        if !nc_m_valid(point){return nc_face_uncertain();}
        if narrow{
            let e0=nc_face_edge(edge0,point,normal);
            let e1=nc_face_edge(edge1,nc_m_sub(point,frame.b),normal);
            let e2=nc_face_edge(edge2,nc_m_sub(point,frame.c),normal);
            if !nc_interval_valid(e0)||!nc_interval_valid(e1)||!nc_interval_valid(e2){return nc_face_uncertain();}
            if e0.y<0.0||e1.y<0.0||e2.y<0.0{return nc_face_miss();}
            if !(e0.x>0.0 && e1.x>0.0 && e2.x>0.0){return nc_face_uncertain();}
            let theta=enclosure.x+(enclosure.y-enclosure.x)*0.5;
            if !(theta>=enclosure.x && theta<=enclosure.y){return nc_face_uncertain();}
            return NcFaceContact(1u,theta,enclosure.x,enclosure.y);
        }
        let value=nc_m_dot(normal,point);
        let correction=nc_f_div(value,derivative);
        let proposed=nc_f_sub(vec2(middle),correction);
        if !nc_interval_valid(proposed){return nc_face_uncertain();}
        let next=vec2(max(enclosure.x,proposed.x),min(enclosure.y,proposed.y));
        if !nc_interval_valid(next)||all(next==enclosure){return nc_face_uncertain();}
        enclosure=next;
    }
    return nc_face_uncertain();
}
