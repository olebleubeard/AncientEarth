//! Original MIT. CPU-only serialization check for the math-owned candidate.
//! The fixture owns every physical input and the expected byte representation.
#[path = "optical_normal_packet_fixture.rs"]
mod fixture;
#[path = "../src/desktop/renderer/refraction/normal.rs"]
mod normal;

use bevy::render::render_resource::{ShaderType, encase};
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    assert_eq!(args.next().as_deref(), Some("--name"));
    let name = args.next().expect("--name requires an evidence name");
    assert!(args.next().is_none(), "Unknown argument");
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "Invalid evidence name"
    );
    let prefix = format!("artifacts/{name}");
    for suffix in ["-actual.bin", "-expected.bin", ".json"] {
        assert!(
            !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve existing evidence"
        );
    }
    let fixture = fixture::build();
    let mut buffer = encase::UniformBuffer::new(Vec::new());
    buffer.write(&fixture.settings).unwrap();
    let actual = buffer.into_inner();
    let shader_size = normal::FieldSettings::min_size().get();
    let size_matches =
        shader_size == normal::FIELD_BYTES && actual.len() as u64 == normal::FIELD_BYTES;
    let bytes_match = actual == fixture.expected;
    // Preserve both serializations and the comparison before any assertion.
    std::fs::write(format!("{prefix}-actual.bin"), &actual).unwrap();
    std::fs::write(format!("{prefix}-expected.bin"), &fixture.expected).unwrap();
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&json!({
            "scope":"CPU-only host ABI and byte serialization. No GPU execution or application performance claim.",
            "fixture":fixture.metadata,
            "shader_type_bytes":shader_size,
            "declared_bytes":normal::FIELD_BYTES,
            "actual_bytes":actual.len(),
            "expected_bytes":fixture.expected.len(),
            "sizes_match":size_matches,
            "bytes_match":bytes_match,
            "passed":size_matches && bytes_match,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        size_matches,
        "Candidate uniform ABI differs; evidence retained"
    );
    assert!(
        bytes_match,
        "Candidate serialization differs; evidence retained"
    );
    println!("CPU-only candidate packet matches retained uniform bytes");
}
