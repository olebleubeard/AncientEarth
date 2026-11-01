// Original MIT. Isolated diagnostic: an operand-domain fast path for the
// existing interval controller. Requires optical_normal_residual_fast.wgsl.
// These operations enclose the same represented arithmetic, with unchanged
// outward-neighbor and FTZ policies. They do not change ODE/contact budgets.
const NC_F_ADD_LIMIT:f32=0x1p120f;
const NC_F_MUL_LIMIT:f32=0x1p60f;
fn nc_f_add(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    // Ordered comparisons reject NaNs. At most 2^121 is produced; this is
    // well inside the original helper's 3e37 finite domain.
    if a.x<=a.y && b.x<=b.y
        && all(abs(a)<=vec2(NC_F_ADD_LIMIT)) && all(abs(b)<=vec2(NC_F_ADD_LIMIT)){
        return nc_rf_add(a,b);
    }
    return nc_interval_add(a,b);
}
fn nc_f_sub(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if a.x<=a.y && b.x<=b.y
        && all(abs(a)<=vec2(NC_F_ADD_LIMIT)) && all(abs(b)<=vec2(NC_F_ADD_LIMIT)){
        return nc_rf_sub(a,b);
    }
    return nc_interval_sub(a,b);
}
fn nc_f_mul(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    // Every endpoint product has magnitude <=2^120, so four separate exponent
    // preguards and a result-finiteness scan are redundant in this domain.
    if a.x<=a.y && b.x<=b.y
        && all(abs(a)<=vec2(NC_F_MUL_LIMIT)) && all(abs(b)<=vec2(NC_F_MUL_LIMIT)){
        return nc_rf_mul(a,b);
    }
    return nc_interval_mul(a,b);
}
fn nc_f_div(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(a)||!nc_interval_valid(b)||(b.x<=0.0 && b.y>=0.0){return nc_interval_invalid();}
    if all(a==vec2(0.0)){return vec2(0.0);}
    var numerator=a;var denominator=b;
    if b.y<0.0{numerator=-a.yx;denominator=-b.yx;}
    // With denominator>0, each exact extreme is selected by the sign of its
    // numerator. No interior ratio can lie outside these two extrema.
    let lower_denominator=select(denominator.y,denominator.x,numerator.x<0.0);
    let upper_denominator=select(denominator.x,denominator.y,numerator.y<0.0);
    if !nc_interval_quotient_safe(numerator.x,lower_denominator)
        ||!nc_interval_quotient_safe(numerator.y,upper_denominator){return nc_interval_invalid();}
    let lo=numerator.x/lower_denominator;
    var hi=lo;
    if numerator.x!=numerator.y || lower_denominator!=upper_denominator{
        hi=numerator.y/upper_denominator;
    }
    if !(abs(lo)<3.0e37 && abs(hi)<3.0e37){return nc_interval_invalid();}
    return vec2(nc_interval_down_steps(lo,8u),nc_interval_up_steps(hi,8u));
}
