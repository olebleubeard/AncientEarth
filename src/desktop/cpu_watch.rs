//! Opt-in elapsed-section diagnostics for native frame captures. These include
//! blocking and scheduling; they are not CPU busy time or GPU execution time.
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{CachedPipelineState, PipelineCache},
    },
};
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

pub(super) struct Section {
    name: &'static str,
    started: Option<Instant>,
}

impl Section {
    pub(super) fn start(name: &'static str) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| std::env::args().any(|arg| arg == "--frame-capture"));
        Self {
            name,
            started: enabled.then(Instant::now),
        }
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = started.elapsed();
            if elapsed >= Duration::from_millis(100) {
                bevy::log::warn!(section = self.name, ?elapsed, "Slow CPU section");
            }
        }
    }
}

#[derive(Resource, Default)]
struct RenderTimer(Option<Section>);

#[derive(Resource)]
struct PipelineProbe(bool);

pub(super) fn install(app: &mut App, args: &[String]) {
    if !args.iter().any(|arg| arg == "--frame-capture") {
        return;
    }
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app.insert_resource(PipelineProbe(
        args.iter().any(|arg| arg == "--diagnose-pipelines"),
    ));
    render_app.init_resource::<RenderTimer>().add_systems(
        Render,
        (
            begin_render
                .after(RenderSystems::Prepare)
                .before(RenderSystems::Render),
            end_render
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
        ),
    );
}

fn begin_render(
    mut timer: ResMut<RenderTimer>,
    mut pipelines: ResMut<PipelineCache>,
    probe: Res<PipelineProbe>,
) {
    timer.0 = Some(Section::start("render stage: pipelines and render graph"));
    if !probe.0 {
        return;
    }
    let count = pipelines.pipelines().count();
    let pending: Vec<_> = pipelines
        .pipelines()
        .enumerate()
        .filter_map(|(index, pipeline)| {
            (!matches!(pipeline.state, CachedPipelineState::Ok(_))).then_some(index)
        })
        .collect();
    let started = Instant::now();
    // Diagnostic-only: the public API permits processing here. The normal
    // Render-stage call follows and sees the same already-processed queue.
    pipelines.process_queue();
    let elapsed = started.elapsed();
    if elapsed >= Duration::from_millis(100) {
        warn!(?elapsed, "Slow pipeline queue processing");
        for (index, pipeline) in pipelines.pipelines().enumerate() {
            if index >= count || pending.contains(&index) {
                warn!(index, descriptor = ?pipeline.descriptor, state = ?pipeline.state,
                    "Pipeline considered during slow queue processing");
            }
        }
    }
}

fn end_render(mut timer: ResMut<RenderTimer>) {
    timer.0.take();
}
