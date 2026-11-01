//! Independent finite checks of the unused normal-coordinate state.
#[allow(dead_code)]
#[path = "optical_normal_coordinates.rs"]
mod implementation;

use earth_math::{Vec3d, WGS84_A_M, WGS84_E2};
use implementation::{Chart, State};

fn frame(latitude: f64, longitude: f64) -> (Vec3d, Vec3d, Vec3d, f64, f64) {
    let (s, c) = latitude.to_radians().sin_cos();
    let (sl, cl) = longitude.to_radians().sin_cos();
    let d = 1. - WGS84_E2 * s * s;
    (
        Vec3d::new(c * cl, c * sl, s),
        Vec3d::new(-sl, cl, 0.),
        Vec3d::new(-s * cl, -s * sl, c),
        WGS84_A_M / d.sqrt(),
        WGS84_A_M * (1. - WGS84_E2) / d.powf(1.5),
    )
}

#[test]
fn cartesian_coordinates_and_principal_curvatures_are_independent_oracles() {
    for latitude in [-90., -70., -30., 0., 29.3, 80., 90.] {
        for longitude in [-179.9, -94.7, 0., 140.] {
            let (n0, _, _, nr0, _) = frame(latitude, longitude);
            let origin =
                Vec3d::new(nr0 * n0.x, nr0 * n0.y, nr0 * (1. - WGS84_E2) * n0.z) + n0 * -27.;
            let chart = Chart::at(origin).unwrap();
            for offset in [-1., 0., 1.] {
                let target_lat = (latitude + offset).clamp(-90., 90.);
                let (n, east, north, nr, mr) = frame(target_lat, longitude + offset);
                for height in [-10000., 0., 120000.] {
                    let state = State {
                        height,
                        a: n.dot(chart.east) / n.dot(chart.up),
                        b: n.dot(chart.north) / n.dot(chart.up),
                        // Deliberately non-unit stage direction exercises the
                        // stated off-unit extension, not just physical endpoints.
                        direction: east * 0.4 + north * -0.7 + n * 0.6,
                    };
                    let expected = Vec3d::new(
                        (nr + height) * n.x,
                        (nr + height) * n.y,
                        (nr * (1. - WGS84_E2) + height) * n.z,
                    );
                    assert!((chart.position(state).unwrap() - expected).length() < 8e-9);
                    let rhs = chart.derivative(state, 1.0003, -2e-8).unwrap();
                    let w = chart.east * rhs.a + chart.north * rhs.b;
                    let scale = (1. + state.a * state.a + state.b * state.b).sqrt();
                    let actual_dn = (w - n * n.dot(w)) / scale;
                    let expected_dn = east * (state.direction.dot(east) / (nr + height))
                        + north * (state.direction.dot(north) / (mr + height));
                    assert!((actual_dn - expected_dn).length() < 4e-22);
                    assert!((rhs.height - state.direction.dot(n)).abs() < 5e-16);
                    let expected_du =
                        (n - state.direction * state.direction.dot(n)) * (-2e-8 / 1.0003);
                    assert!((rhs.direction - expected_du).length() < 4e-23);
                }
            }
        }
    }
}

#[test]
fn expanded_second_derivative_stays_within_the_chart_curve_acceleration_bound() {
    let (normal, _, _, radius, _) = frame(45., 30.);
    let chart = Chart::at(Vec3d::new(
        radius * normal.x,
        radius * normal.y,
        radius * (1. - WGS84_E2) * normal.z,
    ))
    .unwrap();
    let polar = Vec3d::new(0., 0., 1.);
    for speed in [1e-7_f64, 0.02] {
        let av = speed;
        let bv = -0.6 * speed;
        let aa = 0.2 * speed;
        let ba = -0.3 * speed;
        let q1 = (av * av + bv * bv).sqrt() + (aa * aa + ba * ba).sqrt();
        let q2 = (aa * aa + ba * ba).sqrt();
        let bound = Chart::acceleration_bound(q1, q2, 52002., 2004., 4.).unwrap();
        for i in -50..=50 {
            let t = f64::from(i) / 50.;
            let a = 0.1 + av * t + 0.5 * aa * t * t;
            let b = -0.2 + bv * t + 0.5 * ba * t * t;
            let x = chart.up + chart.east * a + chart.north * b;
            let w = chart.east * (av + aa * t) + chart.north * (bv + ba * t);
            let acceleration = chart.east * aa + chart.north * ba;
            let r = x.length();
            let dr = x.dot(w) / r;
            let ddr = (w.dot(w) + x.dot(acceleration) - dr * dr) / r;
            let n = x / r;
            let dn = w / r - x * (dr / (r * r));
            let ddn = acceleration / r - w * (2. * dr / (r * r)) - x * (ddr / (r * r))
                + x * (2. * dr * dr / (r * r * r));
            let d = 1. - WGS84_E2 * n.z * n.z;
            let nr = WGS84_A_M / d.sqrt();
            let c = WGS84_A_M * WGS84_E2 / d.powf(1.5);
            let dnr = c * n.z * dn.z;
            let ddnr =
                c * (dn.z * dn.z + n.z * ddn.z) + 3. * c * WGS84_E2 * n.z * n.z * dn.z * dn.z / d;
            let h = 50000. + 2000. * t + 2. * t * t;
            let dh = 2000. + 4. * t;
            // Product-rule expansion of the Cartesian normal-coordinate map,
            // independently of the implementation's differential/bound formula.
            let ddp = (n - polar * (WGS84_E2 * n.z)) * ddnr
                + (dn - polar * (WGS84_E2 * dn.z)) * (2. * dnr)
                + (ddn - polar * (WGS84_E2 * ddn.z)) * nr
                + n * 4.
                + dn * (2. * dh)
                + ddn * h;
            assert!(
                ddp.length() <= bound,
                "{speed} {t}: {} > {bound}",
                ddp.length()
            );
        }
    }
}
