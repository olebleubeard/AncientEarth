//! Original MIT. Unused diagnostic of two finite WGS84 representations.
//! Common-a centered ellipsoids with B and a*sqrt(1-e²) differ in Hausdorff
//! distance by at most their polar-axis difference. Their signed distances
//! (and regular-tube normal heights) differ by at most the same quantity.
//! A B-based scene ceiling must include this gap before an e²-map comparison.
#[path="optical_taylor.rs"]mod arithmetic;
use arithmetic::Interval as I;
use earth_math::{WGS84_A_M,WGS84_B_M,WGS84_E2};
pub fn canonical_axis_gap_upper()->f64{
    let b=I::point(WGS84_A_M)*(I::point(1.)-I::point(WGS84_E2)).sqrt();
    (I::point(WGS84_B_M)-b).abs_upper().next_up()
}
fn main(){
    println!("{{\"a_bits\":{},\"b_bits\":{},\"e2_bits\":{},\"polar_axis_gap_upper_m\":{:.18e}}}",
        WGS84_A_M.to_bits(),WGS84_B_M.to_bits(),WGS84_E2.to_bits(),canonical_axis_gap_upper());
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn gap_is_retained_and_bounded_in_normal_domain(){
        let gap=canonical_axis_gap_upper();assert!(gap>0.&&gap<1e-8);
        let old=166.4064954508793_f64;let packed=166.40650939941406_f64;
        assert!((I::point(old)+I::point(gap)).hi<packed);
        // A generic zero ceiling needs the same positive model allowance.
        assert!((I::point(0.)+I::point(gap)).hi>0.);
    }
}
