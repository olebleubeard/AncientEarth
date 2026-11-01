// Original MIT. Unused diagnostic one-sided version of nh_state_per_height.
// Requires the reviewed nh_b_* helpers from optical_height_bounds_fast.wgsl.
// The original metric remains the single fallback; no endpoint charge or
// acceptance allocation changes. This returns an upper bound, never a sampled
// or fitted replacement for the complete curve's physical derivative metric.
//
// Domain: finite coefficients<=1e8, |step| in[1e-9,1e8], 0<path_upper<=1.1e10,
// |u0_i|<=2, bounded start/anchor, |computed H0|<=1e8, norm_lower>.25.
// Before norm admission Q<9e8 and U<1.3e9. Afterwards U<2, radius<2.1e8,
// span*norm_lower>2.49e-10, physical<2e26, direction<2e20,
// and path/optical<9e17. Every division denominator is positive normal within
// [2^-126,2^126], and all intermediate magnitudes stay far below3e37.
// All positive sums/products round upward by one representable neighbor;
// norm subtraction and the denominator product round downward. Quotients use
// eight upward neighbors, retaining the original WGSL/FTZ enclosure contract.
//
// The dense derivative basis magnitudes are <=1 on[0,1]. Thus Q/span bounds
// chart speed, U/span bounds raw direction speed, and max|u0_i|-U bounds raw
// normalizing norm. The original physical/path/optical/2L-direction formulas
// below are preserved as sufficient upper bounds, including all four dense
// coefficients and both geometric-path and optical-excess contributions.

fn nh_try_state_per_height_fast(start:NcState,curve:NhCurve)->vec2<f32>{
    let c=curve.data;let span=abs(curve.height_step);
    if c.valid!=1u || !nh_b_small3(c.q2)||!nh_b_small3(c.q3)||!nh_b_small3(c.q4)||!nh_b_small3(c.q5)
        ||!nh_b_small3(c.u2)||!nh_b_small3(c.u3)||!nh_b_small3(c.u4)||!nh_b_small3(c.u5)
        ||!all(abs(c.optical)<=vec4(1.0e8))||!nh_b_small3(start.q)
        ||!all(abs(start.u)<=vec3(2.0))||!all(abs(nc_anchor)<=vec2(1.0e8))
        ||!(span>=1.0e-9 && span<=1.0e8)
        ||!(curve.path_upper>0.0 && curve.path_upper<=1.1e10){return nc_interval_invalid();}
    let initial_height=nc_height(start.q);
    if !(abs(initial_height)<=1.0e8){return nc_interval_invalid();}
    let chart_sum=nh_b_add(nh_b_add(nh_b_l1_2(c.q2.yz),nh_b_l1_2(c.q3.yz)),nh_b_add(nh_b_l1_2(c.q4.yz),nh_b_l1_2(c.q5.yz)));
    let direction_sum=nh_b_add(nh_b_add(nh_b_l1_3(c.u2),nh_b_l1_3(c.u3)),nh_b_add(nh_b_l1_3(c.u4),nh_b_l1_3(c.u5)));
    let norm_lower=nc_interval_down(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))-direction_sum);
    if !(norm_lower>0.25){return nc_interval_invalid();}
    let radius=nh_b_add(6399594.0,nh_b_add(nh_b_abs(initial_height),span));
    let chart_speed=nc_interval_up_steps(chart_sum/span,8u);
    let physical=nh_b_add(1.0,nh_b_mul(radius,chart_speed));
    let direction_numerator=nh_b_mul(nh_b_mul(2.0,curve.path_upper),direction_sum);
    let direction_denominator=nc_interval_down(span*norm_lower);
    // This positive normal denominator follows from the admitted span/norm.
    // Keep the explicit sign guard before issuing the quotient nevertheless.
    if !(direction_denominator>0.0){return nc_interval_invalid();}
    let direction=nc_interval_up_steps(direction_numerator/direction_denominator,8u);
    let path_sum=nh_b_add(nh_b_add(nh_b_abs(c.q2.x),nh_b_abs(c.q3.x)),nh_b_add(nh_b_abs(c.q4.x),nh_b_abs(c.q5.x)));
    let excess_sum=nh_b_add(nh_b_add(nh_b_abs(c.optical.x),nh_b_abs(c.optical.y)),nh_b_add(nh_b_abs(c.optical.z),nh_b_abs(c.optical.w)));
    let path=nc_interval_up_steps(path_sum/span,8u);
    let optical=nc_interval_up_steps(nh_b_add(path_sum,excess_sum)/span,8u);
    return vec2(0.0,max(max(physical,direction),max(path,optical)));
}

fn nh_state_per_height_fast(start:NcState,curve:NhCurve)->vec2<f32>{
    let candidate=nh_try_state_per_height_fast(start,curve);
    if nc_interval_valid(candidate){return candidate;}
    return nh_state_per_height(start,curve);
}
