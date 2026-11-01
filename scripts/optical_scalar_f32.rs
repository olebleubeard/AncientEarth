//! Unused f32 scalar-seed experiment. Never replaces production geometry.
//! Inputs retain the actual uploaded rows and the original local implicit
//! evaluation. The ideal rank-one calculation is only a normal seed.
use earth_math::atmosphere::gpu::GpuEllipsoid;

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    // GPU counterpart explicitly uses inverseSqrt(dot(v,v))*v. Its WGSL
    // accuracy allowance is bounded separately; this CPU evaluation is only
    // one binary32 realization, not an emulator of every GPU inverseSqrt.
    let reciprocal = 1.0 / length(v);
    v.map(|x| x * reciprocal)
}
pub fn query(packet: &GpuEllipsoid, p: [f32; 3]) -> ([f32; 3], f32) {
    let mp = packet.rows.map(|r| dot([r[0], r[1], r[2]], p));
    let origin = [packet.origin[0], packet.origin[1], packet.origin[2]];
    let q = std::array::from_fn(|i| origin[i] + mp[i]);
    let f = packet.origin[3] + dot(p, std::array::from_fn(|i| 2.0 * origin[i] + mp[i]));
    (q, f)
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub normal: [f32; 3],
    pub height: f32,
    pub input_normal: [f32; 3],
    pub line_root_residual: f32,
    pub normal_change: f32,
    pub refinements: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct EncodedCertificate {
    pub height_error: f64,
    pub normal_error: f64,
    pub root_error: f64,
    pub computed_normal_error: f64,
    pub minimum_normal_radius: f64,
    pub skew_norm_bound: f64,
}

/// Exact read-back values from the candidate's final GPU refinement. Supplying
/// these avoids replacing a GPU-fused operation by a CPU recomputation.
#[derive(Clone, Copy, Debug)]
pub struct EncodedQuery {
    pub q: [f32; 3],
    pub f: f32,
    pub qn: [f32; 3],
    pub v: f32,
    pub coefficient: f32,
    pub gradient: [f32; 3],
    pub input_normal_squared: f32,
}
pub fn cpu_query(packet: &GpuEllipsoid, p: [f32; 3], got: Candidate) -> EncodedQuery {
    let (q, f) = query(packet, p);
    let n = got.input_normal;
    let qn = packet.rows.map(|r| dot([r[0], r[1], r[2]], n));
    EncodedQuery {
        q,
        f,
        qn,
        v: dot(n, q),
        coefficient: dot(n, qn),
        gradient: std::array::from_fn(|i| q[i] - got.height * qn[i]),
        input_normal_squared: dot(n, n),
    }
}

/// Conservative arithmetic enclosure for review. Arithmetic below is f64 so
/// the enclosure calculation itself is not yet a GPU implementation: a port
/// must round bounds outward and use explicit inverseSqrt(dot(v,v))*v.
/// The gamma factors describe binary32 elementary arithmetic, not a universal
/// promise about every vendor compiler's fast-math implementation. WGSL's
/// inverseSqrt 2-ULP allowance plus nonnegative dot3 and multiplication is
/// enclosed by gamma8 for finite nonzero normals. Generic normalize inherits
/// a looser expression chain and must not silently replace that contract.
pub fn rounded_enclosure(
    packet: &GpuEllipsoid,
    p: [f32; 3],
    got: Candidate,
) -> Option<EncodedCertificate> {
    rounded_enclosure_encoded(packet, cpu_query(packet, p, got), got)
}
pub fn rounded_enclosure_encoded(
    packet: &GpuEllipsoid,
    query: EncodedQuery,
    got: Candidate,
) -> Option<EncodedCertificate> {
    use earth_math::Vec3d;
    let vec = |v: [f32; 3]| Vec3d::new(v[0] as f64, v[1] as f64, v[2] as f64);
    let u = 2.0_f64.powi(-24);
    let gamma = |k: f64| k * u / (1.0 - k * u);
    let q32 = query.q;
    let q = vec(q32);
    let f = query.f as f64;
    let n = got.input_normal;
    let m = vec(got.normal);
    let h = got.height as f64;
    let v = query.v as f64;
    let c = query.coefficient as f64;
    let abs_v = (0..3)
        .map(|i| (n[i] as f64 * q32[i] as f64).abs())
        .sum::<f64>();
    let abs_an: [f64; 3] = std::array::from_fn(|i| {
        (0..3)
            .map(|j| (packet.rows[i][j] as f64 * n[j] as f64).abs())
            .sum()
    });
    let abs_c = (0..3).map(|i| (n[i] as f64).abs() * abs_an[i]).sum::<f64>();
    let abs_an_norm = (abs_an.iter().map(|v| v * v).sum::<f64>()).sqrt();
    let root_residual = (got.line_root_residual as f64).abs()
        + gamma(10.0) * (f.abs() + 2.0 * h.abs() * abs_v + h * h * abs_c);
    let slope = 2.0 * (v - h * c) - gamma(8.0) * 2.0 * (abs_v + h.abs() * abs_c);
    let coefficient = (1.0 + gamma(8.0)) * abs_c;
    let discriminant = slope * slope - 4.0 * coefficient * root_residual;
    if !(slope > 0.0 && discriminant > 0.0) {
        return None;
    }
    let root_error = 2.0 * root_residual / (slope + discriminant.sqrt());
    let norm2 = query.input_normal_squared as f64;
    let unit_defect = (norm2 - 1.0).abs() + gamma(3.0) / (1.0 - gamma(3.0)) * norm2.abs();
    if !(unit_defect < 0.001) {
        return None;
    }
    let skew = (0..3)
        .map(|i| {
            (0..3)
                .map(|j| ((packet.rows[i][j] as f64 - packet.rows[j][i] as f64) * 0.5).abs())
                .sum::<f64>()
        })
        .fold(0.0, f64::max);
    let g32 = query.gradient;
    let norm_g = vec(g32).length();
    let gradient_error = gamma(2.0) * (q.length() + h.abs() * abs_an_norm)
        + h.abs() * (1.0 + gamma(2.0)) * gamma(3.0) * abs_an_norm
        + root_error * abs_an_norm
        + (h.abs() + root_error) * skew * (1.0 + unit_defect);
    if !(norm_g > gradient_error) {
        return None;
    }
    let normal_error = 2.0 * gradient_error / (norm_g - gradient_error) + gamma(8.0);
    let distance = (h.abs() + root_error)
        * (1.0 + unit_defect)
        * ((vec(n) - m).length() + unit_defect + normal_error);
    let mut low = f64::INFINITY;
    let mut high = 0.0_f64;
    for i in 0..3 {
        let off = (0..3)
            .filter(|&j| j != i)
            .map(|j| ((packet.rows[i][j] as f64 + packet.rows[j][i] as f64) * 0.5).abs())
            .sum::<f64>();
        low = low.min(packet.rows[i][i] as f64 - off);
        high = high.max(packet.rows[i][i] as f64 + off);
    }
    let level = q.dot(q) / high - f;
    let rho = (level * low).sqrt() / high;
    let denominator = rho - (h.abs() + root_error) * (1.0 + unit_defect) - distance;
    if !(low > 0.0 && level > 0.0 && denominator > 0.0) {
        return None;
    }
    Some(EncodedCertificate {
        height_error: h.abs() * unit_defect + root_error * (1.0 + unit_defect) + distance,
        normal_error: normal_error + distance / denominator,
        root_error,
        computed_normal_error: normal_error,
        minimum_normal_radius: rho,
        skew_norm_bound: skew,
    })
}

/// f64 audit of the encoded f32 query, not a GPU rounding certificate.
/// The tiny uploaded skew is reported explicitly; the reference surface uses
/// the symmetric part because a quadratic form has that symmetric gradient.
/// This construction compares a point on the same quadratic with a nearby
/// point along its exact normal. It does not mistake the old-normal line-root
/// residual for a certificate of the returned height/new-normal pair.
pub fn encoded_certificate(
    packet: &GpuEllipsoid,
    p: [f32; 3],
    got: Candidate,
) -> Option<EncodedCertificate> {
    encoded_certificate_query(packet, cpu_query(packet, p, got), got)
}
pub fn encoded_certificate_query(
    packet: &GpuEllipsoid,
    query: EncodedQuery,
    got: Candidate,
) -> Option<EncodedCertificate> {
    use earth_math::Vec3d;
    let vector = |v: [f32; 3]| Vec3d::new(v[0] as f64, v[1] as f64, v[2] as f64);
    let q = vector(query.q);
    let f = query.f as f64;
    let matrix: [[f64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| (packet.rows[i][j] as f64 + packet.rows[j][i] as f64) * 0.5)
    });
    let rows = matrix.map(|r| Vec3d::new(r[0], r[1], r[2]));
    let apply = |v: Vec3d| Vec3d::new(rows[0].dot(v), rows[1].dot(v), rows[2].dot(v));
    let eigen_low = (0..3)
        .map(|i| {
            matrix[i][i]
                - (0..3)
                    .filter(|&j| j != i)
                    .map(|j| matrix[i][j].abs())
                    .sum::<f64>()
        })
        .fold(f64::INFINITY, f64::min);
    let eigen_high = (0..3)
        .map(|i| {
            matrix[i][i]
                + (0..3)
                    .filter(|&j| j != i)
                    .map(|j| matrix[i][j].abs())
                    .sum::<f64>()
        })
        .fold(0.0, f64::max);
    let level_low = q.dot(q) / eigen_high - f;
    let rho = (level_low * eigen_low).sqrt() / eigen_high;
    if !(eigen_low > 0.0 && level_low > 0.0 && rho > 0.0) {
        return None;
    }
    let n = vector(got.input_normal);
    let n_length = n.length();
    let an = apply(n);
    let v = n.dot(q);
    let coefficient = n.dot(an);
    let discriminant = v * v - f * coefficient;
    if !(discriminant > 0.0 && n_length > 0.0) {
        return None;
    }
    let t = f / (v + discriminant.sqrt());
    let actual_normal = (q - an * t).normalized()?;
    let computed_normal = vector(got.normal);
    let normal_rounding = (computed_normal - actual_normal).length();
    let distance =
        (t * n_length).abs() * ((n / n_length - computed_normal).length() + normal_rounding);
    let denominator = rho - (t * n_length).abs() - distance;
    if !(denominator > 0.0) {
        return None;
    }
    let arithmetic = 128.0 * f64::EPSILON;
    let root_error = (got.height as f64 - t).abs();
    Some(EncodedCertificate {
        height_error: (got.height as f64 - t * n_length).abs()
            + distance
            + arithmetic * (1.0 + (got.height as f64).abs()),
        normal_error: normal_rounding + distance / denominator + arithmetic,
        root_error,
        computed_normal_error: normal_rounding,
        minimum_normal_radius: rho,
        skew_norm_bound: (0..3)
            .map(|i| {
                (0..3)
                    .map(|j| ((packet.rows[i][j] as f64 - packet.rows[j][i] as f64) * 0.5).abs())
                    .sum::<f64>()
            })
            .fold(0.0, f64::max),
    })
}

/// Two scalar Newton updates followed by `refinements` ORIGINAL actual-matrix
/// quadratic/normal iterations. No new physical field or acceptance policy.
pub fn candidate(packet: &GpuEllipsoid, p: [f32; 3], refinements: usize) -> Option<Candidate> {
    candidate_with_updates(packet, p, refinements, 2)
}
/// The certificate is independent of seed quality; bounded seed-only cost
/// comparisons may use zero/one/two updates, retaining the same final guard.
pub fn candidate_with_updates(
    packet: &GpuEllipsoid,
    p: [f32; 3],
    refinements: usize,
    scalar_updates: usize,
) -> Option<Candidate> {
    if scalar_updates > 2 {
        return None;
    }
    if refinements == 0 {
        return None;
    }
    let (q, f) = query(packet, p);
    let alpha = (1.0 / earth_math::WGS84_A_M) as f32;
    let beta = (earth_math::WGS84_A_M / earth_math::WGS84_B_M.powi(2)) as f32;
    let axis = normalize([0.0, packet.latitude[0], -packet.latitude[1]]);
    if !f.is_finite() || length(q) <= 0.75 {
        return None;
    }
    let z = dot(q, axis);
    let transverse = std::array::from_fn(|i| q[i] - axis[i] * z);
    let p2 = dot(transverse, transverse);
    let z2 = z * z;
    let mut mu = f / (2.0 * (p2 + z2));
    for _ in 0..scalar_updates {
        let da = 1.0 + mu * alpha;
        let db = 1.0 + mu * beta;
        if da <= 0.5 || db <= 0.5 {
            return None;
        }
        let value =
            f - mu * (p2 * (2.0 + mu * alpha) / (da * da) + z2 * (2.0 + mu * beta) / (db * db));
        let derivative = -2.0 * (p2 / (da * da * da) + z2 / (db * db * db));
        mu -= value / derivative;
    }
    let correction = z * mu * (beta - alpha) / (1.0 + mu * beta);
    let mut n = normalize(std::array::from_fn(|i| q[i] - axis[i] * correction));
    let mut result = Candidate {
        normal: n,
        height: 0.0,
        input_normal: n,
        line_root_residual: 0.0,
        normal_change: 0.0,
        refinements,
    };
    for _ in 0..refinements {
        let qn = packet.rows.map(|r| dot([r[0], r[1], r[2]], n));
        let v = dot(n, q);
        let coefficient = dot(n, qn);
        let discriminant = v * v - f * coefficient;
        if discriminant < 0.0 || !discriminant.is_finite() {
            return None;
        }
        let h = f / (v + discriminant.sqrt());
        let next = normalize(std::array::from_fn(|i| q[i] - h * qn[i]));
        result = Candidate {
            normal: next,
            height: h,
            input_normal: n,
            line_root_residual: f - h * (2.0 * v - h * coefficient),
            normal_change: length(std::array::from_fn(|i| next[i] - n[i])),
            refinements,
        };
        n = next;
    }
    if result.height.is_finite() && result.normal.iter().all(|x| x.is_finite()) {
        Some(result)
    } else {
        None
    }
}

fn main() {
    use earth_math::{Geodetic, Observer};
    let vector = |v: [f32; 3]| {
        format!(
            "[{:.17e},{:.17e},{:.17e}]",
            v[0] as f64, v[1] as f64, v[2] as f64
        )
    };
    let mut cases = Vec::new();
    for lat in [-90.0, -67.5, -12.0, 0.0, 29.305549, 62.0, 89.999, 90.0] {
        for lon in [-179.99, -94.767015, 0.0, 90.0] {
            let observer = Observer::new(Geodetic::new(lat, lon, -27.7).unwrap());
            let packet = GpuEllipsoid::at(observer);
            for p in [
                [0.0, 0.0, 0.0],
                [30.0, 0.0, -60.0],
                [3000.0, 1000.0, -2000.0],
                [-60000.0, 20000.0, 65000.0],
                [100000.0, 80000.0, -40000.0],
                [500000.0, 100000.0, -1000000.0],
                [-1700000.0, 0.0, -1000000.0],
            ] {
                let got = candidate(&packet, p, 1).unwrap();
                let (q, f) = query(&packet, p);
                let old = packet.height_normal(p);
                let certificate = encoded_certificate(&packet, p, got).unwrap();
                let rows = packet.rows.map(|r| [r[0], r[1], r[2]]);
                cases.push(format!("{{\"name\":\"{lat},{lon},{p:?}\",\"matrix_rows\":[{},{},{}],\"q\":{},\"f0\":{:.17e},\"candidate\":{{\"normal\":{},\"height\":{:.17e},\"input_normal\":{},\"residual\":{:.17e}}},\"legacy\":{{\"normal\":{},\"height\":{:.17e}}},\"encoded_certificate\":{{\"height_error\":{:.17e},\"normal_error\":{:.17e},\"skew_norm_bound\":{:.17e}}}}}",
                    vector(rows[0]),vector(rows[1]),vector(rows[2]),vector(q),f as f64,
                    vector(got.normal),got.height as f64,vector(got.input_normal),got.line_root_residual as f64,
                    vector(old.1),old.0 as f64,certificate.height_error,certificate.normal_error,certificate.skew_norm_bound));
            }
        }
    }
    println!(
        "{{\"format\":\"unused-scalar-inverse-f32-query-v1\",\"cases\":[{}]}}",
        cases.join(",")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use earth_math::{Geodetic, Observer};
    #[test]
    fn actual_uploaded_matrix_seed_remains_near_legacy_geometry() {
        let mut maximum_h = 0.0_f32;
        let mut maximum_n = 0.0_f32;
        let mut maximum_change = 0.0_f32;
        let mut count = 0;
        let mut accepted = 0;
        let mut max_normal_enclosure = 0.0_f64;
        for lat in [-90.0, -67.5, -12.0, 0.0, 29.305549, 62.0, 89.999, 90.0] {
            for lon in [-179.99, -94.767015, 0.0, 90.0] {
                let observer = Observer::new(Geodetic::new(lat, lon, -27.7).unwrap());
                let packet = GpuEllipsoid::at(observer);
                for p in [
                    [0.0, 0.0, 0.0],
                    [30.0, 0.0, -60.0],
                    [3000.0, 1000.0, -2000.0],
                    [-60000.0, 20000.0, 65000.0],
                    [100000.0, 80000.0, -40000.0],
                    [500000.0, 100000.0, -1000000.0],
                    [-1700000.0, 0.0, -1000000.0],
                ] {
                    let got = candidate(&packet, p, 1).unwrap();
                    let old = packet.height_normal(p);
                    let encoded = encoded_certificate(&packet, p, got).unwrap();
                    let enclosed = rounded_enclosure(&packet, p, got).unwrap();
                    assert!(encoded.height_error <= enclosed.height_error);
                    assert!(encoded.normal_error <= enclosed.normal_error);
                    max_normal_enclosure = max_normal_enclosure.max(enclosed.normal_error);
                    let allowed_h =
                        (16.0 * 2.0_f64.powi(-24) * (length(p) as f64 + (got.height as f64).abs()))
                            .max(1e-6);
                    let allowed_n = 16.0 * 2.0_f64.powi(-24);
                    accepted += usize::from(
                        enclosed.height_error <= allowed_h && enclosed.normal_error <= allowed_n,
                    );
                    maximum_h = maximum_h.max((got.height - old.0).abs());
                    maximum_n =
                        maximum_n.max(length(std::array::from_fn(|i| got.normal[i] - old.1[i])));
                    maximum_change = maximum_change.max(got.normal_change);
                    // Comparison only, NOT a proof that legacy values are exact.
                    assert!(
                        (got.height - old.0).abs()
                            <= 16.0 * f32::EPSILON * (length(p) + got.height.abs()).max(1.0)
                    );
                    assert!(
                        length(std::array::from_fn(|i| got.normal[i] - old.1[i]))
                            < 16.0 * f32::EPSILON
                    );
                    count += 1;
                }
            }
        }
        eprintln!(
            "{count} f32 queries: max height vs legacy {maximum_h:e} m, normal {maximum_n:e}, refinement change {maximum_change:e}; finite guard {accepted}/{count}, maxnormalbound {max_normal_enclosure:e}"
        );
    }
}
