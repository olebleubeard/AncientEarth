// Original MIT. Unrolled Dormand--Prince5(4) smooth-branch prototype.
// Rational tableau independently reviewed against the primary DOPRI5 reference.
// Not registered in production until one-sided contacts and GPU oracles pass.
struct DpStep {delta:State,error:f32,geometry:vec4<f32>,r3:vec3<f32>,r4:vec3<f32>,r5:vec3<f32>};
fn dp_finite(v:vec4<f32>)->bool{return all(abs(v)<=vec4(3.0e38));}
fn dp_invalid()->DpStep{return DpStep(State(vec3(0.0),vec3(0.0)),1e30,vec4(0.0),vec3(0.0),vec3(0.0),vec3(0.0));}
// Original MIT. Diagnostic auxiliary WGS84 normal/height DP54 variant.
// The physical field evaluates the unit direction of the auxiliary RK normal;
// the raw RK state is not projected at intermediate stages.
struct AuxField{g:vec4<f32>,derivative:vec4<f32>,valid:u32};
fn aux_field(raw:vec4<f32>,u:vec3<f32>)->AuxField{
    var result=AuxField(raw,vec4(0.0),0u);
    if !dp_finite(raw) || !dp_finite(vec4(u,0.0)){return result;}
    let norm=length(raw.xyz);if !(norm>1e-10 && norm<1e10){return result;}
    let n=raw.xyz/norm;let h=raw.w;let axis=vec3(0.0,settings.latitude.x,-settings.latitude.y);
    let z=dot(n,axis);let d=1.0-0.0066943799901413165*z*z;
    if !(d>0.0){return result;}
    let root=sqrt(d);let nr=6378137.0/root;let mr=6335439.3272928195/(d*root);
    if !(nr+h>0.0 && mr+h>0.0){return result;}
    let q=axis-n*z;let kh=dot(u,n);
    let kn=(u-n*kh)/(nr+h)+q*(42697.67270717996/(d*root*(mr+h)*(nr+h))*dot(u,q));
    if !dp_finite(vec4(kn,kh)){return result;}
    result.g=vec4(n,h);result.derivative=vec4(kn,kh);result.valid=1u;return result;
}
// branch=(anchor height, refractivity at anchor, dN/dh).
fn dopri_delta(s:State,step:f32,branch:vec3<f32>,g0:vec4<f32>)->DpStep{
    let auxiliary0=aux_field(g0,s.u);if auxiliary0.valid==0u{return dp_invalid();}
    let n0=branch.y+branch.z*(g0.w-branch.x);
    if !dp_finite(g0) || !(1.0+n0>=0.5 && 1.0+n0<=3e38){return dp_invalid();}
    let b0=(auxiliary0.g.xyz-s.u*dot(s.u,auxiliary0.g.xyz))*(branch.z/(1.0+n0));
    if !dp_finite(vec4(b0,0.0)){return dp_invalid();}
    let v0=vec3(0.0);
    let delta1=(s.u*(1.0/5.0)+(v0*(1.0/5.0)))*step;
    let v1=(b0*(1.0/5.0))*step;
    let u1=s.u+v1;
    let raw1=g0+(auxiliary0.derivative*(1.0/5.0)+vec4(0.0))*step;
    let auxiliary1=aux_field(raw1,u1);if auxiliary1.valid==0u{return dp_invalid();}
    let g1=auxiliary1.g;
    let n1=branch.y+branch.z*(g1.w-branch.x);
    if !dp_finite(g1) || !(1.0+n1>=0.5 && 1.0+n1<=3e38){return dp_invalid();}
    let b1=(g1.xyz-u1*dot(u1,g1.xyz))*(branch.z/(1.0+n1));
    if !dp_finite(vec4(b1,0.0)){return dp_invalid();}
    let delta2=(s.u*(3.0/10.0)+(v0*(3.0/40.0)+v1*(9.0/40.0)))*step;
    let v2=(b0*(3.0/40.0)+b1*(9.0/40.0))*step;
    let u2=s.u+v2;
    let raw2=g0+(auxiliary0.derivative*(3.0/10.0)+(auxiliary1.derivative-auxiliary0.derivative)*(9.0/40.0))*step;
    let auxiliary2=aux_field(raw2,u2);if auxiliary2.valid==0u{return dp_invalid();}
    let g2=auxiliary2.g;
    let n2=branch.y+branch.z*(g2.w-branch.x);
    if !dp_finite(g2) || !(1.0+n2>=0.5 && 1.0+n2<=3e38){return dp_invalid();}
    let b2=(g2.xyz-u2*dot(u2,g2.xyz))*(branch.z/(1.0+n2));
    if !dp_finite(vec4(b2,0.0)){return dp_invalid();}
    let delta3=(s.u*(4.0/5.0)+(v0*(44.0/45.0)+v1*(-56.0/15.0)+v2*(32.0/9.0)))*step;
    let v3=(b0*(44.0/45.0)+b1*(-56.0/15.0)+b2*(32.0/9.0))*step;
    let u3=s.u+v3;
    let raw3=g0+(auxiliary0.derivative*(4.0/5.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-56.0/15.0)+(auxiliary2.derivative-auxiliary0.derivative)*(32.0/9.0))*step;
    let auxiliary3=aux_field(raw3,u3);if auxiliary3.valid==0u{return dp_invalid();}
    let g3=auxiliary3.g;
    let n3=branch.y+branch.z*(g3.w-branch.x);
    if !dp_finite(g3) || !(1.0+n3>=0.5 && 1.0+n3<=3e38){return dp_invalid();}
    let b3=(g3.xyz-u3*dot(u3,g3.xyz))*(branch.z/(1.0+n3));
    if !dp_finite(vec4(b3,0.0)){return dp_invalid();}
    let delta4=(s.u*(8.0/9.0)+(v0*(19372.0/6561.0)+v1*(-25360.0/2187.0)+v2*(64448.0/6561.0)+v3*(-212.0/729.0)))*step;
    let v4=(b0*(19372.0/6561.0)+b1*(-25360.0/2187.0)+b2*(64448.0/6561.0)+b3*(-212.0/729.0))*step;
    let u4=s.u+v4;
    let raw4=g0+(auxiliary0.derivative*(8.0/9.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-25360.0/2187.0)+(auxiliary2.derivative-auxiliary0.derivative)*(64448.0/6561.0)+(auxiliary3.derivative-auxiliary0.derivative)*(-212.0/729.0))*step;
    let auxiliary4=aux_field(raw4,u4);if auxiliary4.valid==0u{return dp_invalid();}
    let g4=auxiliary4.g;
    let n4=branch.y+branch.z*(g4.w-branch.x);
    if !dp_finite(g4) || !(1.0+n4>=0.5 && 1.0+n4<=3e38){return dp_invalid();}
    let b4=(g4.xyz-u4*dot(u4,g4.xyz))*(branch.z/(1.0+n4));
    if !dp_finite(vec4(b4,0.0)){return dp_invalid();}
    let delta5=(s.u*(1.0/1.0)+(v0*(9017.0/3168.0)+v1*(-355.0/33.0)+v2*(46732.0/5247.0)+v3*(49.0/176.0)+v4*(-5103.0/18656.0)))*step;
    let v5=(b0*(9017.0/3168.0)+b1*(-355.0/33.0)+b2*(46732.0/5247.0)+b3*(49.0/176.0)+b4*(-5103.0/18656.0))*step;
    let u5=s.u+v5;
    let raw5=g0+(auxiliary0.derivative*(1.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-355.0/33.0)+(auxiliary2.derivative-auxiliary0.derivative)*(46732.0/5247.0)+(auxiliary3.derivative-auxiliary0.derivative)*(49.0/176.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-5103.0/18656.0))*step;
    let auxiliary5=aux_field(raw5,u5);if auxiliary5.valid==0u{return dp_invalid();}
    let g5=auxiliary5.g;
    let n5=branch.y+branch.z*(g5.w-branch.x);
    if !dp_finite(g5) || !(1.0+n5>=0.5 && 1.0+n5<=3e38){return dp_invalid();}
    let b5=(g5.xyz-u5*dot(u5,g5.xyz))*(branch.z/(1.0+n5));
    if !dp_finite(vec4(b5,0.0)){return dp_invalid();}
    let delta6=(s.u*(1.0/1.0)+(v0*(35.0/384.0)+v2*(500.0/1113.0)+v3*(125.0/192.0)+v4*(-2187.0/6784.0)+v5*(11.0/84.0)))*step;
    let v6=(b0*(35.0/384.0)+b2*(500.0/1113.0)+b3*(125.0/192.0)+b4*(-2187.0/6784.0)+b5*(11.0/84.0))*step;
    let u6=s.u+v6;
    let raw6=g0+(auxiliary0.derivative*(1.0)+(auxiliary2.derivative-auxiliary0.derivative)*(500.0/1113.0)+(auxiliary3.derivative-auxiliary0.derivative)*(125.0/192.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-2187.0/6784.0)+(auxiliary5.derivative-auxiliary0.derivative)*(11.0/84.0))*step;
    let auxiliary6=aux_field(raw6,u6);if auxiliary6.valid==0u{return dp_invalid();}
    let g6=auxiliary6.g;
    let n6=branch.y+branch.z*(g6.w-branch.x);
    if !dp_finite(g6) || !(1.0+n6>=0.5 && 1.0+n6<=3e38){return dp_invalid();}
    let b6=(g6.xyz-u6*dot(u6,g6.xyz))*(branch.z/(1.0+n6));
    if !dp_finite(vec4(b6,0.0)){return dp_invalid();}
    let ep=(v0*(71.0/57600.0)+v2*(-71.0/16695.0)+v3*(71.0/1920.0)+v4*(-17253.0/339200.0)+v5*(22.0/525.0)+v6*(-1.0/40.0))*step;
    let eu=(b0*(71.0/57600.0)+b2*(-71.0/16695.0)+b3*(71.0/1920.0)+b4*(-17253.0/339200.0)+b5*(22.0/525.0)+b6*(-1.0/40.0))*(step*step);
    // Subtract refractivity before adding unity; do not subtract rounded1+N.
    let en=((n2-n0)*(-71.0/16695.0)+(n3-n0)*(71.0/1920.0)+(n4-n0)*(-17253.0/339200.0)+(n5-n0)*(22.0/525.0)+(n6-n0)*(-1.0/40.0))*step;
    let high=v2*(500.0/1113.0)+v3*(125.0/192.0)+v4*(-2187.0/6784.0)+v5*(11.0/84.0);
    let r3=-high*step;
    let r4=(-v6+2.0*high)*step;
    let r5=(v2*(87487479700.0/32700410799.0)+v3*(-10690763975.0/1880347072.0)+v4*(701980252875.0/199316789632.0)+v5*(-1453857185.0/822651844.0)+v6*(69997945.0/29380423.0))*step;
    let auxiliary_error=((auxiliary2.derivative-auxiliary0.derivative)*(-71.0/16695.0)+(auxiliary3.derivative-auxiliary0.derivative)*(71.0/1920.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-17253.0/339200.0)+(auxiliary5.derivative-auxiliary0.derivative)*(22.0/525.0)+(auxiliary6.derivative-auxiliary0.derivative)*(-1.0/40.0))*step;
    let height_error=((auxiliary2.derivative.w-auxiliary0.derivative.w)*(-71.0/16695.0)+(auxiliary3.derivative.w-auxiliary0.derivative.w)*(71.0/1920.0)+(auxiliary4.derivative.w-auxiliary0.derivative.w)*(-17253.0/339200.0)+(auxiliary5.derivative.w-auxiliary0.derivative.w)*(22.0/525.0)+(auxiliary6.derivative.w-auxiliary0.derivative.w)*(-1.0/40.0))*step;
    if !dp_finite(vec4(ep,en)) || !dp_finite(vec4(eu,height_error)) || !dp_finite(auxiliary_error){return dp_invalid();}
    let actual=geometry(s.p+delta6);let normal_scale=6399594.0+abs(g6.w);
    let low_normal=raw6.xyz-auxiliary_error.xyz;
    let embedded_dot=dot(raw6.xyz,low_normal);let constraint_dot=dot(raw6.xyz,actual.xyz);
    if !(embedded_dot>0.0 && constraint_dot>0.0) || !dp_finite(actual){return dp_invalid();}
    let high_norm=length(raw6.xyz);let embedded_norm=high_norm*length(low_normal);let constraint_norm=high_norm*length(actual.xyz);
    if !(embedded_norm>1e-10 && constraint_norm>1e-10 && embedded_norm<1e20 && constraint_norm<1e20){return dp_invalid();}
    // For positive dot, sqrt(2)*sin(theta) bounds the unit-vector chord.
    let embedded_normal=1.41421356237*length(cross(raw6.xyz,auxiliary_error.xyz))/embedded_norm*normal_scale;
    let constraint_normal=1.41421356237*length(cross(raw6.xyz,actual.xyz))/constraint_norm*normal_scale;
    if !dp_finite(vec4(embedded_normal,constraint_normal,actual.w-g6.w,0.0)){return dp_invalid();}
    let error=max(max(max(length(ep),length(eu)),abs(en)),max(max(abs(height_error),embedded_normal),max(abs(actual.w-g6.w),constraint_normal)));
    if !(error>=0.0 && error<1e29){return dp_invalid();}
    return DpStep(State(delta6,normalize(u6)),error,actual,r3,r4,r5);
}
