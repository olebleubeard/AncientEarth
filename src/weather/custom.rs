//! Draft and source lifecycle only; earth-math owns all weather interpretation.
use super::WeatherState;
use earth_math::atmosphere::weather::custom::CustomWeatherDraft;
use earth_math::{TerrainScene, UnitSystem, atmosphere::AtmosphereProfile};
use std::sync::Arc;

#[derive(Default)]
pub struct CustomWeatherEditor {
    pub open: bool,
    pub active: bool,
    pub draft: Option<CustomWeatherDraft>,
    pub error: Option<String>,
    seed_profile: Option<Arc<AtmosphereProfile>>,
    seed_scene: Option<Arc<TerrainScene>>,
}

impl CustomWeatherEditor {
    pub(super) fn reset_source(&mut self) {
        self.active = false;
        self.draft = None;
        self.error = None;
        self.seed_profile = None;
        self.seed_scene = None;
    }
}

impl WeatherState {
    /// Opening the editor does not activate refraction or modify the live field.
    pub fn toggle_custom_editor(&mut self, units: UnitSystem) {
        self.custom.open = !self.custom.open;
        if self.custom.open {
            self.prepare_custom_editor(units);
        }
    }

    /// Seed once after a pending download arrives; never overwrite typed edits.
    pub fn prepare_custom_editor(&mut self, units: UnitSystem) {
        if self.custom.open && self.custom.draft.is_none() && self.profile.is_some() {
            self.reset_custom_draft(units);
        }
    }

    pub fn reset_custom_draft(&mut self, units: UnitSystem) {
        let Some(source) = self.profile.as_ref() else {
            self.custom.error = Some("Load weather for your current location first.".into());
            return;
        };
        let profile = self.registered_profile().unwrap_or(source.as_ref());
        match CustomWeatherDraft::from_profile(profile, units) {
            Ok(draft) => {
                self.custom.draft = Some(draft);
                self.custom.seed_profile = Some(Arc::clone(source));
                self.custom.seed_scene = self.anchored_scene.clone();
                self.custom.error = None;
            }
            Err(error) => self.custom.error = Some(error.to_string()),
        }
    }

    pub fn custom_draft_is_current(&self) -> bool {
        let same_source = match (&self.custom.seed_profile, &self.profile) {
            (Some(seed), Some(current)) => Arc::ptr_eq(seed, current),
            _ => false,
        };
        let same_scene = match (&self.custom.seed_scene, &self.anchored_scene) {
            (Some(seed), Some(current)) => Arc::ptr_eq(seed, current),
            (None, None) => true,
            _ => false,
        };
        same_source && same_scene
    }

    /// Validation and provenance construction complete before any active state changes.
    pub fn apply_custom_weather(&mut self) -> Result<(), String> {
        let result = self.validated_custom_profile();
        let profile = match result {
            Ok(profile) => Arc::new(profile),
            Err(error) => {
                self.custom.error = Some(error.clone());
                return Err(error);
            }
        };
        self.cancel_pending();
        self.wanted_key = None;
        self.completed_key = None;
        self.next_attempt = None;
        self.enabled = true;
        self.reference_snapshot = false;
        self.profile = Some(Arc::clone(&profile));
        // A draft seeded from the registered field retains that exact anchor.
        // The source and physical scene identities were checked before apply.
        if profile.anchor().is_some() && self.anchored_scene.is_some() {
            self.scene_profile = Some(Arc::clone(&profile));
            self.anchored_source = Some(Arc::clone(&profile));
        } else {
            self.scene_profile = None;
            self.anchored_source = None;
            self.anchored_scene = None;
        }
        self.scene_error = None;
        self.generation = self.generation.wrapping_add(1);
        self.custom.active = true;
        self.custom.error = None;
        self.custom.seed_profile = Some(profile);
        self.custom.seed_scene = self.anchored_scene.clone();
        self.from_cache = false;
        self.last_error = None;
        self.status = "Active - custom weather scenario; automatic weather updates paused.".into();
        Ok(())
    }

    fn validated_custom_profile(&self) -> Result<AtmosphereProfile, String> {
        if !self.custom_draft_is_current() {
            return Err("Weather or terrain changed while editing. Reset from current weather before applying.".into());
        }
        self.custom
            .draft
            .as_ref()
            .ok_or_else(|| {
                "Load the current location's weather to initialize the editor.".to_owned()
            })?
            .build_profile()
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::Pending;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    fn editor() -> WeatherState {
        let mut weather = WeatherState::default();
        weather.load_reference().unwrap();
        let observer = earth_math::atmosphere::weather::reference_observer();
        let scene = Arc::new(TerrainScene::boundary_only(Vec::new()).unwrap());
        weather.prepare_scene(scene, observer.feet);
        weather.toggle_custom_editor(UnitSystem::Imperial);
        weather
    }

    #[test]
    fn editing_and_invalid_apply_leave_active_weather_untouched() {
        let mut weather = editor();
        let source = weather.profile.clone().unwrap();
        let registered = weather.scene_profile.clone().unwrap();
        let generation = weather.generation;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, receiver) = mpsc::channel();
        weather.pending = Some(Pending {
            key: "pending-live-request".into(),
            cancelled: cancelled.clone(),
            result: Mutex::new(receiver),
        });
        weather.custom.draft.as_mut().unwrap().rows[0].temperature = "invalid".into();
        weather.prepare_custom_editor(UnitSystem::Imperial);
        assert_eq!(
            weather.custom.draft.as_ref().unwrap().rows[0].temperature,
            "invalid"
        );
        assert!(weather.apply_custom_weather().is_err());
        assert!(Arc::ptr_eq(&source, weather.profile.as_ref().unwrap()));
        assert!(Arc::ptr_eq(
            &registered,
            weather.scene_profile.as_ref().unwrap()
        ));
        assert_eq!(weather.generation, generation);
        assert!(weather.reference_snapshot && !weather.custom.active);
        assert!(!cancelled.load(Ordering::Relaxed));
        assert!(weather.loading());
    }

    #[test]
    fn apply_cancels_late_download_and_preserves_registered_scene() {
        let mut weather = editor();
        let anchor = weather.registered_profile().unwrap().anchor();
        let scene = weather.anchored_scene.clone().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        weather.pending = Some(Pending {
            key: "pending-live-request".into(),
            cancelled: cancelled.clone(),
            result: Mutex::new(receiver),
        });
        weather.apply_custom_weather().unwrap();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(sender.send(Err("late result".into())).is_err());
        assert!(!weather.loading());
        assert!(weather.custom.active && weather.enabled && !weather.reference_snapshot);
        assert_eq!(weather.registered_profile().unwrap().anchor(), anchor);
        assert!(
            weather
                .profile
                .as_ref()
                .unwrap()
                .metadata
                .as_ref()
                .unwrap()
                .custom_origin
                .is_some()
        );
        let source = weather.profile.clone().unwrap();
        let generation = weather.generation;
        weather.prepare_scene(
            scene,
            earth_math::atmosphere::weather::reference_observer().feet,
        );
        assert!(Arc::ptr_eq(
            &source,
            weather.scene_profile.as_ref().unwrap()
        ));
        assert_eq!(weather.generation, generation);
        assert!(weather.custom_draft_is_current());
    }

    #[test]
    fn custom_weather_survives_toggle_and_live_refresh_restores_source_lifecycle() {
        let mut weather = editor();
        weather.apply_custom_weather().unwrap();
        let source = weather.profile.clone().unwrap();
        weather.set_enabled(false);
        assert!(weather.active_profile().is_none());
        assert!(Arc::ptr_eq(&source, weather.profile.as_ref().unwrap()));
        weather.set_enabled(true);
        weather.tick_at(
            earth_math::atmosphere::weather::reference_observer().feet,
            0,
        );
        assert!(Arc::ptr_eq(&source, weather.profile.as_ref().unwrap()));
        assert!(!weather.loading() && weather.wanted_key.is_none());
        weather.refresh();
        assert!(!weather.custom.active && weather.custom.draft.is_none());
        assert!(weather.enabled && !weather.reference_snapshot);
        assert!(weather.profile.is_none());
        assert!(weather.next_attempt.is_none());
    }

    #[test]
    fn stale_draft_cannot_replace_a_new_source_or_scene() {
        let mut weather = editor();
        let replacement = Arc::new(weather.profile.as_deref().unwrap().clone());
        weather.profile = Some(replacement.clone());
        assert!(!weather.custom_draft_is_current());
        assert!(weather.apply_custom_weather().is_err());
        assert!(Arc::ptr_eq(&replacement, weather.profile.as_ref().unwrap()));
        weather.reset_custom_draft(UnitSystem::Imperial);
        assert!(weather.custom_draft_is_current());
        weather.prepare_scene(
            Arc::new(TerrainScene::boundary_only(Vec::new()).unwrap()),
            earth_math::atmosphere::weather::reference_observer().feet,
        );
        assert!(!weather.custom_draft_is_current());
        assert!(weather.apply_custom_weather().is_err());
        weather.reset_custom_draft(UnitSystem::Imperial);
        weather.apply_custom_weather().unwrap();
    }

    #[test]
    fn empty_editor_waits_for_weather_without_inventing_a_profile() {
        let mut weather = WeatherState::default();
        weather.toggle_custom_editor(UnitSystem::Imperial);
        assert!(weather.custom.open && weather.custom.draft.is_none());
        assert!(!weather.enabled && !weather.loading());
        weather.load_reference().unwrap();
        weather.prepare_custom_editor(UnitSystem::Imperial);
        assert!(weather.custom.draft.is_some());
        assert!(!weather.custom.active);
    }
}
