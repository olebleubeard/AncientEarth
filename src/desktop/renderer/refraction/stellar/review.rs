//! Independent byte-layout and source-sharing regression for the stellar packet.
use super::*;

#[test]
fn stellar_uniform_retains_field_bits_and_full_token_without_shading_dependencies() {
    use bevy::render::render_resource::encase;
    let record = |i: u32| {
        Vec4::from_array(std::array::from_fn(|j| {
            f32::from_bits(0x3e00_0000 + 16 * i + j as u32)
        }))
    };
    let mut view = Settings {
        q0: record(0),
        q1: record(1),
        q2: record(2),
        origin: record(3),
        center: record(4),
        budget: record(5),
        scene: record(6),
        lens: record(7),
        icrf0: record(8),
        icrf1: record(9),
        icrf2: record(10),
        environment: record(11),
        layers: std::array::from_fn(|i| record(100 + i as u32)),
        right: record(500),
        up: record(501),
        forward: record(502),
        sun: record(503),
        control: UVec4::new(1, 2, 3, 4),
        ..Settings::default()
    };
    let token = UVec4::new(33, 5, 0x8000_0001, 0xffff_fff1);
    let packet = StellarSettings::from_view(&view, token);
    let mut buffer = encase::UniformBuffer::new(Vec::new());
    buffer.write(&packet).unwrap();
    let bytes = buffer.into_inner();
    let actual: Vec<_> = bytes
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let mut expected = Vec::new();
    for i in 0..12 {
        expected.extend(record(i).to_array().map(f32::to_bits));
    }
    expected.extend(token.to_array());
    for i in 0..128 {
        expected.extend(record(100 + i).to_array().map(f32::to_bits));
    }
    assert_eq!(actual, expected);
    assert_eq!(bytes.len(), 2256);
    assert_eq!(REQUEST_BYTES, 12_000);
    assert_eq!(ARRIVAL_BYTES, 48_016);
    assert_eq!(STAR_GROUPS, 94);
    let covered_ids = (0..STAR_GROUPS * 32).filter(|&id| id < 3000).count();
    assert_eq!(covered_ids, 3000);
    for (weight, wanted) in [
        (-1., false),
        (0., false),
        (f32::MIN_POSITIVE, true),
        (1., true),
        (f32::NAN, false),
    ] {
        view.environment.z = weight;
        assert_eq!(eligible(&view), wanted);
    }
    view.environment.z = 1.;
    for dims in [(0, 1), (1, 0), (0, 0)] {
        view.control.x = dims.0;
        view.control.y = dims.1;
        assert!(!eligible(&view));
    }

    let shader = stellar_shader();
    let original = include_str!("../../../shaders/refraction.wesl");
    // Inspect the actual assembled program: atmosphere and inverse come from
    // production, while finite-scene intersection/radiance are absent here.
    let core = &original[original.find("fn qmul(").unwrap()..original.find("fn trace(").unwrap()];
    assert!(shader.contains(core));
    for name in [
        "inverse_star",
        "trace_atmosphere",
        "star_face",
        "gather_stellar_requests",
        "requested_stellar_table",
    ] {
        assert_eq!(shader.matches(&format!("fn {name}(")).count(), 1);
    }
    assert!(!shader.contains("fn trace("));
    assert!(!shader.contains("fn radiance("));
    assert!(!shader.contains("fn scene_event("));
    assert!(shader.contains("let pixel_index=id.y*settings.control.x+id.x;"));
    assert!(shader.contains("primary_terminals[1u+3u*pixel_index+1u]"));
}
