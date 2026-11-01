//! Audited geographic download planning and source DEM interpretation.
//! Download bounds are conservative ellipsoidal-radius envelopes; imported
//! elevations retain an explicit source lattice and vertical datum.
use crate::{GeoidError, GeometryError, TerrainError};
use std::fmt;

mod region;
pub use region::*;
pub(crate) mod copernicus;
pub use copernicus::*;

/// Explicit native-QA pose: view actual retained relief near a supplied center.
/// It neither modifies source heights nor changes ordinary startup/navigation.
pub fn inspection_observer(
    scene: &crate::TerrainScene,
    center: crate::Geodetic,
) -> Result<crate::Observer, TopographyError> {
    let center = crate::Geodetic::new(center.lat_deg, center.lon_deg, 0.0)
        .map_err(TopographyError::InvalidCoordinate)?;
    let origin = crate::geodetic_to_ecef(center);
    let mut target = None;
    for patch in scene.patches() {
        for &position in patch.canonical_vertices() {
            let point =
                crate::ecef_to_geodetic(position).map_err(TopographyError::InvalidCoordinate)?;
            // Select by horizontal geography, independently of mountain height.
            // The h=0 chord is a lower bound on the surface geodesic and cheaply
            // excludes distant posts before the full inverse geodesic query.
            let projected = crate::geodetic_to_ecef(
                point
                    .with_height(0.0)
                    .map_err(TopographyError::InvalidCoordinate)?,
            );
            if projected.distance(origin) > 2000.0
                || crate::geodesic::wgs84_surface_distance(center, point)
                    .map_err(TopographyError::InvalidCoordinate)?
                    > 2000.0
            {
                continue;
            }
            if target.is_none_or(|old: crate::Geodetic| point.height_m > old.height_m) {
                target = Some(point);
            }
        }
    }
    let target = target.ok_or(TopographyError::MissingData)?;
    let basis = crate::EnuBasis::at(target);
    let offset = crate::ecef_to_geodetic(
        crate::geodetic_to_ecef(target) - basis.north * 220.0 + basis.east * 80.0,
    )
    .map_err(TopographyError::InvalidCoordinate)?;
    let ground = scene
        .imported_ground(offset)
        .ok_or(TopographyError::MissingData)?;
    let feet = ground
        .hit
        .geodetic
        .with_height(ground.hit.geodetic.height_m + 70.0)
        .map_err(TopographyError::InvalidCoordinate)?;
    let mut observer = crate::Observer::new(feet);
    scene.try_teleport(&mut observer, feet)?;
    observer
        .aim_at(crate::geodetic_to_ecef(target))
        .map_err(TopographyError::InvalidCoordinate)?;
    Ok(observer)
}

#[derive(Clone, Debug, PartialEq)]
pub enum TopographyError {
    InvalidCoordinate(GeometryError),
    InvalidRadius,
    InvalidBounds,
    TooManyTiles,
    InvalidRaster(String),
    MissingData,
    Surface(TerrainError),
    Geoid(GeoidError),
    Cancelled,
}
impl fmt::Display for TopographyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCoordinate(error) => write!(f, "Invalid download coordinate: {error}"),
            Self::InvalidRadius => {
                f.write_str("Radius must be positive and no greater than 100 miles (160.9344 km).")
            }
            Self::InvalidBounds => f.write_str(
                "Geographic bounds must be finite, ordered and within latitude/longitude limits.",
            ),
            Self::TooManyTiles => f.write_str(
                "This area exceeds the interactive source-tile budget; reduce the radius.",
            ),
            Self::InvalidRaster(message) => {
                write!(f, "Unsupported or invalid source DEM: {message}")
            }
            Self::MissingData => {
                f.write_str("No complete source elevation samples are available for this area.")
            }
            Self::Surface(error) => write!(f, "Terrain construction failed: {error}"),
            Self::Geoid(error) => write!(f, "Terrain datum conversion failed: {error}"),
            Self::Cancelled => f.write_str("Terrain import cancelled."),
        }
    }
}
impl std::error::Error for TopographyError {}
impl From<TerrainError> for TopographyError {
    fn from(error: TerrainError) -> Self {
        Self::Surface(error)
    }
}
impl From<GeoidError> for TopographyError {
    fn from(error: GeoidError) -> Self {
        Self::Geoid(error)
    }
}
