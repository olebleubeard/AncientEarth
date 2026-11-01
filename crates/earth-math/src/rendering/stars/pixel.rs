//! Point-star display quantization and finite inverse atmospheric shooting.
use crate::{
    CameraFrame, EnuBasis, Lens, Observer, Ray, TerrainScene, Vec3d, atmosphere::AtmosphereProfile,
};
/// Exactly one integer framebuffer pixel owns a resolved star centre. The right
/// and bottom viewport boundaries are excluded; no device-scale factor enters.
pub fn owner(lens: Lens, frame: CameraFrame, direction: Vec3d, size: [u32; 2]) -> Option<[u32; 2]> {
    if size.contains(&0) {
        return None;
    }
    let ndc = lens.project(frame, direction, f64::from(size[0]) / f64::from(size[1]))?;
    let x = ((ndc[0] + 1.0) * 0.5 * f64::from(size[0])).floor();
    let y = ((1.0 - ndc[1]) * 0.5 * f64::from(size[1])).floor();
    (x >= 0.0 && y >= 0.0 && x < f64::from(size[0]) && y < f64::from(size[1]))
        .then_some([x as u32, y as u32])
}
/// A finite inverse search for one vacuum stellar direction. This intentionally
/// does not claim to enumerate multiple images in a ducting atmosphere. Failure
/// or an obstructed/unresolved trace returns None, never a cosmetic displacement.
/// `tolerance` is an exit-direction chord error, approximately radians nearby.
pub fn apparent_direction(
    profile: &AtmosphereProfile,
    scene: &TerrainScene,
    observer: Observer,
    target: Vec3d,
    tolerance: f64,
) -> Option<Vec3d> {
    let target = target.normalized()?;
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return None;
    }
    let vertical = EnuBasis::at(observer.eye_geodetic()).up;
    let horizontal = target - vertical * target.dot(vertical);
    let mut guess = if target.dot(vertical) < 0.05 {
        (horizontal.normalized()? + vertical * 0.05).normalized()?
    } else {
        target
    };
    let mut last_sky = guess;
    for _ in 0..16 {
        let path = profile
            .trace(
                Ray {
                    origin: observer.eye_position(),
                    direction: guess,
                },
                scene,
            )
            .ok()?;
        if path.surface.is_some() {
            guess = (guess + last_sky).normalized()?;
            continue;
        }
        let error = target - path.exit_ray.direction;
        if error.length() <= tolerance {
            return Some(guess);
        }
        last_sky = guess;
        // Fixed-point shooting is locally convergent for modest refractive
        // angular gradients. Strong profiles can fail this finite search.
        guess = (guess + error).normalized()?;
    }
    None
}
