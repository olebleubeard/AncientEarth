//! Original MIT. Math-owned immutable fixture for requested-star scheduling.
//! Identity catalogue/local rotation and no luminous body occluders are explicit
//! test conditions. Final native rendering still performs Sun/Moon occlusion.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
use earth_math::{
    Projection,
    atmosphere::gpu::{GpuEllipsoid, scene::PackedScene},
    rendering::{LocalFrame, stars},
};

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const STAR_COUNT: u32 = 3000;
pub const GROUP: u32 = 32;
pub const CONTROL: [u32; 4] = [WIDTH, HEIGHT, 0x12345678, 0x9abcdef0];
pub const PRIMARY_SOURCE: &str = "artifacts/optical-selected-launch-input-baseline.wgsl";
pub const ALL_STAR_SOURCE: &str = "artifacts/optical-stellar-table-probe.wgsl";

pub struct Fixture {
    /// All buffers use vec4 alignment; uniform appends control then lens.
    pub settings: Vec<[f32; 4]>,
    pub catalogue: Vec<[f32; 4]>,
    pub view: Vec<[f32; 4]>,
    pub geometry: Vec<[f32; 4]>,
    pub frames: Vec<[f32; 4]>,
    pub primary_shader: String,
    pub stellar_shader: String,
    pub faces: usize,
    pub markers: usize,
}
pub fn bytes(v: &[[f32; 4]]) -> Vec<u8> {
    source::bytes(v)
}
pub fn terminal_bytes() -> u64 {
    16 + u64::from(WIDTH) * u64::from(HEIGHT) * 48
}
pub fn arrival_bytes() -> u64 {
    16 + u64::from(STAR_COUNT) * 16
}
pub fn request_bytes() -> u64 {
    u64::from(STAR_COUNT) * 4
}
pub fn consumer_bytes() -> u64 {
    u64::from(STAR_COUNT) * 48
}
pub fn pixel_color_bytes() -> u64 {
    u64::from(WIDTH) * u64::from(HEIGHT) * 16
}
pub fn compact_request_bytes() -> u64 {
    // 3000 flags, one append counter, then 3000 packed-record offsets.
    u64::from(2 * STAR_COUNT + 1) * 4
}
fn replace_once(text: &mut String, old: &str, new: &str) {
    assert_eq!(
        text.matches(old).count(),
        1,
        "Fixture source changed: {old}"
    );
    *text = text.replacen(old, new, 1);
}

pub fn fixture() -> Fixture {
    assert_eq!(stars::STAR_COUNT, STAR_COUNT as usize);
    assert_eq!(stars::STAR_GRID, 32);
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
            (gradient
                + 1.0
                    / (earth_math::WGS84_A_M * (1.0 - earth_math::WGS84_E2)
                        + earth_math::MIN_HEIGHT_M)) as f32,
            1.0,
        ],
        [
            gradient as f32,
            case.profile.base_height_m() as f32,
            earth_math::MIN_HEIGHT_M as f32,
            0.0,
        ],
    ];
    for i in 0..128 {
        settings.push(if let Some(l) = layers.get(i) {
            let slope = layers.get(i + 1).map_or(0.0, |n| {
                (n.refractivity - l.refractivity) / (n.height_m - l.height_m)
            });
            [
                (case.profile.base_height_m() + l.height_m) as f32,
                l.refractivity as f32,
                slope as f32,
                0.0,
            ]
        } else {
            [0.0; 4]
        });
    }
    settings.push(g.latitude);
    settings.extend([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        CONTROL.map(f32::from_bits),
    ]);
    let demo = earth_math::survey::nearby_demo(&case.scene).unwrap();
    let lens = demo.lens;
    let optical_lens = [
        WIDTH as f32 / HEIGHT as f32,
        (lens.vertical_fov_deg.to_radians() * 0.5) as f32,
        u8::from(lens.projection == Projection::EquidistantFisheye) as f32,
        if lens.projection == Projection::EquidistantFisheye {
            (f64::from(HEIGHT) / lens.vertical_fov_deg.to_radians()) as f32
        } else {
            earth_math::rendering::focal_pixels(lens.vertical_fov_deg, HEIGHT) as f32
        },
    ];
    settings.push(optical_lens);
    let frame = case.observer.frame();
    let vector = |v| {
        let a = local.direction(v);
        [a[0], a[1], a[2], 0.0]
    };
    let exposure = 1.0 / 60.0;
    let phase = stars::display_phases(0.0, exposure);
    settings.extend([
        vector(frame.right),
        vector(frame.up),
        vector(frame.forward),
        phase[0],
        phase[1],
        [
            stars::scintillation::reference_zenith_variance(case.observer.eye_geodetic().height_m)
                .unwrap_or(0.0) as f32,
            exposure as f32,
            WIDTH as f32,
            HEIGHT as f32,
        ],
        [0.0, 1.0, 1.0, 0.0],
    ]);
    let view = vec![
        vector(frame.right),
        vector(frame.up),
        vector(frame.forward),
        optical_lens,
        [WIDTH as f32, HEIGHT as f32, 0.0, 0.0],
    ];
    let catalogue = stars::packed_catalogue();
    assert_eq!(catalogue.len(), 6144 + 2 * STAR_COUNT as usize);
    let mut ids = vec![false; STAR_COUNT as usize];
    for pair in catalogue[6144..].chunks_exact(2) {
        let id = pair[1][3] as usize;
        assert_eq!(id as f32, pair[1][3]);
        assert!(id < ids.len() && !ids[id]);
        ids[id] = true;
    }
    assert!(ids.into_iter().all(|v| v));
    // Retained physical midpoint code and pixel-centre expression are untouched.
    // Only prepend the same 16-byte token used by the native primary buffer.
    let mut primary_shader = std::fs::read_to_string(PRIMARY_SOURCE).unwrap();
    assert_eq!(primary_shader.matches("id.x*3u").count(), 6);
    primary_shader = primary_shader.replace("id.x*3u", "1u+id.x*3u");
    replace_once(
        &mut primary_shader,
        "if id.x>=dimensions.x*dimensions.y{return;}",
        &format!(
            "if id.x>=dimensions.x*dimensions.y{{return;}}if id.x==0u{{output[0]=vec4({}u,{}u,{}u,{}u);}}",
            CONTROL[0], CONTROL[1], CONTROL[2], CONTROL[3]
        ),
    );
    let mut stellar_shader = std::fs::read_to_string(ALL_STAR_SOURCE).unwrap();
    stellar_shader.truncate(stellar_shader.find("@compute").unwrap());
    replace_once(
        &mut stellar_shader,
        "control:vec4<u32>}",
        "control:vec4<u32>,lens:vec4<f32>,right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,star_sin:vec4<f32>,star_cos:vec4<f32>,star_environment:vec4<f32>,environment:vec4<f32>}",
    );
    let common = std::fs::read_to_string("src/desktop/shaders/stars_common.wesl").unwrap();
    stellar_shader.push_str(&common);
    stellar_shader
        .push_str(&std::fs::read_to_string("scripts/optical_stellar_requested.wgsl").unwrap());
    let production = std::fs::read_to_string("src/desktop/shaders/refraction.wesl").unwrap();
    let start = production.find("fn catalogue_stars(").unwrap();
    let end = start
        + production[start..]
            .find("fn shared_stellar_arrival(")
            .unwrap();
    let direct = production[start..end]
        .replace("fn catalogue_stars(", "fn direct_catalogue_stars(")
        .replace("star_data", "rays")
        .replace(
            "shared_stellar_arrival(stellar_direction,u32(tint.w+0.5))",
            "inverse_star(stellar_direction)",
        );
    let mut requested = direct.replace(
        "fn direct_catalogue_stars(",
        "fn requested_catalogue_stars(",
    );
    replace_once(
        &mut requested,
        "let arrival=inverse_star(stellar_direction);",
        "let arrival=requested_arrival(star,tint);",
    );
    stellar_shader.push_str(&direct);
    stellar_shader.push_str(&requested);
    stellar_shader
        .push_str(&std::fs::read_to_string("scripts/optical_stellar_pixels.wgsl").unwrap());
    Fixture {
        settings,
        catalogue,
        view,
        geometry: packet.geometry,
        frames,
        primary_shader,
        stellar_shader,
        faces: packet.triangle_count,
        markers: case.markers.len(),
    }
}

/// Same candidate and inverse arithmetic with a compact independent-ID queue.
/// Atomic first-request ownership bounds the append count by STAR_COUNT. The
/// overflow guard still withdraws the output token if that invariant fails.
pub fn compact_fixture() -> Fixture {
    let mut value = fixture();
    replace_once(
        &mut value.stellar_shader,
        "if id.x==0u{output[0]=vec4(0u);}",
        "if id.x==0u{output[0]=vec4(0u);atomicStore(&stellar_requests[3000u],0u);}",
    );
    replace_once(
        &mut value.stellar_shader,
        "atomicOr(&stellar_requests[u32(tint.w+0.5)],1u);",
        "let old=atomicOr(&stellar_requests[u32(tint.w+0.5)],1u);if old==0u{let slot=atomicAdd(&stellar_requests[3000u],1u);if slot<3000u{atomicStore(&stellar_requests[3001u+slot],u32(bin.x)+2u*index);}}",
    );
    replace_once(
        &mut value.stellar_shader,
        "if id.x==0u{output[0]=settings.control;}\n    let star=rays[6144u+2u*id.x];let tint=rays[6144u+2u*id.x+1u];",
        "let count=atomicLoad(&stellar_requests[3000u]);if count>3000u{if id.x==0u{output[0]=vec4(0u);}return;}if id.x==0u{output[0]=settings.control;}if id.x>=count{return;}\n    let offset=atomicLoad(&stellar_requests[3001u+id.x]);let star=rays[offset];let tint=rays[offset+1u];",
    );
    value
}

// Binding contracts (explicit layouts are allowed; unused bindings remain valid):
// primary performance:0 uniform settings prefix;1 view RO;2 terminal RW;3 geometry RO;4frames RO.
// stellar entries:0 uniform settings;1 catalogue RO;2arrival RW;3terminal RO;4atomic requests RW.
// Ordering: primary once (untimed fixture); each measured frame clear→gather→solve.
// clear_stellar_requests, requested_stellar_table:ceil(3000/32),1,1.
// gather_stellar_requests:ceil(WIDTH*HEIGHT/32),1,1.
// The consumer requires current token and interprets states0/1/2 as documented
// in optical_stellar_requested.wgsl. A record0 must use original inverse fallback.
