// Original MIT. Isolated Cartesian certificate for the SAME stored physical
// quartic. No ODE, source ownership, contact predicate or budget is changed.
// Requires the reviewed map/interval helpers and NcCartesianPiece consumer.
// The caller owns the true-unit canonical axis and source residual metadata.
//
// Scalar factorial-jet majorants (orders 0..4): V bounds (a,1,-b), X=V*V,
// B bounds X^(-1/2), N=V*B, D bounds derivatives of 1-e*(k.N)^2, and R
// bounds aEarth/sqrt(D). For j>=1 the reciprocal-root recurrence is
// B_j <= sum_i (2*j-i)*X_i*B_(j-i)/(2*j*X_lower), and likewise R.
// X_lower>=1, true |N_0|=1, |k|=1 and D_lower>=1-e_upper are used only as
// exact geometric inequalities. The resulting scalar bounds ||P^(4)/4!||;
// each component receives that bound, conservatively charging all three.
//
// Basic positive operations widen one representable neighbor; quotients
// widen eight. All operands are normal or proved zero. Exponent preguards
// precede multiplication/division, invalid sentinels cannot be erased by a
// zero identity, and tiny results use the existing normal FTZ allowance.
// Polynomial jets and endpoint maps use the original outward intervals.
//
// A local copy extends theta_end to1 ONLY to certify the full represented
// polynomial on an aligned dyadic piece. The caller must still query/commit
// only its original accepted prefix. Failed extension-domain checks decline.
// The caller supplies fit_limit<=quarter remaining contact allocation.

fn nc_cp_positive(x:f32)->bool{
    return x==0.0||(x>=NC_I_MIN_NORMAL&&x<3.0e37);
}
fn nc_cp_add(a:f32,b:f32)->f32{
    if !nc_cp_positive(a)||!nc_cp_positive(b){return -1.0;}
    if a==0.0{return b;}if b==0.0{return a;}
    // a+b<6e37, so this operation cannot overflow finite f32.
    let result=nc_interval_up(a+b);
    if !nc_cp_positive(result){return -1.0;}return result;
}
fn nc_cp_mul(a:f32,b:f32)->f32{
    if !nc_cp_positive(a)||!nc_cp_positive(b){return -1.0;}
    if a==0.0||b==0.0{return 0.0;}
    if !nc_interval_product_safe(a,b){return -1.0;}
    let result=nc_interval_up(a*b);
    if !nc_cp_positive(result){return -1.0;}return result;
}
fn nc_cp_div(a:f32,b:f32)->f32{
    if !nc_cp_positive(a)||!nc_cp_positive(b)||!(b>0.0)
        ||!nc_interval_quotient_safe(a,b){return -1.0;}
    if a==0.0{return 0.0;}
    let result=nc_interval_up_steps(a/b,8u);
    if !nc_cp_positive(result){return -1.0;}return result;
}
fn nc_cp_abs(a:vec2<f32>)->f32{
    if !nc_interval_valid(a){return -1.0;}
    return nc_interval_abs_upper(a);
}
fn nc_cp_invalid()->NcCartesianPiece{
    var result:NcCartesianPiece;result.valid=0u;return result;
}

// All jets use ORIGINAL theta units, with factorial scaling. Direct expanded
// powers for orders2..4 avoid a vector interval reciprocal-root jet graph.
fn nc_cp_coordinate_jets(curve:NcSceneCurve,t:vec2<f32>)->array<NcMapVec,5>{
    let c=curve.physical;
    let c3=nc_m_exact(c.q3);let c4=nc_m_exact(c.q4);let c5=nc_m_exact(c.q5);
    let a2=nc_m_add(nc_m_sub(c4,c3),c5);
    let a3=nc_m_sub(nc_m_scale(c4,vec2(-1.0)),nc_m_scale(c5,vec2(2.0)));
    var jets:array<NcMapVec,5>;
    jets[0]=nc_m_poly(curve.start.q,c.q2,c.q3,c.q4,c.q5,t);
    jets[1]=nc_m_poly_derivative(c.q2,c.q3,c.q4,c.q5,t);
    jets[2]=nc_m_add(a2,nc_m_scale(nc_m_add(nc_m_scale(a3,vec2(3.0)),nc_m_scale(c5,nc_f_mul(vec2(6.0),t))),t));
    jets[3]=nc_m_add(a3,nc_m_scale(c5,nc_f_mul(vec2(4.0),t)));
    jets[4]=c5;
    return jets;
}
fn nc_cp_fourth_upper(curve:NcSceneCurve,parameters:NcMapParameters,range:vec2<f32>)->f32{
    if curve.valid==0u||!nc_interval_valid(range)||range.x<0.0||range.y>1.0
        ||!nc_interval_valid(parameters.eccentricity)||parameters.eccentricity.x<0.0
        ||!(parameters.eccentricity.y<0.007)||!nc_m_valid(parameters.axis)
        ||!nc_interval_valid(parameters.anchor_residual){return -1.0;}
    let jets=nc_cp_coordinate_jets(curve,range);
    var v:array<f32,5>;var height:array<f32,5>;
    for(var j=0u;j<5u;j++){
        if !nc_m_valid(jets[j]){return -1.0;}
        v[j]=nc_cp_add(nc_cp_abs(jets[j].y),nc_cp_abs(jets[j].z));
        height[j]=nc_cp_abs(jets[j].x);
        if !nc_cp_positive(v[j])||!nc_cp_positive(height[j]){return -1.0;}
    }
    if !(v[0]<=1.0){return -1.0;}
    let anchor=nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),parameters.anchor_residual);
    let h=nc_f_add(anchor,jets[0].x);
    // Conservative supported normal tube. Subdivision cannot waive it.
    if !nc_interval_valid(h)||h.x< -10000.0||h.y>120001.0{return -1.0;}
    height[0]=nc_cp_abs(h);v[0]=2.0;
    var xx:array<f32,5>;var inv:array<f32,5>;inv[0]=1.0;
    var normal:array<f32,5>;normal[0]=1.0;
    for(var j=1u;j<5u;j++){
        var sum=0.0;
        for(var i=0u;i<=j;i++){sum=nc_cp_add(sum,nc_cp_mul(v[i],v[j-i]));}
        xx[j]=sum;sum=0.0;
        for(var i=1u;i<=j;i++){sum=nc_cp_add(sum,nc_cp_mul(nc_cp_mul(f32(2u*j-i),xx[i]),inv[j-i]));}
        inv[j]=nc_cp_div(sum,f32(2u*j));sum=0.0;
        for(var i=0u;i<=j;i++){sum=nc_cp_add(sum,nc_cp_mul(v[i],inv[j-i]));}
        normal[j]=sum;
        if !nc_cp_positive(xx[j])||!nc_cp_positive(inv[j])||!nc_cp_positive(normal[j]){return -1.0;}
    }
    let e=parameters.eccentricity.y;let d=nc_f_sub(vec2(1.0),vec2(e));
    let reciprocal_root=nc_m_rsqrt(d);
    if !nc_interval_valid(d)||!(d.x>0.0)||!nc_interval_valid(reciprocal_root){return -1.0;}
    var dd:array<f32,5>;var radius:array<f32,5>;
    radius[0]=nc_cp_mul(6378137.0,reciprocal_root.y);
    if !nc_cp_positive(radius[0]){return -1.0;}
    for(var j=1u;j<5u;j++){
        var sum=0.0;
        for(var i=0u;i<=j;i++){sum=nc_cp_add(sum,nc_cp_mul(normal[i],normal[j-i]));}
        dd[j]=nc_cp_mul(e,sum);sum=0.0;
        for(var i=1u;i<=j;i++){sum=nc_cp_add(sum,nc_cp_mul(nc_cp_mul(f32(2u*j-i),dd[i]),radius[j-i]));}
        let denominator=nc_f_mul(vec2(f32(2u*j)),d);
        if !nc_interval_valid(denominator)||!(denominator.x>0.0){return -1.0;}
        radius[j]=nc_cp_div(sum,denominator.x);
        if !nc_cp_positive(dd[j])||!nc_cp_positive(radius[j]){return -1.0;}
    }
    var surface=0.0;var offset=0.0;
    for(var i=0u;i<5u;i++){
        surface=nc_cp_add(surface,nc_cp_mul(radius[i],normal[4u-i]));
        offset=nc_cp_add(offset,nc_cp_mul(height[i],normal[4u-i]));
    }
    return nc_cp_add(nc_cp_mul(nc_cp_add(1.0,e),surface),offset);
}

// A finite midpoint is merely a chosen exact represented origin. Its entire
// endpoint interval is subtracted below; midpoint rounding is not discarded.
fn nc_cp_midpoint(a:vec2<f32>)->f32{return a.x*0.5+a.y*0.5;}
fn nc_cp_control_radius(a:NcMapVec)->f32{
    if !nc_m_valid(a){return -1.0;}
    let center=vec3(nc_cp_midpoint(a.x),nc_cp_midpoint(a.y),nc_cp_midpoint(a.z));
    if !nc_finite3(center){return -1.0;}
    let difference=nc_m_sub(a,nc_m_exact(center));
    return nc_cp_add(nc_cp_add(nc_cp_abs(difference.x),nc_cp_abs(difference.y)),nc_cp_abs(difference.z));
}
fn nc_cartesian_certify(curve:NcSceneCurve,parameters:NcMapParameters,range:vec2<f32>,fit_limit:f32)->NcCartesianPiece{
    if curve.valid==0u||!nc_interval_valid(range)||!(range.x>=0.0&&range.y<=1.0&&range.y>range.x)
        ||!nc_cp_positive(fit_limit)||!(fit_limit>0.0){return nc_cp_invalid();}
    // Require exact binary lattice endpoints. Multiplication by2^depth is
    // exact for these normal values; scaled endpoints are <=2^24 integers.
    var inverse_span=1.0;var found=false;
    for(var depth=0u;depth<=24u;depth++){
        let left=range.x*inverse_span;let right=range.y*inverse_span;
        if left==floor(left)&&right==floor(right)&&right-left==1.0{found=true;break;}
        inverse_span=inverse_span*2.0;
    }
    if !found{return nc_cp_invalid();}
    // Reciprocal of an admitted normal power of two is exact and normal.
    // Construct from exponent bits, avoiding WGSL's approximate division.
    let inverse_exponent=(bitcast<u32>(inverse_span)>>23u)&255u;
    let span=bitcast<f32>((254u-inverse_exponent)<<23u);
    let fourth=nc_cp_fourth_upper(curve,parameters,range);
    if !nc_cp_positive(fourth){return nc_cp_invalid();}
    let width2=nc_cp_mul(span,span);
    let fit=nc_cp_mul(nc_cp_mul(fourth,nc_cp_mul(width2,width2)),0.0625);
    let derivative_fit=nc_cp_div(nc_cp_mul(nc_cp_mul(nc_cp_mul(fourth,width2),span),10.0),3.0);
    if !nc_cp_positive(fit)||!nc_cp_positive(derivative_fit){return nc_cp_invalid();}
    var full_curve=curve;full_curve.theta_end=1.0;
    let left=nc_map_curve_point(full_curve,vec2(range.x),parameters);
    let right=nc_map_curve_point(full_curve,vec2(range.y),parameters);
    if left.valid==0u||right.valid==0u{return nc_cp_invalid();}
    let origin=vec3(nc_cp_midpoint(left.position.x),nc_cp_midpoint(left.position.y),nc_cp_midpoint(left.position.z));
    if !nc_finite3(origin){return nc_cp_invalid();}
    let local_left=nc_m_sub(left.position,nc_m_exact(origin));
    let local_right=nc_m_sub(right.position,nc_m_exact(origin));
    let third_span=nc_f_div(vec2(span),vec2(3.0));
    var piece:NcCartesianPiece;
    piece.origin_hi=origin;piece.origin_lo=vec3(0.0);
    piece.p0=local_left;piece.p1=nc_m_add(local_left,nc_m_scale(left.velocity,third_span));
    piece.p2=nc_m_sub(local_right,nc_m_scale(right.velocity,third_span));piece.p3=local_right;
    piece.d0=left.velocity;piece.d2=right.velocity;
    piece.d1=nc_m_sub(nc_m_sub(nc_m_scale(nc_m_sub(local_right,local_left),nc_f_mul(vec2(3.0),vec2(inverse_span))),left.velocity),right.velocity);
    if !nc_m_valid(piece.p0)||!nc_m_valid(piece.p1)||!nc_m_valid(piece.p2)||!nc_m_valid(piece.p3)
        ||!nc_m_valid(piece.d0)||!nc_m_valid(piece.d1)||!nc_m_valid(piece.d2){return nc_cp_invalid();}
    let r0=nc_cp_control_radius(piece.p0);let r1=nc_cp_control_radius(piece.p1);
    let r2=nc_cp_control_radius(piece.p2);let r3=nc_cp_control_radius(piece.p3);
    if !nc_cp_positive(r0)||!nc_cp_positive(r1)||!nc_cp_positive(r2)||!nc_cp_positive(r3){return nc_cp_invalid();}
    let total=nc_cp_add(max(max(r0,r1),max(r2,r3)),nc_cp_mul(3.0,fit));
    if !nc_cp_positive(total)||!(total<=fit_limit){return nc_cp_invalid();}
    piece.position_remainder=vec3(fit);piece.derivative_remainder=vec3(derivative_fit);
    piece.parameter=vec4(range,inverse_span,total);piece.valid=1u;
    return piece;
}
