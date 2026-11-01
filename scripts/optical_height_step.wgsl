// Original MIT. Unregistered monotone-height diagnostic.
// The wrapped NcCurve stores (geometric path, chart a, chart b) in q2..q5.
// It MUST NOT be passed to the arclength controller's height-curve helpers.
struct NhDerivative { q:vec3<f32>,u:vec3<f32>,excess:f32,valid:u32 };
struct NhCurve { data:NcCurve,height_step:f32,vertical_lower:f32,path_upper:f32 };
fn nh_invalid()->NhCurve{return NhCurve(nc_invalid(),0.0,0.0,0.0);}
fn nh_rhs(state:NcState,orientation:f32)->NhDerivative{
    let invalid=NhDerivative(vec3(0.0),vec3(0.0),0.0,0u);
    let derivative=nc_rhs(state);if derivative.valid==0u{return invalid;}
    let vertical=derivative.q.x;
    // Conditioning policy only. A failed height trial returns to the original
    // arclength solver; it does not terminate or approximate a physical ray.
    if !(orientation*vertical>0.01)||!(abs(vertical)<1.0e8){return invalid;}
    let inverse_vertical=1.0/vertical;
    let q=vec3(inverse_vertical,derivative.q.yz*inverse_vertical);
    let u=derivative.u*inverse_vertical;
    let excess=derivative.refractivity*inverse_vertical;
    if !nc_finite3(q)||!nc_finite3(u)||!nc_finite(excess){return invalid;}
    return NhDerivative(q,u,excess,1u);
}
fn nh_curve_state(start:NcState,curve:NhCurve,theta:f32)->NcState{
    let c=curve.data;
    let chart=start.q.yz+theta*(c.q2.yz+(1.0-theta)*(c.q3.yz+theta*(c.q4.yz+(1.0-theta)*c.q5.yz)));
    return NcState(vec3(start.q.x+theta*curve.height_step,chart),nc_curve_u(start,c,theta));
}
fn nh_curve_path(curve:NhCurve,theta:f32)->f32{
    let c=curve.data;
    return theta*(c.q2.x+(1.0-theta)*(c.q3.x+theta*(c.q4.x+(1.0-theta)*c.q5.x)));
}
fn nh_curve_dq(curve:NhCurve,theta:f32)->vec3<f32>{
    let derivative=nc_curve_dq(curve.data,theta,1.0);
    return vec3(curve.height_step,derivative.yz);
}
fn nh_curve_path_derivative(curve:NhCurve,theta:f32)->f32{
    return nc_curve_dq(curve.data,theta,1.0).x;
}
// BEGIN GENERATED HEIGHT DP54 TRIAL
fn nh_trial(start:NcState,step:f32)->NhCurve{
    if !nc_finite(step)||!(abs(step)>0.000000000001){return nh_invalid();}
    if abs(start.q.y)+abs(start.q.z)>1.0||any(abs(start.u)>vec3(2.0)){return nh_invalid();}
    let orientation=select(-1.0,1.0,step>0.0);
    let k0=nh_rhs(start,orientation);if k0.valid==0u{return nh_invalid();}
    let stage_q1=vec3(start.q.x+(1.0/5.0)*step,start.q.yz+step*((1.0/5.0)*k0.q).yz);
    let stage_u1=start.u+step*((1.0/5.0)*k0.u);
    let k1=nh_rhs(NcState(stage_q1,stage_u1),orientation);if k1.valid==0u{return nh_invalid();}
    let stage_q2=vec3(start.q.x+(3.0/10.0)*step,start.q.yz+step*((3.0/40.0)*k0.q+(9.0/40.0)*k1.q).yz);
    let stage_u2=start.u+step*((3.0/40.0)*k0.u+(9.0/40.0)*k1.u);
    let k2=nh_rhs(NcState(stage_q2,stage_u2),orientation);if k2.valid==0u{return nh_invalid();}
    let stage_q3=vec3(start.q.x+(4.0/5.0)*step,start.q.yz+step*((44.0/45.0)*k0.q+(-56.0/15.0)*k1.q+(32.0/9.0)*k2.q).yz);
    let stage_u3=start.u+step*((44.0/45.0)*k0.u+(-56.0/15.0)*k1.u+(32.0/9.0)*k2.u);
    let k3=nh_rhs(NcState(stage_q3,stage_u3),orientation);if k3.valid==0u{return nh_invalid();}
    let stage_q4=vec3(start.q.x+(8.0/9.0)*step,start.q.yz+step*((19372.0/6561.0)*k0.q+(-25360.0/2187.0)*k1.q+(64448.0/6561.0)*k2.q+(-212.0/729.0)*k3.q).yz);
    let stage_u4=start.u+step*((19372.0/6561.0)*k0.u+(-25360.0/2187.0)*k1.u+(64448.0/6561.0)*k2.u+(-212.0/729.0)*k3.u);
    let k4=nh_rhs(NcState(stage_q4,stage_u4),orientation);if k4.valid==0u{return nh_invalid();}
    let stage_q5=vec3(start.q.x+(1.0)*step,start.q.yz+step*((9017.0/3168.0)*k0.q+(-355.0/33.0)*k1.q+(46732.0/5247.0)*k2.q+(49.0/176.0)*k3.q+(-5103.0/18656.0)*k4.q).yz);
    let stage_u5=start.u+step*((9017.0/3168.0)*k0.u+(-355.0/33.0)*k1.u+(46732.0/5247.0)*k2.u+(49.0/176.0)*k3.u+(-5103.0/18656.0)*k4.u);
    let k5=nh_rhs(NcState(stage_q5,stage_u5),orientation);if k5.valid==0u{return nh_invalid();}
    let q3=-step*((500.0/1113.0)*(k2.q-k0.q)+(125.0/192.0)*(k3.q-k0.q)+(-2187.0/6784.0)*(k4.q-k0.q)+(11.0/84.0)*(k5.q-k0.q));
    let q2=step*k0.q-q3;
    let u2=step*((35.0/384.0)*k0.u+(500.0/1113.0)*k2.u+(125.0/192.0)*k3.u+(-2187.0/6784.0)*k4.u+(11.0/84.0)*k5.u);
    let stage_q6=vec3(start.q.x+step,start.q.yz+q2.yz);let stage_u6=start.u+u2;
    let k6=nh_rhs(NcState(stage_q6,stage_u6),orientation);if k6.valid==0u{return nh_invalid();}
    let q4=step*(k0.q-k6.q)-2.0*q3;
    let q5=step*((87487479700.0/32700410799.0)*(k2.q-k0.q)+(-10690763975.0/1880347072.0)*(k3.q-k0.q)+(701980252875.0/199316789632.0)*(k4.q-k0.q)+(-1453857185.0/822651844.0)*(k5.q-k0.q)+(69997945.0/29380423.0)*(k6.q-k0.q));
    let u3=step*k0.u-u2;let u4=u2-step*k6.u-u3;
    let u5=step*((87487479700.0/32700410799.0)*(k2.u-k0.u)+(-10690763975.0/1880347072.0)*(k3.u-k0.u)+(701980252875.0/199316789632.0)*(k4.u-k0.u)+(-1453857185.0/822651844.0)*(k5.u-k0.u)+(69997945.0/29380423.0)*(k6.u-k0.u));
    let n3=-step*((500.0/1113.0)*(k2.excess-k0.excess)+(125.0/192.0)*(k3.excess-k0.excess)+(-2187.0/6784.0)*(k4.excess-k0.excess)+(11.0/84.0)*(k5.excess-k0.excess));
    let n2=step*k0.excess-n3;let n4=step*(k0.excess-k6.excess)-2.0*n3;
    let n5=step*((87487479700.0/32700410799.0)*(k2.excess-k0.excess)+(-10690763975.0/1880347072.0)*(k3.excess-k0.excess)+(701980252875.0/199316789632.0)*(k4.excess-k0.excess)+(-1453857185.0/822651844.0)*(k5.excess-k0.excess)+(69997945.0/29380423.0)*(k6.excess-k0.excess));
    let eq=step*((-71.0/16695.0)*(k2.q-k0.q)+(71.0/1920.0)*(k3.q-k0.q)+(-17253.0/339200.0)*(k4.q-k0.q)+(22.0/525.0)*(k5.q-k0.q)+(-1.0/40.0)*(k6.q-k0.q));
    let eu=step*((-71.0/16695.0)*(k2.u-k0.u)+(71.0/1920.0)*(k3.u-k0.u)+(-17253.0/339200.0)*(k4.u-k0.u)+(22.0/525.0)*(k5.u-k0.u)+(-1.0/40.0)*(k6.u-k0.u));
    let en=step*((-71.0/16695.0)*(k2.excess-k0.excess)+(71.0/1920.0)*(k3.excess-k0.excess)+(-17253.0/339200.0)*(k4.excess-k0.excess)+(22.0/525.0)*(k5.excess-k0.excess)+(-1.0/40.0)*(k6.excess-k0.excess));
    if !nc_finite3(q2)||!nc_finite3(q3)||!nc_finite3(q4)||!nc_finite3(q5)||!nc_finite3(u2)||!nc_finite3(u3)||!nc_finite3(u4)||!nc_finite3(u5)||!nc_finite3(eq)||!nc_finite3(eu)||!nc_finite(en)||!all(abs(vec4(n2,n3,n4,n5))<vec4(3.0e37)){return nh_invalid();}
    let chart_variation=nc_f_add(nc_f_add(nc_l1(q2.yz),nc_l1(q3.yz)),nc_f_add(nc_l1(q4.yz),nc_l1(q5.yz)));
    let direction_variation=nc_f_add(nc_f_add(nc_l1_3(u2),nc_l1_3(u3)),nc_f_add(nc_l1_3(u4),nc_l1_3(u5)));
    let initial_norm_upper=nc_l1_3(start.u);
    let vertical_error=nc_f_add(direction_variation,nc_f_mul(initial_norm_upper,chart_variation));
    let signed_vertical=orientation*dot(start.u,nc_normal(start.q));
    // For |a|+|b|<=1 and bounded finite u: the positive normal denominator,
    // explicit inverseSqrt, component product and dot contribute <16u*L1(u).
    // 32u (u=2^-24) also covers tiny/FTZ terms when |vertical|>.01.
    let vertical_rounding=nc_f_mul(nc_interval_exact(0.0000019073486328125),initial_norm_upper);
    let vertical_lower=nc_f_sub(nc_interval_exact(signed_vertical),nc_f_add(vertical_error,vertical_rounding)).x;
    if !nc_interval_valid(chart_variation)||!nc_interval_valid(direction_variation)||!nc_finite(vertical_lower)||!(vertical_lower>0.01){return nh_invalid();}
    let low_u=start.u+u2-eu;let low_chart=start.q.yz+q2.yz-eq.yz;
    let chart_extra=nc_f_add(nc_f_mul(nc_l1(q5.yz),nc_interval_exact(0.0625)),nc_l1(eq.yz));
    let chart_bound=nc_f_add(nc_l1(start.q.yz),nc_f_add(chart_variation,chart_extra));
    if !nc_interval_valid(chart_bound)||chart_bound.y>1.0||!nc_finite3(low_u)||!all(abs(low_chart)<vec2(3.0e37)){return nh_invalid();}
    let direction_extra=nc_f_add(nc_f_mul(nc_l1_3(u5),nc_interval_exact(0.0625)),nc_l1_3(eu));
    let norm_lower=nc_f_sub(nc_interval_exact(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))),nc_f_add(direction_variation,direction_extra)).x;
    if !(norm_lower>0.0)||!nc_finite(norm_lower){return nh_invalid();}
    let path_sum=nc_f_add(nc_f_add(nc_interval_exact(abs(q2.x)),nc_interval_exact(abs(q3.x))),nc_f_add(nc_interval_exact(abs(q4.x)),nc_interval_exact(abs(q5.x))));
    let path_upper_interval=nc_f_div(nc_interval_exact(abs(step)),nc_interval_exact(vertical_lower));
    if !nc_interval_valid(path_sum)||!nc_interval_valid(path_upper_interval){return nh_invalid();}
    let path_upper=max(path_sum.y,path_upper_interval.y);
    let c1=nc_f_add(nc_interval_exact(q2.x),nc_interval_exact(q3.x));
    let c2=nc_f_add(nc_f_sub(nc_interval_exact(q4.x),nc_interval_exact(q3.x)),nc_interval_exact(q5.x));
    let c3=nc_f_sub(nc_f_sub(nc_interval_zero(),nc_interval_exact(q4.x)),nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(q5.x)));
    let c4=nc_interval_exact(q5.x);
    let b0=c1;let b1=nc_f_add(c1,nc_f_div(nc_f_mul(nc_interval_exact(2.0),c2),nc_interval_exact(3.0)));
    let b2=nc_f_add(nc_f_add(c1,nc_f_div(nc_f_mul(nc_interval_exact(4.0),c2),nc_interval_exact(3.0))),c3);
    let b3=nc_f_add(nc_f_add(nc_f_add(c1,nc_f_mul(nc_interval_exact(2.0),c2)),nc_f_mul(nc_interval_exact(3.0),c3)),nc_f_mul(nc_interval_exact(4.0),c4));
    if !nc_interval_valid(b0)||!nc_interval_valid(b1)||!nc_interval_valid(b2)||!nc_interval_valid(b3)||min(min(b0.x,b1.x),min(b2.x,b3.x))<=0.0{return nh_invalid();}
    let radius=6399594.0+abs(nc_height(start.q))+abs(step);
    let embedded=max(max(radius*length(eq.yz),2.0*path_upper*length(eu)/norm_lower),abs(eq.x)+abs(en));
    let interior=max(max(radius*length(q5.yz),2.0*path_upper*length(u5)/norm_lower),abs(q5.x)+abs(n5))*0.0625;
    let error=embedded+interior;if !nc_finite(error){return nh_invalid();}
    return NhCurve(NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),error,1u),step,vertical_lower,path_upper);
}
