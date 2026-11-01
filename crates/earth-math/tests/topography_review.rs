//! Independent geodesic enclosure and height/radius unit checks.
use earth_math::{topography::*, *};
#[path = "fixtures/topography_proj.rs"]
mod reference;

fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b} (tolerance {tolerance})"
    );
}

#[test]
fn conservative_rectangles_cover_independent_proj_direct_geodesic_sweeps() {
    let mut samples = 0;
    let mut policy_rejected_samples = 0;
    for fixture in reference::CASES {
        if fixture.radius_m > 160_934.4 {
            assert!(matches!(
                DownloadRegion::new(
                    Geodetic::new(fixture.center[0], fixture.center[1], 1234.0).unwrap(),
                    fixture.radius_m,
                ),
                Err(TopographyError::InvalidRadius)
            ));
            policy_rejected_samples += fixture.samples;
            continue;
        }
        let region = DownloadRegion::new(
            Geodetic::new(fixture.center[0], fixture.center[1], 1234.0).unwrap(),
            fixture.radius_m,
        )
        .unwrap();
        assert!((1..=2).contains(&region.query_boxes.len()));
        let mut intervals = Vec::new();
        for rectangle in &region.query_boxes {
            assert!(rectangle.south_deg <= fixture.latitude[0]);
            assert!(rectangle.north_deg >= fixture.latitude[1]);
            for turn in -2..=2 {
                intervals.push((
                    rectangle.west_deg + f64::from(turn) * 360.0,
                    rectangle.east_deg + f64::from(turn) * 360.0,
                ));
            }
        }
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered = fixture.longitude_unwrapped[0];
        for (left, right) in intervals {
            if left <= covered && right > covered {
                covered = right;
            }
        }
        // Rectangular interval coverage of the extrema certifies all retained
        // samples, including date-line splits. It is not a dense-grid proof.
        assert!(
            covered >= fixture.longitude_unwrapped[1],
            "longitude gap for {:?}",
            fixture.center
        );
        samples += fixture.samples;
    }
    assert_eq!(samples, 23_771);
    assert_eq!(policy_rejected_samples, 6_483);
}

#[test]
fn bounds_handle_poles_date_line_tiny_radii_and_reject_global_requests() {
    for (latitude, longitude) in [
        (0.0, 180.0),
        (0.0, -180.0),
        (20.0, 179.999),
        (-20.0, -179.999),
    ] {
        let center = Geodetic::new(latitude, longitude, 0.0).unwrap();
        let region = DownloadRegion::new(center, DEFAULT_RADIUS_M).unwrap();
        assert_eq!(region.query_boxes.len(), 2);
        assert!(region.query_boxes.iter().any(|b| b.west_deg == -180.0));
        assert!(region.query_boxes.iter().any(|b| b.east_deg == 180.0));
        assert_eq!(
            region.query_boxes,
            DownloadRegion::new(center.with_height(1e9).unwrap(), DEFAULT_RADIUS_M)
                .unwrap()
                .query_boxes
        );
    }
    for latitude in [-90.0, -89.0, 89.0, 90.0] {
        let region = DownloadRegion::new(
            Geodetic::new(latitude, 45.0, 0.0).unwrap(),
            DEFAULT_RADIUS_M,
        )
        .unwrap();
        assert_eq!(region.query_boxes.len(), 1);
        assert_eq!(
            (
                region.query_boxes[0].west_deg,
                region.query_boxes[0].east_deg
            ),
            (-180.0, 180.0)
        );
    }
    let center = Geodetic::new(12.3, -45.6, 0.0).unwrap();
    for radius in [f64::from_bits(1), 1e-9, 1e-3] {
        let b = DownloadRegion::new(center, radius).unwrap().query_boxes[0];
        assert!(b.west_deg < center.lon_deg && b.east_deg > center.lon_deg);
        assert!(b.south_deg < center.lat_deg && b.north_deg > center.lat_deg);
    }
    assert!(matches!(
        DownloadRegion::new(center, f64::MAX),
        Err(TopographyError::InvalidRadius)
    ));
}

#[test]
fn invalid_requests_fail_closed_and_query_text_preserves_numeric_bounds() {
    let center = Geodetic::new(35.0, -95.0, 0.0).unwrap();
    for radius in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(DownloadRegion::new(center, radius).is_err());
    }
    for invalid in [
        Geodetic {
            lat_deg: 91.0,
            ..center
        },
        Geodetic {
            lon_deg: -181.0,
            ..center
        },
        Geodetic {
            height_m: f64::NAN,
            ..center
        },
    ] {
        assert!(DownloadRegion::new(invalid, DEFAULT_RADIUS_M).is_err());
    }
    for coordinates in [
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, -1.0, 1.0],
        [-181.0, 0.0, 0.0, 1.0],
        [0.0, -91.0, 1.0, 1.0],
        [0.0, 0.0, f64::NAN, 1.0],
        [0.0, 0.0, 1.0, f64::INFINITY],
    ] {
        assert!(
            GeoBounds::new(
                coordinates[0],
                coordinates[1],
                coordinates[2],
                coordinates[3]
            )
            .is_err()
        );
    }
    for radius in [1e-9, DEFAULT_RADIUS_M, 100_000.0] {
        for b in DownloadRegion::new(center, radius).unwrap().query_boxes {
            let decoded: Vec<f64> = b
                .query_value()
                .split(',')
                .map(|s| s.parse().unwrap())
                .collect();
            assert_eq!(
                decoded,
                vec![b.west_deg, b.south_deg, b.east_deg, b.north_deg]
            );
        }
    }
}

#[test]
fn heights_keep_primary_units_and_radius_uses_exact_international_miles() {
    close(DEFAULT_RADIUS_M, 160934.4, 0.0);
    assert_eq!(radius_unit(UnitSystem::Imperial), "mi");
    assert_eq!(radius_unit(UnitSystem::Metric), "km");
    assert_eq!(
        radius_display_value(DEFAULT_RADIUS_M, UnitSystem::Imperial),
        100.0
    );
    close(
        radius_display_value(DEFAULT_RADIUS_M, UnitSystem::Metric),
        160.9344,
        3e-14,
    );
    assert_eq!(
        radius_from_display(100.0, UnitSystem::Imperial).unwrap(),
        DEFAULT_RADIUS_M
    );
    close(
        radius_from_display(160.9344, UnitSystem::Metric).unwrap(),
        DEFAULT_RADIUS_M,
        3e-11,
    );
    for units in [UnitSystem::Metric, UnitSystem::Imperial] {
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(radius_from_display(invalid, units).is_err());
        }
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(units.format_height(invalid), "Unavailable");
        }
    }
    assert_eq!(
        UnitSystem::Imperial.format_height(1609.344),
        "5280.000 ft (1.000000 mi)"
    );
    assert_eq!(
        UnitSystem::Imperial.format_height(-1609.344),
        "-5280.000 ft (-1.000000 mi)"
    );
    assert_eq!(
        UnitSystem::Imperial.format_height(1.8288),
        "6.000 ft (0.001136 mi)"
    );
    assert_eq!(UnitSystem::Imperial.format_height(f64::MAX), "Unavailable");
    assert_eq!(
        UnitSystem::Metric.format_height(1000000.0),
        "1000000.000 m (1000.000000 km)"
    );
    assert_eq!(
        UnitSystem::Metric.format_height(-35.125),
        "-35.125 m (-0.035125 km)"
    );
}
