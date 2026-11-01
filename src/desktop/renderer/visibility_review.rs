//! Independent regression for the unused-origin placeholder compilation stall.
use super::*;
use bevy::mesh::VertexAttributeValues;
use earth_math::{Geodetic, TerrainPatch, TerrainScene, water_mask::GlobalLandMask};

fn triangle_area(mesh: &Mesh) -> f64 {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("position attribute required");
    };
    let indices = mesh.indices().unwrap().iter().collect::<Vec<_>>();
    indices
        .chunks_exact(3)
        .map(|face| {
            let point = |index: usize| {
                let p = points[index];
                Vec3d::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
            };
            (point(face[1]) - point(face[0]))
                .cross(point(face[2]) - point(face[0]))
                .length()
                * 0.5
        })
        .sum()
}

#[test]
fn classifier_placeholders_stay_hidden_without_suppressing_real_surface_owners() {
    // A zero-area mesh still has an indexed primitive and can become a frustum
    // candidate when the coordinate origin enters view. It must be hidden,
    // rather than relying on its eventual raster coverage being zero.
    let blank = blank_mesh();
    assert_eq!(blank.indices().unwrap().len(), 3);
    assert_eq!(triangle_area(&blank), 0.0);

    let boundaries = Arc::new(TerrainScene::boundary_only(vec![]).unwrap());
    assert!(!terrain::uses_classifier(&boundaries));
    let global = GlobalLandMask::from_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/boundaries/global-land.aev"
    )))
    .unwrap();
    let smooth = TerrainScene::boundary_only(vec![])
        .unwrap()
        .with_global_land_mask(Arc::new(global));
    assert!(smooth.is_boundary_only());
    assert!(terrain::uses_classifier(&smooth));

    let patch = TerrainPatch::from_grid(2, 2, 0.0, 0.001, 0.001, 0.001, vec![20.0; 4]).unwrap();
    let imported = Arc::new(TerrainScene::with_terrain(vec![patch], boundaries).unwrap());
    assert!(terrain::uses_classifier(&imported));
    let observer = Observer::new(Geodetic::new(0.0005, 0.0005, 200.0).unwrap());
    let plan = DisplayPlan::at(observer, false);
    let packet = preparation::Request {
        scene: imported,
        observer,
        local: plan.local,
        plan,
        focal: 1.0,
        classify_import: true,
    }
    .prepare();
    assert!(packet.boundary.regions.is_empty());
    assert_eq!(triangle_area(&packet.boundary.reference), 0.0);
    let real = packet.imported.unwrap();
    assert!(real.triangles > 2);
    // The separately reviewed complete-worker fixture checks the exact source
    // face/position multiset inside this owner, including at 3 Mm and 4 AU.

    for coherent in [false, true] {
        for classifier in [false, true] {
            for enabled in [false, true] {
                for present in [false, true] {
                    for object in [
                        SceneObject::Reference,
                        SceneObject::Patch,
                        SceneObject::Water,
                    ] {
                        let expected = if coherent && !classifier {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        };
                        assert_eq!(
                            surface_visibility(&object, coherent, classifier, enabled, present),
                            Some(expected)
                        );
                    }
                    let grass = surface_visibility(
                        &SceneObject::Grass,
                        coherent,
                        classifier,
                        enabled,
                        present,
                    );
                    assert_eq!(
                        grass,
                        Some(if coherent && enabled && present {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        })
                    );
                    // This gate must not override celestial, sky, or separate
                    // classifier-chunk visibility decisions.
                    for object in [
                        SceneObject::Sun,
                        SceneObject::Moon,
                        SceneObject::Stars,
                        SceneObject::Sky,
                    ] {
                        assert_eq!(
                            surface_visibility(&object, coherent, classifier, enabled, present),
                            None
                        );
                    }
                }
            }
        }
    }
}
