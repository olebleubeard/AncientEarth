mod controls;
mod cpu_watch;
mod frame_capture;
mod frame_pacing;
mod graphics_loading;
mod renderer;
mod survey;
mod ui;

use ancient_earth::datum::SeaLevel;
use ancient_earth::navigation::Explorer;
use ancient_earth::orbit::OrbitOverview;
use ancient_earth::simulation::AstronomicalScene;
use ancient_earth::surface::WorldSurface;
use ancient_earth::survey::SurveyState;
use ancient_earth::topography::TopographyState;
use ancient_earth::weather::WeatherState;
use bevy::prelude::*;
use dear_imgui_bevy::prelude::*;
use earth_math::{Lens, Projection, UnitSystem};

#[derive(Resource)]
pub struct AutomatedView(pub bool);

#[derive(Resource)]
struct TopographyInspection {
    local_detail: bool,
    wide_view: bool,
    smooth: bool,
}

#[derive(Resource)]
struct SmokeTest {
    output: Option<String>,
    loading_screen: bool,
    frames: u32,
    requested: bool,
}

pub fn run() -> AppExit {
    let mut explorer = Explorer::default();
    let surface = WorldSurface::bundled().expect("valid bundled land/water boundaries");
    let sea_level = SeaLevel::bundled().expect("valid bundled EGM2008 geoid");
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--star-phase") {
        match args
            .get(index + 1)
            .and_then(|value| value.parse::<f64>().ok())
        {
            Some(seconds) if seconds.is_finite() => explorer.star_phase_seconds = Some(seconds),
            _ => {
                eprintln!("--star-phase requires finite presentation-clock seconds");
                return AppExit::error();
            }
        }
    }
    let mut weather = WeatherState::default();
    let regional_reference = args.iter().any(|arg| arg == "--weather-regional-reference");
    if regional_reference && args.iter().any(|arg| arg == "--weather-reference") {
        eprintln!("Choose one explicit weather verification snapshot.");
        return AppExit::error();
    }
    if regional_reference || args.iter().any(|arg| arg == "--weather-reference") {
        if args.iter().any(|arg| arg == "--weather") {
            eprintln!(
                "Choose --weather for live data or --weather-reference for the fixed verification snapshot"
            );
            return AppExit::error();
        }
        let result = if regional_reference {
            weather.load_regional_reference()
        } else {
            weather.load_reference()
        };
        if let Err(error) = result {
            eprintln!("Could not load the fixed weather verification snapshot: {error}");
            return AppExit::error();
        }
    } else if args.iter().any(|arg| arg == "--weather") {
        weather.set_enabled(true);
    }
    if args.iter().any(|arg| arg == "--coast") {
        explorer.visit_coast(surface.active(true));
    }
    if args.iter().any(|arg| arg == "--show-licenses") {
        explorer.show_licenses = true;
    }
    if args.iter().any(|arg| arg == "--reference") {
        explorer.boundaries_enabled = false;
    }
    if args.iter().any(|arg| arg == "--ground") {
        explorer.ground_on_surface(surface.active(explorer.boundaries_enabled));
        explorer.observer.pitch_deg = -30.0;
    }
    if args.iter().any(|arg| arg == "--overview") {
        explorer.observer.overview();
    }
    if args.iter().any(|arg| arg == "--rockies-cost-view") {
        explorer.observer = renderer::rockies_cost_observer();
        explorer.latitude = explorer.observer.feet.lat_deg;
        explorer.longitude = explorer.observer.feet.lon_deg;
        explorer.altitude_m = explorer.observer.feet.height_m;
    }
    if args.iter().any(|arg| arg == "--global-coast-view") {
        explorer.observer = renderer::classification_inspection_observer(explorer.observer.feet);
        explorer.latitude = explorer.observer.feet.lat_deg;
        explorer.longitude = explorer.observer.feet.lon_deg;
        explorer.altitude_m = explorer.observer.feet.height_m;
    }
    if args.iter().any(|arg| arg == "--imperial") {
        explorer.units = UnitSystem::Imperial;
    }
    if args.iter().any(|arg| arg == "--metric") {
        explorer.units = UnitSystem::Metric;
    }
    if args.iter().any(|arg| arg == "--crouch") {
        explorer.crouch_locked = true;
        explorer.update_crouch(false);
    }
    if args.iter().any(|arg| arg == "--line") {
        explorer.set_horizontal_line(true);
    }
    if args.iter().any(|arg| arg == "--vertical-guide") {
        explorer.show_horizontal_line = true;
        explorer.show_vertical_line = true;
    }
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--guide-column") {
        match pair[1].parse::<f64>() {
            Ok(column) if column.is_finite() && (0.0..=1.0).contains(&column) => {
                explorer.guide_column = column;
            }
            _ => {
                eprintln!("--guide-column requires a fraction from 0 to 1");
                return AppExit::error();
            }
        }
    }
    if args.iter().any(|arg| arg == "--earth-unlocked") {
        explorer.follow_earth = false;
    }
    if args.iter().any(|arg| arg == "--no-horizon-lock") {
        explorer.set_horizon_lock(false);
    }
    if args.iter().any(|arg| arg == "--horizon-lock-demo") {
        explorer.observer = earth_math::navigation::horizon_lock_demo_observer();
    }
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--guide-demo") {
        use earth_math::guides::{GuideDemo, demo_observer};
        let demo = match pair[1].as_str() {
            "horizon" => GuideDemo::HighAltitude,
            "rolled" => GuideDemo::Rolled,
            "sky" => GuideDemo::SkyOnly,
            _ => {
                eprintln!("--guide-demo requires horizon, rolled or sky");
                return AppExit::error();
            }
        };
        explorer.observer = demo_observer(demo);
        explorer.show_horizontal_line = true;
        explorer.show_panel = false;
    }
    if args.iter().any(|arg| arg == "--clean-view") {
        explorer.show_ui = false;
    }
    if args.iter().any(|arg| arg == "--panels-collapsed") {
        explorer.show_panel = false;
        explorer.show_sight_panel = false;
        explorer.show_help_panel = false;
    }
    if args.iter().any(|arg| arg == "--body-info") {
        explorer.show_body_info = true;
    }
    if args.iter().any(|arg| arg == "--no-grass") {
        explorer.grass_enabled = false;
    }
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--quality") {
        explorer.quality = match pair[1].as_str() {
            "performance" => 0,
            "balanced" => 1,
            "fine" => 2,
            "high" => 3,
            _ => {
                eprintln!("--quality requires performance, balanced, fine or high");
                std::process::exit(2);
            }
        };
    }
    let mut survey = SurveyState::default();
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--survey-demo") {
        let demo = match pair[1].as_str() {
            "close" => earth_math::survey::demo(surface.active(explorer.boundaries_enabled), true),
            "horizon" => {
                earth_math::survey::demo(surface.active(explorer.boundaries_enabled), false)
            }
            "nearby" => {
                earth_math::survey::nearby_demo(surface.active(explorer.boundaries_enabled))
            }
            _ => {
                eprintln!("--survey-demo requires close, horizon or nearby");
                return AppExit::error();
            }
        };
        match demo {
            Ok(demo) => {
                explorer.observer = demo.observer;
                explorer.normal_lens = demo.lens;
                explorer.latitude = demo.observer.feet.lat_deg;
                explorer.longitude = demo.observer.feet.lon_deg;
                explorer.altitude_m = demo.observer.feet.height_m;
                survey.accept_markers(std::sync::Arc::new(demo.markers));
                survey.set_tape_enabled(true);
                survey.accept_tape_point(ancient_earth::survey::TapeAction::SetA, demo.tape_a);
                survey.accept_tape_point(ancient_earth::survey::TapeAction::SetB, demo.tape_b);
            }
            Err(error) => {
                eprintln!("Survey demonstration unavailable: {error}");
                return AppExit::error();
            }
        }
    }
    if args.iter().any(|arg| arg == "--marker-menu") {
        survey.open_marker_dialog(explorer.observer);
    }
    // Explicit native QA route through the same asynchronous placement lifecycle.
    if args.iter().any(|arg| arg == "--survey-row") {
        survey.placement_origin = Some(explorer.observer);
        if !survey.request_placement() {
            eprintln!("Could not request survey row: {:?}", survey.last_error);
            return AppExit::error();
        }
    }
    let mut astronomy = AstronomicalScene::bundled(explorer.observer)
        .expect("valid bundled astronomy data and supported system date");
    if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--datetime") {
        astronomy.calendar_input = pair[1].clone();
        astronomy.apply_calendar();
        astronomy.sample(explorer.observer, explorer.follow_earth);
    }
    if args.iter().any(|arg| arg == "--paused") {
        astronomy.clock.running = false;
    }
    if args.iter().any(|arg| arg == "--fisheye") {
        explorer.normal_lens = Lens::new(Projection::EquidistantFisheye, 140.0);
    }
    if args.iter().any(|arg| arg == "--telescope") {
        explorer.telescope = true;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--view-fov") {
        let value = args
            .get(index + 1)
            .and_then(|value| value.parse::<f64>().ok());
        match value {
            Some(degrees) if degrees.is_finite() => {
                let lens = if explorer.telescope {
                    &mut explorer.telescope_lens
                } else {
                    &mut explorer.normal_lens
                };
                *lens = Lens::new(lens.projection, degrees);
            }
            _ => {
                eprintln!("--view-fov requires finite vertical field-of-view degrees");
                return AppExit::error();
            }
        }
    }
    if args.iter().any(|arg| arg == "--weather-horizon-check") {
        explorer.observer = earth_math::atmosphere::weather::reference_observer();
        explorer.normal_lens = earth_math::atmosphere::weather::reference_lens();
        explorer.telescope = false;
        explorer.grass_enabled = false;
    }
    if let Some(frame) = astronomy.frame {
        if args.iter().any(|arg| arg == "--look-sun") {
            explorer
                .observer
                .aim_at(frame.sun.apparent_center_ecef_m)
                .expect("valid Sun direction");
        }
        if args.iter().any(|arg| arg == "--look-moon") {
            explorer
                .observer
                .aim_at(frame.moon.apparent_center_ecef_m)
                .expect("valid Moon direction");
        }
    }
    if args.iter().any(|arg| arg == "--look-earth") {
        explorer
            .observer
            .aim_at(earth_math::Vec3d::ZERO)
            .expect("valid Earth direction");
    }
    if let Err(error) = astronomy.reset_tracking(explorer.observer, explorer.follow_earth) {
        eprintln!("Could not establish observer frame: {error}");
        return AppExit::error();
    }
    astronomy.sample(explorer.observer, explorer.follow_earth);
    let output = args
        .windows(2)
        .find(|pair| pair[0] == "--screenshot")
        .map(|pair| pair[1].clone());
    // Exercise physical-pixel overlays without modifying the host display.
    let test_hidpi = output.is_some() && args.iter().any(|arg| arg == "--test-hidpi");
    let automated_view = output.is_some()
        || args
            .iter()
            .any(|arg| arg == "--frame-capture" || arg == "--frame-capture-motion");
    // Opt-in bounded native rendering diagnostic; normal launches stay 1080p.
    let diagnostic_small_window =
        automated_view && args.iter().any(|arg| arg == "--diagnostic-small-window");
    let capture_fullscreen = automated_view
        && args.iter().any(|arg| arg == "--capture-fullscreen")
        && !diagnostic_small_window;
    if let Some(path) = output
        .as_ref()
        .and_then(|path| std::path::Path::new(path).parent())
        && !path.as_os_str().is_empty()
    {
        std::fs::create_dir_all(path).expect("create screenshot output directory");
    }
    let mut app = App::new();
    let mut overview = OrbitOverview::default();
    if args.iter().any(|arg| arg == "--globe-view") {
        explorer.observer.overview();
    }
    if args.iter().any(|arg| arg == "--orbit-view")
        && let Err(error) = astronomy.fit_orbit(&mut explorer, &mut overview, (1920, 1080))
    {
        explorer.status = error;
    }
    if let Err(error) = astronomy.reset_tracking(explorer.observer, explorer.follow_earth) {
        explorer.status = error;
    }
    astronomy.sample(explorer.observer, explorer.follow_earth);
    astronomy.refresh_overview(&mut overview, explorer.observer, explorer.lens());
    let mut topography = TopographyState::default();
    let inspect_topography = args.iter().any(|arg| arg == "--topography-inspect");
    let wide_topography = args.iter().any(|arg| arg == "--topography-wide-view");
    let terrain_survey = args
        .iter()
        .any(|arg| arg == "--optical-terrain-survey-check");
    if args.iter().any(|arg| arg == "--topography-saved")
        || inspect_topography
        || wide_topography
        || terrain_survey
    {
        topography.load_saved(&surface);
    }
    app.insert_resource(ClearColor(Color::srgb(0.015, 0.025, 0.05)))
        .insert_resource(survey)
        .insert_resource(overview)
        .init_resource::<ui::MeasurementStatus>()
        .init_resource::<survey::OpticalSight>()
        .insert_resource(survey::TerrainSurveyInspection(terrain_survey))
        .insert_resource(topography)
        .insert_resource(TopographyInspection {
            local_detail: inspect_topography,
            wide_view: wide_topography,
            smooth: args.iter().any(|arg| arg == "--topography-smooth"),
        })
        .insert_resource(surface)
        .insert_resource(sea_level)
        .insert_resource(explorer)
        .insert_resource(astronomy)
        .insert_resource(weather)
        .insert_resource(AutomatedView(automated_view))
        .insert_resource(SmokeTest {
            output,
            loading_screen: args.iter().any(|arg| arg == "--capture-loading"),
            frames: 0,
            requested: false,
        })
        .add_plugins(
            DefaultPlugins
                .set(bevy::render::RenderPlugin {
                    // Keep the event loop alive while the graphics driver compiles.
                    // graphics_loading covers initial partial frames and gates captures.
                    synchronous_pipeline_compilation: false,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Ancient Earth | WGS84 Observatory".into(),
                        mode: if capture_fullscreen {
                            bevy::window::WindowMode::BorderlessFullscreen(
                                bevy::window::MonitorSelection::Current,
                            )
                        } else {
                            bevy::window::WindowMode::Windowed
                        },
                        resolution: if diagnostic_small_window {
                            (640, 360).into()
                        } else if test_hidpi {
                            bevy::window::WindowResolution::new(1920, 1080)
                                .with_scale_factor_override(1.5)
                        } else {
                            (1920, 1080).into()
                        },
                        present_mode: bevy::window::PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_systems(Startup, (renderer::setup, ui::configure_style))
        .add_systems(Update, (controls::fullscreen, screenshot_test));
    renderer::install(&mut app);
    graphics_loading::install(&mut app);
    cpu_watch::install(&mut app, &args);
    if let Err(error) = frame_pacing::install(&mut app, &args) {
        eprintln!("{error}");
        return AppExit::error();
    }
    frame_capture::install(&mut app, &args);
    app.try_install_imgui(ImguiPlugin::new(
        ImguiPluginConfig::default().with_docking(false),
    ))
    .expect("valid Dear ImGui configuration");
    let pass = app.imgui_primary_pass().expect("primary Dear ImGui pass");
    app.add_imgui_systems(
        &pass,
        (
            pass.system(ui::draw_controls),
            pass.system(update_topography),
            pass.system(controls::fly),
            pass.system(advance_astronomy),
            pass.system(refresh_overview),
            pass.system(update_weather),
            pass.system(survey::update),
            pass.system(renderer::render),
            pass.system(ui::draw_overlay),
            pass.system(ui::draw_body_info),
            pass.system(graphics_loading::draw),
            pass.system(controls::cursor_feedback),
        )
            .chain(),
    )
    .expect("register the ordered UI, controls, astronomy, renderer and HUD");
    app.run()
}

fn advance_astronomy(
    _frame: ImguiFrame<'_>,
    time: Res<Time<Real>>,
    mut state: ResMut<Explorer>,
    surface: Res<WorldSurface>,
    mut astronomy: ResMut<AstronomicalScene>,
) {
    let active = surface.active(state.boundaries_enabled);
    let _timing = cpu_watch::Section::start("astronomy");
    astronomy.advance(time.delta_secs_f64(), &mut state, active);
}

fn refresh_overview(
    _frame: ImguiFrame<'_>,
    astronomy: Res<AstronomicalScene>,
    state: Res<Explorer>,
    mut overview: ResMut<OrbitOverview>,
) {
    let _timing = cpu_watch::Section::start("orbit annotations");
    astronomy.refresh_overview(&mut overview, state.observer, state.lens());
}

fn update_weather(
    _frame: ImguiFrame<'_>,
    state: Res<Explorer>,
    surface: Res<WorldSurface>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    mut weather: ResMut<WeatherState>,
    mut previous: Local<Option<(String, Option<String>)>>,
) {
    let _timing = cpu_watch::Section::start("weather");
    weather.set_display_block_reason(match adapter.as_deref() {
        None => Some("Waiting for the graphics adapter; geometric view is active."),
        Some(adapter) if adapter.backend.to_str() == "dx12" => Some(
            "DirectX 12 refraction is disabled after native device-loss failures. Geometric view and measurements remain active. Restart with WGPU_BACKEND=vulkan to use weather refraction.",
        ),
        Some(_) => None,
    });
    weather.tick(state.observer.feet);
    weather.prepare_scene(
        surface.shared(state.boundaries_enabled),
        state.observer.feet,
    );
    let changed = previous
        .as_ref()
        .is_none_or(|(status, error)| status != &weather.status || error != &weather.last_error);
    if changed {
        info!("Weather: {}", weather.status);
        if let Some(error) = &weather.last_error {
            warn!("Weather: {error}");
        }
        *previous = Some((weather.status.clone(), weather.last_error.clone()));
    }
}

fn update_topography(
    _frame: ImguiFrame<'_>,
    mut state: ResMut<Explorer>,
    mut surface: ResMut<WorldSurface>,
    mut topography: ResMut<TopographyState>,
    mut inspection: ResMut<TopographyInspection>,
) {
    let _timing = cpu_watch::Section::start("topography");
    topography.tick(&mut surface, &mut state);
    if (inspection.local_detail || inspection.wide_view)
        && !topography.loading()
        && topography.last_error.is_none()
        && let Some(region) = topography.active_region.as_ref()
    {
        let observer = if inspection.wide_view {
            Ok(renderer::classification_inspection_observer(region.center))
        } else {
            earth_math::topography::inspection_observer(
                surface.active(state.boundaries_enabled),
                region.center,
            )
        };
        inspection.local_detail = false;
        inspection.wide_view = false;
        match observer {
            Ok(observer) => {
                state.observer = observer;
                state.latitude = observer.feet.lat_deg;
                state.longitude = observer.feet.lon_deg;
                state.altitude_m = observer.feet.height_m;
                state.status = "Inspection view of the saved area's surface coverage.".into();
                if inspection.smooth {
                    topography.set_elevations(false, &mut surface, &mut state);
                }
            }
            Err(error) => topography.last_error = Some(format!("Terrain inspection: {error}")),
        }
    }
}

#[allow(clippy::too_many_arguments)] // Read readiness across independently prepared scene tools.
fn screenshot_test(
    mut commands: Commands,
    mut smoke: ResMut<SmokeTest>,
    time: Res<Time<Real>>,
    rendered: Res<renderer::RenderedEarth>,
    graphics: Res<graphics_loading::GraphicsLoading>,
    weather: Res<WeatherState>,
    measurements: Res<ui::MeasurementStatus>,
    topography: Res<TopographyState>,
    survey: Res<SurveyState>,
    explorer: Res<Explorer>,
) {
    if smoke.output.is_none() {
        return;
    }
    smoke.frames += 1;
    // Ordinary captures must show the completed scene. The explicit loading QA
    // route instead captures an actual frame while background pipelines run.
    let scene_ready = smoke.frames >= 120
        && time.elapsed_secs() >= 8.0
        && rendered.presentation_ready
        && graphics.ready()
        && !measurements.pending
        && !topography.loading()
        && topography.last_error.is_none()
        && survey.placement.is_none()
        && !survey.occlusion_pending
        && (!weather.enabled || weather.active_profile().is_some());
    let capture_ready = if smoke.loading_screen {
        graphics.pending_capture_ready()
    } else {
        scene_ready
    };
    if capture_ready && !smoke.requested {
        info!(graphics = %graphics.metadata(), loading_screen = smoke.loading_screen,
            horizon_lock_enabled = explorer.horizon_lock_enabled,
            horizon_lock_active = explorer.horizon_lock_active(),
            heading_deg = explorer.observer.heading_deg,
            pitch_deg = explorer.observer.pitch_deg,
            roll_deg = explorer.observer.roll_deg,
            "Native screenshot readiness");
        smoke.requested = true;
        use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(smoke.output.clone().unwrap()))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
    let live_weather = weather.enabled && !weather.reference_snapshot;
    // The complete optical material can require more than two minutes for a
    // cold DX12 pipeline compile. Keep every readiness condition above; allow
    // a bounded cold-start budget rather than capturing an unfinished scene.
    let preparation_timeout = if live_weather {
        360.0
    } else if weather.enabled {
        300.0
    } else {
        120.0
    };
    let completion_timeout = if weather.enabled { 420.0 } else { 180.0 };
    if (time.elapsed_secs() >= preparation_timeout && !smoke.requested)
        || time.elapsed_secs() >= completion_timeout
    {
        error!(
            graphics = %graphics.metadata(),
            "Screenshot smoke test timed out after {} frames (scene ready: {}; measurements pending: {}; topography loading: {}; topography error: {:?}; topography status: {}; weather: {}; weather error: {:?})",
            smoke.frames,
            rendered.presentation_ready,
            measurements.pending,
            topography.loading(),
            topography.last_error,
            topography.progress.status,
            weather.status,
            weather.last_error
        );
        commands.queue(|world: &mut World| {
            world.write_message(AppExit::error());
        });
    }
}
