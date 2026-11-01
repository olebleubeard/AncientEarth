//! Unregistered canonical normal-coordinate candidate packet.
//!
//! Host contract: bindings 0..3 retain PrimarySettings, canonical geometry,
//! current frames and the lossless current-token terminal output. Binding 4
//! is this FieldSettings; binding 5 is read-only scene certificate metadata.
//! Camera construction and the 16+48*pixels terminal layout stay unchanged.
//! No candidate shader or pipeline is registered by this module.
use bevy::{prelude::Vec4, render::render_resource::ShaderType};
use earth_math::{
    atmosphere::{AtmosphereLayer, AtmosphereProfile},
    Geodetic, MIN_HEIGHT_M,
};

pub(super) const FIELD_VEC4_COUNT: usize = 261;
pub(super) const FIELD_BYTES: u64 = FIELD_VEC4_COUNT as u64 * 16;

/// Byte-for-byte shape of the independently tested NcSettings. Every field
/// starts on a vec4 boundary. Canonical f64 inputs are required; reconstructing
/// these values from the old f32 material/profile packet loses source gaps.
#[derive(Clone, Debug, ShaderType)]
pub(super) struct FieldSettings {
    pub pose: Vec4,
    pub initial: Vec4,
    pub limits: Vec4,
    pub initial_lower: Vec4,
    pub initial_upper: Vec4,
    pub layers: [Vec4; 128],
    pub gaps: [Vec4; 128],
}

fn split(value: f64) -> [f32; 2] {
    let high = value as f32;
    [high, (value - f64::from(high)) as f32]
}

// Same outward host envelope as the independently checked clear-ray fixture.
// It bounds the difference of the actual encoded split source heights.
fn encoded_gap(a: [f32; 2], b: [f32; 2]) -> Vec4 {
    let av = f64::from(a[0]) + f64::from(a[1]);
    let bv = f64::from(b[0]) + f64::from(b[1]);
    let lower = (av.next_down() - bv.next_up()).next_down();
    let upper = (av.next_up() - bv.next_down()).next_up();
    let pair = split(av - bv);
    let encoded = f64::from(pair[0]) + f64::from(pair[1]);
    let error = (encoded.next_up() - lower)
        .max(upper - encoded.next_down())
        .max(0.0)
        .next_up();
    Vec4::new(pair[0], pair[1], (error as f32).next_up(), 0.0)
}

impl FieldSettings {
    /// Initial diagnostic eligibility is strictly inside the atmospheric
    /// source domain. Exact-node starts and vacuum entry require their own
    /// direction-dependent initialization before this candidate is complete.
    /// An error leaves the existing complete solver available to the host.
    pub(super) fn from_canonical(
        eye: Geodetic,
        profile: &AtmosphereProfile,
    ) -> Result<Self, String> {
        Self::pack(eye, profile.base_height_m(), profile.layers())
    }

    /// Explicit canonical parts for retained-source diagnostics. Validate the
    /// same profile constraints before using the common production packer.
    pub(super) fn from_canonical_parts(
        eye: Geodetic,
        base_height_m: f64,
        nodes: &[AtmosphereLayer],
    ) -> Result<Self, String> {
        let profile = AtmosphereProfile::from_layers(nodes.to_vec())
            .map_err(|_| "Invalid canonical source profile".to_string())?;
        Self::pack(eye, base_height_m, profile.layers())
    }

    fn pack(eye: Geodetic, base_height_m: f64, nodes: &[AtmosphereLayer]) -> Result<Self, String> {
        if !eye.lat_deg.is_finite()
            || !eye.lon_deg.is_finite()
            || !eye.height_m.is_finite()
            || !(-90.0..=90.0).contains(&eye.lat_deg)
            || eye.height_m < MIN_HEIGHT_M
        {
            return Err("Normal-coordinate candidate eye is outside its finite domain".into());
        }
        if !(2..=128).contains(&nodes.len()) {
            return Err("Normal-coordinate candidate needs 2..128 source nodes".into());
        }
        let heights: Vec<_> = nodes
            .iter()
            .map(|node| base_height_m + node.height_m)
            .collect();
        if heights.iter().any(|h| !h.is_finite() || h.abs() > 1.0e8)
            || heights.windows(2).any(|pair| pair[0] >= pair[1])
            || heights[0] <= MIN_HEIGHT_M
        {
            return Err("Normal-coordinate candidate source heights are unsupported".into());
        }
        let encoded: Vec<_> = heights.iter().copied().map(split).collect();
        let encoded_values: Vec<_> = encoded
            .iter()
            .map(|pair| f64::from(pair[0]) + f64::from(pair[1]))
            .collect();
        let eye_pair = split(eye.height_m);
        let encoded_eye = f64::from(eye_pair[0]) + f64::from(eye_pair[1]);
        let owner = heights.partition_point(|h| *h < eye.height_m);
        if owner >= nodes.len()
            || heights.contains(&eye.height_m)
            || encoded_values.contains(&encoded_eye)
            || encoded_values.partition_point(|h| *h < encoded_eye) != owner
            || encoded_values.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(
                "Normal-coordinate candidate needs a strict represented initial source interval"
                    .into(),
            );
        }
        let slopes: Vec<_> = nodes
            .windows(2)
            .zip(heights.windows(2))
            .map(|(n, h)| (n[1].refractivity - n[0].refractivity) / (h[1] - h[0]))
            .collect();
        if slopes.iter().any(|value| !value.is_finite()) {
            return Err("Normal-coordinate candidate source slope is nonfinite".into());
        }
        let (refractivity, slope) = if owner == 0 {
            (nodes[0].refractivity, 0.0)
        } else {
            let slope = slopes[owner - 1];
            (
                nodes[owner - 1].refractivity + slope * (eye.height_m - heights[owner - 1]),
                slope,
            )
        };
        let (sin, cos) = eye.lat_deg.to_radians().sin_cos();
        let mut layers = [Vec4::ZERO; 128];
        let mut gaps = [Vec4::ZERO; 128];
        for (i, node) in nodes.iter().enumerate() {
            layers[i] = Vec4::new(
                encoded[i][0],
                encoded[i][1],
                node.refractivity as f32,
                slopes.get(i).copied().unwrap_or(0.0) as f32,
            );
            if i + 1 < nodes.len() {
                gaps[i] = encoded_gap(encoded[i + 1], encoded[i]);
            }
        }
        // This otherwise unused top-node gap is the below-base domain width.
        gaps[nodes.len() - 1] = encoded_gap(encoded[0], [MIN_HEIGHT_M as f32, 0.0]);
        Ok(Self {
            pose: Vec4::new(sin as f32, cos as f32, eye_pair[0], eye_pair[1]),
            initial: Vec4::new(
                refractivity as f32,
                slope as f32,
                owner as f32,
                nodes.len() as f32,
            ),
            limits: Vec4::new(MIN_HEIGHT_M as f32, 100000.0, 2000000.0, 0.002),
            initial_lower: encoded_gap(
                eye_pair,
                if owner == 0 {
                    [MIN_HEIGHT_M as f32, 0.0]
                } else {
                    encoded[owner - 1]
                },
            ),
            initial_upper: encoded_gap(eye_pair, encoded[owner]),
            layers,
            gaps,
        })
    }
}
