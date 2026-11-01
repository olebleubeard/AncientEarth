//! Dated orbit annotations for the single physical flight camera.
use earth_math::astro::orbit::{OrbitFrame, OrbitTrail};

#[cfg_attr(feature = "desktop", derive(bevy::prelude::Resource))]
pub struct OrbitOverview {
    /// Derived display eligibility; never switches or moves the observer.
    pub visible: bool,
    pub frame: Option<OrbitFrame>,
    pub trail: Option<OrbitTrail>,
    pub error: Option<String>,
    pub show_orbit: bool,
    pub show_axis: bool,
    pub show_moon_axis: bool,
}

impl Default for OrbitOverview {
    fn default() -> Self {
        Self {
            visible: false,
            frame: None,
            trail: None,
            error: None,
            show_orbit: true,
            show_axis: true,
            show_moon_axis: true,
        }
    }
}
