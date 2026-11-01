use ancient_earth::surface::WorldSurface;
use ancient_earth::weather::WeatherState;
use dear_imgui_rs::{TreeNodeFlags, Ui};
use earth_math::{UnitSystem, astro::AstronomyFrame, atmosphere::OpticalTrace};
mod custom_weather;

pub(super) fn draw(
    ui: &Ui,
    weather: &mut WeatherState,
    units: UnitSystem,
    observer: earth_math::Observer,
) {
    if !ui.collapsing_header("WEATHER & REFRACTION", TreeNodeFlags::empty()) {
        return;
    }
    let mut enabled = weather.enabled;
    if ui.checkbox("Enable weather refraction", &mut enabled) {
        weather.set_enabled(enabled);
    }
    ui.text_wrapped("Disabled by default. Enable when you want to visualize atmospheric refraction; turn it off to return to the normal flight view.");
    if weather.enabled {
        ui.text_wrapped("Full-resolution refraction can greatly reduce frame rate.");
    }
    if ui.collapsing_header("What refraction changes", TreeNodeFlags::empty()) {
        ui.text_wrapped("Air layers bend light, so the visible horizon and the apparent position or shape of the Sun and Moon can change. This mode follows curved sight lines and reports the geometric length of those curved paths instead of assuming perfectly straight light.");
        ui.text_wrapped("It uses a NOAA forecast column or a custom column seeded from it. This can demonstrate refraction and some mirage patterns, but cannot reproduce exact local conditions.");
    }
    ui.text_wrapped(&weather.status);
    if let Some(error) = &weather.scene_error {
        ui.text_wrapped(error);
    }
    if weather.enabled
        && let Some(reason) = &weather.display_block_reason
    {
        ui.text_colored(super::ACCENT, "GEOMETRIC VIEW / GRAPHICS BACKEND");
        ui.text_wrapped(reason);
    }
    if weather.enabled {
        ui.text_wrapped(format!(
            "Optical display eye-height limit: {} above WGS84. Higher views use geometric rays.",
            units.format_distance(earth_math::atmosphere::gpu::MAX_GPU_EYE_HEIGHT_M)
        ));
    }
    if weather.active_profile().is_some()
        && !earth_math::atmosphere::gpu::display_supported(observer)
    {
        ui.text_wrapped("GEOMETRIC VIEW: this eye altitude is outside the supported refraction display range. Weather data remain available; view, range and L/V guides all use geometric rays.");
    }
    if let Some(error) = &weather.last_error {
        ui.text_wrapped(error);
    }
    if weather.enabled
        && ui.small_button(if weather.reference_snapshot || weather.custom.active {
            "Use live weather"
        } else {
            "Refresh weather"
        })
    {
        weather.refresh();
    }
    custom_weather::draw(ui, weather, units);
    if let Some(metadata) = weather
        .profile
        .as_ref()
        .and_then(|profile| profile.metadata.as_ref())
    {
        ui.text_wrapped(&metadata.model);
        let prefix = if weather.custom.active {
            "Seed forecast "
        } else {
            ""
        };
        ui.text_wrapped(format!("{prefix}Valid {} UTC", metadata.valid_time_label()));
        ui.text_wrapped(format!("{prefix}Run {} UTC", metadata.cycle_time_label()));
        ui.text_wrapped(format!(
            "Source grid {:.2}, {:.2}",
            metadata.latitude_deg, metadata.longitude_deg
        ));
        ui.text_wrapped(metadata.surface_conditions_label(units));
        ui.text_wrapped(&metadata.vertical_input_description);
        ui.text_wrapped(format!(
            "Source: {}. Modified optical scenario; no NOAA endorsement.",
            metadata.source
        ));
    }
    if weather.custom.active {
        ui.text_wrapped("Custom weather stays fixed while you fly. Use live weather to resume location and time updates. These settings last for this session.");
    } else {
        ui.text_wrapped("Live weather follows your in-app location and real UTC. The Sun/Moon calendar is independent.");
    }
    if let Some(anchor) = weather
        .registered_profile()
        .and_then(|profile| profile.anchor())
    {
        ui.text_wrapped(anchor.label(units));
    }
    ui.text_wrapped("Forecast layers are registered once to the active ground where the column was loaded. This fixed column is used across the view. Thin local mirage layers and horizontal weather changes along a sight line are unresolved.");
    ui.text_wrapped(
        "Refraction view uses simplified lighting; decorative grass and its shadows are hidden.",
    );
    ui.text_wrapped("Terrain and markers use curved-ray intersections. Model and numerical limits apply; this is not an exact observation of local visibility.");
}

pub(super) fn draw_sight(
    ui: &Ui,
    units: UnitSystem,
    result: Option<&Result<OpticalTrace, String>>,
    astronomy: Option<&AstronomyFrame>,
) {
    let trace = match result {
        Some(Ok(trace)) => trace,
        Some(Err(error)) => {
            ui.text_wrapped(format!("Refracted range unavailable: {error}"));
            return;
        }
        None => {
            ui.text_wrapped("Measuring the current curved sight line...");
            return;
        }
    };
    let distances = |path, chord, optical| {
        ui.text(units.format_distance(path));
        ui.text_colored(super::MUTED, units.format_precise_distance(path));
        ui.text_colored(super::MUTED, "Curved geometric path from the eyes");
        ui.text_wrapped(format!(
            "Straight endpoint distance: {}",
            units.format_distance(chord)
        ));
        ui.text_wrapped(format!(
            "Phase optical path: {}",
            units.format_distance(optical)
        ));
    };
    if let Some(celestial) = astronomy.and_then(|frame| trace.nearest_celestial(frame)) {
        distances(
            celestial.geometric_path_m,
            celestial.chord_m,
            celestial.optical_path_m,
        );
        ui.text_wrapped(celestial.hit.body.description());
    } else if let Some(marker) = trace.marker {
        ui.text(units.format_distance(trace.geometric_path_m));
        ui.text_colored(super::MUTED, "Curved geometric path from the eyes");
        ui.text_wrapped(format!(
            "Phase optical path: {}",
            units.format_distance(trace.optical_path_m)
        ));
        ui.text_colored(
            super::ACCENT,
            format!(
                "MARKER {}",
                earth_math::rendering::survey::marker_label(marker.marker_id)
                    .unwrap_or_else(|| "?".into())
            ),
        );
    } else if let Some(surface) = trace.surface {
        distances(
            trace.geometric_path_m,
            surface.hit.distance_m,
            trace.optical_path_m,
        );
        ui.text_wrapped(WorldSurface::label(surface.kind));
        ui.text_wrapped(format!(
            "Lat {:.7} / Lon {:.7}",
            surface.hit.geodetic.lat_deg, surface.hit.geodetic.lon_deg
        ));
    } else {
        ui.text("No target intersection");
        ui.text_wrapped("Refracted ray leaves the atmosphere toward the sky.");
    }
}
