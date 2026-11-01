//! Read-only cached-source optical packet audit. Compile against the existing
//! earth-math, flate2 and serde_json rlibs; no graphics device or download.
use earth_math::{
    EnuBasis, Geodetic, GeoidGrid, Lens, Projection, Ray, TerrainPatch, TerrainScene,
    atmosphere::{
        gpu::scene::{MAX_GEOMETRY_BYTES, PackedScene},
        weather::{decode_weather, regional_reference_request},
    },
    ecef_to_geodetic, geodetic_to_ecef,
    rendering::LocalFrame,
    survey::nearby_demo,
    topography::{DownloadRegion, copernicus_tiles, import_sources},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};
mod optical_step_diagnostic;
mod optical_boundary_coverage;
mod optical_scene_ceiling;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Pass the already-reviewed topography cache directory.")?,
    );
    let selection: Value = serde_json::from_slice(&fs::read(cache.join("selection.json"))?)?;
    let number = |name| selection[name].as_f64().ok_or("Invalid cache selection.");
    let region = DownloadRegion::new(
        Geodetic::new(number("latitude")?, number("longitude")?, 0.0)?,
        number("radius_m")?,
    )?;
    let planned = copernicus_tiles(&region)?;
    let sources: Vec<_> = planned
        .iter()
        .filter_map(|tile| {
            let path = cache.join("assets").join(format!("{}.tif", tile.id));
            path.is_file().then(|| (tile.clone(), path))
        })
        .collect();
    let mut geoid_bytes = Vec::new();
    flate2::read::GzDecoder::new(fs::File::open("assets/geoid/egm2008-native.aeg.gz")?)
        .read_to_end(&mut geoid_bytes)?;
    let geoid = GeoidGrid::from_bytes(&geoid_bytes)?;
    drop(geoid_bytes);
    let patches = (1..=4)
        .map(|i| {
            TerrainPatch::from_boundary_bytes(&fs::read(format!(
                "assets/boundaries/region-{i:02}.aeb"
            ))?)
            .map_err(Into::into)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let boundaries = Arc::new(TerrainScene::boundary_only(patches)?);
    let import_start = Instant::now();
    let (scene, summary) = import_sources(
        &region,
        &sources,
        &geoid,
        boundaries,
        &AtomicBool::new(false),
    )?;
    let import_seconds = import_start.elapsed().as_secs_f64();
    let pack_start = Instant::now();
    let packet = PackedScene::prepare(&scene)?;
    let pack_seconds = pack_start.elapsed().as_secs_f64();
    let expected_faces: usize = scene.patches().iter().map(|p| p.triangle_count()).sum();
    assert_eq!(expected_faces, packet.triangle_count);
    let geometry_bytes = packet.geometry.len() * std::mem::size_of::<[f32; 4]>();
    assert!(geometry_bytes <= MAX_GEOMETRY_BYTES);
    let demo = nearby_demo(&scene)?;
    if std::env::args().any(|arg| arg == "--height-ceiling") {
        use earth_math::{Vec3d, rendering::survey::marker_sign_frame};
        use optical_scene_ceiling::HeightCeiling;
        let mut global = HeightCeiling::default();
        let mut terrain = Vec::new();
        for (id, (patch, packed)) in scene.patches().iter().zip(&packet.patches).enumerate() {
            let mut ceiling = HeightCeiling::default();
            for &point in patch.canonical_vertices() {
                ceiling.include(point).ok_or("finite terrain ceiling")?;
                global.include(point).ok_or("finite global ceiling")?;
            }
            terrain.push(json!({"patch":id,"vertices":ceiling.vertices,
                "canonical_height_ceiling_m":ceiling.upper_m,
                "packed_canonical_ceiling_m":ceiling.packed_with_position_allowance(0.0),
                "maximizing_vertex_ecef_m":ceiling.maximizing_vertex.map(|p|[p.x,p.y,p.z]),
                "observed_source_quantization_error_m":packed.source_position_error_m,
                "scope":"Canonical face ceiling only. Measured local source quantization is retained separately; finite runtime frame/map/query errors are not silently included."}));
        }
        let mut marker_records = Vec::new();
        for marker in &demo.markers {
            let sign = marker_sign_frame(marker,demo.observer.eye_position()).ok_or("sign frame")?;
            let width = marker.width_m;
            let height = width*9.0/25.0;
            let corners:Vec<Vec3d> = marker.corners().into_iter().chain(
                [-0.5,0.5].into_iter().flat_map(|x|[-0.5,0.5].into_iter().map(move |y|
                    sign.center+sign.right*(x*width)+sign.up*(y*height)))
            ).collect();
            let mut ceiling = HeightCeiling::default();
            for &point in &corners {
                ceiling.include(point).ok_or("finite marker ceiling")?;
                global.include(point).ok_or("finite global ceiling")?;
            }
            marker_records.push(json!({"marker_id":marker.id,"canonical_corner_count":corners.len(),
                "canonical_height_ceiling_m":ceiling.upper_m,
                "packed_canonical_ceiling_m":ceiling.packed_with_position_allowance(0.0),
                "canonical_prism_then_sign_corners_ecef_m":corners.iter().map(|p|[p.x,p.y,p.z]).collect::<Vec<_>>(),
                "maximizing_vertex_ecef_m":ceiling.maximizing_vertex.map(|p|[p.x,p.y,p.z])}));
        }
        let eye_position=demo.observer.eye_position();
        println!("{}",serde_json::to_string_pretty(&json!({
            "schema":"ancientearth.optical-scene-height-ceiling.diagnostic.v1",
            "scope":"Unused sufficient canonical-height metadata for actual retained terrain faces, all prism corners, current eye-facing sign corners, and h=0 fallback. Not a sky/no-return test and not a replacement for BVH queries when the condition fails.",
            "source_ids":sources.iter().map(|(t,_)|&t.id).collect::<Vec<_>>(),
            "eye_ecef_m":[eye_position.x,eye_position.y,eye_position.z],
            "canonical_vertex_count_including_marker_sign_corners":global.vertices,
            "canonical_height_ceiling_m":global.upper_m,
            "packed_canonical_ceiling_m":global.packed_with_position_allowance(0.0),
            "h0_fallback_included":true,"terrain":terrain,"markers":marker_records,
            "packing_and_query_policy":"Pack the canonical scalar upward. Add, outward, a proven bound for any geometric vertex representation displacement using the signed-distance 1-Lipschitz property. The whole-curve height lower bound must also subtract its own finite coordinate/map, integration and contact uncertainty. Skip a scene query only when that final lower bound is STRICTLY above the final ceiling over the entire interval. Unknown or nonfinite allowances defer to the full scene query. Marker/sign ceilings are recomputed whenever their actual geometry/eye-facing frame changes. The canonical ceiling alone does not certify the existing f32 packet's query arithmetic.",
        }))?);
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--coverage-core-packet") {
        use earth_math::Vec3d;
        let eye = LocalFrame::at(demo.observer);
        let probe = eye.origin + eye.basis.east * (-1774.36315918)
            + eye.basis.up * 25.40311050 - eye.basis.north * (-2125.82836914);
        let mut records = Vec::new();
        for (id, patch) in scene.patches().iter().enumerate() {
            let mesh = patch.optical_mesh();
            let frame = packet.patches[id].frame;
            let chain = optical_boundary_coverage::BoundaryChain::in_frame(
                mesh.vertices, &mesh.triangles, frame.origin,
                [frame.basis.east, -frame.basis.north, frame.basis.up],
            ).ok_or("canonical patch-frame boundary")?;
            let Some(core) = chain.interior_core() else {
                records.push(json!({"patch":id,"core":null}));
                continue;
            };
            let center = [core.center.x as f32, core.center.z as f32, core.center.y as f32];
            let delta = Vec3d::new(
                f64::from(center[0]) - core.center.x,
                f64::from(center[2]) - core.center.y,
                f64::from(center[1]) - core.center.z,
            );
            // Recentered packed rectangle is a strict subset of the proven
            // one; this is preferable to an unrecorded per-query center error.
            let half = [
                ((core.half_extent[0] - delta.x.abs()) as f32).next_down(),
                ((core.half_extent[1] - delta.y.abs()) as f32).next_down(),
            ];
            let radius = ((core.boundary_radius + delta.length()) as f32).next_up();
            let vertical = [(chain.min.z as f32).next_down(), (chain.max.z as f32).next_up()];
            assert!(half.iter().all(|x| *x > 0.0));
            assert!(f64::from(half[0]) + delta.x.abs() <= core.half_extent[0]);
            assert!(f64::from(half[1]) + delta.y.abs() <= core.half_extent[1]);
            assert!(f64::from(radius) >= core.boundary_radius + delta.length());
            assert!(f64::from(vertical[0]) <= chain.min.z && f64::from(vertical[1]) >= chain.max.z);
            let edge = chain.edges.first().ok_or("boundary edge")?;
            let external = mesh.vertices[edge.0 as usize];
            let mut queries = Vec::new();
            for (name, point) in [("observer", eye.origin), ("pixel110941", probe), ("external_vertex", external)] {
                let location = ecef_to_geodetic(point)?;
                let p = geodetic_to_ecef(location.with_height(0.0)?);
                let v = EnuBasis::at(location).up;
                let query = frame.position(p);
                let direction = frame.direction(v);
                let dp = p - frame.origin;
                let exact_point = [dp.dot(frame.basis.east),dp.dot(frame.basis.up),-dp.dot(frame.basis.north)];
                let exact_direction = [v.dot(frame.basis.east),v.dot(frame.basis.up),-v.dot(frame.basis.north)];
                let error_norm = |exact:[f64;3],packed:[f32;3]| exact.into_iter().zip(packed)
                    .map(|(a,b)|(a-f64::from(b)).powi(2)).sum::<f64>().sqrt().next_up();
                let point_quantization = error_norm(exact_point,query);
                let direction_quantization = error_norm(exact_direction,direction);
                let transform_allowance = 64.0*f64::EPSILON*(1.0+p.length()+frame.origin.length()+dp.length());
                let point_error = 0.01_f64.max(point_quantization+transform_allowance).next_up();
                let direction_error = 1e-6_f64.max(direction_quantization+64.0*f64::EPSILON).next_up();
                let vertex_error = 0.003_f64.max(packet.patches[id].source_position_error_m+transform_allowance).next_up();
                let certificate = chain.certify_core(core,p,v,point_error,direction_error,vertex_error,[-10001.0,101000.0]);
                let hit = patch.intersect(Ray { origin:p+v*101000.0,direction:-v });
                if name == "external_vertex" { assert!(certificate.is_none()); }
                queries.push(json!({"name":name,"point":query,"direction":direction,
                    "exact_point_before_f32":exact_point,"exact_direction_before_f32":exact_direction,
                    "point_quantization_error_m":point_quantization,"direction_quantization_error":direction_quantization,
                    "f64_transform_allowance_m":transform_allowance,
                    "point_error_m":point_error,"direction_error":direction_error,"vertex_error_m":vertex_error,
                    "parameter_range_m":[-10001.0,101000.0],
                    "f64_core_certifies":certificate.is_some(),"canonical_normal_line_hit_m":hit.map(|h|h.distance_m)}));
            }
            records.push(json!({"patch":id,"faces":mesh.triangles.len(),"degree":core.degree,
                "center":center,"half_extent_xz":half,"boundary_radius":radius,"vertical_bounds_y":vertical,
                "original_center_xyz_up":[core.center.x,core.center.y,core.center.z],
                "original_half_extent":core.half_extent,"original_boundary_radius":core.boundary_radius,
                "original_vertical_bounds_z":[chain.min.z,chain.max.z],"center_quantization_error_m":delta.length(),
                "source_position_error_m":packet.patches[id].source_position_error_m,"queries":queries}));
        }
        println!("{}",serde_json::to_string_pretty(&json!({"scope":"Unused sufficient coverage-core packet in existing E/up/negative-N patch coordinates. Inward rectangle and outward radius/Y bounds preserve the canonical boundary-degree preparation proof. Source/transform query errors remain explicit; no GPU execution or production claim.","patches":records}))?);
        return Ok(());
    }
    if std::env::args().any(|arg|arg=="--boundary-coverage") {
        use earth_math::Vec3d;
        let local=LocalFrame::at(demo.observer);
        let positions=[local.origin,
            local.origin+local.basis.east*(-1774.36315918)+local.basis.up*25.40311050-local.basis.north*(-2125.82836914)];
        let mut patches=Vec::new();
        for (id,patch) in scene.patches().iter().enumerate() {
            let mesh=patch.optical_mesh();
            let chain=optical_boundary_coverage::BoundaryChain::build(mesh.vertices,&mesh.triangles).ok_or("canonicalboundarychain")?;
            let core=chain.interior_core();
            let mut rows=Vec::new();
            for (name,point) in ["observernormal","pixel110941"].into_iter().zip(positions) {
                let location=ecef_to_geodetic(point)?;
                let p=geodetic_to_ecef(location.with_height(0.0)?);
                let v=EnuBasis::at(location).up;
                let certificate=chain.certify(p,v,0.01,1e-6,0.003,[-10001.0,101000.0]);
                let core_certificate=core.and_then(|c|chain.certify_core(c,p,v,0.01,1e-6,0.003,[-10001.0,101000.0]));
                let hit=patch.intersect(Ray{origin:p+v*101000.0,direction:-v});
                rows.push(json!({"name":name,"certificate":certificate.map(|c|json!({"degree":c.degree,"boundary_margin_m":c.minimum_boundary_margin_m,"line_t_m":c.line_parameter_interval_m})),"core_certificate":core_certificate.map(|c|json!({"degree":c.degree,"boundary_margin_m":c.minimum_boundary_margin_m,"line_t_m":c.line_parameter_interval_m})),"exact_canonical_hit":hit.map(|h|h.distance_m)}));
            }
            // Deliberately test an actual external vertex; finite uncertainty
            // must deny a certificate instead of extending the footprint.
            let boundary_point=chain.edges.first().map(|&(a,_,_)|chain.vertex_ecef(a)).unwrap_or(Vec3d::ZERO);
            let vertex_location=ecef_to_geodetic(boundary_point)?;
            let vertex_zero=geodetic_to_ecef(vertex_location.with_height(0.0)?);
            let boundary_certificate=chain.certify(vertex_zero,EnuBasis::at(vertex_location).up,0.01,1e-6,0.003,[-10001.0,101000.0]);
            assert!(boundary_certificate.is_none());
            assert!(core.and_then(|c|chain.certify_core(c,vertex_zero,EnuBasis::at(vertex_location).up,0.01,1e-6,0.003,[-10001.0,101000.0])).is_none());
            patches.push(json!({"patch":id,"faces":mesh.triangles.len(),"boundary_edges":chain.edges.len(),"core":core.map(|c|json!({"center":[c.center.x,c.center.y,c.center.z],"half_extent":c.half_extent,"boundary_radius":c.boundary_radius,"degree":c.degree})),"external_vertex_uncertain":true,"queries":rows}));
        }
        println!("{}",serde_json::to_string_pretty(&json!({"scope":"Unused canonical oriented-boundary coverage certificate. Nonzero degree only; exact source geometry, explicit finite input envelopes, no GPU arithmetic or runtime adoption claim.","patches":patches}))?);
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--contact-audit") {
        let launches: Value =
            serde_json::from_slice(&fs::read("artifacts/optical-selected-launch-review.json")?)?;
        let local = LocalFrame::at(demo.observer);
        let vector = |v: &[Value]| -> Result<earth_math::Vec3d, Box<dyn std::error::Error>> {
            let word = |i: usize| {
                v[i].as_u64()
                    .map(|x| f32::from_bits(x as u32) as f64)
                    .ok_or("terminal word")
            };
            Ok(local.basis.east * word(0)? + local.basis.up * word(1)?
                - local.basis.north * word(2)?)
        };
        let mut results = Vec::new();
        for row in launches["runs"][0]["rows"]
            .as_array()
            .ok_or("launch rows")?
        {
            if row["pixel"] != json!([1257, 956]) && row["pixel"] != json!([1392, 957]) {
                continue;
            }
            let words = row["terminal_words"].as_array().ok_or("terminal words")?;
            let point = local.origin + vector(&words[0..3])?;
            let direction = vector(&words[4..7])?
                .normalized()
                .ok_or("terminal direction")?;
            let location = ecef_to_geodetic(point)?;
            let ground = scene
                .imported_ground(location)
                .ok_or("source coverage at returned point")?;
            let residual = (point - ground.hit.position).dot(ground.hit.normal);
            let slope = direction.dot(ground.hit.normal);
            results.push(json!({"pixel":row["pixel"],"gpu_point_lat_lon_h":[location.lat_deg,location.lon_deg,location.height_m],
                "source_ground_height_m":ground.hit.geodetic.height_m,"source_height_gap_m":location.height_m-ground.hit.geodetic.height_m,
                "normal_plane_residual_m":residual,"ray_dot_source_normal":slope,
                "local_linear_plane_distance_m":-residual/slope,
                "source_ground":format!("{ground:?}"),"cpu_reference":row["f64_physical_reference_from_readback_launch"]}));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"cases":results,
            "scope":"Actual source geometry at exact GPU returned points. Local plane extrapolation diagnoses finite contact bands; it is not a replacement for curved first-contact tracing."}))?
        );
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--coverage") {
        let local = LocalFrame::at(demo.observer);
        let point =
            local.origin + local.basis.east * (-1774.36315918) + local.basis.up * 25.40311050
                - local.basis.north * (-2125.82836914);
        let location = ecef_to_geodetic(point)?;
        let up = EnuBasis::at(location).up;
        let at_zero = geodetic_to_ecef(location.with_height(0.0)?);
        let request = regional_reference_request();
        let loaded = decode_weather(
            &fs::read("assets/weather/gfs-regional-reference.grib2")?,
            Some(&fs::read("assets/weather/hrrr-reference.grib2")?),
            &request,
        )?;
        let profile = loaded
            .profile
            .anchored_to_scene_at(&scene, demo.observer.feet)?;
        let ray = demo
            .lens
            .pixel_ray(demo.observer.frame(), 110, 941, 1920, 1080)
            .ok_or("coverage pixel")?;
        let target = profile.trace_to_plane_scene(ray, &scene, &demo.markers, at_zero, up)?;
        let normal_ray = Ray {
            origin: at_zero + up * 101000.0,
            direction: -up,
        };
        let patches:Vec<_>=scene.patches().iter().enumerate().filter_map(|(index,patch)|{
            patch.intersect(normal_ray).map(|hit|json!({"patch_index":index,"bounds":patch.bounds(),"hit":format!("{hit:?}")}))
        }).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"pixel":[110,941],"gpu_point_ecef_m":[point.x,point.y,point.z],"gpu_point_lat_lon_h":[location.lat_deg,location.lon_deg,location.height_m],"canonical_imported_ground":format!("{:?}",scene.imported_ground(location)),"canonical_normal_hits":patches,"cpu_tangent_plane_point":target.map(|p|[p.x,p.y,p.z]),"cpu_tangent_plane_local":target.map(|p|local.position(p)),"cpu_tangent_plane_geodetic":target.map(ecef_to_geodetic).transpose()?.map(|g|[g.lat_deg,g.lon_deg,g.height_m]),"cpu_tangent_plane_imported_ground":target.map(ecef_to_geodetic).transpose()?.map(|g|format!("{:?}",scene.imported_ground(g))),"scope":"Read-only actual-source normal-line and tangent-plane audit. Tangent plane is locally through h0, not a replacement curved h0 definition."})
            )?
        );
        return Ok(());
    }
    let frame = packet.frame_data(LocalFrame::at(demo.observer), &demo.markers)?;
    let frame_bytes = frame.len() * std::mem::size_of::<[f32; 4]>();
    let cpu_rays = if std::env::args()
        .any(|arg| arg == "--rays" || arg == "--tape" || arg == "--steps")
    {
        let request = regional_reference_request();
        let loaded = decode_weather(
            &fs::read("assets/weather/gfs-regional-reference.grib2")?,
            Some(&fs::read("assets/weather/hrrr-reference.grib2")?),
            &request,
        )?;
        assert!(loaded.regional_fallback.is_none());
        let profile = loaded
            .profile
            .anchored_to_scene_at(&scene, demo.observer.feet)?;
        let mut rays = Vec::new();
        for y in if std::env::args().any(|arg| arg == "--rays" || arg == "--steps") {
            vec![180, 540, 900]
        } else {
            vec![]
        } {
            for x in [320, 960, 1600] {
                let ray = demo
                    .lens
                    .pixel_ray(demo.observer.frame(), x, y, 1920, 1080)
                    .ok_or("Invalid diagnostic pixel.")?;
                let start = Instant::now();
                let result = profile.trace_scene(ray, &scene, &demo.markers);
                let elapsed_seconds = start.elapsed().as_secs_f64();
                let outcome = match result {
                    Ok(hit) => json!({
                        "resolved":true,
                        "steps":hit.steps,
                        "geometric_path_m":hit.geometric_path_m,
                        "optical_path_m":hit.optical_path_m,
                        "marker_id":hit.marker.map(|m|m.marker_id),
                        "surface_kind":hit.surface.map(|s|format!("{:?}",s.kind)),
                        "endpoint_ecef_m":[hit.exit_ray.origin.x,hit.exit_ray.origin.y,hit.exit_ray.origin.z],
                        "exit_direction":[hit.exit_ray.direction.x,hit.exit_ray.direction.y,hit.exit_ray.direction.z],
                        "step_diagnostic":if std::env::args().any(|arg|arg=="--steps") && hit.surface.is_none() && hit.marker.is_none(){
                            Some(optical_step_diagnostic::analyze(&profile,ray,&hit)?)
                        }else{None},
                    }),
                    Err(error) => json!({"resolved":false,"error":error.to_string()}),
                };
                rays.push(json!({
                    "pixel_xy":[x,y],"outcome":outcome,
                    "origin_ecef_m":[ray.origin.x,ray.origin.y,ray.origin.z],
                    "initial_direction":[ray.direction.x,ray.direction.y,ray.direction.z],
                    "elapsed_seconds_in_this_run":elapsed_seconds,
                }));
            }
        }
        let tape = if std::env::args().any(|arg| arg == "--tape") {
            let cancel = AtomicBool::new(false);
            let mut samples = Vec::new();
            for i in 0..=32 {
                let target = demo.tape_a.position_ecef_m
                    + (demo.tape_b.position_ecef_m - demo.tape_a.position_ecef_m)
                        * (f64::from(i) / 32.0);
                let inverse =
                    profile.direction_to_point(demo.observer.eye_position(), target, &cancel);
                let outcome = match inverse {
                    Err(error) => json!({"inverse_error":error.to_string()}),
                    Ok((ray, path)) => {
                        let normal = (target - ray.origin)
                            .normalized()
                            .ok_or("Invalid tape target")?;
                        let plane = profile.trace_to_plane_scene(
                            ray,
                            &scene,
                            &demo.markers,
                            target,
                            normal,
                        );
                        let full = profile.trace_scene(ray, &scene, &demo.markers);
                        let first_contact = full
                            .as_ref()
                            .ok()
                            .filter(|hit| hit.surface.is_some() || hit.marker.is_some());
                        json!({"inverse_path_m":path,
                            "plane":format!("{plane:?}"),
                            "first_contact_target_distance_m":first_contact.map(|h|h.exit_ray.origin.distance(target)),
                            "first_contact_marker_id":first_contact.and_then(|h|h.marker).map(|m|m.marker_id),
                            "full":format!("{full:?}"),
                        })
                    }
                };
                samples.push(json!({"fraction_index_over_32":i,"outcome":outcome}));
            }
            let annotations: Vec<_> = [demo.lens, Lens::new(Projection::EquidistantFisheye, 140.0)]
                .into_iter()
                .map(|lens| {
                    let start = Instant::now();
                    let annotation = earth_math::atmosphere::projection::tape_annotation(
                        &profile,
                        &scene,
                        &demo.markers,
                        Some(demo.tape_a),
                        Some(demo.tape_b),
                        demo.observer,
                        lens,
                        [1920.0, 1080.0],
                        [1.0, 1.0],
                        &cancel,
                    );
                    let elapsed = start.elapsed().as_secs_f64();
                    match annotation {
                        Ok(Some(value)) => json!({
                            "projection":format!("{:?}",lens.projection),
                            "vertical_fov_deg":lens.vertical_fov_deg,
                            "resolved":true,"quad_count":value.quads.len(),
                            "endpoint_positions":value.endpoint_positions,
                            "label_position":value.label_position,
                            "quads":value.quads,
                            "elapsed_seconds_in_this_run":elapsed,
                        }),
                        other => json!({"projection":format!("{:?}",lens.projection),
                        "resolved":false,"result":format!("{other:?}")}),
                    }
                })
                .collect();
            Some(json!({"annotations":annotations,"samples":samples}))
        } else {
            None
        };
        Some(json!({
            "observer_feet_lat_lon_h":[demo.observer.feet.lat_deg,demo.observer.feet.lon_deg,demo.observer.feet.height_m],
            "observer_heading_pitch_roll_deg":[demo.observer.heading_deg,demo.observer.pitch_deg,demo.observer.roll_deg],
            "lens_vertical_fov_deg":demo.lens.vertical_fov_deg,
            "profile_base_height_m":profile.base_height_m(),
            "profile_layers":profile.layers().iter().map(|k|json!({"height_m":k.height_m,"refractivity":k.refractivity})).collect::<Vec<_>>(),
            "maximum_index_gradient_per_m":profile.maximum_index_gradient(),
            "rays":rays,
            "tape":tape,
        }))
    } else {
        None
    };
    let per_patch: Vec<_> = scene
        .patches()
        .iter()
        .zip(&packet.patches)
        .map(|(source, packed)| {
            json!({
                "bounds_wsen_deg":source.bounds(),
                "vertices":source.canonical_vertices().len(),
                "triangles":packed.triangles,
                "maximum_local_f32_vertex_error_m":packed.source_position_error_m,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema":"ancientearth.optical-scene-budget.v1",
            "selection":selection,
            "planned_sources":planned.len(),
            "available_source_files":sources.len(),
            "source_ids":sources.iter().map(|(t,_)|&t.id).collect::<Vec<_>>(),
            "summary":summary.description(),
            "patch_count":packet.patches.len(),
            "triangle_count":packet.triangle_count,
            "retained_vertices":summary.retained_vertices,
            "geometry_vec4_records":packet.geometry.len(),
            "geometry_bytes":geometry_bytes,
            "geometry_budget_bytes":MAX_GEOMETRY_BYTES,
            "marker_count":demo.markers.len(),
            "frame_bytes":frame_bytes,
            "cpu_reference_rays":cpu_rays,
            "patches":per_patch,
            "import_seconds_in_this_run":import_seconds,
            "pack_seconds_in_this_run":pack_seconds,
            "scope":"Production source import, geoid conversion, complete canonical optical face packing and nearby survey frame. Categories omitted from packet audit because they do not change physical geometry. No GPU execution or frame-performance claim; timings may include concurrent work. Source cache is read only."
        }))?
    );
    Ok(())
}
