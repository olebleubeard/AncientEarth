//! Original MIT. GPU dispatch/timestamp plumbing for a math-owned fixture.
//! Atmosphere-only isolation is not scene rendering or application FPS.
#[path = "optical_normal_performance_fixture.rs"]
mod fixture;
use serde_json::json;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;

fn block<F: Future>(future: F) -> F::Output {
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
fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
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
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut validation_only = false;
    let mut fast_bounds = false;
    let mut fast_metric = false;
    let mut projected_height_rhs = false;
    let mut selected = false;
    let mut selection_source = None;
    let mut group_size = None;
    let mut requested_name = None;
    let mut flags = args.iter().skip(1);
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--name" if requested_name.is_none() => {
                requested_name = Some(
                    flags
                        .next()
                        .expect("--name needs a diagnostic name")
                        .as_str(),
                );
            }
            "--validate-only" if !validation_only => validation_only = true,
            "--fast-bounds" if !fast_bounds => fast_bounds = true,
            "--fast-metric" if !fast_metric => fast_metric = true,
            "--projected-height-rhs" if !projected_height_rhs => projected_height_rhs = true,
            "--selected" if !selected => selected = true,
            "--group-size" if group_size.is_none() => {
                group_size = Some(
                    flags
                        .next()
                        .expect("--group-size needs a value")
                        .parse::<u32>()
                        .expect("Invalid workgroup size"),
                );
            }
            "--selection" if selection_source.is_none() => {
                selection_source = Some(
                    flags
                        .next()
                        .expect("--selection needs a fixture path")
                        .as_str(),
                );
            }
            _ => panic!("Unknown or repeated argument: {flag}"),
        }
    }
    assert!(
        selection_source.is_none() || (selected && fast_bounds),
        "--selection requires --selected --fast-bounds"
    );
    assert!(
        !fast_metric || fast_bounds,
        "--fast-metric requires --fast-bounds"
    );
    assert!(
        !projected_height_rhs || fast_metric,
        "--projected-height-rhs requires --fast-metric"
    );
    let name = requested_name.unwrap_or(if selected {
        "optical-height-selected"
    } else {
        "optical-normal-performance"
    });
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "Invalid diagnostic name"
    );
    let prefix = format!("artifacts/{name}");
    for suffix in [
        ".wgsl",
        "-uniform.bin",
        "-view.bin",
        "-readback.bin",
        ".json",
    ] {
        let path = format!("{prefix}{suffix}");
        assert!(
            validation_only || !std::path::Path::new(&path).exists(),
            "Preserve existing evidence: {path}"
        );
    }
    let fixture = if projected_height_rhs {
        if selected {
            fixture::build_selected_projected_from(
                selection_source
                    .unwrap_or("artifacts/optical-height-performance-first-tail-selection.json"),
            )
        } else {
            fixture::build_projected()
        }
    } else if fast_metric {
        if selected {
            fixture::build_selected_metric_fast_from(
                selection_source
                    .unwrap_or("artifacts/optical-height-performance-first-tail-selection.json"),
            )
        } else {
            fixture::build_metric_fast()
        }
    } else {
        match (selected, fast_bounds) {
            (true, true) => match selection_source {
                Some(source) => fixture::build_selected_fast_from(source),
                None => fixture::build_selected_fast(),
            },
            (true, false) => fixture::build_selected(),
            (false, true) => fixture::build_fast(),
            (false, false) => fixture::build(),
        }
    };
    let group_size = group_size.unwrap_or(32);
    let fixture = fixture::with_workgroup_size(fixture, group_size);
    assert!(fixture.width > 0 && fixture.height > 0);
    assert!(fixture.record_count > 0);
    assert!(fixture.record_words >= 12 && fixture.record_words.is_multiple_of(4));
    let record_bytes = usize::try_from(fixture.record_words)
        .unwrap()
        .checked_mul(4)
        .unwrap();
    assert_eq!(
        fixture.output_bytes,
        u64::from(fixture.record_count)
            .checked_mul(record_bytes as u64)
            .unwrap()
    );
    assert_eq!(fixture.sample_pixels.len(), fixture.sample_indices.len());
    if !validation_only {
        std::fs::write(format!("{prefix}.wgsl"), &fixture.shader).unwrap();
        std::fs::write(format!("{prefix}-uniform.bin"), &fixture.uniform).unwrap();
        std::fs::write(format!("{prefix}-view.bin"), &fixture.view).unwrap();
    }
    let parsed = wgpu::naga::front::wgsl::parse_str(&fixture.shader)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&fixture.shader)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&parsed)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&fixture.shader)));
    if validation_only {
        println!(
            "Normal performance shader passed offline Naga validation; no GPU or artifacts created."
        );
        return;
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = block(instance.request_adapter(&Default::default())).unwrap();
    assert!(
        adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        "GPU timestamps unavailable"
    );
    let (device, queue) = block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let period = queue.get_timestamp_period();
    assert!(period.is_finite() && period > 0.0);
    let compilation_start = Instant::now();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Normal-coordinate atmosphere isolation"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(fixture.shader)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("normal_performance"),
        compilation_options: Default::default(),
        cache: None,
    });
    let compilation_seconds = compilation_start.elapsed().as_secs_f64();
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.uniform,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let view = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.view,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: view.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
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
    assert!(fixture.dispatch.iter().all(
        |&groups| groups > 0 && groups <= device.limits().max_compute_workgroups_per_dimension
    ));
    for iteration in 0..8 {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(if selected {
                    "Selected atmosphere ray diagnostics"
                } else {
                    "Atmosphere-only native pixels"
                }),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &queries,
                    beginning_of_pass_write_index: Some(iteration * 2),
                    end_of_pass_write_index: Some(iteration * 2 + 1),
                }),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(
                fixture.dispatch[0],
                fixture.dispatch[1],
                fixture.dispatch[2],
            );
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
        // Preserve the reviewed serial-query method: completed compute/resolve,
        // then a separate readback-copy submission, with unique query slots.
        let mut copy = device.create_command_encoder(&Default::default());
        copy.copy_buffer_to_buffer(&resolve, 0, &times, 0, 16);
        queue.submit([copy.finish()]);
        let raw = read(&device, &times);
        let stamps: Vec<_> = raw
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect();
        timestamps_valid &= stamps[1] > stamps[0] && stamps[0] > previous_end;
        previous_end = stamps[1];
        // Same tick-to-second conversion as the independently reviewed q16serial harness.
        let gpu_seconds = stamps[1]
            .checked_sub(stamps[0])
            .map(|ticks| ticks as f64 * f64::from(period) * 1e-9);
        rows.push(json!({"iteration":iteration,"warmup":iteration<3,"gpu_seconds":gpu_seconds,"raw_timestamps":stamps,"timestamp_period_ns":period,"submit_readback_wall_seconds":before.elapsed().as_secs_f64()}));
        eprintln!("Completed iteration {iteration}");
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut copy = device.create_command_encoder(&Default::default());
    copy.copy_buffer_to_buffer(&output, 0, &readback, 0, fixture.output_bytes);
    queue.submit([copy.finish()]);
    let raw = read(&device, &readback);
    assert_eq!(raw.len() as u64, fixture.output_bytes);
    std::fs::write(format!("{prefix}-readback.bin"), &raw).unwrap();
    let mut statuses = BTreeMap::<u32, u64>::new();
    for record in raw.chunks_exact(record_bytes) {
        let status = u32::from_le_bytes(record[28..32].try_into().unwrap());
        *statuses.entry(status).or_default() += 1;
    }
    let samples: Vec<_> = fixture
        .sample_pixels
        .iter()
        .zip(&fixture.sample_indices)
        .map(|(&[x, y], &record_index)| {
            assert!(x < fixture.width && y < fixture.height);
            assert!(record_index < fixture.record_count);
            let offset = usize::try_from(record_index)
                .unwrap()
                .checked_mul(record_bytes)
                .unwrap();
            let words: Vec<_> = raw[offset..offset + record_bytes]
                .chunks_exact(4)
                .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            json!({"pixel":[x,y],"record_index":record_index,"raw_terminal_words":words})
        })
        .collect();
    let report = json!({
        "scope":if selected { "Selected actual-camera ray diagnostics. Only the explicitly selected records are dispatched. Timings do not represent a full frame, application FPS or production numerical acceptance." } else { "Atmosphere-only full-resolution GPU isolation. Scene contacts, radiance, stellar inverses, UI and presentation are absent. This is not application FPS or production numerical acceptance. Three warmups and all five subsequent samples retained; compilation and submit/readback wall durations are separate." },
        "adapter":format!("{:?}",adapter.get_info()),"dimensions":[fixture.width,fixture.height],"workgroup_size":group_size,
        "selected":selected,"fast_bounds":fast_bounds,"fast_metric":fast_metric,"projected_height_rhs":projected_height_rhs,"selection_source":selection_source,"group_size":group_size,"record_count":fixture.record_count,"record_words":fixture.record_words,"dispatch":fixture.dispatch,
        "pipeline_creation_seconds":compilation_seconds,"terminal_bytes":fixture.output_bytes,
        "iterations":rows,"strictly_increasing_unique_timestamp_pairs":timestamps_valid,
        "status_counts":statuses,"samples":samples,"fixture":fixture.cases,
    });
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        timestamps_valid,
        "Timestamp sequence invalid; all raw evidence retained"
    );
    println!("Wrote {prefix}.json; all dispatched raw records retained.");
}
