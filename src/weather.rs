//! Live NOAA transport lifecycle. All physical interpretation lives in earth-math.
mod custom;
mod transport;
pub use custom::CustomWeatherEditor;

use earth_math::{
    Geodetic,
    atmosphere::{
        AtmosphereProfile,
        weather::{WeatherProfile, WeatherRequest, decode_gfs, decode_weather, weather_requests},
    },
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Download {
    profile: WeatherProfile,
    cached: bool,
    fetched_unix: u64,
}
struct Pending {
    key: String,
    cancelled: Arc<AtomicBool>,
    result: Mutex<mpsc::Receiver<Result<Download, String>>>,
}

#[cfg_attr(feature = "desktop", derive(bevy::prelude::Resource))]
pub struct WeatherState {
    pub enabled: bool,
    pub custom: CustomWeatherEditor,
    pub profile: Option<Arc<AtmosphereProfile>>,
    scene_profile: Option<Arc<AtmosphereProfile>>,
    anchored_source: Option<Arc<AtmosphereProfile>>,
    anchored_scene: Option<Arc<earth_math::TerrainScene>>,
    pub scene_error: Option<String>,
    pub display_block_reason: Option<String>,
    pub generation: u64,
    pub status: String,
    pub last_error: Option<String>,
    pub reference_snapshot: bool,
    pub fetched_unix: Option<u64>,
    pub from_cache: bool,
    pending: Option<Pending>,
    wanted_key: Option<String>,
    completed_key: Option<String>,
    location_changed: Instant,
    next_attempt: Option<Instant>,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            enabled: false,
            custom: CustomWeatherEditor::default(),
            profile: None,
            scene_profile: None,
            anchored_source: None,
            anchored_scene: None,
            scene_error: None,
            display_block_reason: None,
            generation: 0,
            status: "Off - geometric view; no weather downloads.".into(),
            last_error: None,
            reference_snapshot: false,
            fetched_unix: None,
            from_cache: false,
            pending: None,
            wanted_key: None,
            completed_key: None,
            location_changed: Instant::now(),
            next_attempt: None,
        }
    }
}

impl Drop for WeatherState {
    fn drop(&mut self) {
        self.cancel_pending();
    }
}

impl WeatherState {
    pub fn active_profile(&self) -> Option<&AtmosphereProfile> {
        self.enabled.then_some(self.profile.as_deref()).flatten()
    }

    /// A single display-mode decision shared by renderer, range and guides.
    pub fn view_profile(&self, observer: earth_math::Observer) -> Option<&AtmosphereProfile> {
        if self.display_block_reason.is_some() {
            return None;
        }
        self.active_profile()
            .filter(|_| earth_math::atmosphere::gpu::display_supported(observer))
    }

    /// Graphics eligibility is shared with every optical measurement client.
    /// Keep downloaded weather available, but invalidate any old optical jobs.
    pub fn set_display_block_reason(&mut self, reason: Option<&str>) {
        if self.display_block_reason.as_deref() == reason {
            return;
        }
        self.display_block_reason = reason.map(str::to_owned);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Render and measurement clients use the same terrain-anchored column.
    pub fn view_profile_for_scene(
        &self,
        observer: earth_math::Observer,
        scene: &earth_math::TerrainScene,
    ) -> Option<&AtmosphereProfile> {
        self.view_profile(observer)?;
        self.anchored_scene
            .as_ref()
            .filter(|active| std::ptr::eq(active.as_ref(), scene))?;
        self.registered_profile()
    }

    pub fn registered_profile(&self) -> Option<&AtmosphereProfile> {
        self.active_profile()?;
        let source = self.profile.as_ref()?;
        self.anchored_source
            .as_ref()
            .filter(|active| Arc::ptr_eq(active, source))?;
        self.scene_profile.as_deref()
    }

    /// Re-anchor only when the accepted source or immutable physical scene changes.
    pub fn prepare_scene(&mut self, scene: Arc<earth_math::TerrainScene>, location: Geodetic) {
        let source_unchanged = match (&self.anchored_source, &self.profile) {
            (Some(previous), Some(current)) => Arc::ptr_eq(previous, current),
            (None, None) => true,
            _ => false,
        };
        if source_unchanged
            && self
                .anchored_scene
                .as_ref()
                .is_some_and(|previous| Arc::ptr_eq(previous, &scene))
        {
            return;
        }
        self.scene_profile = None;
        self.scene_error = None;
        if let Some(profile) = &self.profile {
            match profile.anchored_to_scene_at(&scene, location) {
                Ok(anchored) => self.scene_profile = Some(Arc::new(anchored)),
                Err(error) => {
                    self.scene_error = Some(format!(
                        "Weather column cannot be registered to this terrain: {error}"
                    ))
                }
            }
        }
        self.anchored_source = self.profile.clone();
        self.anchored_scene = Some(scene);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancelled.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.generation = self.generation.wrapping_add(1);
        self.cancel_pending();
        self.wanted_key = None;
        self.completed_key = None;
        if !enabled && !self.reference_snapshot && !self.custom.active {
            self.clear_profile();
        }
        self.last_error = None;
        self.status = if enabled && self.custom.active && self.profile.is_some() {
            "Active - custom weather scenario; automatic weather updates paused.".into()
        } else if enabled && self.reference_snapshot && self.profile.is_some() {
            "Fixed NOAA verification snapshot - not live weather.".into()
        } else if enabled {
            "Waiting for a validated weather profile...".into()
        } else {
            "Off - geometric view; no new weather downloads.".into()
        };
    }

    pub fn refresh(&mut self) {
        if self.reference_snapshot || self.custom.active {
            self.clear_profile();
        }
        self.custom.reset_source();
        self.reference_snapshot = false;
        self.completed_key = None;
        self.wanted_key = None;
        self.next_attempt = None;
        self.cancel_pending();
        self.set_enabled(true);
        self.status = "Live weather refresh queued.".into();
    }

    pub fn load_reference(&mut self) -> Result<(), String> {
        let request = earth_math::atmosphere::weather::reference_request();
        let loaded = decode_gfs(
            include_bytes!("../assets/weather/gfs-reference.grib2"),
            &request,
        )
        .map_err(|error| error.to_string())?;
        self.cancel_pending();
        self.custom.reset_source();
        self.enabled = true;
        self.reference_snapshot = true;
        self.profile = Some(Arc::new(loaded.profile));
        self.generation = self.generation.wrapping_add(1);
        self.status = "Fixed NOAA verification snapshot - not live weather.".into();
        self.last_error = None;
        Ok(())
    }

    /// Explicit source-verification fixture; never used as a live fallback.
    pub fn load_regional_reference(&mut self) -> Result<(), String> {
        let request = earth_math::atmosphere::weather::regional_reference_request();
        let loaded = decode_weather(
            include_bytes!("../assets/weather/gfs-regional-reference.grib2"),
            Some(include_bytes!("../assets/weather/hrrr-reference.grib2")),
            &request,
        )
        .map_err(|error| error.to_string())?;
        if let Some(error) = loaded.regional_fallback {
            return Err(error);
        }
        self.cancel_pending();
        self.custom.reset_source();
        self.enabled = true;
        self.reference_snapshot = true;
        self.profile = Some(Arc::new(loaded.profile));
        self.generation = self.generation.wrapping_add(1);
        self.status = "Fixed HRRR + GFS verification snapshot - not live weather.".into();
        self.last_error = None;
        Ok(())
    }

    /// Called after navigation, before rendering, so an old location's profile
    /// cannot remain active during a teleport or a source-grid transition.
    pub fn tick(&mut self, location: Geodetic) {
        if !self.enabled || self.reference_snapshot || self.custom.active {
            return;
        }
        match transport::unix_now() {
            Ok(now) => self.tick_at(location, now),
            Err(error) => {
                self.cancel_pending();
                self.clear_profile();
                self.last_error = Some(error);
                self.status = "Weather unavailable - geometric view.".into();
            }
        }
    }

    fn clear_profile(&mut self) {
        self.scene_profile = None;
        self.anchored_source = None;
        self.scene_error = None;
        if self.profile.take().is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn tick_at(&mut self, location: Geodetic, now_unix: u64) {
        if !self.enabled || self.reference_snapshot || self.custom.active {
            return;
        }
        if self.profile.as_ref().is_some_and(|profile| {
            profile
                .metadata
                .as_ref()
                .is_none_or(|metadata| !metadata.usable_at(location, now_unix as f64))
        }) {
            self.clear_profile();
            self.status =
                "Profile expired or location changed - geometric view until updated.".into();
        }
        let candidates = match weather_requests(location.lat_deg, location.lon_deg, now_unix as f64)
        {
            Ok(candidates) if !candidates.is_empty() => candidates,
            Ok(_) => return,
            Err(error) => {
                self.cancel_pending();
                self.clear_profile();
                self.last_error = Some(error.to_string());
                self.status = "Weather unavailable - geometric view.".into();
                return;
            }
        };
        let key = &candidates[0].key;
        if self.wanted_key.as_ref() != Some(key) {
            self.cancel_pending();
            self.wanted_key = Some(key.clone());
            self.location_changed = Instant::now();
        }
        let received = self.pending.as_mut().and_then(|pending| {
            let result = pending
                .result
                .get_mut()
                .expect("weather receiver mutex unpoisoned")
                .try_recv();
            match result {
                Ok(result) => Some((pending.key.clone(), result)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some((
                    pending.key.clone(),
                    Err("Weather worker stopped without a result".into()),
                )),
            }
        });
        if let Some((received_key, result)) = received {
            self.pending = None;
            self.next_attempt = Some(Instant::now() + Duration::from_secs(60));
            if self.wanted_key.as_ref() == Some(&received_key) {
                match result {
                    Ok(download)
                        if download
                            .profile
                            .metadata
                            .usable_at(location, now_unix as f64) =>
                    {
                        self.profile = Some(Arc::new(download.profile.profile));
                        self.from_cache = download.cached;
                        self.fetched_unix = Some(download.fetched_unix);
                        self.generation = self.generation.wrapping_add(1);
                        self.completed_key = Some(received_key);
                        self.last_error = download.profile.regional_fallback;
                        self.status = if download.cached {
                            "Active - validated NOAA forecast from local cache.".into()
                        } else {
                            "Active - live NOAA forecast profile.".into()
                        };
                    }
                    Ok(_) => {
                        self.last_error = Some(
                            "Downloaded profile no longer matches the current location or time."
                                .into(),
                        );
                        self.status = "Weather response rejected - retry pending; only an existing valid profile may remain active.".into();
                    }
                    Err(error) => {
                        self.last_error = Some(error);
                        self.status = if self.profile.is_some() {
                            "Refresh failed - previous valid snapshot remains active; see its timestamp.".into()
                        } else {
                            "Weather unavailable - geometric view; automatic retry pending.".into()
                        };
                    }
                }
            }
        }
        if self.pending.is_some()
            || (self.profile.is_some() && self.completed_key.as_ref() == Some(key))
            || self.location_changed.elapsed() < Duration::from_millis(500)
            || self.next_attempt.is_some_and(|next| Instant::now() < next)
        {
            return;
        }
        self.next_attempt = Some(Instant::now() + Duration::from_secs(60));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, result) = mpsc::channel();
        self.pending = Some(Pending {
            key: key.clone(),
            cancelled,
            result: Mutex::new(result),
        });
        self.status = if self.profile.is_some() {
            "Refreshing NOAA weather in the background; previous valid snapshot active.".into()
        } else {
            "Downloading NOAA profile in the background - geometric view until ready.".into()
        };
        let spawn = std::thread::Builder::new()
            .name("noaa-weather".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    download_candidates(&candidates, &worker_cancelled)
                })
                .unwrap_or_else(|_| {
                    Err("Weather decoder rejected an unexpected provider response.".into())
                });
                let _ = sender.send(result);
            });
        if let Err(error) = spawn {
            self.cancel_pending();
            self.last_error = Some(format!("Could not start weather worker: {error}"));
            self.status = "Weather worker unavailable - automatic retry pending.".into();
        }
    }
}

fn download_candidates(
    candidates: &[WeatherRequest],
    cancelled: &AtomicBool,
) -> Result<Download, String> {
    let directory = transport::cache_directory();
    let global_directory = directory.as_ref().map(|path| path.join("gfs"));
    let regional_directory = directory.as_ref().map(|path| path.join("hrrr"));
    let mut failure = "No NOAA forecast was available.".to_owned();
    for (index, request) in candidates.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Weather request cancelled".into());
        }
        let result = (|| {
            let mut payload =
                transport::load_or_fetch(&request.url, global_directory.as_deref(), cancelled)?;
            let mut decoded = decode_gfs(&payload.bytes, &request.gfs);
            if decoded.is_err() && payload.cached {
                payload = transport::load_or_fetch(&request.url, None, cancelled)?;
                decoded = decode_gfs(&payload.bytes, &request.gfs);
            }
            decoded.map_err(|error| error.to_string())?;
            if cancelled.load(Ordering::Relaxed) {
                return Err("Weather request cancelled".into());
            }
            if !payload.cached
                && let Some(directory) = global_directory.as_deref()
            {
                // Cache failures must not prevent valid live weather. Integrity
                // and scientific checks are repeated before any later reuse.
                let _ = transport::save_cache(directory, &request.url, &payload);
            }
            let mut regional = request.hrrr.as_ref().map(|request| {
                transport::load_or_fetch(&request.url, regional_directory.as_deref(), cancelled)
            });
            let mut profile = decode_weather(
                &payload.bytes,
                regional
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map(|p| p.bytes.as_slice()),
                request,
            )
            .map_err(|error| error.to_string())?;
            if profile.regional_fallback.is_some()
                && regional
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .is_some_and(|p| p.cached)
                && let Some(regional_request) = request.hrrr.as_ref()
            {
                regional = Some(transport::load_or_fetch(
                    &regional_request.url,
                    None,
                    cancelled,
                ));
                profile = decode_weather(
                    &payload.bytes,
                    regional
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .map(|p| p.bytes.as_slice()),
                    request,
                )
                .map_err(|error| error.to_string())?;
            }
            let regional_payload = regional.as_ref().and_then(|result| result.as_ref().ok());
            if let Some(Err(error)) = &regional {
                profile.regional_fallback = Some(format!(
                    "Regional weather unavailable: {error}. Validated GFS column is active."
                ));
            }
            let regional_used = matches!(
                profile.metadata.source_grid,
                earth_math::atmosphere::weather::WeatherGrid::Hrrr { .. }
            );
            if cancelled.load(Ordering::Relaxed) {
                return Err("Weather request cancelled".into());
            }
            if regional_used
                && let Some(regional_payload) = regional_payload
                && !regional_payload.cached
                && let Some(directory) = regional_directory.as_deref()
                && let Some(regional_request) = request.hrrr.as_ref()
            {
                let _ = transport::save_cache(directory, &regional_request.url, regional_payload);
            }
            Ok(Download {
                profile,
                cached: payload.cached
                    && (!regional_used || regional_payload.is_some_and(|p| p.cached)),
                fetched_unix: payload.fetched_unix,
            })
        })();
        match result {
            Ok(download) => return Ok(download),
            Err(error) => {
                failure = error;
                // NOAA's GRIB-filter scripting guidance requests a pause after
                // failures so missing files cannot trigger a tight request loop.
                if index + 1 < candidates.len() && !wait_after_provider_failure(cancelled) {
                    return Err("Weather request cancelled".into());
                }
            }
        }
    }
    Err(failure)
}

fn wait_after_provider_failure(cancelled: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !cancelled.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::atmosphere::weather::GfsProfile;

    #[test]
    fn disabled_weather_never_polls_or_launches_requests() {
        let mut weather = WeatherState::default();
        weather.tick(Geodetic::new(0.0, 0.0, 0.0).unwrap());
        assert!(!weather.loading());
        assert!(weather.active_profile().is_none());
        assert!(weather.wanted_key.is_none());
    }

    #[test]
    fn disabling_cancels_pending_work_and_ignores_its_late_result() {
        let mut weather = WeatherState::default();
        weather.set_enabled(true);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        weather.pending = Some(Pending {
            key: "old-request".into(),
            cancelled: cancelled.clone(),
            result: Mutex::new(receiver),
        });
        weather.set_enabled(false);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(sender.send(Err("late result".into())).is_err());
        assert!(!weather.loading());
        assert!(weather.active_profile().is_none());
        weather.set_enabled(true);
        assert!(weather.last_error.is_none());
    }

    fn fixed_profile() -> GfsProfile {
        decode_gfs(
            include_bytes!("../assets/weather/gfs-reference.grib2"),
            &earth_math::atmosphere::weather::reference_request(),
        )
        .unwrap()
    }

    #[test]
    fn scene_registration_is_cached_and_never_reused_for_another_scene() {
        let mut weather = WeatherState::default();
        weather.load_reference().unwrap();
        let observer = earth_math::boundary::default_observer();
        let scene = Arc::new(earth_math::TerrainScene::boundary_only(Vec::new()).unwrap());
        weather.prepare_scene(scene.clone(), observer.feet);
        assert!(weather.view_profile_for_scene(observer, &scene).is_some());
        let original = Arc::clone(weather.scene_profile.as_ref().unwrap());
        let generation = weather.generation;
        weather.prepare_scene(scene.clone(), observer.feet);
        assert_eq!(weather.generation, generation);
        assert!(Arc::ptr_eq(
            weather.scene_profile.as_ref().unwrap(),
            &original
        ));
        let replacement = Arc::new(earth_math::TerrainScene::boundary_only(Vec::new()).unwrap());
        assert!(
            weather
                .view_profile_for_scene(observer, &replacement)
                .is_none()
        );
        weather.prepare_scene(replacement.clone(), observer.feet);
        assert_ne!(weather.generation, generation);
        assert!(weather.view_profile_for_scene(observer, &scene).is_none());
        assert!(
            weather
                .view_profile_for_scene(observer, &replacement)
                .is_some()
        );
        assert!(weather.enabled && weather.reference_snapshot);
        weather.load_reference().unwrap();
        assert!(
            weather
                .view_profile_for_scene(observer, &replacement)
                .is_none()
        );
        weather.prepare_scene(replacement.clone(), observer.feet);
        assert!(
            weather
                .view_profile_for_scene(observer, &replacement)
                .is_some()
        );
        weather.set_enabled(false);
        assert!(
            weather
                .view_profile_for_scene(observer, &replacement)
                .is_none()
        );
    }

    #[test]
    fn provider_retry_wait_can_be_cancelled_immediately() {
        assert!(!wait_after_provider_failure(&AtomicBool::new(true)));
    }

    #[test]
    fn blocked_display_keeps_weather_but_invalidates_all_scene_optics() {
        let mut weather = WeatherState::default();
        weather.load_reference().unwrap();
        let observer = earth_math::boundary::default_observer();
        let scene = Arc::new(earth_math::TerrainScene::boundary_only(Vec::new()).unwrap());
        weather.prepare_scene(scene.clone(), observer.feet);
        assert!(weather.view_profile_for_scene(observer, &scene).is_some());
        let generation = weather.generation;
        weather.set_display_block_reason(Some("Unsupported graphics backend"));
        assert_ne!(weather.generation, generation);
        assert!(weather.active_profile().is_some());
        assert!(weather.registered_profile().is_some());
        assert!(weather.view_profile(observer).is_none());
        assert!(weather.view_profile_for_scene(observer, &scene).is_none());
        let blocked_generation = weather.generation;
        weather.set_display_block_reason(Some("Unsupported graphics backend"));
        assert_eq!(weather.generation, blocked_generation);
        weather.set_display_block_reason(None);
        assert_ne!(weather.generation, blocked_generation);
        assert!(weather.view_profile_for_scene(observer, &scene).is_some());
    }

    #[test]
    fn regional_reference_requires_explicit_selection_and_cannot_silently_fallback() {
        let mut weather = WeatherState::default();
        weather.load_regional_reference().unwrap();
        assert!(weather.reference_snapshot);
        assert!(matches!(
            weather
                .active_profile()
                .unwrap()
                .metadata
                .as_ref()
                .unwrap()
                .source_grid,
            earth_math::atmosphere::weather::WeatherGrid::Hrrr { .. }
        ));
        weather.refresh();
        assert!(!weather.reference_snapshot);
        assert!(weather.active_profile().is_none());
    }

    #[test]
    fn moving_invalidates_profile_before_render_and_return_can_request_again() {
        let fixed = fixed_profile();
        let original = Geodetic::new(
            fixed.metadata.latitude_deg,
            fixed.metadata.longitude_deg,
            0.0,
        )
        .unwrap();
        let now = fixed.metadata.valid_unix_seconds as u64;
        let key = weather_requests(original.lat_deg, original.lon_deg, now as f64).unwrap()[0]
            .key
            .clone();
        let mut weather = WeatherState::default();
        weather.set_enabled(true);
        weather.profile = Some(Arc::new(fixed.profile));
        weather.completed_key = Some(key.clone());
        weather.wanted_key = Some(key.clone());
        weather.tick_at(Geodetic::new(0.0, 0.0, 0.0).unwrap(), now);
        assert!(weather.active_profile().is_none());
        assert!(!weather.loading());
        weather.tick_at(original, now);
        assert_eq!(weather.wanted_key.as_ref(), Some(&key));
        assert!(weather.active_profile().is_none());
        // The debounce prevents a request in this test. A previous completed
        // key cannot by itself suppress fetching when the profile is absent.
        assert!(!weather.loading());
    }

    #[test]
    fn expired_profile_is_removed_and_reference_mode_requires_explicit_selection() {
        let mut fixed = fixed_profile();
        let location = Geodetic::new(
            fixed.metadata.latitude_deg,
            fixed.metadata.longitude_deg,
            0.0,
        )
        .unwrap();
        let now = fixed.metadata.valid_unix_seconds as u64;
        fixed.profile.metadata.as_mut().unwrap().valid_unix_seconds = 0;
        let mut weather = WeatherState::default();
        weather.set_enabled(true);
        weather.profile = Some(Arc::new(fixed.profile));
        weather.tick_at(location, now);
        assert!(weather.active_profile().is_none());
        assert!(!weather.reference_snapshot);
        weather.load_reference().unwrap();
        assert!(weather.reference_snapshot);
        assert!(weather.active_profile().is_some());
        assert!(
            weather
                .view_profile(earth_math::atmosphere::weather::reference_observer())
                .is_some()
        );
        assert!(
            weather
                .view_profile(earth_math::guides::demo_observer(
                    earth_math::guides::GuideDemo::HighAltitude
                ))
                .is_none()
        );
        weather.set_enabled(false);
        assert!(weather.active_profile().is_none());
        weather.set_enabled(true);
        assert!(weather.active_profile().is_some());
        weather.refresh();
        assert!(!weather.reference_snapshot);
        assert!(weather.active_profile().is_none());
    }
}
