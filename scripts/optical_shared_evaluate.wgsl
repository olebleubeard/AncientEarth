// Original MIT. Unregistered finite evaluation of ONE certified bivariate
// cubic. Coefficient intervals enclose the original stored-binary64 values;
// x/y intervals enclose the exact native-pixel coordinates in [-1,1].
// Requires the reviewed nc_interval_* basic arithmetic. This helper neither
// generates the shared transport nor certifies scene visibility.
struct NsPolynomial {
    c0:vec2<f32>,c1:vec2<f32>,c2:vec2<f32>,c3:vec2<f32>,c4:vec2<f32>,
    c5:vec2<f32>,c6:vec2<f32>,c7:vec2<f32>,c8:vec2<f32>,c9:vec2<f32>,
};
struct NsEvaluated {value:f32,error:f32,valid:u32};
fn ns_polynomial_interval(p:NsPolynomial,x:vec2<f32>,y:vec2<f32>)->vec2<f32>{
    if !nc_interval_valid(x)||!nc_interval_valid(y)||x.x< -1.0||x.y>1.0||y.x< -1.0||y.y>1.0{return nc_interval_invalid();}
    // Powers: 1,x,y,x²,xy,y²,x³,x²y,xy²,y³. Each coefficient
    // participates once; source c0 is added only at the final operation.
    let xx=nc_interval_add(p.c3,nc_interval_add(nc_interval_mul(x,p.c6),nc_interval_mul(y,p.c7)));
    let xy=nc_interval_add(p.c4,nc_interval_mul(y,p.c8));
    let xpart=nc_interval_mul(x,nc_interval_add(p.c1,nc_interval_add(nc_interval_mul(x,xx),nc_interval_mul(y,xy))));
    let ypart=nc_interval_mul(y,nc_interval_add(p.c2,nc_interval_mul(y,nc_interval_add(p.c5,nc_interval_mul(y,p.c9)))));
    return nc_interval_add(p.c0,nc_interval_add(xpart,ypart));
}
fn ns_polynomial_evaluate(p:NsPolynomial,x:vec2<f32>,y:vec2<f32>)->NsEvaluated{
    let enclosure=ns_polynomial_interval(p,x,y);
    if !nc_interval_valid(enclosure){return NsEvaluated(0.0,0.0,0u);}
    // Publication uses a represented scalar and a separately retained error.
    // Midpoint rounding is covered by the subtraction intervals; it is not
    // silently assumed exact or used to erase the coefficient enclosure.
    let value=enclosure.x*0.5+enclosure.y*0.5;
    if !(abs(value)<3.0e37){return NsEvaluated(0.0,0.0,0u);}
    let difference=nc_interval_sub(enclosure,nc_interval_exact(value));
    if !nc_interval_valid(difference){return NsEvaluated(0.0,0.0,0u);}
    return NsEvaluated(value,nc_interval_abs_upper(difference),1u);
}
