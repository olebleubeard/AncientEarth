//! Independent source-frame ownership check through complete worker packets.
use super::*;
use crate::desktop::renderer::preparation;
use bevy::mesh::VertexAttributeValues;
use earth_math::{Geodetic, TerrainPatch, Vec3d};
use std::collections::BTreeSet;

#[test]
fn complete_prepared_snapshots_retain_their_own_scene_and_cartesian_frame() {
    let patch = TerrainPatch::from_grid(
        3,
        3,
        -0.005,
        0.005,
        0.005,
        0.005,
        vec![10.0, 20.0, 30.0, 40.0, 150.0, 60.0, 70.0, 80.0, 90.0],
    )
    .unwrap();
    let original = patch.canonical_vertices().to_vec();
    let scene = Arc::new(
        TerrainScene::with_terrain(
            vec![patch],
            Arc::new(TerrainScene::boundary_only(vec![]).unwrap()),
        )
        .unwrap(),
    );
    let observers = [
        Observer::new(Geodetic::new(0.0, 0.0, 200.0).unwrap()),
        Observer::new(Geodetic::new(0.002, -0.003, 1200.0).unwrap()),
    ];
    let frames = observers.map(LocalFrame::at);
    let packets = observers
        .into_iter()
        .zip(frames)
        .map(|(observer, local)| {
            preparation::Request {
                scene: scene.clone(),
                observer,
                local,
                plan: earth_math::rendering::continuous::DisplayPlan::at(observer, false),
                focal: 1.0,
                classify_import: true,
            }
            .prepare()
        })
        .collect::<Vec<_>>();
    let key = |p: [f32; 3]| p.map(f32::to_bits);
    for (i, packet) in packets.iter().enumerate() {
        assert!(Arc::ptr_eq(&packet.request.scene, &scene));
        assert_eq!(packet.request.observer, observers[i]);
        assert_eq!(packet.request.local.origin, observers[i].eye_position());
        let imported = packet.imported.as_ref().unwrap();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            imported.meshes[1].attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("actual terrain positions")
        };
        let expected: BTreeSet<_> = original
            .iter()
            .map(|&p| {
                // Explicit independent ENU-to-display expansion, not a call to
                // the production LocalFrame.position conversion under review.
                let delta = p - frames[i].origin;
                key([
                    delta.dot(frames[i].basis.east) as f32,
                    delta.dot(frames[i].basis.up) as f32,
                    -delta.dot(frames[i].basis.north) as f32,
                ])
            })
            .collect();
        assert_eq!(
            positions.iter().copied().map(key).collect::<BTreeSet<_>>(),
            expected
        );
        assert_eq!(imported.meshes[1].indices().unwrap().len(), 8 * 3);
        for &position in positions {
            let reconstruct = |frame: LocalFrame| {
                frame.origin
                    + frame.basis.east * f64::from(position[0])
                    + frame.basis.up * f64::from(position[1])
                    - frame.basis.north * f64::from(position[2])
            };
            let nearest = |p: Vec3d| {
                original
                    .iter()
                    .map(|&q| p.distance(q))
                    .fold(f64::INFINITY, f64::min)
            };
            assert!(nearest(reconstruct(frames[i])) < 0.0002);
            assert!(
                nearest(reconstruct(frames[1 - i])) > 500.0,
                "fixture distinguishes stale mesh/new frame"
            );
        }
    }
    // A remote observer chooses the same fixed near-Earth coordinate frame
    // at every distance; canonical source vertices must never acquire the
    // actual AU-sized eye offset. Exercise the real complete worker path.
    let mut previous_positions = None;
    for height in [3_000_000.0, earth_math::MAX_HEIGHT_M - 2.0] {
        let observer = Observer::new(Geodetic::new(37.0, -94.0, height).unwrap());
        let plan = earth_math::rendering::continuous::DisplayPlan::at(observer, false);
        let packet = preparation::Request {
            scene: scene.clone(),
            observer,
            local: plan.local,
            plan,
            focal: 1.0,
            classify_import: true,
        }
        .prepare();
        assert_eq!(packet.request.plan.detail.eye, observer.eye_position());
        assert_eq!(packet.request.plan.coordinate_observer.feet.height_m, 0.0);
        let imported = packet.imported.as_ref().unwrap();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            imported.meshes[1].attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("actual far terrain positions");
        };
        let actual = positions.iter().copied().map(key).collect::<BTreeSet<_>>();
        let fixed_origin = Vec3d::new(
            earth_math::WGS84_A_M + earth_math::DEFAULT_EYE_HEIGHT_M,
            0.0,
            0.0,
        );
        let expected = original
            .iter()
            .map(|&p| {
                let d = p - fixed_origin;
                key([d.y as f32, d.x as f32, -d.z as f32])
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        if let Some(previous) = previous_positions {
            assert_eq!(actual, previous);
        }
        previous_positions = Some(actual);
        assert_eq!(imported.meshes[1].indices().unwrap().len(), 8 * 3);
    }
}
