//! Canonical DSM or stitched smooth Earth with independent fragment categories.
#[cfg(test)]
mod review;
use super::{
    FishQuad, SceneObject, classification,
    materials::WaterSettings,
    mesh_from,
    refraction::{RefractionQuad, Region, boundary_data},
};
use bevy::{
    asset::uuid_handle,
    ecs::system::SystemParam,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
};
use earth_math::{
    Observer, TerrainScene,
    atmosphere::gpu::GpuEllipsoid,
    rendering::{
        LocalFrame, display_bounds, imported_scene_meshes_with_detail, spatial_draw_chunks,
    },
};
use std::sync::Arc;
const SHADER: Handle<Shader> = uuid_handle!("8336c676-0647-429b-8c56-c2b156859bd4");
#[derive(Clone, Copy, Debug, ShaderType)]
struct Settings {
    water: WaterSettings,
    q0: Vec4,
    q1: Vec4,
    q2: Vec4,
    geometry_origin: Vec4,
    latitude: Vec4,
    region0: [Vec4; 4],
    region1: [Vec4; 4],
    classification: classification::Settings,
}
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct Extension {
    #[uniform(100)]
    settings: Settings,
    #[storage(101, read_only)]
    boundaries: Handle<ShaderBuffer>,
    #[storage(102, read_only)]
    native_water: Handle<ShaderBuffer>,
    #[storage(103, read_only)]
    global_land: Handle<ShaderBuffer>,
}
impl MaterialExtension for Extension {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
type Material = ExtendedMaterial<StandardMaterial, Extension>;
#[derive(Component)]
struct ImportedPart {
    triangles: usize,
}
#[derive(Resource)]
pub struct Display {
    material: Handle<Material>,
    meshes: Vec<(Entity, Handle<Mesh>)>,
    buffer: Handle<ShaderBuffer>,
    native_water: Handle<ShaderBuffer>,
    global_land: Handle<ShaderBuffer>,
    regions: Vec<Region>,
    scene: Option<Arc<TerrainScene>>,
    pub triangles: usize,
}
pub fn install(app: &mut App) {
    app.add_plugins(MaterialPlugin::<Material>::default())
        .add_systems(Startup, setup);
    let ground = include_str!("../shaders/ground.wesl");
    let body = &ground[ground.find("fn hash_cell").unwrap()..];
    let body = body
        .replace(
            "@fragment\nfn fragment(in: VertexOutput, @builtin(front_facing) front: bool)",
            "fn shade_ground(in: VertexOutput, front: bool, footprint:f32)",
        )
        .replace("ground.", "terrain.water.coordinates.")
        .replace(
            "    let footprint = max(length(dpdx(ecef)), length(dpdy(ecef)));\n",
            "",
        );
    let water = include_str!("../shaders/water.wesl");
    let water = &water[water.find("fn ripple").unwrap()..];
    let water=water.replace("@fragment\nfn fragment(in: VertexOutput, @builtin(front_facing) front: bool)","fn shade_water(in: VertexOutput, front: bool, footprint:f32)")
 .replace("water.","terrain.water.")
 .replace("    let footprint = max(length(dpdx(ecef)),length(dpdy(ecef)));\n","")
 .replace("    var input = pbr_input_from_standard_material(in,front);","    var input = pbr_input_from_standard_material(in,front);\n    input.material.base_color=vec4(0.012,0.055,0.070,1.0);\n    input.material.reflectance=vec3(0.35355339059);");
    let shader = format!(
        "{}\n{}\n{}\n{}\n{}",
        include_str!("../shaders/terrain.wesl"),
        body,
        water,
        include_str!("../shaders/eclipse.wesl"),
        include_str!("../shaders/water_mask.wesl")
    );
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(shader, "ancient_earth/terrain.wesl"),
        )
        .expect("original imported surface shader");
}
fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<Material>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    state: Res<super::Explorer>,
) {
    let buffer = buffers.add(ShaderBuffer::from(vec![[0.0_f32; 4]]));
    let empty = classification::Packed::empty();
    let native_water = buffers.add(ShaderBuffer::from(empty.native));
    let global_land = buffers.add(ShaderBuffer::from(empty.global));
    let local = LocalFrame::at(state.observer);
    let settings = Settings {
        water: WaterSettings::at(local),
        q0: Vec4::ZERO,
        q1: Vec4::ZERO,
        q2: Vec4::ZERO,
        geometry_origin: Vec4::ZERO,
        latitude: Vec4::ZERO,
        region0: [Vec4::ZERO; 4],
        region1: [Vec4::ZERO; 4],
        classification: classification::Settings::default(),
    };
    let material = materials.add(Material {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.96,
            reflectance: 0.12,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: Extension {
            settings,
            boundaries: buffer.clone(),
            native_water: native_water.clone(),
            global_land: global_land.clone(),
        },
    });
    commands.insert_resource(Display {
        material,
        meshes: Vec::new(),
        buffer,
        native_water,
        global_land,
        regions: Vec::new(),
        scene: None,
        triangles: 0,
    });
}
#[derive(SystemParam)]
pub struct Parameters<'w, 's> {
    display: ResMut<'w, Display>,
    materials: ResMut<'w, Assets<Material>>,
    #[allow(clippy::type_complexity)]
    visible: Query<
        'w,
        's,
        (
            &'static mut Visibility,
            &'static ViewVisibility,
            &'static ImportedPart,
        ),
        (
            With<ImportedPart>,
            Without<SceneObject>,
            Without<FishQuad>,
            Without<RefractionQuad>,
        ),
    >,
}
impl Parameters<'_, '_> {
    pub fn contains_scene(&self, scene: &Arc<TerrainScene>) -> bool {
        self.display
            .scene
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, scene))
    }
    /// All preparation and classification packing occurred on the worker. Only
    /// owned asset installation takes place in the same frame as origin commit.
    pub(super) fn commit(
        &mut self,
        prepared: Prepared,
        scene: Arc<TerrainScene>,
        meshes: &mut Assets<Mesh>,
        buffers: &mut Assets<ShaderBuffer>,
        commands: &mut Commands,
    ) {
        // Keep the reference at index zero, and replace every local chunk in
        // the same origin commit. Deferred commands apply before visibility /
        // render extraction, including removal of obsolete chunk entities.
        while self.display.meshes.len() > prepared.meshes.len() {
            let (entity, handle) = self.display.meshes.pop().unwrap();
            commands.entity(entity).despawn();
            meshes.remove(handle.id());
        }
        for (index, (mesh, bounds)) in prepared.meshes.into_iter().zip(prepared.bounds).enumerate()
        {
            let triangles = mesh.indices().map_or(0, |indices| indices.len() / 3);
            if let Some((entity, handle)) = self.display.meshes.get(index) {
                meshes
                    .insert(handle.id(), mesh)
                    .expect("surface draw chunk retained");
                commands
                    .entity(*entity)
                    .insert((bounds, ImportedPart { triangles }));
            } else {
                let handle = meshes.add(mesh);
                let mut entity = commands.spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(self.display.material.clone()),
                    ImportedPart { triangles },
                    bounds,
                    bevy::camera::visibility::NoAutoAabb,
                    Visibility::Visible,
                ));
                if index == 0 {
                    entity.insert(bevy::light::NotShadowCaster);
                }
                self.display.meshes.push((entity.id(), handle));
            }
        }
        if let Some((data, regions)) = prepared.classification {
            buffers
                .insert(self.display.buffer.id(), ShaderBuffer::from(data))
                .expect("boundary data retained");
            self.display.regions = regions;
        }
        if let Some(masks) = prepared.masks {
            buffers
                .insert(
                    self.display.native_water.id(),
                    ShaderBuffer::from(masks.native),
                )
                .expect("native mask buffer retained");
            buffers
                .insert(
                    self.display.global_land.id(),
                    ShaderBuffer::from(masks.global),
                )
                .expect("global mask buffer retained");
        }
        self.display.scene = Some(scene);
        self.display.triangles = prepared.triangles;
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        scene: &TerrainScene,
        boundaries_enabled: bool,
        origin_observer: Observer,
        coherent: bool,
        water: WaterSettings,
    ) -> usize {
        let active = coherent && uses_classifier(scene);
        for (mut visible, _, _) in &mut self.visible {
            *visible = if active {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if !active {
            return 0;
        }
        let geometry = GpuEllipsoid::at(origin_observer);
        let mut settings = Settings {
            water,
            q0: geometry.rows[0].into(),
            q1: geometry.rows[1].into(),
            q2: geometry.rows[2].into(),
            geometry_origin: geometry.origin.into(),
            latitude: geometry.latitude.into(),
            region0: [Vec4::ZERO; 4],
            region1: [Vec4::ZERO; 4],
            classification: classification::Settings::at(
                scene,
                origin_observer.eye_geodetic(),
                boundaries_enabled,
            ),
        };
        // The immutable classification buffer stays cached while hidden. Zero
        // region descriptors bypass every lookup without changing physical DSM
        // triangles, grass anchors, collisions, or the source classification.
        if boundaries_enabled {
            for (i, region) in self.display.regions.iter().take(4).enumerate() {
                (settings.region0[i], settings.region1[i]) =
                    region.uniforms(origin_observer.eye_geodetic());
            }
        }
        if let Some(mut material) = self.materials.get_mut(&self.display.material) {
            material.extension.settings = settings;
        }
        self.display.triangles
    }

    /// Last computed Bevy per-view visibility, not a hardware draw counter.
    /// New/changed chunks acquire current visibility in PostUpdate afterwards.
    pub fn visibility_statistics(&self) -> (usize, usize, usize) {
        let mut count = 0;
        let mut faces = 0;
        for (visibility, view, part) in &self.visible {
            if *visibility != Visibility::Hidden && view.get() {
                count += 1;
                faces += part.triangles;
            }
        }
        (self.display.meshes.len(), count, faces)
    }
}

pub(super) struct Prepared {
    meshes: Vec<Mesh>,
    bounds: Vec<bevy::camera::primitives::Aabb>,
    classification: Option<(Vec<[f32; 4]>, Vec<Region>)>,
    masks: Option<classification::Packed>,
    pub triangles: usize,
}
impl Prepared {
    pub(super) fn new(
        scene: &TerrainScene,
        local: LocalFrame,
        focal: f64,
        detail: earth_math::rendering::continuous::ReferenceDetail,
        classify: bool,
    ) -> Self {
        let (reference, terrain) = if scene.is_boundary_only() {
            // Preserve the shared reference/region perimeter exactly. Only
            // material assignment moves from separate draw parts to lookup.
            let display = earth_math::rendering::boundary_scene_meshes_with_detail(
                scene, local, focal, detail,
            );
            let land_color = display
                .reference
                .colors
                .first()
                .copied()
                .unwrap_or([0.18, 0.27, 0.11, 1.0]);
            let mut joined = earth_math::rendering::RenderMesh::default();
            for region in display.regions {
                for mut mesh in [region.land, region.water] {
                    // With classification hidden, all h=0 parts retain the same
                    // illustrative land base, including former water-only parts.
                    mesh.colors.fill(land_color);
                    let offset = joined.positions.len() as u32;
                    joined.positions.extend(mesh.positions);
                    joined.normals.extend(mesh.normals);
                    joined.colors.extend(mesh.colors);
                    joined
                        .indices
                        .extend(mesh.indices.into_iter().map(|index| index + offset));
                }
            }
            (display.reference, joined)
        } else {
            imported_scene_meshes_with_detail(scene, local, focal, detail)
        };
        let triangles = (reference.indices.len() + terrain.indices.len()) / 3;
        let chunks = std::iter::once(reference).chain(spatial_draw_chunks(terrain));
        let mut meshes = Vec::new();
        let mut bounds = Vec::new();
        for chunk in chunks {
            let (center, half_extents) = display_bounds(&chunk.positions);
            bounds.push(bevy::camera::primitives::Aabb {
                center: Vec3::from_array(center).into(),
                half_extents: Vec3::from_array(half_extents).into(),
            });
            meshes.push(mesh_from(chunk));
        }
        Self {
            meshes,
            bounds,
            classification: classify.then(|| boundary_data(scene.boundaries())),
            masks: classify.then(|| classification::Packed::from_scene(scene)),
            triangles,
        }
    }
}
pub(super) fn uses_classifier(scene: &TerrainScene) -> bool {
    !scene.is_boundary_only() || scene.water_masks().is_some() || scene.global_land_mask().is_some()
}
