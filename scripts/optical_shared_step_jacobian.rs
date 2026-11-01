//! Interval AD of the exact stored-coefficient height-parameter RK map.
//! Parameters share the same intervals as the separate Taylor evaluation.
use super::{A,C,D,E,E2,Branch};
use super::optical_taylor::{Interval as I,Taylor as T};
use super::optical_shared_jacobian::Jet as J;
type Y=[J;7];
fn k(v:f64)->J{J::constant(v)}
fn dot(a:[J;3],b:[J;3])->J{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn rhs(axis:[J;3],h:J,y:Y,b:Branch)->Option<Y>{
    if !y.iter().all(|v|v.valid()){return None;}
    let chart=y[..2].iter().fold(I::point(0.),|s,v|s+I::point(v.value.abs_upper()));
    if !chart.valid()||chart.hi>1.{return None;}
    let scale=(k(1.)+y[0]*y[0]+y[1]*y[1]).inverse_sqrt();
    let n=[y[0]*scale,scale,-y[1]*scale];let u=[y[2],y[3],y[4]];
    let v=dot(u,n);if !v.valid()||v.value.lo<=0.01{return None;}
    let inverse_v=k(1.)/v;let z=dot(n,axis);let d=k(1.)-k(E2)*z*z;
    let nr=k(6378137.)*d.inverse_sqrt();let mr=nr*k(1.-E2)/d;
    let g=J::interval(b.g.range());let refractivity=k(b.n)+g*(h-k(b.anchor));let index=k(1.)+refractivity;
    if !index.valid()||index.value.lo<0.5{return None;}
    let coefficient=nr*k(E2)/(d*(mr+h)*(nr+h));
    let meridian=std::array::from_fn(|i|axis[i]-n[i]*z);let meridian_dot=dot(meridian,u);
    let dn:[J;3]=std::array::from_fn(|i|(u[i]-n[i]*v)/(nr+h)+meridian[i]*coefficient*meridian_dot);
    let bend=g/index;
    let out=[(dn[0]-y[0]*dn[1])/n[1]*inverse_v,(-dn[2]-y[1]*dn[1])/n[1]*inverse_v,
        bend*(n[0]*inverse_v-u[0]),bend*(n[1]*inverse_v-u[1]),bend*(n[2]*inverse_v-u[2]),inverse_v,refractivity*inverse_v];
    out.iter().all(|v|v.valid()).then_some(out)
}
pub struct Step {pub next:Y,pub r:[Y;4],pub embedded:Y,pub dense:Y}
pub fn step(axis:[T;3],h:f64,polynomial:[T;7],incoming:[f64;4],endpoint:f64,b:Branch)->Option<Step>{
    if incoming.iter().any(|v|!v.is_finite()||*v<0.){return None;}
    let groups=[0,0,1,1,1,2,3];
    let y:Y=std::array::from_fn(|i|J::variable(polynomial[i].range()+I{lo:-incoming[groups[i]],hi:incoming[groups[i]]},i));
    if !y.iter().all(|v|v.valid()){return None;}
    let axis=axis.map(|v|J::interval(v.range()));
    let dt=J::interval(I::point(endpoint)-I::point(h));if !dt.valid()||dt.value.lo<=0.{return None;}
    let mut stages=[[k(0.);7];7];
    for i in 0..7{
        let mut yi=y;for j in 0..i{for a in 0..7{yi[a]=yi[a]+dt*k(A[i][j])*stages[j][a];}}
        stages[i]=rhs(axis,k(h)+dt*k(C[i]),yi,b)?;
    }
    let delta:Y=std::array::from_fn(|a|dt*A[6].iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let r3=std::array::from_fn(|a|dt*stages[0][a]-delta[a]);
    let r4=std::array::from_fn(|a|k(2.)*delta[a]-dt*(stages[0][a]+stages[6][a]));
    let r5=std::array::from_fn(|a|dt*D.iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let embedded=std::array::from_fn(|a|dt*E.iter().enumerate().fold(k(0.),|s,(j,c)|s+k(*c)*stages[j][a]));
    let r=[delta,r3,r4,r5];let next=std::array::from_fn(|a|y[a]+delta[a]);
    let theta=J::interval(I{lo:0.,hi:1.});
    let dense=std::array::from_fn(|a|y[a]+theta*delta[a]+theta*(k(1.)-theta)*(r3[a]+theta*(r4[a]+(k(1.)-theta)*r5[a])));
    r.iter().flatten().chain(next.iter()).chain(embedded.iter()).chain(dense.iter()).all(|v|v.valid())
        .then_some(Step{next,r,embedded,dense})
}
