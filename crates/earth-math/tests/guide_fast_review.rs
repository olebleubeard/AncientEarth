//! Independent ray-conic checks for the direct WGS84 silhouette path.
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile},
    guides::*,
    *,
};
use std::sync::atomic::AtomicBool;

fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a:.15} != {b:.15} (tolerance {tolerance})"
    );
}
fn scene() -> TerrainScene {
    TerrainScene::boundary_only(vec![]).unwrap()
}
fn scaled(p: Vec3d) -> Vec3d {
    Vec3d::new(p.x / WGS84_A_M, p.y / WGS84_A_M, p.z / WGS84_B_M)
}
fn observer(lat: f64, lon: f64, height: f64, heading: f64, pitch: f64, roll: f64) -> Observer {
    let mut o = Observer::new(Geodetic::new(lat, lon, height).unwrap());
    o.heading_deg = heading;
    o.pitch_deg = pitch;
    o.roll_deg = roll;
    o
}
fn viewport() -> GuideViewport {
    GuideViewport::new([1920.0, 1080.0], [1.0, 1.0]).unwrap()
}

// Substitute the camera ray d=F+uR+vU into the ellipsoid discriminant.
// This computes a vertical screen slice without the production tangent circle.
fn conic_rows(frame: CameraFrame, lens: Lens, view: GuideViewport, column: f64) -> Vec<f64> {
    let focal = view.pixels[1] / (2.0 * (lens.vertical_fov_deg.to_radians() * 0.5).tan());
    let u = (column - 0.5) * view.pixels[0] / focal;
    let m = scaled(frame.origin);
    let d = scaled(frame.forward + frame.right * u);
    let v = scaled(frame.up);
    let c = m.dot(m) - 1.0;
    let a = m.dot(v).powi(2) - c * v.dot(v);
    let b = 2.0 * (m.dot(d) * m.dot(v) - c * d.dot(v));
    let cc = m.dot(d).powi(2) - c * d.dot(d);
    let discriminant = b * b - 4.0 * a * cc;
    if discriminant < 0.0 {
        return vec![];
    }
    let q = -0.5 * (b + discriminant.sqrt().copysign(b));
    let mut roots = vec![q / a, cc / q];
    roots.retain(|v| v.is_finite() && m.dot(d + scaled(frame.up) * *v) < 0.0);
    let mut rows: Vec<_> = roots
        .into_iter()
        .map(|v| view.pixels[1] * 0.5 - focal * v)
        .filter(|y| (-1e-6..=view.pixels[1] + 1e-6).contains(y))
        .collect();
    rows.sort_by(f64::total_cmp);
    rows
}
fn assert_tangent(point: HorizonPoint, frame: CameraFrame) {
    let p = point.hit.hit.position;
    close(scaled(p).dot(scaled(p)), 1.0, 9e-16);
    // Tangent ray perpendicular to the implicit surface normal.
    close(
        (p - frame.origin)
            .normalized()
            .unwrap()
            .dot(point.hit.hit.normal),
        0.0,
        3e-12,
    );
    close(point.hit.hit.distance_m, p.distance(frame.origin), 4e-8);
    assert_eq!(point.bracket_pixels, 0.0);
}

#[test]
fn rectilinear_rows_match_ray_quadratic_for_rolled_polar_and_portrait_views() {
    let scene = scene();
    let mut checked = 0;
    for (lat, lon, heading, roll) in [
        (0.0, 0.0, 90.0, 0.0),
        (48.0, 179.9, 13.0, 31.0),
        (89.999, -120.0, 233.0, -72.0),
        (-80.0, 60.0, 45.0, 128.0),
    ] {
        let observer = observer(lat, lon, 100000.0, heading, -10.0, roll);
        for view in [
            viewport(),
            GuideViewport::new([600.0, 1000.0], [1.5, 1.5]).unwrap(),
        ] {
            let lens = Lens::new(Projection::Rectilinear, 70.0);
            let profile = HorizonProfile::measure(&scene, observer.frame(), lens, view).unwrap();
            for point in [profile.apex, profile.left, profile.right] {
                assert_tangent(point, observer.frame());
                let rows = conic_rows(
                    observer.frame(),
                    lens,
                    view,
                    point.pixel[0] / view.pixels[0],
                );
                assert!(
                    rows.iter().any(|y| (*y - point.pixel[1]).abs() < 2e-7),
                    "no independent conic root for {:?}; roots={rows:?}",
                    point.pixel
                );
            }
            for column in [0.1, 0.3, 0.5, 0.7, 0.9] {
                if let Some(ruler) = profile.ruler(&scene, column) {
                    let rows = conic_rows(observer.frame(), lens, view, column);
                    assert!(
                        rows.iter()
                            .any(|y| (*y - ruler.ground.pixel[1]).abs() < 2e-7)
                    );
                    assert!(ruler.top_pixel[1] <= ruler.ground.pixel[1] + 1e-9);
                    close(
                        (ruler.reference_ecef_m - ruler.ground.hit.hit.position)
                            .dot(observer.frame().forward),
                        0.0,
                        4e-8,
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 15, "only {checked} independently checked rulers");
}

#[test]
fn telescope_and_subpixel_visible_slivers_do_not_depend_on_a_screen_lattice() {
    let scene = scene();
    let mut o = observer(0.0, 0.0, 100000.0, 90.0, 0.0, 0.0);
    let h = o.eye_geodetic().height_m;
    let dip = -(h * (2.0 * WGS84_A_M + h)).sqrt().atan2(WGS84_A_M);
    for fov in [MIN_FOV_DEG, 0.001, 0.2, 60.0] {
        for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
            o.pitch_deg = dip.to_degrees();
            let lens = Lens::new(projection, fov);
            let p = HorizonProfile::measure(&scene, o.frame(), lens, viewport())
                .unwrap_or_else(|| panic!("missing centered {projection:?} {fov}"));
            close(p.apex.pixel[1], 540.0, 0.03);
            assert_tangent(p.apex, o.frame());
        }
    }
    for fov in [MIN_FOV_DEG, 0.001, 0.2] {
        for roll in [0.0, 37.0, 89.0, 179.0] {
            for offset in [-0.999, 0.0, 0.999] {
                o.roll_deg = roll;
                o.pitch_deg = dip.to_degrees() + offset * fov * 0.5;
                let p = HorizonProfile::measure(
                    &scene,
                    o.frame(),
                    Lens::new(Projection::EquidistantFisheye, fov),
                    viewport(),
                )
                .unwrap_or_else(|| {
                    panic!("missing clipped fish fov={fov} roll={roll} offset={offset}")
                });
                assert_tangent(p.apex, o.frame());
            }
        }
    }
    o.roll_deg = 0.0;
    // At the very bottom, only a subpixel-high part of the ellipse is visible.
    // Set pitch by exact rectilinear inverse projection, not a sampled scan.
    let lens = Lens::new(Projection::Rectilinear, 60.0);
    let focal = 540.0 / 30.0_f64.to_radians().tan();
    for visible_height in [0.1, 0.001] {
        let row = 1080.0 - visible_height;
        o.pitch_deg = (dip - ((540.0 - row) / focal).atan()).to_degrees();
        let p = HorizonProfile::measure(&scene, o.frame(), lens, viewport())
            .expect("subpixel visible arc omitted");
        close(p.apex.pixel[1], row, 2e-8);
        assert!(p.left.pixel[0] < 960.0 && p.right.pixel[0] > 960.0);
        close(p.left.pixel[1], 1080.0, 2e-7);
        close(p.right.pixel[1], 1080.0, 2e-7);
    }
}

#[test]
fn fisheye_visible_contour_respects_aperture_roll_and_portrait_clipping() {
    let scene = scene();
    for (roll, pitch) in [(0.0, -20.0), (37.0, 0.0), (137.0, -40.0)] {
        let o = observer(61.0, -179.9, 200000.0, 17.0, pitch, roll);
        let lens = Lens::new(Projection::EquidistantFisheye, 140.0);
        for view in [
            viewport(),
            GuideViewport::new([600.0, 1000.0], [1.5, 1.5]).unwrap(),
        ] {
            let p = HorizonProfile::measure(&scene, o.frame(), lens, view).unwrap();
            for point in [p.apex, p.left, p.right] {
                assert_tangent(point, o.frame());
                assert!(point.pixel[0] >= 0.0 && point.pixel[0] <= view.pixels[0]);
                let radial = ((point.pixel[0] - view.pixels[0] * 0.5).powi(2)
                    + (point.pixel[1] - view.pixels[1] * 0.5).powi(2))
                .sqrt();
                assert!(radial <= view.pixels[1] * 0.5 + 1e-6);
                let projected = lens.project(
                    o.frame(),
                    point.hit.hit.position - o.frame().origin,
                    view.pixels[0] / view.pixels[1],
                );
                // Lens::project may reject an edge through its own strict f64
                // comparison, so compare angular geometry without that gate.
                let d = (point.hit.hit.position - o.frame().origin)
                    .normalized()
                    .unwrap();
                let angle = d.dot(o.frame().forward).clamp(-1.0, 1.0).acos();
                close(
                    radial / (view.pixels[1] * 0.5) * 70.0_f64.to_radians(),
                    angle,
                    4e-12,
                );
                if let Some(ndc) = projected {
                    close((ndc[0] + 1.0) * view.pixels[0] * 0.5, point.pixel[0], 2e-8);
                    close((1.0 - ndc[1]) * view.pixels[1] * 0.5, point.pixel[1], 2e-8);
                }
            }
            for column in [0.2, 0.5, 0.8] {
                if let Some(ruler) = p.ruler(&scene, column) {
                    assert_tangent(ruler.ground, o.frame());
                    close(ruler.ground.pixel[0], column * view.pixels[0], 2e-7);
                }
            }
        }
    }
}

#[test]
fn no_visible_boundary_means_no_guide_and_physical_pixel_size_survives_hidpi() {
    let scene = scene();
    for pitch in [-90.0, 90.0] {
        let o = observer(35.0, 20.0, 1000.0, 0.0, pitch, 0.0);
        for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
            assert!(
                HorizonProfile::measure(&scene, o.frame(), Lens::new(projection, 30.0), viewport())
                    .is_none()
            );
        }
    }
    let o = observer(0.0, 0.0, 100000.0, 90.0, -10.0, 0.0);
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let view = GuideViewport::new([1280.0, 720.0], [scale, scale]).unwrap();
        let p = HorizonProfile::measure(&scene, o.frame(), Lens::default(), view).unwrap();
        let horizontal = p.horizontal_rect();
        close(
            f64::from(horizontal.max[1] - horizontal.min[1]) * f64::from(scale),
            1.0,
            6e-5,
        );
        if let Some(ruler) = p.ruler(&scene, 0.8) {
            let vertical = p.vertical_rect(ruler);
            close(
                f64::from(vertical.max[0] - vertical.min[0]) * f64::from(scale),
                1.0,
                1e-4,
            );
        }
    }
}

#[test]
fn cancellation_never_returns_a_measurement_and_constant_index_keeps_geometric_horizon() {
    let scene = scene();
    let o = observer(0.0, 0.0, 100.0, 90.0, -0.2, 0.0);
    let lens = Lens::new(Projection::Rectilinear, 5.0);
    let cancel = AtomicBool::new(true);
    let profile = AtmosphereProfile::from_layers(vec![
        AtmosphereLayer {
            height_m: 0.0,
            refractivity: 0.0,
        },
        AtmosphereLayer {
            height_m: 120000.0,
            refractivity: 0.0,
        },
    ])
    .unwrap();
    for optical in [None, Some(&profile)] {
        assert!(
            HorizonProfile::measure_cancellable(
                &scene,
                o.frame(),
                lens,
                viewport(),
                optical,
                &cancel
            )
            .is_none()
        );
    }
    let geometry = HorizonProfile::measure(&scene, o.frame(), lens, viewport()).unwrap();
    assert!(
        geometry
            .ruler_cancellable(&scene, 0.8, None, &cancel)
            .is_none()
    );
    let optical =
        HorizonProfile::measure_refracted(&scene, o.frame(), lens, viewport(), &profile).unwrap();
    close(optical.apex.pixel[1], geometry.apex.pixel[1], 0.04);
    assert!(optical.apex.bracket_pixels <= 0.025);
    assert!(
        optical
            .ruler_cancellable(&scene, 0.8, Some(&profile), &cancel)
            .is_none()
    );
}
