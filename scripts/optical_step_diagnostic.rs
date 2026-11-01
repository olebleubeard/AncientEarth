//! Read-only f64 step-controller diagnostic, not a production solver.
//! Replays only independently resolved clear rays; no scene events are omitted
//! from the reference measurement used to admit those rays.
use earth_math::{
    atmosphere::{
        AtmosphereProfile, KNOT_HEIGHT_BAND_M, MAX_ATMOSPHERIC_PATH_M, MAX_STEP_M, MAX_TRACE_STEPS,
        OpticalTrace, POSITION_TOLERANCE_M,
    },
    *,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
static GEOMETRY_CALLS: AtomicU64 = AtomicU64::new(0);
static DP_CALLS: AtomicU64 = AtomicU64::new(0);
#[path = "optical_seeded_normal.rs"]
mod seeded_normal;
type Error = Box<dyn std::error::Error>;
#[derive(Clone, Copy)]
struct State {
    p: Vec3d,
    u: Vec3d,
    optical: f64,
}
#[derive(Clone, Copy)]
enum Method {
    Midpoint,
    Rk4,
    Dopri54,
    Dopri54Auxiliary,
}
#[derive(Clone, Copy)]
struct Options {
    method: Method,
    preserve_proposal: bool,
    maximum_step: f64,
    tolerance: f64,
    target_path: Option<f64>,
    one_sided: bool,
    dense_events: bool,
    refined_dense: bool,
    hermite_dense: bool,
    carried_auxiliary: bool,
    height_only: bool,
    seeded_normals: bool,
}
impl Options {
    fn baseline(preserve_proposal: bool) -> Self {
        Self {
            method: Method::Midpoint,
            preserve_proposal,
            maximum_step: MAX_STEP_M,
            tolerance: POSITION_TOLERANCE_M,
            target_path: None,
            one_sided: false,
            dense_events: false,
            refined_dense: false,
            hermite_dense: false,
            carried_auxiliary: false,
            height_only: false,
            seeded_normals: false,
        }
    }
    fn rk4(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            method: Method::Rk4,
            preserve_proposal: true,
            maximum_step,
            tolerance,
            target_path: None,
            one_sided: true,
            dense_events: false,
            refined_dense: false,
            hermite_dense: false,
            carried_auxiliary: false,
            height_only: false,
            seeded_normals: false,
        }
    }
    fn dopri(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            method: Method::Dopri54,
            ..Self::rk4(maximum_step, tolerance)
        }
    }
    fn dopri_dense(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            dense_events: true,
            ..Self::dopri(maximum_step, tolerance)
        }
    }
    fn dopri_auxiliary(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            method: Method::Dopri54Auxiliary,
            ..Self::dopri_dense(maximum_step, tolerance)
        }
    }
    fn dopri_refined(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            refined_dense: true,
            ..Self::dopri_dense(maximum_step, tolerance)
        }
    }
    fn dopri_hermite(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            hermite_dense: true,
            ..Self::dopri_dense(maximum_step, tolerance)
        }
    }
    fn dopri_carried(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            carried_auxiliary: true,
            hermite_dense: true,
            ..Self::dopri_auxiliary(maximum_step, tolerance)
        }
    }
    fn dopri_height(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            height_only: true,
            ..Self::dopri_carried(maximum_step, tolerance)
        }
    }
    fn dopri_seeded(maximum_step: f64, tolerance: f64) -> Self {
        Self {
            seeded_normals: true,
            ..Self::dopri_height(maximum_step, tolerance)
        }
    }
}
#[derive(Clone, Copy)]
struct Branch {
    anchor: f64,
    index: f64,
    slope: f64,
    lower: f64,
    upper: f64,
}
fn branch(profile: &AtmosphereProfile, height: f64, vertical: f64) -> Branch {
    branch_with_band(profile, height, vertical, 1e-7)
}
// Persistent ownership cannot use the legacy near-knot reselection band: an
// unowned ray just below a discontinuity must first resolve that contact.
fn carried_branch_at(profile: &AtmosphereProfile, height: f64, vertical: f64) -> Branch {
    branch_at_index(
        profile,
        carried_branch_index(profile.layers(), profile.base_height_m(), height, vertical),
    )
}
fn carried_branch_index(
    knots: &[atmosphere::AtmosphereLayer],
    base: f64,
    height: f64,
    vertical: f64,
) -> usize {
    // Compare the same represented absolute knot used by the root solver.
    // Subtracting base again need not recover the original relative knot bits.
    let mut index = knots.partition_point(|k| base + k.height_m <= height);
    if vertical < 0.0 && index > 0 && height == base + knots[index - 1].height_m {
        index -= 1;
    }
    index
}
fn branch_with_band(profile: &AtmosphereProfile, height: f64, vertical: f64, band: f64) -> Branch {
    let knots = profile.layers();
    let relative = height - profile.base_height_m();
    // This is a numerical contact band, not a smoothing of source gradients.
    let mut index = knots.partition_point(|k| k.height_m <= relative);
    if vertical > 0.0 && index < knots.len() && knots[index].height_m - relative <= band {
        index += 1;
    }
    if vertical < 0.0 && index > 0 && relative - knots[index - 1].height_m <= band {
        index -= 1;
    }
    branch_at_index(profile, index)
}
fn branch_at_index(profile: &AtmosphereProfile, index: usize) -> Branch {
    let knots = profile.layers();
    if index == 0 {
        return Branch {
            anchor: profile.base_height_m(),
            index: 1.0 + knots[0].refractivity,
            slope: 0.0,
            lower: f64::NEG_INFINITY,
            upper: profile.base_height_m(),
        };
    }
    let i = (index - 1).min(knots.len() - 2);
    Branch {
        anchor: profile.base_height_m() + knots[i].height_m,
        index: 1.0 + knots[i].refractivity,
        slope: (knots[i + 1].refractivity - knots[i].refractivity)
            / (knots[i + 1].height_m - knots[i].height_m),
        lower: profile.base_height_m() + knots[i].height_m,
        upper: profile.base_height_m() + knots[i + 1].height_m,
    }
}
fn source_event(b: Branch, height: f64, end_height: f64, carried: bool) -> Option<f64> {
    let band = if carried { 0.0 } else { 1e-8 };
    if (end_height > b.upper || (carried && end_height == b.upper)) && height < b.upper - band {
        Some(b.upper)
    } else if (end_height < b.lower || (carried && end_height == b.lower))
        && height > b.lower + band
    {
        Some(b.lower)
    } else {
        None
    }
}
fn geometry(p: Vec3d) -> Result<(f64, Vec3d), Error> {
    GEOMETRY_CALLS.fetch_add(1, Ordering::Relaxed);
    let g = ecef_to_geodetic(p)?;
    Ok((g.height_m, EnuBasis::at(g).up))
}
fn midpoint(profile: &AtmosphereProfile, s: State, step: f64) -> Result<State, Error> {
    let derivative = |s: State| -> Result<(Vec3d, Vec3d, f64), Error> {
        let (h, up) = geometry(s.p)?;
        let (n, slope) = profile.index_gradient(h);
        Ok((s.u, (up - s.u * s.u.dot(up)) * (slope / n), n))
    };
    let (dp, du, n) = derivative(s)?;
    let middle = State {
        p: s.p + dp * (step * 0.5),
        u: (s.u + du * (step * 0.5)).normalized().ok_or("direction")?,
        optical: s.optical + n * step * 0.5,
    };
    let (dp, du, n) = derivative(middle)?;
    Ok(State {
        p: s.p + dp * step,
        u: (s.u + du * step).normalized().ok_or("direction")?,
        optical: s.optical + n * step,
    })
}
fn advance(profile: &AtmosphereProfile, s: State, step: f64) -> Result<State, Error> {
    midpoint(profile, midpoint(profile, s, step * 0.5)?, step * 0.5)
}
/// Classical fourth-order Runge--Kutta for the SAME arclength ODE. Intermediate
/// stage directions remain RK states; normalization projects only the accepted
/// endpoint back onto its exact unit-vector invariant. No source interpolation
/// or optical-medium approximation is introduced.
fn rk4(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    branch: Option<Branch>,
) -> Result<State, Error> {
    let derivative = |s: State| -> Result<State, Error> {
        let (height, up) = geometry(s.p)?;
        let (n, slope) = branch.map_or_else(
            || profile.index_gradient(height),
            |b| (b.index + b.slope * (height - b.anchor), b.slope),
        );
        Ok(State {
            p: s.u,
            u: (up - s.u * s.u.dot(up)) * (slope / n),
            optical: n,
        })
    };
    let add = |a: State, k: State, distance: f64| State {
        p: a.p + k.p * distance,
        u: a.u + k.u * distance,
        optical: a.optical + k.optical * distance,
    };
    let k1 = derivative(s)?;
    let k2 = derivative(add(s, k1, step * 0.5))?;
    let k3 = derivative(add(s, k2, step * 0.5))?;
    let k4 = derivative(add(s, k3, step))?;
    Ok(State {
        p: s.p + (k1.p + k2.p * 2.0 + k3.p * 2.0 + k4.p) * (step / 6.0),
        u: (s.u + (k1.u + k2.u * 2.0 + k3.u * 2.0 + k4.u) * (step / 6.0))
            .normalized()
            .ok_or("direction")?,
        optical: s.optical
            + (k1.optical + 2.0 * k2.optical + 2.0 * k3.optical + k4.optical) * (step / 6.0),
    })
}
/// Original implementation of the published Dormand--Prince5(4) tableau.
/// Rational mathematical coefficients verified against Hairer/Wanner's
/// primary reference, https://www.unige.ch/~hairer/prog/nonstiff/dopri5.f .
/// No source code from that implementation is copied here.
fn dopri(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    branch: Option<Branch>,
) -> Result<(State, f64), Error> {
    let result = dopri_data(profile, s, step, branch, false)?;
    Ok((result.end, result.error))
}
#[derive(Clone, Copy)]
struct DenseStep {
    start: State,
    end: State,
    error: f64,
    step: f64,
    r: [State; 4],
    auxiliary: Option<AuxDense>,
}
#[derive(Clone, Copy)]
struct AuxState {
    h: f64,
    n: Vec3d,
}
impl AuxState {
    fn consistency(self, p: Vec3d) -> Result<f64, Error> {
        let (h, n) = geometry(p)?;
        self.discrepancy(Self { h, n }, true)
    }
    fn discrepancy(self, actual: Self, compare_normal: bool) -> Result<f64, Error> {
        let Self { h, n } = actual;
        if !compare_normal {
            let error = (h - self.h).abs();
            return if error.is_finite() {
                Ok(error)
            } else {
                Err("nonfinite carried height consistency".into())
            };
        }
        let product = self.n.length() * n.length();
        if !product.is_finite() || product <= 0.0 || self.n.dot(n) <= 0.0 {
            return Err("carried normal comparison domain".into());
        }
        let radius = WGS84_A_M / (1.0 - WGS84_E2).sqrt() + self.h.abs();
        let error = (h - self.h)
            .abs()
            .max(std::f64::consts::SQRT_2 * self.n.cross(n).length() / product * radius);
        if !error.is_finite() {
            return Err("nonfinite carried consistency".into());
        }
        Ok(error)
    }
}
#[derive(Clone, Copy)]
struct AuxDense {
    start: AuxState,
    r: [AuxState; 4],
    step: f64,
    normal_evolved: bool,
}
impl AuxDense {
    fn at(self, distance: f64) -> Result<AuxState, Error> {
        let t = distance / self.step;
        let v = 1.0 - t;
        let [a, b, c, d] = self.r;
        let h = self.start.h + t * (a.h + v * (b.h + t * (c.h + v * d.h)));
        let raw = self.start.n + (a.n + (b.n + (c.n + d.n * v) * t) * v) * t;
        if !h.is_finite() {
            return Err("nonfinite auxiliary dense height".into());
        }
        Ok(AuxState {
            h,
            n: raw.normalized().ok_or("auxiliary dense normal")?,
        })
    }
    fn height_derivative(self, distance: f64) -> f64 {
        let t = distance / self.step;
        let [a, b, c, d] = self.r;
        (a.h + (1.0 - 2.0 * t) * b.h
            + (2.0 * t - 3.0 * t * t) * c.h
            + (2.0 * t - 6.0 * t * t + 4.0 * t * t * t) * d.h)
            / self.step
    }
    fn height_curvature(self) -> f64 {
        let [_, b, c, d] = self.r;
        (2.0 * b.h.abs() + 4.0 * c.h.abs() + 2.0 * d.h.abs()) / self.step.powi(2)
    }
}
impl DenseStep {
    fn at(self, distance: f64) -> Result<State, Error> {
        let t = distance / self.step;
        let v = 1.0 - t;
        let [r2, r3, r4, r5] = self.r;
        Ok(State {
            p: self.start.p + (r2.p + (r3.p + (r4.p + r5.p * v) * t) * v) * t,
            u: (self.start.u + (r2.u + (r3.u + (r4.u + r5.u * v) * t) * v) * t)
                .normalized()
                .ok_or("dense direction")?,
            optical: self.start.optical
                + t * (r2.optical + v * (r3.optical + t * (r4.optical + v * r5.optical))),
        })
    }
    fn position_derivative(self, distance: f64) -> Vec3d {
        let t = distance / self.step;
        let [r2, r3, r4, r5] = self.r;
        // Derivative of the position polynomial, not the separately
        // interpolated direction polynomial; used only for Newton proposals.
        (r2.p
            + r3.p * (1.0 - 2.0 * t)
            + r4.p * (2.0 * t - 3.0 * t * t)
            + r5.p * (2.0 * t - 6.0 * t * t + 4.0 * t * t * t))
            / self.step
    }
    fn height_second_derivative_bound(self) -> Result<f64, Error> {
        let [r2, r3, r4, r5] = self.r;
        let velocity = (r2.p.length() + r3.p.length() + r4.p.length() + r5.p.length()) / self.step;
        let acceleration = (2.0 * r3.p.length() + 4.0 * r4.p.length() + 2.0 * r5.p.length())
            / (self.step * self.step);
        // Geodetic height is one-Lipschitz within the normal tube. From a
        // starting height>=MIN, the polynomial cannot be lower than
        // MIN−V*L. This also bounds the Hessian without assuming all polynomial
        // points stayed within the physical tracing domain.
        let radius = WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M - velocity * self.step;
        if !radius.is_finite() || radius <= 0.0 {
            return Err("dense normal tube".into());
        }
        Ok(velocity * velocity / radius + acceleration)
    }
    fn acceleration_bound(self) -> f64 {
        let [_, r3, r4, r5] = self.r;
        (2.0 * r3.p.length() + 4.0 * r4.p.length() + 2.0 * r5.p.length()) / self.step.powi(2)
    }
}
#[path = "optical_dense_refinement.rs"]
mod dense_refinement;
use dense_refinement::DenseCurve;
fn dopri_data(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    branch: Option<Branch>,
    auxiliary: bool,
) -> Result<DenseStep, Error> {
    dopri_data_from(profile, s, step, branch, auxiliary, None, false, false)
}
fn dopri_data_from(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    branch: Option<Branch>,
    auxiliary: bool,
    carried: Option<AuxState>,
    height_only: bool,
    seeded_normals: bool,
) -> Result<DenseStep, Error> {
    DP_CALLS.fetch_add(1, Ordering::Relaxed);
    const A: [&[f64]; 7] = [
        &[],
        &[1.0 / 5.0],
        &[3.0 / 40.0, 9.0 / 40.0],
        &[44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0],
        &[
            19372.0 / 6561.0,
            -25360.0 / 2187.0,
            64448.0 / 6561.0,
            -212.0 / 729.0,
        ],
        &[
            9017.0 / 3168.0,
            -355.0 / 33.0,
            46732.0 / 5247.0,
            49.0 / 176.0,
            -5103.0 / 18656.0,
        ],
        &[
            35.0 / 384.0,
            0.0,
            500.0 / 1113.0,
            125.0 / 192.0,
            -2187.0 / 6784.0,
            11.0 / 84.0,
        ],
    ];
    const C: [f64; 7] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0, 1.0];
    const E: [f64; 7] = [
        71.0 / 57600.0,
        0.0,
        -71.0 / 16695.0,
        71.0 / 1920.0,
        -17253.0 / 339200.0,
        22.0 / 525.0,
        -1.0 / 40.0,
    ];
    // Published Shampine continuous-extension coefficients (1986), also
    // tabulated in Hairer/Wanner's primary reference linked above. These
    // mathematical coefficients are used in an original implementation.
    const D: [f64; 7] = [
        -12715105075.0 / 11282082432.0,
        0.0,
        87487479700.0 / 32700410799.0,
        -10690763975.0 / 1880347072.0,
        701980252875.0 / 199316789632.0,
        -1453857185.0 / 822651844.0,
        69997945.0 / 29380423.0,
    ];
    let zero = State {
        p: Vec3d::ZERO,
        u: Vec3d::ZERO,
        optical: 0.0,
    };
    let mut k = [zero; 7];
    let mut kh = [0.0; 7];
    let mut kn = [Vec3d::ZERO; 7];
    let initial_geometry = if let Some(value) = carried {
        Some((value.h, value.n))
    } else if auxiliary {
        Some(geometry(s.p)?)
    } else {
        None
    };
    let mut last_aux = (0.0, Vec3d::ZERO);
    let mut high = s;
    for i in 0..7 {
        let mut sum = zero;
        for (j, &coefficient) in A[i].iter().enumerate() {
            // Sum a_ij=c_i: subtract common translation/index before the
            // weighted sum. This also defines a cancellation-resistant GPU form.
            sum.p = sum.p + (k[j].p - s.u) * coefficient;
            sum.u = sum.u + k[j].u * coefficient;
            sum.optical += (k[j].optical - k[0].optical) * coefficient;
        }
        let stage = State {
            p: s.p + (s.u * C[i] + sum.p) * step,
            u: s.u + sum.u * step,
            optical: s.optical + (k[0].optical * C[i] + sum.optical) * step,
        };
        let (h, up) = if let Some((h0, n0)) = initial_geometry {
            let mut dh = 0.0;
            let mut dn = Vec3d::ZERO;
            for (j, &coefficient) in A[i].iter().enumerate() {
                dh += kh[j] * coefficient;
                dn = dn + kn[j] * coefficient;
            }
            let stage_height = h0 + dh * step;
            (
                stage_height,
                if height_only {
                    if seeded_normals {
                        seeded_normal::seeded_normal(stage.p, stage_height)
                            .ok_or("seeded normal scalar domain")?
                            .normal
                    } else {
                        geometry(stage.p)?.1
                    }
                } else {
                    n0 + dn * step
                },
            )
        } else {
            geometry(stage.p)?
        };
        if !h.is_finite() || !up.is_finite() || !stage.u.is_finite() {
            return Err("nonfinite RK stage geometry".into());
        }
        let physical_up = if auxiliary {
            up.normalized().ok_or("auxiliary normal norm")?
        } else {
            up
        };
        let (n, slope) = branch.map_or_else(
            || profile.index_gradient(h),
            |b| (b.index + b.slope * (h - b.anchor), b.slope),
        );
        // The exact branch extension bound does not bound every explicit
        // tableau stage (some coefficients are negative). Reject rather than
        // clamp a stage outside the branch's proved finite-index domain.
        if !n.is_finite() || n < 0.5 {
            return Err("one-sided RK stage index domain".into());
        }
        k[i] = State {
            p: stage.u,
            u: (physical_up - stage.u * stage.u.dot(physical_up)) * (slope / n),
            optical: n,
        };
        if !k[i].u.is_finite() {
            return Err("nonfinite RK derivative".into());
        }
        if auxiliary {
            kh[i] = stage.u.dot(physical_up);
            last_aux = (h, up);
        }
        if auxiliary && !height_only {
            // Smooth off-unit RK-stage extension: evaluate the physical field
            // with the normalized normal, without mutating the RK stage state.
            // Every accepted endpoint is checked against the coordinate inverse.
            let polar = Vec3d::new(0.0, 0.0, 1.0);
            let z = physical_up.z;
            let d = 1.0 - WGS84_E2 * z * z;
            if !d.is_finite() || d <= 0.0 {
                return Err("auxiliary curvature domain".into());
            }
            let n_radius = WGS84_A_M / d.sqrt();
            let m_radius = WGS84_A_M * (1.0 - WGS84_E2) / (d * d.sqrt());
            if n_radius + h <= 0.0 || m_radius + h <= 0.0 {
                return Err("auxiliary normal tube".into());
            }
            let q = polar - physical_up * z;
            kn[i] = (stage.u - physical_up * kh[i]) / (n_radius + h)
                + q * (WGS84_A_M * WGS84_E2 / (d * d.sqrt() * (m_radius + h) * (n_radius + h))
                    * stage.u.dot(q));
            if !kh[i].is_finite() || !kn[i].is_finite() {
                return Err("nonfinite auxiliary derivative".into());
            }
        }
        if i == 6 {
            high = stage;
        }
    }
    let mut difference = zero;
    for i in 0..7 {
        difference.p = difference.p + (k[i].p - k[0].p) * E[i];
        difference.u = difference.u + k[i].u * E[i];
        difference.optical += (k[i].optical - k[0].optical) * E[i];
    }
    let mut error = (difference.p.length() * step)
        .max(difference.u.length() * step * step)
        .max(difference.optical.abs() * step);
    if auxiliary {
        let mut eh = 0.0;
        let mut en = Vec3d::ZERO;
        for i in 0..7 {
            eh += (kh[i] - kh[0]) * E[i];
            en = en + kn[i] * E[i];
        }
        let (actual_height, actual_normal) = geometry(high.p)?;
        // Normal discrepancy is expressed in metres using the maximum WGS84
        // principal radius plus the current height enclosure. This is an
        // additional acceptance check, not a substitute for position/direction.
        let normal_scale = WGS84_A_M / (1.0 - WGS84_E2).sqrt() + last_aux.0.abs();
        if !eh.is_finite() || !en.is_finite() || !normal_scale.is_finite() {
            return Err("nonfinite auxiliary error norm".into());
        }
        let high_normal = last_aux.1;
        let low_normal = high_normal - en * step;
        let normal_error = |a: Vec3d, b: Vec3d, cross: Vec3d| -> Result<f64, Error> {
            let product = a.length() * b.length();
            if !product.is_finite() || product <= 0.0 || a.dot(b) <= 0.0 {
                return Err("auxiliary normal comparison domain".into());
            }
            // For positive dot, sqrt(2)*sin(theta) bounds unit-vector chord.
            Ok(std::f64::consts::SQRT_2 * cross.length() / product)
        };
        error = error
            .max(eh.abs() * step)
            .max((actual_height - last_aux.0).abs());
        if !height_only {
            error = error
                .max(
                    normal_error(high_normal, low_normal, (en * step).cross(high_normal))?
                        * normal_scale,
                )
                .max((actual_height - last_aux.0).abs())
                .max(
                    normal_error(high_normal, actual_normal, high_normal.cross(actual_normal))?
                        * normal_scale,
                );
        }
    }
    // Construct the dense polynomial without subtracting large ECEF positions
    // or large optical path totals. Sum(B)=1 and Sum(D)=0 exactly.
    let mut high_sum = zero;
    for (j, &b) in A[6].iter().enumerate() {
        high_sum.p = high_sum.p + (k[j].p - s.u) * b;
        high_sum.u = high_sum.u + k[j].u * b;
        high_sum.optical += (k[j].optical - k[0].optical) * b;
    }
    let r2 = State {
        p: (s.u + high_sum.p) * step,
        u: high_sum.u * step,
        optical: (k[0].optical + high_sum.optical) * step,
    };
    let r3 = State {
        p: -high_sum.p * step,
        u: k[0].u * step - r2.u,
        optical: -high_sum.optical * step,
    };
    let r4 = State {
        p: (s.u - k[6].p + high_sum.p * 2.0) * step,
        u: r2.u - k[6].u * step - r3.u,
        optical: (k[0].optical - k[6].optical + 2.0 * high_sum.optical) * step,
    };
    let mut r5 = zero;
    for i in 0..7 {
        r5.p = r5.p + (k[i].p - s.u) * (D[i] * step);
        r5.u = r5.u + k[i].u * (D[i] * step);
        r5.optical += (k[i].optical - k[0].optical) * (D[i] * step);
    }
    high.u = high.u.normalized().ok_or("direction")?;
    let auxiliary_dense = initial_geometry.map(|(h, n)| {
        let mut dh = 0.0;
        let mut dn = Vec3d::ZERO;
        for (j, &b) in A[6].iter().enumerate() {
            dh += (kh[j] - kh[0]) * b;
            dn = dn + kn[j] * b;
        }
        let a = AuxState {
            h: (kh[0] + dh) * step,
            n: dn * step,
        };
        let b = AuxState {
            h: -dh * step,
            n: kn[0] * step - a.n,
        };
        let c = AuxState {
            h: (kh[0] - kh[6] + 2.0 * dh) * step,
            n: a.n - kn[6] * step - b.n,
        };
        let mut d = AuxState {
            h: 0.0,
            n: Vec3d::ZERO,
        };
        for i in 0..7 {
            d.h += (kh[i] - kh[0]) * D[i] * step;
            d.n = d.n + (kn[i] - kn[0]) * (D[i] * step);
        }
        AuxDense {
            start: AuxState { h, n },
            r: [a, b, c, d],
            step,
            normal_evolved: !height_only,
        }
    });
    Ok(DenseStep {
        start: s,
        end: high,
        error,
        step,
        r: [r2, r3, r4, r5],
        auxiliary: auxiliary_dense,
    })
}
fn single(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    method: Method,
    branch: Option<Branch>,
) -> Result<State, Error> {
    match method {
        Method::Midpoint => midpoint(profile, s, step),
        Method::Rk4 => rk4(profile, s, step, branch),
        Method::Dopri54 => Ok(dopri(profile, s, step, branch)?.0),
        Method::Dopri54Auxiliary => Ok(dopri_data(profile, s, step, branch, true)?.end),
    }
}
fn doubled(
    profile: &AtmosphereProfile,
    s: State,
    step: f64,
    method: Method,
    branch: Option<Branch>,
) -> Result<State, Error> {
    match method {
        Method::Midpoint => advance(profile, s, step),
        Method::Rk4 => rk4(
            profile,
            rk4(profile, s, step * 0.5, branch)?,
            step * 0.5,
            branch,
        ),
        Method::Dopri54 => Ok(dopri(profile, s, step, branch)?.0),
        Method::Dopri54Auxiliary => Ok(dopri_data(profile, s, step, branch, true)?.end),
    }
}
fn layer_step(
    profile: &AtmosphereProfile,
    height: f64,
    vertical: f64,
    curvature: f64,
    maximum_step: f64,
) -> f64 {
    let height = height - profile.base_height_m();
    let bound = |gap: f64, velocity: f64| {
        let root = (velocity * velocity + 2.0 * curvature * gap).sqrt();
        if velocity >= 0.0 {
            2.0 * gap / (root + velocity)
        } else {
            (root - velocity) / curvature
        }
    };
    let mut result = maximum_step;
    if let Some(upper) = profile
        .layers()
        .iter()
        .find(|k| k.height_m - height > KNOT_HEIGHT_BAND_M)
    {
        result = result.min(bound(upper.height_m - height, vertical));
    }
    if let Some(lower) = profile
        .layers()
        .iter()
        .rev()
        .find(|k| height - k.height_m > KNOT_HEIGHT_BAND_M)
    {
        result = result.min(bound(height - lower.height_m, -vertical));
    }
    result.max(0.01)
}
struct Replay {
    state: State,
    path: f64,
    attempts: u32,
    stats: Value,
}
fn replay(profile: &AtmosphereProfile, ray: Ray, options: Options) -> Result<Replay, Error> {
    let geometry_start = GEOMETRY_CALLS.load(Ordering::Relaxed);
    let dp_start = DP_CALLS.load(Ordering::Relaxed);
    let mut state = State {
        p: ray.origin,
        u: ray.direction,
        optical: 0.0,
    };
    let mut path = 0.0;
    let mut proposed = options.maximum_step;
    let mut carried = if options.carried_auxiliary {
        let (h, n) = geometry(state.p)?;
        Some(AuxState { h, n })
    } else {
        None
    };
    let mut carried_branch = carried.map(|g| carried_branch_at(profile, g.h, state.u.dot(g.n)));
    let mut carried_departing = false;
    // One-sided trial extensions can lie just beyond a knot. Cap their length
    // so n cannot drop by0.49 from its source value>=1, and use n>=0.5 in the
    // curvature bound. This protects the bracket proof without clamping n.
    let extension_limit = if options.one_sided && profile.maximum_index_gradient() > 0.0 {
        0.49 / profile.maximum_index_gradient()
    } else {
        f64::INFINITY
    };
    let curvature = 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M)
        + profile.maximum_index_gradient() * if options.one_sided { 2.0 } else { 1.0 };
    let mut counts = BTreeMap::<&str, u32>::new();
    let mut per_layer = BTreeMap::<usize, u32>::new();
    for attempts in 0..MAX_TRACE_STEPS {
        if options.target_path.is_some_and(|target| path >= target) {
            return Ok(Replay {
                state,
                path,
                attempts,
                stats: json!({"counts":counts,"attempts_by_lower_layer":per_layer,"geometry_calls":GEOMETRY_CALLS.load(Ordering::Relaxed)-geometry_start,"dp_calls":DP_CALLS.load(Ordering::Relaxed)-dp_start}),
            });
        }
        let (height, up) = match carried {
            Some(g) => (g.h, g.n),
            None => geometry(state.p)?,
        };
        let vertical = state.u.dot(up);
        let active_branch = if options.carried_auxiliary {
            carried_branch
        } else {
            options.one_sided.then(|| branch(profile, height, vertical))
        };
        if height >= profile.top_height_m() - 1e-6 && vertical > 0.0 {
            return Ok(Replay {
                state,
                path,
                attempts,
                stats: json!({"counts":counts,"attempts_by_lower_layer":per_layer,"geometry_calls":GEOMETRY_CALLS.load(Ordering::Relaxed)-geometry_start,"dp_calls":DP_CALLS.load(Ordering::Relaxed)-dp_start}),
            });
        }
        if path > MAX_ATMOSPHERIC_PATH_M {
            return Err("path budget".into());
        }
        let layer = profile
            .layers()
            .partition_point(|l| l.height_m <= height - profile.base_height_m())
            .saturating_sub(1);
        *per_layer.entry(layer).or_default() += 1;
        let clearance = height.abs();
        let toward = -height.signum() * vertical;
        let discriminant = (toward * toward + 2.0 * curvature * clearance).sqrt();
        let safe = if clearance == 0.0 {
            0.0
        } else if toward >= 0.0 {
            2.0 * clearance / (discriminant + toward)
        } else {
            (discriminant - toward) / curvature
        };
        let reference_limit = (safe * 0.8).max(0.01);
        let mut knot_limit = layer_step(profile, height, vertical, curvature, options.maximum_step);
        let mut monotone_crossing_bracket = false;
        // If the height derivative cannot turn before the next knot, a
        // conservative UPPER crossing bound brackets that knot directly. This
        // replaces repeated approaches, while every gradient discontinuity
        // still receives its own one-sided reintegrated contact.
        if matches!(options.method, Method::Dopri54 | Method::Dopri54Auxiliary)
            && let Some(b) = active_branch
        {
            let gap = if vertical > 0.0 {
                b.upper - height
            } else {
                height - b.lower
            };
            let speed = vertical.abs();
            let discriminant = speed * speed - 2.0 * curvature * (gap + 2.0 * options.tolerance);
            if gap > 1e-8 && gap.is_finite() && discriminant > 0.0 {
                let upper = 2.0 * (gap + 2.0 * options.tolerance) / (speed + discriminant.sqrt());
                knot_limit = upper.max(0.01).min(options.maximum_step);
                monotone_crossing_bracket = true;
            }
        }
        // At a just-resolved knot, excluding its zero gap from the ordinary
        // nearest-knot search must not let a ray turn and return across it
        // unseen. Taylor's bound gives first-return distance >=2|v|/B.
        if options.one_sided
            && !monotone_crossing_bracket
            && let Some(b) = active_branch
        {
            let departure_gap = if vertical >= 0.0 {
                height - b.lower
            } else {
                b.upper - height
            };
            if departure_gap.abs() <= 1e-7 || carried_departing {
                let departure_limit = 1.6 * vertical.abs() / curvature;
                if departure_limit < 0.01 {
                    return Err("unresolved tangent knot departure".into());
                }
                knot_limit = knot_limit.min(departure_limit);
            }
        }
        let mut step = proposed
            .min(options.maximum_step)
            .min(extension_limit)
            .min(reference_limit)
            .min(knot_limit);
        if let Some(target) = options.target_path {
            step = step.min(target - path);
        }
        if (0.0..0.001).contains(&height) && vertical < -1e-8 {
            step = step.min((2.0 * height.max(1e-8) / -vertical).clamp(0.01, 1.0));
        }
        let limiter = if step == knot_limit && step < proposed {
            "knot"
        } else if step == reference_limit && step < proposed {
            "reference"
        } else if step == options.maximum_step {
            "maximum"
        } else {
            "proposal"
        };
        let dense = if options.dense_events {
            let full = dopri_data_from(
                profile,
                state,
                step,
                active_branch,
                matches!(options.method, Method::Dopri54Auxiliary),
                carried,
                options.height_only,
                options.seeded_normals,
            )?;
            Some(if options.refined_dense {
                DenseCurve::refined(profile, full, active_branch, false)?
            } else if options.hermite_dense {
                DenseCurve::hermite(full)?
            } else {
                DenseCurve::single(full)
            })
        } else {
            None
        };
        let (mut half, error) = if let Some(value) = dense {
            (value.end(), value.error())
        } else if matches!(options.method, Method::Dopri54) {
            dopri(profile, state, step, active_branch)?
        } else {
            let full = single(profile, state, step, options.method, active_branch)?;
            let half = doubled(profile, state, step, options.method, active_branch)?;
            let error = full
                .p
                .distance(half.p)
                .max((full.u - half.u).length() * step)
                .max((full.optical - half.optical).abs())
                / match options.method {
                    Method::Midpoint => 3.0,
                    Method::Rk4 => 1.0,
                    Method::Dopri54 | Method::Dopri54Auxiliary => unreachable!(),
                };
            (half, error)
        };
        // No division by15 is used for RK4 in this experiment: the raw
        // full-versus-half discrepancy is deliberately the acceptance norm.
        if error > options.tolerance && step > 0.01000001 {
            proposed = step * 0.5;
            *counts.entry("rejected_error").or_default() += 1;
            continue;
        }
        if !error.is_finite() || error > options.tolerance {
            return Err("numerical budget".into());
        }
        if options.carried_auxiliary {
            let value = dense.ok_or("missing carried dense")?;
            let h1 = value.auxiliary_at(step)?.h;
            let derivative = value.auxiliary_height_derivative(0.0)?;
            let bound = value.auxiliary_height_curvature()?;
            let active = active_branch.ok_or("missing carried branch")?;
            let monotone = derivative.abs() > bound * step;
            let mut certified = true;
            for target in [active.lower, active.upper, 0.0] {
                if !target.is_finite() {
                    continue;
                }
                let f0 = height - target;
                let f1 = h1 - target;
                let crossing = f0 * f1 <= 0.0;
                let margin = f0.abs().min(f1.abs());
                // A scalar C² curve lies within B*s*(L-s)/2 of its endpoint
                // chord. Monotonicity instead certifies the first crossing.
                if !monotone && (crossing || margin <= bound * step * step / 8.0 + 1e-9) {
                    certified = false;
                    break;
                }
            }
            if !certified {
                if step <= 1e-6 {
                    return Err("carried polynomial height event ambiguity".into());
                }
                proposed = step * 0.5;
                *counts
                    .entry("rejected_polynomial_height_guard")
                    .or_default() += 1;
                continue;
            }
        }
        let mut top_contact = false;
        let mut source_contact = None;
        if let Some(b) = active_branch {
            let end_height = if options.carried_auxiliary {
                dense.ok_or("missing carried dense")?.auxiliary_at(step)?.h
            } else {
                geometry(half.p)?.0
            };
            let event = source_event(b, height, end_height, options.carried_auxiliary);
            if let Some(target) = event {
                let ascending = target > height;
                let (mut low, mut high) = match dense {
                    Some(value) => value.root_bracket(target, ascending)?,
                    None => (0.0, step),
                };
                let mut trial = (step * (target - height) / (end_height - height)).clamp(low, high);
                let mut resolved = false;
                for _ in 0..55 {
                    half = if let Some(value) = dense {
                        value.at(trial)?
                    } else {
                        doubled(profile, state, trial, options.method, active_branch)?
                    };
                    let (h, normal) = if options.carried_auxiliary {
                        let g = dense.ok_or("missing carried dense")?.auxiliary_at(trial)?;
                        (g.h, g.n)
                    } else {
                        geometry(half.p)?
                    };
                    let derivative = if options.carried_auxiliary {
                        dense
                            .ok_or("missing carried dense")?
                            .auxiliary_height_derivative(trial)?
                    } else {
                        dense
                            .map_or(half.u, |value| value.position_derivative(trial))
                            .dot(normal)
                    };
                    let residual = h - target;
                    // |h''|<=curvature bounds the slope throughout the entire
                    // remaining bracket. A transverse residual is converted to
                    // a PATH bound only when that minimum slope is positive.
                    let root_curvature = if options.carried_auxiliary {
                        dense
                            .ok_or("missing carried dense")?
                            .auxiliary_height_curvature()?
                    } else {
                        match dense {
                            Some(value) => value.height_second_derivative_bound()?,
                            None => curvature,
                        }
                    };
                    let minimum_slope = derivative.abs() - root_curvature * (high - low);
                    if high - low <= 1e-6
                        || (minimum_slope > 0.0 && (residual.abs() + 1e-9) / minimum_slope <= 1e-6)
                    {
                        resolved = true;
                        break;
                    }
                    if (h < target) == ascending {
                        low = trial;
                    } else {
                        high = trial;
                    }
                    let newton = trial - residual / derivative;
                    trial = if newton.is_finite() && newton > low && newton < high {
                        newton
                    } else {
                        (low + high) * 0.5
                    };
                }
                if !resolved {
                    return Err("one-sided event refinement budget".into());
                }
                step = trial;
                top_contact = target == profile.top_height_m();
                source_contact = Some((target, ascending));
                *counts.entry("one_sided_knot_event").or_default() += 1;
            }
        }
        *counts.entry(limiter).or_default() += 1;
        if step < 1.0 {
            *counts.entry("accepted_below_one_metre").or_default() += 1;
        }
        if limiter == "proposal" && error < options.tolerance * 0.125 {
            *counts.entry("proposal_doubling").or_default() += 1;
        }
        if options.carried_auxiliary {
            let mut next = dense.ok_or("missing carried dense")?.auxiliary_at(step)?;
            let allowance = if source_contact.is_some() { 1e-6 } else { 0.0 };
            let (actual_h, actual_n) = geometry(half.p)?;
            let actual = AuxState {
                h: actual_h,
                n: actual_n,
            };
            if error + next.discrepancy(actual, !options.height_only)? + allowance
                > options.tolerance
            {
                proposed = step * 0.5;
                *counts.entry("rejected_carried_consistency").or_default() += 1;
                continue;
            }
            if options.height_only {
                // Normal is algebraic in this variant. Ordinary same-interval
                // height projection charges its discrepancy above and changes
                // neither physical position/direction nor contact ownership.
                next.n = actual_n;
                let b = active_branch.ok_or("missing height-only branch")?;
                if source_contact.is_none()
                    && next.h > b.lower
                    && next.h < b.upper
                    && actual_h > b.lower
                    && actual_h < b.upper
                {
                    next.h = actual_h;
                }
            }
            carried = Some(next);
            carried_departing = source_contact.is_some();
            if let Some((target, ascending)) = source_contact {
                carried_branch = Some(carried_branch_at(
                    profile,
                    target,
                    if ascending { 1.0 } else { -1.0 },
                ));
            }
        }
        if top_contact {
            return Ok(Replay {
                state: half,
                path: path + step,
                attempts: attempts + 1,
                stats: json!({"counts":counts,"attempts_by_lower_layer":per_layer,"geometry_calls":GEOMETRY_CALLS.load(Ordering::Relaxed)-geometry_start,"dp_calls":DP_CALLS.load(Ordering::Relaxed)-dp_start}),
            });
        }
        let next_height = match carried {
            Some(g) => g.h,
            None => geometry(half.p)?.0,
        };
        if next_height >= profile.top_height_m() && height < profile.top_height_m() {
            let mut low = 0.0;
            let mut high = step;
            for _ in 0..40 {
                if high - low < 1e-6 {
                    break;
                }
                let mid = (low + high) * 0.5;
                if geometry(doubled(profile, state, mid, options.method, active_branch)?.p)?.0
                    < profile.top_height_m()
                {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            let distance = (low + high) * 0.5;
            return Ok(Replay {
                state: doubled(profile, state, distance, options.method, active_branch)?,
                path: path + distance,
                attempts: attempts + 1,
                stats: json!({"counts":counts,"attempts_by_lower_layer":per_layer,"geometry_calls":GEOMETRY_CALLS.load(Ordering::Relaxed)-geometry_start,"dp_calls":DP_CALLS.load(Ordering::Relaxed)-dp_start}),
            });
        }
        state = half;
        path += step;
        proposed = if error < options.tolerance * 0.125 {
            if options.preserve_proposal {
                proposed.max(step * 2.0)
            } else {
                step * 2.0
            }
        } else {
            step
        };
    }
    Err("attempt budget".into())
}
pub fn analyze(
    profile: &AtmosphereProfile,
    ray: Ray,
    reference: &OpticalTrace,
) -> Result<Value, Error> {
    if reference.surface.is_some()
        || reference.marker.is_some()
        || reference.initial_vacuum_distance_m != 0.0
    {
        return Err("diagnostic requires resolved clear ray starting inside atmosphere".into());
    }
    let baseline = replay(profile, ray, Options::baseline(false))?;
    let endpoint_error = baseline.state.p.distance(reference.exit_ray.origin);
    let path_error = (baseline.path - reference.geometric_path_m).abs();
    let direction_error = (baseline.state.u - reference.exit_ray.direction).length();
    if endpoint_error > 1e-6
        || path_error > 1e-6
        || direction_error > 1e-12
        || baseline.attempts != reference.steps
    {
        return Err(format!("production replay mismatch: endpoint={endpoint_error}, path={path_error}, direction={direction_error}, attempts={}/{}",baseline.attempts,reference.steps).into());
    }
    let candidate = replay(profile, ray, Options::baseline(true))?;
    let experiments = |make_options: fn(f64, f64) -> Options| -> [Value; 3] {
        [2000.0,20_000.0,100_000.0].map(|maximum_step|{
        let options=make_options(maximum_step,POSITION_TOLERANCE_M);
        match replay(profile,ray,options) {
            Ok(result)=>json!({"maximum_step_m":maximum_step,"attempts":result.attempts,"statistics":result.stats,
                "endpoint_ecef_m":[result.state.p.x,result.state.p.y,result.state.p.z],
                "exit_direction":[result.state.u.x,result.state.u.y,result.state.u.z],
                "path_m":result.path,"optical_path_m":result.state.optical,
                "baseline_endpoint_difference_m":result.state.p.distance(baseline.state.p),
                "baseline_direction_difference":result.state.u.distance(baseline.state.u)}),
            Err(error)=>json!({"maximum_step_m":maximum_step,"error":error.to_string()})
        }
    })
    };
    let rk4_experiments = experiments(Options::rk4);
    let dopri_experiments = experiments(Options::dopri);
    let dopri_dense_experiments = experiments(Options::dopri_dense);
    let dopri_auxiliary_experiments = experiments(Options::dopri_auxiliary);
    let dopri_refined_experiments = experiments(Options::dopri_refined);
    let dopri_hermite_experiments = experiments(Options::dopri_hermite);
    let dopri_carried_experiments = experiments(Options::dopri_carried);
    let dopri_height_experiments = experiments(Options::dopri_height);
    let dopri_seeded_experiments = experiments(Options::dopri_seeded);
    Ok(
        json!({"baseline":{"attempts":baseline.attempts,"statistics":baseline.stats,"production_endpoint_difference_m":endpoint_error,"production_path_difference_m":path_error,"production_direction_difference":direction_error},
        "proposal_preservation_experiment":{"attempts":candidate.attempts,"statistics":candidate.stats,"baseline_endpoint_difference_m":candidate.state.p.distance(baseline.state.p),"baseline_path_difference_m":(candidate.path-baseline.path).abs(),"baseline_direction_difference":(candidate.state.u-baseline.state.u).length(),"baseline_optical_integral_difference_m":(candidate.state.optical-baseline.state.optical).abs()},
        "rk4_experiments":rk4_experiments,
        "dopri_experiments":dopri_experiments,
        "dopri_dense_experiments":dopri_dense_experiments,
        "dopri_auxiliary_experiments":dopri_auxiliary_experiments,
        "dopri_refined_experiments":dopri_refined_experiments,
        "dopri_hermite_experiments":dopri_hermite_experiments,
        "dopri_carried_experiments":dopri_carried_experiments,
        "dopri_height_experiments":dopri_height_experiments,
        "dopri_seeded_experiments":dopri_seeded_experiments,
        "scope":"Read-only f64 clear-ray controller experiment. Baseline must reproduce the physical-scene solver. Experimental changed step partitions are not a validated runtime replacement or accuracy proof; no source, shader or tolerance is changed."}),
    )
}

/// Independent-oracle driver: no production solver is used as the expected
/// value. The fixture supplies its independently integrated final state/path.
pub fn analyze_reference(case: &Value) -> Result<Value, Error> {
    let vector = |name: &str| -> Result<Vec3d, Error> {
        let array = case[name].as_array().ok_or("reference vector")?;
        if array.len() != 3 {
            return Err("reference vector length".into());
        }
        Ok(Vec3d::new(
            array[0].as_f64().ok_or("x")?,
            array[1].as_f64().ok_or("y")?,
            array[2].as_f64().ok_or("z")?,
        ))
    };
    let layers = case["layers"]
        .as_array()
        .ok_or("reference layers")?
        .iter()
        .map(|v| {
            Ok(earth_math::atmosphere::AtmosphereLayer {
                height_m: v["height_m"].as_f64().ok_or("height")?,
                refractivity: v["refractivity"].as_f64().ok_or("refractivity")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let profile = AtmosphereProfile::from_layers(layers)?;
    let ray = Ray {
        origin: vector("origin_ecef_m")?,
        direction: vector("initial_direction")?,
    };
    let expected_position = vector("endpoint_ecef_m")?;
    let expected_direction = vector("exit_direction")?;
    let expected_path = case["geometric_path_m"]
        .as_f64()
        .ok_or("reference geometric path")?;
    let expected_optical = case["optical_path_m"]
        .as_f64()
        .ok_or("reference optical path")?;
    let mut configurations = vec![
        ("midpoint_baseline", Options::baseline(false)),
        ("midpoint_preserved", Options::baseline(true)),
    ];
    for maximum in [2000.0, 20_000.0, 100_000.0] {
        configurations.push(("rk4_raw_1mm", Options::rk4(maximum, 0.001)));
        configurations.push(("rk4_raw_10um", Options::rk4(maximum, 0.00001)));
        configurations.push(("dopri_raw_1mm", Options::dopri(maximum, 0.001)));
        configurations.push(("dopri_raw_10um", Options::dopri(maximum, 0.00001)));
        configurations.push(("dopri_dense_raw_1mm", Options::dopri_dense(maximum, 0.001)));
        configurations.push((
            "dopri_refined_raw_1mm",
            Options::dopri_refined(maximum, 0.001),
        ));
        configurations.push((
            "dopri_hermite_raw_1mm",
            Options::dopri_hermite(maximum, 0.001),
        ));
        configurations.push((
            "dopri_carried_raw_1mm",
            Options::dopri_carried(maximum, 0.001),
        ));
        configurations.push((
            "dopri_height_raw_1mm",
            Options::dopri_height(maximum, 0.001),
        ));
        configurations.push((
            "dopri_seeded_raw_1mm",
            Options::dopri_seeded(maximum, 0.001),
        ));
        configurations.push((
            "dopri_auxiliary_raw_1mm",
            Options::dopri_auxiliary(maximum, 0.001),
        ));
        configurations.push((
            "dopri_auxiliary_raw_10um",
            Options::dopri_auxiliary(maximum, 0.00001),
        ));
        configurations.push((
            "dopri_dense_raw_10um",
            Options::dopri_dense(maximum, 0.00001),
        ));
    }
    let results:Vec<_>=configurations.into_iter().map(|(name,mut options)|{
        options.target_path=case.get("target_path_m").and_then(Value::as_f64);
        match replay(&profile,ray,options) {
            Ok(result)=>json!({"method":name,"maximum_step_m":options.maximum_step,"attempts":result.attempts,"statistics":result.stats,
                "endpoint_error_m":result.state.p.distance(expected_position),"direction_error":result.state.u.distance(expected_direction),
                "path_error_m":(result.path-expected_path).abs(),"optical_path_error_m":(result.state.optical-expected_optical).abs()}),
            Err(error)=>json!({"method":name,"maximum_step_m":options.maximum_step,"error":error.to_string()})
        }
    }).collect();
    Ok(
        json!({"name":case["name"],"results":results,"scope":"read-only prototype versus independent Bouguer reference; no runtime accuracy certification"}),
    )
}

/// Interior continuous-output and earliest plane-contact diagnostic. Expected
/// points/tangents come from the independent reference, not this interpolant.
pub fn analyze_dense_reference(case: &Value) -> Result<Value, Error> {
    let vector = |name: &str| -> Result<Vec3d, Error> {
        let array = case[name].as_array().ok_or("interior reference vector")?;
        if array.len() != 3 {
            return Err("interior vector length".into());
        }
        Ok(Vec3d::new(
            array[0].as_f64().ok_or("x")?,
            array[1].as_f64().ok_or("y")?,
            array[2].as_f64().ok_or("z")?,
        ))
    };
    let layers = case["layers"]
        .as_array()
        .ok_or("interior layers")?
        .iter()
        .map(|v| {
            Ok(earth_math::atmosphere::AtmosphereLayer {
                height_m: v["height_m"].as_f64().ok_or("height")?,
                refractivity: v["refractivity"].as_f64().ok_or("index")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let profile = AtmosphereProfile::from_layers(layers)?;
    let start = State {
        p: vector("origin_ecef_m")?,
        u: vector("initial_direction")?
            .normalized()
            .ok_or("initial direction")?,
        optical: 0.0,
    };
    let trial = case["trial_path_m"].as_f64().ok_or("trial path")?;
    let target = case["target_path_m"].as_f64().ok_or("target path")?;
    if !(trial > 0.0 && target > 0.0 && target < trial) {
        return Err("interior path domain".into());
    }
    let point = vector("endpoint_ecef_m")?;
    let tangent = vector("exit_direction")?
        .normalized()
        .ok_or("interior tangent")?;
    let expected_optical = case["optical_path_m"].as_f64().ok_or("interior optical")?;
    let (height, up) = geometry(start.p)?;
    let active = Some(branch(&profile, height, start.u.dot(up)));
    let full = dopri_data(&profile, start, trial, active, false)?;
    let variants = [
        ("hermite", DenseCurve::hermite(full)?),
        (
            "refined",
            DenseCurve::refined(&profile, full, active, false)?,
        ),
    ];
    let results = variants.into_iter().map(|(name,curve)| -> Result<Value,Error> {
        let sample = curve.at(target)?;
        let (mut low,mut high,minimum_slope) = curve.plane_bracket(point,tangent)?;
        let before=(curve.at(low)?.p-point).dot(tangent);
        let after=(curve.at(high)?.p-point).dot(tangent);
        // Chord-plane seed uses only the numerical bracket, never the oracle's
        // expected contact path (which is reserved for the final comparison).
        let mut distance = (low+(high-low)*before/(before-after)).clamp(low,high);
        let mut resolved = false;
        for _ in 0..64 {
            let at = curve.at(distance)?;
            let residual = (at.p-point).dot(tangent);
            if high-low<=1e-7 || (residual.abs()+2e-9)/minimum_slope<=1e-7 {
                resolved=true;break;
            }
            if residual<0.0 { low=distance; } else { high=distance; }
            let derivative=curve.position_derivative(distance).dot(tangent);
            let newton=distance-residual/derivative;
            let next=if newton>low && newton<high {newton} else {(low+high)*0.5};
            if next==distance { break; }
            distance=next;
        }
        if !resolved { return Err("interior plane root unresolved".into()); }
        let contact=curve.at(distance)?;
        Ok(json!({"method":name,"controller_estimate_m":curve.error(),"eligible_at_1mm":curve.error()<=0.001,
            "interior_endpoint_error_m":sample.p.distance(point),"interior_direction_error":sample.u.distance(tangent),
            "interior_optical_error_m":(sample.optical-expected_optical).abs(),
            "plane_contact_error_m":contact.p.distance(point),"plane_path_error_m":(distance-target).abs(),
            "plane_transverse_lower_bound":minimum_slope}))
    }).collect::<Result<Vec<_>,_>>()?;
    Ok(json!({"name":case["name"],"trial_path_m":trial,"target_path_m":target,"results":results}))
}

#[cfg(test)]
mod carried_branch_regression {
    use super::*;

    fn discontinuous_profile() -> AtmosphereProfile {
        AtmosphereProfile::from_layers(vec![
            atmosphere::AtmosphereLayer {
                height_m: 0.0,
                refractivity: 0.0003,
            },
            atmosphere::AtmosphereLayer {
                height_m: 1000.0,
                refractivity: 0.00029,
            },
            atmosphere::AtmosphereLayer {
                height_m: 2000.0,
                refractivity: 0.0001,
            },
            atmosphere::AtmosphereLayer {
                height_m: 3000.0,
                refractivity: 0.0,
            },
        ])
        .unwrap()
    }

    #[test]
    fn unowned_near_knot_contacts_are_not_excluded() {
        let profile = discontinuous_profile();
        let knot = 1000.0_f64;
        for height in [knot.next_down(), knot - 5e-9] {
            let active = carried_branch_at(&profile, height, 1.0);
            assert_eq!(active.upper, knot);
            assert_eq!(source_event(active, height, knot + 1.0, true), Some(knot));
        }
        for height in [knot.next_up(), knot + 5e-9] {
            let active = carried_branch_at(&profile, height, -1.0);
            assert_eq!(active.lower, knot);
            assert_eq!(source_event(active, height, knot - 1.0, true), Some(knot));
        }
        let ascending = carried_branch_at(&profile, knot, 1.0);
        let descending = carried_branch_at(&profile, knot, -1.0);
        assert_eq!(ascending.lower, knot);
        assert_eq!(descending.upper, knot);
        assert_ne!(ascending.slope, descending.slope);
        // A resolved departure owns the adjacent branch without an immediate
        // duplicate zero-distance event. A subsequent return must be detected.
        assert_eq!(source_event(ascending, knot, knot + 1.0, true), None);
        assert_eq!(source_event(descending, knot, knot - 1.0, true), None);
        assert_eq!(
            source_event(ascending, knot + 5e-9, knot - 1.0, true),
            Some(knot)
        );
        assert_eq!(
            source_event(descending, knot - 5e-9, knot + 1.0, true),
            Some(knot)
        );
    }

    #[test]
    fn represented_absolute_contact_preserves_nonzero_anchor_ownership() {
        let knots = [
            atmosphere::AtmosphereLayer {
                height_m: 0.0,
                refractivity: 0.0003,
            },
            atmosphere::AtmosphereLayer {
                height_m: 0.2,
                refractivity: 0.00029,
            },
            atmosphere::AtmosphereLayer {
                height_m: 1.0,
                refractivity: 0.0,
            },
        ];
        let base = 0.1;
        let contact = base + knots[1].height_m;
        assert_ne!(contact - base, knots[1].height_m);
        assert_eq!(carried_branch_index(&knots, base, contact, 1.0), 2);
        assert_eq!(carried_branch_index(&knots, base, contact, -1.0), 1);
        assert_eq!(
            carried_branch_index(&knots, base, contact.next_down(), 1.0),
            1
        );
        assert_eq!(
            carried_branch_index(&knots, base, contact.next_up(), -1.0),
            2
        );
    }

    #[test]
    fn near_knot_radial_replay_crosses_real_slope_discontinuity() {
        let profile = discontinuous_profile();
        for sign in [-1.0, 1.0] {
            let origin = geodetic_to_ecef(Geodetic::new(0.0, 0.0, 1000.0 - sign * 5e-9).unwrap());
            let direction = Vec3d::new(sign, 0.0, 0.0);
            let actual_height = geometry(origin).unwrap().0;
            assert!((1000.0 - actual_height) * sign > 0.0);
            for mut options in [
                Options::dopri_carried(100_000.0, 0.001),
                Options::dopri_height(100_000.0, 0.001),
                Options::dopri_seeded(100_000.0, 0.001),
            ] {
                options.target_path = Some(2.0);
                let result = replay(&profile, Ray { origin, direction }, options).unwrap();
                assert!(result.state.p.distance(origin + direction * 2.0) < 1e-7);
                assert!(result.state.u.distance(direction) < 1e-14);
                assert!((result.path - 2.0).abs() < 1e-12);
                assert_eq!(
                    result.stats["counts"]["one_sided_knot_event"].as_u64(),
                    Some(1)
                );
                // On this equatorial normal ray direction is constant. The index
                // integral is exactly two trapezoids split at the source knot.
                let split = (1000.0 - actual_height) / sign;
                let (n0, _) = profile.index_gradient(actual_height);
                let (nk, _) = profile.index_gradient(1000.0);
                let (n1, _) = profile.index_gradient(actual_height + sign * 2.0);
                let expected = 0.5 * (n0 + nk) * split + 0.5 * (nk + n1) * (2.0 - split);
                assert!((result.state.optical - expected).abs() < 1e-12);
            }
        }
    }
}
