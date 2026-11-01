// Original MIT. Diagnostic one-sided DP54 controller; not a runtime shader.
// Integer branch ownership persists across resolved layer contacts. The field
// and every scene-root reintegration use the same branch throughout a trial.
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
fn dp_advance_delta(s:State,step:f32)->State{return dopri_delta(s,step,dp_branch(),geometry(s.p)).delta;}
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
// Shampine's position extension proposes a root only. Every accepted event
// below is confirmed by the one-sided DP integrator in a narrow path bracket.
fn dp_dense_point(s:State,end:DpStep,step:f32,distance:f32)->vec3<f32>{
    let t=distance/step;let v=1.0-t;
    return s.p+t*(end.delta.p+v*(end.r3+t*(end.r4+v*end.r5)));
}
fn dp_dense_derivative(end:DpStep,step:f32,distance:f32)->vec3<f32>{
    let t=distance/step;
    return (end.delta.p+(1.0-2.0*t)*end.r3+(2.0*t-3.0*t*t)*end.r4+(2.0*t-6.0*t*t+4.0*t*t*t)*end.r5)/step;
}
fn dp_dense_candidate(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,allocation:f32)->f32{
    var low=0.0;var high=step;let ascending=contact_height>g.w;
    var trial=clamp(step*(contact_height-g.w)/(end.geometry.w-g.w),low,high);
    for(var j=0;j<24;j+=1){
        let at=geometry(dp_dense_point(start,end,step,trial));let residual=at.w-contact_height;
        if high-low<=allocation*0.25 || residual==0.0{return trial;}
        if (at.w<contact_height)==ascending{low=trial;}else{high=trial;}
        let derivative=dot(dp_dense_derivative(end,step,trial),at.xyz);
        let newton=trial-residual/derivative;var next=(low+high)*0.5;
        if newton>low && newton<high && newton!=trial{next=newton;}
        if next==trial{return trial;}trial=next;
    }
    return trial;
}
struct DpContact{distance:f32,step:DpStep,uncertainty:f32,valid:u32};
fn dp_contact(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,tolerance:f32)->DpContact{
    let ascending=contact_height>g.w;var low=0.0;var high=step;
    var trial=clamp(step*(contact_height-g.w)/(end.geometry.w-g.w),low,high);
    let allocation=max(0.000001,tolerance*0.125);
    var result=DpContact(-1.0,end,allocation,0u);
    // Confirm a crossing rather than assuming endpoint proximity proves one.
    if (g.w-contact_height)*(end.geometry.w-contact_height)>0.0{return result;}
    let guess=dp_dense_candidate(start,g,end,step,contact_height,allocation);
    let left=max(0.0,guess-allocation*0.5);let right=min(step,guess+allocation*0.5);
    if right>left && right-left<=allocation{
        let a=dopri_delta(start,left,dp_branch(),g);let b=dopri_delta(start,right,dp_branch(),g);
        let bracket=select(a.geometry.w>=contact_height && b.geometry.w<=contact_height,a.geometry.w<=contact_height && b.geometry.w>=contact_height,ascending);
        if bracket && max(a.error,b.error)+allocation<=tolerance{
            let candidate=dopri_delta(start,guess,dp_branch(),g);
            if candidate.error+allocation<=tolerance{return DpContact(guess,candidate,allocation,1u);}
        }
    }
    if guess>low && guess<high{trial=guess;}

    for(var j=0;j<32;j+=1){
        let candidate=dopri_delta(start,trial,dp_branch(),g);
        if candidate.error>=1e29{return result;}
        let h=candidate.geometry.w;let residual=h-contact_height;
        let derivative=dot(candidate.delta.u,candidate.geometry.xyz);
        let slope=abs(derivative)-(settings.budget.z+settings.scene.x)*(high-low);
        let height_roundoff=max(0.000001,9.536743e-7*(length(start.p+candidate.delta.p)+abs(h)));
        if high-low<=allocation || (slope>0.0 && (abs(residual)+height_roundoff)/slope<=allocation){
            return DpContact(trial,candidate,allocation,1u);
        }
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
