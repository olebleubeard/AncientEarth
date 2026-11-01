// Original MIT. Isolated normal-coordinate diagnostic, not registered.
// The bounded entry point removes repeated overflow/validity tests from the
// scalar height polynomial only. Endpoint operations, tiny-value treatment,
// Sterbenz identities and outward rounding match optical_normal_interval.wgsl.
// This is not a different polynomial, error tolerance or root acceptance rule.
fn nc_rf_exact(x:f32)->vec2<f32>{
    if abs(x)<NC_I_MIN_NORMAL{return vec2(-NC_I_TINY_BOUND,NC_I_TINY_BOUND);}
    return vec2(x);
}
fn nc_rf_add(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if all(a==vec2(0.0)){return b;}
    if all(b==vec2(0.0)){return a;}
    let lo=a.x+b.x;let hi=a.y+b.y;
    var lower=nc_interval_down(lo);var upper=nc_interval_up(hi);
    if nc_interval_sterbenz(a.x,-b.x,lo){lower=lo;}
    if nc_interval_sterbenz(a.y,-b.y,hi){upper=hi;}
    return vec2(lower,upper);
}
fn nc_rf_sub(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if all(b==vec2(0.0)){return a;}
    if a.x==a.y && b.x==b.y && a.x==b.x{return vec2(0.0);}
    let lo=a.x-b.y;let hi=a.y-b.x;
    var lower=nc_interval_down(lo);var upper=nc_interval_up(hi);
    if nc_interval_sterbenz(a.x,b.y,lo){lower=lo;}
    if nc_interval_sterbenz(a.y,b.x,hi){upper=hi;}
    return vec2(lower,upper);
}
fn nc_rf_mul(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if all(a==vec2(0.0))||all(b==vec2(0.0)){return vec2(0.0);}
    let p=vec4(a.x*b.x,a.x*b.y,a.y*b.x,a.y*b.y);
    let lo=min(min(p.x,p.y),min(p.z,p.w));let hi=max(max(p.x,p.y),max(p.z,p.w));
    return vec2(nc_interval_down(lo),nc_interval_up(hi));
}
fn nc_fast_residual(start:NcState,curve:NcCurve,parameter:f32,boundary_gap:vec3<f32>)->vec2<f32>{
    // Every admitted external magnitude is <=1e8; theta is in [0,1].
    // Even with outward steps the nested polynomial intermediates have
    // magnitude <1e9 (a conservative 1e10 bound suffices), and factors from
    // theta or 1-theta have magnitude <2. All products/additions are thus
    // finite normal-range operations or explicitly handled tiny operations.
    // Reject NaN/infinity through the ordered comparisons before any work.
    let coefficients=vec4(curve.q2.x,curve.q3.x,curve.q4.x,curve.q5.x);
    if !(parameter>=0.0 && parameter<=1.0)
        ||!all(abs(coefficients)<=vec4(1e8))
        ||!all(abs(boundary_gap)<=vec3(1e8))
        ||!(abs(start.q.x)<=1e8)||!(boundary_gap.z>=0.0){
        return nc_residual(start,curve,parameter,boundary_gap);
    }
    var theta=nc_rf_exact(parameter);if parameter==0.0{theta=vec2(0.0);}
    let z=nc_rf_sub(vec2(1.0),theta);
    let poly=nc_rf_add(nc_rf_exact(curve.q2.x),nc_rf_mul(z,nc_rf_add(nc_rf_exact(curve.q3.x),nc_rf_mul(theta,nc_rf_add(nc_rf_exact(curve.q4.x),nc_rf_mul(z,nc_rf_exact(curve.q5.x)))))));
    var high=nc_rf_exact(boundary_gap.x);if boundary_gap.x==0.0{high=vec2(0.0);}
    let low=nc_rf_add(nc_rf_exact(boundary_gap.y),vec2(-boundary_gap.z,boundary_gap.z));
    return nc_rf_add(nc_rf_add(nc_rf_add(high,nc_rf_exact(start.q.x)),nc_rf_mul(theta,poly)),low);
}
