//! Archived scalar packet fixtures for the retired analytic overview prototype.
//! Production uses the continuous canonical world renderer at every distance.
mod review;
use bevy::{prelude::*, render::render_resource::ShaderType};
use earth_math::{
    Vec3d, WGS84_A_M, WGS84_B_M,
    astro::{Mat3d, orbit::OrbitFrame},
    rendering::orbit::OrbitBody,
};
#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct Body {
    eye: Vec4,
    row0: Vec4,
    row1: Vec4,
    row2: Vec4,
    sun: Vec4,
}
impl Body {
    fn at(body: OrbitBody, frame: &OrbitFrame, eye: Vec3d) -> Self {
        let attitude = match body {
            OrbitBody::Earth => frame.earth_to_icrf,
            OrbitBody::Sun => frame.sun_to_icrf,
            OrbitBody::Moon => frame.moon_to_icrf,
        };
        let inverse = attitude.transpose();
        let center = body.center(frame);
        let radius = body.radius();
        let origin = inverse.transform(eye - center) / radius;
        let sunlight = inverse.transform(-center) / radius;
        Self {
            eye: vector(origin).extend(radius as f32),
            row0: row(inverse, 0),
            row1: row(inverse, 1),
            row2: row(inverse, 2).with_w(if body == OrbitBody::Earth {
                (WGS84_B_M / WGS84_A_M) as f32
            } else {
                1.0
            }),
            sun: vector(sunlight).extend(0.0),
        }
    }
}
fn vector(v: Vec3d) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}
fn row(m: Mat3d, i: usize) -> Vec4 {
    Vec4::new(
        m.rows[i][0] as f32,
        m.rows[i][1] as f32,
        m.rows[i][2] as f32,
        0.0,
    )
}
