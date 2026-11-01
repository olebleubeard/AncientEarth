//! Independent equidistant rays and compensated perspective-source sampling.
use earth_math::{rendering::fisheye::*, *};
use std::f64::consts::{FRAC_PI_4, TAU};

// Scalar transcription of the actual adaptive shader. Rational coefficients
// round to the same f32 values as its decimal literals. Native contraction and
// hardware texture interpolation remain separate presentation checks.
fn scalar_ray(xy: [f32; 2], half: f32, adaptive: bool) -> [f32; 3] {
    let r = (xy[0] * xy[0] + xy[1] * xy[1]).sqrt();
    let theta = r * half;
    if adaptive {
        let q = theta * theta;
        let sinc = 1.0
            + q * (-1.0 / 6.0
                + q * (1.0 / 120.0
                    + q * (-1.0 / 5040.0 + q * (1.0 / 362880.0 + q * (-1.0 / 39916800.0)))));
        let cosine = 1.0
            + q * (-0.5
                + q * (1.0 / 24.0
                    + q * (-1.0 / 720.0
                        + q * (1.0 / 40320.0 + q * (-1.0 / 3628800.0 + q * (1.0 / 479001600.0))))));
        [xy[0] * (half * sinc), xy[1] * (half * sinc), cosine]
    } else {
        let s = if r > 1e-8 {
            theta.sin() / r.max(1e-8)
        } else {
            half
        };
        [xy[0] * s, xy[1] * s, theta.cos()]
    }
}
fn v(a: [f32; 3]) -> Vec3d {
    Vec3d::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]))
}
fn ideal(xy: [f32; 2], half: f64) -> Vec3d {
    let (x, y) = (f64::from(xy[0]), f64::from(xy[1]));
    let r = x.hypot(y);
    let s = if r == 0.0 { half } else { (r * half).sin() / r };
    Vec3d::new(x * s, y * s, (r * half).cos())
}

#[test]
fn adaptive_source_covers_the_aperture_and_retains_small_angle_resolution() {
    let boundary = 2.0 * (FRAC_PI_4 - 0.002).to_degrees();
    let mut maximum_ray_error = 0.0_f64;
    let mut maximum_uv_error = 0.0_f64;
    let mut minimum_edge_margin = 1.0_f64;
    let mut rays = 0;
    // Alternating Taylor remainder bounds are separate from f32 arithmetic.
    let factorial = |n: u32| (1..=n).map(f64::from).product::<f64>();
    assert!(FRAC_PI_4.powi(12) / factorial(13) < 9e-12);
    assert!(FRAC_PI_4.powi(14) / factorial(14) < 4e-13);
    for fov in [
        1e-6,
        1e-5,
        0.001,
        0.1,
        1.0,
        5.0,
        45.0,
        60.0,
        boundary - 1e-8,
    ] {
        let lens = Lens::new(Projection::EquidistantFisheye, fov);
        let plan = forward_projection(lens);
        assert!(plan.adaptive);
        assert_eq!(
            active_faces(lens),
            [true, false, false, false, false, false]
        );
        let (sine, cosine) = (plan.fov_radians * 0.5).sin_cos();
        assert_eq!(plan.projection_scale, cosine / sine);
        assert_eq!(plan.tan_half_fov, 1.0 / plan.projection_scale);
        assert!(f64::from(plan.fov_radians) > lens.vertical_fov_deg.to_radians());
        let half = lens.vertical_fov_deg.to_radians() * 0.5;
        for radius in [0.0_f64, 1e-9, 1e-6, 0.1, 0.5, 0.999, 1.0] {
            for azimuth in 0..360 {
                let a = azimuth as f64 * TAU / 360.0;
                let xy = [(radius * a.cos()) as f32, (radius * a.sin()) as f32];
                if (xy[0] * xy[0] + xy[1] * xy[1]).sqrt() > 1.0 {
                    continue;
                }
                let ray = scalar_ray(xy, half as f32, true);
                let expected = ideal(xy, half);
                maximum_ray_error = maximum_ray_error.max(v(ray).distance(expected));
                let uv = [
                    (ray[0] * plan.projection_scale) / ray[2] * 0.5 + 0.5,
                    (-ray[1] * plan.projection_scale) / ray[2] * 0.5 + 0.5,
                ];
                for coord in uv {
                    minimum_edge_margin =
                        minimum_edge_margin.min(f64::from(coord.min(1.0 - coord)));
                }
                assert!(
                    uv.into_iter()
                        .all(|q| q.is_finite() && (0.0..=1.0).contains(&q))
                );
                let exact_uv = [
                    expected.x * f64::from(plan.projection_scale) / expected.z * 0.5 + 0.5,
                    -expected.y * f64::from(plan.projection_scale) / expected.z * 0.5 + 0.5,
                ];
                for i in 0..2 {
                    maximum_uv_error = maximum_uv_error.max((f64::from(uv[i]) - exact_uv[i]).abs());
                }
                let reconstructed = Vec3d::new(
                    (2.0 * f64::from(uv[0]) - 1.0) / f64::from(plan.projection_scale),
                    -(2.0 * f64::from(uv[1]) - 1.0) / f64::from(plan.projection_scale),
                    1.0,
                )
                .normalized()
                .unwrap();
                assert!(reconstructed.distance(expected) < 5e-7);
                rays += 1;
            }
        }
    }
    assert!(maximum_ray_error < 3e-7);
    assert!(maximum_uv_error < 3e-7);
    assert!(minimum_edge_margin > 8e-6);
    println!(
        "adaptive fisheye {rays} scalar rays: max direction {maximum_ray_error:e}, max UV {maximum_uv_error:e}, minimum UV edge margin {minimum_edge_margin:e}"
    );
    let plan = forward_projection(Lens::new(Projection::EquidistantFisheye, 1.0));
    let half_moon = 0.25_f64.to_radians();
    let source_radius = plan.focal_pixels(1080) * half_moon.tan();
    assert!((269.0..271.0).contains(&source_radius));
    assert!(540.0 * half_moon.tan() < 3.0); // Reproduces the old source undersampling.
    assert!((plan.focal_pixels(1080) / (1080.0 / 1.0_f64.to_radians()) - 1.0).abs() < 5e-5);
    assert_eq!(plan.focal_pixels(2160), 2.0 * plan.focal_pixels(1080));
    for fov in [boundary + 1e-8, 90.0, 140.0, 180.0] {
        let wide = forward_projection(Lens::new(Projection::EquidistantFisheye, fov));
        assert!(!wide.adaptive);
        assert_eq!(wide.fov_radians, std::f32::consts::FRAC_PI_2);
    }
    for invalid in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        let plan = forward_projection(Lens {
            projection: Projection::EquidistantFisheye,
            vertical_fov_deg: invalid,
        });
        assert!(plan.fov_radians.is_finite() && plan.tan_half_fov > 0.0);
    }
}

#[test]
fn source_projection_switch_keeps_physical_camera_rays_and_annotations_continuous() {
    let boundary = 2.0 * (FRAC_PI_4 - 0.002).to_degrees();
    for (lat, lon) in [(0.0, 0.0), (90.0, 157.0), (-72.0, -179.0)] {
        let mut observer = Observer::new(Geodetic::new(lat, lon, 1e6).unwrap());
        observer.heading_deg = 117.0;
        observer.pitch_deg = 67.0;
        observer.roll_deg = -43.0;
        let frame = observer.frame();
        for aspect in [0.3, 1.0, 16.0 / 9.0, 3.0] {
            for xy in [[0.0_f32, 0.0], [0.1, 0.8], [-0.7, 0.2], [0.6, -0.79]] {
                if f64::from(xy[0]).abs() > aspect {
                    continue;
                }
                let mut before = None;
                for fov in [1e-6, 1.0, 60.0, boundary - 1e-6, boundary + 1e-6, 140.0] {
                    let lens = Lens::new(Projection::EquidistantFisheye, fov);
                    let plan = forward_projection(lens);
                    let ray = scalar_ray(xy, (fov.to_radians() * 0.5) as f32, plan.adaptive);
                    let world = (frame.right * f64::from(ray[0])
                        + frame.up * f64::from(ray[1])
                        + frame.forward * f64::from(ray[2]))
                    .normalized()
                    .unwrap();
                    let exact = lens
                        .ray(frame, f64::from(xy[0]) / aspect, f64::from(xy[1]), aspect)
                        .unwrap();
                    assert!(world.distance(exact) < 4e-7);
                    if fov == boundary - 1e-6 {
                        before = Some(world);
                    }
                    if fov == boundary + 1e-6 {
                        assert!(world.distance(before.unwrap()) < 4e-7);
                    }
                    let point = lens.project(frame, exact, aspect).unwrap();
                    assert!((point[0] - f64::from(xy[0]) / aspect).abs() < 1e-7);
                    assert!((point[1] - f64::from(xy[1])).abs() < 1e-7);
                }
            }
        }
    }
}
