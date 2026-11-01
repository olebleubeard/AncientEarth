// Original MIT. Diagnostic sufficient-only canonical terrain coverage test.
// Requires optical_normal_interval.wgsl. No production registration/bindings.
//
// Preparation must prove nonzero boundary degree throughout the closed XZ
// rectangle centered at `center`; `radius` encloses its entire boundary in3D.
// Y is the reference up axis. Bounds and errors refer to ONE coherent patch
// frame. Quantization/transform/metadata errors must be included by the caller.
// A false result means "defer to the ordinary triangle query", NEVER no terrain.
fn nc_coverage_l1_difference(a:vec3<f32>,b:vec3<f32>)->vec2<f32>{
    let x=nc_interval_abs_upper(nc_interval_sub(nc_interval_exact(a.x),nc_interval_exact(b.x)));
    let y=nc_interval_abs_upper(nc_interval_sub(nc_interval_exact(a.y),nc_interval_exact(b.y)));
    let z=nc_interval_abs_upper(nc_interval_sub(nc_interval_exact(a.z),nc_interval_exact(b.z)));
    return nc_interval_add(nc_interval_add(nc_interval_exact(x),nc_interval_exact(y)),nc_interval_exact(z));
}
fn nc_coverage_core(
    point:vec3<f32>,direction:vec3<f32>,center:vec3<f32>,half_extent:vec2<f32>,
    radius:f32,vertical_bounds:vec2<f32>,point_error:f32,direction_error:f32,
    vertex_error:f32,parameter_range:vec2<f32>
)->bool{
    if !all(abs(point)<vec3(3.0e37))||!all(abs(direction)<vec3(3.0e37))||!all(abs(center)<vec3(3.0e37)){return false;}
    if !all(half_extent>vec2(0.0))||!all(half_extent<vec2(3.0e37))||!(radius>=0.0 && radius<3.0e37){return false;}
    if !nc_interval_valid(vertical_bounds)||!nc_interval_valid(parameter_range)||!(parameter_range.x<parameter_range.y){return false;}
    let errors=vec3(point_error,direction_error,vertex_error);
    if !all(errors>=vec3(0.0))||!all(errors<vec3(3.0e37)){return false;}
    let e=nc_interval_add(nc_coverage_l1_difference(direction,vec3(0.0,1.0,0.0)),nc_interval_exact(direction_error));
    if !nc_interval_valid(e)||!(e.y<1.0){return false;}
    // ||normalize(v_true)-up|| <= 2E/(1-E). A proper rotation aligning the
    // normals displaces any vector by at most this factor times its norm.
    let delta=nc_interval_div(nc_interval_mul(nc_interval_exact(2.0),e),nc_interval_sub(nc_interval_exact(1.0),e));
    let reach=nc_interval_add(nc_interval_exact(radius),nc_coverage_l1_difference(point,center));
    let erosion=nc_interval_add(nc_interval_mul(delta,reach),nc_interval_add(nc_interval_exact(point_error),nc_interval_exact(vertex_error)));
    if !nc_interval_valid(delta)||!nc_interval_valid(reach)||!nc_interval_valid(erosion){return false;}
    let x=nc_interval_abs_upper(nc_interval_sub(nc_interval_exact(point.x),nc_interval_exact(center.x)));
    let z=nc_interval_abs_upper(nc_interval_sub(nc_interval_exact(point.z),nc_interval_exact(center.z)));
    let mx=nc_interval_sub(nc_interval_sub(nc_interval_exact(half_extent.x),nc_interval_exact(x)),erosion);
    let mz=nc_interval_sub(nc_interval_sub(nc_interval_exact(half_extent.y),nc_interval_exact(z)),erosion);
    if !nc_interval_valid(mx)||!nc_interval_valid(mz)||!(mx.x>0.0 && mz.x>0.0){return false;}
    // Every possible intersection lies between the canonical patch's outward
    // Y extrema. Parameter t is for point+t*direction, without normalization.
    let surface_low=nc_interval_sub(nc_interval_exact(vertical_bounds.x),nc_interval_exact(vertex_error));
    let surface_high=nc_interval_add(nc_interval_exact(vertical_bounds.y),nc_interval_exact(vertex_error));
    let point_low=nc_interval_sub(nc_interval_exact(point.y),nc_interval_exact(point_error));
    let point_high=nc_interval_add(nc_interval_exact(point.y),nc_interval_exact(point_error));
    let velocity_low=nc_interval_sub(nc_interval_exact(direction.y),nc_interval_exact(direction_error));
    let velocity_high=nc_interval_add(nc_interval_exact(direction.y),nc_interval_exact(direction_error));
    if !nc_interval_valid(surface_low)||!nc_interval_valid(surface_high)||!nc_interval_valid(point_low)||!nc_interval_valid(point_high)||!nc_interval_valid(velocity_low)||!nc_interval_valid(velocity_high){return false;}
    let numerator=nc_interval_sub(vec2(surface_low.x,surface_high.y),vec2(point_low.x,point_high.y));
    let denominator=vec2(velocity_low.x,velocity_high.y);
    if !(denominator.x>0.0){return false;}
    let parameters=nc_interval_div(numerator,denominator);
    return nc_interval_valid(parameters)&&parameters.x>=parameter_range.x&&parameters.y<=parameter_range.y;
}
