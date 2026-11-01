// Original MIT. Diagnostic-only sufficient chronological exclusion.
// The caller already holds a certified first contact, with its complete
// canonical/numerical enclosure. This helper proves only that this face
// cannot precede or overlap it; it neither accepts a contact nor proves
// uniqueness. Invalid or overlapping bounds leave the original query intact.
//
// For the canonical plane function f, f(theta)=f(theta0)+d*(theta-theta0)
// for some d in the whole-piece derivative enclosure D. If an admissible
// position tube has a zero, its plane error e is in the same uniform tube
// used below, so theta is in theta0-(F0+e)/D. No derivative of that tube is
// assumed. Face/axis/source/endpoint uncertainties remain in F0 and D.
fn nc_cartesian_face_is_later(curve:NcSceneCurve,frame:NcFaceFrame,context:NcFaceContext,best:NcSceneEvent)->bool {
    let theta0=nc_cartesian_piece.parameter.x;
    if best.kind!=1u||curve.valid==0u||context.start.valid==0u
        ||!nc_finite(theta0)||!nc_finite(curve.theta_end)
        ||!(theta0>=0.0&&theta0<curve.theta_end&&curve.theta_end<=1.0)
        ||!nc_finite(best.lower)||!nc_finite(best.upper)
        ||!(theta0<=best.lower&&best.lower<=best.upper&&best.upper<=curve.theta_end)
        ||!nc_m_valid(frame.row0)||!nc_m_valid(frame.row1)||!nc_m_valid(frame.row2)
        ||!nc_m_valid(frame.shift)||!nc_m_valid(frame.b)||!nc_m_valid(frame.c){return false;}
    let normal=nc_face_cross(frame.b,frame.c);
    let squared=nc_m_dot(normal,normal);
    if !nc_m_valid(normal)||!nc_interval_valid(squared)||!(squared.x>0.0){return false;}
    // nc_face_point retains the existing split-origin cancellation and
    // transforms the entire numerical position tube with the face frame.
    let point=nc_face_point(frame,context.start,curve.numerical_error);
    let before=nc_m_dot(normal,point);
    let frame_norm=nc_f_add(nc_f_add(nc_face_l1(frame.row0),nc_face_l1(frame.row1)),nc_face_l1(frame.row2));
    let plane_acceleration=nc_f_mul(nc_f_mul(nc_face_l1(normal),frame_norm),context.acceleration);
    let duration=nc_f_sub(vec2(curve.theta_end),vec2(theta0));
    let initial_derivative=nc_m_dot(normal,nc_face_transform(frame,context.start.velocity));
    let change=nc_f_mul(plane_acceleration,duration);
    let derivative=nc_f_add(initial_derivative,vec2(-change.y,change.y));
    if !nc_m_valid(point)||!nc_interval_valid(before)||!nc_interval_valid(plane_acceleration)
        ||!nc_interval_valid(duration)||!(duration.x>0.0)||!nc_interval_valid(change)
        ||!nc_interval_valid(derivative)||nc_interval_sign(derivative)==0{return false;}
    let root=nc_f_sub(vec2(theta0),nc_f_div(before,derivative));
    if !nc_interval_valid(root){return false;}
    let clipped=vec2(max(theta0,root.x),min(curve.theta_end,root.y));
    if !nc_interval_valid(clipped){return false;}
    // Strict chronological separation under the unchanged monotone path map.
    // Equality, overlap and an empty/invalid bound all retain original work.
    return clipped.x>best.upper;
}
