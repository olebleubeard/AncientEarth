//! Independent finite launch audit. Uses retained full-frame shaders and returns
//! the exact f32 launch vector alongside their terminal words. This is not timing.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
#[allow(dead_code)]
#[path = "optical_shader_variants.rs"]
mod variants;
use earth_math::{
    atmosphere::gpu::{GpuEllipsoid, scene::PackedScene},
    rendering::LocalFrame,
    *,
};
use serde_json::{Value, json};
use std::{borrow::Cow, time::Duration};
use wgpu::util::DeviceExt;

const CHECKPOINT: &str = "artifacts/optical-selected-launch-input-checkpoint.json";
const CANDIDATE: &str = "artifacts/optical-selected-launch-input-candidate.wgsl";
const BASELINE: &str = "artifacts/optical-selected-launch-input-baseline.wgsl";
const OUTPUT: &str = "artifacts/optical-selected-launch-review";

fn replace_once(text: &mut String, old: &str, new: &str) {
    assert_eq!(
        text.matches(old).count(),
        1,
        "source checkpoint changed: {old}"
    );
    *text = text.replacen(old, new, 1);
}
fn selected_shader(original: &str, count: usize) -> String {
    let mut text = original.to_owned();
    replace_once(
        &mut text,
        "if id.x>=dimensions.x*dimensions.y{return;}",
        &format!("if id.x>={count}u{{return;}}"),
    );
    replace_once(
        &mut text,
        "let pixel=vec2(id.x%dimensions.x,id.x/dimensions.x);",
        "let pixel=vec2<u32>(rays[5u+id.x].xy);",
    );
    // The launch expression, trace, workgroup size, packing and all numerical
    // functions remain those of the retained checkpoint. Output stride alone
    // grows to retain the launch. A different compiler context is measured below.
    assert_eq!(text.matches("id.x*3u").count(), 6);
    text = text.replace("id.x*3u", "id.x*4u");
    replace_once(
        &mut text,
        "let r=trace(direction);",
        "output[id.x*4u+3u]=vec4(bitcast<vec3<u32>>(direction),0u);let r=trace(direction);",
    );
    text
}
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
    let result = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    result
}
fn vector(words: &[u32], offset: usize) -> Vec3d {
    Vec3d::new(
        f64::from(f32::from_bits(words[offset])),
        f64::from(f32::from_bits(words[offset + 1])),
        f64::from(f32::from_bits(words[offset + 2])),
    )
}
fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mut candidate_path = CANDIDATE.to_string();
    let mut output_prefix = OUTPUT.to_string();
    let mut strict = false;
    let mut custom_candidate = false;
    let mut i = 0;
    while i < arguments.len() {
        match arguments[i].as_str() {
            "--candidate-shader" => {
                i += 1;
                candidate_path = arguments.get(i).expect("candidate shader path").clone();
                custom_candidate = true;
            }
            "--output-prefix" => {
                i += 1;
                output_prefix = arguments.get(i).expect("output prefix").clone();
            }
            "--strict" => strict = true,
            other => panic!("Unknown argument: {other}"),
        }
        i += 1;
    }
    assert!(
        !(strict && custom_candidate),
        "Choose strict retained input or a custom assembled shader."
    );
    assert!(
        !std::path::Path::new(&format!("{output_prefix}.json")).exists(),
        "Use a new output label; retained reports are never overwritten."
    );
    let checkpoint: Value =
        serde_json::from_str(&std::fs::read_to_string(CHECKPOINT).unwrap()).unwrap();
    assert_eq!(checkpoint["dimensions"], json!([1920, 1080]));
    assert_eq!(checkpoint["workgroup_size"], 32);
    let comparison = &checkpoint["full_frame_comparison"];
    let selected: Vec<_> = comparison["all_changed_classification"]
        .as_array()
        .unwrap()
        .iter()
        .chain(
            comparison["same_status_owner_worst_references"]
                .as_array()
                .unwrap(),
        )
        .cloned()
        .collect();
    assert_eq!(selected.len(), 12);
    let pixels: Vec<[u32; 2]> = selected
        .iter()
        .map(|v| {
            [
                v["pixel"][0].as_u64().unwrap() as u32,
                v["pixel"][1].as_u64().unwrap() as u32,
            ]
        })
        .collect();
    let case = source::actual_scene();
    let local = LocalFrame::at(case.observer);
    let packet = PackedScene::prepare(&case.scene).unwrap();
    let frames = packet.frame_data(local, &case.markers).unwrap();
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
    let local_vector = |v| {
        let p = local.direction(v);
        [p[0], p[1], p[2], 0.0]
    };
    let mut view = vec![
        local_vector(frame.right),
        local_vector(frame.up),
        local_vector(frame.forward),
        [
            1920.0 / 1080.0,
            (demo.lens.vertical_fov_deg.to_radians() * 0.5) as f32,
            u8::from(demo.lens.projection == Projection::EquidistantFisheye) as f32,
            0.0,
        ],
        [1920.0, 1080.0, 0.0, 0.0],
    ];
    view.extend(pixels.iter().map(|p| [p[0] as f32, p[1] as f32, 0.0, 0.0]));
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = source::block(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = source::block(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
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
    let geometry = make(&packet.geometry, false);
    let frames = make(&frames, false);
    let size = selected.len() as u64 * 64;
    let mut runs = Vec::new();
    for (label, original_path) in [
        ("candidate", candidate_path.as_str()),
        ("baseline", BASELINE),
    ] {
        let mut original = std::fs::read_to_string(original_path).unwrap();
        let modified_candidate = label == "candidate" && (strict || custom_candidate);
        if label == "candidate" && strict {
            original = variants::strict_geometry_variant(original);
        }
        assert!(
            original.contains("@workgroup_size(32)"),
            "The retained selected-ray audit uses 32 invocations."
        );
        let shader = selected_shader(&original, selected.len());
        std::fs::write(format!("{output_prefix}-{label}.wgsl"), &shader).unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader)),
        });
        eprintln!("Creating {label} selected-launch pipeline");
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: None,
            module: &module,
            entry_point: Some("performance"),
            compilation_options: Default::default(),
            cache: None,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let inputs = [&settings, &view, &output, &geometry, &frames];
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &inputs
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(60)),
            })
            .unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, size);
        queue.submit([encoder.finish()]);
        let bytes = read(&device, &staging);
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let world =
            |v: Vec3d| local.basis.east * v.x + local.basis.up * v.y - local.basis.north * v.z;
        let mut rows = Vec::new();
        for (i, sample) in words.chunks_exact(16).enumerate() {
            let stored: Vec<u32> = selected[i][label]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u32)
                .collect();
            let launch_local = vector(sample, 12);
            let ray = Ray::new(local.origin, world(launch_local)).unwrap();
            let canonical = demo
                .lens
                .pixel_ray(frame, pixels[i][0], pixels[i][1], 1920, 1080)
                .unwrap();
            let reference = match case.profile.trace_scene(ray, &case.scene, &case.markers) {
                Ok(result) => {
                    let errors = |w: &[u32]| {
                        json!({
                            "position_m": (local.origin + world(vector(w,0))).distance(result.exit_ray.origin),
                            "path_m": (f64::from(f32::from_bits(w[3])) - result.geometric_path_m).abs(),
                            "direction": world(vector(w,4)).distance(result.exit_ray.direction),
                        })
                    };
                    json!({"status":u8::from(result.surface.is_some() || result.marker.is_some()),
                        "marker_id":result.marker.map(|m|m.marker_id),
                        "surface_kind":result.surface.map(|s|format!("{:?}",s.kind)),
                        "endpoint_ecef_m":[result.exit_ray.origin.x,result.exit_ray.origin.y,result.exit_ray.origin.z],
                        "exit_direction":[result.exit_ray.direction.x,result.exit_ray.direction.y,result.exit_ray.direction.z],
                        "path_m":result.geometric_path_m,"optical_path_m":result.optical_path_m,
                        "selected_errors":errors(sample),"full_frame_errors":errors(&stored)})
                }
                Err(error) => json!({"error":error.to_string()}),
            };
            rows.push(json!({"pixel":pixels[i],"terminal_words":&sample[..12],
                "retained_full_frame_words":stored,
                "terminal_context_parity_applicable":!modified_candidate,
                "exact_terminal_context_parity":if modified_candidate {None} else {Some(sample[..12]==stored)},
                "matches_reference_checkpoint_words":sample[..12]==stored,
                "launch_words":&sample[12..15],"launch_local":[launch_local.x,launch_local.y,launch_local.z],
                "launch_local_norm":launch_local.length(),
                "launch_ecef_normalized":[ray.direction.x,ray.direction.y,ray.direction.z],
                "canonical_launch_direction_difference":ray.direction.distance(canonical.direction),
                "f64_physical_reference_from_readback_launch":reference}));
        }
        runs.push(json!({"variant":label,"retained_shader":original_path,
            "diagnostic_modified_candidate":modified_candidate,"strict_geometry_injection":label=="candidate"&&strict,"rows":rows}));
    }
    let report = json!({"scope":"Finite selected-pixel audit, not a timing or full-frame equivalence claim. Retained shader launch expressions and trace functions; added launch output changes compiler context, tested explicitly against retained full-frame terminal words. CPU uses the readback f32 local direction transformed by the original f64 local frame and normalized by Ray::new; canonical unquantized terrain, profile and physical markers remain the CPU model. Decorative signs are excluded from CPU picking.",
        "adapter":format!("{:?}",adapter.get_info()),"dimensions":[1920,1080],"workgroup_size":32,
        "reference_checkpoint":CHECKPOINT,
        "comparison_scope":if strict||custom_candidate {"The modified candidate is compared with the original reference checkpoint; differing terminals are expected and are not a compiler-context parity failure. The unchanged baseline still checks exact retained context parity."} else {"Both shaders are the retained reference checkpoint; terminal comparisons measure compiler-context parity."},
        "physical_faces":packet.triangle_count,"markers":case.markers.len(),
        "origin_ecef_m":[local.origin.x,local.origin.y,local.origin.z],"runs":runs});
    std::fs::write(
        format!("{output_prefix}.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    println!("Wrote {output_prefix}.json");
}
