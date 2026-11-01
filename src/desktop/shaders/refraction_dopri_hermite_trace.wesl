// Original MIT. Diagnostic one-sided DP54 controller; not a runtime shader.
// Direct dense diagnostic: one accepted polynomial serves every contact.
// The raw DP embedded estimate plus quartic/Hermite interior estimate is retained.
var<private> dp_layer:i32;
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
var<private> dp_curve:DpStep;
var<private> dp_curve_step:f32;
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
fn dp_prefix(s:State,distance:f32)->DpStep{
    var result=dp_curve;result.delta=dp_advance_delta(s,distance);result.geometry=geometry(s.p+result.delta.p);return result;
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
fn dp_contact(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,tolerance:f32)->DpContact{
    let ascending=contact_height>g.w;var low=0.0;var high=step;
    var trial=clamp(step*(contact_height-g.w)/(end.geometry.w-g.w),low,high);
    let allocation=max(0.000001,tolerance*0.125);
    var result=DpContact(-1.0,end,allocation,0u);
    if (g.w-contact_height)*(end.geometry.w-contact_height)>0.0{return result;}
    let curvature=dp_height_bound(start,g);
    if !(curvature>=0.0){return result;}
    for(var j=0;j<32;j+=1){
        let candidate=dp_prefix(start,trial);
        if !dp_finite(candidate.geometry) || !(candidate.error+allocation<=tolerance){return result;}
        let h=candidate.geometry.w;let residual=h-contact_height;
        let derivative=dot(dp_curve_derivative(trial),candidate.geometry.xyz);
        let slope=abs(derivative)-curvature*(high-low);
        let height_roundoff=max(0.000001,9.536743e-7*(length(start.p+candidate.delta.p)+abs(h)));
        if high-low<=allocation || (slope>0.0 && (abs(residual)+height_roundoff)/slope<=allocation){return DpContact(trial,candidate,allocation,1u);}
        if (h<contact_height)==ascending{low=trial;}else{high=trial;}
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
    let entry=path;var g=geometry(state.p);dp_layer=dp_select_layer(g.w,dot(state.u,g.xyz));
    var departing=0; // signed last resolved knot, zero away from a contact
    for(var i=0;i<4096;i+=1){
        let vertical=dot(state.u,g.xyz);let curvature=(settings.budget.z+settings.scene.x);
        if physical && g.w==0.0 && vertical<0.0{
            let coverage=terrain_coverage(state.p,g.xyz);
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
        let lower_gap=g.w-bounds.x;let upper_gap=bounds.y-g.w;
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
        dp_curve=accepted;dp_curve_step=step;
        let polynomial_bound=dp_height_bound(state,g);let polynomial_vertical=dot(dp_curve_derivative(0.0),g.xyz);
        let polynomial_padding=max(dp_height_uncertainty(state,g),dp_height_uncertainty(State(state.p+accepted.delta.p,accepted.delta.u),accepted.geometry));
        let lower_safe=dp_boundary_safe(g.w-bounds.x,accepted.geometry.w-bounds.x,polynomial_vertical,polynomial_bound,step,polynomial_padding,select(0,1,departing>0));
        let upper_safe=dp_boundary_safe(g.w-bounds.y,accepted.geometry.w-bounds.y,polynomial_vertical,polynomial_bound,step,polynomial_padding,select(0,-1,departing<0));
        let reference_safe=!physical || dp_boundary_safe(g.w,accepted.geometry.w,polynomial_vertical,polynomial_bound,step,polynomial_padding,0);
        if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe || !reference_safe{
            if step>0.000002{proposed=step*0.5;continue;}
            return trace_result(state.p,state.u,path,2u);
        }
        var contact=0;var allocation=0.0;
        var contact_height=0.0;
        if accepted.geometry.w>=bounds.y && g.w<bounds.y{contact_height=bounds.y;contact=1;}
        if accepted.geometry.w<=bounds.x && g.w>bounds.x{contact_height=bounds.x;contact=-1;}
        if contact!=0{
            let root=dp_contact(state,g,accepted,step,contact_height,tolerance);
            if root.valid==0u{if step>0.000002{proposed=step*0.5;continue;}return trace_result(state.p,state.u,path,2u);}
            step=root.distance;accepted=root.step;allocation=root.uncertainty;
        }
        if accepted.error+allocation>tolerance{proposed=step*0.5;continue;}
        var reference_distance=-1.0;var reference=accepted;
        if physical && ((g.w>0.0 && accepted.geometry.w<=0.0)||(g.w<0.0 && accepted.geometry.w>=0.0)){
            let root=dp_contact(state,g,accepted,step,0.0,tolerance);
            if root.valid==0u{if step>0.000002{proposed=step*0.5;continue;}return trace_result(state.p,state.u,path,2u);}
            let coverage=terrain_coverage(state.p+root.step.delta.p,root.step.geometry.xyz);
            if coverage==2u{return trace_result(state.p,state.u,path,2u);}
            if coverage==0u{reference_distance=root.distance;reference=root.step;}
        }
        var event_step=step;var event_value=accepted;
        if reference_distance>=0.0{event_step=reference_distance;event_value=reference;allocation=max(allocation,tolerance*0.125);}
        if physical{
            let end=State(state.p+event_value.delta.p,event_value.delta.u);
            let error=event_value.error+allocation;
            if error>tolerance{proposed=step*0.5;continue;}
            let event=scene_event(state,end,event_value.delta.p,event_step,error);
            if event.ambiguous!=0u{
                let floor=max(0.001,9.536743e-7*max(length(state.p),length(end.p)));
                if event_step>floor{proposed=event_step*0.5;continue;}
                return trace_result(state.p,state.u,path,2u);
            }
            if event.distance>=0.0{let hit=dp_advance(state,event.distance);return Trace(hit.p,hit.u,path+event.distance,1u,event.normal,event.kind,event.marker);}
            if reference_distance>=0.0{return trace_result(end.p,end.u,path+reference_distance,1u);}
        }
        if contact>0 && contact_height==settings.budget.y{return trace_result(state.p+accepted.delta.p,accepted.delta.u,path+step,0u);}
        state=State(state.p+accepted.delta.p,accepted.delta.u);g=accepted.geometry;path+=step;
        if contact!=0{dp_layer+=contact;departing=contact;}else{departing=0;}
        proposed=select(proposed,min(100000.0,max(proposed,step)*2.0),accepted.error<tolerance*0.125);
    }
    return trace_result(state.p,state.u,path,2u);
}
