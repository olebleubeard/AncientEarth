use ancient_earth::{navigation::Explorer, surface::WorldSurface, topography::TopographyState};
use dear_imgui_rs::{TreeNodeFlags, Ui};
use earth_math::topography::{
    format_download_size, radius_display_value, radius_from_display, radius_unit,
};

pub(super) fn draw(
    ui: &Ui,
    state: &mut TopographyState,
    surface: &mut WorldSurface,
    explorer: &mut Explorer,
) {
    if !ui.collapsing_header("TOPOGRAPHY DOWNLOAD", TreeNodeFlags::empty()) {
        return;
    }
    let busy = state.loading();
    {
        let _disabled = ui.begin_disabled_with_cond(busy);
        let mut radius = radius_display_value(state.radius_m, explorer.units);
        if ui.input_double(
            format!("Radius ({})", radius_unit(explorer.units)),
            &mut radius,
        ) {
            match radius_from_display(radius, explorer.units) {
                Ok(metres) => {
                    state.radius_m = metres;
                    state.last_error = None;
                }
                Err(error) => state.last_error = Some(error.to_string()),
            }
        }
        ui.text_wrapped("Maximum radius: 100 mi / 160.9344 km.");
        ui.text_wrapped("Downloads elevations and water boundaries around your current position. Full source tiles and seam-support neighbors can extend beyond the radius.");
        if ui.button("Estimate size") {
            state.request(explorer.observer.feet, surface, false);
        }
        ui.same_line();
        if ui.button("Download + enable") {
            state.request(explorer.observer.feet, surface, true);
        }
        if ui.small_button("Load last saved area (offline)") {
            state.load_saved(surface);
        }
    }
    if busy && ui.button("Cancel download / preparation") {
        state.cancel();
    }
    {
        let _disabled = ui.begin_disabled_with_cond(!surface.has_terrain());
        let mut enabled = surface.elevations_enabled;
        if ui.checkbox("Elevations (off = smooth Earth)", &mut enabled) {
            state.set_elevations(enabled, surface, explorer);
        }
    }
    if surface.elevations_enabled {
        let scene = surface.active(explorer.boundaries_enabled);
        match scene.imported_ground(explorer.observer.feet) {
            Some(ground) => {
                ui.text_colored(super::ACCENT, "Downloaded terrain is active beneath you.");
                ui.text_wrapped(format!(
                    "Ground height relative to WGS84: {}",
                    explorer.units.format_height(ground.hit.geodetic.height_m)
                ));
                if ui.small_button("Stand on downloaded ground") {
                    explorer.ground_on_surface(scene);
                }
                ui.text_wrapped("Source heights are not exaggerated. Coastal water and low land can remain nearly level.");
            }
            None => {
                ui.text_wrapped("No imported elevation coverage beneath you. This location uses the smooth reference surface.");
            }
        }
    } else if surface.has_terrain() {
        ui.text_wrapped(
            "Smooth Earth is active. Downloaded elevations are loaded and can be enabled above.",
        );
    } else {
        ui.text_wrapped("Smooth Earth is active. Download an area or load your saved area to prepare elevations.");
    }
    ui.text_wrapped(&state.progress.status);
    if let Some(region) = &state.request_region {
        ui.text_wrapped(format!(
            "Request: {:.5}, {:.5}; radius {:.2} {}",
            region.center.lat_deg,
            region.center.lon_deg,
            radius_display_value(region.radius_m, explorer.units),
            radius_unit(explorer.units)
        ));
    }
    if let Some(total) = state.progress.total_bytes {
        ui.text_wrapped(format!(
            "Source size: approximately {}",
            format_download_size(total)
        ));
        if let Some(download) = state.progress.download_bytes {
            ui.text_wrapped(format!(
                "New download: {} / received {}",
                format_download_size(download),
                format_download_size(state.progress.received_bytes)
            ));
        }
    }
    if state.progress.missing_tiles > 0 {
        ui.text_wrapped(format!("{} source tiles unavailable. Ocean-only / restricted areas may be absent; uncovered areas retain the smooth reference.",state.progress.missing_tiles));
    }
    if state.progress.total_bytes.is_some() {
        ui.text_wrapped(format!(
            "Water-boundary tiles available for this request: {}. Elevations and water coverage are separate layers.",
            state.progress.water_tiles
        ));
        if state.progress.missing_water_tiles > 0 {
            ui.text_wrapped("Where detailed water data is unavailable, Natural Earth supplies coarse global land/ocean context. Missing elevation tiles are never treated as evidence of ocean.");
        }
        if state.progress.water_tiles == 0 && surface.has_terrain() && !busy {
            ui.text_wrapped("This saved area has no downloaded water masks. Click Download + enable to add them; verified elevation files will be reused.");
        }
    }
    if let Some(bytes) = state.active_source_bytes {
        ui.text_wrapped(format!(
            "Active area's saved source files: {}",
            format_download_size(bytes)
        ));
    }
    if let Some(error) = &state.last_error {
        ui.text_wrapped(format!("Request: {error}"));
    }
    if let Some(description) = &state.imported_description {
        ui.text_wrapped(description);
    }
    if surface.elevations_enabled {
        ui.text_wrapped("Weather refraction supports enabled terrain. Preparing a new area also prepares its curved-ray visibility data in the background.");
    }
    ui.text_wrapped("Copernicus GLO-30: nominal 30 m surface model, including vegetation and buildings. Native water categories accompany downloaded elevations. Retained mesh resolution is reported after import. No imagery is downloaded.");
    if let Some(path) = &state.cache {
        ui.text_wrapped(format!("Saved in: {}", path.display()));
    }
}
