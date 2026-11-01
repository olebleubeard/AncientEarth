// Original MIT. Isolated diagnostic: stable displacement on the same canonical
// normal-coordinate map. delta_q comes directly from the stored dense curve;
// do not form it by subtracting two already-rounded global chart states.
fn nc_position_delta(start_q:vec3<f32>,delta_q:vec3<f32>)->vec3<f32>{
    let chart0=start_q.yz;let delta=delta_q.yz;let chart1=chart0+delta;
    let scale0=sqrt(1.0+dot(chart0,chart0));
    let scale1=sqrt(1.0+dot(chart1,chart1));
    let v0=vec3(chart0.x,1.0,-chart0.y);
    let dv=vec3(delta.x,0.0,-delta.y);
    let square_delta=dot(delta,2.0*chart0+delta);
    let dn=dv/scale1-v0*(square_delta/(scale1*scale0*(scale1+scale0)));
    let n0=v0/scale0;
    let z0=dot(nc_axis,n0);let dz=dot(nc_axis,dn);let z1=z0+dz;
    let d0=sqrt(1.0-NC_E2*z0*z0);let d1=sqrt(1.0-NC_E2*z1*z1);
    let radius1=NC_A/d1;
    let dr=NC_A*NC_E2*dz*(z1+z0)/(d1*d0*(d1+d0));
    let h1=nc_height(start_q)+delta_q.x;
    return (dn-nc_axis*(NC_E2*dz))*radius1
        +(n0-nc_axis*(NC_E2*z0))*dr+dn*h1+n0*delta_q.x;
}
fn nc_curve_delta_q(curve:NcCurve,theta:f32)->vec3<f32>{
    return theta*(curve.q2+(1.0-theta)*(curve.q3+theta*(curve.q4+(1.0-theta)*curve.q5)));
}
