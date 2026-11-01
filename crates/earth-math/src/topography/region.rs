//! Conservative geographic query rectangles for a WGS84 surface-distance disk.
//!
//! This is an enclosure, not a spherical replacement for geodesic distance.
//! Source services and complete source tiles can return data outside the disk.
use super::TopographyError;
use crate::{Geodetic, METRES_PER_MILE, UnitSystem, WGS84_A_M, WGS84_E2};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// User-selected download radius limit: exactly 100 international miles,
/// or 160.9344 kilometres. The f64 constant is its nearest representation.
pub const MAX_RADIUS_M: f64 = 100.0 * METRES_PER_MILE;
pub const DEFAULT_RADIUS_M: f64 = MAX_RADIUS_M;
/// Outward padding of the angular radius and longitude half-span. This small
/// numerical guard is about 0.64 mm at an Earth-radius angular scale; it does
/// not express source accuracy or surveyed precision.
const ANGULAR_PADDING_RAD: f64 = 1e-10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoBounds {
    pub west_deg: f64,
    pub south_deg: f64,
    pub east_deg: f64,
    pub north_deg: f64,
}
impl GeoBounds {
    /// A finite positive-area rectangle which does not cross the date line.
    pub fn new(
        west_deg: f64,
        south_deg: f64,
        east_deg: f64,
        north_deg: f64,
    ) -> Result<Self, TopographyError> {
        if ![west_deg, south_deg, east_deg, north_deg]
            .into_iter()
            .all(f64::is_finite)
            || !(-180.0..=180.0).contains(&west_deg)
            || !(-180.0..=180.0).contains(&east_deg)
            || !(-90.0..=90.0).contains(&south_deg)
            || !(-90.0..=90.0).contains(&north_deg)
            || west_deg >= east_deg
            || south_deg >= north_deg
        {
            return Err(TopographyError::InvalidBounds);
        }
        Ok(Self {
            west_deg,
            south_deg,
            east_deg,
            north_deg,
        })
    }
    /// Longitude/latitude order used by bounding-box query parameters. Rust's
    /// round-trip f64 formatting avoids shrinking a bound to fixed decimals.
    pub fn query_value(self) -> String {
        format!(
            "{},{},{},{}",
            self.west_deg, self.south_deg, self.east_deg, self.north_deg
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadRegion {
    pub center: Geodetic,
    pub radius_m: f64,
    pub query_boxes: Vec<GeoBounds>,
}
impl DownloadRegion {
    pub fn new(center: Geodetic, radius_m: f64) -> Result<Self, TopographyError> {
        let center = Geodetic::new(center.lat_deg, center.lon_deg, center.height_m)
            .map_err(TopographyError::InvalidCoordinate)?;
        if !radius_m.is_finite() || radius_m <= 0.0 || radius_m > MAX_RADIUS_M {
            return Err(TopographyError::InvalidRadius);
        }
        // ds²=M²dφ²+(N cosφ)²dλ² ≥ Mmin²(dφ²+cos²φ dλ²), where
        // Mmin=a(1-e²), M≥Mmin and N≥a. Thus a WGS84 geodesic disk
        // of radius R is contained in the geodetic spherical cap R/Mmin.
        let delta = (radius_m / (WGS84_A_M * (1.0 - WGS84_E2)) + ANGULAR_PADDING_RAD).min(PI);
        let latitude = center.lat_deg.to_radians();
        let south = (latitude - delta).max(-FRAC_PI_2);
        let north = (latitude + delta).min(FRAC_PI_2);
        let south_deg = south.to_degrees().clamp(-90.0, 90.0);
        let north_deg = north.to_degrees().clamp(-90.0, 90.0);
        let mut query_boxes = Vec::with_capacity(2);
        if south <= -FRAC_PI_2 || north >= FRAC_PI_2 {
            query_boxes.push(GeoBounds::new(-180.0, south_deg, 180.0, north_deg)?);
        } else {
            // A non-polar cap has δ<π/2−|φ0|. Its longitude extrema
            // obey sin(|Δλ|)=sinδ/cosφ0; choose the smaller positive root.
            let span = (delta.sin() / latitude.cos()).clamp(0.0, 1.0).asin() + ANGULAR_PADDING_RAD;
            let longitude = center.lon_deg.to_radians();
            let west = longitude - span;
            let east = longitude + span;
            if west < -PI {
                query_boxes.push(GeoBounds::new(
                    -180.0,
                    south_deg,
                    east.to_degrees(),
                    north_deg,
                )?);
                query_boxes.push(GeoBounds::new(
                    (west + TAU).to_degrees(),
                    south_deg,
                    180.0,
                    north_deg,
                )?);
            } else if east > PI {
                query_boxes.push(GeoBounds::new(
                    west.to_degrees(),
                    south_deg,
                    180.0,
                    north_deg,
                )?);
                query_boxes.push(GeoBounds::new(
                    -180.0,
                    south_deg,
                    (east - TAU).to_degrees(),
                    north_deg,
                )?);
            } else {
                query_boxes.push(GeoBounds::new(
                    west.to_degrees(),
                    south_deg,
                    east.to_degrees(),
                    north_deg,
                )?);
            }
        }
        Ok(Self {
            center,
            radius_m,
            query_boxes,
        })
    }
}

pub fn radius_display_value(radius_m: f64, units: UnitSystem) -> f64 {
    if radius_m == MAX_RADIUS_M {
        // Preserve the exact decimal endpoint when the units are changed.
        return match units {
            UnitSystem::Metric => 160.9344,
            UnitSystem::Imperial => 100.0,
        };
    }
    radius_m
        / match units {
            UnitSystem::Metric => 1000.0,
            UnitSystem::Imperial => METRES_PER_MILE,
        }
}
pub fn radius_from_display(value: f64, units: UnitSystem) -> Result<f64, TopographyError> {
    let maximum = radius_display_value(MAX_RADIUS_M, units);
    if !value.is_finite() || value <= 0.0 || value > maximum {
        return Err(TopographyError::InvalidRadius);
    }
    let metres = value
        * match units {
            UnitSystem::Metric => 1000.0,
            UnitSystem::Imperial => METRES_PER_MILE,
        };
    if !metres.is_finite() || metres <= 0.0 {
        return Err(TopographyError::InvalidRadius);
    }
    // The accepted exact decimal 160.9344 km can multiply to one f64 step
    // above MAX_RADIUS_M. Canonicalize that endpoint without admitting any
    // displayed input above the maximum checked before multiplication.
    Ok(metres.min(MAX_RADIUS_M))
}
pub fn radius_unit(units: UnitSystem) -> &'static str {
    match units {
        UnitSystem::Metric => "km",
        UnitSystem::Imperial => "mi",
    }
}
