//! Explicit application pacing; real elapsed time and astronomical time remain intact.
use bevy::{ecs::system::NonSendMarker, prelude::*, time::TimeSystems, window::PrimaryWindow};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameLimit {
    Fps30,
    #[default]
    Fps60,
    Fps120,
    Unlimited,
}
impl FrameLimit {
    pub const ALL: [Self; 4] = [Self::Fps30, Self::Fps60, Self::Fps120, Self::Unlimited];
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "30" => Some(Self::Fps30),
            "60" => Some(Self::Fps60),
            "120" => Some(Self::Fps120),
            "unlimited" => Some(Self::Unlimited),
            _ => None,
        }
    }
    pub fn fps(self) -> Option<u32> {
        match self {
            Self::Fps30 => Some(30),
            Self::Fps60 => Some(60),
            Self::Fps120 => Some(120),
            Self::Unlimited => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Fps30 => "30 FPS",
            Self::Fps60 => "60 FPS",
            Self::Fps120 => "120 FPS",
            Self::Unlimited => "Unlimited",
        }
    }
}

#[derive(Resource, Default)]
pub struct FramePacing {
    pub limit: FrameLimit,
    /// Opt-in measurement control, never enabled by the ordinary GUI.
    pub qa_unrestricted: bool,
    pub effective_fps: Option<u32>,
    /// Main-thread window observation, shared with expensive render submissions.
    pub window_minimized: bool,
    pub last_sleep: Duration,
    cadence: Cadence,
}

pub fn effective_rate(
    limit: FrameLimit,
    focused: bool,
    minimized: bool,
    unrestricted: bool,
) -> Option<u32> {
    if unrestricted {
        return None;
    }
    let background = if minimized {
        Some(5)
    } else if !focused {
        Some(15)
    } else {
        None
    };
    match (limit.fps(), background) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

#[derive(Default)]
struct Cadence {
    last_start: Option<Instant>,
}
impl Cadence {
    fn delay(&self, now: Instant, rate: Option<u32>) -> Duration {
        let Some(rate) = rate.filter(|&rate| rate > 0) else {
            return Duration::ZERO;
        };
        // Round upward to an integral nanosecond; never deliberately request a
        // frame earlier than the selected period. OS wakeup may be later.
        let period = Duration::from_nanos(1_000_000_000_u64.div_ceil(u64::from(rate)));
        self.last_start
            .and_then(|last| last.checked_add(period))
            .and_then(|deadline| deadline.checked_duration_since(now))
            .unwrap_or(Duration::ZERO)
    }
    fn started(&mut self, actual_start: Instant) {
        self.last_start = Some(actual_start);
    }
}

pub fn install(app: &mut App, args: &[String]) -> Result<(), String> {
    let mut pacing = FramePacing::default();
    for (index, arg) in args.iter().enumerate() {
        if arg == "--frame-limit" {
            pacing.limit = args
                .get(index + 1)
                .and_then(|value| FrameLimit::parse(value))
                .ok_or("--frame-limit requires 30, 60, 120 or unlimited")?;
        }
    }
    pacing.qa_unrestricted = args.iter().any(|arg| arg == "--frame-pacing-unrestricted");
    app.insert_resource(pacing)
        .add_systems(First, pace.before(TimeSystems));
    Ok(())
}

pub(super) fn pace(
    _main_thread: NonSendMarker,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut pacing: ResMut<FramePacing>,
) {
    let Ok((entity, window)) = windows.single() else {
        return;
    };
    let minimized = !window.visible
        || window.physical_width() == 0
        || window.physical_height() == 0
        || bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
            windows
                .get_window(entity)
                .and_then(|window| window.is_minimized())
                .unwrap_or(false)
        });
    pacing.window_minimized = minimized;
    pacing.effective_fps = effective_rate(
        pacing.limit,
        window.focused,
        minimized,
        pacing.qa_unrestricted,
    );
    let delay = pacing.cadence.delay(Instant::now(), pacing.effective_fps);
    let before = Instant::now();
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    let actual = Instant::now();
    pacing.last_sleep = actual.duration_since(before);
    // Anchor to actual starts, not an accumulating deadline: slow frames cause
    // no catch-up burst. TimeSystems keeps its existing real-clock source
    // (possibly a preceding render timestamp); no elapsed interval is clamped.
    pacing.cadence.started(actual);
}

#[cfg(test)]
mod review;
