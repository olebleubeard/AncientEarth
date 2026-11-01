// NASA source-backed lunar color and non-displacing LOLA shading normals.
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    mesh_view_bindings::view,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> moon: LunarSettings;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var lunar_color:texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var lunar_color_sampler:sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var lunar_normal:texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var lunar_normal_sampler:sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    let input = pbr_input_from_standard_material(in, front);
    let n=normalize(lunar_body(in.world_normal,moon));
    let eye=normalize(lunar_body(view.world_position-in.world_position.xyz,moon));
    let sun=normalize(lunar_body(moon.illumination.xyz,moon));
    let footprint=max(length(dpdx(n)),length(dpdy(n)));
    var out: FragmentOutput;
    out.color = vec4(lunar_radiance(n,eye,sun,footprint),1.0);
    out.color = main_pass_post_lighting_processing(input,out.color);
    return out;
}
