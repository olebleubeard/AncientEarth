//! Original MIT. Shared diagnostic plumbing; no physical model or solver.
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

pub fn block<F: Future>(future: F) -> F::Output {
    struct Signal;
    impl Wake for Signal {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Signal));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

pub fn copy_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
) -> Vec<u8> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("untimed diagnostic readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit([encoder.finish()]);
    read(device, &staging)
}

pub fn validate_dispatch(device: &wgpu::Device, dispatch: [u32; 3]) {
    assert!(dispatch.iter().all(
        |&groups| groups > 0 && groups <= device.limits().max_compute_workgroups_per_dimension
    ));
}

/// Identical serial timestamp protocol to optical_normal_performance_probe:
/// unique pairs, completed compute/resolve, separate copy/readback submission.
pub fn timed_dispatch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    bind: &wgpu::BindGroup,
    dispatch: [u32; 3],
) -> (Vec<Value>, bool) {
    timed_dispatch_observed(device, queue, pipeline, bind, dispatch, |_| {}, |_| None)
}

/// The hooks run outside the timestamped compute pass. The before hook can
/// initialize output, and the after hook retains this dispatch's complete
/// output before a later dispatch can overwrite it. Neither hook changes the
/// serial timestamp protocol or its independently reviewed tick conversion.
pub fn timed_dispatch_observed(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    bind: &wgpu::BindGroup,
    dispatch: [u32; 3],
    mut before_iteration: impl FnMut(u32),
    mut after_iteration: impl FnMut(u32) -> Option<Value>,
) -> (Vec<Value>, bool) {
    validate_dispatch(device, dispatch);
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
        before_iteration(iteration);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("optical diagnostic dispatch"),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &queries,
                    beginning_of_pass_write_index: Some(iteration * 2),
                    end_of_pass_write_index: Some(iteration * 2 + 1),
                }),
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
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
        let raw = read(device, &times);
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
        let mut row = json!({"iteration":iteration,"warmup":iteration<3,"gpu_seconds":gpu_seconds,"raw_timestamps":stamps,"timestamp_period_ns":period,"submit_readback_wall_seconds":before.elapsed().as_secs_f64()});
        if let Some(capture) = after_iteration(iteration) {
            row["capture"] = capture;
        }
        rows.push(row);
    }
    (rows, timestamps_valid)
}
