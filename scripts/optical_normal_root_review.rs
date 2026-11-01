//! Independent finite polynomial-root and departure certificates.
use super::{Dense, Y};

fn height_curve(start: f64, power: [f64; 4], step: f64) -> Dense {
    // Expand theta*(r2+(1-theta)*(r3+theta*(r4+(1-theta)*r5))).
    let [c1, c2, c3, c4] = power;
    let mut initial: Y = [0.; 7];
    initial[0] = start;
    initial[3] = 1.;
    let mut r = [[0.; 7]; 4];
    r[0][0] = c1 + c2 + c3 + c4;
    r[1][0] = -c2 - c3 - c4;
    r[2][0] = -c3 - 2. * c4;
    r[3][0] = c4;
    Dense {
        start: initial,
        r,
        step,
        error: 0.,
    }
}

#[test]
fn analytic_root_uncertainty_contains_asymmetric_linear_and_quadratic_contacts() {
    // The expected roots and target heights below are exact dyadic rationals;
    // neither the production root iteration nor its derivative finds the oracle.
    for step in [1., 1024., 131072.] {
        for fraction in [1. / 1024., 1. / 8., 3. / 8., 7. / 8., 1023. / 1024.] {
            for sign in [-1., 1.] {
                for quadratic in [0., 0.25] {
                    let curve = height_curve(128., [sign, sign * quadratic, 0., 0.], step);
                    let target = 128. + sign * (fraction + quadratic * fraction * fraction);
                    let allocation = 1e-8;
                    let root = curve
                        .height_root(target, allocation)
                        .expect("transverse root");
                    let exact_path = step * fraction;
                    assert!(
                        (root.distance - exact_path).abs() <= root.path_error,
                        "path={} exact={} bound={}",
                        root.distance,
                        exact_path,
                        root.path_error
                    );
                    assert!(
                        curve.contact_speed().unwrap() * root.path_error + root.height_error <= allocation
                    );
                    let actual_height = 128.
                        + sign
                            * ((root.distance / step) + quadratic * (root.distance / step).powi(2));
                    assert!((actual_height - target).abs() <= root.height_error);
                }
            }
        }
    }
    let curve = height_curve(0., [1., 0., 0., 0.], 1.);
    assert!(curve.height_root(0., 1e-6).is_none()); // No strict initial bracket.
    assert!(curve.height_root(1., 1e-6).is_none());
    assert!(curve.height_root(2., 1e-6).is_none());
    assert!(curve.height_root(0.5, 0.).is_none());
}

#[test]
fn tangent_departure_certifies_only_owned_side_without_a_later_return() {
    for sign in [-1., 1.] {
        for power in [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 1., 1., 0.],
            [0., 1., 0., 1.],
        ] {
            let curve = height_curve(1000., power.map(|x| sign * x), 10000.);
            assert!(curve.departure_safe(sign), "{power:?}");
            assert!(!curve.departure_safe(-sign));
        }
        // theta^2*(1-2theta): starts on the owned side, returns at theta=1/2.
        let returning = height_curve(1000., [0., sign, -2. * sign, 0.], 10000.);
        assert!(!returning.departure_safe(sign));
        // theta^2*(1-theta): endpoint contact is not a strict departure proof.
        let touches_end = height_curve(1000., [0., sign, -sign, 0.], 10000.);
        assert!(!touches_end.departure_safe(sign));
    }
    assert!(!height_curve(1000., [0.; 4], 1.).departure_safe(1.));
}
