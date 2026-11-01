//! Camera aiming only: positions, physical body sizes and optical lens are unchanged.
use super::{AstroInstant, AstronomyFrame};
use crate::{Geodetic, Observer, STATURE_M, Vec3d};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewBody {
    Earth,
    Moon,
    Sun,
}
impl ViewBody {
    pub fn label(self) -> &'static str {
        match self {
            Self::Earth => "Earth",
            Self::Moon => "Moon",
            Self::Sun => "Sun",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyAimError {
    Unavailable,
    StaleSample,
    InvalidObserver,
    InvalidDirection,
}
impl fmt::Display for BodyAimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Current astronomy is unavailable; aiming cannot be updated.",
            Self::StaleSample => {
                "Body tracking requires the current time, eye position and observing convention."
            }
            Self::InvalidObserver => "The current observer does not define a valid camera pose.",
            Self::InvalidDirection => {
                "The body center does not define a finite nonzero sight direction."
            }
        })
    }
}
impl std::error::Error for BodyAimError {}

/// A selected viewing constraint, independent of Earth's position-tracking mode.
/// Missing current astronomy retains the selection but never applies stale axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyTracking {
    target: Option<ViewBody>,
}
impl BodyTracking {
    pub fn target(self) -> Option<ViewBody> {
        self.target
    }
    pub fn cancel(&mut self) {
        self.target = None;
    }
    /// A successful single aim replaces/cancels any previous tracking selection.
    pub fn aim_once(
        &mut self,
        body: ViewBody,
        observer: &mut Observer,
        frame: Option<&AstronomyFrame>,
        instant: AstroInstant,
        follow_earth: bool,
    ) -> Result<(), BodyAimError> {
        aim(body, observer, frame, instant, follow_earth)?;
        self.cancel();
        Ok(())
    }
    /// Repeating the selected body releases tracking without changing the pose.
    /// Selecting another body first validates/aims, then commits the selection.
    pub fn toggle(
        &mut self,
        body: ViewBody,
        observer: &mut Observer,
        frame: Option<&AstronomyFrame>,
        instant: AstroInstant,
        follow_earth: bool,
    ) -> Result<(), BodyAimError> {
        if self.target == Some(body) {
            self.cancel();
            return Ok(());
        }
        aim(body, observer, frame, instant, follow_earth)?;
        self.target = Some(body);
        Ok(())
    }
    pub fn update(
        &self,
        observer: &mut Observer,
        frame: Option<&AstronomyFrame>,
        instant: AstroInstant,
        follow_earth: bool,
    ) -> Result<(), BodyAimError> {
        if let Some(body) = self.target {
            aim(body, observer, frame, instant, follow_earth)?;
        }
        Ok(())
    }
}

fn aim(
    body: ViewBody,
    observer: &mut Observer,
    astronomy: Option<&AstronomyFrame>,
    instant: AstroInstant,
    follow_earth: bool,
) -> Result<(), BodyAimError> {
    let eye_geo = observer.eye_geodetic();
    if Geodetic::new(
        observer.feet.lat_deg,
        observer.feet.lon_deg,
        observer.feet.height_m,
    )
    .is_err()
        || Geodetic::new(eye_geo.lat_deg, eye_geo.lon_deg, eye_geo.height_m).is_err()
        || !(0.1..=STATURE_M).contains(&observer.eye_height_m)
        || observer.feet.height_m > observer.maximum_feet_height_m()
        || ![
            observer.heading_deg,
            observer.pitch_deg,
            observer.roll_deg,
            observer.speed_mps,
        ]
        .into_iter()
        .all(f64::is_finite)
    {
        return Err(BodyAimError::InvalidObserver);
    }
    let eye = observer.eye_position();
    let center = match body {
        ViewBody::Earth => Vec3d::ZERO,
        ViewBody::Moon | ViewBody::Sun => {
            let frame = astronomy.ok_or(BodyAimError::Unavailable)?;
            if frame.sample_instant != instant
                || frame.sample_observer_ecef_m != eye
                || frame.sample_follow_earth != follow_earth
            {
                return Err(BodyAimError::StaleSample);
            }
            match body {
                ViewBody::Moon => frame.moon.apparent_center_ecef_m,
                ViewBody::Sun => frame.sun.apparent_center_ecef_m,
                ViewBody::Earth => unreachable!(),
            }
        }
    };
    let direction = (center - eye)
        .normalized()
        .ok_or(BodyAimError::InvalidDirection)?;
    let old = observer.frame();
    let cross = old.forward.cross(direction);
    let sine = cross.length();
    let cosine = old.forward.dot(direction).clamp(-1.0, 1.0);
    let up = if sine > 64.0 * f64::EPSILON {
        // The shortest proper rotation transports bank without local-ENU flips.
        crate::navigation::rotate_axis(old.up, cross / sine, sine.atan2(cosine))
    } else {
        // Parallel: unchanged up. Antipodal: a deterministic half-turn around
        // old up also leaves up unchanged. set_view_axes removes roundoff.
        old.up
    };
    let mut candidate = *observer;
    candidate
        .set_view_axes(direction, up)
        .map_err(|_| BodyAimError::InvalidDirection)?;
    *observer = candidate;
    Ok(())
}
