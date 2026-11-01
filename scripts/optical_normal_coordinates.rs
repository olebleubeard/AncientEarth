//! Unused six-state normal-coordinate optical diagnostic. No runtime change.
//! H and two gnomonic normal coordinates replace Cartesian position; they are
//! not redundant auxiliary variables. The physical direction remains Cartesian.
use earth_math::{
    EnuBasis, Geodetic, Vec3d, WGS84_A_M, WGS84_E2, ecef_to_geodetic, geodetic_to_ecef,
};

#[derive(Clone, Copy, Debug)]
pub struct Chart {
    pub origin: Vec3d,
    pub up: Vec3d,
    pub east: Vec3d,
    pub north: Vec3d,
    pub height: f64,
    pub polar: Vec3d,
    pub n_radius: f64,
    pub sqrt_d: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct State {
    pub height: f64,
    pub a: f64,
    pub b: f64,
    pub direction: Vec3d,
}
impl Chart {
    pub fn at(origin: Vec3d) -> Option<Self> {
        let location = ecef_to_geodetic(origin).ok()?;
        let basis = EnuBasis::at(location);
        let polar = Vec3d::new(0., 0., 1.);
        let sqrt_d = (1. - WGS84_E2 * basis.up.z * basis.up.z).sqrt();
        Some(Self {
            origin,
            up: basis.up,
            east: basis.east,
            north: basis.north,
            height: location.height_m,
            polar,
            n_radius: WGS84_A_M / sqrt_d,
            sqrt_d,
        })
    }
    pub fn initial(self, direction: Vec3d) -> Option<State> {
        Some(State {
            height: self.height,
            a: 0.,
            b: 0.,
            direction: direction.normalized()?,
        })
    }
    pub fn normal(self, s: State) -> Option<(Vec3d, Vec3d)> {
        if !s.height.is_finite() || !s.a.is_finite() || !s.b.is_finite() || !s.direction.is_finite()
        {
            return None;
        }
        let q2 = s.a * s.a + s.b * s.b;
        // This explicit chart window contains every normal reached by a
        // <=2Mm physical path in the supported terrestrial normal tube.
        // A numerical stage outside it fails and must retry; it is not clamped.
        if q2 > 1. {
            return None;
        }
        let scale = (1. + q2).sqrt();
        let q = self.east * s.a + self.north * s.b;
        let n = (self.up + q) / scale;
        let delta = (q - self.up * (q2 / (scale + 1.))) / scale;
        Some((n, delta))
    }
    /// Cancellation-resistant position relative to the original eye. At the
    /// initial state every difference is exactly zero, including below h=0.
    pub fn position_delta(self, s: State) -> Option<Vec3d> {
        let (n, dn) = self.normal(s)?;
        let z = self.polar.dot(n);
        let dz = self.polar.dot(dn);
        let d = (1. - WGS84_E2 * z * z).sqrt();
        let radius = WGS84_A_M / d;
        let dr =
            WGS84_A_M * WGS84_E2 * dz * (z + self.up.z) / (d * self.sqrt_d * (d + self.sqrt_d));
        let tangent_delta = dn - self.polar * (WGS84_E2 * dz);
        let initial = self.up - self.polar * (WGS84_E2 * self.up.z);
        let p = tangent_delta * radius
            + initial * dr
            + dn * s.height
            + self.up * (s.height - self.height);
        p.is_finite().then_some(p)
    }
    pub fn position(self, s: State) -> Option<Vec3d> {
        Some(self.origin + self.position_delta(s)?)
    }

    /// Exact smooth ODE under the same normal-height index law. The off-unit
    /// Runge--Kutta direction extension is the same as the Cartesian solver.
    pub fn derivative(self, s: State, index: f64, slope: f64) -> Option<State> {
        let (n, _) = self.normal(s)?;
        if !index.is_finite() || index < 0.5 || !slope.is_finite() {
            return None;
        }
        let z = n.z;
        let d = 1. - WGS84_E2 * z * z;
        let sqrt_d = d.sqrt();
        let nr = WGS84_A_M / sqrt_d;
        let mr = WGS84_A_M * (1. - WGS84_E2) / (d * sqrt_d);
        if !(nr + s.height > 0. && mr + s.height > 0.) {
            return None;
        }
        let dh = s.direction.dot(n);
        let tangent = s.direction - n * dh;
        let meridian = self.polar - n * z;
        let coefficient = WGS84_A_M * WGS84_E2 / (d * sqrt_d * (mr + s.height) * (nr + s.height));
        let dn = tangent / (nr + s.height) + meridian * (coefficient * meridian.dot(s.direction));
        let divisor = n.dot(self.up);
        if !(divisor > 0.) {
            return None;
        }
        let radial = dn.dot(self.up);
        let result = State {
            height: dh,
            a: (dn.dot(self.east) - s.a * radial) / divisor,
            b: (dn.dot(self.north) - s.b * radial) / divisor,
            direction: (n - s.direction * dh) * (slope / index),
        };
        (result.height.is_finite()
            && result.a.is_finite()
            && result.b.is_finite()
            && result.direction.is_finite())
        .then_some(result)
    }
    /// Physical derivative of the represented coordinate curve. This can
    /// differ from its separately interpolated ray direction; plane roots must
    /// use this derivative, never assume P'=u for a finite dense interpolant.
    pub fn position_derivative(self, s: State, derivative: State) -> Option<Vec3d> {
        let (n, _) = self.normal(s)?;
        let w = self.east * derivative.a + self.north * derivative.b;
        let scale = (1. + s.a * s.a + s.b * s.b).sqrt();
        let dn = (w - n * n.dot(w)) / scale;
        let z = n.z;
        let d = 1. - WGS84_E2 * z * z;
        let nr = WGS84_A_M / d.sqrt();
        let p = n * derivative.height + dn * (nr + s.height)
            - (self.polar - n * z) * (nr * WGS84_E2 / d * dn.z);
        p.is_finite().then_some(p)
    }
    /// Conservative physical acceleration for any twice differentiable chart
    /// curve. q1/q2 bound |(a,b)'|/|(a,b)''|; h0/h1/h2 bound |H|/|H'|/|H''|.
    pub fn acceleration_bound(q1: f64, q2: f64, h0: f64, h1: f64, h2: f64) -> Option<f64> {
        if [q1, q2, h0, h1, h2]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
        {
            return None;
        }
        let d = 1. - WGS84_E2;
        let nr = WGS84_A_M / d.sqrt();
        let c = WGS84_A_M * WGS84_E2 / d.powf(1.5);
        let n2 = q2 + 3. * q1 * q1;
        let bound = h2 + 2. * h1 * q1 + (nr + h0 + c) * n2 + c * (3. + 3. * WGS84_E2 / d) * q1 * q1;
        bound.is_finite().then_some(bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chart_reconstruction_and_ode_preserve_canonical_normal_coordinates() {
        for latitude in [-90., -63., 0., 29.305549, 80., 90.] {
            for height in [-27.7, 0., 100., 20000.] {
                let chart = Chart::at(geodetic_to_ecef(
                    Geodetic::new(latitude, -94.767015, height).unwrap(),
                ))
                .unwrap();
                for (a, b, h) in [(0., 0., height), (1e-8, -2e-8, -27.), (0.2, -0.15, 120000.)] {
                    let s = State {
                        height: h,
                        a,
                        b,
                        direction: Vec3d::new(2., -3., 5.).normalized().unwrap(),
                    };
                    let (n, _) = chart.normal(s).unwrap();
                    let p = chart.position(s).unwrap();
                    let location = ecef_to_geodetic(p).unwrap();
                    assert!(
                        (location.height_m - h).abs() < 5e-9,
                        "{latitude} {a} {b} {h} {:?}",
                        location
                    );
                    assert!((EnuBasis::at(location).up - n).length() < 1e-15);
                    let rhs = chart.derivative(s, 1.0002, -1e-8).unwrap();
                    assert!(
                        (chart.position_derivative(s, rhs).unwrap() - s.direction).length() < 2e-15
                    );
                    assert!(
                        (rhs.direction - (n - s.direction * s.direction.dot(n)) * (-1e-8 / 1.0002))
                            .length()
                            < 1e-22
                    );
                }
                assert_eq!(
                    chart
                        .position_delta(chart.initial(Vec3d::new(1., 0., 0.)).unwrap())
                        .unwrap(),
                    Vec3d::ZERO
                );
            }
        }
    }
    #[test]
    fn physical_dense_derivative_matches_independent_symmetric_difference() {
        let chart = Chart::at(geodetic_to_ecef(Geodetic::new(29.3, -94.7, -27.).unwrap())).unwrap();
        let s = State {
            height: 50000.,
            a: 0.1,
            b: -0.2,
            direction: Vec3d::new(1., 0., 0.),
        };
        let derivative = State {
            height: 0.2,
            a: 1e-7,
            b: -0.5e-7,
            direction: Vec3d::ZERO,
        };
        let at = |t: f64| State {
            height: s.height + t * derivative.height,
            a: s.a + t * derivative.a,
            b: s.b + t * derivative.b,
            ..s
        };
        let finite =
            (chart.position_delta(at(1.)).unwrap() - chart.position_delta(at(-1.)).unwrap()) * 0.5;
        assert!((finite - chart.position_derivative(s, derivative).unwrap()).length() < 3e-10);
        assert!(Chart::acceleration_bound(1e-6, 1e-10, 120000., 1., 1e-6).unwrap() > 0.);
    }
}
