//! Original MIT. One-time canonical scene certificate preparation.
//! Does not register a renderer or change the retained source triangles.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
#[allow(dead_code)]
#[path = "optical_taylor.rs"]
mod arithmetic;
#[allow(dead_code)]
#[path = "optical_normal_map_packet.rs"]
mod map_packet;
#[allow(dead_code)]
#[path = "optical_normal_ideal_frame.rs"]
mod ideal_frame;
#[path = "optical_scene_ceiling.rs"]
mod ceiling;
use arithmetic::Interval as I;
use earth_math::{Vec3d, ecef_to_geodetic, atmosphere::gpu::scene::PackedScene,
    rendering::{LocalFrame, survey::marker_sign_frame}};
use serde_json::{Value, json};
use std::fs;

const PREFIX:&str="artifacts/optical-normal-scene-packet";
const BOX:[[usize;3];12]=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],
    [1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];
fn xyz(p:Vec3d)->[f64;3]{[p.x,p.y,p.z]}
fn axes(f:LocalFrame)->[Vec3d;3]{[f.basis.east,f.basis.up,-f.basis.north]}
fn dot(a:[I;3],b:Vec3d)->I{a.into_iter().zip(xyz(b)).fold(I::point(0.),|s,(a,b)|s+a*I::point(b))}
fn difference(a:Vec3d,b:Vec3d)->[I;3]{std::array::from_fn(|k|I::point(xyz(a)[k])-I::point(xyz(b)[k]))}
fn projected_difference(a:Vec3d,b:Vec3d,axis:[Vec3d;3])->[I;3]{axis.map(|v|dot(difference(a,b),v))}
fn pack(i:I)->[f32;2]{
    assert!(i.valid());
    let mut a=i.lo as f32;let mut b=i.hi as f32;
    if f64::from(a)>i.lo{a=a.next_down();}if f64::from(b)<i.hi{b=b.next_up();}
    if a!=0.&&a.abs()<f32::MIN_POSITIVE{a=if a<0.{-f32::MIN_POSITIVE}else{0.};}
    if b!=0.&&b.abs()<f32::MIN_POSITIVE{b=if b>0.{f32::MIN_POSITIVE}else{0.};}
    if i.lo<0.&&a==0.{a=-f32::MIN_POSITIVE;}if i.hi>0.&&b==0.{b=f32::MIN_POSITIVE;}
    assert!(f64::from(a)<=i.lo&&f64::from(b)>=i.hi);[a,b]
}
fn append_vector(out:&mut Vec<[f32;4]>,v:[I;3]){
    let p=v.map(pack);out.push([p[0][0],p[1][0],p[2][0],0.]);out.push([p[0][1],p[1][1],p[2][1],0.]);
}
fn intervals(v:ideal_frame::VectorBounds)->[I;3]{v.map(|x|I{lo:x[0],hi:x[1]})}
fn vector_bounds(v:[I;3])->ideal_frame::VectorBounds{v.map(|x|[x.lo,x.hi])}
fn append_face(out:&mut Vec<[f32;4]>,vertices:[Vec3d;3],eye:ideal_frame::IdealFrame)->u32{
    let index=u32::try_from(out.len()).unwrap();let [a,b,c]=vertices;
    append_vector(out,intervals(eye.eye_minus_point(xyz(a))));
    append_vector(out,intervals(eye.project(vector_bounds(difference(b,a)))));
    append_vector(out,intervals(eye.project(vector_bounds(difference(c,a)))));index
}
fn bytes(v:&[[f32;4]])->Vec<u8>{v.iter().flatten().flat_map(|x|x.to_le_bytes()).collect()}
fn v3(v:&Value)->Vec3d{Vec3d::new(v[0].as_f64().unwrap(),v[1].as_f64().unwrap(),v[2].as_f64().unwrap())}
fn main(){
    for suffix in [".json","-geometry.bin","-frames.bin","-certificates.bin"]{assert!(!std::path::Path::new(&format!("{PREFIX}{suffix}")).exists(),"Preserve scene preparation evidence");}
    let data:Value=serde_json::from_slice(&fs::read("artifacts/optical-normal-coordinate-current.json").unwrap()).unwrap();
    let input=&data["cases"].as_array().unwrap().iter().find(|c|c["maximum_step_m"].as_f64()==Some(100000.)).unwrap()["inputs"];
    let origin=v3(&input["origin_ecef_m"]);let geodetic=ecef_to_geodetic(origin).unwrap();
    let eye=ideal_frame::IdealFrame::from_eye(geodetic).unwrap();
    let case=source::actual_scene();
    assert_eq!(xyz(case.observer.frame().origin),xyz(origin),"Canonical observer origin changed");
    let mut packed=PackedScene::prepare(&case.scene).unwrap();
    let frames=packed.frame_data(LocalFrame::at(case.observer),&case.markers).unwrap();
    let geometry_original=bytes(&packed.geometry);let frames_bytes=bytes(&frames);
    assert_eq!(geometry_original,fs::read("artifacts/optical-requested-stellar-sparse-chain-geometry.bin").unwrap(),"Retained source geometry changed");
    assert_eq!(frames_bytes,fs::read("artifacts/optical-requested-stellar-sparse-chain-frames.bin").unwrap(),"Retained source/marker frames changed");
    let nodes=input["layers"].as_array().unwrap();let base=input["profile_base_height_m"].as_f64().unwrap();
    let (axis_low,axis_high)=map_packet::axis_intervals(geodetic).unwrap();
    let e2=map_packet::eccentricity_interval().unwrap();let eye_residual=map_packet::split_residual(geodetic.height_m).unwrap();
    // Header0 count/offset words; header1 marker/source/version words. Header2/3
    // canonical axis and e² endpoints; header4 eye-height residual and ceiling.
    let mut cert=vec![[0.;4];5];let source_start=cert.len();
    for node in nodes{let p=map_packet::split_residual(base+node["height_m"].as_f64().unwrap()).unwrap();cert.push([p[0],p[1],0.,0.]);}
    let patch_start=cert.len();cert.resize(patch_start+9*packed.patches.len(),[0.;4]);
    let marker_start=cert.len();cert.resize(marker_start+3*case.markers.len(),[0.;4]);
    let face_start=cert.len();let mut patch_records=Vec::new();let mut global=ceiling::HeightCeiling::default();
    for (id,(patch,record)) in case.scene.patches().iter().zip(&packed.patches).enumerate(){
        let mesh=patch.optical_mesh();let frame=record.frame;let frame_axes=axes(frame);
        let mut error=0_f64;
        for &vertex in mesh.vertices{
            global.include(vertex).unwrap();let expected=projected_difference(vertex,frame.origin,frame_axes);let stored=frame.position(vertex);
            for k in 0..3{error=error.max((expected[k]-I::point(f64::from(stored[k]))).abs_upper());}
        }
        let error=(error as f32).next_up();let mut metadata=Vec::new();
        metadata.push([error,f32::from_bits(record.root),f32::from_bits(packed.geometry[record.root as usize][3].to_bits()),0.]);
        let shift=std::array::from_fn(|k|intervals(eye.origin_ecef)[k]-I::point(xyz(frame.origin)[k]));
        append_vector(&mut metadata,frame_axes.map(|v|dot(shift,v)));
        for row in frame_axes{append_vector(&mut metadata,eye.columns.map(|v|dot(intervals(v),row)));}
        assert_eq!(metadata.len(),9);cert[patch_start+9*id..patch_start+9*(id+1)].copy_from_slice(&metadata);
        let triangle_start=record.root as usize+mesh.nodes.len()*2;
        for (j,triangle) in mesh.triangles.iter().enumerate(){
            let vertices=triangle.map(|index|mesh.vertices[index as usize]);let address=triangle_start+3*j;
            for k in 0..3{assert_eq!(&packed.geometry[address+k][..3],&frame.position(vertices[k]));}
            let certificate=append_face(&mut cert,vertices,eye);packed.geometry[address][3]=f32::from_bits(certificate);
        }
        patch_records.push(json!({"patch":id,"root":record.root,"triangles":mesh.triangles.len(),"source_component_error_m":error,"metadata_vec4":patch_start+9*id}));
    }
    let mut marker_records=Vec::new();
    for (id,marker) in case.markers.iter().enumerate(){
        let sign=marker_sign_frame(marker,origin).unwrap();let w=marker.width_m;let h=w*9./25.;
        let a=sign.center-sign.right*(w*0.5)-sign.up*(h*0.5);let b=a+sign.right*w;let c=b+sign.up*h;let d=a+sign.up*h;
        let mut corners=marker.corners().to_vec();corners.extend([a,b,c,d]);
        let mut lower=[f64::INFINITY;3];let mut upper=[f64::NEG_INFINITY;3];
        for &p in &corners{global.include(p).unwrap();for (k,v) in intervals(eye.eye_minus_point(xyz(p))).map(|x|-x).into_iter().enumerate(){lower[k]=lower[k].min(v.lo);upper[k]=upper[k].max(v.hi);}}
        let address=cert.len();let mut faces=BOX.to_vec();faces.extend([[8,9,10],[8,10,11]]);
        for triangle in faces{append_face(&mut cert,triangle.map(|k|corners[k]),eye);}
        let bounds:Vec<_>=(0..3).map(|k|pack(I{lo:lower[k],hi:upper[k]})).collect();
        cert[marker_start+3*id]=[f32::from_bits(address as u32),f32::from_bits(id as u32),0.,0.];
        cert[marker_start+3*id+1]=[bounds[0][0],bounds[1][0],bounds[2][0],0.];
        cert[marker_start+3*id+2]=[bounds[0][1],bounds[1][1],bounds[2][1],0.];
        marker_records.push(json!({"marker":id,"canonical_id":marker.id,"face_certificates_vec4":address}));
    }
    let height_ceiling=global.packed_with_position_allowance(0.).unwrap();
    cert[0]=[packed.patches.len() as u32,case.markers.len() as u32,nodes.len() as u32,patch_start as u32].map(f32::from_bits);
    cert[1]=[marker_start as u32,source_start as u32,face_start as u32,1].map(f32::from_bits);
    cert[2]=[axis_low[0],axis_low[1],axis_low[2],e2[0]];cert[3]=[axis_high[0],axis_high[1],axis_high[2],e2[1]];
    cert[4]=[eye_residual[0],eye_residual[1],height_ceiling,0.];
    let certificate_bytes=bytes(&cert);assert!(certificate_bytes.len()<128*1024*1024);
    fs::write(format!("{PREFIX}-geometry.bin"),bytes(&packed.geometry)).unwrap();fs::write(format!("{PREFIX}-frames.bin"),frames_bytes).unwrap();fs::write(format!("{PREFIX}-certificates.bin"),certificate_bytes).unwrap();
    fs::write(format!("{PREFIX}.json"),serde_json::to_vec_pretty(&json!({
        "schema":"ancientearth.normal-scene-certificate.v1","scope":"Canonical source terrain and current canonical prism/sign faces. Existing packed BVH xyz/order unchanged; first triangle vertex w stores canonical face certificate offset. No solver acceptance.",
        "canonical_input":"artifacts/optical-normal-coordinate-current.json","retained_geometry":"artifacts/optical-requested-stellar-sparse-chain-geometry.bin","retained_frames":"artifacts/optical-requested-stellar-sparse-chain-frames.bin",
        "canonical_eye_ecef_m":xyz(origin),"canonical_eye_geodetic":[geodetic.lat_deg,geodetic.lon_deg,geodetic.height_m],"ideal_eye_origin_ecef_interval_m":eye.origin_ecef,"ideal_minus_supplied_eye_ecef_interval_m":eye.origin_residual(xyz(origin)),"ideal_frame_columns":eye.columns,"certificate_vec4_count":cert.len(),"triangles":packed.triangle_count,"markers":case.markers.len(),"canonical_height_ceiling_m":height_ceiling,
        "patches":patch_records,"marker_records":marker_records,
        "layout":{"header_vec4":5,"patch_stride_vec4":9,"marker_stride_vec4":3,"face_stride_vec4":6,"vectors":"lower xyz then upper xyz, w unused","face":"eye-minus-A, B-minus-A, C-minus-A in canonical eye E/U/-N"}
    })).unwrap()).unwrap();
}
