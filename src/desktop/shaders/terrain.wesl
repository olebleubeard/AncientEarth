// Fixed surface geometry; material labels use regional, native and global sources.
import bevy_pbr::render::{forward_io::{VertexOutput,FragmentOutput},pbr_fragment::pbr_input_from_standard_material,
 pbr_functions::{apply_pbr_lighting,main_pass_post_lighting_processing},mesh_view_bindings::view};
struct GroundSettings {origin:vec4<f32>,east:vec4<f32>,up:vec4<f32>,south:vec4<f32>,sky_fill:vec4<f32>,eclipse:EclipseSettings};
struct WaterSettings {coordinates:GroundSettings,phase:vec4<f32>,environment:vec4<f32>};
struct TerrainSettings {water:WaterSettings,q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,geometry_origin:vec4<f32>,latitude:vec4<f32>,region0:array<vec4<f32>,4>,region1:array<vec4<f32>,4>,classification:ClassificationSettings};
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> terrain:TerrainSettings;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage,read> boundaries:array<vec4<f32>>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<storage,read> native_water:array<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var<storage,read> global_land:array<u32>;
@fragment
fn fragment(in:VertexOutput,@builtin(front_facing) front:bool)->FragmentOutput{
 let basis=mat3x3(terrain.water.coordinates.east.xyz,terrain.water.coordinates.up.xyz,terrain.water.coordinates.south.xyz);
 let p=basis*in.world_position.xyz;
 let footprint=max(length(dpdx(p)),length(dpdy(p)));
 if water_at(geometry(in.world_position.xyz).xyz){return shade_water(in,front,footprint);}
 return shade_ground(in,front,footprint);
}
fn qmul(p:vec3<f32>)->vec3<f32>{return vec3(dot(terrain.q0.xyz,p),dot(terrain.q1.xyz,p),dot(terrain.q2.xyz,p));}
// Height relative to the true geodetic parallel surfaces, not inflated axes.
fn geometry(p:vec3<f32>)->vec4<f32>{
    let mp=qmul(p);let qp=terrain.geometry_origin.xyz+mp;
    let f=terrain.geometry_origin.w+dot(p,2.0*terrain.geometry_origin.xyz+mp);
    if length(qp)<0.75 {return vec4(0.0,1.0,0.0,-1000000.0);}
    var normal=normalize(qp);var h=0.0;
    for(var i=0;i<5;i+=1){let qn=qmul(normal);let v=dot(normal,qp);
        h=f/(v+sqrt(max(0.0,v*v-f*dot(normal,qn))));normal=normalize(qp-h*qn);}
    return vec4(normal,h);
}
fn relative_angles(n:vec3<f32>)->vec2<f32>{
    let a=n.y*terrain.latitude.y+n.z*terrain.latitude.x;
    let h=length(vec2(a,n.x));var d=h-a;
    if a>=0.0 && h+a>0.0 {d=n.x*n.x/(h+a);}
    return vec2(atan2(-n.z-d*terrain.latitude.x,n.y+d*terrain.latitude.y),atan2(n.x,a));
}
fn cross2(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
fn water_at(n:vec3<f32>)->bool{
    if terrain.classification.control.x==0u {return false;}
    let angles=relative_angles(n);
    for(var region=0u;region<4u;region+=1u){
        let a=terrain.region0[region];let b=terrain.region1[region];if b.z<1.0 {continue;}
        let point=vec2((a.x-angles.x)*a.z,(a.y+angles.y)*a.w);
        if any(point<vec2(0.0)) || point.x>3.0*b.y || point.y>3.0*b.x {continue;}
        let cell=min(vec2<u32>(point/3.0),vec2<u32>(b.yx)-vec2(1u));
        let bin=boundaries[u32(b.z)+cell.x*u32(b.x)+cell.y];
        for(var i=0u;i<u32(bin.y);i+=1u){
            let id=u32(boundaries[u32(bin.x)+i].x);let t=boundaries[id];let v=boundaries[id+1u];
            let p0=t.xy;let p1=t.zw;let p2=v.xy;
            let signs=vec3(cross2(p1-p0,point-p0),cross2(p2-p1,point-p1),cross2(p0-p2,point-p2));
            // Signed edge distance, not a fixed area tolerance on skinny faces.
            let epsilon=0.0002*vec3(length(p1-p0),length(p2-p1),length(p0-p2));
            if all(signs>=-epsilon) || all(signs<=epsilon) {return v.z>0.5;}
        }
    }
    return additional_water(angles,terrain.classification);
}
