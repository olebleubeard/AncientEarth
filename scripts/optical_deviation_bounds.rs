//! Unregistered CPU whole-cell derivative bounds for the stable deviation RHS.
//!
//! This is a transcription over NormJet of optical_refractive_deviation.rs.
//! Each scalar encloses an exact C2 function of the same normalized square.
//! Every division/root is checked on the ENTIRE square. Stage samples alone
//! cannot admit a height interval, a cell, an ODE solution, or a scene ray.
//! The caller supplies unit launch and polar-axis functions, with correlated
//! dependencies retained by the mathematical functions (interval evaluation
//! may overestimate them). No numerical errors are reset here.
use super::optical_second_variation::NormJet as J;
use super::optical_taylor::Interval as I;

const A:f64=6378137.;
const E:f64=0.0066943799901413165;
pub type State=[J;10];
fn c(v:f64)->J{J::point(v)}
fn dot(a:[J;3],b:[J;3])->J{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn positive(x:J,lower:f64)->bool{x.valid()&&x.value.lo>lower}

/// Same states as the scalar cost screen: q0.xy, s0, dq.xy, du.xyz,
/// delta-s, optical excess. `refractivity` and `gradient` are the current
/// one-sided source branch, not a blend across source-height derivatives.
pub fn rhs(axis:[J;3],launch:[J;3],h:J,refractivity:J,gradient:J,y:State)->Option<State>{
    if !axis.iter().chain(launch.iter()).chain(y.iter()).chain([h,refractivity,gradient].iter()).all(|v|v.valid())
        ||h.value.lo < -10000.||h.value.hi>120001. {return None;}
    let q0=[y[0],y[1]];let dq=[y[3],y[4]];let du=[y[5],y[6],y[7]];
    let q1=[q0[0]+dq[0],q0[1]+dq[1]];
    let norm0=q0[0]*q0[0]+q0[1]*q0[1];let norm1=q1[0]*q1[0]+q1[1]*q1[1];
    if !norm0.valid()||!norm1.valid()||norm0.value.hi>1.||norm1.value.hi>1.{return None;}
    let r0=(c(1.)+norm0).sqrt();let r1=(c(1.)+norm1).sqrt();
    let dr=(dq[0]*(q1[0]+q0[0])+dq[1]*(q1[1]+q0[1]))/(r1+r0);
    let raw0=[q0[0],c(1.),-q0[1]];let draw=[dq[0],c(0.),-dq[1]];
    let n0=raw0.map(|v|v/r0);
    let dn:[J;3]=std::array::from_fn(|i|draw[i]/r1-raw0[i]*dr/(r1*r0));
    let n1=std::array::from_fn(|i|n0[i]+dn[i]);
    let z0=dot(axis,n0);let dz=dot(axis,dn);let z1=z0+dz;
    let d0=c(1.)-c(E)*z0*z0;let d1=c(1.)-c(E)*z1*z1;
    if !positive(d0,0.)||!positive(d1,0.){return None;}
    let t0=d0.sqrt();let t1=d1.sqrt();
    let radius0=c(A)/t0;let radius1=c(A)/t1;
    let dradius=c(A)*c(E)*dz*(z1+z0)/(t1*t0*(t1+t0));
    let v0=dot(launch,n0);let dv=dot(launch,dn)+dot(du,n1);let v1=v0+dv;
    let index=c(1.)+refractivity;
    if !positive(v0,0.02)||!positive(v1,0.02)||!index.valid()||index.value.lo<0.5{return None;}
    let r0h=radius0+h;let r1h=radius1+h;
    let den0=c(A)*(c(1.)-c(E))+h*d0*t0;
    let den1=c(A)*(c(1.)-c(E))+h*d1*t1;
    if ![r0h,r1h,den0,den1].iter().all(|&v|positive(v,0.)){return None;}
    let shape0=r0/r0h;
    let dshape=dr/r1h-r0*dradius/(r1h*r0h);
    let corr0=c(A)*c(E)/den0;
    let dd=-c(E)*dz*(z1+z0);
    let dden=h*dd*(t1+d0/(t1+t0));
    let dcorr=-c(A)*c(E)*dden/(den1*den0);
    let mu0=dot(axis,launch)-z0*v0;
    let dmu=dot(axis,du)-z0*dv-dz*v1;let mu1=mu0+dmu;
    let projection0=[launch[0]-q0[0]*launch[1],-launch[2]-q0[1]*launch[1]];
    let dprojection=[du[0]-q0[0]*du[1]-dq[0]*(launch[1]+du[1]),
        -du[2]-q0[1]*du[1]-dq[1]*(launch[1]+du[1])];
    let axis0=[axis[0]-q0[0]*axis[1],-axis[2]-q0[1]*axis[1]];
    let daxis=dq.map(|v|-v*axis[1]);
    let axis1=[axis0[0]+daxis[0],axis0[1]+daxis[1]];
    let bracket0=std::array::from_fn::<_,2,_>(|i|projection0[i]+corr0*axis0[i]*mu0);
    let dbracket=std::array::from_fn::<_,2,_>(|i|dprojection[i]+dcorr*axis1[i]*mu1
        +corr0*daxis[i]*mu1+corr0*axis0[i]*dmu);
    let scale0=shape0/v0;let dscale=dshape/v1-shape0*dv/(v1*v0);
    let geometric=bracket0.map(|v|scale0*v);
    let correction=std::array::from_fn::<_,2,_>(|i|dscale*(bracket0[i]+dbracket[i])+scale0*dbracket[i]);
    let direction=std::array::from_fn::<_,3,_>(|i|(gradient/index)*(n1[i]/v1-launch[i]-du[i]));
    let result=[geometric[0],geometric[1],c(1.)/v0,correction[0],correction[1],
        direction[0],direction[1],direction[2],-dv/(v1*v0),refractivity/v1];
    result.iter().all(|v|v.valid()).then_some(result)
}

/// Cartesian refractive correction at a common prescribed height. It is the
/// same stable normal/radius difference as the scalar screen. This bounds the
/// correction's derivatives, rather than the large geometric Earth position.
pub fn position_delta(axis:[J;3],h:J,q:[J;2],dq:[J;2])->Option<[J;3]>{
    let q1=[q[0]+dq[0],q[1]+dq[1]];
    let r0=(c(1.)+q[0]*q[0]+q[1]*q[1]).sqrt();
    let r1=(c(1.)+q1[0]*q1[0]+q1[1]*q1[1]).sqrt();
    let dr=(dq[0]*(q1[0]+q[0])+dq[1]*(q1[1]+q[1]))/(r1+r0);
    let n0=[q[0]/r0,c(1.)/r0,-q[1]/r0];
    let raw_delta=[dq[0],c(0.),-dq[1]];
    let dn=std::array::from_fn::<_,3,_>(|i|raw_delta[i]/r1-n0[i]*dr/r1);
    let z0=dot(axis,n0);let dz=dot(axis,dn);let z1=z0+dz;
    let t0=(c(1.)-c(E)*z0*z0).sqrt();let t1=(c(1.)-c(E)*z1*z1).sqrt();
    let nr0=c(A)/t0;let nr1=c(A)/t1;
    let dnr=c(A)*c(E)*dz*(z1+z0)/(t0*t1*(t0+t1));
    let result=std::array::from_fn(|i|(nr1+h)*dn[i]+dnr*n0[i]-c(E)*(dnr*z1+nr0*dz)*axis[i]);
    result.iter().all(|v|v.valid()).then_some(result)
}

/// Mean-value enclosure about the center of the normalized square. The
/// center must enclose THIS SAME function at xi=0, including inherited state
/// and arithmetic uncertainty. Intersection tightens only values; derivative
/// bounds are never reset. A disjoint intersection is an invalid proof input.
pub fn recenter(whole:J,center:I)->Option<J>{
    if !whole.valid()||!center.valid(){return None;}
    let radius_interval=I::point(2.).sqrt()*I::point(whole.gradient);
    if !radius_interval.valid()||radius_interval.lo<0.{return None;}
    let radius=radius_interval.hi;
    let enclosure=center+I{lo:-radius,hi:radius};
    if !enclosure.valid(){return None;}
    let value=I{lo:whole.value.lo.max(enclosure.lo),hi:whole.value.hi.min(enclosure.hi)};
    let result=J::from_parts(value,whole.gradient,whole.hessian);
    result.valid().then_some(result)
}

/// Bilinear interpolation from the four exact corner values: sequential
/// one-dimensional interpolation errors are each <= max|f_ii|/2 on [-1,1].
/// Convex interpolation in the other coordinate does not enlarge that bound.
/// Since each |f_ii| <= ||D2 f||op, the total is <= hessian. Corner numerical
/// error, evaluation/encoding, and the physical solver budget are separate.
pub fn bilinear_error(whole:J)->f64{whole.square_remainder()}

#[cfg(test)]
#[path="optical_deviation_bounds_review.rs"]
mod independent_review;
