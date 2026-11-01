//! Original MIT. Isolated unguarded scalar evaluator cost and exact GPU-query audit.
//! This is not an integrator, a field acceptance policy, or native frame timing.
use earth_math::{Geodetic, Observer, atmosphere::gpu::GpuEllipsoid};
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
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut cx = Context::from_waker(&waker);
    let mut f = Box::pin(future);
    loop {
        if let Poll::Ready(value) = f.as_mut().poll(&mut cx) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|v| v.iter().flat_map(|x| x.to_le_bytes()))
        .collect()
}
fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    let out = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    out
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let argument = |name: &str, default: usize| {
        args.iter()
            .position(|a| a == name)
            .map_or(default, |i| args[i + 1].parse::<usize>().unwrap())
    };
    let scalar_updates = argument("--updates", 2);
    let inner = argument("--inner", 1);
    assert!(scalar_updates <= 2 && (1..=64).contains(&inner));
    let prefix = if args.len() == 1 {
        "artifacts/optical-scalar-query-probe".to_owned()
    } else {
        format!("artifacts/optical-scalar-query-u{scalar_updates}-i{inner}")
    };
    assert!(
        !std::path::Path::new(&format!("{prefix}.json")).exists(),
        "Preserve existing evidence before replay"
    );
    let mut inputs = Vec::new();
    let mut cases = Vec::new();
    for lat in [-90.0, -67.5, -12.0, 0.0, 29.305549, 62.0, 89.999, 90.0] {
        for lon in [-179.99, -94.767015, 0.0, 90.0] {
            let packet = GpuEllipsoid::at(Observer::new(Geodetic::new(lat, lon, -27.7).unwrap()));
            for p in [
                [0.0, 0.0, 0.0],
                [30.0, 0.0, -60.0],
                [3000.0, 1000.0, -2000.0],
                [-60000.0, 20000.0, 65000.0],
                [100000.0, 80000.0, -40000.0],
                [500000.0, 100000.0, -1000000.0],
                [-1700000.0, 0.0, -1000000.0],
            ] {
                inputs.extend([
                    packet.rows[0],
                    packet.rows[1],
                    packet.rows[2],
                    packet.origin,
                    packet.latitude,
                    [p[0], p[1], p[2], 0.0],
                ]);
                cases.push(json!({"latitude":lat,"longitude":lon,"position":p,"matrix_rows":packet.rows,"origin":packet.origin,"latitude_packet":packet.latitude}));
            }
        }
    }
    assert_eq!(cases.len(), 224);
    assert_eq!(inputs.len(), 224 * 6);
    let production = std::fs::read_to_string("src/desktop/shaders/refraction.wesl")
        .unwrap()
        .replace("\r\n", "\n");
    let start = production.find("fn geometry(").unwrap();
    let end = production.find("fn index_gradient(").unwrap();
    let old = &production[start..end];
    let legacy = old
        .replace(
            "fn geometry(p:vec3<f32>)->vec4<f32>{",
            "fn query_legacy(q:Query)->vec4<f32>{let p=q.position.xyz;",
        )
        .replace("qmul(p)", "query_mul(q,p)")
        .replace("qmul(normal)", "query_mul(q,normal)")
        .replace("settings.origin", "q.origin");
    assert!(!legacy.contains("settings") && !legacy.contains("qmul"));
    let mut shader = std::fs::read_to_string("scripts/optical_scalar_query.wgsl").unwrap();
    assert_eq!(shader.matches("i<2;i+=1").count(), 1);
    shader = shader.replace("i<2;i+=1", &format!("i<{scalar_updates};i+=1"));
    if inner > 1 {
        for (method, value) in [
            ("legacy", "query_legacy(queries[(id.x+17u*iteration)%224u])"),
            (
                "candidate",
                "scalar_query(queries[(id.x+17u*iteration)%224u]).value",
            ),
        ] {
            let old = if method == "legacy" {
                "results[id.x]=bitcast<vec4<u32>>(query_legacy(queries[id.x%224u]));"
            } else {
                "results[id.x]=bitcast<vec4<u32>>(scalar_query(queries[id.x%224u]).value);"
            };
            assert_eq!(shader.matches(old).count(), 1);
            shader=shader.replace(old,&format!("var sum=vec4(0.0);for(var iteration=0u;iteration<{inner}u;iteration+=1u){{sum+={value};}}results[id.x]=bitcast<vec4<u32>>(sum);"));
        }
    }
    shader = shader.replace(
        "let alpha=1.0/6378137.0;let beta=6378137.0/(6356752.314245179*6356752.314245179);",
        &format!(
            "let alpha={:.9e};let beta={:.9e};",
            (1.0 / earth_math::WGS84_A_M) as f32,
            (earth_math::WGS84_A_M / earth_math::WGS84_B_M.powi(2)) as f32
        ),
    );
    shader.push_str(&legacy);
    std::fs::write(format!("{prefix}.wgsl"), &shader).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = block(instance.request_adapter(&Default::default())).unwrap();
    assert!(adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY));
    let (device, queue) = block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scalar geometry audit"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = 917504u64 * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: None,
        ty: wgpu::QueryType::Timestamp,
        count: 32,
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
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut methods = Vec::new();
    let mut cost_outputs: Vec<Vec<u32>> = Vec::new();
    let mut previous = 0u64;
    for (method, entry) in ["legacy_cost", "candidate_cost", "query_details"]
        .iter()
        .enumerate()
    {
        let started = std::time::Instant::now();
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        });
        let compilation_seconds = started.elapsed().as_secs_f64();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut samples = Vec::new();
        for iteration in 0..if method < 2 { 8 } else { 1 } {
            let mut encoder = device.create_command_encoder(&Default::default());
            let query_index = (method as u32 * 8 + iteration) * 2;
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: if method < 2 {
                        Some(wgpu::ComputePassTimestampWrites {
                            query_set: &queries,
                            beginning_of_pass_write_index: Some(query_index),
                            end_of_pass_write_index: Some(query_index + 1),
                        })
                    } else {
                        None
                    },
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(if method < 2 { 917504 / 32 } else { 224 / 32 }, 1, 1);
            }
            if method < 2 {
                encoder.resolve_query_set(&queries, query_index..query_index + 2, &resolve, 0);
            }
            let submission = queue.submit([encoder.finish()]);
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(Duration::from_secs(60)),
                })
                .unwrap();
            if method < 2 {
                let mut copy = device.create_command_encoder(&Default::default());
                copy.copy_buffer_to_buffer(&resolve, 0, &times, 0, 16);
                queue.submit([copy.finish()]);
                let raw = read(&device, &times);
                let ticks = raw
                    .chunks_exact(8)
                    .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
                    .collect::<Vec<_>>();
                assert!(ticks[1] > ticks[0] && ticks[0] > previous);
                previous = ticks[1];
                let seconds =
                    (ticks[1] - ticks[0]) as f64 * f64::from(queue.get_timestamp_period()) * 1e-9;
                samples.push(json!({"iteration":iteration,"warmup":iteration<3,"raw_timestamps":ticks,"timestamp_period_ns":queue.get_timestamp_period(),"gpu_seconds":seconds}));
                eprintln!("{entry} {iteration}: {seconds}s");
            }
        }
        if method < 2 {
            let mut timed = samples[3..]
                .iter()
                .map(|x| x["gpu_seconds"].as_f64().unwrap())
                .collect::<Vec<_>>();
            timed.sort_by(f64::total_cmp);
            methods.push(json!({"entry":entry,"compilation_seconds":compilation_seconds,"timestamp_samples":samples,"median_gpu_seconds":timed[2]}));
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 224 * 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut copy = device.create_command_encoder(&Default::default());
            copy.copy_buffer_to_buffer(&output, 0, &staging, 0, 224 * 16);
            queue.submit([copy.finish()]);
            let raw = read(&device, &staging);
            std::fs::write(format!("{prefix}-{entry}.bin"), &raw).unwrap();
            cost_outputs.push(
                raw.chunks_exact(4)
                    .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                    .collect(),
            );
        }
    }
    let detail_size = 224 * 7 * 16;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: detail_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut copy = device.create_command_encoder(&Default::default());
    copy.copy_buffer_to_buffer(&output, 0, &staging, 0, detail_size);
    queue.submit([copy.finish()]);
    let raw = read(&device, &staging);
    std::fs::write(format!("{prefix}-details.bin"), &raw).unwrap();
    let words = raw
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .collect::<Vec<_>>();
    let mut context_differences = Vec::new();
    for (index, (case, record)) in cases.iter_mut().zip(words.chunks_exact(28)).enumerate() {
        let f = record
            .iter()
            .map(|x| f32::from_bits(*x))
            .collect::<Vec<_>>();
        case["gpu_raw_words"] = json!(record);
        case["candidate"] = json!(&f[0..4]);
        case["input_normal_residual"] = json!(&f[4..8]);
        case["gpu_q_f"] = json!(&f[8..12]);
        case["gpu_qn_v"] = json!(&f[12..16]);
        case["coefficient_change_valid"] = json!(&f[16..20]);
        case["gradient_normal_squared"] = json!(&f[20..24]);
        case["legacy"] = json!(&f[24..28]);
        case["legacy_cost_words"] = json!(&cost_outputs[0][index * 4..index * 4 + 4]);
        case["candidate_cost_words"] = json!(&cost_outputs[1][index * 4..index * 4 + 4]);
        for (method, offset) in [(0, 24), (1, 0)] {
            if inner != 1 {
                continue;
            }
            if cost_outputs[method][index * 4..index * 4 + 4] != record[offset..offset + 4] {
                context_differences.push(json!({"case":index,"method":method,"cost_words":&cost_outputs[method][index*4..index*4+4],"detail_words":&record[offset..offset+4]}));
            }
        }
        assert!(
            f.iter().all(|x| x.is_finite()),
            "Retain failed nonfinite record before acceptance"
        );
    }
    let report = json!({"scope":"Unregistered isolated coordinate-evaluator diagnostic.224actual uploaded-Q queries repeated over917504invocations. Inner>1 evaluates a cyclic sequence of queries and writes a running sum to reduce output-memory dominance; it is not a physical trajectory. Timed candidate excludes finite acceptance enclosure,fallback and DP field-error propagation. Detailed outputs preserve actual rounded GPU intermediates for independent audit. Cost/detail context comparison applies only to inner=1; inner>1 retains summed outputs separately. No integrator or native FPS claim.","adapter":format!("{:?}",adapter.get_info()),"query_count":224,"cost_invocations":917504,"scalar_seed_updates":scalar_updates,"inner_evaluations":inner,"methods":methods,"cost_detail_context_comparison_applicable":inner==1,"cost_detail_context_differences":context_differences,"cases":cases});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}
