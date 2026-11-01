//! Original MIT. Mechanical dispatch sequencing for isolated diagnostics.
//! No ray equations, fixture coordinates, or stopping predicates are defined here.
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub struct Dispatch<'a> {
    pub pipeline: &'a wgpu::ComputePipeline,
    pub bind: &'a wgpu::BindGroup,
    pub groups: [u32; 3],
    pub repeats: u32,
}

pub fn encode(pass: &mut wgpu::ComputePass<'_>, steps: &[Dispatch<'_>]) {
    assert!(!steps.is_empty());
    for step in steps {
        assert!(step.repeats > 0);
        pass.set_pipeline(step.pipeline);
        pass.set_bind_group(0, step.bind, &[]);
        for _ in 0..step.repeats {
            pass.dispatch_workgroups(step.groups[0], step.groups[1], step.groups[2]);
        }
    }
}

// Same serial unique-query protocol and reviewed tick conversion as runtime::timed_dispatch.
// The timestamp pair encloses the complete ordered sequence, not just one slice.
pub fn timed_sequence(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    steps: &[Dispatch<'_>],
    clear_before_sequence: &[&wgpu::Buffer],
) -> (Vec<Value>, bool) {
    for step in steps {
        super::runtime::validate_dispatch(device, step.groups);
        assert!(step.repeats > 0);
    }
    let period = queue.get_timestamp_period();
    assert!(period.is_finite() && period > 0.0);
    let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: None,
        ty: wgpu::QueryType::Timestamp,
        count: 16,
    });
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let times = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut rows = Vec::new();
    let mut previous_end = 0_u64;
    let mut timestamps_valid = true;
    for iteration in 0..8 {
        let mut encoder = device.create_command_encoder(&Default::default());
        for buffer in clear_before_sequence {
            encoder.clear_buffer(buffer, 0, None);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("optical diagnostic dispatch"),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &queries,
                    beginning_of_pass_write_index: Some(iteration * 2),
                    end_of_pass_write_index: Some(iteration * 2 + 1),
                }),
            });
            encode(&mut pass, steps);
        }
        encoder.resolve_query_set(&queries, iteration * 2..iteration * 2 + 2, &resolve, 0);
        let before = Instant::now();
        let submitted = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: Some(Duration::from_secs(60)),
            })
            .unwrap();
        let mut copy = device.create_command_encoder(&Default::default());
        copy.copy_buffer_to_buffer(&resolve, 0, &times, 0, 16);
        queue.submit([copy.finish()]);
        let raw = super::runtime::read(device, &times);
        let stamps: Vec<_> = raw
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect();
        timestamps_valid &= stamps[1] > stamps[0] && stamps[0] > previous_end;
        previous_end = stamps[1];
        // Reuse the independently reviewed q16 serial tick conversion.
        let gpu_seconds = stamps[1]
            .checked_sub(stamps[0])
            .map(|ticks| ticks as f64 * f64::from(period) * 1e-9);
        rows.push(json!({"iteration":iteration,"warmup":iteration<3,"gpu_seconds":gpu_seconds,"raw_timestamps":stamps,"timestamp_period_ns":period,"submit_readback_wall_seconds":before.elapsed().as_secs_f64()}));
    }
    (rows, timestamps_valid)
}
