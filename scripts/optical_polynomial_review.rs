//! Independent tests of diagnostic polynomial transforms and continuous bounds.
//! Run with rustc --test; this is not an application/runtime test inventory.
#[path = "optical_polynomial.rs"]
mod polynomial;

fn horner(a: [f64; 5], t: f64) -> f64 {
    a.into_iter()
        .rev()
        .fold(0.0, |value, coefficient| value * t + coefficient)
}

fn casteljau(mut control: [f64; 5], t: f64) -> f64 {
    for remaining in (1..5).rev() {
        for i in 0..remaining {
            control[i] = (1.0 - t) * control[i] + t * control[i + 1];
        }
    }
    control[0]
}

fn close(a: f64, b: f64, scale: f64) {
    assert!(
        (a - b).abs() <= 128.0 * f64::EPSILON * scale.max(1.0),
        "{a:.17e} != {b:.17e}"
    );
}

#[test]
fn monomial_oracles_and_de_casteljau_recover_the_original_polynomial() {
    // Independently known quartic Bernstein representations of 1,t,t²,t³,t⁴.
    let expected = [
        [1.0, 1.0, 1.0, 1.0, 1.0],
        [0.0, 0.25, 0.5, 0.75, 1.0],
        [0.0, 0.0, 1.0 / 6.0, 0.5, 1.0],
        [0.0, 0.0, 0.0, 0.25, 1.0],
        [0.0, 0.0, 0.0, 0.0, 1.0],
    ];
    for (degree, oracle) in expected.into_iter().enumerate() {
        let mut power = [0.0; 5];
        power[degree] = 1.0;
        assert_eq!(polynomial::power_to_bernstein(power), oracle);
    }
    for coefficients in [
        [2.0, -3.0, 5.0, -7.0, 11.0],
        [-2e-12, 7e-12, -5e-12, 3e-12, 1e-12],
        [100000.0, -200000.0, 300000.0, -400000.0, 500000.0],
        [0.0, 0.0, 16.0, -32.0, 16.0],
    ] {
        let control = polynomial::power_to_bernstein(coefficients);
        let scale = coefficients.into_iter().map(f64::abs).sum();
        for i in 0..=256 {
            let t = f64::from(i) / 256.0;
            close(casteljau(control, t), horner(coefficients, t), scale);
            let minimum = control.into_iter().fold(f64::INFINITY, f64::min);
            let maximum = control.into_iter().fold(f64::NEG_INFINITY, f64::max);
            let value = horner(coefficients, t);
            let allowance = 128.0 * f64::EPSILON * f64::max(scale, 1.0);
            assert!(value >= minimum - allowance && value <= maximum + allowance);
        }
    }
}

#[test]
fn half_interval_and_reversal_restrictions_preserve_values_and_join() {
    let power = [2.0, -3.0, 5.0, -7.0, 11.0];
    assert_eq!(
        polynomial::restrict_power(power, 0.0, 0.5),
        [2.0, -1.5, 1.25, -0.875, 0.6875]
    );
    assert_eq!(
        polynomial::restrict_power(power, 0.5, 0.5),
        [1.5625, 1.125, 2.75, 1.875, 0.6875]
    );
    for (offset, scale) in [
        (0.0, 0.5),
        (0.5, 0.5),
        (1.0, -1.0),
        (0.25, 0.125),
        (0.75, 0.0),
    ] {
        let restricted = polynomial::restrict_power(power, offset, scale);
        let control = polynomial::power_to_bernstein(restricted);
        for i in 0..=128 {
            let t = f64::from(i) / 128.0;
            close(
                casteljau(control, t),
                horner(power, offset + scale * t),
                28.0,
            );
        }
    }
    let left = polynomial::restrict_power(power, 0.0, 0.5);
    let right = polynomial::restrict_power(power, 0.5, 0.5);
    assert_eq!(horner(left, 1.0), horner(right, 0.0));
    // The physical derivative is the local derivative divided by interval width.
    let left_derivative = left[1] + 2.0 * left[2] + 3.0 * left[3] + 4.0 * left[4];
    assert_eq!(left_derivative, right[1]);
}

#[test]
fn hermite_difference_and_positive_projection_bound_hold_in_the_interior() {
    // This exact bubble vanishes with its derivative at both endpoints.
    // Its quartic/cubic discrepancy reaches the r5/16 bound at t=1/2.
    let bubble = [0.0, 0.0, 16.0, -32.0, 16.0];
    assert_eq!(horner(bubble, 0.0), 0.0);
    assert_eq!(horner(bubble, 1.0), 0.0);
    assert_eq!(horner(bubble, 0.5), 1.0);
    assert_eq!(bubble[1], 0.0);
    assert_eq!(
        bubble[1] + 2.0 * bubble[2] + 3.0 * bubble[3] + 4.0 * bubble[4],
        0.0
    );
    for i in 0..=256 {
        let t = f64::from(i) / 256.0;
        close(
            horner(bubble, t),
            16.0 * t * t * (1.0 - t) * (1.0 - t),
            16.0,
        );
        assert!(horner(bubble, t) <= 1.0);
    }
    // q=(1,0,0). Positive projected Bernstein controls bound both raw norms
    // below; normalization can then amplify a raw difference by at most 2/m.
    let a = [
        [1.0, -0.08, 0.12, -0.04, 0.0],
        [0.0, 0.2, -0.1, 0.0, 0.0],
        [0.0; 5],
    ];
    let b = [
        [1.001, -0.081, 0.12, -0.04, 0.0],
        [0.001, 0.201, -0.1, 0.0, 0.0],
        [0.0; 5],
    ];
    let ca = a.map(polynomial::power_to_bernstein);
    let cb = b.map(polynomial::power_to_bernstein);
    let minimum = ca[0].into_iter().chain(cb[0]).fold(f64::INFINITY, f64::min);
    assert!(minimum > 0.9);
    let difference = std::array::from_fn::<_, 3, _>(|axis| {
        polynomial::power_to_bernstein(std::array::from_fn(|j| a[axis][j] - b[axis][j]))
    });
    let bound = (0..5)
        .map(|j| {
            difference
                .iter()
                .map(|axis| axis[j].powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .fold(0.0, f64::max);
    for i in 0..=256 {
        let t = f64::from(i) / 256.0;
        let av = a.map(|axis| horner(axis, t));
        let bv = b.map(|axis| horner(axis, t));
        let an = av.into_iter().map(|x| x * x).sum::<f64>().sqrt();
        let bn = bv.into_iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(an >= minimum && bn >= minimum);
        let delta = av
            .into_iter()
            .zip(bv)
            .map(|(x, y)| (x / an - y / bn).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!(delta <= 2.0 * bound / minimum);
    }
}
