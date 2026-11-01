//! Exercise the same manual download lifecycle without a GPU window.
//! Usage: cargo run --no-default-features --example topography_download --
//!        <cache-folder> <latitude> <longitude> <radius-miles> [--download|--load]
use ancient_earth::{navigation::Explorer, surface::WorldSurface, topography::TopographyState};
use earth_math::{Geodetic, UnitSystem, topography::radius_from_display};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        return Err(
            "Provide cache-folder latitude longitude radius-miles [--download|--load]".into(),
        );
    }
    let mut explorer = Explorer::default();
    let mut surface = WorldSurface::bundled()?;
    let mut state = TopographyState::default();
    state.cache = Some(PathBuf::from(&args[1]));
    state.radius_m = radius_from_display(args[4].parse()?, UnitSystem::Imperial)?;
    let center = Geodetic::new(args[2].parse()?, args[3].parse()?, 0.0)?;
    if args.iter().any(|a| a == "--load") {
        state.load_saved(&surface);
    } else {
        state.request(center, &surface, args.iter().any(|a| a == "--download"));
    }
    let start = Instant::now();
    let mut previous = String::new();
    let mut last_print = Instant::now();
    while state.loading() {
        state.tick(&mut surface, &mut explorer);
        if state.progress.status != previous && last_print.elapsed() >= Duration::from_secs(1) {
            println!("{}", state.progress.status);
            previous = state.progress.status.clone();
            last_print = Instant::now();
        }
        if start.elapsed() > Duration::from_secs(900) {
            state.cancel();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("{}", state.progress.status);
    if let Some(error) = &state.last_error {
        return Err(error.clone().into());
    }
    if let Some(description) = &state.imported_description {
        println!("{description}");
    }
    if surface.has_terrain() {
        assert!(surface.elevations_enabled);
        state.set_elevations(false, &mut surface, &mut explorer);
        assert!(surface.active(true).is_boundary_only());
        state.set_elevations(true, &mut surface, &mut explorer);
        assert!(!surface.active(true).is_boundary_only());
        println!("Cached terrain and smooth-surface toggles passed.");
    }
    Ok(())
}
