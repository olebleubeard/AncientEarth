//! Original MIT. Local manufactured canonical contact curves against the
//! unchanged actual source scene. No synthesized record is called an ODE
//! trial. Per-piece first-contact decisions remain separate from ray tracing.
use super::CurvesFixture;
use serde_json::{Value, json};
const PREFIX: &str = "artifacts/optical-positive-contact-packet-first";
fn words(bytes: &[u8]) -> Vec<u32> {
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn function(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (index, b) in source.as_bytes()[open..].iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        }
        if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                return start..open + index + 1;
            }
        }
    }
    panic!("Missing function close")
}
pub fn build() -> CurvesFixture {
    let mut fixture = super::build_cartesian_covered();
    let metadata: Value =
        serde_json::from_slice(&std::fs::read(format!("{PREFIX}.json")).unwrap()).unwrap();
    assert_eq!(
        metadata["schema"],
        "ancientearth.manufactured-canonical-contact.v1"
    );
    assert_eq!(
        metadata["fresh_canonical_face_certificates_all_bytes_equal"],
        true
    );
    assert_eq!(metadata["curve_record_words"], 64);
    assert_eq!(metadata["piece_record_words"], 80);
    let curves = std::fs::read(format!("{PREFIX}-curves.bin")).unwrap();
    let pieces = std::fs::read(format!("{PREFIX}-pieces.bin")).unwrap();
    let curve_words = words(&curves);
    let piece_words = words(&pieces);
    let count = u32::try_from(metadata["curve_count"].as_u64().unwrap()).unwrap();
    let piece_count = u32::try_from(metadata["piece_count"].as_u64().unwrap()).unwrap();
    assert_eq!(count, 6);
    assert_eq!(curve_words.len(), count as usize * 64);
    assert_eq!(piece_words.len(), piece_count as usize * 80);
    let cases = metadata["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count as usize);
    let mut cursor = 0;
    for (case, row) in cases.iter().enumerate() {
        assert_eq!(row["case"].as_u64().unwrap(), case as u64);
        assert_eq!(row["first_piece"].as_u64().unwrap(), cursor as u64);
        let subdivisions = usize::try_from(row["pieces"].as_u64().unwrap()).unwrap();
        assert!(subdivisions.is_power_of_two() && subdivisions <= 64);
        let record = &curve_words[64 * case..64 * (case + 1)];
        assert_eq!(record[51], 1);
        assert_eq!(record[63], case as u32);
        assert_eq!(
            record[60],
            u32::try_from(row["source_certificate_vec4"].as_u64().unwrap()).unwrap()
        );
        assert_eq!(
            record[61],
            u32::try_from(row["source_material"].as_u64().unwrap()).unwrap()
        );
        assert_eq!(
            record[62],
            u32::try_from(row["source_owner"].as_u64().unwrap()).unwrap()
        );
        for (i, v) in record[..60].iter().enumerate() {
            if i != 51 {
                assert!(f32::from_bits(*v).is_finite());
            }
        }
        assert!(
            f32::from_bits(record[48]) >= 0.
                && f32::from_bits(record[49]) > f32::from_bits(record[48])
        );
        for piece in 0..subdivisions {
            let data = &piece_words[80 * (cursor + piece)..80 * (cursor + piece + 1)];
            assert_eq!(
                &data[76..],
                &[case as u32, piece as u32, subdivisions as u32, 0]
            );
            assert_eq!(data[0], (piece as f32 / subdivisions as f32).to_bits());
            assert_eq!(
                data[1],
                ((piece + 1) as f32 / subdivisions as f32).to_bits()
            );
            assert_eq!(data[2], (subdivisions as f32).to_bits());
            assert_eq!(
                (f32::from_bits(data[1]) - f32::from_bits(data[0])) * f32::from_bits(data[2]),
                1.
            );
            for v in &data[..76] {
                assert!(f32::from_bits(*v).is_finite());
            }
            for lower in [3, 5, 7, 9, 11, 13, 15] {
                for axis in 0..3 {
                    assert!(
                        f32::from_bits(data[lower * 4 + axis])
                            <= f32::from_bits(data[(lower + 1) * 4 + axis])
                    );
                }
            }
            for index in [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            ] {
                assert_eq!(data[index * 4 + 3], 0);
            }
            for v in data[68..71].iter().chain(&data[72..75]) {
                assert!(f32::from_bits(*v) >= 0.);
            }
        }
        cursor += subdivisions;
    }
    assert_eq!(cursor, piece_count as usize);
    let range = function(&fixture.scene.shader, "normal_scene");
    let entry = ENTRY
        .replace("COUNT", &format!("{piece_count}u"))
        .replace("PIECE_BASE", &format!("{}u", count * 16));
    // Existing compute/workgroup attributes remain before the replaced fn.
    fixture.scene.shader.replace_range(range, &entry);
    fixture.curves = curves;
    fixture.curves.extend_from_slice(&pieces);
    let control = [piece_count, 1, 0x12345678, 0x9abcdef0];
    fixture.scene.primary_uniform[176..192].copy_from_slice(
        &control
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    fixture.scene.width = piece_count;
    fixture.scene.height = 1;
    fixture.scene.record_count = piece_count;
    fixture.scene.control = control;
    fixture.scene.dispatch = [piece_count.div_ceil(32), 1, 1];
    fixture.scene.terminal_bytes = 16 + 48 * u64::from(piece_count);
    fixture.scene.output_bytes = 16 + 80 * u64::from(piece_count);
    fixture.scene.sample_pixels = (0..piece_count).map(|i| [i, 0]).collect();
    fixture.scene.sample_indices = (0..piece_count).collect();
    let scene = fixture.scene.cases["retained_curve_fixture"]["packet_scope"].clone();
    fixture.scene.cases = json!({
        "scope":"Manufactured local canonical geometric-curve contact tests, not ODE trials, camera rays or native performance. Actual source terrain/prism/sign scene is unchanged. Each piece is queried independently; all earlier pieces/uncertainties must be reduced before a whole-case first-contact claim. Explicit constant direction and affine path are synthetic reported state components, not asserted physical tangent/arclength.",
        "positive_packet_prefix":PREFIX,"positive_packet":metadata,"source_scene_context":scene,
        "binding6_layout":{"curve_records":count,"curve_record_vec4":16,"piece_base_vec4":count*16,"piece_record_vec4":20,"pieces":piece_count},
        "record_layout":["16-byte token","48-byte per-piece local terminal; status4 clear piece/1contact/2uncertain","32-byte details: eventkind,visits,case,piece; theta,lower,upper,tolerance"],
        "h0_coverage_before_width":true
    });
    fixture
}
const ENTRY: &str = r#"
fn normal_scene(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=COUNT{return;}if id.x==0u{terminals[0]=settings.control;}
    let piece_base=PIECE_BASE+20u*id.x;let identity=nc_curve_inputs[piece_base+19u];let base=identity.x*16u;
    nc_cartesian_piece=nc_cartesian_load(piece_base);
    let index=1u+3u*id.x;let detail_index=1u+3u*COUNT+2u*id.x;
    let q=bitcast<vec4<f32>>(nc_curve_inputs[base]);let upath=bitcast<vec4<f32>>(nc_curve_inputs[base+1u]);
    let start=NcState(q.xyz,upath.xyz);let field=bitcast<vec4<f32>>(nc_curve_inputs[base+2u]);nc_anchor=field.xy;nc_index=field.zw;
    let eye_residual=bitcast<vec4<f32>>(nc_curve_inputs[base+14u]);nc_eye_height=eye_residual.xy;nc_scene_anchor_residual=eye_residual.zw;
    nc_axis=bitcast<vec4<f32>>(nc_curve_inputs[base+13u]).xyz;nc_scene_visits=0u;nc_root_count=0u;
    nc_scene_normal=vec3(0.0);nc_scene_material=0u;nc_scene_owner=0u;
    let limits=nc_curve_inputs[base+12u];let error=bitcast<f32>(limits.x);let tolerance=bitcast<f32>(limits.y);
    let data=NcCurve(bitcast<vec4<f32>>(nc_curve_inputs[base+3u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+4u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+5u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+6u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+7u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+8u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+9u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+10u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+11u]),error,limits.w);
    let curve=NcSceneCurve(start,data,vec4(upath.w,0.0,0.0,0.0),nc_cartesian_piece.parameter.y,error,limits.w);
    var event=nc_scene_uncertain();if curve.valid==1u&&error<=tolerance{event=nc_scene_query_cached(curve,tolerance);}
    var theta=nc_cartesian_piece.parameter.x;var status=2u;if event.kind==0u{theta=curve.theta_end;status=4u;}if event.kind==1u{theta=event.theta;status=1u;}
    terminals[index]=bitcast<vec4<u32>>(vec4(nc_position(nc_scene_q(curve,theta)),nc_scene_path(curve,theta)));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(nc_scene_direction(curve,theta)),status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(event.normal),(event.material<<24u)|event.owner);
    terminals[detail_index]=vec4(event.kind,nc_scene_visits,identity.xy);
    terminals[detail_index+1u]=bitcast<vec4<u32>>(vec4(event.theta,event.lower,event.upper,tolerance));
}
"#;
