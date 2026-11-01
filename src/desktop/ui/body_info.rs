use super::{ACCENT, Explorer, MUTED};
use ancient_earth::simulation::AstronomicalScene;
use dear_imgui_rs::{Condition, StyleColor, StyleVar, Ui, WindowFlags};
use earth_math::astro::info::AstronomyInfo;
use earth_math::rendering::layout::{centered_info_panel, rect_size};

pub(super) fn draw(ui: &Ui, state: &mut Explorer, astronomy: &AstronomicalScene) {
    if !state.show_body_info {
        return;
    }
    let Some(rect) = centered_info_panel(
        ui.io().display_size(),
        ui.io().display_framebuffer_scale(),
        [780.0, 760.0],
    ) else {
        return;
    };
    let info = AstronomyInfo::new(
        astronomy.clock.instant(),
        state.observer,
        state.follow_earth,
        astronomy.frame.as_ref(),
    );
    let _minimum = ui.push_style_var(StyleVar::WindowMinSize([0.0, 0.0]));
    ui.window("Earth, Sun & Moon / mathematical information [I]")
        .opened(&mut state.show_body_info)
        .position(rect.min, Condition::Always)
        .size(rect_size(rect), Condition::Always)
        .flags(WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE | WindowFlags::NO_COLLAPSE)
        .build(|| {
            ui.text_colored(ACCENT, &info.epoch);
            ui.text_wrapped(&info.status);
            ui.text_colored(
                MUTED,
                "Metric and Imperial together. Press I or the close button to return.",
            );
            if let Some(_tabs) = ui.tab_bar("Astronomical bodies") {
                for body in &info.bodies {
                    if let Some(_tab) = ui.tab_item(body.name) {
                        for row in &body.rows {
                            ui.text_colored(ACCENT, row.label);
                            ui.text_wrapped(&row.value);
                        }
                        ui.separator();
                        {
                            let _muted = ui.push_style_color(StyleColor::Text, MUTED);
                            for note in &body.notes {
                                ui.text_wrapped(note);
                            }
                        }
                        ui.separator();
                        ui.text("Sources and definitions");
                        for source in &body.sources {
                            ui.text_link_open_url(source.label, source.url);
                        }
                    }
                }
            }
        });
}
