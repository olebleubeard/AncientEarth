// Original MIT. Diagnostic sufficient certificate for the unchanged Nh stages.
// No stage, field, tolerance, coefficient, or controller equation is replaced.
// An inconclusive admission/certificate uses the complete original nh_bounds.
//
// FINITE DOMAIN: every admitted stored coefficient/error <= 1e8; |u0_i|<=2,
// |chart0|_1<=1, |step| in [1e-9,1e8], and |computed H0|<=1e8.
// Anchor components are checked before computing H0. Positive upper sums use
// one representable outward neighbor after each basic operation; the tiny
// allowance is the existing 2*minimum-normal FTZ bound. Each bitcast widening
// is observable before the next arithmetic operation. No RN-only gamma bound
// is substituted. The positive quotient retains eight outward neighbors.
//
// Pre-certificate upper magnitudes (including ample rounding slack) are:
// Q<9e8, U<1.3e9, |u0|_1<7, vertical error<8e9. After v_lower>.01,
// path_upper<1.1e10; after m_lower>.25 and chart_bound<=1, radius<2.1e8,
// each Euclidean error norm<1.8e8, and final estimator<4e19. All are far below
// the existing 3e37 finite domain, with squared norm sums below 3.1e16.
// The original low_u/low_chart finite checks follow directly from admission:
// their components have magnitude <2.1e8, so these sums cannot overflow.
//
// Q and U bound the complete quartic chart/direction variation and the low/
// Hermite extension is charged exactly as in nh_bounds. Normal-chart Jacobian
// norm <=1 gives |v-v0|<=U+|u0|*Q; the existing 32u*|u0|_1 evaluation allowance
// remains. A lower normal norm is max_i|u0_i|-U-extra. Bounds are one-sided;
// unused negative endpoints and generic validity graphs are not evaluated.
//
// Positive path chronology has the sufficient test q2.x > |q3.x|+|q4.x|+
// |q5.x|. In the exact represented derivative, the three remaining basis
// factors are 1-2t, 2t-3t^2, and 2t-6t^2+4t^3, each with magnitude <=1 on
// [0,1]. A positive normal q2.x is exact as a stored input. Failure defers to
// the original, more general Bernstein test; it never certifies a rejection.
// This is a finite polynomial certificate, not a global ODE accuracy proof.

fn nh_b_abs(x:f32)->f32{
    return select(NC_I_TINY_BOUND,abs(x),abs(x)>=NC_I_MIN_NORMAL);
}
fn nh_b_add(a:f32,b:f32)->f32{return nc_interval_up(a+b);}
fn nh_b_mul(a:f32,b:f32)->f32{return nc_interval_up(a*b);}
fn nh_b_l1_2(x:vec2<f32>)->f32{return nh_b_add(nh_b_abs(x.x),nh_b_abs(x.y));}
fn nh_b_l1_3(x:vec3<f32>)->f32{return nh_b_add(nh_b_l1_2(x.xy),nh_b_abs(x.z));}
fn nh_b_small3(x:vec3<f32>)->bool{return all(abs(x)<=vec3(1.0e8));}

fn nh_try_bounds_fast(start:NcState,step:f32,stage:NhStage)->NhCurve{
    if stage.index!=8u || stage.data.valid!=1u{return nh_invalid();}
    let q2=stage.data.q2;let q3=stage.data.q3;let q4=stage.data.q4;let q5=stage.data.q5;
    let u2=stage.data.u2;let u3=stage.data.u3;let u4=stage.data.u4;let u5=stage.data.u5;
    let n2=stage.data.optical.x;let n3=stage.data.optical.y;let n4=stage.data.optical.z;let n5=stage.data.optical.w;
    let eq=stage.eq;let eu=stage.eu;let en=stage.en;
    if !nh_b_small3(q2)||!nh_b_small3(q3)||!nh_b_small3(q4)||!nh_b_small3(q5)
        ||!nh_b_small3(u2)||!nh_b_small3(u3)||!nh_b_small3(u4)||!nh_b_small3(u5)
        ||!nh_b_small3(eq)||!nh_b_small3(eu)||!(abs(en)<=1.0e8)
        ||!all(abs(stage.data.optical)<=vec4(1.0e8))||!nh_b_small3(start.q)
        ||!all(abs(start.u)<=vec3(2.0))||!all(abs(nc_anchor)<=vec2(1.0e8))
        ||!(abs(step)>=1.0e-9 && abs(step)<=1.0e8){return nh_invalid();}
    let initial_chart=nh_b_l1_2(start.q.yz);
    if !(initial_chart<=1.0)||!(abs(nc_height(start.q))<=1.0e8){return nh_invalid();}
    let chart_variation=nh_b_add(nh_b_add(nh_b_l1_2(q2.yz),nh_b_l1_2(q3.yz)),nh_b_add(nh_b_l1_2(q4.yz),nh_b_l1_2(q5.yz)));
    let direction_variation=nh_b_add(nh_b_add(nh_b_l1_3(u2),nh_b_l1_3(u3)),nh_b_add(nh_b_l1_3(u4),nh_b_l1_3(u5)));
    let initial_norm_upper=nh_b_l1_3(start.u);
    let vertical_error=nh_b_add(direction_variation,nh_b_mul(initial_norm_upper,chart_variation));
    let orientation=select(-1.0,1.0,step>0.0);
    let signed_vertical=orientation*dot(start.u,nc_normal(start.q));
    let vertical_rounding=nh_b_mul(0.0000019073486328125,initial_norm_upper);
    let vertical_lower=nc_interval_down(signed_vertical-nh_b_add(vertical_error,vertical_rounding));
    if !(vertical_lower>0.01){return nh_invalid();}
    let chart_extra=nh_b_add(nh_b_mul(nh_b_l1_2(q5.yz),0.0625),nh_b_l1_2(eq.yz));
    let chart_bound=nh_b_add(initial_chart,nh_b_add(chart_variation,chart_extra));
    if !(chart_bound<=1.0){return nh_invalid();}
    let direction_extra=nh_b_add(nh_b_mul(nh_b_l1_3(u5),0.0625),nh_b_l1_3(eu));
    let norm_lower=nc_interval_down(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))-nh_b_add(direction_variation,direction_extra));
    if !(norm_lower>0.25){return nh_invalid();}
    let path_sum=nh_b_add(nh_b_add(nh_b_abs(q2.x),nh_b_abs(q3.x)),nh_b_add(nh_b_abs(q4.x),nh_b_abs(q5.x)));
    // v_lower>.01 and |step|<=1e8 prove the WGSL quotient domain and no
    // overflow before issuing this division. Eight neighbors include binades.
    let path_upper=max(path_sum,nc_interval_up_steps(abs(step)/vertical_lower,8u));
    let chronology_rest=nh_b_add(nh_b_add(nh_b_abs(q3.x),nh_b_abs(q4.x)),nh_b_abs(q5.x));
    if !(q2.x>=NC_I_MIN_NORMAL && q2.x>chronology_rest){return nh_invalid();}
    // Deliberately retain the original embedded/interior estimator expressions.
    let radius=6399594.0+abs(nc_height(start.q))+abs(step);
    let embedded=max(max(radius*length(eq.yz),2.0*path_upper*length(eu)/norm_lower),abs(eq.x)+abs(en));
    let interior=max(max(radius*length(q5.yz),2.0*path_upper*length(u5)/norm_lower),abs(q5.x)+abs(n5))*0.0625;
    let error=embedded+interior;if !nc_finite(error){return nh_invalid();}
    return NhCurve(NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),error,1u),step,vertical_lower,path_upper);
}

// Keep one reference fallback graph. The private invalid result means only
// that the sufficient shortcut declined; callers still receive nh_bounds.
fn nh_bounds_fast(start:NcState,step:f32,stage:NhStage)->NhCurve{
    let candidate=nh_try_bounds_fast(start,step,stage);
    if candidate.data.valid==1u{return candidate;}
    return nh_bounds(start,step,stage);
}
