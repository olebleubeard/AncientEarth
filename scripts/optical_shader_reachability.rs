//! Mechanical diagnostic packaging: Naga determines function reachability;
//! retained WGSL bodies are copied byte-for-byte, never reserialized.
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn parsed(source: &str) -> wgpu::naga::Module {
    let module = wgpu::naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    module
}

pub fn prune(source: &str) -> (String, Value) {
    let original = parsed(source);
    // A second parse avoids depending on Naga's optional IR Clone feature.
    let mut compacted = parsed(source);
    wgpu::naga::compact::compact(&mut compacted, wgpu::naga::compact::KeepUnused::No);
    let kept: BTreeSet<_> = compacted
        .functions
        .iter()
        .map(|(_, function)| function.name.clone().expect("named WGSL function"))
        .collect();
    let mut removed = Vec::new();
    let mut kept_bodies = Vec::new();
    for (handle, function) in original.functions.iter() {
        let name = function.name.as_ref().expect("named WGSL function");
        let range = original
            .functions
            .get_span(handle)
            .to_range()
            .expect("source span");
        let body = &source[range.clone()];
        assert!(
            body.contains(&format!("fn {name}(")) || body.contains(&format!("fn {name} (")),
            "unexpected function span"
        );
        if kept.contains(name) {
            kept_bodies.push(body);
        } else {
            removed.push((range, name.clone()));
        }
    }
    removed.sort_by_key(|(range, _)| range.start);
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (range, _) in &removed {
        assert!(range.start >= cursor && range.end <= source.len());
        output.push_str(&source[cursor..range.start]);
        output.push('\n');
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);
    for body in &kept_bodies {
        assert!(output.contains(body), "a retained function body changed");
    }
    let after = parsed(&output);
    let actual: BTreeSet<_> = after
        .functions
        .iter()
        .map(|(_, function)| function.name.clone().unwrap())
        .collect();
    assert_eq!(actual, kept);
    assert_eq!(after.entry_points.len(), original.entry_points.len());
    for (a, b) in after.entry_points.iter().zip(&original.entry_points) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.stage, b.stage);
        assert_eq!(a.workgroup_size, b.workgroup_size);
    }
    let metadata = json!({
        "scope":"Only source spans of functions absent from Naga's entry-point reachability graph are removed. Retained function bodies and all other source bytes are preserved. This is not a numerical or performance acceptance result.",
        "original_bytes":source.len(),"pruned_bytes":output.len(),
        "original_functions":original.functions.len(),"retained_functions":kept,
        "removed_functions":removed.iter().map(|(range,name)|json!({"name":name,"source_byte_range":[range.start,range.end]})).collect::<Vec<_>>(),
        "retained_bodies_unchanged":true,"naga_valid_before_and_after":true,
    });
    (output, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_transitive_calls_and_all_entries_without_following_comments() {
        let source = r#"
@group(0) @binding(0) var<storage, read_write> output:array<u32>;
fn leaf(x:u32)->u32{return x;}
fn unused(x:u32)->u32{return x;}
fn unused_caller(x:u32)->u32{return unused(x);}
fn live(x:u32)->u32{ /* unused_caller(x) { } */ return leaf(x); }
@compute @workgroup_size(1) fn first(){output[0]=live(7u);}
@compute @workgroup_size(1) fn second(){output[1]=leaf(8u);}
"#;
        let (result, report) = prune(source);
        assert!(
            result.contains("fn live(x:u32)->u32{ /* unused_caller(x) { } */ return leaf(x); }")
        );
        assert!(!result.contains("fn unused("));
        assert!(!result.contains("fn unused_caller("));
        assert_eq!(report["removed_functions"].as_array().unwrap().len(), 2);
        assert_eq!(prune(&result).0, result);
    }
}
