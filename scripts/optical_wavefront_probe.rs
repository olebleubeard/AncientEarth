//! Original MIT. Host-only orchestration of math-owned continuation fixtures.
#[path = "optical_cartesian_composite_fixture.rs"]
mod composite_f64;
#[path = "optical_cartesian_composite_full_fixture.rs"]
mod composite_full;
#[path = "optical_normal_scene_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "optical_original_clearance_fixture.rs"]
mod original_clearance;
#[path = "optical_wavefront_reduced_fixture.rs"]
mod reduced;
#[path = "optical_gpu_runtime.rs"]
#[allow(dead_code)]
mod runtime;
#[path = "optical_dispatch_sequence.rs"]
mod sequence;
#[path = "optical_cartesian_suffix_fixture.rs"]
mod suffix;
#[path = "optical_cartesian_suffix_branch_fixture.rs"]
mod suffix_branch;
#[path = "optical_cartesian_suffix_f64_fixture.rs"]
mod suffix_f64;
#[path = "optical_cartesian_suffix_tight_fixture.rs"]
mod suffix_tight;
#[path = "optical_cartesian_suffix_trace_fixture.rs"]
mod suffix_trace;
#[path = "optical_midpoint_wavefront_fixture.rs"]
mod wavefront;
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
    let mut full = false;
    let mut timed = false;
    let mut reference = false;
    let mut validate_only = false;
    let mut reduce_bookkeeping = false;
    let mut cartesian_suffix = false;
    let mut cartesian_suffix_trace = false;
    let mut cartesian_suffix_tight = false;
    let mut cartesian_suffix_branch = None;
    let mut cartesian_suffix_f64 = None;
    let mut cartesian_composite_f64 = None;
    let mut cartesian_composite_reference = false;
    let mut cartesian_composite_full = false;
    let mut original_clearance = false;
    let mut branch_reference = false;
    let mut slice_attempts = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" if name.is_none() => name = Some(args.next().expect("Missing name")),
            "--slice-attempts" if slice_attempts.is_none() => {
                slice_attempts = Some(args.next().expect("Missing count").parse::<u32>().unwrap())
            }
            "--full" if !full => full = true,
            "--time" if !timed => timed = true,
            "--reference" if !reference => reference = true,
            "--reduce-bookkeeping" if !reduce_bookkeeping => reduce_bookkeeping = true,
            "--cartesian-suffix" if !cartesian_suffix => cartesian_suffix = true,
            "--cartesian-suffix-trace" if !cartesian_suffix_trace => cartesian_suffix_trace = true,
            "--cartesian-suffix-tight" if !cartesian_suffix_tight => cartesian_suffix_tight = true,
            "--original-clearance" if !original_clearance => original_clearance = true,
            "--cartesian-suffix-branch" if cartesian_suffix_branch.is_none() => {
                cartesian_suffix_branch = Some(args.next().expect("Missing branch case"));
            }
            "--cartesian-suffix-f64" if cartesian_suffix_f64.is_none() => {
                cartesian_suffix_f64 = Some(args.next().expect("Missing f64 suffix case"));
            }
            "--cartesian-composite-f64" if cartesian_composite_f64.is_none() => {
                cartesian_composite_f64 = Some(args.next().expect("Missing composite case"));
            }
            "--cartesian-composite-reference" if !cartesian_composite_reference => {
                cartesian_composite_reference = true;
            }
            "--cartesian-composite-full" if !cartesian_composite_full => {
                cartesian_composite_full = true;
            }
            "--branch-reference" if !branch_reference => branch_reference = true,
            "--validate-only" if !validate_only => validate_only = true,
            _ => panic!("Unknown or repeated argument: {arg}"),
        }
    }
    assert!(!timed || full);
    assert!(
        !cartesian_composite_full
            || (full
                && !(reference
                    || reduce_bookkeeping
                    || original_clearance
                    || cartesian_suffix
                    || cartesian_suffix_trace
                    || cartesian_suffix_tight
                    || cartesian_suffix_branch.is_some()
                    || cartesian_suffix_f64.is_some()
                    || cartesian_composite_f64.is_some()
                    || cartesian_composite_reference
                    || branch_reference
                    || slice_attempts.is_some())),
        "--cartesian-composite-full requires --full and excludes other fixtures"
    );
    assert!(
        !branch_reference || cartesian_suffix_branch.is_some(),
        "--branch-reference requires --cartesian-suffix-branch"
    );
    cartesian_suffix_tight |= cartesian_suffix_branch.is_some();
    cartesian_suffix_trace |= cartesian_suffix_tight;
    assert!(
        !cartesian_suffix_trace || (!full && !timed),
        "Suffix trace is an untimed selected-record diagnostic"
    );
    cartesian_suffix |= cartesian_suffix_trace;
    if let Some(case) = cartesian_suffix_f64.as_deref() {
        assert!(matches!(case, "selected" | "atmosphere-100-gradient"));
        assert!(
            !(full
                || timed
                || reference
                || reduce_bookkeeping
                || original_clearance
                || cartesian_suffix
                || cartesian_composite_f64.is_some()
                || branch_reference
                || slice_attempts.is_some()),
            "The f64 suffix has only two independently reviewed selected, untimed cases."
        );
    }
    assert!(
        !cartesian_composite_reference
            || cartesian_composite_f64.as_deref() == Some("returning-gradient"),
        "Composite reference is only available for returning-gradient"
    );
    if let Some(case) = cartesian_composite_f64.as_deref() {
        assert!(matches!(
            case,
            "readiness"
                | "fisheye"
                | "vacuum-outward"
                | "vacuum-inward"
                | "atmosphere-minus20-vacuum"
                | "atmosphere-minus20-gradient"
                | "atmosphere-100-vacuum"
                | "atmosphere-100-gradient"
                | "atmosphere-5000-vacuum"
                | "atmosphere-5000-gradient"
                | "returning-gradient"
        ));
        assert!(
            !(full
                || timed
                || reference
                || reduce_bookkeeping
                || original_clearance
                || cartesian_suffix
                || cartesian_suffix_f64.is_some()
                || branch_reference
                || slice_attempts.is_some()),
            "Composite correctness cases are selected, untimed and separate from other fixtures."
        );
    }
    assert!(
        !original_clearance
            || !(cartesian_suffix || reference || reduce_bookkeeping || slice_attempts.is_some()),
        "--original-clearance is a separate original-solver fixture"
    );
    let separate_modules = cartesian_suffix
        || original_clearance
        || cartesian_suffix_f64.is_some()
        || cartesian_composite_f64.is_some()
        || cartesian_composite_full;
    assert!(!(reference && reduce_bookkeeping));
    assert!(
        !cartesian_suffix || (!reference && !reduce_bookkeeping && slice_attempts.is_none()),
        "--cartesian-suffix is separate from reference and sliced continuation modes"
    );
    let slice_attempts = slice_attempts.unwrap_or(32);
    assert!([1, 8, 32, 128].contains(&slice_attempts));
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
        ".wgsl",
        "-primary.bin",
        "-field.bin",
        "-geometry.bin",
        "-frames.bin",
        "-certificates.bin",
        "-readback.bin",
        "-states.bin",
        "-progress.bin",
        "-partials.bin",
        "-auxiliary.bin",
        "-validation.json",
    ] {
        assert!(
            !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve evidence"
        );
    }
    let mut suffix_modules = None;
    let mut suffix_step_modules = Vec::new();
    let mut state_words = 12_u32;
    let mut auxiliary_layout = None;
    let (prepared, partial_layout) = if separate_modules {
        let candidate = if cartesian_composite_full {
            composite_full::build_full()
        } else if let Some(case) = cartesian_composite_f64.as_deref() {
            composite_f64::build(case, cartesian_composite_reference)
        } else if let Some(case) = cartesian_suffix_f64.as_deref() {
            suffix_f64::build(case)
        } else if original_clearance {
            original_clearance::build(!full)
        } else if cartesian_suffix_trace {
            let capture = if let Some(case) = &cartesian_suffix_branch {
                suffix_branch::build(case, branch_reference)
            } else if cartesian_suffix_tight {
                suffix_tight::build()
            } else {
                suffix_trace::build()
            };
            auxiliary_layout = Some((
                capture.auxiliary_bytes,
                capture.log_words_per_record,
                capture.max_steps,
            ));
            capture.suffix
        } else {
            suffix::build(!full)
        };
        state_words = candidate.state_words;
        suffix_step_modules = candidate.steps.iter().map(|s| s.module_index).collect();
        suffix_modules = Some(candidate.modules);
        (
            wavefront::WaveFixture {
                scene: candidate.scene,
                state_bytes: candidate.state_bytes,
                progress_bytes: candidate.progress_bytes,
                steps: candidate
                    .steps
                    .into_iter()
                    .map(|s| wavefront::WaveStep {
                        entry: s.entry,
                        dispatch: s.dispatch,
                        repetitions: s.repetitions,
                    })
                    .collect(),
            },
            None,
        )
    } else if reduce_bookkeeping {
        let reduced = reduced::build(!full, slice_attempts);
        (
            reduced.wave,
            Some((reduced.partial_bytes, reduced.block_count)),
        )
    } else if reference {
        let scene = wavefront::build_counted_reference(!full);
        (
            wavefront::WaveFixture {
                state_bytes: u64::from(scene.record_count).checked_mul(48).unwrap(),
                progress_bytes: 16,
                steps: vec![wavefront::WaveStep {
                    entry: "normal_scene",
                    dispatch: scene.dispatch,
                    repetitions: 1,
                }],
                scene,
            },
            None,
        )
    } else {
        (wavefront::build(!full, slice_attempts), None)
    };
    let fixture = &prepared.scene;
    assert!(fixture.record_count > 0);
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
    assert!(state_words > 0 && state_words.is_multiple_of(4));
    let state_stride = u64::from(state_words).checked_mul(4).unwrap();
    assert_eq!(
        prepared.state_bytes,
        u64::from(fixture.record_count)
            .checked_mul(state_stride)
            .unwrap()
    );
    assert_eq!(prepared.progress_bytes, 16);
    if let Some((bytes, log_words, max_steps)) = auxiliary_layout {
        assert!(bytes > 0 && log_words > 0 && max_steps > 0);
        assert_eq!(
            bytes,
            u64::from(fixture.record_count)
                .checked_mul(u64::from(log_words))
                .unwrap()
                .checked_mul(4)
                .unwrap()
        );
    }
    assert!(!prepared.steps.is_empty());
    assert_eq!(fixture.sample_indices.len(), fixture.sample_pixels.len());
    assert!(
        fixture
            .sample_indices
            .iter()
            .all(|&i| i < fixture.record_count)
    );
    if full {
        assert_eq!(
            fixture.record_count,
            fixture.width.checked_mul(fixture.height).unwrap()
        );
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
    std::fs::write(format!("{prefix}.wgsl"), &fixture.shader).unwrap();
    for (_, label, bytes) in inputs {
        std::fs::write(format!("{prefix}-{label}.bin"), bytes).unwrap();
    }
    let module_sources: Vec<(&str, &str)> = if let Some(modules) = &suffix_modules {
        modules
            .iter()
            .map(|m| (m.name, m.shader.as_str()))
            .collect()
    } else {
        vec![("continuation", fixture.shader.as_str())]
    };
    let step_modules = if separate_modules {
        suffix_step_modules
    } else {
        vec![0; prepared.steps.len()]
    };
    assert!(!module_sources.is_empty());
    assert_eq!(step_modules.len(), prepared.steps.len());
    assert!(
        step_modules
            .iter()
            .all(|&index| index < module_sources.len())
    );
    let mut module_names = std::collections::BTreeSet::new();
    eprintln!("Continuation fixture prepared; validating every WGSL module and entry.");
    for (index, &(label, source)) in module_sources.iter().enumerate() {
        assert!(
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        );
        assert!(module_names.insert(label), "Duplicate module name");
        if separate_modules {
            let path = format!("{prefix}-{label}.wgsl");
            assert!(
                !std::path::Path::new(&path).exists(),
                "Preserve module evidence"
            );
            std::fs::write(path, source).unwrap();
        }
        let parsed = wgpu::naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&parsed)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        for (_, variable) in parsed.global_variables.iter() {
            let Some(binding) = &variable.binding else {
                continue;
            };
            assert_eq!(binding.group, 0, "Unexpected bind group");
            assert!(
                binding.binding
                    < if reduce_bookkeeping || cartesian_suffix_trace {
                        9
                    } else {
                        8
                    },
                "Unexpected binding"
            );
            match variable.space {
                wgpu::naga::AddressSpace::Uniform => {
                    assert!([0, 4].contains(&binding.binding), "Unexpected uniform slot");
                }
                wgpu::naga::AddressSpace::Storage { access } => {
                    assert!(
                        ![0, 4].contains(&binding.binding),
                        "Storage in uniform slot"
                    );
                    assert!(access.contains(wgpu::naga::StorageAccess::LOAD));
                    assert_eq!(
                        access.contains(wgpu::naga::StorageAccess::STORE),
                        [3, 6, 7, 8].contains(&binding.binding),
                        "Storage access differs from host layout"
                    );
                }
                _ => panic!("Unexpected resource type in buffer-only fixture"),
            }
        }
        for (step, &module_index) in prepared.steps.iter().zip(&step_modules) {
            if module_index == index {
                assert!(
                    parsed
                        .entry_points
                        .iter()
                        .any(|entry| entry.name == step.entry
                            && entry.stage == wgpu::naga::ShaderStage::Compute),
                    "Missing compute entry in selected module"
                );
            }
        }
    }
    std::fs::write(
        format!("{prefix}-validation.json"),
        serde_json::to_vec_pretty(&json!({
            "all_modules_valid":true,
            "module_count":module_sources.len(),
            "modules":module_sources.iter().map(|(name,_)|name).collect::<Vec<_>>(),
            "cartesian_suffix":cartesian_suffix,
            "original_clearance":original_clearance,
            "cartesian_suffix_trace":cartesian_suffix_trace,
            "cartesian_suffix_tight":cartesian_suffix_tight,
            "cartesian_suffix_branch":cartesian_suffix_branch,
            "cartesian_suffix_f64":cartesian_suffix_f64,
            "cartesian_composite_f64":cartesian_composite_f64,
            "cartesian_composite_reference":cartesian_composite_reference,
            "cartesian_composite_full":cartesian_composite_full,
            "branch_reference":branch_reference,
            "auxiliary_layout":auxiliary_layout.map(|(bytes,words,max_steps)|json!({"bytes":bytes,"log_words_per_record":words,"max_steps":max_steps})),
            "selected":!full,
            "state_words_per_record":state_words,
            "scope":"Offline syntax, validation, bind-layout and entry-point checks only; not numerical acceptance or GPU execution."
        })).unwrap(),
    ).unwrap();
    if validate_only {
        println!("Offline validation passed; no graphics device requested.");
        return;
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = runtime::block(instance.request_adapter(&Default::default())).unwrap();
    let mut features = if timed {
        wgpu::Features::TIMESTAMP_QUERY
    } else {
        wgpu::Features::empty()
    };
    if cartesian_suffix_f64.is_some()
        || cartesian_composite_f64.is_some()
        || cartesian_composite_full
    {
        features |= wgpu::Features::SHADER_F64 | wgpu::Features::SHADER_INT64;
    }
    assert!(adapter.features().contains(features));
    let (device, queue) = runtime::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    for step in &prepared.steps {
        runtime::validate_dispatch(&device, step.dispatch);
        assert!(step.repetitions > 0);
    }
    for bytes in [
        fixture.output_bytes,
        prepared.state_bytes,
        prepared.progress_bytes,
    ] {
        assert!(
            bytes > 0
                && bytes <= device.limits().max_storage_buffer_binding_size
                && bytes <= device.limits().max_buffer_size
        );
    }
    if let Some((bytes, blocks)) = partial_layout {
        assert!(blocks > 0 && bytes == u64::from(blocks) * 16);
        assert!(
            bytes <= device.limits().max_storage_buffer_binding_size
                && bytes <= device.limits().max_buffer_size
        );
    }
    if let Some((bytes, _, _)) = auxiliary_layout {
        assert!(
            bytes <= device.limits().max_storage_buffer_binding_size
                && bytes <= device.limits().max_buffer_size
        );
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("isolated continuation fixture"),
        entries: &(0..if reduce_bookkeeping || cartesian_suffix_trace {
            9
        } else {
            8
        })
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
    let buffers: Vec<_> = inputs
        .iter()
        .map(|&(binding, label, bytes)| {
            let usage = if binding == 0 || binding == 4 {
                wgpu::BufferUsages::UNIFORM
            } else {
                wgpu::BufferUsages::STORAGE
            };
            (
                binding,
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage,
                }),
            )
        })
        .collect();
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terminals"),
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let states = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("continuation states"),
        size: prepared.state_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let progress = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sequence progress"),
        size: prepared.progress_bytes,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let partials = partial_layout.map(|(size, _)| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("final validation partial records"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    });
    let auxiliary = auxiliary_layout.map(|(size, _, _)| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("suffix accepted-step diagnostic"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });
    let mut entries: Vec<_> = buffers
        .iter()
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding: *binding,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend([
        wgpu::BindGroupEntry {
            binding: 3,
            resource: output.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 6,
            resource: states.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 7,
            resource: progress.as_entire_binding(),
        },
    ]);
    if let Some(partials) = &partials {
        entries.push(wgpu::BindGroupEntry {
            binding: 8,
            resource: partials.as_entire_binding(),
        });
    }
    if let Some(auxiliary) = &auxiliary {
        assert!(partials.is_none());
        entries.push(wgpu::BindGroupEntry {
            binding: 8,
            resource: auxiliary.as_entire_binding(),
        });
    }
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &entries,
    });
    let module_started = Instant::now();
    let modules: Vec<_> = module_sources
        .iter()
        .map(|&(label, source)| {
            eprintln!("Creating shader module {label}.");
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
            })
        })
        .collect();
    let module_seconds = module_started.elapsed().as_secs_f64();
    eprintln!("Shader modules ready; creating sequence pipelines.");
    let mut pipelines = BTreeMap::new();
    let mut pipeline_times = Vec::new();
    for (step, &module_index) in prepared.steps.iter().zip(&step_modules) {
        let key = (module_index, step.entry);
        if pipelines.contains_key(&key) {
            continue;
        }
        let started = Instant::now();
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(step.entry),
            layout: Some(&pipeline_layout),
            module: &modules[module_index],
            entry_point: Some(step.entry),
            compilation_options: Default::default(),
            cache: None,
        });
        pipeline_times.push(json!({"module_index":module_index,"entry":step.entry,"seconds":started.elapsed().as_secs_f64()}));
        pipelines.insert(key, pipeline);
        eprintln!("Pipeline {} ready.", step.entry);
    }
    let steps: Vec<_> = prepared
        .steps
        .iter()
        .zip(&step_modules)
        .map(|(s, &module_index)| sequence::Dispatch {
            pipeline: &pipelines[&(module_index, s.entry)],
            bind: &bind,
            groups: s.dispatch,
            repeats: s.repetitions,
        })
        .collect();
    eprintln!("Dispatching complete sequence.");
    let (iterations, timestamps_valid) = if timed {
        let clear_buffers: &[&wgpu::Buffer] = if original_clearance || cartesian_composite_full {
            &[]
        } else {
            &[&progress]
        };
        sequence::timed_sequence(&device, &queue, &steps, clear_buffers)
    } else {
        let mut encoder = device.create_command_encoder(&Default::default());
        if !(original_clearance || cartesian_composite_full) {
            encoder.clear_buffer(&progress, 0, None);
        }
        if let Some(auxiliary) = &auxiliary {
            encoder.clear_buffer(auxiliary, 0, None);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("untimed continuation validation"),
                timestamp_writes: None,
            });
            sequence::encode(&mut pass, &steps);
        }
        queue.submit([encoder.finish()]);
        (Vec::new(), true)
    };
    eprintln!("Reading complete terminal, state and progress buffers.");
    let raw = runtime::copy_read(&device, &queue, &output, fixture.output_bytes);
    let state_raw = runtime::copy_read(&device, &queue, &states, prepared.state_bytes);
    let progress_raw = runtime::copy_read(&device, &queue, &progress, prepared.progress_bytes);
    if let Some(partials) = &partials {
        let raw = runtime::copy_read(&device, &queue, partials, partial_layout.unwrap().0);
        std::fs::write(format!("{prefix}-partials.bin"), raw).unwrap();
    }
    if let Some(auxiliary) = &auxiliary {
        let raw = runtime::copy_read(&device, &queue, auxiliary, auxiliary_layout.unwrap().0);
        std::fs::write(format!("{prefix}-auxiliary.bin"), raw).unwrap();
    }
    for (label, bytes) in [
        ("readback", &raw),
        ("states", &state_raw),
        ("progress", &progress_raw),
    ] {
        std::fs::write(format!("{prefix}-{label}.bin"), bytes).unwrap();
    }
    let header = words(&raw[..16]);
    let header_matches = header == fixture.control;
    let terminal_end = usize::try_from(fixture.terminal_bytes).unwrap();
    let mut statuses = BTreeMap::<u32, usize>::new();
    for record in raw[16..terminal_end].chunks_exact(48) {
        *statuses
            .entry(u32::from_le_bytes(record[28..32].try_into().unwrap()))
            .or_default() += 1;
    }
    let samples: Vec<_> = fixture.sample_indices.iter().zip(&fixture.sample_pixels).map(|(&index, pixel)| {
        let a = 16 + index as usize * 48;
        let b = terminal_end + index as usize * 32;
        let stride = usize::try_from(state_stride).unwrap();
        let c = index as usize * stride;
        json!({"pixel":pixel,"record_index":index,"terminal_words":words(&raw[a..a+48]),"diagnostic_words":words(&raw[b..b+32]),"state_words":words(&state_raw[c..c+stride])})
    }).collect();
    let mut report = json!({
        "scope":"Isolated complete sequence. No application FPS or numerical acceptance is implied. Preserve all raw inputs, final states and outputs for independent review.",
        "adapter":format!("{:?}",adapter.get_info()),"reference":reference,"selected":!full,"timed":timed,"slice_attempts":if separate_modules { None } else { Some(slice_attempts) },"reduce_bookkeeping":reduce_bookkeeping,"cartesian_suffix":cartesian_suffix,"original_clearance":original_clearance,"partial_layout":partial_layout.map(|(bytes,blocks)|json!({"bytes":bytes,"blocks":blocks})),
        "dimensions":[fixture.width,fixture.height],"record_count":fixture.record_count,"output_bytes":fixture.output_bytes,"terminal_bytes":fixture.terminal_bytes,"state_bytes":prepared.state_bytes,"progress_bytes":prepared.progress_bytes,
        "state_words_per_record":state_words,"modules":module_sources.iter().map(|(label,_)|label).collect::<Vec<_>>(),
        "cartesian_suffix_trace":cartesian_suffix_trace,"cartesian_suffix_tight":cartesian_suffix_tight,"auxiliary_layout":auxiliary_layout.map(|(bytes,words,max_steps)|json!({"bytes":bytes,"log_words_per_record":words,"max_steps":max_steps})),
        "cartesian_suffix_branch":cartesian_suffix_branch,"cartesian_suffix_f64":cartesian_suffix_f64,"branch_reference":branch_reference,"required_features":format!("{features:?}"),
        "cartesian_composite_f64":cartesian_composite_f64,"cartesian_composite_reference":cartesian_composite_reference,
        "diagnostic_bytes_per_record":32,"steps":prepared.steps.iter().zip(&step_modules).map(|(s,&module_index)|json!({"module_index":module_index,"entry":s.entry,"dispatch":s.dispatch,"repetitions":s.repetitions})).collect::<Vec<_>>(),
        "shader_module_creation_seconds":module_seconds,"pipeline_creation":pipeline_times,
        "header":header,"expected_header":fixture.control,"header_matches":header_matches,"progress_words":words(&progress_raw),"raw_status_counts":statuses,"samples":samples,
        "strictly_increasing_unique_timestamp_pairs":if timed{Some(timestamps_valid)}else{None},"iterations":iterations,
        "timing_scope":if original_clearance || cartesian_composite_full { "Entire fixture-owned initialization, trace, fallback and validation inside one timestamped compute pass. No pre-pass progress clear. Readbacks and pipeline compilation are excluded." } else { "Entire initialization, all continuation slices and finalization inside one timestamped compute pass. Progress reset is encoded immediately before the pass. Readbacks and pipeline compilation are excluded." },"fixture":fixture.cases,
    });
    report["cartesian_composite_full"] = json!(cartesian_composite_full);
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        header_matches,
        "Incomplete or wrong terminal header; raw evidence retained"
    );
    if !reference {
        assert_eq!(
            words(&progress_raw)[..2],
            [0, fixture.record_count],
            "Incomplete continuation progress; raw evidence retained"
        );
    }
    assert!(timestamps_valid, "Invalid timing; raw evidence retained");
    println!("Wrote {prefix}; independent comparison required.");
}
