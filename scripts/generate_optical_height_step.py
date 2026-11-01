"""Emit the unregistered height-parameter DP54 trial; no production imports."""
from pathlib import Path

A = [[], ["1.0/5.0"], ["3.0/40.0", "9.0/40.0"],
     ["44.0/45.0", "-56.0/15.0", "32.0/9.0"],
     ["19372.0/6561.0", "-25360.0/2187.0", "64448.0/6561.0", "-212.0/729.0"],
     ["9017.0/3168.0", "-355.0/33.0", "46732.0/5247.0", "49.0/176.0", "-5103.0/18656.0"],
     ["35.0/384.0", None, "500.0/1113.0", "125.0/192.0", "-2187.0/6784.0", "11.0/84.0"]]
C = ["0.0", "1.0/5.0", "3.0/10.0", "4.0/5.0", "8.0/9.0", "1.0", "1.0"]
E = ["71.0/57600.0", None, "-71.0/16695.0", "71.0/1920.0", "-17253.0/339200.0", "22.0/525.0", "-1.0/40.0"]
D = ["-12715105075.0/11282082432.0", None, "87487479700.0/32700410799.0", "-10690763975.0/1880347072.0", "701980252875.0/199316789632.0", "-1453857185.0/822651844.0", "69997945.0/29380423.0"]

def weighted(coefficients, component, differences=False):
    return "+".join(f"({coefficient})*" +
        (f"(k{i}.{component}-k0.{component})" if differences else f"k{i}.{component}")
        for i, coefficient in enumerate(coefficients) if coefficient is not None and not(differences and i == 0))

lines = ["// BEGIN GENERATED HEIGHT DP54 TRIAL", "fn nh_trial(start:NcState,step:f32)->NhCurve{",
    "    if !nc_finite(step)||!(abs(step)>0.000000000001){return nh_invalid();}",
    "    if abs(start.q.y)+abs(start.q.z)>1.0||any(abs(start.u)>vec3(2.0)){return nh_invalid();}",
    "    let orientation=select(-1.0,1.0,step>0.0);",
    "    let k0=nh_rhs(start,orientation);if k0.valid==0u{return nh_invalid();}"]
for i in range(1, 7):
    if i == 6:
        lines += [f"    let q3=-step*({weighted(A[6], 'q', True)});",
                  "    let q2=step*k0.q-q3;",
                  f"    let u2=step*({weighted(A[6], 'u')});",
                  "    let stage_q6=vec3(start.q.x+step,start.q.yz+q2.yz);let stage_u6=start.u+u2;"]
    else:
        lines += [f"    let stage_q{i}=vec3(start.q.x+({C[i]})*step,start.q.yz+step*({weighted(A[i], 'q')}).yz);",
                  f"    let stage_u{i}=start.u+step*({weighted(A[i], 'u')});"]
    lines += [f"    let k{i}=nh_rhs(NcState(stage_q{i},stage_u{i}),orientation);if k{i}.valid==0u{{return nh_invalid();}}"]
lines += ["    let q4=step*(k0.q-k6.q)-2.0*q3;",
          f"    let q5=step*({weighted(D, 'q', True)});",
          "    let u3=step*k0.u-u2;let u4=u2-step*k6.u-u3;",
          f"    let u5=step*({weighted(D, 'u', True)});",
          f"    let n3=-step*({weighted(A[6], 'excess', True)});",
          "    let n2=step*k0.excess-n3;let n4=step*(k0.excess-k6.excess)-2.0*n3;",
          f"    let n5=step*({weighted(D, 'excess', True)});",
          f"    let eq=step*({weighted(E, 'q', True)});",
          f"    let eu=step*({weighted(E, 'u', True)});",
          f"    let en=step*({weighted(E, 'excess', True)});",
          "    if !nc_finite3(q2)||!nc_finite3(q3)||!nc_finite3(q4)||!nc_finite3(q5)||!nc_finite3(u2)||!nc_finite3(u3)||!nc_finite3(u4)||!nc_finite3(u5)||!nc_finite3(eq)||!nc_finite3(eu)||!nc_finite(en)||!all(abs(vec4(n2,n3,n4,n5))<vec4(3.0e37)){return nh_invalid();}",
          "    let chart_variation=nc_f_add(nc_f_add(nc_l1(q2.yz),nc_l1(q3.yz)),nc_f_add(nc_l1(q4.yz),nc_l1(q5.yz)));",
          "    let direction_variation=nc_f_add(nc_f_add(nc_l1_3(u2),nc_l1_3(u3)),nc_f_add(nc_l1_3(u4),nc_l1_3(u5)));",
          "    let initial_norm_upper=nc_l1_3(start.u);",
          "    let vertical_error=nc_f_add(direction_variation,nc_f_mul(initial_norm_upper,chart_variation));",
          "    let signed_vertical=orientation*dot(start.u,nc_normal(start.q));",
          "    // For |a|+|b|<=1 and bounded finite u: the positive normal denominator,",
          "    // explicit inverseSqrt, component product and dot contribute <16u*L1(u).",
          "    // 32u (u=2^-24) also covers tiny/FTZ terms when |vertical|>.01.",
          "    let vertical_rounding=nc_f_mul(nc_interval_exact(0.0000019073486328125),initial_norm_upper);",
          "    let vertical_lower=nc_f_sub(nc_interval_exact(signed_vertical),nc_f_add(vertical_error,vertical_rounding)).x;",
          "    if !nc_interval_valid(chart_variation)||!nc_interval_valid(direction_variation)||!nc_finite(vertical_lower)||!(vertical_lower>0.01){return nh_invalid();}",
          "    let low_u=start.u+u2-eu;let low_chart=start.q.yz+q2.yz-eq.yz;",
          "    let chart_extra=nc_f_add(nc_f_mul(nc_l1(q5.yz),nc_interval_exact(0.0625)),nc_l1(eq.yz));",
          "    let chart_bound=nc_f_add(nc_l1(start.q.yz),nc_f_add(chart_variation,chart_extra));",
          "    if !nc_interval_valid(chart_bound)||chart_bound.y>1.0||!nc_finite3(low_u)||!all(abs(low_chart)<vec2(3.0e37)){return nh_invalid();}",
          "    let direction_extra=nc_f_add(nc_f_mul(nc_l1_3(u5),nc_interval_exact(0.0625)),nc_l1_3(eu));",
          "    let norm_lower=nc_f_sub(nc_interval_exact(max(max(abs(start.u.x),abs(start.u.y)),abs(start.u.z))),nc_f_add(direction_variation,direction_extra)).x;",
          "    if !(norm_lower>0.0)||!nc_finite(norm_lower){return nh_invalid();}",
          "    let path_sum=nc_f_add(nc_f_add(nc_interval_exact(abs(q2.x)),nc_interval_exact(abs(q3.x))),nc_f_add(nc_interval_exact(abs(q4.x)),nc_interval_exact(abs(q5.x))));",
          "    let path_upper_interval=nc_f_div(nc_interval_exact(abs(step)),nc_interval_exact(vertical_lower));",
          "    if !nc_interval_valid(path_sum)||!nc_interval_valid(path_upper_interval){return nh_invalid();}",
          "    let path_upper=max(path_sum.y,path_upper_interval.y);",
          "    let c1=nc_f_add(nc_interval_exact(q2.x),nc_interval_exact(q3.x));",
          "    let c2=nc_f_add(nc_f_sub(nc_interval_exact(q4.x),nc_interval_exact(q3.x)),nc_interval_exact(q5.x));",
          "    let c3=nc_f_sub(nc_f_sub(nc_interval_zero(),nc_interval_exact(q4.x)),nc_f_mul(nc_interval_exact(2.0),nc_interval_exact(q5.x)));",
          "    let c4=nc_interval_exact(q5.x);",
          "    let b0=c1;let b1=nc_f_add(c1,nc_f_div(nc_f_mul(nc_interval_exact(2.0),c2),nc_interval_exact(3.0)));",
          "    let b2=nc_f_add(nc_f_add(c1,nc_f_div(nc_f_mul(nc_interval_exact(4.0),c2),nc_interval_exact(3.0))),c3);",
          "    let b3=nc_f_add(nc_f_add(nc_f_add(c1,nc_f_mul(nc_interval_exact(2.0),c2)),nc_f_mul(nc_interval_exact(3.0),c3)),nc_f_mul(nc_interval_exact(4.0),c4));",
          "    if !nc_interval_valid(b0)||!nc_interval_valid(b1)||!nc_interval_valid(b2)||!nc_interval_valid(b3)||min(min(b0.x,b1.x),min(b2.x,b3.x))<=0.0{return nh_invalid();}",
          "    let radius=6399594.0+abs(nc_height(start.q))+abs(step);",
          "    let embedded=max(max(radius*length(eq.yz),2.0*path_upper*length(eu)/norm_lower),abs(eq.x)+abs(en));",
          "    let interior=max(max(radius*length(q5.yz),2.0*path_upper*length(u5)/norm_lower),abs(q5.x)+abs(n5))*0.0625;",
          "    let error=embedded+interior;if !nc_finite(error){return nh_invalid();}",
          "    return NhCurve(NcCurve(q2,q3,q4,q5,u2,u3,u4,u5,vec4(n2,n3,n4,n5),error,1u),step,vertical_lower,path_upper);", "}"]
target = Path(__file__).with_name("optical_height_step.wgsl")
prefix = target.read_text().split("// BEGIN GENERATED HEIGHT DP54 TRIAL", 1)[0]
target.write_text(prefix + "\n".join(lines) + "\n")
