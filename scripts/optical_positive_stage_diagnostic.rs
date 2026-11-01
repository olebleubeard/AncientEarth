//! Original MIT. Unregistered return-site telemetry for the six manufactured Cartesian
//! consumer. It changes no predicate, contact equation, budget or ordering.
//! The original terminal words and first eight detail words remain intact.
use super::CurvesFixture;
use serde_json::{Value, json};

fn span(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0u32;
    for (i, b) in source.as_bytes()[open..].iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        }
        if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                return start..open + i + 1;
            }
        }
    }
    panic!("Missing function close: {name}")
}
fn replace(source: &mut String, old: &str, new: &str, count: usize) {
    assert_eq!(
        source.matches(old).count(),
        count,
        "Telemetry source drift: {old}"
    );
    *source = source.replace(old, new);
}
fn tag(
    source: &mut String,
    name: &str,
    statement: &str,
    first: u32,
    expected: usize,
) -> Vec<Value> {
    let range = span(source, name);
    let body = source[range.clone()].to_owned();
    assert_eq!(
        body.matches(statement).count(),
        expected,
        "Telemetry return-count drift: {name}"
    );
    let mut output = String::new();
    let mut cursor = 0;
    let mut rows = Vec::new();
    for (index, (at, _)) in body.match_indices(statement).enumerate() {
        output.push_str(&body[cursor..at]);
        let code = first + u32::try_from(index).unwrap();
        output.push_str(&format!("nc_stage_code={code}u;{statement}"));
        cursor = at + statement.len();
        let line_start = body[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = body[at..].find('\n').map_or(body.len(), |i| at + i);
        rows.push(json!({"code":code,"function":name,"return_index":index,"original_source_line":&body[line_start..line_end]}));
    }
    output.push_str(&body[cursor..]);
    source.replace_range(range, &output);
    rows
}

pub fn build() -> CurvesFixture {
    let mut fixture = super::build_positive_contacts();
    apply(&mut fixture);
    fixture
}
pub fn apply(fixture: &mut CurvesFixture) {
    assert_eq!(
        fixture.scene.record_count, 6,
        "This telemetry adapter is pinned to the six manufactured contact pieces"
    );
    let shader = &mut fixture.scene.shader;
    assert!(
        fixture
            .scene
            .cases
            .get("h0_coverage_before_width")
            .is_some()
    );
    assert!(fixture.scene.cases.get("stage_diagnostics").is_none());
    let mut stages = Vec::new();
    // Inner face failures retain their exact return site through wrappers.
    stages.extend(tag(
        shader,
        "nc_face_contact_cached",
        "return nc_face_uncertain();",
        200,
        15,
    ));
    stages.extend(tag(
        shader,
        "nc_scene_h0",
        "return nc_scene_uncertain();",
        100,
        8,
    ));
    let range = span(shader, "nc_scene_h0");
    let mut body = shader[range.clone()].to_owned();
    replace(
        &mut body,
        "let coverage=nc_scene_coverage(curve,parameters,enclosure);",
        "let coverage=nc_scene_coverage(curve,parameters,enclosure);nc_stage_coverage=coverage;",
        1,
    );
    shader.replace_range(range, &body);
    let range = span(shader, "nc_scene_face_event_cached");
    let mut body = shader[range.clone()].to_owned();
    replace(
        &mut body,
        "let face=nc_scene_face(address);",
        "nc_stage_face=address;let face=nc_scene_face(address);",
        1,
    );
    replace(
        &mut body,
        "if contact.kind==2u{return nc_scene_uncertain();}",
        "if contact.kind==2u{return nc_scene_uncertain();}",
        1,
    );
    replace(
        &mut body,
        "if !nc_interval_valid(charge)||charge.y>tolerance{return nc_scene_uncertain();}",
        "if !nc_interval_valid(charge)||charge.y>tolerance{nc_stage_code=300u;return nc_scene_uncertain();}",
        1,
    );
    replace(
        &mut body,
        "if !nc_finite3(n)||!nc_finite(norm2)||!(norm2>1.17549435e-38){return nc_scene_uncertain();}",
        "if !nc_finite3(n)||!nc_finite(norm2)||!(norm2>1.17549435e-38){nc_stage_code=301u;return nc_scene_uncertain();}",
        1,
    );
    shader.replace_range(range, &body);
    stages.push(json!({"code":300,"function":"nc_scene_face_event_cached","reason":"complete contact charge invalid or beyond unchanged tolerance"}));
    stages.push(json!({"code":301,"function":"nc_scene_face_event_cached","reason":"returned face normal evaluation invalid"}));
    let range = span(shader, "nc_scene_combine");
    let mut body = shader[range.clone()].to_owned();
    let final_return = "    return nc_scene_uncertain();";
    replace(
        &mut body,
        final_return,
        "    nc_stage_code=400u;return nc_scene_uncertain();",
        1,
    );
    shader.replace_range(range, &body);
    stages.push(json!({"code":400,"function":"nc_scene_combine","reason":"two certified event enclosures overlap"}));
    // Query's direct failure returns are distinct from propagated h0/face
    // uncertainty; tagging them cannot overwrite the inner return-site code.
    stages.extend(tag(
        shader,
        "nc_scene_query_cached",
        "return nc_scene_uncertain();",
        500,
        4,
    ));
    let range = span(shader, "normal_scene");
    let mut body = shader[range.clone()].to_owned();
    replace(
        &mut body,
        "let detail_index=1u+3u*6u+2u*id.x;",
        "let detail_index=1u+3u*6u+3u*id.x;",
        1,
    );
    replace(
        &mut body,
        "nc_scene_visits=0u;nc_root_count=0u;",
        "nc_scene_visits=0u;nc_root_count=0u;nc_stage_code=0u;nc_stage_face=0xffffffffu;nc_stage_coverage=0xffffffffu;",
        1,
    );
    let last = "terminals[detail_index+1u]=bitcast<vec4<u32>>(vec4(event.theta,event.lower,event.upper,tolerance));";
    replace(
        &mut body,
        last,
        &format!(
            "{last}\n    terminals[detail_index+2u]=vec4(nc_stage_code,nc_stage_face,nc_stage_coverage,0u);"
        ),
        1,
    );
    shader.replace_range(range, &body);
    shader.push_str("\nvar<private> nc_stage_code:u32;\nvar<private> nc_stage_face:u32;\nvar<private> nc_stage_coverage:u32;\n");
    fixture.scene.output_bytes =
        fixture.scene.terminal_bytes + 48 * u64::from(fixture.scene.record_count);
    fixture.scene.cases["stage_diagnostics"] = json!({
        "scope":"Return-site telemetry only. Original terminal words and eight original detail words retain semantics; append one uvec4 per record. No predicate, finite operation, contact allocation or ordering intentionally changes. Compiler-context parity must still be checked.",
        "source":"scripts/optical_positive_stage_diagnostic.rs","diagnostic_bytes_per_record":48,
        "appended_words":["uncertainty return-site code,0 if no tagged failure","most recently tested canonical face certificate address or0xffffffff","h0 coverage result0/1/2 or0xffffffff if never reached","reserved0"],
        "stages":stages
    });
}
