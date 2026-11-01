// Original MIT. Isolated h0 scheduling correction for the Cartesian consumer.
// A covered root enclosure does not return a contact/state and therefore need
// not meet the separate numerical contact-root width. Uncovered/ambiguous
// enclosures retain the original width, total-charge and unresolved policies.
// There is one coverage call site, including the interval-Newton fixed point.
fn nc_scene_h0(curve:NcSceneCurve,parameters:NcMapParameters,width:f32,tolerance:f32)->NcSceneEvent{
    let c=curve.physical;let before=nc_scene_height(curve,vec2(nc_cartesian_piece.parameter.x),parameters);let after=nc_scene_height(curve,vec2(curve.theta_end),parameters);
    let a=nc_interval_sign(before);let b=nc_interval_sign(after);
    let accel=nc_f_add(nc_f_add(nc_f_mul(vec2(2.0),vec2(abs(c.q3.x))),nc_f_mul(vec2(4.0),vec2(abs(c.q4.x)))),nc_f_mul(vec2(2.0),vec2(abs(c.q5.x))));
    let derivative0=nc_f_add(vec2(c.q2.x),vec2(c.q3.x));let bend=nc_f_mul(accel,vec2(curve.theta_end));
    let derivative=nc_f_add(derivative0,vec2(-bend.y,bend.y));
    if !nc_interval_valid(before)||!nc_interval_valid(after)||!nc_interval_valid(accel)||!nc_interval_valid(derivative){return nc_scene_uncertain();}
    if a!=0&&a==b{
        if nc_interval_sign(derivative)!=0{return nc_scene_none();}
        let sag=nc_f_mul(nc_f_mul(accel,nc_f_mul(vec2(curve.theta_end),vec2(curve.theta_end))),vec2(0.125));
        let margin=select(min(-before.y,-after.y),min(before.x,after.x),a>0);
        if nc_interval_valid(sag)&&margin>sag.y{return nc_scene_none();}return nc_scene_uncertain();
    }
    if a==0||b==0||nc_interval_sign(derivative)!=b{return nc_scene_uncertain();}
    var enclosure=vec2(nc_cartesian_piece.parameter.x,curve.theta_end);
    for(var iteration=0u;iteration<32u;iteration+=1u){
        let mid=enclosure.x+(enclosure.y-enclosure.x)*0.5;
        let remaining=nc_f_sub(vec2(enclosure.y),vec2(enclosure.x));
        if !nc_interval_valid(remaining){return nc_scene_uncertain();}
        let narrow=remaining.y<=width;
        if !narrow{
            let correction=nc_f_div(nc_scene_height(curve,vec2(mid),parameters),derivative);let proposed=nc_f_sub(vec2(mid),correction);
            let next=vec2(max(enclosure.x,proposed.x),min(enclosure.y,proposed.y));
            if !nc_interval_valid(proposed)||!nc_interval_valid(next){return nc_scene_uncertain();}
            if any(next!=enclosure){enclosure=next;continue;}
        }
        // The original strict signs/transverse derivative certify this root
        // enclosure. Coverage must hold for EVERY enclosed point/normal and
        // keeps its existing numerical padding and traversal limit.
        let coverage=nc_scene_coverage(curve,parameters,enclosure);
        if coverage==1u{return nc_scene_none();}
        if coverage==2u||!narrow{return nc_scene_uncertain();}
        let charge=nc_scene_contact_charge(curve,parameters,mid,enclosure);
        if !nc_interval_valid(charge)||charge.y>tolerance{return nc_scene_uncertain();}
        return NcSceneEvent(1u,mid,enclosure.x,enclosure.y,nc_normal(nc_scene_q(curve,mid)),0u,0u);
    }
    return nc_scene_uncertain();
}
