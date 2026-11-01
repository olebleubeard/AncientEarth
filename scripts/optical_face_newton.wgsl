// Diagnostic replacement for the sign-changing face-root branch. The existing
// finite transverse, triangle-membership and owner-uncertainty tests remain.
        var low=0.0;var high=step;
        let width=max(0.000001,band*0.01);
        var trial=clamp(step*before/(before-after),low,high);
        var resolved=false;
        for(var i=0;i<24;i+=1){
            let candidate_step=dopri_delta(start,trial,dp_branch(),geometry(start.p));
            if !(candidate_step.error<=error){result.ambiguous=1u;return result;}
            let candidate=candidate_step.delta;
            let h=dot(n,p+frame_vector(candidate.p,base));
            if !dp_finite(vec4(candidate.p,h)) || !dp_finite(vec4(candidate.u,0.0)) || !(dot(candidate.u,candidate.u)>0.5){result.ambiguous=1u;return result;}
            if high-low<=width{distance=trial;root_delta=candidate.p;resolved=true;break;}
            // Confirm an actual same-branch narrow bracket, not proximity.
            let a=max(low,trial-width*0.5);let b=min(high,trial+width*0.5);
            if b>a && b-a<=width{
                let sa=dopri_delta(start,a,dp_branch(),geometry(start.p));let sb=dopri_delta(start,b,dp_branch(),geometry(start.p));
                if !(max(sa.error,sb.error)<=error){result.ambiguous=1u;return result;}
                let da=sa.delta;let db=sb.delta;
                let ha=dot(n,p+frame_vector(da.p,base));let hb=dot(n,p+frame_vector(db.p,base));
                if !dp_finite(vec4(da.p,ha)) || !dp_finite(vec4(db.p,hb)) || !(dot(da.u,da.u)>0.5 && dot(db.u,db.u)>0.5){result.ambiguous=1u;return result;}
                if (ha*before>=0.0 && hb*before<=0.0){distance=trial;root_delta=candidate.p;resolved=true;break;}
            }
            if h*before>0.0{low=trial;}else{high=trial;}
            let derivative=dot(n,frame_vector(candidate.u,base));
            let newton=trial-h/derivative;var next=(low+high)*0.5;
            if newton>low && newton<high && newton!=trial{next=newton;}
            if next==trial || !(next>low && next<high){break;}
            trial=next;
        }
        if !resolved{result.ambiguous=1u;return result;}
