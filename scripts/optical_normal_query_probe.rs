//! Original MIT. Finite canonical normal-chart field/trial readback only.
//! No runtime registration, source-contact controller or timing claim.
#[allow(dead_code)]
#[path = "optical_normal_coordinates.rs"]
mod coordinates;
use earth_math::{EnuBasis, Geodetic, Vec3d, geodetic_to_ecef};
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
fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect()
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let name = match args.as_slice() {
        [_] => "optical-normal-query",
        [_, flag, name] if flag == "--name" => name.as_str(),
        _ => panic!("Usage: optical-normal-query-probe [--name lowercase-diagnostic-name]"),
    };
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "Diagnostic name must contain only lowercase ASCII letters, digits, and hyphens"
    );
    let prefix = format!("artifacts/{name}");
    for suffix in [".wgsl", "-inputs.bin", "-readback.bin", ".json"] {
        let output = format!("{prefix}{suffix}");
        assert!(
            !std::path::Path::new(&output).exists(),
            "Preserve existing diagnostic evidence: {output}"
        );
    }
    let mut inputs = Vec::new();
    let mut cases = Vec::new();
    for latitude in [-90., -67.5, -12., 0., 29.305549, 62., 89.999, 90.] {
        for eye_height in [-27.7, 1.7, 20000.125, 120000.035] {
            let location = Geodetic::new(latitude, -94.767015, eye_height).unwrap();
            let origin = geodetic_to_ecef(location);
            let chart = coordinates::Chart::at(origin).unwrap();
            let basis = EnuBasis::at(location);
            let local = |v: Vec3d| [v.dot(basis.east), v.dot(basis.up), -v.dot(basis.north)];
            let (sin, cos) = latitude.to_radians().sin_cos();
            let height_high = chart.height as f32;
            let height_low = (chart.height - f64::from(height_high)) as f32;
            for (height, a, b, step) in [
                (chart.height, 0., 0., 1.),
                (-30., 0.000001, -0.000002, 100.),
                (2000., -0.01, 0.02, 1000.),
                (50000., 0.1, -0.15, 10000.),
                (120000., -0.2, 0.3, 100000.),
                (0., 0.5, -0.5, 100.),
            ] {
                let anchor_high = height as f32;
                let anchor_low = (height - f64::from(anchor_high)) as f32;
                let a = a as f32;
                let b = b as f32;
                let direction = Vec3d::new(0.25, 0.1, -1.).normalized().unwrap();
                let u = [direction.x as f32, direction.y as f32, direction.z as f32];
                let global_u = basis.east * f64::from(u[0]) + basis.up * f64::from(u[1])
                    - basis.north * f64::from(u[2]);
                let s = coordinates::State {
                    height: f64::from(anchor_high) + f64::from(anchor_low),
                    a: f64::from(a),
                    b: f64::from(b),
                    direction: global_u,
                };
                let refractivity = 0.0003_f32;
                let slope = -3e-8_f32;
                let rhs = chart
                    .derivative(s, 1. + f64::from(refractivity), f64::from(slope))
                    .unwrap();
                inputs.extend([
                    [sin as f32, cos as f32, height_high, height_low],
                    [anchor_high, anchor_low, refractivity, slope],
                    [0., a, b, step as f32],
                    [u[0], u[1], u[2], 0.],
                ]);
                cases.push(json!({
                    "latitude_deg":latitude,"longitude_deg":-94.767015,"eye_height_m":chart.height,
                    "encoded_pose":inputs[inputs.len()-4],"encoded_branch":inputs[inputs.len()-3],
                    "encoded_state":inputs[inputs.len()-2],"encoded_direction":inputs[inputs.len()-1],
                    "canonical_normal":local(chart.normal(s).unwrap().0),
                    "canonical_position_delta_m":local(chart.position_delta(s).unwrap()),
                    "canonical_coordinate_derivative":[rhs.height,rhs.a,rhs.b],
                    "canonical_direction_derivative":local(rhs.direction),
                    "canonical_position_derivative":local(chart.position_derivative(s,rhs).unwrap()),
                }));
            }
        }
    }
    assert_eq!(cases.len(), 192);
    let mut shader = std::fs::read_to_string("scripts/optical_normal_step.wgsl").unwrap();
    shader.push_str(
        r#"
struct Query{pose:vec4<f32>,branch:vec4<f32>,state:vec4<f32>,direction:vec4<f32>};
@group(0) @binding(0) var<storage,read> queries:array<Query>;
@group(0) @binding(1) var<storage,read_write> result:array<vec4<u32>>;
@compute @workgroup_size(32)
fn normal_queries(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=arrayLength(&queries){return;}
    let query=queries[id.x];let axis=vec3(0.0,query.pose.x,-query.pose.y);
    nc_axis=axis*inverseSqrt(dot(axis,axis));nc_eye_height=query.pose.zw;
    nc_anchor=query.branch.xy;nc_index=query.branch.zw;
    let s=NcState(query.state.xyz,query.direction.xyz);let rhs=nc_rhs(s);
    let curve=nc_trial(s,query.state.w);let endpoint=nc_curve_q(s,curve,1.0);
    result[7u*id.x]=bitcast<vec4<u32>>(vec4(nc_normal(s.q),nc_height(s.q)));
    result[7u*id.x+1u]=bitcast<vec4<u32>>(vec4(nc_position(s.q),0.0));
    result[7u*id.x+2u]=vec4(bitcast<vec3<u32>>(rhs.q),rhs.valid);
    result[7u*id.x+3u]=bitcast<vec4<u32>>(vec4(rhs.u,rhs.refractivity));
    result[7u*id.x+4u]=bitcast<vec4<u32>>(vec4(nc_position_derivative(s.q,rhs.q),0.0));
    result[7u*id.x+5u]=bitcast<vec4<u32>>(vec4(nc_position(endpoint),curve.error));
    result[7u*id.x+6u]=vec4(bitcast<vec3<u32>>(nc_curve_u(s,curve,1.0)),curve.valid);
}
"#,
    );
    std::fs::write(format!("{prefix}.wgsl"), &shader).unwrap();
    std::fs::write(format!("{prefix}-inputs.bin"), bytes(&inputs)).unwrap();
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
        label: Some("Normal-chart finite field/trial queries"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("normal_queries"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = cases.len() as u64 * 7 * 16;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
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
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((cases.len() as u32).div_ceil(32), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(60)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    let raw = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    std::fs::write(format!("{prefix}-readback.bin"), &raw).unwrap();
    for (case, record) in cases.iter_mut().zip(raw.chunks_exact(112)) {
        let words: Vec<_> = record
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let values: Vec<_> = words.iter().map(|&v| f32::from_bits(v)).collect();
        case["gpu_raw_words"] = json!(words);
        case["gpu_values"] = json!(values);
    }
    let report = json!({"scope":"192 finite normal-coordinate field and one-trial readbacks. No source-contact solver, acceptance envelope or timing claim. Canonical f64 reference uses the same authored eye and encoded input state/direction; packed latitude/axis and f32 arithmetic errors remain visible.","adapter":format!("{:?}",adapter.get_info()),"cases":cases});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("Wrote {prefix}.json; all raw words retained, no runtime acceptance claim.");
}
