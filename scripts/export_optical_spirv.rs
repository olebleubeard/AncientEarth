//! Original MIT. Offline Naga export for register/scratch investigation.
//! These selected wgpu Vulkan policy options are recorded explicitly; this
//! does not claim to capture the driver's native shader binary.
use std::path::Path;
fn main() {
    let args:Vec<_>=std::env::args().collect();
    let input=Path::new(&args[1]);let output=Path::new(&args[2]);
    let text=std::fs::read_to_string(input).unwrap();
    let module=naga::front::wgsl::parse_str(&text).unwrap();
    let info=naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all()).validate(&module).unwrap();
    let mut options=naga::back::spv::Options::default();
    options.lang_version=(1,6);
    options.flags=naga::back::spv::WriterFlags::DEBUG|naga::back::spv::WriterFlags::LABEL_VARYINGS|naga::back::spv::WriterFlags::FORCE_POINT_SIZE;
    options.fake_missing_bindings=false;
    options.bounds_check_policies.index=naga::proc::BoundsCheckPolicy::Restrict;
    options.bounds_check_policies.buffer=if args.iter().any(|x|x=="--restrict-buffer"){naga::proc::BoundsCheckPolicy::Restrict}else{naga::proc::BoundsCheckPolicy::Unchecked};
    options.bounds_check_policies.binding_array=naga::proc::BoundsCheckPolicy::Unchecked;
    options.bounds_check_policies.image_load=naga::proc::BoundsCheckPolicy::Unchecked;
    for (_,v) in module.global_variables.iter(){if let Some(b)=v.binding{options.binding_map.insert(b,naga::back::spv::BindingInfo{descriptor_set:b.group,binding:b.binding,binding_array_size:None});}}
    let entry=args.iter().position(|x|x=="--entry").map(|i|args.get(i+1).expect("--entry requires a compute entry point").as_str()).unwrap_or("performance");
    let pipeline=naga::back::spv::PipelineOptions{shader_stage:naga::ShaderStage::Compute,entry_point:entry.into()};
    let words=naga::back::spv::write_vec(&module,&info,&options,Some(&pipeline)).unwrap();
    let bytes:Vec<_>=words.iter().flat_map(|x|x.to_le_bytes()).collect();std::fs::write(output,bytes).unwrap();
    println!("{} -> {}: {} words, SPIRV1.6, index restrict, buffer {}, loop bounding enabled; no workgroup variables/images/ray queries in input",input.display(),output.display(),words.len(),if args.iter().any(|x|x=="--restrict-buffer"){"restrict"}else{"unchecked (robustBufferAccess2 hardware policy)"});
}
