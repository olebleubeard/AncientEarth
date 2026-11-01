//! Conditional reference-turbulence strength and a positive temporal display.
//!
//! Original implementation of the Hufnagel–Valley 5/7 profile and weak plane-wave
//! integral described by S. Pirandola, Phys. Rev. Research 3, 023130 (2021),
//! Appendix C, equations C2 and C14–C15, DOI:10.1103/PhysRevResearch.3.023130.
//! This assumes a fixed reference column, 550 nm, a point receiver and a planar
//! upward path. It does not infer optical turbulence from GFS mean weather.
//! Height is transplanted onto WGS84 ellipsoidal height; negative heights use
//! the reference ground value. Elevations below 35 degrees use the 35-degree
//! value, explicitly a display continuation outside the chosen slant domain.
//! The temporal spectrum is illustrative; its independent-phase ensemble
//! variance matches the supplied weak-theory variance before exposure averaging.

use crate::{MAX_HEIGHT_M, MIN_HEIGHT_M};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub const REFERENCE_WAVELENGTH_M: f64 = 550e-9;
pub const MIN_REFERENCE_ELEVATION_DEG: f64 = 35.0;
pub const TEMPORAL_FREQUENCIES_CENTIHZ: [i32; 4] = [73, 117, 191, 271];
const GAMMA_ELEVEN_SIXTHS: f64 = 0.940_655_858_256_771_6;

/// Reference Cn² in m^(-2/3). This numerical function follows the fixed HV5/7
/// column, not a measured local profile. The finite input domain matches Earth.
pub fn reference_cn2(height_m: f64) -> Option<f64> {
    if !height_m.is_finite() || !(MIN_HEIGHT_M..=MAX_HEIGHT_M).contains(&height_m) {
        return None;
    }
    let h = height_m.max(0.0);
    Some(
        5.94e-53 * (21.0_f64 / 27.0).powi(2) * h.powi(10) * (-h / 1000.0).exp()
            + 2.7e-16 * (-h / 1500.0).exp()
            + 1.7e-14 * (-h / 100.0).exp(),
    )
}

/// Zenith weak plane-wave intensity variance for the assumed column above the
/// receiver. With x=z-h, integrate Cn²(h+x) x^(5/6) from zero to infinity.
/// Expanding (h+x)^10 reduces the wind term to eleven positive gamma moments;
/// this avoids a numerical quadrature or an artificial atmospheric cutoff.
pub fn reference_zenith_variance(height_m: f64) -> Option<f64> {
    if !height_m.is_finite() || !(MIN_HEIGHT_M..=MAX_HEIGHT_M).contains(&height_m) {
        return None;
    }
    let h = height_m.max(0.0);
    let r = h / 1000.0;
    let mut polynomial = 0.0;
    let mut binomial = 1.0;
    let mut gamma_ratio = 1.0;
    for j in 0..=10 {
        polynomial += binomial * gamma_ratio * r.powi(10 - j);
        if j < 10 {
            binomial *= f64::from(10 - j) / f64::from(j + 1);
            gamma_ratio *= 11.0 / 6.0 + f64::from(j);
        }
    }
    let wind = 5.94e-53
        * (21.0_f64 / 27.0).powi(2)
        * 1000.0_f64.powf(71.0 / 6.0)
        * (-r).exp()
        * polynomial;
    let background = 2.7e-16 * 1500.0_f64.powf(11.0 / 6.0) * (-h / 1500.0).exp();
    let ground = 1.7e-14 * 100.0_f64.powf(11.0 / 6.0) * (-h / 100.0).exp();
    let integral = GAMMA_ELEVEN_SIXTHS * (wind + background + ground);
    Some(2.25 * (TAU / REFERENCE_WAVELENGTH_M).powf(7.0 / 6.0) * integral)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScintillationStrength {
    pub variance: f64,
    pub factor_amplitude: f64,
    pub low_elevation_limited: bool,
    /// The weak-theory criterion is necessary, not a measured validity claim.
    pub weak_regime: bool,
}

/// Apply the planar slant factor sin(elevation)^(-11/6). Below the documented
/// 35-degree domain it is held fixed. Invalid/unrepresentable positive-process
/// inputs fail closed, rather than silently compressing the requested variance.
pub fn strength_at_elevation(
    zenith_variance: f64,
    elevation_sine: f64,
) -> Option<ScintillationStrength> {
    if !zenith_variance.is_finite()
        || zenith_variance < 0.0
        || !elevation_sine.is_finite()
        || !(-1.0..=1.0).contains(&elevation_sine)
    {
        return None;
    }
    let minimum = MIN_REFERENCE_ELEVATION_DEG.to_radians().sin();
    let variance = zenith_variance / elevation_sine.max(minimum).powf(11.0 / 6.0);
    // Four independent unit-mean factors (1+a sinθ) have variance
    // (1+a²/2)^4−1. log1p/expm1 retain small high-altitude strengths.
    let amplitude = (2.0 * (variance.ln_1p() * 0.25).exp_m1()).sqrt();
    if !variance.is_finite() || !amplitude.is_finite() || amplitude > 1.0 {
        return None;
    }
    Some(ScintillationStrength {
        variance,
        factor_amplitude: amplitude,
        low_elevation_limited: elevation_sine < minimum,
        weak_regime: variance < 1.0,
    })
}

/// Unaveraged mode phases at the midpoint of the preceding exposure. Reduction
/// before multiplication preserves finite phase evaluation for long runtimes.
/// Add each source's stable independent phase before calling four_factor_average.
pub fn frame_midpoint_phases(seconds: f64, exposure_seconds: f64) -> Option<[f64; 4]> {
    if !seconds.is_finite() || !exposure_seconds.is_finite() || exposure_seconds < 0.0 {
        return None;
    }
    Some(TEMPORAL_FREQUENCIES_CENTIHZ.map(|centihz| {
        let frequency = f64::from(centihz) / 100.0;
        let period = 1.0 / frequency;
        ((seconds.rem_euclid(period) - (exposure_seconds * 0.5).rem_euclid(period))
            * TAU
            * frequency)
            .rem_euclid(TAU)
    }))
}

/// Exact real-arithmetic rectangular-exposure average of ∏(1+a sinθ_i).
/// The 3^4 Fourier terms retain cross frequencies; multiplying four separately
/// averaged factors would be incorrect. Frequencies are exact integer centihertz
/// so their signed combinations are formed without cancellation ambiguity.
/// The final bound clamp protects only against floating-point roundoff.
pub fn four_factor_average(
    amplitude: f64,
    midpoint_phases: [f64; 4],
    exposure_seconds: f64,
) -> Option<f64> {
    if !amplitude.is_finite()
        || !(0.0..=1.0).contains(&amplitude)
        || !midpoint_phases.into_iter().all(f64::is_finite)
        || !exposure_seconds.is_finite()
        || exposure_seconds < 0.0
    {
        return None;
    }
    let phases = midpoint_phases.map(|phase| phase.rem_euclid(TAU));
    let mut sum = 0.0;
    for code in 0..81 {
        let mut digits = code;
        let mut coefficient = 1.0;
        let mut phase = 0.0;
        let mut frequency_centihz = 0;
        for mode in 0..4 {
            let sign = digits % 3 - 1;
            digits /= 3;
            if sign != 0 {
                coefficient *= amplitude * 0.5;
                phase += f64::from(sign) * (phases[mode] - FRAC_PI_2);
                frequency_centihz += sign * TEMPORAL_FREQUENCIES_CENTIHZ[mode];
            }
        }
        let half_phase = PI * (f64::from(frequency_centihz) / 100.0) * exposure_seconds;
        let average = if frequency_centihz == 0 {
            1.0
        } else if !half_phase.is_finite() {
            0.0
        } else if half_phase.abs() < 1e-5 {
            1.0 - half_phase * half_phase / 6.0 + half_phase.powi(4) / 120.0
        } else {
            half_phase.sin() / half_phase
        };
        sum += coefficient * phase.cos() * average;
    }
    Some(sum.clamp((1.0 - amplitude).powi(4), (1.0 + amplitude).powi(4)))
}
