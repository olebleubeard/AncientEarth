//! Independent compositor-face coverage, including permitted shader trig error.
use earth_math::{Lens, Projection, rendering::fisheye::active_faces};

const TRIG_ERROR: f64 = 1.0 / 2048.0;

// Retained wide-view fisheye.wesl order and major-axis ties. Narrow views now
// use the separately reviewed adaptive source and polynomial ray branch.
// The optional perturbations
// exercise the WGSL sin/cos absolute-error extrema, plus a conservative small
// envelope around the scalar length/division arithmetic. This is a scalar
// fixture, not an assertion about one vendor's native transcendental library.
fn selected_face(xy: [f32; 2], half: f32, error: [f64; 2], arithmetic: f32) -> Option<usize> {
    let radius = (xy[0] * xy[0] + xy[1] * xy[1]).sqrt() * arithmetic;
    if radius > 1.0 {
        return None;
    }
    let theta = radius * half;
    let sine = (f64::from(theta).sin() + error[0]) as f32;
    let cosine = (f64::from(theta).cos() + error[1]) as f32;
    let scale = if radius > 1e-8 {
        sine / radius.max(1e-8) / arithmetic
    } else {
        half
    };
    let ray = [xy[0] * scale, xy[1] * scale, cosine];
    let axis = ray.map(f32::abs);
    Some(if axis[2] >= axis[0] && axis[2] >= axis[1] {
        if ray[2] >= 0.0 { 0 } else { 2 }
    } else if axis[0] >= axis[1] {
        if ray[0] >= 0.0 { 1 } else { 3 }
    } else if ray[1] >= 0.0 {
        4
    } else {
        5
    })
}

#[test]
fn every_tested_compositor_ray_selects_an_enabled_face_including_trig_error_extrema() {
    // WGSL 15.7.4 permits absolute sin/cos error 2^-11 on [-pi, pi].
    // https://www.w3.org/TR/WGSL/#floating-point-accuracy
    // A 1e-6-radian cutoff fails even with exact ordinary arithmetic.
    let old_cutoff = (std::f64::consts::FRAC_PI_4 - 1e-6) as f32;
    assert_eq!(
        selected_face([1.0, 0.0], old_cutoff, [TRIG_ERROR, -TRIG_ERROR], 1.0),
        Some(1)
    );
    let guard = 0.002;
    let boundary = 2.0 * (std::f64::consts::FRAC_PI_4 - guard).to_degrees();
    let safe_half = (std::f64::consts::FRAC_PI_4 - guard) as f32;
    let transverse_upper =
        (f64::from(safe_half).sin() + TRIG_ERROR) * (1.0 + 32.0 * f64::from(f32::EPSILON));
    let forward_lower = f64::from(safe_half).cos() - TRIG_ERROR;
    assert!(forward_lower - transverse_upper > 0.0018);
    assert_eq!(
        active_faces(Lens::new(Projection::EquidistantFisheye, boundary - 1e-9)),
        [true, false, false, false, false, false]
    );
    assert_eq!(
        active_faces(Lens::new(Projection::EquidistantFisheye, boundary + 1e-9)),
        [true, true, false, true, true, true]
    );
    let mut fovs = vec![
        -1.0,
        0.0,
        0.001,
        1.0,
        45.0,
        60.0,
        89.0,
        boundary - 1e-9,
        boundary,
        boundary + 1e-9,
        89.99,
        90.0,
        90.01,
        120.0,
        179.99,
        180.0,
        181.0,
        f64::NAN,
        f64::INFINITY,
    ];
    fovs.extend((0..=36).map(|step| f64::from(step) * 5.0));
    let errors = [
        [0.0, 0.0],
        [-TRIG_ERROR, -TRIG_ERROR],
        [-TRIG_ERROR, TRIG_ERROR],
        [TRIG_ERROR, -TRIG_ERROR],
        [TRIG_ERROR, TRIG_ERROR],
    ];
    let mut observed = [false; 6];
    for fov in fovs {
        let raw = Lens {
            projection: Projection::EquidistantFisheye,
            vertical_fov_deg: fov,
        };
        let lens = Lens::new(raw.projection, raw.vertical_fov_deg);
        let enabled = active_faces(raw);
        let half = (lens.vertical_fov_deg.to_radians() * 0.5) as f32;
        assert!(!enabled[2]);
        assert_eq!(
            active_faces(Lens {
                projection: Projection::Rectilinear,
                ..raw
            }),
            [false; 6]
        );
        for radial in 0..=32 {
            let radius = radial as f32 / 32.0;
            for angle in 0..128 {
                let azimuth = f64::from(angle) * std::f64::consts::TAU / 128.0;
                let xy = [radius * azimuth.cos() as f32, radius * azimuth.sin() as f32];
                for error in errors {
                    for arithmetic in [1.0 - 8.0 * f32::EPSILON, 1.0, 1.0 + 8.0 * f32::EPSILON] {
                        if let Some(face) = selected_face(xy, half, error, arithmetic) {
                            assert!(
                                enabled[face],
                                "FOV {fov}, xy {xy:?}, error {error:?}, face {face}"
                            );
                            observed[face] = true;
                        }
                    }
                }
            }
        }
        // Actual UV/aspect expression for portrait, ultrawide and tiny views.
        for [width, height] in [
            [1, 1],
            [3, 1],
            [1, 3],
            [1920, 1080],
            [1080, 1920],
            [8192, 256],
        ] {
            let aspect = width as f32 / height as f32;
            for iy in 0..=24 {
                for ix in 0..=24 {
                    let uv = [ix as f32 / 24.0, iy as f32 / 24.0];
                    let xy = [(2.0 * uv[0] - 1.0) * aspect, 1.0 - 2.0 * uv[1]];
                    if let Some(face) = selected_face(xy, half, [0.0; 2], 1.0) {
                        assert!(enabled[face]);
                    }
                }
            }
        }
    }
    assert_eq!(observed, [true, true, false, true, true, true]);
}
