// Shared real-map registration and uncalibrated single-scattering display law.
struct LunarSettings { body_x:vec4<f32>,body_y:vec4<f32>,body_z:vec4<f32>,illumination:vec4<f32> };
fn lunar_body(v:vec3<f32>,s:LunarSettings)->vec3<f32>{return vec3(dot(v,s.body_x.xyz),dot(v,s.body_y.xyz),dot(v,s.body_z.xyz));}
fn lunar_uv(n:vec3<f32>)->vec2<f32>{return vec2(atan2(n.y,n.x)/6.28318530718+0.5,0.5-asin(clamp(n.z,-1.0,1.0))/3.14159265359);}
fn lunar_lod(n:vec3<f32>,footprint:f32,dimensions:vec2<u32>)->f32{
    let density=max(f32(dimensions.x)/(6.28318530718*max(length(n.xy),0.00001)),f32(dimensions.y)/3.14159265359);
    return max(0.0,log2(max(footprint*density,1.0)));
}
fn lunar_radiance(n:vec3<f32>,eye:vec3<f32>,sun:vec3<f32>,footprint:f32)->vec3<f32>{
    let uv=lunar_uv(n);
    let color=textureSampleLevel(lunar_color,lunar_color_sampler,uv,lunar_lod(n,footprint,textureDimensions(lunar_color))).rgb;
    let tangent=textureSampleLevel(lunar_normal,lunar_normal_sampler,uv,lunar_lod(n,footprint,textureDimensions(lunar_normal))).xyz*2.0-1.0;
    var east=vec3(0.0,1.0,0.0);if length(n.xy)>0.000001 {east=normalize(vec3(-n.y,n.x,0.0));}
    let north=cross(n,east);let normal=normalize(east*tangent.x+north*tangent.y+n*tangent.z);
    let mu0=max(dot(normal,sun),0.0);let mu=max(dot(normal,eye),0.0);
    var lighting=0.0;
    // Mean sphere blocks light on its back hemisphere; mapped slopes shade only.
    if dot(n,sun)>0.0 && mu0>0.0 && mu>0.0 {lighting=2.0*mu0/(mu0+mu);}
    return color*lighting+vec3(0.006); // explicit artistic dark-side display floor
}
