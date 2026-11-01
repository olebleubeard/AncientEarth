//! Original MIT. Unused sufficient coverage metadata in the exact stored
//! patch-coordinate matrix. The matrix is proved invertible, not assumed to
//! be orthogonal. Query intervals must use this same matrix.
#[allow(dead_code)]
#[path = "optical_boundary_coverage.rs"]
mod boundary;
#[allow(dead_code)]
#[path = "optical_taylor.rs"]
mod arithmetic;
use arithmetic::Interval as I;
use earth_math::Vec3d;

#[derive(Clone, Debug)]
pub struct CoverageCore {
    /// center XYZ/radius; half XZ/minY/maxY; vertex error/valid bits/0/0.
    pub words: [[f32; 4]; 3],
    pub degree: i64,
    pub determinant: [f64; 2],
    pub canonical_vertex_error_m: f64,
    pub nominal_center: [f64; 3],
    pub nominal_half_extent: [f64; 2],
    pub nominal_boundary_radius: f64,
    pub nominal_vertical_bounds: [f64; 2],
}
fn xyz(v: Vec3d) -> [f64; 3] { [v.x, v.y, v.z] }
fn exact(v: Vec3d) -> [I; 3] { xyz(v).map(I::point) }
fn dot(a: [I; 3], b: [I; 3]) -> I {
    a.into_iter().zip(b).fold(I::point(0.), |s, (x, y)| s + x * y)
}
fn cross(a: [I; 3], b: [I; 3]) -> [I; 3] {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}
fn lower(x: f64) -> Option<f32> {
    if !x.is_finite() { return None; }
    let mut y = x as f32;
    if f64::from(y) > x { y = y.next_down(); }
    if y != 0. && y.abs() < f32::MIN_POSITIVE { y = if y < 0. { -f32::MIN_POSITIVE } else { 0. }; }
    if x < 0. && y == 0. { y = -f32::MIN_POSITIVE; }
    (y.is_finite() && f64::from(y) <= x).then_some(y)
}
fn upper(x: f64) -> Option<f32> {
    lower(-x).map(|y| -y)
}

/// `axes` are exact stored binary64 rows X/up/Z of the patch transform.
/// Triangle geometry remains canonical. The temporary rounded projection is
/// used only for the existing boundary-degree proof; its discrepancy from
/// exact transformed vertices is explicitly enclosed below.
pub fn prepare(vertices: &[Vec3d], triangles: &[[u32; 3]], origin: Vec3d, axes: [Vec3d; 3]) -> Option<CoverageCore> {
    if vertices.is_empty() || !origin.is_finite() || axes.iter().any(|v| !v.is_finite()) { return None; }
    let determinant = dot(exact(axes[0]), cross(exact(axes[1]), exact(axes[2])));
    if !determinant.valid() || (determinant.lo <= 0. && determinant.hi >= 0.) { return None; }
    // BoundaryChain uses XY horizontal and Z vertical; the GPU uses XZ/Y.
    let chain = boundary::BoundaryChain::in_frame(vertices, triangles, origin, [axes[0], axes[2], axes[1]])?;
    let core = chain.interior_core()?;
    let mut vertex_error = 0_f64;
    for (&vertex, &rounded) in vertices.iter().zip(&chain.vertices) {
        if !vertex.is_finite() { return None; }
        let delta = std::array::from_fn(|i| I::point(xyz(vertex)[i])-I::point(xyz(origin)[i]));
        let projected = axes.map(|axis| dot(exact(axis), delta));
        let represented = [rounded.x, rounded.z, rounded.y];
        let error = projected.into_iter().zip(represented).fold(I::point(0.), |s,(x,y)| s+I::point((x-I::point(y)).abs_upper()));
        if !error.valid() { return None; }
        vertex_error = vertex_error.max(error.hi);
    }
    let center = [core.center.x as f32, core.center.z as f32, core.center.y as f32];
    if center.iter().any(|v| !v.is_finite()) { return None; }
    let nominal = [core.center.x, core.center.z, core.center.y];
    let delta: [I; 3] = std::array::from_fn(|i| I::point(f64::from(center[i]))-I::point(nominal[i]));
    let half_x = I::point(core.half_extent[0])-I::point(delta[0].abs_upper());
    let half_z = I::point(core.half_extent[1])-I::point(delta[2].abs_upper());
    let recenter = delta.into_iter().fold(I::point(0.),|s,x|s+I::point(x.abs_upper()));
    let radius = I::point(core.boundary_radius)+recenter;
    if !half_x.valid() || !half_z.valid() || !radius.valid() { return None; }
    let half = [lower(half_x.lo)?, lower(half_z.lo)?];
    if half.iter().any(|v| *v <= 0.) { return None; }
    Some(CoverageCore {
        words: [
            [center[0], center[1], center[2], upper(radius.hi)?],
            [half[0], half[1], lower(chain.min.z)?, upper(chain.max.z)?],
            [upper(vertex_error)?, f32::from_bits(1), 0., 0.],
        ],
        degree: core.degree,
        determinant: [determinant.lo, determinant.hi],
        canonical_vertex_error_m: vertex_error,
        nominal_center: nominal,
        nominal_half_extent: core.half_extent,
        nominal_boundary_radius: core.boundary_radius,
        nominal_vertical_bounds: [chain.min.z, chain.max.z],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planar_chain_has_core_and_exact_matrix_must_be_invertible() {
        let p = [Vec3d::new(-10.,2.,-10.),Vec3d::new(10.,2.,-10.),Vec3d::new(10.,2.,10.),Vec3d::new(-10.,2.,10.)];
        let t = [[0,1,2],[0,2,3]];
        let axes = [Vec3d::new(1.,0.,0.),Vec3d::new(0.,1.,0.),Vec3d::new(0.,0.,1.)];
        let result = prepare(&p,&t,Vec3d::ZERO,axes).unwrap();
        assert_ne!(result.degree,0);
        assert!(result.determinant[0]>0.);
        assert!(result.words[1][0]>9.99 && result.words[1][1]>9.99);
        assert!(result.words[1][2]<=2. && result.words[1][3]>=2.);
        assert_eq!(result.words[2][1].to_bits(),1);
        assert!(prepare(&p,&t,Vec3d::ZERO,[axes[0],axes[0],axes[2]]).is_none());
    }
    #[test]
    fn signed_tiny_outward_packing_survives_flush_to_zero() {
        let tiny=2_f64.powi(-150);
        assert_eq!(lower(tiny),Some(0.));
        assert_eq!(upper(tiny),Some(f32::MIN_POSITIVE));
        assert_eq!(lower(-tiny),Some(-f32::MIN_POSITIVE));
        assert_eq!(upper(-tiny),Some(0.));
    }
}

#[cfg(test)]
#[path = "optical_canonical_coverage_packet_review.rs"]
mod independent_review;
