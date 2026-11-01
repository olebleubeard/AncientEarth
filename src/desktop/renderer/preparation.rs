//! Background preparation of a complete, coherently anchored Earth display.
//! A finished same-scene request remains useful after camera movement: commit
//! it, then prepare the newest view rather than starving continuous motion.
use super::{blank_mesh, mesh_from, terrain};
use bevy::prelude::Mesh;
use earth_math::{
    Observer, TerrainScene,
    rendering::{LocalFrame, boundary_scene_meshes_with_detail, continuous::DisplayPlan},
};
use std::{sync::Arc, time::Instant};

pub(super) struct Request {
    pub scene: Arc<TerrainScene>,
    pub observer: Observer,
    pub local: LocalFrame,
    pub plan: DisplayPlan,
    pub focal: f64,
    pub classify_import: bool,
}
pub(super) struct Boundary {
    pub reference: Mesh,
    pub regions: Vec<(Mesh, Mesh, usize)>,
}
pub(super) struct Prepared {
    pub request: Request,
    pub boundary: Boundary,
    pub imported: Option<terrain::Prepared>,
    pub triangles: usize,
    pub preparation_ms: u128,
    pub worker_thread: std::thread::ThreadId,
}
impl Request {
    pub fn prepare(self) -> Prepared {
        debug_assert_eq!(self.observer.eye_position(), self.plan.detail.eye);
        let started = Instant::now();
        let (boundary, imported, triangles) = if !terrain::uses_classifier(&self.scene) {
            let data = boundary_scene_meshes_with_detail(
                &self.scene,
                self.local,
                self.focal,
                self.plan.detail,
            );
            let triangles = (data.reference.indices.len()
                + data
                    .regions
                    .iter()
                    .map(|r| r.land.indices.len() + r.water.indices.len())
                    .sum::<usize>())
                / 3;
            (
                Boundary {
                    reference: mesh_from(data.reference),
                    regions: data
                        .regions
                        .into_iter()
                        .map(|r| (mesh_from(r.land), mesh_from(r.water), r.stride))
                        .collect(),
                },
                None,
                triangles,
            )
        } else {
            let imported = terrain::Prepared::new(
                &self.scene,
                self.local,
                self.focal,
                self.plan.detail,
                self.classify_import,
            );
            let triangles = imported.triangles;
            (
                Boundary {
                    reference: blank_mesh(),
                    regions: Vec::new(),
                },
                Some(imported),
                triangles,
            )
        };
        Prepared {
            request: self,
            boundary,
            imported,
            triangles,
            preparation_ms: started.elapsed().as_millis(),
            worker_thread: std::thread::current().id(),
        }
    }
}
