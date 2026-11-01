// Canonical triangle/OBB visibility along finite curved-ray intervals.
@group(constants::MATERIAL_BIND_GROUP) @binding(9) var<storage,read> optical_geometry:array<vec4<f32>>;
@group(constants::MATERIAL_BIND_GROUP) @binding(10) var<storage,read> optical_frames:array<vec4<f32>>;
const OPTICAL_LEAF:u32=0x80000000u;
const OPTICAL_ADDRESS:u32=0x00ffffffu;

fn frame_point(p:vec3<f32>,base:u32)->vec3<f32>{
    if base==0u{return p;}
    return optical_frames[base].xyz+vec3(dot(optical_frames[base+1u].xyz,p),dot(optical_frames[base+2u].xyz,p),dot(optical_frames[base+3u].xyz,p));
}
fn frame_vector(p:vec3<f32>,base:u32)->vec3<f32>{
    if base==0u{return p;}
    return vec3(dot(optical_frames[base+1u].xyz,p),dot(optical_frames[base+2u].xyz,p),dot(optical_frames[base+3u].xyz,p));
}
fn frame_inverse(p:vec3<f32>,base:u32)->vec3<f32>{
    if base==0u{return p;}
    return optical_frames[base+1u].xyz*p.x+optical_frames[base+2u].xyz*p.y+optical_frames[base+3u].xyz*p.z;
}
fn segment_box(a:vec3<f32>,b:vec3<f32>,lo:vec3<f32>,hi:vec3<f32>,padding:f32)->bool{
    let d=b-a;var low=0.0;var high=1.0;
    for(var k=0u;k<3u;k+=1u){
        if d[k]==0.0 {if a[k]<lo[k]-padding||a[k]>hi[k]+padding{return false;}}
        else {let x=(lo[k]-padding-a[k])/d[k];let y=(hi[k]+padding-a[k])/d[k];low=max(low,min(x,y));high=min(high,max(x,y));if low>high{return false;}}
    }
    return true;
}
fn triangle_contains(a:vec3<f32>,b:vec3<f32>,c:vec3<f32>,p:vec3<f32>,n:vec3<f32>)->bool{
    return dot(cross(b-a,p-a),n)>=0.0 && dot(cross(c-b,p-b),n)>=0.0 && dot(cross(a-c,p-c),n)>=0.0;
}
// Necessary supporting-halfspace conditions for a triangle's radius tube.
// Equality is retained. These reject candidates only; roots still require
// reintegration and finite-face membership below.
fn triangle_candidate(p:vec3<f32>,q:vec3<f32>,a:vec3<f32>,b:vec3<f32>,c:vec3<f32>,n:vec3<f32>,radius:f32)->bool{
    let before=dot(n,p-a);let after=dot(n,q-a);
    if before*after>0.0 && min(abs(before),abs(after))>radius{return false;}
    let vertices=array<vec3<f32>,3>(a,b,c);
    for(var i=0u;i<3u;i+=1u){
        let origin=vertices[i];let edge=vertices[(i+1u)%3u]-origin;
        let x=dot(cross(edge,p-origin),n);let y=dot(cross(edge,q-origin),n);
        if max(x,y)<-radius*length(edge){return false;}
    }
    return true;
}
struct SceneEvent {distance:f32,normal:vec3<f32>,kind:u32,marker:u32,ambiguous:u32,lower:f32,upper:f32};
fn empty_event()->SceneEvent{return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);}
fn combine_event(a:SceneEvent,b:SceneEvent)->SceneEvent{
    var result=a;
    if b.distance>=0.0 && (a.distance<0.0 || b.upper<a.lower || (b.lower<=a.upper && b.distance<a.distance)){result=b;}
    result.ambiguous=a.ambiguous|b.ambiguous;
    // Disjoint uncertainty intervals have an ordered first contact. Overlapping
    // distinct physical/material owners cannot be silently resolved by a chord.
    if a.distance>=0.0 && b.distance>=0.0 && a.lower<=b.upper && b.lower<=a.upper && (a.kind!=b.kind || a.marker!=b.marker){result.ambiguous=1u;}
    return result;
}
fn triangle_band(a:vec3<f32>,b:vec3<f32>,c:vec3<f32>,p:vec3<f32>,n:vec3<f32>,band:f32)->bool{
    return dot(cross(b-a,p-a),n)>=-band*length(b-a) && dot(cross(c-b,p-b),n)>=-band*length(c-b) && dot(cross(a-c,p-c),n)>=-band*length(a-c);
}
fn face_event(start:State,end:State,delta:vec3<f32>,step:f32,origin:vec3<f32>,vb:vec3<f32>,vc:vec3<f32>,base:u32,kind:u32,marker:u32,error:f32,deviation:f32)->SceneEvent{
    var result=empty_event();let a=vec3(0.0);let b=vb-origin;let c=vc-origin;
    let cross_normal=cross(b,c);let area=length(cross_normal);
    if area<=1e-16 {return result;}
    let n=cross_normal/area;
    // Work around a single face anchor. Midpoint displacement is accumulated
    // without subtracting two large ray positions. Source quantization stays
    // in broad-phase bounds; this finite band describes display arithmetic.
    let p=frame_point(start.p,base)-origin;
    let q=p+frame_vector(delta,base);
    var transform_scale=length(start.p)+length(origin);
    if base>0u {transform_scale+=length(optical_frames[base].xyz);}
    let band=max(0.002,9.536743e-7*(transform_scale+length(p)+max(length(b),length(c))+step));
    let tube=deviation+error;
    let before=dot(n,p);let after=dot(n,q);
    if !triangle_candidate(p,q,a,b,c,n,tube+band){return result;}
    if tube>band*0.25 {result.ambiguous=1u;return result;}
    let velocity=dot(n,frame_vector(start.u,base));
    let transverse=abs(velocity)-settings.scene.x*step;
    if transverse<=0.00001 {result.ambiguous=1u;return result;}
    var distance=0.0;
    if before*after<=0.0 && before!=after {
        var low=0.0;var high=step;
        for(var i=0;i<24;i+=1){
            if high-low<=max(0.000001,band*0.01){break;}
            let middle=(low+high)*0.5;
            let h=dot(n,p+frame_vector(advance_delta(start,middle).p,base));
            if h*before>0.0 {low=middle;}else{high=middle;}
        }
        distance=(low+high)*0.5;
    }else{
        // Finite transverse contact band. A same-sign interval farther from
        // the plane remains ambiguous; grazing/tangent contacts never use it.
        let estimate=-before/velocity;
        let uncertainty=(band+tube)/transverse;
        if estimate < -uncertainty || estimate>step+uncertainty {result.ambiguous=1u;return result;}
        distance=clamp(estimate,0.0,step);
    }
    let hit=p+frame_vector(advance_delta(start,distance).p,base);
    if abs(dot(n,hit))>band+tube || !triangle_band(a,b,c,hit,n,band+tube){result.ambiguous=1u;return result;}
    result.distance=distance;result.normal=normalize(frame_inverse(n,base));result.kind=kind;result.marker=marker;
    let uncertainty=(band+tube)/transverse;
    result.lower=max(0.0,distance-uncertainty);result.upper=distance+uncertainty;
    return result;
}
fn terrain_events(start:State,end:State,delta:vec3<f32>,step:f32,tolerance:f32,deviation:f32)->SceneEvent{
    var result=empty_event();let root=bitcast<u32>(optical_frames[0].x);if root==0u{return result;}
    let ga=frame_point(start.p,2u);let gb=frame_point(end.p,2u);
    let global_padding=deviation+tolerance+2e-6*max(length(ga),length(gb));
    var index=root;let limit=bitcast<u32>(optical_geometry[root].w);
    var visited=0u;
    loop{
        if index>=limit{break;}visited+=1u;if visited>8192u{result.ambiguous=1u;return result;}
        let lo=optical_geometry[index];let hi=optical_geometry[index+1u];let escape=bitcast<u32>(lo.w);let payload=bitcast<u32>(hi.w);
        if !segment_box(ga,gb,lo.xyz,hi.xyz,global_padding){index=escape;continue;}
        if (payload&OPTICAL_LEAF)==0u{index+=2u;continue;}
        let region_id=payload&OPTICAL_ADDRESS;let base=6u+region_id*4u;
        let p=frame_point(start.p,base);let q=frame_point(end.p,base);
        let pad=deviation+tolerance+optical_frames[base+1u].w+2e-6*max(length(p),length(q));
        var node=bitcast<u32>(optical_frames[base].w);let stop=bitcast<u32>(optical_geometry[node].w);
        loop{
            if node>=stop{break;}visited+=1u;if visited>8192u{result.ambiguous=1u;return result;}
            let lower=optical_geometry[node];let upper=optical_geometry[node+1u];let next=bitcast<u32>(lower.w);let leaf=bitcast<u32>(upper.w);
            if !segment_box(p,q,lower.xyz,upper.xyz,pad){node=next;continue;}
            if (leaf&OPTICAL_LEAF)==0u{node+=2u;continue;}
            let address=leaf&OPTICAL_ADDRESS;let count=((leaf>>24u)&7u)+1u;
            for(var j=0u;j<count;j+=1u){let first=address+3u*j;
                result=combine_event(result,face_event(start,end,delta,step,optical_geometry[first].xyz,optical_geometry[first+1u].xyz,optical_geometry[first+2u].xyz,base,0u,region_id,tolerance,deviation));
            }
            node=next;
        }
        index=escape;
    }
    return result;
}
const BOX_TRIANGLES:array<vec3<u32>,12>=array<vec3<u32>,12>(vec3(0u,2u,1u),vec3(0u,3u,2u),vec3(4u,5u,6u),vec3(4u,6u,7u),vec3(0u,1u,5u),vec3(0u,5u,4u),vec3(1u,2u,6u),vec3(1u,6u,5u),vec3(2u,3u,7u),vec3(2u,7u,6u),vec3(3u,0u,4u),vec3(3u,4u,7u));
fn marker_events(start:State,end:State,delta:vec3<f32>,step:f32,tolerance:f32,deviation:f32)->SceneEvent{
    var result=empty_event();var node=bitcast<u32>(optical_frames[0].z);if node==0u{return result;}
    let stop=bitcast<u32>(optical_frames[node].w);let start_records=bitcast<u32>(optical_frames[1].x);
    let pad=deviation+tolerance+2e-6*max(length(start.p),length(end.p));var visited=0u;
    loop{
        if node>=stop{break;}visited+=1u;if visited>2048u{result.ambiguous=1u;return result;}
        let lo=optical_frames[node];let hi=optical_frames[node+1u];let next=bitcast<u32>(lo.w);let payload=bitcast<u32>(hi.w);
        if !segment_box(start.p,end.p,lo.xyz,hi.xyz,pad){node=next;continue;}
        if (payload&OPTICAL_LEAF)==0u{node+=2u;continue;}
        let marker=payload&OPTICAL_ADDRESS;let first=start_records+marker*8u;
        let base=optical_frames[first].xyz;let right=optical_frames[first+1u];let forward=optical_frames[first+2u];let up=optical_frames[first+3u];
        var corners:array<vec3<f32>,12>;
        for(var k=0u;k<8u;k+=1u){
            let sx=select(-1.0,1.0,k%4u==1u||k%4u==2u);let sy=select(-1.0,1.0,k%4u>=2u);
            corners[k]=base+right.xyz*(right.w*sx)+forward.xyz*(forward.w*sy)+up.xyz*select(0.0,up.w,k>=4u);
        }
        let center=optical_frames[first+4u];let sr=optical_frames[first+5u];let su=optical_frames[first+6u].xyz;
        let a=center.xyz-sr.xyz*(center.w*0.5)-su*(sr.w*0.5);let b=a+sr.xyz*center.w;let c=b+su*sr.w;let d=a+su*sr.w;
        corners[8]=a;corners[9]=b;corners[10]=c;corners[11]=d;
        // One call site retains the same twelve prism and two plate faces,
        // avoiding three copies of the costly refined event code in DX12.
        for(var k=0u;k<14u;k+=1u){
            var triangle=vec3(8u,10u,11u);var kind=2u;
            if k<12u{triangle=BOX_TRIANGLES[k];kind=1u;}else if k==12u{triangle=vec3(8u,9u,10u);}
            result=combine_event(result,face_event(start,end,delta,step,corners[triangle.x],corners[triangle.y],corners[triangle.z],0u,kind,marker,tolerance,deviation));
        }
        node=next;
    }
    return result;
}
fn scene_event(start:State,end:State,delta:vec3<f32>,step:f32,tolerance:f32)->SceneEvent{
    let deviation=settings.scene.x*step*step*0.125;
    return combine_event(terrain_events(start,end,delta,step,tolerance,deviation),marker_events(start,end,delta,step,tolerance,deviation));
}

// Coverage is actual triangle intersection on the geodetic normal line. It is
// not a rectangle or a categorical mask. Valid source heights lie[-10,100]km.
fn terrain_coverage(p:vec3<f32>,up:vec3<f32>)->u32{
    let root=bitcast<u32>(optical_frames[0].x);if root==0u{return 0u;}
    let a=p+up*101000.0;let b=p-up*10001.0;let ga=frame_point(a,2u);let gb=frame_point(b,2u);
    var index=root;let limit=bitcast<u32>(optical_geometry[root].w);var visited=0u;
    loop{
        if index>=limit{break;}visited+=1u;if visited>8192u{return 2u;}
        let lo=optical_geometry[index];let hi=optical_geometry[index+1u];let escape=bitcast<u32>(lo.w);let payload=bitcast<u32>(hi.w);
        if !segment_box(ga,gb,lo.xyz,hi.xyz,2e-6*max(length(ga),length(gb))){index=escape;continue;}
        if (payload&OPTICAL_LEAF)==0u{index+=2u;continue;}
        let base=6u+(payload&OPTICAL_ADDRESS)*4u;let x=frame_point(a,base);let y=frame_point(b,base);
        var node=bitcast<u32>(optical_frames[base].w);let stop=bitcast<u32>(optical_geometry[node].w);
        loop{
            if node>=stop{break;}visited+=1u;if visited>8192u{return 2u;}
            let lower=optical_geometry[node];let upper=optical_geometry[node+1u];let next=bitcast<u32>(lower.w);let leaf=bitcast<u32>(upper.w);
            if !segment_box(x,y,lower.xyz,upper.xyz,2e-6*max(length(x),length(y))){node=next;continue;}
            if (leaf&OPTICAL_LEAF)==0u{node+=2u;continue;}
            let address=leaf&OPTICAL_ADDRESS;let count=((leaf>>24u)&7u)+1u;
            for(var j=0u;j<count;j+=1u){let first=address+3u*j;let va=optical_geometry[first].xyz;let vb=optical_geometry[first+1u].xyz;let vc=optical_geometry[first+2u].xyz;
                let n=normalize(cross(vb-va,vc-va));let before=dot(n,x-va);let after=dot(n,y-va);
                if before*after<=0.0 && before!=after {let at=x+(y-x)*(before/(before-after));if triangle_contains(va,vb,vc,at,n){return 1u;}}
            }
            node=next;
        }
        index=escape;
    }
    return 0u;
}

// The original five-column glyphs, with their first six rows packed into
// successive five-bit fields. Avoid a dynamically indexed private array in
// the large optical material; the glyphs and cell ownership are unchanged.
fn sign_row(value:u32,row:u32)->u32{
    var rows=0u;var last=0u;
    switch value {
        case 0u:{rows=597347886u;last=14u;}
        case 1u:{rows=138547588u;last=14u;}
        case 2u:{rows=272696878u;last=31u;}
        case 3u:{rows=35062846u;last=30u;}
        case 4u:{rows=100214978u;last=2u;}
        case 5u:{rows=35602975u;last=30u;}
        case 6u:{rows=589251086u;last=14u;}
        case 7u:{rows=276957247u;last=8u;}
        case 8u:{rows=588727854u;last=14u;}
        case 9u:{rows=35112494u;last=14u;}
        default:{return 0u;}
    }
    if row==6u{return last;}
    if row>=7u{return 0u;}
    return (rows>>(row*5u))&31u;
}
fn sign_divisor(digit:u32)->u32{
    switch digit {
        case 0u:{return 1000u;}
        case 1u:{return 100u;}
        case 2u:{return 10u;}
        default:{return 1u;}
    }
}
fn sign_radiance(p:vec3<f32>,marker:u32)->vec3<f32>{
    let first=bitcast<u32>(optical_frames[1].x)+marker*8u;let center=optical_frames[first+4u];let right=optical_frames[first+5u];let up=optical_frames[first+6u].xyz;
    let q=p-center.xyz;let column=u32(clamp(floor((dot(q,right.xyz)/center.w+0.5)*25.0),0.0,24.0));let row=u32(clamp(floor((0.5-dot(q,up)/right.w)*9.0),0.0,8.0));
    if row==0u||row==8u||column==0u||column==24u{return vec3(0.0);}
    let offset=column-1u;let digit=offset/6u;let cell=offset%6u;if digit>=4u||cell>=5u{return vec3(0.0);}
    let number=bitcast<u32>(optical_frames[first].w)+1u;let value=(number/sign_divisor(digit))%10u;
    return vec3(select(0.0,1.0,(sign_row(value,row-1u)&(1u<<(4u-cell)))!=0u));
}
fn marker_radiance(p:vec3<f32>,normal:vec3<f32>,marker:u32,footprint:f32,direct:f32,fill:f32)->vec3<f32>{
    let first=bitcast<u32>(optical_frames[1].x)+marker*8u;let base=optical_frames[first].xyz;let up=optical_frames[first+3u];
    var height=dot(p-base,up.xyz)/0.3048;
    if abs(dot(normal,up.xyz))>0.999 {height=select(0.5,ceil(up.w/0.3048)-0.5,dot(normal,up.xyz)>0.0);}
    let width=max(0.0001,footprint/0.3048);let phase=height-floor(height/2.0)*2.0;
    var red=0.5;
    if width<1024.0 {let low=phase-width*0.5;let high=phase+width*0.5;
        let f0=floor(low/2.0)+min(low-floor(low/2.0)*2.0,1.0);let f1=floor(high/2.0)+min(high-floor(high/2.0)*2.0,1.0);
        red=clamp((f1-f0)/width,0.0,1.0);
    }
    return mix(vec3(0.035,0.55,0.065),vec3(0.75,0.018,0.012),red)*(0.08+1.35*direct+0.32*fill);
}
