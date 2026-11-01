//! Explicit live transport smoke check, without launching the GPU renderer.
//! Run with `cargo run --example weather_download --no-default-features`.
use ancient_earth::{surface::WorldSurface, weather::WeatherState};
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let surface = WorldSurface::bundled().map_err(|error| error.to_string())?;
    let observer = earth_math::boundary::default_observer();
    let mut weather = WeatherState::default();
    weather.set_enabled(true);
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut previous = String::new();
    loop {
        weather.tick(observer.feet);
        weather.prepare_scene(surface.shared(true), observer.feet);
        if previous != weather.status {
            println!("{}", weather.status);
            previous.clone_from(&weather.status);
        }
        if !weather.loading()
            && let Some(profile) = weather.registered_profile()
        {
            let metadata = profile
                .metadata
                .as_ref()
                .ok_or("Missing provider metadata")?;
            println!(
                "Model: {}\nValid: {}\nCycle: {}\nSource: {}",
                metadata.model,
                metadata.valid_time_label(),
                metadata.cycle_time_label(),
                metadata.source_url
            );
            if let Some(url) = &metadata.secondary_source_url {
                println!("Secondary source: {url}");
            }
            if let Some(reason) = &weather.last_error {
                println!("Regional fallback: {reason}");
            }
            println!("Cache: {}", weather.from_cache);
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(weather
                .last_error
                .take()
                .unwrap_or_else(|| "Weather smoke check timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
