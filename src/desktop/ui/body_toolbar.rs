use super::{ACCENT, Explorer, MUTED, edge_panels};
use ancient_earth::simulation::AstronomicalScene;
use dear_imgui_rs::{Condition, MouseButton, StyleColor, StyleVar, Ui, WindowFlags};
use earth_math::{
    astro::aim::ViewBody,
    rendering::layout::{
        body_icon, body_toolbar, body_toolbar_buttons, horizon_lock_icon, rect_size,
    },
};

#[derive(Debug, PartialEq)]
enum Action {
    Aim,
    ToggleTracking,
}

fn click_action(body: ViewBody, tracked: Option<ViewBody>, double_click: bool) -> Option<Action> {
    if double_click {
        Some(Action::ToggleTracking)
    } else if tracked == Some(body) {
        // The first click of a stopping double-click must retain the target.
        None
    } else {
        Some(Action::Aim)
    }
}

pub(super) fn draw(ui: &Ui, state: &mut Explorer, astronomy: &AstronomicalScene, modal: bool) {
    let scale = ui.io().display_framebuffer_scale();
    let layout = edge_panels::layout(ui, state);
    let Some(rect) = body_toolbar(ui.io().display_size(), scale, layout.as_ref()) else {
        return;
    };
    let Some(buttons) = body_toolbar_buttons(rect, scale) else {
        return;
    };
    let _padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let _minimum = ui.push_style_var(StyleVar::WindowMinSize([0.0, 0.0]));
    ui.window("View toolbar")
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
            let _disabled = ui.begin_disabled_with_cond(modal);
            for (body, button) in [ViewBody::Earth, ViewBody::Moon, ViewBody::Sun]
                .into_iter()
                .zip(buttons)
            {
                let tracked = state.body_tracking.target() == Some(body);
                let _color = tracked.then(|| {
                    ui.push_style_color(StyleColor::Button, [0.12, 0.43, 0.36, 1.0])
                });
                ui.set_cursor_screen_pos(button.min);
                ui.button_with_size(format!("##target-{body:?}"), rect_size(button));
                let hovered = ui.is_item_hovered();
                // Use presses only; a release must not repeat a double-click action.
                if !modal && hovered && ui.is_mouse_clicked(MouseButton::Left) {
                    state.looking = false;
                    let result = match click_action(
                        body,
                        state.body_tracking.target(),
                        ui.is_mouse_double_clicked(MouseButton::Left),
                    ) {
                        Some(Action::Aim) => state.aim_body(
                            body,
                            astronomy.frame.as_ref(),
                            astronomy.clock.instant(),
                        ),
                        Some(Action::ToggleTracking) => state.toggle_body_tracking(
                            body,
                            astronomy.frame.as_ref(),
                            astronomy.clock.instant(),
                        ),
                        None => Ok(()),
                    };
                    if let Err(error) = result {
                        state.status = error.to_string();
                    }
                }
                if let Some(strokes) = body_icon(body, button, scale) {
                    let draw = ui.get_window_draw_list();
                    let color = if modal {
                        MUTED
                    } else if tracked {
                        ACCENT
                    } else {
                        match body {
                            ViewBody::Earth => [0.43, 0.78, 1.0, 1.0],
                            ViewBody::Moon => [0.9, 0.91, 0.94, 1.0],
                            ViewBody::Sun => [1.0, 0.8, 0.3, 1.0],
                        }
                    };
                    for stroke in strokes {
                        draw.add_polyline(stroke, color).thickness(1.0).build();
                    }
                }
                if state.show_tooltips && hovered {
                    ui.tooltip_text(format!(
                        "{body:?}\nClick: aim at center.\nDouble-click: start / stop tracking.\n{}\nManual mouse look or unlocked Q/E roll stops tracking.\nStraight-axis aim; refraction can shift the visible image. Occluded targets remain hidden.",
                        if tracked { "Tracking is active (highlighted)." } else { "Tracking is off." },
                    ));
                }
            }
            let button = buttons[3];
            let _color = state.horizon_lock_enabled.then(|| {
                ui.push_style_color(StyleColor::Button, [0.12, 0.43, 0.36, 1.0])
            });
            ui.set_cursor_screen_pos(button.min);
            if ui.button_with_size("##horizon-lock", rect_size(button)) && !modal {
                state.looking = false;
                state.set_horizon_lock(!state.horizon_lock_enabled);
            }
            let hovered = ui.is_item_hovered();
            if let Some(strokes) = horizon_lock_icon(button, scale) {
                let color = if modal || !state.horizon_lock_enabled {
                    MUTED
                } else if state.horizon_lock_active() {
                    ACCENT
                } else {
                    [1.0, 0.8, 0.3, 1.0]
                };
                for stroke in strokes {
                    ui.get_window_draw_list()
                        .add_polyline(stroke, color)
                        .thickness(1.0)
                        .build();
                }
            }
            if state.show_tooltips && hovered {
                horizon_lock_tooltip(ui, state);
            }
        });
}

pub(super) fn horizon_lock_tooltip(ui: &Ui, state: &Explorer) {
    ui.tooltip_text(format!(
        "Horizon lock: {}\nPrevents sideways tilt; looking up/down stays free.\nQ/E roll is disabled while active. Earth curvature and horizon dip remain visible.\nAutomatically active through {} WGS84 eye height; free roll above. This is a control threshold, not a physical atmosphere edge.\nTurn off for unrestricted roll, including F9 inertial orientation.",
        if !state.horizon_lock_enabled {
            "off"
        } else if state.horizon_lock_active() {
            "active"
        } else {
            "armed for return to Earth"
        },
        state.units.format_distance(earth_math::navigation::HORIZON_LOCK_MAX_HEIGHT_M),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_can_start_and_stop_tracking_without_first_click_clearing_it() {
        assert_eq!(click_action(ViewBody::Moon, None, false), Some(Action::Aim));
        assert_eq!(
            click_action(ViewBody::Moon, None, true),
            Some(Action::ToggleTracking)
        );
        assert_eq!(
            click_action(ViewBody::Moon, Some(ViewBody::Moon), false),
            None
        );
        assert_eq!(
            click_action(ViewBody::Moon, Some(ViewBody::Moon), true),
            Some(Action::ToggleTracking)
        );
        assert_eq!(
            click_action(ViewBody::Sun, Some(ViewBody::Moon), false),
            Some(Action::Aim)
        );
        assert_eq!(
            click_action(ViewBody::Sun, Some(ViewBody::Moon), true),
            Some(Action::ToggleTracking)
        );
    }
}
