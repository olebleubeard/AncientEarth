//! Bundled surface data. All geometry and datum calculations live in earth-math.
use earth_math::{SurfaceKind, TerrainError, TerrainPatch, TerrainScene, WaterKind};
use std::sync::Arc;

#[cfg_attr(feature = "desktop", derive(bevy::prelude::Resource))]
pub struct WorldSurface {
    base_boundaries: Arc<TerrainScene>,
    boundaries: Arc<TerrainScene>,
    reference: Arc<TerrainScene>,
    terrain: Option<Arc<TerrainScene>>,
    pub elevations_enabled: bool,
    pub generation: u64,
}

impl WorldSurface {
    pub fn bundled() -> Result<Self, TerrainError> {
        let global = Arc::new(
            earth_math::water_mask::GlobalLandMask::from_bytes(include_bytes!(
                "../assets/boundaries/global-land.aev"
            ))
            .map_err(|_| TerrainError::InvalidFormat)?,
        );
        let boundaries = Arc::new(
            TerrainScene::boundary_only(vec![
                TerrainPatch::from_boundary_bytes(include_bytes!(
                    "../assets/boundaries/region-01.aeb"
                ))?,
                TerrainPatch::from_boundary_bytes(include_bytes!(
                    "../assets/boundaries/region-02.aeb"
                ))?,
                TerrainPatch::from_boundary_bytes(include_bytes!(
                    "../assets/boundaries/region-03.aeb"
                ))?,
                TerrainPatch::from_boundary_bytes(include_bytes!(
                    "../assets/boundaries/region-04.aeb"
                ))?,
            ])?
            .with_global_land_mask(global),
        );
        Ok(Self {
            base_boundaries: Arc::clone(&boundaries),
            boundaries,
            reference: Arc::new(TerrainScene::boundary_only(vec![])?),
            terrain: None,
            elevations_enabled: false,
            generation: 0,
        })
    }

    pub fn active(&self, boundaries_enabled: bool) -> &TerrainScene {
        if self.elevations_enabled
            && let Some(scene) = &self.terrain
        {
            return scene;
        }
        if boundaries_enabled {
            &self.boundaries
        } else {
            &self.reference
        }
    }

    /// Share immutable canonical data with background presentation workers.
    pub fn shared(&self, boundaries_enabled: bool) -> Arc<TerrainScene> {
        if self.elevations_enabled
            && let Some(scene) = &self.terrain
        {
            return Arc::clone(scene);
        }
        Arc::clone(if boundaries_enabled {
            &self.boundaries
        } else {
            &self.reference
        })
    }

    pub fn boundaries(&self) -> Arc<TerrainScene> {
        Arc::clone(&self.base_boundaries)
    }

    pub fn has_terrain(&self) -> bool {
        self.terrain.is_some()
    }

    pub fn install_terrain(&mut self, scene: TerrainScene) {
        if let Some(masks) = scene.shared_water_masks() {
            // Same immutable categories remain available with physical heights
            // disabled. This copies no source raster or native terrain mesh.
            self.boundaries = Arc::new(
                TerrainScene::with_shared_category_masks(Arc::clone(&self.base_boundaries), masks)
                    .expect("bundled category scene remains boundary-only"),
            );
        } else {
            self.boundaries = Arc::clone(&self.base_boundaries);
        }
        self.terrain = Some(Arc::new(scene));
        self.elevations_enabled = true;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn set_elevations(&mut self, enabled: bool) {
        let enabled = enabled && self.has_terrain();
        if self.elevations_enabled != enabled {
            self.elevations_enabled = enabled;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn surface_label(&self, kind: SurfaceKind) -> &'static str {
        if self.elevations_enabled {
            match kind {
                SurfaceKind::Terrain(_) => return "Copernicus DSM / retained source elevations",
                SurfaceKind::Water { .. } => {
                    return "Mapped water / source DSM + EGM2008 datum conversion";
                }
                SurfaceKind::ReferenceEllipsoid => return "WGS84 h=0 / outside imported terrain",
            }
        }
        Self::label(kind)
    }

    pub fn label(kind: SurfaceKind) -> &'static str {
        match kind {
            SurfaceKind::Terrain(_) => "Mapped land / WGS84 h=0",
            SurfaceKind::Water {
                kind: WaterKind::Ocean,
                ..
            } => "Mapped ocean / WGS84 h=0",
            SurfaceKind::Water {
                kind: WaterKind::Lake,
                ..
            } => "Mapped lake / WGS84 h=0",
            SurfaceKind::Water {
                kind: WaterKind::River,
                ..
            } => "Mapped river / WGS84 h=0",
            SurfaceKind::ReferenceEllipsoid => "WGS84 h=0 / category not mapped",
        }
    }
}
