//! Presentation of the authoritative survey prisms and nonphysical tape marks.
//! No distance-dependent geometric inflation is applied to a survey prism.
use super::{LocalFrame, RenderMesh};
use crate::{
    Vec3d,
    survey::{SurveyMarker, TapePoint},
};

const FOOT_M: f64 = 0.3048;
pub const TAPE_RADIUS_M: f64 = 0.015;
pub const TAPE_ENDPOINT_RADIUS_M: f64 = 0.15;
pub const BAND_MEAN_FOOTPRINT_FEET: f64 = 1024.0;

/// Original bitmap-like digit geometry; no font or image asset is embedded.
/// Each row stores five columns, most significant bit at the left.
const SIGN_DIGITS: [[u8; 7]; 10] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
];
const SIGN_COLUMNS: usize = 25;
const SIGN_ROWS: usize = 9;
const SIGN_FACING_GUARD_M: f64 = 1e-6;

/// Display labels are one-based; the canonical row/station identifier stays
/// zero-based. Invalid IDs cannot wrap or silently produce a fifth digit.
pub fn marker_label(id: usize) -> Option<String> {
    let number = id.checked_add(1).filter(|&number| number <= 9999)?;
    Some(format!("{number:04}"))
}

/// Upright annotation frame in ECEF. It changes no canonical prism dimensions.
#[derive(Clone, Copy, Debug)]
pub struct MarkerSignFrame {
    pub center: Vec3d,
    pub right: Vec3d,
    pub up: Vec3d,
    pub normal: Vec3d,
}

fn sign_dimensions(marker: &SurveyMarker) -> Option<(f64, f64)> {
    marker_label(marker.id)?;
    (marker.width_m.is_finite() && marker.width_m > 0.0 && marker.width_m <= 1_000_000.0).then_some(
        (
            marker.width_m,
            marker.width_m * SIGN_ROWS as f64 / SIGN_COLUMNS as f64,
        ),
    )
}

/// An axial billboard: the actual eye selects only yaw around the marker's
/// fixed geodetic up. External horizontal eyes put the plate on an eye-facing
/// support plane of the prism, so its roof cannot hide the number. An eye inside
/// the horizontal footprint retains the top-center plate; overhead yaw uses
/// the marker's forward direction and never tilts the vertical axis.
pub fn marker_sign_frame(marker: &SurveyMarker, eye: Vec3d) -> Option<MarkerSignFrame> {
    let (_, height) = sign_dimensions(marker)?;
    if !eye.is_finite()
        || !marker.base_ecef_m.is_finite()
        || !marker.height_m.is_finite()
        || marker.height_m <= 0.0
        || marker.height_m > 1_000_000.0
        || !marker.depth_m.is_finite()
        || marker.depth_m <= 0.0
        || marker.depth_m > 1_000_000.0
        || [marker.up, marker.right, marker.forward]
            .into_iter()
            .any(|axis| !axis.is_finite() || (axis.length() - 1.0).abs() > 1e-10)
    {
        return None;
    }
    let up = marker.up.normalized()?;
    let right = marker.right.normalized()?;
    let forward = marker.forward.normalized()?;
    if right.dot(up).abs() > 1e-10
        || forward.dot(up).abs() > 1e-10
        || right.dot(forward).abs() > 1e-10
        || right.cross(forward).dot(up) < 1.0 - 1e-10
    {
        return None;
    }
    let mut center = marker.centerline(marker.height_m + height * 0.5);
    if !center.is_finite() {
        return None;
    }
    let delta = eye - center;
    if !delta.is_finite() {
        return None;
    }
    // Components in the horizontal marker basis avoid subtracting two large,
    // nearly equal vertical displacement vectors near the overhead case.
    let x = delta.dot(right);
    let y = delta.dot(forward);
    let horizontal = right * x + forward * y;
    if !horizontal.is_finite() {
        return None;
    }
    let mut normal = if horizontal.length() > SIGN_FACING_GUARD_M {
        horizontal.normalized()?
    } else {
        forward
    };
    let half_width = marker.width_m * 0.5;
    let half_depth = marker.depth_m * 0.5;
    if x.abs() > half_width || y.abs() > half_depth {
        let offset =
            right * x.clamp(-half_width, half_width) + forward * y.clamp(-half_depth, half_depth);
        let separation = horizontal - offset;
        let support = half_width * normal.dot(right).abs() + half_depth * normal.dot(forward).abs();
        if horizontal.dot(normal) - support > SIGN_FACING_GUARD_M {
            center = center + normal * support;
        } else if separation.length() > SIGN_FACING_GUARD_M {
            // An eye just outside a corner can be behind the radial support
            // plane. Projection onto the rectangle gives another support plane
            // whose normal actually points toward that external eye.
            normal = separation.normalized()?;
            center = center + offset;
        }
    }
    let right = up.cross(normal).normalized()?;
    // Restore an orthonormal frame after finite input roundoff while keeping
    // up fixed. It is this shared frame that supplies all rendered axes.
    let normal = right.cross(up).normalized()?;
    Some(MarkerSignFrame {
        center,
        right,
        up,
        normal,
    })
}

/// Flat local-XY plate, centered at zero, with front normal +Z. Its complete
/// black/white cell partition avoids overlapping coplanar text polygons.
/// Row runs reduce geometry while preserving the original five-by-seven cells.
pub fn marker_sign_mesh(marker: &SurveyMarker) -> Option<RenderMesh> {
    let (width, height) = sign_dimensions(marker)?;
    let number = marker.id + 1;
    let digits = [
        number / 1000,
        number / 100 % 10,
        number / 10 % 10,
        number % 10,
    ];
    let white = |row: usize, column: usize| {
        if row == 0 || row == SIGN_ROWS - 1 || column == 0 || column == SIGN_COLUMNS - 1 {
            return false;
        }
        let offset = column - 1;
        let digit = offset / 6;
        let digit_column = offset % 6;
        digit < 4
            && digit_column < 5
            && SIGN_DIGITS[digits[digit]][row - 1] & (1 << (4 - digit_column)) != 0
    };
    let mut mesh = RenderMesh::default();
    for row in 0..SIGN_ROWS {
        let mut column = 0;
        while column < SIGN_COLUMNS {
            let lit = white(row, column);
            let first_column = column;
            column += 1;
            while column < SIGN_COLUMNS && white(row, column) == lit {
                column += 1;
            }
            let x0 = width * (first_column as f64 / SIGN_COLUMNS as f64 - 0.5);
            let x1 = width * (column as f64 / SIGN_COLUMNS as f64 - 0.5);
            let y1 = height * (0.5 - row as f64 / SIGN_ROWS as f64);
            let y0 = height * (0.5 - (row + 1) as f64 / SIGN_ROWS as f64);
            let first = mesh.positions.len() as u32;
            let color = if lit { [1.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
            for (x, y) in [(x0, y1), (x0, y0), (x1, y0), (x1, y1)] {
                mesh.positions.push([x as f32, y as f32, 0.0]);
                mesh.normals.push([0.0, 0.0, 1.0]);
                mesh.colors.push(color);
            }
            mesh.indices.extend_from_slice(&[
                first,
                first + 1,
                first + 2,
                first,
                first + 2,
                first + 3,
            ]);
        }
    }
    Some(mesh)
}

/// The marker hover has already been checked against canonical Earth and other
/// prisms. The ordinary eye-range HUD must additionally respect visible grass.
/// Invalid supplied distances fail closed; None means no grass intersection.
pub fn marker_precedes_grass(marker_distance_m: f64, grass_distance_m: Option<f64>) -> bool {
    if !marker_distance_m.is_finite() || marker_distance_m < 0.0 {
        return false;
    }
    grass_distance_m
        .is_none_or(|grass| grass.is_finite() && grass >= 0.0 && marker_distance_m <= grass)
}

pub struct MarkerMesh {
    pub mesh: RenderMesh,
    /// UV.x is height in international feet. Caps use the adjacent band's centre.
    pub band_coordinates: Vec<[f32; 2]>,
}

/// Six independently normalled faces retain the exact OBB corners. Float32
/// conversion occurs only after subtracting the current floating origin in f64.
pub fn marker_mesh(marker: &SurveyMarker, local: LocalFrame) -> MarkerMesh {
    let corners = marker.corners();
    let height = marker.height_m / FOOT_M;
    let faces = [
        ([0, 3, 2, 1], -marker.up, Some(0.5)),
        ([4, 5, 6, 7], marker.up, Some(height.ceil() - 0.5)),
        ([0, 1, 5, 4], -marker.forward, None),
        ([1, 2, 6, 5], marker.right, None),
        ([2, 3, 7, 6], marker.forward, None),
        ([3, 0, 4, 7], -marker.right, None),
    ];
    let mut result = MarkerMesh {
        mesh: RenderMesh::default(),
        band_coordinates: Vec::new(),
    };
    for (indices, normal, cap_height) in faces {
        let first = result.mesh.positions.len() as u32;
        for index in indices {
            result.mesh.vertex(local, corners[index], normal, [1.0; 4]);
            result.band_coordinates.push([
                cap_height.unwrap_or(if index < 4 { 0.0 } else { height }) as f32,
                0.0,
            ]);
        }
        result.mesh.indices.extend_from_slice(&[
            first,
            first + 1,
            first + 2,
            first,
            first + 2,
            first + 3,
        ]);
    }
    result
}

/// Scalar specification for the shader's one-dimensional box filter. The
/// derivative footprint is a shading heuristic, not an exact 2D pixel integral.
pub fn red_band_fraction(height_feet: f64, footprint_feet: f64) -> Option<f64> {
    if !height_feet.is_finite() || !footprint_feet.is_finite() || footprint_feet < 0.0 {
        return None;
    }
    let width = footprint_feet.max(1e-4);
    if width >= BAND_MEAN_FOOTPRINT_FEET {
        return Some(0.5);
    }
    let phase = height_feet.rem_euclid(2.0);
    let lower = phase - width / 2.0;
    let upper = phase + width / 2.0;
    if lower >= phase.floor() && upper <= phase.floor() + 1.0 {
        return Some(if phase < 1.0 { 1.0 } else { 0.0 });
    }
    let primitive = |x: f64| (x / 2.0).floor() + x.rem_euclid(2.0).min(1.0);
    Some(((primitive(upper) - primitive(lower)) / width).clamp(0.0, 1.0))
}

/// Decorative, depth-tested tube plus A(cyan)/B(yellow) marks. These are never
/// measurement targets: the actual picked positions and ECEF chord are unchanged.
pub fn tape_mesh(a: Option<&TapePoint>, b: Option<&TapePoint>, local: LocalFrame) -> RenderMesh {
    let mut mesh = RenderMesh::default();
    if let (Some(a), Some(b)) = (a, b) {
        let start = a.position_ecef_m;
        let end = b.position_ecef_m;
        if let Some(axis) = (end - start).normalized() {
            let helper = if axis.z.abs() < 0.9 {
                Vec3d::new(0.0, 0.0, 1.0)
            } else {
                Vec3d::new(0.0, 1.0, 0.0)
            };
            let right = axis
                .cross(helper)
                .normalized()
                .expect("nonparallel tape frame");
            let forward = axis.cross(right);
            const SIDES: usize = 12;
            for index in 0..SIDES {
                let angle = std::f64::consts::TAU * index as f64 / SIDES as f64;
                let normal = right * angle.cos() + forward * angle.sin();
                mesh.vertex(
                    local,
                    start + normal * TAPE_RADIUS_M,
                    normal,
                    [0.95, 0.8, 0.05, 1.0],
                );
                mesh.vertex(
                    local,
                    end + normal * TAPE_RADIUS_M,
                    normal,
                    [0.95, 0.8, 0.05, 1.0],
                );
            }
            for index in 0..SIDES {
                let next = (index + 1) % SIDES;
                let i = (index * 2) as u32;
                let j = (next * 2) as u32;
                mesh.indices
                    .extend_from_slice(&[i, j, j + 1, i, j + 1, i + 1]);
            }
        }
    }
    for (point, color) in [(a, [0.0, 0.75, 1.0, 1.0]), (b, [1.0, 0.65, 0.0, 1.0])] {
        let Some(point) = point else { continue };
        let axes = [
            Vec3d::new(1.0, 0.0, 0.0),
            Vec3d::new(-1.0, 0.0, 0.0),
            Vec3d::new(0.0, 1.0, 0.0),
            Vec3d::new(0.0, -1.0, 0.0),
            Vec3d::new(0.0, 0.0, 1.0),
            Vec3d::new(0.0, 0.0, -1.0),
        ];
        for face in [
            [0, 2, 4],
            [2, 1, 4],
            [1, 3, 4],
            [3, 0, 4],
            [2, 0, 5],
            [1, 2, 5],
            [3, 1, 5],
            [0, 3, 5],
        ] {
            let normal = (axes[face[1]] - axes[face[0]])
                .cross(axes[face[2]] - axes[face[0]])
                .normalized()
                .expect("nondegenerate annotation face");
            let first = mesh.positions.len() as u32;
            for index in face {
                mesh.vertex(
                    local,
                    point.position_ecef_m + axes[index] * TAPE_ENDPOINT_RADIUS_M,
                    normal,
                    color,
                );
            }
            mesh.indices
                .extend_from_slice(&[first, first + 1, first + 2]);
        }
    }
    mesh
}
