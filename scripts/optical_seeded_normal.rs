//! Diagnostic only: solve the ellipsoid-normal equation at a supplied height.
//! No production field, tolerance or coordinate inverse is replaced here.
use earth_math::{Vec3d, WGS84_A_M, WGS84_B_M};

#[derive(Clone, Copy, Debug)]
pub struct SeededNormal {
    pub normal: Vec3d,
    /// Finite scalar residual envelope mapped through the monotone root slope.
    pub root_error_bound: f64,
    /// Normalization sensitivity plus a declared f64 arithmetic allowance.
    pub normal_error_bound: f64,
    pub iterations: usize,
}

/// For A = a Q = alpha I + delta k k^T and q=A p, the fixed-height normal
/// satisfies (m I + c k k^T)n=q, c=H delta, |n|=1, m>max(0,-c).
/// Its unique scalar root obeys |q|-max(c,0) <= m <= |q|-min(c,0).
/// We solve for d=m-|q| so the small correction is never obtained by
/// subtracting two rounded values near unity.
pub fn seeded_normal(position: Vec3d, height: f64) -> Option<SeededNormal> {
    let alpha = 1.0 / WGS84_A_M;
    let delta = WGS84_A_M / (WGS84_B_M * WGS84_B_M) - alpha;
    let q = Vec3d::new(
        position.x * alpha,
        position.y * alpha,
        position.z * (alpha + delta),
    );
    solve(q, Vec3d::new(0.0, 0.0, 1.0), height * delta)
}

/// `polar` must be a unit axis, and q must use the same orthonormal frame.
/// Exposed within this diagnostic for independent rotated-coordinate checks.
pub fn solve(q: Vec3d, polar: Vec3d, c: f64) -> Option<SeededNormal> {
    let r = q.length();
    if !q.is_finite()
        || !polar.is_finite()
        || (polar.length() - 1.0).abs() > 32.0 * f64::EPSILON
        || !c.is_finite()
        || !(r > 0.75 && c.abs() < r * 0.25)
    {
        return None;
    }
    let z = q.dot(polar);
    let lower = -c.max(0.0);
    let upper = -c.min(0.0);
    let floor = r - c.abs();
    // G(d)=d(2r+d)+z^2*c*(2m+c)/(m+c)^2.
    // G'=2m*[1-z^2*c/(m+c)^3] is strictly positive on this bracket.
    let slope_lower = 2.0 * floor * (1.0 - r * r * c.max(0.0) / floor.powi(3));
    if !(slope_lower > 0.0) {
        return None;
    }
    let mut d = (-c * (z / r).powi(2)).clamp(lower, upper);
    let mut iterations = 0;
    for _ in 0..2 {
        let m = r + d;
        let denominator = m + c;
        let value = d * (2.0 * r + d) + z * z * c * (2.0 * m + c) / (denominator * denominator);
        let derivative = 2.0 * m * (1.0 - z * z * c / denominator.powi(3));
        if !value.is_finite() || !(derivative > 0.0) {
            return None;
        }
        let next = (d - value / derivative).clamp(lower, upper);
        iterations += 1;
        if next == d {
            break;
        }
        d = next;
    }
    let m = r + d;
    let denominator = m + c;
    let value = d * (2.0 * r + d) + z * z * c * (2.0 * m + c) / (denominator * denominator);
    let u = f64::EPSILON * 0.5;
    let gamma64 = 64.0 * u / (1.0 - 64.0 * u);
    // Deliberately includes the large r^2 scale: this finite scalar diagnostic
    // does not pretend that a tiny computed residual has no rounding floor.
    let roundoff = gamma64
        * (r * r
            + d.abs() * (2.0 * r + d.abs())
            + z * z * c.abs() * (2.0 * m.abs() + c.abs()) / (denominator * denominator));
    let root_error_bound = (value.abs() + roundoff) / slope_lower;
    let w = q - polar * ((c / denominator) * z);
    let normal = w.normalized()?;
    let w_floor = r - c.abs() * r / floor;
    let sensitivity = 2.0 * c.abs() * r / (floor * floor * w_floor);
    let normal_error_bound = sensitivity * root_error_bound + gamma64;
    if !normal_error_bound.is_finite() {
        return None;
    }
    Some(SeededNormal {
        normal,
        root_error_bound,
        normal_error_bound,
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::{EnuBasis, Geodetic, geodetic_to_ecef};

    #[test]
    fn exact_height_and_bounded_height_perturbation() {
        let delta = WGS84_A_M / (WGS84_B_M * WGS84_B_M) - 1.0 / WGS84_A_M;
        let mut maximum: f64 = 0.0;
        for latitude in [
            -90.0, -89.999, -62.0, -15.0, 0.0, 29.305549, 67.0, 89.999, 90.0,
        ] {
            for longitude in [-180.0, -94.767015, 0.0, 177.0] {
                for height in [-10000.0, -27.7, 0.0, 2.0, 1000.0, 20000.0, 100000.0] {
                    let location = Geodetic::new(latitude, longitude, height).unwrap();
                    let p = geodetic_to_ecef(location);
                    let actual = EnuBasis::at(location).up;
                    for perturbation in [-2.0, -0.002, 0.0, 0.002, 2.0] {
                        let result = seeded_normal(p, height + perturbation).unwrap();
                        // Differentiate the constrained normal equation in H.
                        // The tangent operator's least eigenvalue is >=|q|-|c|.
                        let q = Vec3d::new(
                            p.x / WGS84_A_M,
                            p.y / WGS84_A_M,
                            p.z * WGS84_A_M / (WGS84_B_M * WGS84_B_M),
                        );
                        let floor = q.length() - delta * (height.abs() + perturbation.abs());
                        let height_allowance = delta * perturbation.abs() / (2.0 * floor);
                        let error = (result.normal - actual).length();
                        assert!(
                            error <= height_allowance + result.normal_error_bound,
                            "{latitude} {longitude} {height} {perturbation}: {error}"
                        );
                        assert!(result.root_error_bound < 1e-12);
                        assert!((1..=2).contains(&result.iterations));
                        if perturbation == 0.0 {
                            maximum = maximum.max(error);
                        }
                    }
                }
            }
        }
        eprintln!("maximum exact-height normal discrepancy: {maximum:e}");
    }

    #[test]
    fn scalar_root_residual_and_input_domains() {
        for q in [
            Vec3d::new(0.8, 0.0, 0.6),
            Vec3d::new(0.0, 0.0, 1.01),
            Vec3d::new(1.0, 0.0, 0.0),
        ] {
            for c in [-0.0001, -1e-9, 0.0, 1e-9, 0.0001] {
                let result = solve(q, Vec3d::new(0.0, 0.0, 1.0), c).unwrap();
                let n = result.normal;
                // A separately bracketed norm equation supplies the scalar root.
                let mut lo = q.length() - c.max(0.0);
                let mut hi = q.length() - c.min(0.0);
                for _ in 0..80 {
                    let m = lo + (hi - lo) * 0.5;
                    let value =
                        (q.x * q.x + q.y * q.y) / (m * m) + q.z * q.z / ((m + c) * (m + c)) - 1.0;
                    if value > 0.0 {
                        lo = m;
                    } else {
                        hi = m;
                    }
                }
                let m = lo + (hi - lo) * 0.5;
                let reference = Vec3d::new(q.x / m, q.y / m, q.z / (m + c))
                    .normalized()
                    .unwrap();
                assert!((n - reference).length() <= result.normal_error_bound);
            }
        }
        assert!(seeded_normal(Vec3d::ZERO, 0.0).is_none());
        assert!(seeded_normal(Vec3d::new(f64::NAN, 0.0, 0.0), 0.0).is_none());
        assert!(seeded_normal(Vec3d::new(WGS84_A_M, 0.0, 0.0), f64::NAN).is_none());
        assert!(solve(Vec3d::new(1.0, 0.0, 0.0), Vec3d::new(0.0, 0.0, 2.0), 0.0).is_none());
    }
}
