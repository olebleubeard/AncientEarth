use ancient_earth::weather::WeatherState;
use dear_imgui_rs::{TreeNodeFlags, Ui};
use earth_math::UnitSystem;

pub(super) fn draw(ui: &Ui, weather: &mut WeatherState, units: UnitSystem) {
    if ui.small_button(if weather.custom.open {
        "Hide custom weather"
    } else {
        "Custom weather"
    }) {
        weather.toggle_custom_editor(units);
    }
    if !weather.custom.open {
        return;
    }
    weather.prepare_custom_editor(units);
    ui.separator();
    ui.text_wrapped("Edit the current weather column, then apply. Changes here do not affect the view until applied.");
    let current = weather.custom_draft_is_current();
    if weather.custom.draft.is_some() {
        if !current {
            ui.text_wrapped(
                "Weather or terrain changed while editing. Reset edits to use the current profile.",
            );
        }
        {
            let _disabled = ui.begin_disabled_with_cond(!current);
            if ui.button("Apply custom weather") {
                let _ = weather.apply_custom_weather();
            }
        }
        ui.same_line();
        if ui.small_button("Reset edits") {
            weather.reset_custom_draft(units);
        }
    }
    if let Some(error) = &weather.custom.error {
        ui.text_wrapped(error);
    }
    if let Some(draft) = &mut weather.custom.draft {
        if draft.units() != units
            && let Err(error) = draft.set_units(units)
        {
            weather.custom.error = Some(error.to_string());
        }
        let temperature_label = match draft.units() {
            UnitSystem::Imperial => "Temperature (°F)",
            UnitSystem::Metric => "Temperature (°C)",
        };
        ui.text_wrapped("Expand a layer to edit its air conditions. Layer heights stay at their source values. Humidity is water mass in grams per kilogram of moist air, not relative humidity (%).");
        let _editor_id = ui.push_id("custom-weather-layers");
        for (index, row) in draft.rows.iter_mut().enumerate() {
            let _id = ui.push_id(index);
            if ui.collapsing_header(row.height_label(), TreeNodeFlags::empty()) {
                ui.text(temperature_label);
                ui.set_next_item_width(-1.0);
                let mut changed = ui.input_text("##temperature", &mut row.temperature).build();
                ui.text("Pressure (hPa)");
                ui.set_next_item_width(-1.0);
                changed |= ui.input_text("##pressure", &mut row.pressure_hpa).build();
                ui.text("Specific humidity (g/kg)");
                ui.set_next_item_width(-1.0);
                changed |= ui
                    .input_text("##humidity", &mut row.specific_humidity_g_kg)
                    .build();
                if changed {
                    weather.custom.error = None;
                }
            }
        }
    } else {
        ui.text_wrapped(if weather.loading() {
            "Downloading weather for this location. The editor will fill in when it is ready."
        } else {
            "Enable weather refraction and load weather for your current location to initialize the editor."
        });
    }
    ui.separator();
}
