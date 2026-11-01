//! Native GPU presentation. Measurements use the canonical f64 surface.
mod classification;
mod lunar;
mod materials;
#[cfg(test)]
mod orbit;
pub use classification::inspection_observer as classification_inspection_observer;
pub use classification::rockies_cost_observer;
mod preparation;
mod refraction;
pub use refraction::DisplayStatus as OpticalDisplayStatus;
pub(crate) use refraction::OpticalReadback;
mod survey;
mod terrain;
#[cfg(test)]
mod visibility_review;
use super::Explorer;
use ancient_earth::{simulation::AstronomicalScene, surface::WorldSurface};
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::{
    asset::{RenderAssetUsages, uuid_handle},
    camera::{Exposure, RenderTarget},
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    ecs::system::SystemParam,
    light::{
        CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster, NotShadowReceiver,
    },
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::{AsBindGroup, ShaderType, TextureFormat},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{Material2d, Material2dPlugin},
    window::PrimaryWindow,
};
use dear_imgui_bevy::ImguiFrame;
use earth_math::{
    Observer, Projection as LensProjection, Vec3d,
    rendering::{
        GrassBlade, GrassPlacementCache, GrassRayBounds, LocalFrame, RenderMesh,
        continuous::{DisplayPlan, ReferenceDetail},
        focal_pixels, grass_intersection_cached, grass_ray_bounds, grass_render_mesh, grass_wind,
    },
};
use materials::{
    EclipseSettings, GrassExtension, GrassMaterial, GrassSettings, GroundExtension, GroundMaterial,
    GroundSettings, MoonExtension, MoonMaterial, SkyExtension, SkyMaterial, StarExtension,
    StarMaterial, StarSettings, WaterExtension, WaterMaterial, WaterSettings,
};
use std::{sync::Arc, time::Instant};

const FISHEYE_SHADER: Handle<Shader> = uuid_handle!("7340542d-ff49-4a7b-902c-c934ec50de87");

#[derive(Clone, Copy, Default, Debug, ShaderType)]
pub struct FisheyeStars {
    phase_sin: Vec4,
    phase_cos: Vec4,
    vertical: Vec4,
    right: Vec4,
    up: Vec4,
    forward: Vec4,
    visibility: Vec4,
}
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct FisheyeMaterial {
    #[uniform(0)]
    parameters: Vec4,
    #[texture(1)]
    #[sampler(2)]
    front: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    right: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    back: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    left: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    up: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    down: Handle<Image>,
    #[uniform(13)]
    stars: FisheyeStars,
    #[storage(14, read_only)]
    catalogue: Handle<ShaderBuffer>,
    #[uniform(15)]
    sampling: Vec4,
}
impl Material2d for FisheyeMaterial {
    fn fragment_shader() -> ShaderRef {
        FISHEYE_SHADER.into()
    }
}
pub fn install(app: &mut App) {
    lunar::install(app);
    materials::install(app);
    refraction::install(app);
    terrain::install(app);
    survey::install(app);
    app.add_plugins(Material2dPlugin::<FisheyeMaterial>::default());
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            FISHEYE_SHADER.id(),
            Shader::from_wesl(
                format!(
                    "{}\n{}",
                    include_str!("shaders/fisheye.wesl"),
                    include_str!("shaders/stars_common.wesl")
                ),
                "ancient_earth/fisheye.wesl",
            ),
        )
        .expect("insert embedded original shader");
}

#[derive(Resource)]
pub struct RenderedEarth {
    pub dimensions: (u32, u32),
    /// Raster target size is retained independently while the optical pass is active.
    raster_dimensions: (u32, u32),
    /// CPU presentation preparation, not GPU frame time.
    pub render_ms: u128,
    cpu_timings: RenderCpuTimings,
    pub triangles: usize,
    /// Prepared draw chunks and previous Bevy visibility results, not GPU counters.
    pub draw_chunks: usize,
    pub view_visible_chunks: usize,
    pub view_visible_triangles: usize,
    pub lod_levels: [usize; 4],
    pub mode: &'static str,
    pub grass_distance_m: Option<f64>,
    /// Requested decorative field has finished preparation; GPU warmup remains separate.
    pub presentation_ready: bool,
    pub optical_status: OpticalDisplayStatus,
    /// Pose and lens submitted with the current optical material update.
    /// This identifies the main-world frame, not asynchronous GPU completion.
    pub optical_view: Option<(Observer, earth_math::Lens)>,
    /// Exact stellar input submitted for this optical frame, when ready.
    pub optical_stellar_input: Option<f32>,
    pub optical_submission_path: Option<&'static str>,
    /// Current requested packet identity; completion is separately audited.
    pub optical_submission_control: Option<[u32; 4]>,
    pub optical_stellar_control: Option<[u32; 4]>,
    mesh_counts: [u64; 5],
    mesh_job: Option<Task<preparation::Prepared>>,
    active_scene: Option<Arc<earth_math::TerrainScene>>,
    /// Worker duration is separate from frame-thread preparation timing.
    pub mesh_preparation_ms: u128,
    origin: LocalFrame,
    display_plan: DisplayPlan,
    last_detail_focal: f64,
    last_quality: usize,
    reference: Handle<Mesh>,
    patches: Vec<Handle<Mesh>>,
    waters: Vec<Handle<Mesh>>,
    water_material: Handle<WaterMaterial>,
    grass: Handle<Mesh>,
    grass_entity: Entity,
    grass_material: Handle<GrassMaterial>,
    last_grass_enabled: bool,
    blades: Vec<GrassBlade>,
    grass_placement: GrassPlacementCache,
    grass_bounds: Vec<GrassRayBounds>,
    grass_job: Option<Task<PreparedGrass>>,
    grass_generation: u64,
    scene_identity: usize,
    grass_observer: Option<Observer>,
    fish_targets: Vec<Handle<Image>>,
    fish_material: Handle<FisheyeMaterial>,
    moon_material: Handle<MoonMaterial>,
    stars_material: Handle<StarMaterial>,
    sky_mesh: Handle<Mesh>,
    sky_material: Handle<SkyMaterial>,
    last_sky: Option<(f32, f32)>,
    ground_material: Handle<GroundMaterial>,
}
/// Main-world timings only. GPU work and driver/pipeline time outside this
/// system are deliberately not included. Populated for native QA correlation.
#[derive(Clone, Copy, Default)]
pub struct RenderCpuTimings {
    started_unix_us: u128,
    total_us: u128,
    mesh_poll_us: u128,
    mesh_commit_us: u128,
    mesh_request_us: u128,
    completed_worker_ms: Option<u128>,
    worker_on_render_thread: Option<bool>,
}
impl RenderCpuTimings {
    pub fn json(self) -> serde_json::Value {
        serde_json::json!({
            "started_unix_us":self.started_unix_us,"total_us":self.total_us,
            "mesh_poll_us":self.mesh_poll_us,"mesh_commit_us":self.mesh_commit_us,
            "mesh_request_us":self.mesh_request_us,"completed_worker_ms":self.completed_worker_ms,
            "worker_on_render_thread":self.worker_on_render_thread,
        })
    }
}
pub struct ContinuousSnapshot {
    pub space: bool,
    pub origin: Vec3d,
    pub reconstructed_eye: Vec3d,
    pub mesh_counts: [u64; 5],
    pub cpu: RenderCpuTimings,
}
impl RenderedEarth {
    /// CPU-side coordinate contract for the opt-in continuous traversal probe.
    pub fn continuous_qa_snapshot(&self, observer: Observer) -> ContinuousSnapshot {
        let position = self.origin.position(observer.eye_position());
        let reconstructed = self.origin.origin
            + self.origin.basis.east * f64::from(position[0])
            + self.origin.basis.up * f64::from(position[1])
            - self.origin.basis.north * f64::from(position[2]);
        ContinuousSnapshot {
            space: self.display_plan.space,
            origin: self.origin.origin,
            reconstructed_eye: reconstructed,
            mesh_counts: self.mesh_counts,
            cpu: self.cpu_timings,
        }
    }
    /// Requested/completed/committed/discarded/in-flight presentation jobs.
    pub fn mesh_qa_snapshot(&self) -> [u64; 5] {
        self.mesh_counts
    }
    /// Read-only state for the opt-in native movement/culling coherence fixture.
    pub fn grass_qa_snapshot(&self) -> (Vec3d, u64, usize, usize) {
        (
            self.origin.origin,
            self.grass_generation,
            self.blades.len(),
            self.grass_bounds.len(),
        )
    }
}

struct PreparedGrass {
    generation: u64,
    scene: Arc<earth_math::TerrainScene>,
    observer: Observer,
    placement: GrassPlacementCache,
    blades: Vec<GrassBlade>,
    bounds: Vec<GrassRayBounds>,
    mesh: Mesh,
    aabb: bevy::camera::primitives::Aabb,
}
#[derive(Component, Clone, Copy)]
pub enum CameraRole {
    Main,
    Face(usize),
    Composite,
}
#[derive(Component, Clone, Copy)]
pub enum SceneObject {
    Reference,
    Patch,
    Water,
    Grass,
    Sun,
    Moon,
    Stars,
    Sky,
}
#[derive(Component)]
pub struct FishQuad;

/// The classifier owns every real surface face when active. The retained
/// legacy handles then contain only a degenerate placeholder at local origin.
/// Submitting those placeholders can compile otherwise unused material
/// pipelines when camera motion brings that origin into the view frustum.
fn surface_visibility(
    object: &SceneObject,
    coherent: bool,
    classifier: bool,
    grass_enabled: bool,
    grass_present: bool,
) -> Option<Visibility> {
    let drawn = match object {
        SceneObject::Reference | SceneObject::Patch | SceneObject::Water => coherent && !classifier,
        SceneObject::Grass => coherent && grass_enabled && grass_present,
        _ => return None,
    };
    Some(if drawn {
        Visibility::Visible
    } else {
        Visibility::Hidden
    })
}

fn blank_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; 3])
    .with_inserted_indices(Indices::U32(vec![0, 1, 2]))
}
fn mesh_from(data: RenderMesh) -> Mesh {
    if data.indices.is_empty() {
        return blank_mesh();
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colors)
    .with_inserted_indices(Indices::U32(data.indices))
}
#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ground_materials: ResMut<Assets<GroundMaterial>>,
    mut moon_materials: ResMut<Assets<MoonMaterial>>,
    mut star_materials: ResMut<Assets<StarMaterial>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut grass_materials: ResMut<Assets<GrassMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut fisheye_materials: ResMut<Assets<FisheyeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    state: Res<Explorer>,
    lunar_maps: Res<lunar::LunarMaps>,
) {
    let origin = LocalFrame::at(state.observer);
    let ground = ground_materials.add(GroundMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.96,
            reflectance: 0.12,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: GroundExtension {
            settings: GroundSettings::at(origin),
        },
    });
    let grass_material = grass_materials.add(GrassMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.84,
            reflectance: 0.22,
            diffuse_transmission: 0.55,
            thickness: 0.001,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: GrassExtension {
            settings: GrassSettings {
                wind: Vec4::ZERO,
                eye: Vec4::ZERO,
                sky_fill: Vec4::ZERO,
                eclipse: EclipseSettings::default(),
            },
        },
    });
    let water_material = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::linear_rgb(0.012, 0.055, 0.070),
            metallic: 0.0,
            perceptual_roughness: 0.22,
            // Chosen dielectric F0=0.02 under Bevy's F0=0.16*r^2 mapping.
            // This is an appearance parameter, not measured local water optics.
            reflectance: (0.02_f32 / 0.16).sqrt(),
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: WaterExtension {
            settings: WaterSettings::at(origin),
        },
    });
    let reference = meshes.add(blank_mesh());
    commands.spawn((
        Mesh3d(reference.clone()),
        MeshMaterial3d(ground.clone()),
        SceneObject::Reference,
        NotShadowCaster,
        Visibility::Hidden,
    ));
    let patches: Vec<_> = (0..4)
        .map(|_| {
            let mesh = meshes.add(blank_mesh());
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(ground.clone()),
                SceneObject::Patch,
                Visibility::Hidden,
            ));
            mesh
        })
        .collect();
    let waters: Vec<_> = (0..4)
        .map(|_| {
            let mesh = meshes.add(blank_mesh());
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(water_material.clone()),
                SceneObject::Water,
                Visibility::Hidden,
            ));
            mesh
        })
        .collect();
    let (empty_grass, bounds) = materials::grass_mesh(Default::default());
    let grass = meshes.add(empty_grass);
    let grass_entity = commands
        .spawn((
            Mesh3d(grass.clone()),
            MeshMaterial3d(grass_material.clone()),
            SceneObject::Grass,
            bounds,
            bevy::camera::visibility::NoAutoAabb,
            Visibility::Hidden,
        ))
        .id();
    let sun_material = materials.add(StandardMaterial {
        base_color: Color::linear_rgb(8.0, 7.0, 5.0),
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(192, 96))),
        MeshMaterial3d(sun_material),
        SceneObject::Sun,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    let moon_material = moon_materials.add(MoonMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            ..default()
        },
        extension: MoonExtension {
            settings: lunar::LunarSettings::default(),
            color: lunar_maps.color.clone(),
            normal: lunar_maps.normal.clone(),
        },
    });
    commands.spawn((
        Mesh3d(meshes.add(mesh_from(earth_math::rendering::lunar::mesh()))),
        MeshMaterial3d(moon_material.clone()),
        SceneObject::Moon,
        NotShadowCaster,
        NotShadowReceiver,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    let stars_mesh = meshes.add(star_mesh());
    let stars_material = star_materials.add(StarMaterial {
        base: StandardMaterial {
            base_color: Color::linear_rgb(0.18, 0.18, 0.18),
            unlit: true,
            fog_enabled: false,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        },
        extension: StarExtension {
            settings: StarSettings::default(),
        },
    });
    commands.spawn((
        Mesh3d(stars_mesh.clone()),
        MeshMaterial3d(stars_material.clone()),
        SceneObject::Stars,
        NotShadowCaster,
        NotShadowReceiver,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    let sky_mesh = meshes.add(blank_mesh());
    let sky_material = sky_materials.add(SkyMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        },
        extension: SkyExtension { unused: Vec4::ZERO },
    });
    commands.spawn((
        Mesh3d(sky_mesh.clone()),
        MeshMaterial3d(sky_material.clone()),
        SceneObject::Sky,
        NotShadowCaster,
        NotShadowReceiver,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.insert_resource(DirectionalLightShadowMap { size: 4096 });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.65, 0.76, 1.0),
        brightness: 350.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 100_000.0,
            shadow_maps_enabled: true,
            shadow_depth_bias: DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS,
            shadow_normal_bias: DirectionalLight::DEFAULT_SHADOW_NORMAL_BIAS,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.02,
            first_cascade_far_bound: 12.0,
            maximum_distance: 12_000.0,
            ..default()
        }
        .build(),
        Transform::default(),
    ));
    commands.spawn((
        Camera3d::default(),
        Msaa::Sample4,
        Projection::Perspective(PerspectiveProjection {
            near: 0.02,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.02),
            far: 4.0e12,
            ..default()
        }),
        Tonemapping::SomewhatBoringDisplayTransform,
        DebandDither::Disabled,
        Exposure::SUNLIGHT,
        bevy::pbr::DistanceFog::default(),
        CameraRole::Main,
    ));
    let fish_targets: Vec<_> = (0..6)
        .map(|face| {
            let target = images.add(Image::new_target_texture(
                1080,
                1080,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
            commands.spawn((
                Camera3d::default(),
                Camera {
                    order: -10 + face as isize,
                    is_active: false,
                    ..default()
                },
                RenderTarget::Image(target.clone().into()),
                Msaa::Sample4,
                Projection::Perspective(PerspectiveProjection {
                    fov: std::f32::consts::FRAC_PI_2,
                    aspect_ratio: 1.0,
                    near: 0.02,
                    far: 4.0e12,
                    near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.02),
                }),
                Tonemapping::SomewhatBoringDisplayTransform,
                DebandDither::Disabled,
                Exposure::SUNLIGHT,
                bevy::pbr::DistanceFog::default(),
                CameraRole::Face(face),
            ));
            target
        })
        .collect();
    let fish_material = fisheye_materials.add(FisheyeMaterial {
        parameters: Vec4::new(1920.0 / 1080.0, 70.0_f32.to_radians(), 0.0, 0.0),
        front: fish_targets[0].clone(),
        right: fish_targets[1].clone(),
        back: fish_targets[2].clone(),
        left: fish_targets[3].clone(),
        up: fish_targets[4].clone(),
        down: fish_targets[5].clone(),
        stars: FisheyeStars::default(),
        catalogue: buffers.add(ShaderBuffer::from(
            earth_math::rendering::stars::packed_catalogue(),
        )),
        sampling: Vec4::new(1.0, 0.0, 0.0, 0.0),
    });
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            is_active: false,
            ..default()
        },
        CameraRole::Composite,
        Tonemapping::None,
        DebandDither::Disabled,
    ));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(fish_material.clone()),
        Transform::from_scale(Vec3::new(1920.0, 1080.0, 1.0)),
        FishQuad,
    ));
    commands.insert_resource(RenderedEarth {
        dimensions: (1920, 1080),
        raster_dimensions: (1920, 1080),
        render_ms: 0,
        cpu_timings: RenderCpuTimings::default(),
        triangles: 0,
        draw_chunks: 0,
        view_visible_chunks: 0,
        view_visible_triangles: 0,
        lod_levels: [1; 4],
        mode: "GPU / 4x MSAA",
        grass_distance_m: None,
        presentation_ready: false,
        optical_status: OpticalDisplayStatus::Inactive,
        optical_view: None,
        optical_stellar_input: None,
        optical_submission_path: None,
        optical_submission_control: None,
        optical_stellar_control: None,
        mesh_counts: [0; 5],
        mesh_job: None,
        active_scene: None,
        mesh_preparation_ms: 0,
        origin,
        display_plan: DisplayPlan::at(state.observer, false),
        last_detail_focal: 0.0,
        last_quality: state.quality,
        reference,
        patches,
        waters,
        water_material,
        grass,
        grass_entity,
        grass_material,
        last_grass_enabled: state.grass_enabled,
        blades: Vec::new(),
        grass_placement: GrassPlacementCache::default(),
        grass_bounds: Vec::new(),
        grass_job: None,
        grass_generation: 0,
        scene_identity: 0,
        grass_observer: None,
        fish_targets,
        fish_material,
        moon_material,
        stars_material,
        sky_mesh,
        sky_material,
        last_sky: None,
        ground_material: ground,
    });
}

#[derive(SystemParam)]
// Keep the mutually exclusive ECS filters beside each mutable Transform query.
#[allow(clippy::type_complexity)]
pub struct RenderParameters<'w, 's> {
    commands: Commands<'w, 's>,
    refraction: refraction::Parameters<'w, 's>,
    terrain: terrain::Parameters<'w, 's>,
    survey: survey::Parameters<'w>,
    framebuffer: ResMut<'w, RenderedEarth>,
    meshes: ResMut<'w, Assets<Mesh>>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    ground_materials: ResMut<'w, Assets<GroundMaterial>>,
    moon_materials: ResMut<'w, Assets<MoonMaterial>>,
    star_materials: ResMut<'w, Assets<StarMaterial>>,
    sky_materials: ResMut<'w, Assets<SkyMaterial>>,
    grass_materials: ResMut<'w, Assets<GrassMaterial>>,
    water_materials: ResMut<'w, Assets<WaterMaterial>>,
    fish_materials: ResMut<'w, Assets<FisheyeMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    ambient: ResMut<'w, GlobalAmbientLight>,
    cameras: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Projection,
            &'static mut Camera,
            &'static CameraRole,
            Option<&'static mut bevy::pbr::DistanceFog>,
        ),
        (
            Without<SceneObject>,
            Without<DirectionalLight>,
            Without<FishQuad>,
            Without<refraction::RefractionQuad>,
        ),
    >,
    objects: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Visibility,
            &'static SceneObject,
        ),
        (
            Without<CameraRole>,
            Without<DirectionalLight>,
            Without<FishQuad>,
            Without<refraction::RefractionQuad>,
        ),
    >,
    lights: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut DirectionalLight),
        (
            Without<CameraRole>,
            Without<SceneObject>,
            Without<FishQuad>,
            Without<refraction::RefractionQuad>,
        ),
    >,
    quads: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (
            With<FishQuad>,
            Without<refraction::RefractionQuad>,
            Without<CameraRole>,
            Without<SceneObject>,
            Without<DirectionalLight>,
        ),
    >,
}

#[allow(clippy::too_many_arguments)] // Bevy injects independent scene and presentation resources.
pub fn render(
    _frame: ImguiFrame<'_>,
    state: Res<Explorer>,
    surface: Res<WorldSurface>,
    astronomy: Res<AstronomicalScene>,
    weather: Res<ancient_earth::weather::WeatherState>,
    survey_state: Res<ancient_earth::survey::SurveyState>,
    pacing: Res<super::frame_pacing::FramePacing>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut params: RenderParameters,
) {
    let window = match windows.single() {
        Ok(window)
            if !pacing.window_minimized
                && window.visible
                && window.physical_width() > 0
                && window.physical_height() > 0 =>
        {
            window
        }
        _ => {
            params.refraction.suspend();
            params.framebuffer.optical_view = None;
            params.framebuffer.optical_stellar_input = None;
            params.framebuffer.optical_submission_path = None;
            params.framebuffer.optical_submission_control = None;
            params.framebuffer.optical_stellar_control = None;
            params.framebuffer.optical_status = OpticalDisplayStatus::Inactive;
            params.framebuffer.presentation_ready = false;
            return;
        }
    };
    let started = Instant::now();
    let _cpu_section = super::cpu_watch::Section::start("renderer");
    params.framebuffer.cpu_timings = RenderCpuTimings {
        started_unix_us: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_micros()),
        ..Default::default()
    };
    let dimensions = (window.physical_width(), window.physical_height());
    let lens = state.lens();
    let fisheye = lens.projection == LensProjection::EquidistantFisheye;
    let fish_projection = earth_math::rendering::fisheye::forward_projection(lens);
    let frame = state.observer.frame();
    let source_scene = surface.shared(state.boundaries_enabled);
    let scene = source_scene.as_ref();
    let refracted = weather
        .view_profile_for_scene(state.observer, scene)
        .is_some();
    let grass_enabled = state.grass_enabled && !refracted;
    let optical_status = params.refraction.update(
        weather.view_profile_for_scene(state.observer, scene),
        &state,
        &survey_state,
        &source_scene,
        &astronomy,
        dimensions,
        Vec2::new(window.width(), window.height()),
        time.elapsed_secs_f64(),
        time.delta_secs_f64(),
        &mut params.buffers,
    );
    let cache = &mut params.framebuffer;
    cache.optical_status = optical_status;
    cache.optical_view =
        (cache.optical_status == OpticalDisplayStatus::Ready).then_some((state.observer, lens));
    cache.optical_stellar_input = params.refraction.stellar_input();
    cache.optical_submission_path = params.refraction.submission_path();
    cache.optical_submission_control = params.refraction.submission_control();
    cache.optical_stellar_control = params.refraction.stellar_control();
    if let Some(mut material) = params.sky_materials.get_mut(&cache.sky_material) {
        material.extension.unused.x = if fisheye { 1.0 } else { 0.0 };
    }
    if refracted {
        cache.draw_chunks = 0;
        cache.view_visible_chunks = 0;
        cache.view_visible_triangles = 0;
        // Curved rays have their own canonical surface/prism lookup. Do not rebuild
        // unused raster meshes or decorative fields while flying in this mode.
        // Preserve their coherent caches for the next straight-mode frame.
        for (_, _, mut camera, role, _) in &mut params.cameras {
            camera.is_active = matches!(role, CameraRole::Composite);
        }
        for (_, mut visibility) in &mut params.quads {
            *visibility = Visibility::Hidden;
        }
        cache.grass_distance_m = None;
        cache.presentation_ready = cache.optical_status == OpticalDisplayStatus::Ready;
        cache.dimensions = dimensions;
        cache.mode = "Curved optical rays / native pixels / f32 display";
        cache.render_ms = started.elapsed().as_millis();
        cache.cpu_timings.total_us = started.elapsed().as_micros();
        return;
    }
    let scene_identity = scene as *const earth_math::TerrainScene as usize;
    let boundaries_changed = cache.scene_identity != scene_identity;
    cache.scene_identity = scene_identity;
    let grass_setting_changed = cache.last_grass_enabled != grass_enabled;
    let grass_quality_changed = cache.last_quality != state.quality;
    cache.last_quality = state.quality;
    let mut origin_changed = false;
    if boundaries_changed {
        cache.grass_placement.clear();
    }
    let focal = if fisheye {
        fish_projection.focal_pixels(dimensions.1)
    } else {
        focal_pixels(lens.vertical_fov_deg, dimensions.1)
    };
    // Optical FOV updates every frame. Only finite tessellation preparation is
    // deferred, retaining the existing geometric bound and 16k focal budget.
    let detail_focal = (focal * [0.5, 1.0, 2.0, 4.0][state.quality.min(3)]).min(16_000.0);
    let rebase_distance = (state.observer.feet.height_m.abs() * 0.02).clamp(80.0, 20_000.0);
    let poll_started = Instant::now();
    let prepared = cache
        .mesh_job
        .as_mut()
        .and_then(|job| block_on(poll_once(job)));
    cache.cpu_timings.mesh_poll_us = poll_started.elapsed().as_micros();
    if let Some(prepared) = prepared {
        let commit_started = Instant::now();
        let _commit_section = super::cpu_watch::Section::start("renderer.mesh_commit");
        cache.cpu_timings.completed_worker_ms = Some(prepared.preparation_ms);
        cache.cpu_timings.worker_on_render_thread =
            Some(prepared.worker_thread == std::thread::current().id());
        cache.mesh_job = None;
        cache.mesh_counts[1] += 1;
        cache.mesh_counts[4] = 0;
        if Arc::as_ptr(&prepared.request.scene) as usize == scene_identity {
            // Accept useful same-scene work even after further view movement.
            // Every local mesh and its origin changes in this single system;
            // camera, material, survey and grass updates below use that origin.
            let next_origin = prepared.request.local;
            origin_changed = next_origin.origin != cache.origin.origin
                || next_origin.basis.east != cache.origin.basis.east
                || next_origin.basis.up != cache.origin.basis.up;
            cache.origin = next_origin;
            cache.display_plan = prepared.request.plan;
            cache.last_detail_focal = prepared.request.focal;
            cache.active_scene = Some(prepared.request.scene.clone());
            cache.triangles = prepared.triangles;
            cache.mesh_preparation_ms = prepared.preparation_ms;
            params
                .meshes
                .insert(cache.reference.id(), prepared.boundary.reference)
                .expect("reference mesh retained");
            let mut regions = prepared.boundary.regions.into_iter();
            for index in 0..cache.patches.len() {
                let (land, water, stride) = regions
                    .next()
                    .unwrap_or_else(|| (blank_mesh(), blank_mesh(), 0));
                params
                    .meshes
                    .insert(cache.patches[index].id(), land)
                    .expect("land mesh retained");
                params
                    .meshes
                    .insert(cache.waters[index].id(), water)
                    .expect("water mesh retained");
                cache.lod_levels[index] = stride;
            }
            if let Some(imported) = prepared.imported {
                params.terrain.commit(
                    imported,
                    prepared.request.scene,
                    &mut params.meshes,
                    &mut params.buffers,
                    &mut params.commands,
                );
            }
            if let Some(mut material) = params.ground_materials.get_mut(&cache.ground_material) {
                material.extension.settings = GroundSettings::at(cache.origin);
            }
            if let Some(mut material) = params.water_materials.get_mut(&cache.water_material) {
                material.extension.settings = WaterSettings::at(cache.origin);
            }
            cache.mesh_counts[2] += 1;
        } else {
            // Arc ownership prevents identity reuse while a job is in flight.
            // Never show a different physical scene under current measurements.
            cache.mesh_counts[3] += 1;
        }
        cache.cpu_timings.mesh_commit_us = commit_started.elapsed().as_micros();
    }
    let coherent = cache
        .active_scene
        .as_ref()
        .is_some_and(|source| Arc::as_ptr(source) as usize == scene_identity);
    let desired_plan = DisplayPlan::at(state.observer, cache.display_plan.space);
    let anchor_transition = desired_plan.space != cache.display_plan.space;
    let rebase = !coherent
        || anchor_transition
        || !desired_plan.space && frame.origin.distance(cache.origin.origin) > rebase_distance;
    let detail_valid = if desired_plan.space {
        cache
            .display_plan
            .detail
            .valid_for(state.observer, detail_focal, cache.last_detail_focal)
    } else {
        cache.last_detail_focal == detail_focal
    };
    let mesh_needed = rebase || !detail_valid;
    if mesh_needed && cache.mesh_job.is_none() {
        let request_started = Instant::now();
        let source = surface.shared(state.boundaries_enabled);
        let mut plan = desired_plan;
        if !rebase {
            // Projection responds now. Coordinate origin remains the coherent
            // installed origin; reference detail follows the requested real eye.
            plan.local = cache.origin;
            plan.coordinate_observer = cache.display_plan.coordinate_observer;
            if !plan.space {
                plan.detail = ReferenceDetail::near(frame.origin);
            }
        }
        let request = preparation::Request {
            classify_import: !params.terrain.contains_scene(&source),
            scene: source,
            observer: state.observer,
            local: plan.local,
            plan,
            focal: detail_focal,
        };
        cache.mesh_job = Some(AsyncComputeTaskPool::get().spawn(async move { request.prepare() }));
        cache.mesh_counts[0] += 1;
        cache.mesh_counts[4] = 1;
        cache.cpu_timings.mesh_request_us = request_started.elapsed().as_micros();
    }

    if boundaries_changed || origin_changed || grass_quality_changed || grass_setting_changed {
        cache.grass_generation = cache.grass_generation.wrapping_add(1);
        cache.grass_observer = None;
    }
    cache.last_grass_enabled = grass_enabled;
    if boundaries_changed || origin_changed || !grass_enabled && grass_setting_changed {
        cache.blades.clear();
        cache.grass_bounds.clear();
        let (empty, bounds) = materials::grass_mesh(Default::default());
        params
            .meshes
            .insert(cache.grass.id(), empty)
            .expect("grass mesh retained");
        params.commands.entity(cache.grass_entity).insert(bounds);
    }
    // A completed job carries geometry and the matching range data together.
    // Keep its immutable scene alive until identity validation finishes.
    let prepared = cache
        .grass_job
        .as_mut()
        .and_then(|job| block_on(poll_once(job)));
    if let Some(prepared) = prepared {
        cache.grass_job = None;
        let same_scene = Arc::as_ptr(&prepared.scene) as usize == scene_identity;
        if same_scene {
            cache.grass_placement = prepared.placement;
        }
        if same_scene
            && prepared.generation == cache.grass_generation
            && grass_enabled
            && prepared.observer.eye_position().distance(frame.origin) <= 4.0
        {
            params
                .meshes
                .insert(cache.grass.id(), prepared.mesh)
                .expect("grass mesh retained");
            params
                .commands
                .entity(cache.grass_entity)
                .insert(prepared.aabb);
            cache.blades = prepared.blades;
            cache.grass_bounds = prepared.bounds;
            cache.grass_observer = Some(prepared.observer);
        }
    }
    let regenerate_grass = cache
        .grass_observer
        .is_none_or(|observer| observer.eye_position().distance(frame.origin) > 4.0);
    if grass_enabled && coherent && regenerate_grass && cache.grass_job.is_none() {
        let mut placement = std::mem::take(&mut cache.grass_placement);
        let source = surface.shared(state.boundaries_enabled);
        let local = cache.origin;
        let observer = state.observer;
        let quality = state.quality;
        let generation = cache.grass_generation;
        cache.grass_job = Some(AsyncComputeTaskPool::get().spawn(async move {
            let blades = placement.rebuild(&source, observer, quality);
            let bounds = grass_ray_bounds(&blades, local);
            let (mesh, aabb) = materials::grass_mesh(grass_render_mesh(&blades, local));
            PreparedGrass {
                generation,
                scene: source,
                observer,
                placement,
                blades,
                bounds,
                mesh,
                aabb,
            }
        }));
    }
    let wind = grass_wind(time.elapsed_secs_f64());
    if let Some(mut material) = params.grass_materials.get_mut(&cache.grass_material) {
        material.extension.settings = GrassSettings {
            wind: Vec4::from_array(wind),
            eye: Vec3::from_array(cache.origin.position(frame.origin)).extend(0.0),
            sky_fill: Vec4::ZERO,
            eclipse: EclipseSettings::default(),
        };
    }
    cache.grass_distance_m = grass_intersection_cached(
        &cache.blades,
        cache.origin,
        &cache.grass_bounds,
        wind,
        frame.center_ray(),
    )
    .map(|hit| hit.distance_m)
    .filter(|distance| {
        scene
            .intersect(frame.center_ray())
            .is_none_or(|ground| *distance < ground.hit.distance_m)
    });
    cache.presentation_ready = coherent
        && !mesh_needed
        && cache.mesh_job.is_none()
        && cache.grass_job.is_none()
        && (!grass_enabled || cache.grass_observer.is_some());
    let (position, forward, up) = cache.origin.camera(frame);
    let position = Vec3::from_array(position);
    let forward = Vec3::from_array(forward).normalize();
    let up = Vec3::from_array(up).normalize();
    let right = forward.cross(up).normalize();
    let mut sky = Color::srgb(0.08, 0.17, 0.28);
    let mut haze = bevy::pbr::DistanceFog {
        color: Color::NONE,
        ..default()
    };
    for (_, mut light) in &mut params.lights {
        // Bevy computes the cascade depth reciprocal from translated f32
        // corner extrema. Remote 12 m slices can quantize to zero extent.
        light.shadow_maps_enabled =
            earth_math::rendering::continuous::nearby_shadow_maps(state.observer);
    }
    if let Some(mut material) = params.ground_materials.get_mut(&cache.ground_material) {
        material.extension.settings.sky_fill = Vec4::ZERO;
        material.extension.settings.eclipse = EclipseSettings::default();
    }
    if let Some(mut material) = params.water_materials.get_mut(&cache.water_material) {
        material.extension.settings.phase = materials::water_phase(time.elapsed_secs_f64());
        material.extension.settings.coordinates.sky_fill = Vec4::ZERO;
        material.extension.settings.coordinates.eclipse = EclipseSettings::default();
        material.extension.settings.environment = Vec4::ZERO;
    }
    if let Some(astro) = astronomy.frame {
        let sun_direction = (astro.sun.apparent_center_ecef_m - frame.origin)
            .normalized()
            .unwrap();
        let sun_up = sun_direction.dot(earth_math::EnuBasis::at(state.observer.eye_geodetic()).up);
        // Artistic clear-sky color/eye adaptation, not an atmospheric measurement.
        let daylight = ((sun_up + 0.12) / 0.22).clamp(0.0, 1.0) as f32;
        let atmosphere =
            (1.0 - (state.observer.eye_geodetic().height_m / 120_000.0).clamp(0.0, 1.0)) as f32;
        let observer_eclipse = astro.solar_eclipse(frame.origin).ok();
        let observer_visibility =
            observer_eclipse.map_or(1.0, |eclipse| eclipse.visible_fraction) as f32;
        // A categorical illustrative rule, not a magnitude/adaptation model:
        // partial and annular daytime eclipses do not reveal the star field.
        let eclipse_stars = if observer_eclipse
            .is_some_and(|eclipse| eclipse.kind == earth_math::astro::EclipseKind::Total)
        {
            0.96
        } else {
            0.0
        };
        // Artistic atmospheric response. Surface illumination uses its own
        // per-fragment field below, never this observer-level multiplier.
        let sky_response = 0.04 + 0.96 * observer_visibility.clamp(0.0, 1.0).sqrt();
        let sky_daylight = daylight * sky_response;
        let eclipse = EclipseSettings::from(astro.eclipse.gpu(cache.origin));
        if cache.last_sky != Some((sky_daylight, atmosphere)) {
            params
                .meshes
                .insert(
                    cache.sky_mesh.id(),
                    mesh_from(sky_mesh(sky_daylight, atmosphere)),
                )
                .expect("sky mesh retained");
            cache.last_sky = Some((sky_daylight, atmosphere));
        }
        sky = Color::srgb(
            0.003 + 0.20 * sky_daylight * atmosphere,
            0.006 + 0.39 * sky_daylight * atmosphere,
            0.014 + 0.62 * sky_daylight * atmosphere,
        );
        params.ambient.brightness = 0.5 + 2_400.0 * daylight * atmosphere;
        if let Some(mut material) = params.grass_materials.get_mut(&cache.grass_material) {
            material.extension.settings.eclipse = eclipse;
            material.extension.settings.sky_fill =
                Vec4::new(6_000.0, 8_000.0, 10_500.0, 0.0) * daylight * atmosphere;
        }
        if let Some(mut material) = params.ground_materials.get_mut(&cache.ground_material) {
            material.extension.settings.eclipse = eclipse;
            material.extension.settings.sky_fill =
                Vec4::new(4_800.0, 6_500.0, 8_500.0, 0.0) * daylight * atmosphere;
        }
        if let Some(mut material) = params.water_materials.get_mut(&cache.water_material) {
            material.extension.settings.coordinates.eclipse = eclipse;
            material.extension.settings.coordinates.sky_fill =
                Vec4::new(2_200.0, 4_000.0, 6_000.0, 0.0) * daylight * atmosphere;
            material.extension.settings.environment = Vec4::new(daylight, atmosphere, 0.0, 0.0);
        }
        // Finite illustrative aerial perspective, not measured meteorology.
        let low_sun = (1.0 - (sun_up.max(0.0) / 0.4).clamp(0.0, 1.0)) as f32;
        haze = bevy::pbr::DistanceFog {
            color: Color::linear_rgba(
                (0.42 + 0.08 * low_sun) * sky_response,
                (0.58 - 0.06 * low_sun) * sky_response,
                (0.76 - 0.18 * low_sun) * sky_response,
                0.70 * daylight * atmosphere,
            ),
            directional_light_color: Color::linear_rgba(
                sky_response,
                0.85 * sky_response,
                0.62 * sky_response,
                0.12,
            ),
            directional_light_exponent: 8.0,
            falloff: bevy::pbr::FogFalloff::Exponential {
                density: 0.000018 * atmosphere,
            },
        };
        // A single Earth-centered directional illumination model at every
        // altitude; apparent eye-to-Sun direction is retained for sky viewing.
        let earth_sun = astro.sun.center_ecef_m.normalized().unwrap();
        let light_direction = Vec3::from_array(cache.origin.direction(earth_sun)).normalize();
        let safe_up = if light_direction.dot(Vec3::Y).abs() > 0.99 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        for (mut transform, mut light) in &mut params.lights {
            *transform = Transform::default().looking_to(-light_direction, safe_up);
            light.illuminance = if sun_up > -0.01 || atmosphere < 0.1 {
                100_000.0
            } else {
                0.0
            };
            light.color = Color::linear_rgb(
                1.0,
                0.97 - 0.26 * low_sun * atmosphere,
                0.91 - 0.43 * low_sun * atmosphere,
            );
        }
        if let Some(mut material) = params.moon_materials.get_mut(&cache.moon_material) {
            material.extension.settings = lunar::LunarSettings::at(cache.origin, &astro);
        }
        if let Some(mut material) = params.star_materials.get_mut(&cache.stars_material) {
            let phase = earth_math::rendering::stars::display_phases(
                state.star_phase_seconds.unwrap_or(time.elapsed_secs_f64()),
                if state.star_phase_seconds.is_some() {
                    0.0
                } else {
                    time.delta_secs_f64()
                },
            );
            let up = cache
                .origin
                .direction(earth_math::EnuBasis::at(state.observer.eye_geodetic()).up);
            material.extension.settings = StarSettings {
                phase_sin: Vec4::from_array(phase[0]),
                phase_cos: Vec4::from_array(phase[1]),
                up: Vec3::from_array(up).extend(
                    earth_math::rendering::stars::scintillation::reference_zenith_variance(
                        state.observer.eye_geodetic().height_m,
                    )
                    .unwrap_or(0.0) as f32,
                ),
                visibility: Vec4::new(
                    1.0 - daylight * atmosphere * (1.0 - eclipse_stars),
                    if state.star_phase_seconds.is_some() {
                        0.0
                    } else {
                        time.delta_secs()
                    },
                    0.0,
                    0.0,
                ),
            };
        }
        if let Some(mut material) = params.fish_materials.get_mut(&cache.fish_material) {
            let phase = earth_math::rendering::stars::display_phases(
                state.star_phase_seconds.unwrap_or(time.elapsed_secs_f64()),
                if state.star_phase_seconds.is_some() {
                    0.0
                } else {
                    time.delta_secs_f64()
                },
            );
            let rotation = astro.icrf_to_ecef.transpose();
            let pack = |v: Vec3d| {
                let v = rotation.transform(v);
                Vec4::new(v.x as f32, v.y as f32, v.z as f32, 0.0)
            };
            material.stars = FisheyeStars {
                phase_sin: Vec4::from_array(phase[0]),
                phase_cos: Vec4::from_array(phase[1]),
                vertical: pack(earth_math::EnuBasis::at(state.observer.eye_geodetic()).up).with_w(
                    earth_math::rendering::stars::scintillation::reference_zenith_variance(
                        state.observer.eye_geodetic().height_m,
                    )
                    .unwrap_or(0.0) as f32,
                ),
                right: pack(frame.right),
                up: pack(frame.up),
                forward: pack(frame.forward),
                visibility: Vec4::new(
                    1.0 - daylight * atmosphere * (1.0 - eclipse_stars),
                    if state.star_phase_seconds.is_some() {
                        0.0
                    } else {
                        time.delta_secs()
                    },
                    0.0,
                    0.0,
                ),
            };
        }
        for (mut transform, mut visibility, object) in &mut params.objects {
            *visibility = Visibility::Visible;
            match object {
                SceneObject::Sun => {
                    transform.translation =
                        Vec3::from_array(cache.origin.position(astro.sun.apparent_center_ecef_m));
                    transform.scale = Vec3::splat(astro.sun.radius_m as f32);
                    transform.rotation = celestial_rotation(cache.origin, astro.sun.body_to_ecef);
                }
                SceneObject::Moon => {
                    transform.translation =
                        Vec3::from_array(cache.origin.position(astro.moon.apparent_center_ecef_m));
                    transform.scale = Vec3::splat(astro.moon.radius_m as f32);
                    transform.rotation = celestial_rotation(cache.origin, astro.moon.body_to_ecef);
                }
                SceneObject::Stars => {
                    if fisheye {
                        *visibility = Visibility::Hidden;
                    }
                    transform.translation = position;
                    transform.rotation = celestial_rotation(cache.origin, astro.icrf_to_ecef);
                }
                SceneObject::Sky => {
                    transform.translation = position;
                }
                _ => {}
            }
        }
    } else {
        if let Some(mut material) = params.fish_materials.get_mut(&cache.fish_material) {
            material.stars.visibility.x = 0.0;
        }
        for (_, mut visibility, object) in &mut params.objects {
            if matches!(
                object,
                SceneObject::Sun | SceneObject::Moon | SceneObject::Stars
            ) {
                *visibility = Visibility::Hidden;
            }
        }
    }
    let water_settings = params
        .water_materials
        .get(&cache.water_material)
        .map(|material| material.extension.settings)
        .unwrap_or(WaterSettings::at(cache.origin));
    let imported_triangles = params.terrain.update(
        scene,
        state.boundaries_enabled,
        cache.display_plan.coordinate_observer,
        coherent,
        water_settings,
    );
    let (draw_chunks, visible_chunks, visible_faces) = params.terrain.visibility_statistics();
    cache.draw_chunks = draw_chunks;
    cache.view_visible_chunks = visible_chunks;
    cache.view_visible_triangles = visible_faces;
    let survey_lighting = params
        .ground_materials
        .get(&cache.ground_material)
        .map(|material| material.extension.settings)
        .unwrap_or(GroundSettings::at(cache.origin));
    params.survey.update(
        &survey_state,
        cache.origin,
        frame.origin,
        survey_lighting,
        &mut params.commands,
        &mut params.meshes,
    );
    // A new physical scene is hidden until its complete display is available.
    // Global/DEM classification uses terrain's own draw chunks; its retained
    // legacy placeholders must never become visible during an origin rebase.
    let classifier = terrain::uses_classifier(scene);
    for (_, mut visibility, object) in &mut params.objects {
        if let Some(surface_visibility) = surface_visibility(
            object,
            coherent,
            classifier,
            grass_enabled,
            !cache.blades.is_empty(),
        ) {
            *visibility = surface_visibility;
        }
    }
    if !coherent {
        cache.triangles = 0;
    }
    if classifier {
        cache.triangles = imported_triangles;
        cache.lod_levels = [1; 4];
    }
    let fish_faces = earth_math::rendering::fisheye::active_faces(lens);
    for (mut transform, mut projection, mut camera, role, fog) in &mut params.cameras {
        if let Some(mut fog) = fog {
            *fog = haze.clone();
        }
        match role {
            CameraRole::Main => {
                camera.is_active = !fisheye && !refracted;
                camera.clear_color = sky.into();
                *transform = Transform::from_translation(position).looking_to(forward, up);
                if let Projection::Perspective(p) = &mut *projection {
                    p.fov = lens.vertical_fov_deg.to_radians() as f32;
                }
            }
            CameraRole::Face(face) => {
                camera.is_active = fish_faces[*face] && !refracted;
                camera.clear_color = sky.into();
                let (direction, face_up) = match face {
                    0 => (forward, up),
                    1 => (right, up),
                    2 => (-forward, up),
                    3 => (-right, up),
                    4 => (up, -forward),
                    _ => (-up, forward),
                };
                *transform = Transform::from_translation(position).looking_to(direction, face_up);
                if let Projection::Perspective(p) = &mut *projection {
                    p.fov = if *face == 0 {
                        fish_projection.fov_radians
                    } else {
                        std::f32::consts::FRAC_PI_2
                    };
                }
            }
            CameraRole::Composite => {
                camera.is_active = fisheye || refracted;
            }
        }
    }
    if dimensions != cache.raster_dimensions {
        for handle in &cache.fish_targets {
            params
                .images
                .insert(
                    handle.id(),
                    Image::new_target_texture(
                        dimensions.1,
                        dimensions.1,
                        TextureFormat::Rgba8UnormSrgb,
                        None,
                    ),
                )
                .expect("face image retained");
        }
    }
    if let Some(mut material) = params.fish_materials.get_mut(&cache.fish_material) {
        material.parameters = Vec4::new(
            dimensions.0 as f32 / dimensions.1 as f32,
            (lens.vertical_fov_deg.to_radians() * 0.5) as f32,
            dimensions.0 as f32,
            dimensions.1 as f32,
        );
        material.sampling = Vec4::new(
            fish_projection.projection_scale,
            if fish_projection.adaptive { 1.0 } else { 0.0 },
            0.0,
            0.0,
        );
    }
    for (mut quad, mut visibility) in &mut params.quads {
        *visibility = if refracted {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        quad.scale = Vec3::new(window.width(), window.height(), 1.0);
    }
    cache.dimensions = dimensions;
    cache.raster_dimensions = dimensions;
    cache.mode = if refracted {
        "Curved optical rays / native pixels / f32 display"
    } else if fisheye {
        if fish_faces[1] {
            "GPU fisheye / five views / 4x MSAA"
        } else {
            "GPU fisheye / one view / 4x MSAA"
        }
    } else {
        "GPU / 4x MSAA"
    };
    cache.render_ms = started.elapsed().as_millis();
    cache.cpu_timings.total_us = started.elapsed().as_micros();
}

fn celestial_rotation(local: LocalFrame, rotation: earth_math::astro::Mat3d) -> Quat {
    let axes = [
        Vec3d::new(1.0, 0.0, 0.0),
        Vec3d::new(0.0, 1.0, 0.0),
        Vec3d::new(0.0, 0.0, 1.0),
    ]
    .map(|axis| Vec3::from_array(local.direction(rotation.transform(axis))));
    Quat::from_mat3(&Mat3::from_cols(axes[0], axes[1], axes[2]))
}
fn star_mesh() -> Mesh {
    let mut mesh = RenderMesh::default();
    let mut uv = Vec::new();
    let mut ids = Vec::new();
    for (id, star) in earth_math::rendering::stars::catalogue().iter().enumerate() {
        let normal = star.direction;
        let axis = if normal.z.abs() > 0.9 {
            Vec3d::new(1.0, 0.0, 0.0)
        } else {
            Vec3d::new(0.0, 0.0, 1.0)
        };
        let right = normal.cross(axis).normalized().unwrap();
        let up = right.cross(normal).normalized().unwrap();
        let size = 1.0;
        let base = mesh.positions.len() as u32;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let point = normal * 2.0e12 + right * (size * x) + up * (size * y);
            mesh.positions
                .push([point.x as f32, point.y as f32, point.z as f32]);
            mesh.normals
                .push([normal.x as f32, normal.y as f32, normal.z as f32]);
            mesh.colors.push([
                star.color[0] * star.relative_brightness as f32,
                star.color[1] * star.relative_brightness as f32,
                star.color[2] * star.relative_brightness as f32,
                1.0,
            ]);
            uv.push([(x as f32 + 1.0) * 0.5, (y as f32 + 1.0) * 0.5]);
            ids.push([id as f32, 0.0]);
        }
        mesh.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh_from(mesh)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, ids)
}

/// Original artistic clear-sky gradient, located behind all modeled bodies and
/// decorative stars. It is neither an atmosphere solver nor a measurement target.
fn sky_mesh(daylight: f32, atmosphere: f32) -> RenderMesh {
    let mut mesh = RenderMesh::default();
    let columns = 96_usize;
    let rows = 64_usize;
    for row in 0..=rows {
        let latitude =
            std::f64::consts::FRAC_PI_2 - std::f64::consts::PI * row as f64 / rows as f64;
        let zenith = latitude.sin().max(0.0).powf(0.55) as f32;
        let below = if latitude < 0.0 { 0.8 } else { 1.0 };
        let strength = daylight * atmosphere * below;
        let color = [
            0.0003 + strength * (0.46 * (1.0 - zenith) + 0.055 * zenith),
            0.0007 + strength * (0.67 * (1.0 - zenith) + 0.20 * zenith),
            0.0018 + strength * (0.88 * (1.0 - zenith) + 0.48 * zenith),
            1.0,
        ];
        for column in 0..=columns {
            let longitude = std::f64::consts::TAU * column as f64 / columns as f64;
            let direction = Vec3d::new(
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            );
            let point = direction * 3.5e12;
            mesh.positions
                .push([point.x as f32, point.y as f32, point.z as f32]);
            mesh.normals.push([0.0, -1.0, 0.0]);
            mesh.colors.push(color);
        }
    }
    for row in 0..rows {
        for column in 0..columns {
            let nw = (row * (columns + 1) + column) as u32;
            let sw = nw + columns as u32 + 1;
            mesh.indices.extend([nw, sw, sw + 1, nw, sw + 1, nw + 1]);
        }
    }
    mesh
}
