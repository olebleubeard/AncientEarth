//! Original MIT. Unregistered manufactured-curve tests against the retained
//! canonical terrain/prism/sign scene. These curves are NOT ODE trajectories.
//! Approximate inverse coordinates choose a finite represented path only;
//! its actual stored coefficients and all source faces are exported for an
//! independent first-contact oracle. No expected hit is inferred from a seed.
#[allow(dead_code)]
#[path = "optical_normal_anchor_packet.rs"]
mod anchor;
#[allow(dead_code)]
#[path = "optical_cartesian_curve.rs"]
mod cartesian;
#[allow(dead_code)]
#[path = "optical_normal_ideal_frame.rs"]
mod ideal;
#[allow(dead_code)]
#[path = "optical_normal_map_packet.rs"]
mod map_packet;
#[allow(dead_code)]
#[path = "optical_gpu_probe.rs"]
mod source;
use cartesian::{Bounds as I, Curve};
use earth_math::{
    EnuBasis, Vec3d, WGS84_A_M, WGS84_E2,
    atmosphere::gpu::scene::PackedScene,
    ecef_to_geodetic,
    rendering::{LocalFrame, survey::marker_sign_frame},
};
use serde_json::json;
use std::{fs, path::Path};

const SCENE: &str = "artifacts/optical-normal-scene-packet";
const BOX: [[usize; 3]; 12] = [
    [0, 2, 1],
    [0, 3, 2],
    [4, 5, 6],
    [4, 6, 7],
    [0, 1, 5],
    [0, 5, 4],
    [1, 2, 6],
    [1, 6, 5],
    [2, 3, 7],
    [2, 7, 6],
    [3, 0, 4],
    [3, 4, 7],
];
#[derive(Clone)]
struct Face {
    p: [Vec3d; 3],
    material: u32,
    owner: u32,
    address: u32,
}
fn xyz(v: Vec3d) -> [f64; 3] {
    [v.x, v.y, v.z]
}
fn vec3(v: [f64; 3]) -> Vec3d {
    Vec3d::new(v[0], v[1], v[2])
}
fn centroid(f: &Face) -> Vec3d {
    f.p[0] + (f.p[1] - f.p[0]) / 3. + (f.p[2] - f.p[0]) / 3.
}
fn normal(f: &Face) -> Vec3d {
    (f.p[1] - f.p[0])
        .cross(f.p[2] - f.p[0])
        .normalized()
        .unwrap()
}
fn words(path: &str) -> Vec<u32> {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len().is_multiple_of(4));
    bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn floats(bytes: &[[f32; 4]]) -> Vec<u8> {
    bytes
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}
fn f(v: u32) -> f64 {
    f64::from(f32::from_bits(v))
}
fn midpoint(v: [f64; 2]) -> f64 {
    v[0] + (v[1] - v[0]) * 0.5
}
fn put(record: &mut [[u32; 4]; 16], row: usize, v: [f32; 3]) {
    for i in 0..3 {
        record[row][i] = v[i].to_bits();
    }
}
fn outward(v: [f64; 2]) -> [f32; 2] {
    let mut lo = v[0] as f32;
    let mut hi = v[1] as f32;
    if f64::from(lo) > v[0] {
        lo = lo.next_down();
    }
    if f64::from(hi) < v[1] {
        hi = hi.next_up();
    }
    if lo != 0. && lo.abs() < f32::MIN_POSITIVE {
        lo = if lo < 0. { -f32::MIN_POSITIVE } else { 0. };
    }
    if hi != 0. && hi.abs() < f32::MIN_POSITIVE {
        hi = if hi > 0. { f32::MIN_POSITIVE } else { 0. };
    }
    if v[0] < 0. && lo == 0. {
        lo = -f32::MIN_POSITIVE;
    }
    if v[1] > 0. && hi == 0. {
        hi = f32::MIN_POSITIVE;
    }
    assert!(lo.is_finite() && hi.is_finite() && f64::from(lo) <= v[0] && f64::from(hi) >= v[1]);
    [lo, hi]
}
fn prove_face_identity(face: &Face, frame: ideal::IdealFrame, retained: &[u32]) {
    let difference = |a: Vec3d, b: Vec3d| {
        std::array::from_fn(|i| {
            let v = I::point(xyz(a)[i]) - I::point(xyz(b)[i]);
            [v.lo, v.hi]
        })
    };
    let vectors = [
        frame.eye_minus_point(xyz(face.p[0])),
        frame.project(difference(face.p[1], face.p[0])),
        frame.project(difference(face.p[2], face.p[0])),
    ];
    let mut encoded = Vec::new();
    for v in vectors {
        let p = v.map(outward);
        encoded.extend(
            [p[0][0], p[1][0], p[2][0], 0., p[0][1], p[1][1], p[2][1], 0.].map(f32::to_bits),
        );
    }
    let start = face.address as usize * 4;
    assert_eq!(
        encoded,
        &retained[start..start + 24],
        "Fresh canonical face/ideal frame must reproduce the retained certificate exactly"
    );
}

struct Test {
    name: &'static str,
    face: usize,
    point: Vec3d,
    direction: Vec3d,
    before: f64,
    after: f64,
    normal_line: bool,
    padding: f32,
    expectation: &'static str,
}

fn main() {
    let prefix = std::env::args().nth(1).expect("new artifact prefix");
    for suffix in [".json", "-curves.bin", "-pieces.bin", "-faces.bin"] {
        assert!(
            !Path::new(&format!("{prefix}{suffix}")).exists(),
            "Preserve prior evidence"
        );
    }
    let case = source::actual_scene();
    let observer = case.observer.frame().origin;
    let eye = ecef_to_geodetic(observer).unwrap();
    let frame = ideal::IdealFrame::from_eye(eye).unwrap();
    let packet = PackedScene::prepare(&case.scene).unwrap();
    assert_eq!(
        floats(&packet.geometry),
        fs::read("artifacts/optical-requested-stellar-sparse-chain-geometry.bin").unwrap()
    );
    assert_eq!(
        floats(
            &packet
                .frame_data(LocalFrame::at(case.observer), &case.markers)
                .unwrap()
        ),
        fs::read(format!("{SCENE}-frames.bin")).unwrap()
    );
    let certificates = words(&format!("{SCENE}-certificates.bin"));
    let field = words("artifacts/optical-height-trial-capture-uniform.bin");
    assert_eq!(field.len(), 1044);
    let tolerance = f32::from_bits(field[11]);
    assert_eq!(
        tolerance, 0.002,
        "Use the retained minimum contact allocation"
    );
    let axis = std::array::from_fn(|i| I {
        lo: f(certificates[8 + i]),
        hi: f(certificates[12 + i]),
    });
    let eccentricity = I {
        lo: f(certificates[11]),
        hi: f(certificates[15]),
    };
    let mut faces = Vec::new();
    let mut patch_rows = Vec::new();
    let mut address = certificates[6];
    for (owner, patch) in case.scene.patches().iter().enumerate() {
        let mesh = patch.optical_mesh();
        let first = faces.len();
        for indices in mesh.triangles {
            faces.push(Face {
                p: indices.map(|i| mesh.vertices[i as usize]),
                material: 0,
                owner: owner as u32,
                address,
            });
            address += 6;
        }
        patch_rows.push(json!({"owner":owner,"first_face":first,"faces":faces.len()-first}));
    }
    let terrain_count = faces.len();
    let mut marker_rows = Vec::new();
    for (owner, marker) in case.markers.iter().enumerate() {
        let sign = marker_sign_frame(marker, observer).unwrap();
        let w = marker.width_m;
        let h = w * 9. / 25.;
        let a = sign.center - sign.right * (w * 0.5) - sign.up * (h * 0.5);
        let b = a + sign.right * w;
        let c = b + sign.up * h;
        let d = a + sign.up * h;
        let mut corners = marker.corners().to_vec();
        corners.extend([a, b, c, d]);
        let first = faces.len();
        for (j, indices) in BOX.into_iter().chain([[8, 9, 10], [8, 10, 11]]).enumerate() {
            faces.push(Face {
                p: indices.map(|i| corners[i]),
                material: if j < 12 { 1 } else { 2 },
                owner: owner as u32,
                address,
            });
            address += 6;
        }
        marker_rows.push(json!({"owner":owner,"first_face":first,"faces":14}));
    }
    assert_eq!(address as usize * 4, certificates.len());
    for face in &faces {
        prove_face_identity(face, frame, &certificates);
    }
    // Exact canonical ECEF vertices, not the rounded GPU mesh, let the
    // independent oracle inspect every possible earlier source face.
    let face_bytes: Vec<u8> = faces
        .iter()
        .flat_map(|face| face.p)
        .flat_map(xyz)
        .flat_map(f64::to_le_bytes)
        .collect();
    let nearest = (0..terrain_count)
        .filter(|&i| {
            (faces[i].p[1] - faces[i].p[0])
                .cross(faces[i].p[2] - faces[i].p[0])
                .length()
                > 1e-6
        })
        .min_by(|&a, &b| {
            (centroid(&faces[a]) - observer)
                .length()
                .total_cmp(&(centroid(&faces[b]) - observer).length())
        })
        .unwrap();
    let marker0 = terrain_count;
    // Pick an upright prism side whose outward normal faces the observer;
    // the short strict-centroid test never deliberately touches a side edge.
    let prism = (marker0 + 4..marker0 + 12)
        .max_by(|&a, &b| {
            normal(&faces[a])
                .dot(observer - centroid(&faces[a]))
                .total_cmp(&normal(&faces[b]).dot(observer - centroid(&faces[b])))
        })
        .unwrap();
    let sign = marker0 + 12;
    let sign_frame = marker_sign_frame(&case.markers[0], observer).unwrap();
    let n = normal(&faces[prism]);
    let marker = case.markers[0];
    let through = 2. * (marker.width_m + marker.depth_m);
    let tests = [
        Test {
            name: "terrain-interior",
            face: nearest,
            point: centroid(&faces[nearest]),
            direction: normal(&faces[nearest]),
            before: 0.25,
            after: 0.25,
            normal_line: true,
            padding: 0.,
            expectation: "Transverse strict interior terrain contact; canonical oracle decides the first source face.",
        },
        Test {
            name: "prism-interior",
            face: prism,
            point: centroid(&faces[prism]),
            direction: n,
            before: 0.25,
            after: 0.25,
            normal_line: false,
            padding: 0.,
            expectation: "Transverse strict interior prism contact.",
        },
        Test {
            name: "sign-interior",
            face: sign,
            point: centroid(&faces[sign]),
            direction: sign_frame.normal,
            before: 0.25,
            after: 0.25,
            normal_line: false,
            padding: 0.,
            expectation: "Transverse strict interior sign contact, above the prism roof.",
        },
        Test {
            name: "sign-exterior-miss",
            face: sign,
            point: sign_frame.center + sign_frame.right * (marker.width_m * 0.6),
            direction: sign_frame.normal,
            before: 0.25,
            after: 0.25,
            normal_line: false,
            padding: 0.,
            expectation: "Outside the true plate edge; all-scene oracle must exclude any other source contact.",
        },
        Test {
            name: "sign-shared-edge-tube",
            face: sign,
            point: (faces[sign].p[0] + faces[sign].p[2]) * 0.5,
            direction: sign_frame.normal,
            before: 0.25,
            after: 0.25,
            normal_line: false,
            padding: 0.0001,
            expectation: "Declared 0.1mm position tube covers the shared sign diagonal; preserve ownership uncertainty. This is synthetic input uncertainty, not ODE error.",
        },
        Test {
            name: "prism-nearer-blocker",
            face: prism,
            point: centroid(&faces[prism]),
            direction: n,
            before: 0.25,
            after: through,
            normal_line: false,
            padding: 0.,
            expectation: "Long traversal contains entry before later prism face; nearest contact must win only if the complete source ordering is strict.",
        },
    ];
    let mut curve_words = Vec::new();
    let mut piece_words = Vec::new();
    let mut rows = Vec::new();
    let axis_center = [0., f32::from_bits(field[0]), -f32::from_bits(field[1])];
    let axis_norm = axis_center.iter().map(|v| v * v).sum::<f32>().sqrt();
    let gpu_axis = axis_center.map(|v| v / axis_norm);
    for (id, test) in tests.into_iter().enumerate() {
        let seed = anchor::AnchorPacket::from_vertex(eye, test.point).unwrap();
        let before =
            anchor::AnchorPacket::from_vertex(eye, test.point + test.direction * test.before)
                .unwrap();
        let after =
            anchor::AnchorPacket::from_vertex(eye, test.point - test.direction * test.after)
                .unwrap();
        let (start, delta) = if test.normal_line {
            let a = seed.canonical_chart;
            (
                [test.before as f32, a[0] as f32, a[1] as f32],
                [-(test.before + test.after) as f32, 0., 0.],
            )
        } else {
            (
                [
                    (before.canonical_height - seed.canonical_height) as f32,
                    before.canonical_chart[0] as f32,
                    before.canonical_chart[1] as f32,
                ],
                [
                    (after.canonical_height - before.canonical_height) as f32,
                    (after.canonical_chart[0] - before.canonical_chart[0]) as f32,
                    (after.canonical_chart[1] - before.canonical_chart[1]) as f32,
                ],
            )
        };
        let h = seed.canonical_height;
        let hi = h as f32;
        let lo = (h - f64::from(hi)) as f32;
        let residual = map_packet::residual_for_split(h, [hi, lo]).unwrap();
        let curve = Curve {
            start: start.map(f64::from),
            r: [delta.map(f64::from), [0.; 3], [0.; 3], [0.; 3]],
            anchor: I::point(h),
            eye: I::point(eye.height_m),
            axis,
            eccentricity,
        };
        let begin = curve.position_jet(I::point(0.)).unwrap().map(|j| j.0[0]);
        let end = curve.position_jet(I::point(1.)).unwrap().map(|j| j.0[0]);
        let chord = vec3(std::array::from_fn(|i| {
            midpoint([end[i].lo, end[i].hi]) - midpoint([begin[i].lo, begin[i].hi])
        }));
        let path = chord.length() as f32;
        let direction = xyz(chord.normalized().unwrap()).map(|v| v as f32);
        let mut record = [[0u32; 4]; 16];
        put(&mut record, 0, start);
        record[0][3] = delta[0].to_bits();
        put(&mut record, 1, direction);
        record[1][3] = path.to_bits();
        record[2] = [hi.to_bits(), lo.to_bits(), 0, 0];
        put(&mut record, 3, delta);
        record[12] = [test.padding.to_bits(), tolerance.to_bits(), 0, 1];
        put(&mut record, 13, gpu_axis);
        record[14] = [
            field[2],
            field[3],
            residual[0].to_bits(),
            residual[1].to_bits(),
        ];
        let face = &faces[test.face];
        record[15] = [face.address, face.material, face.owner, id as u32];
        let first = piece_words.len() / 80;
        let mut accepted = None;
        let remaining = I::point(f64::from(tolerance)) - I::point(f64::from(test.padding));
        let fit_limit = (I::point(remaining.lo) * I::point(0.25)).lo;
        assert!(remaining.valid() && remaining.lo > 0. && fit_limit > 0.);
        for count in [1, 2, 4, 8, 16, 32, 64] {
            let mut pieces = Vec::new();
            let mut valid = true;
            for piece in 0..count {
                let range = I {
                    lo: piece as f64 / count as f64,
                    hi: (piece + 1) as f64 / count as f64,
                };
                let c =
                    cartesian::certify(curve, range).and_then(|c| cartesian::pack_certificate(&c));
                if let Some(c) = c {
                    if c.position_radius <= fit_limit {
                        pieces.push((range, c));
                    } else {
                        valid = false;
                        break;
                    }
                } else {
                    valid = false;
                    break;
                }
            }
            if valid {
                accepted = Some(pieces);
                break;
            }
        }
        let pieces =
            accepted.expect("Manufactured curve must not silently omit an uncertified piece");
        for (piece, (range, packed)) in pieces.iter().enumerate() {
            cartesian::append_piece(
                &mut piece_words,
                packed,
                *range,
                id,
                piece,
                pieces.len(),
                false,
            );
        }
        curve_words.extend(record.into_iter().flatten());
        rows.push(json!({"case":id,"name":test.name,"scope":test.expectation,"source_face":test.face,"source_certificate_vec4":face.address,"source_material":face.material,"source_owner":face.owner,"source_vertices_ecef_m":face.p.map(xyz),"seed_ecef_m":xyz(test.point),"approximate_inverse_is_only_a_seed":true,"canonical_anchor_height_m":h,"canonical_anchor_height_f64_bits":h.to_bits(),"anchor_words":[hi.to_bits(),lo.to_bits()],"anchor_residual_words":residual.map(f32::to_bits),"start_words":start.map(f32::to_bits),"delta_words":delta.map(f32::to_bits),"declared_position_tube_m":test.padding,"tolerance_m":tolerance,"first_piece":first,"pieces":pieces.len(),"affine_path_scale_m":path,"constant_direction_words":direction.map(f32::to_bits)}));
    }
    fs::write(
        format!("{prefix}-curves.bin"),
        curve_words
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(
        format!("{prefix}-pieces.bin"),
        piece_words
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(format!("{prefix}-faces.bin"), face_bytes).unwrap();
    let raw_basis = EnuBasis::at(eye);
    fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&json!({
        "schema":"ancientearth.manufactured-canonical-contact.v1","scope":"Local manufactured geometric curves against unchanged actual canonical source faces. Not accepted ODE trials, field integration, camera rays or production behavior. The independent oracle must solve the actual stored-f32 canonical curve and retain all earlier faces; source-centroid seeding is not a correctness result. Path is an explicitly affine bookkeeping coordinate; direction is explicitly constant, not asserted to be the tangent of the manufactured curve.",
        "source_scene":SCENE,"fresh_canonical_face_certificates_all_bytes_equal":true,"curve_record_words":64,"piece_record_words":80,"curve_count":rows.len(),"piece_count":piece_words.len()/80,"canonical_eye_geodetic":[eye.lat_deg,eye.lon_deg,eye.height_m],"canonical_eye_ecef_m":xyz(observer),"ideal_frame_columns":frame.columns,"axis_intervals":axis.map(|v|[v.lo,v.hi]),"eccentricity_interval":[eccentricity.lo,eccentricity.hi],
        "exact_f64_inputs":{"eye_height":eye.height_m.to_bits(),"a":WGS84_A_M.to_bits(),"eccentricity_squared":WGS84_E2.to_bits(),"east_x":raw_basis.east.x.to_bits(),"east_y":raw_basis.east.y.to_bits(),"up_z":raw_basis.up.z.to_bits(),"north_z":raw_basis.north.z.to_bits()},
        "face_binary":"All canonical source faces in retained terrain order then fourteen prism/sign faces per marker; 9 little-endian binary64 ECEF coordinates per face. No quantized GPU vertex is substituted.","face_count":faces.len(),"terrain_faces":terrain_count,"patches":patch_rows,"markers":marker_rows,
        "curve_layout":{"0":"start q.xyz, redundant deltaH","1":"constant u.xyz, affine path scale","2":"anchor hi/lo, unused source n/g zero","3..6":"physical q2..q5.xyz","7..10":"u2..u5.xyz zero","11":"optical coefficients zero","12":"declared position tube,tolerance,unused,valid1","13":"rounded evaluator axis.xyz","14":"eye H hi/lo, canonical anchor residual lower/upper","15":"u32 intended certificate address/material/owner/case"},"cases":rows
    })).unwrap()).unwrap();
}
