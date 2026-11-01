// Original MIT. Diagnostic exact vertex expression reuse; no local array.
// Every index follows the same f32 operation order as marker_events corners.
fn marker_corner(first:u32,index:u32)->vec3<f32>{
    if index<8u{
        let base=optical_frames[first].xyz;let right=optical_frames[first+1u];let forward=optical_frames[first+2u];let up=optical_frames[first+3u];
        let sx=select(-1.0,1.0,index%4u==1u||index%4u==2u);let sy=select(-1.0,1.0,index%4u>=2u);
        return base+right.xyz*(right.w*sx)+forward.xyz*(forward.w*sy)+up.xyz*select(0.0,up.w,index>=4u);
    }
    let center=optical_frames[first+4u];let sr=optical_frames[first+5u];let su=optical_frames[first+6u].xyz;
    let a=center.xyz-sr.xyz*(center.w*0.5)-su*(sr.w*0.5);
    if index==8u{return a;}
    let b=a+sr.xyz*center.w;
    if index==9u{return b;}
    if index==10u{return b+su*sr.w;}
    return a+su*sr.w;
}
