//! Opt-in same-camera traversal through both display-coordinate anchor regimes.
//! Actual frame intervals are retained; capped command time is separately logged.
use super::super::Explorer;
use ancient_earth::surface::WorldSurface;
use earth_math::{
    Vec3d,
    astro::AstronomyFrame,
    navigation::{FlightController, FlightSettings, geodetic_vertical_body_axes},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Ascend,
    FarSettle,
    Descend,
    NearSettle,
    Complete,
}
impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::Ascend => "ascent",
            Self::FarSettle => "far_settle",
            Self::Descend => "descent",
            Self::NearSettle => "near_settle",
            Self::Complete => "complete",
        }
    }
}
pub struct ContinuousMotionProbe {
    stage: Stage,
    previous_action: &'static str,
    started: f64,
    command_seconds: f64,
    ready_since: Option<f64>,
    initial: earth_math::Observer,
    lens: earth_math::Lens,
    controller: FlightController,
    expected_eye: Vec3d,
    seen_space: bool,
    returned_near: bool,
    seen_annotations: bool,
    annotations_returned_off: bool,
    frames: Vec<Value>,
    events: Vec<Value>,
    maximum_camera_error_m: f64,
}
impl ContinuousMotionProbe {
    pub fn begin(state: &Explorer, elapsed: f64) -> Result<Self, &'static str> {
        if !elapsed.is_finite()
            || !(0.0..50_000.0).contains(&state.observer.eye_geodetic().height_m)
            || !(45.0..=90.0).contains(&state.lens().vertical_fov_deg)
        {
            return Err(
                "continuous-flight QA requires an initial pose below 50 km and a 45-90 degree vertical field",
            );
        }
        Ok(Self {
            stage: Stage::Ascend,
            previous_action: "warmup_boundary",
            started: elapsed,
            command_seconds: 0.0,
            ready_since: None,
            initial: state.observer,
            lens: state.lens(),
            controller: FlightController::default(),
            expected_eye: state.observer.eye_position(),
            seen_space: false,
            returned_near: false,
            seen_annotations: false,
            annotations_returned_off: false,
            frames: Vec::new(),
            events: Vec::new(),
            maximum_camera_error_m: 0.0,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        state: &mut Explorer,
        surface: &WorldSurface,
        astronomy: Option<&AstronomyFrame>,
        snapshot: super::super::renderer::ContinuousSnapshot,
        ready: bool,
        elapsed: f64,
        interval: f64,
    ) -> Result<bool, &'static str> {
        if !elapsed.is_finite() || !interval.is_finite() || interval <= 0.0 {
            return Err("invalid continuous-flight QA interval");
        }
        if elapsed - self.started > 240.0 {
            return Err("continuous-flight QA exceeded its 240 second watchdog");
        }
        let eye = state.observer.eye_position();
        if eye.distance(self.expected_eye) > 0.001 {
            return Err(
                "continuous-flight QA detected an uncommanded pose change; pause simulation and other controls",
            );
        }
        if state.lens() != self.lens {
            return Err("continuous-flight QA detected a lens change");
        }
        let camera_error = eye.distance(snapshot.reconstructed_eye);
        self.maximum_camera_error_m = self.maximum_camera_error_m.max(camera_error);
        // Conservative fixture envelope for quantized local translation plus
        // f64 reconstruction. This is not a native GPU/image error certificate.
        let camera_tolerance =
            0.001 + 2.0 * f64::from(f32::EPSILON) * eye.distance(snapshot.origin);
        if camera_error > camera_tolerance {
            return Err(
                "continuous-flight QA display translation exceeded its scale-dependent f32 fixture tolerance",
            );
        }
        self.seen_space |= snapshot.space;
        self.returned_near |= self.seen_space && !snapshot.space;
        let annotations_eligible = earth_math::rendering::continuous::space_annotations_visible(
            state.observer,
            state.lens(),
        );
        self.seen_annotations |= annotations_eligible;
        self.annotations_returned_off |= self.seen_annotations && !annotations_eligible;
        let xyz = |p: Vec3d| [p.x, p.y, p.z];
        self.frames.push(json!({"phase":self.previous_action,"interval_seconds":interval,
            "elapsed_recording_seconds":elapsed-self.started,"eye_ecef_m":xyz(eye),
            "eye_height_m":state.observer.eye_geodetic().height_m,"vertical_fov_deg":state.lens().vertical_fov_deg,
            "space_anchor":snapshot.space,"coordinate_origin_ecef_m":xyz(snapshot.origin),
            "cpu_reconstructed_f32_camera_ecef_m":xyz(snapshot.reconstructed_eye),"camera_translation_error_m":camera_error,"camera_translation_tolerance_m":camera_tolerance,
            "orbital_annotations_eligible":annotations_eligible,
            "presentation_ready":ready,"mesh_counts":snapshot.mesh_counts,"renderer_cpu":snapshot.cpu.json()}));
        self.previous_action = self.stage.label();
        match self.stage {
            Stage::Ascend | Stage::Descend => {
                let dt = interval.min(0.1).min(6.0 - self.command_seconds);
                state.observer.speed_mps = 5_000_000.0;
                let previous = state.observer.frame();
                // Preserve the diagnostic's geodetic ascent/descent while
                // exercising the same body-axis controller as interactive flight.
                let axes = geodetic_vertical_body_axes(
                    state.observer,
                    if self.stage == Stage::Ascend {
                        1.0
                    } else {
                        -1.0
                    },
                );
                let report = self.controller.step_body(
                    surface.active(state.boundaries_enabled),
                    &mut state.observer,
                    astronomy,
                    FlightSettings {
                        adaptive_speed: false,
                    },
                    axes,
                    dt,
                    false,
                    false,
                );
                if report.blocked {
                    return Err("continuous-flight QA movement was blocked");
                }
                let next = state.observer.frame();
                if next.forward.distance(previous.forward) > 1e-8
                    || next.up.distance(previous.up) > 1e-8
                {
                    return Err("continuous-flight QA detected uncommanded camera-axis rotation");
                }
                if (next.origin.distance(previous.origin) - report.travelled_m).abs() > 0.001 {
                    return Err("continuous-flight QA movement differs from its controller report");
                }
                self.command_seconds += dt;
                if self.command_seconds >= 6.0 {
                    self.events.push(json!({"finished_phase":self.stage.label(),"elapsed_recording_seconds":elapsed-self.started,
                        "command_seconds":self.command_seconds,"eye_height_m":state.observer.eye_geodetic().height_m}));
                    self.stage = if self.stage == Stage::Ascend {
                        Stage::FarSettle
                    } else {
                        Stage::NearSettle
                    };
                    self.command_seconds = 0.0;
                }
            }
            Stage::FarSettle | Stage::NearSettle => {
                if ready
                    && snapshot.space == (self.stage == Stage::FarSettle)
                    && annotations_eligible == (self.stage == Stage::FarSettle)
                {
                    let since = *self.ready_since.get_or_insert(elapsed);
                    if elapsed - since >= 3.0 {
                        if self.stage == Stage::FarSettle {
                            self.stage = Stage::Descend;
                        } else {
                            if !self.seen_space
                                || !self.returned_near
                                || !self.seen_annotations
                                || !self.annotations_returned_off
                                || eye.distance(self.initial.eye_position()) > 0.01
                            {
                                return Err(
                                    "continuous-flight QA did not complete the physical out-and-back path",
                                );
                            }
                            state.observer.speed_mps = self.initial.speed_mps;
                            self.stage = Stage::Complete;
                        }
                        self.ready_since = None;
                    }
                } else {
                    self.ready_since = None;
                }
            }
            Stage::Complete => {}
        }
        self.expected_eye = state.observer.eye_position();
        Ok(self.stage == Stage::Complete)
    }
    pub fn report(&self) -> Value {
        json!({"passed":self.stage==Stage::Complete,"seen_space_anchor":self.seen_space,
            "returned_to_near_anchor":self.returned_near,
            "seen_orbital_annotations_eligible":self.seen_annotations,"annotations_returned_ineligible":self.annotations_returned_off,"maximum_camera_translation_error_m":self.maximum_camera_error_m,
            "scope":"same body-axis FlightController and Observer; no fit, mode switch, pose reset or lens change; CPU display-translation check, not GPU image proof",
            "command_policy":"6 command seconds geodetically up and down at 5000000 m/s (30 Mm each way), expressed in the current body axes; adaptive speed disabled, dt <= 0.1s; continuous 3s ready settlement at both endpoints",
            "frames":self.frames,"events":self.events})
    }
}
