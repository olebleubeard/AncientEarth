// Original MIT. Diagnostic one-sided DP54 controller; not a runtime shader.
// Unregistered carried auxiliary direct dense diagnostic: one accepted polynomial serves every contact.
// The raw DP embedded estimate plus quartic/Hermite interior estimate is retained.
var<private> dp_layer:i32;
var<private> dp_height_offset:f32;
var<private> dp_curve_offset:f32;
fn dp_select_layer(height:f32,vertical:f32)->i32{
    let count=i32(settings.budget.x);var result=-1;
    for(var i=0;i<count-1;i+=1){
        if height>settings.layers[u32(i)].x || (height==settings.layers[u32(i)].x && vertical>=0.0){result=i;}else{break;}
    }
    return result;
}
fn dp_branch()->vec3<f32>{
    if dp_layer<0{return vec3(settings.scene.y,settings.layers[0].y,0.0);}
    return settings.layers[u32(dp_layer)].xyz;
}
fn dp_bounds()->vec2<f32>{
    if dp_layer<0{return vec2(settings.scene.z,settings.scene.y);}
    return vec2(settings.layers[u32(dp_layer)].x,settings.layers[u32(dp_layer)+1u].x);
}
fn dp_anchor(layer:i32)->f32{if layer<0{return settings.scene.y;}return settings.layers[u32(layer)].x;}
var<private> dp_curve:DpStep;
var<private> dp_curve_step:f32;
var<private> dp_curve_start:vec4<f32>;
fn dp_advance_delta(s:State,distance:f32)->State{
    let t=distance/dp_curve_step;let v=1.0-t;
    let p=t*(dp_curve.delta.p+v*(dp_curve.r3+t*(dp_curve.r4+v*dp_curve.r5)));
    let u=s.u+t*(dp_curve.du+v*(dp_curve.ur3+t*(dp_curve.ur4+v*dp_curve.ur5)));
    return State(p,normalize(u));
}
fn dp_curve_derivative(distance:f32)->vec3<f32>{
    let t=distance/dp_curve_step;
    return (dp_curve.delta.p+(1.0-2.0*t)*dp_curve.r3+(2.0*t-3.0*t*t)*dp_curve.r4+(2.0*t-6.0*t*t+4.0*t*t*t)*dp_curve.r5)/dp_curve_step;
}
fn dp_curve_acceleration()->f32{return (2.0*length(dp_curve.r3)+4.0*length(dp_curve.r4)+2.0*length(dp_curve.r5))/(dp_curve_step*dp_curve_step);}
fn dp_curve_velocity()->f32{return (length(dp_curve.delta.p)+length(dp_curve.r3)+length(dp_curve.r4)+length(dp_curve.r5))/dp_curve_step;}
fn dp_height_uncertainty(start:State,g:vec4<f32>)->f32{
    return max(0.000001,9.536743e-7*(length(start.p)+abs(g.w)+dp_curve_velocity()*dp_curve_step));
}
fn dp_height_bound(start:State,g:vec4<f32>)->f32{
    let v=dp_curve_velocity();let lower=min(settings.scene.z,g.w-dp_height_uncertainty(start,g));
    let radius=6335439.0+lower-v*dp_curve_step;
    if !(radius>0.0){return -1.0;}
    return v*v/radius+dp_curve_acceleration();
}
// Every relevant source/reference boundary must be monotone on the numerical
// curve, or stay on one side by the endpoint-chord sag bound. Ambiguity retries
// a shorter step; it never skips a possible return or double crossing.
fn dp_boundary_safe(f0:f32,f1:f32,derivative:f32,bound:f32,step:f32,padding:f32,departure:i32)->bool{
    let speed_error=9.536743e-7*dp_curve_velocity();
    if departure>0{return derivative-speed_error>bound*step;}
    if departure<0{return -derivative-speed_error>bound*step;}
    if abs(derivative)-speed_error>bound*step{return true;}
    return f0*f1>0.0 && min(abs(f0),abs(f1))>bound*step*step*0.125+padding;
}
fn dp_auxiliary_delta(distance:f32)->vec4<f32>{
    let t=distance/dp_curve_step;let v=1.0-t;
    return t*(dp_curve.gr2+v*(dp_curve.gr3+t*(dp_curve.gr4+v*dp_curve.gr5)));
}
fn dp_auxiliary_derivative(distance:f32)->f32{
    let t=distance/dp_curve_step;
    return (dp_curve.gr2.w+(1.0-2.0*t)*dp_curve.gr3.w+(2.0*t-3.0*t*t)*dp_curve.gr4.w+(2.0*t-6.0*t*t+4.0*t*t*t)*dp_curve.gr5.w)/dp_curve_step;
}
fn dp_auxiliary_bound()->f32{return (2.0*abs(dp_curve.gr3.w)+4.0*abs(dp_curve.gr4.w)+2.0*abs(dp_curve.gr5.w))/(dp_curve_step*dp_curve_step);}
fn dp_prefix(s:State,distance:f32)->DpStep{
    var result=dp_curve;result.delta=dp_advance_delta(s,distance);
    let change=dp_auxiliary_delta(distance);let raw=dp_curve_start+change;
    result.geometry=vec4(normalize(raw.xyz),dp_branch().x+(dp_curve_offset+change.w));return result;
}
fn dp_consistency(aux:vec4<f32>,actual:vec4<f32>)->f32{
    let product=length(aux.xyz)*length(actual.xyz);
    if !dp_finite(aux)||!dp_finite(actual)||!(dot(aux.xyz,actual.xyz)>0.0 && product>1e-10 && product<1e20){return 1e30;}
    let angular=1.41421356237*length(cross(aux.xyz,actual.xyz))/product*(6399594.0+abs(aux.w));
    if !dp_finite(vec4(angular,0.0,0.0,0.0)){return 1e30;}
    return max(abs(aux.w-actual.w),angular);
}
fn dp_advance(s:State,step:f32)->State{let delta=dp_advance_delta(s,step);return State(s.p+delta.p,delta.u);}
// The exact height derivative cannot reverse before this upper bracket. If
// that cannot be proved, retain the conservative first-contact lower bound.
fn dp_bound(gap:f32,speed:f32,curvature:f32,tolerance:f32)->f32{
    if gap<0.0{return 0.0;}
    let discriminant=speed*speed-2.0*curvature*(gap+2.0*tolerance);
    if speed>0.0 && discriminant>0.0{
        return 2.0*(gap+2.0*tolerance)/(speed+sqrt(discriminant));
    }
    return knot_bound(gap,speed,curvature)*0.8;
}
// Root of the already accepted numerical polynomial. The branch bracket is
// preserved; its source event allowance is included in the same local budget.
struct DpContact{distance:f32,step:DpStep,uncertainty:f32,valid:u32};
fn dp_contact(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,tolerance:f32,actual_height:bool)->DpContact{
    var first=(dp_branch().x-contact_height)+dp_curve_offset;
    var last=first+dp_auxiliary_delta(step).w;
    if actual_height{first=g.w-contact_height;last=end.geometry.w-contact_height;}
    let ascending=first<0.0;var low=0.0;var high=step;
    var trial=clamp(step*(-first)/(last-first),low,high);
    let allocation=max(0.000001,tolerance*0.125);
    var result=DpContact(-1.0,end,allocation,0u);
    if first*last>0.0 || first==last{return result;}
    let curvature=select(dp_auxiliary_bound(),dp_height_bound(start,g),actual_height);
    if !(curvature>=0.0){return result;}
    for(var j=0;j<32;j+=1){
        let candidate=dp_prefix(start,trial);
        if !dp_finite(candidate.geometry) || !(candidate.error+allocation<=tolerance){return result;}
        var at=candidate.geometry;
        var residual=first+dp_auxiliary_delta(trial).w;
        var derivative=dp_auxiliary_derivative(trial);
        if actual_height{at=geometry(start.p+candidate.delta.p);residual=at.w-contact_height;derivative=dot(dp_curve_derivative(trial),at.xyz);}
        let h=at.w;
        let slope=abs(derivative)-curvature*(high-low);
        var height_roundoff=max(0.000001,9.536743e-7*(abs(first)+abs(dp_curve.gr2.w)+abs(dp_curve.gr3.w)+abs(dp_curve.gr4.w)+abs(dp_curve.gr5.w)));
        if actual_height{height_roundoff=max(0.000001,9.536743e-7*(length(start.p+candidate.delta.p)+abs(h)));}
        if high-low<=allocation || (slope>0.0 && (abs(residual)+height_roundoff)/slope<=allocation){return DpContact(trial,candidate,allocation,1u);}
        if (residual<0.0)==ascending{low=trial;}else{high=trial;}
        let newton=trial-residual/derivative;var next=(low+high)*0.5;
        if newton>low && newton<high && newton!=trial{next=newton;}
        if next==trial || !(next>low && next<high){return result;}
        trial=next;
    }
    return result;
}
fn dp_trace(direction:vec3<f32>,physical:bool)->Trace{
    var state=State(vec3(0.0),direction);var path=0.0;var proposed=100000.0;
    if settings.budget.w<0.5{return trace_result(state.p,state.u,path,2u);}
    // The same vacuum-entry construction as the existing presentation solver.
    if settings.center.w>settings.budget.y{
        let o=-settings.center.xyz;let along=dot(o,direction);let distance=length(cross(o,direction));
        let radius=6378137.0+settings.budget.y;
        if distance>radius || along>=0.0{return trace_result(state.p,state.u,path,0u);}
        let span=sqrt(max(0.0,(radius-distance)*(radius+distance)));
        let lower=max(0.0,-along-span);var a=lower;var b=-along+span;
        for(var i=0;i<40;i+=1){let x=a+(b-a)*0.38196601125;let y=a+(b-a)*0.61803398875;
            if geometry(direction*x).w<geometry(direction*y).w{b=y;}else{a=x;}}
        var upper=(a+b)*0.5;if geometry(direction*upper).w>settings.budget.y{return trace_result(state.p,state.u,path,0u);}
        var low=lower;for(var i=0;i<32;i+=1){let mid=(low+upper)*0.5;
            if geometry(direction*mid).w>settings.budget.y{low=mid;}else{upper=mid;}}
        state.p=direction*upper;path=upper;
    }
    let entry=path;var g=geometry(state.p);var actual_g=g;dp_layer=dp_select_layer(g.w,dot(state.u,g.xyz));dp_height_offset=g.w-dp_branch().x;
    var departing=0; // signed last resolved knot, zero away from a contact
    for(var i=0;i<4096;i+=1){
        let vertical=dot(state.u,g.xyz);let curvature=(settings.budget.z+settings.scene.x);
        if physical && actual_g.w==0.0 && dot(state.u,actual_g.xyz)<0.0{
            let coverage=terrain_coverage(state.p,actual_g.xyz);
            if coverage==2u{return trace_result(state.p,state.u,path,2u);}
            if coverage==0u{return trace_result(state.p,state.u,path,1u);}
        }
        if g.w>=settings.budget.y-0.01 && vertical>0.0{return trace_result(state.p,state.u,path,0u);}
        if path-entry>2000000.0 || g.w<settings.scene.z-1.0{return trace_result(state.p,state.u,path,2u);}
        let tolerance=max(0.002,4e-7*length(state.p));let branch=dp_branch();let bounds=dp_bounds();
        // Preserve the existing source-spacing eligibility even though
        // active branch ownership is no longer inferred from a near-knot band.
        let band=max(1e-5,4e-7*length(state.p));let count=i32(settings.budget.x);
        for(var adjacent=max(0,dp_layer-1);adjacent<=min(count-2,dp_layer+1);adjacent+=1){
            if settings.layers[u32(adjacent)+1u].x-settings.layers[u32(adjacent)].x<4.0*band{return trace_result(state.p,state.u,path,2u);}
        }
        // Trial extension keeps exact branch n positive; explicit RK stages
        // additionally reject n<0.5/nonfinite before any derivative division.
        var step=min(proposed,min(100000.0,0.49/max(abs(branch.z),1e-30)));
        let lower_gap=(branch.x-bounds.x)+dp_height_offset;let upper_gap=(bounds.y-branch.x)-dp_height_offset;
        if departing>0{
            // A ray just leaving a lower boundary cannot return earlier than
            // 2v/B. A near-tangent unresolved departure is never skipped.
            if vertical<=0.0{return trace_result(state.p,state.u,path,2u);}
            step=min(step,1.6*vertical/curvature);
        }else{step=min(step,dp_bound(max(0.0,lower_gap),-vertical,curvature,tolerance));}
        if departing<0{
            if vertical>=0.0{return trace_result(state.p,state.u,path,2u);}
            step=min(step,-1.6*vertical/curvature);
        }else{step=min(step,dp_bound(max(0.0,upper_gap),vertical,curvature,tolerance));}
        // h0 is a separate possible visibility event, including under terrain.
        let clearance=abs(g.w);let toward=select(-vertical,vertical,g.w<0.0);
        if clearance>0.0{step=min(step,max(0.01,dp_bound(clearance,toward,curvature,tolerance)));}
        if !(step>=0.000001){return trace_result(state.p,state.u,path,2u);}
        var accepted=dopri_delta(state,step,branch,g);
        if accepted.error>tolerance*0.875{
            if step>0.000002{proposed=step*0.5;continue;}
            return trace_result(state.p,state.u,path,2u);
        }
        dp_curve=accepted;dp_curve_step=step;dp_curve_start=g;dp_curve_offset=dp_height_offset;
        accepted=dp_prefix(state,step);
        let polynomial_bound=dp_auxiliary_bound();let polynomial_vertical=dp_auxiliary_derivative(0.0);
        let polynomial_padding=max(0.000001,9.536743e-7*(abs(g.w)+abs(dp_curve.gr2.w)+abs(dp_curve.gr3.w)+abs(dp_curve.gr4.w)+abs(dp_curve.gr5.w)));
        let lower_safe=dp_boundary_safe(lower_gap,lower_gap+dp_curve.gr2.w,polynomial_vertical,polynomial_bound,step,polynomial_padding,select(0,1,departing>0));
        let upper_safe=dp_boundary_safe(-upper_gap,-upper_gap+dp_curve.gr2.w,polynomial_vertical,polynomial_bound,step,polynomial_padding,select(0,-1,departing<0));
        if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe{
            if step>0.000002{proposed=step*0.5;continue;}
            return trace_result(state.p,state.u,path,2u);
        }
        var contact=0;var allocation=0.0;
        var contact_height=0.0;
        if dp_curve.gr2.w>=upper_gap && upper_gap>0.0{contact_height=bounds.y;contact=1;}
        if dp_curve.gr2.w<=-lower_gap && lower_gap>0.0{contact_height=bounds.x;contact=-1;}
        if contact!=0{
            let root=dp_contact(state,g,accepted,step,contact_height,tolerance,false);
            if root.valid==0u{if step>0.000002{proposed=step*0.5;continue;}return trace_result(state.p,state.u,path,2u);}
            step=root.distance;accepted=root.step;allocation=root.uncertainty;
        }
        let next_anchor=dp_anchor(dp_layer+contact);
        let next_offset=(dp_branch().x-next_anchor)+(dp_curve_offset+dp_auxiliary_delta(step).w);
        // Check the exact representation that will be carried into the next
        // source interval, including the finite anchor rebase at a knot.
        accepted.geometry.w=next_anchor+next_offset;
        var end_actual=geometry(state.p+accepted.delta.p);
        var consistency=dp_consistency(accepted.geometry,end_actual);
        if accepted.error+allocation+consistency>tolerance{proposed=step*0.5;continue;}
        let reference_bound=dp_height_bound(state,actual_g);
        let reference_padding=max(dp_height_uncertainty(state,actual_g),dp_height_uncertainty(State(state.p+accepted.delta.p,accepted.delta.u),end_actual));
        if physical && (!dp_boundary_safe(actual_g.w,end_actual.w,dot(dp_curve_derivative(0.0),actual_g.xyz),reference_bound,step,reference_padding,0) || !(reference_bound>=0.0)){
            if step>0.000002{proposed=step*0.5;continue;}
            return trace_result(state.p,state.u,path,2u);
        }
        var reference_distance=-1.0;var reference=accepted;var reference_actual=end_actual;
        if physical && ((actual_g.w>0.0 && end_actual.w<=0.0)||(actual_g.w<0.0 && end_actual.w>=0.0)){
            var actual_end=accepted;actual_end.geometry=end_actual;
            let root=dp_contact(state,actual_g,actual_end,step,0.0,tolerance,true);
            if root.valid==0u{if step>0.000002{proposed=step*0.5;continue;}return trace_result(state.p,state.u,path,2u);}
            let at=geometry(state.p+root.step.delta.p);
            let coverage=terrain_coverage(state.p+root.step.delta.p,at.xyz);
            if coverage==2u{return trace_result(state.p,state.u,path,2u);}
            if coverage==0u{reference_distance=root.distance;reference=root.step;reference_actual=at;}
        }
        var event_step=step;var event_value=accepted;
        if reference_distance>=0.0{event_step=reference_distance;event_value=reference;allocation=max(allocation,tolerance*0.125);}
        if physical{
            let end=State(state.p+event_value.delta.p,event_value.delta.u);
            let error=event_value.error+allocation+dp_consistency(event_value.geometry,select(end_actual,reference_actual,reference_distance>=0.0));
            if error>tolerance{proposed=step*0.5;continue;}
            let event=scene_event(state,end,event_value.delta.p,event_step,error);
            if event.ambiguous!=0u{
                let floor=max(0.001,9.536743e-7*max(length(state.p),length(end.p)));
                if event_step>floor{proposed=event_step*0.5;continue;}
                return trace_result(state.p,state.u,path,2u);
            }
            if event.distance>=0.0{
                let prefix=dp_prefix(state,event.distance);let hit=State(state.p+prefix.delta.p,prefix.delta.u);
                let prefix_consistency=dp_consistency(prefix.geometry,geometry(hit.p));
                if prefix.error+allocation+prefix_consistency>tolerance{
                    if event.distance>0.000002{proposed=min(step*0.5,event.distance*0.5);continue;}
                    return trace_result(state.p,state.u,path,2u);
                }
                return Trace(hit.p,hit.u,path+event.distance,1u,event.normal,event.kind,event.marker);
            }
            if reference_distance>=0.0{return trace_result(end.p,end.u,path+reference_distance,1u);}
        }
        if contact>0 && contact_height==settings.budget.y{return trace_result(state.p+accepted.delta.p,accepted.delta.u,path+step,0u);}
        // Project only redundant coordinates at an already accepted ordinary
        // endpoint, strictly inside the same source interval. The checked
        // discrepancy is already charged; neither physical p/u nor source
        // ownership changes. Contacts retain their integrated height/offset.
        var committed_geometry=accepted.geometry;var committed_offset=next_offset;
        if contact==0 && accepted.geometry.w>bounds.x && accepted.geometry.w<bounds.y && end_actual.w>bounds.x && end_actual.w<bounds.y{
            let projected_offset=end_actual.w-branch.x;
            if branch.x+projected_offset==end_actual.w{committed_geometry=end_actual;committed_offset=projected_offset;}
        }
        state=State(state.p+accepted.delta.p,accepted.delta.u);g=committed_geometry;actual_g=end_actual;path+=step;
        if contact!=0{dp_layer+=contact;departing=contact;}else{departing=0;}
        dp_height_offset=committed_offset;
        proposed=select(proposed,min(100000.0,max(proposed,step)*2.0),accepted.error<tolerance*0.125);
    }
    return trace_result(state.p,state.u,path,2u);
}
