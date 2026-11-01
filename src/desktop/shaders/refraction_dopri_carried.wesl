// Original MIT. Unregistered carried auxiliary Hermite diagnostic. Unrolled Dormand--Prince5(4) smooth-branch prototype.
// Rational tableau independently reviewed against the primary DOPRI5 reference.
// Not registered in production until one-sided contacts and GPU oracles pass.
struct DpStep {delta:State,error:f32,geometry:vec4<f32>,r3:vec3<f32>,r4:vec3<f32>,r5:vec3<f32>,du:vec3<f32>,ur3:vec3<f32>,ur4:vec3<f32>,ur5:vec3<f32>,gr2:vec4<f32>,gr3:vec4<f32>,gr4:vec4<f32>,gr5:vec4<f32>};
fn dp_finite(v:vec4<f32>)->bool{return all(abs(v)<=vec4(3.0e38));}
fn dp_invalid()->DpStep{return DpStep(State(vec3(0.0),vec3(0.0)),1e30,vec4(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec4(0.0),vec4(0.0),vec4(0.0),vec4(0.0));}
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
    let n0=branch.y+branch.z*dp_height_offset;
    if !dp_finite(g0) || !(1.0+n0>=0.5 && 1.0+n0<=3e38){return dp_invalid();}
    let b0=(auxiliary0.g.xyz-s.u*dot(s.u,auxiliary0.g.xyz))*(branch.z/(1.0+n0));
    if !dp_finite(vec4(b0,0.0)){return dp_invalid();}
    let v0=vec3(0.0);
    let delta1=(s.u*(1.0/5.0)+(v0*(1.0/5.0)))*step;
    let v1=(b0*(1.0/5.0))*step;
    let u1=s.u+v1;
    let increment1=(auxiliary0.derivative*(1.0/5.0)+vec4(0.0))*step;
    let raw1=g0+increment1;
    let auxiliary1=aux_field(raw1,u1);if auxiliary1.valid==0u{return dp_invalid();}
    let g1=auxiliary1.g;
    let n1=branch.y+branch.z*(dp_height_offset+increment1.w);
    if !dp_finite(g1) || !(1.0+n1>=0.5 && 1.0+n1<=3e38){return dp_invalid();}
    let b1=(g1.xyz-u1*dot(u1,g1.xyz))*(branch.z/(1.0+n1));
    if !dp_finite(vec4(b1,0.0)){return dp_invalid();}
    let delta2=(s.u*(3.0/10.0)+(v0*(3.0/40.0)+v1*(9.0/40.0)))*step;
    let v2=(b0*(3.0/40.0)+b1*(9.0/40.0))*step;
    let u2=s.u+v2;
    let increment2=(auxiliary0.derivative*(3.0/10.0)+(auxiliary1.derivative-auxiliary0.derivative)*(9.0/40.0))*step;
    let raw2=g0+increment2;
    let auxiliary2=aux_field(raw2,u2);if auxiliary2.valid==0u{return dp_invalid();}
    let g2=auxiliary2.g;
    let n2=branch.y+branch.z*(dp_height_offset+increment2.w);
    if !dp_finite(g2) || !(1.0+n2>=0.5 && 1.0+n2<=3e38){return dp_invalid();}
    let b2=(g2.xyz-u2*dot(u2,g2.xyz))*(branch.z/(1.0+n2));
    if !dp_finite(vec4(b2,0.0)){return dp_invalid();}
    let delta3=(s.u*(4.0/5.0)+(v0*(44.0/45.0)+v1*(-56.0/15.0)+v2*(32.0/9.0)))*step;
    let v3=(b0*(44.0/45.0)+b1*(-56.0/15.0)+b2*(32.0/9.0))*step;
    let u3=s.u+v3;
    let increment3=(auxiliary0.derivative*(4.0/5.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-56.0/15.0)+(auxiliary2.derivative-auxiliary0.derivative)*(32.0/9.0))*step;
    let raw3=g0+increment3;
    let auxiliary3=aux_field(raw3,u3);if auxiliary3.valid==0u{return dp_invalid();}
    let g3=auxiliary3.g;
    let n3=branch.y+branch.z*(dp_height_offset+increment3.w);
    if !dp_finite(g3) || !(1.0+n3>=0.5 && 1.0+n3<=3e38){return dp_invalid();}
    let b3=(g3.xyz-u3*dot(u3,g3.xyz))*(branch.z/(1.0+n3));
    if !dp_finite(vec4(b3,0.0)){return dp_invalid();}
    let delta4=(s.u*(8.0/9.0)+(v0*(19372.0/6561.0)+v1*(-25360.0/2187.0)+v2*(64448.0/6561.0)+v3*(-212.0/729.0)))*step;
    let v4=(b0*(19372.0/6561.0)+b1*(-25360.0/2187.0)+b2*(64448.0/6561.0)+b3*(-212.0/729.0))*step;
    let u4=s.u+v4;
    let increment4=(auxiliary0.derivative*(8.0/9.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-25360.0/2187.0)+(auxiliary2.derivative-auxiliary0.derivative)*(64448.0/6561.0)+(auxiliary3.derivative-auxiliary0.derivative)*(-212.0/729.0))*step;
    let raw4=g0+increment4;
    let auxiliary4=aux_field(raw4,u4);if auxiliary4.valid==0u{return dp_invalid();}
    let g4=auxiliary4.g;
    let n4=branch.y+branch.z*(dp_height_offset+increment4.w);
    if !dp_finite(g4) || !(1.0+n4>=0.5 && 1.0+n4<=3e38){return dp_invalid();}
    let b4=(g4.xyz-u4*dot(u4,g4.xyz))*(branch.z/(1.0+n4));
    if !dp_finite(vec4(b4,0.0)){return dp_invalid();}
    let delta5=(s.u*(1.0/1.0)+(v0*(9017.0/3168.0)+v1*(-355.0/33.0)+v2*(46732.0/5247.0)+v3*(49.0/176.0)+v4*(-5103.0/18656.0)))*step;
    let v5=(b0*(9017.0/3168.0)+b1*(-355.0/33.0)+b2*(46732.0/5247.0)+b3*(49.0/176.0)+b4*(-5103.0/18656.0))*step;
    let u5=s.u+v5;
    let increment5=(auxiliary0.derivative*(1.0)+(auxiliary1.derivative-auxiliary0.derivative)*(-355.0/33.0)+(auxiliary2.derivative-auxiliary0.derivative)*(46732.0/5247.0)+(auxiliary3.derivative-auxiliary0.derivative)*(49.0/176.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-5103.0/18656.0))*step;
    let raw5=g0+increment5;
    let auxiliary5=aux_field(raw5,u5);if auxiliary5.valid==0u{return dp_invalid();}
    let g5=auxiliary5.g;
    let n5=branch.y+branch.z*(dp_height_offset+increment5.w);
    if !dp_finite(g5) || !(1.0+n5>=0.5 && 1.0+n5<=3e38){return dp_invalid();}
    let b5=(g5.xyz-u5*dot(u5,g5.xyz))*(branch.z/(1.0+n5));
    if !dp_finite(vec4(b5,0.0)){return dp_invalid();}
    let delta6=(s.u*(1.0/1.0)+(v0*(35.0/384.0)+v2*(500.0/1113.0)+v3*(125.0/192.0)+v4*(-2187.0/6784.0)+v5*(11.0/84.0)))*step;
    let v6=(b0*(35.0/384.0)+b2*(500.0/1113.0)+b3*(125.0/192.0)+b4*(-2187.0/6784.0)+b5*(11.0/84.0))*step;
    let u6=s.u+v6;
    let increment6=(auxiliary0.derivative*(1.0)+(auxiliary2.derivative-auxiliary0.derivative)*(500.0/1113.0)+(auxiliary3.derivative-auxiliary0.derivative)*(125.0/192.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-2187.0/6784.0)+(auxiliary5.derivative-auxiliary0.derivative)*(11.0/84.0))*step;
    let raw6=g0+increment6;
    let auxiliary6=aux_field(raw6,u6);if auxiliary6.valid==0u{return dp_invalid();}
    let g6=auxiliary6.g;
    let n6=branch.y+branch.z*(dp_height_offset+increment6.w);
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
    let normal_scale=6399594.0+abs(g6.w);
    let low_normal=raw6.xyz-auxiliary_error.xyz;
    let embedded_dot=dot(raw6.xyz,low_normal);let embedded_norm=length(raw6.xyz)*length(low_normal);
    if !(embedded_dot>0.0 && embedded_norm>1e-10 && embedded_norm<1e20){return dp_invalid();}
    let embedded_normal=1.41421356237*length(cross(raw6.xyz,auxiliary_error.xyz))/embedded_norm*normal_scale;
    if !dp_finite(vec4(embedded_normal,height_error,0.0,0.0)){return dp_invalid();}
    let embedded=max(max(max(length(ep),length(eu)),abs(en)),max(abs(height_error),embedded_normal));
    let ur3=b0*step-v6;let ur4=v6-b6*step-ur3;
    let ur5=((b2-b0)*(87487479700.0/32700410799.0)+(b3-b0)*(-10690763975.0/1880347072.0)+(b4-b0)*(701980252875.0/199316789632.0)+(b5-b0)*(-1453857185.0/822651844.0)+(b6-b0)*(69997945.0/29380423.0))*step;
    let nr5=((n2-n0)*(87487479700.0/32700410799.0)+(n3-n0)*(-10690763975.0/1880347072.0)+(n4-n0)*(701980252875.0/199316789632.0)+(n5-n0)*(-1453857185.0/822651844.0)+(n6-n0)*(69997945.0/29380423.0))*step;
    let gd=(auxiliary2.derivative-auxiliary0.derivative)*(500.0/1113.0)+(auxiliary3.derivative-auxiliary0.derivative)*(125.0/192.0)+(auxiliary4.derivative-auxiliary0.derivative)*(-2187.0/6784.0)+(auxiliary5.derivative-auxiliary0.derivative)*(11.0/84.0);
    let gr2=(auxiliary0.derivative+gd)*step;
    let gr3=-gd*step;
    let gr4=(auxiliary0.derivative-auxiliary6.derivative+2.0*gd)*step;
    let gr5=((auxiliary2.derivative-auxiliary0.derivative)*(87487479700.0/32700410799.0)+(auxiliary3.derivative-auxiliary0.derivative)*(-10690763975.0/1880347072.0)+(auxiliary4.derivative-auxiliary0.derivative)*(701980252875.0/199316789632.0)+(auxiliary5.derivative-auxiliary0.derivative)*(-1453857185.0/822651844.0)+(auxiliary6.derivative-auxiliary0.derivative)*(69997945.0/29380423.0))*step;
    let m=dp_normal_lower(s.u,v6,ur3,ur4,ur5);let nm=dp_normal_lower(g0.xyz,gr2.xyz,gr3.xyz,gr4.xyz,gr5.xyz);
    if !(m>0.0 && nm>0.0) || !dp_finite(gr2) || !dp_finite(gr3) || !dp_finite(gr4) || !dp_finite(gr5){return dp_invalid();}
    let radius=6399594.0+abs(g0.w)+abs(gr2.w)+abs(gr3.w)+abs(gr4.w)+abs(gr5.w);
    let interior=max(max(max(length(r5),2.0*step*length(ur5)/m),abs(nr5)),max(abs(gr5.w),2.0*radius*length(gr5.xyz)/nm))/16.0;
    let error=embedded+interior;
    if !dp_finite(vec4(r3,error)) || !dp_finite(vec4(r4,interior)) || !dp_finite(vec4(r5,nr5)) || !dp_finite(vec4(ur3,0.0)) || !dp_finite(vec4(ur4,0.0)) || !dp_finite(vec4(ur5,0.0)){return dp_invalid();}
    return DpStep(State(delta6,normalize(u6)),error,g6,r3,r4,r5,v6,ur3,ur4,ur5,gr2,gr3,gr4,gr5);
}
fn dp_normal_lower(start:vec3<f32>,a:vec3<f32>,b:vec3<f32>,c:vec3<f32>,d:vec3<f32>)->f32{
    let axis=normalize(start);let c0=dot(start,axis);let c1=dot(a+b,axis);let c2=dot(c-b+d,axis);let c3=dot(-c-2.0*d,axis);let c4=dot(d,axis);
    let q2=c2-c4;let q3=c3+2.0*c4;
    return min(min(min(c0,c0+c1*0.25),min(c0+c1*0.5+c2/6.0,c0+c1*0.75+c2*0.5+c3*0.25)),min(c0+c1+c2+c3+c4,min(c0+c1*0.5+q2/6.0,c0+c1*0.75+q2*0.5+q3*0.25)));
}
