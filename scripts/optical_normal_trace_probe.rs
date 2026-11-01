//! Original MIT. Mechanical dispatch/readback for math-owned clear-ray fixtures.
//! This diagnostic does not modify the application or claim frame performance.
#[path = "optical_normal_trace_fixture.rs"]
mod fixture;
#[path = "optical_normal_map_fixture.rs"]
mod map_fixture;
use serde_json::json;
use std::{
    borrow::Cow,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
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

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut validation_only = false;
    let mut fast_bounds = false;
    let mut fast_metric = false;
    let mut capture = false;
    let mut map = false;
    let mut projected_height_rhs = false;
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
            "--capture" if !capture => capture = true,
            "--map" if !map => map = true,
            "--projected-height-rhs" if !projected_height_rhs => projected_height_rhs = true,
            _ => panic!("Unknown or repeated argument: {flag}"),
        }
    }
    assert!(
        !fast_metric || fast_bounds,
        "--fast-metric requires --fast-bounds"
    );
    assert!(
        !map || !(capture || fast_bounds || fast_metric || projected_height_rhs),
        "--map is a separate diagnostic"
    );
    assert!(
        !projected_height_rhs || fast_metric,
        "--projected-height-rhs requires --fast-metric"
    );
    let name = requested_name.unwrap_or(if capture {
        "optical-height-trials"
    } else {
        "optical-normal-trace"
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
        "-rays.bin",
        "-readback.bin",
        ".json",
    ] {
        let path = format!("{prefix}{suffix}");
        assert!(
            validation_only || !std::path::Path::new(&path).exists(),
            "Preserve existing evidence: {path}"
        );
    }
    assert!(
        !(fast_bounds && capture),
        "Fast bounds and trial capture are separate diagnostics"
    );
    let fixture = if map {
        let f = map_fixture::build();
        fixture::Fixture {
            shader: f.shader,
            uniform: f.uniform,
            rays: f.rays,
            cases: f.cases,
            count: f.count,
            record_words: f.record_words,
            output_bytes: f.output_bytes,
        }
    } else if projected_height_rhs {
        fixture::build_projected()
    } else if fast_metric {
        fixture::build_metric_fast()
    } else if fast_bounds {
        fixture::build_fast()
    } else if capture {
        fixture::build_capture()
    } else {
        fixture::build()
    };
    assert!(fixture.count > 0 && fixture.output_bytes > 0);
    let input_vectors = if map {
        usize::try_from(fixture.cases["input_vectors_per_case"].as_u64().unwrap()).unwrap()
    } else {
        1
    };
    assert_eq!(
        fixture.rays.len(),
        fixture.count as usize * input_vectors * 16
    );
    assert!(fixture.record_words > 0 && fixture.record_words.is_multiple_of(4));
    let record_bytes = u64::from(fixture.record_words).checked_mul(4).unwrap();
    assert_eq!(
        fixture.output_bytes,
        u64::from(fixture.count).checked_mul(record_bytes).unwrap()
    );
    if !validation_only {
        std::fs::write(format!("{prefix}.wgsl"), &fixture.shader).unwrap();
        std::fs::write(format!("{prefix}-uniform.bin"), &fixture.uniform).unwrap();
        std::fs::write(format!("{prefix}-rays.bin"), &fixture.rays).unwrap();
    }
    // Parse and validate without creating a GPU instance. This is a syntax and
    // type check, not proof of the equations or device-specific execution.
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
            "Normal-chart shader passed offline Naga validation; no GPU or output artifacts created."
        );
        return;
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = block(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Normal-coordinate complete clear-ray diagnostic"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(fixture.shader)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("normal_trace"),
        compilation_options: Default::default(),
        cache: None,
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.uniform,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let rays = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.rays,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
                resource: rays.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(fixture.count.div_ceil(32), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, fixture.output_bytes);
    let submitted = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    let raw = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    assert_eq!(raw.len() as u64, fixture.output_bytes);
    std::fs::write(format!("{prefix}-readback.bin"), &raw).unwrap();
    let records: Vec<Vec<u32>> = raw
        .chunks_exact(usize::try_from(record_bytes).unwrap())
        .map(|record| {
            record
                .chunks_exact(4)
                .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
                .collect()
        })
        .collect();
    let report = json!({
        "scope": if map {"Canonical map/velocity interval GPU readback. Every raw record is retained; no integration, contact, timing or application-acceptance claim."} else {"Complete clear-ray GPU readback of math-owned fixtures. Every raw record is retained; no scene-contact, timing or application-acceptance claim."},
        "adapter": format!("{:?}", adapter.get_info()),
        "count": fixture.count, "output_bytes": fixture.output_bytes,
        "record_words": fixture.record_words,
        "trial_capture": capture,
        "canonical_map": map,
        "projected_height_rhs": projected_height_rhs,
        "fast_bounds": fast_bounds || map,
        "fast_metric": fast_metric || map,
        "fixture": fixture.cases, "gpu_raw_records": records,
    });
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("Wrote {prefix}.json; all raw records retained.");
}
