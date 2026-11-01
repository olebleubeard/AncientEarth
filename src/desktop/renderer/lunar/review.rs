//! Actual uniform and embedded loader checks; scalar f32 is not native GPU proof.
use super::*;
use earth_math::{
    Geodetic, Observer,
    astro::{AstroInstant, Ephemeris},
    rendering::lunar as model,
};

fn d(a: Vec3) -> Vec3d {
    Vec3d::new(f64::from(a.x), f64::from(a.y), f64::from(a.z))
}
fn body(v: Vec3, s: LunarSettings) -> Vec3 {
    Vec3::new(
        v.dot(s.body_x.truncate()),
        v.dot(s.body_y.truncate()),
        v.dot(s.body_z.truncate()),
    )
}
fn uv(n: Vec3) -> [f32; 2] {
    [
        n.y.atan2(n.x) / std::f32::consts::TAU + 0.5,
        0.5 - n.z.clamp(-1.0, 1.0).asin() / std::f32::consts::PI,
    ]
}

#[test]
fn actual_lunar_uniforms_recover_body_coordinates_and_embedded_maps_keep_declared_formats() {
    let ephemeris =
        Ephemeris::from_bytes(include_bytes!("../../../../assets/astronomy/de440s.bsp")).unwrap();
    let mut maximum_direction_error = 0.0_f64;
    let mut maximum_u_error = 0.0_f64;
    let mut maximum_v_error = 0.0_f64;
    for tt in [-2e9, 0.0, 845856123.456] {
        for (lat, lon) in [(29.3, -94.7), (90.0, 137.0), (-80.0, 179.9)] {
            let observer = Observer::new(Geodetic::new(lat, lon, 10.0).unwrap());
            let local = LocalFrame::at(observer);
            let frame = ephemeris
                .sample_for_inspection(
                    AstroInstant::from_tt_seconds_j2000(tt).unwrap(),
                    None,
                    observer.eye_position(),
                    true,
                )
                .unwrap();
            let settings = LunarSettings::at(local, &frame);
            let scalar_sun = body(settings.illumination.truncate(), settings).normalize();
            let exact_sun = frame
                .moon
                .body_to_ecef
                .transpose()
                .transform(frame.moon_to_sun_direction_ecef);
            assert!(d(scalar_sun).distance(exact_sun) < 3e-7);
            for phi in [-90.0_f64, -89.999, -60.0, 0.0, 43.0, 89.999, 90.0] {
                for lambda in [-180.0_f64, -179.999, -90.0, 0.0, 90.0, 179.999, 180.0] {
                    let (p, l) = (phi.to_radians(), lambda.to_radians());
                    let exact = Vec3d::new(p.cos() * l.cos(), p.cos() * l.sin(), p.sin());
                    let world = frame.moon.body_to_ecef.transform(exact);
                    // Explicit committed ENU basis expansion, independent of the
                    // production local.direction used to upload the body axes.
                    let vertex = Vec3::new(
                        world.dot(local.basis.east) as f32,
                        world.dot(local.basis.up) as f32,
                        -world.dot(local.basis.north) as f32,
                    );
                    let n = body(vertex, settings).normalize();
                    maximum_direction_error = maximum_direction_error.max(d(n).distance(exact));
                    let actual = uv(n);
                    let expected = model::map_uv(exact).unwrap();
                    let u_error = (f64::from(actual[0]) - expected[0] + 0.5).rem_euclid(1.0) - 0.5;
                    if p.cos().abs() > 0.01 {
                        maximum_u_error = maximum_u_error.max(u_error.abs());
                    }
                    maximum_v_error =
                        maximum_v_error.max((f64::from(actual[1]) - expected[1]).abs());
                    let east = if n.truncate().length() > 1e-6 {
                        Vec3::new(-n.y, n.x, 0.0).normalize()
                    } else {
                        Vec3::Y
                    };
                    let north = n.cross(east);
                    let mapped = (east * 0.4 + north * (-0.3) + n * (0.75_f32.sqrt())).normalize();
                    assert!(mapped.is_finite() && mapped.dot(n) > 0.8);
                    for footprint in [0.0_f32, 1e-8, 1e-4, 0.1, 1e6] {
                        let density = (8192.0
                            / (std::f32::consts::TAU * n.truncate().length().max(0.00001)))
                        .max(4096.0 / std::f32::consts::PI);
                        let lod = (footprint * density).max(1.0).log2().max(0.0);
                        assert!(lod.is_finite() && lod >= 0.0);
                    }
                }
            }
        }
    }
    assert!(maximum_direction_error < 4e-7);
    assert!(maximum_u_error < 5e-6);
    assert!(maximum_v_error < 1.5e-4);
    println!(
        "lunar actual-uniform scalar maxima: direction={maximum_direction_error:e}, u away from pole={maximum_u_error:e}, v={maximum_v_error:e}"
    );
    for (bytes, width, height, levels, format) in [
        (
            &include_bytes!("../../../../assets/lunar/color.ael")[..],
            8192,
            4096,
            14,
            TextureFormat::Rgba8UnormSrgb,
        ),
        (
            &include_bytes!("../../../../assets/lunar/normal.ael")[..],
            5760,
            2880,
            13,
            TextureFormat::Rgba8Unorm,
        ),
    ] {
        let texture = image(bytes);
        assert_eq!(texture.texture_descriptor.size.width, width);
        assert_eq!(texture.texture_descriptor.size.height, height);
        assert_eq!(texture.texture_descriptor.mip_level_count, levels);
        assert_eq!(texture.texture_descriptor.format, format);
        let expected: usize = (0..levels)
            .map(|i| ((width >> i).max(1) * (height >> i).max(1) * 4) as usize)
            .sum();
        assert_eq!(texture.data.as_ref().unwrap().len(), expected);
        let ImageSampler::Descriptor(sampler) = texture.sampler else {
            panic!("explicit lunar sampler")
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
    }
}
