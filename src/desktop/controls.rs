use super::{AutomatedView, Explorer};
use ancient_earth::simulation::AstronomicalScene;
use ancient_earth::surface::WorldSurface;
use ancient_earth::survey::{SurveyState, TapeAction};
use bevy::{
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
    window::{
        CursorGrabMode, CursorOptions, MonitorSelection, PrimaryWindow, WindowMode, WindowPosition,
        WindowResolution,
    },
};
use dear_imgui_bevy::ImguiFrame;
use dear_imgui_bevy::input::ImguiInputCapture;
use earth_math::navigation::FlightController;
use earth_math::pixels_to_wheel_lines;

#[derive(Default)]
pub struct WindowedState(Option<(WindowResolution, WindowPosition)>);

fn toggle_window_mode(window: &mut Window, saved: &mut WindowedState) {
    if window.mode == WindowMode::Windowed {
        saved.0 = Some((window.resolution.clone(), window.position));
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
    } else {
        window.mode = WindowMode::Windowed;
        if let Some((resolution, position)) = saved.0.take() {
            window.resolution = resolution;
            window.position = position;
        }
    }
}

pub fn fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    automated: Res<AutomatedView>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut saved: Local<WindowedState>,
) {
    if automated.0 {
        return;
    }
    if let Ok(mut window) = windows.single_mut()
        && window.focused
        && keys.just_pressed(KeyCode::F5)
    {
        toggle_window_mode(&mut window, &mut saved);
    }
}

fn axis(keys: &ButtonInput<KeyCode>, positive: KeyCode, negative: KeyCode) -> f64 {
    button_axis(keys.pressed(positive), keys.pressed(negative))
}

fn button_axis(positive: bool, negative: bool) -> f64 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

struct FlightInput {
    axes: [f64; 3],
    roll_right: f64,
    precision: bool,
}

impl FlightInput {
    fn from_keys(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            axes: [
                axis(keys, KeyCode::KeyW, KeyCode::KeyS),
                axis(keys, KeyCode::KeyD, KeyCode::KeyA),
                button_axis(
                    keys.pressed(KeyCode::Space),
                    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
                ),
            ],
            roll_right: axis(keys, KeyCode::KeyE, KeyCode::KeyQ),
            precision: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        }
    }
}

fn display_shortcuts(state: &mut Explorer, keys: &ButtonInput<KeyCode>, enabled: bool) {
    if !enabled {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        state.show_panel = !state.show_panel;
    }
    if keys.just_pressed(KeyCode::F2) {
        state.show_ui = !state.show_ui;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        state.show_body_info = !state.show_body_info;
        if state.show_body_info {
            state.looking = false;
        }
    }
}

fn keyboard_shortcuts(state: &mut Explorer, keys: &ButtonInput<KeyCode>, enabled: bool) {
    state.update_crouch(enabled && keys.pressed(KeyCode::KeyC));
    if !enabled {
        return;
    }
    display_shortcuts(state, keys, enabled);
    if keys.just_pressed(KeyCode::KeyT) {
        state.telescope = !state.telescope;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        state.reset_view();
    }
    if keys.just_pressed(KeyCode::KeyL) {
        state.set_horizontal_line(!state.show_horizontal_line);
    }
    if state.show_horizontal_line && keys.just_pressed(KeyCode::KeyV) {
        state.show_vertical_line = !state.show_vertical_line;
    }
    if keys.just_pressed(KeyCode::F9) {
        state.follow_earth = !state.follow_earth;
    }
}

fn survey_shortcuts(
    state: &mut Explorer,
    survey: &mut SurveyState,
    keys: &ButtonInput<KeyCode>,
    enabled: bool,
) {
    if !enabled || survey.dialog_open {
        return;
    }
    if keys.just_pressed(KeyCode::Digit1) || keys.just_pressed(KeyCode::Numpad1) {
        state.looking = false;
        survey.open_marker_dialog(state.observer);
    } else if keys.just_pressed(KeyCode::KeyM) {
        survey.set_tape_enabled(!survey.tape_enabled);
    }
}

/// Capture is a click/release interaction, independent of camera geometry.
fn update_look_capture(
    looking: &mut bool,
    focused: bool,
    blocked: bool,
    pointer_over_ui: bool,
    escape: bool,
    background_click: bool,
) -> bool {
    if !focused || blocked || escape {
        *looking = false;
        return false;
    }
    let acquired = !*looking && background_click && !pointer_over_ui;
    if acquired {
        *looking = true;
    }
    acquired
}

fn apply_cursor_capture(cursor: &mut CursorOptions, looking: bool) {
    let mode = if looking {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if cursor.grab_mode != mode {
        cursor.grab_mode = mode;
    }
    if cursor.visible == looking {
        cursor.visible = !looking;
    }
}

/// Dear ImGui applies its requested cursor after the UI pass. Give that bridge
/// the same visibility decision as Bevy so it cannot restore a captured cursor.
pub fn cursor_feedback(frame: ImguiFrame<'_>, state: Res<Explorer>) {
    if state.looking {
        frame.ui().set_mouse_cursor(None);
    }
}

#[allow(clippy::too_many_arguments)] // Bevy injects independent system parameters.
pub fn fly(
    _frame: ImguiFrame<'_>,
    automated: Res<AutomatedView>,
    mut state: ResMut<Explorer>,
    loading: Res<super::graphics_loading::GraphicsLoading>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    capture: Res<ImguiInputCapture>,
    surface: Res<WorldSurface>,
    astronomy: Res<AstronomicalScene>,
    mut flight: Local<FlightController>,
    mut survey: ResMut<SurveyState>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let _timing = super::cpu_watch::Section::start("flight controls");
    if automated.0 {
        return;
    }
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    if loading.startup_blocked() {
        state.looking = false;
        apply_cursor_capture(&mut cursor, false);
        motion.clear();
        wheel.clear();
        return;
    }
    let capture = capture.primary();
    let keyboard_enabled = window.focused && !capture.wants_keyboard_input() && !survey.dialog_open;
    let blocked = survey.dialog_open || state.show_body_info || state.show_licenses;
    let acquired = update_look_capture(
        &mut state.looking,
        window.focused,
        blocked,
        capture.wants_pointer_input(),
        keys.just_pressed(KeyCode::Escape),
        buttons.just_pressed(MouseButton::Left),
    );
    keyboard_shortcuts(&mut state, &keys, keyboard_enabled);
    survey_shortcuts(&mut state, &mut survey, &keys, keyboard_enabled);
    if survey.dialog_open || state.show_body_info || state.show_licenses {
        state.looking = false;
    }
    apply_cursor_capture(&mut cursor, state.looking);
    state.apply_horizon_lock();
    let lens = state.lens();
    for event in motion.read() {
        if state.looking && !acquired {
            if event.delta != Vec2::ZERO {
                state.cancel_body_tracking();
            }
            state.observer.apply_free_look(
                f64::from(event.delta.x),
                f64::from(event.delta.y),
                lens,
            );
            state.apply_horizon_lock();
        }
    }
    for event in wheel.read() {
        if window.focused && (!capture.wants_pointer_input() || state.looking) {
            let lines = match event.unit {
                MouseScrollUnit::Line => f64::from(event.y),
                MouseScrollUnit::Pixel => pixels_to_wheel_lines(f64::from(event.y)),
            };
            if state.telescope {
                state.telescope_lens.zoom(lines);
            } else {
                flight.scroll(&mut state.observer, lines);
            }
        }
    }
    let scene = surface.active(state.boundaries_enabled);
    let input = FlightInput::from_keys(&keys);
    state.flight_speed_mps = flight.effective_speed(
        scene,
        state.observer,
        astronomy.frame.as_ref(),
        state.flight_settings,
        false,
        input.precision && state.looking && keyboard_enabled,
    );
    if survey.dialog_open {
        return;
    }
    if survey.tape_enabled && state.looking && !acquired && buttons.just_pressed(MouseButton::Left)
    {
        survey.request_tape_point(TapeAction::Next);
    }
    if !keyboard_enabled {
        return;
    }
    if state.show_horizontal_line && state.show_vertical_line {
        state.guide_column = earth_math::guides::move_column(
            state.guide_column,
            axis(&keys, KeyCode::ArrowRight, KeyCode::ArrowLeft),
            time.delta_secs_f64(),
        );
    }
    if keys.just_pressed(KeyCode::KeyG) {
        let scene = surface.active(state.boundaries_enabled);
        state.ground_on_surface(scene);
        return;
    }
    if keys.just_pressed(KeyCode::KeyO) {
        state.cancel_body_tracking();
        state.observer.overview();
    }
    if !state.looking {
        return;
    }
    if !state.horizon_lock_active() {
        if input.roll_right != 0.0 {
            state.cancel_body_tracking();
        }
        state
            .observer
            .apply_body_roll(input.roll_right, time.delta_secs_f64());
    }
    let settings = state.flight_settings;
    let report = flight.step_body(
        scene,
        &mut state.observer,
        astronomy.frame.as_ref(),
        settings,
        input.axes,
        time.delta_secs_f64(),
        false,
        input.precision,
    );
    state.flight_speed_mps = report.speed_mps;
    state.apply_horizon_lock();
    if report.blocked {
        const STOPPED: &str = "Movement stopped at a modeled surface, navigation limit, or conservative terrain boundary. Turn or rise to continue.";
        if state.status != STOPPED {
            state.status = STOPPED.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_bindings_separate_roll_from_body_translation() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyQ);
        let input = FlightInput::from_keys(&keys);
        assert_eq!(input.roll_right, -1.0);
        assert_eq!(input.axes, [0.0; 3]);
        keys.release(KeyCode::KeyQ);
        keys.press(KeyCode::KeyE);
        let input = FlightInput::from_keys(&keys);
        assert_eq!(input.roll_right, 1.0);
        assert_eq!(input.axes, [0.0; 3]);
        keys.press(KeyCode::KeyQ);
        assert_eq!(FlightInput::from_keys(&keys).roll_right, 0.0);

        for shift in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            keys.reset_all();
            keys.press(shift);
            assert_eq!(FlightInput::from_keys(&keys).axes, [0.0, 0.0, -1.0]);
            keys.press(KeyCode::Space);
            assert_eq!(FlightInput::from_keys(&keys).axes, [0.0; 3]);
            keys.release(shift);
            assert_eq!(FlightInput::from_keys(&keys).axes, [0.0, 0.0, 1.0]);
        }
        keys.press(KeyCode::ShiftLeft);
        keys.press(KeyCode::ShiftRight);
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyA);
        keys.press(KeyCode::ControlRight);
        let input = FlightInput::from_keys(&keys);
        assert_eq!(input.axes, [1.0, -1.0, 0.0]);
        assert!(input.precision);
    }

    #[test]
    fn background_click_latches_look_until_escape_focus_loss_or_modal() {
        let mut looking = false;
        assert!(!update_look_capture(
            &mut looking,
            true,
            false,
            true,
            false,
            true
        ));
        assert!(!looking, "a GUI click must not capture the cursor");
        assert!(update_look_capture(
            &mut looking,
            true,
            false,
            false,
            false,
            true
        ));
        assert!(looking);
        assert!(!update_look_capture(
            &mut looking,
            true,
            false,
            true,
            false,
            false
        ));
        assert!(
            looking,
            "button release and later UI hover cannot end captured look"
        );
        assert!(!update_look_capture(
            &mut looking,
            true,
            false,
            false,
            true,
            true
        ));
        assert!(!looking, "Escape wins over a simultaneous capture click");
        for (focused, blocked) in [(false, false), (true, true)] {
            looking = true;
            assert!(!update_look_capture(
                &mut looking,
                focused,
                blocked,
                false,
                false,
                true
            ));
            assert!(!looking);
        }
        assert!(!update_look_capture(
            &mut looking,
            true,
            false,
            false,
            false,
            false
        ));
        assert!(!looking, "returning focus does not automatically recapture");
    }

    #[test]
    fn captured_cursor_is_locked_hidden_and_restores_on_release() {
        let mut cursor = CursorOptions::default();
        apply_cursor_capture(&mut cursor, true);
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
        assert!(!cursor.visible);
        apply_cursor_capture(&mut cursor, true);
        apply_cursor_capture(&mut cursor, false);
        assert_eq!(cursor.grab_mode, CursorGrabMode::None);
        assert!(cursor.visible);
    }

    #[test]
    fn reset_view_shortcut_restores_normal_optics_without_moving_or_resetting_telescope() {
        let mut state = Explorer {
            normal_lens: earth_math::Lens::new(earth_math::Projection::EquidistantFisheye, 140.0),
            telescope: true,
            show_ui: false,
            ..Default::default()
        };
        state.telescope_lens.zoom(1.0);
        let observer = state.observer;
        let telescope = state.telescope_lens;
        let normal = state.normal_lens;
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyR);
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(state.telescope);
        assert_eq!(state.normal_lens, normal);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.telescope);
        assert_eq!(state.normal_lens, earth_math::Lens::default());
        assert_eq!(state.telescope_lens, telescope);
        assert_eq!(state.observer, observer);
        assert!(!state.show_ui);
        state.normal_lens = normal;
        keyboard_shortcuts(&mut state, &keys, true);
        assert_eq!(state.normal_lens, earth_math::Lens::default());
    }

    #[test]
    fn marker_shortcut_only_opens_menu_and_does_not_spawn_while_typing() {
        let mut state = Explorer::default();
        let mut survey = SurveyState::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Digit1);
        survey_shortcuts(&mut state, &mut survey, &keys, false);
        assert!(!survey.dialog_open);
        survey_shortcuts(&mut state, &mut survey, &keys, true);
        assert!(survey.dialog_open && survey.dialog_requested);
        assert_eq!(survey.placement_origin, Some(state.observer));
        assert!(survey.markers.is_empty() && survey.placement.is_none());
        keys.clear();
        keys.press(KeyCode::KeyM);
        survey_shortcuts(&mut state, &mut survey, &keys, true);
        assert!(!survey.tape_enabled, "modal captures shortcuts");
        survey.dialog_open = false;
        survey_shortcuts(&mut state, &mut survey, &keys, true);
        assert!(survey.tape_enabled);
    }

    #[test]
    fn fullscreen_restores_window_size_position_and_dpi() {
        let mut window = Window {
            resolution: WindowResolution::new(1920, 1080).with_scale_factor_override(1.5),
            position: WindowPosition::At(IVec2::new(75, 120)),
            ..default()
        };
        let original = window.resolution.clone();
        let mut saved = WindowedState::default();
        toggle_window_mode(&mut window, &mut saved);
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
        window.resolution.set_physical_resolution(3840, 2160);
        toggle_window_mode(&mut window, &mut saved);
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(window.resolution, original);
        assert_eq!(window.position, WindowPosition::At(IVec2::new(75, 120)));
    }

    #[test]
    fn crouch_releases_on_key_release_or_keyboard_capture_and_keeps_gui_lock() {
        let mut state = Explorer::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyC);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.posture.crouched);
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(!state.posture.crouched);
        keyboard_shortcuts(&mut state, &keys, true);
        keys.release(KeyCode::KeyC);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.posture.crouched);
        state.crouch_locked = true;
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(state.posture.crouched);
    }

    #[test]
    fn line_toggle_ignores_typing_and_only_toggles_on_press() {
        let mut state = Explorer::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(!state.show_horizontal_line);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_horizontal_line);
        keys.clear();
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_horizontal_line);
        keys.release(KeyCode::KeyL);
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_horizontal_line);
    }

    #[test]
    fn vertical_guide_requires_horizontal_and_survives_hiding_it() {
        let mut state = Explorer::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyV);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_vertical_line);
        state.show_horizontal_line = true;
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(state.show_vertical_line);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_vertical_line);
        keys.clear();
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_horizontal_line);
        assert!(!state.show_vertical_line);
        keys.clear();
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_vertical_line);
        keys.release(KeyCode::KeyL);
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_horizontal_line);
        assert!(state.show_vertical_line);
    }

    #[test]
    fn clean_view_preserves_independent_guide_controls() {
        let mut state = Explorer::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::F2);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_ui);
        keys.clear();
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_horizontal_line);
        assert!(state.show_vertical_line);
        keys.clear();
        keys.press(KeyCode::KeyV);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_vertical_line);
        keys.clear();
        keys.release(KeyCode::KeyV);
        keys.press(KeyCode::KeyV);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_vertical_line);
        keys.clear();
        keys.release(KeyCode::KeyL);
        keys.press(KeyCode::KeyL);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_horizontal_line);
        assert!(!state.show_ui);
        keys.clear();
        keys.release(KeyCode::F2);
        keys.press(KeyCode::F2);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_ui);
        assert!(!state.show_horizontal_line);
        assert!(state.show_vertical_line);
    }

    #[test]
    fn body_info_works_with_panels_hidden_and_ignores_typing() {
        let mut state = Explorer {
            show_ui: false,
            show_panel: false,
            show_sight_panel: false,
            show_help_panel: false,
            looking: true,
            ..Explorer::default()
        };
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyI);
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(!state.show_body_info);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_body_info);
        assert!(!state.looking);
        keys.clear();
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_body_info);
        keys.release(KeyCode::KeyI);
        keys.press(KeyCode::KeyI);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.show_body_info);
        keys.clear();
        keys.press(KeyCode::F2);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_ui);
        assert!(!state.show_panel);
        assert!(!state.show_sight_panel);
        assert!(!state.show_help_panel);
        keys.clear();
        keys.press(KeyCode::Tab);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.show_panel);
        assert!(!state.show_sight_panel);
        assert!(!state.show_help_panel);
    }

    #[test]
    fn earth_follow_toggle_respects_keyboard_capture_and_key_edges() {
        let mut state = Explorer::default();
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::F9);
        keyboard_shortcuts(&mut state, &keys, false);
        assert!(state.follow_earth);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.follow_earth);
        keys.clear();
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(!state.follow_earth);
        keys.release(KeyCode::F9);
        keys.press(KeyCode::F9);
        keyboard_shortcuts(&mut state, &keys, true);
        assert!(state.follow_earth);
    }
}
