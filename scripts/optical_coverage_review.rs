//! Independent finite scalar review of optical_coverage_local.wgsl.
//! Compile with `rustc --test scripts/optical_coverage_review.rs`.
//! This exercises declared f32 arithmetic, not native shader execution or a
//! universal rounding proof. Reference membership uses f64 line/barycentric math.
use std::ops::{Add, Div, Mul, Sub};

#[derive(Clone, Copy, Debug)]
struct V([f32; 3]);
impl Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + b.0[i]))
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] - b.0[i]))
    }
}
impl Mul<f32> for V {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self(self.0.map(|x| x * b))
    }
}
impl Div<f32> for V {
    type Output = Self;
    fn div(self, b: f32) -> Self {
        Self(self.0.map(|x| x / b))
    }
}
impl V {
    fn dot(self, b: Self) -> f32 {
        self.0[0] * b.0[0] + self.0[1] * b.0[1] + self.0[2] * b.0[2]
    }
    fn cross(self, b: Self) -> Self {
        let [x, y, z] = self.0;
        let [u, v, w] = b.0;
        Self([y * w - z * v, z * u - x * w, x * v - y * u])
    }
    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
}
fn gamma(n: f32) -> f32 {
    let u = f32::EPSILON / 2.;
    n * u / (1. - n * u)
}
fn finite(x: f32) -> bool {
    x.abs() <= 3e38
}
fn cross_error(a: V, b: V, ea: f32, eb: f32) -> f32 {
    a.length() * eb + b.length() * ea + ea * eb + 2. * gamma(2.) * a.length() * b.length()
}
fn dot_error(a: V, b: V, ea: f32, eb: f32) -> f32 {
    a.length() * eb + b.length() * ea + ea * eb + gamma(3.) * a.length() * b.length()
}

// Scalar transcription of the shader's finite arithmetic envelope. The oracle
// below uses neither these errors nor the shader's oriented edge predicate.
fn coverage(x: V, v: V, ex: f32, ev: f32, vertices: [V; 3], source_error: f32) -> u32 {
    let [va, vb, vc] = vertices;
    let a = V([0.; 3]);
    let b = vb - va;
    let c = vc - va;
    let q = x - va;
    let eb = 2. * source_error + gamma(1.) * (vb.length() + va.length());
    let ec = 2. * source_error + gamma(1.) * (vc.length() + va.length());
    let eq = ex + source_error + gamma(1.) * (x.length() + va.length());
    let raw = b.cross(c);
    let area = raw.length();
    let ce = cross_error(b, c, eb, ec);
    if !(area > ce) || !finite(area) {
        return 2;
    }
    let n = raw / area;
    let en = 2. * ce / (area - ce) + gamma(16.);
    let numerator = n.dot(q);
    let denominator = n.dot(v);
    let ne = dot_error(n, q, en, eq);
    let de = dot_error(n, v, en, ev);
    if !(denominator.abs() > de) || !finite(de) {
        return 2;
    }
    let distance = -numerator / denominator;
    let distance_error =
        (ne + distance.abs() * de + gamma(1.) * distance.abs() * denominator.abs())
            / (denominator.abs() - de);
    if !finite(distance_error) || !finite(distance) {
        return 2;
    }
    if distance + distance_error < -10001. || distance - distance_error > 101000. {
        return 0;
    }
    if distance - distance_error < -10001. || distance + distance_error > 101000. {
        return 2;
    }
    let at = q + v * distance;
    let ae = eq
        + distance.abs() * ev
        + distance_error * (v.length() + ev)
        + gamma(2.) * (q.length() + distance.abs() * v.length());
    let points = [a, b, c];
    let errors = [0., eb, ec];
    let mut uncertain = false;
    for i in 0..3 {
        let j = (i + 1) % 3;
        let edge = points[j] - points[i];
        let w = at - points[i];
        let ee = errors[i] + errors[j] + gamma(1.) * (points[i].length() + points[j].length());
        let we = ae + errors[i] + gamma(1.) * (at.length() + points[i].length());
        let product = edge.cross(w);
        let pe = cross_error(edge, w, ee, we);
        let signed = product.dot(n);
        let se = dot_error(product, n, pe, en);
        if !finite(se) || !finite(signed) {
            return 2;
        }
        if signed < -se {
            return 0;
        }
        if signed <= se {
            uncertain = true;
        }
    }
    if uncertain { 2 } else { 1 }
}

fn reference(x: V, v: V, vertices: [V; 3]) -> Option<(f64, [f64; 3])> {
    let widen = |a: V| a.0.map(f64::from);
    let sub = |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|i| a[i] - b[i]);
    let dot = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let [a, b, c] = vertices.map(widen);
    let ab = sub(b, a);
    let ac = sub(c, a);
    let n = cross(ab, ac);
    let d = dot(n, widen(v));
    if d == 0. {
        return None;
    }
    let t = -dot(n, sub(widen(x), a)) / d;
    let p = std::array::from_fn(|i| f64::from(x.0[i]) + t * f64::from(v.0[i]) - a[i]);
    let aa = dot(ab, ab);
    let bb = dot(ab, ac);
    let cc = dot(ac, ac);
    let den = aa * cc - bb * bb;
    if den <= 0. {
        return None;
    }
    let beta = (cc * dot(p, ab) - bb * dot(p, ac)) / den;
    let eta = (aa * dot(p, ac) - bb * dot(p, ab)) / den;
    Some((t, [1. - beta - eta, beta, eta]))
}
fn aggregate(values: impl IntoIterator<Item = u32>) -> u32 {
    let mut uncertain = false;
    for value in values {
        if value == 1 {
            return 1;
        }
        uncertain |= value == 2;
    }
    if uncertain { 2 } else { 0 }
}

fn old_long_segment_contains(x: V, v: V, face: [V; 3]) -> bool {
    let [a, b, c] = face;
    let raw = (b - a).cross(c - a);
    let n = raw / raw.length();
    let start = x + v * 101000.;
    let end = x - v * 10001.;
    let before = (start - a).dot(n);
    let after = (end - a).dot(n);
    let at = start + (end - start) * (before / (before - after));
    (0..3).all(|i| (face[(i + 1) % 3] - face[i]).cross(at - face[i]).dot(n) >= 0.)
}

#[test]
fn translated_interiors_and_outside_points_follow_independent_barycentrics() {
    for shift in [
        V([0.; 3]),
        V([65000., -48000., 1200.]),
        V([-127000., 22000., -3500.]),
    ] {
        for scale in [1., 10., 100.] {
            let vertices = [
                shift,
                shift + V([100., 0., 0.]) * scale,
                shift + V([0., 100., 0.]) * scale,
            ];
            let velocity = V([0.2, -0.1, -1.]);
            for (position, expect) in [(V([25., 25., 50.]), 1), (V([150., 25., 50.]), 0)] {
                let x = shift + position * scale;
                let (distance, weights) = reference(x, velocity, vertices).unwrap();
                assert!((-10001. ..=101000.).contains(&distance));
                assert_eq!(weights.iter().all(|&b| b > 0.), expect == 1);
                for face in [vertices, [vertices[2], vertices[1], vertices[0]]] {
                    assert_eq!(
                        coverage(x, velocity, 0., gamma(16.), face, 0.001),
                        expect,
                        "{shift:?} {scale} {position:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn shared_edges_stay_uncertain_and_do_not_hide_a_later_strict_interior() {
    for shift in [
        V([0.; 3]),
        V([65000., -48000., 1200.]),
        V([-127000., 22000., -3500.]),
    ] {
        let a = shift;
        let b = shift + V([100., 0., 0.]);
        let c = shift + V([100., 100., 0.]);
        let d = shift + V([0., 100., 0.]);
        let x = shift + V([50., 50., 40.]);
        let v = V([0., 0., -1.]);
        let faces = [[a, b, c], [a, c, d]];
        for face in faces {
            let (_, weights) = reference(x, v, face).unwrap();
            assert!(weights.iter().any(|&b| b == 0.));
            assert_eq!(coverage(x, v, 0., gamma(16.), face, 0.001), 2);
        }
        assert_eq!(
            aggregate(faces.map(|f| coverage(x, v, 0., gamma(16.), f, 0.001))),
            2
        );
        let covering = [
            shift + V([-100., -100., 0.]),
            shift + V([300., -100., 0.]),
            shift + V([-100., 300., 0.]),
        ];
        let certain = coverage(x, v, 0., gamma(16.), covering, 0.001);
        assert_eq!(certain, 1);
        assert_eq!(aggregate([2, certain]), 1);
        assert_eq!(aggregate([certain, 2]), 1);
        assert_eq!(aggregate([0, 2, 0]), 2);
        assert_eq!(aggregate([0, 0]), 0);
    }
    // Fixed finite counterexample: both former 111 km interpolations reject,
    // but the represented line intersects the second represented triangle.
    // The new policy must retain uncertainty instead of inventing an h0 hole.
    let x = V([65057.41796875, -47947.01171875, 1223.685791015625]);
    let v = V([0.29678812623023987, 0.11955760419368744, 0.9474295973777771]);
    let a = V([65000., -48000., 1200.]);
    let b = V([65100., -48000., 1174.3323974609375]);
    let c = V([65100., -47900., 1200.]);
    let d = V([65000., -47900., 1155.716064453125]);
    let faces = [[a, b, c], [a, c, d]];
    assert!(reference(x, v, faces[1]).unwrap().1.iter().all(|&w| w > 0.));
    assert!(!old_long_segment_contains(x, v, faces[0]));
    assert!(!old_long_segment_contains(x, v, faces[1]));
    assert_eq!(
        aggregate(faces.map(|f| coverage(x, v, 0., gamma(16.), f, 0.001))),
        2
    );
}

#[test]
fn near_parallel_degenerate_and_interval_edge_cases_fail_closed() {
    let face = [V([0., 0., 0.]), V([100., 0., 0.]), V([0., 100., 0.])];
    assert_eq!(
        coverage(
            V([20., 20., 1.]),
            V([1., 0., 1e-8]),
            0.,
            gamma(16.),
            face,
            0.
        ),
        2
    );
    assert_eq!(
        coverage(V([20., 20., 1.]), V([1., 0., 0.]), 0., gamma(16.), face, 0.),
        2
    );
    assert_eq!(
        coverage(
            V([20., 20., 1.]),
            V([0., 0., -1.]),
            0.,
            0.,
            [face[0]; 3],
            0.
        ),
        2
    );
    assert_eq!(
        coverage(V([20., 20., 1.]), V([0., 0., -1.]), 0., 0., face, 100.),
        2
    );
    assert_eq!(
        coverage(V([20., 20., f32::NAN]), V([0., 0., -1.]), 0., 0., face, 0.),
        2
    );
    assert_eq!(
        coverage(V([20., 20., 1.]), V([0., 0., -1.]), 0., 0., face, f32::NAN),
        2
    );
    assert_eq!(
        coverage(V([20., 20., -101000.]), V([0., 0., 1.]), 0., 0., face, 0.),
        2
    );
    assert_eq!(
        coverage(V([20., 20., -102000.]), V([0., 0., 1.]), 0., 0., face, 0.),
        0
    );
    assert_eq!(
        coverage(V([20., 20., 10001.]), V([0., 0., 1.]), 0., 0., face, 0.),
        2
    );
    assert_eq!(
        coverage(V([20., 20., 11000.]), V([0., 0., 1.]), 0., 0., face, 0.),
        0
    );
}
