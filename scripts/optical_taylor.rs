//! Original diagnostic-only bivariate cubic Taylor arithmetic.
//! Coefficients describe x,y in [-1,1]. `remainder` encloses the difference
//! from that represented polynomial over the ENTIRE square. No runtime use.
//! Binary64 elementary arithmetic is widened before truncation; analytic
//! reciprocal/inverse-square-root tails are retained, never inferred from
//! agreement of samples. A nonfinite or singular operation fails closed.
use std::ops::{Add, Div, Mul, Neg, Sub};

pub const POWERS: [(usize, usize); 10] = [
    (0,0),(1,0),(0,1),(2,0),(1,1),(0,2),(3,0),(2,1),(1,2),(0,3),
];

#[derive(Clone, Copy, Debug)]
pub struct Interval { pub lo:f64, pub hi:f64 }
impl Interval {
    pub fn point(x:f64)->Self { Self{lo:x,hi:x} }
    pub fn invalid()->Self { Self{lo:f64::INFINITY,hi:f64::NEG_INFINITY} }
    pub fn valid(self)->bool { self.lo.is_finite() && self.hi.is_finite() && self.lo<=self.hi }
    pub fn abs_upper(self)->f64 { self.lo.abs().max(self.hi.abs()) }
    pub fn abs_lower(self)->f64 { if self.lo<=0. && self.hi>=0. {0.} else {self.lo.abs().min(self.hi.abs())} }
    pub fn sqrt(self)->Self {
        if !self.valid() || self.lo<0. {return Self::invalid();}
        Self{lo:if self.lo==0.{0.}else{self.lo.sqrt().next_down()},hi:if self.hi==0.{0.}else{self.hi.sqrt().next_up()}}
    }
}
impl Add for Interval {type Output=Self;fn add(self,b:Self)->Self{
    if !self.valid()||!b.valid(){return Self::invalid();}
    if self.lo==0.&&self.hi==0.{return b;} if b.lo==0.&&b.hi==0.{return self;}
    Self{lo:(self.lo+b.lo).next_down(),hi:(self.hi+b.hi).next_up()}
}}
impl Neg for Interval {type Output=Self;fn neg(self)->Self{Self{lo:-self.hi,hi:-self.lo}}}
impl Sub for Interval {type Output=Self;fn sub(self,b:Self)->Self{self+-b}}
impl Mul for Interval {type Output=Self;fn mul(self,b:Self)->Self{
    if !self.valid()||!b.valid(){return Self::invalid();}
    if (self.lo==0.&&self.hi==0.)||(b.lo==0.&&b.hi==0.){return Self::point(0.);}
    let p=[self.lo*b.lo,self.lo*b.hi,self.hi*b.lo,self.hi*b.hi];
    if !p.iter().all(|x|x.is_finite()){return Self::invalid();}
    Self{lo:p.into_iter().fold(f64::INFINITY,f64::min).next_down(),hi:p.into_iter().fold(f64::NEG_INFINITY,f64::max).next_up()}
}}
impl Div for Interval {type Output=Self;fn div(self,b:Self)->Self{
    if !b.valid()||(b.lo<=0.&&b.hi>=0.){return Self::invalid();}
    self*Self{lo:(1./b.hi).next_down(),hi:(1./b.lo).next_up()}
}}

fn upper_add(a:f64,b:f64)->f64 { if a==0.{b}else if b==0.{a}else{(a+b).next_up()} }
fn rounding(center:f64,enclosure:Interval)->f64 {
    if !center.is_finite()||!enclosure.valid(){f64::INFINITY}
    else {(center-enclosure.lo).abs().max((enclosure.hi-center).abs()).next_up()}
}

#[derive(Clone, Copy, Debug)]
pub struct Taylor { pub c:[f64;10], pub remainder:f64 }
impl Taylor {
    pub fn constant(value:f64)->Self { let mut c=[0.;10];c[0]=value;Self{c,remainder:0.} }
    pub fn interval(i:Interval)->Self {
        if !i.valid(){return Self::invalid();}
        if i.lo==i.hi{return Self::constant(i.lo);}
        let center=i.lo+0.5*(i.hi-i.lo);let mut out=Self::constant(center);
        out.remainder=rounding(center,i);out
    }
    pub fn invalid()->Self {Self{c:[0.;10],remainder:f64::INFINITY}}
    pub fn variable(axis:usize,center:f64,radius:f64)->Self {
        assert!(axis<2);let mut out=Self::constant(center);out.c[axis+1]=radius;out
    }
    pub fn valid(self)->bool {self.c.iter().all(|x|x.is_finite())&&self.remainder.is_finite()&&self.remainder>=0.}
    pub fn polynomial_abs(self)->f64 {self.c.into_iter().fold(0.,|s,x|upper_add(s,x.abs()))}
    pub fn variation(self)->f64 {self.c[1..].iter().fold(self.remainder,|s,x|upper_add(s,x.abs()))}
    pub fn range(self)->Interval {
        if !self.valid(){return Interval::invalid();}
        let r=self.variation();if r==0.{return Interval::point(self.c[0]);}
        Interval{lo:(self.c[0]-r).next_down(),hi:(self.c[0]+r).next_up()}
    }
    /// Polynomial evaluation only. The remainder and evaluation roundoff must
    /// both be charged before a consumer treats this as a physical state.
    pub fn polynomial(self,x:f64,y:f64)->f64 {
        POWERS.iter().zip(self.c).map(|(&(a,b),c)|c*x.powi(a as i32)*y.powi(b as i32)).sum()
    }
    pub fn with_extra_remainder(mut self,error:f64)->Self {self.remainder=upper_add(self.remainder,error);self}
    pub fn reciprocal(self)->Self {
        if !self.valid()||self.c[0]==0.{return Self::invalid();}
        let inverse=Taylor::interval(Interval::point(1.)/Interval::point(self.c[0]));
        let z=(self-Taylor::constant(self.c[0]))*inverse;
        let bound=z.range().abs_upper();
        if !bound.is_finite()||bound>=0.5{return Self::invalid();}
        let z2=z*z;let z3=z2*z;
        let polynomial=(Taylor::constant(1.)-z+z2-z3)*inverse;
        let r=Interval::point(bound);let r2=r*r;
        let tail=(r2*r2)/(Interval::point(self.c[0].abs())*(Interval::point(1.)-r));
        if !tail.valid(){Self::invalid()}else{polynomial.with_extra_remainder(tail.abs_upper())}
    }
    pub fn inverse_sqrt(self)->Self {
        if !self.valid()||self.c[0]<=0.{return Self::invalid();}
        let z=(self-Taylor::constant(self.c[0]))/
            Taylor::constant(self.c[0]);
        let bound=z.range().abs_upper();
        if !bound.is_finite()||bound>=0.5{return Self::invalid();}
        let z2=z*z;let z3=z2*z;
        let root_inverse=Interval::point(1.)/Interval::point(self.c[0]).sqrt();
        let polynomial=(Taylor::constant(1.)-z*Taylor::constant(0.5)
            +z2*Taylor::constant(0.375)-z3*Taylor::constant(0.3125))*Taylor::interval(root_inverse);
        // Taylor's fourth-order tail: |f''''(t)|/4! <=
        // (35/128)/(1-r)^(9/2), for f(t)=(1+t)^(-1/2).
        let r=Interval::point(bound);let r2=r*r;let lower=Interval::point(1.)-r;
        let lower2=lower*lower;let denominator=lower2*lower2*lower.sqrt();
        let tail=Interval::point(35./128.)*r2*r2/denominator*root_inverse;
        if !tail.valid(){Self::invalid()}else{polynomial.with_extra_remainder(tail.abs_upper())}
    }
}
impl Neg for Taylor {type Output=Self;fn neg(mut self)->Self{for c in &mut self.c{*c=-*c;}self}}
impl Add for Taylor {type Output=Self;fn add(self,b:Self)->Self{
    if !self.valid()||!b.valid(){return Self::invalid();}
    let mut out=Self::constant(0.);out.remainder=upper_add(self.remainder,b.remainder);
    for i in 0..10 {
        out.c[i]=self.c[i]+b.c[i];
        let exact=Interval::point(self.c[i])+Interval::point(b.c[i]);
        out.remainder=upper_add(out.remainder,rounding(out.c[i],exact));
    }
    out
}}
impl Sub for Taylor {type Output=Self;fn sub(self,b:Self)->Self{self+-b}}
impl Mul for Taylor {type Output=Self;fn mul(self,b:Self)->Self{
    if !self.valid()||!b.valid(){return Self::invalid();}
    let mut out=Self::constant(0.);
    let cross=Interval::point(self.polynomial_abs())*Interval::point(b.remainder)
        +Interval::point(b.polynomial_abs())*Interval::point(self.remainder)
        +Interval::point(self.remainder)*Interval::point(b.remainder);
    out.remainder=cross.abs_upper();
    for i in 0..10 { for j in 0..10 {
        if self.c[i]==0.||b.c[j]==0.{continue;}
        let product=Interval::point(self.c[i])*Interval::point(b.c[j]);
        let powers=(POWERS[i].0+POWERS[j].0,POWERS[i].1+POWERS[j].1);
        if let Some(k)=POWERS.iter().position(|p|*p==powers) {
            let exact=Interval::point(out.c[k])+product;
            out.c[k]+=self.c[i]*b.c[j];
            out.remainder=upper_add(out.remainder,rounding(out.c[k],exact));
        } else {out.remainder=upper_add(out.remainder,product.abs_upper());}
    }}
    out
}}
impl Div for Taylor {type Output=Self;fn div(self,b:Self)->Self{self*b.reciprocal()}}

#[cfg(test)]
mod author_checks {
    use super::*;
    #[test]
    fn exact_linear_and_truncated_quartic_are_enclosed(){
        let x=Taylor::variable(0,0.,0.01);let y=Taylor::variable(1,0.,0.02);
        let a=x+y;let fourth=a*a*a*a;
        assert!(fourth.remainder>=0.03_f64.powi(4));
        for i in -10..=10{for j in -10..=10{
            let (xi,yi)=(f64::from(i)/10.,f64::from(j)/10.);
            let exact=(0.01*xi+0.02*yi).powi(4);
            assert!((fourth.polynomial(xi,yi)-exact).abs()<=fourth.remainder);
        }}
    }
    #[test]
    fn analytic_functions_and_singular_deferral(){
        let a=Taylor::constant(1.)+Taylor::variable(0,0.,0.005)+Taylor::variable(1,0.,0.008);
        for result in [a.reciprocal(),a.inverse_sqrt()]{assert!(result.valid());}
        for i in -10..=10{for j in -10..=10{
            let (x,y)=(f64::from(i)/10.,f64::from(j)/10.);let exact=1.+0.005*x+0.008*y;
            let r=a.reciprocal();assert!((r.polynomial(x,y)-1./exact).abs()<=r.remainder);
            let r=a.inverse_sqrt();assert!((r.polynomial(x,y)-1./exact.sqrt()).abs()<=r.remainder);
        }}
        assert!(!Taylor::variable(0,0.,1.).reciprocal().valid());
        assert!(!Taylor::constant(-1.).inverse_sqrt().valid());
    }
}
