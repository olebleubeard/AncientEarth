// Original MIT. Unused canonical scene-map enclosure, not registered.
// This certifies the REAL canonical map at an exact stored dense polynomial
// parameter. It does not claim the floating nc_position evaluation is exact.
// Endpoint enclosures + canonical P'' bound enclose a whole curve's chord
// tube; root/plane predicates consume these intervals directly.
//
// Host inputs: outward component intervals for the canonical f64 eye-frame
// polar axis, canonical WGS84 e², and canonical-minus-split source/eye height.
// Axis intervals must enclose a true unit axis in the same f64 frame as scene
// packing. This helper does not certify that host fact or scene transforms.
// Numerical ODE, dense/contact, packed source and frame errors are additional
// explicit allocations; no default allowance is supplied here.
//
// inverseSqrt has2ULP allowance (WGSL2026-10-21 §15.7.4.1). Four representable
// neighbors cover that allowance across binades; division retains the existing
// eight-neighbor helper. Every interior arithmetic operation is observable
// through interval widening. Tiny external inputs retain the FTZ allowance.
struct NcMapVec { x:vec2<f32>,y:vec2<f32>,z:vec2<f32> };
struct NcMapParameters {
    axis:NcMapVec,eccentricity:vec2<f32>,
    anchor_residual:vec2<f32>,eye_residual:vec2<f32>,
};
struct NcMapPoint { position:NcMapVec,velocity:NcMapVec,valid:u32 };
fn nc_m_invalid()->NcMapVec{
    return NcMapVec(nc_interval_invalid(),nc_interval_invalid(),nc_interval_invalid());
}
fn nc_m_valid(a:NcMapVec)->bool{
    return nc_interval_valid(a.x)&&nc_interval_valid(a.y)&&nc_interval_valid(a.z);
}
fn nc_m_exact(a:vec3<f32>)->NcMapVec{
    return NcMapVec(nc_interval_exact(a.x),nc_interval_exact(a.y),nc_interval_exact(a.z));
}
fn nc_m_up()->NcMapVec{return NcMapVec(nc_interval_zero(),vec2(1.0),nc_interval_zero());}
fn nc_m_add(a:NcMapVec,b:NcMapVec)->NcMapVec{
    return NcMapVec(nc_f_add(a.x,b.x),nc_f_add(a.y,b.y),nc_f_add(a.z,b.z));
}
fn nc_m_sub(a:NcMapVec,b:NcMapVec)->NcMapVec{
    return NcMapVec(nc_f_sub(a.x,b.x),nc_f_sub(a.y,b.y),nc_f_sub(a.z,b.z));
}
fn nc_m_scale(a:NcMapVec,b:vec2<f32>)->NcMapVec{
    return NcMapVec(nc_f_mul(a.x,b),nc_f_mul(a.y,b),nc_f_mul(a.z,b));
}
fn nc_m_dot(a:NcMapVec,b:NcMapVec)->vec2<f32>{
    return nc_f_add(nc_f_add(nc_f_mul(a.x,b.x),nc_f_mul(a.y,b.y)),nc_f_mul(a.z,b.z));
}
fn nc_m_rsqrt(a:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!(a.x>=NC_I_MIN_NORMAL && a.y<=1.0e20){return nc_interval_invalid();}
    let lower=inverseSqrt(a.y);let upper=inverseSqrt(a.x);
    if !(lower>0.0 && upper>0.0)||!nc_finite(lower)||!nc_finite(upper){return nc_interval_invalid();}
    return vec2(nc_interval_down_steps(lower,4u),nc_interval_up_steps(upper,4u));
}
fn nc_m_square(a:vec2<f32>)->vec2<f32>{
    let product=nc_f_mul(a,a);
    if !nc_interval_valid(product){return nc_interval_invalid();}
    return vec2(max(0.0,product.x),product.y);
}
fn nc_m_poly(start:vec3<f32>,c2:vec3<f32>,c3:vec3<f32>,c4:vec3<f32>,c5:vec3<f32>,theta:vec2<f32>)->NcMapVec{
    let complement=nc_f_sub(vec2(1.0),theta);
    return nc_m_add(nc_m_exact(start),nc_m_scale(nc_m_add(nc_m_exact(c2),nc_m_scale(
        nc_m_add(nc_m_exact(c3),nc_m_scale(nc_m_add(nc_m_exact(c4),nc_m_scale(nc_m_exact(c5),complement)),theta)),complement)),theta));
}
fn nc_m_poly_derivative(c2:vec3<f32>,c3:vec3<f32>,c4:vec3<f32>,c5:vec3<f32>,theta:vec2<f32>)->NcMapVec{
    let t2=nc_f_mul(theta,theta);let t3=nc_f_mul(t2,theta);
    let b3=nc_f_sub(vec2(1.0),nc_f_mul(vec2(2.0),theta));
    let b4=nc_f_sub(nc_f_mul(vec2(2.0),theta),nc_f_mul(vec2(3.0),t2));
    let b5=nc_f_add(nc_f_sub(nc_f_mul(vec2(2.0),theta),nc_f_mul(vec2(6.0),t2)),nc_f_mul(vec2(4.0),t3));
    return nc_m_add(nc_m_add(nc_m_exact(c2),nc_m_scale(nc_m_exact(c3),b3)),nc_m_add(nc_m_scale(nc_m_exact(c4),b4),nc_m_scale(nc_m_exact(c5),b5)));
}
fn nc_map_curve_point(curve:NcSceneCurve,theta:vec2<f32>,parameters:NcMapParameters)->NcMapPoint{
    var failure=NcMapPoint(nc_m_invalid(),nc_m_invalid(),0u);
    if curve.valid==0u||!nc_interval_valid(theta)||!(theta.x>=0.0 && theta.y<=curve.theta_end)||!nc_m_valid(parameters.axis)
        ||!nc_interval_valid(parameters.eccentricity)||parameters.eccentricity.x<0.0||parameters.eccentricity.y>0.007
        ||!nc_interval_valid(parameters.anchor_residual)||!nc_interval_valid(parameters.eye_residual){return failure;}
    let c=curve.physical;let t=theta;
    let q=nc_m_poly(curve.start.q,c.q2,c.q3,c.q4,c.q5,t);
    let dq=nc_m_poly_derivative(c.q2,c.q3,c.q4,c.q5,t);
    if !nc_m_valid(q)||!nc_m_valid(dq){return failure;}
    let q_squared=nc_f_add(nc_m_square(q.y),nc_m_square(q.z));
    let inverse_scale=nc_m_rsqrt(nc_f_add(vec2(1.0),q_squared));
    // n-n0=(a/s,-(a²+b²)/(s(s+1)),-b/s). Rationalizing the
    // middle component preserves near-eye precision without subtracting1.
    let delta_y=nc_f_sub(nc_interval_zero(),nc_f_div(nc_f_mul(q_squared,nc_f_mul(inverse_scale,inverse_scale)),nc_f_add(vec2(1.0),inverse_scale)));
    let dn=NcMapVec(nc_f_mul(q.y,inverse_scale),delta_y,nc_f_sub(nc_interval_zero(),nc_f_mul(q.z,inverse_scale)));
    let n=nc_m_add(nc_m_up(),dn);let axis=parameters.axis;let e=parameters.eccentricity;
    let z0=axis.y;let dz=nc_m_dot(axis,dn);let z=nc_f_add(z0,dz);
    let d0=nc_f_sub(vec2(1.0),nc_f_mul(e,nc_m_square(z0)));
    let d=nc_f_sub(vec2(1.0),nc_f_mul(e,nc_m_square(z)));
    let r0=nc_m_rsqrt(d0);let r=nc_m_rsqrt(d);
    let radius=nc_f_mul(vec2(6378137.0),r);
    let product=nc_f_mul(r,r0);
    let dr=nc_f_div(nc_f_mul(nc_f_mul(nc_f_mul(nc_f_mul(vec2(6378137.0),e),dz),nc_f_add(z,z0)),nc_f_mul(product,product)),nc_f_add(r,r0));
    let h=nc_f_add(nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),q.x),parameters.anchor_residual);
    let dh=nc_f_add(nc_f_add(nc_f_add(nc_f_sub(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_eye_height.x)),nc_f_sub(nc_interval_exact(nc_anchor.y),nc_interval_exact(nc_eye_height.y))),q.x),nc_f_sub(parameters.anchor_residual,parameters.eye_residual));
    let tangent_delta=nc_m_sub(dn,nc_m_scale(axis,nc_f_mul(e,dz)));
    let initial=nc_m_sub(nc_m_up(),nc_m_scale(axis,nc_f_mul(e,z0)));
    let p=nc_m_add(nc_m_add(nc_m_scale(tangent_delta,radius),nc_m_scale(initial,dr)),nc_m_add(nc_m_scale(dn,h),nc_m_scale(nc_m_up(),dh)));
    // Canonical derivative dP/dtheta, with no assumption that the stored raw
    // ray direction is the derivative of the represented dense position.
    let w=NcMapVec(dq.y,nc_interval_zero(),nc_f_sub(nc_interval_zero(),dq.z));
    let nprime=nc_m_scale(nc_m_sub(w,nc_m_scale(n,nc_m_dot(n,w))),inverse_scale);
    let meridian=nc_m_sub(axis,nc_m_scale(n,z));
    let coefficient=nc_f_mul(nc_f_div(nc_f_mul(radius,e),d),nc_m_dot(nprime,axis));
    let velocity=nc_m_sub(nc_m_add(nc_m_scale(n,dq.x),nc_m_scale(nprime,nc_f_add(radius,h))),nc_m_scale(meridian,coefficient));
    if !nc_m_valid(p)||!nc_m_valid(velocity){return failure;}
    return NcMapPoint(p,velocity,1u);
}
fn nc_map_point_radius(point:NcMapPoint,computed:vec3<f32>)->vec2<f32>{
    if point.valid==0u||!nc_finite3(computed){return nc_interval_invalid();}
    let difference=nc_m_sub(point.position,nc_m_exact(computed));
    if !nc_m_valid(difference){return nc_interval_invalid();}
    // L1 is an upper bound on Euclidean distance, with outward sum.
    let radius=nc_f_add(nc_f_add(vec2(nc_interval_abs_upper(difference.x)),vec2(nc_interval_abs_upper(difference.y))),vec2(nc_interval_abs_upper(difference.z)));
    if !nc_interval_valid(radius){return nc_interval_invalid();}
    return vec2(0.0,radius.y);
}
