// Sky RGB is unchanged. Alpha records unobstructed sky for the final fisheye star pass.
import bevy_pbr::render::{
    forward_io::{VertexOutput,FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
};
struct SkySettings { mask:vec4<f32> };
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> sky:SkySettings;
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing) front:bool)->FragmentOutput {
    let input=pbr_input_from_standard_material(in,front);
    var out:FragmentOutput;
    out.color=main_pass_post_lighting_processing(input,input.material.base_color);
    out.color.a=select(1.0,0.0,sky.mask.x>0.5);
    return out;
}
