use super::Explorer;
use dear_imgui_rs::{Condition, StyleVar, Ui, WindowFlags};
use earth_math::ScreenRect;
use earth_math::rendering::layout::{EdgePanelLayout, edge_panel_layout, rect_size};

pub(super) fn layout(ui: &Ui, state: &Explorer) -> Option<EdgePanelLayout> {
    state
        .show_ui
        .then(|| {
            edge_panel_layout(
                ui.io().display_size(),
                ui.io().display_framebuffer_scale(),
                [
                    state.show_panel,
                    state.show_sight_panel,
                    state.show_help_panel,
                ],
            )
        })
        .flatten()
}

fn handle(
    ui: &Ui,
    name: &str,
    rect: ScreenRect,
    arrow: &str,
    tooltip: &str,
    tooltips: bool,
) -> bool {
    let _font = ui.push_font_with_size(None, 12.0);
    let _padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let _minimum = ui.push_style_var(StyleVar::WindowMinSize([0.0, 0.0]));
    let _frame = ui.push_style_var(StyleVar::FramePadding([0.0, 0.0]));
    let mut clicked = false;
    ui.window(name)
        .position(rect.min, Condition::Always)
        .size(rect_size(rect), Condition::Always)
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SCROLL_WITH_MOUSE
                | WindowFlags::NO_SAVED_SETTINGS
                | WindowFlags::NO_FOCUS_ON_APPEARING,
        )
        .build(|| {
            clicked = ui.button_with_size(arrow, rect_size(rect));
            if tooltips && ui.is_item_hovered() {
                ui.tooltip_text(tooltip);
            }
        });
    clicked
}

pub(super) fn draw_handles(ui: &Ui, state: &mut Explorer) {
    let Some(layout) = layout(ui, state) else {
        return;
    };
    if handle(
        ui,
        "Navigation edge",
        layout.left.handle,
        if state.show_panel { "<" } else { ">" },
        if state.show_panel {
            "Collapse navigation"
        } else {
            "Show navigation"
        },
        state.show_tooltips,
    ) {
        state.show_panel = !state.show_panel;
    }
    if handle(
        ui,
        "Measurements edge",
        layout.right.handle,
        if state.show_sight_panel { ">" } else { "<" },
        if state.show_sight_panel {
            "Collapse measurements"
        } else {
            "Show measurements"
        },
        state.show_tooltips,
    ) {
        state.show_sight_panel = !state.show_sight_panel;
    }
    if handle(
        ui,
        "Flight help edge",
        layout.bottom.handle,
        if state.show_help_panel { "v" } else { "^" },
        if state.show_help_panel {
            "Collapse flight help"
        } else {
            "Show flight help"
        },
        state.show_tooltips,
    ) {
        state.show_help_panel = !state.show_help_panel;
    }
}
