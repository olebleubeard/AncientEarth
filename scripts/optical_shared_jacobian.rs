//! Unused interval automatic differentiation of the SAME discrete RK map.
//! This is not a continuous-flow Lipschitz bound substituted for RK staging.
use super::optical_taylor::Interval as I;
use std::ops::{Add,Sub,Mul,Div,Neg};
#[derive(Clone,Copy,Debug)]
pub struct Jet { pub value:I,pub derivative:[I;7] }
impl Jet {
    pub fn constant(x:f64)->Self {Self::interval(I::point(x))}
    pub fn interval(value:I)->Self {Self{value,derivative:[I::point(0.);7]}}
    pub fn variable(value:I,index:usize)->Self {
        let mut result=Self::interval(value);result.derivative[index]=I::point(1.);result
    }
    pub fn valid(self)->bool {self.value.valid()&&self.derivative.iter().all(|x|x.valid())}
    pub fn inverse_sqrt(self)->Self {
        let root=self.value.sqrt();let value=I::point(1.)/root;
        let slope=-I::point(0.5)/(self.value*root);
        Self{value,derivative:self.derivative.map(|d|slope*d)}
    }
}
impl Add for Jet{type Output=Self;fn add(self,b:Self)->Self{
    Self{value:self.value+b.value,derivative:std::array::from_fn(|i|self.derivative[i]+b.derivative[i])}
}}
impl Neg for Jet{type Output=Self;fn neg(self)->Self{
    Self{value:-self.value,derivative:self.derivative.map(|x|-x)}
}}
impl Sub for Jet{type Output=Self;fn sub(self,b:Self)->Self{self+-b}}
impl Mul for Jet{type Output=Self;fn mul(self,b:Self)->Self{
    Self{value:self.value*b.value,derivative:std::array::from_fn(|i|self.derivative[i]*b.value+self.value*b.derivative[i])}
}}
impl Div for Jet{type Output=Self;fn div(self,b:Self)->Self{
    Self{value:self.value/b.value,derivative:std::array::from_fn(|i|(self.derivative[i]*b.value-self.value*b.derivative[i])/(b.value*b.value))}
}}

fn norm(v:impl IntoIterator<Item=f64>)->f64{
    let sum=v.into_iter().fold(I::point(0.),|s,x|s+I::point(x)*I::point(x));
    if !sum.valid(){return f64::INFINITY;}
    // Each exact summand is a square. Outward multiplication may straddle
    // zero after underflow; intersecting with its known nonnegative domain
    // retains the enclosure without admitting invalid or overflowing sums.
    let root=I{lo:sum.lo.max(0.),hi:sum.hi}.sqrt();
    if root.valid(){root.hi}else{f64::INFINITY}
}
/// Normwise grouped perturbation bound. In own coordinate/direction blocks,
/// subtract the identity before taking a Frobenius norm. This retains the
/// near-identity discrete map instead of repeatedly inflating a projection's
/// componentwise row sum. The actual derivative enclosure supplies the rest.
pub fn transport(map:&[Jet;7],incoming:[f64;4])->[f64;4]{
    transport_impl(map,incoming,true)
}
/// Derivative-only grouped norm bound for an error/dense-correction map.
/// Such a map does not contain the state identity; adding it would create
/// artificial uncertainty even for an identically zero estimator.
pub fn transport_correction(map:&[Jet;7],incoming:[f64;4])->[f64;4]{
    transport_impl(map,incoming,false)
}
fn transport_impl(map:&[Jet;7],incoming:[f64;4],identity:bool)->[f64;4]{
    if !map.iter().all(|v|v.valid())||incoming.iter().any(|x|!x.is_finite()||*x<0.){
        return [f64::INFINITY;4];
    }
    let blocks=[(0,2),(2,5),(5,6),(6,7)];
    std::array::from_fn(|row|{
        let (lo,hi)=blocks[row];let mut sum=I::point(0.);
        for (column,&(clo,chi)) in blocks.iter().enumerate(){
            let residual=norm((lo..hi).flat_map(|i|(clo..chi).map(move|j|{
                (map[i].derivative[j]-I::point(if identity && row==column && i==j{1.}else{0.})).abs_upper()
            })));
            if !residual.is_finite(){return f64::INFINITY;}
            let operator_interval=if identity && row==column{I::point(1.)+I::point(residual)}else{I::point(residual)};
            if !operator_interval.valid(){return f64::INFINITY;}
            let operator=operator_interval.hi;
            sum=sum+I::point(operator)*I::point(incoming[column]);
            if !sum.valid(){return f64::INFINITY;}
        }
        sum.hi
    })
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn identity_transport_is_not_componentwise_exponential(){
        let state=std::array::from_fn(|i|Jet::variable(I{lo:-1.,hi:1.},i));
        let input=[1e-9,2e-9,3e-9,4e-9];let output=transport(&state,input);
        for i in 0..4{assert!(output[i]>=input[i]);assert!(output[i]<input[i]*(1.+1e-12));}
    }
    #[test]fn derivative_quotient_and_crossblock_are_enclosed(){
        let x=Jet::variable(I{lo:1.,hi:2.},0);let y=Jet::variable(I{lo:3.,hi:4.},2);
        let expression=x*y+x/y;
        for a in [1.,1.5,2.]{for b in [3.,3.5,4.]{
            let dx=b+1./b;let dy=a-a/(b*b);
            assert!(expression.derivative[0].lo<=dx&&expression.derivative[0].hi>=dx);
            assert!(expression.derivative[2].lo<=dy&&expression.derivative[2].hi>=dy);
        }}
    }
}
