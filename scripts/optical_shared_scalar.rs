//! Unused local-frame scalar control for the shared-cell CPU cost comparison.
//! Same seven physical stages, source ownership, metre metric, proposal and
//! conditioning policy; no angular polynomial or interval-AD certificate.
//! This is a CPU comparison, not the production GPU implementation.
use super::{A,C,D,E,E2};
type Y=[f64;7];
fn dot(a:[f64;3],b:[f64;3])->f64{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn l1(v:&[f64])->f64{v.iter().map(|x|x.abs()).sum()}
fn normal(y:Y)->[f64;3]{let r=(1.+y[0]*y[0]+y[1]*y[1]).sqrt().recip();[y[0]*r,r,-y[1]*r]}
fn unit(v:[f64;3])->[f64;3]{let r=dot(v,v).sqrt().recip();v.map(|x|x*r)}
fn rhs(axis:[f64;3],h:f64,y:Y,branch:(f64,f64,f64))->Option<Y>{
    if !y.iter().all(|v|v.is_finite())||l1(&y[..2])>1.{return None;}
    let n=normal(y);let u=[y[2],y[3],y[4]];let v=dot(u,n);if !v.is_finite()||v<=0.01{return None;}
    let z=dot(n,axis);let d=1.-E2*z*z;let nr=6378137./d.sqrt();let mr=nr*(1.-E2)/d;
    let (anchor,n0,g)=branch;let refractivity=n0+g*(h-anchor);let index=1.+refractivity;
    if !index.is_finite()||index<0.5{return None;}
    let coefficient=nr*E2/(d*(mr+h)*(nr+h));let meridian=std::array::from_fn(|i|axis[i]-n[i]*z);
    let md=dot(meridian,u);let dn:[f64;3]=std::array::from_fn(|i|(u[i]-n[i]*v)/(nr+h)+meridian[i]*coefficient*md);
    let inverse_v=1./v;let bend=g/index;
    let out=[(dn[0]-y[0]*dn[1])/n[1]*inverse_v,(-dn[2]-y[1]*dn[1])/n[1]*inverse_v,
        bend*(n[0]*inverse_v-u[0]),bend*(n[1]*inverse_v-u[1]),bend*(n[2]*inverse_v-u[2]),inverse_v,refractivity*inverse_v];
    out.iter().all(|v|v.is_finite()).then_some(out)
}
fn trial(axis:[f64;3],h:f64,y:Y,end:f64,branch:(f64,f64,f64))->Option<(Y,f64)>{
    let dt=end-h;if !dt.is_finite()||dt<=0.{return None;}
    let mut stages=[[0.;7];7];
    for i in 0..7{let mut yi=y;for j in 0..i{for a in 0..7{yi[a]+=dt*A[i][j]*stages[j][a];}}
        stages[i]=rhs(axis,h+dt*C[i],yi,branch)?;}
    let delta:Y=std::array::from_fn(|a|dt*A[6].iter().enumerate().map(|(j,c)|c*stages[j][a]).sum::<f64>());
    let r3:Y=std::array::from_fn(|a|dt*stages[0][a]-delta[a]);
    let r4:Y=std::array::from_fn(|a|2.*delta[a]-dt*(stages[0][a]+stages[6][a]));
    let r5:Y=std::array::from_fn(|a|dt*D.iter().enumerate().map(|(j,c)|c*stages[j][a]).sum::<f64>());
    let embedded:Y=std::array::from_fn(|a|dt*E.iter().enumerate().map(|(j,c)|c*stages[j][a]).sum::<f64>());
    let r=[delta,r3,r4,r5];
    if !r.iter().flatten().chain(embedded.iter()).all(|x|x.is_finite()){return None;}
    let qv=r.iter().map(|v|l1(&v[..2])).sum::<f64>();let uv=r.iter().map(|v|l1(&v[2..5])).sum::<f64>();
    let qextra=l1(&r5[..2])/16.+l1(&embedded[..2]);let uextra=l1(&r5[2..5])/16.+l1(&embedded[2..5]);
    let u=[y[2],y[3],y[4]];let rounding=256.*f64::EPSILON*(1.+l1(&u)+qv+uv);
    let vertical=dot(u,normal(y))-uv-l1(&u)*qv-rounding;
    let lower=dot(u,u).sqrt()-uv-uextra-rounding;let chart=l1(&y[..2])+qv+qextra+rounding;
    if ![vertical,lower,chart].iter().all(|x|x.is_finite())||vertical<=0.01||lower<=0.25||chart>1.{return None;}
    let path_upper=(dt/vertical).max(r.iter().map(|v|v[5].abs()).sum());if !path_upper.is_finite()||path_upper<=0.||path_upper>100000.{return None;}
    let c1=delta[5]+r3[5];let c2=r4[5]-r3[5]+r5[5];let c3=-r4[5]-2.*r5[5];
    if [c1,c1+2.*c2/3.,c1+4.*c2/3.+c3,c1+2.*c2+3.*c3+4.*r5[5]].iter().any(|v|!v.is_finite()||*v<=rounding*path_upper){return None;}
    let radius=6399594.+h.abs()+dt;
    if !radius.is_finite()||radius<=0.{return None;}
    let metric=|v:Y|(radius*l1(&v[..2])).max(2.*path_upper*l1(&v[2..5])/lower).max(l1(&v[5..]));
    let error=metric(embedded)+metric(r5)/16.;if !error.is_finite(){return None;}
    let mut next=std::array::from_fn(|a|y[a]+delta[a]);let direction=unit([next[2],next[3],next[4]]);next[2..5].copy_from_slice(&direction);
    if !next.iter().all(|x|x.is_finite()){return None;}
    Some((next,error))
}
pub struct Result {pub attempts:usize,pub accepted:usize,pub exits:usize,pub deferred:usize,pub checksum:f64}
pub fn run(axis:[f64;3],initial_h:f64,knots:&[(f64,f64)],view:&[f64],cells:&[[u32;4]])->Result{
    let mut out=Result{attempts:0,accepted:0,exits:0,deferred:0,checksum:0.};
    for &[x0,y0,w,h] in cells{for py in y0..y0+h{for px in x0..x0+w{
        let sx=(2.*(f64::from(px)+0.5)/view[16]-1.)*(view[12]*view[13].tan());
        let sy=(1.-2.*(f64::from(py)+0.5)/view[17])*view[13].tan();
        let u=unit(std::array::from_fn(|i|view[i]*sx+view[4+i]*sy+view[8+i]));
        if !u.iter().all(|x|x.is_finite())||u[1]<=0.02{out.deferred+=1;continue;}
        let mut y=[0.,0.,u[0],u[1],u[2],0.,0.];let mut height=initial_h;
        let mut owner=knots.partition_point(|p|p.0<=height);let top=knots.last().unwrap().0;let mut proposal=2000.*u[1];
        let mut attempts=0;while height<top{
            attempts+=1;out.attempts+=1;if attempts>2048||proposal<1e-7{break;}
            let branch=if owner==0{(knots[0].0,knots[0].1,0.)}else{
                let (lo,n)=knots[owner-1];let (hi,n1)=knots[owner];(lo,n,(n1-n)/(hi-lo))};
            let target=knots[owner].0;let end=(height+proposal).min(target);let dt=end-height;
            let tolerance=0.002_f64.max(4e-7*(height-initial_h).abs());
            let Some((next,error))=trial(axis,height,y,end,branch)else{proposal=dt*0.5;continue;};
            if error>tolerance{proposal=dt*0.5;continue;}
            y=next;height=end;out.accepted+=1;if end==target{owner+=1;}
            proposal=if error<tolerance/32.{(proposal.max(dt)*2.).min(100000.)}else{proposal.max(dt)};
        }
        if height==top{out.exits+=1}else{out.deferred+=1;}
        out.checksum+=y.iter().sum::<f64>();
    }}}
    out
}
