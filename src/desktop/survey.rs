//! Background survey preparation and current-dot selection.
use super::ui::jobs::SnapshotJob;
use ancient_earth::{
    navigation::Explorer,
    surface::WorldSurface,
    survey::{SurveyState, TapeAction},
    weather::WeatherState,
};
use bevy::prelude::*;
use dear_imgui_bevy::ImguiFrame;
use earth_math::{
    Observer, Vec3d,
    atmosphere::OpticalTrace,
    survey::{self, MarkerOcclusion, SurveyMarker, SurveyTarget},
};
use std::sync::Arc;

type PlacementResult = Result<Arc<Vec<SurveyMarker>>, String>;
type OcclusionKey = (Vec3d, usize, u64, u64, u64, bool);
type PickKey = (Observer, u64, u64, u64, bool, bool);
type SightResult = Result<OpticalTrace, String>;

#[derive(Resource, Default)]
pub(super) struct OpticalSight {
    pub result: Option<SightResult>,
    pub pending: bool,
}

#[derive(Resource)]
pub(super) struct TerrainSurveyInspection(pub bool);

#[derive(Default)]
pub(super) struct SurveyJobs {
    placement: SnapshotJob<(u64, u64), PlacementResult>,
    occlusion: SnapshotJob<OcclusionKey, Result<MarkerOcclusion, String>>,
    surface_generation: Option<u64>,
    pick_key: Option<PickKey>,
    optical: SnapshotJob<PickKey, SightResult>,
    pending_tape: Option<(PickKey, TapeAction)>,
}

#[allow(clippy::too_many_arguments)] // Bevy supplies independent survey and scene resources.
pub(super) fn update(
    _frame: ImguiFrame<'_>,
    mut explorer: ResMut<Explorer>,
    surface: Res<WorldSurface>,
    mut state: ResMut<SurveyState>,
    weather: Res<WeatherState>,
    mut sight: ResMut<OpticalSight>,
    mut inspection: ResMut<TerrainSurveyInspection>,
    mut jobs: Local<SurveyJobs>,
) {
    let _timing = super::cpu_watch::Section::start("survey");
    if jobs
        .surface_generation
        .is_some_and(|generation| generation != surface.generation)
    {
        state.surface_changed();
        jobs.placement.clear();
        jobs.occlusion.clear();
        jobs.pick_key = None;
        jobs.optical.clear();
        jobs.pending_tape = None;
    }
    jobs.surface_generation = Some(surface.generation);

    if inspection.0
        && !surface
            .active(explorer.boundaries_enabled)
            .is_boundary_only()
    {
        inspection.0 = false;
        match survey::nearby_demo(surface.active(explorer.boundaries_enabled)) {
            Ok(demo) => {
                explorer.observer = demo.observer;
                explorer.normal_lens = demo.lens;
                explorer.latitude = demo.observer.feet.lat_deg;
                explorer.longitude = demo.observer.feet.lon_deg;
                explorer.altitude_m = demo.observer.feet.height_m;
                state.accept_markers(Arc::new(demo.markers));
                state.set_tape_enabled(true);
                state.accept_tape_point(TapeAction::SetA, demo.tape_a);
                state.accept_tape_point(TapeAction::SetB, demo.tape_b);
            }
            Err(error) => state.last_error = Some(format!("Terrain survey inspection: {error}")),
        }
    }

    if let Some(request) = state.placement {
        let key = (request.id, surface.generation);
        let result = jobs
            .placement
            .update(key, || {
                let scene = surface.shared(explorer.boundaries_enabled);
                move |cancelled| {
                    survey::place_markers(&scene, request.observer, request.settings, &cancelled)
                        .map(Arc::new)
                        .map_err(|error| error.to_string())
                }
            })
            .cloned();
        if let Some(result) = result {
            match result {
                Ok(markers) => state.accept_markers(markers),
                Err(error) => {
                    state.last_error = Some(error);
                    state.placement = None;
                }
            }
            jobs.placement.clear();
        }
    } else {
        jobs.placement.clear();
    }

    let scene = surface.active(explorer.boundaries_enabled);
    let ray = explorer.observer.frame().center_ray();
    let profile = weather.view_profile_for_scene(explorer.observer, scene);
    let key = (
        explorer.observer,
        state.generation,
        surface.generation,
        weather.generation,
        state.markers_visible,
        explorer.boundaries_enabled,
    );
    let needs_sight = (explorer.show_ui && explorer.show_sight_panel)
        || (state.markers_visible && !state.markers.is_empty())
        || state.tape_action.is_some()
        || jobs.pending_tape.is_some();
    if let Some(profile) = profile.filter(|_| needs_sight) {
        sight.result = jobs
            .optical
            .update(key, || {
                let profile = profile.clone();
                let scene = surface.shared(explorer.boundaries_enabled);
                let markers = if state.markers_visible {
                    Arc::clone(&state.markers)
                } else {
                    Arc::default()
                };
                move |cancelled| {
                    profile
                        .trace_scene_cancellable(ray, &scene, &markers, &cancelled)
                        .map_err(|error| error.to_string())
                }
            })
            .cloned();
        sight.pending = sight.result.is_none();
        state.hover = sight
            .result
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(OpticalTrace::marker_pick);
        jobs.pick_key = None;
    } else {
        jobs.optical.clear();
        sight.result = None;
        sight.pending = false;
        if profile.is_some() {
            state.hover = None;
        }
    }
    if profile.is_none() && jobs.pick_key != Some(key) {
        state.hover = if state.markers_visible && !state.markers.is_empty() {
            survey::pick_marker(scene, &state.markers, ray)
        } else {
            None
        };
        jobs.pick_key = Some(key);
    }
    if let Some(hit) = state.hover
        && let SurveyTarget::Marker(id) = hit.point.target
        && let Some(marker) = state.markers.iter().find(|marker| marker.id == id).cloned()
    {
        let eye = ray.origin;
        let key = (
            eye,
            id,
            state.generation,
            surface.generation,
            weather.generation,
            explorer.boundaries_enabled,
        );
        let result = jobs
            .occlusion
            .update(key, || {
                let scene = surface.shared(explorer.boundaries_enabled);
                let profile = profile.cloned();
                move |cancelled| {
                    if let Some(profile) = profile {
                        profile
                            .marker_occlusion(&scene, eye, &marker, &cancelled)
                            .map_err(|error| error.to_string())
                    } else {
                        survey::marker_occlusion(&scene, eye, &marker, &cancelled)
                            .map_err(|error| error.to_string())
                    }
                }
            })
            .cloned();
        state.occlusion_pending = result.is_none();
        state.occlusion = result.and_then(|result| match result {
            Ok(measurement) => Some(measurement),
            Err(error) => {
                state.last_error = Some(error);
                None
            }
        });
    } else {
        jobs.occlusion.clear();
        state.occlusion = None;
        state.occlusion_pending = false;
    }
    if let Some(action) = state.tape_action.take() {
        jobs.pending_tape = Some((key, action));
    }
    if jobs
        .pending_tape
        .is_some_and(|(requested, _)| requested != key)
    {
        jobs.pending_tape = None;
        state.last_error = Some(
            "View or weather changed while selecting the tape endpoint. Aim and click again."
                .into(),
        );
    }
    if let Some((_, action)) = jobs.pending_tape
        && (!sight.pending || profile.is_none())
    {
        jobs.pending_tape = None;
        let markers = if state.markers_visible {
            state.markers.as_slice()
        } else {
            &[]
        };
        let picked = if profile.is_some() {
            match &sight.result {
                Some(Ok(trace)) => trace.survey_pick(),
                Some(Err(error)) => {
                    state.last_error = Some(format!("Optical selection unresolved: {error}"));
                    None
                }
                None => None,
            }
        } else {
            survey::pick(scene, markers, ray)
        };
        if let Some(hit) = picked {
            state.accept_tape_point(action, hit.point);
        } else {
            state.last_error = Some(match (&sight.result, profile.is_some()) {
                (Some(Err(error)), true) => format!("Optical selection unresolved: {error}"),
                _ => "The dot does not hit ground or a visible marker. Aim at a surface and try again.".into(),
            });
        }
    }
}
