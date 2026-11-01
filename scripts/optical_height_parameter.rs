//! Original, unused f64 diagnostic. Height is the independent variable only
//! on certified monotone clear-ray intervals. No renderer/runtime registration.
#[path = "optical_normal_coordinates.rs"]
mod coordinates;
use coordinates::{Chart, State};
use earth_math::{Vec3d, WGS84_A_M, WGS84_E2};
use serde_json::{Value, json};

#[cfg(test)]
#[path = "optical_height_parameter_review.rs"]
mod independent_review;

type Y = [f64; 7]; // a,b,u.xyz, geometric path, optical excess integral.
const A: [&[f64]; 7] = [
    &[], &[1. / 5.], &[3. / 40., 9. / 40.],
    &[44. / 45., -56. / 15., 32. / 9.],
    &[19372. / 6561., -25360. / 2187., 64448. / 6561., -212. / 729.],
    &[9017. / 3168., -355. / 33., 46732. / 5247., 49. / 176., -5103. / 18656.],
    &[35. / 384., 0., 500. / 1113., 125. / 192., -2187. / 6784., 11. / 84.],
];
const C: [f64; 7] = [0., 1. / 5., 3. / 10., 4. / 5., 8. / 9., 1., 1.];
const E: [f64; 7] = [71. / 57600., 0., -71. / 16695., 71. / 1920., -17253. / 339200., 22. / 525., -1. / 40.];
const D: [f64; 7] = [-12715105075. / 11282082432., 0., 87487479700. / 32700410799., -10690763975. / 1880347072., 701980252875. / 199316789632., -1453857185. / 822651844., 69997945. / 29380423.];
const MIN_VERTICAL: f64 = 0.01; // Conditioning policy, not a physical cutoff.

#[derive(Clone, Copy)]
struct Branch { anchor: f64, refractivity: f64, gradient: f64 }
fn state(h: f64, y: Y) -> State {
    State { height: h, a: y[0], b: y[1], direction: Vec3d::new(y[2], y[3], y[4]) }
}
fn norm(v: &[f64]) -> f64 { v.iter().map(|x| x*x).sum::<f64>().sqrt() }
fn rhs(chart: Chart, h: f64, y: Y, branch: Branch, orientation: f64) -> Option<Y> {
    let s = state(h, y);
    let (normal, _) = chart.normal(s)?;
    let vertical = s.direction.dot(normal);
    if orientation * vertical <= MIN_VERTICAL { return None; }
    let refractivity = branch.refractivity + branch.gradient * (h - branch.anchor);
    let index = 1. + refractivity;
    let d = chart.derivative(s, index, branch.gradient)?;
    Some([d.a/vertical, d.b/vertical, d.direction.x/vertical,
        d.direction.y/vertical, d.direction.z/vertical, 1./vertical, refractivity/vertical])
}
#[derive(Clone, Copy)]
struct Dense {
    start: Y, r: [Y; 4], step_h: f64, error: f64,
    vertical_lower: f64, path_upper: f64, norm_lower: f64,
}
impl Dense {
    fn at(&self, theta: f64) -> Y {
        std::array::from_fn(|i| self.start[i] + theta * (self.r[0][i]
            + (1.-theta)*(self.r[1][i]+theta*(self.r[2][i]+(1.-theta)*self.r[3][i]))))
    }
    fn derivative(&self, theta: f64) -> Y {
        std::array::from_fn(|i| (self.r[0][i]+(1.-2.*theta)*self.r[1][i]
            +(2.*theta-3.*theta*theta)*self.r[2][i]
            +(2.*theta-6.*theta*theta+4.*theta.powi(3))*self.r[3][i])/self.step_h)
    }
    fn physical_derivative(&self, chart: Chart, h: f64, theta: f64) -> Option<Vec3d> {
        let y=self.at(theta);let d=self.derivative(theta);
        chart.position_derivative(state(h+self.step_h*theta,y),
            State{height:1.,a:d[0],b:d[1],direction:Vec3d::new(d[2],d[3],d[4])})
    }
    fn physical_acceleration_bound(&self, h: f64) -> Option<f64> {
        let q1=self.r.iter().map(|r|norm(&r[..2])).sum::<f64>()/self.step_h.abs();
        let q2=(2.*norm(&self.r[1][..2])+4.*norm(&self.r[2][..2])+2.*norm(&self.r[3][..2]))/self.step_h.powi(2);
        Chart::acceleration_bound(q1,q2,h.abs()+self.step_h.abs(),1.,0.)
    }
    fn endpoint_coordinate_charge(&self, h: f64, correction: f64) -> f64 {
        let inverse_step=1./self.step_h.abs();
        let radius=WGS84_A_M/(1.-WGS84_E2).sqrt()+h.abs()+self.step_h.abs();
        let physical=1.+radius*self.r.iter().map(|r|norm(&r[..2])).sum::<f64>()*inverse_step;
        let direction=2.*self.path_upper*self.r.iter().map(|r|norm(&r[2..5])).sum::<f64>()*inverse_step/self.norm_lower;
        let optical=self.r.iter().map(|r|r[5].abs()+r[6].abs()).sum::<f64>()*inverse_step;
        correction*physical.max(direction).max(optical)
    }
}
fn trial(chart: Chart, h: f64, y: Y, step_h: f64, branch: Branch) -> Option<Dense> {
    if !step_h.is_finite() || step_h == 0. { return None; }
    let orientation = step_h.signum();
    let mut stages = [[0.; 7]; 7];
    for i in 0..7 {
        let yi = std::array::from_fn(|axis| y[axis] + step_h * A[i].iter()
            .enumerate().map(|(j, a)| a*stages[j][axis]).sum::<f64>());
        stages[i] = rhs(chart, h+C[i]*step_h, yi, branch, orientation)?;
    }
    let delta: Y = std::array::from_fn(|i| step_h*A[6].iter().enumerate().map(|(j,a)| a*stages[j][i]).sum::<f64>());
    let r3: Y = std::array::from_fn(|i| step_h*stages[0][i]-delta[i]);
    let r4: Y = std::array::from_fn(|i| 2.*delta[i]-step_h*(stages[0][i]+stages[6][i]));
    let r5: Y = std::array::from_fn(|i| step_h*D.iter().enumerate().map(|(j,d)| d*stages[j][i]).sum::<f64>());
    let embedded: Y = std::array::from_fn(|i| step_h*E.iter().enumerate().map(|(j,e)| e*stages[j][i]).sum::<f64>());
    let r = [delta, r3, r4, r5];
    let q_variation = r.iter().map(|c| norm(&c[..2])).sum::<f64>();
    let u_variation = r.iter().map(|c| norm(&c[2..5])).sum::<f64>();
    let s = state(h, y);
    let (n, _) = chart.normal(s)?;
    let initial_norm = s.direction.length();
    // The map (a,b)->normalize(n0+aE+bN) has differential norm <=1.
    // |u(theta)·n(theta)-u0·n0| <= |delta_u|+|u0| |delta_chart|.
    // All quartic basis factors have absolute value <=1 on the unit interval.
    let rounding = 256.*f64::EPSILON*(1.+initial_norm+u_variation+q_variation);
    let vertical_lower = orientation*s.direction.dot(n)-u_variation-initial_norm*q_variation-rounding;
    let norm_lower = initial_norm-u_variation-rounding;
    if vertical_lower <= MIN_VERTICAL || norm_lower <= 0.
        || norm(&y[..2])+q_variation+rounding > 1. { return None; }
    // Positive derivative of the represented path polynomial in theta is
    // required independently of stage signs. This preserves event chronology.
    let c1 = delta[5]+r3[5]; let c2 = r4[5]-r3[5]+r5[5];
    let c3 = -r4[5]-2.*r5[5]; let c4 = r5[5];
    let path_derivative_lower = [c1, c1+2.*c2/3., c1+4.*c2/3.+c3,
        c1+2.*c2+3.*c3+4.*c4].into_iter().fold(f64::INFINITY, f64::min)
        - 256.*f64::EPSILON*r.iter().map(|c| c[5].abs()).sum::<f64>();
    if path_derivative_lower <= 0. { return None; }
    let path_upper = (step_h.abs()/vertical_lower).max(r.iter().map(|c| c[5].abs()).sum::<f64>());
    let radius = WGS84_A_M/(1.-WGS84_E2).sqrt()+h.abs()+step_h.abs();
    let metric = |v: Y| (radius*norm(&v[..2])).max(2.*path_upper*norm(&v[2..5])/norm_lower)
        .max(v[5].abs()+v[6].abs());
    let error = metric(embedded)+metric(r5)/16.;
    if !error.is_finite() || !r.iter().flatten().all(|x| x.is_finite()) { return None; }
    Some(Dense { start:y,r,step_h,error,vertical_lower,path_upper,norm_lower })
}

pub fn integrate(inputs: &Value, target_height: f64, maximum_path: f64, tolerance: f64) -> Result<Value,String> {
    let vec3 = |key: &str| -> Result<Vec3d,String> {
        let a=inputs[key].as_array().ok_or(key.to_owned())?;
        Ok(Vec3d::new(a[0].as_f64().ok_or(key)?,a[1].as_f64().ok_or(key)?,a[2].as_f64().ok_or(key)?))
    };
    let chart=Chart::at(vec3("origin_ecef_m")?).ok_or("chart")?;
    let initial=chart.initial(vec3("initial_direction")?).ok_or("initial")?;
    let base=inputs["profile_base_height_m"].as_f64().unwrap_or(0.);
    let knots: Vec<(f64,f64)>=inputs["layers"].as_array().ok_or("layers")?.iter().map(|r|
        Ok((base+r["height_m"].as_f64().ok_or("height")?,r["refractivity"].as_f64().ok_or("index")?)))
        .collect::<Result<_,&str>>()?;
    if knots.len()<2 || !knots.windows(2).all(|k| k[0].0<k[1].0) { return Err("profile".into()); }
    let orientation=(target_height-initial.height).signum();
    let mut h=initial.height;
    let mut y=[initial.a,initial.b,initial.direction.x,initial.direction.y,initial.direction.z,0.,0.];
    let mut owner=knots.partition_point(|&(z,_)|z<h || (z==h && orientation>0.));
    let mut proposed=maximum_path.min(2000.)*initial.direction.dot(chart.up).abs();
    let mut attempts=0usize;let mut accepted=0usize;let mut contacts=0usize;
    let mut smallest_vertical=f64::INFINITY;let mut maximum_error=0_f64;
    while orientation*(target_height-h)>0. {
        attempts+=1;if attempts>20000 { return Err("attempt budget".into()); }
        let branch=if owner==0 { Branch{anchor:knots[0].0,refractivity:knots[0].1,gradient:0.} }
            else if owner>=knots.len() { Branch{anchor:knots[knots.len()-1].0,refractivity:0.,gradient:0.} }
            else { let (lo,n)=knots[owner-1];let (hi,n1)=knots[owner];Branch{anchor:lo,refractivity:n,gradient:(n1-n)/(hi-lo)} };
        let boundary=if orientation>0. { knots.get(owner).map(|k|k.0) }
            else { owner.checked_sub(1).map(|i|knots[i].0) };
        let target=boundary.filter(|b|orientation*(*b-target_height)<0.).unwrap_or(target_height);
        let remaining=(target-h).abs();
        let delta=proposed.min(remaining)*orientation;
        if delta.abs()<1e-10 { return Err(format!("step floor H{h},owner{owner}")); }
        let Some(dense)=trial(chart,h,y,delta,branch) else { proposed=delta.abs()*0.5;continue; };
        if dense.error>tolerance || dense.path_upper>maximum_path {
            proposed=delta.abs()*0.5;continue;
        }
        let mut next=dense.at(1.);
        let reaches=delta.abs()==remaining;
        let next_height=if reaches { target } else { h+delta };
        // The independent-variable endpoint is prescribed. Charge its finite
        // subtraction/addition roundtrip rather than inventing a source root.
        let correction=(next_height-(h+delta)).abs();
        let charge=dense.endpoint_coordinate_charge(h,correction);
        if dense.error+charge>tolerance { proposed=delta.abs()*0.5;continue; }
        let u=state(next_height,next).direction.normalized().ok_or("direction")?;
        next[2]=u.x;next[3]=u.y;next[4]=u.z;
        h=next_height;y=next;accepted+=1;smallest_vertical=smallest_vertical.min(dense.vertical_lower);
        maximum_error=maximum_error.max(dense.error+charge);
        if reaches && boundary==Some(target) {
            if orientation>0. {owner+=1;}else{owner-=1;}contacts+=1;
        }
        // Proposal only: every following trial still passes all unchanged
        // error, monotonicity and path-length gates. Keep an untruncated
        // proposal at an exactly prescribed source endpoint.
        let factor=if dense.error==0. {2.} else {(0.9*(tolerance/dense.error).powf(0.2)).clamp(0.5,2.)};
        proposed=(if reaches {proposed.max(delta.abs()*factor)} else {delta.abs()*factor}).min(maximum_path);
    }
    let p=chart.position(state(h,y)).ok_or("position")?;
    Ok(json!({"endpoint_ecef_m":[p.x,p.y,p.z],"exit_direction":[y[2],y[3],y[4]],
        "geometric_path_m":y[5],"optical_path_m":y[5]+y[6],"attempts":attempts,"accepted_steps":accepted,
        "layer_contacts":contacts,"minimum_certified_vertical":smallest_vertical,
        "maximum_local_error_metric_m":maximum_error,"target_height_m":target_height,
        "maximum_path_step_m":maximum_path,"tolerance_m":tolerance,"height_root_calls":0}))
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let input=std::env::args().nth(1).ok_or("reference JSON")?;
    let output=std::env::args().nth(2).ok_or("output JSON")?;
    let source:Value=serde_json::from_slice(&std::fs::read(&input)?)?;
    let mut results=Vec::new();
    for case in source["cases"].as_array().ok_or("cases")?{
        let endpoint=case["reference"]["endpoint_ecef_m"].as_array().ok_or("endpoint")?;
        let p=Vec3d::new(endpoint[0].as_f64().ok_or("x")?,endpoint[1].as_f64().ok_or("y")?,endpoint[2].as_f64().ok_or("z")?);
        // Actual-column references terminate at the declared profile top.
        // Re-inverting their numerical endpoint is not a new target boundary.
        let target=if case["inputs"]["target_path_m"].is_number() {
            Chart::at(p).ok_or("targetheight")?.height
        }else{
            case["inputs"]["profile_base_height_m"].as_f64().unwrap_or(0.)
                +case["inputs"]["layers"].as_array().ok_or("layers")?.last().ok_or("last layer")?["height_m"].as_f64().ok_or("height")?
        };
        for maximum in [2000.,20000.,100000.] {
            let result=integrate(&case["inputs"],target,maximum,0.001);
            results.push(json!({"name":case["name"],"inputs":case["inputs"],"reference":case["reference"],
                "maximum_step_m":maximum,"result":result.as_ref().ok(),"error":result.as_ref().err()}));
        }
    }
    std::fs::write(output,serde_json::to_string_pretty(&json!({"scope":"Unused f64 monotone-height parameter diagnostic at independently supplied endpoint heights. Same physical field and local metre metric; near-turning cases require the retained arclength solver and are not silently approximated. No scene or GPU acceptance.","input":input,"cases":results}))?+"\n")?;
    Ok(())
}
