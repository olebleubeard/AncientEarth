// Original bounded scintillation, shared by raster and curved-ray stars.
fn star_hash(value:u32)->u32 {
    var x=value; x=x^(x>>16u);x=x*0x7feb352du;
    x=x^(x>>15u);x=x*0x846ca68bu;return x^(x>>16u);
}
// HV5/7 weak plane-wave variance drives a positive four-factor process.
// The four temporal frequencies and independent hashed phases are illustrative.
fn star_twinkle(id:u32,phase_sin:vec4<f32>,phase_cos:vec4<f32>,zenith_variance:f32,elevation:f32,exposure:f32)->f32 {
    let phases=vec4(
        f32(star_hash(id)>>8u),f32(star_hash(id+0x9e3779b9u)>>8u),
        f32(star_hash(id+0x3c6ef372u)>>8u),f32(star_hash(id+0xdaa66d2bu)>>8u)
    )*(6.28318530718/16777216.0)+atan2(phase_sin,phase_cos);
    let variance=zenith_variance/pow(max(elevation,0.57357643635),11.0/6.0);
    // Algebraically equivalent fourth-root difference, stable near variance0.
    let root=sqrt(sqrt(1.0+variance));
    let amplitude=sqrt(2.0*variance/((root+1.0)*(root*root+1.0)));
    if amplitude<=1e-8{return 1.0;}
    var sum=0.0;
    for(var code=0u;code<81u;code+=1u){
        let digits=vec4<i32>(i32(code%3u)-1,i32((code/3u)%3u)-1,
            i32((code/9u)%3u)-1,i32(code/27u)-1);
        let signs=vec4<f32>(digits);
        let count=dot(abs(signs),vec4(1.0));
        let coefficient=pow(amplitude*0.5,count);
        let angle=dot(signs,phases-1.57079632679);
        let centihz=digits.x*73+digits.y*117+digits.z*191+digits.w*271;
        let half_phase=3.14159265359*0.01*f32(centihz)*exposure;
        var average=1.0;
        if centihz!=0 {
            if abs(half_phase)>1e12 {average=0.0;}
            else if abs(half_phase)>1e-4 {average=sin(half_phase)/half_phase;}
        }
        sum+=coefficient*cos(angle)*average;
    }
    return clamp(sum,pow(1.0-amplitude,4.0),pow(1.0+amplitude,4.0));
}
// Floor ownership is a display quantization rule, not a stellar angular diameter.
fn star_pixel(d:vec3<f32>,size:vec2<f32>,half_fov:f32,fisheye:bool)->vec3<f32> {
    var offset:vec2<f32>;
    if fisheye {
        let r=length(d.xy);let theta=atan2(r,d.z);
        if theta>half_fov {return vec3(0.0);}
        offset=d.xy*(theta/max(r,1e-20))*(size.y/(2.0*half_fov));
    } else {
        if d.z<=0.0 {return vec3(0.0);}
        offset=d.xy/d.z*(size.y/(2.0*tan(half_fov)));
    }
    let pixel=floor(size*0.5+offset*vec2(1.0,-1.0));
    if any(pixel<vec2(0.0))||any(pixel>=size){return vec3(0.0);}
    return vec3(pixel,1.0);
}
fn star_face(d:vec3<f32>,face:u32)->vec3<f32> {
    switch face {
        case 0u:{return vec3(d.y,d.z,d.x);} case 1u:{return vec3(d.y,d.z,-d.x);}
        case 2u:{return vec3(d.x,d.z,d.y);} case 3u:{return vec3(d.x,d.z,-d.y);}
        case 4u:{return vec3(d.x,d.y,d.z);} default:{return vec3(d.x,d.y,-d.z);}
    }
}
