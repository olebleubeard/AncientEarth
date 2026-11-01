//! Independent finite checks of the unused height-parameter diagnostic.
//! Included as a child so the actual trial and dense-curve gates are tested.
use super::*;

fn vector(value: &Value, key: &str) -> Vec3d {
    let a = value[key].as_array().unwrap();
    Vec3d::new(
        a[0].as_f64().unwrap(),
        a[1].as_f64().unwrap(),
        a[2].as_f64().unwrap(),
    )
}

fn prescribed_frame(latitude: f64, longitude: f64, height: f64) -> (Chart, Vec3d, Vec3d, Vec3d) {
    let (s, c) = latitude.to_radians().sin_cos();
    let (sl, cl) = longitude.to_radians().sin_cos();
    let n = Vec3d::new(c * cl, c * sl, s);
    let east = Vec3d::new(-sl, cl, 0.);
    let north = Vec3d::new(-s * cl, -s * sl, c);
    let radius = WGS84_A_M / (1. - WGS84_E2 * s * s).sqrt();
    let point = Vec3d::new(
        (radius + height) * n.x,
        (radius + height) * n.y,
        (radius * (1. - WGS84_E2) + height) * n.z,
    );
    (Chart::at(point).unwrap(), n, east, north)
}

#[test]
fn transformed_rhs_matches_principal_curvatures_in_both_height_directions() {
    for latitude in [-90_f64, -57., 0., 29.3, 83., 90.] {
        for height in [-30., 2000., 120000.] {
            let (chart, normal, east, north) = prescribed_frame(latitude, 37., height);
            for vertical in [-0.8_f64, -0.15, 0.15, 0.8] {
                let u = normal * vertical + east * 0.3 + north * 0.4;
                let y = [0., 0., u.x, u.y, u.z, 17., 0.02];
                let branch = Branch {
                    anchor: height,
                    refractivity: 0.0003,
                    gradient: -2e-8,
                };
                let actual = rhs(chart, height, y, branch, vertical.signum()).unwrap();
                let d = 1. - WGS84_E2 * normal.z * normal.z;
                let nr = WGS84_A_M / d.sqrt();
                let mr = WGS84_A_M * (1. - WGS84_E2) / d.powf(1.5);
                // Principal-curvature construction, independent of the chart's
                // pole-safe shape-operator implementation.
                let dn_dh = (east * (u.dot(east) / (nr + height))
                    + north * (u.dot(north) / (mr + height)))
                    / vertical;
                let chart_dn = chart.east * actual[0] + chart.north * actual[1];
                assert!((chart_dn - dn_dh).length() < 3e-21);
                let expected_bend = (normal - u * vertical) * (-2e-8 / (1.0003 * vertical));
                assert!(
                    (Vec3d::new(actual[2], actual[3], actual[4]) - expected_bend).length() < 2e-21
                );
                assert!((actual[5] - 1. / vertical).abs() < 2e-13);
                assert!((actual[6] - 0.0003 / vertical).abs() < 1e-16);
                let physical = chart
                    .position_derivative(
                        state(height, y),
                        State {
                            height: 1.,
                            a: actual[0],
                            b: actual[1],
                            direction: Vec3d::ZERO,
                        },
                    )
                    .unwrap();
                assert!((physical - u / vertical).length() < 2e-13);
            }
        }
    }
}

#[test]
fn monotone_equatorial_paths_match_the_exact_straight_line_intersections() {
    let start_height = 2000.;
    let r = WGS84_A_M + start_height;
    for vertical in [-0.9_f64, -0.5, -0.1, 0.1, 0.5, 0.9] {
        let direction = Vec3d::new(vertical, (1. - vertical * vertical).sqrt(), 0.);
        let target_height = start_height + 1000. * vertical.signum();
        let rt = WGS84_A_M + target_height;
        let radial_square_difference = (target_height - start_height) * (rt + r);
        let discriminant_root = (r * r * vertical * vertical + radial_square_difference).sqrt();
        // The nearer positive intersection for a descending ray, and the
        // outward intersection for an ascending ray; no numerical ODE oracle.
        let distance = if vertical > 0. {
            radial_square_difference / (discriminant_root + r * vertical)
        } else {
            -radial_square_difference / (discriminant_root - r * vertical)
        };
        let expected_point = Vec3d::new(r, 0., 0.) + direction * distance;
        let inputs = json!({"origin_ecef_m":[r,0.,0.],
            "initial_direction":[direction.x,direction.y,direction.z],
            "layers":[{"height_m":-10000.,"refractivity":0.0003},
                      {"height_m":120000.,"refractivity":0.0003}]});
        let result = integrate(&inputs, target_height, 20000., 1e-8).unwrap();
        assert!(
            (vector(&result, "endpoint_ecef_m") - expected_point).length() < 2e-6,
            "vertical={vertical}: {result}"
        );
        assert!((vector(&result, "exit_direction") - direction).length() < 2e-15);
        assert!((result["geometric_path_m"].as_f64().unwrap() - distance).abs() < 2e-6);
        assert!((result["optical_path_m"].as_f64().unwrap() - 1.0003 * distance).abs() < 2e-6);
        assert!(result["minimum_certified_vertical"].as_f64().unwrap() > MIN_VERTICAL);
    }
}

// Expanded Cartesian normal-coordinate derivatives for a prescribed quartic.
// This uses the product rule directly, not Chart::position_derivative/bound.
fn independent_physical(dense: Dense, chart: Chart, h: f64, t: f64) -> (Vec3d, Vec3d) {
    let coefficient = |axis: usize| {
        [
            dense.start[axis],
            dense.r[0][axis] + dense.r[1][axis],
            dense.r[2][axis] - dense.r[1][axis] + dense.r[3][axis],
            -dense.r[2][axis] - 2. * dense.r[3][axis],
            dense.r[3][axis],
        ]
    };
    let evaluate = |c: [f64; 5]| {
        let value = c[0] + c[1] * t + c[2] * t * t + c[3] * t.powi(3) + c[4] * t.powi(4);
        let first =
            (c[1] + 2. * c[2] * t + 3. * c[3] * t * t + 4. * c[4] * t.powi(3)) / dense.step_h;
        let second = (2. * c[2] + 6. * c[3] * t + 12. * c[4] * t * t) / dense.step_h.powi(2);
        (value, first, second)
    };
    let (a, da, dda) = evaluate(coefficient(0));
    let (b, db, ddb) = evaluate(coefficient(1));
    let x = chart.up + chart.east * a + chart.north * b;
    let dx = chart.east * da + chart.north * db;
    let ddx = chart.east * dda + chart.north * ddb;
    let scale = x.length();
    let ds = x.dot(dx) / scale;
    let dds = (dx.dot(dx) + x.dot(ddx) - ds * ds) / scale;
    let n = x / scale;
    let dn = dx / scale - x * (ds / scale.powi(2));
    let ddn = ddx / scale - dx * (2. * ds / scale.powi(2)) - x * (dds / scale.powi(2))
        + x * (2. * ds * ds / scale.powi(3));
    let d = 1. - WGS84_E2 * n.z * n.z;
    let nr = WGS84_A_M / d.sqrt();
    let c = WGS84_A_M * WGS84_E2 / d.powf(1.5);
    let dnr = c * n.z * dn.z;
    let ddnr = c * (dn.z * dn.z + n.z * ddn.z) + 3. * c * WGS84_E2 * n.z * n.z * dn.z * dn.z / d;
    let polar = Vec3d::new(0., 0., 1.);
    let height = h + dense.step_h * t;
    let dp = (n - polar * (WGS84_E2 * n.z)) * dnr
        + (dn - polar * (WGS84_E2 * dn.z)) * nr
        + n
        + dn * height;
    let ddp = (n - polar * (WGS84_E2 * n.z)) * ddnr
        + (dn - polar * (WGS84_E2 * dn.z)) * (2. * dnr)
        + (ddn - polar * (WGS84_E2 * ddn.z)) * nr
        + dn * 2.
        + ddn * height;
    (dp, ddp)
}

#[test]
fn dense_height_curves_certify_monotonicity_and_physical_plane_contacts() {
    for latitude in [-65., 0., 71.] {
        let h = 2000.;
        let (chart, up, east, north) = prescribed_frame(latitude, -94.7, h);
        for orientation in [-1_f64, 1.] {
            let u = up * (0.6 * orientation) + east * 0.48 + north * 0.64;
            let dense = trial(
                chart,
                h,
                [0., 0., u.x, u.y, u.z, 0., 0.],
                200. * orientation,
                Branch {
                    anchor: h,
                    refractivity: 0.0003,
                    gradient: -2e-8,
                },
            )
            .unwrap();
            let bound = dense.physical_acceleration_bound(h).unwrap();
            let mut previous_path = -1.;
            for i in 0..=64 {
                let theta = f64::from(i) / 64.;
                let y = dense.at(theta);
                let x = chart.up + chart.east * y[0] + chart.north * y[1];
                let n = x / x.length();
                let direction = Vec3d::new(y[2], y[3], y[4]);
                assert!(orientation * direction.dot(n) >= dense.vertical_lower);
                assert!(y[5] > previous_path);
                assert!(dense.derivative(theta)[5] * dense.step_h > 0.);
                assert!(y[5] <= dense.path_upper);
                previous_path = y[5];
                let (dp, ddp) = independent_physical(dense, chart, h, theta);
                assert!(
                    (dense.physical_derivative(chart, h, theta).unwrap() - dp).length() < 2e-14
                );
                assert!(ddp.length() <= bound);
                let correction = 1e-5;
                let charge = dense.endpoint_coordinate_charge(h, correction);
                assert!(charge >= correction * dp.length());
                assert!(
                    charge
                        >= correction
                            * (dense.derivative(theta)[5].abs() + dense.derivative(theta)[6].abs())
                );
            }
            for target in [0.25, 0.5, 0.75] {
                let point = |t| {
                    chart
                        .position_delta(state(h + dense.step_h * t, dense.at(t)))
                        .unwrap()
                };
                let target_point = point(target);
                let plane_normal = independent_physical(dense, chart, h, target)
                    .0
                    .normalized()
                    .unwrap()
                    * orientation;
                let first_derivative =
                    dense.physical_derivative(chart, h, 0.).unwrap() * orientation;
                assert!(first_derivative.dot(plane_normal) > bound * dense.step_h.abs());
                let residual = |t| plane_normal.dot(point(t) - target_point);
                assert!(residual(0.) < 0. && residual(1.) > 0.);
                let (mut lo, mut hi) = (0., 1.);
                for _ in 0..48 {
                    let mid = (lo + hi) * 0.5;
                    if residual(mid) < 0. {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                assert!(((lo + hi) * 0.5 - target).abs() * dense.step_h.abs() < 2e-8);
            }
        }
    }
}

#[test]
fn height_conditioning_refuses_tangent_wrong_direction_and_true_turning_paths() {
    let h = 2000.;
    let (chart, up, east, _) = prescribed_frame(0., 0., h);
    let branch = Branch {
        anchor: h,
        refractivity: 0.,
        gradient: 0.,
    };
    for orientation in [-1_f64, 1.] {
        for vertical in [0., 0.005, MIN_VERTICAL] {
            let u = up * (orientation * vertical) + east * (1. - vertical * vertical).sqrt();
            assert!(
                trial(
                    chart,
                    h,
                    [0., 0., u.x, u.y, u.z, 0., 0.],
                    orientation,
                    branch
                )
                .is_none()
            );
        }
        let u = up * (-0.5 * orientation) + east * (0.75_f64).sqrt();
        assert!(
            trial(
                chart,
                h,
                [0., 0., u.x, u.y, u.z, 0., 0.],
                orientation,
                branch
            )
            .is_none()
        );
    }
    let vertical = -0.02_f64;
    let u = up * vertical + east * (1. - vertical * vertical).sqrt();
    let y = [0., 0., u.x, u.y, u.z, 0., 0.];
    assert!(trial(chart, h, y, -1., branch).is_some());
    // For an equatorial vacuum line, closest approach is exactly r*sqrt(1-v²).
    // A target 1500 m lower lies beyond that line's minimum radius (~1276 m
    // lower), so no monotone-height continuation can reach it.
    let radius = WGS84_A_M + h;
    let turning_drop = radius * (1. - (1. - vertical * vertical).sqrt());
    assert!(turning_drop > 1200. && turning_drop < 1300.);
    assert!(trial(chart, h, y, -1500., branch).is_none());

    // A finite local error check alone cannot establish stored progress.
    // All f32 inputs below are exact f64 values after conversion, giving an
    // independent arithmetic reference for the actual encoded addition.
    let offset = f32::from_bits(64_f32.to_bits() - 1);
    let tiny = 2_f32.powi(-22);
    let stored = offset + tiny;
    let exact = f64::from(offset) + f64::from(tiny);
    assert!(exact > f64::from(offset));
    assert_eq!(stored.to_bits(), offset.to_bits());
    assert!((exact - f64::from(stored)).abs() < 0.002);
    assert!(!(stored > offset));

    // Likewise, a stored endpoint can cross a source boundary although its
    // exact affine endpoint is inside and its rounding error is under 2 mm.
    // The strict source-gap check is independent of the total error budget.
    let offset = 10000_f32;
    let step = 0.0007_f32;
    let remaining = 0.0008_f32;
    let exact_delta = f64::from(step);
    let stored_delta = f64::from(offset + step) - f64::from(offset);
    assert!(exact_delta < f64::from(remaining));
    assert!(stored_delta > f64::from(remaining));
    assert!((stored_delta - exact_delta).abs() < 0.002);
}
