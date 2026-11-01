// Original MIT. Curved geometric-optics rays through a weather-derived profile.
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
struct Settings {
    q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,latitude:vec4<f32>,
    right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,lens:vec4<f32>,budget:vec4<f32>,scene:vec4<f32>,
    sun:vec4<f32>,moon:vec4<f32>,moon_light:vec4<f32>,lunar:LunarSettings,environment:vec4<f32>,
    icrf0:vec4<f32>,icrf1:vec4<f32>,icrf2:vec4<f32>,
    ground_origin:vec4<f32>,east:vec4<f32>,vertical:vec4<f32>,south:vec4<f32>,phase:vec4<f32>,star_sin:vec4<f32>,star_cos:vec4<f32>,star_environment:vec4<f32>,
    region0:array<vec4<f32>,4>,region1:array<vec4<f32>,4>,
    layers:array<vec4<f32>,128>, eclipse:EclipseSettings, classification:ClassificationSettings,control:vec4<u32>,stellar_control:vec4<u32>,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings:Settings;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<storage,read> boundaries:array<vec4<f32>>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<storage,read> native_water:array<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var<storage,read> global_land:array<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var lunar_color:texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var lunar_color_sampler:sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(7) var lunar_normal:texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(8) var lunar_normal_sampler:sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(11) var<storage,read> terminals:array<vec4<u32>>;
@group(constants::MATERIAL_BIND_GROUP) @binding(12) var<storage,read> stellar_arrivals:array<vec4<u32>>;
fn qmul(p:vec3<f32>)->vec3<f32>{return vec3(dot(settings.q0.xyz,p),dot(settings.q1.xyz,p),dot(settings.q2.xyz,p));}
// Height relative to the true geodetic parallel surfaces, not inflated axes.
fn geometry(p:vec3<f32>)->vec4<f32>{
    let mp=qmul(p);let qp=settings.origin.xyz+mp;
    let f=settings.origin.w+dot(p,2.0*settings.origin.xyz+mp);
    if length(qp)<0.75 {return vec4(0.0,1.0,0.0,-1000000.0);}
    var normal=normalize(qp);var h=0.0;
    // An identical normal bit pattern repeats the same h/normal next time.
    // This skips exact fixed points only, not an epsilon convergence test.
    for(var i=0;i<5;i+=1){let previous_normal=normal;let qn=qmul(normal);let v=dot(normal,qp);
        h=f/(v+sqrt(max(0.0,v*v-f*dot(normal,qn))));normal=normalize(qp-h*qn);if all(bitcast<vec3<u32>>(normal)==bitcast<vec3<u32>>(previous_normal)){break;}}
    return vec4(normal,h);
}
fn index_gradient(h:f32)->vec2<f32>{
    let count=u32(settings.budget.x);
    if h>=settings.budget.y {return vec2(0.0);}
    if h<settings.scene.y {return vec2(settings.layers[0].y,0.0);}
    var lo=0u;var hi=count-1u;
    for(var i=0;i<8;i+=1){if hi<=lo+1u {break;}let mid=(lo+hi)/2u;
        if settings.layers[mid].x<=h {lo=mid;}else{hi=mid;}}
    let knot=settings.layers[lo];return vec2(knot.y+knot.z*(h-knot.x),knot.z);
}
fn knot_bound(gap:f32,velocity:f32,curvature:f32)->f32 {
    let root=sqrt(velocity*velocity+2.0*curvature*gap);
    if velocity>=0.0 {return 2.0*gap/max(root+velocity,1e-20);}
    return (root-velocity)/curvature;
}
// Conservative height excursion to the nearest profile knots in either direction.
// A negative result means the local f32 geometry cannot resolve those layers.
fn layer_step(height:f32,vertical:f32,curvature:f32,band:f32)->f32 {
    let count=u32(settings.budget.x);var lo=0u;var hi=count-1u;
    for(var i=0;i<8;i+=1){if hi<=lo+1u {break;}let mid=(lo+hi)/2u;
        if settings.layers[mid].x<=height {lo=mid;}else{hi=mid;}}
    let first=select(0u,lo-1u,lo>0u);let last=min(lo+1u,count-2u);
    for(var i=first;i<=last;i+=1u){
        if settings.layers[i+1u].x-settings.layers[i].x<4.0*band {return -1.0;}
    }
    var limit=1e30;
    var below=i32(lo);
    if height-settings.layers[lo].x<=band {below-=1;}
    if below>=0 {let gap=height-settings.layers[u32(below)].x;
        if gap>band {limit=min(limit,knot_bound(gap,-vertical,curvature));}}
    var above=select(lo+1u,0u,height<settings.layers[0].x-band);
    if settings.layers[above].x-height<=band {above+=1u;}
    if above<count {let gap=settings.layers[above].x-height;
        if gap>band {limit=min(limit,knot_bound(gap,vertical,curvature));}}
    return limit;
}
struct State{p:vec3<f32>,u:vec3<f32>};
fn midpoint_delta(s:State,step:f32)->State{
    let g=geometry(s.p);let ng=index_gradient(g.w);
    let bend=(g.xyz-s.u*dot(s.u,g.xyz))*(ng.y/(1.0+ng.x));
    let mp=s.p+s.u*(step*0.5);let mu=normalize(s.u+bend*(step*0.5));
    let mg=geometry(mp);let mn=index_gradient(mg.w);
    return State(mu*step,normalize(s.u+(mg.xyz-mu*dot(mu,mg.xyz))*(mn.y/(1.0+mn.x)*step)));
}
fn midpoint(s:State,step:f32)->State{let delta=midpoint_delta(s,step);return State(s.p+delta.p,delta.u);}
fn advance_delta(s:State,step:f32)->State{
    let first=midpoint_delta(s,step*0.5);
    let second=midpoint_delta(State(s.p+first.p,first.u),step*0.5);
    return State(first.p+second.p,second.u);
}
fn advance(s:State,step:f32)->State{let delta=advance_delta(s,step);return State(s.p+delta.p,delta.u);}
struct Trace{p:vec3<f32>,u:vec3<f32>,path:f32,status:u32,normal:vec3<f32>,material:u32,marker:u32}; //0 sky,1 surface,2 unresolved
fn trace_result(p:vec3<f32>,u:vec3<f32>,path:f32,status:u32)->Trace{return Trace(p,u,path,status,vec3(0.0),0u,0u);}
fn trace(direction:vec3<f32>)->Trace{
    var state=State(vec3(0.0),direction);var path=0.0;var proposed=2000.0;
    if settings.budget.w<0.5 {return trace_result(state.p,state.u,path,2u);}
    if settings.center.w>settings.budget.y {
        let o=-settings.center.xyz;let along=dot(o,direction);let distance=length(cross(o,direction));
        let radius=6378137.0+settings.budget.y;
        if distance>radius || along>=0.0 {return trace_result(state.p,state.u,path,0u);}
        let span=sqrt(max(0.0,(radius-distance)*(radius+distance)));
        let lower=max(0.0,-along-span);var a=lower;var b=-along+span;
        for(var i=0;i<40;i+=1){let x=a+(b-a)*0.38196601125;let y=a+(b-a)*0.61803398875;
            if geometry(direction*x).w<geometry(direction*y).w {b=y;}else{a=x;}}
        var upper=(a+b)*0.5;
        if geometry(direction*upper).w>settings.budget.y {return trace_result(state.p,state.u,path,0u);}
        var low=lower;
        for(var i=0;i<32;i+=1){let mid=(low+upper)*0.5;
            if geometry(direction*mid).w>settings.budget.y {low=mid;}else{upper=mid;}}
        state.p=direction*upper;path=upper;
    }
    let entry=path;let curvature=settings.budget.z;
    for(var i=0;i<4096;i+=1){
        let g=geometry(state.p);let vertical=dot(state.u,g.xyz);
        if g.w==0.0 && vertical<0.0 {
            let coverage=terrain_coverage(state.p,g.xyz);
            if coverage==2u{return trace_result(state.p,state.u,path,2u);}
            if coverage==0u{return trace_result(state.p,state.u,path,1u);}
        }
        if g.w>=settings.budget.y-0.01 && vertical>0.0 {return trace_result(state.p,state.u,path,0u);}
        if path-entry>2000000.0 || g.w<settings.scene.z-1.0 {return trace_result(state.p,state.u,path,2u);}
        // Retain the h0 event bound even where terrain masks that surface:
        // both entering and exiting crossings must be isolated, not skipped.
        let clearance=abs(g.w);let away=select(vertical,-vertical,g.w<0.0);
        let disc=sqrt(max(0.0,away*away+2.0*curvature*clearance));
        var safe=(away+disc)/curvature;
        if away<0.0 {safe=2.0*clearance/max(disc-away,1e-20);}
        var step=min(min(proposed,2000.0),max(safe*0.8,0.01));
        // f32 presentation accuracy is scale-dependent; f64 metrology remains independent.
        let tolerance=max(0.002,4e-7*length(state.p));
        if clearance<tolerance && away< -1e-8 {step=min(step,clamp(2.0*max(clearance,1e-8)/-away,0.01,1.0));}
        let knot=layer_step(g.w,vertical,curvature,max(1e-5,4e-7*length(state.p)));
        if knot<0.0 {return trace_result(state.p,state.u,path,2u);}
        step=min(step,max(0.01,knot));
        let full=midpoint_delta(state,step);let half_delta=advance_delta(state,step);
        let half=State(state.p+half_delta.p,half_delta.u);
        let error=max(length(full.p-half_delta.p),length(full.u-half_delta.u)*step)/3.0;
        if error>tolerance && step>0.01000001 {proposed=step*0.5;continue;}
        if error>tolerance {return trace_result(state.p,state.u,path,2u);}
        let next_height=geometry(half.p).w;
        var reference_distance=-1.0;
        if (g.w>0.0 && next_height<=0.0)||(g.w<0.0 && next_height>=0.0) {
            var low=0.0;var high=step;
            for(var j=0;j<20;j+=1){let mid=(low+high)*0.5;
                if geometry(advance(state,mid).p).w*g.w>0.0 {low=mid;}else{high=mid;}}
            let delta=(low+high)*0.5;let end=advance(state,delta);
            let coverage=terrain_coverage(end.p,geometry(end.p).xyz);
            if coverage==2u{return trace_result(state.p,state.u,path,2u);}
            if coverage==0u {reference_distance=delta;}
        }
        // Reuse the accepted interval's exact displacement and error. Only an
        // earlier uncovered h0 contact needs a separately integrated interval.
        var event_step=step;var event_end=half;var event_delta=half_delta.p;var event_error=error;
        if reference_distance>=0.0 {
            event_step=reference_distance;
            let event_full=midpoint_delta(state,event_step);let event_half=advance_delta(state,event_step);
            event_delta=event_half.p;event_end=State(state.p+event_delta,event_half.u);
            event_error=max(length(event_full.p-event_delta),length(event_full.u-event_half.u)*event_step)/3.0;
        }
        let event=scene_event(state,event_end,event_delta,event_step,event_error);
        if event.ambiguous!=0u {
            let event_floor=max(0.001,9.536743e-7*max(length(state.p),length(event_end.p)));
            if event_step>event_floor {proposed=event_step*0.5;continue;}
            return trace_result(state.p,state.u,path,2u);
        }
        if event.distance>=0.0 {
            let end=advance(state,event.distance);
            return Trace(end.p,end.u,path+event.distance,1u,event.normal,event.kind,event.marker);
        }
        if reference_distance>=0.0 {
            let end=advance(state,reference_distance);
            return trace_result(end.p,end.u,path+reference_distance,1u);
        }
        if g.w<settings.budget.y && next_height>=settings.budget.y {
            var low=0.0;var high=step;
            for(var j=0;j<20;j+=1){let mid=(low+high)*0.5;
                if geometry(advance(state,mid).p).w<settings.budget.y {low=mid;}else{high=mid;}}
            let delta=(low+high)*0.5;let end=advance(state,delta);
            return trace_result(end.p,end.u,path+delta,0u);
        }
        state=half;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);
    }
    return trace_result(state.p,state.u,path,2u);
}
fn relative_angles(n:vec3<f32>)->vec2<f32>{
    let a=n.y*settings.latitude.y+n.z*settings.latitude.x;
    let h=length(vec2(a,n.x));var d=h-a;
    if a>=0.0 && h+a>0.0 {d=n.x*n.x/(h+a);}
    return vec2(atan2(-n.z-d*settings.latitude.x,n.y+d*settings.latitude.y),atan2(n.x,a));
}
fn cross2(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
fn water_at(n:vec3<f32>)->bool{
    if settings.classification.control.x==0u {return false;}
    let angles=relative_angles(n);
    for(var region=0u;region<4u;region+=1u){
        let a=settings.region0[region];let b=settings.region1[region];if b.z<1.0 {continue;}
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
    return additional_water(angles,settings.classification);
}
fn sky_color(direction:vec3<f32>)->vec3<f32>{
    let t=pow(max(direction.y,0.0),0.55);let daylight=settings.environment.x*settings.environment.y;
    return mix(vec3(0.34,0.48,0.65),vec3(0.055,0.20,0.43),t)*daylight+vec3(0.0004,0.0007,0.0016);
}
fn sphere_hit(o:vec3<f32>,d:vec3<f32>,body:vec4<f32>)->f32{
    if body.w<=0.0 {return -1.0;}let relative=body.xyz-o;let along=dot(relative,d);
    let closest=length(cross(relative,d));if along<=0.0 || closest>body.w {return -1.0;}
    return along-sqrt(max(0.0,(body.w-closest)*(body.w+closest)));
}

fn radiance(ray:Trace,launch:vec3<f32>,pixel:vec2<f32>)->vec3<f32>{
    if ray.status==2u {return vec3(0.24,0.015,0.18);} // explicit unresolved diagnostic
    if ray.status==0u {
        let sun_hit=sphere_hit(ray.p,ray.u,settings.sun);let moon_hit=sphere_hit(ray.p,ray.u,settings.moon);
        if moon_hit>0.0 && (sun_hit<0.0 || moon_hit<sun_hit){
            let normal=normalize(ray.p+ray.u*moon_hit-settings.moon.xyz);
            let body_normal=normalize(lunar_body(normal,settings.lunar));
            let eye=normalize(lunar_body(-ray.u,settings.lunar));
            let sun=normalize(lunar_body(settings.moon_light.xyz,settings.lunar));
            // Local spherical footprint heuristic; not a curved-ray Jacobian bound.
            let footprint=moon_hit/(settings.moon.w*settings.lens.w*max(abs(dot(normal,-ray.u)),0.02));
            return lunar_radiance(body_normal,eye,sun,footprint);
        }
        if sun_hit>0.0 {return vec3(1.0,0.92,0.68);}
        var color=sky_color(ray.u);
        // The same stable original ICRF catalogue as the raster view.
        let inertial=normalize(vec3(dot(ray.u,settings.icrf0.xyz),dot(ray.u,settings.icrf1.xyz),dot(ray.u,settings.icrf2.xyz)));
        color+=catalogue_stars(inertial,pixel)*settings.environment.z;
        return color;
    }
    if ray.material==2u{return sign_radiance(ray.p,ray.marker);}
    let geometry_at=geometry(ray.p);
    var normal=geometry_at.xyz;
    if length(ray.normal)>0.5 {normal=ray.normal;}
    if ray.material==0u && dot(normal,geometry_at.xyz)<0.0 {normal=-normal;}
    let sun=normalize(select(vec3(0.0,1.0,0.0),settings.sun.xyz-ray.p,settings.sun.w>0.0));
    let visible=eclipse_solar_visibility(ray.p,settings.eclipse);
    let direct=select(0.0,max(dot(normal,sun),0.0)*visible,settings.sun.w>0.0);
    let fill=settings.environment.x*(0.04+0.96*sqrt(visible));
    let ecef=settings.ground_origin.xyz+settings.east.xyz*ray.p.x+settings.vertical.xyz*ray.p.y+settings.south.xyz*ray.p.z;
    let point=ecef-floor(ecef/4096.0)*4096.0;
    // Grazing surface projection stretches a pixel footprint. This local-plane
    // heuristic suppresses shimmer; it is not a caustic/Jacobian error bound.
    let footprint=ray.path/(max(settings.lens.w,1.0)*max(abs(dot(ray.u,normal)),1e-6));
    if ray.material==1u{return marker_radiance(ray.p,normal,ray.marker,footprint,direct,fill);}
    var color=vec3(0.18,0.27,0.11)*(0.08+1.35*direct+0.32*fill);
    if water_at(geometry_at.xyz){
        let fade=1.0-smoothstep(0.5,5.0,footprint);
        let k=6.28318530718/16.0;
        let phase=dot(point,vec3(1.0,2.0,0.0))*k;
        let derivative=cos(phase)*settings.phase.y-sin(phase)*settings.phase.x;
        let ecef_slope=vec3(1.0,2.0,0.0)*0.018*derivative*fade;
        let local_slope=vec3(dot(ecef_slope,settings.east.xyz),dot(ecef_slope,settings.vertical.xyz),dot(ecef_slope,settings.south.xyz));
        let n=normalize(normal-(local_slope-normal*dot(local_slope,normal)));
        let reflected=reflect(ray.u,n);let fresnel=0.02+0.98*pow(1.0-max(dot(-ray.u,n),0.0),5.0);
        let highlight=select(0.0,pow(max(dot(reflect(-sun,n),-ray.u),0.0),180.0)*visible,settings.sun.w>0.0);
        color=vec3(0.012,0.055,0.070)*(0.10+direct+0.3*fill)
            +sky_color(reflected)*fresnel+vec3(1.0,0.90,0.70)*highlight*0.8;
    }else{
        let detail=(sin(point.x*0.392699)*sin(point.y*0.392699)*sin(point.z*0.392699));
        color*=1.0+0.035*detail*(1.0-smoothstep(0.5,3.0,footprint));
    }
    // Original illustrative haze, distinct from the physical ray index model.
    let haze=(1.0-exp(-ray.path*0.000018))*0.55*settings.environment.x;
    return mix(color,sky_color(normalize(ray.u+normal*0.3)),haze);
}
@fragment
fn fragment(in:VertexOutput)->@location(0) vec4<f32>{
    // An enabled table belongs to this same physical pixel packet. Missing or
    // stale storage is pending; a disabled table takes the original inverse.
    if any(settings.stellar_control!=vec4(0u)){
        if any(settings.stellar_control!=settings.control) || arrayLength(&stellar_arrivals)<3001u{return vec4(0.002,0.004,0.008,1.0);}
        if any(stellar_arrivals[0]!=settings.control){return vec4(0.002,0.004,0.008,1.0);}
    }
    // The compute path owns the physical pixel's ray and aperture. No stale
    // terminal may be combined with another eye/profile/scene snapshot.
    if settings.control.x>0u && settings.control.y>0u {
        let pixel=vec2<u32>(floor(in.position.xy));
        if any(pixel>=settings.control.xy) || any(terminals[0]!=settings.control){return vec4(0.002,0.004,0.008,1.0);}
        let base=1u+3u*(pixel.y*settings.control.x+pixel.x);
        if base+2u>=arrayLength(&terminals){return vec4(0.002,0.004,0.008,1.0);}
        let position=terminals[base];let direction=terminals[base+1u];let surface=terminals[base+2u];
        if direction.w==3u{return vec4(0.002,0.004,0.008,1.0);}
        let ray=Trace(bitcast<vec3<f32>>(position.xyz),bitcast<vec3<f32>>(direction.xyz),bitcast<f32>(position.w),direction.w,bitcast<vec3<f32>>(surface.xyz),surface.w>>24u,surface.w&0x00ffffffu);
        return vec4(radiance(ray,vec3(0.0),vec2<f32>(pixel)),1.0);
    }
    let xy=vec2((2.0*in.uv.x-1.0)*settings.lens.x,1.0-2.0*in.uv.y);
    var optical=normalize(vec3(xy*tan(settings.lens.y),1.0));
    if settings.lens.z>0.5 {
        let radius=length(xy);if radius>1.0 {return vec4(0.002,0.004,0.008,1.0);}
        let theta=radius*settings.lens.y;let scale=select(settings.lens.y,sin(theta)/max(radius,1e-8),radius>1e-8);
        optical=vec3(xy*scale,cos(theta));
    }
    let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);
    return vec4(radiance(trace(direction),direction,floor(in.position.xy)),1.0);
}

@group(constants::MATERIAL_BIND_GROUP) @binding(2)
var<storage,read> star_data:array<vec4<f32>>;
fn catalogue_stars(direction:vec3<f32>,pixel:vec2<f32>)->vec3<f32> {
    var color=vec3<f32>(0.0);
    if settings.environment.z<=0.0 {return color;}
    for(var face=0u;face<6u;face+=1u){
        let projected=star_face(direction,face);
        if projected.z<0.5 {continue;}
        let coord=projected.xy/projected.z;
        if max(abs(coord.x),abs(coord.y))>1.06 {continue;}
        let cell=clamp(vec2<i32>(floor((coord+1.0)*16.0)),vec2<i32>(0),vec2<i32>(31));
        for(var dy=-1;dy<=1;dy+=1){for(var dx=-1;dx<=1;dx+=1){
            let c=cell+vec2(dx,dy);
            if any(c<vec2<i32>(0))||any(c>vec2<i32>(31)){continue;}
            let bin=star_data[face*1024u+u32(c.y)*32u+u32(c.x)];
            for(var index=0u;index<u32(bin.y);index+=1u){
                let star=star_data[u32(bin.x)+2u*index];
                let tint=star_data[u32(bin.x)+2u*index+1u];
                let normal=normalize(star.xyz);
                // Finite candidate-search heuristic before repeated optical shooting.
                // The finite f32 inverse tolerance is distinct from f64 metrology.
                if length(cross(direction,normal))>max(3.0/max(settings.lens.w,1.0),0.00003){continue;}
                let stellar_direction=normalize(settings.icrf0.xyz*normal.x+settings.icrf1.xyz*normal.y+settings.icrf2.xyz*normal.z);
                let arrival=shared_stellar_arrival(stellar_direction,u32(tint.w+0.5));
                if arrival.w==0.0 {continue;}
                let camera=vec3(dot(arrival.xyz,settings.right.xyz),dot(arrival.xyz,settings.up.xyz),dot(arrival.xyz,settings.forward.xyz));
                let owned=star_pixel(camera,settings.star_environment.zw,settings.lens.y,settings.lens.z>0.5);
                if owned.z==0.0||any(owned.xy!=pixel){continue;}
                color+=tint.rgb*(0.18*star.w)*star_twinkle(u32(tint.w+0.5),settings.star_sin,settings.star_cos,
                    settings.star_environment.x,arrival.y,settings.star_environment.y);
            }
        }}
    }
    return color;
}

fn shared_stellar_arrival(stellar_direction:vec3<f32>,stable_id:u32)->vec4<f32>{
    if any(settings.stellar_control!=vec4(0u)){
        let record=stellar_arrivals[1u+stable_id];
        if record.w==1u{return vec4(0.0);}
        if record.w==2u{return vec4(bitcast<vec3<f32>>(record.xyz),1.0);}
    }
    // A gather/fragment boundary rounding disagreement keeps the exact original
    // inverse path; an unrequested ID never silently removes a visible star.
    return inverse_star(stellar_direction);
}

// Each candidate is solved from its fixed catalogue direction, independently of
// the fragment asking for it. Consequently the final floor rule has one owner.
fn inverse_star(stellar_direction:vec3<f32>)->vec4<f32> {
    var guess=stellar_direction;
    if stellar_direction.y<0.05 {
        if length(stellar_direction.xz)<1e-10{return vec4(0.0);}
        guess=normalize(vec3(normalize(stellar_direction.xz).x,0.05,normalize(stellar_direction.xz).y));
    }
    var last_sky=guess;
    for(var i=0;i<16;i+=1){
        let ray=trace_atmosphere(guess);
        if ray.status==2u{return vec4(0.0);}
        if ray.status==1u{guess=normalize(guess+last_sky);continue;}
        let error=stellar_direction-ray.u;
        if length(error)<=3e-6{return vec4(guess,1.0);}
        last_sky=guess;guess=normalize(guess+error);
    }
    return vec4(0.0);
}

// Stellar inverse shooting needs the refractive exit map, not repeated scene
// intersections. The final pixel trace still supplies all physical occlusion.
// Keep the same field, knot/adaptive steps, top refinement and finite budgets.
// Candidates below the supported lower atmosphere fail closed; h0 alone is not
// an obstruction because imported canonical terrain may lie below it.
fn trace_atmosphere(direction:vec3<f32>)->Trace{
    var state=State(vec3(0.0),direction);var path=0.0;var proposed=2000.0;
    if settings.budget.w<0.5 {return trace_result(state.p,state.u,path,2u);}
    if settings.center.w>settings.budget.y {
        let o=-settings.center.xyz;let along=dot(o,direction);let distance=length(cross(o,direction));
        let radius=6378137.0+settings.budget.y;
        if distance>radius || along>=0.0 {return trace_result(state.p,state.u,path,0u);}
        let span=sqrt(max(0.0,(radius-distance)*(radius+distance)));
        let lower=max(0.0,-along-span);var a=lower;var b=-along+span;
        for(var i=0;i<40;i+=1){let x=a+(b-a)*0.38196601125;let y=a+(b-a)*0.61803398875;
            if geometry(direction*x).w<geometry(direction*y).w {b=y;}else{a=x;}}
        var upper=(a+b)*0.5;
        if geometry(direction*upper).w>settings.budget.y {return trace_result(state.p,state.u,path,0u);}
        var low=lower;
        for(var i=0;i<32;i+=1){let mid=(low+upper)*0.5;
            if geometry(direction*mid).w>settings.budget.y {low=mid;}else{upper=mid;}}
        state.p=direction*upper;path=upper;
    }
    let entry=path;let curvature=settings.budget.z;
    for(var i=0;i<4096;i+=1){
        let g=geometry(state.p);let vertical=dot(state.u,g.xyz);
        if g.w>=settings.budget.y-0.01 && vertical>0.0 {return trace_result(state.p,state.u,path,0u);}
        if path-entry>2000000.0 || g.w<settings.scene.z-1.0 {return trace_result(state.p,state.u,path,2u);}
        // Retain the primary trace's h0 step bound for the same clear-sky
        // step schedule; this candidate map does not treat h0 as a surface.
        let clearance=abs(g.w);let away=select(vertical,-vertical,g.w<0.0);
        let disc=sqrt(max(0.0,away*away+2.0*curvature*clearance));
        var safe=(away+disc)/curvature;
        if away<0.0 {safe=2.0*clearance/max(disc-away,1e-20);}
        var step=min(min(proposed,2000.0),max(safe*0.8,0.01));
        // f32 presentation accuracy is scale-dependent; f64 metrology remains independent.
        let tolerance=max(0.002,4e-7*length(state.p));
        if clearance<tolerance && away< -1e-8 {step=min(step,clamp(2.0*max(clearance,1e-8)/-away,0.01,1.0));}
        let knot=layer_step(g.w,vertical,curvature,max(1e-5,4e-7*length(state.p)));
        if knot<0.0 {return trace_result(state.p,state.u,path,2u);}
        step=min(step,max(0.01,knot));
        let full=midpoint_delta(state,step);let half_delta=advance_delta(state,step);
        let half=State(state.p+half_delta.p,half_delta.u);
        let error=max(length(full.p-half_delta.p),length(full.u-half_delta.u)*step)/3.0;
        if error>tolerance && step>0.01000001 {proposed=step*0.5;continue;}
        if error>tolerance {return trace_result(state.p,state.u,path,2u);}
        let next_height=geometry(half.p).w;
        if g.w<settings.budget.y && next_height>=settings.budget.y {
            var low=0.0;var high=step;
            for(var j=0;j<20;j+=1){let mid=(low+high)*0.5;
                if geometry(advance(state,mid).p).w<settings.budget.y {low=mid;}else{high=mid;}}
            let delta=(low+high)*0.5;let end=advance(state,delta);
            return trace_result(end.p,end.u,path+delta,0u);
        }
        state=half;path+=step;proposed=select(step,step*2.0,error<tolerance*0.125);
    }
    return trace_result(state.p,state.u,path,2u);
}
