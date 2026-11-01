//! Original MIT. Mechanical GPU plumbing for math-owned trial cost isolation.
#[path = "optical_cartesian_direct_fixture.rs"]
mod cartesian_direct_fixture;
#[path = "optical_cartesian_trial_fixture.rs"]
mod cartesian_fixture;
#[path = "optical_height_trial_fixture.rs"]
mod fixture;
#[path = "optical_gpu_runtime.rs"]
mod runtime;
use serde_json::json;
use std::{borrow::Cow, time::Instant};
use wgpu::util::DeviceExt;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut requested_name = None;
    let mut validation_only = false;
    let mut fast_bounds = false;
    let mut cartesian_promoted = false;
    let mut cartesian_direct = false;
    let mut flags = args.iter().skip(1);
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--name" if requested_name.is_none() => {
                requested_name = Some(flags.next().expect("--name requires a value").as_str())
            }
            "--validate-only" if !validation_only => validation_only = true,
            "--fast-bounds" if !fast_bounds => fast_bounds = true,
            "--cartesian-promoted" if !cartesian_promoted => cartesian_promoted = true,
            "--cartesian-direct" if !cartesian_direct => cartesian_direct = true,
            _ => panic!("Unknown or repeated argument: {flag}"),
        }
    }
    assert!(
        !(fast_bounds && cartesian_promoted
            || cartesian_direct && (fast_bounds || cartesian_promoted)),
        "Choose one trial fixture."
    );
    let name = requested_name.unwrap_or("optical-height-trial-cost");
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    );
    let prefix = format!("artifacts/{name}");
    for suffix in [
        ".wgsl",
        "-uniform.bin",
        "-inputs.bin",
        "-detail.bin",
        "-cost-full.bin",
        "-cost-stages.bin",
        "-cost-bounds.bin",
        "-cost-reference.bin",
        "-cost-promoted.bin",
        "-validation.json",
        ".json",
    ] {
        assert!(
            !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve existing evidence"
        );
    }
    let fixture = if cartesian_direct {
        cartesian_direct_fixture::build(8, 256)
    } else if cartesian_promoted {
        cartesian_fixture::build(8, 256)
    } else if fast_bounds {
        fixture::build_fast("artifacts/optical-height-trial-capture", 8, 256)
    } else {
        fixture::build("artifacts/optical-height-trial-capture", 8, 256)
    };
    assert!(
        fixture.count > 0
            && fixture.detail_record_words > 0
            && fixture.detail_record_words.is_multiple_of(4)
    );
    let detail_bytes = u64::from(fixture.count)
        .checked_mul(u64::from(fixture.detail_record_words))
        .unwrap()
        .checked_mul(4)
        .unwrap();
    let cost_bytes = if cartesian_direct {
        fixture.metadata["direct_output_bytes"]
            .as_u64()
            .expect("Direct-output allocation must be supplied by the reviewed fixture.")
    } else {
        u64::from(fixture.cost_invocations).checked_mul(16).unwrap()
    };
    assert!(cost_bytes > 0);
    std::fs::write(format!("{prefix}.wgsl"), &fixture.shader).unwrap();
    std::fs::write(format!("{prefix}-uniform.bin"), &fixture.uniform).unwrap();
    std::fs::write(format!("{prefix}-inputs.bin"), &fixture.inputs).unwrap();
    let parsed = wgpu::naga::front::wgsl::parse_str(&fixture.shader)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&fixture.shader)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&parsed)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&fixture.shader)));
    let mut bindings = std::collections::BTreeSet::new();
    for (_, variable) in parsed.global_variables.iter() {
        let Some(binding) = &variable.binding else {
            continue;
        };
        assert_eq!(binding.group, 0, "Unexpected bind group.");
        assert!(binding.binding < 3 && bindings.insert(binding.binding));
        match variable.space {
            wgpu::naga::AddressSpace::Uniform => assert_eq!(binding.binding, 0),
            wgpu::naga::AddressSpace::Storage { access } => {
                assert!([1, 2].contains(&binding.binding));
                assert!(access.contains(wgpu::naga::StorageAccess::LOAD));
                assert_eq!(
                    access.contains(wgpu::naga::StorageAccess::STORE),
                    binding.binding == 2
                );
            }
            _ => panic!("Unexpected resource in buffer-only trial fixture."),
        }
    }
    assert_eq!(bindings.into_iter().collect::<Vec<_>>(), [0, 1, 2]);
    let expected_entries = if cartesian_direct {
        vec!["direct_reference", "direct_promoted"]
    } else if cartesian_promoted {
        vec!["trial_detail", "cost_reference", "cost_promoted"]
    } else if fast_bounds {
        vec![
            "trial_detail",
            "cost_full",
            "cost_stages",
            "cost_bounds",
            "cost_reference",
        ]
    } else {
        vec!["trial_detail", "cost_full", "cost_stages", "cost_bounds"]
    };
    let fixture_entries: Vec<_> = fixture.metadata["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap())
        .collect();
    assert_eq!(fixture_entries, expected_entries);
    let parsed_entries: Vec<_> = parsed
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(
        parsed_entries, expected_entries,
        "Shader entries differ from fixture contract."
    );
    if validation_only {
        let report = json!({"scope":"Offline Naga validation only; no graphics-device calls or dispatch.","cartesian_direct":cartesian_direct,"cartesian_promoted":cartesian_promoted,"fast_bounds":fast_bounds,"entries":fixture_entries,"count":fixture.count,"detail_record_words":fixture.detail_record_words,"cost_invocations":fixture.cost_invocations,"fixture":fixture.metadata});
        std::fs::write(
            format!("{prefix}-validation.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        println!(
            "Trial fixture passed offline Naga validation; source and input artifacts retained, no GPU execution."
        );
        return;
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = runtime::block(instance.request_adapter(&Default::default())).unwrap();
    let mut features = wgpu::Features::TIMESTAMP_QUERY;
    if cartesian_promoted || cartesian_direct {
        features |= wgpu::Features::SHADER_F64 | wgpu::Features::SHADER_INT64;
    }
    assert!(
        adapter.features().contains(features),
        "Adapter lacks required diagnostic features: {features:?}"
    );
    let (device, queue) = runtime::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("identical trial inputs"),
        entries: &(0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: binding == 1,
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
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.uniform,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let inputs = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &fixture.inputs,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let module_start = Instant::now();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("actual trial isolation"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(fixture.shader)),
    });
    let module_seconds = module_start.elapsed().as_secs_f64();
    let mut entries = Vec::new();
    let mut timestamps_valid = true;
    for entry in fixture_entries {
        let detail = entry == "trial_detail";
        let size = if detail { detail_bytes } else { cost_bytes };
        let dispatch = if detail {
            fixture.detail_dispatch
        } else {
            fixture.cost_dispatch
        };
        runtime::validate_dispatch(&device, dispatch);
        let compilation_start = Instant::now();
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        });
        let compilation_seconds = compilation_start.elapsed().as_secs_f64();
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(entry),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | if cartesian_direct {
                    wgpu::BufferUsages::COPY_DST
                } else {
                    wgpu::BufferUsages::empty()
                },
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(entry),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: inputs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let rows = if detail {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(entry),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
            }
            queue.submit([encoder.finish()]);
            Vec::new()
        } else if cartesian_direct {
            let (rows, valid) = runtime::timed_dispatch_observed(
                &device,
                &queue,
                &pipeline,
                &bind,
                dispatch,
                |_| {
                    let mut clear =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("untimed full output initialization"),
                        });
                    clear.clear_buffer(&output, 0, None);
                    queue.submit([clear.finish()]);
                },
                |iteration| {
                    let raw = runtime::copy_read(&device, &queue, &output, size);
                    assert_eq!(raw.len() as u64, size);
                    let path = format!("{prefix}-{entry}-iteration-{iteration}.bin");
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .unwrap();
                    std::io::Write::write_all(&mut file, &raw).unwrap();
                    Some(
                        json!({"path":path,"bytes":raw.len(),"complete_output_retained":true,"output_cleared_before_dispatch":true,"output_captured_after_dispatch":true}),
                    )
                },
            );
            timestamps_valid &= valid;
            rows
        } else {
            let (rows, valid) =
                runtime::timed_dispatch(&device, &queue, &pipeline, &bind, dispatch);
            timestamps_valid &= valid;
            rows
        };
        if !cartesian_direct {
            let raw = runtime::copy_read(&device, &queue, &output, size);
            assert_eq!(raw.len() as u64, size);
            let suffix = if detail {
                "detail".to_string()
            } else {
                entry.replace('_', "-")
            };
            std::fs::write(format!("{prefix}-{suffix}.bin"), &raw).unwrap();
        }
        entries.push(json!({"entry":entry,"dispatch":dispatch,"output_bytes":size,"pipeline_creation_seconds":compilation_seconds,"iterations":rows}));
        eprintln!("Completed {entry}; raw output retained.");
    }
    let report = json!({"scope":"Isolated fixed trial workload. Components are not additive; endpoint controller work, scene work and rendering are absent. This is not full-frame or native application FPS. All raw buffers retained for independent numerical/checksum audit.","direct_output_scope":if cartesian_direct {Some("Every dispatch writes the full declared numeric output. Full buffer clear precedes each dispatch; clear and readback/file retention are outside timestamps. All warmup and measured outputs are retained. These timings include full output stores and do not measure an uninstrumented solver.")} else {None},"adapter":format!("{:?}",adapter.get_info()),"required_features":format!("{features:?}"),"cartesian_direct":cartesian_direct,"cartesian_promoted":cartesian_promoted,"fast_bounds":fast_bounds,"count":fixture.count,"detail_record_words":fixture.detail_record_words,"cost_invocations":fixture.cost_invocations,"shader_module_creation_seconds":module_seconds,"strictly_increasing_unique_timestamp_pairs":timestamps_valid,"entries":entries,"fixture":fixture.metadata});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        timestamps_valid,
        "Invalid timestamps; all raw evidence retained"
    );
    println!("Wrote {prefix}.json");
}
