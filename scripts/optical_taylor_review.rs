//! Independent finite fixtures for the unused whole-square Taylor foundation.
//! Dyadic products below are exactly representable; analytic checks use a
//! separate scalar reference with an explicit evaluation-rounding allowance.
#[path = "optical_taylor.rs"]
mod taylor;
use taylor::{Interval, Taylor, POWERS};

fn monomial(index: usize, coefficient: f64) -> Taylor {
    let mut result = Taylor::constant(0.0);
    result.c[index] = coefficient;
    result
}

#[test]
fn prescribed_monomial_products_and_signed_remainders() {
    for i in 0..10 {
        for j in 0..10 {
            for sign in [-1.0, 1.0] {
                let a = monomial(i, sign / 16.0);
                let b = monomial(j, 3.0 / 32.0);
                let product = a * b;
                assert!(product.valid());
                let degree = (POWERS[i].0 + POWERS[j].0, POWERS[i].1 + POWERS[j].1);
                let exact = sign * 3.0 / 512.0;
                match POWERS.iter().position(|p| *p == degree) {
                    Some(k) => {
                        assert_eq!(product.c[k], exact);
                        for n in 0..10 {
                            if n != k { assert_eq!(product.c[n], 0.0); }
                        }
                    }
                    None => {
                        assert_eq!(product.c, [0.0; 10]);
                        assert!(product.remainder >= exact.abs());
                    }
                }
                // An independent degree-six monomial, including all corners.
                for x in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
                    for y in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
                        let truth = exact * x.powi(degree.0 as i32) * y.powi(degree.1 as i32);
                        assert!((product.polynomial(x, y) - truth).abs() <= product.remainder);
                    }
                }
            }
        }
    }
    // Full-square remainder interactions, independent of coefficient roundoff.
    let a = Taylor::variable(0, 0.5, 0.125).with_extra_remainder(1.0 / 64.0);
    let b = Taylor::variable(1, -0.25, 0.0625).with_extra_remainder(1.0 / 128.0);
    let result = a * b;
    let cross = 0.625 / 128.0 + 0.3125 / 64.0 + 1.0 / 8192.0;
    assert!(result.remainder >= cross);
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for ra in [-1.0 / 64.0, 1.0 / 64.0] {
                for rb in [-1.0 / 128.0, 1.0 / 128.0] {
                    let truth = (0.5 + 0.125 * x + ra) * (-0.25 + 0.0625 * y + rb);
                    assert!((result.polynomial(x, y) - truth).abs() <= result.remainder);
                }
            }
        }
    }
}

#[test]
fn independent_nonlinear_values_and_full_interval_uncertainty() {
    let positive = Taylor::constant(1.0) + Taylor::variable(0, 0.0, 1.0 / 16.0)
        + Taylor::variable(1, 0.0, 1.0 / 32.0);
    let uncertain = positive.with_extra_remainder(1.0 / 1024.0);
    for model in [positive, uncertain, -positive, -uncertain] {
        let inverse = model.reciprocal();
        assert!(inverse.valid());
        for i in -8..=8 {
            for j in -8..=8 {
                let (x, y) = (f64::from(i) / 8.0, f64::from(j) / 8.0);
                for residual in [-model.remainder, 0.0, model.remainder] {
                    let argument = model.polynomial(x, y) + residual;
                    let reference = 1.0 / argument;
                    let allowance = 16.0 * f64::EPSILON * reference.abs().max(1.0);
                    assert!((inverse.polynomial(x, y) - reference).abs() <= inverse.remainder + allowance);
                }
            }
        }
    }
    let inverse_root = uncertain.inverse_sqrt();
    assert!(inverse_root.valid());
    // Choose arguments with separately prescribed dyadic square roots. These
    // check the nonlinear result without using sqrt as the reference oracle.
    for numerator in 62..=66 {
        let root = f64::from(numerator) / 64.0;
        let x = 16.0 * (root * root - 1.0);
        if x.abs() > 1.0 { continue; }
        let reference = 1.0 / root;
        assert!((inverse_root.polynomial(x, 0.0) - reference).abs()
            <= inverse_root.remainder + 16.0 * f64::EPSILON);
    }
    for residual in [-uncertain.remainder, uncertain.remainder] {
        let exact_argument = 1.0 + residual;
        let reference = 1.0 / exact_argument.sqrt();
        assert!((inverse_root.polynomial(0.0, 0.0) - reference).abs()
            <= inverse_root.remainder + 16.0 * f64::EPSILON);
    }
}

#[test]
fn singular_domains_and_invalid_arithmetic_defer() {
    assert!(!Taylor::variable(0, 0.0, 1.0).reciprocal().valid());
    assert!(!Taylor::variable(0, 1.0, 0.5).reciprocal().valid());
    assert!(!Taylor::variable(0, 1.0, 0.5).inverse_sqrt().valid());
    assert!(!Taylor::constant(-1.0).inverse_sqrt().valid());
    assert!(!Taylor::constant(f64::INFINITY).reciprocal().valid());
    assert!(!(Taylor::constant(f64::MAX) * Taylor::constant(2.0)).valid());
    assert!(!(Interval::point(1.0) / Interval { lo: -1.0, hi: 1.0 }).valid());
    assert!(!Interval { lo: -1.0, hi: 1.0 }.sqrt().valid());
    // The range is a full-square bound, including the supplied uncertainty.
    let model = Taylor::variable(0, 2.0, 0.25)
        + Taylor::variable(1, 0.0, -0.125);
    let model = model.with_extra_remainder(1.0 / 64.0);
    let range = model.range();
    assert!(range.lo <= 2.0 - 0.375 - model.remainder);
    assert!(range.hi >= 2.0 + 0.375 + model.remainder);
}
