//! Original MIT. Independent finite checks for unused interval AD transport.
#[path="optical_taylor.rs"] mod optical_taylor;
#[path="optical_shared_jacobian.rs"] mod jacobian;
use jacobian::{Jet,transport,transport_correction};
use optical_taylor::Interval as I;

#[cfg(test)] mod review {
    use super::*;
    fn encloses(i:I,x:f64){assert!(i.valid()&&i.lo<=x&&x<=i.hi,"{i:?} does not enclose {x}");}
    fn norm(x:&[f64])->f64{x.iter().map(|v|v*v).sum::<f64>().sqrt()}
    #[test]
    fn analytic_multivariate_partials_and_values_are_enclosed(){
        let ranges=[(1.,2.),(0.25,0.75),(2.,3.),(4.,5.),(-0.5,0.5),(0.,1.),(0.,1.)];
        let x:[Jet;7]=std::array::from_fn(|i|Jet::variable(I{lo:ranges[i].0,hi:ranges[i].1},i));
        let f=x[0]*x[2]+x[1]/x[3]+(Jet::constant(1.)+x[4]*x[4]).inverse_sqrt();
        let g=x[0]/(x[2]+x[3])+x[1]*x[1]*x[4];
        for code in 0..243{
            let mut n=code;let mut v=[0.;5];
            for j in 0..5{v[j]=ranges[j].0+(ranges[j].1-ranges[j].0)*(n%3)as f64/2.;n/=3;}
            let [a,b,c,d,e]=v;let square=1.+e*e;
            encloses(f.value,a*c+b/d+square.sqrt().recip());
            encloses(g.value,a/(c+d)+b*b*e);
            let df=[c,1./d,a,-b/(d*d),-e/(square* square.sqrt()),0.,0.];
            let dg=[1./(c+d),2.*b*e,-a/(c+d).powi(2),-a/(c+d).powi(2),b*b,0.,0.];
            for j in 0..7{encloses(f.derivative[j],df[j]);encloses(g.derivative[j],dg[j]);}
        }
    }
    fn real_map(x:[f64;7])->[f64;7]{
        [x[0]+0.1*x[2]*x[3], x[1]+0.2*x[4]/(2.+x[0]),
         x[2]+0.3*x[0]*x[1],x[3]+0.1*x[4]*(1.+x[1]*x[1]).sqrt().recip(),
         x[4]-0.25*x[0]*x[2],x[5]+0.5/(2.+x[3]),x[6]+0.1*x[1]/(2.+x[4])]
    }
    #[test]
    fn grouped_mean_value_transport_contains_finite_nonlinear_perturbations(){
        let center=[0.1,-0.2,0.3,0.9,-0.1,100.,0.05];
        let incoming=[0.003,0.007,0.011,0.013];
        let component_radius=[incoming[0],incoming[0],incoming[1],incoming[1],incoming[1],incoming[2],incoming[3]];
        let x:[Jet;7]=std::array::from_fn(|i|Jet::variable(I{lo:center[i]-component_radius[i],hi:center[i]+component_radius[i]},i));
        let k=Jet::constant;
        let map=[x[0]+k(0.1)*x[2]*x[3],x[1]+k(0.2)*x[4]/(k(2.)+x[0]),
            x[2]+k(0.3)*x[0]*x[1],x[3]+k(0.1)*x[4]*(k(1.)+x[1]*x[1]).inverse_sqrt(),
            x[4]-k(0.25)*x[0]*x[2],x[5]+k(0.5)/(k(2.)+x[3]),x[6]+k(0.1)*x[1]/(k(2.)+x[4])];
        let bounds=transport(&map,incoming);assert!(bounds.iter().all(|x|x.is_finite()&&*x>0.));
        let corrections=std::array::from_fn(|i|map[i]-x[i]);
        let correction_bounds=transport_correction(&corrections,incoming);
        assert!(correction_bounds.iter().all(|x|x.is_finite()&&*x>0.));
        assert_eq!(transport_correction(&[Jet::constant(0.);7],incoming),[0.;4]);
        let original=real_map(center);let blocks=[(0,2),(2,5),(5,6),(6,7)];
        for mask in 0..128{
            let mut perturbed=center;
            for (group,&(lo,hi)) in blocks.iter().enumerate(){
                let amplitude=0.99*incoming[group]/((hi-lo)as f64).sqrt();
                for j in lo..hi{perturbed[j]+=if mask&(1<<j)==0{amplitude}else{-amplitude};}
            }
            let actual=real_map(perturbed);let error:Vec<_>=actual.iter().zip(original).map(|(a,b)|a-b).collect();
            let correction_error:Vec<_>=(0..7).map(|i|(actual[i]-perturbed[i])-(original[i]-center[i])).collect();
            for (group,&(lo,hi)) in blocks.iter().enumerate(){
                assert!(norm(&error[lo..hi])<=bounds[group]);
                assert!(norm(&correction_error[lo..hi])<=correction_bounds[group]);
            }
        }
    }
    #[test]
    fn invalid_transport_and_singular_domains_fail_closed(){
        let map:[Jet;7]=std::array::from_fn(|i|Jet::variable(I::point(0.),i));
        for incoming in [[-1.,0.,0.,0.],[f64::NAN,0.,0.,0.],[0.,f64::INFINITY,0.,0.]]{
            assert!(transport(&map,incoming).iter().all(|x|*x==f64::INFINITY));
        }
        let mut invalid=map;invalid[2].derivative[4]=I::invalid();
        assert!(transport(&invalid,[1.;4]).iter().all(|x|*x==f64::INFINITY));
        assert!(!(Jet::constant(-1.).inverse_sqrt()).valid());
        assert!(!(Jet::constant(1.)/Jet::interval(I{lo:-1.,hi:1.})).valid());
        invalid=map;invalid[0].derivative[0]=I::point(1e308);
        assert!(transport(&invalid,[1.;4]).iter().any(|x|*x==f64::INFINITY));
    }
    #[test]
    fn normalization_uses_both_raw_norms(){
        for a in [[1.,0.,0.],[0.01,0.02,-0.03],[2.,-3.,4.]]{
            for b in [[-1.,0.,0.],[0.011,-0.021,0.032],[3.,2.,-1.]]{
                let an=norm(&a);let bn=norm(&b);let minimum=an.min(bn);
                let raw:Vec<_>=a.iter().zip(b).map(|(x,y)|x-y).collect();
                let unit:Vec<_>=a.iter().zip(b).map(|(x,y)|x/an-y/bn).collect();
                assert!(norm(&unit)<=norm(&raw)/minimum*(1.+1e-14));
                let identity=(an-bn).powi(2)+an*bn*norm(&unit).powi(2);
                assert!((norm(&raw).powi(2)-identity).abs()<1e-12);
            }
        }
    }
}
