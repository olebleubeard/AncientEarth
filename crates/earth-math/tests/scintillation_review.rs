//! Reference-column quadrature and temporal integration use different algorithms
//! from the production positive gamma-moment sum and Fourier expansion.
use earth_math::rendering::stars::scintillation::*;
#[path = "fixtures/scintillation_integral.rs"]
mod reference;

fn relative(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance * b.abs(), "{a:e} != {b:e}");
}

#[test]
fn gamma_moments_match_direct_high_precision_column_quadrature() {
    for &(height, expected) in reference::ZENITH {
        relative(reference_zenith_variance(height).unwrap(), expected, 2e-14);
    }
    assert_eq!(
        reference_zenith_variance(-123.0),
        reference_zenith_variance(0.0)
    );
    assert_eq!(reference_zenith_variance(1e9), Some(0.0));
    // The reference column is numerically exhausted far above the atmosphere;
    // the expanded coordinate domain does not create additional turbulence.
    for height in [
        1e9 + 1.0,
        earth_math::ASTRONOMICAL_UNIT_M,
        earth_math::MAX_HEIGHT_M,
    ] {
        assert_eq!(reference_zenith_variance(height), Some(0.0));
        assert_eq!(reference_cn2(height), Some(0.0));
    }
    for invalid in [
        f64::NAN,
        f64::INFINITY,
        -10_001.0,
        earth_math::MAX_HEIGHT_M.next_up(),
    ] {
        assert!(reference_zenith_variance(invalid).is_none());
        assert!(reference_cn2(invalid).is_none());
    }
}

#[test]
fn slant_strength_has_explicit_domain_and_positive_ensemble_moments() {
    let zenith = reference_zenith_variance(1.72).unwrap();
    let up = strength_at_elevation(zenith, 1.0).unwrap();
    let low = strength_at_elevation(zenith, 35.0_f64.to_radians().sin()).unwrap();
    relative(
        low.variance / up.variance,
        35.0_f64.to_radians().sin().powf(-11.0 / 6.0),
        1e-14,
    );
    assert!(low.variance > up.variance && low.weak_regime && !low.low_elevation_limited);
    let horizon = strength_at_elevation(zenith, 0.0).unwrap();
    assert!(horizon.low_elevation_limited);
    assert_eq!(horizon.variance, low.variance);
    // A tensor product of independent uniformly spaced phases integrates
    // every one-factor first/second moment exactly to rounding accuracy.
    for strength in [up, low, strength_at_elevation(1e-20, 1.0).unwrap()] {
        let a = strength.factor_amplitude;
        assert!((0.0..1.0).contains(&a));
        let values: Vec<_> = (0..32)
            .map(|i| 1.0 + a * (std::f64::consts::TAU * f64::from(i) / 32.0).sin())
            .collect();
        let mean = values.iter().sum::<f64>() / 32.0;
        let second = values.iter().map(|x| x * x).sum::<f64>() / 32.0;
        assert!((mean.powi(4) - 1.0).abs() < 3e-15);
        if strength.variance > 1e-10 {
            relative(second.powi(4) - mean.powi(8), strength.variance, 2e-14);
        }
    }
    for pair in [
        (f64::NAN, 1.0),
        (-1.0, 1.0),
        (1.0, f64::NAN),
        (1.0, 1.001),
        (100.0, 1.0),
    ] {
        assert!(strength_at_elevation(pair.0, pair.1).is_none());
    }
}

#[test]
fn full_fourier_exposure_average_matches_independent_time_quadrature() {
    let phases = [0.13, 2.3, 5.7, 1.11];
    for amplitude in [0.0, 0.3, 0.6, 0.99] {
        for exposure in [0.0, 0.017, 0.4, 2.3] {
            let value = four_factor_average(amplitude, phases, exposure).unwrap();
            let count = 16_384;
            let integrand = |t: f64| {
                (0..4)
                    .map(|i| {
                        1.0 + amplitude
                            * (phases[i]
                                + std::f64::consts::TAU
                                    * f64::from(TEMPORAL_FREQUENCIES_CENTIHZ[i])
                                    / 100.0
                                    * t)
                                .sin()
                    })
                    .product::<f64>()
            };
            let expected = if exposure == 0.0 {
                integrand(0.0)
            } else {
                let step = exposure / f64::from(count);
                let sum = (0..=count)
                    .map(|i| {
                        let weight = if i == 0 || i == count {
                            1.0
                        } else if i % 2 == 0 {
                            2.0
                        } else {
                            4.0
                        };
                        weight * integrand(-exposure * 0.5 + f64::from(i) * step)
                    })
                    .sum::<f64>();
                sum / (3.0 * f64::from(count))
            };
            assert!((value - expected).abs() < 2e-12, "{value} != {expected}");
            assert!(value >= (1.0 - amplitude).powi(4) && value <= (1.0 + amplitude).powi(4));
        }
    }
    // All nonconstant integer-centihertz combinations integrate away in100s.
    assert!((four_factor_average(0.8, phases, 100.0).unwrap() - 1.0).abs() < 2e-15);
    assert_eq!(four_factor_average(0.8, phases, f64::MAX), Some(1.0));
    assert!(four_factor_average(1.01, phases, 0.0).is_none());
    assert!(four_factor_average(0.5, phases, -0.01).is_none());
    let extreme = frame_midpoint_phases(f64::MAX, f64::MAX).unwrap();
    assert!(
        extreme
            .into_iter()
            .all(|x| x.is_finite() && (0.0..std::f64::consts::TAU).contains(&x))
    );
    assert!(frame_midpoint_phases(f64::NAN, 0.0).is_none());
}

#[test]
fn scalar_f32_shader_terms_track_the_reference_without_changing_positive_bounds() {
    use earth_math::rendering::stars::{display_phases, phase_hash, star_phase};
    let mut maximum = 0.0_f64;
    for height in [0.0, 5000.0, 20000.0, 40000.0] {
        for elevation in [0.0, 0.7, 1.0] {
            for id in [0_u32, 23, 2999] {
                for (time, exposure) in [(0.0, 0.0), (0.4, 0.017), (123.123, 0.4)] {
                    let variance = reference_zenith_variance(height).unwrap();
                    let strength = strength_at_elevation(variance, elevation).unwrap();
                    let mid = frame_midpoint_phases(time, exposure).unwrap();
                    let expected = four_factor_average(
                        strength.factor_amplitude,
                        std::array::from_fn(|i| mid[i] + star_phase(id, i as u32)),
                        exposure,
                    )
                    .unwrap();
                    let uploaded = display_phases(time, exposure);
                    let phases: [f32; 4] = std::array::from_fn(|i| {
                        (phase_hash(id.wrapping_add((i as u32).wrapping_mul(0x9e3779b9))) >> 8)
                            as f32
                            * (std::f32::consts::TAU / 16_777_216.0)
                            + uploaded[0][i].atan2(uploaded[1][i])
                    });
                    let v = variance as f32 / (elevation as f32).max(0.573_576_45).powf(11.0 / 6.0);
                    let root = (1.0 + v).sqrt().sqrt();
                    let a = (2.0 * v / ((root + 1.0) * (root * root + 1.0))).sqrt();
                    let mut actual = 0.0_f32;
                    for code in 0_i32..81 {
                        let digits = [
                            code % 3 - 1,
                            (code / 3) % 3 - 1,
                            (code / 9) % 3 - 1,
                            code / 27 - 1,
                        ];
                        let count = digits.into_iter().map(i32::abs).sum::<i32>();
                        let coefficient = (a * 0.5).powf(count as f32);
                        let angle = (0..4)
                            .map(|i| digits[i] as f32 * (phases[i] - std::f32::consts::FRAC_PI_2))
                            .sum::<f32>();
                        let frequency = (0..4)
                            .map(|i| digits[i] * TEMPORAL_FREQUENCIES_CENTIHZ[i])
                            .sum::<i32>();
                        let half = std::f32::consts::PI * 0.01 * frequency as f32 * exposure as f32;
                        let average = if frequency != 0 && half.abs() > 1e-4 {
                            half.sin() / half
                        } else {
                            1.0
                        };
                        actual += coefficient * angle.cos() * average;
                    }
                    actual = actual.clamp((1.0 - a).powf(4.0), (1.0 + a).powf(4.0));
                    maximum = maximum.max((f64::from(actual) - expected).abs());
                    assert!(actual >= 0.0 && actual.is_finite());
                }
            }
        }
    }
    eprintln!("Finite scalar-f32 scintillation maximum brightness-factor error: {maximum:e}");
    assert!(maximum < 5e-6);
}
