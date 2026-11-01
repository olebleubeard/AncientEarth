//! Original MIT. Full native-pixel solver work distribution, not a timing claim.
//! Retained assembled shaders and immutable input buffers define each replay.
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
    let w = Waker::from(Arc::new(Noop));
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(future);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut c) {
            return v;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, buffer: &wgpu::Buffer, size: u64) -> Vec<u8> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut e = device.create_command_encoder(&Default::default());
    e.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
    queue.submit([e.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    let result = staging.slice(..).get_mapped_range().unwrap().to_vec();
    staging.unmap();
    result
}
fn once(source: &mut String, old: &str, new: &str) {
    assert_eq!(
        source.matches(old).count(),
        1,
        "changed retained source:{old}"
    );
    *source = source.replacen(old, new, 1);
}
fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .collect()
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "shaderpath label");
    let label = &args[2];
    assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let prefix = format!("artifacts/optical-frame-counters-{label}");
    assert!(!std::path::Path::new(&format!("{prefix}.json")).exists());
    let original = std::fs::read_to_string(&args[1])
        .unwrap()
        .replace("\r\n", "\n");
    assert!(original.contains("@workgroup_size(32) fn performance("));
    let mut counted = original.clone();
    counted.push_str("\nvar<private> full_frame_counts:vec4<u32>;\n@group(0) @binding(5) var<storage,read_write> counts:array<vec4<u32>>;\n");
    once(
        &mut counted,
        "fn geometry(p:vec3<f32>)->vec4<f32>{",
        "fn geometry(p:vec3<f32>)->vec4<f32>{full_frame_counts.z+=1u;",
    );
    once(
        &mut counted,
        "fn dopri_delta(s:State,step:f32,branch:vec3<f32>,g0:vec4<f32>)->DpStep{",
        "fn dopri_delta(s:State,step:f32,branch:vec3<f32>,g0:vec4<f32>)->DpStep{full_frame_counts.y+=1u;",
    );
    once(
        &mut counted,
        "for(var i=0;i<4096;i+=1){",
        "for(var i=0;i<4096;i+=1){full_frame_counts.x+=1u;",
    );
    let face = counted.find("fn face_event(").unwrap();
    let brace = face + counted[face..].find('{').unwrap();
    counted.insert_str(brace + 1, "full_frame_counts.w+=1u;");
    once(
        &mut counted,
        "let r=trace(direction);",
        "let r=trace(direction);counts[id.x]=full_frame_counts;",
    );
    std::fs::write(format!("{prefix}.wgsl"), &counted).unwrap();
    let input_prefix = "artifacts/optical-requested-stellar-sparse-chain";
    let mut uniform = std::fs::read(format!("{input_prefix}-uniform.bin")).unwrap();
    assert!(uniform.len() >= 136 * 16);
    uniform.truncate(136 * 16);
    let view = std::fs::read(format!("{input_prefix}-view.bin")).unwrap();
    assert_eq!(view.len(), 80);
    let vw = words(&view);
    assert_eq!(
        [f32::from_bits(vw[16]), f32::from_bits(vw[17])],
        [1920.0, 1080.0]
    );
    assert_eq!(f32::from_bits(vw[14]), 0.0);
    let geometry = std::fs::read(format!("{input_prefix}-geometry.bin")).unwrap();
    let frames = std::fs::read(format!("{input_prefix}-frames.bin")).unwrap();
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
    let make = |data: &[u8], uniform: bool| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: data,
            usage: if uniform {
                wgpu::BufferUsages::UNIFORM
            } else {
                wgpu::BufferUsages::STORAGE
            },
        })
    };
    let settings = make(&uniform, true);
    let rays = make(&view, false);
    let geo = make(&geometry, false);
    let frame = make(&frames, false);
    let output_size = 1920 * 1080 * 48;
    let count_size = 1920 * 1080 * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let counter = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: count_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let all = [&settings, &rays, &output, &geo, &frame, &counter];
    let mut outputs = Vec::new();
    for (i, shader) in [original, counted].into_iter().enumerate() {
        eprintln!(
            "compile {}",
            if i == 0 { "retained" } else { "instrumented" }
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("performance"),
            compilation_options: Default::default(),
            cache: None,
        });
        let entries = all[..5 + i]
            .iter()
            .enumerate()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: b.as_entire_binding(),
            })
            .collect::<Vec<_>>();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut e = device.create_command_encoder(&Default::default());
        {
            let mut p = e.begin_compute_pass(&Default::default());
            p.set_pipeline(&pipeline);
            p.set_bind_group(0, &bind, &[]);
            p.dispatch_workgroups(64800, 1, 1);
        }
        let submission = queue.submit([e.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(90)),
            })
            .unwrap();
        let raw = read(&device, &queue, &output, output_size);
        std::fs::write(
            format!(
                "{prefix}-{}.bin",
                if i == 0 { "retained" } else { "instrumented" }
            ),
            &raw,
        )
        .unwrap();
        outputs.push(words(&raw));
    }
    let raw = read(&device, &queue, &counter, count_size);
    std::fs::write(format!("{prefix}-counts.bin"), &raw).unwrap();
    let count_words = words(&raw);
    let mut differences = Vec::new();
    let mut changed_status = 0;
    let mut changed_owner = 0;
    let mut histograms = vec![vec![std::collections::BTreeMap::<u32, u64>::new(); 4]; 4];
    let mut status_counts = [0u64; 4];
    let mut exhausted = Vec::new();
    for pixel in 0..1920 * 1080 {
        let a = &outputs[0][pixel * 12..pixel * 12 + 12];
        let b = &outputs[1][pixel * 12..pixel * 12 + 12];
        let c = &count_words[pixel * 4..pixel * 4 + 4];
        if a != b {
            differences
                .push(json!({"pixel":[pixel%1920,pixel/1920],"retained":a,"instrumented":b}));
        }
        changed_status += usize::from(a[7] != b[7]);
        changed_owner += usize::from(a[11] != b[11]);
        let status = b[7] as usize;
        assert!(status < 4);
        status_counts[status] += 1;
        for k in 0..4 {
            *histograms[status][k].entry(c[k]).or_default() += 1;
        }
        if c[0] >= 4096 {
            exhausted.push(json!({"pixel":[pixel%1920,pixel/1920],"counts":c,"terminal":b}));
        }
    }
    let summaries=histograms.iter().map(|status|status.iter().map(|hist|{
        let total=hist.values().sum::<u64>();let percentile=|numerator:u64,denominator:u64|{let target=(total*numerator).div_ceil(denominator).max(1);let mut accum=0;for(value,count)in hist{accum+=count;if accum>=target{return *value;}}0};
        json!({"count":total,"sum":hist.iter().map(|(v,c)|u64::from(*v)*c).sum::<u64>(),"median":percentile(1,2),"p90":percentile(9,10),"p99":percentile(99,100),"p999":percentile(999,1000),"max":hist.keys().next_back().copied().unwrap_or(0),"histogram":hist})
    }).collect::<Vec<_>>()).collect::<Vec<_>>();
    let report = json!({"scope":"Counter-only full1920x1080 replay of a retained numerical diagnostic. Extra private counters may affect compiler rounding/resource use; every terminalword is independently compared against the retained uninstrumented shader on identical saved inputs. No timing or runtime-adoption claim. Counter order:attempts,DP evaluations,inverse geometry evaluations,face_event calls. Status order:sky,surface,unresolved,aperture.","adapter":format!("{:?}",adapter.get_info()),"retained_shader":args[1],"input_prefix":input_prefix,"status_counts":status_counts,"counter_summaries_by_status":summaries,"attempt_budget_exhausted":exhausted,"terminal_difference_pixels":differences.len(),"status_differences":changed_status,"owner_differences":changed_owner,"all_terminal_differences":differences});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}
