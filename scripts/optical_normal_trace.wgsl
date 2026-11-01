// Original MIT. Isolated clear-ray normal-chart controller, not registered.
// It uses the same canonical n(H) and unchanged local positional allowance.
// Scene/h0 contacts are deliberately outside this first diagnostic's scope.
struct NcSettings{
    pose:vec4<f32>, // sin/cos latitude, authored eye height high/low
    initial:vec4<f32>, // authored initial refractivity/slope, owner, knot count
    limits:vec4<f32>, // lower domain, max trial, max in-air path, minimum tolerance
    initial_lower:vec4<f32>, // signed eye-minus-lower gap high/low/error
    initial_upper:vec4<f32>, // signed eye-minus-upper gap high/low/error
    layers:array<vec4<f32>,128>, // height high/low, source refractivity, upward slope
    gaps:array<vec4<f32>,128>, // adjacent positive gaps; last node stores base-minus-domain
};
@group(0) @binding(0) var<uniform> nc_settings:NcSettings;
struct NcResult{p:vec3<f32>,u:vec3<f32>,path:f32,status:u32,attempts:u32,roots:u32,reason:u32};
var<private> nc_root_count:u32;
var<private> nc_lower_gap:vec3<f32>;
var<private> nc_upper_gap:vec3<f32>;
// Diagnostic-only raw telemetry. It never affects a controller decision.
var<private> nc_debug0:vec4<f32>;
var<private> nc_debug1:vec4<f32>;
var<private> nc_debug2:vec4<f32>;
var<private> nc_debug3:vec4<f32>;
var<private> nc_debug4:vec4<f32>;
fn nc_split_difference(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    return nc_interval_add(nc_interval_sub(nc_interval_exact(a.x),nc_interval_exact(b.x)),nc_interval_sub(nc_interval_exact(a.y),nc_interval_exact(b.y)));
}
fn nc_residual(start:NcState,curve:NcCurve,parameter:f32,boundary_gap:vec3<f32>)->vec2<f32>{
    var theta=nc_interval_exact(parameter);if parameter==0.0{theta=nc_interval_zero();}
    let z=nc_interval_sub(nc_interval_exact(1.0),theta);
    let poly=nc_interval_add(nc_interval_exact(curve.q2.x),nc_interval_mul(z,nc_interval_add(nc_interval_exact(curve.q3.x),nc_interval_mul(theta,nc_interval_add(nc_interval_exact(curve.q4.x),nc_interval_mul(z,nc_interval_exact(curve.q5.x)))))));
    // Cancel the large high gap against the carried offset BEFORE adding its
    // low part. The packet bounds its encoded split-height subtraction error.
    var high=nc_interval_exact(boundary_gap.x);if boundary_gap.x==0.0{high=nc_interval_zero();}
    let offset=nc_interval_exact(start.q.x);
    let low=nc_interval_add(nc_interval_exact(boundary_gap.y),vec2(-boundary_gap.z,boundary_gap.z));
    return nc_interval_add(nc_interval_add(nc_interval_add(high,offset),nc_interval_mul(theta,poly)),low);
}
fn nc_height_speed(curve:NcCurve,step:f32)->vec2<f32>{
    let sum=nc_interval_add(nc_interval_add(nc_interval_exact(abs(curve.q2.x)),nc_interval_exact(abs(curve.q3.x))),nc_interval_add(nc_interval_exact(abs(curve.q4.x)),nc_interval_exact(abs(curve.q5.x))));
    return nc_interval_div(sum,nc_interval_exact(step));
}
fn nc_height_acceleration(curve:NcCurve,step:f32)->vec2<f32>{
    let sum=nc_interval_add(nc_interval_add(nc_interval_mul(nc_interval_exact(2.0),nc_interval_exact(abs(curve.q3.x))),nc_interval_mul(nc_interval_exact(4.0),nc_interval_exact(abs(curve.q4.x)))),nc_interval_mul(nc_interval_exact(2.0),nc_interval_exact(abs(curve.q5.x))));
    return nc_interval_div(sum,nc_interval_mul(nc_interval_exact(step),nc_interval_exact(step)));
}
fn nc_l1(v:vec2<f32>)->vec2<f32>{return nc_interval_add(nc_interval_exact(abs(v.x)),nc_interval_exact(abs(v.y)));}
fn nc_physical_speed(start:NcState,curve:NcCurve,step:f32)->vec2<f32>{
    let qsum=nc_interval_add(nc_interval_add(nc_l1(curve.q2.yz),nc_l1(curve.q3.yz)),nc_interval_add(nc_l1(curve.q4.yz),nc_l1(curve.q5.yz)));
    let qspeed=nc_interval_div(qsum,nc_interval_exact(step));let hspeed=nc_height_speed(curve,step);
    let initial_h=nc_interval_add(nc_interval_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),nc_interval_exact(start.q.x));
    let height=nc_interval_add(nc_interval_exact(nc_interval_abs_upper(initial_h)),nc_interval_mul(hspeed,nc_interval_exact(step)));
    return nc_interval_add(hspeed,nc_interval_mul(nc_interval_add(nc_interval_exact(6399594.0),height),qspeed));
}
fn nc_l1_3(v:vec3<f32>)->vec2<f32>{return nc_interval_add(nc_l1(v.xy),nc_interval_exact(abs(v.z)));}
fn nc_root_speed(start:NcState,curve:NcCurve,step:f32)->vec2<f32>{
    // Triangle inequality bounds the norm of every raw direction on the
    // quartic, independently of endpoint normalization. All basis factors
    // multiplying u2..u5 have absolute value <=1 on [0,1].
    let variation=nc_interval_add(nc_interval_add(nc_l1_3(curve.u2),nc_l1_3(curve.u3)),nc_interval_add(nc_l1_3(curve.u4),nc_l1_3(curve.u5)));
    let norm_lower=nc_interval_sub(nc_interval_exact(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))),variation);
    if !nc_interval_valid(norm_lower)||!(norm_lower.x>0.0){return nc_interval_invalid();}
    // 2*step*raw_direction_speed/norm_lower; step cancels algebraically.
    let direction=nc_interval_div(nc_interval_mul(nc_interval_exact(2.0),variation),vec2(norm_lower.x));
    let optical=nc_interval_add(nc_interval_exact(1.0),nc_interval_div(nc_interval_add(nc_interval_add(nc_interval_exact(abs(curve.optical.x)),nc_interval_exact(abs(curve.optical.y))),nc_interval_add(nc_interval_exact(abs(curve.optical.z)),nc_interval_exact(abs(curve.optical.w)))),nc_interval_exact(step)));
    let physical=nc_physical_speed(start,curve,step);
    if !nc_interval_valid(direction)||!nc_interval_valid(optical)||!nc_interval_valid(physical){return nc_interval_invalid();}
    return vec2(0.0,max(max(1.0,physical.y),max(direction.y,optical.y)));
}
fn nc_vertical_start(curve:NcCurve,step:f32)->vec2<f32>{
    var sum=nc_interval_add(nc_interval_exact(curve.q2.x),nc_interval_exact(curve.q3.x));
    if curve.q2.x==-curve.q3.x{sum=nc_interval_zero();}
    return nc_interval_div(sum,nc_interval_exact(step));
}
fn nc_departure_safe(curve:NcCurve,ascending:bool)->bool{
    let r2=nc_interval_exact(curve.q2.x);let r3=nc_interval_exact(curve.q3.x);
    let r4=nc_interval_exact(curve.q4.x);let r5=nc_interval_exact(curve.q5.x);
    var c1=nc_interval_add(r2,r3);if curve.q2.x==-curve.q3.x{c1=nc_interval_zero();}
    let c2=nc_interval_add(nc_interval_sub(r4,r3),r5);
    let c3=nc_interval_sub(nc_interval_sub(nc_interval_zero(),r4),nc_interval_mul(r5,nc_interval_exact(2.0)));
    let third=nc_interval_div(nc_interval_exact(1.0),nc_interval_exact(3.0));
    let two_thirds=nc_interval_div(nc_interval_exact(2.0),nc_interval_exact(3.0));
    let b0=c1;let b1=nc_interval_add(c1,nc_interval_mul(c2,third));
    let b2=nc_interval_add(nc_interval_add(c1,nc_interval_mul(c2,two_thirds)),nc_interval_mul(c3,third));
    let b3=nc_interval_add(nc_interval_add(nc_interval_add(c1,c2),c3),r5);
    if !nc_interval_valid(b0)||!nc_interval_valid(b1)||!nc_interval_valid(b2)||!nc_interval_valid(b3){return false;}
    if ascending{return b0.x>=0.0 && b1.x>=0.0 && b2.x>=0.0 && b3.x>0.0;}
    return b0.y<=0.0 && b1.y<=0.0 && b2.y<=0.0 && b3.y<0.0;
}
// 0 unresolved,1 proven no crossing,2 monotone crossing. The original full
// numerical curve is retained even when a source contact clips its interval.
fn nc_boundary(start:NcState,curve:NcCurve,step:f32,boundary_gap:vec3<f32>,lower:bool)->u32{
    let acceleration=nc_height_acceleration(curve,step);let derivative=nc_vertical_start(curve,step);
    if !nc_interval_valid(acceleration)||!nc_interval_valid(derivative){return 0u;}
    let bend=nc_interval_mul(acceleration,nc_interval_exact(step));
    if !nc_interval_valid(bend){return 0u;}
    let increasing=derivative.x>bend.y;let decreasing=derivative.y< -bend.y;
    if all(boundary_gap==vec3(0.0)) && start.q.x==0.0{
        if (lower && increasing)||(!lower && decreasing)||nc_departure_safe(curve,lower){return 1u;}
        return 0u;
    }
    let before=nc_residual(start,curve,0.0,boundary_gap);let after=nc_residual(start,curve,1.0,boundary_gap);
    nc_debug1=vec4(before,after);
    let a=nc_interval_sign(before);let b=nc_interval_sign(after);
    if a==0||b==0{return 0u;}
    if a!=b{if increasing||decreasing{return 2u;}return 0u;}
    if increasing||decreasing{return 1u;}
    let margin=select(min(-before.y,-after.y),min(before.x,after.x),a>0);
    let sag=nc_interval_mul(nc_interval_mul(acceleration,nc_interval_mul(nc_interval_exact(step),nc_interval_exact(step))),nc_interval_exact(0.125));
    if nc_interval_valid(sag)&&margin>sag.y{return 1u;}return 0u;
}
struct NcRoot{theta:f32,path_error:f32,charge:f32,valid:u32};
fn nc_height_root(start:NcState,curve:NcCurve,step:f32,boundary_gap:vec3<f32>,allocation:f32)->NcRoot{
    nc_root_count+=1u;let invalid=NcRoot(-1.0,3.0e37,3.0e37,0u);
    nc_debug3=vec4(0.0,step,allocation,1.0);
    var lo=0.0;var hi=1.0;
    let before=nc_residual(start,curve,lo,boundary_gap);let after=nc_residual(start,curve,hi,boundary_gap);
    let sign=nc_interval_sign(before);if sign==0||nc_interval_sign(after)==0||sign==nc_interval_sign(after){return invalid;}
    let velocity=nc_root_speed(start,curve,step);let derivative=nc_vertical_start(curve,step);
    let bend=nc_interval_mul(nc_height_acceleration(curve,step),nc_interval_exact(step));
    let slope_bound=nc_interval_sub(nc_interval_exact(select(-derivative.y,derivative.x,derivative.x>0.0)),bend);
    let slope=slope_bound.x;
    if !nc_interval_valid(velocity)||!nc_interval_valid(derivative)||!nc_interval_valid(bend)||!nc_interval_valid(slope_bound)||!(slope>0.0)||!(allocation>0.0){return invalid;}
    let f0=(before.x+before.y)*0.5;let f1=(after.x+after.y)*0.5;
    var trial=lo+(hi-lo)*0.5;let chord_denominator=f0-f1;
    if nc_interval_quotient_safe(f0,chord_denominator){
        let fraction=f0/chord_denominator;
        if nc_finite(fraction)&&fraction>0.0&&fraction<1.0{trial=fraction;}
    }
    for(var i=0;i<32;i+=1){
        let residual=nc_residual(start,curve,trial,boundary_gap);if !nc_interval_valid(residual){return invalid;}
        let herror=nc_interval_abs_upper(residual);
        let residual_path=nc_interval_div(nc_interval_exact(herror),nc_interval_exact(slope));
        let left=nc_interval_sub(nc_interval_exact(trial),nc_interval_exact(lo));let right=nc_interval_sub(nc_interval_exact(hi),nc_interval_exact(trial));
        if !nc_interval_valid(residual_path)||!nc_interval_valid(left)||!nc_interval_valid(right){return invalid;}
        let bracket_path=nc_interval_mul(nc_interval_exact(max(left.y,right.y)),nc_interval_exact(step));
        let path_product=nc_interval_mul(nc_interval_exact(trial),nc_interval_exact(step));
        let rounded_path=trial*step;
        let product_error=nc_interval_abs_upper(nc_interval_sub(path_product,nc_interval_exact(rounded_path)));
        let path_bound=nc_interval_add(nc_interval_exact(min(bracket_path.y,residual_path.y)),nc_interval_exact(product_error));
        if !nc_interval_valid(bracket_path)||!nc_interval_valid(path_product)||!nc_interval_valid(path_bound){return invalid;}
        let perror=path_bound.y;
        let charge=nc_interval_add(nc_interval_mul(velocity,nc_interval_exact(perror)),nc_interval_exact(herror));
        nc_debug2=vec4(residual,perror,charge.y);nc_debug3=vec4(trial,step,allocation,2.0);
        if nc_interval_valid(charge)&&charge.y<=allocation{return NcRoot(trial,perror,charge.y,1u);}
        let current_sign=nc_interval_sign(residual);if current_sign==0{return invalid;}
        if current_sign==sign{lo=trial;}else{hi=trial;}
        let value=(residual.x+residual.y)*0.5;
        let derivative_at=nc_curve_dq(curve,trial,1.0).x;
        var next=lo+(hi-lo)*0.5;
        if nc_interval_quotient_safe(value,derivative_at){
            let proposed=trial-value/derivative_at;
            if nc_finite(proposed)&&proposed>lo&&proposed<hi{next=proposed;}
        }
        if next==trial||!(next>lo&&next<hi){return invalid;}trial=next;
    }
    return invalid;
}
fn nc_result(state:NcState,path:f32,status:u32,attempts:u32,reason:u32)->NcResult{
    return NcResult(nc_position(state.q),state.u,path,status,attempts,nc_root_count,reason);
}
fn nc_source_proposal(state:NcState,adaptive:f32)->f32{
    // A proposal only: every trial still passes the unchanged polynomial
    // no-hidden-crossing, error and confirmed-root checks. Its finite gap
    // estimate is never used as a certificate or an accepted contact.
    let vertical=dot(state.u,nc_normal(state.q));
    let gap=select(nc_lower_gap,nc_upper_gap,vertical>=0.0);
    let residual=(gap.x+state.q.x)+gap.y;
    if !nc_interval_quotient_safe(-residual,vertical){return adaptive;}
    let estimate=-residual/vertical;
    if !(estimate>0.0 && estimate<1e30){return adaptive;}
    return min(adaptive,max(0.000001,1.125*estimate));
}
fn nc_trace(direction:vec3<f32>)->NcResult{
    let axis=vec3(0.0,nc_settings.pose.x,-nc_settings.pose.y);nc_axis=axis*inverseSqrt(dot(axis,axis));
    nc_eye_height=nc_settings.pose.zw;nc_anchor=nc_eye_height;nc_index=nc_settings.initial.xy;nc_root_count=0u;
    nc_lower_gap=nc_settings.initial_lower.xyz;nc_upper_gap=nc_settings.initial_upper.xyz;
    var owner=u32(nc_settings.initial.z);let count=u32(nc_settings.initial.w);
    var state=NcState(vec3(0.0),direction);var path=0.0;var proposed=min(2000.0,nc_settings.limits.y);
    if count<2u||count>128u||owner>=count{return nc_result(state,path,2u,0u,1u);}
    for(var attempt=1u;attempt<=4096u;attempt+=1u){
        let position=nc_position(state.q);let tolerance=max(nc_settings.limits.w,4e-7*length(position));
        nc_debug0=vec4(state.q.x,proposed,tolerance,f32(owner));
        if path>nc_settings.limits.z{return nc_result(state,path,2u,attempt,2u);}
        let spacing_band=max(1e-5,4e-7*length(position));
        for(var adjacent=max(0,i32(owner)-2);adjacent<=min(i32(count)-2,i32(owner));adjacent+=1){
            let gap=nc_settings.gaps[u32(adjacent)];
            let spacing=nc_interval_add(nc_interval_add(nc_interval_exact(gap.x),nc_interval_exact(gap.y)),vec2(-gap.z,gap.z));
            if !nc_interval_valid(spacing)||spacing.x<4.0*spacing_band{return nc_result(state,path,2u,attempt,7u);}
        }
        let adaptive=min(proposed,min(nc_settings.limits.y,0.49/max(abs(nc_index.y),1e-30)));
        let step=nc_source_proposal(state,adaptive);
        if !(step>=0.000001){return nc_result(state,path,2u,attempt,3u);}
        let curve=nc_trial(state,step);
        nc_debug4=vec4(curve.error,nc_anchor,0.0);
        if curve.valid==0u||!(curve.error<tolerance){proposed=step*0.5;continue;}
        let lower_kind=nc_boundary(state,curve,step,nc_lower_gap,true);let upper_kind=nc_boundary(state,curve,step,nc_upper_gap,false);
        nc_debug4.w=f32(lower_kind*10u+upper_kind);
        if lower_kind==0u||upper_kind==0u{proposed=step*0.5;continue;}
        var contact=NcRoot(1.0,0.0,0.0,1u);var crossing=0;
        let allocation=(tolerance-curve.error)*0.125;
        if lower_kind==2u{contact=nc_height_root(state,curve,step,nc_lower_gap,allocation);crossing=-1;}
        if upper_kind==2u{
            let root=nc_height_root(state,curve,step,nc_upper_gap,allocation);
            if root.valid==0u{contact=root;}else if crossing==0||root.theta<contact.theta{contact=root;crossing=1;}
        }
        if contact.valid==0u{proposed=step*0.5;continue;}
        let t=contact.theta;var next=NcState(nc_curve_q(state,curve,t),normalize(nc_curve_u(state,curve,t)));
        if !nc_finite3(next.q)||!nc_finite3(next.u){return nc_result(state,path,2u,attempt,4u);}
        if crossing!=0{
            if crossing<0 && owner==0u{return nc_result(state,path,2u,attempt,5u);}
            let target_index=select(owner,owner-1u,crossing<0);let node=nc_settings.layers[target_index];
            // The root already certified its complete outward state/path
            // charge for this unchanged curve, branch and accepted theta.
            let charge=nc_interval_add(nc_interval_exact(curve.error),nc_interval_exact(contact.charge));
            if !nc_interval_valid(charge)||charge.y>tolerance{proposed=step*0.5;continue;}
            // At a certified source contact, height itself is the canonical
            // coordinate. Its bounded correction is charged above. No spatial
            // inverse resets or ambiguous near-knot owner selection occur.
            owner=select(owner+1u,owner-1u,crossing<0);nc_anchor=node.xy;next.q.x=0.0;
            nc_index=vec2(node.z,0.0);if owner>0u && owner<count{nc_index.y=nc_settings.layers[owner-1u].w;}
            if crossing>0{
                nc_lower_gap=vec3(0.0);
                let gap=nc_settings.gaps[target_index];nc_upper_gap=vec3(-gap.xy,gap.z);
            }else{
                nc_upper_gap=vec3(0.0);
                let gap=nc_settings.gaps[select(target_index-1u,count-1u,target_index==0u)];nc_lower_gap=gap.xyz;
            }
        }
        state=next;path+=step*contact.theta;
        if owner==count{return nc_result(state,path,0u,attempt,0u);}
        proposed=step;
        // A low-error source-limited trial does not discard the preceding
        // adaptive proposal. Rejections still halve the actual tested step.
        if curve.error<tolerance*0.03125{proposed=min(adaptive*2.0,nc_settings.limits.y);}
    }
    return nc_result(state,path,2u,4096u,6u);
}
