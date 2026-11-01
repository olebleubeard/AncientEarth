//! Original longitude/latitude polygon edges in a bounded adaptive index.
//!
//! Cells accelerate lookup; their edges never become geographic coastlines.
//! The seed-to-query path stays in one convex cell, so only its indexed source
//! segments can change the source ring winding. No physical height is stored.
use super::{Geodetic, TopographyError, invalid};

pub const ROOT_WIDTH: usize = 360;
pub const ROOT_HEIGHT: usize = 180;
pub const MAX_VECTOR_EDGES: usize = 32;
pub const MAX_VECTOR_DEPTH: usize = 20;
const MAX_GPU_WORDS: usize = 16_000_000;

#[derive(Clone, Copy, Debug)]
pub struct VectorNode {
    /// Zero for a leaf, otherwise first of four contiguous SW, SE, NW, NE nodes.
    pub children: u32,
    pub edge_start: u32,
    pub edge_count: u32,
    /// Signed sum of all source rings: clockwise land exteriors are negative.
    pub winding: i32,
    /// Signed sum of counterclockwise inland-water hole rings only.
    pub hole_winding: i32,
    /// Strictly interior, source-classified seed in normalized cell [x,y].
    pub seed: [f64; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct VectorEdge {
    /// A directed original segment fragment, normalized within its leaf.
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub hole: bool,
}

#[derive(Clone, Debug)]
pub struct GlobalLandVector {
    nodes: Vec<VectorNode>,
    edges: Vec<VectorEdge>,
}

impl GlobalLandVector {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TopographyError> {
        if bytes.len() < 32 || &bytes[..8] != b"AEGLND02" {
            return Err(invalid("invalid global vector format"));
        }
        let word = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let scalar = |i| f64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        let nodes = word(16) as usize;
        let edges = word(20) as usize;
        if word(8) as usize != ROOT_WIDTH
            || word(12) as usize != ROOT_HEIGHT
            || word(24) as usize != MAX_VECTOR_EDGES
            || word(28) as usize != MAX_VECTOR_DEPTH
            || !(ROOT_WIDTH * ROOT_HEIGHT..=MAX_GPU_WORDS / 8).contains(&nodes)
            || edges > MAX_GPU_WORDS / 5
            || nodes * 8 + edges * 5 > MAX_GPU_WORDS
            || 32 + (nodes + edges) * 36 != bytes.len()
        {
            return Err(invalid("invalid global vector bounds"));
        }
        let mut result = Self {
            nodes: Vec::with_capacity(nodes),
            edges: Vec::with_capacity(edges),
        };
        for index in 0..nodes {
            let offset = 32 + index * 36;
            let node = VectorNode {
                children: word(offset),
                edge_start: word(offset + 4),
                edge_count: word(offset + 8),
                winding: word(offset + 12) as i32,
                hole_winding: word(offset + 16) as i32,
                seed: [scalar(offset + 20), scalar(offset + 28)],
            };
            if !node
                .seed
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v < 1.0)
                || node.winding > 0
                || node.winding < -1_000_000
                || !(0..=1_000_000).contains(&node.hole_winding)
                || node.edge_count as usize > MAX_VECTOR_EDGES
                || u64::from(node.edge_start) + u64::from(node.edge_count) > edges as u64
                || (node.children != 0
                    && (node.children as usize <= index
                        || (node.children as usize) < ROOT_WIDTH * ROOT_HEIGHT
                        || u64::from(node.children) + 4 > nodes as u64
                        || node.edge_count != 0))
            {
                return Err(invalid("invalid global vector node"));
            }
            result.nodes.push(node);
        }
        for index in 0..edges {
            let offset = 32 + (nodes + index) * 36;
            let a = [scalar(offset), scalar(offset + 8)];
            let b = [scalar(offset + 16), scalar(offset + 24)];
            let flag = word(offset + 32);
            if !a
                .iter()
                .chain(b.iter())
                .all(|v| v.is_finite() && *v >= -1e-10 && *v <= 1.0 + 1e-10)
                || a == b
                || flag > 1
            {
                return Err(invalid("invalid global source segment"));
            }
            result.edges.push(VectorEdge {
                a,
                b,
                hole: flag != 0,
            });
        }
        // Validate a complete forest and unique edge ownership, not just indices.
        let mut visited = vec![false; nodes];
        let mut used_edges = vec![false; edges];
        let mut stack: Vec<_> = (0..ROOT_WIDTH * ROOT_HEIGHT).map(|i| (i, 0)).collect();
        while let Some((index, depth)) = stack.pop() {
            if visited[index] || depth > MAX_VECTOR_DEPTH {
                return Err(invalid("cyclic or overdeep global vector index"));
            }
            visited[index] = true;
            let node = result.nodes[index];
            if node.children != 0 {
                stack.extend((0..4).map(|i| (node.children as usize + i, depth + 1)));
            } else {
                let start = node.edge_start as usize;
                let end = (node.edge_start + node.edge_count) as usize;
                for (used, edge) in used_edges[start..end]
                    .iter_mut()
                    .zip(&result.edges[start..end])
                {
                    if *used {
                        return Err(invalid("shared global vector edge interval"));
                    }
                    *used = true;
                    let d = [edge.b[0] - edge.a[0], edge.b[1] - edge.a[1]];
                    let p = [node.seed[0] - edge.a[0], node.seed[1] - edge.a[1]];
                    let t = (dot(p, d) / dot(d, d)).clamp(0.0, 1.0);
                    let distance = (p[0] - t * d[0]).hypot(p[1] - t * d[1]);
                    if distance <= 1e-6 {
                        return Err(invalid("global vector seed touches source boundary"));
                    }
                }
            }
        }
        if visited.contains(&false) || used_edges.contains(&false) {
            return Err(invalid("unreachable global vector records"));
        }
        Ok(result)
    }

    pub fn nodes(&self) -> &[VectorNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[VectorEdge] {
        &self.edges
    }

    pub fn classify(&self, location: Geodetic) -> Option<u8> {
        let location = Geodetic::new(location.lat_deg, location.lon_deg, 0.0).ok()?;
        let longitude = (location.lon_deg + 180.0).rem_euclid(360.0);
        let latitude = location.lat_deg + 90.0;
        let column = longitude.floor() as usize;
        let row = (latitude.floor() as usize).min(ROOT_HEIGHT - 1);
        let mut point = [longitude - column as f64, latitude - row as f64];
        let mut index = row * ROOT_WIDTH + column;
        for _ in 0..=MAX_VECTOR_DEPTH {
            let node = self.nodes[index];
            if node.children == 0 {
                return Some(classify_leaf(node, &self.edges, point));
            }
            let east = usize::from(point[0] >= 0.5);
            let north = usize::from(point[1] >= 0.5);
            point = [point[0] * 2.0 - east as f64, point[1] * 2.0 - north as f64];
            index = node.children as usize + east + 2 * north;
        }
        None
    }
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn classify_leaf(node: VectorNode, edges: &[VectorEdge], point: [f64; 2]) -> u8 {
    let direction = [point[0] - node.seed[0], point[1] - node.seed[1]];
    let extent = dot(direction, direction);
    let mut winding = node.winding;
    let mut hole = node.hole_winding;
    if extent > 0.0 {
        for edge in &edges[node.edge_start as usize..(node.edge_start + node.edge_count) as usize] {
            let a = [edge.a[0] - node.seed[0], edge.a[1] - node.seed[1]];
            let b = [edge.b[0] - node.seed[0], edge.b[1] - node.seed[1]];
            let ya = cross(direction, a);
            let yb = cross(direction, b);
            if (ya > 0.0) != (yb > 0.0) {
                let t = (dot(direction, a) * yb - dot(direction, b) * ya) / (yb - ya);
                // Half-open edge crossings; exact point-on-coast decisions are
                // finite numerical category conventions, never height changes.
                if t > 0.0 && t < extent {
                    let change = if ya > yb { 1 } else { -1 };
                    winding += change;
                    if edge.hole {
                        hole += change;
                    }
                }
            }
        }
    }
    if winding < 0 {
        0
    } else if hole > 0 {
        2
    } else {
        1
    }
}
