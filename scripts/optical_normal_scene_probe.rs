//! Original MIT. Mechanical host for math-owned scene and curve fixtures.
//! This does not define ray physics, contact predicates or fixture geometry.
#[path = "optical_normal_scene_fixture.rs"]
mod fixture;
#[path = "optical_normal_scene_loop_policy.rs"]
mod loop_policy;
#[path = "optical_shader_reachability.rs"]
mod reachability;
#[path = "optical_gpu_runtime.rs"]
mod runtime;
#[path = "optical_primary_termination.rs"]
mod termination;
use serde_json::json;
use std::{borrow::Cow, collections::BTreeMap, time::Instant};
use wgpu::util::DeviceExt;

fn words(bytes: &[u8]) -> Vec<u32> {
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut name = None;
    let mut validation_only = false;
    let mut full = false;
    let mut timed = false;
    let mut reference = false;
    let mut reference_reuse = false;
    let mut reference_carry = false;
    let mut curves = false;
    let mut cartesian_curves = false;
    let mut cartesian_covered = false;
    let mut cartesian_stages = false;
    let mut positive_contacts = false;
    let mut positive_stages = false;
    let mut positive_chronology = false;
    let mut prune_unused = false;
    let mut uniform_loops = false;
    let mut finite_loops = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" if name.is_none() => name = Some(args.next().expect("--name needs a value")),
            "--validate-only" if !validation_only => validation_only = true,
            "--full" if !full => full = true,
            "--time" if !timed => timed = true,
            "--reference" if !reference => reference = true,
            "--reference-reuse" if !reference_reuse => reference_reuse = true,
            "--reference-carry" if !reference_carry => reference_carry = true,
            "--curves" if !curves => curves = true,
            "--cartesian-curves" if !cartesian_curves => cartesian_curves = true,
            "--cartesian-covered" if !cartesian_covered => cartesian_covered = true,
            "--cartesian-stages" if !cartesian_stages => cartesian_stages = true,
            "--positive-contacts" if !positive_contacts => positive_contacts = true,
            "--positive-stages" if !positive_stages => positive_stages = true,
            "--positive-chronology" if !positive_chronology => positive_chronology = true,
            "--prune-unused" if !prune_unused => prune_unused = true,
            "--uniform-loops" if !uniform_loops => uniform_loops = true,
            "--finite-loops" if !finite_loops => finite_loops = true,
            _ => panic!("Unknown or repeated argument: {arg}"),
        }
    }
    assert!(!timed || full, "--time requires --full");
    let interval_fixture = curves
        || cartesian_curves
        || cartesian_covered
        || cartesian_stages
        || positive_contacts
        || positive_stages
        || positive_chronology;
    assert!(
        usize::from(curves)
            + usize::from(cartesian_curves)
            + usize::from(cartesian_covered)
            + usize::from(cartesian_stages)
            + usize::from(positive_contacts)
            + usize::from(positive_stages)
            + usize::from(positive_chronology)
            <= 1,
        "Choose one interval fixture"
    );
    assert!(
        !interval_fixture || !(full || timed || reference || reference_reuse || reference_carry),
        "--curves is an isolated interval diagnostic"
    );
    assert!(
        usize::from(reference) + usize::from(reference_reuse) + usize::from(reference_carry) <= 1,
        "Choose one reference implementation"
    );
    assert!(
        !uniform_loops || (interval_fixture && prune_unused),
        "--uniform-loops requires an isolated interval fixture and --prune-unused"
    );
    assert!(
        !finite_loops || (reference && full && !prune_unused && !uniform_loops),
        "--finite-loops requires the unchanged --reference --full fixture"
    );
    let name = name.unwrap_or_else(|| "optical-normal-scene".into());
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    );
    let prefix = format!("artifacts/{name}");
    for suffix in [
        ".wgsl",
        "-original.wgsl",
        "-reachability.json",
        "-loop-policy.json",
        "-termination.json",
        "-primary.bin",
        "-field.bin",
        "-geometry.bin",
        "-frames.bin",
        "-certificates.bin",
        "-curves.bin",
        "-readback.bin",
        ".json",
    ] {
        assert!(
            validation_only || !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve existing evidence"
        );
    }
    let (mut fixture, curve_inputs) = if positive_chronology {
        let fixture = fixture::build_positive_chronology();
        (fixture.scene, Some(fixture.curves))
    } else if positive_stages {
        let fixture = fixture::build_positive_stages();
        (fixture.scene, Some(fixture.curves))
    } else if positive_contacts {
        let fixture = fixture::build_positive_contacts();
        (fixture.scene, Some(fixture.curves))
    } else if cartesian_stages {
        let fixture = fixture::build_cartesian_stages();
        (fixture.scene, Some(fixture.curves))
    } else if cartesian_covered {
        let fixture = fixture::build_cartesian_covered();
        (fixture.scene, Some(fixture.curves))
    } else if cartesian_curves {
        let fixture = fixture::build_cartesian_curves();
        (fixture.scene, Some(fixture.curves))
    } else if curves {
        let fixture = fixture::build_curves();
        (fixture.scene, Some(fixture.curves))
    } else if reference_carry {
        (fixture::build_reference_carry(!full), None)
    } else if reference_reuse {
        (fixture::build_reference_reuse(!full), None)
    } else if reference {
        (fixture::build_reference(!full), None)
    } else {
        (fixture::build(!full), None)
    };
    let loop_report = if uniform_loops {
        Some(loop_policy::apply(
            &mut fixture.shader,
            &mut fixture.primary_uniform,
        ))
    } else {
        None
    };
    if let Some(report) = &loop_report {
        std::fs::write(
            format!("{prefix}-loop-policy.json"),
            serde_json::to_vec_pretty(report).unwrap(),
        )
        .unwrap();
    }
    let reachability_report = if prune_unused {
        let (pruned, report) = reachability::prune(&fixture.shader);
        if !validation_only {
            std::fs::write(format!("{prefix}-original.wgsl"), &fixture.shader).unwrap();
        }
        std::fs::write(
            format!("{prefix}-reachability.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!(
            "Reachability packaging: {} -> {} source bytes; retained function bodies unchanged.",
            fixture.shader.len(),
            pruned.len()
        );
        fixture.shader = pruned;
        Some(report)
    } else {
        None
    };
    let termination_report = if finite_loops {
        let report = termination::audit(
            &fixture.shader,
            &fixture.primary_uniform,
            fixture.width,
            fixture.height,
            fixture.record_count,
            fixture.dispatch,
        );
        std::fs::write(
            format!("{prefix}-termination.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        Some(report)
    } else {
        None
    };
    eprintln!("Scene fixture prepared; validating input layout and WGSL.");
    assert!(fixture.width > 0 && fixture.height > 0 && fixture.record_count > 0);
    assert_eq!(fixture.control[..2], [fixture.width, fixture.height]);
    if full {
        assert_eq!(
            fixture.record_count,
            fixture.width.checked_mul(fixture.height).unwrap()
        );
    }
    assert_eq!(fixture.sample_pixels.len(), fixture.sample_indices.len());
    assert!(
        fixture
            .sample_pixels
            .iter()
            .all(|pixel| pixel[0] < fixture.width && pixel[1] < fixture.height)
    );
    assert!(
        fixture
            .sample_indices
            .iter()
            .all(|&i| i < fixture.record_count)
    );
    assert_eq!(
        fixture.terminal_bytes,
        u64::from(fixture.record_count)
            .checked_mul(48)
            .unwrap()
            .checked_add(16)
            .unwrap()
    );
    let diagnostic_bytes = fixture
        .output_bytes
        .checked_sub(fixture.terminal_bytes)
        .unwrap();
    assert!(
        diagnostic_bytes == 0
            || diagnostic_bytes == u64::from(fixture.record_count) * 16
            || diagnostic_bytes == u64::from(fixture.record_count) * 32
            || diagnostic_bytes == u64::from(fixture.record_count) * 48
    );
    let diagnostic_stride =
        usize::try_from(diagnostic_bytes / u64::from(fixture.record_count)).unwrap();
    let mut inputs = vec![
        (0, "primary", &fixture.primary_uniform),
        (1, "geometry", &fixture.geometry),
        (2, "frames", &fixture.frames),
        (4, "field", &fixture.field_uniform),
        (5, "certificates", &fixture.certificates),
    ];
    if let Some(curve_inputs) = &curve_inputs {
        inputs.push((6, "curves", curve_inputs));
    }
    assert!(
        inputs
            .iter()
            .all(|(_, _, bytes)| !bytes.is_empty() && bytes.len().is_multiple_of(16))
    );
    if !validation_only {
        std::fs::write(format!("{prefix}.wgsl"), &fixture.shader).unwrap();
        for &(_, name, bytes) in &inputs {
            std::fs::write(format!("{prefix}-{name}.bin"), bytes).unwrap();
        }
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
        println!("Scene fixture passed offline Naga validation; no GPU execution.");
        return;
    }
    eprintln!("Offline WGSL validation passed; requesting Vulkan device.");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = runtime::block(instance.request_adapter(&Default::default())).unwrap();
    let features = if timed {
        wgpu::Features::TIMESTAMP_QUERY
    } else {
        wgpu::Features::empty()
    };
    assert!(adapter.features().contains(features));
    let (device, queue) = runtime::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    runtime::validate_dispatch(&device, fixture.dispatch);
    eprintln!("Device ready; preparing explicit bindings and buffers.");
    assert!(fixture.output_bytes <= device.limits().max_storage_buffer_binding_size);
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("canonical scene fixture"),
        entries: &(0..if interval_fixture { 7 } else { 6 })
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: match binding {
                        0 | 4 => wgpu::BufferBindingType::Uniform,
                        _ => wgpu::BufferBindingType::Storage {
                            read_only: binding != 3,
                        },
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
        .map(|&(binding, name, bytes)| {
            let usage = if binding == 0 || binding == 4 {
                wgpu::BufferUsages::UNIFORM
            } else {
                wgpu::BufferUsages::STORAGE
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: bytes,
                usage,
            });
            (binding, buffer)
        })
        .collect();
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene terminals and optional diagnostics"),
        size: fixture.output_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut entries: Vec<_> = buffers
        .iter()
        .map(|(binding, buffer)| wgpu::BindGroupEntry {
            binding: *binding,
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 3,
        resource: output.as_entire_binding(),
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("current canonical scene inputs"),
        layout: &layout,
        entries: &entries,
    });
    let module_start = Instant::now();
    let descriptor = wgpu::ShaderModuleDescriptor {
        label: Some("candidate scene"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&fixture.shader)),
    };
    let module = if finite_loops {
        assert!(termination_report.is_some(), "Missing finite-loop proof");
        let mut checks = wgpu::ShaderRuntimeChecks::checked();
        checks.force_loop_bounding = false;
        // SAFETY: termination::audit admitted these exact shader/uniform bytes
        // and this dispatch before device creation. Its independently reviewed
        // finite-loop proof is specific to this fixture. All other runtime
        // checks, Naga validation, shader arithmetic and buffers are unchanged.
        unsafe { device.create_shader_module_trusted(descriptor, checks) }
    } else {
        device.create_shader_module(descriptor)
    };
    let module_seconds = module_start.elapsed().as_secs_f64();
    eprintln!("Shader module ready after {module_seconds:.3}s; creating compute pipeline.");
    let pipeline_start = Instant::now();
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("normal_scene"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("normal_scene"),
        compilation_options: Default::default(),
        cache: None,
    });
    let pipeline_seconds = pipeline_start.elapsed().as_secs_f64();
    eprintln!("Compute pipeline ready after {pipeline_seconds:.3}s; dispatching scene fixture.");
    let (iterations, valid_timestamps) = if timed {
        runtime::timed_dispatch(&device, &queue, &pipeline, &bind, fixture.dispatch)
    } else {
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("untimed scene validation"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(
                fixture.dispatch[0],
                fixture.dispatch[1],
                fixture.dispatch[2],
            );
        }
        queue.submit([encoder.finish()]);
        (Vec::new(), true)
    };
    eprintln!("Dispatch submitted; waiting for complete output readback.");
    let raw = runtime::copy_read(&device, &queue, &output, fixture.output_bytes);
    eprintln!("Readback complete; preserving raw bytes and report.");
    std::fs::write(format!("{prefix}-readback.bin"), &raw).unwrap();
    assert_eq!(raw.len() as u64, fixture.output_bytes);
    let header = words(&raw[..16]);
    let header_matches = header == fixture.control;
    let terminal_end = usize::try_from(fixture.terminal_bytes).unwrap();
    let mut status_counts = BTreeMap::<u32, usize>::new();
    for record in raw[16..terminal_end].chunks_exact(48) {
        *status_counts
            .entry(u32::from_le_bytes(record[28..32].try_into().unwrap()))
            .or_default() += 1;
    }
    let samples: Vec<_> = fixture.sample_indices.iter().zip(&fixture.sample_pixels).map(|(&index, pixel)| {
        let start = 16 + index as usize * 48;
        let diagnostics = if diagnostic_bytes > 0 {
            let start = terminal_end + index as usize * diagnostic_stride;
            words(&raw[start..start + diagnostic_stride])
        } else { Vec::new() };
        json!({"pixel":pixel,"record_index":index,"terminal_words":words(&raw[start..start + 48]),"diagnostic_words":diagnostics})
    }).collect();
    let report = json!({
        "scope":"Headless candidate scene dispatch. All raw buffers retained for independent review. Timing excludes stellar solves, shading, UI and presentation; no native application FPS claim.",
        "adapter":format!("{:?}", adapter.get_info()),
        "selected":!full && !interval_fixture,"timed":timed,"reference":reference,"reference_reuse":reference_reuse,"reference_carry":reference_carry,"curves":curves,"cartesian_curves":cartesian_curves || cartesian_stages || positive_contacts || positive_stages || positive_chronology,"cartesian_covered":cartesian_covered || cartesian_stages || positive_contacts || positive_stages || positive_chronology,"cartesian_stages":cartesian_stages,"positive_contacts":positive_contacts || positive_stages || positive_chronology,"positive_stages":positive_stages,"positive_chronology":positive_chronology,
        "prune_unused":prune_unused,"reachability":reachability_report,
        "uniform_loops":uniform_loops,"loop_policy":loop_report,
        "finite_loops":finite_loops,"termination":termination_report,
        "dimensions":[fixture.width,fixture.height],
        "coordinate_scope":if interval_fixture {"interval record storage, not camera pixels"} else {"camera pixels"},
        "record_count":fixture.record_count,"dispatch":fixture.dispatch,
        "output_bytes":fixture.output_bytes,"terminal_bytes":fixture.terminal_bytes,
        "diagnostic_bytes_per_record":diagnostic_stride,
        "header":header,"expected_header":fixture.control,"header_matches":header_matches,
        "raw_status_counts":status_counts,"samples":samples,
        "shader_module_creation_seconds":module_seconds,"pipeline_creation_seconds":pipeline_seconds,
        "strictly_increasing_unique_timestamp_pairs":if timed {Some(valid_timestamps)} else {None},
        "iterations":iterations,"fixture":fixture.cases,
    });
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        header_matches,
        "Unexpected terminal header; raw evidence retained"
    );
    assert!(
        valid_timestamps,
        "Invalid timestamps; raw evidence retained"
    );
    println!("Wrote {prefix}.json; independent numerical review required.");
}
