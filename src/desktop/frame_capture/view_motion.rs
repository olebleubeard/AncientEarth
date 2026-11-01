//! Opt-in zoom/ascent timing probe. Command time and wall time remain distinct.
use super::super::Explorer;
use ancient_earth::surface::WorldSurface;
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    ZoomIn,
    ZoomOut,
    Ascend,
    Recover,
    Complete,
}
impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::Ascend => "ascent",
            Self::Recover => "recovery",
            Self::Complete => "complete",
        }
    }
}
pub struct ViewMotionProbe {
    stage: Stage,
    previous_action: &'static str,
    started: f64,
    command_seconds: f64,
    initialized: bool,
    ready_since: Option<f64>,
    original_speed: f64,
    initial_fov: f64,
    initial_height: f64,
    frames: Vec<Value>,
    events: Vec<Value>,
}
impl ViewMotionProbe {
    pub fn begin(state: &Explorer, elapsed: f64) -> Result<Self, &'static str> {
        if !elapsed.is_finite() || !(45.0..=90.0).contains(&state.lens().vertical_fov_deg) {
            return Err("view-motion QA requires a 45–90 degree initial field and finite time");
        }
        Ok(Self {
            stage: Stage::ZoomIn,
            previous_action: "warmup_boundary",
            started: elapsed,
            command_seconds: 0.0,
            initialized: false,
            ready_since: None,
            original_speed: state.observer.speed_mps,
            initial_fov: state.lens().vertical_fov_deg,
            initial_height: state.observer.eye_geodetic().height_m,
            frames: Vec::new(),
            events: Vec::new(),
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        state: &mut Explorer,
        surface: &WorldSurface,
        ready: bool,
        render_ms: u128,
        mesh_counts: [u64; 5],
        elapsed: f64,
        interval: f64,
    ) -> Result<bool, &'static str> {
        if !elapsed.is_finite() || !interval.is_finite() || interval <= 0.0 {
            return Err("view-motion QA requires finite positive frame intervals");
        }
        if elapsed - self.started > 180.0 {
            return Err("view-motion QA exceeded its 180 second watchdog");
        }
        self.frames.push(json!({"phase":self.previous_action,"interval_seconds":interval,
            "elapsed_recording_seconds":elapsed-self.started,"vertical_fov_degrees":state.lens().vertical_fov_deg,
            "eye_height_metres":state.observer.eye_geodetic().height_m,"presentation_ready":ready,
            "renderer_cpu_milliseconds":render_ms,"mesh_counts":mesh_counts}));
        if !self.initialized {
            state.telescope_lens = state.lens();
            state.telescope = true;
            self.initialized = true;
        }
        self.previous_action = self.stage.label();
        match self.stage {
            Stage::ZoomIn | Stage::ZoomOut | Stage::Ascend => {
                let dt = interval.min(0.25).min(4.0 - self.command_seconds);
                if self.stage == Stage::Ascend {
                    state.observer.speed_mps = 250.0;
                    surface.active(state.boundaries_enabled).fly(
                        &mut state.observer,
                        0.0,
                        0.0,
                        1.0,
                        dt,
                        false,
                    );
                } else {
                    state.telescope_lens.zoom(if self.stage == Stage::ZoomIn {
                        4.0 * dt
                    } else {
                        -4.0 * dt
                    });
                }
                self.command_seconds += dt;
                if self.command_seconds >= 4.0 {
                    self.events.push(json!({"finished_phase":self.stage.label(),"elapsed_recording_seconds":elapsed-self.started,
                        "command_seconds":self.command_seconds,"vertical_fov_degrees":state.lens().vertical_fov_deg,
                        "eye_height_metres":state.observer.eye_geodetic().height_m}));
                    self.stage = match self.stage {
                        Stage::ZoomIn => Stage::ZoomOut,
                        Stage::ZoomOut => Stage::Ascend,
                        _ => Stage::Recover,
                    };
                    self.command_seconds = 0.0;
                }
            }
            Stage::Recover => {
                if ready {
                    let since = *self.ready_since.get_or_insert(elapsed);
                    if elapsed - since >= 3.0 {
                        state.observer.speed_mps = self.original_speed;
                        self.stage = Stage::Complete;
                    }
                } else {
                    self.ready_since = None;
                }
            }
            Stage::Complete => {}
        }
        Ok(self.stage == Stage::Complete)
    }
    pub fn report(&self) -> Value {
        let summaries:[Value;6]=["warmup_boundary","zoom_in","zoom_out","ascent","recovery","complete"].map(|phase|{
            let mut samples:Vec<f64>=self.frames.iter().filter(|v|v["phase"]==phase).map(|v|v["interval_seconds"].as_f64().unwrap()).collect();
            samples.sort_by(f64::total_cmp);
            let n=samples.len();let sum:f64=samples.iter().sum();
            let percentile=|p:f64|if n==0{None}else{Some(samples[((n as f64*p).ceil()as usize).saturating_sub(1).min(n-1)])};
            json!({"phase":phase,"count":n,"sum_seconds":sum,"median_nearest_rank_seconds":percentile(0.5),
                "p95_nearest_rank_seconds":percentile(0.95),"p99_nearest_rank_seconds":percentile(0.99),
                "maximum_seconds":samples.last().copied()})
        });
        json!({"passed":self.stage==Stage::Complete,"initial_fov_degrees":self.initial_fov,"initial_eye_height_metres":self.initial_height,
            "passed_definition":"command sequence completed and presentation remained ready for three seconds; actual attained poses are recorded separately",
            "final_fov_degrees":self.frames.last().map(|frame| &frame["vertical_fov_degrees"]),
            "final_eye_height_metres":self.frames.last().map(|frame| &frame["eye_height_metres"]),
            "scope":"nonstationary repeated zoom/ascent commands; actual frame intervals, not GPU time or stationary FPS",
            "command_policy":"4 s zoom in/out at 4 wheel-lines/s, 4 s ascent at 250 m/s; command dt capped at 0.25 s; 3 s ready recovery",
            "mesh_count_order":["requested","completed","committed","discarded","inflight"],
            "events":self.events,"phase_summaries":summaries,"frames":self.frames})
    }
}
