//! Original MIT. Split scene consumer on retained accepted GPU height curves.
//! No field integration or source-owner commit is reachable from this entry.
//! The initial input adapter preserves the complete original 108-word capture;
//! a future producer must certify its own coefficient/encoding allowance.
use super::{clear, Fixture, HEADER};
use serde_json::{json, Value};
fn words(bytes:&[u8])->Vec<u32>{assert!(bytes.len().is_multiple_of(4));bytes.chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect()}
fn bytes(words:&[u32])->Vec<u8>{words.iter().flat_map(|w|w.to_le_bytes()).collect()}
pub struct CurvesFixture{pub scene:Fixture,pub curves:Vec<u8>}
pub fn build()->CurvesFixture{
    const PREFIX:&str="artifacts/optical-height-trial-capture";
    let mut fixture=super::build(true);
    let metadata:Value=serde_json::from_slice(&std::fs::read(format!("{PREFIX}.json")).unwrap()).unwrap();
    let captured_uniform=std::fs::read(format!("{PREFIX}-uniform.bin")).unwrap();
    assert_eq!(captured_uniform,fixture.field_uniform,"Captured field must match canonical scene field");
    let uniform=words(&captured_uniform);let certificates=words(&fixture.certificates);
    let raw=words(&std::fs::read(format!("{PREFIX}-readback.bin")).unwrap());
    let stride=usize::try_from(metadata["record_words"].as_u64().unwrap()).unwrap();
    let rays=usize::try_from(metadata["count"].as_u64().unwrap()).unwrap();
    assert_eq!(stride,13864);assert_eq!(raw.len(),stride*rays);
    let layer_count=certificates[2] as usize;assert!((2..=128).contains(&layer_count));
    let mut inputs=Vec::new();let mut rows=Vec::new();
    for ray in 0..rays{
        let record=&raw[ray*stride..(ray+1)*stride];let count=record[36] as usize;
        assert_eq!(record[37],128);assert_eq!(record[38],0);assert!(count<=128);
        for trial in 0..count{
            let input=&record[40+108*trial..40+108*(trial+1)];
            if input[96]!=2{continue;}
            assert_eq!(input[49],1);assert_eq!(input[95],8);
            let anchor=&input[8..10];let residual;
            let mut matches=Vec::new();
            for node in 0..layer_count{if anchor==&uniform[20+4*node..22+4*node]{matches.push(node);}}
            if anchor==&uniform[2..4]{assert!(matches.is_empty());residual=[certificates[16],certificates[17]];}
            else{assert_eq!(matches.len(),1,"Every accepted source anchor must be identified exactly");let base=4*(5+matches[0]);residual=[certificates[base],certificates[base+1]];}
            inputs.extend_from_slice(input);inputs.extend_from_slice(&[residual[0],residual[1],ray as u32,trial as u32]);
            rows.push(json!({"record_index":rows.len(),"capture_ray":ray,"capture_trial":trial,"anchor_words":anchor,"canonical_anchor_residual_words":residual}));
        }
    }
    assert_eq!(rows.len(),495,"Preserve the reviewed actual accepted-trial workload");
    let count=u32::try_from(rows.len()).unwrap();assert_eq!(inputs.len(),count as usize*112);
    let mut shader=clear::build_metric_fast().shader;
    let marker="@group(0) @binding(1) var<storage,read> rays:";assert_eq!(shader.matches(marker).count(),1);shader.truncate(shader.find(marker).unwrap());
    super::replace_once(&mut shader,"@group(0) @binding(0) var<uniform> nc_settings:NcSettings;","@group(0) @binding(4) var<uniform> nc_settings:NcSettings;");
    for path in ["scripts/optical_normal_local.wgsl","scripts/optical_normal_scene_curve.wgsl","scripts/optical_normal_map_interval.wgsl","scripts/optical_normal_face_cached.wgsl","scripts/optical_normal_scene_query_cached.wgsl"]{shader.push('\n');shader.push_str(&std::fs::read_to_string(path).unwrap());}
    shader.push_str(HEADER);shader.push_str(&ENTRY.replace("RECORD_COUNT",&format!("{count}u")));
    let control=[count,1,0x12345678,0x9abcdef0];fixture.primary_uniform[176..192].copy_from_slice(&bytes(&control));
    fixture.shader=shader;fixture.width=count;fixture.height=1;fixture.record_count=count;fixture.dispatch=[count.div_ceil(32),1,1];fixture.control=control;
    fixture.terminal_bytes=16+48*u64::from(count);fixture.output_bytes=16+80*u64::from(count);
    fixture.sample_pixels=(0..count).map(|i|[i,0]).collect();fixture.sample_indices=(0..count).collect();
    fixture.cases=json!({"scope":"Split scene-consumer diagnostic on retained accepted GPU height intervals. Each record is independent; no field solve, source ownership commit, first-full-ray contact, atmospheric exit or native speed claim. Storage coordinates identify curve records, not camera pixels. Clear-interval status4 is distinct from sky status0. Returned path is local to this interval. Canonical coefficient encoding is exact captured f32; existing curve/source allowance is retained.",
        "capture_prefix":PREFIX,"input_words_per_record":112,"capture_words_preserved":108,"appended_words":"canonical anchor residual low/high, capture ray ID, capture trial ID",
        "records":rows,"packet_scope":fixture.cases,"record_layout":["16-byte current control header","48-byte terminal-shaped records: local p/path,u/status,normal/material-owner; status4 clear interval,1 contact,2 uncertainty","32-byte details: eventkind/scenevisits/capture-ray/capture-trial; theta/lower/upper/tolerance bits"],
        "continuation_contract":"A production producer must preserve the current token, source ownership/anchor/gaps, accumulated path/excess and complete numerical plus representation/encoding allowance. Scene clear/hit/ambiguous is consumed before any commit. This retained input adapter does not itself provide a production continuation queue."});
    CurvesFixture{scene:fixture,curves:bytes(&inputs)}
}
const ENTRY:&str=r#"
@group(0) @binding(6) var<storage,read> nc_curve_inputs:array<vec4<u32>>;
@compute @workgroup_size(32)
fn normal_scene(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=RECORD_COUNT{return;}if id.x==0u{terminals[0]=settings.control;}
    let base=id.x*28u;let index=1u+id.x*3u;let detail_index=1u+3u*RECORD_COUNT+id.x*2u;
    let s=bitcast<vec4<f32>>(nc_curve_inputs[base]);let start=NcState(s.xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+1u]).xyz);
    let field=bitcast<vec4<f32>>(nc_curve_inputs[base+2u]);nc_anchor=field.xy;nc_index=field.zw;
    nc_axis=bitcast<vec4<f32>>(nc_curve_inputs[base+25u]).xyz;nc_eye_height=bitcast<vec4<f32>>(nc_curve_inputs[base+26u]).xy;
    let extra=nc_curve_inputs[base+27u];nc_scene_anchor_residual=bitcast<vec2<f32>>(extra.xy);nc_scene_visits=0u;nc_root_count=0u;
    nc_scene_normal=vec3(0.0);nc_scene_material=0u;nc_scene_owner=0u;
    let limits=nc_curve_inputs[base+12u];let accepted=nc_curve_inputs[base+24u];
    let data=NcCurve(bitcast<vec4<f32>>(nc_curve_inputs[base+3u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+4u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+5u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+6u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+7u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+8u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+9u]).xyz,bitcast<vec4<f32>>(nc_curve_inputs[base+10u]).xyz,
        bitcast<vec4<f32>>(nc_curve_inputs[base+11u]),bitcast<f32>(limits.x),limits.y);
    let height=NhCurve(data,s.w,bitcast<f32>(limits.z),bitcast<f32>(limits.w));
    let curve=nc_scene_from_height(start,height,bitcast<f32>(accepted.y));let tolerance=bitcast<f32>(accepted.z);
    var event=nc_scene_uncertain();if accepted.x==2u && curve.valid==1u && curve.numerical_error<=tolerance{event=nc_scene_query_cached(curve,tolerance);}
    var theta=0.0;var status=2u;if event.kind==0u{theta=curve.theta_end;status=4u;}if event.kind==1u{theta=event.theta;status=1u;}
    terminals[index]=bitcast<vec4<u32>>(vec4(nc_position(nc_scene_q(curve,theta)),nc_scene_path(curve,theta)));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(nc_scene_direction(curve,theta)),status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(event.normal),(event.material<<24u)|event.owner);
    terminals[detail_index]=vec4(event.kind,nc_scene_visits,extra.zw);
    terminals[detail_index+1u]=bitcast<vec4<u32>>(vec4(event.theta,event.lower,event.upper,tolerance));
}
"#;
