//! CPU-only common-schedule curvature/cost screen. NO production admission.
//! The bounds describe a smooth exact-real discrete RK map of the encoded
//! input/coefficients. They do not by themselves bound ODE truncation, actual
//! floating per-pixel solves, exact-line replacement, or scene visibility.
mod optical_taylor;
mod optical_second_variation;
mod optical_deviation_bounds;
use optical_deviation_bounds::{State,position_delta,recenter,rhs};
use optical_second_variation::NormJet as J;
use optical_taylor::Interval as I;
use serde_json::{Value,json};
use std::{fs,path::Path,time::Instant};
const A:[&[f64];7]=[&[],&[1./5.],&[3./40.,9./40.],
    &[44./45.,-56./15.,32./9.],&[19372./6561.,-25360./2187.,64448./6561.,-212./729.],
    &[9017./3168.,-355./33.,46732./5247.,49./176.,-5103./18656.],
    &[35./384.,0.,500./1113.,125./192.,-2187./6784.,11./84.]];
const C:[f64;7]=[0.,1./5.,3./10.,4./5.,8./9.,1.,1.];
const E:[f64;7]=[71./57600.,0.,-71./16695.,71./1920.,-17253./339200.,22./525.,-1./40.];
const D:[f64;7]=[-12715105075./11282082432.,0.,87487479700./32700410799.,
    -10690763975./1880347072.,701980252875./199316789632.,-1453857185./822651844.,69997945./29380423.];
fn c(v:f64)->J{J::point(v)}
fn sum(v:impl Iterator<Item=J>)->J{v.fold(c(0.),|a,b|a+b)}
fn abs_sum(v:impl Iterator<Item=J>)->I{v.fold(I::point(0.),|a,b|a+I::point(b.value.abs_upper()))}
#[derive(Clone,Copy)]
pub struct Pair{pub whole:State,pub center:State}
fn paired_tighten(pair:Pair)->Option<Pair>{
    let mut out=pair;
    for i in 0..10{out.whole[i]=recenter(out.whole[i],out.center[i].value)?;}
    Some(out)
}
#[derive(Clone,Copy)]
struct Trial{next:Pair,center_estimate:f64,whole_estimate:f64}
fn trial(axis:[J;3],launch:[J;3],center_launch:[J;3],h:f64,end:f64,y:Pair,branch:(f64,f64,f64))->Option<Trial>{
    let step=I::point(end)-I::point(h);
    if !step.valid()||step.lo<=0.{return None;}
    let dt=J::constant_interval(step);
    let mut ks=[[c(0.);10];7];let mut kc=ks;
    for i in 0..7{
        let pair=Pair{
            whole:std::array::from_fn(|k|y.whole[k]+dt*sum(A[i].iter().enumerate().map(|(j,a)|c(*a)*ks[j][k]))),
            center:std::array::from_fn(|k|y.center[k]+dt*sum(A[i].iter().enumerate().map(|(j,a)|c(*a)*kc[j][k])))};
        let pair=paired_tighten(pair)?;
        let height=if i==0{c(h)}else if C[i]==1.{c(end)}else{c(h)+dt*c(C[i])};
        let n=c(branch.1)+c(branch.2)*(height-c(branch.0));
        ks[i]=rhs(axis,launch,height,n,c(branch.2),pair.whole)?;
        kc[i]=rhs(axis,center_launch,height,n,c(branch.2),pair.center)?;
    }
    let parts=|stages:&[State;7]|{
        let delta:State=std::array::from_fn(|k|dt*sum(A[6].iter().enumerate().map(|(j,a)|c(*a)*stages[j][k])));
        let embedded:State=std::array::from_fn(|k|dt*sum(E.iter().enumerate().map(|(j,a)|c(*a)*stages[j][k])));
        let r5:State=std::array::from_fn(|k|dt*sum(D.iter().enumerate().map(|(j,a)|c(*a)*stages[j][k])));
        (delta,embedded,r5)
    };
    let (dw,ew,rw)=parts(&ks);let (dc,ec,rc)=parts(&kc);
    if !dw.iter().chain(ew.iter()).chain(rw.iter()).chain(dc.iter()).chain(ec.iter()).chain(rc.iter()).all(|v|v.valid()){return None;}
    let path=dc[2].value+dc[8].value;
    let whole_path=dw[2].value+dw[8].value;
    if ![path,whole_path].iter().all(|p|p.valid()&&p.lo>0.&&p.hi<=100000.){return None;}
    let radius=I::point(6399594.)+I::point(h.abs())+step;
    let metric=|v:State,path_upper:f64|->f64{
        let p=radius*(abs_sum(v[..2].iter().copied())+abs_sum(v[3..5].iter().copied()));
        let u=I::point(2.)*I::point(path_upper)*abs_sum(v[5..8].iter().copied());
        let optical=abs_sum([v[2],v[8],v[9]].into_iter());
        if ![p,u,optical].iter().all(|v|v.valid()){f64::INFINITY}else{p.hi.max(u.hi).max(optical.hi)}
    };
    let estimate=|e,r,p| (I::point(metric(e,p))+I::point(metric(r,p))/I::point(16.)).hi;
    let center_estimate=estimate(ec,rc,path.hi);let whole_estimate=estimate(ew,rw,whole_path.hi);
    if !center_estimate.is_finite()||!whole_estimate.is_finite(){return None;}
    let next=paired_tighten(Pair{
        whole:std::array::from_fn(|i|y.whole[i]+dw[i]),
        center:std::array::from_fn(|i|y.center[i]+dc[i])})?;
    Some(Trial{next,center_estimate,whole_estimate})
}

/// Coherent real angular cell: normalize an affine raw camera vector. Raw
/// affine coefficients are retained, so pixel arithmetic/encoding differences
/// cannot be silently substituted into this exact-function certificate.
pub fn launch_cell(view:&[f64],cell:[u32;4])->Option<([J;3],[J;3],[[f64;3];3])>{
    if view.len()!=20||view[14]!=0.||cell[2]<2||cell[3]<2{return None;}
    let halfx=f64::from(cell[2]-1)*0.5;let halfy=f64::from(cell[3]-1)*0.5;
    let x=f64::from(cell[0])+halfx;let y=f64::from(cell[1])+halfy;
    let scale=view[13].tan();let sx=(2.*(x+0.5)/view[16]-1.)*view[12]*scale;
    let sy=(1.-2.*(y+0.5)/view[17])*scale;
    let dx=2.*halfx/view[16]*view[12]*scale;let dy=-2.*halfy/view[17]*scale;
    let coefficients=std::array::from_fn::<_,3,_>(|i|[view[i]*sx+view[4+i]*sy+view[8+i],view[i]*dx,view[4+i]*dy]);
    let raw=coefficients.map(|v|J::affine(v[0],[v[1],v[2]]));
    let center=coefficients.map(|v|c(v[0]));
    let unit=|v:[J;3]|{let n=sum(v.into_iter().map(|x|x*x)).sqrt();v.map(|x|x/n)};
    let whole=unit(raw);let center=unit(center);
    whole.iter().chain(center.iter()).all(|v|v.valid()).then_some((whole,center,coefficients))
}

pub fn run(axis:[f64;3],initial:f64,knots:&[(f64,f64)],view:&[f64],cell:[u32;4])->Value{
    let Some((launch,center_launch,coefficients))=launch_cell(view,cell)else{return json!({"cell":cell,"exit":false,"reason":"launch representation"});};
    let axis=axis.map(c);let mut y=Pair{whole:[c(0.);10],center:[c(0.);10]};
    let mut h=initial;let mut owner=knots.partition_point(|v|v.0<=h);
    let top=knots.last().unwrap().0;let mut proposal=2000.*center_launch[1].value.lo;
    let mut attempts=0;let mut accepted=0;let mut domain_rejections=0;let mut estimator_rejections=0;
    let mut rows=Vec::new();let mut max_whole_estimate=0_f64;let mut max_center_estimate=0_f64;
    let mut reason="attempt budget";let mut completed_top=false;let clock=Instant::now();
    if launch[1].value.lo<=0.02||owner>=knots.len(){reason="whole-cell initial conditioning";}
    else {while h<top&&attempts<2048{
        attempts+=1;let target=knots[owner].0;let end=(h+proposal).min(target);
        if !(end>h){reason="no represented height progress";break;}
        let branch=if owner==0{(knots[0].0,knots[0].1,0.)}else{
            let (lo,n)=knots[owner-1];let (hi,n1)=knots[owner];(lo,n,(n1-n)/(hi-lo))};
        let Some(t)=trial(axis,launch,center_launch,h,end,y,branch)else{domain_rejections+=1;proposal=(end-h)*0.5;continue;};
        // Center-only schedule selection is an explicitly incomplete local
        // error gate. Whole-cell estimator bounds are retained below, never
        // promoted merely because the center passes.
        if t.center_estimate>1e-5{estimator_rejections+=1;proposal=(end-h)*0.5;continue;}
        let step=end-h;y=t.next;h=end;accepted+=1;
        max_whole_estimate=max_whole_estimate.max(t.whole_estimate);
        max_center_estimate=max_center_estimate.max(t.center_estimate);
        if end==target{
            let Some(position)=position_delta(axis,c(h),[y.whole[0],y.whole[1]],[y.whole[3],y.whole[4]])else{reason="physical correction bound";break;};
            let bounds:Vec<_>=position.iter().chain(y.whole[5..10].iter()).map(|v|json!({"value":[v.value.lo,v.value.hi],"gradient":v.gradient,"bilinear_error":v.hessian})).collect();
            rows.push(json!({"height":h,"correction_bounds_xyz_du_path_excess":bounds,"center_states":y.center.map(|v|[v.value.lo,v.value.hi])}));
            owner+=1;completed_top=owner==knots.len();
        }
        proposal=if t.center_estimate<1e-5/32.{(proposal.max(step)*2.).min(100000.)}else{proposal.max(step)};
    }}
    if completed_top{reason="source top";}
    json!({"cell":cell,"exit":completed_top,"reason":reason,"attempts":attempts,"accepted":accepted,
        "domain_rejections":domain_rejections,"estimator_rejections":estimator_rejections,"last_height":h,
        "cpu_seconds":clock.elapsed().as_secs_f64(),"raw_launch_affine_bits":coefficients.map(|v|v.map(f64::to_bits)),
        "max_accepted_center_estimator_m":max_center_estimate,"max_accepted_whole_estimator_m":max_whole_estimate,
        "source_states":rows,"certified_admission":false})
}

fn main(){
    let prefix=std::env::args().nth(1).expect("new output prefix");assert!(!Path::new(&format!("{prefix}.json")).exists());
    let d:Value=serde_json::from_slice(&fs::read("artifacts/optical-deviation-stencil-progress-first.json").unwrap()).unwrap();
    let bits=&d["input_bits"];let axis=std::array::from_fn(|i|f64::from_bits(bits["axis"][i].as_u64().unwrap()));
    let initial=f64::from_bits(bits["height"].as_u64().unwrap());
    let knots:Vec<_>=bits["knots"].as_array().unwrap().iter().map(|v|(f64::from_bits(v[0].as_u64().unwrap()),f64::from_bits(v[1].as_u64().unwrap()))).collect();
    let view:Vec<_>=bits["view"].as_array().unwrap().iter().map(|v|f64::from_bits(v.as_u64().unwrap())).collect();
    let cells:Vec<_>=d["cases"].as_array().unwrap().iter().filter(|v|v["cell"][2].as_u64().unwrap()<=8).map(|v|std::array::from_fn(|i|v["cell"][i].as_u64().unwrap() as u32)).collect();
    let cases:Vec<_>=cells.into_iter().map(|cell|run(axis,initial,&knots,&view,cell)).collect();
    fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({"schema":"ancientearth.deviation-flow-bounds.v1",
        "scope":"Unused CPU common-schedule exact-real discrete-RK value/first/second angular derivative bounds and one-pass operation-cost screen. Center-only numerical selection is NOT a whole-cell physical solver gate. Reported Hessians bound real discrete-map bilinear interpolation, not ODE global error or actual f32 consumers. Whole-cell numerical, geometric-baseline, encoding/evaluation and scene charges remain unapproved. No certified admission or GPU/native performance claim.",
        "input":"artifacts/optical-deviation-stencil-progress-first.json","input_bits":bits,"cases":cases,"certified_admitted_cells":0})).unwrap()).unwrap();
}
