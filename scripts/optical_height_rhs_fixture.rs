//! Original MIT. Isolated projected-field diagnostic injection.
//! Every height stage/control/certificate remains the fast-metric checkpoint;
//! only the field evaluator is algebraically factored. No runtime registration.
pub fn inject(mut shader:String)->String{
    let declaration="fn nh_rhs(state:NcState,orientation:f32)->NhDerivative{";
    assert_eq!(shader.matches(declaration).count(),1);
    shader=shader.replacen(declaration,"fn nh_rhs_reference(state:NcState,orientation:f32)->NhDerivative{",1);
    let calls=shader.matches("nh_rhs(").count();
    assert!(calls>=7,"Expected every original height stage call");
    shader=shader.replace("nh_rhs(","nh_rhs_projected(");
    let helpers=std::fs::read_to_string("scripts/optical_height_rhs_projected.wgsl").unwrap();
    shader.push_str("\n");shader.push_str(&helpers);
    shader
}
