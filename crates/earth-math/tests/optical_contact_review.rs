//! Independent finite endpoint-contact and earlier-blocker checks.
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile, projection::tape_annotation},
    survey::{SurveyMarker, SurveyTarget, TapePoint},
    *,
};
use std::sync::atomic::AtomicBool;

fn point(position_ecef_m: Vec3d) -> TapePoint {
    TapePoint {
        position_ecef_m,
        target: SurveyTarget::Marker(0),
    }
}

fn field(bent: bool) -> AtmosphereProfile {
    AtmosphereProfile::from_layers(
        [(0.0, 0.0003), (2000.0, 0.00023), (120000.0, 0.0)]
            .map(|(height_m, n)| AtmosphereLayer {
                height_m,
                refractivity: if bent { n } else { 0.0 },
            })
            .to_vec(),
    )
    .unwrap()
}

#[test]
fn own_face_contact_preserves_bent_projection_and_cannot_skip_earlier_blockers() {
    let scene = TerrainScene::new(vec![]);
    let cancelled = AtomicBool::new(false);
    let atmosphere = field(true);
    let mut clipped_failures = 0;
    for latitude in [0.0, 29.305549, 70.0] {
        let mut observer = Observer::new(Geodetic::new(latitude, -94.767015, 100.0).unwrap());
        observer.pitch_deg = 24.0;
        let basis = EnuBasis::at(observer.eye_geodetic());
        let marker = SurveyMarker {
            id: 0,
            base_ecef_m: observer.eye_position() + basis.north * 30.48 - basis.up * 1.72,
            right: basis.east,
            forward: basis.north,
            up: basis.up,
            width_m: 3.048,
            depth_m: 3.048,
            height_m: 30.48,
            along_distance_m: 30.48,
        };
        // Both endpoints and all interior samples lie on the same exact
        // front/side edge of the physical OBB, not on its decorative sign.
        let a = point(marker.base_ecef_m - basis.north * 1.524 - basis.east * 1.524);
        let b = point(a.position_ecef_m + basis.up * marker.height_m);
        for index in 0..=32 {
            let target =
                a.position_ecef_m + (b.position_ecef_m - a.position_ecef_m) * (index as f64 / 32.0);
            let (ray, _) = atmosphere
                .direction_to_point(observer.eye_position(), target, &cancelled)
                .unwrap();
            let normal = (target - observer.eye_position()).normalized().unwrap();
            if atmosphere
                .trace_to_plane_scene(ray, &scene, &[marker], target, normal)
                .is_err()
            {
                clipped_failures += 1;
                // The independent unrestricted query must establish actual
                // first physical contact, not merely a nearby atmospheric exit.
                let first = atmosphere.trace_scene(ray, &scene, &[marker]).unwrap();
                assert_eq!(first.marker.unwrap().marker_id, 0);
                assert!(first.exit_ray.origin.distance(target) <= 0.005);
            }
        }
        for projection in [Projection::Rectilinear, Projection::EquidistantFisheye] {
            let annotation = tape_annotation(
                &atmosphere,
                &scene,
                &[marker],
                Some(a),
                Some(b),
                observer,
                Lens::new(projection, 60.0),
                [960.0, 540.0],
                [1.5, 1.5],
                &cancelled,
            )
            .unwrap()
            .unwrap();
            assert!(!annotation.quads.is_empty());
            assert!(annotation.endpoint_positions.iter().all(Option::is_some));
        }
    }
    assert!(
        clipped_failures > 0,
        "fixture must exercise the contact retry"
    );

    // In vacuum the front plane is exactly 100-gap metres along +Y. Thus the
    // first-contact distance to the target is independently the chosen gap.
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 100.0).unwrap());
    observer.heading_deg = 90.0;
    let eye = observer.eye_position();
    let target = point(eye + Vec3d::new(0.0, 100.0, 0.0));
    for gap in [0.02, 0.006, 0.004, 0.001, 0.0] {
        let blocker = SurveyMarker {
            id: 9,
            base_ecef_m: eye + Vec3d::new(-20.0, 105.0 - gap, 0.0),
            right: Vec3d::new(0.0, 0.0, -1.0),
            forward: Vec3d::new(0.0, 1.0, 0.0),
            up: Vec3d::new(1.0, 0.0, 0.0),
            width_m: 40.0,
            depth_m: 10.0,
            height_m: 40.0,
            along_distance_m: 100.0,
        };
        let annotation = tape_annotation(
            &field(false),
            &scene,
            &[blocker],
            Some(target),
            None,
            observer,
            Lens::default(),
            [960.0, 540.0],
            [1.0, 1.0],
            &cancelled,
        )
        .unwrap();
        assert_eq!(annotation.is_some(), gap < 0.005, "gap={gap}");
    }
    assert!(
        tape_annotation(
            &atmosphere,
            &scene,
            &[],
            Some(target),
            None,
            observer,
            Lens::default(),
            [960.0, 540.0],
            [1.0, 1.0],
            &AtomicBool::new(true),
        )
        .is_err()
    );
}
