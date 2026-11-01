//! Original MIT. Separate initialization-branch tests for the unchanged trace.
//! Synthetic vacuum packets exercise scheduling, not retained-scene accuracy.
use super::wavefront::{self, WaveFixture, WaveStep};
use earth_math::{Geodetic, Observer, atmosphere::gpu::GpuEllipsoid};
use serde_json::json;

pub const CASES: [&str; 5] = [
    "actual",
    "readiness",
    "fisheye",
    "vacuum-outward",
    "vacuum-inward",
];

fn record(bytes: &mut [u8], index: usize, value: [f32; 4]) {
    for (component, value) in value.into_iter().enumerate() {
        bytes[index * 16 + component * 4..index * 16 + component * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
}
fn scalar(bytes: &mut [u8], index: usize, component: usize, value: f32) {
    bytes[index * 16 + component * 4..index * 16 + component * 4 + 4]
        .copy_from_slice(&value.to_le_bytes());
}

pub fn build(case: &str, slice_attempts: u32, reference: bool) -> WaveFixture {
    assert!(CASES.contains(&case));
    assert!([1, 8, 32, 128].contains(&slice_attempts));
    let mut fixture = if reference {
        let scene = wavefront::build_counted_reference(true);
        WaveFixture {
            state_bytes: u64::from(scene.record_count) * 48,
            progress_bytes: 16,
            steps: vec![WaveStep {
                entry: "normal_scene",
                dispatch: scene.dispatch,
                repetitions: 1,
            }],
            scene,
        }
    } else {
        wavefront::build(true, slice_attempts)
    };
    let scene = &mut fixture.scene;
    assert_eq!(scene.primary_uniform.len(), 2240);
    let before = scene.primary_uniform.clone();
    let expected;
    match case {
        "actual" => {
            expected = "Retained actual-scene terminals; compare every word and attempt count against counted reference.";
        }
        "readiness" => {
            scalar(&mut scene.primary_uniform, 9, 3, 0.0);
            expected =
                "Every unmasked ray returns unresolved before an outer attempt; all counts zero.";
        }
        "fisheye" => {
            scalar(&mut scene.primary_uniform, 8, 2, 1.0);
            expected = "Exact original radius>1 aperture branch produces status3/count0; interior rays execute unchanged original trace. Both populations must be present.";
        }
        "vacuum-outward" | "vacuum-inward" => {
            let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 2000.0).unwrap());
            observer.feet.height_m = 2000.0 - observer.eye_height_m;
            assert_eq!(observer.eye_geodetic().height_m, 2000.0);
            let ellipsoid = GpuEllipsoid::at(observer);
            for (index, value) in ellipsoid.rows.into_iter().enumerate() {
                record(&mut scene.primary_uniform, index, value);
            }
            record(&mut scene.primary_uniform, 3, ellipsoid.origin);
            record(&mut scene.primary_uniform, 4, ellipsoid.center);
            // E/up/-N is the original local coordinate convention. Each
            // cardinal camera is orthonormal; the original lens expression is
            // retained. A one-degree half-angle keeps every selected ray on
            // the requested outward/inward side of the atmosphere.
            let sign = if case == "vacuum-outward" { 1.0 } else { -1.0 };
            record(&mut scene.primary_uniform, 5, [1.0, 0.0, 0.0, 0.0]);
            record(&mut scene.primary_uniform, 6, [0.0, 0.0, -sign, 0.0]);
            record(&mut scene.primary_uniform, 7, [0.0, sign, 0.0, 0.0]);
            record(
                &mut scene.primary_uniform,
                8,
                [1920.0 / 1080.0, std::f32::consts::PI / 180.0, 0.0, 0.0],
            );
            // Zero refractivity is a legitimate vacuum control. Keep the
            // retained conservative curvature upper bound, error allocation,
            // path/iteration limits and lower operational domain unchanged.
            scalar(&mut scene.primary_uniform, 9, 0, 2.0);
            scalar(&mut scene.primary_uniform, 9, 1, 1000.0);
            scalar(&mut scene.primary_uniform, 9, 3, 1.0);
            scalar(&mut scene.primary_uniform, 10, 0, 0.0);
            scalar(&mut scene.primary_uniform, 10, 1, 0.0);
            for index in 12..140 {
                record(&mut scene.primary_uniform, index, [0.0; 4]);
            }
            record(&mut scene.primary_uniform, 13, [1000.0, 0.0, 0.0, 0.0]);
            // No imported scene is claimed at this synthetic eye. The normal
            // empty-scene roots disable both BVHs; the actual h0 fallback and
            // vacuum entry code still run unmodified.
            scene.frames[..16].fill(0);
            expected = if case == "vacuum-outward" {
                "Every ray misses atmosphere outward: status0/path0/attempt0."
            } else {
                "Every ray enters the 1000m atmosphere from 2000m then meets uncovered h0: status1, positive entry/path, nonzero attempts. Compare actual words to counted reference; no analytic-error acceptance inferred."
            };
        }
        _ => unreachable!(),
    }
    let changed_words: Vec<_> = before
        .chunks_exact(4)
        .zip(scene.primary_uniform.chunks_exact(4))
        .enumerate()
        .filter_map(|(index, (a, b))| (a != b).then_some(index))
        .collect();
    scene.cases["wavefront_branch_matrix"] = json!({
        "case":case,"source":"scripts/optical_wavefront_branch_fixture.rs",
        "scope":"Selected-record scheduling and initialization branches. Same source expressions and immutable input bytes per reference/continuation pair. Synthetic vacuum cases are deliberately separate from retained actual-scene numerical acceptance.",
        "expected":expected,"primary_changed_word_indices":changed_words,
        "synthetic_vacuum":case.starts_with("vacuum-"),
        "geometry_buffer":"Retained original; synthetic vacuum cases have both ordinary scene roots zero, so it is unreachable.",
        "exhaustion_coverage":"No altered iteration cutoff. A real numerical4096-attempt case is not asserted by this fixture; independent scheduling-state tests cover the exact exhausted-loop semantics until a real case is retained.",
        "record_identity":"State metadata.z is selected output-record ID; sample_pixels supplies native pixel identity."
    });
    fixture
}
