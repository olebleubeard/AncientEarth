//! Independent finite analytic checks for the unused common-height transport.
//! Included as a child of optical_shared_transport; no renderer/runtime use.
use super::*;

fn polynomial_interval(value:T,x:f64,y:f64)->I{
    optical_taylor::POWERS.iter().zip(value.c).fold(I::point(0.),|sum,(&(a,b),c)|{
        let xp=(0..a).fold(I::point(1.),|v,_|v*I::point(x));
        let yp=(0..b).fold(I::point(1.),|v,_|v*I::point(y));
        sum+I::point(c)*xp*yp
    })
}
fn contains(value:I,truth:f64){assert!(value.valid()&&value.lo<=truth&&truth<=value.hi,"{value:?} misses {truth:.17e}");}

#[test]
fn analytic_radial_normal_rays_cover_dense_and_normalized_endpoint(){
    // Along a fixed ellipsoid normal u=n, both curvature and refractive bend
    // vanish. Thus chart/u are constant, ds/dH=1 and d(excess)/dH=N(H).
    // The dyadic linear field integrates exactly without an ODE oracle.
    let n=2_f64.powi(-12);let g=-2_f64.powi(-30);
    let axes=[[0.,0.,-1.],[0.,1.,0.],[0.,0.6,-0.8]];
    for axis in axes{for h in [-128.,0.,4096.]{for step in [8.,64.,512.]{
        let initial=[k(0.),k(0.),k(0.),k(1.),k(0.),T::variable(0,16.,0.25),T::variable(1,0.125,2_f64.powi(-12))];
        let branch=Branch{anchor:0.,n,g:k(g)};
        let result=trial(axis.map(k),h,initial,[0.;4],h+step,branch).expect("radial trial");
        assert!(result.ode_error.is_finite()&&result.representation.is_finite());
        for x in [-1.,0.,1.]{for y in [-1.,0.,1.]{
            for theta in [0.,0.125,0.5,0.875,1.]{
                let dh=theta*step;
                let exact=[0.,0.,0.,1.,0.,16.+0.25*x+dh,
                    0.125+2_f64.powi(-12)*y+n*dh+g*(h*dh+0.5*dh*dh)];
                let t=I::point(theta);let one=I::point(1.);
                for i in 0..7{
                    let r=result.r.map(|r|polynomial_interval(r[i],x,y));
                    let represented=polynomial_interval(initial[i],x,y)+t*r[0]
                        +t*(one-t)*(r[1]+t*(r[2]+(one-t)*r[3]));
                    let error=result.dense_error[GROUP[i]];
                    contains(represented+I{lo:-error,hi:error},exact[i]);
                    if theta==1.{
                        let error=result.error[GROUP[i]];
                        contains(polynomial_interval(result.next[i],x,y)+I{lo:-error,hi:error},exact[i]);
                    }
                }
            }
        }}
    }}}
}

#[test]
fn radial_path_and_excess_uncertainty_survive_while_invalid_groups_decline(){
    let axis=[k(0.),k(0.),k(-1.)];
    let y=[k(0.),k(0.),k(0.),k(1.),k(0.),k(32.),k(0.25)];
    let b=Branch{anchor:0.,n:2_f64.powi(-12),g:k(0.)};
    let incoming=[0.,0.,0.125,0.0625];
    let result=trial(axis,0.,y,incoming,64.,b).unwrap();
    assert!(result.error[2]>=incoming[2]&&result.error[3]>=incoming[3]);
    assert!(result.dense_error[2]>=incoming[2]&&result.dense_error[3]>=incoming[3]);
    for invalid in [[0.,0.,f64::INFINITY,0.],[0.,0.,0.,f64::NAN],[0.,-1.,0.,0.]]{
        assert!(trial(axis,0.,y,invalid,64.,b).is_err());
        assert!(add_errors([0.;4],invalid).iter().all(|v|*v==f64::INFINITY));
    }
    assert!(add_errors([f64::MAX;4],[f64::MAX;4]).iter().all(|v|*v==f64::INFINITY));
    for invalid in [I::invalid(),I{lo:0.,hi:f64::INFINITY},I{lo:-2.,hi:-1.}]{
        for index in 0..3{
            let mut parts=[I::point(1.);3];parts[index]=invalid;
            assert_eq!(positive_metric(parts),f64::INFINITY);
        }
    }
}
