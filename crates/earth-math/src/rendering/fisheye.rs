//! Perspective source coverage for the equidistant compositor.
use crate::{Lens, Projection};

/// Forward raster camera and matching compositor divisor. These are f32 values
/// deliberately computed in the same order as Bevy's perspective projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForwardProjection {
    pub fov_radians: f32,
    pub tan_half_fov: f32,
    pub projection_scale: f32,
    pub adaptive: bool,
}

impl ForwardProjection {
    /// Raster focal length; the existing finite terrain-detail cap still applies.
    pub fn focal_pixels(self, height: u32) -> f64 {
        f64::from(height) * f64::from(self.projection_scale) * 0.5
    }
}

/// Narrow single-face views use a matching forward perspective camera rather
/// than magnifying a tiny crop of a fixed 90-degree texture. A relative 20 ppm
/// aperture pad covers ordinary f32 projection/polynomial rounding even at the
/// minimum lens FOV. Wide views retain the original 90-degree cube faces.
pub fn forward_projection(lens: Lens) -> ForwardProjection {
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    let adaptive = lens.projection == Projection::EquidistantFisheye && !active_faces(lens)[1];
    let fov_radians = if adaptive {
        (lens.vertical_fov_deg.to_radians() * 1.000_02) as f32
    } else {
        std::f32::consts::FRAC_PI_2
    };
    // Pinned Bevy/glam perspective_infinite_reverse uses cos/sin, not 1/tan.
    let (sine, cosine) = (fov_radians * 0.5).sin_cos();
    let projection_scale = cosine / sine;
    ForwardProjection {
        fov_radians,
        tan_half_fov: 1.0 / projection_scale,
        projection_scale,
        adaptive,
    }
}

/// Active perspective faces in forward/right/back/left/up/down order.
/// The compositor's circular aperture and sanitized full FOV <= 180 degrees
/// never select the rear face: at the horizon a side component dominates.
/// Below a 45-degree half-angle, forward dominates both transverse components.
/// Keep all four side views within 0.002 rad of that threshold. WGSL permits
/// sin/cos absolute error up to 2^-11 here; the retained angular margin gives
/// a cos-minus-sin gap greater than both errors plus ordinary f32 arithmetic.
/// https://www.w3.org/TR/WGSL/#floating-point-accuracy
/// The forward source projection is chosen separately by `forward_projection`.
pub fn active_faces(lens: Lens) -> [bool; 6] {
    let lens = Lens::new(lens.projection, lens.vertical_fov_deg);
    if lens.projection != Projection::EquidistantFisheye {
        return [false; 6];
    }
    let sides = lens.vertical_fov_deg.to_radians() * 0.5 >= std::f64::consts::FRAC_PI_4 - 0.002;
    [true, sides, false, sides, sides, sides]
}
