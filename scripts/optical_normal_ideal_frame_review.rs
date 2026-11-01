//! Original MIT. Independent-output fixture for the unused ideal scene frame.
//! The Python companion evaluates direct spherical angles at 90 digits.
#[path = "optical_normal_ideal_frame.rs"]
mod frame;
use earth_math::{EnuBasis, Geodetic, WGS84_A_M, WGS84_E2};
use serde_json::json;

fn bits(v: [f64; 3]) -> [u64; 3] { v.map(f64::to_bits) }
fn intervals(v: frame::VectorBounds) -> [[u64; 2]; 3] {
    v.map(|x| x.map(f64::to_bits))
}
fn main() {
    let mut cases = Vec::new();
    for lat in [-90., -89.999999, -70., -29.305549, 0., 29.305549, 70., 89.999999, 90.] {
        for lon in [-180., -94.767015, 0., 89.999999, 179.9] {
            for h in [-10000., -26.00552287325263, 0., 100., 120001.] {
                let eye = Geodetic::new(lat, lon, h).unwrap();
                let raw = EnuBasis::at(eye);
                let f = frame::IdealFrame::from_eye(eye).unwrap();
                let point = [6_001_234.125, -2_012_345.25, 1_123_456.5];
                let vector = [[-13., 17.], [-19., -7.], [0.125, 0.375]];
                cases.push(json!({
                    "geodetic_bits": bits([lat,lon,h]),
                    "raw_east_bits": bits([raw.east.x,raw.east.y,raw.east.z]),
                    "raw_up_bits": bits([raw.up.x,raw.up.y,raw.up.z]),
                    "raw_north_bits": bits([raw.north.x,raw.north.y,raw.north.z]),
                    "columns_bits": f.columns.map(intervals),
                    "origin_bits": intervals(f.origin_ecef),
                    "point_bits": bits(point),
                    "eye_minus_point_bits": intervals(f.eye_minus_point(point)),
                    "origin_residual_bits": intervals(f.origin_residual(point)),
                    "vector_bits": intervals(vector),
                    "projected_vector_bits": intervals(f.project(vector))
                }));
            }
        }
    }
    let path = std::env::args().nth(1).expect("output JSON path");
    assert!(!std::path::Path::new(&path).exists(), "retain earlier evidence");
    std::fs::write(path, serde_json::to_vec_pretty(&json!({
        "scope":"Unused ideal-frame finite enclosures; exact exported binary64 inputs, not a universal GPU scene certificate",
        "a_bits":WGS84_A_M.to_bits(), "e2_bits":WGS84_E2.to_bits(), "cases":cases
    })).unwrap()).unwrap();
}
