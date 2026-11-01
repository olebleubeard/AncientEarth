//! Unregistered algebraic prototype: same height-parameter ray equations,
//! expressed as a refractive deviation from a geometric ray. This file does
//! NOT provide a whole-cell certificate or an accepted numerical integrator.
//! In particular, changing a numerical baseline to an exact line at output
//! requires a separately bounded baseline/correction consistency term.
const A: f64 = 6378137.;
const E: f64 = 0.0066943799901413165;
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
#[derive(Clone, Copy, Debug)]
pub struct Deviation {
    pub geometric_chart: [f64; 2],
    pub chart_delta: [f64; 2],
    pub direction_delta: [f64; 3],
}
#[derive(Clone, Copy, Debug)]
pub struct Derivative {
    pub geometric_chart: [f64; 2],
    pub chart_delta: [f64; 2],
    pub direction_delta: [f64; 3],
    pub path_delta: f64,
    pub optical_excess: f64,
}
/// The caller supplies a constant unit geometric launch and polar axis.
/// Every expression is an exact algebraic rearrangement over real operands.
/// Finite f64 agreement tests do not replace the future error certificate.
pub fn rhs(
    axis: [f64; 3],
    launch: [f64; 3],
    h: f64,
    n_refractivity: f64,
    gradient: f64,
    s: Deviation,
) -> Option<Derivative> {
    let q0 = s.geometric_chart;
    let dq = s.chart_delta;
    let du = s.direction_delta;
    let q1 = [q0[0] + dq[0], q0[1] + dq[1]];
    if !axis
        .iter()
        .chain(launch.iter())
        .chain(q0.iter())
        .chain(dq.iter())
        .chain(du.iter())
        .all(|x| x.is_finite())
        || !h.is_finite()
        || !n_refractivity.is_finite()
        || !gradient.is_finite()
        || q0[0] * q0[0] + q0[1] * q0[1] > 1.
        || q1[0] * q1[0] + q1[1] * q1[1] > 1.
        || !(-10000.0..=120001.0).contains(&h)
        || 1. + n_refractivity < 0.5
    {
        return None;
    }
    let r0 = (1. + q0[0] * q0[0] + q0[1] * q0[1]).sqrt();
    let r1 = (1. + q1[0] * q1[0] + q1[1] * q1[1]).sqrt();
    let dr = (dq[0] * (q1[0] + q0[0]) + dq[1] * (q1[1] + q0[1])) / (r1 + r0);
    let unscaled0 = [q0[0], 1., -q0[1]];
    let dn_unscaled = [dq[0], 0., -dq[1]];
    let n0 = unscaled0.map(|v| v / r0);
    let dn: [f64; 3] = std::array::from_fn(|i| dn_unscaled[i] / r1 - unscaled0[i] * dr / (r1 * r0));
    let n1 = add(n0, dn);
    let z0 = dot(axis, n0);
    let dz = dot(axis, dn);
    let z1 = z0 + dz;
    let d0 = 1. - E * z0 * z0;
    let d1 = 1. - E * z1 * z1;
    if d0 <= 0. || d1 <= 0. {
        return None;
    }
    let t0 = d0.sqrt();
    let t1 = d1.sqrt();
    let radius0 = A / t0;
    let radius1 = A / t1;
    let d_radius = A * E * dz * (z1 + z0) / (t1 * t0 * (t1 + t0));
    let v0 = dot(launch, n0);
    let dv = dot(launch, dn) + dot(du, n1);
    let v1 = v0 + dv;
    if v0 <= 0.02 || v1 <= 0.02 {
        return None;
    }
    let index = 1. + n_refractivity;
    let shape0 = r0 / (radius0 + h);
    let d_shape = dr / (radius1 + h) - r0 * d_radius / ((radius1 + h) * (radius0 + h));
    let den0 = A * (1. - E) + h * d0 * t0;
    let den1 = A * (1. - E) + h * d1 * t1;
    if radius0 + h <= 0. || radius1 + h <= 0. || den0 <= 0. || den1 <= 0. {
        return None;
    }
    let corr0 = A * E / den0;
    let d_d = -E * dz * (z1 + z0);
    let d_den = h * d_d * (t1 + d0 / (t1 + t0));
    let d_corr = -A * E * d_den / (den1 * den0);
    let corr1 = corr0 + d_corr;
    let mu0 = dot(axis, launch) - z0 * v0;
    let d_mu = dot(axis, du) - z0 * dv - dz * v1;
    let mu1 = mu0 + d_mu;
    let projection0 = [
        launch[0] - q0[0] * launch[1],
        -launch[2] - q0[1] * launch[1],
    ];
    let d_projection = [
        du[0] - q0[0] * du[1] - dq[0] * (launch[1] + du[1]),
        -du[2] - q0[1] * du[1] - dq[1] * (launch[1] + du[1]),
    ];
    let axis0 = [axis[0] - q0[0] * axis[1], -axis[2] - q0[1] * axis[1]];
    let d_axis = dq.map(|v| -v * axis[1]);
    let axis1 = [axis0[0] + d_axis[0], axis0[1] + d_axis[1]];
    let bracket0 = std::array::from_fn::<_, 2, _>(|i| projection0[i] + corr0 * axis0[i] * mu0);
    let d_bracket = std::array::from_fn::<_, 2, _>(|i| {
        d_projection[i]
            + d_corr * axis1[i] * mu1
            + corr0 * d_axis[i] * mu1
            + corr0 * axis0[i] * d_mu
    });
    let scale0 = shape0 / v0;
    let d_scale = d_shape / v1 - shape0 * dv / (v1 * v0);
    let geometric_chart = bracket0.map(|v| scale0 * v);
    let chart_delta =
        std::array::from_fn(|i| d_scale * (bracket0[i] + d_bracket[i]) + scale0 * d_bracket[i]);
    let direction_delta =
        std::array::from_fn(|i| (gradient / index) * (n1[i] / v1 - launch[i] - du[i]));
    let result = Derivative {
        geometric_chart,
        chart_delta,
        direction_delta,
        path_delta: -dv / (v1 * v0),
        optical_excess: n_refractivity / v1,
    };
    let _ = corr1; // Its telescoping form is used implicitly by d_bracket.
    result
        .geometric_chart
        .iter()
        .chain(result.chart_delta.iter())
        .chain(result.direction_delta.iter())
        .chain([result.path_delta, result.optical_excess].iter())
        .all(|x| x.is_finite())
        .then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn direct(
        axis: [f64; 3],
        u: [f64; 3],
        h: f64,
        n: f64,
        g: f64,
        q: [f64; 2],
    ) -> ([f64; 2], [f64; 3], f64, f64) {
        let r = (1. + q[0] * q[0] + q[1] * q[1]).sqrt();
        let normal = [q[0] / r, 1. / r, -q[1] / r];
        let z = dot(axis, normal);
        let v = dot(u, normal);
        let d = 1. - E * z * z;
        let nr = A / d.sqrt();
        let mr = nr * (1. - E) / d;
        let c = nr * E / (d * (mr + h) * (nr + h));
        let k: [f64; 3] = std::array::from_fn(|i| axis[i] - normal[i] * z);
        let ku = dot(k, u);
        let dn: [f64; 3] =
            std::array::from_fn(|i| (u[i] - normal[i] * v) / (nr + h) + k[i] * c * ku);
        (
            [
                (dn[0] - q[0] * dn[1]) / normal[1] / v,
                (-dn[2] - q[1] * dn[1]) / normal[1] / v,
            ],
            std::array::from_fn(|i| g / (1. + n) * (normal[i] / v - u[i])),
            1. / v,
            n / v,
        )
    }
    #[test]
    fn zero_deviation_has_exact_zero_geometric_difference() {
        for q in [[0., 0.], [0.1, -0.2], [-0.4, 0.3]] {
            for h in [-100., 0., 5000., 100000.] {
                let got = rhs(
                    [0., 0.5, -3_f64.sqrt() / 2.],
                    [0.2, 0.9, -0.15_f64.sqrt()],
                    h,
                    0.,
                    0.,
                    Deviation {
                        geometric_chart: q,
                        chart_delta: [0.; 2],
                        direction_delta: [0.; 3],
                    },
                )
                .unwrap();
                assert_eq!(got.chart_delta, [0.; 2]);
                assert_eq!(got.direction_delta, [0.; 3]);
                assert_eq!(got.path_delta, 0.);
                assert_eq!(got.optical_excess, 0.);
            }
        }
    }
    #[test]
    fn rearranged_rhs_matches_independent_shape_operator() {
        for q in [[0., 0.], [0.1, -0.2], [-0.4, 0.3]] {
            for h in [-100., 0., 5000., 100000.] {
                for scale in [0., 1e-12, 1e-8, 1e-4, 1e-2] {
                    let axis = [0., 0.5, -3_f64.sqrt() / 2.];
                    let launch = [0.2, 0.9, -0.15_f64.sqrt()];
                    let dq = [scale, -0.7 * scale];
                    let du = [0.2 * scale, -0.3 * scale, 0.4 * scale];
                    let got = rhs(
                        axis,
                        launch,
                        h,
                        0.00025,
                        -3e-8,
                        Deviation {
                            geometric_chart: q,
                            chart_delta: dq,
                            direction_delta: du,
                        },
                    )
                    .unwrap();
                    let baseline = direct(axis, launch, h, 0., 0., q);
                    let full = direct(
                        axis,
                        add(launch, du),
                        h,
                        0.00025,
                        -3e-8,
                        [q[0] + dq[0], q[1] + dq[1]],
                    );
                    for i in 0..2 {
                        assert!((got.geometric_chart[i] - baseline.0[i]).abs() < 2e-21);
                        assert!(
                            (got.geometric_chart[i] + got.chart_delta[i] - full.0[i]).abs() < 2e-21
                        );
                    }
                    for i in 0..3 {
                        assert!((got.direction_delta[i] - full.1[i]).abs() < 2e-22);
                    }
                    assert!((baseline.2 + got.path_delta - full.2).abs() < 2e-14);
                    assert!((got.optical_excess - full.3).abs() < 2e-18);
                }
            }
        }
    }
}
