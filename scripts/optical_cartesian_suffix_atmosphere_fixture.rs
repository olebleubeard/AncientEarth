//! Original MIT. The existing eighteen atmosphere-only controls, repacked for
//! the selected suffix ABI. No terrain/marker visibility claim follows here.
use super::TraceFixture;
use earth_math::{
    EnuBasis, Geodetic, MIN_HEIGHT_M, Observer, TerrainScene, WGS84_A_M, WGS84_E2,
    atmosphere::{
        AtmosphereLayer, AtmosphereProfile,
        gpu::{GpuEllipsoid, scene::PackedScene},
    },
    rendering::LocalFrame,
};
use serde_json::json;

pub const CASES: [&str; 6] = [
    "atmosphere-minus20-vacuum",
    "atmosphere-minus20-gradient",
    "atmosphere-100-vacuum",
    "atmosphere-100-gradient",
    "atmosphere-5000-vacuum",
    "atmosphere-5000-gradient",
];

fn record(bytes: &mut [u8], index: usize, value: [f32; 4]) {
    for (component, value) in value.into_iter().enumerate() {
        bytes[index * 16 + component * 4..index * 16 + component * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
}
fn bytes(values: &[[f32; 4]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect()
}
fn replace_body(source: &mut String, name: &str, body: &str) {
    let signature = format!("fn {name}(");
    let count = source.matches(&signature).count();
    assert!(count <= 1, "Atmosphere function drift: {name}");
    if count == 0 {
        return;
    } // Already absent from this module's call closure.
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    let mut close = None;
    for (offset, value) in source.as_bytes()[open..].iter().enumerate() {
        if *value == b'{' {
            depth += 1;
        }
        if *value == b'}' {
            depth -= 1;
            if depth == 0 {
                close = Some(open + offset);
                break;
            }
        }
    }
    source.replace_range(open + 1..close.expect("Unclosed atmosphere function"), body);
}

pub fn apply(case: &str, fixture: &mut TraceFixture) {
    let index = CASES
        .iter()
        .position(|name| *name == case)
        .expect("Unknown atmosphere control");
    let height = [-20.0, 100.0, 5000.0][index / 2];
    let bend = index % 2 == 1;
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 0.0).unwrap());
    observer.feet.height_m = height - observer.eye_height_m;
    assert_eq!(observer.eye_geodetic().height_m, height);
    // Exact layer values and packing formula from optical_gpu_probe.rs::profile
    // and ::run, whose existing atmosphere scope owns the absolute error limits.
    let profile = AtmosphereProfile::from_layers(if bend {
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
    .unwrap();
    let layers = profile.layers();
    let gradient = layers
        .windows(2)
        .map(|p| (p[1].refractivity - p[0].refractivity).abs() / (p[1].height_m - p[0].height_m))
        .fold(0.0_f64, f64::max);
    let gpu = GpuEllipsoid::at(observer);
    let scene = &mut fixture.suffix.scene;
    assert_eq!(scene.primary_uniform.len(), 2240);
    assert_eq!(scene.record_count, 23);
    for (i, row) in gpu.rows.into_iter().enumerate() {
        record(&mut scene.primary_uniform, i, row);
    }
    record(&mut scene.primary_uniform, 3, gpu.origin);
    record(&mut scene.primary_uniform, 4, gpu.center);
    record(
        &mut scene.primary_uniform,
        9,
        [
            layers.len() as f32,
            profile.top_height_m() as f32,
            (gradient + 1.0 / (WGS84_A_M * (1.0 - WGS84_E2) + MIN_HEIGHT_M)) as f32,
            1.0,
        ],
    );
    record(
        &mut scene.primary_uniform,
        10,
        [
            gradient as f32,
            profile.base_height_m() as f32,
            MIN_HEIGHT_M as f32,
            0.0,
        ],
    );
    for i in 0..128 {
        let value = if i < layers.len() {
            let l = layers[i];
            let slope = layers.get(i + 1).map_or(0.0, |n| {
                (n.refractivity - l.refractivity) / (n.height_m - l.height_m)
            });
            [
                (l.height_m + profile.base_height_m()) as f32,
                l.refractivity as f32,
                slope as f32,
                0.0,
            ]
        } else {
            [0.0; 4]
        };
        record(&mut scene.primary_uniform, 12 + i, value);
    }
    assert_eq!(
        f32::from_le_bytes(scene.primary_uniform[136..140].try_into().unwrap()),
        0.0,
        "Atmosphere controls require the original rectangular lens"
    );
    let local = LocalFrame::at(observer);
    let basis = EnuBasis::at(observer.eye_geodetic());
    let rays = [
        basis.up,
        (basis.up + basis.east * 0.5).normalized().unwrap(),
        (basis.up + basis.north).normalized().unwrap(),
    ];
    let ray_words: Vec<_> = rays
        .into_iter()
        .map(|r| local.direction(r).map(f32::to_bits))
        .collect();
    let declarations = ray_words
        .iter()
        .map(|w| {
            format!(
                "bitcast<vec3<f32>>(vec3<u32>({}u,{}u,{}u))",
                w[0], w[1], w[2]
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let old = "let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);";
    let new = format!(
        "let atmosphere_rays=array<vec3<f32>,3>({declarations});let direction=normalize(atmosphere_rays[id.x%3u]);"
    );
    let prefix = &mut fixture.suffix.modules[0].shader;
    assert_eq!(
        prefix.matches(old).count(),
        1,
        "Atmosphere launch source drift"
    );
    *prefix = prefix.replacen(old, &new, 1);
    // Preserve the exact old atmosphere-only control semantics: no scene
    // contact, h0 covered. This is not a primary terrain-visibility fixture.
    for module in &mut fixture.suffix.modules {
        assert!(
            !module.shader.contains("nc_settings."),
            "Canonical field became live"
        );
        let expected = match module.name {
            "prefix" | "fallback" => 1,
            "suffix" => 0,
            _ => panic!("Unknown atmosphere shader module"),
        };
        for name in ["scene_event", "terrain_coverage"] {
            assert_eq!(
                module.shader.matches(&format!("fn {name}(")).count(),
                expected,
                "Atmosphere stub closure drift: {} {name}",
                module.name
            );
        }
        replace_body(
            &mut module.shader,
            "scene_event",
            "return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);",
        );
        replace_body(&mut module.shader, "terrain_coverage", "return 1u;");
    }
    // The sufficient-only clearance reader still receives coherent metadata;
    // no original runtime geometry guard is bypassed or weakened.
    let empty = PackedScene::prepare(&TerrainScene::new(vec![])).unwrap();
    let frames = empty.frame_data(local, &[]).unwrap();
    assert_eq!(empty.geometry.len(), 1);
    assert_eq!(frames.len(), 6);
    assert_eq!(frames[0].map(f32::to_bits), [0; 4]);
    assert_eq!(frames[1][0].to_bits(), 6);
    assert_eq!(frames[2][3].to_bits(), 0);
    scene.geometry = bytes(&empty.geometry);
    scene.frames = bytes(&frames);
    scene.shader = fixture.suffix.modules[0].shader.clone();
    let origin = observer.eye_position();
    scene.cases["cartesian_suffix_atmosphere"] = json!({
        "case":case,"source":"scripts/optical_cartesian_suffix_atmosphere_fixture.rs",
        "prior_fixture":"scripts/optical_gpu_probe.rs atmosphere_only18clearcases",
        "eye_geodetic":[0.0,0.0,height],"origin_ecef_m":[origin.x,origin.y,origin.z],
        "base_height_m":profile.base_height_m(),"top_height_m":profile.top_height_m(),
        "layers":layers.iter().map(|l|[l.height_m,l.refractivity]).collect::<Vec<_>>(),
        "raw_local_launch_words":ray_words,"launch_mapping":"record_id modulo3; normalize exactly once in the unchanged probe convention",
        "unique_rays":3,"duplicate_records":20,
        "scope":"Reuses the original eighteen atmosphere-only fields and directions in six packets. The fixed23-record transport ABI repeats three distinct rays; nominal sample pixel labels no longer describe a camera projection. No scene contact and covered h0 are explicit inherited diagnostic stubs, never a production visibility policy.",
        "expected":"All three unique rays exit the profile top. The original baseline must pass the retained atmosphere5m endpoint,5m path,3e-6 direction gates. Original/fallback terminals must match that baseline exactly. Accepted suffixes must pass BOTH those absolute gates and the prospective same-ray no-worse-than-original plus independently measured reference-convergence rule. A failing original absolute check blocks this case's regression gate; it does not authorize a wider limit.",
        "absolute_limits":{"position_m":5.0,"path_m":5.0,"direction":3e-6},
        "not_inherited":"The other two oldstellar controls distinguish visibility and lower-domain stellar behavior; they are not reclassified as primary clear-sky results. Actual terrain/marker and physical h0 fallback are tested in separate real-scene/vacuum branches.",
        "live_field":"Same original packed quadric/layer equations through PrimarySettings; canonical field/certificate buffers remain unreachable and make no synthetic-profile claim.",
        "factor":0.25,"candidate_original_baseline_required_before_readback":true
    });
}
