//! Original MIT. Isolated canonical scene + height/path controller fixture.
//! All geometry/certificates are immutable preparation outputs; no application
//! registration. The reference retains the original primary numerical source.
#[path = "optical_normal_trace_fixture.rs"]
mod clear;
#[path = "optical_normal_scene_curves_fixture.rs"]
mod curves;
#[path = "optical_midpoint_reuse.rs"]
mod midpoint_reuse;
#[path = "optical_cartesian_scene_fixture.rs"]
mod cartesian;
#[path = "optical_scene_stage_diagnostic.rs"]
mod stage_diagnostic;
#[path = "optical_positive_contact_fixture.rs"]
mod positive_contacts;
#[path = "optical_positive_stage_diagnostic.rs"]
mod positive_stages;
#[path = "optical_positive_chronology_fixture.rs"]
mod positive_chronology;
pub use curves::CurvesFixture;
pub fn build_curves()->CurvesFixture{curves::build()}
pub fn build_cartesian_curves()->CurvesFixture{cartesian::build()}
pub fn build_cartesian_covered()->CurvesFixture{cartesian::build_covered()}
pub fn build_cartesian_stages()->CurvesFixture{stage_diagnostic::build()}
pub fn build_positive_contacts()->CurvesFixture{positive_contacts::build()}
pub fn build_positive_stages()->CurvesFixture{positive_stages::build()}
pub fn build_positive_chronology()->CurvesFixture{positive_chronology::build()}
use serde_json::{Value,json};
pub struct Fixture{
    pub shader:String,pub primary_uniform:Vec<u8>,pub field_uniform:Vec<u8>,
    pub geometry:Vec<u8>,pub frames:Vec<u8>,pub certificates:Vec<u8>,
    pub width:u32,pub height:u32,pub record_count:u32,pub dispatch:[u32;3],
    pub output_bytes:u64,pub terminal_bytes:u64,pub sample_pixels:Vec<[u32;2]>,
    pub sample_indices:Vec<u32>,pub control:[u32;4],pub cases:Value,
}
const WIDTH:u32=1920;const HEIGHT:u32=1080;
const CONTROL:[u32;4]=[WIDTH,HEIGHT,0x12345678,0x9abcdef0];
const PACKET:&str="artifacts/optical-normal-scene-packet";
fn replace_once(source:&mut String,old:&str,new:&str){assert_eq!(source.matches(old).count(),1,"Scene fixture source drift: {old}");*source=source.replacen(old,new,1);}
fn compose_candidate(mut shader:String)->String{
    let marker="@group(0) @binding(1) var<storage,read> rays:";
    assert_eq!(shader.matches(marker).count(),1);shader.truncate(shader.find(marker).unwrap());
    replace_once(&mut shader,"@group(0) @binding(0) var<uniform> nc_settings:NcSettings;","@group(0) @binding(4) var<uniform> nc_settings:NcSettings;");
    // Keep the clear-ray controller as an unused reference. Both accepted
    // parameterizations enter the separately reviewed common scene gate once.
    for path in ["scripts/optical_normal_local.wgsl","scripts/optical_normal_scene_curve.wgsl","scripts/optical_normal_map_interval.wgsl","scripts/optical_normal_face_interval.wgsl","scripts/optical_normal_scene_query.wgsl","scripts/optical_normal_scene_trace.wgsl"]{
        shader.push('\n');shader.push_str(&std::fs::read_to_string(path).unwrap());
    }
    shader
}
fn compose_reference()->String{
    let source=std::fs::read_to_string("src/desktop/shaders/refraction.wesl").unwrap();
    let core=&source[source.find("fn qmul").unwrap()..source.find("fn relative_angles").unwrap()];
    let source=std::fs::read_to_string("src/desktop/shaders/refraction_scene.wesl").unwrap();
    let scene=source[..source.find("fn sign_row(").unwrap()].replace("constants::MATERIAL_BIND_GROUP","0").replace("@binding(9)","@binding(1)").replace("@binding(10)","@binding(2)");
    format!("{core}\n{scene}\n")
}
pub fn build(selected:bool)->Fixture{build_mode(selected,false)}
pub fn build_reference(selected:bool)->Fixture{build_mode(selected,true)}
pub fn build_reference_reuse(selected:bool)->Fixture{
    let mut fixture=build_reference(selected);
    fixture.shader=midpoint_reuse::apply(&fixture.shader);
    fixture.cases["reference_reuse"]=json!({
        "scope":"Original primary midpoint field/controller/scene with its immutable start geometry/index reused in first midpoint stages. Second-half start is evaluated separately; all event equations and budgets retained. Finite compiler-context terminal parity remains to be measured.",
        "source":"scripts/optical_midpoint_reuse.rs",
        "geometry_source":"current trace iteration geometry(state.p)",
        "index_source":"index_gradient(g.w), after all pre-trial terminal and step guards",
        "state_mutation":"only the unchanged final accepted state=half; all reused calls precede it"
    });
    fixture
}
pub fn build_reference_carry(selected:bool)->Fixture{
    let mut fixture=build_reference(selected);
    fixture.shader=midpoint_reuse::apply_carry(&fixture.shader);
    fixture.cases["reference_carry"]=json!({
        "scope":"Separate original-midpoint reuse diagnostic: includes immutable first-stage field reuse and carries geometry(half.p) only with the unchanged accepted state=half commit. Rejected attempts keep their unchanged start geometry. No field/controller/contact arithmetic or budget changes; finite compiler-context parity remains to be measured.",
        "source":"scripts/optical_midpoint_reuse.rs"
    });
    fixture
}
fn build_mode(selected:bool,reference:bool)->Fixture{
    let clear=clear::build_metric_fast();let mut shader=if reference{compose_reference()}else{compose_candidate(clear.shader)};
    let packet:Value=serde_json::from_slice(&std::fs::read(format!("{PACKET}.json")).unwrap()).unwrap();
    let settings=std::fs::read("artifacts/optical-requested-stellar-sparse-chain-uniform.bin").unwrap();
    let view=std::fs::read("artifacts/optical-requested-stellar-sparse-chain-view.bin").unwrap();assert_eq!(view.len(),80);assert!(settings.len()>=135*16);
    // Exact production PrimarySettings order. This retained original source
    // packet is also the unchanged reference's field; candidate field is the
    // separately reviewed canonical source packet, kept explicit in metadata.
    let mut primary_uniform=Vec::new();primary_uniform.extend_from_slice(&settings[..5*16]);
    primary_uniform.extend_from_slice(&view[..4*16]);primary_uniform.extend_from_slice(&settings[5*16..7*16]);
    primary_uniform.extend(CONTROL.into_iter().flat_map(u32::to_le_bytes));primary_uniform.extend_from_slice(&settings[7*16..135*16]);assert_eq!(primary_uniform.len(),140*16);
    let selected_source="artifacts/optical-selected-launch-review.json";
    let old:Value=serde_json::from_slice(&std::fs::read(selected_source).unwrap()).unwrap();
    let mut pixels:Vec<[u32;2]>=old["runs"][0]["rows"].as_array().unwrap().iter().map(|r|[u32::try_from(r["pixel"][0].as_u64().unwrap()).unwrap(),u32::try_from(r["pixel"][1].as_u64().unwrap()).unwrap()]).collect();
    for pixel in [[320,180],[960,180],[1600,180],[320,540],[960,540],[1600,540],[320,900],[960,900],[1600,900],[959,961],[960,961]]{if !pixels.contains(&pixel){pixels.push(pixel);}}
    let record_count=if selected{pixels.len() as u32}else{WIDTH*HEIGHT};
    let sample_indices=if selected{(0..record_count).collect()}else{pixels.iter().map(|p|p[1]*WIDTH+p[0]).collect()};
    let terminal_bytes=16+48*u64::from(record_count);let output_bytes=terminal_bytes+32*u64::from(record_count);
    shader.push_str(HEADER);
    let declaration=pixels.iter().map(|p|format!("vec2({}u,{}u)",p[0],p[1])).collect::<Vec<_>>().join(",");
    shader.push_str(&format!("\nconst NC_SCENE_PIXELS:array<vec2<u32>,{}>=array<vec2<u32>,{}>({declaration});\n",pixels.len(),pixels.len()));
    let mut entry=ENTRY.replace("RECORD_COUNT",&format!("{record_count}u"));
    entry=entry.replace("PHYSICAL_PIXEL",if selected{"NC_SCENE_PIXELS[id.x]"}else{"vec2(id.x%settings.control.x,id.x/settings.control.x)"});
    entry=entry.replace("TRACE_BODY",if reference{REFERENCE_TRACE}else{CANDIDATE_TRACE});shader.push_str(&entry);
    Fixture{shader,primary_uniform,field_uniform:clear.uniform,
        geometry:std::fs::read(if reference{"artifacts/optical-requested-stellar-sparse-chain-geometry.bin".to_string()}else{format!("{PACKET}-geometry.bin")}).unwrap(),
        frames:std::fs::read(format!("{PACKET}-frames.bin")).unwrap(),certificates:std::fs::read(format!("{PACKET}-certificates.bin")).unwrap(),
        width:WIDTH,height:HEIGHT,record_count,dispatch:[record_count.div_ceil(32),1,1],output_bytes,terminal_bytes,sample_pixels:pixels,sample_indices,control:CONTROL,
        cases:json!({"scope":"Unregistered full-scene canonical height/path diagnostic. Actual retained terrain,81 markers and h0 normal-line coverage; strict uncertain contacts remain unresolved. Current fixed eye begins strictly inside a source interval; no vacuum-entry/exact-node matrix acceptance. Per-eye face certificate preparation and upload are diagnostic cost, not free native work.",
            "reference":reference,"selected":selected,"packet":packet,"clear_fixture":clear.cases,"selection_source":selected_source,
            "primary_settings_source":"artifacts/optical-requested-stellar-sparse-chain-uniform.bin","camera_source":"artifacts/optical-requested-stellar-sparse-chain-view.bin",
            "record_layout":["16-byte current control header","48-byte production terminal records: p/path;u/status;normal/material-owner","32-byte appended diagnostic per record: attempts/roots/reason/scene visits (reference counters zero); exact GPU launch.xyz/0"],
            "coverage":"Canonical interval normal-line triangle fallback; reviewed coverage cores intentionally not enabled until their frame discrepancy is established.",
            "contact_policy":"Strict interval plane bracket/transversality/interior, original numerical width and complete state uncertainty checked within current tolerance; ambiguous candidates retry then remain unresolved. No same-owner or proximity acceptance."})}
}
const HEADER:&str=r#"
struct Settings{
    q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,
    right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,lens:vec4<f32>,
    budget:vec4<f32>,scene:vec4<f32>,control:vec4<u32>,layers:array<vec4<f32>,128>,
};
@group(0) @binding(0) var<uniform> settings:Settings;
@group(0) @binding(3) var<storage,read_write> terminals:array<vec4<u32>>;
"#;
const ENTRY:&str=r#"
@compute @workgroup_size(32)
fn normal_scene(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=RECORD_COUNT{return;}if id.x==0u{terminals[0]=settings.control;}
    let index=1u+id.x*3u;let detail_index=1u+3u*RECORD_COUNT+id.x*2u;
    let pixel=PHYSICAL_PIXEL;let uv=(vec2<f32>(pixel)+vec2(0.5))/vec2<f32>(settings.control.xy);
    let xy=vec2((2.0*uv.x-1.0)*settings.lens.x,1.0-2.0*uv.y);
    var optical=normalize(vec3(xy*tan(settings.lens.y),1.0));
    if settings.lens.z>0.5{let radius=length(xy);if radius>1.0{terminals[index]=vec4(0u);terminals[index+1u]=vec4(0u,0u,0u,3u);terminals[index+2u]=vec4(0u);terminals[detail_index]=vec4(0u);terminals[detail_index+1u]=vec4(0u);return;}let theta=radius*settings.lens.y;let scale=select(settings.lens.y,sin(theta)/max(radius,1e-8),radius>1e-8);optical=vec3(xy*scale,cos(theta));}
    let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);
    terminals[detail_index+1u]=bitcast<vec4<u32>>(vec4(direction,0.0));
    TRACE_BODY
}
"#;
const CANDIDATE_TRACE:&str=r#"
    let ray=nc_trace_scene(direction);
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(nc_scene_normal),(nc_scene_material<<24u)|nc_scene_owner);
    terminals[detail_index]=vec4(ray.attempts,ray.roots,ray.reason,nc_scene_visits);
"#;
const REFERENCE_TRACE:&str=r#"
    let ray=trace(direction);
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);
    terminals[detail_index]=vec4(0u);
"#;
