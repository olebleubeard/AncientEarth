//! Geodesic marker rows, geometric picking, tape chords and centerline hiding.
//! These are model measurements, not surveyed terrain or optical refraction.
use crate::*;
use geographiclib_rs::{DirectGeodesic, Geodesic};
use std::sync::atomic::{AtomicBool, Ordering};
mod annotation;
pub use annotation::{
    TapeAnnotation, project_chord_segments, tape_annotation, tape_endpoint_position,
};

pub const MARKER_WIDTH_M: f64 = 10.0 * METRES_PER_FOOT;
pub const MARKER_DEPTH_M: f64 = 10.0 * METRES_PER_FOOT;
pub const MARKER_HEIGHT_M: f64 = 100.0 * METRES_PER_FOOT;
pub const MARKER_BAND_M: f64 = METRES_PER_FOOT;
pub const MAX_MARKERS: usize = 512;
const MAX_REACH_M: f64 = 10_000_000.0;
const HEIGHT_BRACKET_M: f64 = 1e-5;
const TERRAIN_INTERVALS: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum SurveyError {
    InvalidSettings,
    InvalidGeometry,
    Cancelled,
    Coordinate(GeometryError),
    Surface(TerrainError),
}
impl std::fmt::Display for SurveyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSettings=>f.write_str("Use finite spacing of at least 10 ft, reach from spacing through 10,000 km, left offset 0–1,000 km, and at most 512 markers."),
            Self::InvalidGeometry=>f.write_str("Invalid survey position, direction or prism geometry."),
            Self::Cancelled=>f.write_str("Survey calculation cancelled."),
            Self::Coordinate(error)=>write!(f,"Survey coordinate: {error}"),
            Self::Surface(error)=>write!(f,"Survey surface: {error}"),
        }
    }
}
impl std::error::Error for SurveyError {}
impl From<GeometryError> for SurveyError {
    fn from(error: GeometryError) -> Self {
        Self::Coordinate(error)
    }
}
impl From<TerrainError> for SurveyError {
    fn from(error: TerrainError) -> Self {
        Self::Surface(error)
    }
}
fn check_cancel(cancelled: &AtomicBool) -> Result<(), SurveyError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(SurveyError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerSettings {
    pub spacing_m: f64,
    pub reach_m: f64,
    pub left_offset_m: f64,
}
impl Default for MarkerSettings {
    fn default() -> Self {
        Self {
            spacing_m: 0.5 * METRES_PER_MILE,
            reach_m: 40.0 * METRES_PER_MILE,
            left_offset_m: 100.0 * METRES_PER_FOOT,
        }
    }
}
impl MarkerSettings {
    pub fn validate(self) -> Result<usize, SurveyError> {
        if ![self.spacing_m, self.reach_m, self.left_offset_m]
            .into_iter()
            .all(f64::is_finite)
            || self.spacing_m < MARKER_DEPTH_M
            || self.reach_m < self.spacing_m
            || self.reach_m > MAX_REACH_M
            || !(0.0..=1_000_000.0).contains(&self.left_offset_m)
        {
            return Err(SurveyError::InvalidSettings);
        }
        let count = (self.reach_m / self.spacing_m).floor() as usize + 1;
        if count > MAX_MARKERS {
            return Err(SurveyError::InvalidSettings);
        }
        Ok(count)
    }
}

/// Convert a positive row spacing/reach from displayed kilometres or miles.
/// Download-radius limits do not apply to survey rows; MarkerSettings::validate
/// separately enforces the existing row length, spacing and marker-count limits.
pub fn row_distance_from_display(value: f64, units: UnitSystem) -> Result<f64, SurveyError> {
    let metres = value
        * match units {
            UnitSystem::Metric => 1000.0,
            UnitSystem::Imperial => METRES_PER_MILE,
        };
    if !metres.is_finite() || metres <= 0.0 {
        return Err(SurveyError::InvalidSettings);
    }
    Ok(metres)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurveyMarker {
    /// Zero-based stable row index; station zero is the initial left offset.
    pub id: usize,
    pub base_ecef_m: Vec3d,
    pub right: Vec3d,
    pub forward: Vec3d,
    pub up: Vec3d,
    pub width_m: f64,
    pub depth_m: f64,
    pub height_m: f64,
    pub along_distance_m: f64,
}
impl SurveyMarker {
    pub fn centerline(self, height_m: f64) -> Vec3d {
        self.base_ecef_m + self.up * height_m
    }
    /// Bottom [-right-forward,+right-forward,+right+forward,-right+forward],
    /// then corresponding top vertices. right×forward=up; dimensions are exact
    /// Euclidean lengths in this local orthonormal frame.
    pub fn corners(self) -> [Vec3d; 8] {
        let r = self.right * (self.width_m * 0.5);
        let f = self.forward * (self.depth_m * 0.5);
        let u = self.up * self.height_m;
        let b = self.base_ecef_m;
        [
            b - r - f,
            b + r - f,
            b + r + f,
            b - r + f,
            b - r - f + u,
            b + r - f + u,
            b + r + f + u,
            b - r + f + u,
        ]
    }
    pub(crate) fn valid(self) -> bool {
        self.base_ecef_m.is_finite()
            && [self.right, self.forward, self.up]
                .into_iter()
                .all(|v| v.is_finite() && (v.length() - 1.0).abs() < 1e-10)
            && self.right.dot(self.forward).abs() < 1e-10
            && self.right.dot(self.up).abs() < 1e-10
            && self.forward.dot(self.up).abs() < 1e-10
            && self.right.cross(self.forward).dot(self.up) > 1.0 - 1e-10
            && [self.width_m, self.depth_m, self.height_m]
                .into_iter()
                .all(|v| v.is_finite() && v > 0.0 && v <= 1_000_000.0)
    }
    /// Nearest forward prism boundary, with true eye-origin metres even for a
    /// nonunit input ray. An inside observer receives the positive exit face.
    pub fn intersect(self, ray: Ray) -> Option<f64> {
        if !self.valid() || !ray.origin.is_finite() {
            return None;
        }
        let direction = ray.direction.normalized()?;
        let relative = ray.origin - self.base_ecef_m;
        let mut near = f64::NEG_INFINITY;
        let mut far = f64::INFINITY;
        for (axis, lo, hi) in [
            (self.right, -self.width_m * 0.5, self.width_m * 0.5),
            (self.forward, -self.depth_m * 0.5, self.depth_m * 0.5),
            (self.up, 0.0, self.height_m),
        ] {
            let o = axis.dot(relative);
            let d = axis.dot(direction);
            if d == 0.0 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let a = (lo - o) / d;
                let b = (hi - o) / d;
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        let distance = if near >= 0.0 { near } else { far };
        (distance.is_finite() && distance >= 0.0).then_some(distance)
    }
}

pub fn place_markers(
    scene: &TerrainScene,
    observer: Observer,
    settings: MarkerSettings,
    cancelled: &AtomicBool,
) -> Result<Vec<SurveyMarker>, SurveyError> {
    let count = settings.validate()?;
    let location = Geodetic::new(observer.feet.lat_deg, observer.feet.lon_deg, 0.0)?;
    if !observer.heading_deg.is_finite() {
        return Err(SurveyError::InvalidGeometry);
    }
    let heading = observer.heading_deg.rem_euclid(360.0);
    let geodesic = Geodesic::new(WGS84_A_M, WGS84_F);
    // One initial leftward offset, then a single geodesic row. This is not a
    // constant-distance parallel curve around the observer's original meridian.
    let (latitude, longitude): (f64, f64) = geodesic.direct(
        location.lat_deg,
        location.lon_deg,
        heading - 90.0,
        settings.left_offset_m,
    );
    let mut markers = Vec::with_capacity(count);
    for id in 0..count {
        check_cancel(cancelled)?;
        let distance = id as f64 * settings.spacing_m;
        let (latitude, longitude, azimuth): (f64, f64, f64) =
            geodesic.direct(latitude, longitude, heading, distance);
        let ground = scene.ground(Geodetic::new(latitude, longitude, 0.0)?)?;
        let basis = EnuBasis::at(ground.hit.geodetic);
        let (sin, cos) = azimuth.to_radians().sin_cos();
        let marker = SurveyMarker {
            id,
            base_ecef_m: ground.hit.position,
            right: basis.east * cos - basis.north * sin,
            forward: basis.north * cos + basis.east * sin,
            up: basis.up,
            width_m: MARKER_WIDTH_M,
            depth_m: MARKER_DEPTH_M,
            height_m: MARKER_HEIGHT_M,
            along_distance_m: distance,
        };
        if !marker.valid() {
            return Err(SurveyError::InvalidGeometry);
        }
        markers.push(marker);
    }
    check_cancel(cancelled)?;
    Ok(markers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurveyTarget {
    Surface,
    Marker(usize),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapePoint {
    pub position_ecef_m: Vec3d,
    pub target: SurveyTarget,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurveyPick {
    pub point: TapePoint,
    pub distance_m: f64,
}
fn nearest_marker(markers: &[SurveyMarker], ray: Ray) -> Option<SurveyPick> {
    let direction = ray.direction.normalized()?;
    markers
        .iter()
        .filter_map(|marker| {
            marker.intersect(ray).map(|distance_m| SurveyPick {
                point: TapePoint {
                    position_ecef_m: ray.origin + direction * distance_m,
                    target: SurveyTarget::Marker(marker.id),
                },
                distance_m,
            })
        })
        .min_by(|a, b| a.distance_m.total_cmp(&b.distance_m))
}
/// Cheap marker-first hover: the Earth BVH is queried only if a box is hit.
pub fn pick_marker(scene: &TerrainScene, markers: &[SurveyMarker], ray: Ray) -> Option<SurveyPick> {
    let marker = nearest_marker(markers, ray)?;
    if scene
        .intersect(ray)
        .is_some_and(|earth| earth.hit.distance_m + 1e-6 < marker.distance_m)
    {
        None
    } else {
        Some(marker)
    }
}
/// Tape targets are the nearest active Earth surface or visible marker. Grass,
/// stars and celestial spheres are intentionally not tape endpoints.
pub fn pick(scene: &TerrainScene, markers: &[SurveyMarker], ray: Ray) -> Option<SurveyPick> {
    let marker = nearest_marker(markers, ray);
    let earth = scene.intersect(ray).map(|hit| SurveyPick {
        point: TapePoint {
            position_ecef_m: hit.hit.position,
            target: SurveyTarget::Surface,
        },
        distance_m: hit.hit.distance_m,
    });
    match (marker, earth) {
        (Some(marker), Some(earth)) => Some(if earth.distance_m + 1e-6 < marker.distance_m {
            earth
        } else {
            marker
        }),
        (marker, earth) => marker.or(earth),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TapeMeasurement {
    pub straight_distance_m: f64,
    pub surface_distance_m: f64,
}
impl TapeMeasurement {
    pub fn between(a: TapePoint, b: TapePoint) -> Result<Self, SurveyError> {
        let from = ecef_to_geodetic(a.position_ecef_m)?;
        let to = ecef_to_geodetic(b.position_ecef_m)?;
        Ok(Self {
            straight_distance_m: a.position_ecef_m.distance(b.position_ecef_m),
            surface_distance_m: wgs84_surface_distance(from, to)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcclusionMethod {
    SmoothEllipsoidBisection,
    SampledTerrain,
    SampledRefractedBranch,
}
#[derive(Clone, Debug)]
pub struct MarkerOcclusion {
    pub hidden_from_base_m: f64,
    pub sampled_total_hidden_m: f64,
    pub sampling_step_m: f64,
    pub boundary_tolerance_m: f64,
    pub method: OcclusionMethod,
    pub hidden_intervals_m: Vec<[f64; 2]>,
}
impl MarkerOcclusion {
    pub fn summary(&self, units: UnitSystem) -> String {
        match self.method {
            OcclusionMethod::SampledRefractedBranch => format!(
                "Refracted sampled image: about {} hidden from the base; sampled total {}. Centerline spacing {}; one numerical image branch, narrow intervals/other mirage images may be missed.",
                units.format_height(self.hidden_from_base_m),
                units.format_height(self.sampled_total_hidden_m),
                units.format_height(self.sampling_step_m)
            ),
            OcclusionMethod::SmoothEllipsoidBisection => format!(
                "Earth hides {} from the marker base (vertical centerline).",
                units.format_height(self.hidden_from_base_m)
            ),
            OcclusionMethod::SampledTerrain => format!(
                "Earth/terrain hides about {} from the base; sampled total {} along the vertical centerline. Height sample spacing {}; narrow intervals can be missed.",
                units.format_height(self.hidden_from_base_m),
                units.format_height(self.sampled_total_hidden_m),
                units.format_height(self.sampling_step_m)
            ),
        }
    }
}
pub fn marker_occlusion(
    scene: &TerrainScene,
    eye: Vec3d,
    marker: &SurveyMarker,
    cancelled: &AtomicBool,
) -> Result<MarkerOcclusion, SurveyError> {
    if !marker.valid() {
        return Err(SurveyError::InvalidGeometry);
    }
    let eye_geodetic = ecef_to_geodetic(eye)?;
    let hidden = |height: f64| -> Result<bool, SurveyError> {
        check_cancel(cancelled)?;
        let target = marker.centerline(height);
        let delta = target - eye;
        let length = delta.length();
        if !length.is_finite() || length <= 1e-8 {
            return Err(SurveyError::InvalidGeometry);
        }
        let endpoint_guard = 1e-5 + 128.0 * f64::EPSILON * length.max(WGS84_A_M);
        Ok(scene
            .intersect(Ray {
                origin: eye,
                direction: delta,
            })
            .is_some_and(|hit| hit.hit.distance_m < length - endpoint_guard))
    };
    let refine = |mut lo: f64, mut hi: f64, lower_hidden: bool| -> Result<f64, SurveyError> {
        for _ in 0..64 {
            if hi - lo <= HEIGHT_BRACKET_M {
                break;
            }
            let midpoint = (lo + hi) * 0.5;
            if hidden(midpoint)? == lower_hidden {
                lo = midpoint;
            } else {
                hi = midpoint;
            }
        }
        Ok((lo + hi) * 0.5)
    };
    let bottom = hidden(0.0)?;
    let smooth = scene.is_boundary_only() || scene.patches().is_empty();
    if smooth && eye_geodetic.height_m < -1e-7 {
        return Err(SurveyError::InvalidGeometry);
    }
    let mut intervals = Vec::new();
    if smooth {
        // For a normal-line prism on the convex ellipsoid, the exterior shadow
        // interval is a prefix. Require that actual alignment before using it.
        let base = ecef_to_geodetic(marker.base_ecef_m)?;
        if marker.up.dot(EnuBasis::at(base).up) < 1.0 - 1e-10 {
            return Err(SurveyError::InvalidGeometry);
        }
        if bottom {
            let end = if hidden(marker.height_m)? {
                marker.height_m
            } else {
                refine(0.0, marker.height_m, true)?
            };
            intervals.push([0.0, end]);
        }
    } else {
        let step = marker.height_m / TERRAIN_INTERVALS as f64;
        let mut previous = bottom;
        let mut start = 0.0;
        for index in 1..=TERRAIN_INTERVALS {
            let height = index as f64 * step;
            let current = hidden(height)?;
            if current != previous {
                let crossing = refine((index - 1) as f64 * step, height, previous)?;
                if previous {
                    intervals.push([start, crossing]);
                } else {
                    start = crossing;
                }
            }
            previous = current;
        }
        if previous {
            intervals.push([start, marker.height_m]);
        }
    }
    check_cancel(cancelled)?;
    let prefix = intervals
        .first()
        .filter(|interval| interval[0] == 0.0)
        .map_or(0.0, |interval| interval[1]);
    Ok(MarkerOcclusion {
        hidden_from_base_m: prefix,
        sampled_total_hidden_m: intervals.iter().map(|range| range[1] - range[0]).sum(),
        sampling_step_m: if smooth {
            0.0
        } else {
            marker.height_m / TERRAIN_INTERVALS as f64
        },
        boundary_tolerance_m: HEIGHT_BRACKET_M,
        method: if smooth {
            OcclusionMethod::SmoothEllipsoidBisection
        } else {
            OcclusionMethod::SampledTerrain
        },
        hidden_intervals_m: intervals,
    })
}

/// Deterministic native verification poses, built from the actual active model.
/// This does not introduce a second set of geographic marker coordinates.
#[derive(Clone, Debug)]
pub struct SurveyDemo {
    pub observer: Observer,
    pub lens: Lens,
    pub markers: Vec<SurveyMarker>,
    pub tape_a: TapePoint,
    pub tape_b: TapePoint,
    pub marker_id: usize,
}
/// Inspect station zero from the normal startup feet without moving the
/// observer sideways or forward. Ground placement uses the active surface.
pub fn nearby_demo(scene: &TerrainScene) -> Result<SurveyDemo, SurveyError> {
    let mut observer = crate::boundary::default_observer();
    let location = observer.feet;
    scene.place_on_ground(&mut observer, location)?;
    let markers = place_markers(
        scene,
        observer,
        MarkerSettings::default(),
        &AtomicBool::new(false),
    )?;
    let marker = *markers.first().ok_or(SurveyError::InvalidGeometry)?;
    observer.aim_at(marker.centerline(marker.height_m * 0.5))?;
    let corners = marker.corners();
    Ok(SurveyDemo {
        observer,
        lens: Lens::default(),
        markers,
        tape_a: TapePoint {
            position_ecef_m: corners[1],
            target: SurveyTarget::Marker(marker.id),
        },
        tape_b: TapePoint {
            position_ecef_m: corners[5],
            target: SurveyTarget::Marker(marker.id),
        },
        marker_id: marker.id,
    })
}
pub fn demo(scene: &TerrainScene, close: bool) -> Result<SurveyDemo, SurveyError> {
    let mut observer = crate::boundary::default_observer();
    let location = observer.feet;
    scene.place_on_ground(&mut observer, location)?;
    let cancelled = AtomicBool::new(false);
    let markers = place_markers(scene, observer, MarkerSettings::default(), &cancelled)?;
    let (selected, lens) = if close {
        let marker = markers.first().ok_or(SurveyError::InvalidGeometry)?;
        let near =
            ecef_to_geodetic(marker.base_ecef_m - marker.forward * 60.0 + marker.right * 20.0)?;
        scene.place_on_ground(&mut observer, near)?;
        observer.aim_at(marker.centerline(marker.height_m * 0.5))?;
        (marker.id, Lens::new(Projection::Rectilinear, 45.0))
    } else {
        let mut best: Option<(f64, usize, Observer)> = None;
        for marker in &markers {
            let occlusion = marker_occlusion(scene, observer.eye_position(), marker, &cancelled)?;
            if !(marker.height_m * 0.15..marker.height_m * 0.85)
                .contains(&occlusion.hidden_from_base_m)
            {
                continue;
            }
            let height = (occlusion.hidden_from_base_m + marker.height_m) * 0.5;
            let mut candidate = observer;
            // The row's nearer prisms can obscure a farther prism's center.
            // Its outer right side supplies a real visible fragment; verify it
            // through the same pick used by the application before selecting.
            candidate.aim_at(marker.centerline(height) + marker.right * (marker.width_m * 0.45))?;
            let ray = Ray {
                origin: candidate.eye_position(),
                direction: candidate.frame().forward,
            };
            if pick_marker(scene, &markers, ray)
                .is_none_or(|pick| pick.point.target != SurveyTarget::Marker(marker.id))
            {
                continue;
            }
            let error = (occlusion.hidden_from_base_m - marker.height_m * 0.5).abs();
            if best.is_none_or(|(previous, _, _)| error < previous) {
                best = Some((error, marker.id, candidate));
            }
        }
        let (_, id, pose) = best.ok_or(SurveyError::InvalidGeometry)?;
        observer = pose;
        (id, Lens::new(Projection::Rectilinear, 0.2))
    };
    let marker = markers
        .iter()
        .find(|marker| marker.id == selected)
        .ok_or(SurveyError::InvalidGeometry)?;
    let corners = marker.corners();
    let tape_a = TapePoint {
        position_ecef_m: corners[1],
        target: SurveyTarget::Marker(selected),
    };
    let tape_b = TapePoint {
        position_ecef_m: corners[5],
        target: SurveyTarget::Marker(selected),
    };
    Ok(SurveyDemo {
        observer,
        lens,
        markers,
        tape_a,
        tape_b,
        marker_id: selected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demonstration_poses_really_hover_the_selected_box_and_measure_its_edge() {
        let scene = TerrainScene::boundary_only(vec![]).unwrap();
        for close in [false, true] {
            let view = demo(&scene, close).unwrap();
            let ray = Ray {
                origin: view.observer.eye_position(),
                direction: view.observer.frame().forward,
            };
            assert_eq!(
                pick_marker(&scene, &view.markers, ray)
                    .unwrap()
                    .point
                    .target,
                SurveyTarget::Marker(view.marker_id)
            );
            let length = TapeMeasurement::between(view.tape_a, view.tape_b).unwrap();
            assert!((length.straight_distance_m - MARKER_HEIGHT_M).abs() < 2e-9);
            if !close {
                let occlusion = marker_occlusion(
                    &scene,
                    ray.origin,
                    &view.markers[view.marker_id],
                    &AtomicBool::new(false),
                )
                .unwrap();
                assert!(
                    occlusion.hidden_from_base_m > MARKER_HEIGHT_M * 0.25
                        && occlusion.hidden_from_base_m < MARKER_HEIGHT_M * 0.75
                );
                eprintln!(
                    "Survey horizon fixture: marker{} at{}m, hidden{}m, observer{:?}, lens{:?}",
                    view.marker_id,
                    view.markers[view.marker_id].along_distance_m,
                    occlusion.hidden_from_base_m,
                    view.observer,
                    view.lens
                );
            }
        }
    }
}
