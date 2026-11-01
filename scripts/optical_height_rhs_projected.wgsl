// Original MIT. Unused algebraic height-field diagnostic; no runtime adoption.
// This projects the canonical shape operator into gnomonic coordinates BEFORE
// evaluation. Real-number equations equal nc_rhs(state)/vertical. Floating
// operation order differs and requires the unchanged independent ray oracles.
// The diagnostic injector renames original nh_rhs to nh_rhs_reference.
fn nh_rhs_projected_try(state:NcState,orientation:f32)->NhDerivative{
    let invalid=NhDerivative(vec3(0.0),vec3(0.0),0.0,0u);
    if !nc_finite3(state.q)||!nc_finite3(state.u)||dot(state.q.yz,state.q.yz)>1.0{return invalid;}
    let h=nc_height(state.q);
    // Bounded fast domain is the existing supported atmospheric tube. Inputs
    // outside this operational domain defer to the original general evaluator.
    if !(h>=-10000.0 && h<=120001.0)||any(abs(state.u)>vec3(2.0)){return invalid;}
    let refractivity=nc_index.x+nc_index.y*state.q.x;
    let index=1.0+refractivity;
    if !nc_finite(index)||index<0.5{return invalid;}
    let n=nc_normal(state.q);
    if !nc_finite3(n)||!(n.y>0.0){return invalid;}
    let vertical=dot(state.u,n);
    if !(orientation*vertical>0.01)||!(abs(vertical)<1.0e8){return invalid;}
    let z=dot(n,nc_axis);let d=1.0-NC_E2*z*z;
    if !(d>0.0)||!nc_finite(d){return invalid;}
    let root=sqrt(d);
    // D=root*(N+h), J=d*root*(M+h). Positivity is exactly the
    // canonical normal-tube condition, without constructing N and M separately.
    let d_radius=NC_A+h*root;
    let d_meridian=NC_A*(1.0-NC_E2)+h*d*root;
    if !(d_radius>0.0 && d_meridian>0.0)||!nc_finite(d_radius)||!nc_finite(d_meridian){return invalid;}
    let inverse_vertical=1.0/vertical;
    let ratio=(NC_A*NC_E2)/d_meridian;
    let scale=(root*inverse_vertical)/(n.y*d_radius);
    let chart=state.q.yz;
    let projected_u=vec2(state.u.x-chart.x*state.u.y,-state.u.z-chart.y*state.u.y);
    let projected_axis=vec2(nc_axis.x-chart.x*nc_axis.y,-nc_axis.z-chart.y*nc_axis.y);
    let meridian_dot=dot(nc_axis,state.u)-z*vertical;
    let chart_derivative=scale*(projected_u+(ratio*meridian_dot)*projected_axis);
    let q=vec3(inverse_vertical,chart_derivative);
    let u=(n*inverse_vertical-state.u)*(nc_index.y/index);
    let excess=refractivity*inverse_vertical;
    if !nc_finite3(q)||!nc_finite3(u)||!nc_finite(excess){return invalid;}
    return NhDerivative(q,u,excess,1u);
}
fn nh_rhs_projected(state:NcState,orientation:f32)->NhDerivative{
    let candidate=nh_rhs_projected_try(state,orientation);
    if candidate.valid!=0u{return candidate;}
    return nh_rhs_reference(state,orientation);
}
