//! Adopted body dimensions and current-frame astronomical quantities.
//! This presentation model does not run another ephemeris, terrain or optical
//! solver. Call it only when its information panel is open.
use super::{AstroInstant, AstronomyFrame, CelestialBody, MOON_RADIUS_M, SUN_RADIUS_M};
use crate::{
    Geodetic, METRES_PER_FOOT, METRES_PER_MILE, Observer, STATURE_M, Vec3d, WGS84_A_M, WGS84_B_M,
    WGS84_E2, WGS84_F, WGS84_INVERSE_FLATTENING, wgs84_surface_distance,
};
use std::f64::consts::TAU;

#[derive(Clone, Debug)]
pub struct InfoRow {
    pub label: &'static str,
    pub value: String,
}
#[derive(Clone, Copy, Debug)]
pub struct InfoSource {
    pub label: &'static str,
    pub url: &'static str,
}
#[derive(Clone, Debug)]
pub struct BodyInfo {
    pub name: &'static str,
    pub rows: Vec<InfoRow>,
    pub notes: Vec<&'static str>,
    pub sources: Vec<InfoSource>,
}
#[derive(Clone, Debug)]
pub struct AstronomyInfo {
    pub epoch: String,
    pub status: String,
    pub live_available: bool,
    /// Stable section order: Earth, Sun, Moon.
    pub bodies: [BodyInfo; 3],
}

/// Derived dimensions of the adopted sphere, not a measured physical limb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereGeometry {
    pub radius_m: f64,
    pub diameter_m: f64,
    pub circumference_m: f64,
}
impl SphereGeometry {
    pub fn new(radius_m: f64) -> Option<Self> {
        let diameter_m = 2.0 * radius_m;
        let circumference_m = TAU * radius_m;
        (radius_m.is_finite() && radius_m > 0.0 && circumference_m.is_finite()).then_some(Self {
            radius_m,
            diameter_m,
            circumference_m,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyObservation {
    /// Simultaneous receive-epoch Earth-center to body-center distance.
    pub geometric_geocentric_distance_m: f64,
    /// Simultaneous receive-epoch eye to body-center distance.
    pub geometric_eye_distance_m: f64,
    /// Eye to the existing light-time-corrected, center-aberrated model center.
    pub apparent_eye_distance_m: f64,
    /// Angular diameter of that unchanged-radius apparent sphere, without
    /// atmospheric refraction. Undefined on or inside the sphere.
    pub apparent_angular_diameter_deg: Option<f64>,
    pub light_time_seconds: f64,
}
impl BodyObservation {
    pub fn new(body: CelestialBody, eye: Vec3d) -> Option<Self> {
        let dimensions = SphereGeometry::new(body.radius_m)?;
        let geometric_geocentric_distance_m = body.center_ecef_m.length();
        let geometric_eye_distance_m = body.center_ecef_m.distance(eye);
        let apparent_eye_distance_m = body.apparent_center_ecef_m.distance(eye);
        if !eye.is_finite()
            || !body.center_ecef_m.is_finite()
            || !body.apparent_center_ecef_m.is_finite()
            || ![
                geometric_geocentric_distance_m,
                geometric_eye_distance_m,
                apparent_eye_distance_m,
                body.light_time_seconds,
            ]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
        {
            return None;
        }
        Some(Self {
            geometric_geocentric_distance_m,
            geometric_eye_distance_m,
            apparent_eye_distance_m,
            apparent_angular_diameter_deg: (apparent_eye_distance_m > dimensions.radius_m).then(
                || {
                    2.0 * (dimensions.radius_m / apparent_eye_distance_m)
                        .asin()
                        .to_degrees()
                },
            ),
            light_time_seconds: body.light_time_seconds,
        })
    }
}

/// Both unit systems are shown together regardless of the main HUD setting.
/// Three decimal places are display rounding, not source accuracy.
pub fn format_paired_length(metres: f64) -> String {
    let feet = metres / METRES_PER_FOOT;
    if !metres.is_finite() || !feet.is_finite() {
        return "Unavailable".to_owned();
    }
    format!(
        "{:.3} km / {metres:.3} m | {:.3} mi / {feet:.3} ft",
        metres / 1000.0,
        metres / METRES_PER_MILE
    )
}

fn row(label: &'static str, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}
fn length(label: &'static str, value: f64) -> InfoRow {
    row(label, format_paired_length(value))
}
const UNAVAILABLE: &str = "Unavailable for the current sample";
const ROUNDING_NOTE: &str =
    "Displayed digits are numerical rounding, not measured physical accuracy.";
const UNIT_SOURCE: InfoSource = InfoSource {
    label: "NIST: international foot and mile",
    url: "https://www.nist.gov/pml/us-surveyfoot/revised-unit-conversion-factors",
};

impl AstronomyInfo {
    /// A frame is usable only for this exact instant, eye position and motion
    /// convention. Looking around does not change these scalar body quantities.
    /// Missing, stale or invalid frames never retain old live values.
    pub fn new(
        instant: AstroInstant,
        observer: Observer,
        follow_earth: bool,
        frame: Option<&AstronomyFrame>,
    ) -> Self {
        let eye_geo = observer.eye_geodetic();
        let eye = observer.eye_position();
        let observer_valid = Geodetic::new(
            observer.feet.lat_deg,
            observer.feet.lon_deg,
            observer.feet.height_m,
        )
        .is_ok()
            && Geodetic::new(eye_geo.lat_deg, eye_geo.lon_deg, eye_geo.height_m).is_ok()
            && eye.is_finite()
            && crate::supported_astronomy_position(eye)
            && (0.1..=STATURE_M).contains(&observer.eye_height_m);
        let identity_matches = frame.is_some_and(|f| {
            f.sample_instant == instant
                && f.sample_observer_ecef_m == eye
                && f.sample_follow_earth == follow_earth
        });
        let observations = frame.and_then(|f| {
            if !observer_valid
                || !identity_matches
                || f.sun.radius_m != SUN_RADIUS_M
                || f.moon.radius_m != MOON_RADIUS_M
                || !f.earth_rotation_angle_rad.is_finite()
                || !(0.0..=1.0).contains(&f.moon_illuminated_fraction)
            {
                return None;
            }
            Some([
                BodyObservation::new(f.sun, eye)?,
                BodyObservation::new(f.moon, eye)?,
            ])
        });
        let live = observations.is_some();
        let status = if !observer_valid {
            "Current observer unavailable; adopted body dimensions remain available.".to_owned()
        } else if frame.is_none() {
            "Current ephemeris unavailable; adopted body dimensions remain available.".to_owned()
        } else if !identity_matches {
            "Current astronomy sample is updating; previous live quantities are withheld."
                .to_owned()
        } else if !live {
            "Current astronomy sample is invalid; live quantities are unavailable.".to_owned()
        } else {
            let current = frame.unwrap();
            format!(
                "{} Receiver convention: {}.",
                current.quality.description(),
                current.velocity_convention.description()
            )
        };
        let mut earth = earth_info();
        earth.rows.push(row(
            "Apparent-body receiver convention",
            frame.filter(|_| live).map_or_else(
                || UNAVAILABLE.to_owned(),
                |f| f.velocity_convention.description().to_owned(),
            ),
        ));
        earth.rows.push(row(
            "Eyes to Earth's center",
            if observer_valid {
                format_paired_length(eye.length())
            } else {
                UNAVAILABLE.to_owned()
            },
        ));
        earth.rows.push(row(
            "Eye height above WGS84 ellipsoid",
            if observer_valid {
                format_paired_length(eye_geo.height_m)
            } else {
                UNAVAILABLE.to_owned()
            },
        ));
        earth.rows.push(row(
            "Earth rotation angle (UT1)",
            frame.filter(|_| live).map_or_else(
                || UNAVAILABLE.to_owned(),
                |f| {
                    format!(
                        "{:.9} deg",
                        f.earth_rotation_angle_rad.to_degrees().rem_euclid(360.0)
                    )
                },
            ),
        ));
        let sun = sphere_info("Sun", SUN_RADIUS_M, observations.map(|o| o[0]), None);
        let moon = sphere_info(
            "Moon",
            MOON_RADIUS_M,
            observations.map(|o| o[1]),
            frame.filter(|_| live).map(|f| f.moon_illuminated_fraction),
        );
        Self {
            epoch: format!(
                "{} ({})",
                instant.calendar_string(),
                if instant.calendar().year < 1960 {
                    "modeled UT1 calendar"
                } else {
                    "UTC calendar"
                }
            ),
            status,
            live_available: live,
            bodies: [earth, sun, moon],
        }
    }
}

fn earth_info() -> BodyInfo {
    let quarter = wgs84_surface_distance(
        Geodetic {
            lat_deg: 0.0,
            lon_deg: 0.0,
            height_m: 0.0,
        },
        Geodetic {
            lat_deg: 90.0,
            lon_deg: 0.0,
            height_m: 0.0,
        },
    );
    BodyInfo {
        name: "Earth",
        rows: vec![
            length("Equatorial radius a (defining WGS84)", WGS84_A_M),
            length("Polar radius b (derived WGS84)", WGS84_B_M),
            length("Equatorial diameter", 2.0 * WGS84_A_M),
            length("Polar diameter", 2.0 * WGS84_B_M),
            length("Equatorial circumference", TAU * WGS84_A_M),
            row(
                "Meridional circumference",
                quarter.map_or_else(
                    |_| UNAVAILABLE.to_owned(),
                    |q| format_paired_length(4.0 * q),
                ),
            ),
            row(
                "Inverse flattening 1/f (defining WGS84)",
                format!("{WGS84_INVERSE_FLATTENING:.9}"),
            ),
            row("Flattening f (derived)", format!("{WGS84_F:.15}")),
            row(
                "First eccentricity squared e^2 (derived)",
                format!("{WGS84_E2:.15}"),
            ),
        ],
        notes: vec![
            "WGS84 is an oblate reference ellipsoid, not a sphere. These dimensions exclude terrain, tides and geoid undulation.",
            "Eye height here is above WGS84 h=0, not clearance above terrain or height above a sea-level reference.",
            "Meridional circumference is four pole-to-equator WGS84 geodesics; it is not 2 pi times the polar radius.",
            ROUNDING_NOTE,
        ],
        sources: vec![
            InfoSource {
                label: "NGA: defining WGS84 parameters",
                url: "https://earth-info.nga.mil/index.php?dir=wgs84&action=wgs84",
            },
            UNIT_SOURCE,
        ],
    }
}

fn sphere_info(
    name: &'static str,
    radius: f64,
    observation: Option<BodyObservation>,
    illumination: Option<f64>,
) -> BodyInfo {
    let dimensions = SphereGeometry::new(radius).expect("finite adopted body radius");
    let mut rows = vec![
        length(
            if name == "Sun" {
                "Adopted nominal solar radius"
            } else {
                "Adopted mean lunar radius"
            },
            dimensions.radius_m,
        ),
        length("Model diameter", dimensions.diameter_m),
        length(
            "Model great-circle circumference",
            dimensions.circumference_m,
        ),
    ];
    for (label, quantity) in [
        (
            "Earth center to body center (simultaneous)",
            observation.map(|o| o.geometric_geocentric_distance_m),
        ),
        (
            "Eyes to body center (simultaneous)",
            observation.map(|o| o.geometric_eye_distance_m),
        ),
        (
            "Eyes to apparent model center",
            observation.map(|o| o.apparent_eye_distance_m),
        ),
    ] {
        rows.push(row(
            label,
            quantity.map_or_else(|| UNAVAILABLE.to_owned(), format_paired_length),
        ));
    }
    rows.push(row(
        "Apparent model angular diameter (unrefracted)",
        observation.map_or_else(
            || UNAVAILABLE.to_owned(),
            |o| {
                o.apparent_angular_diameter_deg.map_or_else(
                    || "Undefined on or inside the model sphere".to_owned(),
                    |degrees| format!("{degrees:.9} deg / {:.6} arcmin", degrees * 60.0),
                )
            },
        ),
    ));
    rows.push(row(
        "Center light travel time (reception solution)",
        observation.map_or_else(
            || UNAVAILABLE.to_owned(),
            |o| format!("{:.6} s", o.light_time_seconds),
        ),
    ));
    if name == "Moon" {
        rows.push(row(
            "Geometric illuminated fraction",
            illumination.map_or_else(
                || UNAVAILABLE.to_owned(),
                |fraction| format!("{:.4}%", fraction * 100.0),
            ),
        ));
    }
    let (note, source) = if name == "Sun" {
        (
            "The IAU nominal solar radius is an exact conversion constant adopted for this sphere, not the instantaneous measured photospheric radius.",
            InfoSource {
                label: "IAU 2015 Resolution B3: nominal solar radius",
                url: "https://www.iau.org/common/Uploaded%20files/IAUGA2015-Resolution-B3-recommended-nominal-conversion.pdf",
            },
        )
    } else {
        (
            "The adopted mean lunar radius is a spherical approximation. Lunar terrain, shape variations and photometric brightness are not modeled here.",
            InfoSource {
                label: "NASA/JPL: lunar mean radius",
                url: "https://ssd.jpl.nasa.gov/sats/phys_par/",
            },
        )
    };
    let mut notes = vec![
        note,
        "Distances refer to body centers, not visible surface range. Simultaneous centers use the receive epoch; apparent centers include light time and observer aberration.",
        "Angular diameter describes the existing unrefracted model sphere: only its center direction is aberrated. This is not a full relativistic extended-disk calculation or a test of visibility through Earth, terrain or weather.",
        ROUNDING_NOTE,
    ];
    if name == "Moon" {
        notes.push("Illuminated fraction is evaluated at the Moon's emission event with retarded sunlight. It describes geometric phase, not measured brightness or simultaneous receive-time illumination.");
    }
    BodyInfo {
        name,
        rows,
        notes,
        sources: vec![
            source,
            InfoSource {
                label: "NASA/JPL: DE440 and planetary ephemerides",
                url: "https://ssd.jpl.nasa.gov/planets/eph_export.html",
            },
            UNIT_SOURCE,
        ],
    }
}
