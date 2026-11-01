//! Unused preparation-time sufficient height ceiling for canonical faces.
//! No ray, surface, source height, or runtime packet is changed by this module.
use earth_math::{Vec3d, WGS84_A_M, WGS84_B_M};

/// A ceiling includes h=0 even for an empty or entirely negative-height scene.
#[derive(Clone, Copy, Debug)]
pub struct HeightCeiling {
    pub upper_m: f64,
    pub vertices: usize,
    pub maximizing_vertex: Option<Vec3d>,
}

// Preparation is binary64; the extra adjacent representable value encloses
// nearest-rounded basic operations and sqrt. Reject overflow rather than
// returning a usable certificate for a nonfinite source or intermediate.
fn add_up(a: f64, b: f64) -> f64 { (a + b).next_up() }
fn mul_up(a: f64, b: f64) -> f64 { (a * b).next_up() }
fn div_up(a: f64, b: f64) -> f64 { (a / b).next_up() }

/// Outward upper bound for the signed distance to the canonical ellipsoid.
/// For S=P^T Q P>1, P/sqrt(S) is a point on its surface, so nearest distance is
/// at most |P|*(S-1)/(sqrt(S)*(sqrt(S)+1)). Inside, signed distance is <=0.
/// This is a bound, not a substitute inverse geodetic coordinate conversion.
pub fn vertex_height_upper(p: Vec3d) -> Option<f64> {
    if !p.is_finite() { return None; }
    let components = [p.x.abs(), p.y.abs(), p.z.abs()];
    let axes = [WGS84_A_M, WGS84_A_M, WGS84_B_M];
    let mut squared_norm = 0.0;
    let mut s_upper = 0.0;
    for (value, axis) in components.into_iter().zip(axes) {
        squared_norm = add_up(squared_norm, mul_up(value, value));
        let scaled = div_up(value, axis);
        s_upper = add_up(s_upper, mul_up(scaled, scaled));
    }
    if !s_upper.is_finite() || !squared_norm.is_finite() { return None; }
    if s_upper <= 1.0 { return Some(0.0); }
    let norm_upper = squared_norm.sqrt().next_up();
    // g(S)=1-1/sqrt(S) is increasing for positive S. Evaluate g(s_upper)
    // outward in its rationalized form to avoid cancellation close to h=0.
    let sqrt_lower = s_upper.sqrt().next_down();
    let numerator = (s_upper - 1.0).next_up();
    let denominator = (sqrt_lower * (sqrt_lower + 1.0).next_down()).next_down();
    if !(denominator > 0.0) { return None; }
    let upper = mul_up(norm_upper, div_up(numerator, denominator));
    (upper.is_finite() && upper >= 0.0).then_some(upper)
}

impl Default for HeightCeiling {
    fn default() -> Self {
        Self { upper_m: 0.0, vertices: 0, maximizing_vertex: None }
    }
}

impl HeightCeiling {
    pub fn include(&mut self, point: Vec3d) -> Option<()> {
        let upper = vertex_height_upper(point)?;
        self.vertices = self.vertices.checked_add(1)?;
        if upper > self.upper_m {
            self.upper_m = upper;
            self.maximizing_vertex = Some(point);
        }
        Some(())
    }

    /// Signed distance is 1-Lipschitz, so a proven position-error bound adds
    /// directly to the ceiling. This function does not invent that bound.
    pub fn packed_with_position_allowance(self, allowance_m: f64) -> Option<f32> {
        if !allowance_m.is_finite() || allowance_m < 0.0 { return None; }
        if self.upper_m == 0.0 && allowance_m == 0.0 { return Some(0.0); }
        let upper = add_up(self.upper_m, allowance_m);
        // GPU arithmetic may flush subnormals. A positive certificate must
        // therefore be at least the smallest normal f32, never a tiny value
        // that could silently turn into an inward zero bound.
        let packed = (upper as f32).next_up().max(f32::MIN_POSITIVE);
        (packed.is_finite() && f64::from(packed) >= upper).then_some(packed)
    }
}

#[cfg(test)]
#[path = "optical_scene_ceiling_review.rs"]
mod review;
