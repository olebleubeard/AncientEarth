import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions, mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
};
struct StarSettings {
    phase_sin:vec4<f32>,phase_cos:vec4<f32>,up:vec4<f32>,visibility:vec4<f32>,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> stars:StarSettings;
struct StarVertex {
    @builtin(instance_index) instance_index:u32,
    @location(0) position:vec3<f32>, @location(1) normal:vec3<f32>,
    @location(2) uv:vec2<f32>, @location(3) uv_b:vec2<f32>,
    @location(5) color:vec4<f32>,
};
@vertex
fn vertex(in:StarVertex)->VertexOutput {
    let matrix=mesh_functions::get_world_from_local(in.instance_index);
    var out:VertexOutput;
    out.world_position=mesh_functions::mesh_position_local_to_world(matrix,vec4(in.normal*2e12,1.0));
    out.world_normal=mesh_functions::mesh_normal_local_to_world(in.normal,in.instance_index);
    out.position=position_world_to_clip(out.world_position.xyz);
    let size=view.viewport.zw;
    let centre=(out.position.xy/out.position.w*vec2(0.5,-0.5)+0.5)*size;
    let corner=floor(centre)+vec2(in.uv.x,1.0-in.uv.y);
    out.position=vec4((corner/size*2.0-1.0)*vec2(1.0,-1.0)*out.position.w,out.position.zw);
    out.uv=in.uv;out.uv_b=in.uv_b;out.color=in.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index=in.instance_index;
    return out;
}
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing) front:bool)->FragmentOutput {
    let input=pbr_input_from_standard_material(in,front);
    let scintillation=star_twinkle(u32(in.uv_b.x+0.5),stars.phase_sin,stars.phase_cos,
        stars.up.w,dot(normalize(in.world_normal),stars.up.xyz),stars.visibility.y);
    var out:FragmentOutput;
    out.color=vec4(input.material.base_color.rgb*scintillation,
        stars.visibility.x);
    out.color=main_pass_post_lighting_processing(input,out.color);
    return out;
}
