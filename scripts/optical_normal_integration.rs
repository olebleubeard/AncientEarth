//! Original, unused f64 diagnostic of the six-state normal-coordinate ODE.
//! Read independently prepared Cartesian/DOP853 reference inputs; no GPU use.
#[path = "optical_normal_coordinates.rs"]
mod coordinates;
use coordinates::{Chart, State};
use earth_math::{Vec3d, WGS84_A_M, WGS84_E2};
use serde_json::{Value, json};

#[cfg(test)]
#[path = "optical_normal_root_review.rs"]
mod review;
type Y = [f64; 7]; // H,a,b,u.xyz plus diagnostic refractivity path integral.
const A: [&[f64]; 7] = [
    &[],
    &[1. / 5.],
    &[3. / 40., 9. / 40.],
    &[44. / 45., -56. / 15., 32. / 9.],
    &[
        19372. / 6561.,
        -25360. / 2187.,
        64448. / 6561.,
        -212. / 729.,
    ],
    &[
        9017. / 3168.,
        -355. / 33.,
        46732. / 5247.,
        49. / 176.,
        -5103. / 18656.,
    ],
    &[
        35. / 384.,
        0.,
        500. / 1113.,
        125. / 192.,
        -2187. / 6784.,
        11. / 84.,
    ],
];
const E: Y = [
    71. / 57600.,
    0.,
    -71. / 16695.,
    71. / 1920.,
    -17253. / 339200.,
    22. / 525.,
    -1. / 40.,
];
const D: Y = [
    -12715105075. / 11282082432.,
    0.,
    87487479700. / 32700410799.,
    -10690763975. / 1880347072.,
    701980252875. / 199316789632.,
    -1453857185. / 822651844.,
    69997945. / 29380423.,
];
#[derive(Clone, Copy)]
struct Branch {
    anchor: f64,
    n: f64,
    slope: f64,
}
#[derive(Clone)]
struct Profile {
    knots: Vec<(f64, f64)>,
}
impl Profile {
    fn branch(&self, owner: usize) -> Branch {
        if owner == 0 {
            return Branch {
                anchor: self.knots[0].0,
                n: self.knots[0].1,
                slope: 0.,
            };
        }
        if owner == self.knots.len() {
            return Branch {
                anchor: self.knots[owner - 1].0,
                n: 0.,
                slope: 0.,
            };
        }
        let (h, n) = self.knots[owner - 1];
        let (h1, n1) = self.knots[owner];
        Branch {
            anchor: h,
            n,
            slope: (n1 - n) / (h1 - h),
        }
    }
    fn owner(&self, h: f64, vertical: f64) -> usize {
        self.knots
            .partition_point(|&(z, _)| z < h || (z == h && vertical >= 0.))
    }
}
fn state(y: Y) -> State {
    State {
        height: y[0],
        a: y[1],
        b: y[2],
        direction: Vec3d::new(y[3], y[4], y[5]),
    }
}
fn array(s: State, optical: f64) -> Y {
    [
        s.height,
        s.a,
        s.b,
        s.direction.x,
        s.direction.y,
        s.direction.z,
        optical,
    ]
}
fn rhs(chart: Chart, y: Y, branch: Branch) -> Option<Y> {
    let r = branch.n + branch.slope * (y[0] - branch.anchor);
    Some(array(chart.derivative(state(y), 1. + r, branch.slope)?, r))
}
fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}
#[derive(Clone, Copy)]
struct Dense {
    start: Y,
    r: [Y; 4],
    step: f64,
    error: f64,
}
#[derive(Clone, Copy)]
struct Bound(f64, f64);
impl Bound {
    fn exact(x: f64) -> Self {
        Self(x, x)
    }
    fn add(self, b: Self) -> Self {
        Self((self.0 + b.0).next_down(), (self.1 + b.1).next_up())
    }
    fn sub(self, b: Self) -> Self {
        Self((self.0 - b.1).next_down(), (self.1 - b.0).next_up())
    }
    fn mul(self, b: Self) -> Self {
        let p = [self.0 * b.0, self.0 * b.1, self.1 * b.0, self.1 * b.1];
        Self(
            p.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            p.into_iter().fold(f64::NEG_INFINITY, f64::max).next_up(),
        )
    }
    fn absolute(self) -> f64 {
        self.0.abs().max(self.1.abs())
    }
    fn sign(self) -> i32 {
        if self.0 > 0. {
            1
        } else if self.1 < 0. {
            -1
        } else {
            0
        }
    }
}
#[derive(Clone, Copy)]
struct Root {
    distance: f64,
    path_error: f64,
    height_error: f64,
}
impl Dense {
    fn at(&self, t: f64) -> Y {
        let x = t / self.step;
        let z = 1. - x;
        std::array::from_fn(|i| {
            self.start[i]
                + x * (self.r[0][i] + z * (self.r[1][i] + x * (self.r[2][i] + z * self.r[3][i])))
        })
    }
    fn derivative(&self, t: f64) -> Y {
        let x = t / self.step;
        std::array::from_fn(|i| {
            (self.r[0][i]
                + (1. - 2. * x) * self.r[1][i]
                + (2. * x - 3. * x * x) * self.r[2][i]
                + (2. * x - 6. * x * x + 4. * x * x * x) * self.r[3][i])
                / self.step
        })
    }
    fn height_acceleration(&self) -> f64 {
        (2. * self.r[1][0].abs() + 4. * self.r[2][0].abs() + 2. * self.r[3][0].abs())
            / self.step.powi(2)
    }
    fn height_speed(&self) -> f64 {
        self.r.iter().map(|r| r[0].abs()).sum::<f64>() / self.step
    }
    fn height_residual(&self, t: f64, target: f64) -> Bound {
        let theta = t / self.step;
        let x = Bound(theta.next_down(), theta.next_up());
        let z = Bound::exact(1.).sub(x);
        let c = self.r.map(|r| Bound::exact(r[0]));
        Bound::exact(self.start[0])
            .sub(Bound::exact(target))
            .add(x.mul(c[0].add(z.mul(c[1].add(x.mul(c[2].add(z.mul(c[3]))))))))
    }
    fn physical_speed(&self) -> f64 {
        let q1 = self.r.iter().map(|r| norm(&r[1..3])).sum::<f64>() / self.step;
        let h0 = self.start[0].abs() + self.r.iter().map(|r| r[0].abs()).sum::<f64>();
        self.height_speed() + (WGS84_A_M / (1. - WGS84_E2).sqrt() + h0) * q1
    }
    fn contact_speed(&self) -> Option<f64> {
        // Contact-time uncertainty is measured in the same metre-scaled
        // position/direction/path norm as the trial estimator, not position
        // alone. The positive projection bounds the raw direction norm over
        // this complete represented quartic.
        let axis = state(self.start).direction.normalized()?;
        let project = |v: Y| axis.dot(Vec3d::new(v[3], v[4], v[5]));
        let r = self.r.map(project);
        let c0 = project(self.start);
        let c1 = r[0] + r[1];
        let c2 = r[2] - r[1] + r[3];
        let c3 = -r[2] - 2.0 * r[3];
        let c4 = r[3];
        let lower = [
            c0,
            c0 + c1 / 4.,
            c0 + c1 / 2. + c2 / 6.,
            c0 + 3. * c1 / 4. + c2 / 2. + c3 / 4.,
            c0 + c1 + c2 + c3 + c4,
        ].into_iter().fold(f64::INFINITY, f64::min)
            - 128. * f64::EPSILON * (c0.abs() + r.iter().map(|x| x.abs()).sum::<f64>());
        if !(lower > 0.0) { return None; }
        let raw_speed = self.r.iter().map(|r| norm(&r[3..6])).sum::<f64>() / self.step;
        let optical_speed = 1. + self.r.iter().map(|r| r[6].abs()).sum::<f64>() / self.step;
        let bound = self.physical_speed().max(optical_speed).max(2. * self.step * raw_speed / lower);
        bound.is_finite().then_some(bound)
    }
    fn departure_safe(&self, direction: f64) -> bool {
        // At an owned knot H(0)=target. Divide the exact represented quartic
        // residual by theta>0 and bound the remaining cubic in Bernstein form.
        // This certifies a tangent departure without inventing a nonzero slope.
        let r = self.r.map(|x| Bound::exact(x[0]));
        let c1 = if self.r[0][0] == -self.r[1][0] {
            Bound::exact(0.)
        } else {
            r[0].add(r[1])
        };
        let c2 = r[2].sub(r[1]).add(r[3]);
        let c3 = Bound::exact(0.).sub(r[2]).sub(r[3].mul(Bound::exact(2.)));
        let c4 = r[3];
        let third = Bound((1_f64 / 3.).next_down(), (1_f64 / 3.).next_up());
        let two_thirds = Bound((2_f64 / 3.).next_down(), (2_f64 / 3.).next_up());
        let coefficients = [
            c1,
            c1.add(c2.mul(third)),
            c1.add(c2.mul(two_thirds)).add(c3.mul(third)),
            c1.add(c2).add(c3).add(c4),
        ];
        let signed = coefficients.map(|v| if direction > 0. { v } else { Bound(-v.1, -v.0) });
        signed.iter().all(|v| v.0 >= 0.) && signed[3].0 > 0.
    }
    fn height_root(&self, target: f64, allocation: f64) -> Option<Root> {
        let mut lo = 0.;
        let mut hi = self.step;
        let low_residual = self.height_residual(lo, target);
        let high_residual = self.height_residual(hi, target);
        let start = low_residual.sign();
        let end = high_residual.sign();
        if start == 0 || end == 0 || start == end || allocation <= 0. {
            return None;
        }
        let speed = self.contact_speed()?;
        if !(speed > 0. && speed.is_finite()) {
            return None;
        }
        let derivative = self.derivative(0.)[0];
        let derivative_round = 128. * f64::EPSILON * self.height_speed();
        let slope = derivative.abs() - derivative_round - self.height_acceleration() * self.step;
        if !(slope > 0.) {
            return None;
        }
        let f0 = (low_residual.0 + low_residual.1) * 0.5;
        let f1 = (high_residual.0 + high_residual.1) * 0.5;
        let mut m = (self.step * f0 / (f0 - f1)).clamp(lo, hi);
        for _ in 0..64 {
            let residual = self.height_residual(m, target);
            let path_error = (m - lo).max(hi - m).min(residual.absolute() / slope);
            if speed * path_error + residual.absolute() <= allocation {
                // One additional safeguarded scalar proposal often makes the
                // accepted contact much tighter at negligible cost. It never
                // replaces a certified value with a worse uncertainty bound.
                let value = (residual.0 + residual.1) * 0.5;
                let proposed = m - value / self.derivative(m)[0];
                if proposed > lo && proposed < hi {
                    let refined = self.height_residual(proposed, target);
                    let refined_error = (proposed - lo)
                        .max(hi - proposed)
                        .min(refined.absolute() / slope);
                    if speed * refined_error + refined.absolute()
                        < speed * path_error + residual.absolute()
                    {
                        return Some(Root {
                            distance: proposed,
                            path_error: refined_error,
                            height_error: refined.absolute(),
                        });
                    }
                }
                return Some(Root {
                    distance: m,
                    path_error,
                    height_error: residual.absolute(),
                });
            }
            let sign = residual.sign();
            if sign == 0 {
                return None;
            } // finite floor: retry from a closer start
            if sign == start {
                lo = m;
            } else {
                hi = m;
            }
            let value = (residual.0 + residual.1) * 0.5;
            let next = m - value / self.derivative(m)[0];
            let proposal = if next > lo && next < hi {
                next
            } else {
                (lo + hi) * 0.5
            };
            if proposal == m {
                return None;
            }
            m = proposal;
        }
        None
    }
    fn physical_acceleration(&self) -> Option<f64> {
        let q1 = self.r.iter().map(|r| norm(&r[1..3])).sum::<f64>() / self.step;
        let q2 = (2. * norm(&self.r[1][1..3])
            + 4. * norm(&self.r[2][1..3])
            + 2. * norm(&self.r[3][1..3]))
            / self.step.powi(2);
        Chart::acceleration_bound(
            q1,
            q2,
            self.start[0].abs() + self.r.iter().map(|r| r[0].abs()).sum::<f64>(),
            self.height_speed(),
            self.height_acceleration(),
        )
    }
}
fn trial(chart: Chart, y: Y, step: f64, branch: Branch) -> Option<Dense> {
    let mut k = [[0.; 7]; 7];
    let mut high = y;
    for i in 0..7 {
        let stage = std::array::from_fn(|axis| {
            y[axis]
                + step
                    * A[i]
                        .iter()
                        .enumerate()
                        .map(|(j, a)| a * k[j][axis])
                        .sum::<f64>()
        });
        k[i] = rhs(chart, stage, branch)?;
        if i == 6 {
            high = stage;
        }
    }
    let delta: Y = std::array::from_fn(|i| {
        step * A[6]
            .iter()
            .enumerate()
            .map(|(j, a)| a * k[j][i])
            .sum::<f64>()
    });
    let r3: Y = std::array::from_fn(|i| k[0][i] * step - delta[i]);
    let r4: Y = std::array::from_fn(|i| delta[i] - k[6][i] * step - r3[i]);
    let r5: Y = std::array::from_fn(|i| {
        step * D
            .iter()
            .enumerate()
            .map(|(j, d)| d * (k[j][i] - k[0][i]))
            .sum::<f64>()
    });
    let embedded: Y = std::array::from_fn(|i| {
        step * E
            .iter()
            .enumerate()
            .map(|(j, e)| e * (k[j][i] - k[0][i]))
            .sum::<f64>()
    });
    let radius = WGS84_A_M / (1. - WGS84_E2).sqrt()
        + y[0].abs()
        + delta[0].abs()
        + r3[0].abs()
        + r4[0].abs()
        + r5[0].abs()
        + embedded[0].abs();
    let axis = state(y).direction.normalized()?;
    let projected = |v: Y| axis.dot(Vec3d::new(v[3], v[4], v[5]));
    let c0 = projected(y);
    let c1 = projected(delta) + projected(r3);
    let c2 = projected(r4) - projected(r3) + projected(r5);
    let c3 = -projected(r4) - 2. * projected(r5);
    let c4 = projected(r5);
    let q2 = c2 - c4;
    let q3 = c3 + 2. * c4;
    let lower = [
        c0,
        c0 + c1 / 4.,
        c0 + c1 / 2. + c2 / 6.,
        c0 + 3. * c1 / 4. + c2 / 2. + c3 / 4.,
        c0 + c1 + c2 + c3 + c4,
        c0 + c1 / 2. + q2 / 6.,
        c0 + 3. * c1 / 4. + q2 / 2. + q3 / 4.,
        projected(high) - projected(embedded),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min);
    if lower <= 0. {
        return None;
    }
    let metric = |v: Y| {
        (v[0].abs() + radius * norm(&v[1..3]))
            .max(2. * step * norm(&v[3..6]) / lower)
            .max(v[6].abs())
    };
    let error = metric(embedded) + metric(r5) / 16.;
    if !high.iter().all(|x| x.is_finite()) || !error.is_finite() {
        return None;
    }
    Some(Dense {
        start: y,
        r: [delta, r3, r4, r5],
        step,
        error,
    })
}
pub fn integrate(inputs: &Value, maximum: f64, tolerance: f64) -> Result<Value, String> {
    let vec3 = |key: &str| -> Result<Vec3d, String> {
        let a = inputs[key].as_array().ok_or(key.to_owned())?;
        Ok(Vec3d::new(
            a[0].as_f64().ok_or(key)?,
            a[1].as_f64().ok_or(key)?,
            a[2].as_f64().ok_or(key)?,
        ))
    };
    let chart = Chart::at(vec3("origin_ecef_m")?).ok_or("chart")?;
    let initial = chart
        .initial(vec3("initial_direction")?)
        .ok_or("direction")?;
    let base = inputs["profile_base_height_m"].as_f64().unwrap_or(0.);
    let profile = Profile {
        knots: inputs["layers"]
            .as_array()
            .ok_or("layers")?
            .iter()
            .map(|r| {
                Ok((
                    r["height_m"].as_f64().ok_or("height")? + base,
                    r["refractivity"].as_f64().ok_or("refractivity")?,
                ))
            })
            .collect::<Result<Vec<_>, &str>>()?,
    };
    let total = inputs["geometric_path_m"].as_f64().ok_or("path")?;
    let mut y = array(initial, 0.);
    let mut path = 0.;
    let mut proposed = maximum.min(2000.);
    let mut owner = profile.owner(y[0], initial.direction.dot(chart.up));
    let mut attempts = 0;
    let mut accepted = 0;
    let mut contacts = 0;
    let mut last_departure = 0_i32;
    let mut maximum_acceleration = 0_f64;
    while path < total {
        attempts += 1;
        if attempts > 20000 {
            return Err(format!(
                "attempt budget at path{path},H{},owner{owner},step{proposed}",
                y[0]
            ));
        }
        let step = proposed.min(total - path);
        if step < 1e-9 {
            return Err("step floor".into());
        }
        let Some(dense) = trial(chart, y, step, profile.branch(owner)) else {
            proposed = step * 0.5;
            continue;
        };
        if dense.error > tolerance {
            proposed = step * 0.5;
            continue;
        }
        let end = dense.at(step);
        let acceleration = dense.height_acceleration();
        let derivative = dense.derivative(0.)[0];
        let mut contact: Option<(Root, usize)> = None;
        let mut safe = true;
        for index in [
            owner.checked_sub(1),
            (owner < profile.knots.len()).then_some(owner),
        ]
        .into_iter()
        .flatten()
        {
            let target = profile.knots[index].0;
            let f0 = y[0] - target;
            let f1 = end[0] - target;
            if f0 == 0. {
                let departure = if index < owner { 1. } else { -1. };
                if departure * derivative <= acceleration * step && !dense.departure_safe(departure)
                {
                    safe = false;
                    break;
                }
                continue;
            }
            if (f0 > 0.) != (f1 > 0.) || f1 == 0. {
                // Strict transversality makes this the first crossing of this
                // source boundary on the complete candidate polynomial.
                if derivative.abs() <= acceleration * step {
                    safe = false;
                    break;
                }
                let Some(t) = dense.height_root(target, (tolerance - dense.error) / 8.) else {
                    safe = false;
                    break;
                };
                if contact.is_none_or(|c| t.distance < c.0.distance) {
                    contact = Some((t, index));
                }
            } else if derivative.abs() <= acceleration * step
                && f0.abs().min(f1.abs()) <= acceleration * step * step / 8.
            {
                safe = false;
                break;
            }
        }
        if !safe {
            proposed = step * 0.5;
            continue;
        }
        let distance = contact.map_or(step, |c| c.0.distance);
        let mut next = dense.at(distance);
        if let Some((root, index)) = contact {
            // Exact coordinate event with explicitly charged root/height
            // allocation; no independent H/position variables can diverge.
            if dense.error + root.height_error + dense.contact_speed().ok_or("contact speed")? * root.path_error
                > tolerance
            {
                proposed = step * 0.5;
                continue;
            }
            next[0] = profile.knots[index].0;
            let vertical = dense.derivative(distance)[0];
            if vertical == 0. {
                return Err("tangent source event".into());
            }
            owner = if vertical > 0. { index + 1 } else { index };
            last_departure = if vertical > 0. { 1 } else { -1 };
            contacts += 1;
        }
        let u = state(next)
            .direction
            .normalized()
            .ok_or("accepted direction")?;
        next[3] = u.x;
        next[4] = u.y;
        next[5] = u.z;
        if chart.position(state(next)).is_none() {
            return Err("position reconstruction".into());
        }
        maximum_acceleration =
            maximum_acceleration.max(dense.physical_acceleration().ok_or("curve bound")?);
        y = next;
        path += distance;
        accepted += 1;
        proposed = if dense.error < tolerance / 32. {
            (step * 2.).min(maximum)
        } else {
            step
        };
        if total - path < 1e-8 {
            path = total;
        }
    }
    let p = chart.position(state(y)).ok_or("finalposition")?;
    Ok(
        json!({"endpoint_ecef_m":[p.x,p.y,p.z],"exit_direction":[y[3],y[4],y[5]],"geometric_path_m":path,
        "optical_path_m":path+y[6],"attempts":attempts,"accepted_steps":accepted,"layer_contacts":contacts,
        "last_departure":last_departure,"maximum_dense_acceleration_bound":maximum_acceleration,
        "stage_coordinate_inverse_calls":0,"maximum_step_m":maximum,"tolerance_m":tolerance}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args().nth(1).ok_or("reference input JSON")?;
    let output = std::env::args().nth(2).ok_or("output JSON")?;
    let source: Value = serde_json::from_slice(&std::fs::read(&input)?)?;
    let mut results = Vec::new();
    for case in source["cases"].as_array().ok_or("cases")? {
        let mut inputs = case["inputs"].clone();
        inputs["geometric_path_m"] = case["reference"]["geometric_path_m"].clone();
        for maximum in [2000., 20000., 100000.] {
            let result = integrate(&inputs, maximum, 0.001);
            results.push(json!({"name":case["name"],"inputs":inputs,"reference":case["reference"],"maximum_step_m":maximum,
                "result":result.as_ref().ok(),"error":result.as_ref().err()}));
        }
    }
    std::fs::write(
        output,
        serde_json::to_string_pretty(
            &json!({"scope":"Unused f64 six-state WGS84 normal-chart optical prototype at fixed independently supplied path lengths. One-sided source events and conservative physical dense-curve bounds; no GPU/scene/performance acceptance.","input":input,"cases":results}),
        )? + "\n",
    )?;
    Ok(())
}
