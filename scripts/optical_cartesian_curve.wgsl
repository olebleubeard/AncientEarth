// Original MIT. Unused scalar consumer of a certified Cartesian Hermite
// piece. The producer owns the canonical endpoint/derivative enclosures,
// fourth-derivative remainder, split-origin packing and exact dyadic domain.
// This file neither fits a curve nor grants a contact/error allocation.
// Requires NcMapVec/NcMapPoint and the reviewed nc_m_*/nc_f_* primitives.
struct NcCartesianPiece {
    origin_hi:vec3<f32>,origin_lo:vec3<f32>,
    p0:NcMapVec,p1:NcMapVec,p2:NcMapVec,p3:NcMapVec,
    d0:NcMapVec,d1:NcMapVec,d2:NcMapVec,
    position_remainder:vec3<f32>,derivative_remainder:vec3<f32>,
    // theta0,theta1,exact power-of-two inverse span; w reserved.
    parameter:vec4<f32>,valid:u32,
};
fn nc_cartesian_invalid_point()->NcMapPoint {
    return NcMapPoint(nc_m_invalid(),nc_m_invalid(),0u);
}
fn nc_cartesian_lerp(a:NcMapVec,b:NcMapVec,t:vec2<f32>)->NcMapVec {
    // Difference form keeps arithmetic at the local-control scale. Every
    // subtraction/multiply/add is enclosed, including interval dependency.
    return nc_m_add(a,nc_m_scale(nc_m_sub(b,a),t));
}
fn nc_cartesian_pad(v:NcMapVec,error:vec3<f32>)->NcMapVec {
    if !nc_finite3(error)||any(error<vec3(0.0)){return nc_m_invalid();}
    return nc_m_add(v,NcMapVec(vec2(-error.x,error.x),vec2(-error.y,error.y),vec2(-error.z,error.z)));
}
fn nc_cartesian_local_point(piece:NcCartesianPiece,theta:vec2<f32>)->NcMapPoint {
    let range=piece.parameter.xy;
    if piece.valid==0u||!nc_interval_valid(theta)||!nc_interval_valid(range)
        ||!(range.x>=0.0&&range.y<=1.0&&range.y>range.x)
        ||theta.x<range.x||theta.y>range.y
        ||!nc_finite(piece.parameter.z)||!(piece.parameter.z>0.0)
        ||!nc_finite3(piece.origin_hi)||!nc_finite3(piece.origin_lo){return nc_cartesian_invalid_point();}
    var position=nc_m_invalid();var velocity=nc_m_invalid();
    if all(theta==vec2(range.x)) {
        // Hermite interpolation agrees with the endpoint and derivative
        // exactly. Their canonical/packing intervals remain in p0/d0.
        position=piece.p0;velocity=piece.d0;
    } else if all(theta==vec2(range.y)) {
        position=piece.p3;velocity=piece.d2;
    } else {
        let scaled=nc_f_mul(nc_f_sub(theta,vec2(range.x)),vec2(piece.parameter.z));
        if !nc_interval_valid(scaled){return nc_cartesian_invalid_point();}
        // The input domain proves the exact local parameter lies in[0,1].
        // Clipping only removes outward arithmetic excess at its endpoints.
        let t=vec2(max(0.0,scaled.x),min(1.0,scaled.y));
        if !nc_interval_valid(t){return nc_cartesian_invalid_point();}
        let a=nc_cartesian_lerp(piece.p0,piece.p1,t);
        let b=nc_cartesian_lerp(piece.p1,piece.p2,t);
        let c=nc_cartesian_lerp(piece.p2,piece.p3,t);
        let left=nc_cartesian_lerp(a,b,t);let right=nc_cartesian_lerp(b,c,t);
        position=nc_cartesian_pad(nc_cartesian_lerp(left,right,t),piece.position_remainder);
        let da=nc_cartesian_lerp(piece.d0,piece.d1,t);let db=nc_cartesian_lerp(piece.d1,piece.d2,t);
        velocity=nc_cartesian_pad(nc_cartesian_lerp(da,db,t),piece.derivative_remainder);
    }
    if !nc_m_valid(position)||!nc_m_valid(velocity){return nc_cartesian_invalid_point();}
    // Position is RELATIVE to the split origin; velocity already uses the
    // original theta, because the producer's derivative controls do so.
    return NcMapPoint(position,velocity,1u);
}
fn nc_cartesian_absolute_point(piece:NcCartesianPiece,local:NcMapPoint)->NcMapPoint {
    if local.valid==0u{return nc_cartesian_invalid_point();}
    // Suitable for conservative broadphase only. Narrow faces should fold
    // origin_hi then origin_lo into their canonical eye-minus-A interval
    // before adding local.position, preserving the useful cancellation.
    let position=nc_m_add(nc_m_add(nc_m_exact(piece.origin_hi),nc_m_exact(piece.origin_lo)),local.position);
    if !nc_m_valid(position){return nc_cartesian_invalid_point();}
    return NcMapPoint(position,local.velocity,1u);
}
fn nc_cartesian_relative_point(piece:NcCartesianPiece,local:NcMapPoint,eye_minus_vertex:NcMapVec)->NcMapVec {
    if local.valid==0u{return nc_m_invalid();}
    let shifted=nc_m_add(nc_m_add(eye_minus_vertex,nc_m_exact(piece.origin_hi)),nc_m_exact(piece.origin_lo));
    return nc_m_add(shifted,local.position);
}
