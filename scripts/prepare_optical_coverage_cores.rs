//! Original MIT. Optional immutable coverage-core metadata for the retained
//! actual scene. No renderer registration or modification of older packets.
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
#[allow(dead_code)]
#[path = "optical_canonical_coverage_packet.rs"]
mod core;
#[allow(dead_code)]
#[path = "optical_normal_ideal_frame.rs"]
mod ideal_frame;
#[allow(dead_code)]
#[path = "optical_taylor.rs"]
mod arithmetic;
use arithmetic::Interval as I;
use earth_math::{Vec3d, ecef_to_geodetic, atmosphere::gpu::scene::PackedScene};
use serde_json::json;
use std::fs;
fn xyz(v:Vec3d)->[f64;3]{[v.x,v.y,v.z]}
fn interval(v:[f64;2])->I{I{lo:v[0],hi:v[1]}}
fn dot(a:[I;3],b:Vec3d)->I{a.into_iter().zip(xyz(b)).fold(I::point(0.),|s,(a,b)|s+a*I::point(b))}
fn pack(i:I)->[f32;2]{
    assert!(i.valid());let mut a=i.lo as f32;let mut b=i.hi as f32;
    if f64::from(a)>i.lo{a=a.next_down();}if f64::from(b)<i.hi{b=b.next_up();}
    if a!=0.&&a.abs()<f32::MIN_POSITIVE{a=if a<0.{-f32::MIN_POSITIVE}else{0.};}
    if b!=0.&&b.abs()<f32::MIN_POSITIVE{b=if b>0.{f32::MIN_POSITIVE}else{0.};}
    if i.lo<0.&&a==0.{a=-f32::MIN_POSITIVE;}if i.hi>0.&&b==0.{b=f32::MIN_POSITIVE;}
    assert!(f64::from(a)<=i.lo&&f64::from(b)>=i.hi);[a,b]
}
fn vector(out:&mut Vec<u32>,v:[I;3]){
    let p=v.map(pack);out.extend([p[0][0],p[1][0],p[2][0],0.,p[0][1],p[1][1],p[2][1],0.].map(f32::to_bits));
}
fn main(){
    let prefix=std::env::args().nth(1).expect("Specify a fresh output prefix");
    for suffix in [".json","-cores.bin"]{
        assert!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists(),"Preserve prior evidence");
    }
    let scene=source::actual_scene();
    let packed=PackedScene::prepare(&scene.scene).unwrap();
    let original:Vec<u8>=packed.geometry.iter().flatten().flat_map(|v|v.to_le_bytes()).collect();
    assert_eq!(original,fs::read("artifacts/optical-requested-stellar-sparse-chain-geometry.bin").unwrap(),"Canonical scene/frame order must match retained packet");
    let retained:serde_json::Value=serde_json::from_slice(&fs::read("artifacts/optical-normal-scene-packet.json").unwrap()).unwrap();
    let certificate_bytes=fs::read("artifacts/optical-normal-scene-packet-certificates.bin").unwrap();
    assert!(certificate_bytes.len().is_multiple_of(16));
    let certificate:Vec<u32>=certificate_bytes.chunks_exact(4).map(|w|u32::from_le_bytes(w.try_into().unwrap())).collect();
    assert_eq!(certificate[0] as usize,packed.patches.len());
    assert_eq!(retained["patches"].as_array().unwrap().len(),packed.patches.len());
    let ideal_eye=ideal_frame::IdealFrame::from_eye(ecef_to_geodetic(scene.observer.frame().origin).unwrap()).unwrap();
    let patch_start=certificate[3] as usize;
    let mut words=Vec::new();let mut rows=Vec::new();
    for (index,(patch,stored)) in scene.scene.patches().iter().zip(&packed.patches).enumerate(){
        let mesh=patch.optical_mesh();let frame=stored.frame;
        let axes=[frame.basis.east,frame.basis.up,-frame.basis.north];
        let base=4*(patch_start+9*index);let prior=&retained["patches"][index];
        assert_eq!(prior["patch"].as_u64().unwrap(),index as u64);
        assert_eq!(prior["triangles"].as_u64().unwrap(),mesh.triangles.len() as u64);
        assert_eq!(prior["root"].as_u64().unwrap(),u64::from(stored.root));
        assert_eq!(certificate[base+1],stored.root);
        assert_eq!(certificate[base+2],packed.geometry[stored.root as usize][3].to_bits());
        let shift=std::array::from_fn(|k|interval(ideal_eye.origin_ecef[k])-I::point(xyz(frame.origin)[k]));
        let mut transform=Vec::new();vector(&mut transform,axes.map(|row|dot(shift,row)));
        for row in axes{vector(&mut transform,ideal_eye.columns.map(|column|dot(column.map(interval),row)));}
        assert_eq!(transform.len(),32);
        assert_eq!(&transform, &certificate[base+4..base+36],"Canonical eye-to-patch transform identity changed at patch {index}");
        let result=core::prepare(mesh.vertices,&mesh.triangles,frame.origin,axes);
        let (data,proof)=match result{
            Some(value)=>(value.words,json!({"degree":value.degree,"determinant_interval":value.determinant,
                "canonical_vertex_error_m":value.canonical_vertex_error_m,"nominal_center":value.nominal_center,
                "nominal_half_extent_xz":value.nominal_half_extent,"nominal_boundary_radius":value.nominal_boundary_radius,
                "nominal_vertical_bounds_y":value.nominal_vertical_bounds})),
            None=>([[0.;4];3],serde_json::Value::Null),
        };
        let bits=data.map(|v|v.map(f32::to_bits));words.extend(bits.into_iter().flatten());
        rows.push(json!({"patch":index,"root":stored.root,"triangles":mesh.triangles.len(),
            "vertices":mesh.vertices.len(),"source_frame_origin":xyz(frame.origin),
            "source_frame_rows_x_up_z":axes.map(xyz),"words":bits,"proof":proof}));
    }
    assert_eq!(words.len(),12*packed.patches.len());
    fs::write(format!("{prefix}-cores.bin"),words.iter().flat_map(|w|w.to_le_bytes()).collect::<Vec<_>>()).unwrap();
    fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({
        "schema":"ancientearth.canonical-coverage-cores.v1","patch_count":packed.patches.len(),
        "record_words":12,"records":rows,
        "retained_scene_geometry":"artifacts/optical-requested-stellar-sparse-chain-geometry.bin",
        "retained_scene_certificates":"artifacts/optical-normal-scene-packet-certificates.bin",
        "all_canonical_eye_to_patch_transform_words_exact":true,
        "layout":["center XYZ, boundary radius","half XZ, lowerY, upperY","canonical vertex error, u32 valid,0,0"],
        "scope":"Unused sufficient-only core metadata. Exact stored patch matrix is interval-proved invertible; temporary rounded canonical vertex projections are enclosed by vertex error. Query point/direction bounds must use the identical canonical eye-to-patch interval transform. False/invalid always defers to canonical triangles. Existing source geometry, masks and older packets remain unchanged. No GPU execution/contact/throughput acceptance."
    })).unwrap()).unwrap();
}
