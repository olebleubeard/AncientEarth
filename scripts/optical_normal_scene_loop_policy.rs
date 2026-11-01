//! Original MIT. Unused exact-value compiler diagnostic for the split consumer.
//! The isolated consumer does not use PrimarySettings.budget for physics.
//! Its otherwise unused budget.w stores the original literal iteration count.
//! This must never be applied to the production/full-ray shader.
use serde_json::{Value,json};
pub fn apply(shader:&mut String,primary:&mut [u8])->Value{
    const ORIGINAL:&str="for(var iteration=0u;iteration<32u;iteration+=1u)";
    const REPLACEMENT:&str="for(var iteration=0u;iteration<u32(settings.budget.w);iteration+=1u)";
    assert!(shader.contains("@group(0) @binding(6) var<storage,read> nc_curve_inputs:"));
    assert_eq!(shader.matches(ORIGINAL).count(),2,"Only face and h0 root loops are expected");
    assert_eq!(primary.len(),2240);
    let previous:u32=u32::from_le_bytes(primary[156..160].try_into().unwrap());
    primary[156..160].copy_from_slice(&32.0f32.to_le_bytes());
    *shader=shader.replace(ORIGINAL,REPLACEMENT);
    assert_eq!(f32::from_le_bytes(primary[156..160].try_into().unwrap()),32.0);
    json!({"scope":"Isolated split-scene compiler diagnostic only: both original32-iteration root loops read exact32 from immutable uniform storage. No root tolerance, predicate, ordering or maximum count changes. No inference about unrolling until measured.",
        "uniform_byte_offset":156,"previous_word":previous,"current_word":32.0f32.to_bits(),"loop_count":2,"iterations":32})
}
