// Original MIT. Sufficient-only f64 dense-curve -> existing f32 interval bridge.
// Requires unchanged optical_normal_interval + optical_cartesian_clearance.
// Stored f64 operands are enclosed, never rounded into f32 singleton inputs.
fn cf64_interval(value:f64)->vec2<f32>{
    if !(abs(value)<f64(3.0e37)){return nc_interval_invalid();}
    if abs(value)<f64(NC_I_MIN_NORMAL){return vec2(-NC_I_TINY_BOUND,NC_I_TINY_BOUND);}
    let rounded=f32(value);
    if !(abs(rounded)>=NC_I_MIN_NORMAL && abs(rounded)<3.0e37){return nc_interval_invalid();}
    if f64(rounded)==value{return vec2(rounded);}
    let result=vec2(nc_interval_down(rounded),nc_interval_up(rounded));
    // Widening finite normal f32 endpoints to f64 is exact. This observed
    // comparison makes containment explicit, independent of cast direction.
    if !nc_interval_valid(result)||f64(result.x)>value||f64(result.y)<value{return nc_interval_invalid();}
    return result;
}
fn cf64_vector(value:vec3<f64>)->CsI3{
    return CsI3(cf64_interval(value.x),cf64_interval(value.y),cf64_interval(value.z));
}
fn cf64_curve_scene_clear(position:vec3<f64>,delta:vec3<f64>,curve_r3:vec3<f64>,curve_r4:vec3<f64>,curve_r5:vec3<f64>,accepted_distance:f64,full_step:f64,error:f64,allocation:f64,tolerance:f64)->bool{
    if !(accepted_distance>=f64(0.0)&&accepted_distance<=full_step&&full_step>=f64(0.000001)&&full_step<=f64(100000.0)
        &&error>=f64(0.0)&&allocation>=f64(0.0)&&tolerance>f64(0.0)&&tolerance<f64(3.0e37)&&settings.scene.x>=0.0){return false;}
    let distance=cf64_interval(accepted_distance);let full=cf64_interval(full_step);
    let alpha=nc_interval_div(distance,full);
    if !nc_interval_valid(alpha)||alpha.y>1.000002{return false;}
    // Include the actual native division as well as the outward real quotient.
    // The existing eight-f32-neighbor division padding also exceeds the native
    // f64 division roundoff allowance over this admitted finite normal domain.
    let observed_alpha=accepted_distance/full_step;
    if !(observed_alpha>=f64(0.0)&&observed_alpha<=f64(alpha.y)&&observed_alpha>=f64(alpha.x)){return false;}
    let a=vec2(max(0.0,alpha.x),alpha.y);let a2=nc_interval_mul(a,a);let a3=nc_interval_mul(a2,a);let a4=nc_interval_mul(a2,a2);
    let p=cf64_vector(position);let d=cf64_vector(delta);let r3=cf64_vector(curve_r3);let r4=cf64_vector(curve_r4);let r5=cf64_vector(curve_r5);
    if !cs_valid3(p)||!cs_valid3(d)||!cs_valid3(r3)||!cs_valid3(r4)||!cs_valid3(r5){return false;}
    let c1=cs_scale3(cs_add3(d,r3),a);let c2=cs_scale3(cs_add3(cs_sub3(r4,r3),r5),a2);
    let c3=cs_scale3(cs_sub3(cs_scale3(r4,vec2(-1.0)),cs_scale3(r5,vec2(2.0))),a3);let c4=cs_scale3(r5,a4);
    let sixth=nc_interval_div(vec2(1.0),vec2(6.0));
    let b1=cs_add3(p,cs_scale3(c1,vec2(0.25)));
    let b2=cs_add3(p,cs_add3(cs_scale3(c1,vec2(0.5)),cs_scale3(c2,sixth)));
    let b3=cs_add3(p,cs_add3(cs_add3(cs_scale3(c1,vec2(0.75)),cs_scale3(c2,vec2(0.5))),cs_scale3(c3,vec2(0.25))));
    let b4=cs_add3(p,cs_add3(cs_add3(c1,c2),cs_add3(c3,c4)));
    let controls=CsControls(p,b1,b2,b3,b4);
    let rsum=nc_interval_add(nc_interval_add(nc_interval_mul(vec2(2.0),cs_l1(r3)),nc_interval_mul(vec2(4.0),cs_l1(r4))),nc_interval_mul(vec2(2.0),cs_l1(r5)));
    let polynomial_sag=nc_interval_mul(nc_interval_mul(rsum,a2),vec2(0.125));
    let physical_sag=nc_interval_mul(nc_interval_mul(nc_interval_mul(nc_interval_exact(settings.scene.x),distance),distance),vec2(0.125));
    if !nc_interval_valid(polynomial_sag)||!nc_interval_valid(physical_sag){return false;}
    let displacement_magnitude=nc_interval_add(cs_l1(d),nc_interval_add(cs_l1(r3),nc_interval_add(cs_l1(r4),cs_l1(r5))));
    let magnitude=nc_interval_add(cs_l1(p),displacement_magnitude);
    if !nc_interval_valid(magnitude)||magnitude.y>1e9{return false;}
    // Preserve the reviewed larger f32 rounding band. At alpha<=1.000002 it
    // dominates the <=12 basic f64 dense-position operations and FTZ terms;
    // operand conversion and parameter uncertainty are already enclosed above.
    let evaluation=nc_interval_mul(vec2(0.00000762939453125),vec2(max(1.0,magnitude.y)));
    let speed=nc_interval_div(nc_interval_mul(vec2(1.00001),displacement_magnitude),full);
    let total_error=nc_interval_add(cf64_interval(error),cf64_interval(allocation));
    if !nc_interval_valid(speed)||!nc_interval_valid(total_error)||total_error.y<0.0||!(f64(total_error.y)<=tolerance){return false;}
    let positioned_error=nc_interval_mul(vec2(0.0,total_error.y),vec2(max(1.0,speed.y)));
    let tube=nc_interval_add(nc_interval_add(positioned_error,vec2(0.0,max(polynomial_sag.y,physical_sag.y))),evaluation);
    return cs_all_roots_clear(controls,tube);
}
