// Original MIT. Isolated normal-coordinate solver diagnostic; not registered.
// Scalar intervals for the exact polynomial represented by finite f32 inputs.
// One outward representable neighbor encloses either correctly-rounded result
// of a basic operation. Division uses eight representable steps to cover its
// wider WGSL 2.5-ULP allowance across a binade boundary.
// Tiny results use normal bounds so flush-to-zero cannot erase an allowance.
// This does not bound the physical ODE truncation error or coordinate packing.
// Construct external operands with nc_interval_exact: even a zero input is
// widened because it may have been flushed. nc_interval_zero is reserved for
// an authored zero constant or a separately proved exact algebraic identity.
const NC_I_MIN_NORMAL:f32=1.1754943508222875e-38;
const NC_I_TINY_BOUND:f32=2.350988701644575e-38;
fn nc_interval_invalid()->vec2<f32>{return vec2(1.0,-1.0);}
fn nc_interval_zero()->vec2<f32>{return vec2(0.0);}
fn nc_interval_valid(a:vec2<f32>)->bool{
    return a.x<=a.y && all(abs(a)<vec2(3.0e37));
}
fn nc_interval_down_steps(x:f32,steps:u32)->f32{
    let bits=bitcast<u32>(x);let magnitude=bits&0x7fffffffu;
    if magnitude<=0x00800000u+steps{return -NC_I_TINY_BOUND;}
    if (bits&0x80000000u)!=0u{return bitcast<f32>(bits+steps);}
    return bitcast<f32>(bits-steps);
}
fn nc_interval_up_steps(x:f32,steps:u32)->f32{
    let bits=bitcast<u32>(x);let magnitude=bits&0x7fffffffu;
    if magnitude<=0x00800000u+steps{return NC_I_TINY_BOUND;}
    if (bits&0x80000000u)!=0u{return bitcast<f32>(bits-steps);}
    return bitcast<f32>(bits+steps);
}
fn nc_interval_down(x:f32)->f32{return nc_interval_down_steps(x,1u);}
fn nc_interval_up(x:f32)->f32{return nc_interval_up_steps(x,1u);}
fn nc_interval_product_safe(a:f32,b:f32)->bool{
    if a==0.0 || b==0.0{return true;}
    let ea=(bitcast<u32>(a)&0x7fffffffu)>>23u;
    let eb=(bitcast<u32>(b)&0x7fffffffu)>>23u;
    // |a*b| < 2^(ea+eb-252) <= 2^126; no overflowing operation is issued.
    return ea+eb<=378u;
}
fn nc_interval_quotient_safe(a:f32,b:f32)->bool{
    let mb=bitcast<u32>(b)&0x7fffffffu;
    // This is the denominator domain of the WGSL 2.5-ULP contract.
    if mb<0x00800000u || mb>0x7e800000u{return false;}
    if a==0.0{return true;}
    let ea=i32((bitcast<u32>(a)&0x7fffffffu)>>23u);let eb=i32(mb>>23u);
    return ea-eb<=125;
}
fn nc_interval_sterbenz(a:f32,b:f32,difference:f32)->bool{
    // Exact subtraction of nearby same-sign binary floats. Returning an
    // observed zero is safe only for equal operands: unequal operands could
    // have produced a flushed subnormal. The normal result is exact under
    // either correctly-rounded direction, so no generic ULP widening applies.
    if a==b{return true;}
    let aa=abs(a);let bb=abs(b);
    return ((a>0.0 && b>0.0)||(a<0.0 && b<0.0))
        && min(aa,bb)>=NC_I_MIN_NORMAL && max(aa,bb)<=2.0*min(aa,bb)
        && abs(difference)>=NC_I_MIN_NORMAL;
}
fn nc_interval_exact(x:f32)->vec2<f32>{
    if !(abs(x)<3.0e37){return nc_interval_invalid();}
    if abs(x)<NC_I_MIN_NORMAL{return vec2(-NC_I_TINY_BOUND,NC_I_TINY_BOUND);}
    return vec2(x);
}
fn nc_interval_add(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!nc_interval_valid(b){return nc_interval_invalid();}
    if all(a==vec2(0.0)){return b;}
    if all(b==vec2(0.0)){return a;}
    let lo=a.x+b.x;let hi=a.y+b.y;
    if !(abs(lo)<3.0e37 && abs(hi)<3.0e37){return nc_interval_invalid();}
    var lower=nc_interval_down(lo);var upper=nc_interval_up(hi);
    if nc_interval_sterbenz(a.x,-b.x,lo){lower=lo;}
    if nc_interval_sterbenz(a.y,-b.y,hi){upper=hi;}
    return vec2(lower,upper);
}
fn nc_interval_sub(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!nc_interval_valid(b){return nc_interval_invalid();}
    if all(b==vec2(0.0)){return a;}
    if a.x==a.y && b.x==b.y && a.x==b.x{return vec2(0.0);}
    let lo=a.x-b.y;let hi=a.y-b.x;
    if !(abs(lo)<3.0e37 && abs(hi)<3.0e37){return nc_interval_invalid();}
    var lower=nc_interval_down(lo);var upper=nc_interval_up(hi);
    if nc_interval_sterbenz(a.x,b.y,lo){lower=lo;}
    if nc_interval_sterbenz(a.y,b.x,hi){upper=hi;}
    return vec2(lower,upper);
}
fn nc_interval_mul(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!nc_interval_valid(b){return nc_interval_invalid();}
    if all(a==vec2(0.0))||all(b==vec2(0.0)){return vec2(0.0);}
    if !nc_interval_product_safe(a.x,b.x)||!nc_interval_product_safe(a.x,b.y)||!nc_interval_product_safe(a.y,b.x)||!nc_interval_product_safe(a.y,b.y){return nc_interval_invalid();}
    let p=vec4(a.x*b.x,a.x*b.y,a.y*b.x,a.y*b.y);
    if !all(abs(p)<vec4(3.0e37)){return nc_interval_invalid();}
    let lo=min(min(p.x,p.y),min(p.z,p.w));let hi=max(max(p.x,p.y),max(p.z,p.w));
    return vec2(nc_interval_down(lo),nc_interval_up(hi));
}
fn nc_interval_div(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!nc_interval_valid(b)||(b.x<=0.0 && b.y>=0.0){return nc_interval_invalid();}
    if all(a==vec2(0.0)){return vec2(0.0);}
    if !nc_interval_quotient_safe(a.x,b.x)||!nc_interval_quotient_safe(a.x,b.y)||!nc_interval_quotient_safe(a.y,b.x)||!nc_interval_quotient_safe(a.y,b.y){return nc_interval_invalid();}
    let p=vec4(a.x/b.x,a.x/b.y,a.y/b.x,a.y/b.y);
    if !all(abs(p)<vec4(3.0e37)){return nc_interval_invalid();}
    let lo=min(min(p.x,p.y),min(p.z,p.w));let hi=max(max(p.x,p.y),max(p.z,p.w));
    return vec2(nc_interval_down_steps(lo,8u),nc_interval_up_steps(hi,8u));
}
fn nc_interval_abs_upper(a:vec2<f32>)->f32{
    if !nc_interval_valid(a){return 3.0e37;}
    return max(abs(a.x),abs(a.y));
}
fn nc_interval_sign(a:vec2<f32>)->i32{
    if !nc_interval_valid(a){return 0;}
    if a.x>0.0{return 1;}
    if a.y<0.0{return -1;}
    return 0;
}
