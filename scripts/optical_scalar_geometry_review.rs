//! Independent manufactured normal coordinates for the unused scalar inverse.
#[path = "optical_scalar_geometry.rs"]
mod implementation;

use earth_math::{Vec3d, WGS84_A_M, WGS84_B_M};

#[test]
fn prescribed_quadric_foot_and_normal_survive_rotation_and_signed_offsets() {
    let alpha = 1.0 / WGS84_A_M;
    let beta = WGS84_A_M / (WGS84_B_M * WGS84_B_M);
    let mut count = 0;
    for axis in [
        Vec3d::new(0.0, 0.0, 1.0),
        Vec3d::new(1.0, 0.0, 0.0),
        Vec3d::new(3.0, -4.0, 7.0).normalized().unwrap(),
    ] {
        let apply = |v: Vec3d| v * alpha + axis * ((beta - alpha) * axis.dot(v));
        let invert = |v: Vec3d| {
            v / alpha + axis * ((1.0 / beta - 1.0 / alpha) * axis.dot(v))
        };
        for n in [
            axis,
            -axis,
            Vec3d::new(2.0, 1.0, -6.0).normalized().unwrap(),
            Vec3d::new(-9.0, 3.0, 2.0).normalized().unwrap(),
        ] {
            // X = kappa A^-1 n fixes X.A.X=a and surface normal n.
            // Construct q/F0 by expanding P=X+h*n, preserving small h.
            // No geodetic forward/inverse or scalar root supplies the oracle.
            let kappa = (WGS84_A_M / n.dot(invert(n))).sqrt();
            for h in [-10000.0, -10.0, -1e-9, 0.0, 1e-9, 10.0, 120000.0, 500000.0] {
                let q = n * kappa + apply(n) * h;
                let f0 = 2.0 * h * kappa + h * h * n.dot(apply(n));
                let got = implementation::scalar_geometry(q, f0, axis).unwrap();
                let input_allowance = 32.0 * f64::EPSILON;
                assert!((got.height - h).abs()
                    <= got.height_error_bound + input_allowance * h.abs().max(1.0));
                assert!((got.normal - n).length() <= got.normal_error_bound + input_allowance);
                assert!(got.multiplier_error_bound.is_finite());
                assert!(got.multiplier * h >= 0.0);
                count += 1;
            }
        }
    }
    assert_eq!(count, 96);
}
