use super::{Explorer, renderer::RenderedEarth};
use ancient_earth::datum::SeaLevel;
use ancient_earth::orbit::OrbitOverview;
use ancient_earth::simulation::AstronomicalScene;
use ancient_earth::surface::WorldSurface;
use ancient_earth::survey::SurveyState;
use ancient_earth::weather::WeatherState;
use bevy::prelude::*;
use dear_imgui_bevy::prelude::*;
use dear_imgui_rs::{
    Condition, DrawFlags, FontSource, MouseButton as UiMouseButton, NumericFormat, StyleColor,
    StyleVar, TreeNodeFlags, WindowFlags,
};
use earth_math::atmosphere::OpticalSkyline;
use earth_math::guides::{GuideRuler, GuideViewport, HorizonProfile, column_from_screen};
mod body_info;
mod body_toolbar;
mod edge_panels;
pub(super) mod jobs;
mod orbit_panel;
mod survey_panel;
mod topography_panel;
mod weather_panel;
use earth_math::rendering::layout::rect_size;
use earth_math::{
    Lens, Observer, Projection, STATURE_M, SkylineMeasurement, SkylineMethod, UnitSystem,
    adjust_speed, marker_hit, panel_anchor, screen_center,
};
use jobs::SnapshotJob;

const ACCENT: [f32; 4] = [0.38, 0.85, 0.76, 1.0];
const MUTED: [f32; 4] = [0.58, 0.66, 0.73, 1.0];

pub fn draw_body_info(
    frame: ImguiFrame<'_>,
    loading: Res<super::graphics_loading::GraphicsLoading>,
    mut state: ResMut<Explorer>,
    astronomy: Res<AstronomicalScene>,
) {
    if loading.startup_blocked() {
        return;
    }
    let _timing = super::cpu_watch::Section::start("body information");
    body_info::draw(frame.ui(), &mut state, &astronomy);
}

#[derive(Resource, Default)]
pub struct MeasurementStatus {
    pub pending: bool,
}

#[derive(Default)]
pub struct SkylineCache {
    key: Option<SkylineKey>,
    measurement: Option<SkylineMeasurement>,
    refracted: Option<OpticalSkyline>,
    job: SnapshotJob<SkylineKey, SkylineResult>,
    pending: bool,
}

type SkylineKey = (Observer, bool, u64, u64);
type SkylineResult = (Option<SkylineMeasurement>, Option<OpticalSkyline>);

type GuideKey = (Observer, Lens, bool, GuideViewport, u64, u64);
type GuideJobKey = (GuideKey, Option<f64>);
type OpticalTapeKey = (
    Observer,
    Lens,
    [f32; 2],
    [f32; 2],
    u64,
    u64,
    u64,
    bool,
    bool,
    Option<earth_math::survey::TapePoint>,
    Option<earth_math::survey::TapePoint>,
);
type OpticalTapeResult = Result<Option<earth_math::survey::TapeAnnotation>, String>;
#[derive(Default)]
pub struct OpticalTapeCache {
    job: SnapshotJob<OpticalTapeKey, OpticalTapeResult>,
}

#[derive(Clone)]
struct GuideMeasurement {
    profile: Option<HorizonProfile>,
    ruler: Option<GuideRuler>,
}

#[derive(Default)]
pub struct GuideCache {
    key: Option<GuideKey>,
    profile: Option<HorizonProfile>,
    ruler_column: Option<f64>,
    ruler: Option<GuideRuler>,
    dragging: bool,
    job: SnapshotJob<GuideJobKey, GuideMeasurement>,
    accepted_job: Option<GuideJobKey>,
    pending: bool,
}

fn guide_caption(
    ui: &dear_imgui_rs::Ui,
    draw: &dear_imgui_rs::DrawListMut<'_>,
    position: [f32; 2],
    color: [f32; 4],
    text: &str,
) {
    let _font = ui.push_font_with_size(None, 13.0);
    let size = ui.calc_text_size(text);
    let display = ui.io().display_size();
    let position: [f32; 2] = std::array::from_fn(|axis| {
        position[axis].clamp(6.0, (display[axis] - size[axis] - 6.0).max(6.0))
    });
    draw.add_rect(
        [position[0] - 4.0, position[1] - 4.0],
        [position[0] + size[0] + 4.0, position[1] + size[1] + 4.0],
        [0.025, 0.04, 0.055, 1.0],
    )
    .filled(true)
    .rounding(3.0)
    .build();
    draw.add_text_with_font(
        ui.current_font(),
        ui.current_font_size(),
        position,
        color,
        text,
        0.0,
        None,
    );
}

pub fn configure_style(mut contexts: NonSendMut<ImguiContexts>) {
    let primary = contexts
        .primary_id()
        .expect("ImGui registry")
        .expect("primary context");
    contexts
        .configure(primary, |context| {
            context
                .set_ini_filename(None::<std::path::PathBuf>)
                .expect("disable layout persistence");
            let atlas = context.font_atlas();
            atlas.clear();
            atlas.add_font(&[FontSource::default_vector_with_size(14.0)]);
            let style = context.style_mut();
            style.set_window_rounding(10.0);
            style.set_frame_rounding(4.0);
            style.set_window_padding([14.0, 12.0]);
            style.set_item_spacing([6.0, 6.0]);
            style.set_color(StyleColor::WindowBg, [0.028, 0.043, 0.063, 0.96]);
            style.set_color(StyleColor::Border, [0.16, 0.23, 0.28, 0.65]);
            style.set_color(StyleColor::Text, [0.90, 0.94, 0.96, 1.0]);
            style.set_color(StyleColor::TextDisabled, MUTED);
            style.set_color(StyleColor::FrameBg, [0.07, 0.11, 0.15, 1.0]);
            style.set_color(StyleColor::FrameBgHovered, [0.11, 0.21, 0.25, 1.0]);
            style.set_color(StyleColor::Button, [0.08, 0.25, 0.26, 1.0]);
            style.set_color(StyleColor::ButtonHovered, [0.12, 0.38, 0.37, 1.0]);
            style.set_color(StyleColor::ButtonActive, [0.18, 0.48, 0.43, 1.0]);
            style.set_color(StyleColor::Header, [0.07, 0.19, 0.22, 1.0]);
            style.set_color(StyleColor::HeaderHovered, [0.12, 0.29, 0.31, 1.0]);
            style.set_color(StyleColor::CheckMark, ACCENT);
            style.set_color(StyleColor::SliderGrab, ACCENT);
        })
        .expect("configure idle Dear ImGui context");
}

#[allow(clippy::too_many_arguments)] // Independent application tools share this control panel.
pub fn draw_controls(
    frame: ImguiFrame<'_>,
    loading: Res<super::graphics_loading::GraphicsLoading>,
    mut state: ResMut<Explorer>,
    mut surface: ResMut<WorldSurface>,
    mut overview: ResMut<OrbitOverview>,
    mut astronomy: ResMut<AstronomicalScene>,
    mut weather: ResMut<WeatherState>,
    mut topography: ResMut<ancient_earth::topography::TopographyState>,
    mut survey: ResMut<SurveyState>,
    mut pacing: ResMut<super::frame_pacing::FramePacing>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    if loading.startup_blocked() {
        return;
    }
    let _timing = super::cpu_watch::Section::start("controls panel");
    let ui = frame.ui();
    survey_panel::dialog(ui, &mut survey, &state);
    if !state.show_ui {
        return;
    }
    let modal = survey.dialog_open || state.show_body_info || state.show_licenses;
    body_toolbar::draw(ui, &mut state, &astronomy, modal);
    edge_panels::draw_handles(ui, &mut state);
    if state.show_licenses {
        ui.window("Licenses & attribution")
            .opened(&mut state.show_licenses)
            .position([380.0, 18.0], Condition::FirstUseEver)
            .size([700.0, 640.0], Condition::FirstUseEver)
            .build(|| {
                ui.text_wrapped("Ancient Earth uses open, redistributable data. Each provider retains its own terms. Full notices are embedded in this executable and included in release packages.");
                if ui.collapsing_header("Data attribution and terms", TreeNodeFlags::DEFAULT_OPEN) {
                    for line in include_str!("../../NOTICE.md").lines() {
                        ui.text_wrapped(line);
                    }
                }
                if ui.collapsing_header("Original software / MIT", TreeNodeFlags::empty()) {
                    for line in include_str!("../../LICENSE").lines() {
                        ui.text_wrapped(line);
                    }
                }
                if ui.collapsing_header("Third-party software and fonts", TreeNodeFlags::empty()) {
                    for line in include_str!("../../THIRD_PARTY_LICENSES.txt").lines() {
                        ui.text_wrapped(line);
                    }
                }
            });
    }
    let Some(rect) = edge_panels::layout(ui, &state).and_then(|layout| layout.left.panel) else {
        return;
    };
    let _minimum = ui.push_style_var(StyleVar::WindowMinSize([0.0, 0.0]));
    ui.window("Navigation")
        .position(rect.min, Condition::Always)
        .size(rect_size(rect), Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
        .build(|| {
            ui.text_colored(ACCENT, "A N C I E N T   E A R T H");
            ui.text_colored(MUTED, "WGS84  /  FIRST-PERSON OBSERVATORY");
            if ui.small_button("Licenses & attribution") {
                state.show_licenses = true;
            }
            if ui.small_button("Earth / Sun / Moon info [I]") {
                state.show_body_info = !state.show_body_info;
                state.looking = false;
            }
            ui.separator();
            if ui.button("Surface  [G]") {
                let scene = surface.active(state.boundaries_enabled);
                state.ground_on_surface(scene);
            }
            ui.same_line();
            if ui.button("Overview [O]") {
                state.observer.overview();
            }
            if ui.button("Fit whole orbit") && let Ok(window) = windows.single()
                && let Err(error) = astronomy.fit_orbit(&mut state, &mut overview, (window.physical_width(), window.physical_height())) {
                state.status = error;
            }
            if ui.button("Aim at Earth") && let Err(error) = state.aim_body(earth_math::astro::aim::ViewBody::Earth, astronomy.frame.as_ref(), astronomy.clock.instant()) {
                state.status = error.to_string();
            }
            ui.text_wrapped(&state.status);
            ui.spacing();

            if ui.collapsing_header("FLIGHT CONTROLS", TreeNodeFlags::empty()) {
                ui.text_wrapped("Left-click the scene to look and fly. Escape releases the cursor. WASD moves; Q / E rolls left / right when horizon lock is inactive. Space / Shift moves up / down relative to your body. Ctrl slows for precision. G lands you upright and level.");
                ui.checkbox("Automatic flight speed", &mut state.flight_settings.adaptive_speed);
                ui.text_wrapped("Speed adapts to your distance from modeled surfaces. Wheel adjusts base speed; in telescope mode it adjusts zoom.");
                ui.text_wrapped(format!("Navigation speed: {}", state.units.format_speed(state.flight_speed_mps)));
            }
            orbit_panel::controls(ui, &mut overview);
            survey_panel::draw(ui, &mut survey, &state);
            topography_panel::draw(ui, &mut topography, &mut surface, &mut state);
            weather_panel::draw(ui, &mut weather, state.units, state.observer);

            if ui.collapsing_header("SUN, MOON & TIME", TreeNodeFlags::empty()) {
                ui.text(astronomy.clock.calendar_string());
                ui.text_colored(MUTED, "UTC (before 1960: modeled UT1)");
                if ui.button(if astronomy.clock.running { "Pause" } else { "Play" }) {
                    astronomy.clock.running = !astronomy.clock.running;
                }
                ui.same_line();
                if ui.button("Now") { astronomy.reset_now(); }
                if let Some(frame) = astronomy.frame {
                    if ui.small_button("Aim at Sun") && let Err(error) = state.aim_body(earth_math::astro::aim::ViewBody::Sun, Some(&frame), astronomy.clock.instant()) {
                        state.status = error.to_string();
                    }
                    ui.same_line();
                    if ui.small_button("Aim at Moon") && let Err(error) = state.aim_body(earth_math::astro::aim::ViewBody::Moon, Some(&frame), astronomy.clock.instant()) {
                        state.status = error.to_string();
                    }
                }
                for (label, rate) in [("1x", 1.0), ("60x", 60.0), ("3600x", 3600.0), ("86400x", 86400.0)] {
                    if ui.small_button(label) {
                        astronomy.clock.rate = rate;
                        astronomy.clock.running = true;
                        astronomy.error = None;
                    }
                    ui.same_line();
                }
                ui.new_line();
                ui.set_next_item_width(145.0);
                ui.input_double("Playback rate", &mut astronomy.clock.rate);
                ui.set_next_item_width(-1.0);
                ui.input_text("##datetime", &mut astronomy.calendar_input)
                    .hint("YYYY-MM-DD HH:MM:SS").build();
                if ui.button("Apply date / pause") { astronomy.apply_calendar(); }
                ui.set_next_item_width(145.0);
                ui.input_double("Seconds per step", &mut astronomy.step_seconds);
                ui.set_next_item_width(145.0);
                ui.input_int("Step count", &mut astronomy.step_count);
                if ui.button("Step / pause") { astronomy.step(); }
                ui.text_wrapped("Negative rate or count rewinds. Supported years: 1850-2149.");
                if let Some(error) = &astronomy.error {
                    ui.text_wrapped(error);
                }
                if let Some(frame) = astronomy.frame {
                    ui.text_wrapped(frame.quality.description());
                    match frame.solar_eclipse(state.observer.eye_position()) {
                        Ok(eclipse) => ui.text_wrapped(eclipse.summary()),
                        Err(error) => ui.text_wrapped(format!("Eclipse calculation unavailable: {error}")),
                    }
                }
            }
            if ui.collapsing_header("LAND & WATER", TreeNodeFlags::empty()) {
                ui.checkbox("Show mapped land / water", &mut state.boundaries_enabled);
                if ui.button("Coast view") {
                    state.visit_coast(surface.active(true));
                }
                ui.text_wrapped("Land and water share the smooth WGS84 ellipsoid when elevations are off. Topography Download can enable source elevations.");
                ui.text_wrapped("Detailed Copernicus water masks take precedence over coarse Natural Earth land/ocean context. Categories do not alter elevations. Grass and surface materials are illustrative.");
            }
            if ui.collapsing_header("GO TO COORDINATES", TreeNodeFlags::empty()) {
                ui.set_next_item_width(175.0);
                ui.input_double_config("Latitude").display_format(NumericFormat::new("%.8f").expect("valid degree format")).build(&mut state.latitude);
                ui.set_next_item_width(175.0);
                ui.input_double_config("Longitude").display_format(NumericFormat::new("%.8f").expect("valid degree format")).build(&mut state.longitude);
                let mut height = state.units.length_from_metres(state.altitude_m);
                ui.set_next_item_width(175.0);
                if ui.input_double(format!("Feet height ({})", state.units.short_unit()), &mut height) {
                    state.altitude_m = state.units.length_to_metres(height);
                }
                ui.text_colored(MUTED, "Degrees: north / east positive.");
                ui.text_colored(MUTED, "Height above WGS84, not sea level.");
                if ui.button("Go to exact coordinates") {
                    let scene = surface.active(state.boundaries_enabled);
                    state.visit_coordinates_on_surface(scene);
                }
                if ui.button("Go to ground at coordinates") {
                    let scene = surface.active(state.boundaries_enabled);
                    state.visit_coordinate_ground(scene);
                }
                ui.same_line();
                if ui.button("Use current") {
                    state.latitude = state.observer.feet.lat_deg;
                    state.longitude = state.observer.feet.lon_deg;
                    state.altitude_m = state.observer.feet.height_m;
                }
            }
            if ui.collapsing_header("OPTICS & UNITS", TreeNodeFlags::empty()) {
                if ui.radio_button("Metric", state.units == UnitSystem::Metric) {
                    state.units = UnitSystem::Metric;
                }
                ui.same_line();
                if ui.radio_button("Imperial", state.units == UnitSystem::Imperial) {
                    state.units = UnitSystem::Imperial;
                }
                ui.checkbox("Telescope  [T]", &mut state.telescope);
                ui.same_line();
                if ui.button("Reset view [R]") {
                    state.reset_view();
                }
                if state.show_tooltips && ui.is_item_hovered() {
                    ui.tooltip_text("Exit telescope and restore the default normal perspective.");
                }
                if state.telescope {
                    ui.text_colored(MUTED, "Wheel: zoom | rectilinear optics");
                    if ui.button("Zoom in") { state.telescope_lens.zoom(1.0); }
                    ui.same_line();
                    if ui.button("Zoom out") { state.telescope_lens.zoom(-1.0); }
                    ui.same_line();
                    if ui.button("Reset") {
                        state.telescope_lens = Lens::new(Projection::Rectilinear, 5.0);
                    }
                    ui.text_wrapped("Zoom is limited by source geometry, pixels and floating-point precision.");
                } else {
                    if ui.radio_button("Normal / no fisheye", state.normal_lens.projection == Projection::Rectilinear) {
                        state.normal_lens = Lens::new(Projection::Rectilinear, 60.0);
                    }
                    if ui.radio_button("Equidistant fisheye", state.normal_lens.projection == Projection::EquidistantFisheye) {
                        state.normal_lens = Lens::new(Projection::EquidistantFisheye, 140.0);
                    }
                    let mut fov = state.normal_lens.vertical_fov_deg;
                    let maximum = if state.normal_lens.projection == Projection::Rectilinear { 150.0 } else { 180.0 };
                    if ui.slider("Vertical FOV", 20.0, maximum, &mut fov) {
                        state.normal_lens = Lens::new(state.normal_lens.projection, fov);
                    }
                }
            }
            if ui.collapsing_header("OBSERVER & DISPLAY", TreeNodeFlags::empty()) {
                ui.checkbox("Show tooltips", &mut state.show_tooltips);
                let mut horizon_lock = state.horizon_lock_enabled;
                if ui.checkbox("Horizon lock (prevent sideways tilt)", &mut horizon_lock) {
                    state.set_horizon_lock(horizon_lock);
                }
                if state.show_tooltips && ui.is_item_hovered() {
                    body_toolbar::horizon_lock_tooltip(ui, &state);
                }
                ui.text_colored(MUTED, if state.horizon_lock_active() {
                    "Level assist active; look up/down freely."
                } else if state.horizon_lock_enabled {
                    "Level assist armed; free roll in space."
                } else {
                    "Level assist off; Q/E rolls freely."
                });
                ui.text(format!("Stature: 6 ft / {}", state.units.format_distance(STATURE_M)));
                let mut eye = state.units.length_from_metres(state.posture.standing_eye_height_m);
                if ui.input_double(format!("Standing eyes ({})", state.units.short_unit()), &mut eye) {
                    let metres = state.units.length_to_metres(eye);
                    state.set_standing_eye_height(metres);
                }
                let mut crouching_eye = state.units.length_from_metres(state.posture.crouching_eye_height_m);
                if ui.input_double(format!("Crouching eyes ({})", state.units.short_unit()), &mut crouching_eye) {
                    let metres = state.units.length_to_metres(crouching_eye);
                    state.set_crouching_eye_height(metres);
                }
                if ui.checkbox("Crouch", &mut state.crouch_locked) {
                    state.update_crouch(false);
                }
                ui.text_wrapped("Hold C to crouch temporarily. Standing / crouching defaults: 1.72 m / 1.00 m above the feet. Adjust them to your measurements.");
                ui.text_wrapped("Crouching uses the lower of the two eye-height settings.");
                if ui.checkbox("Horizontal guide [L]", &mut state.show_horizontal_line) {
                    let enabled = state.show_horizontal_line;
                    state.set_horizontal_line(enabled);
                }
                if state.show_horizontal_line {
                    ui.checkbox("Vertical measurement [V]", &mut state.show_vertical_line);
                }
                ui.text_wrapped("L follows the highest visible skyline. With L and V enabled, use Left / Right arrows or drag the marker to move the vertical measurement. Labels use the selected units.");
                ui.checkbox("Follow Earth rotation [F9]", &mut state.follow_earth);
                ui.text_wrapped("Unchecked: hold your position and view in Earth's nonrotating, geocentric frame while the planet turns. Play or step time to see the motion. F9 relocks at your current location.");
                ui.text_wrapped("Horizon lock can still level your view near Earth; turn it off to preserve fully inertial orientation.");
                let mut speed = state.units.length_from_metres(state.observer.speed_mps);
                if ui.input_double(format!("Base speed ({}/s)", state.units.short_unit()), &mut speed) {
                    state.observer.speed_mps = adjust_speed(state.units.length_to_metres(speed), 0.0);
                }
                ui.combo_simple_string("Visual detail", &mut state.quality, &["Performance", "Balanced", "Fine", "High"]);
                let choices = super::frame_pacing::FrameLimit::ALL;
                let labels = choices.map(|limit| limit.label());
                let mut selected = choices.iter().position(|limit| *limit == pacing.limit).unwrap_or(1);
                if ui.combo_simple_string("Frame limit", &mut selected, &labels) {
                    pacing.limit = choices[selected];
                }
                ui.text_wrapped("Background: up to 15 FPS; minimized: up to 5 FPS. Simulation follows elapsed time.");
                ui.checkbox("Show grass", &mut state.grass_enabled);
                ui.text_wrapped("Native resolution with antialiasing. Visual detail controls surface rendering and grass coverage. F5 switches fullscreen / window.");
                ui.text_wrapped("F2 hides or restores the panels and center dot. L and V remain independently available in clean view.");
            }
            if ui.collapsing_header("MODEL & SOURCES", TreeNodeFlags::empty()) {
                ui.text_wrapped("Smooth WGS84 by default, with optional Copernicus elevation downloads. The terrain panel reports retained resolution. EGM2008 supplies datum comparisons and elevation conversion; it is not live sea level.");
                ui.text_wrapped("Weather & Refraction enables weather-informed curved rays. Off or unavailable profiles use geometric rays. Weather resolution and the stated optical model limit accuracy.");
                ui.text_wrapped("Sources: Copernicus water masks (modified); Natural Earth land/ocean context; NGA WGS84 and EGM2008; JPL DE440s; IERS via Astropy; ERFA; NOAA / NCEP GFS. Full notices: Licenses & attribution.");
                ui.text_wrapped("Grass, surface colors and the rotating star field are illustrative. Stars are not a measured catalog. Sun and Moon geometry uses the documented astronomical model.");
            }
        });
}

#[allow(clippy::too_many_arguments)] // Bevy injects independent scene and UI resources.
pub fn draw_overlay(
    frame: ImguiFrame<'_>,
    loading: Res<super::graphics_loading::GraphicsLoading>,
    mut state: ResMut<Explorer>,
    rendered: Res<RenderedEarth>,
    surface: Res<WorldSurface>,
    sea_level: Res<SeaLevel>,
    astronomy: Res<AstronomicalScene>,
    overview: Res<OrbitOverview>,
    weather: Res<WeatherState>,
    mut skyline: Local<SkylineCache>,
    mut guides: Local<GuideCache>,
    mut measurements: ResMut<MeasurementStatus>,
    survey: Res<SurveyState>,
    optical_sight: Res<super::survey::OpticalSight>,
    mut optical_tape: Local<OpticalTapeCache>,
) {
    if loading.startup_blocked() {
        return;
    }
    let _timing = super::cpu_watch::Section::start("measurement overlays");
    let ui = frame.ui();
    let layout = edge_panels::layout(ui, &state);
    let sight_rect = layout.and_then(|layout| layout.right.panel);
    let help_rect = layout.and_then(|layout| layout.bottom.panel);
    measurements.pending = optical_sight.pending;
    let guides = &mut *guides;
    let skyline = &mut *skyline;
    let scene = surface.active(state.boundaries_enabled);
    let atmosphere = weather.view_profile_for_scene(state.observer, scene);
    if weather.enabled
        && let Some(reason) = &weather.display_block_reason
    {
        guide_caption(
            ui,
            &ui.get_foreground_draw_list(),
            [12.0, 48.0],
            ACCENT,
            reason,
        );
    }
    if atmosphere.is_some() {
        let message = match &rendered.optical_status {
            super::renderer::OpticalDisplayStatus::Pending => {
                Some("Preparing terrain for curved-ray visibility...")
            }
            super::renderer::OpticalDisplayStatus::Unsupported(reason)
            | super::renderer::OpticalDisplayStatus::Error(reason) => Some(reason.as_str()),
            _ => None,
        };
        if let Some(message) = message {
            guide_caption(
                ui,
                &ui.get_foreground_draw_list(),
                [12.0, 48.0],
                ACCENT,
                message,
            );
        }
    }
    orbit_panel::draw_annotations(
        ui,
        &state,
        &overview,
        &astronomy,
        scene,
        atmosphere.is_none(),
    );
    let background_measurement = atmosphere.is_some() || !scene.is_boundary_only();
    if let Some(profile) = atmosphere
        && survey.tape_visible
        && survey.tape_a.is_some()
    {
        let viewport = ui.io().display_size();
        let scale = ui.io().display_framebuffer_scale();
        let observer = state.observer;
        let lens = state.lens();
        let a = survey.tape_a;
        let b = survey.tape_b;
        let key = (
            observer,
            lens,
            viewport,
            scale,
            weather.generation,
            surface.generation,
            survey.generation,
            state.boundaries_enabled,
            survey.markers_visible,
            a,
            b,
        );
        let result = optical_tape.job.update(key, || {
            let profile = profile.clone();
            let scene = surface.shared(state.boundaries_enabled);
            let markers = if survey.markers_visible {
                std::sync::Arc::clone(&survey.markers)
            } else {
                std::sync::Arc::default()
            };
            move |cancelled| {
                earth_math::atmosphere::projection::tape_annotation(
                    &profile, &scene, &markers, a, b, observer, lens, viewport, scale, &cancelled,
                )
                .map_err(|error| error.to_string())
            }
        });
        measurements.pending |= result.is_none();
        survey_panel::draw_optical_tape_annotation(ui, &survey, &state, result);
    } else {
        optical_tape.job.clear();
        if atmosphere.is_none() {
            survey_panel::draw_tape_annotation(ui, &survey, &state);
        }
    }
    if !state.show_ui {
        survey_panel::draw_clean_overlay(ui, &survey, state.units);
        let height = match scene.vertical_report(state.observer, &sea_level.0) {
            Ok(vertical) => state.units.format_height(vertical.eyes_above_ground_m),
            Err(_) => "Unavailable".to_owned(),
        };
        let caption = format!("Height: {height}");
        let _font = ui.push_font_with_size(None, 13.0);
        let position = panel_anchor(
            ui.io().display_size(),
            ui.calc_text_size(&caption),
            [1.0, 0.0],
            [18.0, 18.0],
        );
        guide_caption(
            ui,
            &ui.get_foreground_draw_list(),
            position,
            [0.90, 0.94, 0.96, 1.0],
            &caption,
        );
    }
    if !state.show_ui
        && weather.display_block_reason.is_none()
        && weather.active_profile().is_some()
        && atmosphere.is_none()
    {
        let draw = ui.get_foreground_draw_list();
        guide_caption(
            ui,
            &draw,
            [12.0, 48.0],
            ACCENT,
            weather.scene_error.as_deref().unwrap_or(
                "GEOMETRIC VIEW: outside the supported weather-refraction altitude range.",
            ),
        );
    }
    if sight_rect.is_none() {
        skyline.job.clear();
        skyline.key = None;
        skyline.refracted = None;
        skyline.measurement = None;
        skyline.pending = false;
    }
    if !state.show_horizontal_line {
        guides.job.clear();
        guides.accepted_job = None;
        guides.pending = false;
        guides.dragging = false;
    }
    if !state.show_ui && !state.show_horizontal_line {
        return;
    }
    let display = ui.io().display_size();
    let camera = state.observer.frame();
    let lens = state.lens();
    let scene = surface.active(state.boundaries_enabled);
    let skyline_key = (
        state.observer,
        state.boundaries_enabled,
        weather.generation,
        surface.generation,
    );
    if sight_rect.is_some() {
        if background_measurement {
            if skyline.key != Some(skyline_key) {
                skyline.refracted = None;
                skyline.measurement = None;
                skyline.key = Some(skyline_key);
            }
            let observer = state.observer;
            let measured = skyline
                .job
                .update(skyline_key, || {
                    let profile = atmosphere.cloned();
                    let scene = surface.shared(state.boundaries_enabled);
                    move |cancelled| {
                        if let Some(profile) = profile {
                            (
                                None,
                                profile.skyline_cancellable(&scene, observer, &cancelled),
                            )
                        } else {
                            (scene.skyline_cancellable(observer, &cancelled), None)
                        }
                    }
                })
                .copied();
            if let Some(measured) = measured {
                skyline.measurement = measured.0;
                skyline.refracted = measured.1;
            }
            skyline.pending = measured.is_none();
        } else {
            skyline.job.clear();
            skyline.pending = false;
            if skyline.key != Some(skyline_key) {
                skyline.refracted = None;
                skyline.measurement = scene.skyline(state.observer);
                skyline.key = Some(skyline_key);
            }
        }
        measurements.pending |= skyline.pending;
    }
    let center = screen_center(display);
    if state.show_horizontal_line {
        let viewport = GuideViewport::new(display, ui.io().display_framebuffer_scale());
        let key = viewport.map(|viewport| {
            (
                state.observer,
                lens,
                state.boundaries_enabled,
                viewport,
                weather.generation,
                surface.generation,
            )
        });
        if guides.key != key {
            guides.profile = if !background_measurement {
                viewport.and_then(|viewport| HorizonProfile::measure(scene, camera, lens, viewport))
            } else {
                None
            };
            guides.key = key;
            guides.ruler_column = None;
            guides.ruler = None;
            guides.accepted_job = None;
        }
        if let Some(profile) = &guides.profile {
            let marker = profile.marker_position(state.guide_column);
            if state.show_vertical_line
                && !state.looking
                && !ui.io().want_capture_mouse()
                && marker_hit(ui.io().mouse_pos(), marker, 10.0)
                && ui.is_mouse_clicked(UiMouseButton::Left)
            {
                guides.dragging = true;
            }
        }
        if guides.dragging && ui.is_mouse_down(UiMouseButton::Left) {
            if let Some(column) = column_from_screen(ui.io().mouse_pos()[0], display[0]) {
                state.guide_column = column;
            }
        } else {
            guides.dragging = false;
        }
        if let Some(key) = key.filter(|_| background_measurement) {
            let column = state.show_vertical_line.then_some(state.guide_column);
            let job_key = (key, column);
            guides.pending = guides.accepted_job != Some(job_key);
            if guides.pending {
                let previous_profile = &guides.profile;
                let measured = guides
                    .job
                    .update(job_key, || {
                        let previous_profile = previous_profile.clone();
                        let atmosphere = atmosphere.cloned();
                        let scene = surface.shared(state.boundaries_enabled);
                        move |cancelled| {
                            let profile = previous_profile.or_else(|| {
                                HorizonProfile::measure_cancellable(
                                    &scene,
                                    camera,
                                    lens,
                                    key.3,
                                    atmosphere.as_ref(),
                                    &cancelled,
                                )
                            });
                            let ruler = column.and_then(|column| {
                                profile.as_ref()?.ruler_cancellable(
                                    &scene,
                                    column,
                                    atmosphere.as_ref(),
                                    &cancelled,
                                )
                            });
                            GuideMeasurement { profile, ruler }
                        }
                    })
                    .cloned();
                if let Some(measured) = measured {
                    guides.profile = measured.profile;
                    guides.ruler = measured.ruler;
                    guides.ruler_column = column;
                    guides.accepted_job = Some(job_key);
                    guides.pending = false;
                } else {
                    guides.ruler = None;
                }
            }
        } else {
            guides.job.clear();
            guides.pending = false;
        }
        if !background_measurement
            && state.show_vertical_line
            && guides.ruler_column != Some(state.guide_column)
        {
            guides.ruler = guides.profile.as_ref().and_then(|profile| {
                profile.ruler_with_atmosphere(scene, state.guide_column, atmosphere)
            });
            guides.ruler_column = Some(state.guide_column);
        }
    } else {
        guides.dragging = false;
    }
    measurements.pending |= guides.pending;
    {
        let draw = ui.get_foreground_draw_list();
        if state.show_ui {
            draw.add_circle(center, 3.0, [0.01, 0.02, 0.03, 0.9])
                .filled(true)
                .build();
            draw.add_circle(center, 1.6, [0.96, 0.99, 1.0, 1.0])
                .filled(true)
                .build();
        }
        if state.show_horizontal_line {
            if let Some(profile) = &guides.profile {
                let rect = profile.horizontal_rect();
                draw.with_draw_flags(DrawFlags::AA_FILL, false, || {
                    draw.add_rect(rect.min, rect.max, [1.0, 1.0, 1.0, 1.0])
                        .filled(true)
                        .build();
                });
                let width = profile
                    .chord_width_m
                    .map(|metres| state.units.format_distance(metres))
                    .unwrap_or_else(|| "unavailable".to_owned());
                let surface_width = profile
                    .surface_width_m
                    .map(|metres| state.units.format_distance(metres))
                    .unwrap_or_else(|| "unavailable".to_owned());
                let label = format!(
                    "{}: {width}\nWGS84 surface geodesic: {surface_width}",
                    if atmosphere.is_some() {
                        "L sampled refracted horizon chord"
                    } else {
                        "L visible horizon chord"
                    }
                );
                guide_caption(
                    ui,
                    &draw,
                    profile.width_label_position(),
                    [1.0, 1.0, 1.0, 1.0],
                    &label,
                );
                if state.show_vertical_line {
                    let marker = profile.marker_position(state.guide_column);
                    draw.add_circle(marker, 4.0, ACCENT).filled(true).build();
                    if let Some(ruler) = guides.ruler {
                        let rect = profile.vertical_rect(ruler);
                        draw.with_draw_flags(DrawFlags::AA_FILL, false, || {
                            draw.add_rect(rect.min, rect.max, ACCENT)
                                .filled(true)
                                .build();
                        });
                        let label = format!(
                            "V drop below guide: {}\nat skyline depth / view-plane gap",
                            state.units.format_distance(ruler.view_plane_gap_m)
                        );
                        guide_caption(
                            ui,
                            &draw,
                            profile.ruler_label_position(ruler),
                            ACCENT,
                            &label,
                        );
                    } else {
                        guide_caption(
                            ui,
                            &draw,
                            marker,
                            ACCENT,
                            if guides.pending {
                                "V: updating current view..."
                            } else {
                                "V: no measurable skyline at this column"
                            },
                        );
                    }
                    if marker_hit(ui.io().mouse_pos(), marker, 10.0)
                        && !ui.io().want_capture_mouse()
                        && state.show_tooltips
                    {
                        ui.tooltip_text(format!("Drag or use Left / Right arrows.\nV measures the gap in a camera-facing plane through the skyline hit.\nIt is not a gravity height or a ray hit in the sky.\n{}", if atmosphere.is_some() {
                            "Finite visible-screen search, at most 512 optical traces; narrow/disconnected branches or unresolved rays can be missed. The local bracket is not a global error bound."
                        } else {
                            "WGS84 tangent contour clipped to this viewport; rectilinear extrema are analytic, fisheye extrema use bounded numerical refinement."
                        }));
                    }
                }
            } else {
                guide_caption(
                    ui,
                    &draw,
                    [12.0, 12.0],
                    ACCENT,
                    if guides.pending {
                        "L: measuring the current visible horizon..."
                    } else {
                        "L: no usable visible horizon found"
                    },
                );
            }
        }
    }
    if !state.show_ui {
        return;
    }
    let _minimum = ui.push_style_var(StyleVar::WindowMinSize([0.0, 0.0]));
    if let Some(rect) = sight_rect {
        ui.window("Sight measurement")
        .position(rect.min, Condition::Always)
        .size(rect_size(rect), Condition::Always)
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE,
        )
        .build(|| {
            ui.text_colored(ACCENT, "EYE  /  LINE OF SIGHT");
            if atmosphere.is_some() {
                weather_panel::draw_sight(ui, state.units, optical_sight.result.as_ref(), astronomy.frame.as_ref());
                if let Some(measurement) = &survey.occlusion {
                    ui.text_wrapped(measurement.summary(state.units));
                } else if survey.occlusion_pending {
                    ui.text_wrapped("Measuring the marker's refracted visibility...");
                }
            } else if survey.hover.is_some_and(|hit| earth_math::rendering::survey::marker_precedes_grass(hit.distance_m, rendered.grass_distance_m)
                && astronomy.frame.is_none_or(|astro| astro.nearest_celestial(camera.center_ray(), Some(hit.distance_m)).is_none()))
                && survey_panel::draw_marker(ui, &survey, state.units) {
                // Marker selection already excludes surfaces in front of the prism.
            } else {
            let surface_hit = scene.intersect(camera.center_ray());
            let surface_distance = rendered.grass_distance_m
                .or_else(|| surface_hit.map(|surface| surface.hit.distance_m));
            let celestial_hit = astronomy.frame.and_then(|astro| {
                astro.nearest_celestial(camera.center_ray(), surface_distance)
            });
            if let Some(hit) = celestial_hit {
                ui.text(state.units.format_distance(hit.distance_m));
                ui.text_colored(MUTED, state.units.format_precise_distance(hit.distance_m));
                ui.text_colored(MUTED, "Straight-line range from the eyes");
                ui.text_wrapped(hit.body.description());
                ui.text_wrapped("Modeled sphere at its light-time-corrected apparent position.");
            } else if let Some(distance) = rendered.grass_distance_m {
                ui.text(state.units.format_distance(distance));
                ui.text_colored(MUTED, state.units.format_precise_distance(distance));
                ui.text_colored(MUTED, "Straight-line range from the eyes");
                ui.text_wrapped("Animated grass blade / illustrative geometry");
            } else if let Some(surface_hit) = surface_hit {
                let hit = surface_hit.hit;
                ui.text(state.units.format_distance(hit.distance_m));
                ui.text_colored(MUTED, state.units.format_precise_distance(hit.distance_m));
                ui.text_colored(MUTED, "Straight-line range from the eyes");
                ui.text_wrapped(surface.surface_label(surface_hit.kind));
                ui.text(format!(
                    "Lat {:.7}  /  Lon {:.7}",
                    hit.geodetic.lat_deg, hit.geodetic.lon_deg
                ));
            } else {
                ui.text("No surface intersection");
                ui.text_colored(MUTED, "The center dot points into the sky.");
            }
            }
            ui.separator();
            ui.text_colored(
                ACCENT,
                if state.posture.crouched {
                    "EYE HEIGHT  /  CROUCHING"
                } else {
                    "EYE HEIGHT  /  STANDING"
                },
            );
            ui.text_wrapped(format!(
                "Eyes above feet   {}",
                state.units.format_height_feet_inches(state.observer.eye_height_m)
            ));
            match scene.vertical_report(state.observer, &sea_level.0) {
                Ok(vertical) => {
                    ui.text_wrapped(format!(
                        "Eyes above surface   {}",
                        state.units.format_height_feet_inches(vertical.eyes_above_ground_m)
                    ));
                    ui.text_wrapped(format!(
                        "Eyes above sea reference   {}",
                        state.units.format_height_feet_inches(vertical.eye_egm2008_height_m)
                    ));
                    ui.text_wrapped(format!(
                        "Eye WGS84 height   {}",
                        state
                            .units
                            .format_height_feet_inches(state.observer.eye_geodetic().height_m)
                    ));
                    ui.separator();
                    ui.text_colored(ACCENT, "DATUM REFERENCE  /  EGM2008");
                    ui.text_wrapped(format!(
                        "Lat {:.7}  /  Lon {:.7}",
                        state.observer.feet.lat_deg, state.observer.feet.lon_deg
                    ));
                    ui.text_wrapped(format!(
                        "Sea reference vs WGS84   {}",
                        state.units.format_height_feet_inches(vertical.geoid_undulation_m)
                    ));
                    ui.text_wrapped(format!(
                        "Surface above sea reference   {}",
                        state
                            .units
                            .format_height_feet_inches(vertical.ground_egm2008_height_m)
                    ));
                    ui.text_wrapped(format!(
                        "Surface: {}",
                        surface.surface_label(vertical.ground.kind)
                    ));
                    let _muted = ui.push_style_color(StyleColor::Text, MUTED);
                    ui.text_wrapped("EGM2008 is a modeled datum reference, not current local sea level. No live tides. Optional terrain retains the downloaded source elevations.");
                }
                Err(error) => ui.text_wrapped(format!("Height measurement unavailable: {error}")),
            }
            ui.separator();
            ui.text_colored(ACCENT, if atmosphere.is_some() { "SKYLINE AHEAD  /  REFRACTED" } else { "SKYLINE AHEAD  /  GEOMETRIC" });
            if let Some(measurement) = skyline.refracted {
                ui.text(state.units.format_distance(measurement.geometric_path_m));
                ui.text_wrapped(measurement.description());
                ui.text(format!("Elevation {:.6} deg", measurement.elevation_deg));
                ui.text_wrapped(surface.surface_label(measurement.hit.kind));
                ui.text_wrapped("Curved path along heading. Sampled estimate; narrow mirage branches may be missed.");
            } else if let Some(measurement) = skyline.measurement {
                ui.text(state.units.format_distance(measurement.hit.hit.distance_m));
                ui.text_wrapped(measurement.method.description());
                ui.text(format!("Elevation {:.6} deg", measurement.elevation_deg));
                ui.text_wrapped(surface.surface_label(measurement.hit.kind));
                if measurement.method == SkylineMethod::NumericalBoundaryEstimate {
                    ui.text_wrapped("Numerical estimate; narrow features may be missed.");
                }
                ui.text_colored(MUTED, "Along heading; may be outside this view. [?]");
                if state.show_tooltips && ui.is_item_hovered() {
                    ui.tooltip_text(format!(
                        "Skyline: highest surface boundary along heading {:.2} deg.\nStraight-line range from the current eyes.\nNot a universal visibility distance; distant objects can lie below a nearer skyline.\nNo atmospheric refraction or weather model.\nSearch step: {:.8} deg; refined bracket: {:.10} deg.\nZero means not used; the bracket is local only, not a global or range-error bound.",
                        measurement.heading_deg, measurement.angular_search_step_deg, measurement.angular_bracket_deg,
                    ));
                }
            } else {
                ui.text_wrapped(if skyline.pending { "Measuring the current heading..." } else { "No skyline measurement available for this position." });
            }
            ui.separator();
            ui.text(format!(
                "Flight speed    {}",
                state.units.format_speed(state.flight_speed_mps)
            ));
            ui.text(format!(
                "Heading {:.2} deg  /  Pitch {:.2} deg",
                state.observer.heading_deg, state.observer.pitch_deg
            ));
            ui.text(format!("Vertical FOV    {:.8} deg", lens.vertical_fov_deg));
            if state.telescope {
                ui.text(format!("Magnification   {:.2}x", lens.magnification()));
            }
            ui.text_wrapped(if atmosphere.is_some() {
                "Weather-informed optical model. Forecast, profile assumptions and numerical limits apply."
            } else {
                "Geometric WGS84 model measurements. Boundary-source and separate geoid uncertainties still apply."
            });
        });
    }
    if let Some(rect) = help_rect {
        ui.window("Flight help")
            .position(rect.min, Condition::Always)
            .size(rect_size(rect), Condition::Always)
            .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
            .build(|| {
                ui.text_wrapped(
                    "Left click: look / fly  |  Escape: cursor  |  WASD move  |  Q / E roll (unlocked)",
                );
                ui.text_wrapped(
                    "Space / Shift: body up / down  |  Ctrl precision  |  Wheel speed / zoom  |  T telescope",
                );
                ui.text_wrapped(
                "G surface + upright | O overview | R reset view | C crouch | L horizon | V ruler: arrows / drag",
            );
                ui.text_wrapped(
                    "1 markers | M tape | I body info | F5 fullscreen | F2 clean | Tab panel",
                );
                ui.text_colored(
                    ACCENT,
                    if state.follow_earth {
                        "F9: following Earth"
                    } else {
                        "F9: Earth-centered nonrotating"
                    },
                );
                ui.text_colored(
                    MUTED,
                    format!(
                        "{} x {} native view  /  {}  /  {}",
                        rendered.dimensions.0,
                        rendered.dimensions.1,
                        rendered.mode,
                        if atmosphere.is_some() {
                            "Terrain, markers + reference surface".to_owned()
                        } else {
                            format!("{} surface triangles", rendered.triangles)
                        }
                    ),
                );
            });
    }
}
