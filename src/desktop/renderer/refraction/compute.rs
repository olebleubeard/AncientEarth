//! Same-frame primary-ray dispatch. Numerical packing and shader code are
//! supplied by the independently reviewed optical implementation.
use super::{
    reuse,
    stellar::{self, StellarSettings},
};
use bevy::{
    core_pipeline::schedule::camera_driver,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{storage_buffer, storage_buffer_read_only, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
        settings::WgpuLimits,
        storage::{GpuShaderBuffer, ShaderBuffer},
    },
    shader::ShaderCacheError,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, ShaderType)]
pub(super) struct PrimarySettings {
    pub q0: Vec4,
    pub q1: Vec4,
    pub q2: Vec4,
    pub origin: Vec4,
    pub center: Vec4,
    pub right: Vec4,
    pub up: Vec4,
    pub forward: Vec4,
    pub lens: Vec4,
    pub budget: Vec4,
    pub scene: Vec4,
    pub control: UVec4,
    pub layers: [Vec4; 128],
}

#[derive(Clone)]
pub(super) struct Packet {
    pub reuse_requested: bool,
    pub settings: PrimarySettings,
    pub geometry: Handle<ShaderBuffer>,
    pub frames: Handle<ShaderBuffer>,
    pub terminals: Handle<ShaderBuffer>,
    pub stellar: Option<StellarPacket>,
}

#[derive(Clone)]
pub(super) struct StellarPacket {
    pub settings: StellarSettings,
    pub catalogue: Handle<ShaderBuffer>,
    pub arrivals: Handle<ShaderBuffer>,
    pub requests: Handle<ShaderBuffer>,
}

#[derive(Resource, Default, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub(super) struct Submission(pub Option<Packet>);

#[derive(Clone, Debug, Default)]
pub(super) enum Availability {
    #[default]
    Pending,
    /// Current render-world inputs and pipeline can be submitted. This is not
    /// hardware completion or a claim that a main-world frame has appeared.
    Ready {
        control: UVec4,
    },
    Unsupported(String),
    Error(String),
}

#[derive(Resource, Clone, Default)]
pub(super) struct Status(Arc<Mutex<Availability>>);
impl Status {
    pub fn get(&self) -> Availability {
        self.0
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| Availability::Error("Optical compute status is unavailable".into()))
    }
    fn publish(&self, state: Availability) {
        if let Ok(mut current) = self.0.lock() {
            *current = state;
        }
    }
}

#[derive(Resource, Clone, Default)]
pub(super) struct StellarStatus(Status);
impl StellarStatus {
    pub fn get(&self) -> Availability {
        self.0.get()
    }
    fn publish(&self, state: Availability) {
        self.0.publish(state);
    }
}

/// One uvec4 header followed by three lossless uvec4 records per native pixel.
/// Checked sizes prevent an oversized window from allocating a wrapped buffer.
pub(super) fn terminal_buffer_bytes(width: u32, height: u32) -> Option<u64> {
    if width == 0 || height == 0 {
        return None;
    }
    u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(48)?
        .checked_add(16)
}

/// Select a submission path before allocating a native-resolution buffer.
/// Larger targets can use the same solver directly in the fragment stage.
pub(super) fn target_supported(width: u32, height: u32, limits: &WgpuLimits) -> bool {
    terminal_buffer_bytes(width, height).is_some_and(|required| {
        required <= limits.max_storage_buffer_binding_size
            && required <= limits.max_buffer_size
            && width.div_ceil(32) <= limits.max_compute_workgroups_per_dimension
            && height <= limits.max_compute_workgroups_per_dimension
            && limits.max_compute_workgroup_size_x >= 32
            && limits.max_compute_workgroup_size_y >= 1
            && limits.max_compute_workgroup_size_z >= 1
            && limits.max_compute_invocations_per_workgroup >= 32
    })
}

#[derive(Resource)]
struct Source {
    primary: Handle<Shader>,
    stellar: Handle<Shader>,
}

pub(super) fn install(app: &mut App, shader: Handle<Shader>, stellar_shader: Handle<Shader>) {
    let status = Status::default();
    let stellar_status = StellarStatus::default();
    app.init_resource::<Submission>()
        .insert_resource(status.clone())
        .insert_resource(stellar_status.clone())
        .add_plugins(ExtractResourcePlugin::<Submission>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .insert_resource(status)
        .insert_resource(stellar_status)
        .insert_resource(Source {
            primary: shader,
            stellar: stellar_shader,
        })
        .init_resource::<Prepared>()
        .init_resource::<LastPrimary>()
        .add_systems(RenderStartup, initialize)
        .add_systems(Render, prepare.in_set(RenderSystems::PrepareBindGroups))
        .add_systems(RenderGraph, dispatch.before(camera_driver));
}

#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    id: Option<CachedComputePipelineId>,
}

#[derive(Resource)]
struct StellarPipeline {
    layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    ids: Option<[CachedComputePipelineId; 3]>,
}

fn initialize(mut commands: Commands, source: Res<Source>) {
    let layout = BindGroupLayoutDescriptor::new(
        "Optical primary rays",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<PrimarySettings>(false),
                storage_buffer_read_only::<Vec<Vec4>>(false),
                storage_buffer_read_only::<Vec<Vec4>>(false),
                storage_buffer::<Vec<UVec4>>(false),
            ),
        ),
    );
    commands.insert_resource(Pipeline {
        layout,
        shader: source.primary.clone(),
        id: None,
    });
    commands.insert_resource(StellarPipeline {
        layout: BindGroupLayoutDescriptor::new(
            "Optical stellar arrivals",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<StellarSettings>(false),
                    storage_buffer_read_only::<Vec<Vec4>>(false),
                    storage_buffer::<Vec<UVec4>>(false),
                    storage_buffer_read_only::<Vec<UVec4>>(false),
                    storage_buffer::<Vec<u32>>(false),
                ),
            ),
        ),
        shader: source.stellar.clone(),
        ids: None,
    });
}

#[derive(Resource, Default)]
struct Prepared {
    uniform: Option<UniformBuffer<PrimarySettings>>,
    header: Option<UniformBuffer<UVec4>>,
    packet: Option<PreparedPacket>,
    stellar_uniform: Option<UniformBuffer<StellarSettings>>,
    stellar_packet: Option<PreparedStellar>,
}
struct PreparedStellar {
    bindings: BindGroup,
    control: UVec4,
}
struct PreparedPacket {
    bindings: BindGroup,
    workgroups: [u32; 3],
    control: UVec4,
    key: Option<reuse::Key>,
    header: Buffer,
    terminals: Buffer,
}

/// The last payload encoded into this device's ordered render command stream.
/// This is not a CPU assertion that the GPU has finished executing the work.
#[derive(Resource, Default)]
struct LastPrimary(Option<reuse::Key>);

#[allow(clippy::too_many_arguments)]
fn prepare(
    submission: Res<Submission>,
    mut pipeline: ResMut<Pipeline>,
    mut stellar_pipeline: ResMut<StellarPipeline>,
    cache: Res<PipelineCache>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    status: Res<Status>,
    stellar_status: Res<StellarStatus>,
    mut prepared: ResMut<Prepared>,
) {
    // Never dispatch old bindings when extraction, assets or a pipeline are
    // unavailable. The compositor independently rejects an old frame token.
    prepared.packet = None;
    prepared.stellar_packet = None;
    status.publish(Availability::Pending);
    stellar_status.publish(Availability::Pending);
    let Some(packet) = &submission.0 else {
        return;
    };
    let control = packet.settings.control;
    let Some(required) = terminal_buffer_bytes(control.x, control.y) else {
        status.publish(Availability::Unsupported(
            "Invalid optical output extent".into(),
        ));
        return;
    };
    let limits = device.limits();
    let workgroups = [control.x.div_ceil(32), control.y, 1];
    if !target_supported(control.x, control.y, &limits) {
        status.publish(Availability::Unsupported(
            "Native optical output exceeds this device's compute-buffer limits".into(),
        ));
        return;
    }
    // Optics may be disabled for the whole session. Queue the expensive
    // pipeline only when a current optical packet first requests it.
    let pipeline_id = if let Some(id) = pipeline.id {
        id
    } else {
        let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("ancient_earth_primary_trace_pipeline".into()),
            layout: vec![pipeline.layout.clone()],
            shader: pipeline.shader.clone(),
            entry_point: Some("primary_trace".into()),
            ..default()
        });
        pipeline.id = Some(id);
        id
    };
    match cache.get_compute_pipeline_state(pipeline_id) {
        CachedPipelineState::Ok(_) => {}
        CachedPipelineState::Err(
            ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable,
        )
        | CachedPipelineState::Queued
        | CachedPipelineState::Creating(_) => return,
        CachedPipelineState::Err(error) => {
            status.publish(Availability::Error(error.to_string()));
            return;
        }
    }
    let (Some(geometry), Some(frames), Some(terminals)) = (
        buffers.get(&packet.geometry),
        buffers.get(&packet.frames),
        buffers.get(&packet.terminals),
    ) else {
        return;
    };
    if [&geometry.buffer, &frames.buffer, &terminals.buffer]
        .iter()
        .any(|buffer| buffer.size() > limits.max_storage_buffer_binding_size)
    {
        status.publish(Availability::Unsupported(
            "Optical buffer capacity exceeds this device's binding limit".into(),
        ));
        return;
    }
    if terminals.buffer.size() < required {
        return;
    }
    let uniform = prepared
        .uniform
        .get_or_insert_with(|| UniformBuffer::from(packet.settings.clone()));
    uniform.set(packet.settings.clone());
    uniform.write_buffer(&device, &queue);
    let bindings = device.create_bind_group(
        Some("Optical current-frame packet"),
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &*uniform,
            geometry.buffer.as_entire_buffer_binding(),
            frames.buffer.as_entire_buffer_binding(),
            terminals.buffer.as_entire_buffer_binding(),
        )),
    );
    let header = prepared.header.get_or_insert_with(|| {
        let mut buffer = UniformBuffer::from(control);
        buffer.add_usages(BufferUsages::COPY_SRC);
        buffer
    });
    header.set(control);
    header.write_buffer(&device, &queue);
    let header = header.buffer().expect("written optical header").clone();
    let immutable_inputs = !geometry.buffer.usage().contains(BufferUsages::COPY_DST)
        && !frames.buffer.usage().contains(BufferUsages::COPY_DST);
    let key = (packet.reuse_requested && immutable_inputs).then(|| {
        reuse::Key::new(
            &packet.settings,
            [
                geometry.buffer.id(),
                frames.buffer.id(),
                terminals.buffer.id(),
            ],
            cache
                .get_compute_pipeline(pipeline_id)
                .expect("ready primary pipeline")
                .id(),
        )
    });
    prepared.packet = Some(PreparedPacket {
        bindings,
        workgroups,
        control,
        key,
        header,
        terminals: terminals.buffer.clone(),
    });
    status.publish(Availability::Ready { control });
    if let Some(stellar_packet) = &packet.stellar {
        prepare_stellar(
            stellar_packet,
            control,
            terminals,
            &buffers,
            &cache,
            &device,
            &queue,
            &mut stellar_pipeline,
            &mut prepared,
            &stellar_status,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_stellar(
    packet: &StellarPacket,
    control: UVec4,
    terminals: &GpuShaderBuffer,
    buffers: &RenderAssets<GpuShaderBuffer>,
    cache: &PipelineCache,
    device: &RenderDevice,
    queue: &RenderQueue,
    pipeline: &mut StellarPipeline,
    prepared: &mut Prepared,
    status: &StellarStatus,
) {
    if packet.settings.control != control {
        status.publish(Availability::Error(
            "Stellar and primary frame tokens differ".into(),
        ));
        return;
    }
    let ids = *pipeline.ids.get_or_insert_with(|| {
        [
            "clear_stellar_requests",
            "gather_stellar_requests",
            "requested_stellar_table",
        ]
        .map(|entry| {
            cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some(entry.into()),
                layout: vec![pipeline.layout.clone()],
                shader: pipeline.shader.clone(),
                entry_point: Some(entry.into()),
                ..default()
            })
        })
    });
    for id in ids {
        match cache.get_compute_pipeline_state(id) {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Err(
                ShaderCacheError::ShaderNotLoaded(_)
                | ShaderCacheError::ShaderImportNotYetAvailable,
            )
            | CachedPipelineState::Queued
            | CachedPipelineState::Creating(_) => return,
            CachedPipelineState::Err(error) => {
                status.publish(Availability::Error(error.to_string()));
                return;
            }
        }
    }
    let (Some(catalogue), Some(arrivals), Some(requests)) = (
        buffers.get(&packet.catalogue),
        buffers.get(&packet.arrivals),
        buffers.get(&packet.requests),
    ) else {
        return;
    };
    let limits = device.limits();
    if [&catalogue.buffer, &arrivals.buffer, &requests.buffer]
        .iter()
        .any(|buffer| buffer.size() > limits.max_storage_buffer_binding_size)
        || arrivals.buffer.size() < stellar::ARRIVAL_BYTES
        || requests.buffer.size() < stellar::REQUEST_BYTES
        || stellar::STAR_GROUPS > limits.max_compute_workgroups_per_dimension
    {
        status.publish(Availability::Unsupported(
            "Stellar buffers exceed this device's limits".into(),
        ));
        return;
    }
    let uniform = prepared
        .stellar_uniform
        .get_or_insert_with(|| UniformBuffer::from(packet.settings.clone()));
    uniform.set(packet.settings.clone());
    uniform.write_buffer(device, queue);
    let bindings = device.create_bind_group(
        Some("Optical current-frame stellar packet"),
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &*uniform,
            catalogue.buffer.as_entire_buffer_binding(),
            arrivals.buffer.as_entire_buffer_binding(),
            terminals.buffer.as_entire_buffer_binding(),
            requests.buffer.as_entire_buffer_binding(),
        )),
    );
    prepared.stellar_packet = Some(PreparedStellar { bindings, control });
    status.publish(Availability::Ready { control });
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    pipeline: Res<Pipeline>,
    stellar_pipeline: Res<StellarPipeline>,
    prepared: Res<Prepared>,
    submission: Res<Submission>,
    mut last: ResMut<LastPrimary>,
) {
    // Any unavailable/withdrawn packet or pipeline invalidates reuse, even if
    // a later frame happens to contain identical inputs again.
    let previous = last.0.take();
    let (Some(packet), Some(current)) = (&prepared.packet, &submission.0) else {
        return;
    };
    if packet.control != current.settings.control {
        return;
    }
    let Some(pipeline) = pipeline.id.and_then(|id| cache.get_compute_pipeline(id)) else {
        return;
    };
    let key = packet
        .key
        .as_ref()
        .filter(|key| key.matches_pipeline(pipeline.id()));
    let reused = key.is_some() && key == previous.as_ref();
    if reused {
        // The previous payload precedes this copy in the GPU queue. Publish
        // only its current token, before stellar reads and the compositor.
        context.command_encoder().copy_buffer_to_buffer(
            &packet.header,
            0,
            &packet.terminals,
            0,
            16,
        );
    } else {
        let mut pass = context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("Current optical primary rays"),
                ..default()
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &packet.bindings, &[]);
        pass.dispatch_workgroups(
            packet.workgroups[0],
            packet.workgroups[1],
            packet.workgroups[2],
        );
    }
    last.0 = key.cloned();
    debug!(control = ?packet.control.to_array(), reused, "Optical primary payload encoded");
    let (Some(stellar), Some(ids)) = (&prepared.stellar_packet, stellar_pipeline.ids) else {
        return;
    };
    if stellar.control != packet.control {
        return;
    }
    let [Some(clear), Some(gather), Some(solve)] = ids.map(|id| cache.get_compute_pipeline(id))
    else {
        return;
    };
    for (label, pipeline, workgroups) in [
        (
            "Clear current stellar requests",
            clear,
            [stellar::STAR_GROUPS, 1, 1],
        ),
        ("Gather current stellar requests", gather, packet.workgroups),
        (
            "Solve current stellar requests",
            solve,
            [stellar::STAR_GROUPS, 1, 1],
        ),
    ] {
        let mut pass = context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some(label),
                ..default()
            });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &stellar.bindings, &[]);
        pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
    }
}
