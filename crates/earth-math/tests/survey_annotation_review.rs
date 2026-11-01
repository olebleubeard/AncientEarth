//! Independent analytic projection and finite-polyline checks. A screen tape
//! is an annotation, not a depth-tested visibility or physical-radius claim.
use earth_math::survey::{
    SurveyTarget, TapeAnnotation, TapePoint, tape_annotation, tape_endpoint_position,
};
use earth_math::{CameraFrame, Lens, Projection, Vec3d};

fn frame() -> CameraFrame {
    CameraFrame {
        origin: Vec3d::ZERO,
        right: Vec3d::new(1.0, 0.0, 0.0),
        up: Vec3d::new(0.0, 1.0, 0.0),
        forward: Vec3d::new(0.0, 0.0, 1.0),
    }
}
fn point(p: Vec3d) -> TapePoint {
    TapePoint {
        position_ecef_m: p,
        target: SurveyTarget::Surface,
    }
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        a.is_finite() && (a - b).abs() <= tolerance,
        "actual{a}, expected{b}, tolerance{tolerance}"
    );
}
fn physical(p: [f32; 2], scale: [f32; 2]) -> [f64; 2] {
    [
        f64::from(p[0]) * f64::from(scale[0]),
        f64::from(p[1]) * f64::from(scale[1]),
    ]
}
fn centers(annotation: &TapeAnnotation, scale: [f32; 2]) -> Vec<[[f64; 2]; 2]> {
    annotation
        .quads
        .iter()
        .map(|q| {
            let [a, b, c, d] = q.map(|p| physical(p, scale));
            [
                [(a[0] + d[0]) * 0.5, (a[1] + d[1]) * 0.5],
                [(b[0] + c[0]) * 0.5, (b[1] + c[1]) * 0.5],
            ]
        })
        .collect()
}
fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let norm = d[0] * d[0] + d[1] * d[1];
    let t = if norm > 0.0 {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / norm
    } else {
        0.0
    };
    let t = t.clamp(0.0, 1.0);
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}

#[test]
fn rectangular_tape_clips_crossing_endpoints_and_keeps_one_physical_pixel_width() {
    let lens = Lens::new(Projection::Rectilinear, 90.0);
    for scale in [[1.0, 1.0], [1.5, 1.5], [2.0, 1.5]] {
        let viewport = [800.0, 600.0];
        let size = [800.0 * f64::from(scale[0]), 600.0 * f64::from(scale[1])];
        let a = point(Vec3d::new(-10.0, 0.0, 2.0));
        let b = point(Vec3d::new(10.0, 0.0, 2.0));
        let draw = tape_annotation(a, b, frame(), lens, viewport, scale).unwrap();
        assert_eq!(draw.quads.len(), 1);
        assert_eq!(draw.endpoint_positions, [None, None]);
        assert!(tape_endpoint_position(a, frame(), lens, viewport, scale).is_none());
        assert!(tape_endpoint_position(b, frame(), lens, viewport, scale).is_none());
        let segment = centers(&draw, scale)[0];
        close(segment[0][0], 0.0, 1e-5);
        close(segment[1][0], size[0], 1e-5);
        close(segment[0][1], size[1] * 0.5, 1e-5);
        let q = draw.quads[0].map(|p| physical(p, scale));
        close((q[0][0] - q[3][0]).hypot(q[0][1] - q[3][1]), 1.0, 8e-5);
        assert_eq!(a.position_ecef_m, Vec3d::new(-10.0, 0.0, 2.0));
        assert_eq!(b.position_ecef_m, Vec3d::new(10.0, 0.0, 2.0));
    }
}

#[test]
fn tape_projection_uses_physical_aspect_and_full_camera_axes_in_telescope_views() {
    // A proper rolled triad; expected points are specified in camera space.
    let mut f = frame();
    let angle = 0.73_f64;
    f.right = Vec3d::new(angle.cos(), angle.sin(), 0.0);
    f.up = Vec3d::new(-angle.sin(), angle.cos(), 0.0);
    for field in [60.0, 0.2, 0.00001] {
        let lens = Lens::new(Projection::Rectilinear, field);
        let half = lens.vertical_fov_deg.to_radians() * 0.5;
        for scale in [[1.0, 1.0], [1.5, 1.5], [2.0, 1.5]] {
            let size = [800.0 * f64::from(scale[0]), 600.0 * f64::from(scale[1])];
            let focal = size[1] / (2.0 * half.tan());
            let camera = [
                Vec3d::new(-half.tan(), half.tan() * 0.5, 4.0),
                Vec3d::new(half.tan() * 1.3, -half.tan(), 4.0),
            ];
            let world = camera.map(|p| point(f.right * p.x + f.up * p.y + f.forward * p.z));
            let draw = tape_annotation(world[0], world[1], f, lens, [800.0, 600.0], scale).unwrap();
            assert_eq!(draw.quads.len(), 1);
            for (i, p) in camera.into_iter().enumerate() {
                let expected = [
                    size[0] * 0.5 + focal * p.x / p.z,
                    size[1] * 0.5 - focal * p.y / p.z,
                ];
                let actual = physical(draw.endpoint_positions[i].unwrap(), scale);
                assert_eq!(
                    tape_endpoint_position(world[i], f, lens, [800.0, 600.0], scale),
                    draw.endpoint_positions[i]
                );
                close(actual[0], expected[0], 8e-5);
                close(actual[1], expected[1], 8e-5);
            }
            let q = draw.quads[0].map(|p| physical(p, scale));
            close((q[0][0] - q[3][0]).hypot(q[0][1] - q[3][1]), 1.0, 1.5e-4);
        }
    }
}

#[test]
fn fisheye_tape_tracks_the_projected_chord_and_clips_aperture_and_portrait_view() {
    for (field, viewport, scale) in [
        (140.0, [1200.0, 800.0], [1.0, 1.0]),
        (140.0, [400.0, 800.0], [1.5, 1.5]),
        (0.00001, [800.0, 600.0], [2.0, 1.5]),
    ] {
        let lens = Lens::new(Projection::EquidistantFisheye, field);
        let half = lens.vertical_fov_deg.to_radians() * 0.5;
        let stretch = if field < 1.0 { half } else { 1.0 };
        let a = Vec3d::new(-10.0 * stretch, 0.2 * stretch, 1.0);
        let b = Vec3d::new(10.0 * stretch, 0.2 * stretch, 1.0);
        let draw = tape_annotation(point(a), point(b), frame(), lens, viewport, scale).unwrap();
        assert_eq!(draw.endpoint_positions, [None, None]);
        let segments = centers(&draw, scale);
        let size = [
            f64::from(viewport[0]) * f64::from(scale[0]),
            f64::from(viewport[1]) * f64::from(scale[1]),
        ];
        let focal = size[1] / (2.0 * half);
        let mut visible = 0;
        for index in 0..=10_000 {
            let t = f64::from(index) / 10_000.0;
            let p = a + (b - a) * t;
            let r = p.x.hypot(p.y);
            let theta = r.atan2(p.z);
            let projected = [
                size[0] * 0.5 + focal * theta * p.x / r,
                size[1] * 0.5 - focal * theta * p.y / r,
            ];
            if theta <= half
                && projected[0] >= 0.0
                && projected[0] <= size[0]
                && projected[1] >= 0.0
                && projected[1] <= size[1]
            {
                visible += 1;
                let error = segments
                    .iter()
                    .map(|s| distance_to_segment(projected, s[0], s[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(error < 0.115, "finite sweep deviation{error}, field{field}");
            }
        }
        assert!(visible > 500);
        for q in &draw.quads {
            assert!(
                q.iter().all(|p| p[0] >= 0.0
                    && p[0] <= viewport[0]
                    && p[1] >= 0.0
                    && p[1] <= viewport[1])
            );
        }
    }
}

#[test]
fn tape_front_plane_and_invalid_or_degenerate_cases_fail_without_fake_endpoints() {
    for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
        let lens = Lens::new(projection, 90.0);
        let draw = tape_annotation(
            point(Vec3d::new(-2.0, 0.0, -1.0)),
            point(Vec3d::new(0.3, 0.0, 2.0)),
            frame(),
            lens,
            [800.0, 600.0],
            [1.0, 1.0],
        )
        .unwrap();
        assert!(draw.endpoint_positions[0].is_none());
        assert!(draw.endpoint_positions[1].is_some());
        assert!(draw.quads.iter().flatten().flatten().all(|v| v.is_finite()));
        for (a, b) in [
            (Vec3d::new(-1.0, 0.0, -1.0), Vec3d::new(1.0, 0.0, -1.0)),
            (Vec3d::new(0.0, 0.0, 1.0), Vec3d::new(0.0, 0.0, 1.0)),
            (Vec3d::ZERO, Vec3d::ZERO),
            (Vec3d::new(f64::NAN, 0.0, 1.0), Vec3d::new(0.0, 0.0, 1.0)),
        ] {
            assert!(
                tape_annotation(
                    point(a),
                    point(b),
                    frame(),
                    lens,
                    [800.0, 600.0],
                    [1.0, 1.0]
                )
                .is_none()
            );
        }
        assert!(
            tape_annotation(
                point(Vec3d::new(-1.0, 0.0, 1.0)),
                point(Vec3d::new(1.0, 0.0, 1.0)),
                frame(),
                lens,
                [0.0, 600.0],
                [1.0, 1.0]
            )
            .is_none()
        );
    }
}
