//! Original MIT. Mechanical scheduling/readback harness for the math-owned
//! requested-star fixture. This is not the application's rendering pipeline.
#[path = "optical_requested_stellar_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;

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

fn copy_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
) -> Vec<u8> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("untimed evidence readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit([encoder.finish()]);
    read(device, &staging)
}

fn layout(device: &wgpu::Device, stellar: bool, consumer: bool) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(if stellar {
            "requested star fixture"
        } else {
            "primary fixture"
        }),
        entries: &(0..if consumer { 6 } else { 5 })
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: binding != 2 && binding != 5 && !(stellar && binding == 4),
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>(),
    })
}

fn bindings(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: &[&wgpu::Buffer],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &buffers
            .iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>(),
    })
}

struct Stage {
    name: &'static str,
    pipeline: wgpu::ComputePipeline,
    dispatch: u32,
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    compilation_seconds: f64,
}

fn main() {
    assert!(std::env::args().any(|a| a == "--atmosphere-only"));
    let args: Vec<_> = std::env::args().collect();
    let consumer_check = args.iter().any(|arg| arg == "--consumer-check");
    let chain_timing = args.iter().any(|arg| arg == "--chain-timing");
    assert!(!chain_timing || consumer_check);
    let compact = args.iter().any(|arg| arg == "--compact");
    let name = args
        .iter()
        .position(|a| a == "--name")
        .map_or("requested-stellar", |i| args[i + 1].as_str());
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    );
    let prefix = format!("artifacts/{name}");
    for extension in [
        ".json",
        "-primary.wgsl",
        "-stellar.wgsl",
        "-requests.bin",
        "-arrivals.bin",
        "-uniform.bin",
        "-view.bin",
        "-catalogue.bin",
        "-geometry.bin",
        "-frames.bin",
        "-consumer.bin",
        "-direct-pixels.bin",
        "-requested-pixels.bin",
        "-chain-direct-pixels.bin",
        "-chain-requested-pixels.bin",
        "-chain-arrivals.bin",
    ] {
        assert!(
            !std::path::Path::new(&format!("{prefix}{extension}")).exists(),
            "Preserve earlier evidence"
        );
    }
    let data = if compact {
        fixture::compact_fixture()
    } else {
        fixture::fixture()
    };
    let request_size = if compact {
        fixture::compact_request_bytes()
    } else {
        fixture::request_bytes()
    };
    std::fs::write(
        format!("{prefix}-uniform.bin"),
        fixture::bytes(&data.settings),
    )
    .unwrap();
    std::fs::write(format!("{prefix}-view.bin"), fixture::bytes(&data.view)).unwrap();
    std::fs::write(
        format!("{prefix}-catalogue.bin"),
        fixture::bytes(&data.catalogue),
    )
    .unwrap();
    std::fs::write(
        format!("{prefix}-geometry.bin"),
        fixture::bytes(&data.geometry),
    )
    .unwrap();
    std::fs::write(format!("{prefix}-frames.bin"), fixture::bytes(&data.frames)).unwrap();
    std::fs::write(format!("{prefix}-primary.wgsl"), &data.primary_shader).unwrap();
    std::fs::write(format!("{prefix}-stellar.wgsl"), &data.stellar_shader).unwrap();
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
    let make = |name: &str, values: &[[f32; 4]], uniform| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(name),
            contents: &fixture::bytes(values),
            usage: if uniform {
                wgpu::BufferUsages::UNIFORM
            } else {
                wgpu::BufferUsages::STORAGE
            },
        })
    };
    let uniform = make(
        "fixed weather/geometry/identity rotation",
        &data.settings,
        true,
    );
    let catalogue = make("original catalogue", &data.catalogue, false);
    let view = make("unchanged native-pixel camera", &data.view, false);
    let geometry = make("canonical scene", &data.geometry, false);
    let frames = make("scene frames", &data.frames, false);
    let output = |name: &str, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let terminals = output(
        "current physical primary terminals",
        fixture::terminal_bytes(),
    );
    let arrivals = output("all current arrival records", fixture::arrival_bytes());
    let requests = output("current star request mask", request_size);
    let primary_bind_layout = layout(&device, false, false);
    let stellar_bind_layout = layout(&device, true, false);
    let pipeline_layout = |binding_layout: &wgpu::BindGroupLayout| {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(binding_layout)],
            immediate_size: 0,
        })
    };
    let primary_layout = pipeline_layout(&primary_bind_layout);
    let stellar_layout = pipeline_layout(&stellar_bind_layout);
    let primary_bind = bindings(
        &device,
        &primary_bind_layout,
        &[&uniform, &view, &terminals, &geometry, &frames],
    );
    let stellar_bind = bindings(
        &device,
        &stellar_bind_layout,
        &[&uniform, &catalogue, &arrivals, &terminals, &requests],
    );
    let module = |label, shader: String| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
        })
    };
    let primary_module = module("original midpoint primary fixture", data.primary_shader);
    let stellar_module = module("requested arrival diagnostic", data.stellar_shader);
    let pipeline =
        |module: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, entry: &'static str| {
            let started = std::time::Instant::now();
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(layout),
                module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            (pipeline, started.elapsed().as_secs_f64())
        };
    let (primary, primary_compilation_seconds) =
        pipeline(&primary_module, &primary_layout, "performance");
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("untimed physical fixture"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&primary);
        pass.set_bind_group(0, &primary_bind, &[]);
        pass.dispatch_workgroups(
            (fixture::WIDTH * fixture::HEIGHT).div_ceil(fixture::GROUP),
            1,
            1,
        );
    }
    let submission = queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    let primary_header: Vec<_> = copy_read(&device, &queue, &terminals, 16)
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    assert_eq!(primary_header, fixture::CONTROL);
    let stages: Vec<_> = [
        (
            "clear_stellar_requests",
            fixture::STAR_COUNT.div_ceil(fixture::GROUP),
        ),
        (
            "gather_stellar_requests",
            (fixture::WIDTH * fixture::HEIGHT).div_ceil(fixture::GROUP),
        ),
        (
            "requested_stellar_table",
            fixture::STAR_COUNT.div_ceil(fixture::GROUP),
        ),
    ]
    .into_iter()
    .map(|(name, dispatch)| {
        let (pipeline, compilation_seconds) = pipeline(&stellar_module, &stellar_layout, name);
        Stage {
            name,
            pipeline,
            dispatch,
            compilation_seconds,
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some(name),
                ty: wgpu::QueryType::Timestamp,
                count: 16,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(name),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(name),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        }
    })
    .collect();
    let mut samples = Vec::new();
    let mut previous = 0;
    // Deliberately dirty prior request/arrival storage. The real clear/solve
    // sequence must overwrite every record, including unrequested failures.
    queue.write_buffer(&arrivals, 0, &vec![0xff; fixture::arrival_bytes() as usize]);
    queue.write_buffer(&requests, 0, &vec![0xff; request_size as usize]);
    for iteration in 0..8 {
        let mut encoder = device.create_command_encoder(&Default::default());
        for stage in &stages {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(stage.name),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &stage.queries,
                    beginning_of_pass_write_index: Some(2 * iteration),
                    end_of_pass_write_index: Some(2 * iteration + 1),
                }),
            });
            pass.set_pipeline(&stage.pipeline);
            pass.set_bind_group(0, &stellar_bind, &[]);
            pass.dispatch_workgroups(stage.dispatch, 1, 1);
        }
        for stage in &stages {
            encoder.resolve_query_set(
                &stage.queries,
                2 * iteration..2 * iteration + 2,
                &stage.resolve,
                0,
            );
        }
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(60)),
            })
            .unwrap();
        // Match the reviewed serial-query policy: completed resolve submission,
        // then a distinct readback-copy submission. Every pass owns unique slots.
        let mut copy = device.create_command_encoder(&Default::default());
        for stage in &stages {
            copy.copy_buffer_to_buffer(&stage.resolve, 0, &stage.readback, 0, 16);
        }
        queue.submit([copy.finish()]);
        let mut measured = Vec::new();
        for stage in &stages {
            let ticks: Vec<_> = read(&device, &stage.readback)
                .chunks_exact(8)
                .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
                .collect();
            assert!(
                ticks[1] > ticks[0] && ticks[0] >= previous,
                "Repeated/nonmonotonic GPU timestamps"
            );
            previous = ticks[1];
            let seconds =
                (ticks[1] - ticks[0]) as f64 * f64::from(queue.get_timestamp_period()) * 1e-9;
            measured.push(json!({"stage":stage.name,"raw_timestamps":ticks,"timestamp_period_ns":queue.get_timestamp_period(),"gpu_seconds":seconds}));
        }
        samples.push(json!({"iteration":iteration,"warmup":iteration<3,"stages":measured}));
        eprintln!("requested stellar iteration {iteration} completed");
    }
    let arrival_bytes = copy_read(&device, &queue, &arrivals, fixture::arrival_bytes());
    let request_bytes = copy_read(&device, &queue, &requests, request_size);
    std::fs::write(format!("{prefix}-arrivals.bin"), &arrival_bytes).unwrap();
    std::fs::write(format!("{prefix}-requests.bin"), &request_bytes).unwrap();
    let words: Vec<_> = arrival_bytes
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let requested: Vec<_> = request_bytes
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let compact_queue = if compact {
        let count = requested[fixture::STAR_COUNT as usize] as usize;
        assert!(count <= fixture::STAR_COUNT as usize);
        let active =
            &requested[fixture::STAR_COUNT as usize + 1..fixture::STAR_COUNT as usize + 1 + count];
        let mut offsets = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for offset in active {
            assert!(*offset >= 6144 && (*offset - 6144) % 2 == 0);
            assert!((*offset as usize) + 1 < data.catalogue.len());
            let id = data.catalogue[*offset as usize + 1][3] as usize;
            assert!(id < fixture::STAR_COUNT as usize && requested[id] == 1);
            assert!(offsets.insert(*offset) && ids.insert(id));
        }
        assert_eq!(
            count,
            requested[..fixture::STAR_COUNT as usize]
                .iter()
                .filter(|v| **v == 1)
                .count()
        );
        json!({"count":count,"packed_catalogue_offsets":active,"unique_ids":ids})
    } else {
        serde_json::Value::Null
    };
    assert_eq!(&words[..4], &fixture::CONTROL);
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read("artifacts/optical-stellar-table-probe.json").unwrap(),
    )
    .unwrap();
    let reference = oracle["arrivals"].as_array().unwrap();
    assert_eq!(reference.len(), fixture::STAR_COUNT as usize);
    let mut differences = Vec::new();
    let records: Vec<_> = words[4..].chunks_exact(4).enumerate().map(|(id, record)| {
        assert_eq!(reference[id]["stable_id"].as_u64().unwrap(), id as u64);
        let prior: Vec<_> = reference[id]["raw_words"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
        let matches = match record[3] {
            0 => requested[id] == 0 && record == [0; 4],
            1 => requested[id] == 1 && prior[3] == 0 && record[..3] == prior[..3],
            2 => requested[id] == 1 && prior[3] == 1.0_f32.to_bits() && record[..3] == prior[..3],
            _ => false,
        };
        if !matches { differences.push(json!({"stable_id":id,"request":requested[id],"record":record,"all_star_reference":prior})); }
        json!({"stable_id":id,"request":requested[id],"raw_words":record})
    }).collect();
    let consumer_report = if consumer_check {
        let consumer_layout = layout(&device, true, true);
        let consumer_pipeline_layout = pipeline_layout(&consumer_layout);
        let consumer_output = output("untimed consumer checks", fixture::consumer_bytes());
        let consumer_bind = bindings(
            &device,
            &consumer_layout,
            &[
                &uniform,
                &catalogue,
                &arrivals,
                &terminals,
                &requests,
                &consumer_output,
            ],
        );
        let (consumer_pipeline, _) = pipeline(
            &stellar_module,
            &consumer_pipeline_layout,
            "stellar_consumer_checks",
        );
        // Ensure state1 is exercised even if every requested fixture star solves.
        // Its retained original result must already be an explicit failed solve.
        let injected_failure_id = if words[4..].chunks_exact(4).any(|r| r[3] == 1) {
            None
        } else {
            let id = reference
                .iter()
                .position(|r| r["raw_words"][3].as_u64() == Some(0))
                .expect("reference contains an explicit failed solve");
            let mut record = [0u8; 16];
            record[12..].copy_from_slice(&1u32.to_le_bytes());
            queue.write_buffer(&arrivals, 16 + id as u64 * 16, &record);
            Some(id)
        };
        let run_consumer = || {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("untimed consumer state/header check"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&consumer_pipeline);
                pass.set_bind_group(0, &consumer_bind, &[]);
                pass.dispatch_workgroups(fixture::STAR_COUNT.div_ceil(fixture::GROUP), 1, 1);
            }
            queue.submit([encoder.finish()]);
            copy_read(&device, &queue, &consumer_output, fixture::consumer_bytes())
        };
        let checked = run_consumer();
        std::fs::write(format!("{prefix}-consumer.bin"), &checked).unwrap();
        let checked_words: Vec<_> = checked
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let consumer_differences: Vec<_> = checked_words
            .chunks_exact(12)
            .enumerate()
            .filter(|(_, row)| row[..4] != row[4..8])
            .map(|(id, row)| json!({"stable_id":id,"words":row}))
            .collect();
        let states: std::collections::BTreeSet<_> =
            checked_words.chunks_exact(12).map(|row| row[11]).collect();
        assert_eq!(states, [0, 1, 2].into_iter().collect());
        queue.write_buffer(&arrivals, 0, &[0; 16]);
        let wrong_arrival_header = run_consumer();
        let pending = |bytes: &[u8]| {
            bytes
                .chunks_exact(48)
                .all(|row| row[..32].iter().all(|v| *v == 0xff))
        };
        let wrong_arrival_header_pending = pending(&wrong_arrival_header);
        queue.write_buffer(&arrivals, 0, &arrival_bytes);
        queue.write_buffer(&terminals, 0, &[0; 16]);
        let wrong_primary_header = run_consumer();
        let wrong_primary_header_pending = pending(&wrong_primary_header);
        queue.write_buffer(&terminals, 0, &arrival_bytes[..16]);
        std::fs::write(
            format!("{prefix}-consumer-wrong-arrival-header.bin"),
            wrong_arrival_header,
        )
        .unwrap();
        std::fs::write(
            format!("{prefix}-consumer-wrong-primary-header.bin"),
            wrong_primary_header,
        )
        .unwrap();
        // One bounded matched pixel pass per implementation. This first probe is
        // correctness/warmup evidence; steady-state timing needs repeated passes.
        let direct_pixels = output("original star pixels", fixture::pixel_color_bytes());
        let requested_pixels = output("requested star pixels", fixture::pixel_color_bytes());
        let pixel_bind = |pixels: &wgpu::Buffer| {
            bindings(
                &device,
                &consumer_layout,
                &[
                    &uniform, &catalogue, &arrivals, &terminals, &requests, pixels,
                ],
            )
        };
        let direct_bind = pixel_bind(&direct_pixels);
        let requested_bind = pixel_bind(&requested_pixels);
        let (direct_pipeline, _) = pipeline(
            &stellar_module,
            &consumer_pipeline_layout,
            "direct_stellar_pixels",
        );
        let (requested_pipeline, _) = pipeline(
            &stellar_module,
            &consumer_pipeline_layout,
            "requested_stellar_pixels",
        );
        let pixel_queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("bounded pixel comparison"),
            ty: wgpu::QueryType::Timestamp,
            count: 4,
        });
        let pixel_resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        for (index, (pipeline, bind)) in [
            (&direct_pipeline, &direct_bind),
            (&requested_pipeline, &requested_bind),
        ]
        .into_iter()
        .enumerate()
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("single pixel-comparison warmup"),
                timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                    query_set: &pixel_queries,
                    beginning_of_pass_write_index: Some(2 * index as u32),
                    end_of_pass_write_index: Some(2 * index as u32 + 1),
                }),
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(
                (fixture::WIDTH * fixture::HEIGHT).div_ceil(fixture::GROUP),
                1,
                1,
            );
        }
        encoder.resolve_query_set(&pixel_queries, 0..4, &pixel_resolve, 0);
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(60)),
            })
            .unwrap();
        let pixel_ticks: Vec<_> = copy_read(&device, &queue, &pixel_resolve, 32)
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert!(
            pixel_ticks[1] > pixel_ticks[0]
                && pixel_ticks[2] >= pixel_ticks[1]
                && pixel_ticks[3] > pixel_ticks[2]
        );
        let direct_bytes = copy_read(
            &device,
            &queue,
            &direct_pixels,
            fixture::pixel_color_bytes(),
        );
        let requested_bytes = copy_read(
            &device,
            &queue,
            &requested_pixels,
            fixture::pixel_color_bytes(),
        );
        std::fs::write(format!("{prefix}-direct-pixels.bin"), &direct_bytes).unwrap();
        std::fs::write(format!("{prefix}-requested-pixels.bin"), &requested_bytes).unwrap();
        let pixel_differences: Vec<_> = direct_bytes
            .chunks_exact(16)
            .zip(requested_bytes.chunks_exact(16))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, _)| index)
            .collect();
        let chain_report = if chain_timing {
            // Fresh query sets isolate this repeated series from all previous
            // probes. Each iteration owns two slots per stage; no slots repeat.
            let timed_stage = |name, pipeline: &wgpu::ComputePipeline, dispatch| Stage {
                name,
                pipeline: pipeline.clone(),
                dispatch,
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some(name),
                    ty: wgpu::QueryType::Timestamp,
                    count: 16,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(name),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(name),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                compilation_seconds: 0.0,
            };
            let pixels_dispatch = (fixture::WIDTH * fixture::HEIGHT).div_ceil(fixture::GROUP);
            let direct = timed_stage("direct_stellar_pixels", &direct_pipeline, pixels_dispatch);
            let cached: Vec<_> = stages
                .iter()
                .map(|stage| timed_stage(stage.name, &stage.pipeline, stage.dispatch))
                .chain(std::iter::once(timed_stage(
                    "requested_stellar_pixels",
                    &requested_pipeline,
                    pixels_dispatch,
                )))
                .collect();
            let mut measured_iterations = Vec::new();
            let mut last_tick = 0;
            for iteration in 0..8 {
                // Alternate whole path ordering, never the dependent cached
                // clear/gather/solve/consume sequence.
                let sequence: Vec<(&Stage, &wgpu::BindGroup)> = if iteration % 2 == 0 {
                    std::iter::once((&direct, &direct_bind))
                        .chain(cached.iter().enumerate().map(|(index, stage)| {
                            (
                                stage,
                                if index == 3 {
                                    &requested_bind
                                } else {
                                    &stellar_bind
                                },
                            )
                        }))
                        .collect()
                } else {
                    cached
                        .iter()
                        .enumerate()
                        .map(|(index, stage)| {
                            (
                                stage,
                                if index == 3 {
                                    &requested_bind
                                } else {
                                    &stellar_bind
                                },
                            )
                        })
                        .chain(std::iter::once((&direct, &direct_bind)))
                        .collect()
                };
                let mut encoder = device.create_command_encoder(&Default::default());
                for (stage, bind) in &sequence {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(stage.name),
                        timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                            query_set: &stage.queries,
                            beginning_of_pass_write_index: Some(2 * iteration),
                            end_of_pass_write_index: Some(2 * iteration + 1),
                        }),
                    });
                    pass.set_pipeline(&stage.pipeline);
                    pass.set_bind_group(0, *bind, &[]);
                    pass.dispatch_workgroups(stage.dispatch, 1, 1);
                }
                for (stage, _) in &sequence {
                    encoder.resolve_query_set(
                        &stage.queries,
                        2 * iteration..2 * iteration + 2,
                        &stage.resolve,
                        0,
                    );
                }
                let submission = queue.submit([encoder.finish()]);
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(submission),
                        timeout: Some(Duration::from_secs(60)),
                    })
                    .unwrap();
                let mut copy = device.create_command_encoder(&Default::default());
                for (stage, _) in &sequence {
                    copy.copy_buffer_to_buffer(&stage.resolve, 0, &stage.readback, 0, 16);
                }
                queue.submit([copy.finish()]);
                let mut measurements = Vec::new();
                for (stage, _) in &sequence {
                    let ticks: Vec<_> = read(&device, &stage.readback)
                        .chunks_exact(8)
                        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
                        .collect();
                    assert!(ticks[1] > ticks[0] && ticks[0] >= last_tick);
                    last_tick = ticks[1];
                    measurements.push(json!({
                        "stage":stage.name,"raw_timestamps":ticks,
                        "timestamp_period_ns":queue.get_timestamp_period(),
                    }));
                }
                measured_iterations.push(json!({
                    "iteration":iteration,"warmup":iteration<3,
                    "direct_first":iteration%2==0,"stages":measurements,
                }));
                eprintln!("stellar full chain iteration {iteration} completed");
            }
            let final_direct = copy_read(
                &device,
                &queue,
                &direct_pixels,
                fixture::pixel_color_bytes(),
            );
            let final_requested = copy_read(
                &device,
                &queue,
                &requested_pixels,
                fixture::pixel_color_bytes(),
            );
            let final_arrivals = copy_read(&device, &queue, &arrivals, fixture::arrival_bytes());
            std::fs::write(format!("{prefix}-chain-direct-pixels.bin"), &final_direct).unwrap();
            std::fs::write(
                format!("{prefix}-chain-requested-pixels.bin"),
                &final_requested,
            )
            .unwrap();
            std::fs::write(format!("{prefix}-chain-arrivals.bin"), &final_arrivals).unwrap();
            json!({
                "scope":"Repeated original pixel composition and complete clear/gather/solve/cached-pixel chain, alternating whole-path order, fixed original primary terminals. Fresh unique query slots and completed resolve before readback. Does not include the primary trace, native application frame pacing or dated sky.",
                "samples":measured_iterations,
                "final_direct_matches_initial":final_direct==direct_bytes,
                "final_requested_matches_direct":final_requested==final_direct,
                "final_arrivals_match_initial":final_arrivals==arrival_bytes,
                "passed":final_direct==direct_bytes && final_requested==final_direct && final_arrivals==arrival_bytes,
            })
        } else {
            serde_json::Value::Null
        };
        json!({
            "scope":"Untimed consumer states and header stress checks; single bounded original/requested pixel warmup with raw GPU timestamps. Synthetic identity-rotation, no-luminous-occluder fixture; not dated sky, native FPS or steady-state savings.",
            "injected_explicit_reference_failure_id":injected_failure_id,"states_exercised":states,
            "arrival_differences":consumer_differences,"wrong_arrival_header_pending":wrong_arrival_header_pending,
            "wrong_primary_header_pending":wrong_primary_header_pending,
            "pixel_warmup_raw_timestamps":pixel_ticks,"timestamp_period_ns":queue.get_timestamp_period(),
            "pixel_difference_indices":pixel_differences,
            "chain_timing":chain_report,
            "passed":consumer_differences.is_empty() && wrong_arrival_header_pending && wrong_primary_header_pending && pixel_differences.is_empty() && (!chain_timing || chain_report["passed"]==true)
        })
    } else {
        serde_json::Value::Null
    };
    let report = json!({
        "scope":"Headless requested-star scheduling diagnostic. One unchanged actual-scene native-resolution primary fixture is generated before timing. Each timed frame is clear then exact candidate gather then requested inverse. Identity catalogue/local rotation and absent luminous-body occluders are explicit fixture conditions. No native frame rate, consumer pixel/radiance equivalence or performance improvement over the original per-pixel inverse is claimed. Unrequested state0 requires the original inverse fallback in a final consumer.",
        "adapter":format!("{:?}",adapter.get_info()),"control":fixture::CONTROL,
        "faces":data.faces,"markers":data.markers,"primary_header":primary_header,
        "primary_compilation_seconds":primary_compilation_seconds,
        "stages":stages.iter().map(|s|json!({"name":s.name,"compilation_seconds":s.compilation_seconds,"dispatch":[s.dispatch,1,1]})).collect::<Vec<_>>(),
        "timestamp_samples":samples,"records":records,"reference_differences":differences,
        "same_frame_records_match_retained_all_star_results":differences.is_empty(),
        "consumer_check":consumer_report,
        "compact_queue":compact_queue,
    });
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    assert!(
        differences.is_empty(),
        "Requested arrivals differ; evidence retained"
    );
    assert!(
        !consumer_check || consumer_report["passed"] == true,
        "Consumer check failed; raw evidence retained"
    );
}
