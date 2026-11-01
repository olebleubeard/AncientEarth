// Equidistant projection from one adaptive or five fixed perspective views.
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
struct Settings { parameters: vec4<f32> };
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings: Settings;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var front: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var front_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var right: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var right_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var back: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var back_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(7) var left: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(8) var left_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(9) var up: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(10) var up_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(11) var down: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(12) var down_sampler: sampler;
struct Sampling { parameters:vec4<f32> };
@group(constants::MATERIAL_BIND_GROUP) @binding(15) var<uniform> sampling:Sampling;
fn fisheye_ray(xy:vec2<f32>,radius:f32)->vec3<f32>{
    let theta=radius*settings.parameters.y;
    if sampling.parameters.y>0.5 {
        // |theta|<pi/4. Sinc/cos polynomials avoid the fixed absolute error
        // permitted for WGSL sin at tiny telescope angles. No division by r.
        let q=theta*theta;
        let sinc=1.0+q*(-0.16666666666666667+q*(0.008333333333333333+q*(-0.0001984126984126984+q*(0.0000027557319223985893+q*(-0.00000002505210838544172)))));
        let cosine=1.0+q*(-0.5+q*(0.041666666666666664+q*(-0.001388888888888889+q*(0.0000248015873015873+q*(-0.0000002755731922398589+q*0.00000000208767569878681)))));
        return vec3(xy*(settings.parameters.y*sinc),cosine);
    }
    let scale=select(settings.parameters.y,sin(theta)/max(radius,1e-8),radius>1e-8);
    return vec3(xy*scale,cos(theta));
}
fn background(in: VertexOutput) -> vec4<f32> {
    let xy = vec2((2.0 * in.uv.x - 1.0) * settings.parameters.x, 1.0 - 2.0 * in.uv.y);
    let radius = length(xy);
    if radius > 1.0 { return vec4(0.002, 0.004, 0.008, 1.0); }
    let ray=fisheye_ray(xy,radius);
    if sampling.parameters.y>0.5 {
        let uv=(vec2(ray.x,-ray.y)*sampling.parameters.x)/ray.z*0.5+0.5;
        return textureSampleLevel(front,front_sampler,uv,0.0);
    }
    let axis = abs(ray);
    if axis.z >= axis.x && axis.z >= axis.y {
        if ray.z >= 0.0 {
            let uv = vec2(ray.x, -ray.y) / ray.z * 0.5 + 0.5;
            return textureSampleLevel(front, front_sampler, uv, 0.0);
        }
        let uv = vec2(-ray.x, -ray.y) / -ray.z * 0.5 + 0.5;
        return textureSampleLevel(back, back_sampler, uv, 0.0);
    }
    if axis.x >= axis.y {
        if ray.x >= 0.0 {
            let uv = vec2(-ray.z, -ray.y) / ray.x * 0.5 + 0.5;
            return textureSampleLevel(right, right_sampler, uv, 0.0);
        }
        let uv = vec2(ray.z, -ray.y) / -ray.x * 0.5 + 0.5;
        return textureSampleLevel(left, left_sampler, uv, 0.0);
    }
    if ray.y >= 0.0 {
        let uv = vec2(ray.x, ray.z) / ray.y * 0.5 + 0.5;
        return textureSampleLevel(up, up_sampler, uv, 0.0);
    }
    let uv = vec2(ray.x, -ray.z) / -ray.y * 0.5 + 0.5;
    return textureSampleLevel(down, down_sampler, uv, 0.0);
}

struct FisheyeStars {
 phase_sin:vec4<f32>,phase_cos:vec4<f32>,vertical:vec4<f32>,
 right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,visibility:vec4<f32>,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(13) var<uniform> stars:FisheyeStars;
@group(constants::MATERIAL_BIND_GROUP) @binding(14) var<storage,read> star_data:array<vec4<f32>>;
@fragment
fn fragment(in:VertexOutput)->@location(0) vec4<f32> {
    let base=background(in);
    // Opaque Earth, vegetation and finite astronomical bodies mask the pixel.
    if base.a>0.01||stars.visibility.x<=0.0{return vec4(base.rgb,1.0);}
    let xy=vec2((2.0*in.uv.x-1.0)*settings.parameters.x,1.0-2.0*in.uv.y);
    let radius=length(xy);if radius>1.0{return vec4(base.rgb,1.0);}
    let camera=fisheye_ray(xy,radius);
    let direction=normalize(stars.right.xyz*camera.x+stars.up.xyz*camera.y+stars.forward.xyz*camera.z);
    var color=vec3<f32>(0.0);
    for(var face=0u;face<6u;face+=1u){
        let projected=star_face(direction,face);if projected.z<0.5{continue;}
        let coord=projected.xy/projected.z;if max(abs(coord.x),abs(coord.y))>1.06{continue;}
        let cell=clamp(vec2<i32>(floor((coord+1.0)*16.0)),vec2<i32>(0),vec2<i32>(31));
        for(var dy=-1;dy<=1;dy+=1){for(var dx=-1;dx<=1;dx+=1){
            let c=cell+vec2(dx,dy);if any(c<vec2<i32>(0))||any(c>vec2<i32>(31)){continue;}
            let bin=star_data[face*1024u+u32(c.y)*32u+u32(c.x)];
            for(var index=0u;index<u32(bin.y);index+=1u){
                let source=star_data[u32(bin.x)+2u*index];let star=normalize(source.xyz);
                let tint=star_data[u32(bin.x)+2u*index+1u];
                let p=star_pixel(vec3(dot(star,stars.right.xyz),dot(star,stars.up.xyz),dot(star,stars.forward.xyz)),
                    settings.parameters.zw,settings.parameters.y,true);
                if p.z==0.0||any(p.xy!=floor(in.position.xy)){continue;}
                color+=tint.rgb*(0.18*source.w)*star_twinkle(u32(tint.w+0.5),stars.phase_sin,stars.phase_cos,
                    stars.vertical.w,dot(star,stars.vertical.xyz),stars.visibility.y);
            }
        }}
    }
    return vec4(base.rgb+color*stars.visibility.x,1.0);
}
