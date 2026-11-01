//! Manual source download/cache lifecycle. Scientific work lives in earth-math.
mod transport;

use crate::{datum::SeaLevel, navigation::Explorer, surface::WorldSurface};
use earth_math::{
    Geodetic, TerrainScene,
    topography::{
        DEFAULT_RADIUS_M, DownloadRegion, ImportSummary, copernicus_tiles, format_download_size,
        import_sources,
    },
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, Default)]
pub struct DownloadProgress {
    pub status: String,
    pub total_bytes: Option<u64>,
    pub download_bytes: Option<u64>,
    pub received_bytes: u64,
    pub missing_tiles: usize,
    pub water_tiles: usize,
    pub missing_water_tiles: usize,
}

enum WorkResult {
    Estimate,
    Imported {
        scene: Box<TerrainScene>,
        summary: ImportSummary,
        region: DownloadRegion,
    },
}

struct Pending {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<DownloadProgress>>,
    result: Mutex<mpsc::Receiver<Result<WorkResult, String>>>,
}

#[cfg_attr(feature = "desktop", derive(bevy::prelude::Resource))]
pub struct TopographyState {
    pub radius_m: f64,
    pub progress: DownloadProgress,
    pub last_error: Option<String>,
    pub imported_description: Option<String>,
    pub active_region: Option<DownloadRegion>,
    pub request_region: Option<DownloadRegion>,
    pub active_source_bytes: Option<u64>,
    pub cache: Option<PathBuf>,
    pending: Option<Pending>,
}

impl Default for TopographyState {
    fn default() -> Self {
        Self {
            radius_m: DEFAULT_RADIUS_M,
            progress: DownloadProgress {
                status: "No elevation download requested. Smooth WGS84 surface.".into(),
                ..Default::default()
            },
            last_error: None,
            imported_description: None,
            active_region: None,
            request_region: None,
            active_source_bytes: None,
            cache: transport::cache_directory(),
            pending: None,
        }
    }
}

impl Drop for TopographyState {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl TopographyState {
    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn cancel(&mut self) {
        if let Some(pending) = &self.pending {
            pending.cancel.store(true, Ordering::Relaxed);
            self.progress.status = "Cancelling... completed files remain cached.".into();
        }
    }

    pub fn request(&mut self, center: Geodetic, surface: &WorldSurface, download: bool) {
        match DownloadRegion::new(center, self.radius_m) {
            Ok(region) => self.start(region, surface, download, false),
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub fn load_saved(&mut self, surface: &WorldSurface) {
        let saved = self.cache.as_deref().and_then(transport::saved_selection);
        if let Some(region) = saved {
            self.radius_m = region.radius_m;
            self.start(region, surface, false, true);
        } else {
            self.last_error =
                Some("No previous completed download is recorded in this cache.".into());
        }
    }

    fn start(
        &mut self,
        region: DownloadRegion,
        surface: &WorldSurface,
        download: bool,
        offline: bool,
    ) {
        if self.loading() {
            return;
        }
        let Some(root) = self.cache.clone() else {
            self.last_error =
                Some("No cache directory. Set ANCIENT_EARTH_TOPOGRAPHY_CACHE.".into());
            return;
        };
        let tiles = match copernicus_tiles(&region) {
            Ok(tiles) => tiles,
            Err(error) => {
                self.last_error = Some(error.to_string());
                return;
            }
        };
        self.last_error = None;
        self.request_region = Some(region.clone());
        self.progress = DownloadProgress {
            status: if offline {
                "Verifying saved source files..."
            } else {
                "Checking source sizes..."
            }
            .into(),
            ..Default::default()
        };
        let progress = Arc::new(Mutex::new(self.progress.clone()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let worker_progress = Arc::clone(&progress);
        let worker_cancel = Arc::clone(&cancel);
        let boundaries = surface.boundaries();
        let spawn = std::thread::Builder::new()
            .name("topography-download".into())
            .spawn(move || {
                let result = (|| {
                    let plan = transport::plan(&tiles, &root, &worker_cancel, offline, |status| {
                        if let Ok(mut p) = worker_progress.lock() {
                            p.status = status;
                        }
                    })?;
                    if let Ok(mut p) = worker_progress.lock() {
                        p.total_bytes = Some(plan.total_bytes);
                        p.download_bytes = Some(plan.download_bytes);
                        p.missing_tiles = plan.missing;
                        p.water_tiles = plan.water_tiles;
                        p.missing_water_tiles = plan.missing_water;
                        p.status = format!(
                            "Estimated source data: {}; new download: {}.",
                            format_download_size(plan.total_bytes),
                            format_download_size(plan.download_bytes)
                        );
                    }
                    if !download && !offline {
                        return Ok(WorkResult::Estimate);
                    }
                    let sources = if offline {
                        transport::cached_sources(&plan, &root)
                    } else {
                        transport::fetch(&plan, &root, &worker_cancel, |status, received| {
                            if let Ok(mut p) = worker_progress.lock() {
                                p.status = status;
                                p.received_bytes = received;
                            }
                        })?
                    };
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Err("Cancelled; completed source files remain cached.".into());
                    }
                    if let Ok(mut p) = worker_progress.lock() {
                        p.status =
                            "Source files saved. Preparing elevations and water boundaries, please wait...".into();
                    }
                    let geoid = SeaLevel::bundled().map_err(|e| e.to_string())?;
                    let (scene, summary) =
                        import_sources(&region, &sources.elevations, &geoid.0, boundaries, &worker_cancel)
                            .map_err(|e| e.to_string())?;
                    let masks = earth_math::water_mask::WaterMask::from_sources(&sources.water, &worker_cancel)
                        .map_err(|e| e.to_string())?;
                    let scene = scene.with_water_masks(Arc::new(masks));
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Err("Cancelled; completed source files remain cached.".into());
                    }
                    if !offline {
                        transport::save_selection(&root, &region)?;
                    }
                    Ok(WorkResult::Imported {
                        scene: Box::new(scene),
                        summary,
                        region,
                    })
                })();
                let _ = sender.send(result);
            });
        match spawn {
            Ok(_) => {
                self.pending = Some(Pending {
                    cancel,
                    progress,
                    result: Mutex::new(receiver),
                })
            }
            Err(error) => {
                self.last_error = Some(format!("Could not start download worker: {error}"))
            }
        }
    }

    pub fn tick(&mut self, surface: &mut WorldSurface, explorer: &mut Explorer) {
        let Some(pending) = &self.pending else {
            return;
        };
        let cancelled = pending.cancel.load(Ordering::Relaxed);
        if !cancelled && let Ok(progress) = pending.progress.lock() {
            self.progress = progress.clone();
        }
        let result = match pending
            .result
            .lock()
            .expect("download result mutex")
            .try_recv()
        {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Elevation worker stopped before completing.".into())
            }
        };
        self.pending = None;
        if cancelled {
            self.progress.status =
                "Cancelled. Completed source files remain cached; active terrain unchanged.".into();
            return;
        }
        match result {
            Ok(WorkResult::Estimate) => {}
            Ok(WorkResult::Imported {
                scene,
                summary,
                region,
            }) => {
                // Verify observer placement before committing a new scene.
                let mut observer = explorer.observer;
                match scene.ensure_observer_above_surface(&mut observer) {
                    Ok(lift) => {
                        surface.install_terrain(*scene);
                        explorer.observer = observer;
                        explorer.altitude_m = observer.feet.height_m;
                        self.imported_description = Some(summary.description());
                        self.active_region = Some(region);
                        self.active_source_bytes = self.progress.total_bytes;
                        self.progress.status = "Source files loaded. Elevations and water boundaries are ready; downloaded files remain saved.".into();
                        explorer.status = if lift > 0.0 {
                            format!(
                                "Raised observer {} to stay above the new surface.",
                                explorer.units.format_height(lift)
                            )
                        } else {
                            "Downloaded elevations enabled; observer position preserved.".into()
                        };
                    }
                    Err(error) => {
                        self.last_error =
                            Some(format!("Source saved; could not activate terrain: {error}"))
                    }
                }
            }
            Err(error) => {
                self.last_error = Some(error);
                self.progress.status = "Request stopped. Previous active surface and completed cache files are preserved.".into();
            }
        }
    }

    pub fn set_elevations(
        &mut self,
        enabled: bool,
        surface: &mut WorldSurface,
        explorer: &mut Explorer,
    ) {
        let previous = surface.elevations_enabled;
        surface.set_elevations(enabled);
        let mut observer = explorer.observer;
        match surface
            .active(explorer.boundaries_enabled)
            .ensure_observer_above_surface(&mut observer)
        {
            Ok(lift) => {
                explorer.observer = observer;
                explorer.altitude_m = observer.feet.height_m;
                explorer.status = if lift > 0.0 {
                    format!(
                        "Raised observer {} to remain above the selected surface.",
                        explorer.units.format_height(lift)
                    )
                } else if enabled {
                    "Cached elevations enabled.".into()
                } else {
                    "Smooth WGS84 surface enabled. Downloaded files remain cached.".into()
                };
            }
            Err(error) => {
                surface.set_elevations(previous);
                self.last_error = Some(error.to_string());
            }
        }
    }
}
