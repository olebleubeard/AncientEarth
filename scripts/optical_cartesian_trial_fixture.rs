//! Original MIT. Fixed eleven-handoff standalone Cartesian trial screen.
//! Logged accepted distances are new trial lengths, not original full trials.
use super::fixture::Fixture;
use serde_json::{Value, json};

const SELECTED: &str = "artifacts/optical-cartesian-suffix-tight-selected-first";
const ATMOSPHERE: &str =
    "artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first";
const IDS: [usize; 10] = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20];
const INPUT_VECTORS: usize = 9;
const CORE_SCALARS: usize = 80;
const DETAIL_SCALARS: usize = 188;

fn read_words(path: &str) -> Vec<u32> {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .collect()
}
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
fn promote(source: &str) -> String {
    // Same expression tree/rational literals, evaluated in binary64. Uploads
    // are widened exactly; no ideal Q, slope recomputation or initial u norm.
    let mut output = source
        .replace("f32", "f64")
        .replace("vec3<u32>", "vec3<u64>")
        .replace("vec2(", "vec2<f64>(")
        .replace("vec3(", "vec3<f64>(")
        .replace("vec4(", "vec4<f64>(")
        .replace("var h=0.0", "var h:f64=0.0")
        .replace("geometry(", "ct64_geometry(");
    for name in [
        "dopri_delta",
        "dp_invalid",
        "dp_finite",
        "qmul",
        "DpStep",
        "State",
        "settings",
    ] {
        output = output.replace(name, &format!("ct64_{name}"));
    }
    output
}
fn detail_trial(source: &str, wide: bool) -> String {
    let mut output = function(
        source,
        if wide {
            "ct64_dopri_delta"
        } else {
            "dopri_delta"
        },
    );
    let name = if wide {
        "ct64_dopri_delta"
    } else {
        "dopri_delta"
    };
    once(
        &mut output,
        &format!("fn {name}("),
        &format!("fn {name}_detail("),
    );
    let finite = if wide { "ct64_dp_finite" } else { "dp_finite" };
    let invalid = if wide {
        "ct64_dp_invalid"
    } else {
        "dp_invalid"
    };
    let vec = if wide { "vec4<f64>" } else { "vec4" };
    let bits = if wide { "64" } else { "32" };
    for i in 0..7 {
        let line = format!("if !{finite}({vec}(b{i},0.0)){{return {invalid}();}}");
        let p = if i == 0 {
            "s.p".to_owned()
        } else {
            format!("s.p+delta{i}")
        };
        let u = if i == 0 {
            "s.u".to_owned()
        } else {
            format!("u{i}")
        };
        once(
            &mut output,
            &line,
            &format!(
                "{line}\n    ct_stage_{bits}({}u,{p},{u},g{i},n{i},b{i});",
                i * 14
            ),
        );
    }
    let error_line = "let error=max(max(length(ep),length(eu)),abs(en))+interior;";
    once(
        &mut output,
        error_line,
        &format!("{error_line}\n    ct_errors_{bits}(ep,eu,en,m,interior,nr5);"),
    );
    output
}
fn diagnostics(wide: bool) -> String {
    let t = if wide { "f64" } else { "f32" };
    let b = if wide { "64" } else { "32" };
    format!(
        r#"
var<private> ct_capture_{b}:array<{t},108>;
fn ct_stage_{b}(base:u32,p:vec3<{t}>,u:vec3<{t}>,g:vec4<{t}>,n:{t},bend:vec3<{t}>){{
    for(var j=0u;j<3u;j+=1u){{ct_capture_{b}[base+j]=p[j];ct_capture_{b}[base+3u+j]=u[j];ct_capture_{b}[base+11u+j]=bend[j];}}
    for(var j=0u;j<4u;j+=1u){{ct_capture_{b}[base+6u+j]=g[j];}}ct_capture_{b}[base+10u]=n;
}}
fn ct_errors_{b}(ep:vec3<{t}>,eu:vec3<{t}>,en:{t},m:{t},interior:{t},nr5:{t}){{
    for(var j=0u;j<3u;j+=1u){{ct_capture_{b}[98u+j]=ep[j];ct_capture_{b}[101u+j]=eu[j];}}
    ct_capture_{b}[104u]=en;ct_capture_{b}[105u]=m;ct_capture_{b}[106u]=interior;ct_capture_{b}[107u]=nr5;
}}
"#
    )
}
fn core(wide: bool, checksum: bool) -> String {
    let t = if wide { "f64" } else { "f32" };
    let b = if wide { "64" } else { "32" };
    let state = if wide { "ct64_State" } else { "State" };
    let step = if wide { "ct64_DpStep" } else { "DpStep" };
    let fields = [
        "c.delta.p.x",
        "c.delta.p.y",
        "c.delta.p.z",
        "c.delta.u.x",
        "c.delta.u.y",
        "c.delta.u.z",
        "c.error",
        "c.geometry.x",
        "c.geometry.y",
        "c.geometry.z",
        "c.geometry.w",
        "c.r3.x",
        "c.r3.y",
        "c.r3.z",
        "c.r4.x",
        "c.r4.y",
        "c.r4.z",
        "c.r5.x",
        "c.r5.y",
        "c.r5.z",
        "c.du.x",
        "c.du.y",
        "c.du.z",
        "c.ur3.x",
        "c.ur3.y",
        "c.ur3.z",
        "c.ur4.x",
        "c.ur4.y",
        "c.ur4.z",
        "c.ur5.x",
        "c.ur5.y",
        "c.ur5.z",
    ];
    let mut output = if checksum {
        format!(
            "fn ct_hash_core_{b}(s:{state},h:{t},c:{step},seed:vec4<u32>)->vec4<u32>{{var hash=seed;\n"
        )
    } else {
        format!(
            "fn ct_core_{b}(s:{state},h:{t},c:{step})->array<{t},80>{{var result:array<{t},80>;\n"
        )
    };
    for (i, field) in fields.iter().enumerate() {
        output.push_str(&if checksum {
            format!("hash=ct_hash_value(hash,f64({field}));\n")
        } else {
            format!("result[{i}u]={field};\n")
        });
    }
    output.push_str(&format!(r#"
    for(var i=0u;i<5u;i+=1u){{
        let theta={t}(i)*{t}(0.25);let v={t}(1.0)-theta;
        let p=s.p+theta*(c.delta.p+v*(c.r3+theta*(c.r4+v*c.r5)));
        let u=normalize(s.u+theta*(c.du+v*(c.ur3+theta*(c.ur4+v*c.ur5))));
        let derivative=(c.delta.p+({t}(1.0)-{t}(2.0)*theta)*c.r3+({t}(2.0)*theta-{t}(3.0)*theta*theta)*c.r4+({t}(2.0)*theta-{t}(6.0)*theta*theta+{t}(4.0)*theta*theta*theta)*c.r5)/h;
"#));
    if checksum {
        for value in [
            "p.x",
            "p.y",
            "p.z",
            "u.x",
            "u.y",
            "u.z",
            "derivative.x",
            "derivative.y",
            "derivative.z",
        ] {
            output.push_str(&format!("hash=ct_hash_value(hash,f64({value}));\n"));
        }
        output.push_str("}\nhash=ct_hash_value(hash,f64(h));hash=ct_hash_value(hash,f64(0.0));hash=ct_hash_value(hash,f64(0.0));return hash;\n}\n");
    } else {
        output.push_str(&format!("for(var j=0u;j<3u;j+=1u){{result[32u+9u*i+j]=p[j];result[35u+9u*i+j]=u[j];result[38u+9u*i+j]=derivative[j];}}\n}}\nresult[77u]=h;result[78u]={t}(0.0);result[79u]={t}(0.0);return result;\n}}\n"));
    }
    output
}

pub fn build(inner: u32, repetitions: u32) -> Fixture {
    assert_eq!(
        (inner, repetitions),
        (8, 256),
        "One prospective workload only"
    );
    let mut cases = vec![(ATMOSPHERE, 1usize)];
    cases.extend(IDS.iter().map(|&i| (SELECTED, i)));
    let mut inputs = Vec::new();
    let mut mapping = Vec::new();
    for (index, (prefix, id)) in cases.iter().enumerate() {
        let report: Value =
            serde_json::from_slice(&std::fs::read(format!("{prefix}.json")).unwrap()).unwrap();
        assert_eq!(report["record_count"], 23);
        assert_eq!(report["selected"], true);
        assert_eq!(report["timed"], false);
        let primary = read_words(&format!("{prefix}-primary.bin"));
        assert_eq!(primary.len(), 560);
        let states = read_words(&format!("{prefix}-states.bin"));
        assert_eq!(states.len(), 23 * 24);
        let state = &states[id * 24..(id + 1) * 24];
        assert_eq!(
            (
                state[11], state[14], state[15], state[21], state[22], state[23]
            ),
            (0, 0, 0, *id as u32, 1, 0)
        );
        assert!(state[..8].iter().all(|&x| f32::from_bits(x).is_finite()));
        let count = f32::from_bits(primary[36]);
        assert!(count.fract() == 0.0 && (2.0..=128.0).contains(&count));
        let owner = state[20] as i32;
        assert!((-1..count as i32 - 1).contains(&owner));
        let branch = if owner < 0 {
            [primary[41], primary[49], 0]
        } else {
            primary[48 + owner as usize * 4..51 + owner as usize * 4]
                .try_into()
                .unwrap()
        };
        let log = read_words(&format!("{prefix}-auxiliary.bin"));
        assert_eq!(log.len(), 23 * 16388);
        let start = id * 16388;
        let n = log[start];
        assert!(n > 0 && n <= 4096);
        assert_eq!(
            (log[start + 1], log[start + 2], log[start + 3]),
            (0, *id as u32, primary[46])
        );
        let first = &log[start + 4..start + 8];
        assert_eq!(first[1], state[3]);
        assert!(first[2] > 0 && first[3] <= 1);
        let distance = f32::from_bits(first[0]);
        assert!(distance.is_finite() && distance >= 0.000001 && distance <= 100000.0);
        let mut record = Vec::new();
        record.extend_from_slice(&primary[..16]);
        record.extend_from_slice(&state[..4]);
        record.extend_from_slice(&state[4..7]);
        record.push(first[0]);
        record.extend_from_slice(&branch);
        record.push(state[20]);
        record.extend_from_slice(&state[16..20]);
        record.extend_from_slice(&[index as u32, *id as u32, first[2], first[3]]);
        assert_eq!(record.len(), INPUT_VECTORS * 4);
        inputs.extend(record.iter().flat_map(|x| x.to_le_bytes()));
        mapping.push(json!({"input_index":index,"source_prefix":prefix,"record_index":id,"source_owner":owner,
            "saved_state_words":state,"primary_words":primary,"first_log_words":first,"log_count":n,"input_words":record,
            "distance_scope":"First accepted logged distance reused as a NEW standalone trial from the retained handoff. Rejections, original full step and source clipping are not reconstructed."}));
    }
    let production = std::fs::read_to_string("src/desktop/shaders/refraction.wesl")
        .unwrap()
        .replace("\r\n", "\n");
    let dp = std::fs::read_to_string("src/desktop/shaders/refraction_dopri_hermite.wesl")
        .unwrap()
        .replace("\r\n", "\n");
    let original = format!(
        "struct State{{p:vec3<f32>,u:vec3<f32>}};\n{}\n{}\n{dp}",
        function(&production, "qmul"),
        function(&production, "geometry")
    );
    let promoted = promote(&original);
    let settings = "struct CtSettings{q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>};\nvar<private> settings:CtSettings;\nstruct CtSettings64{q0:vec4<f64>,q1:vec4<f64>,q2:vec4<f64>,origin:vec4<f64>};\nvar<private> ct64_settings:CtSettings64;\n";
    let shader = format!(
        "{settings}\n{original}\n{promoted}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{ENTRIES}",
        diagnostics(false),
        diagnostics(true),
        detail_trial(&original, false),
        detail_trial(&promoted, true),
        core(false, false),
        core(true, false),
        core(false, true),
        core(true, true)
    );
    let count = 11u32;
    let cost_invocations = count * repetitions;
    let uniform = [count, inner, cost_invocations, 0x43545031u32]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    Fixture {
        shader,
        uniform,
        inputs,
        count,
        detail_record_words: (4 + 4 * DETAIL_SCALARS) as u32,
        cost_invocations,
        detail_dispatch: [count.div_ceil(32), 1, 1],
        cost_dispatch: [cost_invocations.div_ceil(32), 1, 1],
        metadata: json!({"scope":"One native promoted Cartesian trial rejection screen; no controller, root, scene, complete ray or native FPS claim.",
            "entries":["trial_detail","cost_reference","cost_promoted"],"inner":inner,"repetitions":repetitions,
            "input_vectors":INPUT_VECTORS,"mapping":mapping,"core_scalars":CORE_SCALARS,"detail_scalars_per_precision":DETAIL_SCALARS,
            "detail_layout":"4u32 index/owner/first-log-attempt/kind, then reference188 and promoted188 scalar values encoded as little-endian f64 low/high u32. Reference f32 values widen exactly. Per block0:32 DpStep fields;32:77 five theta P/U/dPds;77 step;78:80 zero;80:178 seven stage p/u/g/N/bend;178:188 ep/eu/en/m/interior/nr5.",
            "input_layout":"9vec4u: exact primary q0/q1/q2/origin, saved p/path, saved u/new trial length, uploaded branch/owner, saved g (provenance only), inputID/sourceID/logordinal/logkind.",
            "precision":"Reference original source expressions. Promoted same rational expression tree typed f64, exact widened packet constants, fresh geometry at saved p, raw saved u, retained five updates and exact fixed-point test. No initial normalization or ideal-matrix substitution.",
            "cost_layout":"One uvec4 checksum/invocation. Eight cycles(index=id+17*iteration mod11); all80 returned/dense/step scalars consumed as both low/high f64 words (reference f32 widens exactly). Same binary encoding/hash work in both modes. Streaming hash avoids a cost-only 80scalar temporary array. Exact contextual checksum equality required before timing interpretation; hashes are not collision-free equality proof.",
            "detail_instrumentation":"Separate detail-only copies capture stage/error intermediates. Cost copies retain uninstrumented original/promoted bodies; returned/dense cost checksums must match the detail-derived values.",
            "policy":"docs/science/cartesian-promoted-trial-policy.md"}),
    }
}

const ENTRIES: &str = r#"
@group(0) @binding(0) var<uniform> ct_policy:vec4<u32>;
@group(0) @binding(1) var<storage,read> ct_inputs:array<vec4<u32>>;
@group(0) @binding(2) var<storage,read_write> ct_output:array<u32>;
fn ct_load_32(index:u32){
    let b=9u*index;
    settings=CtSettings(bitcast<vec4<f32>>(ct_inputs[b]),bitcast<vec4<f32>>(ct_inputs[b+1u]),bitcast<vec4<f32>>(ct_inputs[b+2u]),bitcast<vec4<f32>>(ct_inputs[b+3u]));
}
fn ct_load_64(index:u32){
    let b=9u*index;
    ct64_settings=CtSettings64(vec4<f64>(bitcast<vec4<f32>>(ct_inputs[b])),vec4<f64>(bitcast<vec4<f32>>(ct_inputs[b+1u])),vec4<f64>(bitcast<vec4<f32>>(ct_inputs[b+2u])),vec4<f64>(bitcast<vec4<f32>>(ct_inputs[b+3u])));
}
fn ct_hash(h:vec4<u32>,w:u32)->vec4<u32>{return (h^vec4(w))*vec4(16777619u)+vec4(2166136261u);}
fn ct_hash_value(h:vec4<u32>,value:f64)->vec4<u32>{let bits=bitcast<u64>(value);return ct_hash(ct_hash(h,u32(bits)),u32(bits>>32u));}
fn ct_write64(word:u32,value:f64){let bits=bitcast<u64>(value);ct_output[word]=u32(bits);ct_output[word+1u]=u32(bits>>32u);}
@compute @workgroup_size(32)
fn trial_detail(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=ct_policy.x{return;}ct_load_32(id.x);ct_load_64(id.x);let b=9u*id.x;
    let p=bitcast<vec3<f32>>(ct_inputs[b+4u].xyz);let u=bitcast<vec3<f32>>(ct_inputs[b+5u].xyz);
    let h=bitcast<f32>(ct_inputs[b+5u].w);let branch=bitcast<vec3<f32>>(ct_inputs[b+6u].xyz);
    let s=State(p,u);let s64=ct64_State(vec3<f64>(p),vec3<f64>(u));
    for(var j=0u;j<108u;j+=1u){ct_capture_32[j]=0.0;ct_capture_64[j]=f64(0.0);}
    let a=dopri_delta_detail(s,h,branch,geometry(p));
    let z=ct64_dopri_delta_detail(s64,f64(h),vec3<f64>(branch),ct64_geometry(s64.p));
    let x=ct_core_32(s,h,a);let y=ct_core_64(s64,f64(h),z);let output_base=756u*id.x;
    ct_output[output_base]=id.x;ct_output[output_base+1u]=ct_inputs[b+6u].w;ct_output[output_base+2u]=ct_inputs[b+8u].z;ct_output[output_base+3u]=ct_inputs[b+8u].w;
    for(var j=0u;j<80u;j+=1u){ct_write64(output_base+4u+2u*j,f64(x[j]));ct_write64(output_base+380u+2u*j,y[j]);}
    for(var j=0u;j<108u;j+=1u){ct_write64(output_base+164u+2u*j,f64(ct_capture_32[j]));ct_write64(output_base+540u+2u*j,ct_capture_64[j]);}
}
@compute @workgroup_size(32)
fn cost_reference(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=ct_policy.z{return;}var hash=vec4(0u);
    for(var k=0u;k<ct_policy.y;k+=1u){let index=(id.x+17u*k)%ct_policy.x;ct_load_32(index);let b=9u*index;
        let s=State(bitcast<vec3<f32>>(ct_inputs[b+4u].xyz),bitcast<vec3<f32>>(ct_inputs[b+5u].xyz));let h=bitcast<f32>(ct_inputs[b+5u].w);
        let c=dopri_delta(s,h,bitcast<vec3<f32>>(ct_inputs[b+6u].xyz),geometry(s.p));hash=ct_hash_core_32(s,h,c,hash);}
    for(var j=0u;j<4u;j+=1u){ct_output[4u*id.x+j]=hash[j];}
}
@compute @workgroup_size(32)
fn cost_promoted(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=ct_policy.z{return;}var hash=vec4(0u);
    for(var k=0u;k<ct_policy.y;k+=1u){let index=(id.x+17u*k)%ct_policy.x;ct_load_64(index);let b=9u*index;
        let s=ct64_State(vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+4u].xyz)),vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+5u].xyz)));let h=f64(bitcast<f32>(ct_inputs[b+5u].w));
        let c=ct64_dopri_delta(s,h,vec3<f64>(bitcast<vec3<f32>>(ct_inputs[b+6u].xyz)),ct64_geometry(s.p));hash=ct_hash_core_64(s,h,c,hash);}
    for(var j=0u;j<4u;j+=1u){ct_output[4u*id.x+j]=hash[j];}
}
"#;
