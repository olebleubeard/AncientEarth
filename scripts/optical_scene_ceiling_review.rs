//! Independent finite geometry and packing checks; no renderer registration.
use super::*;

fn normal_coordinate(latitude: f64, longitude: f64, height: f64) -> Vec3d {
    let (s, c) = latitude.to_radians().sin_cos();
    let (sl, cl) = longitude.to_radians().sin_cos();
    let n = Vec3d::new(c * cl, c * sl, s);
    // Support point of the axis ellipsoid, independently constructed from its
    // prescribed normal; no geodetic inverse or radial-height bound is reused.
    let a2 = WGS84_A_M * WGS84_A_M;
    let b2 = WGS84_B_M * WGS84_B_M;
    let support = (a2 * (n.x * n.x + n.y * n.y) + b2 * n.z * n.z).sqrt();
    Vec3d::new(a2 * n.x / support, a2 * n.y / support, b2 * n.z / support) + n * height
}

fn independent_signed_height(p: Vec3d) -> f64 {
    // Diagonal SPD Lagrange multiplier for the nearest ellipsoid point.
    // The finite fixtures stay in the regular near-surface normal tube.
    let squared = [WGS84_A_M.powi(2), WGS84_A_M.powi(2), WGS84_B_M.powi(2)];
    let point = [p.x, p.y, p.z];
    let equation = |lambda: f64| {
        squared
            .iter()
            .zip(point)
            .map(|(&a2, x)| a2 * x * x / (a2 + lambda).powi(2))
            .sum::<f64>()
            - 1.
    };
    let (mut lo, mut hi) = if equation(0.) >= 0. {
        (0., squared[0])
    } else {
        (-0.01 * squared[2], 0.)
    };
    assert!(equation(lo) >= 0. && equation(hi) <= 0.);
    for _ in 0..100 {
        let mid = lo + (hi - lo) * 0.5;
        if mid == lo || mid == hi {
            break;
        }
        if equation(mid) > 0. {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lambda = lo + (hi - lo) * 0.5;
    lambda
        * squared
            .iter()
            .zip(point)
            .map(|(&a2, x)| (x / (a2 + lambda)).powi(2))
            .sum::<f64>()
            .sqrt()
}

#[test]
fn axis_distances_empty_negative_and_nonfinite_inputs_remain_conservative() {
    let empty = HeightCeiling::default();
    assert_eq!(empty.upper_m, 0.);
    assert_eq!(empty.vertices, 0);
    for axis in 0..3 {
        let radius = if axis == 2 { WGS84_B_M } else { WGS84_A_M };
        for sign in [-1_f64, 1.] {
            for offset in [-10000., -30., -0.001, 0., 0.001, 160., 120000., 1e6] {
                let mut coordinates = [0.; 3];
                coordinates[axis] = sign * (radius + offset);
                let p = Vec3d::new(coordinates[0], coordinates[1], coordinates[2]);
                let exact_axis_distance = coordinates[axis].abs() - radius;
                let bound = vertex_height_upper(p).unwrap();
                assert!(bound >= exact_axis_distance);
                assert!(bound >= 0.);
                if exact_axis_distance < -0.001 {
                    assert_eq!(bound, 0.);
                }
                assert!(bound - exact_axis_distance.max(0.) < 1e-7);
            }
        }
    }
    let mut negative = HeightCeiling::default();
    for latitude in [-80., 0., 55.] {
        negative
            .include(normal_coordinate(latitude, 30., -30.))
            .unwrap();
    }
    assert_eq!(negative.upper_m, 0.);
    assert_eq!(negative.vertices, 3);
    assert!(negative.maximizing_vertex.is_none());
    for p in [
        Vec3d::new(f64::NAN, 0., 0.),
        Vec3d::new(0., f64::INFINITY, 0.),
        Vec3d::new(f64::MAX, 0., 0.),
    ] {
        assert!(vertex_height_upper(p).is_none());
        assert!(negative.include(p).is_none());
        assert_eq!(negative.vertices, 3);
    }
}

#[test]
fn prescribed_normal_heights_and_face_interior_stay_below_vertex_ceiling() {
    for latitude in [-90., -67., -30., 0., 29.3, 80., 90.] {
        for longitude in [-170., -94.7, 0., 137.] {
            for height in [-10000., -30., 0., 166., 120000.] {
                let p = normal_coordinate(latitude, longitude, height);
                let bound = vertex_height_upper(p).unwrap();
                let reference = independent_signed_height(p);
                // Explicit binary64 construction/oracle allowance, separate
                // from the module's actual outward certificate.
                assert!((reference - height).abs() < 2e-8);
                assert!(reference <= bound + 2e-8);
            }
            let vertices = [
                normal_coordinate(latitude.clamp(-89., 89.), longitude, -20.),
                normal_coordinate((latitude + 0.02).clamp(-89., 89.), longitude + 0.03, 40.),
                normal_coordinate((latitude - 0.02).clamp(-89., 89.), longitude - 0.02, 170.),
            ];
            let mut ceiling = HeightCeiling::default();
            for point in vertices {
                ceiling.include(point).unwrap();
            }
            for i in 0..=10 {
                for j in 0..=10 - i {
                    let a = f64::from(i) / 10.;
                    let b = f64::from(j) / 10.;
                    let p = vertices[0] * a + vertices[1] * b + vertices[2] * (1. - a - b);
                    assert!(independent_signed_height(p) <= ceiling.upper_m + 2e-8);
                }
            }
        }
    }
}

#[test]
fn sign_extrema_explicit_allowances_and_float_packing_are_not_omitted() {
    let mut ceiling = HeightCeiling::default();
    // Equatorial box plus a separate higher decorative plate. These are
    // prescribed Cartesian corners, not a reuse of marker-corner generation.
    for x in [-5., 5.] {
        for y in [-2., 2.] {
            for z in [-3., 3.] {
                ceiling.include(Vec3d::new(WGS84_A_M + x, y, z)).unwrap();
            }
        }
    }
    let prism_ceiling = ceiling.upper_m;
    for y in [-2., 2.] {
        for z in [-1., 1.] {
            ceiling.include(Vec3d::new(WGS84_A_M + 20., y, z)).unwrap();
        }
    }
    assert_eq!(ceiling.vertices, 12);
    assert!(ceiling.upper_m > prism_ceiling + 14.);
    assert!(ceiling.maximizing_vertex.unwrap().x == WGS84_A_M + 20.);
    for allowance in [0., 1e-40, 0.003, 0.25, 1., 10000.] {
        let packed = ceiling.packed_with_position_allowance(allowance).unwrap();
        assert!(f64::from(packed) >= ceiling.upper_m + allowance);
        assert!(packed.is_normal());
    }
    for bad in [-1., f64::NAN, f64::INFINITY] {
        assert!(ceiling.packed_with_position_allowance(bad).is_none());
    }
    assert!(ceiling.packed_with_position_allowance(f64::MAX).is_none());
    let empty = HeightCeiling::default();
    assert_eq!(empty.packed_with_position_allowance(0.), Some(0.));
    for tiny in [f64::from_bits(1), 1e-46, 1e-40] {
        let packed = empty.packed_with_position_allowance(tiny).unwrap();
        assert!(
            packed.is_normal() && f64::from(packed) >= tiny,
            "Positive subnormal ceilings cannot rely on WGSL preserving them"
        );
    }
}
