"""Emit only the isolated, unrolled DP54 normal-chart trial.

The field and coordinate helpers are authored in optical_normal_step.wgsl.
No production shader imports this diagnostic source.
"""
from pathlib import Path

A = [[], ["1.0/5.0"], ["3.0/40.0", "9.0/40.0"],
     ["44.0/45.0", "-56.0/15.0", "32.0/9.0"],
     ["19372.0/6561.0", "-25360.0/2187.0", "64448.0/6561.0", "-212.0/729.0"],
     ["9017.0/3168.0", "-355.0/33.0", "46732.0/5247.0", "49.0/176.0", "-5103.0/18656.0"],
     ["35.0/384.0", None, "500.0/1113.0", "125.0/192.0", "-2187.0/6784.0", "11.0/84.0"]]
E = ["71.0/57600.0", None, "-71.0/16695.0", "71.0/1920.0", "-17253.0/339200.0", "22.0/525.0", "-1.0/40.0"]
D = ["-12715105075.0/11282082432.0", None, "87487479700.0/32700410799.0", "-10690763975.0/1880347072.0", "701980252875.0/199316789632.0", "-1453857185.0/822651844.0", "69997945.0/29380423.0"]


def weighted(coefficients, component, differences=False):
    terms = []
    for i, coefficient in enumerate(coefficients):
        if coefficient is not None and not (differences and i == 0):
            value = f"k{i}.{component}"
            if differences:
                value = f"({value}-k0.{component})"
            terms.append(f"({coefficient})*{value}")
    return "+".join(terms)


lines = ["// BEGIN GENERATED DP54 TRIAL", "fn nc_trial(start:NcState,step:f32)->NcCurve{",
         "    if !(step>0.0)||!nc_finite(step){return nc_invalid();}",
         "    let k0=nc_rhs(start);if k0.valid==0u{return nc_invalid();}"]
for i in range(1, 7):
    if i == 6:
        lines += [f"    let q3=-step*({weighted(A[6], 'q', True)});",
                  "    let q2=step*k0.q-q3;",
                  f"    let u2=step*({weighted(A[6], 'u')});",
                  "    let stage_q6=start.q+q2;let stage_u6=start.u+u2;"]
    else:
        lines += [f"    let stage_q{i}=start.q+step*({weighted(A[i], 'q')});",
                  f"    let stage_u{i}=start.u+step*({weighted(A[i], 'u')});"]
    lines += [f"    let k{i}=nc_rhs(NcState(stage_q{i},stage_u{i}));if k{i}.valid==0u{{return nc_invalid();}}"]
lines += ["    let q4=step*(k0.q-k6.q)-2.0*q3;",
          f"    let q5=step*({weighted(D, 'q', True)});",
          "    let u3=step*k0.u-u2;let u4=u2-step*k6.u-u3;",
          f"    let u5=step*({weighted(D, 'u', True)});",
          f"    let n3=-step*({weighted(A[6], 'refractivity', True)});",
          "    let n2=step*k0.refractivity-n3;let n4=step*(k0.refractivity-k6.refractivity)-2.0*n3;",
          f"    let n5=step*({weighted(D, 'refractivity', True)});",
          f"    let eq=step*({weighted(E, 'q', True)});",
          f"    let eu=step*({weighted(E, 'u', True)});",
          f"    let en=step*({weighted(E, 'refractivity', True)});",
          "    if !nc_finite3(q2)||!nc_finite3(q3)||!nc_finite3(q4)||!nc_finite3(q5)||!nc_finite3(u2)||!nc_finite3(u3)||!nc_finite3(u4)||!nc_finite3(u5)||!nc_finite3(eq)||!nc_finite3(eu)||!nc_finite(en){return nc_invalid();}",
          "    let high_q=start.q+q2;let low_q=high_q-eq;let high_u=start.u+u2;let low_u=high_u-eu;",
          "    if dot(low_q.yz,low_q.yz)>1.0 || !nc_finite3(low_q)||!nc_finite3(low_u){return nc_invalid();}",
          "    let c0=start.q.yz;let c1=q2.yz+q3.yz;let c2=q4.yz-q3.yz+q5.yz;let c3=-q4.yz-2.0*q5.yz;let c4=q5.yz;",
          "    let b1=c0+c1*0.25;let b2=c0+c1*0.5+c2/6.0;let b3=c0+c1*0.75+c2*0.5+c3*0.25;let b4=c0+c1+c2+c3+c4;",
          "    if max(max(dot(c0,c0),dot(b1,b1)),max(max(dot(b2,b2),dot(b3,b3)),dot(b4,b4)))>1.0{return nc_invalid();}",
          "    let axis=normalize(start.u);let p0=dot(axis,start.u);let p1=dot(axis,u2+u3);let p2=dot(axis,u4-u3+u5);let p3=dot(axis,-u4-2.0*u5);let p4=dot(axis,u5);",
          "    let h2=p2-p4;let h3=p3+2.0*p4;",
          "    let lower=min(min(min(p0,p0+p1*0.25),min(p0+p1*0.5+p2/6.0,p0+p1*0.75+p2*0.5+p3*0.25)),min(min(p0+p1+p2+p3+p4,p0+p1*0.5+h2/6.0),min(p0+p1*0.75+h2*0.5+h3*0.25,dot(axis,low_u))));",
          "    if !(lower>0.0)||!nc_finite(lower){return nc_invalid();}",
          "    let radius=6399594.0+max(abs(nc_height(start.q))+abs(q2.x)+abs(q3.x)+abs(q4.x)+abs(q5.x),abs(nc_height(low_q)));",
          "    let embedded=max(max(abs(eq.x)+radius*length(eq.yz),2.0*step*length(eu)/lower),abs(en));",
          "    let interior=max(max(abs(q5.x)+radius*length(q5.yz),2.0*step*length(u5)/lower),abs(n5))*0.0625;",
          "    let error=embedded+interior;if !nc_finite(error)||!all(abs(vec4(n2,n3,n4,n5))<vec4(3.0e37)){return nc_invalid();}",
          "    return NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),error,1u);", "}"]
target = Path(__file__).with_name("optical_normal_step.wgsl")
prefix = target.read_text().split("// BEGIN GENERATED DP54 TRIAL", 1)[0]
target.write_text(prefix + "\n".join(lines) + "\n")
