//! Exact source-face export for bounded f32 curved-ray presentation.
//!
//! Source coordinates are subtracted/rotated in f64 before conversion. Each
//! patch retains its own local frame and original BVH face order. A separate
//! broad-phase hierarchy contains patch bounds, never substituted geometry.
use crate::{
    EnuBasis, Geodetic, TerrainScene, Vec3d, geodetic_to_ecef,
    rendering::{LocalFrame, survey::marker_sign_frame},
    survey::SurveyMarker,
};

pub const MAX_GEOMETRY_BYTES: usize = 120 * 1024 * 1024;
pub const MAX_PATCHES: usize = 128;
pub const MAX_MARKERS: usize = 512;
pub const LEAF_FLAG: u32 = 0x8000_0000;
pub const ADDRESS_MASK: u32 = 0x00ff_ffff;
pub const FRAME_HEADER_WORDS: usize = 6;
pub const PATCH_WORDS: usize = 4;
pub const MARKER_WORDS: usize = 8;

fn word(value: u32) -> f32 {
    f32::from_bits(value)
}
fn vector(v: [f32; 3], w: f32) -> [f32; 4] {
    [v[0], v[1], v[2], w]
}
fn point(v: [f32; 3]) -> Vec3d {
    Vec3d::new(v[0] as f64, v[1] as f64, v[2] as f64)
}
fn axis(frame: LocalFrame) -> [Vec3d; 3] {
    [frame.basis.east, frame.basis.up, -frame.basis.north]
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vec3d,
    pub max: Vec3d,
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: Vec3d::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            max: Vec3d::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }
    fn include(&mut self, p: Vec3d) {
        self.min = Vec3d::new(
            self.min.x.min(p.x),
            self.min.y.min(p.y),
            self.min.z.min(p.z),
        );
        self.max = Vec3d::new(
            self.max.x.max(p.x),
            self.max.y.max(p.y),
            self.max.z.max(p.z),
        );
    }
    fn union(&mut self, b: Self) {
        self.include(b.min);
        self.include(b.max);
    }
    fn center(self) -> Vec3d {
        (self.min + self.max) * 0.5
    }
    fn corners(self) -> [Vec3d; 8] {
        std::array::from_fn(|i| {
            Vec3d::new(
                if i & 1 == 0 { self.min.x } else { self.max.x },
                if i & 2 == 0 { self.min.y } else { self.max.y },
                if i & 4 == 0 { self.min.z } else { self.max.z },
            )
        })
    }
    fn packed(self, escape: u32, payload: u32) -> [[f32; 4]; 2] {
        // Padding also contains the f64 source positions before quantization.
        let low = [self.min.x, self.min.y, self.min.z].map(|x| (x as f32).next_down());
        let high = [self.max.x, self.max.y, self.max.z].map(|x| (x as f32).next_up());
        [vector(low, word(escape)), vector(high, word(payload))]
    }
}

#[derive(Debug)]
pub struct Patch {
    pub frame: LocalFrame,
    pub root: u32,
    pub source_position_error_m: f64,
    pub triangles: usize,
}

#[derive(Debug)]
pub struct PackedScene {
    /// Two vec4 records/node, three vec4 records/triangle. Integer metadata is
    /// bitcast, never rounded through a numeric float index.
    pub geometry: Vec<[f32; 4]>,
    pub patches: Vec<Patch>,
    pub frame: LocalFrame,
    pub root: u32,
    pub triangle_count: usize,
}

fn frame_at(location: Geodetic) -> LocalFrame {
    LocalFrame {
        origin: geodetic_to_ecef(location),
        basis: EnuBasis::at(location),
    }
}

fn append_tree(data: &mut Vec<[f32; 4]>, objects: &mut [(usize, Bounds)]) -> u32 {
    let index = data.len();
    data.extend([[0.0; 4]; 2]);
    let mut bounds = Bounds::empty();
    for (_, b) in objects.iter() {
        bounds.union(*b);
    }
    let payload = if objects.len() == 1 {
        LEAF_FLAG | objects[0].0 as u32
    } else {
        let extent = bounds.max - bounds.min;
        let dimension = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        let coordinate = |p: Vec3d| [p.x, p.y, p.z][dimension];
        objects
            .sort_unstable_by(|a, b| coordinate(a.1.center()).total_cmp(&coordinate(b.1.center())));
        let middle = objects.len() / 2;
        let (left, right) = objects.split_at_mut(middle);
        append_tree(data, left);
        append_tree(data, right);
        0
    };
    let records = bounds.packed(data.len() as u32, payload);
    data[index..index + 2].copy_from_slice(&records);
    index as u32
}

impl PackedScene {
    pub fn prepare(scene: &TerrainScene) -> Result<Self, String> {
        let physical = if scene.is_boundary_only() {
            &[][..]
        } else {
            scene.patches()
        };
        if physical.len() > MAX_PATCHES {
            return Err(format!(
                "Optical scene exceeds {MAX_PATCHES} terrain patches."
            ));
        }
        let triangle_count = physical.iter().map(|p| p.triangle_count()).sum::<usize>();
        // Original binary BVH has at most two nodes per triangle. This cheap
        // conservative bound rejects excessive allocations before exporting.
        let maximum_words = 1usize
            .checked_add(
                triangle_count
                    .checked_mul(7)
                    .ok_or("Optical geometry size overflow.")?,
            )
            .ok_or("Optical geometry size overflow.")?;
        if maximum_words > MAX_GEOMETRY_BYTES / 16 {
            return Err("Optical geometry exceeds the 120 MiB storage budget.".into());
        }
        let base = physical.first().map_or(
            Geodetic {
                lat_deg: 0.0,
                lon_deg: 0.0,
                height_m: 0.0,
            },
            |p| {
                let [west, south, east, north] = p.bounds();
                Geodetic {
                    lat_deg: (south + north) * 0.5,
                    lon_deg: (west + east) * 0.5,
                    height_m: 0.0,
                }
            },
        );
        let mut result = Self {
            geometry: vec![[0.0; 4]],
            patches: Vec::new(),
            frame: frame_at(base),
            root: 0,
            triangle_count,
        };
        let mut objects = Vec::new();
        for (patch_id, patch) in physical.iter().enumerate() {
            let [west, south, east, north] = patch.bounds();
            let local = frame_at(Geodetic {
                lat_deg: (south + north) * 0.5,
                lon_deg: (west + east) * 0.5,
                height_m: 0.0,
            });
            let mesh = patch.optical_mesh();
            let positions: Vec<_> = mesh.vertices.iter().map(|&p| local.position(p)).collect();
            let mut source_error = 0.0_f64;
            let axes = axis(local);
            for (&source, &position) in mesh.vertices.iter().zip(&positions) {
                let delta = source - local.origin;
                let exact = Vec3d::new(delta.dot(axes[0]), delta.dot(axes[1]), delta.dot(axes[2]));
                source_error = source_error.max(exact.distance(point(position)));
            }
            let root = result.geometry.len();
            result
                .geometry
                .resize(root + mesh.nodes.len() * 2, [0.0; 4]);
            let triangle_start = result.geometry.len();
            for triangle in &mesh.triangles {
                for &vertex in triangle {
                    result
                        .geometry
                        .push(vector(positions[vertex as usize], 0.0));
                }
            }
            let mut bounds = vec![Bounds::empty(); mesh.nodes.len()];
            let mut escapes = vec![0usize; mesh.nodes.len()];
            // Original nodes are depth-first and children follow parents.
            for (index, node) in mesh.nodes.iter().enumerate().rev() {
                if node.count > 0 {
                    if node.count > 8 {
                        return Err("Unsupported optical BVH leaf size.".into());
                    }
                    for triangle in &mesh.triangles[node.start..node.start + node.count] {
                        for &vertex in triangle {
                            bounds[index].include(point(positions[vertex as usize]));
                        }
                    }
                    escapes[index] = index + 1;
                } else {
                    let left = bounds[node.left];
                    let right = bounds[node.right];
                    bounds[index].union(left);
                    bounds[index].union(right);
                    escapes[index] = escapes[node.right];
                }
                let payload = if node.count > 0 {
                    let address = triangle_start + node.start * 3;
                    if address > ADDRESS_MASK as usize {
                        return Err("Optical triangle address overflow.".into());
                    }
                    LEAF_FLAG | ((node.count as u32 - 1) << 24) | address as u32
                } else {
                    0
                };
                let records = bounds[index].packed((root + escapes[index] * 2) as u32, payload);
                result.geometry[root + index * 2..root + index * 2 + 2].copy_from_slice(&records);
            }
            // Top-level bounds include all local box corners after f64
            // transformation, plus the measured source quantization allowance.
            let mut global = Bounds::empty();
            for p in bounds[0].corners() {
                let ecef = local.origin + axes[0] * p.x + axes[1] * p.y + axes[2] * p.z;
                let d = ecef - result.frame.origin;
                let a = axis(result.frame);
                global.include(Vec3d::new(d.dot(a[0]), d.dot(a[1]), d.dot(a[2])));
            }
            let pad = Vec3d::new(source_error, source_error, source_error);
            global.min = global.min - pad;
            global.max = global.max + pad;
            objects.push((patch_id, global));
            result.patches.push(Patch {
                frame: local,
                root: root as u32,
                source_position_error_m: source_error,
                triangles: mesh.triangles.len(),
            });
        }
        if !objects.is_empty() {
            result.root = append_tree(&mut result.geometry, &mut objects);
        }
        if result.geometry.len() > MAX_GEOMETRY_BYTES / 16 {
            return Err("Optical geometry exceeds the 120 MiB storage budget.".into());
        }
        Ok(result)
    }

    /// Frame buffer: header, global transform, one transform per patch, eight
    /// records per marker, then the marker/sign broad-phase tree. Marker signs
    /// remain annotations and are absent from CPU physical hit/range queries.
    pub fn frame_data(
        &self,
        eye: LocalFrame,
        markers: &[SurveyMarker],
    ) -> Result<Vec<[f32; 4]>, String> {
        if markers.len() > MAX_MARKERS {
            return Err(format!(
                "Optical display supports at most {MAX_MARKERS} markers."
            ));
        }
        let mut data = vec![[0.0; 4]; 2];
        let transform = |target: LocalFrame, root: u32, error: f64| {
            let a = axis(target);
            let b = axis(eye);
            [
                vector(target.position(eye.origin), word(root)),
                [
                    a[0].dot(b[0]) as f32,
                    a[0].dot(b[1]) as f32,
                    a[0].dot(b[2]) as f32,
                    error as f32,
                ],
                [
                    a[1].dot(b[0]) as f32,
                    a[1].dot(b[1]) as f32,
                    a[1].dot(b[2]) as f32,
                    0.0,
                ],
                [
                    a[2].dot(b[0]) as f32,
                    a[2].dot(b[1]) as f32,
                    a[2].dot(b[2]) as f32,
                    0.0,
                ],
            ]
        };
        data.extend(transform(self.frame, self.root, 0.0));
        for patch in &self.patches {
            data.extend(transform(
                patch.frame,
                patch.root,
                patch.source_position_error_m,
            ));
        }
        let marker_start = data.len();
        let mut objects = Vec::with_capacity(markers.len());
        for (index, marker) in markers.iter().enumerate() {
            if !marker.valid() {
                return Err("Invalid optical marker geometry.".into());
            }
            let sign =
                marker_sign_frame(marker, eye.origin).ok_or("Invalid optical marker sign.")?;
            let width = marker.width_m;
            let height = width * 9.0 / 25.0;
            data.extend([
                vector(eye.position(marker.base_ecef_m), word(marker.id as u32)),
                vector(eye.direction(marker.right), (width * 0.5) as f32),
                vector(eye.direction(marker.forward), (marker.depth_m * 0.5) as f32),
                vector(eye.direction(marker.up), marker.height_m as f32),
                vector(eye.position(sign.center), width as f32),
                vector(eye.direction(sign.right), height as f32),
                vector(eye.direction(sign.up), 0.0),
                vector(eye.direction(sign.normal), 0.0),
            ]);
            let mut bounds = Bounds::empty();
            for p in marker.corners() {
                bounds.include(point(eye.position(p)));
            }
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    bounds.include(point(eye.position(
                        sign.center + sign.right * (x * width) + sign.up * (y * height),
                    )));
                }
            }
            objects.push((index, bounds));
        }
        let marker_root = if objects.is_empty() {
            0
        } else {
            append_tree(&mut data, &mut objects)
        };
        data[0] = [
            word(self.root),
            word(self.patches.len() as u32),
            word(marker_root),
            word(markers.len() as u32),
        ];
        data[1] = [word(marker_start as u32), 0.0, 0.0, 0.0];
        Ok(data)
    }
}
