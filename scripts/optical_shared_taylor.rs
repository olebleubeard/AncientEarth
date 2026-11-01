//! Unused CPU common-schedule ray-cell experiment. No renderer registration.
//! A cubic launch-cell polynomial carries its interval remainder through every
//! DP stage, normalization and source boundary. Failure means exact-ray
//! fallback is REQUIRED; no failed cell is presented as an accepted sky cell.
mod optical_taylor;
use optical_taylor::{Interval as I,Taylor as T};
use serde_json::{Value,json};
type Y=[T;7]; // chart a,b, local u.xyz, accumulated path, optical excess.
const A:[&[f64];7]=[&[],&[1./5.],&[3./40.,9./40.],
    &[44./45.,-56./15.,32./9.],&[19372./6561.,-25360./2187.,64448./6561.,-212./729.],
    &[9017./3168.,-355./33.,46732./5247.,49./176.,-5103./18656.],
    &[35./384.,0.,500./1113.,125./192.,-2187./6784.,11./84.]];
const C:[f64;7]=[0.,1./5.,3./10.,4./5.,8./9.,1.,1.];
const E:[f64;7]=[71./57600.,0.,-71./16695.,71./1920.,-17253./339200.,22./525.,-1./40.];
const D:[f64;7]=[-12715105075./11282082432.,0.,87487479700./32700410799.,
    -10690763975./1880347072.,701980252875./199316789632.,-1453857185./822651844.,69997945./29380423.];
const E2:f64=0.0066943799901413165;
fn k(x:f64)->T{T::constant(x)}
fn upper_sum(v:impl IntoIterator<Item=f64>)->f64{v.into_iter().fold(0.,|a,b|(a+b).next_up())}
fn abs_sum(v:&[T])->f64{upper_sum(v.iter().map(|x|x.range().abs_upper()))}
fn dot(a:[T;3],b:[T;3])->T{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn normal(y:Y)->[T;3]{
    let scale=(k(1.)+y[0]*y[0]+y[1]*y[1]).inverse_sqrt();
    [y[0]*scale,scale,-y[1]*scale]
}
fn normalized(v:[T;3])->Option<[T;3]>{
    let scale=dot(v,v).inverse_sqrt();let out=v.map(|x|x*scale);
    out.iter().all(|x|x.valid()).then_some(out)
}
#[derive(Clone,Copy)]struct Branch{anchor:f64,n:f64,g:T}
fn rhs(axis:[T;3],h:T,y:Y,b:Branch)->Option<Y>{
    if !y.iter().all(|v|v.valid())||abs_sum(&y[..2])>1.{return None;}
    let n=normal(y);let u=[y[2],y[3],y[4]];let v=dot(u,n);
    if !v.valid()||v.range().lo<=0.01{return None;}
    let inverse_v=v.reciprocal();let z=dot(n,axis);let d=k(1.)-k(E2)*z*z;
    let nr=k(6378137.)*d.inverse_sqrt();let mr=nr*k(1.-E2)/d;
    let refractivity=k(b.n)+b.g*(h-k(b.anchor));let index=k(1.)+refractivity;
    if !index.valid()||index.range().lo<0.5{return None;}
    let coefficient=nr*k(E2)/(d*(mr+h)*(nr+h));
    let meridian=std::array::from_fn(|i|axis[i]-n[i]*z);
    let meridian_dot=dot(meridian,u);
    let dn: [T;3]=std::array::from_fn(|i|(u[i]-n[i]*v)/(nr+h)+meridian[i]*coefficient*meridian_dot);
    let bend=b.g/index;
    let out=[(dn[0]-y[0]*dn[1])/n[1]*inverse_v,
        (-dn[2]-y[1]*dn[1])/n[1]*inverse_v,
        bend*(n[0]*inverse_v-u[0]),bend*(n[1]*inverse_v-u[1]),bend*(n[2]*inverse_v-u[2]),
        inverse_v,refractivity*inverse_v];
    out.iter().all(|x|x.valid()).then_some(out)
}
struct Trial{next:Y,r:[Y;4],ode_error:f64,representation:f64,path_upper:f64,vertical:f64}
fn trial(axis:[T;3],h:f64,y:Y,endpoint:f64,b:Branch)->Result<Trial,&'static str>{
    let mut stages=[[k(0.);7];7];
    // The source endpoint itself is a shared prescribed height. Both basic
    // subtraction and affine arithmetic are enclosed. The stored binary64
    // Butcher coefficients define this finite numerical map; these are not
    // claimed to be exact rational coefficients.
    let dt_interval=I::point(endpoint)-I::point(h);
    let dt=T::interval(dt_interval);
    let step=dt_interval.hi;
    if !(dt_interval.lo>0.){return Err("nonpositive prescribed height interval");}
    for i in 0..7{
        let mut yi=y;
        for j in 0..i{for a in 0..7{yi[a]=yi[a]+dt*k(A[i][j])*stages[j][a];}}
        stages[i]=rhs(axis,k(h)+dt*k(C[i]),yi,b).ok_or("stage/domain/conditioning")?;
    }
    let delta:Y=std::array::from_fn(|a|dt*A[6].iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let r3=std::array::from_fn(|a|dt*stages[0][a]-delta[a]);
    let r4=std::array::from_fn(|a|k(2.)*delta[a]-dt*(stages[0][a]+stages[6][a]));
    let r5:Y=std::array::from_fn(|a|dt*D.iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let embedded:Y=std::array::from_fn(|a|dt*E.iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let r=[delta,r3,r4,r5];
    if !r.iter().flatten().all(|v|v.valid())||!embedded.iter().all(|v|v.valid()){return Err("nonfinite coefficients");}
    let q_variation=upper_sum(r.iter().map(|v|abs_sum(&v[..2])));
    let u_variation=upper_sum(r.iter().map(|v|abs_sum(&v[2..5])));
    let initial_u=abs_sum(&y[2..5]);let v=dot([y[2],y[3],y[4]],normal(y)).range();
    let vertical=(I::point(v.lo)-(I::point(u_variation)+I::point(initial_u)*I::point(q_variation))).lo;
    let chart_extra=(I::point(abs_sum(&r5[..2]))/I::point(16.)+I::point(abs_sum(&embedded[..2]))).hi;
    let direction_extra=(I::point(abs_sum(&r5[2..5]))/I::point(16.)+I::point(abs_sum(&embedded[2..5]))).hi;
    let normal_lower=(I::point(y[2..5].iter().map(|u|u.range().abs_lower()).fold(0.,f64::max))
        -(I::point(u_variation)+I::point(direction_extra))).lo;
    let chart_bound=(I::point(abs_sum(&y[..2]))+I::point(q_variation)+I::point(chart_extra)).hi;
    if vertical<=0.01||normal_lower<=0.25||chart_bound>1.{return Err("whole-curve certificate");}
    let path_upper=(dt_interval/I::point(vertical)).hi.max(upper_sum(r.iter().map(|c|c[5].range().abs_upper())));
    if path_upper>100000.{return Err("path step limit");}
    let p2=r[0][5].range();let p3=r[1][5].range();let p4=r[2][5].range();let p5=r[3][5].range();
    let c1=p2+p3;let c2=p4-p3+p5;let c3=-p4-I::point(2.)*p5;
    let derivatives=[c1,c1+(I::point(2.)/I::point(3.))*c2,
        c1+(I::point(4.)/I::point(3.))*c2+c3,c1+I::point(2.)*c2+I::point(3.)*c3+I::point(4.)*p5];
    if derivatives.iter().any(|d|!d.valid()||d.lo<=0.){return Err("path chronology");}
    let radius=(I::point(6399594.)+I::point(h.abs())+I::point(step)).hi;
    let metric=|v:Y| (I::point(radius)*I::point(abs_sum(&v[..2]))).hi
        .max((I::point(2.)*I::point(path_upper)*I::point(abs_sum(&v[2..5]))/I::point(normal_lower)).hi)
        .max(abs_sum(&v[5..]));
    let ode_error=(I::point(metric(embedded))+I::point(metric(r5))/I::point(16.)).hi;
    let mut next=std::array::from_fn(|a|y[a]+delta[a]);
    let direction=normalized([next[2],next[3],next[4]]).ok_or("normalization")?;
    next[2..5].copy_from_slice(&direction);
    // Include the whole dense polynomial, not only its endpoint. Each basis
    // factor is at most one in magnitude on [0,1]. Normalized endpoint
    // remainder is also retained before the next trial.
    let dense_rem:[f64;7]=std::array::from_fn(|a|upper_sum(std::iter::once(y[a].remainder)
        .chain(r.iter().map(|v|v[a].remainder))).max(next[a].remainder));
    let representation=(I::point(radius)*I::point(upper_sum(dense_rem[..2].iter().copied()))).hi
        .max((I::point(2.)*I::point(path_upper)*I::point(upper_sum(dense_rem[2..5].iter().copied()))/I::point(normal_lower)).hi)
        .max(upper_sum(dense_rem[5..].iter().copied()));
    if !ode_error.is_finite()||!representation.is_finite(){return Err("nonfinite metric");}
    Ok(Trial{next,r,ode_error,representation,path_upper,vertical})
}
fn launch(view:&[f64],cell:[u32;4])->Option<[T;3]>{
    if view[14]>0.5{return None;} // first finite experiment is rectilinear only
    let [x,y,w,h]=cell;
    // Pixel-center endpoints. The continuum enclosure contains every native
    // pixel in the cell; this is not one ray reused for several pixels.
    let px=T::variable(0,f64::from(x)+0.5*f64::from(w),0.5*f64::from(w-1));
    let py=T::variable(1,f64::from(y)+0.5*f64::from(h),0.5*f64::from(h-1));
    let sx=(k(2.)*px/k(view[16])-k(1.))*k(view[12]*view[13].tan());
    let sy=(k(1.)-k(2.)*py/k(view[17]))*k(view[13].tan());
    normalized(std::array::from_fn(|i|k(view[i])*sx+k(view[4+i])*sy+k(view[8+i])))
}
fn run_cell(axis:[T;3],initial_h:f64,knots:&[(f64,f64)],view:&[f64],cell:[u32;4])->Value{
    let Some(direction)=launch(view,cell)else{return json!({"cell":cell,"transport":"deferred","reason":"launch"});};
    let mut y=[k(0.),k(0.),direction[0],direction[1],direction[2],k(0.),k(0.)];
    let initial_v=direction[1].range();
    if initial_v.lo<=0.02{return json!({"cell":cell,"transport":"deferred","reason":"initial conditioning","vertical":initial_v.lo});}
    let mut h=initial_h;let mut owner=knots.partition_point(|p|p.0<=h);
    let top=knots.last().unwrap().0;let mut proposal=2000.*initial_v.lo;
    let mut accepted=0;let mut attempted=0;let mut highest_rem=0_f64;let mut highest_total=0_f64;
    let mut last_reason="";let mut min_vertical=initial_v.lo;
    while h<top{
        attempted+=1;
        if attempted>2048||proposal<1e-7{
            return json!({"cell":cell,"transport":"deferred","reason":last_reason,"attempted":attempted,"accepted":accepted,"last_height_m":h,"maximum_representation_m":highest_rem});
        }
        let (anchor,n,g)=if owner==0{(knots[0].0,knots[0].1,k(0.))}else{
            let (lo,n)=knots[owner-1];let (hi,n1)=knots[owner];
            (lo,n,T::interval((I::point(n1)-I::point(n))/(I::point(hi)-I::point(lo))))};
        let target=knots[owner].0;let endpoint=(h+proposal).min(target);
        let step=endpoint-h;let reaches=endpoint==target;
        // Signed distance is1-Lipschitz, so |H-Heye| is a lower bound on
        // eye displacement. This tolerance is no larger than any pixel's
        // existing max(2mm,4e-7*localdistance) tolerance.
        let tolerance=0.002_f64.max(4e-7*(h-initial_h).abs());
        let dense=match trial(axis,h,y,endpoint,Branch{anchor,n,g}){Ok(v)=>v,Err(e)=>{last_reason=e;proposal=step*0.5;continue;}};
        let total=(dense.ode_error+dense.representation).next_up();
        if total>tolerance{last_reason="combined ODE + representation budget";proposal=step*0.5;continue;}
        highest_rem=highest_rem.max(dense.representation);highest_total=highest_total.max(total);
        min_vertical=min_vertical.min(dense.vertical);y=dense.next;accepted+=1;
        h=endpoint;if reaches{owner+=1;}
        proposal=if total<tolerance/32.{(proposal.max(step)*2.).min(100000.)}else{proposal.max(step)};
    }
    json!({"cell":cell,"transport":"enclosed common-schedule exit","attempted":attempted,"accepted":accepted,
        "maximum_representation_m":highest_rem,"maximum_combined_local_metric_m":highest_total,
        "minimum_vertical":min_vertical,"exit_direction_polynomials":y[2..5].iter().map(|v|json!({"coefficients":v.c,"remainder":v.remainder})).collect::<Vec<_>>(),
        "scene_admitted":false,"scene_reason":"early canonical scene and h0 masking certificate still required; transport success is not visible sky"})
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args().collect();let output=args.get(1).ok_or("output JSON")?;
    let tile:u32=args.get(2).map(|s|s.parse()).transpose()?.unwrap_or(16);
    let limit:usize=args.get(3).map(|s|s.parse()).transpose()?.unwrap_or(12);
    assert!((1..=64).contains(&tile));
    let source_path="artifacts/optical-height-performance-conditioning.json";
    let source:Value=serde_json::from_slice(&std::fs::read(source_path)?)?;
    let fixture=&source["fixture"];let clear=&fixture["clear_ray_fixture"];
    let view:Vec<f64>=fixture["view_values"].as_array().ok_or("view")?.iter().map(|v|v.as_f64().unwrap()).collect();
    let h=clear["eye_height_m"].as_f64().unwrap();
    let encoded=clear["encoded_uniform"][0].as_array().unwrap();
    let sin=encoded[0].as_f64().unwrap();let cos=encoded[1].as_f64().unwrap();
    let axis=normalized([k(0.),k(sin),k(-cos)]).ok_or("axis")?;
    let input=&clear["cases"][0]["canonical_inputs"];
    let base=input["profile_base_height_m"].as_f64().unwrap_or(0.);
    let knots:Vec<_>=input["layers"].as_array().ok_or("layers")?.iter().map(|n|(base+n["height_m"].as_f64().unwrap(),n["refractivity"].as_f64().unwrap())).collect();
    let width=view[16] as u32;let height=view[17] as u32;
    let cells:Vec<_>=(0..height).step_by(tile as usize).flat_map(|y|(0..width).step_by(tile as usize).map(move|x|[x,y,tile.min(width-x),tile.min(height-y)])).collect();
    let selected:Vec<_>=if limit==0||limit>=cells.len(){cells.clone()}else{(0..limit).map(|i|cells[i*(cells.len()-1)/(limit-1).max(1)]).collect()};
    let start=std::time::Instant::now();let mut results=Vec::new();
    for cell in selected{let result=run_cell(axis,h,&knots,&view,cell);eprintln!("cell {:?}: {} after {} attempts",cell,result["transport"],result["attempted"]);results.push(result);}
    let passed=results.iter().filter(|r|r["transport"]=="enclosed common-schedule exit").count();
    std::fs::write(output,serde_json::to_string_pretty(&json!({
        "schema":"ancientearth.optical-shared-taylor.diagnostic.v1","scope":"Unused CPU cubic launch-cell arithmetic certificate, not an exact-ODE/global-error proof or GPU performance claim. Binary64 encoded camera tangent/axis and source samples define this finite experiment. Per-pixel f32 encoding/evaluation and early terrain/marker/h0 exclusion remain mandatory; no cell is admitted for rendering by this report.",
        "source":source_path,"dimensions":[width,height],"tile_size":tile,"complete_frame":results.len()==cells.len(),
        "cells_total":cells.len(),"cells_tested":results.len(),"transport_enclosed_cells":passed,"scene_admitted_cells":0,
        "elapsed_seconds":start.elapsed().as_secs_f64(),"coefficient_order":optical_taylor::POWERS,
        "cases":results}))?+"\n")?;
    Ok(())
}
