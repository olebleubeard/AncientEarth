//! Observe Bevy's pipeline queue without blocking or manually compiling it.
use super::renderer::RenderedEarth;
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor},
    },
    shader::ShaderCacheError,
};
use dear_imgui_bevy::ImguiFrame;
use dear_imgui_rs::{Condition, WindowFlags};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
struct Report {
    generation: u64,
    total: usize,
    pending: usize,
    overlay_ready: bool,
    error: Option<String>,
}

#[derive(Resource, Clone, Default)]
struct Mailbox(Arc<Mutex<Report>>);

#[derive(Resource, Default)]
pub(super) struct GraphicsLoading {
    report: Report,
    settled_reports: u8,
    revealed: bool,
    pending_render_reports: u64,
    overlay_ready_reports: u8,
}

impl GraphicsLoading {
    pub(super) fn startup_blocked(&self) -> bool {
        !self.revealed
    }

    pub(super) fn ready(&self) -> bool {
        self.revealed && self.settled_reports >= 3 && self.report.error.is_none()
    }

    pub(super) fn pending_capture_ready(&self) -> bool {
        self.startup_blocked()
            && self.report.pending > 0
            && self.pending_render_reports >= 20
            && self.overlay_ready_reports >= 3
    }

    fn observe(&mut self, report: Report, scene_ready: bool) -> bool {
        if !scene_ready {
            self.settled_reports = 0;
        }
        // Main and render worlds can advance at different rates. Seeing one
        // successful report repeatedly is not evidence of multiple ready frames.
        if report.generation == self.report.generation {
            return false;
        }
        if !self.revealed && report.pending > 0 {
            self.pending_render_reports += 1;
        }
        self.overlay_ready_reports = if report.overlay_ready {
            self.overlay_ready_reports.saturating_add(1)
        } else {
            0
        };
        let ready = scene_ready
            && report.overlay_ready
            && report.total > 0
            && report.pending == 0
            && report.error.is_none();
        self.settled_reports = if ready {
            if report.total == self.report.total {
                self.settled_reports.saturating_add(1)
            } else {
                1
            }
        } else {
            0
        };
        self.report = report;
        if !self.revealed && self.settled_reports >= 3 {
            self.revealed = true;
            return true;
        }
        false
    }

    pub(super) fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "ready": self.ready(),
            "startup_revealed": self.revealed,
            "completed_render_reports": self.report.generation,
            "pending_render_reports_during_loading": self.pending_render_reports,
            "pipelines": self.report.total,
            "pending_pipelines": self.report.pending,
            "overlay_ready": self.report.overlay_ready,
            "error": self.report.error,
        })
    }
}

pub(super) fn install(app: &mut App) {
    let mailbox = Mailbox::default();
    app.insert_resource(mailbox.clone())
        .init_resource::<GraphicsLoading>()
        .add_systems(PreUpdate, receive);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.insert_resource(mailbox).add_systems(
            Render,
            publish
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
        );
    }
}

fn publish(pipelines: Res<PipelineCache>, mailbox: Res<Mailbox>) {
    let mut report = Report::default();
    let mut overlay_seen = false;
    let mut overlays_ready = true;
    for pipeline in pipelines.pipelines() {
        report.total += 1;
        // The pinned Dear ImGui backend exposes this descriptor label. Require
        // every cached format variant so native loading QA cannot capture the
        // black interval before the UI's own shader is available.
        if let PipelineDescriptor::RenderPipelineDescriptor(descriptor) = &pipeline.descriptor
            && descriptor.label.as_deref() == Some("dear_imgui_bevy_pipeline")
        {
            overlay_seen = true;
            overlays_ready &= matches!(pipeline.state, CachedPipelineState::Ok(_));
        }
        match &pipeline.state {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Queued | CachedPipelineState::Creating(_) => report.pending += 1,
            CachedPipelineState::Err(
                ShaderCacheError::ShaderNotLoaded(_)
                | ShaderCacheError::ShaderImportNotYetAvailable,
            ) => report.pending += 1,
            CachedPipelineState::Err(error) => {
                if report.error.is_none() {
                    report.error = Some(error.to_string());
                }
            }
        }
    }
    report.overlay_ready = overlay_seen && overlays_ready;
    if let Ok(mut shared) = mailbox.0.lock() {
        report.generation = shared.generation + 1;
        *shared = report;
    }
}

fn receive(
    mailbox: Res<Mailbox>,
    mut loading: ResMut<GraphicsLoading>,
    scene: Res<RenderedEarth>,
    time: Res<Time<Real>>,
) {
    let Ok(report) = mailbox.0.lock().map(|report| report.clone()) else {
        return;
    };
    if loading.observe(report, scene.presentation_ready) {
        info!(
            elapsed_seconds = time.elapsed_secs_f64(),
            pending_render_reports = loading.pending_render_reports,
            pipelines = loading.report.total,
            "Graphics startup ready"
        );
    }
}

pub(super) fn draw(
    frame: ImguiFrame<'_>,
    loading: Res<GraphicsLoading>,
    scene: Res<RenderedEarth>,
    mut weather: ResMut<ancient_earth::weather::WeatherState>,
    mut exit: MessageWriter<AppExit>,
) {
    if !loading.startup_blocked() && loading.report.pending == 0 && loading.report.error.is_none() {
        return;
    }
    let ui = frame.ui();
    let startup = loading.startup_blocked();
    if startup {
        ui.get_background_draw_list()
            .add_rect(
                [0.0, 0.0],
                ui.io().display_size(),
                [0.015, 0.025, 0.05, 1.0],
            )
            .filled(true)
            .build();
    }
    let position = if startup {
        earth_math::panel_anchor(
            ui.io().display_size(),
            [540.0, 145.0],
            [0.5, 0.5],
            [0.0, 0.0],
        )
    } else {
        [18.0, 18.0]
    };
    ui.window("Preparing graphics")
        .position(position, Condition::Always)
        .size([540.0, 0.0], Condition::Always)
        .flags(
            WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::ALWAYS_AUTO_RESIZE,
        )
        .build(|| {
            let optical_failure = match &scene.optical_status {
                super::renderer::OpticalDisplayStatus::Unsupported(error)
                | super::renderer::OpticalDisplayStatus::Error(error) => Some(error),
                _ => None,
            };
            if let Some(error) = optical_failure {
                ui.text("Curved-ray visibility could not be prepared.");
                ui.text_wrapped(error);
                if ui.button("Disable refraction and continue") {
                    weather.set_enabled(false);
                }
            } else if let Some(error) = &loading.report.error {
                ui.text("Graphics could not be prepared.");
                ui.text_wrapped(error);
            } else if scene.optical_status == super::renderer::OpticalDisplayStatus::Pending {
                ui.text("Preparing terrain for curved-ray visibility. Please wait.");
            } else if startup {
                ui.text("Preparing Earth and graphics. Please wait.");
                ui.text("The first launch can take longer while shaders compile.");
            } else {
                ui.text("Preparing this view...");
            }
            if loading.report.pending > 0 {
                ui.text(format!(
                    "{} graphics tasks remaining",
                    loading.report.pending
                ));
            }
            if (startup || loading.report.error.is_some()) && ui.button("Close app") {
                exit.write(AppExit::Success);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(generation: u64, pending: usize) -> Report {
        Report {
            generation,
            total: 8,
            pending,
            overlay_ready: true,
            error: None,
        }
    }
    #[test]
    fn requires_distinct_complete_render_reports() {
        let mut loading = GraphicsLoading::default();
        assert!(!loading.observe(report(1, 0), true));
        for _ in 0..10 {
            assert!(!loading.observe(report(1, 0), true));
        }
        assert!(loading.startup_blocked());
        assert!(!loading.observe(report(2, 0), true));
        assert!(loading.observe(report(3, 0), true));
        assert!(loading.ready());
    }
    #[test]
    fn scene_pending_or_new_pipeline_restarts_settling() {
        let mut loading = GraphicsLoading::default();
        loading.observe(report(1, 0), true);
        loading.observe(report(2, 0), false);
        loading.observe(report(3, 0), true);
        let mut changed = report(4, 0);
        changed.total = 9;
        loading.observe(changed.clone(), true);
        assert_eq!(loading.settled_reports, 1);
        changed.generation = 5;
        loading.observe(changed.clone(), true);
        changed.generation = 6;
        assert!(loading.observe(changed, true));
    }
    #[test]
    fn empty_cache_pending_and_errors_never_reveal() {
        let mut loading = GraphicsLoading::default();
        for generation in 1..=10 {
            loading.observe(
                Report {
                    generation,
                    ..default()
                },
                true,
            );
        }
        assert!(loading.startup_blocked());
        for generation in 11..=20 {
            loading.observe(report(generation, 1), true);
        }
        assert!(loading.startup_blocked());
        for generation in 21..=30 {
            let mut error = report(generation, 0);
            error.error = Some("shader failure".into());
            loading.observe(error, true);
        }
        assert!(loading.startup_blocked());
        assert!(!loading.ready());
    }
    #[test]
    fn later_pipeline_work_blocks_capture_without_recovering_startup() {
        let mut loading = GraphicsLoading::default();
        for generation in 1..=3 {
            loading.observe(report(generation, 0), true);
        }
        loading.observe(report(4, 1), true);
        assert!(!loading.startup_blocked());
        assert!(!loading.ready());
        for generation in 5..=7 {
            loading.observe(report(generation, 0), true);
        }
        assert!(loading.ready());
    }

    #[test]
    fn loading_capture_waits_for_the_ui_shader() {
        let mut loading = GraphicsLoading::default();
        for generation in 1..=30 {
            let mut pending = report(generation, 2);
            pending.overlay_ready = false;
            loading.observe(pending, true);
        }
        assert!(!loading.pending_capture_ready());
        for generation in 31..=33 {
            loading.observe(report(generation, 1), true);
        }
        assert!(loading.pending_capture_ready());
        assert!(loading.startup_blocked());
    }
}
