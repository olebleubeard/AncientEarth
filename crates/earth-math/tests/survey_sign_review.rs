//! Independent finite geometry checks for upright, decorative numbered plates.
//! These labels do not change prism bounds or the canonical measurement surface.
use earth_math::{
    rendering::{LocalFrame, RenderMesh, survey::*},
    survey::SurveyMarker,
    *,
};

fn marker_at(latitude: f64, longitude: f64) -> SurveyMarker {
    let position = Geodetic::new(latitude, longitude, 123.0).unwrap();
    let basis = EnuBasis::at(position);
    SurveyMarker {
        id: 0,
        base_ecef_m: geodetic_to_ecef(position),
        right: basis.east,
        forward: basis.north,
        up: basis.up,
        width_m: 3.048,
        depth_m: 3.048,
        height_m: 30.48,
        along_distance_m: 0.0,
    }
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} versus {expected}"
    );
}
fn vector(p: [f32; 3]) -> Vec3d {
    Vec3d::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
}

#[test]
fn axial_frames_preserve_geodetic_up_at_poles_and_have_a_finite_overhead_fallback() {
    for (latitude, longitude) in [
        (0.0, 0.0),
        (29.3, -94.8),
        (90.0, 0.0),
        (-90.0, 123.0),
        (89.999999, 179.9),
    ] {
        let marker = marker_at(latitude, longitude);
        let (s, c) = latitude.to_radians().sin_cos();
        let (sl, cl) = longitude.to_radians().sin_cos();
        let expected_up = Vec3d::new(c * cl, c * sl, s);
        let expected_east = Vec3d::new(-sl, cl, 0.0);
        let expected_north = Vec3d::new(-s * cl, -s * sl, c);
        let plate_height = 3.048 * 9.0 / 25.0;
        let center = marker.base_ecef_m + expected_up * (30.48 + plate_height / 2.0);
        for azimuth in [0.0_f64, 90.0, 180.0, 237.0] {
            let angle = azimuth.to_radians();
            let normal = expected_east * angle.sin() + expected_north * angle.cos();
            // Enumerating the four source footprint corners is an independent
            // support oracle, rather than reusing the author's absolute-dot sum.
            let support = marker.corners()[..4]
                .iter()
                .map(|point| (*point - marker.base_ecef_m).dot(normal))
                .fold(f64::NEG_INFINITY, f64::max);
            for elevation_offset in [-200.0, 0.0, 400.0] {
                let eye = center + normal * 40.0 + expected_up * elevation_offset;
                let frame = marker_sign_frame(&marker, eye).unwrap();
                assert!(frame.center.distance(center + normal * support) < 3e-9);
                assert!(frame.up.distance(expected_up) < 5e-15);
                assert!(frame.normal.distance(normal) < 3e-11);
                assert!(frame.right.distance(expected_up.cross(normal)) < 3e-11);
                close(frame.right.cross(frame.up).dot(frame.normal), 1.0, 5e-15);
                close(frame.up.dot(frame.normal), 0.0, 5e-15);
                let bottom = frame.center - frame.up * (plate_height / 2.0);
                close((bottom - marker.base_ecef_m).dot(expected_up), 30.48, 2e-9);
                close((bottom - marker.base_ecef_m).dot(normal), support, 2e-9);
            }
        }
        for vertical in [-1000.0, 0.0, 1000.0] {
            let frame = marker_sign_frame(&marker, center + expected_up * vertical).unwrap();
            assert!(frame.normal.distance(expected_north) < 5e-15);
            assert!(frame.up.distance(expected_up) < 5e-15);
            close(frame.normal.dot(expected_up), 0.0, 5e-15);
            assert!(frame.center.distance(center) < 2e-9);
        }
        // Near a side, an outside eye can lie behind the radial support plane.
        // The correct front plane in these cases is the indicated axis face.
        for (r, f, support_r, support_f, expected_normal) in [
            (1.6, 0.2, 1.524, 0.2, expected_east),
            (-1.6, 0.2, -1.524, 0.2, -expected_east),
            (0.2, 1.6, 0.2, 1.524, expected_north),
            (0.2, -1.6, 0.2, -1.524, -expected_north),
        ] {
            let eye =
                marker.base_ecef_m + expected_east * r + expected_north * f + expected_up * 1.72;
            let frame = marker_sign_frame(&marker, eye).unwrap();
            assert!(frame.normal.distance(expected_normal) < 3e-11);
            assert!(
                frame
                    .center
                    .distance(center + expected_east * support_r + expected_north * support_f)
                    < 3e-9
            );
            for horizontal in [-0.45, 0.0, 0.45] {
                for vertical in [-0.49, 0.0, 0.49] {
                    let target = frame.center
                        + frame.right * (3.048 * horizontal)
                        + frame.up * (plate_height * vertical);
                    let ray = Ray {
                        origin: eye,
                        direction: target - eye,
                    };
                    assert!(
                        marker
                            .intersect(ray)
                            .is_none_or(|distance| distance >= eye.distance(target) - 1e-8)
                    );
                }
            }
        }
    }
    let marker = marker_at(29.3, -94.8);
    let eye = marker.base_ecef_m + marker.forward * 100.0;
    for invalid in [
        SurveyMarker {
            up: marker.up * 2.0,
            ..marker
        },
        SurveyMarker {
            right: -marker.right,
            ..marker
        },
        SurveyMarker {
            height_m: 1e308,
            ..marker
        },
        SurveyMarker {
            height_m: 0.0,
            ..marker
        },
        SurveyMarker {
            base_ecef_m: Vec3d::new(f64::NAN, 0.0, 0.0),
            ..marker
        },
        SurveyMarker {
            forward: Vec3d::ZERO,
            ..marker
        },
    ] {
        assert!(marker_sign_frame(&invalid, eye).is_none());
    }
    assert!(marker_sign_frame(&marker, Vec3d::new(f64::INFINITY, 0.0, 0.0)).is_none());
}

// Independently rasterize each indexed triangle at an interior point of each
// glyph cell; no author row/run helper is reused. The fractions avoid the
// shared diagonals of the finite source rectangles.
fn cells(mesh: &RenderMesh, width: f64) -> Vec<Vec<bool>> {
    let height = width * 9.0 / 25.0;
    let mut output = vec![vec![false; 25]; 9];
    let mut area = 0.0;
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] =
            [triangle[0], triangle[1], triangle[2]].map(|i| vector(mesh.positions[i as usize]));
        let twice_area = (b - a).cross(c - a).z;
        assert!(twice_area > 0.0);
        area += twice_area / 2.0;
        for index in triangle {
            assert_eq!(mesh.normals[*index as usize], [0.0, 0.0, 1.0]);
            assert_eq!(mesh.positions[*index as usize][2], 0.0);
        }
    }
    close(area, width * height, 5e-7);
    for (row, values) in output.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            let point = Vec3d::new(
                width * ((column as f64 + 0.317) / 25.0 - 0.5),
                height * (0.5 - (row as f64 + 0.639) / 9.0),
                0.0,
            );
            let mut matches = Vec::new();
            for triangle in mesh.indices.chunks_exact(3) {
                let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                    .map(|i| vector(mesh.positions[i as usize]));
                if [(a, b), (b, c), (c, a)]
                    .into_iter()
                    .all(|(a, b)| (b - a).cross(point - a).z >= 0.0)
                {
                    let color = mesh.colors[triangle[0] as usize];
                    assert!(
                        triangle
                            .iter()
                            .all(|index| mesh.colors[*index as usize] == color)
                    );
                    assert!(color == [1.0; 4] || color == [0.0, 0.0, 0.0, 1.0]);
                    matches.push(color == [1.0; 4]);
                }
            }
            assert_eq!(
                matches.len(),
                1,
                "uncovered or overlapping cell {row},{column}"
            );
            *value = matches[0];
        }
    }
    output
}

#[test]
fn numbered_glyphs_form_one_complete_opaque_plate_with_no_number_wrap_or_mirroring() {
    let mut marker = marker_at(0.0, 0.0);
    let mut bitmaps = Vec::new();
    for (id, expected) in [
        (0, "0001"),
        (8, "0009"),
        (9, "0010"),
        (80, "0081"),
        (511, "0512"),
        (9998, "9999"),
    ] {
        marker.id = id;
        assert_eq!(marker_label(id).as_deref(), Some(expected));
        let bitmap = cells(&marker_sign_mesh(&marker).unwrap(), 3.048);
        assert!(bitmap[0].iter().chain(&bitmap[8]).all(|cell| !cell));
        assert!(
            bitmap
                .iter()
                .all(|row| !row[0] && !row[6] && !row[12] && !row[18] && !row[24])
        );
        bitmaps.push(bitmap);
    }
    // Explicit visual rows establish top/bottom and left/right convention for
    // the initial 0001 without copying the production bitmap table.
    let zero = [
        ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
    ];
    let one = [
        "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
    ];
    for row in 0..7 {
        for digit in 0..4 {
            let expected = if digit == 3 { one[row] } else { zero[row] };
            for (column, ch) in expected.chars().enumerate() {
                assert_eq!(bitmaps[0][row + 1][1 + digit * 6 + column], ch == '#');
            }
        }
    }
    // The fourth digit of 0010 is the same zero as the first of 0001, and its
    // third digit is the same one as the last of 0001: carry stays left-to-right.
    for (ten_row, one_row) in bitmaps[2].iter().zip(&bitmaps[0]).take(8).skip(1) {
        assert_eq!(&ten_row[19..24], &one_row[1..6]);
        assert_eq!(&ten_row[13..18], &one_row[19..24]);
    }
    for id in [9999, usize::MAX] {
        marker.id = id;
        assert!(marker_label(id).is_none());
        assert!(marker_sign_mesh(&marker).is_none());
    }
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 1_000_001.0] {
        marker.id = 0;
        marker.width_m = width;
        assert!(marker_sign_mesh(&marker).is_none());
    }
}

#[test]
fn sign_rebasing_and_projection_retain_dimensions_without_enlarging_pickable_prisms() {
    let marker = marker_at(29.3, -94.8);
    let sign_height = 3.048 * 9.0 / 25.0;
    let anchor = marker.centerline(30.48 + sign_height / 2.0);
    let eye = anchor + marker.forward * 60.0;
    let frame = marker_sign_frame(&marker, eye).unwrap();
    let center = frame.center;
    // The center of the decorative plate is above the canonical box: it is
    // deliberately not a new surface or marker pick target.
    assert!(
        marker
            .intersect(Ray {
                origin: eye,
                direction: center - eye
            })
            .is_none()
    );
    let mut camera = Observer::new(ecef_to_geodetic(eye).unwrap());
    camera.feet = camera
        .feet
        .with_height(camera.feet.height_m - camera.eye_height_m)
        .unwrap();
    camera.aim_at(center).unwrap();
    let camera_frame = camera.frame();
    for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
        let lens = Lens {
            projection,
            vertical_fov_deg: 60.0,
        };
        let left = lens
            .project(
                camera_frame,
                center - frame.right * 1.524 - camera_frame.origin,
                16.0 / 9.0,
            )
            .unwrap();
        let right = lens
            .project(
                camera_frame,
                center + frame.right * 1.524 - camera_frame.origin,
                16.0 / 9.0,
            )
            .unwrap();
        let top = lens
            .project(
                camera_frame,
                center + frame.up * sign_height / 2.0 - camera_frame.origin,
                16.0 / 9.0,
            )
            .unwrap();
        let bottom = lens
            .project(
                camera_frame,
                center - frame.up * sign_height / 2.0 - camera_frame.origin,
                16.0 / 9.0,
            )
            .unwrap();
        assert!(left[0] < right[0]);
        assert!(bottom[1] < top[1]);
    }
    let mesh = marker_sign_mesh(&marker).unwrap();
    for origin_height in [0.0, 5000.0, 8_000_000.0] {
        let observer = Observer::new(Geodetic::new(29.3, -94.8, origin_height).unwrap());
        let local = LocalFrame::at(observer);
        let translation = vector(local.position(frame.center));
        let x = vector(local.direction(frame.right));
        let y = vector(local.direction(frame.up));
        let z = vector(local.direction(frame.normal));
        for &point in &mesh.positions {
            let p = vector(point);
            let local_point = translation + x * p.x + y * p.y + z * p.z;
            let reconstructed =
                local.origin + local.basis.east * local_point.x + local.basis.up * local_point.y
                    - local.basis.north * local_point.z;
            let expected = frame.center + frame.right * p.x + frame.up * p.y;
            // Finite f32 display-origin roundoff, not authoritative geometry.
            assert!(reconstructed.distance(expected) <= 1e-5 + origin_height * 1.3e-7);
        }
    }
}
