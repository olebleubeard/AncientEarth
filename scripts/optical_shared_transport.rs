//! Unused CPU ray-cell experiment with separately carried groupwise remainders.
//! The full-input interval Jacobian encloses the SAME discrete RK stages.
//! Existing uncertainty is transported once, separate from fresh truncation.
//! Failure requires per-ray fallback; this report never admits visible sky.
mod optical_taylor;
mod optical_shared_jacobian;
mod optical_shared_step_jacobian;
mod optical_shared_scalar;
use optical_taylor::{Interval as I,Taylor as T};
use optical_shared_jacobian::{transport,transport_correction};
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
const GROUP:[usize;7]=[0,0,1,1,1,2,3];
const BLOCKS:[(usize,usize);4]=[(0,2),(2,5),(5,6),(6,7)];
fn norm_upper(v:impl IntoIterator<Item=f64>)->f64{
    let sum=v.into_iter().fold(I::point(0.),|s,x|s+I::point(x)*I::point(x));
    if !sum.valid(){return f64::INFINITY;}
    let root=I{lo:sum.lo.max(0.),hi:sum.hi}.sqrt();
    if root.valid(){root.hi}else{f64::INFINITY}
}
fn norm_lower(v:&[T])->f64{
    let sum=v.iter().fold(I::point(0.),|s,x|{let a=x.range().abs_lower();s+I::point(a)*I::point(a)});
    if !sum.valid(){return f64::NEG_INFINITY;}
    let root=I{lo:sum.lo.max(0.),hi:sum.hi}.sqrt();if root.valid(){root.lo}else{f64::NEG_INFINITY}
}
fn remainders(y:Y)->[f64;4]{std::array::from_fn(|b|norm_upper(y[BLOCKS[b].0..BLOCKS[b].1].iter().map(|x|x.remainder)))}
fn without_remainder(mut y:Y)->Y{for value in &mut y{value.remainder=0.;}y}
fn with_group_error(y:Y,error:[f64;4])->Y{std::array::from_fn(|i|y[i].with_extra_remainder(error[GROUP[i]]))}
fn valid_errors(v:[f64;4])->bool{v.into_iter().all(|x|x.is_finite()&&x>=0.)}
fn add_errors(a:[f64;4],b:[f64;4])->[f64;4]{
    if !valid_errors(a)||!valid_errors(b){return [f64::INFINITY;4];}
    std::array::from_fn(|i|{let sum=I::point(a[i])+I::point(b[i]);if sum.valid(){sum.hi}else{f64::INFINITY}})
}
fn positive_metric(parts:[I;3])->f64{
    if parts.iter().any(|p|!p.valid()||p.hi<0.){f64::INFINITY}
    else{parts.into_iter().map(|p|p.hi).fold(0.,f64::max)}
}
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
struct Trial{next:Y,error:[f64;4],r:[Y;4],dense_error:[f64;4],ode_error:f64,representation:f64,path_upper:f64,normal_lower:f64,vertical:f64}
fn trial(axis:[T;3],h:f64,y:Y,incoming:[f64;4],endpoint:f64,b:Branch)->Result<Trial,&'static str>{
    if y.iter().any(|x|!x.valid()||x.remainder!=0.)||incoming.iter().any(|x|!x.is_finite()||*x<0.){
        return Err("invalid separated input");
    }
    let derivative=optical_shared_step_jacobian::step(axis,h,y,incoming,endpoint,b).ok_or("full-input Jacobian tube")?;
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
    let fresh_r=[delta,r3,r4,r5];
    let r:[Y;4]=std::array::from_fn(|i|with_group_error(fresh_r[i],transport_correction(&derivative.r[i],incoming)));
    let embedded=with_group_error(embedded,transport_correction(&derivative.embedded,incoming));
    let full_y=with_group_error(y,incoming);
    if !r.iter().flatten().all(|v|v.valid())||!embedded.iter().all(|v|v.valid()){return Err("nonfinite coefficients");}
    let q_variation=upper_sum(r.iter().map(|v|abs_sum(&v[..2])));
    let u_variation=upper_sum(r.iter().map(|v|abs_sum(&v[2..5])));
    let initial_u=abs_sum(&full_y[2..5]);let v=dot([full_y[2],full_y[3],full_y[4]],normal(full_y)).range();
    let vertical=(I::point(v.lo)-(I::point(u_variation)+I::point(initial_u)*I::point(q_variation))).lo;
    let chart_extra=(I::point(abs_sum(&r5[..2]))/I::point(16.)+I::point(abs_sum(&embedded[..2]))).hi;
    let direction_extra=(I::point(abs_sum(&r5[2..5]))/I::point(16.)+I::point(abs_sum(&embedded[2..5]))).hi;
    let normal_lower=(I::point(norm_lower(&full_y[2..5]))
        -(I::point(u_variation)+I::point(direction_extra))).lo;
    let chart_bound=(I::point(abs_sum(&full_y[..2]))+I::point(q_variation)+I::point(chart_extra)).hi;
    if ![vertical,normal_lower,chart_bound].iter().all(|x|x.is_finite())||vertical<=0.01||normal_lower<=0.25||chart_bound>1.{return Err("whole-curve certificate");}
    let path_upper=(dt_interval/I::point(vertical)).hi.max(upper_sum(r.iter().map(|c|c[5].range().abs_upper())));
    if !path_upper.is_finite()||path_upper<=0.||path_upper>100000.{return Err("path step limit");}
    let p2=r[0][5].range();let p3=r[1][5].range();let p4=r[2][5].range();let p5=r[3][5].range();
    let c1=p2+p3;let c2=p4-p3+p5;let c3=-p4-I::point(2.)*p5;
    let derivatives=[c1,c1+(I::point(2.)/I::point(3.))*c2,
        c1+(I::point(4.)/I::point(3.))*c2+c3,c1+I::point(2.)*c2+I::point(3.)*c3+I::point(4.)*p5];
    if derivatives.iter().any(|d|!d.valid()||d.lo<=0.){return Err("path chronology");}
    let radius=(I::point(6399594.)+I::point(h.abs())+I::point(step)).hi;
    if !radius.is_finite()||radius<=0.{return Err("nonfinite radius");}
    let metric=|v:Y|positive_metric([I::point(radius)*I::point(abs_sum(&v[..2])),
        I::point(2.)*I::point(path_upper)*I::point(abs_sum(&v[2..5]))/I::point(normal_lower),
        I::point(abs_sum(&v[5..]))]);
    let ode_interval=I::point(metric(embedded))+I::point(metric(r[3]))/I::point(16.);
    if !ode_interval.valid()||ode_interval.hi<0.{return Err("invalid ODE metric");}
    let ode_error=ode_interval.hi;
    let mut next=std::array::from_fn(|a|y[a]+delta[a]);
    let raw_incoming=transport(&derivative.next,incoming);
    if !valid_errors(raw_incoming){return Err("invalid raw incoming radius");}
    // Both the fresh representative and every perturbed raw endpoint have
    // norm >=m. For them normalization has the Euclidean bound E_u/m.
    let m=(I::point(norm_lower(&next[2..5]))-I::point(raw_incoming[1])).lo;
    if !m.is_finite()||m<=0.25{return Err("endpoint normalization tube");}
    let direction=normalized([next[2],next[3],next[4]]).ok_or("normalization")?;
    next[2..5].copy_from_slice(&direction);
    let normalized_incoming=[raw_incoming[0],(I::point(raw_incoming[1])/I::point(m)).hi,raw_incoming[2],raw_incoming[3]];
    if !valid_errors(normalized_incoming){return Err("invalid normalized incoming radius");}
    let error=add_errors(remainders(next),normalized_incoming);
    if !valid_errors(error){return Err("invalid endpoint radius");}
    // Fresh polynomial truncation plus ONE full dense-map perturbation.
    // No separately reused full budget: ODE estimator and this entire-cell
    // representation are added below before the single acceptance test.
    let fresh_dense_rem:[f64;7]=std::array::from_fn(|a|upper_sum(fresh_r.iter().map(|v|v[a].remainder)));
    let fresh_dense:[f64;4]=std::array::from_fn(|b|norm_upper(fresh_dense_rem[BLOCKS[b].0..BLOCKS[b].1].iter().copied()));
    let dense_error=add_errors(fresh_dense,transport(&derivative.dense,incoming));
    if !valid_errors(dense_error){return Err("invalid dense radius");}
    let total_error:[f64;4]=std::array::from_fn(|i|dense_error[i].max(error[i]));
    if !valid_errors(total_error){return Err("invalid total radius");}
    let representation=positive_metric([I::point(radius)*I::point(total_error[0]),
        I::point(2.)*I::point(path_upper)*I::point(total_error[1])/I::point(normal_lower),
        I::point(total_error[2])+I::point(total_error[3])]);
    if !ode_error.is_finite()||!representation.is_finite(){return Err("nonfinite metric");}
    Ok(Trial{next:without_remainder(next),error,r:fresh_r,dense_error,ode_error,representation,path_upper,normal_lower,vertical})
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
fn run_cell(axis:[T;3],initial_h:f64,knots:&[(f64,f64)],view:&[f64],cell:[u32;4],retain_curves:bool)->Value{
    let Some(direction)=launch(view,cell)else{return json!({"cell":cell,"transport":"deferred","reason":"launch"});};
    let launch_y=[k(0.),k(0.),direction[0],direction[1],direction[2],k(0.),k(0.)];
    let mut error=remainders(launch_y);let mut y=without_remainder(launch_y);
    let initial_v=direction[1].range();
    if initial_v.lo<=0.02{return json!({"cell":cell,"transport":"deferred","reason":"initial conditioning","vertical":initial_v.lo});}
    let mut h=initial_h;let mut owner=knots.partition_point(|p|p.0<=h);
    let top=knots.last().unwrap().0;let mut proposal=2000.*initial_v.lo;
    let mut accepted=0;let mut attempted=0;let mut highest_rem=0_f64;let mut highest_total=0_f64;
    let mut last_reason="";let mut min_vertical=initial_v.lo;let mut curves=Vec::new();
    while h<top{
        attempted+=1;
        if attempted>2048||proposal<1e-7{
            return json!({"cell":cell,"transport":"deferred","reason":last_reason,"attempted":attempted,"accepted":accepted,"last_height_m":h,"maximum_representation_m":highest_rem,"partial_curves":curves});
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
        let dense=match trial(axis,h,y,error,endpoint,Branch{anchor,n,g}){Ok(v)=>v,Err(e)=>{last_reason=e;proposal=step*0.5;continue;}};
        let total=(dense.ode_error+dense.representation).next_up();
        if total>tolerance{last_reason="combined ODE + representation budget";proposal=step*0.5;continue;}
        if retain_curves{curves.push(json!({"height_start_m":h,"height_end_m":endpoint,"active_upper_node":owner,
            "source_boundary_end":reaches,"start_coefficients":y.map(|v|v.c),"dense_coefficients":dense.r.map(|v|v.map(|p|p.c)),
            "incoming_group_error":error,"dense_group_error":dense.dense_error,"normalized_endpoint_group_error":dense.error,
            "ode_metric_m":dense.ode_error,"representation_metric_m":dense.representation,"combined_metric_m":total,"tolerance_m":tolerance,
            "path_upper_m":dense.path_upper,"raw_direction_norm_lower":dense.normal_lower,"vertical_lower":dense.vertical}));}
        highest_rem=highest_rem.max(dense.representation);highest_total=highest_total.max(total);
        min_vertical=min_vertical.min(dense.vertical);y=dense.next;error=dense.error;accepted+=1;
        h=endpoint;if reaches{owner+=1;}
        proposal=if total<tolerance/32.{(proposal.max(step)*2.).min(100000.)}else{proposal.max(step)};
    }
    json!({"cell":cell,"transport":"enclosed common-schedule exit","attempted":attempted,"accepted":accepted,
        "maximum_representation_m":highest_rem,"maximum_combined_local_metric_m":highest_total,
        "minimum_vertical":min_vertical,"exit_group_error":error,"exit_direction_polynomials":y[2..5].iter().map(|v|json!({"coefficients":v.c,"shared_euclidean_remainder":error[1]})).collect::<Vec<_>>(),
        "curves":curves,
        "scene_admitted":false,"scene_reason":"early canonical scene and h0 masking certificate still required; transport success is not visible sky"})
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let args:Vec<_>=std::env::args().collect();let output=args.get(1).ok_or("output JSON")?;
    let tile:u32=args.get(2).map(|s|s.parse()).transpose()?.unwrap_or(16);
    let limit:usize=args.get(3).map(|s|s.parse()).transpose()?.unwrap_or(12);
    let retain_curves=args.iter().any(|v|v=="--curves");
    let scalar_cost=args.iter().any(|v|v=="--scalar-cost");
    let zero_gradient=args.iter().any(|v|v=="--zero-gradient");
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
    let knots:Vec<_>=input["layers"].as_array().ok_or("layers")?.iter().map(|n|(base+n["height_m"].as_f64().unwrap(),if zero_gradient{0.}else{n["refractivity"].as_f64().unwrap()})).collect();
    let width=view[16] as u32;let height=view[17] as u32;
    let cells:Vec<_>=(0..height).step_by(tile as usize).flat_map(|y|(0..width).step_by(tile as usize).map(move|x|[x,y,tile.min(width-x),tile.min(height-y)])).collect();
    let selected:Vec<_>=if limit==0||limit>=cells.len(){cells.clone()}else{(0..limit).map(|i|cells[i*(cells.len()-1)/(limit-1).max(1)]).collect()};
    let start=std::time::Instant::now();let mut results=Vec::new();
    for cell in selected{let result=run_cell(axis,h,&knots,&view,cell,retain_curves);eprintln!("cell {:?}: {} after {} attempts",cell,result["transport"],result["attempted"]);results.push(result);}
    let passed=results.iter().filter(|r|r["transport"]=="enclosed common-schedule exit").count();
    let shared_elapsed=start.elapsed().as_secs_f64();
    let cost=if scalar_cost{
        let selected_cells:Vec<[u32;4]>=results.iter().map(|r|std::array::from_fn(|i|r["cell"][i].as_u64().unwrap()as u32)).collect();
        let accepted_cells:Vec<[u32;4]>=results.iter().filter(|r|r["transport"]=="enclosed common-schedule exit")
            .map(|r|std::array::from_fn(|i|r["cell"][i].as_u64().unwrap()as u32)).collect();
        let mut records=Vec::new();
        for (label,cells) in [("all sampled cells",&selected_cells),("transport-enclosed cells only",&accepted_cells)]{
            let mut raw=Vec::new();let mut check=0.;let iterations=25;
            let mut outcome=None;
            for _ in 0..7{let start=std::time::Instant::now();
                for _ in 0..iterations{
                    let value=optical_shared_scalar::run(axis.map(|v|v.c[0]),h,&knots,&view,std::hint::black_box(cells));
                    check+=std::hint::black_box(value.checksum);outcome=Some(value);
                }
                raw.push(start.elapsed().as_secs_f64());
            }
            let outcome=outcome.unwrap();let mut ordered=raw.clone();ordered.sort_by(f64::total_cmp);
            records.push(json!({"selection":label,"cells":cells.len(),"native_pixels":cells.iter().map(|c|c[2]*c[3]).sum::<u32>(),
                "iterations_per_batch":iterations,"raw_batch_seconds":raw,"median_seconds_per_workload":ordered[3]/f64::from(iterations),
                "scalar_attempts_per_workload":outcome.attempts,"scalar_accepted_steps_per_workload":outcome.accepted,
                "scalar_exits_per_workload":outcome.exits,"scalar_deferred_per_workload":outcome.deferred,"retained_checksum":check}));
        }
        let start=std::time::Instant::now();let mut reused=0;
        for cell in &accepted_cells{let value=run_cell(axis,h,&knots,&view,*cell,false);reused+=usize::from(value["transport"]=="enclosed common-schedule exit");std::hint::black_box(value);}
        json!({"scope":"CPU operation-cost control only. Scalar rays use the same physical seven-stage formula, source ownership, proposal, conditioning and conservative height-based metre tolerance, but not angular Taylor or interval-AD work. They use floating-point acceptance checks with a 256-epsilon heuristic pad, not a rigorous roundoff certificate; this is not bit equivalence, a GPU benchmark or a scalar numerical validation. All native pixel centers in each selected cell are solved independently.",
            "scalar_records":records,"shared_accepted_only_seconds":start.elapsed().as_secs_f64(),"shared_repeated_exit_count":reused})
    }else{Value::Null};
    std::fs::write(output,serde_json::to_string_pretty(&json!({
        "schema":"ancientearth.optical-shared-transport.diagnostic.v1","scope":"Unused CPU cubic launch-cell arithmetic certificate with grouped full-input discrete-RK Jacobian transport, not an exact-ODE/global-error proof or GPU performance claim. Binary64 encoded camera tangent/axis and source samples define this finite experiment. Per-pixel f32 encoding/evaluation and early terrain/marker/h0 exclusion remain mandatory; no cell is admitted for rendering by this report.",
        "source":source_path,"dimensions":[width,height],"tile_size":tile,"curves_retained":retain_curves,"complete_frame":results.len()==cells.len(),
        "zero_gradient_control":zero_gradient,"profile_scope":if zero_gradient{"Synthetic zero-refractivity control on the original source HEIGHT knots. This is not NOAA weather; it isolates pure geometric normal-coordinate variation and uses the same finite source schedule machinery."}else{"Actual retained source profile."},
        "arithmetic_input_bits":{"view":view.iter().map(|v|format!("{:016x}",v.to_bits())).collect::<Vec<_>>(),
            "tan_half_vertical":format!("{:016x}",view[13].tan().to_bits()),"aspect_tan_half_vertical":format!("{:016x}",(view[12]*view[13].tan()).to_bits()),"one_minus_E2":format!("{:016x}",(1.-E2).to_bits()),
            "initial_height":format!("{:016x}",h.to_bits()),"axis_polynomials":axis.map(|v|json!({"coefficients":v.c.map(|x|format!("{:016x}",x.to_bits())),"remainder":format!("{:016x}",v.remainder.to_bits())})),
            "absolute_knots":knots.iter().map(|&(h,n)|[format!("{:016x}",h.to_bits()),format!("{:016x}",n.to_bits())]).collect::<Vec<_>>(),
            "A":A.iter().map(|row|row.iter().map(|x|format!("{:016x}",x.to_bits())).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "C":C.map(|x|format!("{:016x}",x.to_bits())),"D":D.map(|x|format!("{:016x}",x.to_bits())),"E":E.map(|x|format!("{:016x}",x.to_bits())),"E2":format!("{:016x}",E2.to_bits())},
        "state_order":["chart_a","chart_b","raw_u_x","raw_u_y","raw_u_z","geometric_path_m","optical_excess_m"],
        "dense_formula":"Y(theta)=Y0+theta*r0+theta*(1-theta)*(r1+theta*(r2+(1-theta)*r3)); H(theta)=Hstart+theta*(Hend-Hstart). Coefficients are cubic bivariate launch-cell polynomials. Group remainder is a Euclidean bound on chart(2), raw direction(3), path(1), excess(1). Endpoint normalization is separate.",
        "cells_total":cells.len(),"cells_tested":results.len(),"transport_enclosed_cells":passed,"scene_admitted_cells":0,
        "elapsed_seconds":shared_elapsed,"scalar_cost_control":cost,"coefficient_order":optical_taylor::POWERS,
        "cases":results}))?+"\n")?;
    Ok(())
}

#[cfg(test)]mod author_transport_checks{
    use super::*;
    #[test]
    fn discrete_endpoint_encloses_grouped_input_perturbations(){
        let axis=[k(0.),k(0.5),k(-3_f64.sqrt()/2.)];
        let direction=[0.2,0.9,-0.15_f64.sqrt()];
        let center=[k(0.),k(0.),k(direction[0]),k(direction[1]),k(direction[2]),k(100.),k(0.03)];
        let incoming=[1e-8,1e-7,1e-5,1e-8];
        let branch=Branch{anchor:0.,n:0.0003,g:k(-1e-8)};
        let enclosed=trial(axis,10.,center,incoming,30.,branch).expect("finite monotone full tube");
        for mask in 0..128{
            let mut point=center;
            for (group,&(lo,hi)) in BLOCKS.iter().enumerate(){
                let amplitude=0.99*incoming[group]/((hi-lo)as f64).sqrt();
                for (i,p) in point.iter_mut().enumerate().take(hi).skip(lo){
                    p.c[0]+=if mask&(1<<i)==0{amplitude}else{-amplitude};
                }
            }
            let direct=trial(axis,10.,point,[0.;4],30.,branch).expect("perturbed finite trial");
            for (group,&(lo,hi)) in BLOCKS.iter().enumerate(){
                let difference=norm_upper((lo..hi).map(|i|direct.next[i].c[0]-enclosed.next[i].c[0]));
                assert!(difference<=enclosed.error[group]+direct.error[group],"group {group}: {difference} {:?}",enclosed.error);
            }
        }
    }
    #[test]
    fn accumulated_radius_is_not_lost_and_invalid_tube_declines(){
        let y=[k(0.),k(0.),k(0.),k(1.),k(0.),k(0.),k(0.)];
        let axis=[k(0.),k(0.5),k(-3_f64.sqrt()/2.)];let branch=Branch{anchor:0.,n:0.0003,g:k(0.)};
        let trial=trial(axis,0.,y,[1e-10,1e-10,1e-4,1e-5],10.,branch).unwrap();
        assert!(trial.error[2]>=1e-4&&trial.error[3]>=1e-5);
        assert!(super::trial(axis,0.,y,[0.,1.,0.,0.],10.,branch).is_err());
        assert!(super::trial(axis,0.,y,[0.,f64::NAN,0.,0.],10.,branch).is_err());
    }
}
#[cfg(test)]
#[path="optical_shared_transport_review.rs"]
mod independent_transport_review;
