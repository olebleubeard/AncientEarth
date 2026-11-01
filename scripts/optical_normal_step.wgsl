// Original MIT. Isolated diagnostic only: no production registration.
// Six physical state values: source-relative height, two gnomonic normal
// coordinates, and Cartesian direction. There is no redundant position state.
struct NcState { q:vec3<f32>, u:vec3<f32> };
struct NcDerivative { q:vec3<f32>, u:vec3<f32>, refractivity:f32, valid:u32 };
struct NcCurve {
    q2:vec3<f32>,q3:vec3<f32>,q4:vec3<f32>,q5:vec3<f32>,
    u2:vec3<f32>,u3:vec3<f32>,u4:vec3<f32>,u5:vec3<f32>,
    optical:vec4<f32>,error:f32,valid:u32,
};
// The diagnostic controller must set these once from one immutable eye/branch.
// Heights use (high,low); source offsets are kept separate through every stage.
var<private> nc_axis:vec3<f32>;
var<private> nc_eye_height:vec2<f32>;
var<private> nc_anchor:vec2<f32>;
var<private> nc_index:vec2<f32>; // refractivity at anchor, d(index)/dH
const NC_A:f32=6378137.0;
const NC_E2:f32=0.0066943799901413165;
fn nc_finite3(v:vec3<f32>)->bool{return all(abs(v)<vec3(3.0e37));}
fn nc_finite(v:f32)->bool{return abs(v)<3.0e37;}
fn nc_height(q:vec3<f32>)->f32{return nc_anchor.x+(nc_anchor.y+q.x);}
fn nc_height_difference(q:vec3<f32>)->f32{
    return ((nc_anchor.x-nc_eye_height.x)+(nc_anchor.y-nc_eye_height.y))+q.x;
}
// The unscaled chart vector has squared length >=1. The explicit axis
// normalization belongs to packet initialization, not every field query.
fn nc_normal(q:vec3<f32>)->vec3<f32>{
    return vec3(q.y,1.0,-q.z)*inverseSqrt(1.0+dot(q.yz,q.yz));
}
fn nc_normal_delta(q:vec3<f32>)->vec3<f32>{
    let q2=dot(q.yz,q.yz);let scale=sqrt(1.0+q2);
    return vec3(q.y,-q2/(scale+1.0),-q.z)/scale;
}
// Exact normal-coordinate map written as differences from the original eye.
// No ~Earth-radius Cartesian subtraction is used, and P(initial)=exactly0.
fn nc_position(q:vec3<f32>)->vec3<f32>{
    let dn=nc_normal_delta(q);let n=vec3(0.0,1.0,0.0)+dn;
    let z0=nc_axis.y;let dz=dot(nc_axis,dn);let z=z0+dz;
    let d0=sqrt(1.0-NC_E2*z0*z0);let d=sqrt(1.0-NC_E2*z*z);
    let radius=NC_A/d;
    let dr=NC_A*NC_E2*dz*(z+z0)/(d*d0*(d+d0));
    let tangent_delta=dn-nc_axis*(NC_E2*dz);
    let initial=vec3(0.0,1.0,0.0)-nc_axis*(NC_E2*z0);
    return tangent_delta*radius+initial*dr+dn*nc_height(q)+vec3(0.0,nc_height_difference(q),0.0);
}
fn nc_rhs(s:NcState)->NcDerivative{
    var result=NcDerivative(vec3(0.0),vec3(0.0),0.0,0u);
    if !nc_finite3(s.q)||!nc_finite3(s.u)||dot(s.q.yz,s.q.yz)>1.0{return result;}
    let h=nc_height(s.q);let refractivity=nc_index.x+nc_index.y*s.q.x;let index=1.0+refractivity;
    if !nc_finite(h)||!nc_finite(index)||index<0.5{return result;}
    let n=nc_normal(s.q);let z=dot(n,nc_axis);let d=1.0-NC_E2*z*z;let root=sqrt(d);
    let nr=NC_A/root;let mr=NC_A*(1.0-NC_E2)/(d*root);
    if !(nr+h>0.0 && mr+h>0.0){return result;}
    let dh=dot(s.u,n);let tangent=s.u-n*dh;let meridian=nc_axis-n*z;
    let coefficient=NC_A*NC_E2/(d*root*(mr+h)*(nr+h));
    let dn=tangent/(nr+h)+meridian*(coefficient*dot(meridian,s.u));
    let dq=vec3(dh,(dn.x-s.q.y*dn.y)/n.y,(-dn.z-s.q.z*dn.y)/n.y);
    let du=(n-s.u*dh)*(nc_index.y/index);
    if !nc_finite3(dq)||!nc_finite3(du){return result;}
    return NcDerivative(dq,du,refractivity,1u);
}
fn nc_position_derivative(q:vec3<f32>,dq:vec3<f32>)->vec3<f32>{
    let n=nc_normal(q);let w=vec3(dq.y,0.0,-dq.z);
    let dn=(w-n*dot(n,w))*inverseSqrt(1.0+dot(q.yz,q.yz));
    let z=dot(n,nc_axis);let d=1.0-NC_E2*z*z;let radius=NC_A/sqrt(d);
    return n*dq.x+dn*(radius+nc_height(q))-(nc_axis-n*z)*(radius*NC_E2/d*dot(dn,nc_axis));
}
fn nc_invalid()->NcCurve{
    return NcCurve(vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec3(0.0),vec4(0.0),3.0e37,0u);
}
fn nc_curve_q(start:NcState,curve:NcCurve,t:f32)->vec3<f32>{
    return start.q+t*(curve.q2+(1.0-t)*(curve.q3+t*(curve.q4+(1.0-t)*curve.q5)));
}
fn nc_curve_u(start:NcState,curve:NcCurve,t:f32)->vec3<f32>{
    return start.u+t*(curve.u2+(1.0-t)*(curve.u3+t*(curve.u4+(1.0-t)*curve.u5)));
}
fn nc_curve_dq(curve:NcCurve,t:f32,step:f32)->vec3<f32>{
    return (curve.q2+(1.0-2.0*t)*curve.q3+(2.0*t-3.0*t*t)*curve.q4+(2.0*t-6.0*t*t+4.0*t*t*t)*curve.q5)/step;
}
fn nc_curve_acceleration(start:NcState,curve:NcCurve,step:f32)->f32{
    let q1=(length(curve.q2.yz)+length(curve.q3.yz)+length(curve.q4.yz)+length(curve.q5.yz))/step;
    let q2=(2.0*length(curve.q3.yz)+4.0*length(curve.q4.yz)+2.0*length(curve.q5.yz))/(step*step);
    let h0=abs(nc_height(start.q))+abs(curve.q2.x)+abs(curve.q3.x)+abs(curve.q4.x)+abs(curve.q5.x);
    let h1=(abs(curve.q2.x)+abs(curve.q3.x)+abs(curve.q4.x)+abs(curve.q5.x))/step;
    let h2=(2.0*abs(curve.q3.x)+4.0*abs(curve.q4.x)+2.0*abs(curve.q5.x))/(step*step);
    let d=1.0-NC_E2;let radius=NC_A/sqrt(d);let c=NC_A*NC_E2/(d*sqrt(d));
    return h2+2.0*h1*q1+(radius+h0+c)*(q2+3.0*q1*q1)+c*(3.0+3.0*NC_E2/d)*q1*q1;
}
// BEGIN GENERATED DP54 TRIAL
fn nc_trial(start:NcState,step:f32)->NcCurve{
    if !(step>0.0)||!nc_finite(step){return nc_invalid();}
    let k0=nc_rhs(start);if k0.valid==0u{return nc_invalid();}
    let stage_q1=start.q+step*((1.0/5.0)*k0.q);
    let stage_u1=start.u+step*((1.0/5.0)*k0.u);
    let k1=nc_rhs(NcState(stage_q1,stage_u1));if k1.valid==0u{return nc_invalid();}
    let stage_q2=start.q+step*((3.0/40.0)*k0.q+(9.0/40.0)*k1.q);
    let stage_u2=start.u+step*((3.0/40.0)*k0.u+(9.0/40.0)*k1.u);
    let k2=nc_rhs(NcState(stage_q2,stage_u2));if k2.valid==0u{return nc_invalid();}
    let stage_q3=start.q+step*((44.0/45.0)*k0.q+(-56.0/15.0)*k1.q+(32.0/9.0)*k2.q);
    let stage_u3=start.u+step*((44.0/45.0)*k0.u+(-56.0/15.0)*k1.u+(32.0/9.0)*k2.u);
    let k3=nc_rhs(NcState(stage_q3,stage_u3));if k3.valid==0u{return nc_invalid();}
    let stage_q4=start.q+step*((19372.0/6561.0)*k0.q+(-25360.0/2187.0)*k1.q+(64448.0/6561.0)*k2.q+(-212.0/729.0)*k3.q);
    let stage_u4=start.u+step*((19372.0/6561.0)*k0.u+(-25360.0/2187.0)*k1.u+(64448.0/6561.0)*k2.u+(-212.0/729.0)*k3.u);
    let k4=nc_rhs(NcState(stage_q4,stage_u4));if k4.valid==0u{return nc_invalid();}
    let stage_q5=start.q+step*((9017.0/3168.0)*k0.q+(-355.0/33.0)*k1.q+(46732.0/5247.0)*k2.q+(49.0/176.0)*k3.q+(-5103.0/18656.0)*k4.q);
    let stage_u5=start.u+step*((9017.0/3168.0)*k0.u+(-355.0/33.0)*k1.u+(46732.0/5247.0)*k2.u+(49.0/176.0)*k3.u+(-5103.0/18656.0)*k4.u);
    let k5=nc_rhs(NcState(stage_q5,stage_u5));if k5.valid==0u{return nc_invalid();}
    let q3=-step*((500.0/1113.0)*(k2.q-k0.q)+(125.0/192.0)*(k3.q-k0.q)+(-2187.0/6784.0)*(k4.q-k0.q)+(11.0/84.0)*(k5.q-k0.q));
    let q2=step*k0.q-q3;
    let u2=step*((35.0/384.0)*k0.u+(500.0/1113.0)*k2.u+(125.0/192.0)*k3.u+(-2187.0/6784.0)*k4.u+(11.0/84.0)*k5.u);
    let stage_q6=start.q+q2;let stage_u6=start.u+u2;
    let k6=nc_rhs(NcState(stage_q6,stage_u6));if k6.valid==0u{return nc_invalid();}
    let q4=step*(k0.q-k6.q)-2.0*q3;
    let q5=step*((87487479700.0/32700410799.0)*(k2.q-k0.q)+(-10690763975.0/1880347072.0)*(k3.q-k0.q)+(701980252875.0/199316789632.0)*(k4.q-k0.q)+(-1453857185.0/822651844.0)*(k5.q-k0.q)+(69997945.0/29380423.0)*(k6.q-k0.q));
    let u3=step*k0.u-u2;let u4=u2-step*k6.u-u3;
    let u5=step*((87487479700.0/32700410799.0)*(k2.u-k0.u)+(-10690763975.0/1880347072.0)*(k3.u-k0.u)+(701980252875.0/199316789632.0)*(k4.u-k0.u)+(-1453857185.0/822651844.0)*(k5.u-k0.u)+(69997945.0/29380423.0)*(k6.u-k0.u));
    let n3=-step*((500.0/1113.0)*(k2.refractivity-k0.refractivity)+(125.0/192.0)*(k3.refractivity-k0.refractivity)+(-2187.0/6784.0)*(k4.refractivity-k0.refractivity)+(11.0/84.0)*(k5.refractivity-k0.refractivity));
    let n2=step*k0.refractivity-n3;let n4=step*(k0.refractivity-k6.refractivity)-2.0*n3;
    let n5=step*((87487479700.0/32700410799.0)*(k2.refractivity-k0.refractivity)+(-10690763975.0/1880347072.0)*(k3.refractivity-k0.refractivity)+(701980252875.0/199316789632.0)*(k4.refractivity-k0.refractivity)+(-1453857185.0/822651844.0)*(k5.refractivity-k0.refractivity)+(69997945.0/29380423.0)*(k6.refractivity-k0.refractivity));
    let eq=step*((-71.0/16695.0)*(k2.q-k0.q)+(71.0/1920.0)*(k3.q-k0.q)+(-17253.0/339200.0)*(k4.q-k0.q)+(22.0/525.0)*(k5.q-k0.q)+(-1.0/40.0)*(k6.q-k0.q));
    let eu=step*((-71.0/16695.0)*(k2.u-k0.u)+(71.0/1920.0)*(k3.u-k0.u)+(-17253.0/339200.0)*(k4.u-k0.u)+(22.0/525.0)*(k5.u-k0.u)+(-1.0/40.0)*(k6.u-k0.u));
    let en=step*((-71.0/16695.0)*(k2.refractivity-k0.refractivity)+(71.0/1920.0)*(k3.refractivity-k0.refractivity)+(-17253.0/339200.0)*(k4.refractivity-k0.refractivity)+(22.0/525.0)*(k5.refractivity-k0.refractivity)+(-1.0/40.0)*(k6.refractivity-k0.refractivity));
    if !nc_finite3(q2)||!nc_finite3(q3)||!nc_finite3(q4)||!nc_finite3(q5)||!nc_finite3(u2)||!nc_finite3(u3)||!nc_finite3(u4)||!nc_finite3(u5)||!nc_finite3(eq)||!nc_finite3(eu)||!nc_finite(en){return nc_invalid();}
    let high_q=start.q+q2;let low_q=high_q-eq;let high_u=start.u+u2;let low_u=high_u-eu;
    if dot(low_q.yz,low_q.yz)>1.0 || !nc_finite3(low_q)||!nc_finite3(low_u){return nc_invalid();}
    let c0=start.q.yz;let c1=q2.yz+q3.yz;let c2=q4.yz-q3.yz+q5.yz;let c3=-q4.yz-2.0*q5.yz;let c4=q5.yz;
    let b1=c0+c1*0.25;let b2=c0+c1*0.5+c2/6.0;let b3=c0+c1*0.75+c2*0.5+c3*0.25;let b4=c0+c1+c2+c3+c4;
    if max(max(dot(c0,c0),dot(b1,b1)),max(max(dot(b2,b2),dot(b3,b3)),dot(b4,b4)))>1.0{return nc_invalid();}
    let axis=normalize(start.u);let p0=dot(axis,start.u);let p1=dot(axis,u2+u3);let p2=dot(axis,u4-u3+u5);let p3=dot(axis,-u4-2.0*u5);let p4=dot(axis,u5);
    let h2=p2-p4;let h3=p3+2.0*p4;
    let lower=min(min(min(p0,p0+p1*0.25),min(p0+p1*0.5+p2/6.0,p0+p1*0.75+p2*0.5+p3*0.25)),min(min(p0+p1+p2+p3+p4,p0+p1*0.5+h2/6.0),min(p0+p1*0.75+h2*0.5+h3*0.25,dot(axis,low_u))));
    if !(lower>0.0)||!nc_finite(lower){return nc_invalid();}
    let radius=6399594.0+max(abs(nc_height(start.q))+abs(q2.x)+abs(q3.x)+abs(q4.x)+abs(q5.x),abs(nc_height(low_q)));
    let embedded=max(max(abs(eq.x)+radius*length(eq.yz),2.0*step*length(eu)/lower),abs(en));
    let interior=max(max(abs(q5.x)+radius*length(q5.yz),2.0*step*length(u5)/lower),abs(n5))*0.0625;
    let error=embedded+interior;if !nc_finite(error)||!all(abs(vec4(n2,n3,n4,n5))<vec4(3.0e37)){return nc_invalid();}
    return NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),error,1u);
}
