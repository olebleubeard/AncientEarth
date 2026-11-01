//! Apply the math-owned enclosure to EXACT GPU readback intermediates.
#[path = "optical_scalar_f32.rs"]
mod scalar;
use earth_math::atmosphere::gpu::GpuEllipsoid;
use serde_json::{Value, json};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("GPU query report path")?;
    let data: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let mut rows = Vec::new();
    let mut accepted = 0;
    for row in data["cases"].as_array().ok_or("cases")? {
        let values = |key: &str| -> Result<[f32; 4], Box<dyn std::error::Error>> {
            let a = row[key].as_array().ok_or("vec4")?;
            let mut v = [0.0; 4];
            for i in 0..4 {
                v[i] = a[i].as_f64().ok_or("finitefloat")? as f32;
                if !v[i].is_finite() {
                    return Err("nonfinite".into());
                }
            }
            Ok(v)
        };
        let v3 = |v: [f32; 4]| [v[0], v[1], v[2]];
        let mut matrix = [[0.0; 4]; 3];
        for i in 0..3 {
            for j in 0..4 {
                matrix[i][j] = row["matrix_rows"][i][j].as_f64().ok_or("matrix")? as f32;
            }
        }
        let packet = GpuEllipsoid {
            rows: matrix,
            origin: values("origin")?,
            center: [0.0; 4],
            latitude: values("latitude_packet")?,
        };
        let candidate = values("candidate")?;
        let n = values("input_normal_residual")?;
        let q = values("gpu_q_f")?;
        let qn = values("gpu_qn_v")?;
        let c = values("coefficient_change_valid")?;
        let g = values("gradient_normal_squared")?;
        let got = scalar::Candidate {
            normal: v3(candidate),
            height: candidate[3],
            input_normal: v3(n),
            line_root_residual: n[3],
            normal_change: c[1],
            refinements: 1,
        };
        let query = scalar::EncodedQuery {
            q: v3(q),
            f: q[3],
            qn: v3(qn),
            v: qn[3],
            coefficient: c[0],
            gradient: v3(g),
            input_normal_squared: g[3],
        };
        let exact =
            scalar::encoded_certificate_query(&packet, query, got).ok_or("encodedcertificate")?;
        let bound =
            scalar::rounded_enclosure_encoded(&packet, query, got).ok_or("roundedenclosure")?;
        assert!(
            exact.height_error <= bound.height_error && exact.normal_error <= bound.normal_error
        );
        let p: [f64; 3] = std::array::from_fn(|i| row["position"][i].as_f64().unwrap());
        let norm = (p.iter().map(|v| v * v).sum::<f64>()).sqrt();
        let pass = bound.height_error
            <= (16.0 * 2.0_f64.powi(-24) * (norm + (got.height as f64).abs())).max(1e-6)
            && bound.normal_error <= 16.0 * 2.0_f64.powi(-24);
        accepted += usize::from(pass);
        rows.push(json!({"latitude":row["latitude"],"longitude":row["longitude"],"position":p,"guard_accepts":pass,
            "encoded_certificate":{"height_error":exact.height_error,"normal_error":exact.normal_error},
            "rounded_enclosure":{"height_error":bound.height_error,"normal_error":bound.normal_error},
            "candidate":row["candidate"],"legacy":row["legacy"],"gpu_query":row["gpu_q_f"]}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"scope":"Diagnostic finite returned-pair enclosures from exact GPU readback intermediates. No CPU recomputation substitutes query/fused values. No global ray or performance acceptance.","input":path,"accepted":accepted,"fallback":rows.len()-accepted,"cases":rows})
        )?
    );
    Ok(())
}
