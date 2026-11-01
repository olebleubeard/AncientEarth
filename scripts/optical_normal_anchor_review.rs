//! Original MIT. Export exact inputs/outputs for independent anchor residuals.
#[path="optical_normal_anchor_packet.rs"] mod anchor;
use earth_math::{EnuBasis,Geodetic,geodetic_to_ecef,WGS84_A_M,WGS84_E2};
use serde_json::json;
fn f64bits(v:[f64;3])->[u64;3]{v.map(f64::to_bits)}
fn main(){
    let mut cases=Vec::new();
    for (lat,lon) in [(-89.9,-170.),(-45.,42.),(0.,0.),(29.305549,-94.767015),(89.9,120.)]{
        let eye=Geodetic::new(lat,lon,-26.00552287325263).unwrap();let raw=EnuBasis::at(eye);
        for (dlat,dlon) in [(0.,0.),(0.00001,-0.00002),(0.05,0.075),(-0.05,-0.075)]{
            for h in [-9999.5,-27.,166.,100000.125]{
                let vertex=geodetic_to_ecef(Geodetic::new(lat+dlat,lon+dlon,h).unwrap());
                let p=anchor::AnchorPacket::from_vertex(eye,vertex).unwrap();
                cases.push(json!({"eye_bits":f64bits([lat,lon,eye.height_m]),
                    "raw_east_bits":f64bits([raw.east.x,raw.east.y,raw.east.z]),
                    "raw_up_bits":f64bits([raw.up.x,raw.up.y,raw.up.z]),
                    "raw_north_bits":f64bits([raw.north.x,raw.north.y,raw.north.z]),
                    "vertex_bits":f64bits([vertex.x,vertex.y,vertex.z]),
                    "canonical_chart_bits":p.canonical_chart.map(f64::to_bits),
                    "canonical_height_bits":p.canonical_height.to_bits(),
                    "chart_hi_bits":p.chart_hi.map(f32::to_bits),"chart_lo_bits":p.chart_lo.map(f32::to_bits),
                    "chart_residual_bits":p.chart_residual.map(|x|x.map(f32::to_bits)),
                    "height_bits":p.height.map(f32::to_bits),"height_residual_bits":p.height_residual.map(f32::to_bits),
                    "mapped_minus_vertex_lower_bits":p.mapped_minus_vertex_lower.map(f32::to_bits),
                    "mapped_minus_vertex_upper_bits":p.mapped_minus_vertex_upper.map(f32::to_bits)}));
            }
        }
    }
    let output=std::env::args().nth(1).expect("output JSON path");assert!(!std::path::Path::new(&output).exists());
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"a_bits":WGS84_A_M.to_bits(),"e2_bits":WGS84_E2.to_bits(),"cases":cases})).unwrap()).unwrap();
}
