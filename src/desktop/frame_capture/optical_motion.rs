//! Opt-in moving optical-scene check. Every wall-clock interval is retained;
//! command time is capped separately and completion is not an FPS assertion.
use super::super::Explorer;
use ancient_earth::surface::WorldSurface;
use earth_math::{
    Lens, Observer, Vec3d,
    astro::AstronomyFrame,
    navigation::{FlightController, FlightSettings, geodetic_vertical_body_axes},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub struct OpticalMotionSnapshot {
    pub optical_active: bool,
    pub optical_ready: bool,
    pub presentation_ready: bool,
    /// Current main-world material submission is for this Observer and Lens.
    /// This does not certify hardware presentation completion. A future
    /// asynchronous terminal-image pipeline must strengthen its provenance.
    pub render_matches_observer: bool,
    pub focused: bool,
    pub physical_size: [u32; 2],
    pub scale_factor: f32,
    /// Exact current compute producer header, absent on the direct path/pending submission.
    pub submitted_compute_control: Option<[u32; 4]>,
    /// Current compositor's expected stellar-table token, not GPU completion.
    pub expected_stellar_control: Option<[u32; 4]>,
    pub profile_generation: u64,
    pub surface_generation: u64,
    pub marker_generation: u64,
    pub visible_marker_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Ascend,
    ForwardPan,
    SlideZoomIn,
    SlideZoomOut,
    Recover,
    Complete,
}
impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::Ascend => "ascent",
            Self::ForwardPan => "forward_pan",
            Self::SlideZoomIn => "slide_zoom_in",
            Self::SlideZoomOut => "slide_zoom_out",
            Self::Recover => "recovery",
            Self::Complete => "complete",
        }
    }
    fn duration(self) -> f64 {
        if self == Self::Ascend { 2.0 } else { 4.0 }
    }
    fn next(self) -> Self {
        match self {
            Self::Ascend => Self::ForwardPan,
            Self::ForwardPan => Self::SlideZoomIn,
            Self::SlideZoomIn => Self::SlideZoomOut,
            Self::SlideZoomOut => Self::Recover,
            Self::Recover | Self::Complete => Self::Complete,
        }
    }
}

pub struct OpticalMotionProbe {
    stage: Stage,
    started: f64,
    command_seconds: f64,
    ready_since: Option<f64>,
    initial: Observer,
    expected: Observer,
    initial_lens: Lens,
    expected_lens: Lens,
    telescope: bool,
    boundaries_enabled: bool,
    horizon_lock_enabled: bool,
    source: OpticalMotionSnapshot,
    controller: FlightController,
    previous_action: &'static str,
    previous_command: Value,
    frames: Vec<Value>,
    events: Vec<Value>,
    seen_sky: bool,
    seen_horizon: bool,
    seen_ground: bool,
    inactive_intervals: usize,
    optical_pending_intervals: usize,
    presentation_pending_intervals: usize,
    stale_render_intervals: usize,
    unfocused_intervals: usize,
    allow_unfocused_diagnostic: bool,
    failure: Option<&'static str>,
}

impl OpticalMotionProbe {
    pub fn begin(
        state: &Explorer,
        surface: &WorldSurface,
        snapshot: OpticalMotionSnapshot,
        elapsed: f64,
        allow_unfocused_diagnostic: bool,
    ) -> Result<Self, &'static str> {
        let scene = surface.active(state.boundaries_enabled);
        if !elapsed.is_finite()
            || !snapshot.optical_active
            || !snapshot.optical_ready
            || !snapshot.presentation_ready
            || !snapshot.render_matches_observer
            || (!snapshot.focused && !allow_unfocused_diagnostic)
            || snapshot.visible_marker_count == 0
            || snapshot.surface_generation != surface.generation
            || scene.is_boundary_only()
            || scene.imported_ground(state.observer.feet).is_none()
            || !(-1_000.0..19_000.0).contains(&state.observer.eye_geodetic().height_m)
            || (state.lens().vertical_fov_deg - 60.0).abs() > 1e-9
            || !(15.0..=35.0).contains(&state.observer.pitch_deg)
            || state.body_tracking.target().is_some()
        {
            return Err(
                "optical-flight QA requires current ready optics, physical terrain and visible markers, a 60 degree lens and initial pitch 15-35 degrees; focus is required unless the explicit unfocused diagnostic policy is enabled; use the paused nearby optical-terrain-survey fixture",
            );
        }
        Ok(Self {
            stage: Stage::Ascend,
            started: elapsed,
            command_seconds: 0.0,
            ready_since: None,
            initial: state.observer,
            expected: state.observer,
            initial_lens: state.lens(),
            expected_lens: state.lens(),
            telescope: state.telescope,
            boundaries_enabled: state.boundaries_enabled,
            horizon_lock_enabled: state.horizon_lock_enabled,
            source: snapshot,
            controller: FlightController::default(),
            previous_action: "warmup_boundary",
            previous_command: Value::Null,
            frames: Vec::new(),
            events: Vec::new(),
            seen_sky: false,
            seen_horizon: false,
            seen_ground: false,
            inactive_intervals: 0,
            optical_pending_intervals: 0,
            presentation_pending_intervals: 0,
            stale_render_intervals: 0,
            unfocused_intervals: 0,
            allow_unfocused_diagnostic,
            failure: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        state: &mut Explorer,
        surface: &WorldSurface,
        astronomy: Option<&AstronomyFrame>,
        snapshot: OpticalMotionSnapshot,
        elapsed: f64,
        interval: f64,
    ) -> Result<bool, &'static str> {
        let result = self.advance(state, surface, astronomy, snapshot, elapsed, interval);
        if let Err(error) = result {
            self.failure = Some(error);
            state.observer.speed_mps = self.initial.speed_mps;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn advance(
        &mut self,
        state: &mut Explorer,
        surface: &WorldSurface,
        astronomy: Option<&AstronomyFrame>,
        snapshot: OpticalMotionSnapshot,
        elapsed: f64,
        interval: f64,
    ) -> Result<bool, &'static str> {
        if !elapsed.is_finite() || !interval.is_finite() || interval <= 0.0 {
            return Err("optical-flight QA requires finite positive frame intervals");
        }
        let observer = state.observer;
        let frame = observer.frame();
        let xyz = |p: Vec3d| [p.x, p.y, p.z];
        self.inactive_intervals += usize::from(!snapshot.optical_active);
        self.optical_pending_intervals += usize::from(!snapshot.optical_ready);
        self.presentation_pending_intervals += usize::from(!snapshot.presentation_ready);
        self.stale_render_intervals += usize::from(!snapshot.render_matches_observer);
        self.unfocused_intervals += usize::from(!snapshot.focused);
        self.seen_sky |= observer.pitch_deg > 10.0;
        self.seen_horizon |= observer.pitch_deg.abs() <= 5.0;
        self.seen_ground |= observer.pitch_deg < -10.0;
        self.frames.push(json!({
            "phase":self.previous_action,"interval_seconds":interval,
            "elapsed_recording_seconds":elapsed-self.started,"previous_command":self.previous_command,
            "eye_ecef_m":xyz(frame.origin),"forward":xyz(frame.forward),"up":xyz(frame.up),
            "feet_latitude_deg":observer.feet.lat_deg,"feet_longitude_deg":observer.feet.lon_deg,
            "feet_height_m":observer.feet.height_m,"eye_height_m":observer.eye_geodetic().height_m,
            "heading_deg":observer.heading_deg,"pitch_deg":observer.pitch_deg,"roll_deg":observer.roll_deg,
            "vertical_fov_deg":state.lens().vertical_fov_deg,
            "optical_active":snapshot.optical_active,"optical_ready":snapshot.optical_ready,
            "presentation_ready":snapshot.presentation_ready,"render_matches_observer":snapshot.render_matches_observer,
            "focused":snapshot.focused,"profile_generation":snapshot.profile_generation,
            "physical_size":snapshot.physical_size,"scale_factor":snapshot.scale_factor,
            "submitted_compute_control":snapshot.submitted_compute_control,
            "expected_stellar_control":snapshot.expected_stellar_control,
            "surface_generation":snapshot.surface_generation,"marker_generation":snapshot.marker_generation,
            "visible_marker_count":snapshot.visible_marker_count
        }));
        if elapsed - self.started > 240.0 {
            return Err("optical-flight QA exceeded its 240 second watchdog");
        }
        if snapshot.profile_generation != self.source.profile_generation
            || snapshot.surface_generation != self.source.surface_generation
            || surface.generation != self.source.surface_generation
            || snapshot.marker_generation != self.source.marker_generation
            || snapshot.visible_marker_count != self.source.visible_marker_count
            || state.boundaries_enabled != self.boundaries_enabled
            || state.horizon_lock_enabled != self.horizon_lock_enabled
            || snapshot.physical_size != self.source.physical_size
            || snapshot.scale_factor != self.source.scale_factor
        {
            return Err("optical-flight QA source, scene, marker or control configuration changed");
        }
        let expected_frame = self.expected.frame();
        if frame.origin.distance(expected_frame.origin) > 0.001
            || frame.forward.distance(expected_frame.forward) > 1e-10
            || frame.up.distance(expected_frame.up) > 1e-10
            || state.lens() != self.expected_lens
            || state.telescope != self.telescope
            || state.body_tracking.target().is_some()
        {
            return Err(
                "optical-flight QA detected uncommanded pose/lens edits; pause time and other controls",
            );
        }
        let scene = surface.active(state.boundaries_enabled);
        if scene.is_boundary_only() {
            return Err("optical-flight QA lost physical terrain");
        }
        self.previous_action = self.stage.label();
        self.previous_command = Value::Null;
        match self.stage {
            Stage::Ascend | Stage::ForwardPan | Stage::SlideZoomIn | Stage::SlideZoomOut => {
                let dt = interval
                    .min(0.1)
                    .min(self.stage.duration() - self.command_seconds);
                let axes = match self.stage {
                    Stage::Ascend => geodetic_vertical_body_axes(state.observer, 1.0),
                    Stage::ForwardPan => {
                        let lens = state.lens();
                        state.observer.apply_free_look(30.0 * dt, 100.0 * dt, lens);
                        state.apply_horizon_lock();
                        [1.0, 0.0, 0.0]
                    }
                    Stage::SlideZoomIn => [0.0, 1.0, 0.0],
                    _ => [0.0, -1.0, 0.0],
                };
                state.observer.speed_mps = match self.stage {
                    Stage::Ascend => 25.0,
                    Stage::ForwardPan => 20.0,
                    _ => 15.0,
                };
                let previous_eye = state.observer.eye_position();
                let report = self.controller.step_body(
                    scene,
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
                state.flight_speed_mps = report.speed_mps;
                state.apply_horizon_lock();
                self.previous_command = json!({
                    "command_seconds":dt,"body_axes":axes,"speed_mps":report.speed_mps,
                    "travelled_m":report.travelled_m,"blocked":report.blocked,
                    "look_mouse_units":if self.stage==Stage::ForwardPan {[30.0*dt,100.0*dt]}else{[0.0;2]},
                    "zoom_wheel_lines":match self.stage {Stage::SlideZoomIn=>1.5*dt,Stage::SlideZoomOut=>-1.5*dt,_=>0.0}
                });
                if report.blocked {
                    return Err("optical-flight QA movement was blocked by the actual scene");
                }
                if (state.observer.eye_position().distance(previous_eye) - report.travelled_m).abs()
                    > 0.001
                {
                    return Err("optical-flight QA displacement differs from controller output");
                }
                if matches!(self.stage, Stage::SlideZoomIn | Stage::SlideZoomOut) {
                    let lens = if state.telescope {
                        &mut state.telescope_lens
                    } else {
                        &mut state.normal_lens
                    };
                    lens.zoom(if self.stage == Stage::SlideZoomIn {
                        1.5 * dt
                    } else {
                        -1.5 * dt
                    });
                }
                self.command_seconds += dt;
                if self.command_seconds >= self.stage.duration() {
                    if scene.imported_ground(state.observer.feet).is_none() {
                        return Err("optical-flight QA left imported source coverage");
                    }
                    self.events.push(json!({
                        "finished_phase":self.stage.label(),"elapsed_recording_seconds":elapsed-self.started,
                        "command_seconds":self.command_seconds,"eye_height_m":state.observer.eye_geodetic().height_m,
                        "pitch_deg":state.observer.pitch_deg,"vertical_fov_deg":state.lens().vertical_fov_deg,
                        "last_command":self.previous_command
                    }));
                    self.stage = self.stage.next();
                    self.command_seconds = 0.0;
                }
            }
            Stage::Recover => {
                if snapshot.optical_active
                    && snapshot.optical_ready
                    && snapshot.presentation_ready
                    && snapshot.render_matches_observer
                    && (snapshot.focused || self.allow_unfocused_diagnostic)
                {
                    let since = *self.ready_since.get_or_insert(elapsed);
                    if elapsed - since >= 3.0 {
                        if (state.lens().vertical_fov_deg - self.initial_lens.vertical_fov_deg)
                            .abs()
                            > 1e-8
                        {
                            return Err(
                                "optical-flight QA did not return to its initial field of view",
                            );
                        }
                        state.observer.speed_mps = self.initial.speed_mps;
                        self.stage = Stage::Complete;
                    }
                } else {
                    self.ready_since = None;
                }
            }
            Stage::Complete => {}
        }
        self.expected = state.observer;
        self.expected_lens = state.lens();
        Ok(self.stage == Stage::Complete)
    }

    pub fn report(&self) -> Value {
        let complete = self.stage == Stage::Complete && self.failure.is_none();
        let route_checks_passed = complete
            && self.seen_sky
            && self.seen_horizon
            && self.seen_ground
            && self.inactive_intervals == 0
            && self.optical_pending_intervals == 0
            && self.presentation_pending_intervals == 0
            && self.stale_render_intervals == 0;
        let passed = route_checks_passed
            && !self.allow_unfocused_diagnostic
            && self.source.focused
            && self.unfocused_intervals == 0;
        json!({
            "passed":passed,"route_completed":complete,"failure":self.failure,
            "unfocused_diagnostic_policy":self.allow_unfocused_diagnostic,
            "initial_focused":self.source.focused,
            "diagnostic_route_passed":self.allow_unfocused_diagnostic && route_checks_passed,
            "diagnostic_policy_scope":"explicit unfocused diagnostic permits route start and final readiness recovery without focus; all raw focus flags and counts remain; ordinary passed is always false under this policy; no focused-timing eligibility is granted",
            "seen_sky_facing":self.seen_sky,"seen_horizon_facing":self.seen_horizon,"seen_ground_facing":self.seen_ground,
            "inactive_optical_intervals":self.inactive_intervals,"optical_pending_intervals":self.optical_pending_intervals,
            "presentation_pending_intervals":self.presentation_pending_intervals,"stale_render_intervals":self.stale_render_intervals,
            "unfocused_intervals":self.unfocused_intervals,
            "passed_definition":"controller route completed through sky, horizon and ground views with unchanged sources/markers, no fallback, pending presentation or stale optical frame, and three seconds of final focused readiness; NOT a smoothness or FPS assertion",
            "scope":"actual FlightController::step_body; retained raw wall intervals include all slow/pending/unfocused frames; phase names describe the preceding CPU command, not hardware presentation latency; no pose reset, resolution change or atmospheric-model change",
            "render_identity_scope":"exact Observer/Lens equality at current main-world material submission, not hardware presentation completion; any future asynchronous terminal-image pipeline needs matching render-world provenance",
            "command_policy":"2 s geodetic ascent at25m/s; 4 s forward at20m/s with mouse rates30/100units/s; 4 s right then4 s left at15m/s with +/-1.5zoom lines/s; dt<=0.1s; adaptive speed off;3s ready recovery",
            "frames":self.frames,"events":self.events
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::{Geodetic, TerrainPatch, TerrainScene};

    fn fixture() -> (Explorer, WorldSurface, OpticalMotionSnapshot) {
        let mut surface = WorldSurface::bundled().unwrap();
        let scene = TerrainScene::new(vec![
            TerrainPatch::from_grid(21, 21, -0.01, 0.01, 0.001, 0.001, vec![0.0; 441]).unwrap(),
        ]);
        let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap());
        let feet = observer.feet;
        scene.place_on_ground(&mut observer, feet).unwrap();
        observer.pitch_deg = 24.0;
        surface.install_terrain(scene);
        let state = Explorer {
            observer,
            ..Explorer::default()
        };
        let snapshot = OpticalMotionSnapshot {
            optical_active: true,
            optical_ready: true,
            presentation_ready: true,
            render_matches_observer: true,
            focused: true,
            physical_size: [1920, 1080],
            scale_factor: 1.0,
            submitted_compute_control: None,
            expected_stellar_control: None,
            profile_generation: 7,
            surface_generation: surface.generation,
            marker_generation: 9,
            visible_marker_count: 81,
        };
        (state, surface, snapshot)
    }

    #[test]
    fn optical_probe_real_controller_crosses_horizon_and_restores_zoom() {
        let (mut state, surface, snapshot) = fixture();
        let initial = state.observer;
        let mut probe = OpticalMotionProbe::begin(&state, &surface, snapshot, 0.0, false).unwrap();
        let mut elapsed = 0.0;
        for _ in 0..2_000 {
            elapsed += 0.02;
            if probe
                .step(&mut state, &surface, None, snapshot, elapsed, 0.02)
                .unwrap()
            {
                break;
            }
        }
        let report = probe.report();
        assert_eq!(report["passed"], true, "{report}");
        assert_eq!(probe.events.len(), 4);
        assert!(state.observer.eye_geodetic().height_m > initial.eye_geodetic().height_m + 45.0);
        assert!((state.lens().vertical_fov_deg - 60.0).abs() < 1e-8);
        assert_eq!(state.observer.speed_mps, initial.speed_mps);
        assert!(
            probe
                .frames
                .iter()
                .any(|v| v["vertical_fov_deg"].as_f64().unwrap() < 25.0)
        );
    }

    #[test]
    fn optical_probe_keeps_slow_fallback_pending_and_unfocused_intervals() {
        let (mut state, surface, mut snapshot) = fixture();
        let mut probe = OpticalMotionProbe::begin(&state, &surface, snapshot, 0.0, false).unwrap();
        snapshot.optical_active = false;
        snapshot.optical_ready = false;
        snapshot.presentation_ready = false;
        snapshot.render_matches_observer = false;
        snapshot.focused = false;
        assert!(
            !probe
                .step(&mut state, &surface, None, snapshot, 0.7, 0.7)
                .unwrap()
        );
        let report = probe.report();
        assert_eq!(report["passed"], false);
        for name in [
            "inactive_optical_intervals",
            "optical_pending_intervals",
            "presentation_pending_intervals",
            "stale_render_intervals",
            "unfocused_intervals",
        ] {
            assert_eq!(report[name], 1);
        }
        assert_eq!(report["frames"][0]["interval_seconds"], 0.7);
        assert_eq!(probe.previous_command["command_seconds"], 0.1);
    }

    #[test]
    fn optical_probe_rejects_source_replacement_and_keeps_failure_record() {
        let (mut state, surface, mut snapshot) = fixture();
        let mut probe = OpticalMotionProbe::begin(&state, &surface, snapshot, 0.0, false).unwrap();
        snapshot.profile_generation += 1;
        assert!(
            probe
                .step(&mut state, &surface, None, snapshot, 0.02, 0.02)
                .is_err()
        );
        let report = probe.report();
        assert!(report["failure"].is_string());
        assert_eq!(report["frames"].as_array().unwrap().len(), 1);
        assert_eq!(report["passed"], false);
    }

    #[test]
    fn unfocused_diagnostic_preserves_commands_and_rejects_ordinary_acceptance() {
        let (mut focused_state, surface, focused) = fixture();
        let (mut diagnostic_state, _, _) = fixture();
        let unfocused = OpticalMotionSnapshot {
            focused: false,
            ..focused
        };
        assert!(
            OpticalMotionProbe::begin(&diagnostic_state, &surface, unfocused, 0.0, false).is_err()
        );
        let mut normal =
            OpticalMotionProbe::begin(&focused_state, &surface, focused, 0.0, false).unwrap();
        let mut diagnostic =
            OpticalMotionProbe::begin(&diagnostic_state, &surface, unfocused, 0.0, true).unwrap();
        let mut elapsed = 0.0;
        for _ in 0..2_000 {
            elapsed += 0.02;
            let ordinary_done = normal
                .step(&mut focused_state, &surface, None, focused, elapsed, 0.02)
                .unwrap();
            let diagnostic_done = diagnostic
                .step(
                    &mut diagnostic_state,
                    &surface,
                    None,
                    unfocused,
                    elapsed,
                    0.02,
                )
                .unwrap();
            assert_eq!(ordinary_done, diagnostic_done);
            assert_eq!(normal.previous_command, diagnostic.previous_command);
            assert_eq!(focused_state.observer, diagnostic_state.observer);
            assert_eq!(focused_state.lens(), diagnostic_state.lens());
            if ordinary_done {
                break;
            }
        }
        let ordinary = normal.report();
        let report = diagnostic.report();
        assert_eq!(ordinary["passed"], true);
        assert_eq!(report["route_completed"], true);
        assert_eq!(report["diagnostic_route_passed"], true);
        assert_eq!(report["unfocused_diagnostic_policy"], true);
        assert_eq!(report["initial_focused"], false);
        assert_eq!(report["passed"], false);
        assert_eq!(
            report["unfocused_intervals"].as_u64().unwrap() as usize,
            diagnostic.frames.len()
        );
        assert!(
            diagnostic
                .frames
                .iter()
                .all(|frame| frame["focused"] == false)
        );
        // The explicit policy can never masquerade as ordinary focused QA,
        // even if the supplied snapshots happen to be focused throughout.
        normal.allow_unfocused_diagnostic = true;
        assert_eq!(normal.report()["passed"], false);
    }

    #[test]
    fn unfocused_diagnostic_does_not_bypass_readiness_or_configuration() {
        let (mut state, surface, mut snapshot) = fixture();
        snapshot.focused = false;
        for missing in 0..4 {
            let mut bad = snapshot;
            match missing {
                0 => bad.optical_active = false,
                1 => bad.optical_ready = false,
                2 => bad.presentation_ready = false,
                _ => bad.render_matches_observer = false,
            }
            assert!(OpticalMotionProbe::begin(&state, &surface, bad, 0.0, true).is_err());
        }
        let mut probe = OpticalMotionProbe::begin(&state, &surface, snapshot, 0.0, true).unwrap();
        snapshot.marker_generation += 1;
        assert!(
            probe
                .step(&mut state, &surface, None, snapshot, 0.02, 0.02)
                .is_err()
        );
        let report = probe.report();
        assert_eq!(report["passed"], false);
        assert_eq!(report["diagnostic_route_passed"], false);
        assert_eq!(report["unfocused_intervals"], 1);
        assert_eq!(report["frames"][0]["focused"], false);
        assert!(report["failure"].is_string());
    }
}
