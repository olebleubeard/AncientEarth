//! CPU diagnostic writer for immutable terrain faces and per-eye patch frames.
//! Root orchestration owns source hashes/watchdog. This owns all geometry.
#[allow(dead_code)]
#[path="optical_gpu_probe.rs"]mod source;
#[allow(dead_code)]
#[path="optical_patch_certificates.rs"]mod certificates;
use certificates::{PatchBasis,IdealFrame,FACE_VEC4,EYE_VEC4};
use earth_math::{Geodetic,Vec3d,ecef_to_geodetic,atmosphere::gpu::scene::PackedScene,rendering::LocalFrame};
use serde_json::json;
use std::{fs,path::Path,time::Instant};
fn xyz(v:Vec3d)->[f64;3]{[v.x,v.y,v.z]}
fn bytes(v:&[[f32;4]])->Vec<u8>{v.iter().flatten().flat_map(|x|x.to_le_bytes()).collect()}
fn main(){
    let prefix=std::env::args().nth(1).expect("new artifact prefix");
    for suffix in [".json","-faces.bin","-geometry.bin","-eye-0.bin","-eye-1.bin","-eye-2.bin"]{
        assert!(!Path::new(&format!("{prefix}{suffix}")).exists(),"Preserve prior packet evidence");
    }
    let begin=Instant::now();let case=source::actual_scene();
    let mut packed=PackedScene::prepare(&case.scene).unwrap();
    let original=bytes(&packed.geometry);
    assert_eq!(original,fs::read("artifacts/optical-requested-stellar-sparse-chain-geometry.bin").unwrap(),"Canonical scene/BVH identity changed");
    let frames=packed.frame_data(LocalFrame::at(case.observer),&case.markers).unwrap();
    assert_eq!(bytes(&frames),fs::read("artifacts/optical-requested-stellar-sparse-chain-frames.bin").unwrap(),"Original source/observer/marker frames changed");
    let source_path="artifacts/optical-positive-contact-packet-first-faces.bin";
    let source_faces=fs::read(source_path).unwrap();
    assert_eq!(source_faces.len()%72,0);assert!(source_faces.len()/72>=packed.triangle_count);
    let mut faces=Vec::with_capacity(FACE_VEC4*packed.triangle_count);
    let mut bases=Vec::new();let mut patches=Vec::new();let mut checked=0usize;
    for (id,(patch,record)) in case.scene.patches().iter().zip(&packed.patches).enumerate(){
        let basis=PatchBasis::from_frame(record.frame).unwrap();let mesh=patch.optical_mesh();
        let start=faces.len();let triangle_start=record.root as usize+2*mesh.nodes.len();
        for (j,indices) in mesh.triangles.iter().enumerate(){
            let vertices=indices.map(|i|mesh.vertices[i as usize]);
            let exact:Vec<u8>=vertices.map(xyz).into_iter().flatten().flat_map(f64::to_le_bytes).collect();
            assert_eq!(exact,&source_faces[checked*72..(checked+1)*72],"Retained canonical face identity changed");
            let address=triangle_start+3*j;
            for k in 0..3{assert_eq!(&packed.geometry[address+k][..3],&record.frame.position(vertices[k]));}
            let certificate=basis.face(vertices.map(xyz)).unwrap();
            let offset=u32::try_from(faces.len()).unwrap();
            faces.extend(certificate.words);packed.geometry[address][3]=f32::from_bits(offset);
            checked+=1;
        }
        patches.push(json!({"patch":id,"root":record.root,"first_face_vec4":start,"faces":mesh.triangles.len(),
            "origin_bits":basis.origin().map(f64::to_bits),"rows_bits":basis.rows().map(|v|v.map(f64::to_bits)),
            "positive_determinant_interval":basis.determinant()}));bases.push(basis);
    }
    assert_eq!(checked,packed.triangle_count);assert_eq!(faces.len(),FACE_VEC4*checked);
    // Prove every original node/triangle word is unchanged except the first
    // vertex's existing certificate-address slot. XYZ/order never change.
    let modified=bytes(&packed.geometry);let mut altered=0usize;
    for (i,(a,b)) in original.chunks_exact(4).zip(modified.chunks_exact(4)).enumerate(){
        if a!=b{assert_eq!(i%4,3);altered+=1;}
    }
    assert!(altered<=checked);
    fs::write(format!("{prefix}-faces.bin"),bytes(&faces)).unwrap();
    fs::write(format!("{prefix}-geometry.bin"),modified).unwrap();
    let static_seconds=begin.elapsed().as_secs_f64();
    let base=ecef_to_geodetic(case.observer.frame().origin).unwrap();
    let eyes=[base,Geodetic::new(base.lat_deg,base.lon_deg,base.height_m+1000.).unwrap(),
        Geodetic::new(base.lat_deg+0.01,base.lon_deg-0.02,base.height_m+100.).unwrap()];
    let mut eye_records=Vec::new();
    for (id,eye) in eyes.into_iter().enumerate(){
        let clock=Instant::now();let ideal=IdealFrame::from_eye(eye).unwrap();
        let mut words=Vec::with_capacity(EYE_VEC4*bases.len());
        for basis in &bases{words.extend(basis.eye(ideal).unwrap().words);}
        assert_eq!(words.len(),EYE_VEC4*bases.len());let elapsed=clock.elapsed().as_secs_f64();
        fs::write(format!("{prefix}-eye-{id}.bin"),bytes(&words)).unwrap();
        let geodetic_bits=[eye.lat_deg,eye.lon_deg,eye.height_m].map(f64::to_bits);
        eye_records.push(json!({"eye":id,"geodetic_bits":geodetic_bits,
            "ideal_origin_interval":ideal.origin_ecef,"ideal_columns_interval":ideal.columns,
            "metadata_bytes":words.len()*16,"single_pass_update_seconds":elapsed}));
    }
    fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({
        "schema":"ancientearth.immutable-patch-certificates.v1",
        "scope":"Unregistered source-backed terrain certificate preparation and three eye metadata samples. Source/BVH xyz/order unchanged. Faces are immutable across eye changes. This is not a GPU predicate, runtime upload, complete ray or FPS result. Billboard signs remain eye-dependent and are intentionally outside this static terrain packet.",
        "source_faces":source_path,"retained_geometry":"artifacts/optical-requested-stellar-sparse-chain-geometry.bin",
        "retained_frames":"artifacts/optical-requested-stellar-sparse-chain-frames.bin",
        "faces":checked,"patch_count":bases.len(),"source_faces_bit_exact":true,
        "static_face_bytes":faces.len()*16,"face_stride_vec4":FACE_VEC4,"eye_patch_stride_vec4":EYE_VEC4,
        "geometry_w_address_slots_changed":altered,"static_preparation_seconds":static_seconds,
        "face_layout":"0 negative sourceA/patch-origin high xyz;1..2 low residual interval;3..4 B-A interval;5..6 C-A interval. All use exact stored fixed patch rows.",
        "eye_layout":"0 eye-minus-patch-origin high xyz;1..2 low residual interval;3..8 three eye-to-patch interval matrix rows.",
        "consumer_contract":"Fold eyeHigh+negativeAHigh before adding either residual. Transform canonical eye curve by dynamic rows; no orthogonality assumption. Transform a patch plane normal back by the matrix transpose before normalization. Every outward packing/map/evaluation uncertainty remains charged to the existing complete contact budget; a split position is not a precision waiver.",
        "patches":patches,"eyes":eye_records
    })).unwrap()).unwrap();
}
