//! Displayed lunar pole and instantaneous geocentric orbital plane.
use super::{
    AstroError, CartesianState, Mat3d,
    orbit::{axis_tilt_degrees, osculating_normal},
};
use crate::Vec3d;

#[derive(Clone, Copy, Debug)]
pub struct LunarOrientation {
    pub emission_tdb_seconds_j2000: f64,
    /// Text-PCK cartographic north, not the exact angular-velocity vector.
    pub axis_ecef: Vec3d,
    pub orbital_normal_ecef: Vec3d,
    pub orbital_radial_ecef: Vec3d,
    pub orbital_tangent_ecef: Vec3d,
    pub tilt_deg: f64,
}
impl LunarOrientation {
    /// Both states and body attitude are at the same lunar emission epoch;
    /// `icrf_to_ecef` is the receiving Earth's orientation.
    pub fn from_state(
        epoch: f64,
        state: CartesianState,
        body_to_ecef: Mat3d,
        icrf_to_ecef: Mat3d,
    ) -> Result<Self, AstroError> {
        let proper = |m: Mat3d| {
            let axes = [
                Vec3d::new(1.0, 0.0, 0.0),
                Vec3d::new(0.0, 1.0, 0.0),
                Vec3d::new(0.0, 0.0, 1.0),
            ]
            .map(|a| m.transform(a));
            axes.iter()
                .all(|a| a.is_finite() && (a.length() - 1.0).abs() < 1e-10)
                && axes[0].dot(axes[1]).abs() < 1e-10
                && axes[0].dot(axes[2]).abs() < 1e-10
                && axes[1].dot(axes[2]).abs() < 1e-10
                && axes[0].cross(axes[1]).distance(axes[2]) < 1e-10
        };
        if !epoch.is_finite() || !proper(body_to_ecef) || !proper(icrf_to_ecef) {
            return Err(AstroError::InvalidObserver);
        }
        let normal = osculating_normal(state)?;
        let radial = state
            .position_m
            .normalized()
            .ok_or(AstroError::InvalidObserver)?;
        let tangent = normal
            .cross(radial)
            .normalized()
            .ok_or(AstroError::InvalidObserver)?;
        let axis = body_to_ecef.transform(Vec3d::new(0.0, 0.0, 1.0));
        let orbital_normal_ecef = icrf_to_ecef.transform(normal);
        Ok(Self {
            emission_tdb_seconds_j2000: epoch,
            axis_ecef: axis,
            orbital_normal_ecef,
            orbital_radial_ecef: icrf_to_ecef.transform(radial),
            orbital_tangent_ecef: icrf_to_ecef.transform(tangent),
            tilt_deg: axis_tilt_degrees(axis, orbital_normal_ecef)?,
        })
    }
}
