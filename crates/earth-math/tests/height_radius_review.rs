//! Independent boundary and rounding cases for presentation units and the
//! download-only request policy. Unit definitions use exact decimal SI values.
use earth_math::{Geodetic, UnitSystem, survey, topography::*};

#[test]
fn download_policy_accepts_both_exact_unit_endpoints_without_admitting_the_next_float() {
    assert_eq!(MAX_RADIUS_M, 160_934.4);
    assert_eq!(DEFAULT_RADIUS_M, MAX_RADIUS_M);
    let center = Geodetic::new(29.3, -94.8, 17.0).unwrap();
    for (units, endpoint) in [
        (UnitSystem::Imperial, 100.0_f64),
        (UnitSystem::Metric, 160.9344),
    ] {
        let metres = radius_from_display(endpoint, units).unwrap();
        assert_eq!(metres, MAX_RADIUS_M);
        assert_eq!(radius_display_value(metres, units), endpoint);
        assert!(DownloadRegion::new(center, metres).is_ok());
        assert!(matches!(
            radius_from_display(endpoint.next_up(), units),
            Err(TopographyError::InvalidRadius)
        ));
        assert!(radius_from_display(endpoint.next_down(), units).unwrap() <= MAX_RADIUS_M);
        for value in [
            0.0,
            -0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
        ] {
            assert!(matches!(
                radius_from_display(value, units),
                Err(TopographyError::InvalidRadius)
            ));
        }
    }
    assert!(matches!(
        DownloadRegion::new(center, MAX_RADIUS_M.next_up()),
        Err(TopographyError::InvalidRadius)
    ));
    let mut value = MAX_RADIUS_M;
    for index in 0..100 {
        let units = if index % 2 == 0 {
            UnitSystem::Metric
        } else {
            UnitSystem::Imperial
        };
        value = radius_from_display(radius_display_value(value, units), units).unwrap();
        assert_eq!(value, MAX_RADIUS_M);
    }
    // Radius is a download policy; a long survey row keeps its separate domain.
    assert_eq!(
        survey::row_distance_from_display(101.0, UnitSystem::Imperial).unwrap(),
        162_543.744
    );
    assert_eq!(
        survey::row_distance_from_display(200.0, UnitSystem::Metric).unwrap(),
        200_000.0
    );
    for units in [UnitSystem::Imperial, UnitSystem::Metric] {
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(survey::row_distance_from_display(value, units).is_err());
        }
    }
}

#[test]
fn signed_whole_inches_round_before_splitting_and_keep_original_secondary_distance() {
    let imperial = UnitSystem::Imperial;
    for (metres, expected) in [
        (0.0, "0 ft 0 in (0.000000 mi)"),
        (-0.0, "0 ft 0 in (0.000000 mi)"),
        (1.72, "5 ft 8 in (0.001069 mi)"),
        (1.0, "3 ft 3 in (0.000621 mi)"),
        (1.8288, "6 ft 0 in (0.001136 mi)"),
        (-1.8288, "-6 ft 0 in (-0.001136 mi)"),
        (0.0254, "0 ft 1 in (0.000016 mi)"),
        (-0.0254, "-0 ft 1 in (-0.000016 mi)"),
        (0.0127, "0 ft 1 in (0.000008 mi)"),
        (-0.0127, "-0 ft 1 in (-0.000008 mi)"),
        (0.0126, "0 ft 0 in (0.000008 mi)"),
        (-0.0126, "0 ft 0 in (-0.000008 mi)"),
        (1609.344, "5280 ft 0 in (1.000000 mi)"),
    ] {
        assert_eq!(
            imperial.format_height_feet_inches(metres),
            expected,
            "{metres}"
        );
    }
    // Independently choose hundredths of an inch on either side of the carry.
    for (hundredths, expected) in [
        (1149, "0 ft 11 in"),
        (1150, "1 ft 0 in"),
        (1151, "1 ft 0 in"),
        (7149, "5 ft 11 in"),
        (7150, "6 ft 0 in"),
        (7151, "6 ft 0 in"),
    ] {
        let metres = f64::from(hundredths) * 254.0 / 1_000_000.0;
        assert!(
            imperial
                .format_height_feet_inches(metres)
                .starts_with(expected)
        );
        assert!(
            imperial
                .format_height_feet_inches(-metres)
                .starts_with(&format!("-{expected}"))
        );
    }
    for metres in [-10_000.0, -35.125, -0.0, 0.0, 0.01, 1.72, 1e9] {
        assert_eq!(
            UnitSystem::Metric.format_height_feet_inches(metres),
            UnitSystem::Metric.format_height(metres)
        );
    }
    for metres in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        3e14,
        -3e14,
    ] {
        assert_eq!(imperial.format_height_feet_inches(metres), "Unavailable");
    }
    // Every whole-inch primary display in the supported navigation range stays
    // normalized; the parsed result differs by at most a half inch plus f64 ulp.
    for metres in [-10_000.0, -1.72, 0.0126, 1.72, 1e6, 1e9] {
        let text = imperial.format_height_feet_inches(metres);
        let tokens: Vec<_> = text.split_whitespace().collect();
        let feet: i64 = tokens[0].parse().unwrap();
        let inches: i64 = tokens[2].parse().unwrap();
        assert!((0..12).contains(&inches));
        let sign = if tokens[0].starts_with('-') {
            -1.0
        } else {
            1.0
        };
        let reconstructed = sign * (feet.unsigned_abs() as f64 * 12.0 + inches as f64) * 0.0254;
        assert!((reconstructed - metres).abs() <= 0.0127 + 4.0 * f64::EPSILON * metres.abs());
    }
}
