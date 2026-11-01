//! Analytic fixtures for the norm-majorant foundation. Closed-form scalar
//! gradients/Hessians and Taylor remainders are separate from its propagation.
use super::{I, NormJet as J};

fn norm(x: f64, y: f64) -> f64 { (x * x + y * y).sqrt() }
fn symmetric_norm(a: f64, b: f64, d: f64) -> f64 {
    // Exact eigenvalue formula for [[a,b],[b,d]], evaluated independently.
    0.5 * ((a + d).abs() + ((a - d).powi(2) + 4. * b * b).sqrt())
}
fn enclosed(j: J, v: f64, g: f64, h: f64) {
    assert!(j.valid(), "{j:?}");
    assert!(j.value.lo <= v && v <= j.value.hi, "value {v:e}, {j:?}");
    assert!(g <= j.gradient, "gradient {g:e}, {j:?}");
    assert!(h <= j.hessian, "Hessian {h:e}, {j:?}");
}

#[test]
fn mixed_signed_polynomial_and_quotient_have_analytic_derivative_bounds() {
    let x = J::affine(0.25, [0.125, 0.]);
    let y = J::affine(-0.5, [0., 0.25]);
    let f = x * y + x / y;
    let p = x * x + x * y + J::point(2.) * y * y;
    for i in -20..=20 { for k in -20..=20 {
        let a = 0.25 + 0.125 * f64::from(i) / 20.;
        let b = -0.5 + 0.25 * f64::from(k) / 20.;
        let fx = b + 1. / b;
        let fy = a - a / (b * b);
        let fxy = 1. - 1. / (b * b);
        let fyy = 2. * a / (b * b * b);
        enclosed(f, a * b + a / b, norm(0.125 * fx, 0.25 * fy),
            symmetric_norm(0., 0.125 * 0.25 * fxy, 0.25_f64.powi(2) * fyy));
        enclosed(p, a * a + a * b + 2. * b * b,
            norm(0.125 * (2. * a + b), 0.25 * (a + 4. * b)),
            symmetric_norm(2. * 0.125_f64.powi(2), 0.125 * 0.25, 4. * 0.25_f64.powi(2)));
    }}
}

#[test]
fn unary_rules_cover_both_reciprocal_signs_and_positive_fractional_powers() {
    for sign in [-1., 1.] {
        let x = J::affine(sign * 2., [0.125, -0.25]);
        let inverse = x.reciprocal();
        let lnorm = norm(0.125, -0.25);
        for i in -20..=20 { for k in -20..=20 {
            let v = sign * 2. + 0.125 * f64::from(i) / 20. - 0.25 * f64::from(k) / 20.;
            enclosed(inverse, 1. / v, lnorm / v.powi(2), 2. * lnorm.powi(2) / v.abs().powi(3));
        }}
    }
    for base in [0.5, 2., 1000.] {
        let x = J::affine(base, [base / 32., -base / 64.]);
        let lnorm = norm(base / 32., -base / 64.);
        for (power, f) in [(0.5, x.sqrt()), (-0.5, x.inverse_sqrt())] {
            for i in -20..=20 { for k in -20..=20 {
                let v = base + base / 32. * f64::from(i) / 20. - base / 64. * f64::from(k) / 20.;
                enclosed(f, v.powf(power), power.abs() * v.powf(power - 1.) * lnorm,
                    (power * (power - 1.)).abs() * v.powf(power - 2.) * lnorm.powi(2));
            }}
        }
    }
}

#[test]
fn square_taylor_remainder_and_exact_zero_forcing_preserve_their_scope() {
    let x = J::affine(0., [1., 0.]);
    let y = J::affine(0., [0., 1.]);
    let f = x * x + y * y;
    // Independent remainder f(x,y)-f(0)-Df(0)xi = x^2+y^2.
    // The exact Hessian operator norm is 2; the scalar sum bound is looser.
    for i in -16..=16 { for k in -16..=16 {
        let a = f64::from(i) / 16.; let b = f64::from(k) / 16.;
        assert!(a * a + b * b <= f.square_remainder());
    }}
    let zero = J::point(0.);
    let positive = J::point(2.) + J::affine(0., [0.125, 0.25]);
    for correction in [zero * positive, zero / positive, zero * positive.inverse_sqrt()] {
        assert_eq!((correction.value.lo, correction.value.hi, correction.gradient, correction.hessian),
            (0., 0., 0., 0.));
    }
    // A constant nonzero refractivity still has optical excess N/v; zero
    // refractive acceleration must not erase this separate quantity.
    let excess = J::point(0.00025) / positive;
    assert!(excess.value.lo > 0. && excess.gradient > 0. && excess.hessian > 0.);
    assert_eq!(J::point(0.).sqrt().square_remainder(), 0.);
}

#[test]
fn constant_uncertainty_and_inherited_curvature_are_not_reset() {
    let c = J::constant_interval(I { lo: 2., hi: 3. });
    assert_eq!((c.gradient, c.hessian), (0., 0.));
    let incoming = J::from_parts(I { lo: -0.1, hi: 0.1 }, 0.4, 0.7);
    let result = c * incoming;
    assert!(result.gradient >= 1.2 && result.hessian >= 2.1);
    let next = incoming + J::point(0.01) * incoming * incoming;
    assert!(next.hessian >= incoming.hessian);
    assert!(next.square_remainder() >= incoming.square_remainder());
}

#[test]
fn invalid_singular_overflow_and_tiny_inputs_fail_closed() {
    for j in [
        J::point(f64::NAN), J::point(f64::INFINITY),
        J::from_parts(I::point(0.), -1., 0.),
        J::from_parts(I::point(0.), 0., f64::NAN),
        J::constant_interval(I { lo: 2., hi: 1. }),
        J::affine(0., [f64::INFINITY, 0.]),
        J::affine(0., [1., 0.]).reciprocal(),
        J::affine(1., [1., 0.]).sqrt(),
        J::point(-1.).inverse_sqrt(),
        J::point(f64::MAX) * J::point(2.),
        J::point(1e-300).reciprocal(),
        J::from_parts(I::point(1.), f64::MAX, 0.) * J::affine(1., [1., 0.]),
        J::point(0.) * J::invalid(), J::invalid() + J::point(0.),
    ] {
        assert!(!j.valid()); assert_eq!(j.square_remainder(), f64::INFINITY);
    }
    let tiny = f64::from_bits(1);
    let j = J::affine(0., [tiny, 0.]);
    assert!(j.valid() && j.gradient >= tiny);
    let small_product = J::from_parts(I::point(0.), tiny, 0.)
        * J::from_parts(I::point(0.), tiny, 0.);
    assert!(small_product.valid() && small_product.hessian > 0.);
}
