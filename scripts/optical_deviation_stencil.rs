//! Unregistered CPU cost/rejection screen for geometric-factored sharing.
//! This file does NOT certify interpolation, ODE global error, scene freedom,
//! or a GPU result. The four-corner stencil cannot admit a cell by sampling.
//! A separate whole-cell second-variation certificate is required.
#[path = "optical_refractive_deviation.rs"]
mod deviation;
mod optical_shared_scalar;
use earth_math::{WGS84_A_M, WGS84_E2};
use serde_json::{Value, json};
use std::{fs, hint::black_box, path::Path, time::Instant};

// The same encoded Dormand--Prince coefficients as the retained scalar cost
// control. This experiment does not change the production integrator.
const A:[&[f64];7]=[&[],&[1./5.],&[3./40.,9./40.],
    &[44./45.,-56./15.,32./9.],&[19372./6561.,-25360./2187.,64448./6561.,-212./729.],
    &[9017./3168.,-355./33.,46732./5247.,49./176.,-5103./18656.],
    &[35./384.,0.,500./1113.,125./192.,-2187./6784.,11./84.]];
const C:[f64;7]=[0.,1./5.,3./10.,4./5.,8./9.,1.,1.];
const E:[f64;7]=[71./57600.,0.,-71./16695.,71./1920.,-17253./339200.,22./525.,-1./40.];
const D:[f64;7]=[-12715105075./11282082432.,0.,87487479700./32700410799.,
    -10690763975./1880347072.,701980252875./199316789632.,-1453857185./822651844.,69997945./29380423.];
const E2:f64=WGS84_E2;
// q_geometric.xy, geometric path, dq.xy, du.xyz, path delta, optical excess.
type Y=[f64;10];
fn dot(a:[f64;3],b:[f64;3])->f64{a.into_iter().zip(b).map(|(x,y)|x*y).sum()}
fn l1(a:&[f64])->f64{a.iter().map(|v|v.abs()).sum()}
fn norm(a:[f64;3])->f64{dot(a,a).sqrt()}
fn unit(a:[f64;3])->[f64;3]{let n=norm(a);a.map(|v|v/n)}
fn normal(q:[f64;2])->[f64;3]{let s=(1.+q[0]*q[0]+q[1]*q[1]).sqrt();[q[0]/s,1./s,-q[1]/s]}
fn rhs(axis:[f64;3],launch:[f64;3],h:f64,y:Y,b:(f64,f64,f64))->Option<Y>{
    let n=b.1+b.2*(h-b.0);
    let d=deviation::rhs(axis,launch,h,n,b.2,deviation::Deviation{
        geometric_chart:[y[0],y[1]],chart_delta:[y[3],y[4]],direction_delta:[y[5],y[6],y[7]]})?;
    let v=dot(launch,normal([y[0],y[1]]));
    let out=[d.geometric_chart[0],d.geometric_chart[1],1./v,d.chart_delta[0],d.chart_delta[1],
        d.direction_delta[0],d.direction_delta[1],d.direction_delta[2],d.path_delta,d.optical_excess];
    out.iter().all(|v|v.is_finite()).then_some(out)
}
fn trial(axis:[f64;3],launch:[f64;3],h:f64,y:Y,end:f64,b:(f64,f64,f64))->Option<(Y,f64)>{
    let dt=end-h;if !dt.is_finite()||dt<=0.{return None;}
    let mut stages=[[0.;10];7];
    for i in 0..7{let mut stage=y;for j in 0..i{for k in 0..10{stage[k]+=dt*A[i][j]*stages[j][k];}}
        stages[i]=rhs(axis,launch,h+dt*C[i],stage,b)?;}
    let delta:Y=std::array::from_fn(|k|dt*A[6].iter().enumerate().map(|(j,c)|c*stages[j][k]).sum::<f64>());
    let embedded:Y=std::array::from_fn(|k|dt*E.iter().enumerate().map(|(j,c)|c*stages[j][k]).sum::<f64>());
    let r5:Y=std::array::from_fn(|k|dt*D.iter().enumerate().map(|(j,c)|c*stages[j][k]).sum::<f64>());
    let next:Y=std::array::from_fn(|k|y[k]+delta[k]);
    if !next.iter().chain(embedded.iter()).chain(r5.iter()).all(|v|v.is_finite()){return None;}
    let path_delta=delta[2]+delta[8];if path_delta<=0.||path_delta>100000.{return None;}
    let radius=6399594.+h.abs()+dt;
    // Charge both baseline and correction errors. In particular, replacing
    // the integrated baseline with a line at output is NOT assumed exact.
    let metric=|v:Y| (radius*(l1(&v[..2])+l1(&v[3..5])))
        .max(2.*path_delta*l1(&v[5..8])).max(v[2].abs()+v[8].abs()+v[9].abs());
    let error=metric(embedded)+metric(r5)/16.;
    error.is_finite().then_some((next,error))
}
#[derive(Clone,Debug)]
pub struct Sample{pub height:f64,pub state:Y,pub local_estimate:f64}
#[derive(Clone,Debug)]
pub struct Solve{pub samples:Vec<Sample>,pub attempts:usize,pub accepted:usize,pub exit:bool,
    pub final_height:f64,pub final_step:f64,pub final_proposal:f64,pub final_target:f64,
    pub stage_rejections:usize,pub error_rejections:usize,pub stop_reason:&'static str}
/// Scalar cost candidate. Upward conditioning only; failure defers. The
/// metre-valued estimator is a local numerical screen, not a global bound.
pub fn solve(axis:[f64;3],initial_h:f64,launch:[f64;3],knots:&[(f64,f64)],tolerance:f64)->Solve{
    let mut out=Solve{samples:Vec::new(),attempts:0,accepted:0,exit:false,
        final_height:initial_h,final_step:0.,final_proposal:0.,final_target:initial_h,
        stage_rejections:0,error_rejections:0,stop_reason:"initial domain"};
    if knots.len()<2||launch[1]<=0.02||!tolerance.is_finite()||tolerance<=0.{return out;}
    let mut y=[0.;10];let mut h=initial_h;let mut proposal=2000.*launch[1];
    let mut owner=knots.partition_point(|v|v.0<=h);let top=knots.last().unwrap().0;
    if owner>=knots.len(){return out;}
    out.stop_reason="attempt budget";
    while h<top && out.attempts<8192{
        out.attempts+=1;
        let b=if owner==0{(knots[0].0,knots[0].1,0.)}else{
            let (lo,n)=knots[owner-1];let (hi,n1)=knots[owner];(lo,n,(n1-n)/(hi-lo))};
        let target=knots[owner].0;let end=(h+proposal).min(target);let step=end-h;
        out.final_height=h;out.final_step=step;out.final_proposal=proposal;out.final_target=target;
        // A positive represented remainder at an owned source node is a real
        // integration interval, however short. Integrate it; do not snap the
        // state or refuse it merely because it is below an arbitrary metre
        // floor. A rejected step still halves until representable progress
        // ceases, at which point the finite solver defers.
        if !(end>h){out.stop_reason="no represented height progress";break;}
        let Some((next,error))=trial(axis,launch,h,y,end,b)else{out.stage_rejections+=1;proposal=step*0.5;continue;};
        if error>tolerance{out.error_rejections+=1;proposal=step*0.5;continue;}
        y=next;h=end;out.accepted+=1;
        if end==target{out.samples.push(Sample{height:h,state:y,local_estimate:error});owner+=1;}
        proposal=if error<tolerance/32.{(proposal.max(step)*2.).min(100000.)}else{proposal.max(step)};
    }
    out.exit=h==top;out.final_height=h;if out.exit{out.stop_reason="top exit";}
    out
}
// A stable physical difference at the same prescribed height. This preserves
// zero geometric deviation exactly, avoiding subtraction of two Earth radii.
pub fn position_delta(axis:[f64;3],h:f64,q:[f64;2],dq:[f64;2])->[f64;3]{
    let q1=[q[0]+dq[0],q[1]+dq[1]];
    let s0=(1.+dot([q[0],q[1],0.],[q[0],q[1],0.])).sqrt();
    let s1=(1.+dot([q1[0],q1[1],0.],[q1[0],q1[1],0.])).sqrt();
    let ds=(dq[0]*(q1[0]+q[0])+dq[1]*(q1[1]+q[1]))/(s1+s0);
    let n0=normal(q);let dn:[f64;3]=std::array::from_fn(|i|[dq[0],0.,-dq[1]][i]/s1-n0[i]*ds/s1);
    let z0=dot(axis,n0);let dz=dot(axis,dn);let z1=z0+dz;
    let t0=(1.-E2*z0*z0).sqrt();let t1=(1.-E2*z1*z1).sqrt();
    let nr0=WGS84_A_M/t0;let nr1=WGS84_A_M/t1;
    let dnr=WGS84_A_M*E2*dz*(z1+z0)/(t0*t1*(t0+t1));
    std::array::from_fn(|i|(nr1+h)*dn[i]+dnr*n0[i]-E2*(dnr*z1+nr0*dz)*axis[i])
}
fn corrections(axis:[f64;3],s:&Sample)->[f64;8]{
    let y=s.state;let p=position_delta(axis,s.height,[y[0],y[1]],[y[3],y[4]]);
    [p[0],p[1],p[2],y[5],y[6],y[7],y[8],y[9]]
}
fn launch(view:&[f64],px:f64,py:f64)->[f64;3]{
    let sx=(2.*(px+0.5)/view[16]-1.)*view[12]*view[13].tan();
    let sy=(1.-2.*(py+0.5)/view[17])*view[13].tan();
    unit(std::array::from_fn(|i|view[i]*sx+view[4+i]*sy+view[8+i]))
}
fn main(){
    let prefix=std::env::args().nth(1).expect("new output prefix");
    assert!(!Path::new(&format!("{prefix}.json")).exists());
    let input="artifacts/optical-height-performance-conditioning.json";
    let data:Value=serde_json::from_slice(&fs::read(input).unwrap()).unwrap();
    let fixture=&data["fixture"];let clear=&fixture["clear_ray_fixture"];
    let p=&clear["cases"][0]["canonical_inputs"];
    let initial_h=clear["eye_height_m"].as_f64().unwrap();
    let axis=unit([0.,clear["encoded_uniform"][0][0].as_f64().unwrap(),-clear["encoded_uniform"][0][1].as_f64().unwrap()]);
    let base=p["profile_base_height_m"].as_f64().unwrap();
    let knots:Vec<_>=p["layers"].as_array().unwrap().iter().map(|v|(base+v["height_m"].as_f64().unwrap(),v["refractivity"].as_f64().unwrap())).collect();
    let view:Vec<f64>=fixture["view_values"].as_array().unwrap().iter().map(|v|v.as_f64().unwrap()).collect();
    assert_eq!(view.len(),20);assert_eq!(view[14],0.,"Rectilinear cost screen only");
    let old:Value=serde_json::from_slice(&fs::read("artifacts/optical-shared-transport-cell4-first.json").unwrap()).unwrap();
    let origins:Vec<[u32;2]>=old["cases"].as_array().unwrap().iter().map(|c|[c["cell"][0].as_u64().unwrap() as u32,c["cell"][1].as_u64().unwrap() as u32]).collect();
    let tolerance=1e-5;let mut rows=Vec::new();
    for size in [4u32,8,16]{for &o in &origins{
        let cell=[o[0].min(1920-size),o[1].min(1080-size),size,size];let x=f64::from(cell[0]);let y=f64::from(cell[1]);let span=f64::from(size-1);
        let clock=Instant::now();
        let launches:[[f64;3];4]=[[0.,0.],[1.,0.],[0.,1.],[1.,1.]].map(|uv|launch(&view,x+span*uv[0],y+span*uv[1]));
        let corners:Vec<_>=launches.into_iter().map(|u|solve(axis,initial_h,u,&knots,tolerance)).collect();
        let producer_seconds=clock.elapsed().as_secs_f64();
        let corner_exits=corners.iter().all(|s|s.exit);let mut observed=[0_f64;4];let mut pixel_exits=0usize;let mut pixel_attempts=0usize;let mut checks=0usize;
        let clock=Instant::now();
        if corner_exits{for py in 0..size{for px in 0..size{
            let direct=solve(axis,initial_h,launch(&view,x+f64::from(px),y+f64::from(py)),&knots,tolerance);
            pixel_attempts+=direct.attempts;if !direct.exit{continue;}pixel_exits+=1;
            let u=f64::from(px)/span;let v=f64::from(py)/span;let weights=[(1.-u)*(1.-v),u*(1.-v),(1.-u)*v,u*v];
            for (j,s) in direct.samples.iter().enumerate(){
                assert!(corners.iter().all(|c|c.samples[j].height==s.height));
                let exact=corrections(axis,s);let stencil:[f64;8]=std::array::from_fn(|k|corners.iter().zip(weights).map(|(c,w)|w*corrections(axis,&c.samples[j])[k]).sum());
                observed[0]=observed[0].max(norm(std::array::from_fn(|k|stencil[k]-exact[k])));
                observed[1]=observed[1].max(norm(std::array::from_fn(|k|stencil[k+3]-exact[k+3])));
                observed[2]=observed[2].max((stencil[6]-exact[6]).abs());observed[3]=observed[3].max((stencil[7]-exact[7]).abs());checks+=1;
            }
        }}}
        let direct_check_seconds=clock.elapsed().as_secs_f64();
        let clock=Instant::now();let scalar=black_box(optical_shared_scalar::run(axis,initial_h,&knots,&view,&[cell]));let scalar_seconds=clock.elapsed().as_secs_f64();
        rows.push(json!({"cell":cell,"producer_seconds":producer_seconds,"producer_attempts":corners.iter().map(|c|c.attempts).sum::<usize>(),"corner_exits":corner_exits,"direct_check_seconds":direct_check_seconds,"direct_pixel_exits":pixel_exits,"direct_attempts":pixel_attempts,"source_height_comparisons":checks,"observed_maximum_correction_difference":{"position_m":observed[0],"direction":observed[1],"path_m":observed[2],"optical_excess_m":observed[3]},"existing_scalar_control_seconds":scalar_seconds,"existing_scalar_control":{"attempts":scalar.attempts,"accepted":scalar.accepted,"exits":scalar.exits,"deferred":scalar.deferred,"checksum":scalar.checksum},"corner_launch_bits":launches.map(|u|u.map(f64::to_bits)),"corner_terminal":corners.iter().map(|c|json!({"exit":c.exit,"attempts":c.attempts,"accepted":c.accepted,"height_bits":c.final_height.to_bits(),"step_bits":c.final_step.to_bits(),"proposal_bits":c.final_proposal.to_bits(),"target_bits":c.final_target.to_bits(),"stage_rejections":c.stage_rejections,"error_rejections":c.error_rejections,"reason":c.stop_reason})).collect::<Vec<_>>(),"corner_source_states":corners.iter().map(|c|c.samples.iter().map(|s|json!({"height_bits":s.height.to_bits(),"state_bits":s.state.map(f64::to_bits),"local_estimate_bits":s.local_estimate.to_bits()})).collect::<Vec<_>>()).collect::<Vec<_>>(),"certified_admission":false}));
    }}
    fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({"schema":"ancientearth.deviation-stencil-cost-screen.v1","scope":"Unregistered scalar CPU producer cost and finite interpolation rejection screen. Four independently integrated correction corners, with geometric variation factored algebraically. All pixel/source-height differences are observations, NOT whole-cell or between-source error bounds. No scene, GPU, exact-line consumer or certified admission. The existing scalar control uses different tolerances, conditioning/chronology checks and endpoint-normalization policies; there is no ordered or directly comparable error guarantee. Timing is operation-cost evidence, not a speedup forecast.","input":input,"local_estimator_tolerance_m":tolerance,"axis":axis,"initial_height_m":initial_h,"source_knots":knots,"input_bits":{"axis":axis.map(f64::to_bits),"height":initial_h.to_bits(),"knots":knots.iter().map(|&(h,n)|[h.to_bits(),n.to_bits()]).collect::<Vec<_>>(),"view":view.iter().map(|v|v.to_bits()).collect::<Vec<_>>()},"cases":rows,"certified_admitted_cells":0})).unwrap()).unwrap();
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn no_gradient_keeps_all_geometric_deviations_zero(){
        for n in [0.,0.0003]{let result=solve([0.,0.5,-3_f64.sqrt()/2.],-20.,unit([0.2,0.9,-0.3]),&[(-30.,n),(100.,n),(10000.,n)],1e-5);assert!(result.exit);for s in result.samples{assert_eq!(&s.state[3..9],&[0.;6]);assert_eq!(position_delta([0.,0.5,-3_f64.sqrt()/2.],s.height,[s.state[0],s.state[1]],[0.;2]),[0.;3]);}}}
    #[test]fn physical_difference_matches_direct_canonical_maps(){
        let axis=[0.,0.5,-3_f64.sqrt()/2.];let q=[0.1,-0.2];let dq=[1e-4,-2e-4];let h=1000.;
        let map=|q|{let n=normal(q);let z=dot(n,axis);let r=WGS84_A_M/(1.-E2*z*z).sqrt();std::array::from_fn::<_,3,_>(|i|(r+h)*n[i]-E2*r*z*axis[i])};
        let a=map(q);let b=map([q[0]+dq[0],q[1]+dq[1]]);let difference=position_delta(axis,h,q,dq);
        assert!(norm(std::array::from_fn(|i|difference[i]-(b[i]-a[i])))<2e-9);
    }
    #[test]fn two_ulp_source_remainder_is_integrated_without_snapping(){
        let top=119972.27447712887_f64;
        let start=top.next_down().next_down();
        let result=solve([0.,0.5,-3_f64.sqrt()/2.],start,unit([0.2,0.9,-0.3]),
            &[(start-1.,0.0003),(top,0.0003)],1e-5);
        assert!(result.exit);assert_eq!(result.attempts,1);assert_eq!(result.accepted,1);
        assert_eq!(result.samples.len(),1);assert_eq!(result.samples[0].height,top);
        assert!(result.samples[0].state[2]>0.);assert!(result.samples[0].state[9]>0.);
        assert_eq!(&result.samples[0].state[3..9],&[0.;6]);
    }
}
