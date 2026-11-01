// Original MIT. Diagnostic-only experiments; production is unchanged.
// Historical reports are defined by their retained generated WGSL and source
// hashes. Retired DP/air labels do not recreate older mutable prototypes.
fn replace_once(source: &mut String, old: &str, new: &str) {
    assert_eq!(
        source.matches(old).count(),
        1,
        "diagnostic source changed: {old}"
    );
    *source = source.replacen(old, new, 1);
}
fn replace_traces(source: &mut String, old: &str, new: &str) {
    let count = source.matches(old).count();
    assert!(
        (1..=2).contains(&count),
        "diagnostic trace source changed: {old}"
    );
    *source = source.replace(old, new);
}
// Isolated diagnostic geometry hardening, applicable to a retained assembled
// shader without replacing its numerical integrator or camera ray formula.
pub(crate) fn strict_geometry_variant(mut source: String) -> String {
    source = source.replace("\r\n", "\n");
    let replacement = std::fs::read_to_string("scripts/optical_coverage_local.wgsl").unwrap();
    let start = source.find("fn terrain_coverage(").unwrap();
    let body = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    let mut end = body;
    for (i, b) in source.as_bytes()[body..].iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        }
        if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                end = body + i + 1;
                break;
            }
        }
    }
    source.replace_range(start..end, &replacement);
    replace_once(
        &mut source,
        "let p=frame_point(start.p,base)-origin;",
        "var p=start.p-origin;if base>0u{p=(optical_frames[base].xyz-origin)+frame_vector(start.p,base);}",
    );
    let begin = source
        .find("        // Finite transverse contact band.")
        .unwrap();
    let end = begin + source[begin..].find("    }\n    let hit=").unwrap();
    source.replace_range(begin..end,"        // A candidate band is not a confirmed plane crossing.\n        result.ambiguous=1u;return result;\n");
    replace_once(
        &mut source,
        "if abs(dot(n,hit))>band+tube || !triangle_band(a,b,c,hit,n,band+tube){result.ambiguous=1u;return result;}",
        "if abs(dot(n,hit))>band+tube || !(dot(cross(b-a,hit-a),n)>(band+tube)*length(b-a) && dot(cross(c-b,hit-b),n)>(band+tube)*length(c-b) && dot(cross(a-c,hit-c),n)>(band+tube)*length(a-c)){result.ambiguous=1u;return result;}",
    );
    source
}
pub(crate) fn diagnostic_variant(mut source: String, variant: &str) -> String {
    source = source.replace("\r\n", "\n");
    if variant.contains("corners") {
        let start = source
            .find("        let base=optical_frames[first].xyz;let right=")
            .unwrap();
        let end = start
            + source[start..]
                .find("        // One call site retains")
                .unwrap();
        source.replace_range(start..end, "");
        replace_once(
            &mut source,
            "corners[triangle.x],corners[triangle.y],corners[triangle.z]",
            "marker_corner(first,triangle.x),marker_corner(first,triangle.y),marker_corner(first,triangle.z)",
        );
        source.push_str(&std::fs::read_to_string("scripts/optical_marker_corner.wgsl").unwrap());
    }
    if variant.starts_with("dopri") {
        fn replace_function(source: &mut String, name: &str, replacement: &str) {
            let start = source.find(&format!("fn {name}(")).unwrap();
            let body = start + source[start..].find('{').unwrap();
            let mut depth = 0;
            let mut end = body;
            for (offset, byte) in source.as_bytes()[body..].iter().enumerate() {
                if *byte == b'{' {
                    depth += 1;
                }
                if *byte == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = body + offset + 1;
                        break;
                    }
                }
            }
            source.replace_range(start..end, replacement);
        }
        replace_function(
            &mut source,
            "trace",
            "fn trace(direction:vec3<f32>)->Trace{return dp_trace(direction,true);}",
        );
        if variant.ends_with("-air") {
            replace_function(
                &mut source,
                "scene_event",
                "fn scene_event(a:State,b:State,delta:vec3<f32>,step:f32,error:f32)->SceneEvent{return empty_event();}",
            );
            replace_function(
                &mut source,
                "terrain_coverage",
                "fn terrain_coverage(p:vec3<f32>,n:vec3<f32>)->u32{return 1u;}",
            );
        }
        if source.contains("fn trace_atmosphere(") {
            replace_function(
                &mut source,
                "trace_atmosphere",
                "fn trace_atmosphere(direction:vec3<f32>)->Trace{return dp_trace(direction,false);}",
            );
        }
        replace_function(
            &mut source,
            "advance_delta",
            "fn advance_delta(s:State,step:f32)->State{return dp_advance_delta(s,step);}",
        );
        source = source.replace("settings.scene.x*step", "(2.0*settings.scene.x)*step");
        if (variant.ends_with("newton")
            || (variant.contains("hermite")
                || variant.contains("carried")
                || variant.contains("height")))
            && source.contains("fn face_event(")
        {
            let begin = source
                .find("        var low=0.0;var high=step;\n        for(var i=0;i<24;i+=1){")
                .unwrap();
            let end = begin
                + source[begin..]
                    .find("    }else{\n        // Finite transverse contact band.")
                    .unwrap();
            source.replace_range(
                begin..end,
                &std::fs::read_to_string(
                    if variant.contains("hermite")
                        || variant.contains("carried")
                        || variant.contains("height")
                    {
                        "scripts/optical_face_dense.wgsl"
                    } else {
                        "scripts/optical_face_newton.wgsl"
                    },
                )
                .unwrap(),
            );
            source = source
                .replace(
                    "    var distance=0.0;",
                    "    var distance=0.0;var root_delta=vec3(0.0);",
                )
                .replace(
                    "distance=clamp(estimate,0.0,step);",
                    "distance=clamp(estimate,0.0,step);let prefix=dopri_delta(start,distance,dp_branch(),geometry(start.p));if !(prefix.error<=error){result.ambiguous=1u;return result;}root_delta=prefix.delta.p;",
                )
                .replace(
                    "let hit=p+frame_vector(advance_delta(start,distance).p,base);",
                    "let hit=p+frame_vector(root_delta,base);",
                );
            if variant.contains("hermite")
                || variant.contains("carried")
                || variant.contains("height")
            {
                source=source.replace("let prefix=dopri_delta(start,distance,dp_branch(),geometry(start.p));if !(prefix.error<=error){result.ambiguous=1u;return result;}root_delta=prefix.delta.p;", "root_delta=advance_delta(start,distance).p;");
                replace_once(
                    &mut source,
                    "let velocity=dot(n,frame_vector(start.u,base));",
                    "let velocity=dot(n,frame_vector(dp_curve_derivative(0.0),base));",
                );
                replace_once(
                    &mut source,
                    "let transverse=abs(velocity)-(2.0*settings.scene.x)*step;",
                    "let transverse=abs(velocity)-dp_curve_acceleration()*step;",
                );
                if !variant.ends_with("-air") {
                    replace_once(
                        &mut source,
                        "let deviation=(2.0*settings.scene.x)*step*step*0.125;",
                        "let deviation=dp_curve_acceleration()*step*step*0.125;",
                    );
                }
            }
        }

        source.push_str(
            &std::fs::read_to_string(if variant.contains("height") {
                "src/desktop/shaders/refraction_dopri_height.wesl"
            } else if variant.contains("carried") {
                "src/desktop/shaders/refraction_dopri_carried.wesl"
            } else if variant.contains("hermite") {
                "src/desktop/shaders/refraction_dopri_hermite.wesl"
            } else if variant.contains("auxiliary") {
                "src/desktop/shaders/refraction_dopri_auxiliary.wesl"
            } else {
                "src/desktop/shaders/refraction_dopri.wesl"
            })
            .unwrap(),
        );
        if variant.ends_with("-strict") {
            source = strict_geometry_variant(source);
        }
        source.push_str(
            &std::fs::read_to_string(if variant.contains("height") {
                "src/desktop/shaders/refraction_dopri_height_trace.wesl"
            } else if variant.contains("carried") {
                "src/desktop/shaders/refraction_dopri_carried_trace.wesl"
            } else if variant.contains("hermite") {
                "src/desktop/shaders/refraction_dopri_hermite_trace.wesl"
            } else {
                "src/desktop/shaders/refraction_dopri_trace.wesl"
            })
            .unwrap(),
        );
        if source.contains("var<private> event_counters") {
            source=source.replace("fn dp_contact(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,tolerance:f32)->DpContact{","fn dp_contact(start:State,g:vec4<f32>,end:DpStep,step:f32,contact_height:f32,tolerance:f32)->DpContact{event_counters.x+=1u;")
                .replace("for(var j=0;j<32;j+=1){", "for(var j=0;j<32;j+=1){event_counters.z+=1u;")
                .replace("if candidate.error+allocation<=tolerance{return DpContact(guess", "if candidate.error+allocation<=tolerance{event_counters.y+=1u;return DpContact(guess")
                .replace("    return result;\n}\nfn dp_trace", "    event_counters.w+=1u;return result;\n}\nfn dp_trace")
                .replace("if next==trial || !(next>low && next<high){return result;}", "if next==trial || !(next>low && next<high){event_counters.w+=1u;return result;}");
            source = source.replace(
                "tolerance:f32,actual_height:bool)->DpContact{",
                "tolerance:f32,actual_height:bool)->DpContact{event_counters.x+=1u;",
            );
        }
        if source.contains("var<private> rejection_counts") {
            source=source.replace("if accepted.error>tolerance*0.875{", "if accepted.error>tolerance*0.875{rejection_counts.x+=1u;")
                .replace("if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe{", "if !(polynomial_bound>=0.0) || !lower_safe || !upper_safe{rejection_counts.y+=1u;")
                .replace("if accepted.error+allocation+consistency>tolerance{", "if accepted.error+allocation+consistency>tolerance{rejection_counts.z+=1u;rejection_values=vec4(consistency,accepted.error,allocation,tolerance);rejection_start=vec4(dp_consistency(g,actual_g),g.w,actual_g.w,path);rejection_state=vec4(abs(accepted.geometry.w-end_actual.w),1.41421356237*length(cross(accepted.geometry.xyz,end_actual.xyz))/(length(accepted.geometry.xyz)*length(end_actual.xyz))*(6399594.0+abs(accepted.geometry.w)),step,f32(dp_layer));")
                .replace("if step>0.000002{proposed=step*0.5;continue;}\n            return trace_result(state.p,state.u,path,2u);\n        }\n        var reference_distance", "rejection_counts.w+=1u;if step>0.000002{proposed=step*0.5;continue;}\n            return trace_result(state.p,state.u,path,2u);\n        }\n        var reference_distance");
        }
        if source.contains("var<private> trace_counters") {
            source=source.replace("for(var i=0;i<4096;i+=1){","for(var i=0;i<4096;i+=1){trace_counters.x+=1u;")
                .replace("fn dopri_delta(s:State,step:f32,branch:vec3<f32>,g0:vec4<f32>)->DpStep{","fn dopri_delta(s:State,step:f32,branch:vec3<f32>,g0:vec4<f32>)->DpStep{trace_counters.y+=1u;");
        }
    } else if variant == "air" {
        let scene = source.find("// Canonical triangle/OBB visibility").unwrap();
        source.truncate(scene);
        source.push_str("struct SceneEvent {distance:f32,normal:vec3<f32>,kind:u32,marker:u32,ambiguous:u32,lower:f32,upper:f32}; fn scene_event(a:State,b:State,delta:vec3<f32>,step:f32,error:f32)->SceneEvent{return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);} fn terrain_coverage(p:vec3<f32>,n:vec3<f32>)->u32{return 1u;}");
    } else if variant == "cached" || variant == "cached-layer" {
        let counter = if source
            .contains("fn midpoint_delta(s:State,step:f32)->State{ trace_counters.y+=1u;")
        {
            " trace_counters.y+=1u;"
        } else {
            ""
        };
        replace_once(
            &mut source,
            &format!(
                "fn midpoint_delta(s:State,step:f32)->State{{{counter}\n    let g=geometry(s.p);let ng=index_gradient(g.w);"
            ),
            &format!(
                "fn midpoint_delta(s:State,step:f32)->State{{let g=geometry(s.p);return midpoint_field(s,step,g,index_gradient(g.w));}}\nfn midpoint_field(s:State,step:f32,g:vec4<f32>,ng:vec2<f32>)->State{{{counter}"
            ),
        );
        source.push_str("\nfn advance_field(s:State,step:f32,g:vec4<f32>,ng:vec2<f32>)->State{let first=midpoint_field(s,step*0.5,g,ng);let second=midpoint_delta(State(s.p+first.p,first.u),step*0.5);return State(first.p+second.p,second.u);}\n");
        replace_traces(
            &mut source,
            "let entry=path;let curvature=settings.budget.z;",
            "let entry=path;let curvature=settings.budget.z;var cached_geometry=geometry(state.p);",
        );
        replace_traces(
            &mut source,
            "let g=geometry(state.p);let vertical=dot(state.u,g.xyz);",
            "let g=cached_geometry;let vertical=dot(state.u,g.xyz);",
        );
        replace_traces(
            &mut source,
            "let full=midpoint_delta(state,step);let half_delta=advance_delta(state,step);",
            "let ng=index_gradient(g.w);let full=midpoint_field(state,step,g,ng);let half_delta=advance_field(state,step,g,ng);",
        );
        replace_traces(
            &mut source,
            "let next_height=geometry(half.p).w;",
            "let next_geometry=geometry(half.p);let next_height=next_geometry.w;",
        );
        replace_traces(
            &mut source,
            "state=half;path+=step;proposed=",
            "state=half;cached_geometry=next_geometry;path+=step;proposed=",
        );
        if variant == "cached-layer" {
            replace_once(
                &mut source,
                "fn index_gradient(h:f32)->vec2<f32>{",
                "var<private> cached_layer:u32;\nfn index_gradient(h:f32)->vec2<f32>{",
            );
            replace_once(
                &mut source,
                "if h<settings.scene.y {return vec2(settings.layers[0].y,0.0);}",
                "if h<settings.scene.y {return vec2(settings.layers[0].y,0.0);}\n    if cached_layer<count-1u {let knot=settings.layers[cached_layer];if h>=knot.x && h<settings.layers[cached_layer+1u].x{return vec2(knot.y+knot.z*(h-knot.x),knot.z);}}",
            );
            replace_once(
                &mut source,
                "let knot=settings.layers[lo];return vec2",
                "cached_layer=lo;let knot=settings.layers[lo];return vec2",
            );
        }
    }
    source
}
