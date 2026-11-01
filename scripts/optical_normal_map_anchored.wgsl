// Original MIT. UNUSED face-relative canonical map diagnostic.
// Requires optical_normal_map_interval.wgsl and its reviewed arithmetic.
// The anchor is a REAL normal-coordinate point, represented by split chart/H
// values plus outward residuals. mapped_minus_vertex encloses the difference
// between that canonical mapped point and the actual source triangle vertex A,
// expressed in the original eye basis. This host certificate is mandatory;
// an approximate inverse/geodetic conversion is not silently treated as exact.
// Output is canonical curve point minus source A, in the original eye basis.
// Therefore a face consumer rotates it but MUST NOT add eye-minus-A again.
struct NcMapAnchor {
    chart_hi:vec2<f32>, chart_lo:vec2<f32>,
    chart_x_residual:vec2<f32>, chart_y_residual:vec2<f32>,
    height:vec2<f32>,height_residual:vec2<f32>,
    mapped_minus_vertex:NcMapVec,
};
fn nc_map_curve_at_anchor(curve:NcSceneCurve,theta:vec2<f32>,parameters:NcMapParameters,anchor:NcMapAnchor)->NcMapPoint{
    let failure=NcMapPoint(nc_m_invalid(),nc_m_invalid(),0u);
    if curve.valid==0u||!nc_interval_valid(theta)||!(theta.x>=0.0 && theta.y<=curve.theta_end)
        ||!nc_m_valid(parameters.axis)||!nc_interval_valid(parameters.eccentricity)
        ||parameters.eccentricity.x<0.0||parameters.eccentricity.y>0.007
        ||!nc_interval_valid(parameters.anchor_residual)||!nc_interval_valid(anchor.height_residual)
        ||!nc_interval_valid(anchor.chart_x_residual)||!nc_interval_valid(anchor.chart_y_residual)
        ||!nc_m_valid(anchor.mapped_minus_vertex)
        ||!all(abs(anchor.chart_hi)<vec2(3.0e37))||!all(abs(anchor.chart_lo)<vec2(3.0e37))
        ||!all(abs(anchor.height)<vec2(3.0e37)){return failure;}
    let c=curve.physical;
    // Subtract the leading chart anchor BEFORE adding a dense increment. This
    // retains near-face bits even when the original-eye chart is far from zero.
    // Unlike subtracting two independently rounded nc_position calls, every
    // operation encloses the same represented dense polynomial and real anchor.
    let initial=NcMapVec(nc_interval_exact(curve.start.q.x),
        nc_f_sub(nc_f_sub(nc_f_sub(nc_interval_exact(curve.start.q.y),nc_interval_exact(anchor.chart_hi.x)),nc_interval_exact(anchor.chart_lo.x)),anchor.chart_x_residual),
        nc_f_sub(nc_f_sub(nc_f_sub(nc_interval_exact(curve.start.q.z),nc_interval_exact(anchor.chart_hi.y)),nc_interval_exact(anchor.chart_lo.y)),anchor.chart_y_residual));
    let polynomial=nc_m_poly(vec3(0.0),c.q2,c.q3,c.q4,c.q5,theta);
    let delta=nc_m_add(initial,polynomial);
    let dq=nc_m_poly_derivative(c.q2,c.q3,c.q4,c.q5,theta);
    let a0=nc_f_add(nc_f_add(nc_interval_exact(anchor.chart_hi.x),nc_interval_exact(anchor.chart_lo.x)),anchor.chart_x_residual);
    let b0=nc_f_add(nc_f_add(nc_interval_exact(anchor.chart_hi.y),nc_interval_exact(anchor.chart_lo.y)),anchor.chart_y_residual);
    let a1=nc_f_add(a0,delta.y);let b1=nc_f_add(b0,delta.z);
    let r0=nc_m_rsqrt(nc_f_add(vec2(1.0),nc_f_add(nc_m_square(a0),nc_m_square(b0))));
    let r1=nc_m_rsqrt(nc_f_add(vec2(1.0),nc_f_add(nc_m_square(a1),nc_m_square(b1))));
    let v0=NcMapVec(a0,vec2(1.0),nc_f_sub(nc_interval_zero(),b0));
    let n0=nc_m_scale(v0,r0);
    let delta_norm2=nc_f_add(nc_f_mul(delta.y,nc_f_add(nc_f_mul(vec2(2.0),a0),delta.y)),nc_f_mul(delta.z,nc_f_add(nc_f_mul(vec2(2.0),b0),delta.z)));
    let product=nc_f_mul(r1,r0);
    let inverse_s_sum=nc_f_div(nc_f_mul(product,product),nc_f_add(r1,r0));
    let delta_raw=NcMapVec(delta.y,nc_interval_zero(),nc_f_sub(nc_interval_zero(),delta.z));
    let dn=nc_m_sub(nc_m_scale(delta_raw,r1),nc_m_scale(v0,nc_f_mul(delta_norm2,inverse_s_sum)));
    let n=nc_m_add(n0,dn);let axis=parameters.axis;let e=parameters.eccentricity;
    let z0=nc_m_dot(axis,n0);let dz=nc_m_dot(axis,dn);let z=nc_f_add(z0,dz);
    let d0=nc_f_sub(vec2(1.0),nc_f_mul(e,nc_m_square(z0)));
    let d1=nc_f_sub(vec2(1.0),nc_f_mul(e,nc_m_square(z)));
    let radius_inverse0=nc_m_rsqrt(d0);let radius_inverse1=nc_m_rsqrt(d1);
    let radius=nc_f_mul(vec2(6378137.0),radius_inverse1);
    let radius_product=nc_f_mul(radius_inverse1,radius_inverse0);
    let dr=nc_f_div(nc_f_mul(nc_f_mul(nc_f_mul(nc_f_mul(vec2(6378137.0),e),dz),nc_f_add(z,z0)),nc_f_mul(radius_product,radius_product)),nc_f_add(radius_inverse1,radius_inverse0));
    let h=nc_f_add(nc_f_add(nc_f_add(nc_interval_exact(nc_anchor.x),nc_interval_exact(nc_anchor.y)),delta.x),parameters.anchor_residual);
    let dh=nc_f_add(nc_f_add(nc_f_add(nc_f_sub(nc_interval_exact(nc_anchor.x),nc_interval_exact(anchor.height.x)),nc_f_sub(nc_interval_exact(nc_anchor.y),nc_interval_exact(anchor.height.y))),delta.x),nc_f_sub(parameters.anchor_residual,anchor.height_residual));
    let tangent_delta=nc_m_sub(dn,nc_m_scale(axis,nc_f_mul(e,dz)));
    let initial_tangent=nc_m_sub(n0,nc_m_scale(axis,nc_f_mul(e,z0)));
    let position=nc_m_add(anchor.mapped_minus_vertex,nc_m_add(nc_m_add(nc_m_scale(tangent_delta,radius),nc_m_scale(initial_tangent,dr)),nc_m_add(nc_m_scale(dn,h),nc_m_scale(n0,dh))));
    let w=NcMapVec(dq.y,nc_interval_zero(),nc_f_sub(nc_interval_zero(),dq.z));
    let nprime=nc_m_scale(nc_m_sub(w,nc_m_scale(n,nc_m_dot(n,w))),r1);
    let meridian=nc_m_sub(axis,nc_m_scale(n,z));
    let coefficient=nc_f_mul(nc_f_div(nc_f_mul(radius,e),d1),nc_m_dot(nprime,axis));
    let velocity=nc_m_sub(nc_m_add(nc_m_scale(n,dq.x),nc_m_scale(nprime,nc_f_add(radius,h))),nc_m_scale(meridian,coefficient));
    if !nc_m_valid(position)||!nc_m_valid(velocity){return failure;}
    return NcMapPoint(position,velocity,1u);
}
