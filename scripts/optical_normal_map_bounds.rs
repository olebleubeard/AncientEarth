//! Unused canonical normal-map parameter envelope, independent of any GPU
//! arithmetic or source-mesh quantization allowance. No runtime registration.
//!
//! For F(n,H;k,e)=a(1-e(k.n)^2)^(-1/2)(n-e(k.n)k)+Hn,
//! the displayed displacement is F(n,H)-F(n0,H0). Mixed derivatives retain
//! this subtraction: parameter error scales with |n-n0|, not Earth radius.
//! This module does NOT certify the floating evaluation of F, dense-state
//! errors, mesh packing, or eye-to-patch transform arithmetic.
#[path = "optical_taylor.rs"]
mod arithmetic;
use arithmetic::Interval as I;

const A: f64 = 6_378_137.;
pub const AXIS_MIXED_BOUND_M: f64 = 300_000.;
pub const ECCENTRICITY_MIXED_BOUND_M: f64 = 20_000_000.;

/// Bounds for the full convex parameter/normal domain |n|<=1,
/// |k|<=1.000001, 0<=e<=.007. Every scalar calculation is outward binary64.
/// The normal line segment between n0 and n stays inside this domain.
pub fn derivative_bounds() -> Option<[f64; 2]> {
    let one=I::point(1.);let two=I::point(2.);let three=I::point(3.);
    let a=I::point(A);let e=I::point(0.007);let k=I::point(1.000001);
    let d=I::point(0.99);let root=d.sqrt();
    // d>=1-.007*(1.000001)^2>.99 is a deliberately outward common bound.
    if (one-e*k*k).lo<=d.hi{return None;}
    let nr=a/root;
    let nz=a*e*k/(d*root);
    let nzz=a*e/(d*root)+three*a*e*e*k*k/(d*d*root);
    let ne=a*k*k/(two*d*root);
    let nze=a*k/(d*root)+three*a*e*k*k*k/(two*d*d*root);
    // D_n D_k F: derivative of Nz(n.dk)n
    // -e[(Nz*z+N)(n.dk)k + N*z*dk].
    let axis=nzz*k+two*nz+e*((nzz*k+two*nz)*k*k+two*(nz*k+nr)*k);
    // D_n D_e F: derivative of Ne*n-(e*Ne+N)*z*k.
    let eccentricity=nze*k+ne+((e*nze+nz)*k+e*ne+nr)*k*k;
    if !axis.valid()||!eccentricity.valid(){return None;}
    Some([axis.hi,eccentricity.hi])
}

/// Parameter error of a displacement, with normwise axis difference and
/// absolute eccentricity-squared difference. `height_anchor_error` encloses
/// Hcanonical-Hencoded, and `height_difference_error` encloses
/// (Hcanonical-H0canonical)-(Hencoded-H0encoded). The latter is correlated;
/// callers may conservatively sum the two independent errors if necessary.
/// CALLER PRECONDITIONS: both parameter endpoints satisfy |k|<=1.000001 and
/// 0<=e<=.007; ideal n and n0 have norm<=1. This magnitude-only API cannot
/// inspect those endpoints. Off-unit floating normal evaluation must be
/// enclosed separately and is not covered by this parameter theorem.
pub fn parameter_allowance(
    normal_difference_upper:f64,axis_error:f64,eccentricity_error:f64,
    height_anchor_error:f64,height_difference_error:f64,
) -> Option<f64> {
    if [normal_difference_upper,axis_error,eccentricity_error,height_anchor_error,height_difference_error]
        .iter().any(|x|!x.is_finite()||*x<0.)||normal_difference_upper>2.{return None;}
    let derivative=derivative_bounds()?;
    if derivative[0]>=AXIS_MIXED_BOUND_M||derivative[1]>=ECCENTRICITY_MIXED_BOUND_M{return None;}
    let bound=I::point(normal_difference_upper)*(I::point(AXIS_MIXED_BOUND_M)*I::point(axis_error)
        +I::point(ECCENTRICITY_MIXED_BOUND_M)*I::point(eccentricity_error)+I::point(height_anchor_error))
        +I::point(height_difference_error);
    bound.valid().then_some(bound.hi)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn analytic_domain_constants_are_outward(){
        let [axis,eccentricity]=derivative_bounds().unwrap();
        assert!(axis<AXIS_MIXED_BOUND_M);
        assert!(eccentricity<ECCENTRICITY_MIXED_BOUND_M);
        // These are theorem bounds over a convex domain, not grid maxima.
        assert!(axis>200_000.&&eccentricity>15_000_000.);
    }
    #[test]
    fn displacement_correlations_and_invalid_inputs(){
        assert!(parameter_allowance(0.,1e-6,1e-9,1e-9,0.).unwrap()<1e-300);
        let a=parameter_allowance(0.01,1e-6,1e-9,1e-9,1e-9).unwrap();
        assert!(a>=0.00320000101&&a<0.00320000102);
        assert!(parameter_allowance(2.1,0.,0.,0.,0.).is_none());
        assert!(parameter_allowance(1.,-1.,0.,0.,0.).is_none());
        assert!(parameter_allowance(1.,f64::INFINITY,0.,0.,0.).is_none());
    }
}
