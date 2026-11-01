//! Optional QA evidence of completed compute packets. This does not acknowledge
//! fragment consumption, display presentation, or GPU execution duration.
use super::compute::Submission;
use bevy::{
    prelude::*,
    render::gpu_readback::{ReadbackComplete, ReadbackOnce},
};
use serde_json::{Value, json};

#[derive(Resource, Default)]
pub(crate) struct OpticalReadback {
    requests: Vec<RequestResult>,
    stellar_requests: Vec<RequestResult>,
}

struct RequestResult {
    requested: [u32; 4],
    returned: Option<[u32; 4]>,
    error: Option<String>,
}

impl OpticalReadback {
    pub fn report(&self) -> Value {
        json!({
            "scope": "Optional asynchronous 16-byte compute-header readback. Snapshot of all requests present at the capture completion boundary, including warmup. Only exact requested/returned token and extent equality demonstrates completion of that requested packet. Missing responses and mismatches are retained. This is not consumer acknowledgement, hardware presentation, or a GPU duration measurement.",
            "requests": self.requests.iter().map(RequestResult::report).collect::<Vec<_>>(),
            "stellar_requests": self.stellar_requests.iter().map(RequestResult::report).collect::<Vec<_>>(),
        })
    }
}

impl RequestResult {
    fn report(&self) -> Value {
        json!({
                "requested_extent_and_token": self.requested,
                "returned_extent_and_token": self.returned,
                "exact_match": self.returned.map(|returned| returned == self.requested),
                "error": self.error,
        })
    }
}

#[derive(Component)]
struct RequestIndex {
    stellar: bool,
    index: usize,
}

pub(super) fn install(app: &mut App) {
    // Restrict this diagnostic to bounded frame captures; ordinary rendering
    // never allocates staging buffers or maps GPU data back to the CPU.
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--optical-header-readback")
        && args.iter().any(|arg| arg == "--frame-capture")
    {
        app.init_resource::<OpticalReadback>()
            .add_systems(Last, request);
    }
}

fn request(
    mut commands: Commands,
    submission: Res<Submission>,
    mut audit: ResMut<OpticalReadback>,
) {
    let Some(packet) = &submission.0 else { return };
    let index = audit.requests.len();
    audit.requests.push(RequestResult {
        requested: packet.settings.control.to_array(),
        returned: None,
        error: None,
    });
    commands
        .spawn((
            ReadbackOnce::buffer_range(packet.terminals.clone(), 0, 16),
            RequestIndex {
                stellar: false,
                index,
            },
        ))
        .observe(complete);
    if let Some(stellar) = &packet.stellar {
        let index = audit.stellar_requests.len();
        audit.stellar_requests.push(RequestResult {
            requested: stellar.settings.control.to_array(),
            returned: None,
            error: None,
        });
        commands
            .spawn((
                ReadbackOnce::buffer_range(stellar.arrivals.clone(), 0, 16),
                RequestIndex {
                    stellar: true,
                    index,
                },
            ))
            .observe(complete);
    }
}

fn complete(
    event: On<ReadbackComplete>,
    requests: Query<&RequestIndex>,
    mut audit: ResMut<OpticalReadback>,
    mut commands: Commands,
) {
    if let Ok(index) = requests.get(event.entity) {
        let samples = if index.stellar {
            &mut audit.stellar_requests
        } else {
            &mut audit.requests
        };
        if let Some(sample) = samples.get_mut(index.index) {
            if event.data.len() == 16 {
                sample.returned = Some(event.to_shader_type::<UVec4>().to_array());
            } else {
                sample.error = Some(format!(
                    "Expected 16 header bytes, received {}",
                    event.data.len()
                ));
            }
        }
    }
    commands.entity(event.entity).despawn();
}
