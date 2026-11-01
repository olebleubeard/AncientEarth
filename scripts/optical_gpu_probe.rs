// Original MIT trace_counters. Compile against already-built workspace dependencies;
// no new dependency, runtime feature, source geometry or shader math is added.
use earth_math::{
    atmosphere::{
        AtmosphereLayer, AtmosphereProfile,
        gpu::{GpuEllipsoid, scene::PackedScene},
    },
    rendering::LocalFrame,
    survey::SurveyMarker,
    *,
};
use std::{
    borrow::Cow,
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use wgpu::util::DeviceExt;
#[path = "optical_shader_variants.rs"]
mod variants;
pub(crate) fn block<F: Future>(future: F) -> F::Output {
    struct Signal;
    impl Wake for Signal {
        fn wake(self: Arc<Self>) {}
    }
    let w = Waker::from(Arc::new(Signal));
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(future);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}
pub(crate) fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|x| x.to_ne_bytes())
        .collect()
}
fn cost_mode() -> bool {
    std::env::args().any(|arg| arg == "--scene-cost") || geometry_fixed_point()
}
fn geometry_fixed_point() -> bool {
    std::env::args().any(|arg| arg == "--geometry-fixed-point") || geometry_period_two()
}
fn geometry_period_two() -> bool {
    std::env::args().any(|arg| arg == "--geometry-period-two")
}
fn vulkan_requested() -> bool {
    std::env::args().any(|arg| arg == "--vulkan")
}
fn atmosphere_only() -> bool {
    std::env::args().any(|arg| arg == "--atmosphere-only")
}
fn selected_variant() -> &'static str {
    if std::env::args().any(|a| a == "--dopri-height") {
        "dopri-height"
    } else if std::env::args().any(|a| a == "--dopri-carried") {
        "dopri-carried"
    } else if std::env::args().any(|a| a == "--dopri-hermite") {
        "dopri-hermite"
    } else if std::env::args().any(|a| a == "--dopri-auxiliary") {
        if std::env::args().any(|a| a == "--face-newton") {
            "dopri-auxiliary-newton"
        } else {
            "dopri-auxiliary"
        }
    } else if std::env::args().any(|a| a == "--dopri-dense") {
        "dopri-dense"
    } else if std::env::args().any(|a| a == "--cached-field") {
        "cached-layer"
    } else {
        "production"
    }
}
fn diagnostic_output_path(base: &str) -> String {
    let mut path = base.trim_end_matches(".json").to_string();
    if vulkan_requested() {
        path.push_str("-vulkan");
    }
    if geometry_period_two() {
        path = path.replace("geometry-fixed-point", "geometry-period-two");
    }
    match selected_variant() {
        "production" => {}
        "cached-layer" => path.push_str("-cached-field"),
        variant => {
            path.push('-');
            path.push_str(variant);
        }
    }
    format!("{path}.json")
}
pub(crate) fn shader() -> String {
    let shader = original_shader();
    if std::env::args().any(|a| {
        a == "--dopri-auxiliary"
            || a == "--dopri-hermite"
            || a == "--dopri-carried"
            || a == "--dopri-height"
    }) {
        variants::diagnostic_variant(shader, selected_variant())
    } else if std::env::args().any(|a| a == "--dopri-dense") {
        variants::diagnostic_variant(shader, "dopri-dense")
    } else if std::env::args().any(|a| a == "--dopri") {
        variants::diagnostic_variant(shader, "dopri")
    } else if std::env::args().any(|a| a == "--cached-field") {
        variants::diagnostic_variant(shader, "cached-layer")
    } else {
        shader
    }
}
fn original_shader() -> String {
    let source = std::fs::read_to_string("src/desktop/shaders/refraction.wesl").unwrap();
    if atmosphere_only() {
        let core =
            &source[source.find("fn qmul").unwrap()..source.find("fn relative_angles").unwrap()];
        let exit = &source[source.find("fn trace_atmosphere(direction:").unwrap()..];
        let no_contact = "struct SceneEvent {distance:f32,normal:vec3<f32>,kind:u32,marker:u32,ambiguous:u32,lower:f32,upper:f32}; fn scene_event(a:State,b:State,delta:vec3<f32>,step:f32,error:f32)->SceneEvent{return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);} fn terrain_coverage(p:vec3<f32>,n:vec3<f32>)->u32{return 1u;}";
        return format!(
            "struct Settings {{q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,budget:vec4<f32>,scene:vec4<f32>,layers:array<vec4<f32>,128>,latitude:vec4<f32>}};\n@group(0) @binding(0) var<uniform> settings:Settings;\n@group(0) @binding(1) var<storage,read> rays:array<vec4<f32>>;\n@group(0) @binding(2) var<storage,read_write> output:array<vec4<f32>>;\n{core}\n{exit}\n{no_contact}\n@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>){{if id.x>=arrayLength(&rays){{return;}}if rays[id.x].w>0.5{{output[id.x*6u]=vec4(1.0,0.0,0.0,0.0);return;}}let r=trace_atmosphere(normalize(rays[id.x].xyz));output[id.x*6u]=vec4(r.p,f32(r.status));output[id.x*6u+1u]=vec4(r.u,f32(r.material));output[id.x*6u+2u]=vec4(r.path,f32(r.marker),0.0,0.0);let original=trace(normalize(rays[id.x].xyz));output[id.x*6u+3u]=vec4(original.p,f32(original.status));output[id.x*6u+4u]=vec4(original.u,0.0);output[id.x*6u+5u]=vec4(original.path,0.0,0.0,0.0);}}"
        );
    }
    let mut core = source
        [source.find("fn qmul").unwrap()..source.find("fn relative_angles").unwrap()]
        .to_string();
    let scene = std::fs::read_to_string("src/desktop/shaders/refraction_scene.wesl").unwrap();
    let mut scene = scene[..scene.find("fn sign_row(").unwrap()]
        .replace("constants::MATERIAL_BIND_GROUP", "0")
        .replace("@binding(9)", "@binding(3)")
        .replace("@binding(10)", "@binding(4)");
    if cost_mode() {
        if geometry_fixed_point() {
            // Diagnostic only: retain all five production iterations and return
            // their original result. Count an exact earlier fixed point, then
            // compare its stored height/normal with the final production value.
            // Restore the fixed-five baseline when inspecting a production
            // shader that already contains the exact early-exit optimization.
            core = core.replace("let previous_normal=normal;", "").replace(
                "if all(bitcast<vec3<u32>>(normal)==bitcast<vec3<u32>>(previous_normal)){break;}",
                "",
            );
            let before = "var normal=normalize(qp);var h=0.0;";
            assert!(core.contains(before));
            core = core.replace(before, "var normal=normalize(qp);var h=0.0;var fixed_at=0u;var fixed_value=vec4(0.0);geometry_counters.x+=1u;");
            let before = "for(var i=0;i<5;i+=1){let qn=qmul(normal);";
            assert!(core.contains(before));
            core = core.replace(
                before,
                "for(var i=0;i<5;i+=1){let previous_normal=normal;let qn=qmul(normal);",
            );
            let before = "normal=normalize(qp-h*qn);}";
            assert!(core.contains(before));
            core = core.replace(before, "normal=normalize(qp-h*qn);if fixed_at==0u && all(bitcast<vec3<u32>>(normal)==bitcast<vec3<u32>>(previous_normal)){fixed_at=u32(i)+1u;fixed_value=vec4(normal,h);}}");
            if geometry_period_two() {
                core = core.replace(
                    "var fixed_at=0u;",
                    "var older_normal=normal;var previous_value=vec4(normal,h);var fixed_at=0u;",
                );
                core = core.replace("fixed_value=vec4(normal,h);}}", "fixed_value=vec4(normal,h);}if fixed_at==0u && i>=1 && all(bitcast<vec3<u32>>(normal)==bitcast<vec3<u32>>(older_normal)){fixed_at=u32(i)+1u;fixed_value=select(previous_value,vec4(normal,h),((4-i)%2)==0);}older_normal=previous_normal;previous_value=vec4(normal,h);}");
            }
            let before = "return vec4(normal,h);";
            assert!(core.contains(before));
            core = core.replace(before, "geometry_counters.y+=select(5u,fixed_at,fixed_at>0u);if fixed_at>0u && fixed_at<5u{geometry_counters.z+=1u;}if fixed_at>0u && any(bitcast<vec4<u32>>(fixed_value)!=bitcast<vec4<u32>>(vec4(normal,h))){geometry_counters.w+=1u;}return vec4(normal,h);");
        }
        core = core
            .replace(
                "fn midpoint_delta(s:State,step:f32)->State{",
                "fn midpoint_delta(s:State,step:f32)->State{ trace_counters.y+=1u;",
            )
            .replace(
                "for(var i=0;i<4096;i+=1){",
                "for(var i=0;i<4096;i+=1){ trace_counters.x+=1u;",
            );
        scene = scene.replace("visited+=1u;", "visited+=1u;trace_counters.w+=1u;");
        let first = scene.find("fn face_event(").unwrap();
        let body = first + scene[first..].find('{').unwrap() + 1;
        scene.insert_str(body, "trace_counters.z+=1u;");
        let shader = format!(
            "struct Settings {{q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,budget:vec4<f32>,scene:vec4<f32>,layers:array<vec4<f32>,128>,latitude:vec4<f32>}}; @group(0) @binding(0) var<uniform> settings:Settings; @group(0) @binding(1) var<storage,read> rays:array<vec4<f32>>; @group(0) @binding(2) var<storage,read_write> output:array<vec4<f32>>; var<private> trace_counters:vec4<u32>; {core} {scene} @compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>){{if id.x>=arrayLength(&rays){{return;}}if rays[id.x].w>0.5{{output[id.x*4u]=vec4(1.0,0.0,0.0,0.0);return;}}trace_counters=vec4(0u);let r=trace(normalize(rays[id.x].xyz));output[id.x*4u]=vec4(r.p,f32(r.status));output[id.x*4u+1u]=vec4(r.normal,f32(r.material));output[id.x*4u+2u]=vec4(r.path,f32(r.marker),0.0,0.0);output[id.x*4u+3u]=vec4<f32>(trace_counters);}}"
        );
        let shader = if geometry_fixed_point() {
            shader
                .replace("var<private> trace_counters:vec4<u32>;", "var<private> trace_counters:vec4<u32>;var<private> geometry_counters:vec4<u32>;")
                .replace("trace_counters=vec4(0u);", "trace_counters=vec4(0u);geometry_counters=vec4(0u);")
                .replace("id.x*4u", "id.x*5u")
                .replace("output[id.x*5u+3u]=vec4<f32>(trace_counters);", "output[id.x*5u+3u]=vec4<f32>(trace_counters);output[id.x*5u+4u]=vec4<f32>(geometry_counters);")
        } else {
            shader.replace("id.x*4u", "id.x*6u")
                .replace("var<private> trace_counters:vec4<u32>;", "var<private> trace_counters:vec4<u32>;var<private> field_calls:u32;var<private> event_counters:vec4<u32>;")
                .replace("fn geometry(p:vec3<f32>)->vec4<f32>{", "fn geometry(p:vec3<f32>)->vec4<f32>{field_calls+=1u;")
                .replace("trace_counters=vec4(0u);", "trace_counters=vec4(0u);field_calls=0u;event_counters=vec4(0u);")
                .replace("output[id.x*6u+3u]=vec4<f32>(trace_counters);", "output[id.x*6u+3u]=vec4<f32>(trace_counters);output[id.x*6u+4u]=vec4(r.u,f32(field_calls));output[id.x*6u+5u]=vec4<f32>(event_counters);")
        };
        return if selected_variant() == "dopri-carried" {
            shader.replace("id.x*6u", "id.x*10u")
                .replace("var<private> event_counters:vec4<u32>;", "var<private> event_counters:vec4<u32>;var<private> rejection_counts:vec4<u32>;var<private> rejection_values:vec4<f32>;var<private> rejection_state:vec4<f32>;var<private> rejection_start:vec4<f32>;")
                .replace("event_counters=vec4(0u);", "event_counters=vec4(0u);rejection_counts=vec4(0u);rejection_values=vec4(0.0);rejection_state=vec4(0.0);rejection_start=vec4(0.0);")
                .replace("output[id.x*10u+5u]=vec4<f32>(event_counters);", "output[id.x*10u+5u]=vec4<f32>(event_counters);output[id.x*10u+6u]=vec4<f32>(rejection_counts);output[id.x*10u+7u]=rejection_values;output[id.x*10u+8u]=rejection_state;output[id.x*10u+9u]=rejection_start;")
        } else {
            shader
        };
    }
    format!(
        "struct Settings {{q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,budget:vec4<f32>,scene:vec4<f32>,layers:array<vec4<f32>,128>,latitude:vec4<f32>}};\n@group(0) @binding(0) var<uniform> settings:Settings;\n@group(0) @binding(1) var<storage,read> rays:array<vec4<f32>>;\n@group(0) @binding(2) var<storage,read_write> output:array<vec4<f32>>;\n{core}\n{scene}\n@compute @workgroup_size(1) fn probe(@builtin(global_invocation_id) id:vec3<u32>){{if id.x>=arrayLength(&rays){{return;}}if rays[id.x].w>0.5{{let candidate=triangle_candidate(vec3(-1000.0,0.1,0.001),vec3(1000.0,-0.1,0.001),vec3(-1000.0,0.0,0.0),vec3(1000.0,0.0,0.0),vec3(0.0,-0.2,0.0),vec3(0.0,0.0,-1.0),0.00101);output[id.x*3u]=vec4(select(0.0,1.0,candidate),0.0,0.0,0.0);return;}}let r=trace(normalize(rays[id.x].xyz));output[id.x*3u]=vec4(r.p,f32(r.status));output[id.x*3u+1u]=vec4(r.normal,f32(r.material));output[id.x*3u+2u]=vec4(r.path,f32(r.marker),0.0,0.0);}}"
    )
}
fn profile(bend: bool) -> AtmosphereProfile {
    AtmosphereProfile::from_layers(if bend {
        vec![
            AtmosphereLayer {
                height_m: 0.0,
                refractivity: 0.0003,
            },
            AtmosphereLayer {
                height_m: 2000.0,
                refractivity: 0.00023,
            },
            AtmosphereLayer {
                height_m: 120000.0,
                refractivity: 0.0,
            },
        ]
    } else {
        vec![
            AtmosphereLayer {
                height_m: 0.0,
                refractivity: 0.0,
            },
            AtmosphereLayer {
                height_m: 120000.0,
                refractivity: 0.0,
            },
        ]
    })
    .unwrap()
}
pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) observer: Observer,
    pub(crate) scene: TerrainScene,
    pub(crate) markers: Vec<SurveyMarker>,
    pub(crate) profile: AtmosphereProfile,
    pub(crate) rays: Vec<Vec3d>,
    // Independent fixture ownership: 0 reference, 1 terrain, 2 marker;
    // 3 clear sky, 4 owner ambiguity, 5 stellar candidate ignores blocker,
    // 6 stellar candidate leaves the supported lower atmospheric domain.
    expected: Vec<(u32, Option<usize>)>,
}
fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    case: Case,
) -> Vec<serde_json::Value> {
    eprintln!("case {}", case.name);
    let local = LocalFrame::at(case.observer);
    let gpu = GpuEllipsoid::at(case.observer);
    let packet = PackedScene::prepare(&case.scene).unwrap();
    let frames = packet.frame_data(local, &case.markers).unwrap();
    let layers = case.profile.layers();
    let gradient = layers
        .windows(2)
        .map(|p| (p[1].refractivity - p[0].refractivity).abs() / (p[1].height_m - p[0].height_m))
        .fold(0.0, f64::max);
    let mut settings = vec![
        gpu.rows[0],
        gpu.rows[1],
        gpu.rows[2],
        gpu.origin,
        gpu.center,
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
    settings.push(gpu.latitude);
    let mut rays: Vec<_> = case
        .rays
        .iter()
        .map(|&r| {
            let p = local.direction(r);
            [p[0], p[1], p[2], 0.0]
        })
        .collect();
    rays.push([0.0, 0.0, 0.0, 1.0]);
    let stride = if atmosphere_only() {
        24
    } else if geometry_fixed_point() {
        20
    } else if cost_mode() {
        if selected_variant() == "dopri-carried" {
            40
        } else {
            24
        }
    } else {
        12
    };
    let input = [
        bytes(&settings),
        bytes(&rays),
        vec![0; 4 * stride * rays.len()],
        bytes(&packet.geometry),
        bytes(&frames),
    ];
    let buffers: Vec<_> = input
        .iter()
        .enumerate()
        .take(if atmosphere_only() { 3 } else { 5 })
        .map(|(i, b)| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("optical diagnostic"),
                contents: b,
                usage: if i == 0 {
                    wgpu::BufferUsages::UNIFORM
                } else {
                    wgpu::BufferUsages::STORAGE
                        | if i == 2 {
                            wgpu::BufferUsages::COPY_SRC
                        } else {
                            wgpu::BufferUsages::empty()
                        }
                },
            })
        })
        .collect();
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input[2].len() as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(rays.len() as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buffers[2], 0, &read, 0, input[2].len() as u64);
    let dispatch_start = std::time::Instant::now();
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    loop {
        device.poll(wgpu::PollType::Poll).unwrap();
        if let Ok(r) = rx.try_recv() {
            r.unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let dispatch_seconds = dispatch_start.elapsed().as_secs_f64();
    let mapped = read.slice(..).get_mapped_range().unwrap();
    let values: Vec<f32> = mapped
        .chunks_exact(4)
        .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
        .collect();
    let helper = values[case.rays.len() * stride];
    assert!(
        helper == 1.0,
        "Near-parallel true tube candidate was rejected: {helper}"
    );
    let mut result = Vec::new();
    for (i, &direction) in case.rays.iter().enumerate() {
        let r = &values[i * stride..(i + 1) * stride];
        let point = local.origin + local.basis.east * r[0] as f64 + local.basis.up * r[1] as f64
            - local.basis.north * r[2] as f64;
        if cost_mode() {
            let item = serde_json::json!({"solver_variant":selected_variant(),"case":case.name,"ray":i,"passed":true,
                "carried_rejections":if selected_variant()=="dopri-carried" {Some(serde_json::json!({"counts_embedded_boundary_consistency_reference":[r[24],r[25],r[26],r[27]],"last_consistency_error_allocation_tolerance":[r[28],r[29],r[30],r[31]],"last_height_error_normal_error_step_layer":[r[32],r[33],r[34],r[35]],"last_start_consistency_aux_height_inverse_height_path":[r[36],r[37],r[38],r[39]],"last_step_bits":r[34].to_bits(),"terminal_position_bits":[r[0].to_bits(),r[1].to_bits(),r[2].to_bits()]}))}else{None},
                "gpu_status":r[3],"gpu_material":r[7],"gpu_owner":r[9],"gpu_path_m":r[8],
                "root_diagnostics":if geometry_fixed_point(){None}else{Some([r[20],r[21],r[22],r[23]])},"gpu_local_point":[r[0],r[1],r[2]],"gpu_local_direction":if geometry_fixed_point(){None}else{Some([r[16],r[17],r[18]])},"geometry_evaluations":if geometry_fixed_point(){None}else{Some(r[19])},"launch_ecef":[direction.x,direction.y,direction.z],
                "integration_attempts":r[12],"midpoint_delta_calls":r[13],"candidate_faces":r[14],"bvh_nodes":r[15],
                "physical_pixel":actual_scene_pixel(i),"counter_exact_integer_range":r[12..16].iter().all(|x| *x<=16777216.0),"completion_only":true,"batch_dispatch_and_readback_seconds":dispatch_seconds,"batch_ray_count":case.rays.len(),
                "geometry_fixed_point":if geometry_fixed_point(){Some(serde_json::json!({"calls":r[16],"potential_iterations":r[17],"calls_with_skipped_iterations":r[18],"final_value_mismatches":r[19],"counter_exact_integer_range":r[16..20].iter().all(|x| *x<=16777216.0),"scope":if geometry_period_two(){"All five original iterations still execute. Earliest period-one or period-two normal recurrence predicts the fifth complete vec4 using parity; every prediction is compared against the actual fifth vec4. No production change or frame-time claim."}else{"All five original iterations still execute. Potential iteration count stops at first bitwise-identical normal; saved vec4 is compared with final vec4. No production early exit or frame-time claim."}}))}else{None},
                "physical_faces":packet.triangle_count,"markers":case.markers.len(),"geometry_bytes":packet.geometry.len()*16,
                "scope":"Instrumented production compute trace: passed denotes diagnostic completion only. Operation counts are exactly represented f32 integers for these recorded values; one batch wall time includes readback. No shading, full-frame timing, or CPU-hit accuracy assertion."});
            println!("{}", item);
            result.push(item);
            continue;
        }
        let cpu = case.profile.trace_scene(
            Ray::new(local.origin, direction).unwrap(),
            &case.scene,
            &case.markers,
        );
        let item = match cpu {
            Ok(c) => {
                let status = if c.surface.is_some() || c.marker.is_some() {
                    1
                } else {
                    0
                };
                let cpu_kind = if c.marker.is_some() {
                    2
                } else if c
                    .surface
                    .is_some_and(|s| matches!(s.kind, SurfaceKind::ReferenceEllipsoid))
                {
                    0
                } else {
                    1
                };
                let gpu_kind = if r[7] == 1.0 {
                    2
                } else if (r[4] * r[4] + r[5] * r[5] + r[6] * r[6]) < 0.5 {
                    0
                } else {
                    1
                };
                let cpu_id = c.marker.map(|m| m.marker_id);
                let gpu_id = (r[7] == 1.0)
                    .then(|| case.markers.get(r[9] as usize).map(|m| m.id))
                    .flatten();
                let endpoint_error = point.distance(c.exit_ray.origin);
                let path_error = (r[8] as f64 - c.geometric_path_m).abs();
                let expected = case.expected[i];
                let gpu_direction = local.basis.east * r[4] as f64 + local.basis.up * r[5] as f64
                    - local.basis.north * r[6] as f64;
                let direction_error = gpu_direction.distance(c.exit_ray.direction);

                let parity = if atmosphere_only() {
                    let exact = (0..7).all(|j| r[j].to_bits() == r[j + 12].to_bits())
                        && r[8].to_bits() == r[20].to_bits();
                    Some(serde_json::json!({"exact_bits":exact,
                        "position_delta_m":((r[0]-r[12]).powi(2)+(r[1]-r[13]).powi(2)+(r[2]-r[14]).powi(2)).sqrt(),
                        "direction_delta":((r[4]-r[16]).powi(2)+(r[5]-r[17]).powi(2)+(r[6]-r[18]).powi(2)).sqrt(),
                        "path_delta_m":(r[8]-r[20]).abs(),"original_status":r[15]}))
                } else {
                    None
                };
                let passed = if expected.0 == 3 {
                    status == 0
                        && r[3] == 0.0
                        && endpoint_error < 5.0
                        && path_error < 5.0
                        && direction_error < 3e-6
                        && parity.as_ref().is_some_and(|p| p["exact_bits"] == true)
                } else if expected.0 == 5 {
                    status == 1 && cpu_id == expected.1 && r[3] == 0.0
                } else if expected.0 == 6 {
                    status == 1 && r[3] == 2.0
                } else if expected.0 == 4 {
                    r[3] == 2.0 && cpu_id == expected.1
                } else {
                    status == 1
                        && r[3] == 1.0
                        && cpu_kind == expected.0
                        && gpu_kind == expected.0
                        && cpu_id == expected.1
                        && gpu_id == expected.1
                        && endpoint_error < 0.05
                        && path_error < 0.05
                };
                serde_json::json!({"case":case.name,"ray":i,"passed":passed,"expected_kind":expected.0,"expected_id":expected.1,"gpu_status":r[3],"cpu_status":status,"gpu_kind":gpu_kind,"cpu_kind":cpu_kind,"gpu_marker_id":gpu_id,"cpu_marker_id":cpu_id,"endpoint_error_m":endpoint_error,"path_error_m":path_error,"direction_error":if atmosphere_only(){Some(direction_error)}else{None},"primary_no_contact_parity":parity,"gpu_path_m":r[8],"cpu_path_m":c.geometric_path_m,"gpu_point":[r[0],r[1],r[2]],"gpu_normal":[r[4],r[5],r[6]],"triangles":packet.triangle_count,"geometry_bytes":packet.geometry.len()*16,"finite_direction_threshold":if atmosphere_only(){Some(3e-6)}else{None},"finite_endpoint_threshold_m":if atmosphere_only(){5.0}else{0.05},"finite_path_threshold_m":if atmosphere_only(){5.0}else{0.05},"near_parallel_candidate_kept":if atmosphere_only(){None}else{Some(helper)}})
            }
            Err(e) => {
                serde_json::json!({"case":case.name,"ray":i,"passed":false,"gpu_status":r[3],"cpu_error":e.to_string()})
            }
        };
        println!("{}", item);
        result.push(item);
    }
    drop(mapped);
    read.unmap();
    result
}
fn main() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: if geometry_fixed_point() || vulkan_requested() {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::DX12
        },
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = block(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    println!("adapter {:?}", adapter.get_info());
    let (device, queue) = block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    eprintln!("device ready");
    let source = shader();
    let base = if geometry_fixed_point() {
        "artifacts/optical-geometry-fixed-point.json"
    } else if cost_mode() {
        "artifacts/optical-scene-cost.json"
    } else if atmosphere_only() {
        "artifacts/optical-atmosphere-gpu-readback.json"
    } else {
        "artifacts/optical-gpu-readback.json"
    };
    std::fs::write(
        diagnostic_output_path(base).replace(".json", ".wgsl"),
        &source,
    )
    .unwrap();
    std::fs::write(
        if geometry_fixed_point() {
            ".cache/optical-geometry-fixed-point-probe.wgsl"
        } else if atmosphere_only() {
            ".cache/optical-atmosphere-probe.wgsl"
        } else {
            ".cache/optical-scene-probe.wgsl"
        },
        &source,
    )
    .unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("exact production optical trace"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    eprintln!("shader module ready; creating compute pipeline");
    let pipeline_start = std::time::Instant::now();
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    let pipeline_seconds = pipeline_start.elapsed().as_secs_f64();
    eprintln!("pipeline ready after {pipeline_seconds:.6} seconds");
    let mut results = Vec::new();
    if cost_mode() {
        results.extend(run(&device, &queue, &pipeline, actual_scene()));
        write_report(
            if geometry_fixed_point() {
                "artifacts/optical-geometry-fixed-point.json"
            } else {
                "artifacts/optical-scene-cost.json"
            },
            pipeline_seconds,
            &adapter,
            results,
        );
        return;
    }
    if atmosphere_only() {
        for height in [-20.0, 100.0, 5000.0] {
            for bend in [false, true] {
                let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap());
                observer.feet.height_m = height - observer.eye_height_m;
                let basis = EnuBasis::at(observer.eye_geodetic());
                results.extend(run(
                    &device,
                    &queue,
                    &pipeline,
                    Case {
                        name: format!("clear stellar exit h={height} bend={bend}"),
                        observer,
                        scene: TerrainScene::new(vec![
                            TerrainPatch::from_grid(
                                2,
                                2,
                                -0.001,
                                0.001,
                                0.002,
                                0.002,
                                vec![-30.0; 4],
                            )
                            .unwrap(),
                        ]),
                        markers: vec![],
                        profile: profile(bend),
                        rays: vec![
                            basis.up,
                            (basis.up + basis.east * 0.5).normalized().unwrap(),
                            (basis.up + basis.north).normalized().unwrap(),
                        ],
                        expected: vec![(3, None); 3],
                    },
                ));
            }
        }
        let observer = Observer::new(Geodetic::new(0.0, 0.0, 98.28).unwrap());
        let basis = EnuBasis::at(observer.eye_geodetic());
        let marker = SurveyMarker {
            id: 47,
            base_ecef_m: observer.eye_position() + basis.up * 100.0,
            right: basis.east,
            forward: basis.north,
            up: basis.up,
            width_m: 10.0,
            depth_m: 10.0,
            height_m: 10.0,
            along_distance_m: 100.0,
        };
        results.extend(run(
            &device,
            &queue,
            &pipeline,
            Case {
                name: "stellar candidate is not visibility".into(),
                observer,
                scene: TerrainScene::new(vec![]),
                markers: vec![marker],
                profile: profile(true),
                rays: vec![basis.up],
                expected: vec![(5, Some(47))],
            },
        ));
        results.extend(run(
            &device,
            &queue,
            &pipeline,
            Case {
                name: "stellar candidate lower-domain failure".into(),
                observer,
                scene: TerrainScene::new(vec![]),
                markers: vec![],
                profile: profile(false),
                rays: vec![-basis.up],
                expected: vec![(6, None)],
            },
        ));
        write_report(
            "artifacts/optical-atmosphere-gpu-readback.json",
            pipeline_seconds,
            &adapter,
            results,
        );
        return;
    }
    for wide in [false, true] {
        for height in [-30.0, 250.0, 3500.0] {
            for bend in [false, true] {
                let d = if wide { 1.0 } else { 0.001 };
                let scene = TerrainScene::new(vec![
                    TerrainPatch::from_grid(2, 2, -d, d, 2.0 * d, 2.0 * d, vec![height; 4])
                        .unwrap(),
                ]);
                let observer = Observer::new(
                    Geodetic::new(0.0, if wide { 0.75 } else { 0.0 }, 5000.0 - 1.72).unwrap(),
                );
                let basis = EnuBasis::at(observer.eye_geodetic());
                let rays = [
                    -basis.up,
                    (-basis.up + basis.east * 0.01).normalized().unwrap(),
                    (-basis.up + basis.north * 0.01).normalized().unwrap(),
                    (-basis.up + basis.east * 0.25).normalized().unwrap(),
                ];
                results.extend(run(
                    &device,
                    &queue,
                    &pipeline,
                    Case {
                        name: format!("plate wide={wide} h={height} bend={bend}"),
                        observer,
                        scene,
                        markers: vec![],
                        profile: profile(bend),
                        rays: rays.to_vec(),
                        expected: vec![
                            (1, None),
                            (1, None),
                            (1, None),
                            (if wide { 1 } else { 0 }, None),
                        ],
                    },
                ));
            }
        }
    }
    for bend in [false, true] {
        let observer = Observer::new(Geodetic::new(0.0, 0.0, 98.28).unwrap());
        let marker = SurveyMarker {
            id: 47,
            base_ecef_m: Vec3d::new(WGS84_A_M + 70.0, 1000.0, 0.0),
            right: Vec3d::new(0.0, 0.0, -1.0),
            forward: Vec3d::new(0.0, 1.0, 0.0),
            up: Vec3d::new(1.0, 0.0, 0.0),
            width_m: 3.048,
            depth_m: 3.048,
            height_m: 60.0,
            along_distance_m: 1000.0,
        };
        results.extend(run(
            &device,
            &queue,
            &pipeline,
            Case {
                name: format!("marker bend={bend}"),
                observer,
                scene: TerrainScene::new(vec![]),
                markers: vec![marker],
                profile: profile(bend),
                rays: vec![
                    Vec3d::new(0.0, 1.0, 0.0),
                    Vec3d::new(-0.01, 1.0, 0.0).normalized().unwrap(),
                    Vec3d::new(0.01, 1.0, 0.0).normalized().unwrap(),
                ],
                expected: vec![(2, Some(47)); 3],
            },
        ));
    }
    let radius = WGS84_A_M + 1000.0;
    let index = 1.0003;
    let slope = -index / radius;
    let circular = AtmosphereProfile::from_layers(vec![
        AtmosphereLayer {
            height_m: 0.0,
            refractivity: 0.0003 - slope * 1000.0,
        },
        AtmosphereLayer {
            height_m: 2000.0,
            refractivity: 0.0003 + slope * 1000.0,
        },
        AtmosphereLayer {
            height_m: 120000.0,
            refractivity: 0.0,
        },
    ])
    .unwrap();
    let make_marker = |distance: f64, id| {
        let a = distance / radius;
        let up = Vec3d::new(a.cos(), a.sin(), 0.0);
        SurveyMarker {
            id,
            base_ecef_m: up * (radius - 0.005),
            right: Vec3d::new(0.0, 0.0, -1.0),
            forward: Vec3d::new(-a.sin(), a.cos(), 0.0),
            up,
            width_m: 10.0,
            depth_m: 0.002,
            height_m: 0.01,
            along_distance_m: distance,
        }
    };
    for reverse in [false, true] {
        let near = make_marker(1000.0, 47);
        let far = make_marker(1500.0, 12);
        results.extend(run(
            &device,
            &queue,
            &pipeline,
            Case {
                name: format!("circular thin markers reverse={reverse}"),
                observer: Observer::new(Geodetic::new(0.0, 0.0, 1000.0 - 1.72).unwrap()),
                scene: TerrainScene::new(vec![]),
                markers: if reverse {
                    vec![far, near]
                } else {
                    vec![near, far]
                },
                profile: circular.clone(),
                rays: vec![Vec3d::new(0.0, 1.0, 0.0)],
                expected: vec![(2, Some(47))],
            },
        ));
    }
    results.extend(run(
        &device,
        &queue,
        &pipeline,
        Case {
            name: "initial h0 inward contact".into(),
            observer: {
                let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap());
                observer.feet.height_m = -observer.eye_height_m;
                observer
            },
            scene: TerrainScene::new(vec![]),
            markers: vec![],
            profile: profile(false),
            rays: vec![Vec3d::new(-1.0, 0.0, 0.0)],
            expected: vec![(0, None)],
        },
    ));
    let near = make_marker(1000.0, 47);
    let mut duplicate = near;
    duplicate.id = 12;
    results.extend(run(
        &device,
        &queue,
        &pipeline,
        Case {
            name: "duplicate physical owner ambiguity".into(),
            observer: Observer::new(Geodetic::new(0.0, 0.0, 1000.0 - 1.72).unwrap()),
            scene: TerrainScene::new(vec![]),
            markers: vec![near, duplicate],
            profile: circular,
            rays: vec![Vec3d::new(0.0, 1.0, 0.0)],
            expected: vec![(4, Some(47))],
        },
    ));
    write_report(
        "artifacts/optical-gpu-readback.json",
        pipeline_seconds,
        &adapter,
        results,
    );
}
fn write_report(
    path: &str,
    pipeline_seconds: f64,
    adapter: &wgpu::Adapter,
    results: Vec<serde_json::Value>,
) {
    let passed = results.iter().all(|v| v["passed"] == true);
    let path = diagnostic_output_path(path);
    std::fs::write(
        path,
        serde_json::to_string_pretty(
            &serde_json::json!({"solver_variant":selected_variant(),"pipeline_creation_seconds":pipeline_seconds,"passed":passed,"adapter":format!("{:?}",adapter.get_info()),"scope":"Finite compute execution of the declared solver variant and canonical geometry packets with compact repacked uniforms. Experimental DP variants do not change production. Not full material binding or universal accuracy proof.","cases":results}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        passed,
        "One or more finite GPU/CPU diagnostic cases failed; raw outcomes retained."
    );
}

pub(crate) fn actual_scene() -> Case {
    use earth_math::topography::{DownloadRegion, copernicus_tiles, import_sources};
    use std::{fs, io::Read, path::PathBuf, sync::atomic::AtomicBool};
    let cache = PathBuf::from(".cache/topography-wide-area-qa");
    let selection: serde_json::Value =
        serde_json::from_slice(&fs::read(cache.join("selection.json")).unwrap()).unwrap();
    let region = DownloadRegion::new(
        Geodetic::new(
            selection["latitude"].as_f64().unwrap(),
            selection["longitude"].as_f64().unwrap(),
            0.0,
        )
        .unwrap(),
        selection["radius_m"].as_f64().unwrap(),
    )
    .unwrap();
    let sources: Vec<_> = copernicus_tiles(&region)
        .unwrap()
        .into_iter()
        .filter_map(|tile| {
            let path = cache.join("assets").join(format!("{}.tif", tile.id));
            path.is_file().then_some((tile, path))
        })
        .collect();
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(fs::File::open("assets/geoid/egm2008-native.aeg.gz").unwrap())
        .read_to_end(&mut data)
        .unwrap();
    let geoid = GeoidGrid::from_bytes(&data).unwrap();
    drop(data);
    let boundaries = Arc::new(
        TerrainScene::boundary_only(
            (1..=4)
                .map(|i| {
                    TerrainPatch::from_boundary_bytes(
                        &fs::read(format!("assets/boundaries/region-{i:02}.aeb")).unwrap(),
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap(),
    );
    let (scene, _) = import_sources(
        &region,
        &sources,
        &geoid,
        boundaries,
        &AtomicBool::new(false),
    )
    .unwrap();
    let demo = earth_math::survey::nearby_demo(&scene).unwrap();
    let request = earth_math::atmosphere::weather::regional_reference_request();
    let loaded = earth_math::atmosphere::weather::decode_weather(
        &fs::read("assets/weather/gfs-regional-reference.grib2").unwrap(),
        Some(&fs::read("assets/weather/hrrr-reference.grib2").unwrap()),
        &request,
    )
    .unwrap();
    assert!(loaded.regional_fallback.is_none());
    let profile = loaded
        .profile
        .anchored_to_scene_at(&scene, demo.observer.feet)
        .unwrap();
    // Fixed coarse grid plus the independently checked nine rays, including
    // the exact centre pixel so the narrow nearest marker is represented.
    let rays = (0..153)
        .map(actual_scene_pixel)
        .map(|[x, y]| {
            demo.lens
                .pixel_ray(demo.observer.frame(), x, y, 1920, 1080)
                .unwrap()
                .direction
        })
        .collect();
    Case {
        name: "cached100mile terrain with81markers and pairedNOAA".into(),
        observer: demo.observer,
        scene,
        markers: demo.markers,
        profile,
        rays,
        expected: vec![],
    }
}

fn actual_scene_pixel(index: usize) -> [u32; 2] {
    if index < 144 {
        [
            (index as u32 % 16) * 120 + 60,
            (index as u32 / 16) * 120 + 60,
        ]
    } else {
        let i = (index - 144) as u32;
        [320 + (i % 3) * 640, 180 + (i / 3) * 360]
    }
}
