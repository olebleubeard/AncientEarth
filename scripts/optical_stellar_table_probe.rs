//! Original MIT. Isolated all-catalogue arrival-table cost diagnostic.
//! This does not replace fragment shading or claim native-frame equivalence.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
use earth_math::{atmosphere::gpu::GpuEllipsoid, rendering::stars};
use serde_json::json;
use std::{borrow::Cow, time::Duration};
use wgpu::util::DeviceExt;

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
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

fn main() {
    assert!(std::env::args().any(|a| a == "--atmosphere-only"));
    let prefix = "artifacts/optical-stellar-table-probe";
    assert!(
        !std::path::Path::new(&format!("{prefix}.json")).exists(),
        "Preserve previous evidence first"
    );
    let case = source::actual_scene();
    let g = GpuEllipsoid::at(case.observer);
    let layers = case.profile.layers();
    let gradient = case.profile.maximum_index_gradient();
    let mut settings = vec![
        g.rows[0],
        g.rows[1],
        g.rows[2],
        g.origin,
        g.center,
        [
            layers.len() as f32,
            case.profile.top_height_m() as f32,
            (gradient
                + 1.0
                    / (earth_math::WGS84_A_M * (1.0 - earth_math::WGS84_E2)
                        + earth_math::MIN_HEIGHT_M)) as f32,
            1.0,
        ],
        [
            gradient as f32,
            case.profile.base_height_m() as f32,
            earth_math::MIN_HEIGHT_M as f32,
            0.0,
        ],
    ];
    for i in 0..128 {
        settings.push(if let Some(l) = layers.get(i) {
            let slope = layers.get(i + 1).map_or(0.0, |n| {
                (n.refractivity - l.refractivity) / (n.height_m - l.height_m)
            });
            [
                (case.profile.base_height_m() + l.height_m) as f32,
                l.refractivity as f32,
                slope as f32,
                0.0,
            ]
        } else {
            [0.0; 4]
        });
    }
    settings.push(g.latitude);
    // Explicit identity catalogue-to-local rotation isolates the cost over all
    // horizon elevations. It is not a claimed dated astronomical observation.
    settings.extend([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]);
    let control = [1920u32, 1080, 0x12345678, 0x9abcdef0];
    settings.push(control.map(f32::from_bits));
    let catalogue = stars::packed_catalogue();
    assert_eq!(stars::STAR_COUNT, 3000, "Shader dispatch literal changed");
    assert_eq!(stars::STAR_GRID, 32, "Shader catalogue offset literal changed");
    let header_count = 6 * stars::STAR_GRID * stars::STAR_GRID;
    assert_eq!(catalogue.len(), header_count + 2 * stars::STAR_COUNT);
    let mut ids = vec![false; stars::STAR_COUNT];
    for pair in catalogue[header_count..].chunks_exact(2) {
        let id = pair[1][3] as usize;
        assert_eq!(id as f32, pair[1][3]);
        assert!(id < ids.len() && !ids[id]);
        ids[id] = true;
    }
    assert!(ids.iter().all(|v| *v));
    let mut shader = source::shader();
    shader.truncate(shader.find("@compute").unwrap());
    shader = shader.replace(
        "latitude:vec4<f32>}",
        "latitude:vec4<f32>,icrf0:vec4<f32>,icrf1:vec4<f32>,icrf2:vec4<f32>,control:vec4<u32>}",
    );
    shader = shader.replace("output:array<vec4<f32>>", "output:array<vec4<u32>>");
    let production = std::fs::read_to_string("src/desktop/shaders/refraction.wesl").unwrap();
    let begin = production.find("fn inverse_star(").unwrap();
    let end = production[begin..]
        .find("// Stellar inverse shooting")
        .unwrap()
        + begin;
    shader.push_str(&production[begin..end]);
    shader.push_str(r#"
@compute @workgroup_size(32)
fn stellar_table(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=3000u{return;}
    if id.x==0u{output[0]=settings.control;}
    let star=rays[6144u+2u*id.x];let tint=rays[6144u+2u*id.x+1u];
    let normal=normalize(star.xyz);
    let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
    let arrival=inverse_star(stellar_direction);
    output[1u+u32(tint.w+0.5)]=bitcast<vec4<u32>>(arrival);
}
"#);
    std::fs::write(format!("{prefix}.wgsl"), &shader).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = source::block(instance.request_adapter(&Default::default())).unwrap();
    assert!(adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY));
    let (device, queue) = source::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("isolated all-star arrivals"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
    });
    let started = std::time::Instant::now();
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("stellar_table"),
        compilation_options: Default::default(),
        cache: None,
    });
    let compilation_seconds = started.elapsed().as_secs_f64();
    eprintln!("Stellar pipeline ready: {compilation_seconds}s");
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &source::bytes(&settings),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &source::bytes(&catalogue),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = (stars::STAR_COUNT as u64 + 1) * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[&uniform, &input, &output]
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect::<Vec<_>>(),
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
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut samples = Vec::new();
    let mut previous = 0;
    for i in 0..8 {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &queries,
                    beginning_of_pass_write_index: Some(2 * i),
                    end_of_pass_write_index: Some(2 * i + 1),
                }),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((stars::STAR_COUNT as u32).div_ceil(32), 1, 1);
        }
        encoder.resolve_query_set(&queries, 2 * i..2 * i + 2, &resolve, 0);
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(60)),
            })
            .unwrap();
        let mut copy = device.create_command_encoder(&Default::default());
        copy.copy_buffer_to_buffer(&resolve, 0, &times, 0, 16);
        queue.submit([copy.finish()]);
        let raw = read(&device, &times);
        let ticks: Vec<_> = raw
            .chunks_exact(8)
            .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
            .collect();
        assert!(
            ticks[1] > ticks[0] && ticks[0] > previous,
            "Repeated or nonmonotonic query results"
        );
        previous = ticks[1];
        let seconds = (ticks[1] - ticks[0]) as f64 * f64::from(queue.get_timestamp_period()) * 1e-9;
        samples.push(json!({"iteration":i,"warmup":i<3,"raw_timestamps":ticks,"timestamp_period_ns":queue.get_timestamp_period(),"gpu_seconds":seconds}));
        eprintln!("stellar iteration {i}: {seconds}s");
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, size);
    queue.submit([encoder.finish()]);
    let bytes = read(&device, &staging);
    let words: Vec<_> = bytes
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .collect();
    assert_eq!(&words[..4], &control);
    let arrivals: Vec<_> = words[4..]
        .chunks_exact(4)
        .enumerate()
        .map(|(id, w)| {
            let arrival = w.iter().map(|v| f32::from_bits(*v)).collect::<Vec<_>>();
            assert!(arrival.iter().all(|v| v.is_finite()));
            assert!(arrival[3] == 0.0 || arrival[3] == 1.0);
            json!({"stable_id":id,"raw_words":w,"arrival":arrival})
        })
        .collect();
    let resolved = arrivals
        .iter()
        .filter(|a| a["arrival"][3].as_f64() == Some(1.0))
        .count();
    let mut timed: Vec<_> = samples[3..]
        .iter()
        .map(|s| s["gpu_seconds"].as_f64().unwrap())
        .collect();
    timed.sort_by(f64::total_cmp);
    let report = json!({"scope":"Isolated same-frame all-3000 catalogue inverse-arrival cost with production midpoint solver and paired NOAA coastal column. Identity catalogue-to-local rotation is a declared test fixture, not a dated sky. No fragment ownership/radiance/context parity or native FPS claim. Failed arrivals remain w=0. This measures all-star work, not savings over sparse per-pixel requests.","adapter":format!("{:?}",adapter.get_info()),"catalogue_count":stars::STAR_COUNT,"output_bytes":size,"control":control,"resolved":resolved,"unresolved":stars::STAR_COUNT-resolved,"compilation_seconds":compilation_seconds,"timestamp_samples":samples,"median_gpu_seconds":timed[2],"arrivals":arrivals});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}
