//! Unused direct-dense controller experiments. Bounds compare numerical
//! polynomials, not the unknown exact ODE solution.
use super::{
    AtmosphereProfile, AuxState, Branch, DenseStep, Error, State, Vec3d, WGS84_A_M, WGS84_E2,
    dopri_data, geometry,
};
#[path = "optical_polynomial.rs"]
mod polynomial;
use polynomial::{power_to_bernstein, restrict_power};

fn add(a: State, b: State) -> State {
    State {
        p: a.p + b.p,
        u: a.u + b.u,
        optical: a.optical + b.optical,
    }
}
fn sub(a: State, b: State) -> State {
    State {
        p: a.p - b.p,
        u: a.u - b.u,
        optical: a.optical - b.optical,
    }
}
fn mul(a: State, scale: f64) -> State {
    State {
        p: a.p * scale,
        u: a.u * scale,
        optical: a.optical * scale,
    }
}
fn from_components(v: [f64; 7]) -> State {
    State {
        p: Vec3d::new(v[0], v[1], v[2]),
        u: Vec3d::new(v[3], v[4], v[5]),
        optical: v[6],
    }
}
fn components(v: State) -> [f64; 7] {
    [v.p.x, v.p.y, v.p.z, v.u.x, v.u.y, v.u.z, v.optical]
}
fn transform(a: [State; 5], apply: impl Fn([f64; 5]) -> [f64; 5]) -> [State; 5] {
    let transformed: [[f64; 5]; 7] = std::array::from_fn(|component| {
        apply(std::array::from_fn(|i| components(a[i])[component]))
    });
    std::array::from_fn(|i| {
        from_components(std::array::from_fn(|component| transformed[component][i]))
    })
}
fn power(step: DenseStep, origin: State) -> [State; 5] {
    let [r2, r3, r4, r5] = step.r;
    [
        sub(step.start, origin),
        add(r2, r3),
        add(sub(r4, r3), r5),
        sub(mul(r5, -2.0), r4),
        r5,
    ]
}
fn lower_direction_norm(step: DenseStep, cubic: bool) -> Result<f64, Error> {
    let mut coefficients = power(
        step,
        State {
            p: Vec3d::ZERO,
            u: Vec3d::ZERO,
            optical: 0.0,
        },
    );
    if cubic {
        // Remove r5*t²*(1−t)², preserving endpoint values and derivatives.
        coefficients[2] = sub(coefficients[2], step.r[3]);
        coefficients[3] = add(coefficients[3], mul(step.r[3], 2.0));
        coefficients[4] = sub(coefficients[4], step.r[3]);
    }
    let controls = transform(coefficients, power_to_bernstein);
    let minimum = controls
        .into_iter()
        .map(|v| v.u.dot(step.start.u))
        .fold(f64::INFINITY, f64::min);
    if !minimum.is_finite() || minimum <= 0.0 {
        return Err("dense direction lower norm".into());
    }
    Ok(minimum)
}

/// Interior discrepancy between quartic output and its cubic Hermite partner.
/// This is an error ESTIMATOR, even though its polynomial-pair bound is exact.
pub(super) fn hermite_error(step: DenseStep) -> Result<f64, Error> {
    let lower = lower_direction_norm(step, false)?.min(lower_direction_norm(step, true)?);
    let difference = step.r[3];
    let mut interior = difference
        .p
        .length()
        .max(2.0 * step.step * difference.u.length() / lower)
        .max(difference.optical.abs())
        / 16.0;
    if let Some(aux) = step.auxiliary {
        let [a, b, c, d] = aux.r;
        interior = interior.max(d.h.abs() / 16.0);
        if aux.normal_evolved {
            let q = aux.start.n;
            let projected = [
                q.dot(q),
                (a.n + b.n).dot(q),
                (c.n - b.n + d.n).dot(q),
                (-c.n - d.n * 2.0).dot(q),
                d.n.dot(q),
            ];
            let cubic = [
                projected[0],
                projected[1],
                projected[2] - projected[4],
                projected[3] + 2.0 * projected[4],
                0.0,
            ];
            let minimum = power_to_bernstein(projected)
                .into_iter()
                .chain(power_to_bernstein(cubic))
                .fold(f64::INFINITY, f64::min);
            if !minimum.is_finite() || minimum <= 0.0 {
                return Err("auxiliary dense normal lower bound".into());
            }
            let radius = WGS84_A_M / (1.0 - WGS84_E2).sqrt()
                + aux.start.h.abs()
                + aux.r.iter().map(|v| v.h.abs()).sum::<f64>();
            interior = interior
                .max(d.h.abs() / 16.0)
                .max(2.0 * d.n.length() / minimum * radius / 16.0);
        }
    }
    if !interior.is_finite() {
        return Err("nonfinite dense discrepancy".into());
    }
    Ok(step.error + interior)
}

#[derive(Clone, Copy)]
pub(super) struct DenseCurve {
    first: DenseStep,
    second: Option<DenseStep>,
    error: f64,
}
impl DenseCurve {
    pub(super) fn single(first: DenseStep) -> Self {
        Self {
            first,
            second: None,
            error: first.error,
        }
    }
    pub(super) fn hermite(first: DenseStep) -> Result<Self, Error> {
        Ok(Self {
            first,
            second: None,
            error: hermite_error(first)?,
        })
    }
    pub(super) fn refined(
        profile: &AtmosphereProfile,
        full: DenseStep,
        branch: Option<Branch>,
        auxiliary: bool,
    ) -> Result<Self, Error> {
        let first = dopri_data(profile, full.start, full.step * 0.5, branch, auxiliary)?;
        // Use the actually normalized accepted first-half direction.
        let second = dopri_data(profile, first.end, full.step * 0.5, branch, auxiliary)?;
        let whole_power = power(full, full.start);
        let lower_full = lower_direction_norm(full, false)?;
        let mut discrepancy: f64 = 0.0;
        for (index, half) in [first, second].into_iter().enumerate() {
            let restricted = transform(whole_power, |p| restrict_power(p, index as f64 * 0.5, 0.5));
            let half_power = power(half, full.start);
            let difference = std::array::from_fn(|i| sub(restricted[i], half_power[i]));
            let controls = transform(difference, power_to_bernstein);
            let minimum_norm = lower_full.min(lower_direction_norm(half, false)?);
            for control in controls {
                discrepancy = discrepancy
                    .max(control.p.length())
                    .max(2.0 * full.step * control.u.length() / minimum_norm)
                    .max(control.optical.abs());
            }
        }
        let error = full.error.max(first.error + second.error) + discrepancy;
        if !error.is_finite() {
            return Err("nonfinite refined discrepancy".into());
        }
        Ok(Self {
            first,
            second: Some(second),
            error,
        })
    }
    pub(super) fn end(self) -> State {
        self.second.unwrap_or(self.first).end
    }
    pub(super) fn error(self) -> f64 {
        self.error
    }
    fn part(self, distance: f64) -> (DenseStep, f64) {
        match self.second {
            Some(second) if distance > self.first.step => (second, distance - self.first.step),
            _ => (self.first, distance),
        }
    }
    pub(super) fn at(self, distance: f64) -> Result<State, Error> {
        let (part, distance) = self.part(distance);
        part.at(distance)
    }
    pub(super) fn position_derivative(self, distance: f64) -> Vec3d {
        let (part, distance) = self.part(distance);
        part.position_derivative(distance)
    }
    pub(super) fn auxiliary_at(self, distance: f64) -> Result<AuxState, Error> {
        let (part, distance) = self.part(distance);
        part.auxiliary
            .ok_or("missing auxiliary curve")?
            .at(distance)
    }
    pub(super) fn auxiliary_height_derivative(self, distance: f64) -> Result<f64, Error> {
        let (part, distance) = self.part(distance);
        Ok(part
            .auxiliary
            .ok_or("missing auxiliary curve")?
            .height_derivative(distance))
    }
    pub(super) fn auxiliary_height_curvature(self) -> Result<f64, Error> {
        Ok(self
            .first
            .auxiliary
            .ok_or("missing auxiliary curve")?
            .height_curvature())
    }
    pub(super) fn height_second_derivative_bound(self) -> Result<f64, Error> {
        let first = self.first.height_second_derivative_bound()?;
        Ok(match self.second {
            Some(second) => first.max(second.height_second_derivative_bound()?),
            None => first,
        })
    }
    /// Restrict a crossing bracket to one smooth polynomial, so a normalized
    /// direction at the half-step join cannot invalidate its curvature bound.
    pub(super) fn root_bracket(self, target: f64, ascending: bool) -> Result<(f64, f64), Error> {
        match self.second {
            Some(second) => {
                let middle = geometry(self.first.end.p)?.0;
                if (ascending && middle >= target) || (!ascending && middle <= target) {
                    Ok((0.0, self.first.step))
                } else {
                    Ok((self.first.step, self.first.step + second.step))
                }
            }
            None => Ok((0.0, self.first.step)),
        }
    }
    pub(super) fn plane_bracket(
        self,
        point: Vec3d,
        normal: Vec3d,
    ) -> Result<(f64, f64, f64), Error> {
        let lower = |part: DenseStep| {
            part.position_derivative(0.0).dot(normal) - part.acceleration_bound() * part.step
        };
        let minimum = lower(self.first).min(self.second.map_or(f64::INFINITY, lower));
        if !minimum.is_finite() || minimum <= 0.0 {
            return Err("nonmonotone dense plane".into());
        }
        if (self.first.start.p - point).dot(normal) > 0.0
            || (self.end().p - point).dot(normal) < 0.0
        {
            return Err("unbracketed dense plane".into());
        }
        match self.second {
            Some(second) if (self.first.end.p - point).dot(normal) < 0.0 => {
                Ok((self.first.step, self.first.step + second.step, minimum))
            }
            _ => Ok((0.0, self.first.step, minimum)),
        }
    }
}
