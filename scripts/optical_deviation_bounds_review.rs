//! Independent sampled signed derivatives of the principal-curvature field.
//! This full 2-variable dual oracle is test-only; production-bound evaluation
//! uses the cheaper norm majorants. Sampling supplements the analytic rules.
use super::{I, J, State, bilinear_error, position_delta, recenter, rhs};
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug)]
struct D { v: f64, g: [f64;2], h: [[f64;2];2] }
impl D {
    fn c(v:f64)->Self { Self {v,g:[0.;2],h:[[0.;2];2]} }
    fn affine(c:f64,l:[f64;2],x:[f64;2])->Self {Self{v:c+l[0]*x[0]+l[1]*x[1],g:l,h:[[0.;2];2]}}
    fn unary(self,v:f64,p:f64,q:f64)->Self {
        Self{v,g:self.g.map(|g|p*g),h:std::array::from_fn(|i|std::array::from_fn(|j|p*self.h[i][j]+q*self.g[i]*self.g[j]))}
    }
    fn reciprocal(self)->Self {self.unary(1./self.v,-1./self.v.powi(2),2./self.v.powi(3))}
    fn sqrt(self)->Self {self.unary(self.v.sqrt(),0.5/self.v.sqrt(),-0.25/(self.v*self.v.sqrt()))}
    fn gnorm(self)->f64 {(self.g[0].powi(2)+self.g[1].powi(2)).sqrt()}
    fn hnorm(self)->f64 {
        let a=self.h[0][0];let b=(self.h[0][1]+self.h[1][0])*0.5;let d=self.h[1][1];
        0.5*((a+d).abs()+((a-d).powi(2)+4.*b*b).sqrt())
    }
}
impl Add for D {type Output=Self;fn add(self,b:Self)->Self {Self{v:self.v+b.v,g:std::array::from_fn(|i|self.g[i]+b.g[i]),h:std::array::from_fn(|i|std::array::from_fn(|j|self.h[i][j]+b.h[i][j]))}}}
impl Neg for D {type Output=Self;fn neg(self)->Self {Self{v:-self.v,g:self.g.map(|x|-x),h:self.h.map(|r|r.map(|x|-x))}}}
impl Sub for D {type Output=Self;fn sub(self,b:Self)->Self {self+-b}}
impl Mul for D {type Output=Self;fn mul(self,b:Self)->Self {
    Self{v:self.v*b.v,g:std::array::from_fn(|i|self.g[i]*b.v+self.v*b.g[i]),
        h:std::array::from_fn(|i|std::array::from_fn(|j|self.h[i][j]*b.v+self.v*b.h[i][j]+self.g[i]*b.g[j]+self.g[j]*b.g[i]))}
}}
impl Div for D {type Output=Self;fn div(self,b:Self)->Self {self*b.reciprocal()}}
fn ddot(a:[D;3],b:[D;3])->D {a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn dnormal(q:[D;2])->[D;3] {let r=(D::c(1.)+q[0]*q[0]+q[1]*q[1]).sqrt();[q[0]/r,D::c(1.)/r,-q[1]/r]}
fn dunit(u:[D;3])->[D;3] {let r=ddot(u,u).sqrt();u.map(|v|v/r)}
fn junit(u:[J;3])->[J;3] {let r=(u[0]*u[0]+u[1]*u[1]+u[2]*u[2]).sqrt();u.map(|v|v/r)}

fn direct(axis:[D;3],q:[D;2],u:[D;3],height:D,n:D,g:D)->[D;7] {
    let normal=dnormal(q);let z=ddot(axis,normal);let v=ddot(u,normal);
    let d=D::c(1.)-D::c(0.0066943799901413165)*z*z;
    let nr=D::c(6378137.)/d.sqrt();let mr=nr*D::c(1.-0.0066943799901413165)/d;
    let k: [D;3]=std::array::from_fn(|i|axis[i]-normal[i]*z);
    let coefficient=nr*D::c(0.0066943799901413165)/(d*(mr+height)*(nr+height));
    let ku=ddot(k,u);
    let dn: [D;3]=std::array::from_fn(|i|(u[i]-normal[i]*v)/(nr+height)+k[i]*coefficient*ku);
    let accel: [D;3]=std::array::from_fn(|i|g/(D::c(1.)+n)*(normal[i]/v-u[i]));
    [(dn[0]-q[0]*dn[1])/normal[1]/v,(-dn[2]-q[1]*dn[1])/normal[1]/v,
        accel[0],accel[1],accel[2],D::c(1.)/v,n/v]
}

fn direct_map(axis:[D;3],q:[D;2],height:D)->[D;3] {
    let n=dnormal(q);let z=ddot(axis,n);
    let nr=D::c(6378137.)/(D::c(1.)-D::c(0.0066943799901413165)*z*z).sqrt();
    std::array::from_fn(|i|(nr+height)*n[i]-D::c(0.0066943799901413165)*nr*z*axis[i])
}

fn compare(bound:J,reference:D) {
    assert!(bound.valid());
    // All nonzero fixture cells have broad value/derivative margins. The
    // direct subtraction oracle is not used to certify tiny cancellation.
    assert!(bound.value.lo<=reference.v && reference.v<=bound.value.hi,"value {bound:?} {reference:?}");
    assert!(reference.gnorm()<=bound.gradient,"gradient {bound:?} {reference:?}");
    assert!(reference.hnorm()<=bound.hessian,"Hessian {bound:?} {reference:?}");
}

#[test]
fn principal_curvature_rhs_and_cartesian_difference_fit_whole_cell_majorants() {
    for axis in [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]] {
        for height in [-100.,1000.,100000.] {
            let q=[J::affine(0.1,[0.001,0.]),J::affine(-0.2,[0.,-0.001])];
            let dq=[J::affine(0.0001,[0.00001,0.]),J::affine(-0.0002,[0.,0.00002])];
            let du=[J::affine(0.00001,[1e-6,0.]),J::affine(-0.00002,[0.,2e-6]),J::point(0.00003)];
            let raw=[J::affine(0.2,[0.001,0.]),J::point(1.),J::affine(-0.3,[0.,0.001])];
            let launch=junit(raw);let h=J::point(height);let n=J::point(0.00025);let g=J::point(-3e-8);
            let y:State=[q[0],q[1],J::point(0.),dq[0],dq[1],du[0],du[1],du[2],J::point(0.),J::point(0.)];
            let bounds=rhs(axis.map(J::point),launch,h,n,g,y).unwrap();
            let pbound=position_delta(axis.map(J::point),h,q,dq).unwrap();
            for ix in -2..=2 {for iy in -2..=2 {
                let xi=[f64::from(ix)/2.,f64::from(iy)/2.];
                let qd=[D::affine(0.1,[0.001,0.],xi),D::affine(-0.2,[0.,-0.001],xi)];
                let dqd=[D::affine(0.0001,[0.00001,0.],xi),D::affine(-0.0002,[0.,0.00002],xi)];
                let dud=[D::affine(0.00001,[1e-6,0.],xi),D::affine(-0.00002,[0.,2e-6],xi),D::c(0.00003)];
                let ud=dunit([D::affine(0.2,[0.001,0.],xi),D::c(1.),D::affine(-0.3,[0.,0.001],xi)]);
                let q1=[qd[0]+dqd[0],qd[1]+dqd[1]];let u1=std::array::from_fn(|i|ud[i]+dud[i]);
                let baseline=direct(axis.map(D::c),qd,ud,D::c(height),D::c(0.),D::c(0.));
                let full=direct(axis.map(D::c),q1,u1,D::c(height),D::c(0.00025),D::c(-3e-8));
                let reference=[baseline[0],baseline[1],baseline[5],full[0]-baseline[0],full[1]-baseline[1],
                    full[2],full[3],full[4],full[5]-baseline[5],full[6]];
                for i in 0..10 {compare(bounds[i],reference[i]);}
                let p0=direct_map(axis.map(D::c),qd,D::c(height));let p1=direct_map(axis.map(D::c),q1,D::c(height));
                for i in 0..3 {compare(pbound[i],p1[i]-p0[i]);}
            }}
        }
    }
}

#[test]
fn zero_gradient_and_zero_deviation_preserve_exact_correction_zero_over_cell() {
    let q=[J::affine(0.1,[0.01,0.]),J::affine(-0.2,[0.,0.01])];
    let launch=junit([J::affine(0.2,[0.01,0.]),J::point(1.),J::affine(-0.3,[0.,0.01])]);
    let mut y=[J::point(0.);10];y[0]=q[0];y[1]=q[1];
    for n in [0.,0.0003] {
        let output=rhs([J::point(0.),J::point(1.),J::point(0.)],launch,J::point(1000.),J::point(n),J::point(0.),y).unwrap();
        for item in &output[3..9] {assert_eq!((item.value.lo,item.value.hi,item.gradient,item.hessian),(0.,0.,0.,0.));}
        let physical=position_delta([J::point(0.),J::point(1.),J::point(0.)],J::point(1000.),q,[J::point(0.);2]).unwrap();
        for item in physical {assert_eq!((item.value.lo,item.value.hi,item.gradient,item.hessian),(0.,0.,0.,0.));}
        if n>0. {assert!(output[9].value.lo>0. && output[9].hessian>0.);}
    }
}

#[test]
fn whole_cell_refusal_recenter_and_sharp_bilinear_bound() {
    let axis=[J::point(0.),J::point(1.),J::point(0.)];let launch=[J::point(0.),J::point(1.),J::point(0.)];
    let y=[J::point(0.);10];
    assert!(rhs(axis,launch,J::point(0.),J::point(0.),J::point(0.),y).is_some());
    let mut broad=y;broad[0]=J::affine(0.,[1.01,0.]);
    assert!(rhs(axis,launch,J::point(0.),J::point(0.),J::point(0.),broad).is_none());
    assert!(rhs(axis,[J::point(1.),J::point(0.),J::point(0.)],J::point(0.),J::point(0.),J::point(0.),y).is_none());
    assert!(rhs(axis,launch,J::point(0.),J::affine(-0.49,[0.02,0.]),J::point(0.),y).is_none());
    assert!(rhs(axis,launch,J::affine(120000.,[2.,0.]),J::point(0.),J::point(0.),y).is_none());
    let f=J::from_parts(I{lo:-100.,hi:100.},2.,7.);
    let tightened=recenter(f,I{lo:0.9,hi:1.1}).unwrap();
    assert_eq!((tightened.gradient,tightened.hessian),(2.,7.));
    assert!(tightened.value.lo<=0.9-2.*2_f64.sqrt() && tightened.value.hi>=1.1+2.*2_f64.sqrt());
    assert!(tightened.value.lo>-100. && tightened.value.hi<100.);
    assert!(recenter(J::point(0.),I::point(1.)).is_none());
    assert!(recenter(J::invalid(),I::point(0.)).is_none());
    // f=x^2+y^2 has ||H||=2 and corner interpolant 2. Its center error
    // attains exactly H, independently verifying the stated constant.
    let sharp=J::from_parts(I{lo:0.,hi:2.},8_f64.sqrt(),2.);
    for ix in -8..=8 {for iy in -8..=8 {
        let x=f64::from(ix)/8.;let y=f64::from(iy)/8.;
        assert!(2.-x*x-y*y<=bilinear_error(sharp));
    }}
    assert_eq!(bilinear_error(sharp),2.);
}
