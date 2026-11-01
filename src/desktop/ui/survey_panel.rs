use ancient_earth::{
    navigation::Explorer,
    survey::{SurveyState, TapeAction},
};
use dear_imgui_rs::{DrawFlags, TreeNodeFlags, Ui, WindowFlags};
use earth_math::{
    UnitSystem,
    rendering::survey::marker_label,
    survey::{SurveyTarget, row_distance_from_display},
    topography::{radius_display_value, radius_unit},
};

const TITLE: &str = "Place a row of markers";

pub(super) fn dialog(ui: &Ui, state: &mut SurveyState, explorer: &Explorer) {
    if state.dialog_requested {
        ui.open_popup(TITLE);
        state.dialog_requested = false;
    }
    let mut opened = state.dialog_open;
    let mut close = false;
    if let Some(_popup) = ui
        .begin_modal_popup_config(TITLE)
        .opened(&mut opened)
        .flags(WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_RESIZE)
        .begin()
    {
        ui.dummy([520.0, 0.0]);
        ui.text("Choose spacing and distance before spawning.");
        ui.text("A new row replaces the previous row.");
        ui.separator();
        let units = explorer.units;
        let mut spacing = radius_display_value(state.settings.spacing_m, units);
        if ui.input_double(format!("Spacing ({})", radius_unit(units)), &mut spacing) {
            match row_distance_from_display(spacing, units) {
                Ok(metres) => {
                    state.settings.spacing_m = metres;
                    state.last_error = None;
                }
                Err(error) => state.last_error = Some(error.to_string()),
            }
        }
        let mut reach = radius_display_value(state.settings.reach_m, units);
        if ui.input_double(format!("Extend to ({})", radius_unit(units)), &mut reach) {
            match row_distance_from_display(reach, units) {
                Ok(metres) => {
                    state.settings.reach_m = metres;
                    state.last_error = None;
                }
                Err(error) => state.last_error = Some(error.to_string()),
            }
        }
        let mut offset = units.length_from_metres(state.settings.left_offset_m);
        if ui.input_double(format!("Offset left ({})", units.short_unit()), &mut offset) {
            state.settings.left_offset_m = units.length_to_metres(offset);
            state.last_error = None;
        }
        if let Some(origin) = state.placement_origin {
            ui.text(format!(
                "Origin {:.6}, {:.6} / heading {:.2} deg",
                origin.feet.lat_deg, origin.feet.lon_deg, origin.heading_deg
            ));
        }
        ui.text("Grounded boxes: 10 ft wide x 10 ft deep x 100 ft tall.");
        ui.text("Red at the base; alternating red/green every foot.");
        ui.text("First marker is beside you at the left offset.");
        ui.text("Following markers continue ahead at the selected spacing.");
        ui.text("Upright black signs are numbered 0001 onward and face you.");
        let valid = state.settings.validate();
        match &valid {
            Ok(count) => ui.text(format!("{count} markers")),
            Err(error) => ui.text_wrapped(error.to_string()),
        }
        if let Some(error) = &state.last_error {
            ui.text_wrapped(error);
        }
        {
            let _disabled =
                ui.begin_disabled_with_cond(valid.is_err() || state.last_error.is_some());
            if ui.button("Spawn markers") && state.request_placement() {
                ui.close_current_popup();
                close = true;
            }
        }
        ui.same_line();
        if ui.button("Cancel") {
            ui.close_current_popup();
            close = true;
        }
    }
    state.dialog_open = opened && !close && ui.is_popup_open(TITLE);
}

pub(super) fn draw(ui: &Ui, state: &mut SurveyState, explorer: &Explorer) {
    if !ui.collapsing_header("TAPE & MARKERS", TreeNodeFlags::empty()) {
        return;
    }
    if ui.button("Place markers [1]") {
        state.open_marker_dialog(explorer.observer);
    }
    if !state.markers.is_empty() {
        ui.checkbox("Show markers", &mut state.markers_visible);
        ui.same_line();
        if ui.small_button("Clear markers") {
            state.clear_markers();
        }
    }
    if state.placement.is_some() && ui.button("Cancel marker placement") {
        state.cancel_placement();
    }
    let mut enabled = state.tape_enabled;
    if ui.checkbox("Tape measure [M]", &mut enabled) {
        state.set_tape_enabled(enabled);
    }
    if state.tape_enabled {
        ui.text_wrapped("Click the scene once to capture mouse look, then aim and left-click for A, then B. A third click starts a new tape. Escape releases the cursor. Points can be on the ground or markers.");
        if ui.small_button("Set A from dot") {
            state.request_tape_point(TapeAction::SetA);
        }
        ui.same_line();
        {
            let _disabled = ui.begin_disabled_with_cond(state.tape_a.is_none());
            if ui.small_button("Set B from dot") {
                state.request_tape_point(TapeAction::SetB);
            }
        }
        if ui.small_button("Clear tape") {
            state.clear_tape();
        }
        draw_tape(ui, state, explorer.units);
    }
    ui.text_wrapped(&state.status);
    if let Some(error) = &state.last_error {
        ui.text_wrapped(error);
    }
    if state.geometry_active() {
        ui.text_wrapped("With weather refraction enabled, selection follows curved light paths. Tape distances remain labeled straight separation and WGS84 surface distance.");
    }
}

fn target_name(target: SurveyTarget) -> String {
    match target {
        SurveyTarget::Surface => "ground surface".into(),
        SurveyTarget::Marker(id) => {
            format!("marker {}", marker_label(id).unwrap_or_else(|| "?".into()))
        }
    }
}

pub(super) fn draw_tape(ui: &Ui, state: &SurveyState, units: UnitSystem) {
    if let Some(a) = state.tape_a {
        ui.text_wrapped(format!("A (cyan): {}", target_name(a.target)));
    }
    if let Some(b) = state.tape_b {
        ui.text_wrapped(format!("B (yellow): {}", target_name(b.target)));
    }
    if let Some(measurement) = &state.tape_measurement {
        ui.text_wrapped(format!(
            "A to B, straight: {}",
            units.format_distance(measurement.straight_distance_m)
        ));
        ui.text_wrapped(format!(
            "WGS84 surface geodesic: {}",
            units.format_distance(measurement.surface_distance_m)
        ));
        ui.text_wrapped("The geodesic uses endpoint latitude/longitude on WGS84; it is not a terrain-following route. Geometric tape annotations may show hidden chord portions. Optical tape follows one resolved image branch and omits blocked or unresolved portions.");
    }
}

pub(super) fn draw_tape_annotation(ui: &Ui, state: &SurveyState, explorer: &Explorer) {
    if !state.tape_visible {
        return;
    }
    if let Some(a) = state.tape_a
        && state.tape_b.is_none()
        && let Some(point) = earth_math::survey::tape_endpoint_position(
            a,
            explorer.observer.frame(),
            explorer.lens(),
            ui.io().display_size(),
            ui.io().display_framebuffer_scale(),
        )
    {
        let draw = ui.get_foreground_draw_list();
        super::guide_caption(ui, &draw, point, [0.2, 0.9, 1.0, 1.0], "A");
    }
    let (Some(a), Some(b), Some(_measurement)) =
        (state.tape_a, state.tape_b, state.tape_measurement.as_ref())
    else {
        return;
    };
    let Some(annotation) = earth_math::survey::tape_annotation(
        a,
        b,
        explorer.observer.frame(),
        explorer.lens(),
        ui.io().display_size(),
        ui.io().display_framebuffer_scale(),
    ) else {
        return;
    };
    draw_tape_result(ui, state, explorer, &annotation);
}

pub(super) fn draw_optical_tape_annotation(
    ui: &Ui,
    state: &SurveyState,
    explorer: &Explorer,
    result: Option<&Result<Option<earth_math::survey::TapeAnnotation>, String>>,
) {
    match result {
        Some(Ok(Some(annotation))) => draw_tape_result(ui, state, explorer, annotation),
        Some(Err(error)) => super::guide_caption(
            ui,
            &ui.get_foreground_draw_list(),
            [12.0, 112.0],
            super::ACCENT,
            &format!("Optical tape projection unresolved: {error}"),
        ),
        None if state.tape_a.is_some() => super::guide_caption(
            ui,
            &ui.get_foreground_draw_list(),
            [12.0, 112.0],
            super::ACCENT,
            "Projecting tape through the weather column...",
        ),
        _ => {}
    }
}

fn draw_tape_result(
    ui: &Ui,
    state: &SurveyState,
    explorer: &Explorer,
    annotation: &earth_math::survey::TapeAnnotation,
) {
    let draw = ui.get_foreground_draw_list();
    let color = [0.98, 0.96, 0.70, 1.0];
    draw.with_draw_flags(DrawFlags::AA_FILL, false, || {
        for &[p1, p2, p3, p4] in &annotation.quads {
            draw.add_quad_filled(p1, p2, p3, p4, color);
        }
    });
    if let Some(measurement) = &state.tape_measurement {
        let label = format!(
            "A to B: {} straight\n{} WGS84 surface",
            explorer
                .units
                .format_distance(measurement.straight_distance_m),
            explorer
                .units
                .format_distance(measurement.surface_distance_m),
        );
        super::guide_caption(ui, &draw, annotation.label_position, color, &label);
    }
    for (point, name, color) in [
        (annotation.endpoint_positions[0], "A", [0.2, 0.9, 1.0, 1.0]),
        (annotation.endpoint_positions[1], "B", [1.0, 0.9, 0.2, 1.0]),
    ] {
        if let Some(point) = point {
            super::guide_caption(ui, &draw, point, color, name);
        }
    }
}

pub(super) fn draw_marker(ui: &Ui, state: &SurveyState, units: UnitSystem) -> bool {
    let Some(hit) = state.hover else {
        return false;
    };
    let SurveyTarget::Marker(id) = hit.point.target else {
        return false;
    };
    ui.text(units.format_distance(hit.distance_m));
    ui.text_colored(super::MUTED, "Straight-line range from the eyes");
    ui.text_colored(
        super::ACCENT,
        format!("MARKER {}", marker_label(id).unwrap_or_else(|| "?".into())),
    );
    if let Some(marker) = state.markers.iter().find(|marker| marker.id == id) {
        ui.text_wrapped(format!(
            "Station: {} along the row",
            units.format_distance(marker.along_distance_m)
        ));
    }
    if let Some(measurement) = &state.occlusion {
        ui.text_wrapped(measurement.summary(units));
    } else if state.occlusion_pending {
        ui.text_wrapped("Measuring hidden height along the marker's centerline...");
    }
    true
}

pub(super) fn draw_clean_overlay(ui: &Ui, state: &SurveyState, units: UnitSystem) {
    let mut lines = Vec::new();
    if let Some(hit) = state.hover
        && let SurveyTarget::Marker(id) = hit.point.target
    {
        lines.push(format!(
            "Marker {} / {} from eyes",
            marker_label(id).unwrap_or_else(|| "?".into()),
            units.format_distance(hit.distance_m)
        ));
        if let Some(measurement) = &state.occlusion {
            lines.push(measurement.summary(units));
        } else if state.occlusion_pending {
            lines.push("Measuring hidden height...".into());
        }
    }
    if state.tape_visible {
        if let Some(measurement) = &state.tape_measurement {
            lines.push(format!(
                "Tape A to B: {} / straight line",
                units.format_distance(measurement.straight_distance_m)
            ));
        } else if state.tape_a.is_some() {
            lines.push("Tape A selected: aim and left-click for B".into());
        } else if state.tape_enabled {
            lines.push("Tape: aim and left-click for A, then B [M to hide]".into());
        }
    }
    if !lines.is_empty() {
        let draw = ui.get_foreground_draw_list();
        super::guide_caption(ui, &draw, [12.0, 86.0], super::ACCENT, &lines.join("\n"));
    }
}
