//! Native-resolution curved-ray presentation, separate from straight-ray PBR.
mod compute;
mod primary;
mod readback;
mod reuse;
mod stellar;
pub(crate) use readback::OpticalReadback;
#[cfg(test)]
mod review;
use super::{
    CameraRole, Explorer, FishQuad, SceneObject, classification,
    materials::{EclipseSettings, GroundSettings, water_phase},
};
use ancient_earth::simulation::AstronomicalScene;
use bevy::{
    asset::{RenderAssetUsages, uuid_handle},
    ecs::system::SystemParam,
    prelude::*,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        renderer::RenderDevice,
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{Material2d, Material2dPlugin},
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use earth_math::{
    TerrainScene, Vec3d,
    atmosphere::{
        AtmosphereProfile,
        gpu::{GpuEllipsoid, scene::PackedScene},
    },
    rendering::LocalFrame,
};
use std::sync::Arc;

const SHADER: Handle<Shader> = uuid_handle!("5b38ee2c-105a-474a-9a25-e7942efc32cf");
const PRIMARY_SHADER: Handle<Shader> = uuid_handle!("91d029bd-3df0-4574-b8e8-f19aa3c949c4");
const STELLAR_SHADER: Handle<Shader> = uuid_handle!("ce52efdb-f20a-49df-a67d-c4ba99cabf1d");
#[derive(Clone, Debug, ShaderType)]
pub struct Settings {
    q0: Vec4,
    q1: Vec4,
    q2: Vec4,
    origin: Vec4,
    center: Vec4,
    latitude: Vec4,
    right: Vec4,
    up: Vec4,
    forward: Vec4,
    lens: Vec4,
    budget: Vec4,
    scene: Vec4,
    sun: Vec4,
    moon: Vec4,
    moon_light: Vec4,
    lunar: super::lunar::LunarSettings,
    environment: Vec4,
    icrf0: Vec4,
    icrf1: Vec4,
    icrf2: Vec4,
    ground_origin: Vec4,
    east: Vec4,
    vertical: Vec4,
    south: Vec4,
    phase: Vec4,
    star_sin: Vec4,
    star_cos: Vec4,
    star_environment: Vec4,
    region0: [Vec4; 4],
    region1: [Vec4; 4],
    layers: [Vec4; 128],
    eclipse: EclipseSettings,
    classification: classification::Settings,
    control: UVec4,
    stellar_control: UVec4,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            q0: Vec4::ZERO,
            q1: Vec4::ZERO,
            q2: Vec4::ZERO,
            origin: Vec4::ZERO,
            center: Vec4::ZERO,
            latitude: Vec4::ZERO,
            right: Vec4::ZERO,
            up: Vec4::ZERO,
            forward: Vec4::ZERO,
            lens: Vec4::ZERO,
            budget: Vec4::ZERO,
            scene: Vec4::ZERO,
            sun: Vec4::ZERO,
            moon: Vec4::ZERO,
            moon_light: Vec4::ZERO,
            lunar: super::lunar::LunarSettings::default(),
            environment: Vec4::ZERO,
            icrf0: Vec4::ZERO,
            icrf1: Vec4::ZERO,
            icrf2: Vec4::ZERO,
            ground_origin: Vec4::ZERO,
            east: Vec4::ZERO,
            vertical: Vec4::ZERO,
            south: Vec4::ZERO,
            phase: Vec4::ZERO,
            star_sin: Vec4::ZERO,
            star_cos: Vec4::ZERO,
            star_environment: Vec4::ZERO,
            region0: [Vec4::ZERO; 4],
            region1: [Vec4::ZERO; 4],
            layers: [Vec4::ZERO; 128],
            eclipse: EclipseSettings::default(),
            classification: classification::Settings::default(),
            control: UVec4::ZERO,
            stellar_control: UVec4::ZERO,
        }
    }
}
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct RefractionMaterial {
    #[uniform(0)]
    settings: Settings,
    #[storage(1, read_only)]
    boundaries: Handle<ShaderBuffer>,
    #[storage(2, read_only)]
    stars: Handle<ShaderBuffer>,
    #[storage(3, read_only)]
    native_water: Handle<ShaderBuffer>,
    #[storage(4, read_only)]
    global_land: Handle<ShaderBuffer>,
    #[texture(5)]
    #[sampler(6)]
    lunar_color: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    lunar_normal: Handle<Image>,
    #[storage(9, read_only)]
    geometry: Handle<ShaderBuffer>,
    #[storage(10, read_only)]
    frames: Handle<ShaderBuffer>,
    #[storage(11, read_only)]
    terminals: Handle<ShaderBuffer>,
    #[storage(12, read_only)]
    stellar_arrivals: Handle<ShaderBuffer>,
}
impl Material2d for RefractionMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
#[derive(Component)]
pub struct RefractionQuad;
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum DisplayStatus {
    #[default]
    Inactive,
    Pending,
    Ready,
    Unsupported(String),
    Error(String),
}
struct Prepared {
    packet: PackedScene,
    geometry: ShaderBuffer,
    boundaries: ShaderBuffer,
    native_water: ShaderBuffer,
    global_land: ShaderBuffer,
    regions: Vec<Region>,
}
struct Preparation {
    scene: Arc<TerrainScene>,
    task: Task<Result<Prepared, String>>,
}
#[derive(Resource)]
pub struct RefractionDisplay {
    /// Exact stellar input from the current material submission, not a
    /// recomputed daylight estimate or proof of GPU presentation.
    stellar_input: Option<f32>,
    submission_path: Option<&'static str>,
    material: Handle<RefractionMaterial>,
    buffer: Handle<ShaderBuffer>,
    scene: Option<Arc<TerrainScene>>,
    native_water: Handle<ShaderBuffer>,
    global_land: Handle<ShaderBuffer>,
    regions: Vec<Region>,
    geometry: Handle<ShaderBuffer>,
    frames: Handle<ShaderBuffer>,
    packet: Option<PackedScene>,
    frame_upload: reuse::FrameUpload,
    preparing: Option<Preparation>,
    error: Option<(Arc<TerrainScene>, String)>,
    compute_requested: bool,
    compute_required: bool,
    primary_reuse_requested: bool,
    stellar_cache_requested: bool,
    stars: Handle<ShaderBuffer>,
    stellar_arrivals: Handle<ShaderBuffer>,
    stellar_requests: Handle<ShaderBuffer>,
    terminals: Handle<ShaderBuffer>,
    terminal_dimensions: (u32, u32),
    frame_token: u64,
}
pub(super) struct Region {
    bounds: [f64; 4],
    width: usize,
    height: usize,
    offset: usize,
}
impl Region {
    pub(super) fn uniforms(&self, eye: earth_math::Geodetic) -> (Vec4, Vec4) {
        let [west, south, east, north] = self.bounds;
        let longitude = (eye.lon_deg - west + 180.0).rem_euclid(360.0) - 180.0;
        (
            Vec4::new(
                (north - eye.lat_deg).to_radians() as f32,
                longitude.to_radians() as f32,
                (3.0 * (self.height - 1) as f64 / (north - south).to_radians()) as f32,
                (3.0 * (self.width - 1) as f64 / (east - west).to_radians()) as f32,
            ),
            Vec4::new(
                (self.width - 1) as f32,
                (self.height - 1) as f32,
                self.offset as f32,
                0.0,
            ),
        )
    }
}

pub fn install(app: &mut App) {
    app.init_resource::<compute::Submission>()
        .init_resource::<compute::Status>()
        .init_resource::<compute::StellarStatus>();
    if !std::env::args().any(|argument| argument == "--optical-direct") {
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(
                PRIMARY_SHADER.id(),
                Shader::from_wgsl(
                    primary::primary_shader(),
                    "ancient_earth/primary_refraction.wgsl",
                ),
            )
            .expect("original optical compute shader");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(
                STELLAR_SHADER.id(),
                Shader::from_wgsl(
                    stellar::stellar_shader(),
                    "ancient_earth/stellar_refraction.wgsl",
                ),
            )
            .expect("original optical stellar shader");
        compute::install(app, PRIMARY_SHADER, STELLAR_SHADER);
    }
    readback::install(app);
    app.add_plugins(Material2dPlugin::<RefractionMaterial>::default())
        .add_systems(Startup, setup)
        .add_systems(
            First,
            suspend_without_output.after(super::super::frame_pacing::pace),
        );
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(
                format!(
                    "{}\n{}\n{}\n{}\n{}\n{}",
                    include_str!("../shaders/refraction.wesl"),
                    include_str!("../shaders/eclipse.wesl"),
                    include_str!("../shaders/stars_common.wesl"),
                    include_str!("../shaders/water_mask.wesl"),
                    include_str!("../shaders/lunar_common.wesl"),
                    include_str!("../shaders/refraction_scene.wesl")
                ),
                "ancient_earth/refraction.wesl",
            ),
        )
        .expect("original optical shader");
}

// The renderer normally runs inside the Dear ImGui pass. A missing/zero-area
// target can suppress that pass, so withdrawal of old GPU work must also run
// independently of it before extraction.
fn suspend_without_output(
    pacing: Res<super::super::frame_pacing::FramePacing>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut params: Parameters,
    mut rendered: ResMut<super::RenderedEarth>,
) {
    if !pacing.window_minimized
        && windows.single().is_ok_and(|window| {
            window.visible && window.physical_width() != 0 && window.physical_height() != 0
        })
    {
        return;
    }
    params.suspend();
    rendered.optical_view = None;
    rendered.optical_stellar_input = None;
    rendered.optical_submission_path = None;
    rendered.optical_submission_control = None;
    rendered.optical_stellar_control = None;
    rendered.optical_status = DisplayStatus::Inactive;
    rendered.presentation_ready = false;
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RefractionMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    lunar_maps: Res<super::lunar::LunarMaps>,
) {
    let buffer = buffers.add(ShaderBuffer::from(vec![[0.0_f32; 4]]));
    let empty = classification::Packed::empty();
    let native_water = buffers.add(ShaderBuffer::from(empty.native));
    let global_land = buffers.add(ShaderBuffer::from(empty.global));
    let geometry = buffers.add(ShaderBuffer::from(vec![[0.0_f32; 4]]));
    let frames = buffers.add(ShaderBuffer::from(vec![[0.0_f32; 4]; 6]));
    let terminals = buffers.add(ShaderBuffer::from(vec![[0_u32; 4]]));
    let stars = buffers.add(ShaderBuffer::from(
        earth_math::rendering::stars::packed_catalogue(),
    ));
    let stellar_arrivals = buffers.add(ShaderBuffer::with_size(
        stellar::ARRIVAL_BYTES,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let stellar_requests = buffers.add(ShaderBuffer::with_size(
        stellar::REQUEST_BYTES,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let material = materials.add(RefractionMaterial {
        settings: Settings::default(),
        boundaries: buffer.clone(),
        stars: stars.clone(),
        native_water: native_water.clone(),
        global_land: global_land.clone(),
        lunar_color: lunar_maps.color.clone(),
        lunar_normal: lunar_maps.normal.clone(),
        geometry: geometry.clone(),
        frames: frames.clone(),
        terminals: terminals.clone(),
        stellar_arrivals: stellar_arrivals.clone(),
    });
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(material.clone()),
        Transform::from_xyz(0.0, 0.0, 0.1).with_scale(Vec3::new(1920.0, 1080.0, 1.0)),
        Visibility::Hidden,
        RefractionQuad,
    ));
    commands.insert_resource(RefractionDisplay {
        stellar_input: None,
        submission_path: None,
        material,
        buffer,
        scene: None,
        native_water,
        global_land,
        regions: Vec::new(),
        geometry,
        frames,
        packet: None,
        frame_upload: reuse::FrameUpload::default(),
        preparing: None,
        error: None,
        compute_requested: !std::env::args().any(|argument| argument == "--optical-direct"),
        compute_required: std::env::args().any(|argument| argument == "--optical-compute"),
        primary_reuse_requested: !std::env::args()
            .any(|argument| argument == "--optical-no-primary-reuse"),
        // Share current-frame stellar solutions after the independent native
        // compositor and packet checks. Keep an original-path QA override.
        stellar_cache_requested: !std::env::args()
            .any(|argument| argument == "--optical-no-stellar-cache"),
        stars,
        stellar_arrivals,
        stellar_requests,
        terminals,
        terminal_dimensions: (0, 0),
        frame_token: 0,
    });
}
#[derive(SystemParam)]
pub struct Parameters<'w, 's> {
    display: ResMut<'w, RefractionDisplay>,
    materials: ResMut<'w, Assets<RefractionMaterial>>,
    submission: ResMut<'w, compute::Submission>,
    compute_status: Res<'w, compute::Status>,
    stellar_status: Res<'w, compute::StellarStatus>,
    render_device: Option<Res<'w, RenderDevice>>,
    #[allow(clippy::type_complexity)]
    quad: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (
            With<RefractionQuad>,
            Without<CameraRole>,
            Without<SceneObject>,
            Without<DirectionalLight>,
            Without<FishQuad>,
        ),
    >,
}
impl Parameters<'_, '_> {
    /// A minimized or absent window must not keep dispatching its last rays.
    pub fn suspend(&mut self) {
        if self.submission.0.take().is_some() {
            debug!("Withdrew optical compute submission: no drawable window");
        }
        self.display.stellar_input = None;
        self.display.submission_path = None;
        for (_, mut visibility) in &mut self.quad {
            *visibility = Visibility::Hidden;
        }
    }

    pub fn stellar_input(&self) -> Option<f32> {
        self.display.stellar_input
    }

    pub fn submission_path(&self) -> Option<&'static str> {
        self.display.submission_path
    }

    /// Exact current main-world compute request, for optional QA association.
    /// A token here is not proof that the GPU completed or displayed it.
    pub fn submission_control(&self) -> Option<[u32; 4]> {
        self.submission
            .0
            .as_ref()
            .map(|packet| packet.settings.control.to_array())
    }

    /// Exact table token expected by the current compositor. This records the
    /// selected path, not GPU completion or presentation.
    pub fn stellar_control(&self) -> Option<[u32; 4]> {
        self.submission.0.as_ref()?;
        let control = self
            .materials
            .get(&self.display.material)?
            .settings
            .stellar_control;
        (control != UVec4::ZERO).then_some(control.to_array())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        profile: Option<&AtmosphereProfile>,
        state: &Explorer,
        survey: &ancient_earth::survey::SurveyState,
        scene: &Arc<TerrainScene>,
        astronomy: &AstronomicalScene,
        dimensions: (u32, u32),
        logical: Vec2,
        time: f64,
        exposure_seconds: f64,
        buffers: &mut Assets<ShaderBuffer>,
    ) -> DisplayStatus {
        self.submission.0 = None;
        self.display.stellar_input = None;
        self.display.submission_path = None;
        for (mut transform, mut visible) in &mut self.quad {
            *visible = if profile.is_some() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            transform.scale = logical.extend(1.0);
        }
        let Some(profile) = profile else {
            return DisplayStatus::Inactive;
        };
        if let Some(preparing) = self.display.preparing.as_mut()
            && let Some(result) = block_on(poll_once(&mut preparing.task))
        {
            let completed = self.display.preparing.take().unwrap();
            if Arc::ptr_eq(&completed.scene, scene) {
                match result {
                    Ok(prepared) => {
                        buffers
                            .insert(self.display.geometry.id(), prepared.geometry)
                            .expect("retained optical geometry");
                        buffers
                            .insert(self.display.buffer.id(), prepared.boundaries)
                            .expect("retained boundaries");
                        buffers
                            .insert(self.display.native_water.id(), prepared.native_water)
                            .expect("retained native masks");
                        buffers
                            .insert(self.display.global_land.id(), prepared.global_land)
                            .expect("retained global masks");
                        self.display.regions = prepared.regions;
                        self.display.packet = Some(prepared.packet);
                        self.display.scene = Some(completed.scene);
                        self.display.error = None;
                    }
                    Err(error) => self.display.error = Some((completed.scene, error)),
                }
            }
        }
        let coherent = self
            .display
            .scene
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, scene));
        let failure = self
            .display
            .error
            .as_ref()
            .filter(|(old, _)| Arc::ptr_eq(old, scene))
            .map(|(_, error)| error.clone());
        if !coherent && failure.is_none() && self.display.preparing.is_none() {
            let source = scene.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let mut packet = PackedScene::prepare(&source)?;
                let geometry = reuse::immutable(std::mem::take(&mut packet.geometry));
                let (data, regions) = boundary_data(source.boundaries());
                let masks = classification::Packed::from_scene(&source);
                Ok(Prepared {
                    packet,
                    geometry,
                    boundaries: ShaderBuffer::from(data),
                    native_water: ShaderBuffer::from(masks.native),
                    global_land: ShaderBuffer::from(masks.global),
                    regions,
                })
            });
            self.display.preparing = Some(Preparation {
                scene: scene.clone(),
                task,
            });
        }
        let mut status = if coherent {
            DisplayStatus::Ready
        } else if let Some(error) = failure {
            DisplayStatus::Error(error)
        } else {
            DisplayStatus::Pending
        };
        let local = LocalFrame::at(state.observer);
        if coherent {
            let markers = if survey.markers_visible {
                survey.markers.as_slice()
            } else {
                &[]
            };
            match self
                .display
                .packet
                .as_ref()
                .unwrap()
                .frame_data(local, markers)
            {
                Ok(data) => {
                    if let Some(buffer) = self.display.frame_upload.changed(data) {
                        buffers
                            .insert(self.display.frames.id(), buffer)
                            .expect("retained optical frame transforms");
                    }
                }
                Err(error) => status = DisplayStatus::Unsupported(error),
            }
        }
        let geometry = GpuEllipsoid::at(state.observer);
        let frame = state.observer.frame();
        let lens = state.lens();
        let vector = |v: Vec3d| Vec3::from_array(local.direction(v)).extend(0.0);
        let ground = GroundSettings::at(local);
        let star_phase = earth_math::rendering::stars::display_phases(
            state.star_phase_seconds.unwrap_or(time),
            if state.star_phase_seconds.is_some() {
                0.0
            } else {
                exposure_seconds
            },
        );
        let mut s = Settings {
            classification: classification::Settings::at(
                scene,
                state.observer.eye_geodetic(),
                state.boundaries_enabled,
            ),
            q0: Vec4::from_array(geometry.rows[0]),
            q1: Vec4::from_array(geometry.rows[1]),
            q2: Vec4::from_array(geometry.rows[2]),
            origin: Vec4::from_array(geometry.origin),
            center: Vec4::from_array(geometry.center),
            latitude: Vec4::from_array(geometry.latitude),
            right: vector(frame.right),
            up: vector(frame.up),
            forward: vector(frame.forward),
            lens: Vec4::new(
                dimensions.0 as f32 / dimensions.1 as f32,
                (lens.vertical_fov_deg.to_radians() * 0.5) as f32,
                u8::from(lens.projection == earth_math::Projection::EquidistantFisheye) as f32,
                if lens.projection == earth_math::Projection::EquidistantFisheye {
                    (f64::from(dimensions.1) / lens.vertical_fov_deg.to_radians()) as f32
                } else {
                    earth_math::rendering::focal_pixels(lens.vertical_fov_deg, dimensions.1) as f32
                },
            ),
            budget: Vec4::new(
                profile.layers().len() as f32,
                profile.top_height_m() as f32,
                (1.0 / (earth_math::WGS84_A_M * (1.0 - earth_math::WGS84_E2)
                    + earth_math::MIN_HEIGHT_M)
                    + profile.maximum_index_gradient()) as f32,
                u8::from(
                    status == DisplayStatus::Ready
                        && earth_math::atmosphere::gpu::display_supported(state.observer),
                ) as f32,
            ),
            scene: Vec4::new(
                profile.maximum_index_gradient() as f32,
                profile.base_height_m() as f32,
                earth_math::MIN_HEIGHT_M as f32,
                0.0,
            ),
            ground_origin: ground.origin,
            east: ground.east,
            vertical: ground.up,
            south: ground.south,
            phase: water_phase(time),
            star_sin: Vec4::from_array(star_phase[0]),
            star_cos: Vec4::from_array(star_phase[1]),
            star_environment: Vec4::new(
                earth_math::rendering::stars::scintillation::reference_zenith_variance(
                    state.observer.eye_geodetic().height_m,
                )
                .unwrap_or(0.0) as f32,
                if state.star_phase_seconds.is_some() {
                    0.0
                } else {
                    exposure_seconds as f32
                },
                dimensions.0 as f32,
                dimensions.1 as f32,
            ),
            ..default()
        };
        for (i, layer) in profile.layers().iter().enumerate() {
            let gradient = profile.layers().get(i + 1).map_or(0.0, |next| {
                (next.refractivity - layer.refractivity) / (next.height_m - layer.height_m)
            });
            s.layers[i] = Vec4::new(
                (profile.base_height_m() + layer.height_m) as f32,
                layer.refractivity as f32,
                gradient as f32,
                0.0,
            );
        }
        let eye = state.observer.eye_geodetic();
        for (i, region) in self.display.regions.iter().enumerate().take(4) {
            (s.region0[i], s.region1[i]) = region.uniforms(eye);
        }
        if let Some(astro) = astronomy.frame {
            let body = |p: Vec3d, r: f64| Vec3::from_array(local.position(p)).extend(r as f32);
            s.sun = body(astro.sun.apparent_center_ecef_m, astro.sun.radius_m);
            s.moon = body(astro.moon.apparent_center_ecef_m, astro.moon.radius_m);
            s.moon_light = vector(astro.moon_to_sun_direction_ecef);
            s.lunar = super::lunar::LunarSettings::at(local, &astro);
            let sun_direction = (astro.sun.apparent_center_ecef_m - frame.origin)
                .normalized()
                .unwrap();
            let daylight =
                ((sun_direction.dot(local.basis.up) + 0.12) / 0.22).clamp(0.0, 1.0) as f32;
            let atmosphere = (1.0 - (eye.height_m / 120000.0).clamp(0.0, 1.0)) as f32;
            let eclipse = astro.solar_eclipse(frame.origin).ok();
            let visibility = eclipse.map_or(1.0, |e| e.visible_fraction) as f32;
            let response = 0.04 + 0.96 * visibility.sqrt();
            let stars = if eclipse.is_some_and(|e| e.kind == earth_math::astro::EclipseKind::Total)
            {
                0.96
            } else {
                0.0
            };
            s.environment = Vec4::new(
                daylight,
                atmosphere * response,
                1.0 - daylight * atmosphere * (1.0 - stars),
                0.0,
            );
            s.eclipse = EclipseSettings::from(astro.eclipse.gpu(local));
            let rotation = astro.icrf_to_ecef;
            s.icrf0 = vector(rotation.transform(Vec3d::new(1.0, 0.0, 0.0)));
            s.icrf1 = vector(rotation.transform(Vec3d::new(0.0, 1.0, 0.0)));
            s.icrf2 = vector(rotation.transform(Vec3d::new(0.0, 0.0, 1.0)));
        }
        let use_compute = self.display.compute_requested
            && (self.display.compute_required
                || self.render_device.as_ref().is_none_or(|device| {
                    compute::target_supported(dimensions.0, dimensions.1, &device.limits())
                }));
        if use_compute {
            self.display.frame_token = self.display.frame_token.wrapping_add(1);
            let token = self.display.frame_token;
            s.control = UVec4::new(
                dimensions.0,
                dimensions.1,
                token as u32,
                (token >> 32) as u32,
            );
            if status == DisplayStatus::Ready {
                status = self.prepare_compute(&s, dimensions, buffers);
                if status == DisplayStatus::Ready
                    && self.display.stellar_cache_requested
                    && stellar::eligible(&s)
                    && matches!(self.stellar_status.get(), compute::Availability::Ready { control }
                        if control.x == dimensions.0 && control.y == dimensions.1)
                {
                    s.stellar_control = s.control;
                }
            }
        }
        if let Some(mut material) = self.materials.get_mut(&self.display.material) {
            self.display.stellar_input =
                (status == DisplayStatus::Ready).then_some(s.environment.z);
            self.display.submission_path =
                (status == DisplayStatus::Ready).then_some(if use_compute {
                    "compute"
                } else {
                    "direct fragment"
                });
            material.settings = s;
        } else {
            self.submission.0 = None;
            return DisplayStatus::Error("Optical presentation material is unavailable".into());
        }
        status
    }

    fn prepare_compute(
        &mut self,
        settings: &Settings,
        dimensions: (u32, u32),
        buffers: &mut Assets<ShaderBuffer>,
    ) -> DisplayStatus {
        let Some(device) = &self.render_device else {
            return DisplayStatus::Pending;
        };
        let Some(required) = compute::terminal_buffer_bytes(dimensions.0, dimensions.1) else {
            return DisplayStatus::Unsupported("Invalid native optical output extent".into());
        };
        let limits = device.limits();
        if !compute::target_supported(dimensions.0, dimensions.1, &limits) {
            return DisplayStatus::Unsupported(
                "Native optical output exceeds this device's compute-buffer limits".into(),
            );
        }
        if self.display.terminal_dimensions != dimensions {
            buffers
                .insert(
                    self.display.terminals.id(),
                    ShaderBuffer::with_size(required, RenderAssetUsages::RENDER_WORLD),
                )
                .expect("retained optical terminals");
            self.display.terminal_dimensions = dimensions;
        }
        self.submission.0 = Some(compute::Packet {
            reuse_requested: self.display.primary_reuse_requested,
            settings: compute::PrimarySettings::from_view(settings, settings.control),
            geometry: self.display.geometry.clone(),
            frames: self.display.frames.clone(),
            terminals: self.display.terminals.clone(),
            stellar: (self.display.stellar_cache_requested && stellar::eligible(settings)).then(
                || compute::StellarPacket {
                    settings: stellar::StellarSettings::from_view(settings, settings.control),
                    catalogue: self.display.stars.clone(),
                    arrivals: self.display.stellar_arrivals.clone(),
                    requests: self.display.stellar_requests.clone(),
                },
            ),
        });
        match self.compute_status.get() {
            compute::Availability::Ready { control }
                if control.x == dimensions.0 && control.y == dimensions.1 =>
            {
                DisplayStatus::Ready
            }
            compute::Availability::Ready { .. } | compute::Availability::Pending => {
                DisplayStatus::Pending
            }
            compute::Availability::Unsupported(message) => DisplayStatus::Unsupported(message),
            compute::Availability::Error(message) => DisplayStatus::Error(message),
        }
    }
}
pub(super) fn boundary_data(scene: &TerrainScene) -> (Vec<[f32; 4]>, Vec<Region>) {
    let mut data = vec![[0.0; 4]];
    let mut regions = Vec::new();
    for patch in scene.patches().iter().take(4) {
        let Some(parameters) = patch.boundary_parameters() else {
            continue;
        };
        let Some(triangles) = patch.boundary_triangles() else {
            continue;
        };
        let (width, height, _) = patch.presentation_grid();
        let mut bins = vec![Vec::<usize>::new(); (width - 1) * (height - 1)];
        for (indices, code) in triangles {
            let points = indices.map(|index| parameters[index]);
            let row =
                ((points.iter().map(|p| p[0]).sum::<f64>() / 9.0).floor() as usize).min(height - 2);
            let col =
                ((points.iter().map(|p| p[1]).sum::<f64>() / 9.0).floor() as usize).min(width - 2);
            let id = data.len();
            bins[row * (width - 1) + col].push(id);
            data.push([
                points[0][0] as f32,
                points[0][1] as f32,
                points[1][0] as f32,
                points[1][1] as f32,
            ]);
            data.push([
                points[2][0] as f32,
                points[2][1] as f32,
                u8::from((1..=3).contains(&code)) as f32,
                0.0,
            ]);
        }
        let offset = data.len();
        data.resize(offset + bins.len(), [0.0; 4]);
        for (i, bin) in bins.into_iter().enumerate() {
            data[offset + i] = [data.len() as f32, bin.len() as f32, 0.0, 0.0];
            data.extend(bin.into_iter().map(|index| [index as f32, 0.0, 0.0, 0.0]));
        }
        regions.push(Region {
            bounds: patch.bounds(),
            width,
            height,
            offset,
        });
    }
    assert!(
        data.len() < 16_777_216,
        "boundary buffer indices must be exact f32 integers"
    );
    (data, regions)
}
