//! Original MIT. Cost isolation over retained actual trials; no ray feedback.
use serde_json::{json, Value};
#[path = "optical_normal_trace_fixture.rs"]
mod trace_fixture;
#[path = "optical_height_trial_split.rs"]
mod trial_split;

pub struct Fixture {
    pub shader: String,
    pub uniform: Vec<u8>,
    pub inputs: Vec<u8>,
    pub count: u32,
    pub detail_record_words: u32,
    pub cost_invocations: u32,
    pub detail_dispatch: [u32; 3],
    pub cost_dispatch: [u32; 3],
    pub metadata: Value,
}

pub fn build(capture_prefix: &str, inner: u32, repetitions: u32) -> Fixture {
    build_mode(capture_prefix, inner, repetitions, false)
}

pub fn build_fast(capture_prefix: &str, inner: u32, repetitions: u32) -> Fixture {
    build_mode(capture_prefix, inner, repetitions, true)
}

fn build_mode(capture_prefix: &str, inner: u32, repetitions: u32, fast: bool) -> Fixture {
    assert!((1..=64).contains(&inner) && (1..=1024).contains(&repetitions));
    let capture: Value =
        serde_json::from_slice(&std::fs::read(format!("{capture_prefix}.json")).unwrap()).unwrap();
    let record_words = capture["record_words"].as_u64().unwrap() as usize;
    let raw = std::fs::read(format!("{capture_prefix}-readback.bin")).unwrap();
    let uniform = std::fs::read(format!("{capture_prefix}-uniform.bin")).unwrap();
    const TRIAL_VECTORS: usize = 27;
    const CAPACITY: usize = 128;
    assert_eq!(record_words, 4 * (10 + CAPACITY * TRIAL_VECTORS));
    assert_eq!(raw.len(), 7 * record_words * 4);
    let words: Vec<u32> = raw
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let mut inputs = Vec::new();
    let mut mapping = Vec::new();
    let mut stage_counts = [0u32; 9];
    let mut certificate_counts = [0u32; 2];
    let mut controller_counts = [0u32; 3];
    for (ray, record) in words.chunks_exact(record_words).enumerate() {
        let count = record[36] as usize;
        assert_eq!(record[37], CAPACITY as u32);
        assert_eq!(
            record[38], 0,
            "Do not silently truncate actual trial workload"
        );
        assert!(count <= CAPACITY);
        for index in 0..count {
            let trial =
                &record[40 + index * TRIAL_VECTORS * 4..40 + (index + 1) * TRIAL_VECTORS * 4];
            let stage = trial[23 * 4 + 3] as usize;
            let certificate = trial[12 * 4 + 1] as usize;
            let controller = trial[24 * 4] as usize;
            assert!(stage <= 8 && certificate <= 1 && controller <= 2);
            stage_counts[stage] += 1;
            certificate_counts[certificate] += 1;
            controller_counts[controller] += 1;
            inputs.extend(trial.iter().flat_map(|v| v.to_le_bytes()));
            mapping.push(json!({"ray":ray,"trial":index,"stage_index":stage,"certificate_valid":certificate,"controller_kind":controller}));
        }
    }
    let count = mapping.len() as u32;
    assert!(count > 0);
    let mut shader = if fast {
        trace_fixture::build_fast().shader
    } else {
        trace_fixture::build().shader
    };
    let bindings = shader.find("\n@group(0) @binding(1)").unwrap();
    shader.truncate(bindings);
    if !fast {
        shader.push_str(&trial_split::split_trial(&shader));
    }
    let mut entries = ENTRIES.to_string();
    if fast {
        let detail = "let original=nh_trial(start,step);let stages=nh_stages(start,step);let combined=nh_bounds(start,step,stages);";
        assert_eq!(entries.matches(detail).count(), 1);
        entries = entries.replace(detail, "let original=nh_trial_reference(start,step);let stages=nh_stages(start,step);let combined=nh_bounds_fast(start,step,stages);");
        entries = entries.replace(
            "nh_bounds(start,step,trial_stage(",
            "nh_bounds_fast(start,step,trial_stage(",
        );
        let reference = entries[entries
            .find("@compute @workgroup_size(32)\nfn cost_full(")
            .unwrap()
            ..entries
                .find("@compute @workgroup_size(32)\nfn cost_stages(")
                .unwrap()]
            .replace("fn cost_full(", "fn cost_reference(")
            .replace("nh_trial(start,step)", "nh_trial_reference(start,step)");
        entries.push_str(&reference);
    }
    shader.push_str(
        &entries
            .replace("__COUNT__", &format!("{count}u"))
            .replace("__INNER__", &format!("{inner}u"))
            .replace(
                "__INVOCATIONS__",
                &format!("{}u", count.checked_mul(repetitions).unwrap()),
            ),
    );
    let cost_invocations = count.checked_mul(repetitions).unwrap();
    Fixture {
        shader,
        uniform,
        inputs,
        count,
        detail_record_words: 164,
        cost_invocations,
        detail_dispatch: [count.div_ceil(32), 1, 1],
        cost_dispatch: [cost_invocations.div_ceil(32), 1, 1],
        metadata: json!({"scope":"Identical retained actual trial inputs. Full stages/certificate components are isolated; their separate timings are not additive and not native frame timings. Bounds-only includes loading retained intermediate coefficients. No result feeds a ray.",
            "capture_prefix":capture_prefix,"input_vectors":TRIAL_VECTORS,"inner":inner,"repetitions":repetitions,"upper_only_height_bounds":fast,
            "stage_counts":stage_counts,"certificate_counts":certificate_counts,"controller_counts":controller_counts,"mapping":mapping,
            "entries":if fast {vec!["trial_detail","cost_full","cost_stages","cost_bounds","cost_reference"]}else{vec!["trial_detail","cost_full","cost_stages","cost_bounds"]},
            "detail_layout":"41 vec4: original returned curve[0..10), split returned curve[10..20), split stages[20..31), bounds from captured stages[31..41). Curve: q2..q5/u2..u5/optical/error-valid-vlower-pathupper. Stage: q2..q5/u2..u5/optical/eq-en/eu-index.",
            "fast_detail_scope":"When upper_only_height_bounds=true: original[0..10) always remains old interval reference; recombined[10..20) and retained bounds[31..41) use the candidate certificate. Timed cost_full corresponds to recombined block; cost_reference corresponds to original block. Different finite bounds must be reported, not asserted identical.",
            "cost_layout":"one uvec4 checksum per invocation, consumes every returned dense/error field or intermediate coefficient/embedded field; identical input cycle (id+17*iteration)%count"}),
    }
}

const ENTRIES: &str = r#"
@group(0) @binding(1) var<storage,read> trials:array<vec4<u32>>;
@group(0) @binding(2) var<storage,read_write> trial_results:array<vec4<u32>>;
fn trial_input(index:u32)->NcState{
    let base=27u*index;
    nc_anchor=bitcast<vec2<f32>>(trials[base+2u].xy);nc_index=bitcast<vec2<f32>>(trials[base+2u].zw);
    nc_axis=bitcast<vec3<f32>>(trials[base+25u].xyz);nc_eye_height=bitcast<vec2<f32>>(trials[base+26u].xy);
    return NcState(bitcast<vec3<f32>>(trials[base].xyz),bitcast<vec3<f32>>(trials[base+1u].xyz));
}
fn trial_stage(index:u32)->NhStage{
    let base=27u*index+13u;
    let c=NcCurve(bitcast<vec3<f32>>(trials[base].xyz),bitcast<vec3<f32>>(trials[base+1u].xyz),bitcast<vec3<f32>>(trials[base+2u].xyz),bitcast<vec3<f32>>(trials[base+3u].xyz),bitcast<vec3<f32>>(trials[base+4u].xyz),bitcast<vec3<f32>>(trials[base+5u].xyz),bitcast<vec3<f32>>(trials[base+6u].xyz),bitcast<vec3<f32>>(trials[base+7u].xyz),bitcast<vec4<f32>>(trials[base+8u]),0.0,select(0u,1u,trials[base+10u].w==8u));
    return NhStage(c,bitcast<vec3<f32>>(trials[base+9u].xyz),bitcast<vec3<f32>>(trials[base+10u].xyz),bitcast<f32>(trials[base+9u].w),trials[base+10u].w);
}
fn trial_write_curve(base:u32,curve:NhCurve){
    let c=curve.data;
    trial_results[base]=bitcast<vec4<u32>>(vec4(c.q2,0.0));trial_results[base+1u]=bitcast<vec4<u32>>(vec4(c.q3,0.0));
    trial_results[base+2u]=bitcast<vec4<u32>>(vec4(c.q4,0.0));trial_results[base+3u]=bitcast<vec4<u32>>(vec4(c.q5,0.0));
    trial_results[base+4u]=bitcast<vec4<u32>>(vec4(c.u2,0.0));trial_results[base+5u]=bitcast<vec4<u32>>(vec4(c.u3,0.0));
    trial_results[base+6u]=bitcast<vec4<u32>>(vec4(c.u4,0.0));trial_results[base+7u]=bitcast<vec4<u32>>(vec4(c.u5,0.0));
    trial_results[base+8u]=bitcast<vec4<u32>>(c.optical);
    trial_results[base+9u]=vec4(bitcast<u32>(c.error),c.valid,bitcast<u32>(curve.vertical_lower),bitcast<u32>(curve.path_upper));
}
fn trial_write_stage(base:u32,stage:NhStage){
    trial_write_curve(base,NhCurve(stage.data,0.0,0.0,0.0));
    trial_results[base+9u]=bitcast<vec4<u32>>(vec4(stage.eq,stage.en));
    trial_results[base+10u]=vec4(bitcast<vec3<u32>>(stage.eu),stage.index);
}
fn trial_hash(h:vec4<u32>,word:vec4<u32>)->vec4<u32>{return (h^word)*vec4(16777619u)+vec4(2166136261u);}
fn trial_hash_dense(c:NcCurve)->vec4<u32>{
    var h=vec4(0u);
    h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.q2,0.0)));h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.q3,0.0)));
    h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.q4,0.0)));h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.q5,0.0)));
    h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.u2,0.0)));h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.u3,0.0)));
    h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.u4,0.0)));h=trial_hash(h,bitcast<vec4<u32>>(vec4(c.u5,0.0)));
    return trial_hash(h,bitcast<vec4<u32>>(c.optical));
}
fn trial_hash_curve(c:NhCurve)->vec4<u32>{return trial_hash(trial_hash_dense(c.data),vec4(bitcast<u32>(c.data.error),c.data.valid,bitcast<u32>(c.vertical_lower),bitcast<u32>(c.path_upper)));}
fn trial_hash_stage(s:NhStage)->vec4<u32>{return trial_hash(trial_hash(trial_hash_dense(s.data),bitcast<vec4<u32>>(vec4(s.eq,s.en))),vec4(bitcast<vec3<u32>>(s.eu),s.index));}
@compute @workgroup_size(32)
fn trial_detail(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=__COUNT__{return;}let start=trial_input(id.x);let step=bitcast<f32>(trials[27u*id.x].w);
    let original=nh_trial(start,step);let stages=nh_stages(start,step);let combined=nh_bounds(start,step,stages);
    let retained=nh_bounds(start,step,trial_stage(id.x));
    trial_write_curve(41u*id.x,original);trial_write_curve(41u*id.x+10u,combined);
    trial_write_stage(41u*id.x+20u,stages);trial_write_curve(41u*id.x+31u,retained);
}
@compute @workgroup_size(32)
fn cost_full(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=__INVOCATIONS__{return;}var h=vec4(0u);
    for(var iteration=0u;iteration<__INNER__;iteration+=1u){let index=(id.x+17u*iteration)%__COUNT__;let start=trial_input(index);let step=bitcast<f32>(trials[27u*index].w);h=trial_hash(h,trial_hash_curve(nh_trial(start,step)));}
    trial_results[id.x]=h;
}
@compute @workgroup_size(32)
fn cost_stages(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=__INVOCATIONS__{return;}var h=vec4(0u);
    for(var iteration=0u;iteration<__INNER__;iteration+=1u){let index=(id.x+17u*iteration)%__COUNT__;let start=trial_input(index);let step=bitcast<f32>(trials[27u*index].w);h=trial_hash(h,trial_hash_stage(nh_stages(start,step)));}
    trial_results[id.x]=h;
}
@compute @workgroup_size(32)
fn cost_bounds(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=__INVOCATIONS__{return;}var h=vec4(0u);
    for(var iteration=0u;iteration<__INNER__;iteration+=1u){let index=(id.x+17u*iteration)%__COUNT__;let start=trial_input(index);let step=bitcast<f32>(trials[27u*index].w);h=trial_hash(h,trial_hash_curve(nh_bounds(start,step,trial_stage(index))));}
    trial_results[id.x]=h;
}
"#;
