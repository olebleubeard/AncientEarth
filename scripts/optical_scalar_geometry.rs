//! Diagnostic scalar ellipsoid inverse. Production geometry is unchanged.
//! The rank-one solve supplies a seed; a GPU implementation must finish with
//! the actual uploaded quadratic matrix and reject/fallback on its residual.
use earth_math::{Vec3d, WGS84_A_M, WGS84_B_M};

#[derive(Clone, Copy, Debug)]
pub struct ScalarGeometry {
    pub normal: Vec3d,
    pub height: f64,
    pub multiplier: f64,
    pub multiplier_error_bound: f64,
    pub height_error_bound: f64,
    pub normal_error_bound: f64,
    pub iterations: usize,
}

/// A=aQ=alpha I+(beta-alpha)kk^T, q=A P, F0=P.A.P-a.
/// Foot point X=(I+mu A)^(-1)P. The strictly decreasing scalar equation
/// X.A.X-a=0 is evaluated without subtracting two Earth-radius quantities.
/// q/F0/axis must describe one coherent quadratic for this exact identity.
pub fn scalar_geometry(q: Vec3d, f0: f64, polar: Vec3d) -> Option<ScalarGeometry> {
    let alpha = 1.0 / WGS84_A_M;
    let beta = WGS84_A_M / (WGS84_B_M * WGS84_B_M);
    if !q.is_finite()
        || !f0.is_finite()
        || !polar.is_finite()
        || (polar.length() - 1.0).abs() > 32.0 * f64::EPSILON
        || q.length() <= 0.75
        || 1.0 + f0 / WGS84_A_M <= 0.0
    {
        return None;
    }
    let parallel = q.dot(polar);
    let perpendicular = q - polar * parallel;
    let p2 = perpendicular.dot(perpendicular);
    let z2 = parallel * parallel;
    let eval = |mu: f64| {
        let da = 1.0 + mu * alpha;
        let db = 1.0 + mu * beta;
        let ta = p2 * (2.0 + mu * alpha) / (da * da);
        let tb = z2 * (2.0 + mu * beta) / (db * db);
        let value = f0 - mu * (ta + tb);
        let derivative = -2.0 * (p2 / da.powi(3) + z2 / db.powi(3));
        (value, derivative, ta.abs() + tb.abs())
    };
    // On a constant quadratic level, scaling both axes brackets every
    // eigen-direction. Rationalization preserves small/negative heights.
    let scale_minus_one = (f0 / WGS84_A_M) / (1.0 + (1.0 + f0 / WGS84_A_M).sqrt());
    let lo0 = (scale_minus_one / alpha).min(scale_minus_one / beta);
    let hi0 = (scale_minus_one / alpha).max(scale_minus_one / beta);
    let u = f64::EPSILON * 0.5;
    let gamma64 = 64.0 * u / (1.0 - 64.0 * u);
    let padding = gamma64 * (1.0 + lo0.abs().max(hi0.abs()));
    let mut low = lo0 - padding;
    let mut high = hi0 + padding;
    if !(1.0 + low * beta > 0.0) || eval(low).0 < 0.0 || eval(high).0 > 0.0 {
        return None;
    }
    let slope_floor = -eval(high).1;
    if !(slope_floor > 0.0) {
        return None;
    }
    let mut mu = (f0 / (2.0 * (p2 + z2))).clamp(low, high);
    let mut iterations = 0;
    for _ in 0..8 {
        let (value, derivative, scale) = eval(mu);
        if !value.is_finite() || !(derivative < 0.0) {
            return None;
        }
        let rounding_floor = gamma64 * (f0.abs() + mu.abs() * scale);
        let residual = (value.abs() + rounding_floor) / slope_floor;
        if value.abs() <= rounding_floor || residual <= 1e-10 + gamma64 * mu.abs() {
            break;
        }
        if value > 0.0 {
            low = mu;
        } else {
            high = mu;
        }
        let candidate = mu - value / derivative;
        if candidate == mu {
            break;
        }
        let next = if candidate > low && candidate < high {
            candidate
        } else {
            low + (high - low) * 0.5
        };
        iterations += 1;
        if next == mu {
            break;
        }
        mu = next;
    }
    let (value, _, scale) = eval(mu);
    let multiplier_error_bound =
        (value.abs() + gamma64 * (f0.abs() + mu.abs() * scale)) / slope_floor;
    if !multiplier_error_bound.is_finite() {
        return None;
    }
    let delta = beta - alpha;
    let da = 1.0 + mu * alpha;
    let db = 1.0 + mu * beta;
    let w = q - polar * (parallel * mu * delta / db);
    let length = w.length();
    let normal = w.normalized()?;
    let height = mu * length / da;
    let min_den = (1.0 + low * alpha).min(1.0 + low * beta);
    let w_floor = q.length() - parallel.abs() * low.abs().max(high.abs()) * delta / min_den;
    if !(min_den > 0.0 && w_floor > 0.0) {
        return None;
    }
    let normal_error_bound = 2.0 * parallel.abs() * delta / (min_den * min_den * w_floor)
        * multiplier_error_bound
        + gamma64;
    let height_sensitivity =
        q.length() / min_den * (1.0 + low.abs().max(high.abs()) * beta / min_den);
    let height_error_bound =
        height_sensitivity * multiplier_error_bound + gamma64 * (1.0 + height.abs());
    Some(ScalarGeometry {
        normal,
        height,
        multiplier: mu,
        multiplier_error_bound,
        height_error_bound,
        normal_error_bound,
        iterations,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ActualRefinement {
    pub normal: Vec3d,
    pub height: f64,
    pub normal_change: f64,
    /// Root equation for the input normal used to obtain height, not `normal`.
    pub input_normal_implicit_residual: f64,
    pub input_normal_root_derivative: f64,
    /// Residual of the returned height/normal pair (not forced to zero).
    pub returned_pair_implicit_residual: f64,
}
/// One ORIGINAL stable quadratic/normal iteration, using actual matrix rows.
/// This reports residuals for review; it does not yet impose a GPU acceptance
/// threshold or claim that a small computed residual has no rounding error.
pub fn actual_refinement(
    q: Vec3d,
    f0: f64,
    rows: [Vec3d; 3],
    normal: Vec3d,
) -> Option<ActualRefinement> {
    let qn = Vec3d::new(
        rows[0].dot(normal),
        rows[1].dot(normal),
        rows[2].dot(normal),
    );
    let v = normal.dot(q);
    let coefficient = normal.dot(qn);
    let discriminant = v * v - f0 * coefficient;
    if !(discriminant >= 0.0) {
        return None;
    }
    let height = f0 / (v + discriminant.sqrt());
    let next = (q - qn * height).normalized()?;
    let next_qn = Vec3d::new(rows[0].dot(next), rows[1].dot(next), rows[2].dot(next));
    Some(ActualRefinement {
        normal: next,
        height,
        normal_change: (next - normal).length(),
        input_normal_implicit_residual: f0 - height * (2.0 * v - height * coefficient),
        input_normal_root_derivative: 2.0 * (v - height * coefficient),
        returned_pair_implicit_residual: f0
            - height * (2.0 * next.dot(q) - height * next.dot(next_qn)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::{EnuBasis, Geodetic, geodetic_to_ecef};
    #[test]
    fn coherent_positions_recover_known_geodetic_height_and_normal() {
        let a = 1.0 / WGS84_A_M;
        let b = WGS84_A_M / (WGS84_B_M * WGS84_B_M);
        let rows = [
            Vec3d::new(a, 0.0, 0.0),
            Vec3d::new(0.0, a, 0.0),
            Vec3d::new(0.0, 0.0, b),
        ];
        let apply = |p: Vec3d| Vec3d::new(p.x * a, p.y * a, p.z * b);
        let mut maximum_height: f64 = 0.0;
        let mut maximum_normal: f64 = 0.0;
        let mut maximum_iterations = 0;
        for latitude in [
            -90.0, -89.999, -62.0, -15.0, 0.0, 29.305549, 67.0, 89.999, 90.0,
        ] {
            for longitude in [-180.0, -94.767015, 0.0, 177.0] {
                for height in [
                    -10000.0, -27.7, -0.000001, 0.0, 0.000001, 2.0, 1000.0, 20000.0, 100000.0,
                    500000.0,
                ] {
                    let location = Geodetic::new(latitude, longitude, height).unwrap();
                    let base = geodetic_to_ecef(location.with_height(0.0).unwrap());
                    let up = EnuBasis::at(location).up;
                    let p = base + up * height;
                    let q = apply(p);
                    let f0 =
                        2.0 * height * up.dot(apply(base)) + height * height * up.dot(apply(up));
                    let result = scalar_geometry(q, f0, Vec3d::new(0.0, 0.0, 1.0)).unwrap();
                    maximum_height = maximum_height.max((result.height - height).abs());
                    maximum_normal = maximum_normal.max((result.normal - up).length());
                    maximum_iterations = maximum_iterations.max(result.iterations);
                    assert!(
                        (result.height - height).abs() <= result.height_error_bound + 1e-9,
                        "{latitude} {longitude} {height}: {result:?}"
                    );
                    assert!((result.normal - up).length() <= result.normal_error_bound);
                    let refined = actual_refinement(q, f0, rows, result.normal).unwrap();
                    assert!((refined.height - height).abs() < 2e-9);
                    assert!((refined.normal - up).length() < 1e-14);
                    assert!(refined.input_normal_root_derivative > 0.0);
                }
            }
        }
        eprintln!(
            "scalar height maximum {maximum_height:e} m; normal {maximum_normal:e}; iterations {maximum_iterations}"
        );
    }
}
