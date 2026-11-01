// Original MIT. Unregistered hybrid-controller helpers. Height is used only
// for a certified monotone trial; the original arclength loop remains usable
// at every state and can resume height steps after a turning interval.
struct NhAdvance {
    state:NcState,curve:NhCurve,path:f32,attempted_height:f32,
    crossing:i32,kind:u32,charge:f32,
}; // kind 0: decline to path; 1: retry a smaller H trial; 2: accepted.
var<private> nh_attempts:u32;
var<private> nh_accepted:u32;
fn nh_gap_interval(state:NcState,gap:vec3<f32>)->vec2<f32>{
    return nc_f_add(nc_f_add(nc_interval_exact(gap.x),nc_interval_exact(state.q.x)),nc_f_add(nc_interval_exact(gap.y),vec2(-gap.z,gap.z)));
}
fn nh_state_per_height(start:NcState,curve:NhCurve)->vec2<f32>{
    let c=curve.data;let span=nc_interval_exact(abs(curve.height_step));
    let chart_sum=nc_f_add(nc_f_add(nc_l1(c.q2.yz),nc_l1(c.q3.yz)),nc_f_add(nc_l1(c.q4.yz),nc_l1(c.q5.yz)));
    let direction_sum=nc_f_add(nc_f_add(nc_l1_3(c.u2),nc_l1_3(c.u3)),nc_f_add(nc_l1_3(c.u4),nc_l1_3(c.u5)));
    let norm_lower=nc_f_sub(nc_interval_exact(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))),direction_sum);
    if !nc_interval_valid(norm_lower)||!(norm_lower.x>0.0){return nc_interval_invalid();}
    let radius=nc_f_add(nc_interval_exact(6399594.0),nc_f_add(nc_interval_exact(abs(nc_height(start.q))),span));
    let physical=nc_f_add(nc_interval_exact(1.0),nc_f_mul(radius,nc_f_div(chart_sum,span)));
    let direction=nc_f_div(nc_f_mul(nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(curve.path_upper)),direction_sum),nc_f_mul(span,vec2(norm_lower.x)));
    let path_sum=nc_f_add(nc_f_add(nc_interval_exact(abs(c.q2.x)),nc_interval_exact(abs(c.q3.x))),nc_f_add(nc_interval_exact(abs(c.q4.x)),nc_interval_exact(abs(c.q5.x))));
    let excess_sum=nc_f_add(nc_f_add(nc_interval_exact(abs(c.optical.x)),nc_interval_exact(abs(c.optical.y))),nc_f_add(nc_interval_exact(abs(c.optical.z)),nc_interval_exact(abs(c.optical.w))));
    let path=nc_f_div(path_sum,span);let optical=nc_f_div(nc_f_add(path_sum,excess_sum),span);
    if !nc_interval_valid(physical)||!nc_interval_valid(direction)||!nc_interval_valid(path)||!nc_interval_valid(optical){return nc_interval_invalid();}
    return vec2(0.0,max(max(physical.y,direction.y),max(path.y,optical.y)));
}
fn nh_attempt(start:NcState,proposal:f32,path_limit:f32,tolerance:f32)->NhAdvance{
    var result=NhAdvance(start,nh_invalid(),0.0,0.0,0,0u,0.0);
    let vertical=dot(start.u,nc_normal(start.q));
    // Selection headroom keeps the method away from its conditioning edge.
    // The complete accepted height curve still requires |v|>.01. Intervals
    // selected out here use the same physical arclength equations instead.
    if !nc_finite(vertical)||!(abs(vertical)>0.02){return result;}
    let orientation=select(-1.0,1.0,vertical>0.0);
    let gap=select(nc_lower_gap,nc_upper_gap,vertical>0.0);
    let residual=nh_gap_interval(start,gap);
    if !nc_interval_valid(residual)||nc_interval_sign(residual)!=select(1,-1,vertical>0.0){return result;}
    let remaining=-((gap.x+start.q.x)+gap.y);
    if !nc_finite(remaining)||!(orientation*remaining>0.0){return result;}
    let distance_lower=select(residual.x,-residual.y,vertical>0.0);
    let distance_upper=select(residual.y,-residual.x,vertical>0.0);
    let available=min(proposal,abs(vertical)*path_limit);
    let reaches=available>=distance_upper;
    // At a prescribed source endpoint, integrate to the INSIDE endpoint of
    // its certified distance interval. The remaining correction is charged
    // below and must join the scene-event uncertainty in the physical path.
    var amount=select(available,distance_lower,reaches);
    if !(amount>=0.000000001){return result;}
    // A non-contact interval must stay strictly within the same owned branch.
    // If a rounded estimate approaches the uncertain boundary, take a prefix;
    // no ownership is inferred from a rounded zero or proximity test.
    if !reaches{
        if !(amount<distance_lower){amount=min(amount,0.5*distance_lower);}
    }
    if !(amount>=0.000000001){return result;}
    let step=orientation*amount;result.attempted_height=amount;result.kind=1u;
    nh_attempts+=1u;
    let curve=nh_trial(start,step);result.curve=curve;
    if curve.data.valid==0u||!(curve.data.error<tolerance)||!(curve.path_upper<=path_limit){return result;}
    var next=nh_curve_state(start,curve,1.0);
    let direction_norm2=dot(next.u,next.u);
    if !nc_finite3(next.q)||!nc_finite3(next.u)||!(direction_norm2>0.0)||!nc_finite(direction_norm2){return result;}
    next.u=next.u*inverseSqrt(direction_norm2);
    let delta_path=nh_curve_path(curve,1.0);
    if !(delta_path>0.0)||!nc_finite(delta_path){return result;}
    // The stored offset, not only the exact affine increment, must retain
    // the current one-sided source owner at an ordinary accepted endpoint.
    // A small charged addition roundtrip does not itself prove ownership.
    if !reaches{
        if !(orientation*(next.q.x-start.q.x)>0.0){
            // This parameter cannot represent progress at the current offset.
            // Halving cannot repair it: continue from the unchanged accepted
            // state using the original arclength equations instead.
            result.kind=0u;return result;
        }
        let stored_residual=nh_gap_interval(next,gap);
        if !nc_interval_valid(stored_residual)||nc_interval_sign(stored_residual)!=nc_interval_sign(residual){return result;}
    }
    // The independent-variable endpoint is prescribed at a source knot.
    // Bound the affine curve's H discrepancy before replacing its anchor.
    // Interior endpoints also charge their stored offset-addition roundtrip.
    var height_error=nc_f_sub(nc_f_add(nc_interval_exact(start.q.x),nc_interval_exact(step)),nc_interval_exact(next.q.x));
    if reaches{
        height_error=nc_f_add(nc_f_add(nc_f_add(nc_interval_exact(gap.x),nc_interval_exact(start.q.x)),nc_interval_exact(step)),nc_f_add(nc_interval_exact(gap.y),vec2(-gap.z,gap.z)));
    }
    let metric=nh_state_per_height(start,curve);
    if !nc_interval_valid(height_error)||!nc_interval_valid(metric){return result;}
    let charge=nc_f_mul(nc_interval_exact(nc_interval_abs_upper(height_error)),metric);
    let total=nc_f_add(nc_interval_exact(curve.data.error),charge);
    nc_debug2=vec4(height_error,metric.y,charge.y);
    if !nc_interval_valid(charge)||!nc_interval_valid(total)||total.y>tolerance{return result;}
    // The additional one-eighth event allocation belongs to prescribed
    // source endpoint correction. Ordinary representation error is charged
    // against the same total local metric, without a fictitious root budget.
    if reaches && charge.y>(tolerance-curve.data.error)*0.125{return result;}
    result.state=next;result.path=delta_path;result.charge=charge.y;result.kind=2u;
    if reaches{result.crossing=select(-1,1,vertical>0.0);}
    return result;
}
