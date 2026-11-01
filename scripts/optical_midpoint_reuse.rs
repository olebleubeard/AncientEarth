//! Original MIT. Isolated exact-expression reuse of the current midpoint
//! solver's first field evaluation. No production source is changed.

fn function_span(source: &str, name: &str) -> std::ops::Range<usize> {
    let signature = format!("fn {name}(");
    assert_eq!(
        source.matches(&signature).count(),
        1,
        "Function source drift"
    );
    let start = source.find(&signature).unwrap();
    let opening = start + source[start..].find('{').unwrap();
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[opening..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start..opening + offset + 1;
                }
            }
            _ => {}
        }
    }
    panic!("Unterminated {name}")
}

fn replace_count(source: &mut String, old: &str, new: &str, expected: usize) {
    assert_eq!(
        source.matches(old).count(),
        expected,
        "Reuse source drift: {old}"
    );
    *source = source.replace(old, new);
}

pub fn apply(source: &str) -> String {
    // Normalize source line endings only; mathematical expression text and
    // order are retained below and checked by the source-contract test.
    let mut result = source.replace("\r\n", "\n");
    let midpoint_span = function_span(&result, "midpoint_delta");
    let original = result[midpoint_span.clone()].to_owned();
    let mut cached = original.clone();
    replace_count(
        &mut cached,
        "fn midpoint_delta(s:State,step:f32)->State{\n    let g=geometry(s.p);let ng=index_gradient(g.w);",
        "fn midpoint_delta_with_field(s:State,step:f32,g:vec4<f32>,ng:vec2<f32>)->State{",
        1,
    );
    // Preserve the original wrapper for calls outside the primary trace and
    // for the second half step, whose start differs from the cached state.
    let wrapper = "fn midpoint_delta(s:State,step:f32)->State{\n    let g=geometry(s.p);let ng=index_gradient(g.w);\n    return midpoint_delta_with_field(s,step,g,ng);\n}";
    result.replace_range(midpoint_span, &format!("{cached}\n{wrapper}"));

    let advance_span = function_span(&result, "advance_delta");
    let mut cached_advance = result[advance_span].to_owned();
    replace_count(
        &mut cached_advance,
        "fn advance_delta(s:State,step:f32)->State{",
        "fn advance_delta_with_field(s:State,step:f32,g:vec4<f32>,ng:vec2<f32>)->State{",
        1,
    );
    replace_count(
        &mut cached_advance,
        "let first=midpoint_delta(s,step*0.5);",
        "let first=midpoint_delta_with_field(s,step*0.5,g,ng);",
        1,
    );
    // The unchanged second half still calls midpoint_delta and evaluates
    // geometry/index_gradient at State(s.p+first.p, first.u).
    let cached_position = "fn advance_with_field(s:State,step:f32,g:vec4<f32>,ng:vec2<f32>)->State{let delta=advance_delta_with_field(s,step,g,ng);return State(s.p+delta.p,delta.u);}";
    let trace_span = function_span(&result, "trace");
    let mut trace = result[trace_span.clone()].to_owned();
    replace_count(
        &mut trace,
        "let full=midpoint_delta(state,step);let half_delta=advance_delta(state,step);",
        "let ng=index_gradient(g.w);\n        let full=midpoint_delta_with_field(state,step,g,ng);let half_delta=advance_delta_with_field(state,step,g,ng);",
        1,
    );
    replace_count(
        &mut trace,
        "let event_full=midpoint_delta(state,event_step);let event_half=advance_delta(state,event_step);",
        "let event_full=midpoint_delta_with_field(state,event_step,g,ng);let event_half=advance_delta_with_field(state,event_step,g,ng);",
        1,
    );
    for (argument, count) in [
        ("mid", 2),
        ("delta", 2),
        ("event.distance", 1),
        ("reference_distance", 1),
    ] {
        replace_count(
            &mut trace,
            &format!("advance(state,{argument})"),
            &format!("advance_with_field(state,{argument},g,ng)"),
            count,
        );
    }
    result.replace_range(
        trace_span,
        &format!("{cached_advance}\n{cached_position}\n{trace}"),
    );
    result
}

/// Separate diagnostic variant: retain the exact endpoint geometry already
/// evaluated by the primary loop, and retain its start geometry on rejection.
pub fn apply_carry(source: &str) -> String {
    let mut result = apply(source);
    let span = function_span(&result, "trace");
    let mut trace = result[span.clone()].to_owned();
    replace_count(
        &mut trace,
        "let entry=path;let curvature=settings.budget.z;",
        "let entry=path;let curvature=settings.budget.z;\n    var start_geometry=geometry(state.p);",
        1,
    );
    replace_count(
        &mut trace,
        "let g=geometry(state.p);let vertical=dot(state.u,g.xyz);",
        "let g=start_geometry;let vertical=dot(state.u,g.xyz);",
        1,
    );
    replace_count(
        &mut trace,
        "let next_height=geometry(half.p).w;",
        "let next_geometry=geometry(half.p);let next_height=next_geometry.w;",
        1,
    );
    replace_count(
        &mut trace,
        "state=half;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);",
        "state=half;start_geometry=next_geometry;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);",
        1,
    );
    result.replace_range(span, &trace);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_first_stages_and_immutable_primary_state_are_reused() {
        let source = std::fs::read_to_string("src/desktop/shaders/refraction.wesl")
            .unwrap()
            .replace("\r\n", "\n");
        let result = apply(&source);
        let old_midpoint = &source[function_span(&source, "midpoint_delta")];
        let new_midpoint = &result[function_span(&result, "midpoint_delta_with_field")];
        let tail = old_midpoint
            .split_once("let ng=index_gradient(g.w);")
            .unwrap()
            .1;
        assert_eq!(new_midpoint.split_once("->State{").unwrap().1, tail);
        let old_advance = &source[function_span(&source, "advance_delta")];
        assert_eq!(
            &result[function_span(&result, "advance_delta")],
            old_advance
        );
        let cached = &result[function_span(&result, "advance_delta_with_field")];
        assert!(cached.contains("let second=midpoint_delta(State(s.p+first.p,first.u),step*0.5);"));
        assert_eq!(
            &result[function_span(&result, "trace_atmosphere")],
            &source[function_span(&source, "trace_atmosphere")]
        );
    }

    #[test]
    fn endpoint_geometry_is_committed_only_with_the_accepted_state() {
        let source = std::fs::read_to_string("src/desktop/shaders/refraction.wesl").unwrap();
        let result = apply_carry(&source);
        let trace = &result[function_span(&result, "trace")];
        assert_eq!(trace.matches("start_geometry=").count(), 2);
        assert_eq!(trace.matches("geometry(state.p)").count(), 1);
        assert!(
            trace.contains("let next_geometry=geometry(half.p);let next_height=next_geometry.w;")
        );
        assert!(trace.contains("state=half;start_geometry=next_geometry;path+=step;"));
        let current = apply(&source);
        for helper in [
            "geometry",
            "index_gradient",
            "midpoint_delta_with_field",
            "advance_delta_with_field",
            "trace_atmosphere",
        ] {
            assert_eq!(
                &result[function_span(&result, helper)],
                &current[function_span(&current, helper)]
            );
        }
    }
}
