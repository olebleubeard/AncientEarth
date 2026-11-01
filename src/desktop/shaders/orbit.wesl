// Original MIT. Simultaneous heliocentric analytic bodies; no terrain or refraction.
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
struct Body {eye:vec4<f32>,row0:vec4<f32>,row1:vec4<f32>,row2:vec4<f32>,sun:vec4<f32>};
struct Settings {right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,lens:vec4<f32>,bodies:array<Body,3>,classification:ClassificationSettings};
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings:Settings;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<storage,read> native_water:array<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<storage,read> global_land:array<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<storage,read> star_data:array<vec4<f32>>;
fn body_direction(b:Body,d:vec3<f32>)->vec3<f32>{return vec3(dot(b.row0.xyz,d),dot(b.row1.xyz,d),dot(b.row2.xyz,d));}
struct Hit {distance:f32,coverage:f32,point:vec3<f32>};
fn body_hit(b:Body,direction:vec3<f32>)->Hit{
    let flatten=vec3(1.0,1.0,1.0/b.row2.w);
    let o=b.eye.xyz*flatten;let raw=body_direction(b,direction);let d=raw*flatten;
    let speed=length(d);let u=d/speed;let cross_distance=cross(o,u);
    let disc=1.0-dot(cross_distance,cross_distance);
    // Pixel-footprint edge coverage is presentation AA, not a changed radius.
    let span=max(fwidth(disc),1e-10);
    let apparent_pixels=settings.lens.y/(2.0*tan(settings.lens.z))/sqrt(max(dot(b.eye.xyz,b.eye.xyz)-1.0,1e-12));
    let coverage=select(select(0.0,1.0,disc>=0.0),clamp(0.5+disc/span,0.0,1.0),apparent_pixels>=2.0);
    let root=sqrt(max(0.0,disc));let along=dot(o,u);
    let near=(-along-root)/speed;let far=(-along+root)/speed;
    let distance=select(near,far,near<0.0);
    return Hit(distance*b.eye.w,select(coverage,0.0,distance<0.0),b.eye.xyz+raw*distance);
}
fn star_background(direction:vec3<f32>,pixel:vec2<f32>)->vec3<f32>{
    var color=vec3(0.001,0.002,0.004);
    for(var face=0u;face<6u;face+=1u){
        let p=star_face(direction,face);if p.z<0.5{continue;}
        let coord=p.xy/p.z;if max(abs(coord.x),abs(coord.y))>1.06{continue;}
        let cell=clamp(vec2<i32>(floor((coord+1.0)*16.0)),vec2<i32>(0),vec2<i32>(31));
        for(var y=-1;y<=1;y+=1){for(var x=-1;x<=1;x+=1){
            let c=cell+vec2(x,y);if any(c<vec2<i32>(0))||any(c>vec2<i32>(31)){continue;}
            let bin=star_data[face*1024u+u32(c.y)*32u+u32(c.x)];
            for(var i=0u;i<u32(bin.y);i+=1u){
                let source=star_data[u32(bin.x)+2u*i];let star=normalize(source.xyz);
                let projected=star_pixel(vec3(dot(star,settings.right.xyz),dot(star,settings.up.xyz),dot(star,settings.forward.xyz)),settings.lens.xy,settings.lens.z,false);
                if projected.z>0.0&&all(projected.xy==pixel){color+=star_data[u32(bin.x)+2u*i+1u].rgb*(0.18*source.w);}
            }
        }}
    }
    return color;
}
@fragment
fn fragment(in:VertexOutput)->@location(0) vec4<f32>{
    if settings.lens.w<0.5{return vec4(0.001,0.002,0.004,1.0);}
    let xy=vec2((2.0*in.uv.x-1.0)*settings.lens.x/settings.lens.y,1.0-2.0*in.uv.y)*tan(settings.lens.z);
    let direction=normalize(settings.forward.xyz+settings.right.xyz*xy.x+settings.up.xyz*xy.y);
    // All derivatives are evaluated uniformly before selecting the nearest body.
    let earth=body_hit(settings.bodies[0],direction);
    let sun=body_hit(settings.bodies[1],direction);
    let moon=body_hit(settings.bodies[2],direction);
    let hits=array<Hit,3>(earth,sun,moon);
    var index=3u;var distance=1e30;
    for(var i=0u;i<3u;i+=1u){if hits[i].coverage>0.0&&hits[i].distance>0.0&&hits[i].distance<distance{index=i;distance=hits[i].distance;}}
    if index==3u{return vec4(star_background(direction,floor(in.position.xy)),1.0);}
    let hit=hits[index];let body=settings.bodies[index];
    let normal=normalize(hit.point*vec3(1.0,1.0,1.0/(body.row2.w*body.row2.w)));
    var color=vec3(1.0,0.83,0.48);
    if index!=1u{
        let sunlight=normalize(body.sun.xyz-hit.point);let diffuse=max(dot(normal,sunlight),0.0);
        if index==0u{
            let angles=vec2(asin(clamp(normal.z,-1.0,1.0)),atan2(normal.y,normal.x));
            let category=global_category(angles,settings.classification);
            let water=category>=1u&&category<=3u;
            let base=select(vec3(0.12,0.25,0.075),vec3(0.015,0.12,0.21),water);
            // Explicit illustrative diffuse fill; geography/radius are unchanged.
            color=base*(0.055+0.945*diffuse);
            if water {color+=vec3(0.15,0.21,0.25)*pow(max(dot(reflect(-sunlight,normal),-normalize(body_direction(body,direction))),0.0),80.0)*diffuse;}
        }else{color=vec3(0.48,0.47,0.44)*(0.012+0.988*diffuse);}
    }
    if hit.coverage<1.0{color=mix(star_background(direction,floor(in.position.xy)),color,hit.coverage);}
    return vec4(color,1.0);
}
