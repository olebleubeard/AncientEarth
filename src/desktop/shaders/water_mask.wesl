// Exact native categorical posts plus declared coarse global cartographic context.
// Buffer addresses, run endpoints and categories remain u32; bounded coordinate
// anchors are exactly representable f32 integers with separate residuals.
struct ClassificationSettings {native_origin:vec4<f32>,global_origin:vec4<f32>,control:vec4<u32>};
fn floor_divide(a:i32,b:i32)->i32 {let q=a/b;return select(q,q-1,a%b<0);}
fn positive_modulo(a:i32,b:i32)->i32 {return ((a%b)+b)%b;}
fn native_width(south:i32)->i32 {
    let twice_latitude=abs(2*south+1);
    if twice_latitude<100 {return 3600;}
    if twice_latitude<120 {return 2400;}
    if twice_latitude<140 {return 1800;}
    if twice_latitude<160 {return 1200;}
    if twice_latitude<170 {return 720;}
    return 360;
}
fn native_category(angles:vec2<f32>,settings:ClassificationSettings)->u32 {
    let base=settings.native_origin;
    let latitude=i32(base.x)+i32(floor(base.y+angles.x*206264.80624709636+0.5));
    let south=floor_divide(latitude-1,3600);
    if south< -90 || south>=90 {return 255u;}
    let row=u32((south+1)*3600-latitude);
    let width=native_width(south);let step=7200/width;
    let quotient=floor_divide(i32(base.z),step);let remainder=i32(base.z)-quotient*step;
    let k=quotient+i32(floor((f32(remainder)+base.w+angles.y*412529.6124941927)/f32(step)+0.5));
    let units=positive_modulo(k*step+1296000,2592000)-1296000;
    let west=floor_divide(units,7200);let column=u32((units-west*7200)/step);
    let header=native_water[u32(south+90)*360u+u32(west+180)];
    if header==0u || native_water[header]!=u32(width) {return 255u;}
    var lo=native_water[header+1u+row];let end=native_water[header+2u+row];var hi=end;
    for(var i=0;i<16 && lo<hi;i+=1){let mid=lo+(hi-lo)/2u;
        if column<(native_water[mid]>>2u) {hi=mid;}else{lo=mid+1u;}}
    if lo>=end {return 255u;}return (native_water[lo]&3u);
}
fn global_category(angles:vec2<f32>,settings:ClassificationSettings)->u32 {
    if global_land[0]==0xffffffffu {return vector_category(angles,settings);}
    let width=global_land[0];let height=global_land[1];if width==0u || height==0u {return 255u;}
    let base=settings.global_origin;
    let row=u32(clamp(i32(base.x)+i32(floor(base.y-angles.x*f32(height)/3.141592653589793)),0,i32(height)-1));
    let column=u32(positive_modulo(i32(base.z)+i32(floor(base.w+angles.y*f32(width)/6.283185307179586)),i32(width)));
    var lo=global_land[2u+row];let end=global_land[3u+row];var hi=end;
    for(var i=0;i<16 && lo<hi;i+=1){let mid=lo+(hi-lo)/2u;
        if column<(global_land[mid]>>2u) {hi=mid;}else{lo=mid+1u;}}
    if lo>=end {return 255u;}return (global_land[lo]&3u);
}
fn global_cross(a:vec2<f32>,b:vec2<f32>)->f32 {return a.x*b.y-a.y*b.x;}
fn vector_category(angles:vec2<f32>,settings:ClassificationSettings)->u32 {
    let base=settings.global_origin;
    let residual=vec2(base.w,base.y)+angles.yx*57.29577951308232;
    let whole=floor(residual);
    let column=u32(positive_modulo(i32(base.z)+i32(whole.x),360));
    var row=i32(base.x)+i32(whole.y);var point=residual-whole;
    if row>=180 {row=179;point.y=1.0;}
    if row<0 {row=0;point.y=0.0;}
    var node=u32(row)*360u+column;
    // Source parser bounds the forest to20 child descents and32edges per leaf.
    for(var depth=0;depth<=20;depth+=1) {
        let record=3u+node*8u;let child=global_land[record];
        if child!=0u {
            let quadrant=vec2(select(0u,1u,point.x>=0.5),select(0u,1u,point.y>=0.5));
            point=point*2.0-vec2<f32>(quadrant);
            node=child+quadrant.x+2u*quadrant.y;continue;
        }
        var winding=bitcast<i32>(global_land[record+3u]);
        var holes=bitcast<i32>(global_land[record+4u]);
        let seed=vec2(bitcast<f32>(global_land[record+5u]),bitcast<f32>(global_land[record+6u]));
        let direction=point-seed;let extent=dot(direction,direction);
        let start=global_land[record+1u];let count=global_land[record+2u];
        if extent>0.0 {
            for(var i=0u;i<count;i+=1u) {
                let edge=start+i*5u;
                let a=vec2(bitcast<f32>(global_land[edge]),bitcast<f32>(global_land[edge+1u]))-seed;
                let b=vec2(bitcast<f32>(global_land[edge+2u]),bitcast<f32>(global_land[edge+3u]))-seed;
                let ya=global_cross(direction,a);let yb=global_cross(direction,b);
                if (ya>0.0)!=(yb>0.0) {
                    let t=(dot(direction,a)*yb-dot(direction,b)*ya)/(yb-ya);
                    if t>0.0 && t<extent {
                        let change=select(-1,1,ya>yb);winding+=change;
                        if global_land[edge+4u]!=0u {holes+=change;}
                    }
                }
            }
        }
        if winding<0 {return 0u;}if holes>0 {return 2u;}return 1u;
    }
    return 255u;
}
fn additional_water(angles:vec2<f32>,settings:ClassificationSettings)->bool {
    if settings.control.x==0u {return false;}
    var code=native_category(angles,settings);
    if code==255u {code=global_category(angles,settings);}
    return code>=1u && code<=3u;
}
