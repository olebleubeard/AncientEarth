//! Height-free WBM presentation and classification on the exact WGS84 surface.
use crate::{Geodetic, Observer};

/// User-specified startup ground coordinate, unrelated to a city catalogue.
pub fn default_observer() -> Observer {
    let mut observer = Observer::new(Geodetic::new(29.305_549, -94.767_015, 0.0).unwrap());
    observer.heading_deg = 90.0;
    observer.speed_mps = 10.0;
    observer
}
