//! Survey tool interaction state. Geometry and units are owned by earth-math.
use earth_math::{
    Observer,
    survey::{
        MarkerOcclusion, MarkerSettings, SurveyMarker, SurveyPick, TapeMeasurement, TapePoint,
    },
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeAction {
    Next,
    SetA,
    SetB,
}

#[derive(Clone, Copy)]
pub struct PlacementRequest {
    pub id: u64,
    pub observer: Observer,
    pub settings: MarkerSettings,
}

#[cfg_attr(feature = "desktop", derive(bevy::prelude::Resource))]
pub struct SurveyState {
    pub settings: MarkerSettings,
    pub markers: Arc<Vec<SurveyMarker>>,
    pub markers_visible: bool,
    pub generation: u64,
    pub dialog_requested: bool,
    pub dialog_open: bool,
    pub placement_origin: Option<Observer>,
    pub placement: Option<PlacementRequest>,
    pub tape_enabled: bool,
    pub tape_visible: bool,
    pub tape_a: Option<TapePoint>,
    pub tape_b: Option<TapePoint>,
    pub tape_measurement: Option<TapeMeasurement>,
    pub tape_action: Option<TapeAction>,
    pub hover: Option<SurveyPick>,
    pub occlusion: Option<MarkerOcclusion>,
    pub occlusion_pending: bool,
    pub status: String,
    pub last_error: Option<String>,
    next_request: u64,
}

impl Default for SurveyState {
    fn default() -> Self {
        Self {
            settings: MarkerSettings::default(),
            markers: Arc::new(Vec::new()),
            markers_visible: true,
            generation: 0,
            dialog_requested: false,
            dialog_open: false,
            placement_origin: None,
            placement: None,
            tape_enabled: false,
            tape_visible: false,
            tape_a: None,
            tape_b: None,
            tape_measurement: None,
            tape_action: None,
            hover: None,
            occlusion: None,
            occlusion_pending: false,
            status: "Press 1 to configure markers; M enables the tape measure.".into(),
            last_error: None,
            next_request: 0,
        }
    }
}

impl SurveyState {
    pub fn geometry_active(&self) -> bool {
        (self.markers_visible && !self.markers.is_empty()) || self.tape_enabled || self.tape_visible
    }

    pub fn open_marker_dialog(&mut self, observer: Observer) {
        self.placement_origin = Some(observer);
        self.dialog_requested = true;
        self.dialog_open = true;
        self.last_error = None;
    }

    pub fn request_placement(&mut self) -> bool {
        if let Err(error) = self.settings.validate() {
            self.last_error = Some(error.to_string());
            return false;
        }
        let Some(observer) = self.placement_origin else {
            self.last_error =
                Some("Open marker placement from your current position first.".into());
            return false;
        };
        self.next_request = self.next_request.wrapping_add(1);
        self.placement = Some(PlacementRequest {
            id: self.next_request,
            observer,
            settings: self.settings,
        });
        self.status = "Placing markers on the active ground surface...".into();
        self.last_error = None;
        true
    }

    pub fn cancel_placement(&mut self) {
        self.placement = None;
        self.status = "Marker placement cancelled; the previous row is unchanged.".into();
    }

    pub fn accept_markers(&mut self, markers: Arc<Vec<SurveyMarker>>) {
        self.status = format!(
            "Placed {} markers. Aim the dot at a visible marker to measure its hidden height.",
            markers.len()
        );
        self.markers = markers;
        self.markers_visible = true;
        self.placement = None;
        self.hover = None;
        self.occlusion = None;
        self.generation = self.generation.wrapping_add(1);
        self.last_error = None;
    }

    pub fn clear_markers(&mut self) {
        self.placement = None;
        self.markers = Arc::new(Vec::new());
        self.hover = None;
        self.occlusion = None;
        self.occlusion_pending = false;
        self.generation = self.generation.wrapping_add(1);
        self.status = "Markers cleared.".into();
    }

    pub fn set_tape_enabled(&mut self, enabled: bool) {
        self.tape_enabled = enabled;
        self.tape_visible = enabled;
        self.tape_action = None;
        self.generation = self.generation.wrapping_add(1);
        self.status = if enabled {
            "Tape active: aim the center dot, then left-click for A and again for B. M hides the tape."
        } else {
            "Tape hidden; saved endpoints are retained. Press M to restore it."
        }
        .into();
    }

    pub fn request_tape_point(&mut self, action: TapeAction) {
        self.tape_enabled = true;
        self.tape_visible = true;
        self.tape_action = Some(action);
    }

    pub fn clear_tape(&mut self) {
        self.tape_a = None;
        self.tape_b = None;
        self.tape_measurement = None;
        self.tape_action = None;
        self.generation = self.generation.wrapping_add(1);
        self.status = "Tape cleared. Aim the dot and left-click to select A.".into();
        self.last_error = None;
    }

    pub fn accept_tape_point(&mut self, action: TapeAction, point: TapePoint) {
        let action = match action {
            TapeAction::Next if self.tape_a.is_none() || self.tape_b.is_some() => TapeAction::SetA,
            TapeAction::Next => TapeAction::SetB,
            action => action,
        };
        match action {
            TapeAction::SetA => {
                self.tape_a = Some(point);
                self.tape_b = None;
                self.tape_measurement = None;
                self.status = "Point A set. Aim at point B and left-click.".into();
            }
            TapeAction::SetB => {
                let Some(a) = self.tape_a else {
                    self.last_error = Some("Select point A first.".into());
                    return;
                };
                match TapeMeasurement::between(a, point) {
                    Ok(measurement) => {
                        self.tape_b = Some(point);
                        self.tape_measurement = Some(measurement);
                        self.status = "A to B measured. The next click starts a new tape.".into();
                    }
                    Err(error) => {
                        self.last_error = Some(error.to_string());
                        return;
                    }
                }
            }
            TapeAction::Next => unreachable!(),
        }
        self.generation = self.generation.wrapping_add(1);
        self.last_error = None;
    }

    pub fn surface_changed(&mut self) {
        self.clear_markers();
        self.clear_tape();
        self.status = "Ground surface changed. Markers and tape points were cleared; place them again on the new surface.".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::survey::SurveyTarget;

    fn point() -> TapePoint {
        TapePoint {
            position_ecef_m: earth_math::boundary::default_observer().eye_position(),
            target: SurveyTarget::Surface,
        }
    }

    #[test]
    fn opening_or_cancelling_dialog_does_not_place_a_row() {
        let mut state = SurveyState::default();
        let observer = earth_math::boundary::default_observer();
        state.open_marker_dialog(observer);
        assert!(state.placement.is_none() && state.markers.is_empty());
        assert!(!state.geometry_active());
        assert!(state.request_placement());
        assert_eq!(state.placement.unwrap().observer, observer);
        state.cancel_placement();
        assert!(state.placement.is_none() && state.markers.is_empty());
    }

    #[test]
    fn tape_click_cycle_hides_without_losing_endpoints_and_restarts_cleanly() {
        let mut state = SurveyState::default();
        state.set_tape_enabled(true);
        state.accept_tape_point(TapeAction::Next, point());
        assert!(state.tape_a.is_some() && state.tape_b.is_none());
        state.accept_tape_point(TapeAction::Next, point());
        assert!(state.tape_measurement.is_some());
        state.set_tape_enabled(false);
        assert!(!state.geometry_active());
        assert!(state.tape_a.is_some() && state.tape_b.is_some());
        state.set_tape_enabled(true);
        state.accept_tape_point(TapeAction::Next, point());
        assert!(state.tape_a.is_some() && state.tape_b.is_none());
        assert!(state.tape_measurement.is_none());
        state.surface_changed();
        assert!(state.tape_a.is_none() && state.tape_b.is_none());
    }

    #[test]
    fn setting_b_before_a_keeps_state_unchanged_and_explains_failure() {
        let mut state = SurveyState::default();
        state.accept_tape_point(TapeAction::SetB, point());
        assert!(state.tape_a.is_none() && state.tape_b.is_none());
        assert_eq!(state.last_error.as_deref(), Some("Select point A first."));
    }
}
