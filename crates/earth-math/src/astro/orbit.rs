//! Simultaneous Sun-centered geometric annotations in fixed ICRF directions.
//! Their display shares the continuous Observer; they do not move its pose.
use super::{
    AstroError, AstroInstant, AstroQuality, CalendarDateTime, CartesianState, EarthOrientationData,
    Ephemeris, Mat3d, SECONDS_PER_DAY, eop, iau_body_orientation,
    info::{InfoRow, format_paired_length},
    orientation, tdb_minus_tt_seconds,
};
use crate::Vec3d;

/// A display window, not a fitted or assumed physical orbital period.
pub const TRAIL_SPAN_SECONDS: f64 = 365.25 * SECONDS_PER_DAY;
/// Positions are ephemeris samples; segments joining them are display chords.
pub const TRAIL_STEP_SECONDS: f64 = SECONDS_PER_DAY / 4.0;

#[derive(Clone, Copy, Debug)]
pub struct OrbitFrame {
    pub instant: AstroInstant,
    /// Earth-center minus Sun-center at the same reception epoch.
    pub earth_position_m: Vec3d,
    pub moon_position_m: Vec3d,
    pub earth_velocity_mps: Vec3d,
    /// WGS84/ECEF body axes to fixed celestial axes at this epoch.
    pub earth_to_icrf: Mat3d,
    /// Geometric text-PCK orientations; no retarded emission epoch in this view.
    pub moon_to_icrf: Mat3d,
    pub sun_to_icrf: Mat3d,
    /// Modeled celestial intermediate pole, before terrestrial polar motion.
    pub axis_icrf: Vec3d,
    /// Instantaneous heliocentric Earth-center osculating-plane normal.
    pub orbital_normal_icrf: Vec3d,
    pub orbital_radial_icrf: Vec3d,
    pub orbital_tangent_icrf: Vec3d,
    /// Angle between CIP and the osculating-plane normal; not mean obliquity.
    pub axial_tilt_deg: f64,
    pub quality: AstroQuality,
}

impl OrbitFrame {
    /// UI-ready values. All lengths retain the model's physical scale.
    pub fn information(self) -> Vec<InfoRow> {
        vec![
            InfoRow {
                label: "Current geometric Sun-Earth center distance",
                value: format_paired_length(self.earth_position_m.length()),
            },
            InfoRow {
                label: "Current geometric Earth-Moon center distance",
                value: format_paired_length(self.moon_position_m.distance(self.earth_position_m)),
            },
            InfoRow {
                label: "CIP tilt to instantaneous heliocentric orbital normal",
                value: format!("{:.6} deg", self.axial_tilt_deg),
            },
            InfoRow {
                label: "CIP angle above that orbital plane",
                value: format!("{:.6} deg", (90.0 - self.axial_tilt_deg).abs()),
            },
            InfoRow {
                label: "Heliocentric Earth-center speed",
                value: format!("{:.3} m/s", self.earth_velocity_mps.length()),
            },
            InfoRow {
                label: "Orientation and calendar quality",
                value: self.quality.description(),
            },
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitPoint {
    pub instant: AstroInstant,
    pub earth_position_m: Vec3d,
}

#[derive(Clone, Debug)]
pub struct OrbitTrail {
    pub start: AstroInstant,
    pub end: AstroInstant,
    pub points: Vec<OrbitPoint>,
    pub clipped_to_calendar: bool,
}
impl OrbitTrail {
    /// A stored path remains valid for these dated samples while time runs in
    /// either direction. Rebuild after an out-of-window jump; never extrapolate.
    pub fn covers(&self, instant: AstroInstant) -> bool {
        (self.start.tt_seconds_j2000()..=self.end.tt_seconds_j2000())
            .contains(&instant.tt_seconds_j2000())
    }
    pub fn description(&self) -> String {
        format!(
            "{} to {}; {:.3} TT days, at most 0.25 TT day between ephemeris samples; {}. The polyline is not forced closed.",
            self.start.calendar_string(),
            self.end.calendar_string(),
            (self.end.tt_seconds_j2000() - self.start.tt_seconds_j2000()) / SECONDS_PER_DAY,
            if self.clipped_to_calendar {
                "window clipped to the supported calendar"
            } else {
                "365.25-day display window, not an assumed exact orbital period"
            }
        )
    }
}

/// Normal of the state vector's instantaneous plane, evaluated with scaled
/// unit vectors to avoid overflow. Collinear/degenerate states fail closed.
pub fn osculating_normal(state: CartesianState) -> Result<Vec3d, AstroError> {
    let radial = state
        .position_m
        .normalized()
        .ok_or(AstroError::InvalidObserver)?;
    let velocity = state
        .velocity_mps
        .normalized()
        .ok_or(AstroError::InvalidObserver)?;
    let cross = radial.cross(velocity);
    if !cross.is_finite() || cross.length() <= 64.0 * f64::EPSILON {
        return Err(AstroError::InvalidObserver);
    }
    cross.normalized().ok_or(AstroError::InvalidObserver)
}

/// Stable angle of two oriented nonzero vectors, in [0,180] degrees.
pub fn axis_tilt_degrees(axis: Vec3d, normal: Vec3d) -> Result<f64, AstroError> {
    let a = axis.normalized().ok_or(AstroError::InvalidObserver)?;
    let n = normal.normalized().ok_or(AstroError::InvalidObserver)?;
    Ok(a.cross(n).length().atan2(a.dot(n)).to_degrees())
}

impl Ephemeris {
    /// Simultaneous geometric state in Sun-centered, nonrotating ICRF axes.
    /// The origin follows the actual Sun: this is a display reference system,
    /// not a free-fall trajectory or an apparent view from a physical observer.
    pub fn heliocentric_state(
        &self,
        body: i32,
        instant: AstroInstant,
    ) -> Result<CartesianState, AstroError> {
        let tt = instant.tt_seconds_j2000();
        let tdb = tt + tdb_minus_tt_seconds(tt);
        let sun = self.state(10, tdb)?;
        let target = self.state(body, tdb)?;
        Ok(CartesianState {
            position_m: target.position_m - sun.position_m,
            velocity_mps: target.velocity_mps - sun.velocity_mps,
        })
    }

    pub fn orbit_frame(
        &self,
        instant: AstroInstant,
        earth_orientation: Option<&EarthOrientationData>,
    ) -> Result<OrbitFrame, AstroError> {
        let earth = self.heliocentric_state(399, instant)?;
        let moon = self.heliocentric_state(301, instant)?;
        let orbital_normal_icrf = osculating_normal(earth)?;
        let orbital_radial_icrf = earth
            .position_m
            .normalized()
            .ok_or(AstroError::InvalidObserver)?;
        let orbital_tangent_icrf = orbital_normal_icrf
            .cross(orbital_radial_icrf)
            .normalized()
            .ok_or(AstroError::InvalidObserver)?;
        let tt = instant.tt_seconds_j2000();
        let tdb = tt + tdb_minus_tt_seconds(tt);
        let axis_icrf = orientation::celestial_intermediate_pole(tt)
            .normalized()
            .ok_or(AstroError::InvalidObserver)?;
        let resolved = eop::resolve(instant, earth_orientation);
        let earth_to_icrf =
            orientation::earth_orientation(tt, resolved.ut1, resolved.xp, resolved.yp)
                .0
                .transpose();
        let date = instant.calendar();
        Ok(OrbitFrame {
            instant,
            earth_position_m: earth.position_m,
            moon_position_m: moon.position_m,
            earth_velocity_mps: earth.velocity_mps,
            earth_to_icrf,
            moon_to_icrf: iau_body_orientation(301, tdb)?,
            sun_to_icrf: iau_body_orientation(10, tdb)?,
            axis_icrf,
            orbital_normal_icrf,
            orbital_radial_icrf,
            orbital_tangent_icrf,
            axial_tilt_deg: axis_tilt_degrees(axis_icrf, orbital_normal_icrf)?,
            quality: AstroQuality {
                earth_rotation: resolved.quality,
                historical_ut1_calendar: date.year < 1960,
                future_leap_seconds_assumed: date.year > 2026,
                outside_nutation_reference_interval: date.year < 1900 || date.year > 2100,
            },
        })
    }

    /// Finite annual-scale geometric trajectory. Every point subtracts the
    /// Sun's position at that point's own epoch, not today's Sun position.
    pub fn earth_orbit_trail(&self, center: AstroInstant) -> Result<OrbitTrail, AstroError> {
        let (minimum, maximum) = calendar_limits()?;
        let middle = center.tt_seconds_j2000();
        let requested_start = middle - TRAIL_SPAN_SECONDS / 2.0;
        let requested_end = middle + TRAIL_SPAN_SECONDS / 2.0;
        let start_seconds = requested_start.max(minimum.tt_seconds_j2000());
        let end_seconds = requested_end.min(maximum.tt_seconds_j2000());
        let start = AstroInstant::from_tt_seconds_j2000(start_seconds)?;
        let end = AstroInstant::from_tt_seconds_j2000(end_seconds)?;
        let segments = ((end_seconds - start_seconds) / TRAIL_STEP_SECONDS).ceil() as usize;
        if segments == 0 || segments > 1462 {
            return Err(AstroError::InvalidStep);
        }
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let seconds = if i == segments {
                end_seconds
            } else {
                start_seconds + i as f64 * TRAIL_STEP_SECONDS
            };
            let instant = AstroInstant::from_tt_seconds_j2000(seconds)?;
            let earth_position_m = self.heliocentric_state(399, instant)?.position_m;
            points.push(OrbitPoint {
                instant,
                earth_position_m,
            });
        }
        Ok(OrbitTrail {
            start,
            end,
            points,
            clipped_to_calendar: start_seconds != requested_start || end_seconds != requested_end,
        })
    }
}

/// Last representable TT instant strictly before 2150, with the same held
/// future leap-second convention as the existing calendar. It avoids an
/// arbitrary missing millisecond at the upper endpoint.
fn calendar_limits() -> Result<(AstroInstant, AstroInstant), AstroError> {
    let minimum = AstroInstant::from_calendar(CalendarDateTime {
        year: 1850,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0.0,
    })?;
    let last_second = AstroInstant::from_calendar(CalendarDateTime {
        year: 2149,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 59.0,
    })?;
    let exclusive_end = last_second.tt_seconds_j2000() + 1.0;
    let maximum = AstroInstant::from_tt_seconds_j2000(f64::from_bits(exclusive_end.to_bits() - 1))?;
    Ok((minimum, maximum))
}
