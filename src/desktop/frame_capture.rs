//! Raw application frame pacing, deliberately separate from GPU timer queries.
use super::{Explorer, renderer::RenderedEarth};
use ancient_earth::{simulation::AstronomicalScene, surface::WorldSurface};
use bevy::{
    ecs::system::SystemParam, prelude::*, render::renderer::RenderAdapterInfo,
    window::PrimaryWindow,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod motion;
use motion::MotionProbe;
mod view_motion;
use view_motion::ViewMotionProbe;
mod continuous_motion;
use continuous_motion::ContinuousMotionProbe;
mod optical_motion;
use optical_motion::{OpticalMotionProbe, OpticalMotionSnapshot};

const WARMUP_FRAMES: u64 = 120;
const WARMUP_SECONDS: f64 = 10.0;
const RECORD_SECONDS: f64 = 20.0;
const PREPARATION_TIMEOUT_SECONDS: f64 = 300.0;

#[derive(Default)]
struct SampleWindow {
    frames: u64,
    started: bool,
    ready_since: Option<f64>,
    incomplete_scene_frames: u64,
    intervals: Vec<f64>,
    recorded_seconds: f64,
}
#[derive(Debug, PartialEq, Eq)]
enum SampleState {
    Warming,
    Started,
    Recording,
    Complete,
}
impl SampleWindow {
    fn observe_scene(
        &mut self,
        elapsed: f64,
        interval: f64,
        presentation_ready: bool,
    ) -> Result<SampleState, &'static str> {
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err("invalid elapsed time");
        }
        if !self.started {
            if !presentation_ready {
                self.frames = 0;
                self.ready_since = None;
                return Ok(SampleState::Warming);
            }
            let ready_since = *self.ready_since.get_or_insert(elapsed);
            return self.observe(elapsed - ready_since, interval);
        }
        // Never remove expensive intervals once recording begins. A scene
        // change invalidates a stationary baseline but remains in the raw data.
        self.incomplete_scene_frames += u64::from(!presentation_ready);
        self.observe(elapsed, interval)
    }

    fn observe(&mut self, elapsed: f64, interval: f64) -> Result<SampleState, &'static str> {
        self.frames += 1;
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err("invalid elapsed time");
        }
        if !self.started {
            if self.frames >= WARMUP_FRAMES && elapsed >= WARMUP_SECONDS {
                self.started = true;
                // The just-finished interval began during warmup. Recording
                // begins at this boundary, with the NEXT complete interval.
                return Ok(SampleState::Started);
            }
            return Ok(SampleState::Warming);
        }
        if !interval.is_finite() || interval <= 0.0 {
            return Err("recorded frame interval must be finite and positive");
        }
        self.intervals.push(interval);
        self.recorded_seconds += interval;
        Ok(if self.recorded_seconds >= RECORD_SECONDS {
            SampleState::Complete
        } else {
            SampleState::Recording
        })
    }
}

#[derive(Resource)]
struct FrameCapture {
    output: Result<PathBuf, String>,
    arguments: Vec<String>,
    samples: SampleWindow,
    start_metadata: Option<Value>,
    start_view: Option<Value>,
    view_changed: bool,
    unfocused_frames: u64,
    motion_requested: bool,
    motion: Option<MotionProbe>,
    view_motion_requested: bool,
    view_motion: Option<ViewMotionProbe>,
    continuous_motion_requested: bool,
    continuous_motion: Option<ContinuousMotionProbe>,
    optical_motion_requested: bool,
    unfocused_optical_diagnostic: bool,
    optical_motion: Option<OpticalMotionProbe>,
    optical_motion_error: Option<String>,
    draw_visibility_samples: Vec<[usize; 3]>,
    done: bool,
}

/// Install only when explicitly requested. Root startup owns the other CLI modes.
pub fn install(app: &mut App, args: &[String]) {
    let positions: Vec<_> = args
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| (arg == "--frame-capture").then_some(i))
        .collect();
    let motion_requested = args.iter().any(|arg| arg == "--frame-capture-motion");
    let view_motion_requested = args.iter().any(|arg| arg == "--frame-capture-view-motion");
    let continuous_motion_requested = args
        .iter()
        .any(|arg| arg == "--frame-capture-continuous-motion");
    let optical_motion_requested = args
        .iter()
        .any(|arg| arg == "--frame-capture-refraction-flight");
    let unfocused_optical_diagnostic = args
        .iter()
        .any(|arg| arg == "--frame-capture-unfocused-optical");
    if positions.is_empty()
        && !motion_requested
        && !view_motion_requested
        && !continuous_motion_requested
        && !optical_motion_requested
        && !unfocused_optical_diagnostic
    {
        return;
    }
    let index = positions.first().copied().unwrap_or(0);
    let output = if positions.len() != 1 {
        Err("--frame-capture must occur once".into())
    } else if unfocused_optical_diagnostic && !optical_motion_requested {
        Err("--frame-capture-unfocused-optical requires --frame-capture-refraction-flight".into())
    } else if [
        motion_requested,
        view_motion_requested,
        continuous_motion_requested,
        optical_motion_requested,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count()
        > 1
    {
        Err("Choose only one frame-capture motion probe".into())
    } else if args.iter().any(|arg| arg == "--screenshot") {
        Err("--frame-capture and --screenshot are separate runs".into())
    } else {
        args.get(index + 1)
            .filter(|value| !value.is_empty() && !value.starts_with("--"))
            .map(PathBuf::from)
            .ok_or_else(|| "--frame-capture requires an output JSON path".into())
    };
    app.insert_resource(FrameCapture {
        output,
        arguments: args.to_vec(),
        samples: SampleWindow::default(),
        start_metadata: None,
        start_view: None,
        view_changed: false,
        unfocused_frames: 0,
        motion_requested,
        motion: None,
        view_motion_requested,
        view_motion: None,
        continuous_motion_requested,
        continuous_motion: None,
        optical_motion_requested,
        unfocused_optical_diagnostic,
        optical_motion: None,
        optical_motion_error: None,
        draw_visibility_samples: Vec::new(),
        done: false,
    })
    .add_systems(Update, collect);
}

#[derive(SystemParam)]
struct CaptureContext<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    explorer: ResMut<'w, Explorer>,
    overview: Res<'w, ancient_earth::orbit::OrbitOverview>,
    surface: Res<'w, WorldSurface>,
    topography: Res<'w, ancient_earth::topography::TopographyState>,
    astronomy: Res<'w, AstronomicalScene>,
    weather: Res<'w, ancient_earth::weather::WeatherState>,
    survey: Res<'w, ancient_earth::survey::SurveyState>,
    rendered: Res<'w, RenderedEarth>,
    graphics: Res<'w, super::graphics_loading::GraphicsLoading>,
    measurements: Res<'w, super::ui::MeasurementStatus>,
    pacing: Res<'w, super::frame_pacing::FramePacing>,
    adapter: Option<Res<'w, RenderAdapterInfo>>,
    optical_readback: Option<Res<'w, super::renderer::OpticalReadback>>,
}

fn optical_snapshot(window: &Window, context: &CaptureContext) -> OpticalMotionSnapshot {
    OpticalMotionSnapshot {
        optical_active: context
            .weather
            .view_profile_for_scene(
                context.explorer.observer,
                context.surface.active(context.explorer.boundaries_enabled),
            )
            .is_some(),
        optical_ready: context.rendered.optical_status
            == super::renderer::OpticalDisplayStatus::Ready,
        presentation_ready: context.rendered.presentation_ready,
        render_matches_observer: context.rendered.optical_view
            == Some((context.explorer.observer, context.explorer.lens())),
        submitted_compute_control: context.rendered.optical_submission_control,
        expected_stellar_control: context.rendered.optical_stellar_control,
        focused: window.focused,
        physical_size: [window.physical_width(), window.physical_height()],
        scale_factor: window.scale_factor(),
        profile_generation: context.weather.generation,
        surface_generation: context.surface.generation,
        marker_generation: context.survey.generation,
        visible_marker_count: if context.survey.markers_visible {
            context.survey.markers.len()
        } else {
            0
        },
    }
}

fn view_configuration(window: &Window, context: &CaptureContext) -> Value {
    let state = &context.explorer;
    let eye = state.observer.eye_position();
    let lens = state.lens();
    let mut configuration = json!({
        "physical_size": [window.physical_width(), window.physical_height()],
        "scale_factor": window.scale_factor(),
        "present_mode": format!("{:?}", window.present_mode),
        "frame_limit": context.pacing.limit.label(),
        "frame_pacing_unrestricted": context.pacing.qa_unrestricted,
        "effective_frame_limit_fps": context.pacing.effective_fps,
        "projection": format!("{:?}", lens.projection),
        "vertical_fov_degrees": lens.vertical_fov_deg,
        "eye_ecef_metres": [eye.x, eye.y, eye.z],
        "heading_degrees": state.observer.heading_deg,
        "pitch_degrees": state.observer.pitch_deg,
        "roll_degrees": state.observer.roll_deg,
        "follow_earth": state.follow_earth,
        "horizontal_guide": state.show_horizontal_line,
        "vertical_guide": state.show_vertical_line,
        "guide_column": state.guide_column,
        "boundaries_enabled": state.boundaries_enabled,
        "grass_enabled": state.grass_enabled,
        "show_ui": state.show_ui,
        "show_panel": state.show_panel,
        "show_sight_panel": state.show_sight_panel,
        "show_help_panel": state.show_help_panel,
        "show_body_info": state.show_body_info,
        "quality": state.quality,
        "renderer": context.rendered.mode,
        "orbit_annotations_visible": context.overview.visible,
        "mouse_look_captured": state.looking,
        "automatic_flight_speed": state.flight_settings.adaptive_speed,
        "navigation_speed_metres_per_second": state.flight_speed_mps,
        "orbit_path_enabled": context.overview.show_orbit,
        "orbit_axis_enabled": context.overview.show_axis,
        "weather_enabled": context.weather.enabled,
        "weather_refraction_rendered": context.rendered.optical_status == super::renderer::OpticalDisplayStatus::Ready,
        "weather_profile_generation": context.weather.generation,
        "weather_reference_snapshot": context.weather.reference_snapshot,
        "weather_valid_unix_seconds": context.weather.active_profile().and_then(|p| p.metadata.as_ref()).map(|m| m.valid_unix_seconds),
        "weather_cycle_unix_seconds": context.weather.active_profile().and_then(|p| p.metadata.as_ref()).map(|m| m.cycle_unix_seconds),
        "presentation_ready": context.rendered.presentation_ready,
        "measurements_pending": context.measurements.pending,
        "star_phase_seconds": state.star_phase_seconds,
    });
    configuration["optical_stellar_input"] = json!(context.rendered.optical_stellar_input);
    configuration["optical_submission_path"] = json!(context.rendered.optical_submission_path);
    configuration["optical_stellar_input_active"] = json!(
        context
            .rendered
            .optical_stellar_input
            .map(|value| value > 0.0)
    );
    configuration["weather_optical_status"] =
        json!(format!("{:?}", context.rendered.optical_status));
    configuration["weather_display_block_reason"] = json!(context.weather.display_block_reason);
    configuration["weather_model"] = json!(
        context
            .weather
            .active_profile()
            .and_then(|p| p.metadata.as_ref())
            .map(|m| &m.model)
    );
    configuration["horizon_lock_enabled"] = json!(state.horizon_lock_enabled);
    configuration["horizon_lock_active"] = json!(state.horizon_lock_active());
    configuration
}

fn collect(
    mut capture: ResMut<FrameCapture>,
    time: Res<Time<Real>>,
    mut context: CaptureContext,
    mut exit: MessageWriter<AppExit>,
) {
    if capture.done {
        return;
    }
    if let Err(message) = &capture.output {
        error!("Frame capture: {message}");
        capture.done = true;
        exit.write(AppExit::error());
        return;
    }
    if !capture.samples.started && time.elapsed_secs_f64() >= PREPARATION_TIMEOUT_SECONDS {
        error!("Frame capture preparation/warmup exceeded the documented 300-second watchdog");
        capture.done = true;
        exit.write(AppExit::error());
        return;
    }
    let Ok(window) = context.windows.single() else {
        error!("Frame capture requires one primary window");
        capture.done = true;
        exit.write(AppExit::error());
        return;
    };
    let ready = context.rendered.presentation_ready
        && context.graphics.ready()
        && !context.measurements.pending
        && !context.topography.loading()
        && context.topography.last_error.is_none()
        && (!context.weather.enabled || context.weather.active_profile().is_some());
    let state =
        match capture
            .samples
            .observe_scene(time.elapsed_secs_f64(), time.delta_secs_f64(), ready)
        {
            Ok(state) => state,
            Err(message) => {
                error!("Frame capture: {message}");
                capture.done = true;
                exit.write(AppExit::error());
                return;
            }
        };
    if state == SampleState::Warming {
        return;
    }
    let view = view_configuration(window, &context);
    if state == SampleState::Started {
        if capture.optical_motion_requested {
            match OpticalMotionProbe::begin(
                &context.explorer,
                &context.surface,
                optical_snapshot(window, &context),
                time.elapsed_secs_f64(),
                capture.unfocused_optical_diagnostic,
            ) {
                Ok(probe) => capture.optical_motion = Some(probe),
                Err(message) => {
                    error!("{message}");
                    capture.done = true;
                    exit.write(AppExit::error());
                    return;
                }
            }
        }
        if capture.continuous_motion_requested {
            match ContinuousMotionProbe::begin(&context.explorer, time.elapsed_secs_f64()) {
                Ok(probe) => capture.continuous_motion = Some(probe),
                Err(message) => {
                    error!("{message}");
                    capture.done = true;
                    exit.write(AppExit::error());
                    return;
                }
            }
        }
        if capture.view_motion_requested {
            match ViewMotionProbe::begin(&context.explorer, time.elapsed_secs_f64()) {
                Ok(probe) => capture.view_motion = Some(probe),
                Err(message) => {
                    error!("{message}");
                    capture.done = true;
                    exit.write(AppExit::error());
                    return;
                }
            }
        }
        if capture.motion_requested {
            match MotionProbe::begin(
                &context.explorer,
                context.rendered.grass_qa_snapshot(),
                time.elapsed_secs_f64(),
            ) {
                Ok(probe) => capture.motion = Some(probe),
                Err(message) => {
                    error!("{message}");
                    capture.done = true;
                    exit.write(AppExit::error());
                    return;
                }
            }
        }
        capture.start_view = Some(view.clone());
        capture.start_metadata = Some(json!({
            "graphics_loading": context.graphics.metadata(),
            "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "capture_mode": if capture.optical_motion_requested { "refraction-flight-qa" } else if capture.continuous_motion_requested { "continuous-flight-qa" } else if capture.view_motion_requested { "nonstationary-view-qa" } else if capture.motion_requested { "nonstationary-motion-qa" } else { "stationary" },
            "arguments": capture.arguments,
            "view": view,
            // Frame identity is evidence, not a change in physical view.
            "optical_stellar_control_at_recording_start": context.rendered.optical_stellar_control,
            "earth_draw_visibility_at_start": [context.rendered.draw_chunks, context.rendered.view_visible_chunks, context.rendered.view_visible_triangles],
            "clock_calendar": context.astronomy.clock.calendar_string(),
            "clock_running": context.astronomy.clock.running,
            "clock_rate": context.astronomy.clock.rate,
            "warmup_frames_observed": capture.samples.frames,
            "warmup_elapsed_seconds": time.elapsed_secs_f64()
                - capture.samples.ready_since.expect("ready before recording"),
            "application_elapsed_seconds_at_recording": time.elapsed_secs_f64(),
            // Wall-clock observation of this main-world boundary, not the
            // pipelined render timestamp behind Time<Real>. External counters
            // should use a conservative interior window when aligning clocks.
            "recording_boundary_observed_unix_seconds": SystemTime::now()
                .duration_since(UNIX_EPOCH).ok().map(|value| value.as_secs_f64()),
            "adapter": context.adapter.as_ref().map(|info| json!({
                "name": info.name,
                "backend": format!("{:?}", info.backend),
                "device_type": format!("{:?}", info.device_type),
                "driver": info.driver,
                "driver_info": info.driver_info,
            })),
        }));
        info!("Frame capture warmup complete; recording application intervals");
        return;
    }
    capture.view_changed |=
        capture.start_view.as_ref() != Some(&view) || !context.rendered.presentation_ready;
    capture.unfocused_frames += u64::from(!window.focused);
    capture.draw_visibility_samples.push([
        context.rendered.draw_chunks,
        context.rendered.view_visible_chunks,
        context.rendered.view_visible_triangles,
    ]);
    let motion_complete = if let Some(probe) = &mut capture.motion {
        match probe.step(
            &mut context.explorer,
            &context.surface,
            context.rendered.grass_qa_snapshot(),
            context.rendered.presentation_ready,
            time.elapsed_secs_f64(),
            time.delta_secs_f64(),
        ) {
            Ok(complete) => complete,
            Err(message) => {
                error!("{message}");
                capture.done = true;
                exit.write(AppExit::error());
                return;
            }
        }
    } else {
        true
    };
    let view_motion_complete = if let Some(probe) = &mut capture.view_motion {
        match probe.step(
            &mut context.explorer,
            &context.surface,
            context.rendered.presentation_ready,
            context.rendered.render_ms,
            context.rendered.mesh_qa_snapshot(),
            time.elapsed_secs_f64(),
            time.delta_secs_f64(),
        ) {
            Ok(complete) => complete,
            Err(message) => {
                error!("{message}");
                capture.done = true;
                exit.write(AppExit::error());
                return;
            }
        }
    } else {
        true
    };
    let continuous_motion_complete = if let Some(probe) = &mut capture.continuous_motion {
        let snapshot = context
            .rendered
            .continuous_qa_snapshot(context.explorer.observer);
        match probe.step(
            &mut context.explorer,
            &context.surface,
            context.astronomy.frame.as_ref(),
            snapshot,
            context.rendered.presentation_ready,
            time.elapsed_secs_f64(),
            time.delta_secs_f64(),
        ) {
            Ok(complete) => complete,
            Err(message) => {
                error!("{message}");
                capture.done = true;
                exit.write(AppExit::error());
                return;
            }
        }
    } else {
        true
    };
    let optical_motion_complete = if let Some(probe) = &mut capture.optical_motion {
        let snapshot = optical_snapshot(window, &context);
        match probe.step(
            &mut context.explorer,
            &context.surface,
            context.astronomy.frame.as_ref(),
            snapshot,
            time.elapsed_secs_f64(),
            time.delta_secs_f64(),
        ) {
            Ok(complete) => complete,
            Err(message) => {
                error!("{message}");
                // Keep the failed route and every recorded interval available
                // for review; a failure must not erase the timing evidence.
                capture.optical_motion_error = Some(message.to_owned());
                true
            }
        }
    } else {
        true
    };
    if capture.optical_motion_error.is_none()
        && (state != SampleState::Complete
            || !motion_complete
            || !view_motion_complete
            || !continuous_motion_complete
            || !optical_motion_complete)
    {
        return;
    }
    let mut report = json!({
        "format": "ancient-earth-frame-capture-v1",
        "measurement": "raw application frame intervals, not GPU execution time",
        "warmup_minimum_frames": WARMUP_FRAMES,
        "warmup_minimum_seconds": WARMUP_SECONDS,
        "requested_recording_seconds": RECORD_SECONDS,
        "preparation_watchdog_seconds": PREPARATION_TIMEOUT_SECONDS,
        "metadata": capture.start_metadata,
        "final_view": view,
        "optical_stellar_control_at_recording_end": context.rendered.optical_stellar_control,
        "final_clock_calendar": context.astronomy.clock.calendar_string(),
        "view_changed_during_capture": capture.view_changed,
        "incomplete_scene_frames": capture.samples.incomplete_scene_frames,
        "unfocused_recorded_frames": capture.unfocused_frames,
        "sample_count": capture.samples.intervals.len(),
        "recorded_duration_seconds": capture.samples.recorded_seconds,
        "completion_boundary_observed_unix_seconds": SystemTime::now()
            .duration_since(UNIX_EPOCH).ok().map(|value| value.as_secs_f64()),
        "interval_seconds": capture.samples.intervals,
        "earth_draw_visibility_columns": ["prepared_chunks", "previous_bevy_view_visible_chunks", "previous_bevy_view_visible_faces"],
        "earth_draw_visibility_scope": "Bevy visibility eligibility; not hardware draw submissions or GPU timings",
        "earth_draw_visibility_samples": capture.draw_visibility_samples,
        "motion_qa": capture.motion.as_ref().map(MotionProbe::report),
        "view_motion_qa": capture.view_motion.as_ref().map(ViewMotionProbe::report),
        "continuous_motion_qa": capture.continuous_motion.as_ref().map(ContinuousMotionProbe::report),
        "optical_motion_qa": capture.optical_motion.as_ref().map(OpticalMotionProbe::report),
        "optical_motion_error": capture.optical_motion_error,
    });
    report["optical_compute_readback"] = context
        .optical_readback
        .as_ref()
        .map_or(Value::Null, |audit| audit.report());
    let output = capture.output.as_ref().expect("validated output path");
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(output)?);
        serde_json::to_writer_pretty(&mut file, &report)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            info!("Saved raw frame capture to {}", output.display());
            exit.write(if capture.optical_motion_error.is_some() {
                AppExit::error()
            } else {
                AppExit::Success
            });
        }
        Err(error) => {
            error!(
                "Could not write frame capture {}: {error}",
                output.display()
            );
            exit.write(AppExit::error());
        }
    }
    capture.done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_requires_both_warmup_thresholds_and_excludes_boundary_interval() {
        let mut samples = SampleWindow::default();
        for _ in 0..119 {
            assert_eq!(samples.observe(20.0, 3.0), Ok(SampleState::Warming));
        }
        assert_eq!(samples.observe(20.1, 3.0), Ok(SampleState::Started));
        assert!(samples.intervals.is_empty());
        assert_eq!(samples.observe(20.12, 0.02), Ok(SampleState::Recording));
        assert_eq!(samples.intervals, [0.02]);

        let mut fast = SampleWindow::default();
        for _ in 0..150 {
            assert_eq!(fast.observe(5.0, 0.01), Ok(SampleState::Warming));
        }
        assert_eq!(fast.observe(10.0, 5.0), Ok(SampleState::Started));
        assert!(fast.intervals.is_empty());
    }

    #[test]
    fn capture_keeps_complete_intervals_and_rejects_invalid_recorded_values() {
        let mut samples = SampleWindow {
            started: true,
            ..default()
        };
        assert_eq!(samples.observe(11.0, 7.0), Ok(SampleState::Recording));
        assert_eq!(samples.observe(18.0, 7.0), Ok(SampleState::Recording));
        assert_eq!(samples.observe(25.0, 7.0), Ok(SampleState::Complete));
        assert_eq!(samples.intervals, [7.0, 7.0, 7.0]);
        assert_eq!(samples.recorded_seconds, 21.0);
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(samples.observe(26.0, invalid).is_err());
            assert_eq!(samples.intervals.len(), 3);
        }
    }

    #[test]
    fn preparation_resets_warmup_but_never_drops_intervals_after_recording_starts() {
        let mut samples = SampleWindow::default();
        for _ in 0..200 {
            assert_eq!(
                samples.observe_scene(500.0, 3.0, false),
                Ok(SampleState::Warming)
            );
        }
        for index in 0..120 {
            assert_eq!(
                samples.observe_scene(501.0 + f64::from(index) * 0.01, 0.01, true),
                Ok(SampleState::Warming)
            );
        }
        assert_eq!(
            samples.observe_scene(510.0, 0.01, false),
            Ok(SampleState::Warming)
        );
        assert_eq!(samples.frames, 0);
        for index in 0..120 {
            assert_eq!(
                samples.observe_scene(511.0 + f64::from(index) * 0.01, 0.01, true),
                Ok(SampleState::Warming)
            );
        }
        assert_eq!(
            samples.observe_scene(521.0, 8.81, true),
            Ok(SampleState::Started)
        );
        assert!(samples.intervals.is_empty());
        assert_eq!(
            samples.observe_scene(526.0, 5.0, false),
            Ok(SampleState::Recording)
        );
        assert_eq!(
            samples.observe_scene(526.02, 0.02, true),
            Ok(SampleState::Recording)
        );
        assert_eq!(samples.intervals, [5.0, 0.02]);
        assert_eq!(samples.incomplete_scene_frames, 1);
    }
}
