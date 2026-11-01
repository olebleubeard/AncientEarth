// Original MIT. Unregistered bridge to physical scene queries. This is a
// representation adapter, not a replacement for finite contact certification.
// nc_anchor/nc_axis/eye must remain those of the accepted trial until all
// scene/h0/source ordering decisions on this curve have completed.
struct NcSceneCurve {
    start:NcState,physical:NcCurve,path:vec4<f32>,
    theta_end:f32,numerical_error:f32,valid:u32,
};
fn nc_scene_invalid()->NcSceneCurve{
    return NcSceneCurve(NcState(vec3(0.0),vec3(0.0)),nc_invalid(),vec4(0.0),0.0,0.0,0u);
}
fn nc_scene_from_height(start:NcState,curve:NhCurve,source_charge:f32)->NcSceneCurve{
    let c=curve.data;
    let error=nc_f_add(nc_interval_exact(c.error),nc_interval_exact(source_charge));
    if c.valid==0u||!nc_interval_valid(error)||source_charge<0.0||!nc_finite(curve.height_step){return nc_scene_invalid();}
    // The physical height is affine. The original q.x quartic remains solely
    // the geometric-path map. Direction and optical coefficients are retained.
    let physical=NcCurve(vec3(curve.height_step,c.q2.yz),vec3(0.0,c.q3.yz),vec3(0.0,c.q4.yz),vec3(0.0,c.q5.yz),c.u2,c.u3,c.u4,c.u5,c.optical,c.error,c.valid);
    return NcSceneCurve(start,physical,vec4(c.q2.x,c.q3.x,c.q4.x,c.q5.x),1.0,error.y,1u);
}
fn nc_scene_from_path(start:NcState,curve:NcCurve,step:f32,theta_end:f32,source_charge:f32)->NcSceneCurve{
    let error=nc_f_add(nc_interval_exact(curve.error),nc_interval_exact(source_charge));
    if curve.valid==0u||!nc_interval_valid(error)||source_charge<0.0||!nc_finite(step)||!(step>0.0)||!(theta_end>0.0 && theta_end<=1.0){return nc_scene_invalid();}
    // Preserve the original full dense curve when a source root clips it.
    // Re-scaling its coefficients would describe a different rounded curve.
    return NcSceneCurve(start,curve,vec4(step,0.0,0.0,0.0),theta_end,error.y,1u);
}
fn nc_scene_delta(curve:NcSceneCurve,theta:f32)->vec3<f32>{
    return nc_position_delta(curve.start.q,nc_curve_delta_q(curve.physical,theta));
}
fn nc_scene_q(curve:NcSceneCurve,theta:f32)->vec3<f32>{
    return nc_curve_q(curve.start,curve.physical,theta);
}
fn nc_scene_velocity(curve:NcSceneCurve,theta:f32)->vec3<f32>{
    // This is dP/dtheta of the canonical chart map, NOT interpolated ray u.
    return nc_position_derivative(nc_scene_q(curve,theta),nc_curve_dq(curve.physical,theta,1.0));
}
fn nc_scene_path(curve:NcSceneCurve,theta:f32)->f32{
    let c=curve.path;
    return theta*(c.x+(1.0-theta)*(c.y+theta*(c.z+(1.0-theta)*c.w)));
}
fn nc_scene_direction(curve:NcSceneCurve,theta:f32)->vec3<f32>{
    let u=nc_curve_u(curve.start,curve.physical,theta);
    let squared=dot(u,u);
    if curve.valid==0u||!(theta>=0.0 && theta<=curve.theta_end)||!nc_finite3(u)||!(squared>=1.17549435e-38 && squared<1.0e8){return vec3(0.0);}
    return u*inverseSqrt(squared);
}
// Whole-original-curve bound is valid for every clipped prefix too. Every
// arithmetic operation here uses the independently reviewed outward helpers.
fn nc_scene_height_lower(curve:NcSceneCurve)->vec2<f32>{
    let c=curve.physical;
    let h0=nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),nc_interval_exact(curve.start.q.x));
    let c1=nc_f_add(nc_interval_exact(c.q2.x),nc_interval_exact(c.q3.x));
    let c2=nc_f_add(nc_f_sub(nc_interval_exact(c.q4.x),nc_interval_exact(c.q3.x)),nc_interval_exact(c.q5.x));
    let c3=nc_f_sub(nc_f_sub(nc_interval_zero(),nc_interval_exact(c.q4.x)),nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(c.q5.x)));
    let c4=nc_interval_exact(c.q5.x);
    let b1=nc_f_add(h0,nc_f_mul(c1,nc_interval_exact(0.25)));
    let b2=nc_f_add(nc_f_add(h0,nc_f_mul(c1,nc_interval_exact(0.5))),nc_f_div(c2,nc_interval_exact(6.0)));
    let b3=nc_f_add(nc_f_add(nc_f_add(h0,nc_f_mul(c1,nc_interval_exact(0.75))),nc_f_mul(c2,nc_interval_exact(0.5))),nc_f_mul(c3,nc_interval_exact(0.25)));
    let b4=nc_f_add(nc_f_add(nc_f_add(nc_f_add(h0,c1),c2),c3),c4);
    if !nc_interval_valid(h0)||!nc_interval_valid(b1)||!nc_interval_valid(b2)||!nc_interval_valid(b3)||!nc_interval_valid(b4){return nc_interval_invalid();}
    let lower=min(min(h0.x,b1.x),min(min(b2.x,b3.x),b4.x));
    return vec2(lower);
}
fn nc_scene_above_ceiling(curve:NcSceneCurve,canonical_ceiling:f32,map_allowance:f32,scene_allowance:f32)->bool{
    // All allowances must be supplied by a separately reviewed finite packet
    // contract. There is deliberately no default or heuristic map allowance.
    if curve.valid==0u||!nc_finite(canonical_ceiling)||!nc_finite(map_allowance)||!nc_finite(scene_allowance)||map_allowance<0.0||scene_allowance<0.0{return false;}
    let height=nc_scene_height_lower(curve);
    let padding=nc_f_add(nc_interval_exact(curve.numerical_error),nc_f_add(nc_interval_exact(map_allowance),nc_interval_exact(scene_allowance)));
    let lower=nc_f_sub(height,padding);
    return nc_interval_valid(height)&&nc_interval_valid(padding)&&nc_interval_valid(lower)&&lower.x>max(0.0,canonical_ceiling);
}

// Preferred sufficient skip when both the curve and vertex ceiling refer to
// the canonical WGS84 normal map. The host must certify its true unit polar
// axis and the supported normal tube; canonical H is then this coordinate.
// No Cartesian evaluation or packed scene-frame arithmetic participates.
// The local numerical estimate is retained as the existing allowance, not
// promoted here to a theorem bounding accumulated global trajectory error.
fn nc_scene_canonical_above_ceiling(curve:NcSceneCurve,anchor_residual:vec2<f32>,canonical_ceiling:f32)->bool{
    if curve.valid==0u||!nc_interval_valid(anchor_residual)||!nc_finite(canonical_ceiling){return false;}
    let height=nc_f_add(nc_scene_height_lower(curve),anchor_residual);
    let lower=nc_f_sub(height,nc_interval_exact(curve.numerical_error));
    return nc_interval_valid(height)&&nc_interval_valid(lower)&&lower.x>max(0.0,canonical_ceiling);
}

// Upper bound for |d²P/dtheta²| on the entire original canonical curve.
// Quantized axis/map/frame evaluation errors are separate caller allowances;
// this is not a bound on the derivative of the floating-point evaluation.
// height_allowance encloses any canonical-versus-encoded source-height error.
fn nc_scene_acceleration_upper(curve:NcSceneCurve,height_allowance:f32)->vec2<f32>{
    if curve.valid==0u||!nc_finite(height_allowance)||height_allowance<0.0{return nc_interval_invalid();}
    let c=curve.physical;
    let q1=nc_f_add(nc_f_add(nc_l1(c.q2.yz),nc_l1(c.q3.yz)),nc_f_add(nc_l1(c.q4.yz),nc_l1(c.q5.yz)));
    let q2=nc_f_add(nc_f_add(nc_f_mul(nc_interval_exact(2.0),nc_l1(c.q3.yz)),nc_f_mul(nc_interval_exact(4.0),nc_l1(c.q4.yz))),nc_f_mul(nc_interval_exact(2.0),nc_l1(c.q5.yz)));
    let h1=nc_f_add(nc_f_add(nc_interval_exact(abs(c.q2.x)),nc_interval_exact(abs(c.q3.x))),nc_f_add(nc_interval_exact(abs(c.q4.x)),nc_interval_exact(abs(c.q5.x))));
    let h2=nc_f_add(nc_f_add(nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(abs(c.q3.x))),nc_f_mul(nc_interval_exact(4.0),nc_interval_exact(abs(c.q4.x)))),nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(abs(c.q5.x))));
    let h0=nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),nc_interval_exact(curve.start.q.x));
    if !nc_interval_valid(q1)||!nc_interval_valid(q2)||!nc_interval_valid(h1)||!nc_interval_valid(h2)||!nc_interval_valid(h0){return nc_interval_invalid();}
    let hmax=nc_f_add(nc_f_add(nc_interval_exact(nc_interval_abs_upper(h0)),h1),nc_interval_exact(height_allowance));
    let q1_squared=nc_f_mul(q1,q1);
    let normal_second=nc_f_add(q2,nc_f_mul(nc_interval_exact(3.0),q1_squared));
    // These encoded constants round outward from the canonical WGS84 bounds:
    // Nmax<6400000, Cmax=a*e2/(1-e2)^(3/2)<44000,
    // 3+3e2/(1-e2)<3.025 (the nearest f32 3.025 is above that value).
    let radius=nc_f_add(nc_f_add(nc_interval_exact(6400000.0),hmax),nc_interval_exact(44000.0));
    let coupling=nc_f_mul(nc_f_mul(nc_interval_exact(2.0),h1),q1);
    let last=nc_f_mul(nc_f_mul(nc_interval_exact(44000.0),nc_interval_exact(3.025)),q1_squared);
    let result=nc_f_add(nc_f_add(h2,coupling),nc_f_add(nc_f_mul(radius,normal_second),last));
    if !nc_interval_valid(hmax)||!nc_interval_valid(q1_squared)||!nc_interval_valid(normal_second)||!nc_interval_valid(radius)||!nc_interval_valid(coupling)||!nc_interval_valid(last)||!nc_interval_valid(result){return nc_interval_invalid();}
    return vec2(0.0,result.y);
}
