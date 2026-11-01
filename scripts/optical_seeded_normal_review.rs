//! Independent diagnostic fixture: prescribe the constrained normal directly.
//! This is a finite f64 check, not an approval of a f32 field replacement.
#[path = "optical_seeded_normal.rs"]
mod implementation;

use earth_math::Vec3d;

#[test]
fn rotated_rank_one_system_recovers_prescribed_normals() {
    let axes = [
        Vec3d::new(0.0, 0.0, 1.0),
        Vec3d::new(1.0, 0.0, 0.0),
        Vec3d::new(2.0, -3.0, 7.0).normalized().unwrap(),
        Vec3d::new(-5.0, 11.0, 2.0).normalized().unwrap(),
    ];
    let normals = [
        Vec3d::new(1.0, 0.0, 0.0),
        Vec3d::new(0.0, 0.0, -1.0),
        Vec3d::new(2.0, 5.0, -3.0).normalized().unwrap(),
        Vec3d::new(-7.0, 1.0, 4.0).normalized().unwrap(),
    ];
    let mut count = 0;
    for k in axes {
        for n in normals.into_iter().chain([k, -k]) {
            for m in [0.9, 1.0, 1.1] {
                for c in [-0.1, -0.0002, -1e-9, 0.0, 1e-9, 0.0002, 0.1] {
                    // Manufacture q from the defining linear system. Neither
                    // geodetic conversion nor the implementation's root
                    // equation supplies the expected unit normal.
                    let q = n * m + k * (c * k.dot(n));
                    let got = implementation::solve(q, k, c).unwrap();
                    let input_roundoff = 32.0 * f64::EPSILON * (m + c.abs()) / (m - c.abs());
                    let error = (got.normal - n).length();
                    assert!(
                        error <= got.normal_error_bound + input_roundoff,
                        "axis={k:?} n={n:?} m={m} c={c}: {error} > {}",
                        got.normal_error_bound + input_roundoff
                    );
                    assert!((got.normal.length() - 1.0).abs() < 8.0 * f64::EPSILON);
                    assert!(got.root_error_bound.is_finite() && got.root_error_bound >= 0.0);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 504);
}
