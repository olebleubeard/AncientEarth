//! One kinematic camera from the ground to a finite four-AU exterior domain.
//! Speeds are inspection controls, never dynamical or relativistic velocities.
use crate::{
    CameraFrame, EnuBasis, Geodetic, GeometryError, MAX_SPEED_MPS, MIN_SPEED_MPS, Observer, Ray,
    TerrainScene, Vec3d, adjust_speed, astro::AstronomyFrame, ecef_to_geodetic, geodetic_to_ecef,
    inverse_near_surface, position_roundoff_m,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightSettings {
    pub adaptive_speed: bool,
}
impl Default for FlightSettings {
    fn default() -> Self {
        Self {
            adaptive_speed: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightController {
    _private: (),
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlightReport {
    pub speed_mps: f64,
    pub travelled_m: f64,
    pub blocked: bool,
}

/// Declared control sensitivity, independent of the optical field of view.
pub const BODY_ROLL_DEG_PER_SECOND: f64 = 90.0;

/// Practical camera-control boundary in WGS84 ellipsoidal EYE height. This is
/// not a physical discontinuity or a weather/atmospheric-model coverage limit.
pub const HORIZON_LOCK_MAX_HEIGHT_M: f64 = 100_000.0;

/// Eligibility is deliberately independent of local terrain/geoid height.
/// Invalid/noncanonical poses are left for the existing navigation validation.
pub fn horizon_lock_active(observer: Observer, enabled: bool) -> bool {
    let eye = observer.eye_geodetic();
    enabled
        && Geodetic::new(
            observer.feet.lat_deg,
            observer.feet.lon_deg,
            observer.feet.height_m,
        )
        .is_ok()
        && Geodetic::new(eye.lat_deg, eye.lon_deg, eye.height_m).is_ok()
        && eye.height_m <= HORIZON_LOCK_MAX_HEIGHT_M
        && (0.1..=crate::STATURE_M).contains(&observer.eye_height_m)
        && observer.feet.height_m <= observer.maximum_feet_height_m()
        && observer.heading_deg.is_finite()
        && observer.roll_deg.is_finite()
        && (-90.0..=90.0).contains(&observer.pitch_deg)
}

/// Remove bank only, preserving forward direction and all position, yaw, pitch,
/// posture and speed fields exactly. At a vertical sight the stored heading
/// defines the otherwise nonunique zero-roll frame. Returns whether it changed.
pub fn enforce_horizon_lock(observer: &mut Observer, enabled: bool) -> bool {
    if horizon_lock_active(*observer, enabled) && observer.roll_deg != 0.0 {
        observer.roll_deg = 0.0;
        true
    } else {
        false
    }
}

/// Repeatable native comparison pose, deliberately banked below the control
/// threshold with the curved/dipped Earth horizon in the ordinary field.
pub fn horizon_lock_demo_observer() -> Observer {
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 50_000.0).unwrap());
    observer.heading_deg = 90.0;
    observer.pitch_deg = -10.0;
    observer.roll_deg = 30.0;
    observer
}

/// Express a deliberate geodetic-vertical route in the current body axes.
/// Used by the native ascent/return probe; normal flight uses direct inputs.
pub fn geodetic_vertical_body_axes(observer: Observer, signed_up: f64) -> [f64; 3] {
    if !signed_up.is_finite() {
        return [0.0; 3];
    }
    let frame = observer.frame();
    let direction = EnuBasis::at(observer.feet).up * signed_up.clamp(-1.0, 1.0);
    [
        direction.dot(frame.forward),
        direction.dot(frame.right),
        direction.dot(frame.up),
    ]
}

pub(crate) fn rotate_axis(vector: Vec3d, axis: Vec3d, radians: f64) -> Vec3d {
    let (s, c) = radians.sin_cos();
    vector * c + axis.cross(vector) * s + axis * (axis.dot(vector) * (1.0 - c))
}

/// Convert a deliberate physical camera pose without changing its eye/posture
/// settings. This is an explicit teleport helper, not an automatic view fit.
pub fn observer_from_ecef_pose(
    template: Observer,
    eye: Vec3d,
    forward: Vec3d,
    up: Vec3d,
) -> Result<Observer, GeometryError> {
    let eye_geodetic = ecef_to_geodetic(eye)?;
    let mut observer = template;
    let height = eye_geodetic.height_m - observer.eye_height_m;
    if height > observer.maximum_feet_height_m() + position_roundoff_m(eye) {
        return Err(GeometryError::ObserverHeight);
    }
    observer.feet = Geodetic::new(
        eye_geodetic.lat_deg,
        eye_geodetic.lon_deg,
        height.min(observer.maximum_feet_height_m()),
    )?;
    observer.set_view_axes(forward, up)?;
    Ok(observer)
}

impl FlightController {
    pub fn scroll(&mut self, observer: &mut Observer, wheel_lines: f64) {
        observer.speed_mps = adjust_speed(observer.speed_mps, wheel_lines);
    }
    pub fn effective_speed(
        &self,
        scene: &TerrainScene,
        observer: Observer,
        astronomy: Option<&AstronomyFrame>,
        settings: FlightSettings,
        boost: bool,
        precision: bool,
    ) -> f64 {
        speed(scene, observer, astronomy, settings, boost, precision)
    }
    /// Legacy route API: forward/right/geodetic-up input. Interactive body
    /// flight uses `step_body`; both share the same integration and collision.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        scene: &TerrainScene,
        observer: &mut Observer,
        astronomy: Option<&AstronomyFrame>,
        settings: FlightSettings,
        axes: [f64; 3],
        elapsed_seconds: f64,
        boost: bool,
        precision: bool,
    ) -> FlightReport {
        let vertical = EnuBasis::at(observer.feet).up;
        self.step_with_up(
            scene,
            observer,
            astronomy,
            settings,
            axes,
            elapsed_seconds,
            boost,
            precision,
            vertical,
        )
    }
    /// Forward/right/body-up input follows the current complete camera triad,
    /// including pitch and roll. Combined motion has normalized speed.
    #[allow(clippy::too_many_arguments)]
    pub fn step_body(
        &mut self,
        scene: &TerrainScene,
        observer: &mut Observer,
        astronomy: Option<&AstronomyFrame>,
        settings: FlightSettings,
        axes: [f64; 3],
        elapsed_seconds: f64,
        boost: bool,
        precision: bool,
    ) -> FlightReport {
        let vertical = observer.frame().up;
        self.step_with_up(
            scene,
            observer,
            astronomy,
            settings,
            axes,
            elapsed_seconds,
            boost,
            precision,
            vertical,
        )
    }
    /// Normalize simultaneous input; integrate the clearance-dependent speed
    /// with RK4 step doubling. At most 128 trials and .25 seconds are processed.
    /// Every accepted straight ECEF segment is checked against the same modeled
    /// Earth surface and apparent celestial spheres used for measurement.
    #[allow(clippy::too_many_arguments)]
    fn step_with_up(
        &mut self,
        scene: &TerrainScene,
        observer: &mut Observer,
        astronomy: Option<&AstronomyFrame>,
        settings: FlightSettings,
        axes: [f64; 3],
        elapsed_seconds: f64,
        boost: bool,
        precision: bool,
        vertical: Vec3d,
    ) -> FlightReport {
        let initial_speed = speed(scene, *observer, astronomy, settings, boost, precision);
        let mut report = FlightReport {
            speed_mps: initial_speed,
            ..Default::default()
        };
        if !elapsed_seconds.is_finite()
            || elapsed_seconds <= 0.0
            || !axes.iter().all(|x| x.is_finite())
            || initial_speed == 0.0
        {
            return report;
        }
        let frame = observer.frame();
        let Some(direction) = (frame.forward * axes[0].clamp(-1.0, 1.0)
            + frame.right * axes[1].clamp(-1.0, 1.0)
            + vertical * axes[2].clamp(-1.0, 1.0))
        .normalized() else {
            return report;
        };
        let mut remaining = elapsed_seconds.min(0.25);
        let mut dt = remaining;
        for _ in 0..128 {
            if remaining <= 1e-12 {
                break;
            }
            dt = dt.min(remaining);
            let origin = geodetic_to_ecef(observer.feet);
            let evaluate = |distance: f64| {
                let mut candidate = *observer;
                if let Ok(location) = inverse_near_surface(origin + direction * distance) {
                    candidate.feet = location;
                    speed(scene, candidate, astronomy, settings, boost, precision)
                } else {
                    initial_speed
                }
            };
            let rk = |start: f64, duration: f64| {
                let a = evaluate(start);
                let b = evaluate(start + duration * a * 0.5);
                let c = evaluate(start + duration * b * 0.5);
                let d = evaluate(start + duration * c);
                start + duration * (a + 2.0 * b + 2.0 * c + d) / 6.0
            };
            let full = rk(0.0, dt);
            let half = rk(rk(0.0, dt * 0.5), dt * 0.5);
            let tolerance = (1e-4_f64)
                .max(half.abs() * 1e-7)
                .max(position_roundoff_m(origin));
            let error = (half - full).abs() / 15.0;
            if !half.is_finite() || !error.is_finite() {
                report.blocked = true;
                break;
            }
            if error > tolerance {
                dt *= 0.5;
                continue;
            }
            let accepted = move_segment(scene, observer, astronomy, frame, direction, half);
            report.travelled_m += accepted;
            remaining -= dt;
            if accepted + tolerance < half {
                report.blocked = true;
                break;
            }
            if error < tolerance / 32.0 {
                dt *= 2.0;
            }
        }
        if remaining > 1e-12 {
            report.blocked = true;
        }
        report.speed_mps = speed(scene, *observer, astronomy, settings, boost, precision);
        report
    }
}

fn speed(
    scene: &TerrainScene,
    observer: Observer,
    astronomy: Option<&AstronomyFrame>,
    settings: FlightSettings,
    boost: bool,
    precision: bool,
) -> f64 {
    if !observer.speed_mps.is_finite() {
        return 0.0;
    }
    let multiplier = if settings.adaptive_speed {
        let mut clearance = scene.ground(observer.feet).map_or(0.0, |ground| {
            (observer.feet.height_m - ground.hit.geodetic.height_m).max(0.0)
        });
        if let Some(frame) = astronomy {
            for body in [&frame.sun, &frame.moon] {
                let distance = (observer.eye_position() - body.apparent_center_ecef_m).length()
                    - body.radius_m;
                clearance = clearance.min(distance.max(0.0));
            }
        }
        1.0 + clearance / 50.0
    } else {
        1.0
    };
    (observer.speed_mps.clamp(MIN_SPEED_MPS, MAX_SPEED_MPS)
        * multiplier
        * if boost { 8.0 } else { 1.0 }
        * if precision { 0.1 } else { 1.0 })
    .min(MAX_SPEED_MPS)
}

fn move_segment(
    scene: &TerrainScene,
    observer: &mut Observer,
    astronomy: Option<&AstronomyFrame>,
    frame: CameraFrame,
    direction: Vec3d,
    requested: f64,
) -> f64 {
    let origin = geodetic_to_ecef(observer.feet);
    let allowance = position_roundoff_m(origin);
    if let Ok(ground) = scene.ground(observer.feet)
        && observer.feet.height_m - ground.hit.geodetic.height_m <= allowance
        && direction.dot(ground.hit.normal) < -1e-12
    {
        return 0.0;
    }
    let mut distance = requested;
    if let Some(hit) = scene.intersect(Ray {
        origin: origin + direction * allowance,
        direction,
    }) {
        distance = distance.min((hit.hit.distance_m - allowance).max(0.0));
    }
    if let Some(astronomy) = astronomy {
        let eye = observer.eye_position();
        for body in [astronomy.sun, astronomy.moon] {
            let relative = eye - body.apparent_center_ecef_m;
            let contact = position_roundoff_m(eye)
                .max(position_roundoff_m(body.apparent_center_ecef_m))
                .max(16.0 * f64::EPSILON * body.radius_m);
            if relative.length() <= body.radius_m + contact {
                // For the frozen straight eye proxy, r(s)^2=r(0)^2+2*m*s+s^2.
                // Outward/tangent motion can escape an existing contact; a
                // small negative m permits at most `contact` radial decrease.
                // Never confuse a measurement's far-side exit with safe entry.
                if relative.dot(direction) < -contact {
                    return 0.0;
                }
            } else if let Some(hit) = body.nearest_surface_distance(eye, direction) {
                distance = distance.min((hit - contact).max(0.0));
            }
        }
    }
    let maximum = observer.maximum_feet_height_m();
    let within_ceiling =
        |d| inverse_near_surface(origin + direction * d).is_ok_and(|g| g.height_m <= maximum);
    if !within_ceiling(distance) {
        let mut low = 0.0;
        let mut high = distance;
        for _ in 0..64 {
            let mid = (low + high) * 0.5;
            if within_ceiling(mid) {
                low = mid;
            } else {
                high = mid;
            }
        }
        distance = low;
    }
    let Ok(mut location) = inverse_near_surface(origin + direction * distance) else {
        return 0.0;
    };
    if !scene.permits_uncovered_segment(origin, origin + direction * distance) {
        return 0.0;
    }
    let Ok(ground) = scene.ground(location) else {
        return 0.0;
    };
    // A coverage-edge height discontinuity has no invented connecting wall.
    // Reject an endpoint below that surface rather than tunneling into it.
    if location.height_m + allowance < ground.hit.geodetic.height_m {
        return 0.0;
    }
    location.height_m = location
        .height_m
        .max(ground.hit.geodetic.height_m)
        .min(maximum);
    observer.feet = location;
    let _ = observer.set_view_axes(frame.forward, frame.up);
    origin.distance(geodetic_to_ecef(observer.feet))
}
