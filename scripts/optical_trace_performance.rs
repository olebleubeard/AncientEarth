//! Original MIT. Headless primary-trace GPU timing, not application FPS.
//! Uses unchanged production functions, source scene, and physical pixel rays.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
#[path = "optical_shader_variants.rs"]
mod variants;
use earth_math::{
    atmosphere::gpu::{GpuEllipsoid, scene::PackedScene},
    rendering::LocalFrame,
    *,
};
use std::{borrow::Cow, time::Instant};
use variants::diagnostic_variant;
use wgpu::util::DeviceExt;

fn argument(name: &str, default: u32) -> u32 {
    let args: Vec<_> = std::env::args().collect();
    args.iter()
        .position(|x| x == name)
        .map_or(default, |i| args[i + 1].parse().unwrap())
}
fn variant() -> String {
    let args: Vec<_> = std::env::args().collect();
    args.iter()
        .position(|x| x == "--variant")
        .map_or_else(|| "full".to_string(), |i| args[i + 1].clone())
}
fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    loop {
        device.poll(wgpu::PollType::Poll).unwrap();
        if let Ok(r) = rx.try_recv() {
            r.unwrap();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let result = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    result
}
fn read_output(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    output: &wgpu::Buffer,
    size: u64,
) -> Vec<u8> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("untimed full-frame validation"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(output, 0, &staging, 0, size);
    queue.submit([encoder.finish()]);
    read(device, &staging)
}
fn digest(bytes: &[u8]) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}
fn main() {
    let width = argument("--width", 1920);
    let height = argument("--height", 1080);
    let group = argument("--group", 64);
    let variant = variant();
    let compare = std::env::args().any(|a| a == "--compare-full");
    let upper_sky=std::env::args().any(|a|a=="--upper-sky");
    let args:Vec<_>=std::env::args().collect();
    let comparison_variant=args.iter().position(|a|a=="--compare-variant").map_or("full",|i|args[i+1].as_str());
    assert!(!compare || variant.starts_with("cached") || variant.starts_with("dopri") || variant.contains("corners"));
    assert!(
        [
            "full",
            "air",
            "cached",
            "cached-layer",
            "dopri-dense",
            "dopri-dense-air",
            "dopri-dense-scene-stub-air",
            "dopri-auxiliary",
            "dopri-auxiliary-newton",
            "dopri-hermite",
            "dopri-carried",
            "dopri-height",
            "dopri-hermite-corners",
            "dopri-hermite-air",
            "full-corners"
        ]
        .contains(&variant.as_str())
    );
    assert!(width >= 16 && height >= 16 && u64::from(width) * u64::from(height) <= 2_073_600);
    assert!([32, 64, 128].contains(&group));
    let case = source::actual_scene();
    let local = LocalFrame::at(case.observer);
    let packed = PackedScene::prepare(&case.scene).unwrap();
    let frames = packed.frame_data(local, &case.markers).unwrap();
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
            (gradient + 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M)) as f32,
            1.0,
        ],
        [
            gradient as f32,
            case.profile.base_height_m() as f32,
            MIN_HEIGHT_M as f32,
            0.0,
        ],
    ];
    for i in 0..128 {
        settings.push(if i < layers.len() {
            let l = layers[i];
            let slope = layers.get(i + 1).map_or(0.0, |n| {
                (n.refractivity - l.refractivity) / (n.height_m - l.height_m)
            });
            [
                (l.height_m + case.profile.base_height_m()) as f32,
                l.refractivity as f32,
                slope as f32,
                0.0,
            ]
        } else {
            [0.0; 4]
        });
    }
    settings.push(g.latitude);
    let demo = earth_math::survey::nearby_demo(&case.scene).unwrap();
    let frame = case.observer.frame();
    let vector = |v| {
        let p = local.direction(v);
        [p[0], p[1], p[2], 0.0]
    };
    let view = vec![
        vector(frame.right),
        vector(frame.up),
        vector(frame.forward),
        [
            width as f32 / height as f32,
            (demo.lens.vertical_fov_deg.to_radians() * 0.5) as f32,
            u8::from(demo.lens.projection == Projection::EquidistantFisheye) as f32,
            0.0,
        ],
        [width as f32, height as f32, 0.0, 0.0],
    ];
    let original = source::shader();
    let prefix = &original[..original.find("@compute @workgroup_size").unwrap()];
    let prefix = prefix.replace(
        "var<storage,read_write> output:array<vec4<f32>>;",
        "var<storage,read_write> output:array<vec4<u32>>;",
    );
    let baseline_prefix = diagnostic_variant(prefix.clone(),comparison_variant);
    let prefix = diagnostic_variant(prefix, &variant);
    let shader=format!("{prefix}\n@compute @workgroup_size({group}) fn performance(@builtin(global_invocation_id) id:vec3<u32>){{
        let dimensions=vec2<u32>(rays[4].xy);if id.x>=dimensions.x*dimensions.y{{return;}}
        let pixel=vec2(id.x%dimensions.x,id.x/dimensions.x);let uv=(vec2<f32>(pixel)+vec2(0.5))/vec2<f32>(dimensions);
        let lens=rays[3];let xy=vec2((2.0*uv.x-1.0)*lens.x,1.0-2.0*uv.y);
        var optical=normalize(vec3(xy*tan(lens.y),1.0));
        if lens.z>0.5{{let radius=length(xy);if radius>1.0{{output[id.x*3u]=vec4(0u);output[id.x*3u+1u]=vec4(0u,0u,0u,3u);output[id.x*3u+2u]=vec4(0u);return;}}let theta=radius*lens.y;let scale=select(lens.y,sin(theta)/max(radius,1e-8),radius>1e-8);optical=vec3(xy*scale,cos(theta));}}
        let direction=normalize(rays[0].xyz*optical.x+rays[1].xyz*optical.y+rays[2].xyz*optical.z);let r=trace(direction);
        output[id.x*3u]=bitcast<vec4<u32>>(vec4(r.p,r.path));
        output[id.x*3u+1u]=vec4(bitcast<vec3<u32>>(r.u),r.status);
        output[id.x*3u+2u]=vec4(bitcast<vec3<u32>>(r.normal),(r.material<<24u)|r.marker);
    }}");
    let shader=if upper_sky{shader.replace("let uv=(vec2<f32>(pixel)","if pixel.y>=dimensions.y/2u || (pixel.x>=dimensions.x/2u-150u && pixel.x<dimensions.x/2u+150u){output[id.x*3u]=vec4(0u);output[id.x*3u+1u]=vec4(0u,0u,0u,3u);output[id.x*3u+2u]=vec4(0u);return;}let uv=(vec2<f32>(pixel)")}else{shader};
    let mut suffix = if variant == "full" {
        "-q16serial".to_string()
    } else {
        format!("-{variant}-q16serial")
    };
    if upper_sky{suffix.push_str("-upper-sky");}
    if comparison_variant!="full"{suffix.push_str(&format!("-vs-{comparison_variant}"));}
    let baseline_shader = shader.replacen(&prefix, &baseline_prefix, 1);
    let shader_path =
        format!("artifacts/optical-trace-performance-{width}x{height}-g{group}{suffix}.wgsl");
    std::fs::write(&shader_path, &shader).unwrap();
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
        label: Some("unchanged primary trace performance"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
    });
    let started = Instant::now();
    eprintln!("creating primary trace pipeline group{group}");
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("performance"),
        compilation_options: Default::default(),
        cache: None,
    });
    let compilation_seconds = started.elapsed().as_secs_f64();
    eprintln!("pipeline ready {compilation_seconds}s");
    let make = |data: &[[f32; 4]], uniform| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &source::bytes(data),
            usage: if uniform {
                wgpu::BufferUsages::UNIFORM
            } else {
                wgpu::BufferUsages::STORAGE
            },
        })
    };
    let settings = make(&settings, true);
    let view = make(&view, false);
    let geometry = make(&packed.geometry, false);
    let frame_buffer = make(&frames, false);
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lossless terminal states"),
        size: u64::from(width) * u64::from(height) * 48,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let all_buffers = [&settings, &view, &output, &geometry, &frame_buffer];
    let buffers = &all_buffers[..if variant == "air" || variant.ends_with("-air") {
        3
    } else {
        5
    }];
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &buffers
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
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut rows = Vec::new();
    let mut previous_end = 0u64;
    let mut timestamps_valid = true;
    for iteration in 0..8 {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("native resolution primary rays"),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &queries,
                    beginning_of_pass_write_index: Some(iteration * 2),
                    end_of_pass_write_index: Some(iteration * 2 + 1),
                }),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((width * height).div_ceil(group), 1, 1);
        }
        encoder.resolve_query_set(&queries, iteration * 2..iteration * 2 + 2, &resolve, 0);
        let before = Instant::now();
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        // Resolve completion and readback copy are separate submissions. This
        // avoids counting stale query-resolve data seen on this Vulkan driver.
        let mut copy = device.create_command_encoder(&Default::default());
        copy.copy_buffer_to_buffer(&resolve, 0, &times, 0, 16);
        queue.submit([copy.finish()]);
        let bytes = read(&device, &times);
        let timestamps: Vec<_> = bytes
            .chunks_exact(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        timestamps_valid &= timestamps[1] > timestamps[0] && timestamps[0] > previous_end;
        previous_end = timestamps[1];
        let gpu_seconds =
            (timestamps[1] - timestamps[0]) as f64 * f64::from(queue.get_timestamp_period()) * 1e-9;
        rows.push(serde_json::json!({"iteration":iteration,"warmup":iteration<3,"gpu_seconds":gpu_seconds,"raw_timestamps":timestamps,"timestamp_period_ns":queue.get_timestamp_period(),"submit_readback_wall_seconds":before.elapsed().as_secs_f64()}));
        eprintln!("iteration{iteration}: GPU{gpu_seconds}s");
    }
    let pixels: Vec<_> = [
        (width / 6, height / 6),
        (width / 2, height / 6),
        (width * 5 / 6, height / 6),
        (width / 6, height / 2),
        (width / 2, height / 2),
        (width * 5 / 6, height / 2),
        (width / 6, height * 5 / 6),
        (width / 2, height * 5 / 6),
        (width * 5 / 6, height * 5 / 6),
    ]
    .into_iter()
    .collect();
    let sample = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 48 * 9,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    for (i, &(x, y)) in pixels.iter().enumerate() {
        encoder.copy_buffer_to_buffer(
            &output,
            u64::from(y * width + x) * 48,
            &sample,
            i as u64 * 48,
            48,
        );
    }
    queue.submit([encoder.finish()]);
    let bytes = read(&device, &sample);
    let samples:Vec<_>=pixels.iter().enumerate().map(|(i,&(x,y))|{let words:Vec<_>=bytes[i*48..(i+1)*48].chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect();serde_json::json!({"pixel":[x,y],"position":words[..3].iter().map(|&v|f32::from_bits(v)).collect::<Vec<_>>(),"path_m":f32::from_bits(words[3]),"direction":words[4..7].iter().map(|&v|f32::from_bits(v)).collect::<Vec<_>>(),"normal":words[8..11].iter().map(|&v|f32::from_bits(v)).collect::<Vec<_>>(),"status":words[7],"material":words[11]>>24,"owner":words[11]&0xffffff,"raw_terminal_words":words})}).collect();
    let comparison = if compare {
        let size = u64::from(width) * u64::from(height) * 48;
        let candidate = read_output(&device, &queue, &output, size);
        let baseline_path = format!(
            "artifacts/optical-trace-performance-{width}x{height}-g{group}{suffix}-comparison-baseline.wgsl"
        );
        std::fs::write(&baseline_path, &baseline_shader).unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("unchanged baseline for exact comparison"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(baseline_shader)),
        });
        let baseline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("performance"),
            compilation_options: Default::default(),
            cache: None,
        });
        let baseline_buffers=&all_buffers[..if comparison_variant=="air" || comparison_variant.ends_with("-air"){3}else{5}];
        let baseline_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &baseline.get_bind_group_layout(0),
            entries: &baseline_buffers
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&baseline);
            pass.set_bind_group(0, &baseline_bind, &[]);
            pass.dispatch_workgroups((width * height).div_ceil(group), 1, 1);
        }
        queue.submit([encoder.finish()]);
        let original = read_output(&device, &queue, &output, size);
        let mut different_pixels = 0u64;
        let mut different_words = 0u64;
        let mut different_owners = 0u64;
        let mut different_status = 0u64;
        let mut max_position = 0.0f64;
        let mut max_path = 0.0f64;
        let mut max_direction = 0.0f64;
        let mut matched_max = [0.0f64; 3];
        let mut matched_pixels = [[0u32; 2]; 3];
        let mut changed_classification = Vec::new();
        let mut examples = Vec::new();
        let mut candidate_status = [0u64; 4];
        let mut baseline_status = [0u64; 4];
        let reference_for = |x: u32, y: u32, aw: &[u32], bw: &[u32]| {
            let vector = |w: &[u32], o: usize| {
                Vec3d::new(
                    f64::from(f32::from_bits(w[o])),
                    f64::from(f32::from_bits(w[o + 1])),
                    f64::from(f32::from_bits(w[o + 2])),
                )
            };
            let ray = demo.lens.pixel_ray(frame, x, y, width, height).unwrap();
            match case.profile.trace_scene(ray, &case.scene, &case.markers) {
                Ok(c) => {
                    let world = |v: Vec3d| {
                        local.basis.east * v.x + local.basis.up * v.y - local.basis.north * v.z
                    };
                    let errors = |w: &[u32]| serde_json::json!({"position_m":(local.origin+world(vector(w,0))).distance(c.exit_ray.origin),"path_m":(f64::from(f32::from_bits(w[3]))-c.geometric_path_m).abs(),"direction":world(vector(w,4)).distance(c.exit_ray.direction)});
                    serde_json::json!({"status":u8::from(c.surface.is_some()||c.marker.is_some()),"marker_id":c.marker.map(|m|m.marker_id),"surface_kind":c.surface.map(|s|format!("{:?}",s.kind)),"path_m":c.geometric_path_m,"candidate_errors":errors(aw),"baseline_errors":errors(bw),"scope":"Canonical f64 Lens pixel-centre ray; not exactly the finite GPU launch calculation. Decorative signs are excluded from physical CPU picking."})
                }
                Err(e) => serde_json::json!({"error":e.to_string()}),
            }
        };
        for (i, (a, b)) in candidate
            .chunks_exact(48)
            .zip(original.chunks_exact(48))
            .enumerate()
        {
            let aw: Vec<_> = a
                .chunks_exact(4)
                .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                .collect();
            let bw: Vec<_> = b
                .chunks_exact(4)
                .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                .collect();
            candidate_status[aw[7].min(3) as usize] += 1;
            baseline_status[bw[7].min(3) as usize] += 1;
            different_status += u64::from(aw[7] != bw[7]);
            different_owners += u64::from(aw[11] != bw[11]);
            if a != b {
                different_pixels += 1;
                different_words += aw.iter().zip(&bw).filter(|(x, y)| x != y).count() as u64;
                let vector = |w: &[u32], offset: usize| {
                    Vec3d::new(
                        f64::from(f32::from_bits(w[offset])),
                        f64::from(f32::from_bits(w[offset + 1])),
                        f64::from(f32::from_bits(w[offset + 2])),
                    )
                };
                max_position = max_position.max(vector(&aw, 0).distance(vector(&bw, 0)));
                max_direction = max_direction.max(vector(&aw, 4).distance(vector(&bw, 4)));
                max_path = max_path.max(
                    (f64::from(f32::from_bits(aw[3])) - f64::from(f32::from_bits(bw[3]))).abs(),
                );
                if aw[7] == bw[7] && aw[11] == bw[11] {
                    let metrics = [
                        vector(&aw, 0).distance(vector(&bw, 0)),
                        (f64::from(f32::from_bits(aw[3])) - f64::from(f32::from_bits(bw[3]))).abs(),
                        vector(&aw, 4).distance(vector(&bw, 4)),
                    ];
                    for metric in 0..3 {
                        if metrics[metric] > matched_max[metric] {
                            matched_max[metric] = metrics[metric];
                            matched_pixels[metric] = [i as u32 % width, i as u32 / width];
                        }
                    }
                }
                let classification_changed = aw[7] != bw[7] || aw[11] != bw[11];
                if examples.len() < 4096 || classification_changed {
                    let x = i as u32 % width;
                    let y = i as u32 / width;
                    let reference = reference_for(x, y, &aw, &bw);
                    let record = serde_json::json!({"pixel":[x,y],"candidate":aw,"baseline":bw,"f64_physical_reference":reference});
                    if classification_changed {
                        changed_classification.push(record.clone());
                    }
                    if examples.len() < 4096 {
                        examples.push(record);
                    }
                }
            }
        }
        let worst_references:Vec<_>=matched_pixels.iter().enumerate().map(|(metric,&[x,y])|{
            let offset=((y*width+x) as usize)*48;
            let words=|bytes:&[u8]|bytes[offset..offset+48].chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect::<Vec<_>>();
            let a=words(&candidate);let b=words(&original);
            serde_json::json!({"metric":(["position","path","direction"][metric]),"pixel":[x,y],"candidate":a,"baseline":b,"f64_physical_reference":reference_for(x,y,&a,&b)})
        }).collect();
        Some(
            serde_json::json!({"baseline_variant":comparison_variant,"pixels":width*height,"different_pixels":different_pixels,"different_words":different_words,"different_status":different_status,"different_material_owner":different_owners,"maximum_position_difference_m":max_position,"maximum_path_difference_m":max_path,"maximum_direction_difference":max_direction,"same_status_owner_maximum_differences":matched_max,"same_status_owner_worst_pixels":matched_pixels,"same_status_owner_worst_references":worst_references,"all_changed_classification":changed_classification,"exact_all_words":different_words==0,"candidate_status_counts":candidate_status,"baseline_status_counts":baseline_status,"candidate_fnv1a64":digest(&candidate),"baseline_fnv1a64":digest(&original),"examples":examples,"scope":"Untimed byte-for-byte readback comparison of all 12 u32 terminal words per physical pixel, same immutable inputs/backend/workgroup; FNV is a convenience digest, equality uses every byte."}),
        )
    } else {
        None
    };
    let report = serde_json::json!({"upper_sky_mask":upper_sky,"adapter":format!("{:?}",adapter.get_info()),"dimensions":[width,height],"workgroup_size":group,"variant":variant,"pipeline_creation_seconds":compilation_seconds,"physical_faces":packed.triangle_count,"markers":case.markers.len(),"terminal_bytes":u64::from(width)*u64::from(height)*48,"iterations":rows,"samples":samples,"full_frame_comparison":comparison,"strictly_increasing_unique_timestamp_pairs":timestamps_valid,"scope":"GPU timestamp duration for full-resolution primary tracing and lossless terminal writes only. Full=unchanged production; cached=experimental algebraic field reuse with finite compiler rounding; dopri*=unregistered numerical diagnostic, not production approval; air=diagnostic removal of scene contacts and covered-h0, NOT equivalent physics or production candidate. No radiance, stellar inversion, application present/UI, or FPS claim. Three warmups, five timed dispatches; CPU submit/readback time separate."});
    let path =
        format!("artifacts/optical-trace-performance-{width}x{height}-g{group}{suffix}.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    eprintln!("report {path}");
    assert!(
        timestamps_valid,
        "Timestamp pairs were repeated/nonmonotone; retained report is invalid timing evidence."
    );
}
