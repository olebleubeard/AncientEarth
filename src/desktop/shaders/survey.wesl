// Original survey paint. Physical prism geometry is never displaced or enlarged.
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::view,
};
struct SurveySettings {sky_fill:vec4<f32>,eclipse:EclipseSettings};
@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> survey:SurveySettings;

fn red_primitive(t:f32)->f32 {
    let periods=floor(t/2.0);
    return periods+min(t-2.0*periods,1.0);
}
fn red_fraction(t:f32,footprint:f32)->f32 {
    let width=max(footprint,0.0001);
    // The mean approximation differs from the 1D box average by <=1/width.
    if width>=1024.0{return 0.5;}
    let phase=t-2.0*floor(t/2.0);
    let lower=phase-width*0.5;let upper=phase+width*0.5;
    if lower>=floor(phase)&&upper<=floor(phase)+1.0{
        return select(0.0,1.0,phase<1.0);
    }
    return clamp((red_primitive(upper)-red_primitive(lower))/width,0.0,1.0);
}
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing)front:bool)->FragmentOutput{
    var input=pbr_input_from_standard_material(in,front);
    // UV.x comes directly from canonical height in international feet. The
    // derivative range is a local shading filter, not a measured pixel area.
    let height=in.uv.x;
    let red=red_fraction(height,abs(dpdx(height))+abs(dpdy(height)));
    input.material.base_color=vec4(mix(vec3(0.012,0.48,0.018),vec3(0.72,0.012,0.008),red),1.0);
    var out:FragmentOutput;
    out.color=apply_pbr_lighting(input);
    let visibility=eclipse_solar_visibility(in.world_position.xyz,survey.eclipse);
    out.color=vec4(out.color.rgb*visibility,out.color.a);
    let fill=input.material.base_color.rgb*survey.sky_fill.rgb
        *(0.35+0.65*max(input.N.y,0.0))*eclipse_sky_response(visibility);
    out.color=vec4(out.color.rgb+fill*view.exposure,out.color.a);
    out.color=main_pass_post_lighting_processing(input,out.color);
    return out;
}
