//! Original MIT. Mechanical host for the math-owned selected-ray pipeline.
//! Ray equations, record strides, iteration limits and stopping rules belong
//! to the fixture. This host preserves raw evidence before asserting completion.
#[path = "optical_normal_scene_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "optical_shader_reachability.rs"]
mod reachability;
#[path = "optical_gpu_runtime.rs"]
#[allow(dead_code)]
mod runtime;
#[path = "optical_normal_selected_fixture.rs"]
mod selected;
use serde_json::json;
use std::{borrow::Cow, collections::BTreeMap, time::Instant};
use wgpu::util::DeviceExt;

fn words(bytes: &[u8]) -> Vec<u32> {
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut name = None;
    let mut validate_only = false;
    let mut prune_unused = false;
    let mut split_certificate = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" if name.is_none() => name = Some(args.next().expect("Missing name")),
            "--validate-only" if !validate_only => validate_only = true,
            "--prune-unused" if !prune_unused => prune_unused = true,
            "--split-certificate" if !split_certificate => split_certificate = true,
            _ => panic!("Unknown or repeated argument: {arg}"),
        }
    }
    let name = name.expect("--name required");
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    );
    let prefix = format!("artifacts/{name}");
    for suffix in [
        ".json",
        "-producer.wgsl",
        "-consumer.wgsl",
        "-certificate.wgsl",
        "-producer-original.wgsl",
        "-consumer-original.wgsl",
        "-certificate-original.wgsl",
        "-packaging.json",
        "-validation.json",
        "-primary.bin",
        "-field.bin",
        "-geometry.bin",
        "-frames.bin",
        "-certificates.bin",
        "-readback.bin",
        "-states.bin",
        "-pieces.bin",
        "-progress.bin",
    ] {
        assert!(
            !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve evidence"
        );
    }
    let prepared = if split_certificate {
        selected::build_split_certificate()
    } else {
        selected::build()
    };
    assert_eq!(prepared.certificate_shader.is_some(), split_certificate);
    let fixture = &prepared.scene;
    assert!(fixture.record_count > 0 && prepared.rounds > 0);
    assert_eq!(fixture.control[..2], [fixture.width, fixture.height]);
    assert_eq!(
        fixture.terminal_bytes,
        u64::from(fixture.record_count)
            .checked_mul(48)
            .unwrap()
            .checked_add(16)
            .unwrap()
    );
    assert_eq!(
        fixture.output_bytes - fixture.terminal_bytes,
        u64::from(fixture.record_count) * 32
    );
    assert_eq!(fixture.sample_indices.len(), fixture.sample_pixels.len());
    assert!(
        fixture
            .sample_indices
            .iter()
            .all(|&i| i < fixture.record_count)
    );
    for size in [
        prepared.state_bytes,
        prepared.piece_bytes,
        prepared.progress_bytes,
    ] {
        assert!(size > 0 && size.is_multiple_of(16));
    }
    let inputs = [
        (0, "primary", &fixture.primary_uniform),
        (1, "geometry", &fixture.geometry),
        (2, "frames", &fixture.frames),
        (4, "field", &fixture.field_uniform),
        (5, "certificates", &fixture.certificates),
    ];
    assert!(
        inputs
            .iter()
            .all(|(_, _, b)| !b.is_empty() && b.len().is_multiple_of(16))
    );
    for (_, label, bytes) in inputs {
        std::fs::write(format!("{prefix}-{label}.bin"), bytes).unwrap();
    }
    let mut entries = vec![
        ("normal_selected_init", "producer", fixture.dispatch),
        ("normal_selected_produce", "producer", fixture.dispatch),
    ];
    let mut repeated_entries = vec!["normal_selected_produce"];
    if split_certificate {
        entries.push(("normal_selected_certify", "certificate", fixture.dispatch));
        repeated_entries.push("normal_selected_certify");
    }
    repeated_entries.push("normal_selected_consume");
    entries.extend([
        ("normal_selected_consume", "consumer", fixture.dispatch),
        ("normal_selected_finalize", "producer", [1, 1, 1]),
    ]);
    let mut shaders = BTreeMap::new();
    let mut packaging = Vec::new();
    let mut source_modules = vec![
        ("producer", &prepared.producer_shader),
        ("consumer", &prepared.consumer_shader),
    ];
    if let Some(shader) = &prepared.certificate_shader {
        source_modules.push(("certificate", shader));
    }
    for (label, original) in source_modules {
        std::fs::write(format!("{prefix}-{label}-original.wgsl"), original).unwrap();
        let shader = if prune_unused {
            let (shader, report) = reachability::prune(original);
            packaging.push(json!({"module":label,"reachability":report}));
            shader
        } else {
            original.clone()
        };
        std::fs::write(format!("{prefix}-{label}.wgsl"), &shader).unwrap();
        let parsed = wgpu::naga::front::wgsl::parse_str(&shader)
            .unwrap_or_else(|e| panic!("{label}: {}", e.emit_to_string(&shader)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&parsed)
        .unwrap_or_else(|e| panic!("{label}: {}", e.emit_to_string(&shader)));
        for &(name, module, _) in &entries {
            if module == label {
                assert!(
                    parsed.entry_points.iter().any(|entry| {
                        entry.name == name && entry.stage == wgpu::naga::ShaderStage::Compute
                    }),
                    "Missing compute entry {name} in {label}"
                );
            }
        }
        shaders.insert(label, shader);
    }
    std::fs::write(
        format!("{prefix}-packaging.json"),
        serde_json::to_vec_pretty(&json!({"prune_unused":prune_unused,"split_certificate":split_certificate,"modules":packaging}))
            .unwrap(),
    )
    .unwrap();
    std::fs::write(format!("{prefix}-validation.json"), serde_json::to_vec_pretty(&json!({"all_modules_valid":true,"module_count":shaders.len(),"split_certificate":split_certificate,"rounds":prepared.rounds,"entries":entries,"state_bytes":prepared.state_bytes,"piece_bytes":prepared.piece_bytes,"progress_bytes":prepared.progress_bytes,"scope":"Offline layout and WGSL validation only. No physical-ray acceptance or performance claim."})).unwrap()).unwrap();
    if validate_only {
        println!("All modules passed offline validation; no graphics device requested.");
        return;
    }
    eprintln!("Selected fixture validated; requesting Vulkan device.");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = runtime::block(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = runtime::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    for &(_, _, dispatch) in &entries {
        runtime::validate_dispatch(&device, dispatch);
    }
    for bytes in [
        fixture.output_bytes,
        prepared.state_bytes,
        prepared.piece_bytes,
        prepared.progress_bytes,
    ] {
        assert!(
            bytes > 0
                && bytes <= device.limits().max_storage_buffer_binding_size
                && bytes <= device.limits().max_buffer_size
        );
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("selected canonical ray diagnostic"),
        entries: &(0..9)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 || binding == 4 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: ![3, 6, 7, 8].contains(&binding),
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>(),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let input_buffers: Vec<_> = inputs
        .iter()
        .map(|&(binding, label, bytes)| {
            (
                binding,
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage: if binding == 0 || binding == 4 {
                        wgpu::BufferUsages::UNIFORM
                    } else {
                        wgpu::BufferUsages::STORAGE
                    },
                }),
            )
        })
        .collect();
    let output_specs = [
        (3, "readback", fixture.output_bytes),
        (6, "states", prepared.state_bytes),
        (7, "pieces", prepared.piece_bytes),
        (8, "progress", prepared.progress_bytes),
    ];
    let output_buffers: Vec<_> = output_specs
        .iter()
        .map(|&(binding, label, size)| {
            (
                binding,
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            )
        })
        .collect();
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &input_buffers
            .iter()
            .chain(output_buffers.iter())
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>(),
    });
    let mut modules = BTreeMap::new();
    let mut module_times = Vec::new();
    for (&label, shader) in &shaders {
        let started = Instant::now();
        modules.insert(
            label,
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader)),
            }),
        );
        module_times.push(json!({"module":label,"seconds":started.elapsed().as_secs_f64()}));
    }
    let mut pipelines = BTreeMap::new();
    let mut pipeline_times = Vec::new();
    for &(entry, module, _) in &entries {
        let started = Instant::now();
        eprintln!("Creating pipeline {entry}.");
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&pipeline_layout),
            module: &modules[module],
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        });
        pipeline_times
            .push(json!({"entry":entry,"module":module,"seconds":started.elapsed().as_secs_f64()}));
        pipelines.insert(entry, pipeline);
        eprintln!("Pipeline {entry} ready.");
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    for (_, buffer) in &output_buffers {
        encoder.clear_buffer(buffer, 0, None);
    }
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("selected complete ray sequence"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, &bind, &[]);
        pass.set_pipeline(&pipelines["normal_selected_init"]);
        pass.dispatch_workgroups(
            fixture.dispatch[0],
            fixture.dispatch[1],
            fixture.dispatch[2],
        );
        for _ in 0..prepared.rounds {
            for &entry in &repeated_entries {
                pass.set_pipeline(&pipelines[entry]);
                pass.dispatch_workgroups(
                    fixture.dispatch[0],
                    fixture.dispatch[1],
                    fixture.dispatch[2],
                );
            }
        }
        pass.set_pipeline(&pipelines["normal_selected_finalize"]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    eprintln!("Dispatching bounded sequence, then preserving complete outputs.");
    queue.submit([encoder.finish()]);
    let mut raw_outputs = BTreeMap::new();
    for ((binding, label, size), (actual_binding, buffer)) in
        output_specs.iter().zip(&output_buffers)
    {
        assert_eq!(binding, actual_binding);
        let raw = runtime::copy_read(&device, &queue, buffer, *size);
        std::fs::write(format!("{prefix}-{label}.bin"), &raw).unwrap();
        raw_outputs.insert(*label, raw);
    }
    let raw = &raw_outputs["readback"];
    let header = words(&raw[..16]);
    let header_matches = header == fixture.control;
    let terminal_end = usize::try_from(fixture.terminal_bytes).unwrap();
    let mut statuses = BTreeMap::<u32, usize>::new();
    for record in raw[16..terminal_end].chunks_exact(48) {
        *statuses
            .entry(u32::from_le_bytes(record[28..32].try_into().unwrap()))
            .or_default() += 1;
    }
    let samples:Vec<_>=fixture.sample_indices.iter().zip(&fixture.sample_pixels).map(|(&index,pixel)|{
        let a=16+index as usize*48;let b=terminal_end+index as usize*32;
        json!({"pixel":pixel,"record_index":index,"terminal_words":words(&raw[a..a+48]),"diagnostic_words":words(&raw[b..b+32])})
    }).collect();
    let report = json!({
        "scope":"Selected complete-ray diagnostic. Raw states and generated pieces are retained for independent physics review. No application performance claim.",
        "adapter":format!("{:?}",adapter.get_info()),"selected":true,"timed":false,"prune_unused":prune_unused,"split_certificate":split_certificate,
        "dimensions":[fixture.width,fixture.height],"record_count":fixture.record_count,"dispatch":fixture.dispatch,"rounds":prepared.rounds,"entries":entries,
        "output_bytes":fixture.output_bytes,"terminal_bytes":fixture.terminal_bytes,"diagnostic_bytes_per_record":32,"state_bytes":prepared.state_bytes,"piece_bytes":prepared.piece_bytes,"progress_bytes":prepared.progress_bytes,
        "shader_modules":module_times,"pipelines":pipeline_times,"header":header,"expected_header":fixture.control,"header_matches":header_matches,"progress_words":words(&raw_outputs["progress"]),"raw_status_counts":statuses,"samples":samples,"fixture":fixture.cases,
    });
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        header_matches,
        "Incomplete or wrong terminal header; raw evidence retained"
    );
    println!("Wrote {prefix}; independent comparison required.");
}
