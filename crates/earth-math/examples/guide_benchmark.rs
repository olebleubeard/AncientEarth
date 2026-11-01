//! Repeatable guide wall-clock microbenchmark; excludes GPU/app scheduling.
use earth_math::{atmosphere::weather, guides::*, *};
use std::time::Instant;

fn main() {
    let scene = TerrainScene::boundary_only(vec![]).unwrap();
    let viewport = GuideViewport::new([1920.0, 1080.0], [1.0, 1.0]).unwrap();
    let weather = weather::decode_gfs(
        include_bytes!("../../../assets/weather/gfs-reference.grib2"),
        &weather::reference_request(),
    )
    .unwrap()
    .profile;
    for (name, mut observer, lens, refracted, repeats) in [
        (
            "geometric-coast",
            weather::reference_observer(),
            Lens::default(),
            false,
            20,
        ),
        (
            "geometric-telescope",
            weather::reference_observer(),
            weather::reference_lens(),
            false,
            20,
        ),
        (
            "geometric-fisheye",
            weather::reference_observer(),
            Lens::new(Projection::EquidistantFisheye, 140.0),
            false,
            20,
        ),
        (
            "weather-telescope",
            weather::reference_observer(),
            weather::reference_lens(),
            true,
            1,
        ),
        (
            "weather-coast",
            weather::reference_observer(),
            Lens::default(),
            true,
            1,
        ),
    ] {
        observer.roll_deg = 0.0;
        let profile = refracted.then_some(&weather);
        let start = Instant::now();
        let mut result = None;
        for _ in 0..repeats {
            result = HorizonProfile::measure_with_atmosphere(
                &scene,
                observer.frame(),
                lens,
                viewport,
                profile,
            );
            std::hint::black_box(&result);
        }
        let measure_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(repeats);
        let start = Instant::now();
        let ruler = result
            .as_ref()
            .and_then(|p| p.ruler_with_atmosphere(&scene, 0.8, profile));
        println!(
            "{name}: L={measure_ms:.6}ms V={:.6}ms apex={:?} ruler={}",
            start.elapsed().as_secs_f64() * 1000.0,
            result.map(|p| p.apex.pixel),
            ruler.is_some()
        );
    }
}
