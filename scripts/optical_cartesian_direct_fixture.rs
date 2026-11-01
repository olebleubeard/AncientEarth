//! Original MIT. Direct-output Cartesian trial workload, separately observed.
//! Every timed evaluation stores the actual returned and captured values.
//! This is not a retiming of the rejected checksum-only experiment.
use super::fixture::Fixture;
use serde_json::json;

const MAGIC: u32 = 0x43544431;
const SCALARS: u32 = 188;
const RECORD_WORDS: u32 = 8 + 2 * SCALARS;

fn function(source: &str, name: &str) -> String {
    let signature = format!("fn {name}(");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, c) in source.as_bytes()[open..].iter().enumerate() {
        if *c == b'{' {
            depth += 1;
        }
        if *c == b'}' {
            depth -= 1;
        }
        if depth == 0 {
            return source[start..=open + offset].to_owned();
        }
    }
    panic!("Unclosed function {name}")
}

fn once(source: &mut String, old: &str, new: &str) {
    assert_eq!(source.matches(old).count(), 1, "Source drift: {old}");
    *source = source.replacen(old, new, 1);
}

pub fn build(inner: u32, repetitions: u32) -> Fixture {
    assert_eq!((inner, repetitions), (8, 256));
    let mut fixture = super::cartesian_fixture::build(inner, repetitions);
    assert_eq!(fixture.count, 11);
    assert_eq!(fixture.cost_invocations, 2816);
    assert_eq!(fixture.detail_record_words, 756);
    assert_eq!(fixture.uniform.len(), 16);
    assert_eq!(fixture.inputs.len(), 11 * 36 * 4);
    assert_eq!(fixture.shader.matches("@compute").count(), 3);
    let first_entry = fixture.shader.find("@compute").unwrap();
    let mut shader = fixture.shader[..first_entry].to_owned();
    // Preserve exactly the reviewed numerical bodies and per-mode loaders.
    // Hash-only observation is deliberately absent from this workload.
    for name in [
        "ct_hash_core_32",
        "ct_hash_core_64",
        "ct_hash_value",
        "ct_hash",
    ] {
        let body = function(&shader, name);
        once(&mut shader, &body, "");
    }
    for bits in ["32", "64"] {
        let name = format!("ct_stage_{bits}");
        let original = function(&shader, &name);
        let end = original.rfind('}').unwrap();
        let replacement = format!(
            "{}    cd_stage_mask_{bits}=cd_stage_mask_{bits}|(1u<<(base/14u));\n}}",
            &original[..end]
        );
        once(&mut shader, &original, &replacement);
    }
    shader.push_str(ENTRIES);
    let records = fixture.cost_invocations.checked_mul(inner).unwrap();
    let bytes = u64::from(records)
        .checked_mul(u64::from(RECORD_WORDS))
        .unwrap()
        .checked_mul(4)
        .unwrap();
    assert_eq!((records, RECORD_WORDS, bytes), (22528, 384, 34603008));
    fixture.shader = shader;
    fixture.uniform[12..16].copy_from_slice(&MAGIC.to_le_bytes());
    fixture.detail_record_words = RECORD_WORDS;
    // The common host struct retains a detail dispatch field; this mode has
    // no detail entry and never dispatches it. Both entries use cost_dispatch.
    fixture.metadata["scope"] = json!(
        "Distinct direct-output trial workload. Actual 188 scalar values from every timed evaluation are retained and independently checked. Full-store cost only; no controller, complete ray, intrinsic FP64 or native FPS inference."
    );
    fixture.metadata["entries"] = json!(["direct_reference", "direct_promoted"]);
    fixture.metadata["direct_output_bytes"] = json!(bytes);
    fixture.metadata["direct_record_count"] = json!(records);
    fixture.metadata["direct_record_words"] = json!(RECORD_WORDS);
    fixture.metadata["direct_record_magic"] = json!(MAGIC);
    fixture.metadata["direct_scalar_count"] = json!(SCALARS);
    fixture.metadata["direct_header"] = json!([
        "magic",
        "invocation",
        "cycle",
        "input_index",
        "source_record",
        "owner_bits",
        "precision_bits",
        "stage_written_mask"
    ]);
    fixture.metadata["direct_record_mapping"] = json!(
        "record=8*invocation+cycle; input=(invocation+17*cycle)%11; invocation in 0..2816 and cycle in 0..8. Precision field is 32 or 64; stage mask must be exactly 127."
    );
    fixture.metadata["direct_scalar_layout"] = json!(
        "After 8 header words: 188 scalars, each little-endian low/high f64 words. Reference f32 scalars widen exactly. 0:32 returned DpStep;32:77 five theta P/U/dPds;77 step;78:80 zero;80:178 seven stage p/u/g/N/bend;178:188 ep/eu/en/m/interior/nr5."
    );
    fixture.metadata["direct_reset_contract"] = json!(
        "Host clears the entire output before EACH dispatch outside its timestamp interval. Same ordered submission then executes the timed entry and reads/saves every byte before the next clear. Kernel resets capture array and stage mask for EACH evaluation; writes every payload/header word, publishing nonzero magic last. Retain all 8 dispatches per precision, including warmups."
    );
    fixture.metadata["direct_observation_scope"] = json!(
        "The timed kernel calls the instrumented trial and stores that exact returned trial plus all captured stages and dense values. No separate detail-kernel equality inference, checksum gate, write-disabled retiming or unobserved kernel throughput claim. Common full-output traffic can compress the measured precision ratio."
    );
    fixture.metadata["policy"] = json!("docs/science/cartesian-direct-trial-policy.md");
    for key in ["cost_layout", "detail_layout", "detail_instrumentation"] {
        fixture.metadata.as_object_mut().unwrap().remove(key);
    }
    fixture
}

const ENTRIES: &str = r#"
var<private> cd_stage_mask_32:u32;
var<private> cd_stage_mask_64:u32;
fn cd_header(base:u32,invocation:u32,cycle:u32,index:u32,precision_bits:u32,mask:u32){
    let b=9u*index;
    ct_output[base+1u]=invocation;ct_output[base+2u]=cycle;ct_output[base+3u]=index;
    ct_output[base+4u]=ct_inputs[b+8u].y;ct_output[base+5u]=ct_inputs[b+6u].w;
    ct_output[base+6u]=precision_bits;ct_output[base+7u]=mask;
    ct_output[base]=ct_policy.w;
}
@compute @workgroup_size(32)
fn direct_reference(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=ct_policy.z{return;}
    for(var k=0u;k<ct_policy.y;k+=1u){
        let index=(id.x+17u*k)%ct_policy.x;ct_load_32(index);let b=9u*index;
        let s=State(bitcast<vec3<f32>>(ct_inputs[b+4u].xyz),bitcast<vec3<f32>>(ct_inputs[b+5u].xyz));
        let h=bitcast<f32>(ct_inputs[b+5u].w);let branch=bitcast<vec3<f32>>(ct_inputs[b+6u].xyz);
        cd_stage_mask_32=0u;for(var j=0u;j<108u;j+=1u){ct_capture_32[j]=0.0;}
        let c=dopri_delta_detail(s,h,branch,geometry(s.p));let values=ct_core_32(s,h,c);
        let output_base=384u*(ct_policy.y*id.x+k);
        for(var j=0u;j<80u;j+=1u){ct_write64(output_base+8u+2u*j,f64(values[j]));}
        for(var j=0u;j<108u;j+=1u){ct_write64(output_base+168u+2u*j,f64(ct_capture_32[j]));}
        cd_header(output_base,id.x,k,index,32u,cd_stage_mask_32);
    }
}
@compute @workgroup_size(32)
fn direct_promoted(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=ct_policy.z{return;}
    for(var k=0u;k<ct_policy.y;k+=1u){
        let index=(id.x+17u*k)%ct_policy.x;ct_load_64(index);let b=9u*index;
        let s=ct64_State(vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+4u].xyz)),vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+5u].xyz)));
        let h=f64(bitcast<f32>(ct_inputs[b+5u].w));let branch=vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+6u].xyz));
        cd_stage_mask_64=0u;for(var j=0u;j<108u;j+=1u){ct_capture_64[j]=f64(0.0);}
        let c=ct64_dopri_delta_detail(s,h,branch,ct64_geometry(s.p));let values=ct_core_64(s,h,c);
        let output_base=384u*(ct_policy.y*id.x+k);
        for(var j=0u;j<80u;j+=1u){ct_write64(output_base+8u+2u*j,values[j]);}
        for(var j=0u;j<108u;j+=1u){ct_write64(output_base+168u+2u*j,ct_capture_64[j]);}
        cd_header(output_base,id.x,k,index,64u,cd_stage_mask_64);
    }
}
"#;
