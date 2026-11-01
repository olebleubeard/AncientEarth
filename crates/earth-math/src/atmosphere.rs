//! Weather-informed, horizontally stratified optical rays through the scene.
//! Weather profiles are explicitly transplanted to one fixed surface anchor, not claimed
//! to be the measured atmosphere at every ray point. See science/refraction.md.
use crate::*;
mod refractivity;
pub use refractivity::phase_index;
pub mod gpu;
mod inverse;
pub mod projection;
mod scene;
pub mod weather;

pub const MAX_LAYERS: usize = 128;
pub const MAX_TRACE_STEPS: u32 = 4096;
pub const MAX_STEP_M: f64 = 2000.0;
pub const POSITION_TOLERANCE_M: f64 = 0.001;
pub const MAX_ATMOSPHERIC_PATH_M: f64 = 2_000_000.0;
pub const KNOT_HEIGHT_BAND_M: f64 = 0.00001;

#[derive(Clone, Debug, PartialEq)]
pub enum AtmosphereError {
    InvalidProfile,
    InvalidSample,
    UnsupportedHeight,
    NoConvergence,
    InvalidWeather(String),
}
impl fmt::Display for AtmosphereError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProfile => f.write_str("Invalid or incomplete atmospheric profile."),
            Self::InvalidSample => {
                f.write_str("Atmospheric sample is outside the supported numerical domain.")
            }
            Self::UnsupportedHeight => f.write_str(
                "Optical observer is below the modeled surface or outside supported coordinates.",
            ),
            Self::NoConvergence => {
                f.write_str("Curved ray did not resolve within its numerical/path budget.")
            }
            Self::InvalidWeather(message) => write!(f, "Weather data rejected: {message}"),
        }
    }
}
impl std::error::Error for AtmosphereError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereLayer {
    pub height_m: f64,
    pub refractivity: f64,
}
#[derive(Clone, Debug)]
pub struct AtmosphereProfile {
    layers: Vec<AtmosphereLayer>,
    maximum_gradient: f64,
    base_height_m: f64,
    anchor: Option<AtmosphereAnchor>,
    /// Thermodynamic knots before the synthetic vacuum endpoint. Retained for
    /// editing; refractivity alone cannot recover temperature/pressure/humidity.
    weather_samples: Option<Vec<weather::WeatherSample>>,
    pub metadata: Option<weather::WeatherMetadata>,
}
/// A fixed scene binding, distinct from the weather provider's grid point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereAnchor {
    pub location: Geodetic,
}
impl AtmosphereAnchor {
    pub fn label(self, units: UnitSystem) -> String {
        format!(
            "Fixed column anchor: {:.6}°, {:.6}°; surface {} WGS84",
            self.location.lat_deg,
            self.location.lon_deg,
            units.format_height(self.location.height_m)
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct OpticalTrace {
    pub origin: Vec3d,
    pub initial_direction: Vec3d,
    /// Straight vacuum segment before atmospheric entry from space.
    pub initial_vacuum_distance_m: f64,
    pub surface: Option<SurfaceHit>,
    pub marker: Option<OpticalMarkerHit>,
    /// Vacuum continuation at atmospheric exit, or the tangent at a ground hit.
    pub exit_ray: Ray,
    pub geometric_path_m: f64,
    pub optical_path_m: f64,
    pub steps: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct OpticalMarkerHit {
    pub marker_id: usize,
    pub position_ecef_m: Vec3d,
    pub normal: Vec3d,
}
#[derive(Clone, Copy, Debug)]
pub struct OpticalCelestialHit {
    pub hit: crate::astro::CelestialHit,
    pub geometric_path_m: f64,
    pub optical_path_m: f64,
    pub chord_m: f64,
}
impl OpticalTrace {
    /// The endpoint is physical ECEF; distance is geometric curved path length,
    /// while A-to-B tape measurements remain explicitly Euclidean chords.
    pub fn survey_pick(&self) -> Option<crate::survey::SurveyPick> {
        use crate::survey::{SurveyPick, SurveyTarget, TapePoint};
        let point = if let Some(marker) = self.marker {
            TapePoint {
                position_ecef_m: marker.position_ecef_m,
                target: SurveyTarget::Marker(marker.marker_id),
            }
        } else {
            TapePoint {
                position_ecef_m: self.surface?.hit.position,
                target: SurveyTarget::Surface,
            }
        };
        Some(SurveyPick {
            point,
            distance_m: self.geometric_path_m,
        })
    }
    pub fn marker_pick(&self) -> Option<crate::survey::SurveyPick> {
        self.marker.and_then(|_| self.survey_pick())
    }
    /// Continue the emerging ray through vacuum to the same apparent spherical
    /// bodies used by the renderer. The initial vacuum segment is also tested:
    /// a spacecraft beyond the Moon can see it before the ray reaches Earth.
    pub fn nearest_celestial(
        &self,
        frame: &crate::astro::AstronomyFrame,
    ) -> Option<OpticalCelestialHit> {
        if self.initial_vacuum_distance_m > 0.0
            && let Some(hit) = frame.nearest_celestial(
                Ray {
                    origin: self.origin,
                    direction: self.initial_direction,
                },
                Some(self.initial_vacuum_distance_m),
            )
        {
            return Some(OpticalCelestialHit {
                geometric_path_m: hit.distance_m,
                optical_path_m: hit.distance_m,
                chord_m: hit.distance_m,
                hit,
            });
        }
        if self.surface.is_some() || self.marker.is_some() {
            return None;
        }
        let hit = frame.nearest_celestial(self.exit_ray, None)?;
        Some(OpticalCelestialHit {
            geometric_path_m: self.geometric_path_m + hit.distance_m,
            optical_path_m: self.optical_path_m + hit.distance_m,
            chord_m: self.origin.distance(hit.position_ecef_m),
            hit,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct OpticalSkyline {
    pub hit: SurfaceHit,
    pub geometric_path_m: f64,
    pub optical_path_m: f64,
    pub heading_deg: f64,
    pub elevation_deg: f64,
    pub angular_search_step_deg: f64,
    pub angular_bracket_deg: f64,
}
impl OpticalSkyline {
    pub fn description(&self) -> &'static str {
        "Refracted numerical boundary estimate"
    }
}
#[derive(Clone, Copy)]
struct State {
    p: Vec3d,
    u: Vec3d,
    optical: f64,
}

impl AtmosphereProfile {
    /// Highest hit found in this local vertical heading plane, followed by a
    /// local hit/miss refinement. The 5-degree exploratory sampling can miss
    /// a separate narrow duct/image branch; the bracket is not a global error
    /// guarantee. An unresolved ray fails the measurement instead of silently
    /// substituting a straight geometric horizon.
    pub fn skyline(&self, scene: &TerrainScene, observer: Observer) -> Option<OpticalSkyline> {
        self.skyline_cancellable(scene, observer, &std::sync::atomic::AtomicBool::new(false))
    }
    /// Cancellation is checked between the same bounded optical ray queries.
    pub fn skyline_cancellable(
        &self,
        scene: &TerrainScene,
        observer: Observer,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Option<OpticalSkyline> {
        let eye = observer.eye_geodetic();
        let horizontal = CameraFrame::new(eye, observer.heading_deg, 0.0);
        let up = EnuBasis::at(eye).up;
        let sample = |degrees: f64| {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(AtmosphereError::NoConvergence);
            }
            self.trace(
                Ray {
                    origin: horizontal.origin,
                    direction: horizontal.forward * degrees.to_radians().cos()
                        + up * degrees.to_radians().sin(),
                },
                scene,
            )
        };
        let mut upper = 90.0;
        if sample(upper).ok()?.surface.is_some() {
            return None;
        }
        let mut found = None;
        for index in 1..=36 {
            let lower = 90.0 - f64::from(index) * 5.0;
            let trace = sample(lower).ok()?;
            if trace.surface.is_some() {
                found = Some((lower, trace));
                break;
            }
            upper = lower;
        }
        let (mut lower, mut best) = found?;
        for _ in 0..22 {
            let middle = (lower + upper) * 0.5;
            let trace = sample(middle).ok()?;
            if trace.surface.is_some() {
                lower = middle;
                best = trace;
            } else {
                upper = middle;
            }
        }
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        Some(OpticalSkyline {
            hit: best.surface?,
            geometric_path_m: best.geometric_path_m,
            optical_path_m: best.optical_path_m,
            heading_deg: observer.heading_deg,
            elevation_deg: lower,
            angular_search_step_deg: 5.0,
            angular_bracket_deg: upper - lower,
        })
    }
    /// Numeric profile constructor used by validated ingestion and explicit
    /// scientific scenarios. The last knot must join vacuum continuously.
    pub fn from_layers(layers: Vec<AtmosphereLayer>) -> Result<Self, AtmosphereError> {
        if layers.len() < 2
            || layers.len() > MAX_LAYERS
            || layers[0].height_m != 0.0
            || layers
                .last()
                .is_none_or(|l| l.refractivity != 0.0 || l.height_m > 200_000.0)
            || layers.iter().any(|l| {
                !l.height_m.is_finite()
                    || !l.refractivity.is_finite()
                    || l.height_m < 0.0
                    || !(0.0..=0.002).contains(&l.refractivity)
            })
            || layers
                .windows(2)
                .any(|p| p[1].height_m - p[0].height_m < 0.01)
        {
            return Err(AtmosphereError::InvalidProfile);
        }
        let maximum_gradient = layers
            .windows(2)
            .map(|p| {
                ((p[1].refractivity - p[0].refractivity) / (p[1].height_m - p[0].height_m)).abs()
            })
            .fold(0.0_f64, f64::max);
        if maximum_gradient > 0.001 {
            return Err(AtmosphereError::InvalidProfile);
        }
        Ok(Self {
            layers,
            maximum_gradient,
            base_height_m: 0.0,
            anchor: None,
            weather_samples: None,
            metadata: None,
        })
    }
    pub fn layers(&self) -> &[AtmosphereLayer] {
        &self.layers
    }
    pub fn weather_samples(&self) -> Option<&[weather::WeatherSample]> {
        self.weather_samples.as_deref()
    }
    pub fn top_height_m(&self) -> f64 {
        self.base_height_m + self.layers.last().unwrap().height_m
    }
    pub fn base_height_m(&self) -> f64 {
        self.base_height_m
    }
    pub fn anchor(&self) -> Option<AtmosphereAnchor> {
        self.anchor
    }
    /// One fixed column transplant for this scene, never a terrain-following
    /// field. Relative AGL knots keep their source spacing. A synthetic profile
    /// without weather metadata retains its explicitly chosen base.
    pub fn anchored_to_scene(&self, scene: &TerrainScene) -> Result<Self, AtmosphereError> {
        if let Some(metadata) = &self.metadata {
            let location = Geodetic::new(metadata.latitude_deg, metadata.longitude_deg, 0.0)
                .map_err(|_| AtmosphereError::InvalidSample)?;
            return self.anchored_to_scene_at(scene, location);
        }
        Ok(self.clone())
    }
    /// Bind the forecast column once at the requested observer's horizontal
    /// position. Subsequent ray and observer motion must reuse this profile;
    /// rebinding occurs only when the raw column or canonical scene changes.
    pub fn anchored_to_scene_at(
        &self,
        scene: &TerrainScene,
        location: Geodetic,
    ) -> Result<Self, AtmosphereError> {
        let mut result = self.clone();
        if self.metadata.is_some() {
            let ground = scene
                .ground(location)
                .map_err(|_| AtmosphereError::InvalidSample)?;
            result.base_height_m = ground.hit.geodetic.height_m;
            result.anchor = Some(AtmosphereAnchor {
                location: Geodetic::new(location.lat_deg, location.lon_deg, result.base_height_m)
                    .map_err(|_| AtmosphereError::InvalidSample)?,
            });
        }
        Ok(result)
    }
    pub fn index_gradient(&self, height: f64) -> (f64, f64) {
        if !height.is_finite() {
            return (f64::NAN, f64::NAN);
        }
        if height >= self.top_height_m() {
            return (1.0, 0.0);
        }
        let height = height - self.base_height_m;
        if height < 0.0 {
            return (1.0 + self.layers[0].refractivity, 0.0);
        }
        let i = self
            .layers
            .partition_point(|l| l.height_m <= height)
            .saturating_sub(1)
            .min(self.layers.len() - 2);
        let (a, b) = (self.layers[i], self.layers[i + 1]);
        let gradient = (b.refractivity - a.refractivity) / (b.height_m - a.height_m);
        (
            1.0 + a.refractivity + gradient * (height - a.height_m),
            gradient,
        )
    }
    pub fn maximum_index_gradient(&self) -> f64 {
        self.maximum_gradient
    }
    /// Conservative arc-length bound before another refractivity knot can be
    /// crossed. Checking both signs retains descending/ascending turning rays.
    /// The small numerical band avoids repeatedly approaching the same knot.
    fn layer_step(&self, height: f64, vertical: f64, curvature: f64) -> f64 {
        let height = height - self.base_height_m;
        let bound = |gap: f64, velocity: f64| {
            let root = (velocity * velocity + 2.0 * curvature * gap).sqrt();
            if velocity >= 0.0 {
                2.0 * gap / (root + velocity)
            } else {
                (root - velocity) / curvature
            }
        };
        let mut result = MAX_STEP_M;
        if let Some(upper) = self
            .layers
            .iter()
            .find(|k| k.height_m - height > KNOT_HEIGHT_BAND_M)
        {
            result = result.min(bound(upper.height_m - height, vertical));
        }
        if let Some(lower) = self
            .layers
            .iter()
            .rev()
            .find(|k| height - k.height_m > KNOT_HEIGHT_BAND_M)
        {
            result = result.min(bound(height - lower.height_m, -vertical));
        }
        result.max(0.01)
    }
    fn geometry(p: Vec3d) -> Result<(f64, Vec3d), AtmosphereError> {
        let g = inverse_near_surface(p).map_err(|_| AtmosphereError::UnsupportedHeight)?;
        Ok((g.height_m, EnuBasis::at(g).up))
    }
    fn derivative(&self, state: State) -> Result<(Vec3d, Vec3d, f64), AtmosphereError> {
        let (height, up) = Self::geometry(state.p)?;
        let (n, slope) = self.index_gradient(height);
        Ok((state.u, (up - state.u * state.u.dot(up)) * (slope / n), n))
    }
    fn midpoint(&self, state: State, step: f64) -> Result<State, AtmosphereError> {
        let (dp, du, n) = self.derivative(state)?;
        let middle = State {
            p: state.p + dp * (step * 0.5),
            u: (state.u + du * (step * 0.5))
                .normalized()
                .ok_or(AtmosphereError::NoConvergence)?,
            optical: state.optical + n * step * 0.5,
        };
        let (dp, du, n) = self.derivative(middle)?;
        Ok(State {
            p: state.p + dp * step,
            u: (state.u + du * step)
                .normalized()
                .ok_or(AtmosphereError::NoConvergence)?,
            optical: state.optical + n * step,
        })
    }
    fn advance(&self, state: State, step: f64) -> Result<State, AtmosphereError> {
        self.midpoint(self.midpoint(state, step * 0.5)?, step * 0.5)
    }
    fn plane_crossing(
        &self,
        state: State,
        end: State,
        step: f64,
        plane: Option<(Vec3d, Vec3d)>,
    ) -> Result<Option<Vec3d>, AtmosphereError> {
        let Some((point, normal)) = plane else {
            return Ok(None);
        };
        let before = normal.dot(state.p - point);
        let after = normal.dot(end.p - point);
        if before * after > 0.0 || before.abs() <= 1e-7 {
            return Ok(None);
        }
        let mut low = 0.0;
        let mut high = step;
        for _ in 0..40 {
            let mid = (low + high) * 0.5;
            let value = normal.dot(self.advance(state, mid)?.p - point);
            if value * before > 0.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        Ok(Some(self.advance(state, (low + high) * 0.5)?.p))
    }
    /// First atmospheric entry from vacuum. A radius-a+top sphere brackets the
    /// actual constant-geodetic-height surface; it never substitutes its shape.
    fn entry(&self, ray: Ray) -> Result<Option<f64>, AtmosphereError> {
        let (height, _) = Self::geometry(ray.origin)?;
        if height < MIN_HEIGHT_M {
            return Err(AtmosphereError::UnsupportedHeight);
        }
        if height <= self.top_height_m() {
            return Ok(Some(0.0));
        }
        let radius = WGS84_A_M + self.top_height_m();
        let along = ray.origin.dot(ray.direction);
        let distance = ray.origin.cross(ray.direction).length();
        if distance > radius || along >= 0.0 {
            return Ok(None);
        }
        let span = ((radius - distance) * (radius + distance)).sqrt();
        let low = (-along - span).max(0.0);
        let high = -along + span;
        // Distance to a convex ellipsoid is convex along this line. Golden
        // search finds the bracket's minimum before entry bisection.
        let mut a = low;
        let mut b = high;
        let h = |s| Self::geometry(ray.origin + ray.direction * s).map(|v| v.0);
        for _ in 0..56 {
            let x = a + (b - a) * 0.3819660112501051;
            let y = a + (b - a) * 0.6180339887498949;
            // Deep-interior points already prove atmospheric penetration.
            let hx = h(x).unwrap_or(MIN_HEIGHT_M);
            let hy = h(y).unwrap_or(MIN_HEIGHT_M);
            if hx < hy {
                b = y;
            } else {
                a = x;
            }
        }
        let mut upper = (a + b) * 0.5;
        if h(upper).unwrap_or(MIN_HEIGHT_M) > self.top_height_m() {
            return Ok(None);
        }
        let mut lower = low;
        for _ in 0..56 {
            let middle = (lower + upper) * 0.5;
            if h(middle).unwrap_or(MIN_HEIGHT_M) > self.top_height_m() {
                lower = middle;
            } else {
                upper = middle;
            }
        }
        Ok(Some(upper))
    }
    pub fn trace(&self, ray: Ray, scene: &TerrainScene) -> Result<OpticalTrace, AtmosphereError> {
        self.trace_scene(ray, scene, &[])
    }
    pub fn trace_scene(
        &self,
        ray: Ray,
        scene: &TerrainScene,
        markers: &[crate::survey::SurveyMarker],
    ) -> Result<OpticalTrace, AtmosphereError> {
        self.trace_scene_cancellable(
            ray,
            scene,
            markers,
            &std::sync::atomic::AtomicBool::new(false),
        )
    }
    pub fn trace_scene_cancellable(
        &self,
        ray: Ray,
        scene: &TerrainScene,
        markers: &[crate::survey::SurveyMarker],
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<OpticalTrace, AtmosphereError> {
        self.trace_inner(ray, scene, markers, None, cancelled)
            .map(|result| result.0)
    }
    pub fn trace_to_plane(
        &self,
        ray: Ray,
        scene: &TerrainScene,
        point: Vec3d,
        normal: Vec3d,
    ) -> Result<Option<Vec3d>, AtmosphereError> {
        self.trace_to_plane_scene(ray, scene, &[], point, normal)
    }
    pub fn trace_to_plane_scene(
        &self,
        ray: Ray,
        scene: &TerrainScene,
        markers: &[crate::survey::SurveyMarker],
        point: Vec3d,
        normal: Vec3d,
    ) -> Result<Option<Vec3d>, AtmosphereError> {
        let normal = normal.normalized().ok_or(AtmosphereError::InvalidSample)?;
        if !point.is_finite() {
            return Err(AtmosphereError::InvalidSample);
        }
        self.trace_inner(
            ray,
            scene,
            markers,
            Some((point, normal)),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map(|result| result.1)
    }
    fn trace_inner(
        &self,
        ray: Ray,
        scene: &TerrainScene,
        markers: &[crate::survey::SurveyMarker],
        plane: Option<(Vec3d, Vec3d)>,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(OpticalTrace, Option<Vec3d>), AtmosphereError> {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AtmosphereError::NoConvergence);
        }
        let ray = Ray::new(ray.origin, ray.direction).ok_or(AtmosphereError::InvalidSample)?;
        let initial_geo =
            ecef_to_geodetic(ray.origin).map_err(|_| AtmosphereError::UnsupportedHeight)?;
        let initial_ground = scene
            .ground(initial_geo)
            .map_err(|_| AtmosphereError::UnsupportedHeight)?;
        let clearance = initial_geo.height_m - initial_ground.hit.geodetic.height_m;
        if clearance < -0.000001 {
            return Err(AtmosphereError::UnsupportedHeight);
        }
        if clearance.abs() <= 0.000001 && ray.direction.dot(initial_ground.hit.normal) < 0.0 {
            let mut surface = initial_ground;
            surface.hit.distance_m = 0.0;
            surface.hit.position = ray.origin;
            return Ok((
                OpticalTrace {
                    origin: ray.origin,
                    initial_direction: ray.direction,
                    initial_vacuum_distance_m: 0.0,
                    surface: Some(surface),
                    marker: None,
                    exit_ray: ray,
                    geometric_path_m: 0.0,
                    optical_path_m: 0.0,
                    steps: 0,
                },
                None,
            ));
        }
        let vacuum_plane = |origin: Vec3d, direction: Vec3d| {
            plane.and_then(|(point, normal)| {
                let denominator = normal.dot(direction);
                if denominator.abs() < 1e-15 {
                    return None;
                }
                let distance = normal.dot(point - origin) / denominator;
                (distance > 1e-7).then_some((distance, origin + direction * distance))
            })
        };
        // Every exit path uses the same bounded vacuum ordering. A plane is
        // visible only when no canonical surface or prism precedes it.
        let finish_vacuum = |mut trace: OpticalTrace,
                             known_plane: Option<Vec3d>|
         -> Result<(OpticalTrace, Option<Vec3d>), AtmosphereError> {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(AtmosphereError::NoConvergence);
            }
            if known_plane.is_some() {
                return Ok((trace, known_plane));
            }
            let candidate = vacuum_plane(trace.exit_ray.origin, trace.exit_ray.direction);
            let limit = candidate.map_or(f64::INFINITY, |p| p.0);
            if let Some((distance, surface, marker)) =
                scene::vacuum_event(trace.exit_ray, scene, markers, limit)?
            {
                trace.surface = surface;
                trace.marker = marker;
                trace.geometric_path_m += distance;
                trace.optical_path_m += distance;
                trace.exit_ray.origin = trace.exit_ray.origin + trace.exit_ray.direction * distance;
                if trace.steps == 0 {
                    trace.initial_vacuum_distance_m = trace.geometric_path_m;
                }
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(AtmosphereError::NoConvergence);
                }
                return Ok((trace, None));
            }
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(AtmosphereError::NoConvergence);
            }
            Ok((trace, candidate.map(|p| p.1)))
        };
        let Some(entry) = self.entry(ray)? else {
            return finish_vacuum(
                OpticalTrace {
                    origin: ray.origin,
                    initial_direction: ray.direction,
                    initial_vacuum_distance_m: 0.0,
                    surface: None,
                    marker: None,
                    exit_ray: ray,
                    geometric_path_m: 0.0,
                    optical_path_m: 0.0,
                    steps: 0,
                },
                None,
            );
        };
        if entry > 0.0
            && let Some((distance, surface, marker)) =
                scene::vacuum_event(ray, scene, markers, entry)?
        {
            return Ok((
                OpticalTrace {
                    origin: ray.origin,
                    initial_direction: ray.direction,
                    initial_vacuum_distance_m: distance,
                    surface,
                    marker,
                    exit_ray: Ray {
                        origin: ray.origin + ray.direction * distance,
                        direction: ray.direction,
                    },
                    geometric_path_m: distance,
                    optical_path_m: distance,
                    steps: 0,
                },
                vacuum_plane(ray.origin, ray.direction)
                    .filter(|p| p.0 <= distance)
                    .map(|p| p.1),
            ));
        }
        let mut state = State {
            p: ray.origin + ray.direction * entry,
            u: ray.direction,
            optical: entry,
        };
        let mut path = entry;
        let mut plane_hit = vacuum_plane(ray.origin, ray.direction)
            .filter(|p| p.0 <= entry)
            .map(|p| p.1);
        let mut proposed = MAX_STEP_M;
        // Hessian norm of geodetic height is bounded by reciprocal minimum
        // meridional radius outside WGS84. Add maximum ray curvature.
        let curvature = 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M) + self.maximum_gradient;
        for steps in 0..MAX_TRACE_STEPS {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(AtmosphereError::NoConvergence);
            }
            if plane_hit.is_some() {
                return Ok((
                    OpticalTrace {
                        origin: ray.origin,
                        initial_direction: ray.direction,
                        initial_vacuum_distance_m: entry,
                        surface: None,
                        marker: None,
                        exit_ray: Ray {
                            origin: state.p,
                            direction: state.u,
                        },
                        geometric_path_m: path,
                        optical_path_m: state.optical,
                        steps,
                    },
                    plane_hit,
                ));
            }
            let (height, up) = Self::geometry(state.p)?;
            let vertical = state.u.dot(up);
            if plane_hit.is_none()
                && path > 1e-7
                && let Some((point, normal)) = plane
                && normal.dot(state.p - point).abs() <= 1e-7
            {
                // An event-bound step can land on the plane without changing
                // the sign. Retain that positive-distance event rather than
                // skipping it when the next step leaves the numerical band.
                plane_hit = Some(state.p);
            }
            if height >= self.top_height_m() - 1e-6 && vertical > 0.0 {
                let exit_ray = Ray {
                    origin: state.p,
                    direction: state.u,
                };
                return finish_vacuum(
                    OpticalTrace {
                        origin: ray.origin,
                        initial_direction: ray.direction,
                        initial_vacuum_distance_m: entry,
                        surface: None,
                        marker: None,
                        exit_ray,
                        geometric_path_m: path,
                        optical_path_m: state.optical,
                        steps,
                    },
                    plane_hit,
                );
            }
            if path - entry > MAX_ATMOSPHERIC_PATH_M {
                return Err(AtmosphereError::NoConvergence);
            }
            let reference_clearance = height.abs();
            let toward = -height.signum() * vertical;
            let discriminant = (toward * toward + 2.0 * curvature * reference_clearance).sqrt();
            let safe = if reference_clearance == 0.0 {
                0.0
            } else if toward >= 0.0 {
                2.0 * reference_clearance / (discriminant + toward)
            } else {
                (discriminant - toward) / curvature
            };
            let mut step = proposed
                .min(MAX_STEP_M)
                .min((safe * 0.8).max(0.01))
                .min(self.layer_step(height, vertical, curvature));
            // Prevent a curved segment from crossing a guide plane and
            // returning to its original side without any endpoint bracket.
            if plane_hit.is_none()
                && let Some((point, normal)) = plane
            {
                let signed = normal.dot(state.p - point);
                if signed.abs() > 1e-7 {
                    let toward = -signed.signum() * normal.dot(state.u);
                    let root = (toward * toward + 2.0 * curvature * signed.abs()).sqrt();
                    let bound = if toward >= 0.0 {
                        2.0 * signed.abs() / (root + toward)
                    } else {
                        (root - toward) / curvature
                    };
                    step = step.min(bound.max(0.01));
                }
            }
            // Close descending rays deliberately bracket the ground; numerical
            // boundary uncertainty is resolved by reintegration, not a fake hit.
            if (0.0..0.001).contains(&height) && vertical < -1e-8 {
                step = step.min((2.0 * height.max(1e-8) / -vertical).clamp(0.01, 1.0));
            }
            let full = self.midpoint(state, step)?;
            let half = self.advance(state, step)?;
            let error = full
                .p
                .distance(half.p)
                .max((full.u - half.u).length() * step)
                .max((full.optical - half.optical).abs())
                / 3.0;
            if error > POSITION_TOLERANCE_M && step > 0.01000001 {
                proposed = step * 0.5;
                continue;
            }
            if !error.is_finite() || error > POSITION_TOLERANCE_M {
                return Err(AtmosphereError::NoConvergence);
            }
            let next_height = Self::geometry(half.p)?.0;
            let event =
                self.scene_event(state, half, step, scene, markers, ray.origin, cancelled)?;
            if let Some(event) = event {
                // A reference-surface root can precede a mesh/box event. Its
                // coverage is checked at the actual integrated crossing below.
                let reference_before =
                    if height * Self::geometry(event.state.p)?.0 <= 0.0 && height != 0.0 {
                        let mut lo = 0.0;
                        let mut hi = event.distance;
                        for _ in 0..40 {
                            let m = (lo + hi) * 0.5;
                            if Self::geometry(self.advance(state, m)?.p)?.0 * height > 0.0 {
                                lo = m;
                            } else {
                                hi = m;
                            }
                        }
                        let distance = (lo + hi) * 0.5;
                        let crossing = self.advance(state, distance)?;
                        scene
                            .optical_reference_surface(crossing.p, ray.origin)
                            .map(|surface| (distance, crossing, surface))
                    } else {
                        None
                    };
                let (distance, end, surface, marker) =
                    if let Some((distance, end, surface)) = reference_before {
                        (distance, end, Some(surface), None)
                    } else {
                        (event.distance, event.state, event.surface, event.marker)
                    };
                if plane_hit.is_none() {
                    plane_hit = self.plane_crossing(state, end, distance, plane)?;
                }
                return Ok((
                    OpticalTrace {
                        origin: ray.origin,
                        initial_direction: ray.direction,
                        initial_vacuum_distance_m: entry,
                        surface,
                        marker,
                        exit_ray: Ray {
                            origin: end.p,
                            direction: end.u,
                        },
                        geometric_path_m: path + distance,
                        optical_path_m: end.optical,
                        steps: steps + 1,
                    },
                    plane_hit,
                ));
            }
            if (height > 0.0 && next_height <= 0.0) || (height < 0.0 && next_height >= 0.0) {
                let mut lower = 0.0;
                let mut upper = step;
                for _ in 0..40 {
                    if upper - lower < 1e-6 {
                        break;
                    }
                    let middle = (lower + upper) * 0.5;
                    if Self::geometry(self.advance(state, middle)?.p)?.0 * height > 0.0 {
                        lower = middle;
                    } else {
                        upper = middle;
                    }
                }
                let distance = (lower + upper) * 0.5;
                let end = self.advance(state, distance)?;
                if plane_hit.is_none() {
                    plane_hit = self.plane_crossing(state, end, distance, plane)?;
                }
                if let Some(surface) = scene.optical_reference_surface(end.p, ray.origin) {
                    return Ok((
                        OpticalTrace {
                            origin: ray.origin,
                            initial_direction: ray.direction,
                            initial_vacuum_distance_m: entry,
                            surface: Some(surface),
                            marker: None,
                            exit_ray: Ray {
                                origin: end.p,
                                direction: end.u,
                            },
                            geometric_path_m: path + distance,
                            optical_path_m: end.optical,
                            steps: steps + 1,
                        },
                        plane_hit,
                    ));
                }
            }
            if next_height >= self.top_height_m() && height < self.top_height_m() {
                let mut low = 0.0;
                let mut high = step;
                for _ in 0..40 {
                    if high - low < 1e-6 {
                        break;
                    }
                    let mid = (low + high) * 0.5;
                    if Self::geometry(self.advance(state, mid)?.p)?.0 < self.top_height_m() {
                        low = mid;
                    } else {
                        high = mid;
                    }
                }
                let distance = (low + high) * 0.5;
                let end = self.advance(state, distance)?;
                if plane_hit.is_none() {
                    plane_hit = self.plane_crossing(state, end, distance, plane)?;
                }
                return finish_vacuum(
                    OpticalTrace {
                        origin: ray.origin,
                        initial_direction: ray.direction,
                        initial_vacuum_distance_m: entry,
                        surface: None,
                        marker: None,
                        exit_ray: Ray {
                            origin: end.p,
                            direction: end.u,
                        },
                        geometric_path_m: path + distance,
                        optical_path_m: end.optical,
                        steps: steps + 1,
                    },
                    plane_hit,
                );
            }
            if plane_hit.is_none() {
                plane_hit = self.plane_crossing(state, half, step, plane)?;
            }
            state = half;
            path += step;
            proposed = if error < POSITION_TOLERANCE_M * 0.125 {
                step * 2.0
            } else {
                step
            };
        }
        Err(AtmosphereError::NoConvergence)
    }
}
