//! Independent analytic checks of composite jets and Hermite remainder bounds.
use super::*;

fn contains(interval: I, value: f64) {
    assert!(interval.valid());
    assert!(
        interval.lo <= value && value <= interval.hi,
        "{interval:?} excludes {value:.17e}"
    );
}

#[test]
fn composite_inverse_root_jets_enclose_known_rational_derivatives() {
    // sqrt((2+t)^2)=2+t and sqrt((2+t)^4)=(2+t)^2 on this domain.
    // Their inverse derivatives are independent closed-form rational functions.
    for range in [
        I { lo: -0.5, hi: 0.5 },
        I { lo: 0., hi: 0.75 },
        I { lo: 1., hi: 2. },
    ] {
        let base = k(2.) + Jet::variable(range);
        let square = base * base;
        for (power, jet) in [
            (1, square.inverse_sqrt()),
            (2, (square * square).inverse_sqrt()),
        ] {
            for j in 0..=32 {
                let t = range.lo + (range.hi - range.lo) * j as f64 / 32.;
                for n in 0..5 {
                    let factor = if power == 1 { 1. } else { (n + 1) as f64 };
                    let expected =
                        if n % 2 == 0 { factor } else { -factor } / (2. + t).powi(n as i32 + power);
                    contains(jet.0[n], expected);
                }
            }
        }
    }
    for invalid in [I { lo: -1., hi: 1. }, I::point(0.), I::invalid()] {
        assert!(!Jet::variable(invalid).inverse_sqrt().valid());
    }
}

#[test]
fn spherical_chart_fourth_derivatives_match_independent_cartesian_formula() {
    let radius = 6378137. + 1000.;
    let curve = Curve {
        start: [0., 0.1, 0.],
        r: [[0., 0.2, 0.], [0.; 3], [0.; 3], [0.; 3]],
        anchor: I::point(1000.),
        eye: I::point(0.),
        axis: [I::point(0.), I::point(1.), I::point(0.)],
        eccentricity: I::point(0.),
    };
    for range in [
        I { lo: 0., hi: 1. },
        I { lo: 0.25, hi: 0.5 },
        I { lo: 0.75, hi: 1. },
    ] {
        let jets = curve.position_jet(range).unwrap();
        for j in 0..=32 {
            let theta = range.lo + (range.hi - range.lo) * j as f64 / 32.;
            let t = 0.1 + 0.2 * theta;
            let denominator = (1. + t * t).powf(4.5);
            let x4 = 15. * t * (3. - 4. * t * t) / denominator;
            let y4 = (9. - 72. * t * t + 24. * t.powi(4)) / denominator;
            contains(jets[0].0[4], radius * 0.2_f64.powi(4) * x4 / 24.);
            contains(jets[1].0[4], radius * 0.2_f64.powi(4) * y4 / 24.);
            contains(jets[2].0[4], 0.);
        }
        let certificate = certify(curve, range).unwrap();
        assert!(certificate.total_map_fit >= certificate.endpoint_uncertainty);
    }
}

#[test]
fn hermite_position_constant_is_sharp_and_endpoint_errors_remain_separate() {
    // Independent quartic truth f=t^4, endpoint Hermite H=2t^3-t^2.
    for j in 0..=64 {
        let t = j as f64 / 64.;
        let error = t.powi(4) - (2. * t.powi(3) - t * t);
        assert_eq!(error, t * t * (1. - t) * (1. - t));
        assert!(error <= 24. / 384.);
    }
    assert_eq!(0.5_f64.powi(4) - (2. * 0.5_f64.powi(3) - 0.25), 24. / 384.);
    for sign_bits in 0..16 {
        let signs: [f64; 4] =
            std::array::from_fn(|i| if sign_bits & (1 << i) == 0 { -1. } else { 1. });
        for width in [0.125, 0.5, 1.] {
            for j in 0..=64 {
                let t = j as f64 / 64.;
                let h00 = 2. * t.powi(3) - 3. * t * t + 1.;
                let h01 = 1. - h00;
                let h10 = t * (1. - t) * (1. - t);
                let h11 = -t * t * (1. - t);
                let error = h00 * signs[0] * 0.25
                    + h01 * signs[1] * 0.25
                    + width * (h10 * signs[2] * 0.5 + h11 * signs[3] * 0.5);
                assert!(error.abs() <= 0.25 + width * 0.5 / 4.);
            }
        }
    }
}

#[test]
fn derivative_remainder_nearer_endpoint_bound_covers_independent_polynomials() {
    // f(t)=t^degree on [0,L]. This checks derivative error directly; it
    // deliberately does not differentiate the unknown point in a value remainder.
    for degree in 4_i32..=10 {
        for width in [0.125_f64, 0.5, 1.] {
            let m4=(degree*(degree-1)*(degree-2)*(degree-3)) as f64*width.powi(degree-4);
            let bound=m4*width.powi(3)*5./36.;
            for j in 0..=64 {
                let x=j as f64/64.;
                let t=width*x;
                let h01_prime=6.*x*(1.-x);
                let h11_prime=3.*x*x-2.*x;
                let cubic_prime=h01_prime*width.powi(degree)/width
                    +h11_prime*(degree as f64)*width.powi(degree-1);
                let truth_prime=(degree as f64)*t.powi(degree-1);
                assert!((truth_prime-cubic_prime).abs()<=bound);
                let near=x.min(1.-x);
                assert!(near.powi(3)/6.<=1./48.);
                assert!(6.*near*(1.-near)/24.<=1./16.);
                assert!((3.*near*near-2.*near).abs()/6.<=1./18.);
            }
        }
    }
}

#[test]
fn scalar_majorant_bounds_independent_spherical_fourth_derivatives() {
    let radius=6378137.+1000.;
    let curve=Curve {start:[0.,0.1,0.],r:[[0.,0.2,0.],[0.;3],[0.;3],[0.;3]],
        anchor:I::point(1000.),eye:I::point(0.),axis:[I::point(0.),I::point(1.),I::point(0.)],
        eccentricity:I::point(0.)};
    for range in [I{lo:0.,hi:1.},I{lo:0.25,hi:0.5},I{lo:0.75,hi:1.}] {
        let bound=curve.fourth_coefficient_upper(range).unwrap();
        assert!(bound.is_finite() && bound>=0.);
        for j in 0..=64 {
            let theta=range.lo+(range.hi-range.lo)*j as f64/64.;let t=0.1+0.2*theta;
            let denominator=(1.+t*t).powf(4.5);
            let x4=15.*t*(3.-4.*t*t)/denominator;
            let y4=(9.-72.*t*t+24.*t.powi(4))/denominator;
            let exact_norm=radius*0.2_f64.powi(4)*(x4*x4+y4*y4).sqrt()/24.;
            assert!(exact_norm<=bound);
        }
    }
    let linear=Curve {start:[0.;3],r:[[100.,0.,0.],[0.;3],[0.;3],[0.;3]],..curve};
    assert_eq!(linear.fourth_coefficient_upper(I{lo:0.,hi:1.}),Some(0.));
    let refused=Curve {start:[0.,0.6,0.6],..curve};
    assert!(refused.fourth_coefficient_upper(I{lo:0.,hi:1.}).is_none());
}

#[test]
fn split_packet_preserves_control_intervals_derivative_units_and_signed_tiny_bounds() {
    let curve=Curve {start:[0.;3],r:[[100.,0.,0.],[0.;3],[0.;3],[0.;3]],
        anchor:I::point(0.),eye:I::point(0.),axis:[I::point(0.),I::point(1.),I::point(0.)],eccentricity:I::point(0.)};
    let mut certificate=certify(curve,I{lo:0.25,hi:0.5}).unwrap();
    let centers=[1048576.125,-2097152.0625,0.];
    for j in 0..4 {for i in 0..3 {
        let value=centers[i]+(j as f64-1.)/1024.;
        certificate.controls[j][i]=I{lo:value-2_f64.powi(-20),hi:value+2_f64.powi(-20)};
    }}
    certificate.endpoint_positions=[certificate.controls[0],certificate.controls[3]];
    certificate.component_fit=[2_f64.powi(-150),0.,2_f64.powi(-20)];
    let packed=pack_certificate(&certificate).unwrap();
    assert_eq!(packed.position_remainder[0],f32::MIN_POSITIVE);
    assert_eq!(packed.position_remainder[1],0.);
    for j in 0..4 {for i in 0..3 {
        // These manufactured dyadic values sum exactly in binary64.
        let origin=f64::from(packed.origin_hi[i])+f64::from(packed.origin_lo[i]);
        assert!(origin+packed.controls[j][i].lo<=certificate.controls[j][i].lo);
        assert!(origin+packed.controls[j][i].hi>=certificate.controls[j][i].hi);
    }}
    for j in 0..3 {for i in 0..3 {
        assert!(packed.derivative_controls[j][i].lo<=certificate.derivative_controls[j][i].lo);
        assert!(packed.derivative_controls[j][i].hi>=certificate.derivative_controls[j][i].hi);
    }}
    let tiny=2_f64.powi(-150);
    let negative=pack_interval(I::point(-tiny)).unwrap();
    assert_eq!(negative.lo,-f64::from(f32::MIN_POSITIVE));assert_eq!(negative.hi,0.);
    let mut words=Vec::new();append_piece(&mut words,&packed,I{lo:0.25,hi:0.5},7,1,4,true);
    assert_eq!(words.len(),80);
    assert_eq!(f(words[0]),0.25);assert_eq!(f(words[1]),0.5);assert_eq!(f(words[2]),4.);
    assert_eq!(&words[76..],[7,1,4,1]);
    assert!(f(words[3])>=packed.position_radius);
    for i in 0..3 {assert_eq!(words[4+i],packed.origin_hi[i].to_bits());assert_eq!(words[8+i],packed.origin_lo[i].to_bits());}
    for j in 0..3 {for i in 0..3 {assert_eq!(f(words[44+8*j+i]),packed.derivative_controls[j][i].lo);assert_eq!(f(words[48+8*j+i]),packed.derivative_controls[j][i].hi);}}
}
