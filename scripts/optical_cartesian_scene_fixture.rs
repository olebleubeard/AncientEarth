//! Original MIT. Static certified-Cartesian consumer adapter. Each output is
//! one ordered piece of a retained accepted interval, never a complete ray.
//! The original 495 curve records and certified piece bytes remain verbatim.
use super::CurvesFixture;
use serde_json::{Value, json};

const PACKET: &str = "artifacts/optical-cartesian-curve-packet-jet";
pub fn build_covered() -> CurvesFixture {
    let mut fixture = build();
    let span = function(&fixture.scene.shader, "nc_scene_h0");
    let replacement =
        std::fs::read_to_string("scripts/optical_cartesian_coverage_stagnation.wgsl").unwrap();
    assert_eq!(replacement.matches("fn nc_scene_h0(").count(), 1);
    assert_eq!(replacement.matches("nc_scene_coverage(").count(), 1);
    fixture.scene.shader.replace_range(span, &replacement);
    fixture.scene.cases["h0_coverage_before_width"] = json!({
        "scope":"At a valid interval-Newton fixed enclosure, the unchanged all-points coverage proof may suppress reference h0 without returning a contact/state. Uncovered or ambiguous cases retain the original numerical width, total charge and unresolved rules. One coverage call site; no geometry or tolerance changes.",
        "source":"scripts/optical_cartesian_coverage_stagnation.wgsl"
    });
    fixture
}
fn replace(source: &mut String, old: &str, new: &str, count: usize) {
    assert_eq!(
        source.matches(old).count(),
        count,
        "Cartesian adapter drift: {old}"
    );
    *source = source.replace(old, new);
}
fn function(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        if *byte == b'{' {
            depth += 1;
        }
        if *byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return start..open + offset + 1;
            }
        }
    }
    panic!("Unterminated function {name}")
}
fn edit_function(source: &mut String, name: &str, change: impl FnOnce(&mut String)) {
    let span = function(source, name);
    let mut body = source[span.clone()].to_owned();
    change(&mut body);
    source.replace_range(span, &body);
}
fn words(bytes: &[u8]) -> Vec<u32> {
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .collect()
}

pub fn build() -> CurvesFixture {
    let mut base = super::build_curves();
    let packet: Value =
        serde_json::from_slice(&std::fs::read(format!("{PACKET}.json")).unwrap()).unwrap();
    let piece_bytes = std::fs::read(format!("{PACKET}-pieces.bin")).unwrap();
    let packed = words(&piece_bytes);
    assert_eq!(packet["schema"], "ancientearth.cartesian-scene-curves.v1");
    assert_eq!(packet["curve_count"], 495);
    assert_eq!(packet["record_words"], 80);
    assert_eq!(packet["scalar_majorant"], false);
    let count = u32::try_from(packet["piece_count"].as_u64().unwrap()).unwrap();
    assert_eq!(packed.len(), count as usize * 80);
    assert_eq!(base.curves.len(), 495 * 112 * 4);
    let original = words(&base.curves);
    let metadata = packet["curves"].as_array().unwrap();
    assert_eq!(metadata.len(), 495);
    let mut cursor = 0usize;
    for (curve, row) in metadata.iter().enumerate() {
        assert_eq!(row["curve_index"].as_u64().unwrap(), curve as u64);
        assert_eq!(row["first_piece"].as_u64().unwrap(), cursor as u64);
        let pieces = u32::try_from(row["pieces"].as_u64().unwrap()).unwrap();
        assert!(pieces.is_power_of_two() && pieces <= 64);
        assert_eq!(
            row["capture_ray"].as_u64().unwrap(),
            original[curve * 112 + 110] as u64
        );
        assert_eq!(
            row["capture_trial"].as_u64().unwrap(),
            original[curve * 112 + 111] as u64
        );
        for piece in 0..pieces {
            let v = &packed[cursor * 80..(cursor + 1) * 80];
            assert_eq!(&v[76..80], &[curve as u32, piece, pieces, 0]);
            assert_eq!(v[0], ((piece as f32) / pieces as f32).to_bits());
            assert_eq!(v[1], (((piece + 1) as f32) / pieces as f32).to_bits());
            assert_eq!(v[2], (pieces as f32).to_bits());
            assert_eq!(
                (f32::from_bits(v[1]) - f32::from_bits(v[0])) * f32::from_bits(v[2]),
                1.0
            );
            assert!(v[..76].iter().all(|w| f32::from_bits(*w).is_finite()));
            assert!(f32::from_bits(v[3]) >= 0.0);
            for lower in (3..=9).step_by(2).chain((11..=15).step_by(2)) {
                for axis in 0..3 {
                    assert!(
                        f32::from_bits(v[4 * lower + axis])
                            <= f32::from_bits(v[4 * (lower + 1) + axis])
                    );
                }
            }
            for vector in 1..19 {
                assert_eq!(v[4 * vector + 3], 0);
            }
            assert!(
                v[68..71]
                    .iter()
                    .chain(&v[72..75])
                    .all(|w| f32::from_bits(*w) >= 0.0)
            );
            cursor += 1;
        }
    }
    assert_eq!(cursor, count as usize);
    let piece_base = u32::try_from(base.curves.len() / 16).unwrap();
    base.curves.extend_from_slice(&piece_bytes);
    let mut shader = base.scene.shader.replace("\r\n", "\n");
    shader.push('\n');
    shader.push_str(&std::fs::read_to_string("scripts/optical_cartesian_curve.wgsl").unwrap());
    shader.push_str(LOADERS);
    edit_function(&mut shader, "nc_face_point", |body| {
        // nc_scene_face constructs exactly the identity eye-frame rows.
        // Keep that loader invariant explicit below; only the per-eye static
        // diagnostic uses this specialized cancellation order.
        replace(
            body,
            "return nc_m_add(frame.shift,nc_face_transform(frame,padded));",
            "return nc_cartesian_relative_point(nc_cartesian_piece,NcMapPoint(padded,point.velocity,1u),frame.shift);",
            1,
        );
    });
    let face_loader = &shader[function(&shader, "nc_scene_face")];
    assert!(face_loader.contains("NcFaceFrame(nc_m_exact(vec3(1.0,0.0,0.0)),nc_m_exact(vec3(0.0,1.0,0.0)),nc_m_exact(vec3(0.0,0.0,1.0)),"));
    edit_function(&mut shader, "nc_face_contact_cached", |body| {
        replace(
            body,
            "let duration=vec2(curve.theta_end);",
            "let duration=nc_f_sub(vec2(curve.theta_end),vec2(nc_cartesian_piece.parameter.x));",
            1,
        );
        replace(
            body,
            "var enclosure=vec2(0.0,curve.theta_end);",
            "var enclosure=vec2(nc_cartesian_piece.parameter.x,curve.theta_end);",
            1,
        );
        replace(
            body,
            "nc_map_curve_point(curve,evaluation_theta,parameters)",
            "nc_cartesian_local_point(nc_cartesian_piece,evaluation_theta)",
            1,
        );
    });
    for name in ["nc_scene_contact_charge", "nc_scene_coverage"] {
        edit_function(&mut shader, name, |body| {
            replace(body, "nc_map_curve_point(", "nc_cartesian_mapped_point(", 1);
        });
    }
    edit_function(&mut shader, "nc_scene_h0", |body| {
        replace(
            body,
            "nc_scene_height(curve,vec2(0.0),parameters)",
            "nc_scene_height(curve,vec2(nc_cartesian_piece.parameter.x),parameters)",
            1,
        );
        replace(
            body,
            "var enclosure=vec2(0.0,curve.theta_end);",
            "var enclosure=vec2(nc_cartesian_piece.parameter.x,curve.theta_end);",
            1,
        );
    });
    edit_function(&mut shader, "nc_scene_query_cached", |body| {
        replace(
            body,
            "let start=nc_map_curve_point(curve,vec2(0.0),parameters);let end=nc_map_curve_point(curve,vec2(curve.theta_end),parameters);",
            "let start_local=nc_cartesian_local_point(nc_cartesian_piece,vec2(nc_cartesian_piece.parameter.x));let end_local=nc_cartesian_local_point(nc_cartesian_piece,vec2(curve.theta_end));\n    let start=nc_cartesian_absolute_point(nc_cartesian_piece,start_local);let end=nc_cartesian_absolute_point(nc_cartesian_piece,end_local);",
            1,
        );
        replace(
            body,
            "let context=NcFaceContext(start,end,acceleration);",
            "let context=NcFaceContext(start_local,end_local,acceleration);",
            1,
        );
        // Existing duration theta_end is a conservative upper bound on this
        // piece's shorter duration in the broadphase sag. Preserve it.
    });
    edit_function(&mut shader, "normal_scene", |body| {
        replace(body, "495u", &format!("{count}u"), 2);
        replace(
            body,
            "let base=id.x*28u;",
            &format!(
                "let piece_base={piece_base}u+20u*id.x;let identity=nc_curve_inputs[piece_base+19u];let base=identity.x*28u;nc_cartesian_piece=nc_cartesian_load(piece_base);"
            ),
            1,
        );
        replace(
            body,
            "let curve=nc_scene_from_height(start,height,bitcast<f32>(accepted.y));",
            "var curve=nc_scene_from_height(start,height,bitcast<f32>(accepted.y));curve.theta_end=nc_cartesian_piece.parameter.y;",
            1,
        );
        replace(
            body,
            "var theta=0.0;",
            "var theta=nc_cartesian_piece.parameter.x;",
            1,
        );
    });
    let control = [count, 1, 0x12345678, 0x9abcdef0];
    base.scene.primary_uniform[176..192].copy_from_slice(
        &control
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    base.scene.shader = shader;
    base.scene.width = count;
    base.scene.height = 1;
    base.scene.record_count = count;
    base.scene.dispatch = [count.div_ceil(32), 1, 1];
    base.scene.control = control;
    base.scene.terminal_bytes = 16 + 48 * u64::from(count);
    base.scene.output_bytes = 16 + 80 * u64::from(count);
    base.scene.sample_pixels = (0..count).map(|i| [i, 0]).collect();
    base.scene.sample_indices = (0..count).collect();
    let retained = base.scene.cases.clone();
    base.scene.cases = json!({
        "scope":"Unused static Cartesian scene consumer. Each output is one certified piece, independently queried before any producer commit; status4 means clear piece, never sky. All earlier pieces and intervals still require chronological reduction before full-ray acceptance. Existing numerical/source allowance, polynomial fit/packing/evaluation intervals and source-face uncertainty all remain in complete contact charge and ordering. No production or performance acceptance.",
        "cartesian_packet":packet,"cartesian_packet_prefix":PACKET,"retained_curve_fixture":retained,
        "binding6_layout":{"original_curve_records":495,"original_words_per_record":112,"piece_base_vec4":piece_base,"piece_record_vec4":20,"pieces":count},
        "identity_face_rows":"Current retained per-eye face loader is asserted identity; anchored split-origin folding deliberately does not apply to an arbitrary patch transform.",
        "record_layout":["16-byte current control header","48-byte per-piece terminal-shaped output, original interval-local path","32-byte details: eventkind/visits/capture ray/trial; theta/lower/upper/tolerance"]
    });
    base
}

const LOADERS: &str = r#"
var<private> nc_cartesian_piece:NcCartesianPiece;
fn nc_cartesian_input_vector(address:u32)->NcMapVec{
    let lower=bitcast<vec4<f32>>(nc_curve_inputs[address]).xyz;let upper=bitcast<vec4<f32>>(nc_curve_inputs[address+1u]).xyz;
    return NcMapVec(vec2(lower.x,upper.x),vec2(lower.y,upper.y),vec2(lower.z,upper.z));
}
fn nc_cartesian_load(address:u32)->NcCartesianPiece{
    return NcCartesianPiece(bitcast<vec4<f32>>(nc_curve_inputs[address+1u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[address+2u]).xyz,
        nc_cartesian_input_vector(address+3u),nc_cartesian_input_vector(address+5u),nc_cartesian_input_vector(address+7u),nc_cartesian_input_vector(address+9u),
        nc_cartesian_input_vector(address+11u),nc_cartesian_input_vector(address+13u),nc_cartesian_input_vector(address+15u),
        bitcast<vec4<f32>>(nc_curve_inputs[address+17u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[address+18u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[address]),1u);
}
fn nc_cartesian_mapped_point(curve:NcSceneCurve,theta:vec2<f32>,parameters:NcMapParameters)->NcMapPoint{
    return nc_cartesian_absolute_point(nc_cartesian_piece,nc_cartesian_local_point(nc_cartesian_piece,theta));
}
"#;
