//! Original MIT. Narrow finite-termination admission for one retained diagnostic.
//!
//! This is NOT a general shader safety verifier or production authorization.
//! Exact source and uniform byte equality bind the argument to the reviewed
//! original full-frame primary fixture. The host must retain every checked()
//! runtime option except force_loop_bounding, and must still validate WGSL.
use serde_json::{Value, json};

pub const MANIFEST_PATH: &str = "scripts/optical_primary_termination.json";
pub const REFERENCE_SHADER_PATH: &str = "artifacts/optical-midpoint-reference-full.wgsl";
pub const REFERENCE_UNIFORM_PATH: &str = "artifacts/optical-midpoint-reference-full-primary.bin";

const REFERENCE_SHADER: &str = include_str!("../artifacts/optical-midpoint-reference-full.wgsl");
const REFERENCE_UNIFORM: &[u8] = include_bytes!("../artifacts/optical-midpoint-reference-full-primary.bin");
const MANIFEST: &str = include_str!("optical_primary_termination.json");

/// Panics before trusted shader creation on ANY source, packet or dispatch drift.
/// Byte equality is stricter than accepting a matching source digest. The
/// manifest's SHA256 values are independently checked by the source tests and
/// retained in the host report/provenance; no new hashing implementation is used.
pub fn audit(
    shader: &str,
    primary_uniform: &[u8],
    width: u32,
    height: u32,
    record_count: u32,
    dispatch: [u32; 3],
) -> Value {
    assert_eq!(shader.as_bytes(), REFERENCE_SHADER.as_bytes(), "finite-loop proof: unreviewed shader bytes");
    assert_eq!(primary_uniform, REFERENCE_UNIFORM, "finite-loop proof: unreviewed uniform bytes");
    assert_eq!((width, height, record_count), (1920, 1080, 2_073_600), "finite-loop proof: unreviewed dimensions");
    assert_eq!(dispatch, [64_800, 1, 1], "finite-loop proof: unreviewed dispatch");
    assert_eq!(primary_uniform.len(), 140 * 16);
    let word = |at: usize| u32::from_le_bytes(primary_uniform[at..at + 4].try_into().unwrap());
    let count = f32::from_bits(word(9 * 16));
    assert!(count.is_finite() && (2.0..=128.0).contains(&count) && count.fract() == 0.0);
    assert_eq!(count, 45.0);
    assert_eq!([word(11 * 16), word(11 * 16 + 4)], [width, height]);
    // These are explicit supplementary tripwires, not a text-based proof of
    // arbitrary shader programs. Exact reviewed bytes remain the primary gate.
    for (text, expected) in [
        ("for(var i=0;i<4096;i+=1)", 1),
        ("visited+=1u;if visited>8192u", 4),
        ("visited+=1u;if visited>2048u", 1),
        ("let count=((leaf>>24u)&7u)+1u;", 2),
        ("let last=min(lo+1u,count-2u);", 1),
        ("for(var i=first;i<=last;i+=1u)", 1),
        ("@compute @workgroup_size(32)", 1),
    ] {
        assert_eq!(shader.matches(text).count(), expected, "finite-loop structural prerequisite: {text}");
    }
    let manifest: Value = serde_json::from_str(MANIFEST).expect("reviewed termination manifest");
    assert_eq!(manifest["scope"], "original-primary-fixed-full-frame-diagnostic-only");
    json!({
        "admitted":true,
        "scope":"One exact original-primary full-frame fixture only; no numerical, physical accuracy, native FPS or general production acceptance.",
        "source_exact_bytes":true,"uniform_exact_bytes":true,
        "width":width,"height":height,"record_count":record_count,"dispatch":dispatch,
        "source_layer_count":count as u32,
        "required_runtime_policy":"ShaderRuntimeChecks::checked() with ONLY force_loop_bounding=false; all remaining fields unchanged. Naga validation remains required.",
        "manifest_path":MANIFEST_PATH,"reference_shader_path":REFERENCE_SHADER_PATH,
        "reference_uniform_path":REFERENCE_UNIFORM_PATH,"proof":manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn admitted(shader: &str, uniform: &[u8], dimensions: (u32,u32,u32), dispatch: [u32;3]) -> bool {
        std::panic::catch_unwind(|| audit(shader, uniform, dimensions.0, dimensions.1, dimensions.2, dispatch)).is_ok()
    }
    const D: (u32,u32,u32) = (1920,1080,2_073_600);
    const G: [u32;3] = [64_800,1,1];
    #[test]
    fn exact_reviewed_fixture_is_admitted() {
        assert!(admitted(REFERENCE_SHADER, REFERENCE_UNIFORM, D, G));
    }
    #[test]
    fn source_mutations_fail_closed() {
        for (from,to) in [("i<4096","i<=4096"),("visited>8192u","visited>8193u"),("count-2u","count-1u"),("fn trace(","fn other(")] {
            let changed=REFERENCE_SHADER.replacen(from,to,1);
            assert_ne!(changed,REFERENCE_SHADER);
            assert!(!admitted(&changed,REFERENCE_UNIFORM,D,G));
        }
        assert!(!admitted(&(REFERENCE_SHADER.to_owned()+"\n"),REFERENCE_UNIFORM,D,G));
    }
    #[test]
    fn uniform_drift_is_not_a_new_admission() {
        for value in [0.0_f32,1.0,2.0,44.0,128.0,129.0,f32::INFINITY,f32::NAN] {
            let mut changed=REFERENCE_UNIFORM.to_vec();
            changed[9*16..9*16+4].copy_from_slice(&value.to_le_bytes());
            assert!(!admitted(REFERENCE_SHADER,&changed,D,G));
        }
        let mut changed=REFERENCE_UNIFORM.to_vec();changed[0]^=1;
        assert!(!admitted(REFERENCE_SHADER,&changed,D,G));
    }
    #[test]
    fn dispatch_and_extent_drift_fail_closed() {
        for d in [(1920,1080,23),(1920,1079,2_073_600),(1,1,1),(u32::MAX,u32::MAX,u32::MAX)] {
            assert!(!admitted(REFERENCE_SHADER,REFERENCE_UNIFORM,d,G));
        }
        for g in [[64_799,1,1],[64_800,2,1],[64_800,1,2]] {
            assert!(!admitted(REFERENCE_SHADER,REFERENCE_UNIFORM,D,g));
        }
    }
}
