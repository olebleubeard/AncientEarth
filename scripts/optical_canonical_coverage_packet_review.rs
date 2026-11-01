//! Independent finite fixtures for the unused exact-stored-frame packet.
use super::*;

// All manufactured source coordinates are multiples of 1/16 and matrix
// perturbations are 0 or +/-2^-44. Exact projections therefore fit this
// integer lattice; no production interval or dot routine is the oracle.
const DENOMINATOR: f64 = 281_474_976_710_656.0; // 2^48
fn lattice(x: f64) -> i128 {
    let value = x * DENOMINATOR;
    let integer = value as i128;
    assert_eq!(integer as f64, value);
    integer
}

#[test]
fn exact_dyadic_shear_reflection_and_translated_vertex_discrepancies() {
    let triangles = [[0, 1, 2], [0, 2, 3]];
    for center in [0., 1024., 1_048_576.] {
        for reflection in [-1., 1.] {
            for epsilon_sign in [-1_i128, 0, 1] {
                let epsilon = epsilon_sign as f64 * 2_f64.powi(-44);
                let origin = Vec3d::new(4_194_304., -2_097_152., 1_048_576.);
                let local = [
                    Vec3d::new(center - 10., 3., -10.),
                    Vec3d::new(center + 10., 3., -10.),
                    Vec3d::new(center + 10., 3., 10.),
                    Vec3d::new(center - 10., 3., 10.),
                ];
                let vertices = local.map(|p| origin + p);
                let axes = [
                    Vec3d::new(reflection, epsilon, 0.),
                    Vec3d::new(0., 1., epsilon),
                    Vec3d::new(0., 0., 1.),
                ];
                let packet = prepare(&vertices, &triangles, origin, axes).unwrap();
                assert!(packet.determinant[0] <= reflection);
                assert!(packet.determinant[1] >= reflection);
                assert_ne!(packet.degree, 0);
                let chain = boundary::BoundaryChain::in_frame(
                    &vertices, &triangles, origin, [axes[0], axes[2], axes[1]],
                ).unwrap();
                let mut exact_maximum = 0_i128;
                for (input, represented) in local.iter().zip(&chain.vertices) {
                    let x = (input.x * 16.) as i128;
                    let y = (input.y * 16.) as i128;
                    let z = (input.z * 16.) as i128;
                    let exact = [
                        reflection as i128 * (x << 44) + epsilon_sign * y,
                        (y << 44) + epsilon_sign * z,
                        z << 44,
                    ];
                    let rounded = [represented.x, represented.z, represented.y];
                    let error = exact.into_iter().zip(rounded)
                        .map(|(truth, value)| (truth - lattice(value)).abs()).sum();
                    exact_maximum = exact_maximum.max(error);
                    let y_exact = exact[1] as f64 / DENOMINATOR;
                    assert!(f64::from(packet.words[1][2]) - f64::from(packet.words[2][0]) <= y_exact);
                    assert!(f64::from(packet.words[1][3]) + f64::from(packet.words[2][0]) >= y_exact);
                }
                let exact_error = exact_maximum as f64 / DENOMINATOR;
                assert!(packet.canonical_vertex_error_m >= exact_error);
                assert!(f64::from(packet.words[2][0]) >= packet.canonical_vertex_error_m);
                if center == 1_048_576. && epsilon_sign != 0 {
                    assert!(exact_maximum > 0, "fixture must expose rounded projection loss");
                }
                // Independently intersect projected vertical lines with the
                // exact image plane Y=3+epsilon*Z. These are incidences under
                // an invertible linear map, without orthogonality assumptions.
                let packed_center = packet.words[0].map(f64::from);
                let half = packet.words[1].map(f64::from);
                let error = f64::from(packet.words[2][0]);
                for ix in -4..=4 {
                    for iz in -4..=4 {
                        let x = packed_center[0] + f64::from(ix) * (half[0]-error) / 5.;
                        let z = packed_center[2] + f64::from(iz) * (half[1]-error) / 5.;
                        let y = 3. + epsilon*z;
                        let original_x = reflection*(x-epsilon*3.);
                        assert!(original_x > center-10. && original_x < center+10.);
                        assert!(z > -10. && z < 10.);
                        assert!(y >= half[2]-error && y <= half[3]+error);
                    }
                }
            }
        }
    }
}

#[test]
fn invertibility_alone_does_not_bypass_conservative_admission_or_fill_holes() {
    let p = [Vec3d::new(-10., 0., -10.), Vec3d::new(10., 0., -10.),
             Vec3d::new(10., 0., 10.), Vec3d::new(-10., 0., 10.)];
    let t = [[0, 1, 2], [0, 2, 3]];
    let x = Vec3d::new(1., 0., 0.); let y = Vec3d::new(0., 1., 0.); let z = Vec3d::new(0., 0., 1.);
    assert!(prepare(&p, &t, Vec3d::ZERO, [x*2., y, z]).is_none());
    assert!(prepare(&p, &t, Vec3d::ZERO, [x, x, z]).is_none());
    assert!(prepare(&p, &t, Vec3d::ZERO, [x, y, Vec3d::new(0., 0., f64::NAN)]).is_none());
    let ring = [p[0], p[1], p[2], p[3], Vec3d::new(-2.,0.,-2.),
                Vec3d::new(2.,0.,-2.), Vec3d::new(2.,0.,2.), Vec3d::new(-2.,0.,2.)];
    let faces = [[0,1,5],[0,5,4],[1,2,6],[1,6,5],
                 [2,3,7],[2,7,6],[3,0,4],[3,4,7]];
    assert!(prepare(&ring, &faces, Vec3d::ZERO, [x,y,z]).is_none());
    let tiny = p.map(|point| point*2_f64.powi(-140));
    assert!(prepare(&tiny, &t, Vec3d::ZERO, [x,y,z]).is_none(),
            "a positive subnormal half extent cannot survive inward FTZ-safe packing");
}
