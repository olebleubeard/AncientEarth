//! Independent exact-input exporter for the unused deviation RHS.
#[path = "optical_refractive_deviation.rs"]
mod candidate;
use candidate::{Deviation, rhs};
use serde_json::json;

fn unit(v: [f64; 3]) -> [f64; 3] {
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / norm)
}
fn bits<const N: usize>(v: [f64; N]) -> [u64; N] {
    v.map(f64::to_bits)
}

fn main() {
    let mut cases = Vec::new();
    for axis in [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]] {
        for q in [[0., 0.], [0.1, -0.2], [-0.4, 0.3]] {
            let normal = unit([q[0], 1., -q[1]]);
            let tangent = unit([normal[1], -normal[0], 0.]);
            for vertical_ratio in [0.05, 2.] {
                let launch = unit(std::array::from_fn(|i| {
                    tangent[i] + vertical_ratio * normal[i]
                }));
                for height in [-10000., 5000., 120001.] {
                    for scale in [
                        0.,
                        0.01,
                        1e-6,
                        2f64.powi(-30),
                        2f64.powi(-45),
                        2f64.powi(-60),
                        2f64.powi(-80),
                    ] {
                        for sign in [-1., 1.] {
                            let delta = sign * scale;
                            let state = Deviation {
                                geometric_chart: q,
                                chart_delta: [delta, -0.7 * delta],
                                direction_delta: [0.2 * delta, -0.3 * delta, 0.4 * delta],
                            };
                            let refractivity = 0.00025;
                            let gradient = -3e-8;
                            let result =
                                rhs(axis, launch, height, refractivity, gradient, state).unwrap();
                            let vacuum = rhs(
                                axis,
                                launch,
                                height,
                                0.,
                                0.,
                                Deviation {
                                    chart_delta: [0.; 2],
                                    direction_delta: [0.; 3],
                                    ..state
                                },
                            )
                            .unwrap();
                            assert_eq!(vacuum.chart_delta, [0.; 2]);
                            assert_eq!(vacuum.direction_delta, [0.; 3]);
                            assert_eq!(vacuum.path_delta, 0.);
                            assert_eq!(vacuum.optical_excess, 0.);
                            cases.push(json!({"axis_bits":bits(axis),"launch_bits":bits(launch),
                                "q0_bits":bits(q),"dq_bits":bits(state.chart_delta),"du_bits":bits(state.direction_delta),
                                "height_bits":height.to_bits(),"refractivity_bits":refractivity.to_bits(),"gradient_bits":gradient.to_bits(),
                                "result_bits":{
                                    "geometric_chart":bits(result.geometric_chart),"chart_delta":bits(result.chart_delta),
                                    "direction_delta":bits(result.direction_delta),"path_delta":result.path_delta.to_bits(),
                                    "optical_excess":result.optical_excess.to_bits()}}));
                        }
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&json!({
        "scope":"exact encoded input algebra; high-precision reviewer reconstructs the real split q0+dq and launch+du; no integration or whole-cell certificate",
        "a_bits":6378137_f64.to_bits(),"eccentricity_squared_bits":0.0066943799901413165_f64.to_bits(),"cases":cases
    })).unwrap());
}

#[cfg(test)]
mod independent_tests {
    use super::*;
    #[test]
    fn conditioning_domain_and_nonfinite_values_decline() {
        let state = Deviation {
            geometric_chart: [0.; 2],
            chart_delta: [0.; 2],
            direction_delta: [0.; 3],
        };
        let evaluate = |s, h, n, g, u| rhs([0., 0., 1.], u, h, n, g, s);
        for launch in [[0., -1., 0.], [1., 0., 0.], [1., 0.02, 0.]] {
            assert!(evaluate(state, 0., 0., 0., launch).is_none());
        }
        for height in [-10000.000001, 120001.000001, f64::INFINITY, f64::NAN] {
            assert!(evaluate(state, height, 0., 0., [0., 1., 0.]).is_none());
        }
        assert!(evaluate(state, 0., -0.50001, 0., [0., 1., 0.]).is_none());
        assert!(evaluate(state, 0., 0., f64::NAN, [0., 1., 0.]).is_none());
        assert!(
            evaluate(
                Deviation {
                    geometric_chart: [1.01, 0.],
                    ..state
                },
                0.,
                0.,
                0.,
                [0., 1., 0.]
            )
            .is_none()
        );
        assert!(
            evaluate(
                Deviation {
                    chart_delta: [f64::NAN, 0.],
                    ..state
                },
                0.,
                0.,
                0.,
                [0., 1., 0.]
            )
            .is_none()
        );
        assert!(
            evaluate(
                Deviation {
                    direction_delta: [0., -0.99, 0.],
                    ..state
                },
                0.,
                0.,
                0.,
                [0., 1., 0.]
            )
            .is_none()
        );
    }
}
