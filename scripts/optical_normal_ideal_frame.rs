//! Original MIT. Unused canonical scene-frame intervals.
//!
//! Define one EXACT orthonormal frame from the existing binary64 trigonometric
//! outputs, rather than pretending the stored EnuBasis matrix is orthogonal.
//! Its polar axis is precisely the current map packet's normalized
//! [east.z, up.z, -north.z], since EnuBasis::at has east.z exactly zero.
//! This is a canonical representation convention, not an astronomy/frame change.
#[path = "optical_taylor.rs"]
mod arithmetic;
use arithmetic::Interval as I;
use earth_math::{EnuBasis, Geodetic, WGS84_A_M as WGS84_A, WGS84_E2};

pub type Bounds = [f64; 2];
pub type VectorBounds = [Bounds; 3];

#[derive(Clone, Copy, Debug)]
pub struct IdealFrame {
    /// E, U, -N columns of the local-to-ECEF orthogonal matrix.
    pub columns: [VectorBounds; 3],
    /// Canonical F(normal-at-eye, exact supplied H), rather than an assumed
    /// bit-exact inverse conversion of a separately supplied ECEF eye.
    pub origin_ecef: VectorBounds,
}
fn bounds(x: I) -> Bounds { [x.lo,x.hi] }
fn interval(x: Bounds) -> I { I{lo:x[0],hi:x[1]} }
fn dot(a:[I;3],b:[I;3])->I{
    a.into_iter().zip(b).fold(I::point(0.),|sum,(x,y)|sum+x*y)
}
impl IdealFrame {
    pub fn from_eye(eye:Geodetic)->Result<Self,String>{
        if !eye.lat_deg.is_finite()||!(-90.0..=90.0).contains(&eye.lat_deg)
            ||!eye.lon_deg.is_finite()||!eye.height_m.is_finite()
            ||!(-10000.0..=120001.0).contains(&eye.height_m){
            return Err("Ideal scene eye outside finite normal tube".into());
        }
        let raw=EnuBasis::at(eye);
        if raw.east.z!=0. {return Err("Canonical scene basis requires horizontal east".into());}
        let ex=I::point(raw.east.x);let ey=I::point(raw.east.y);
        let lon_norm=(ex*ex+ey*ey).sqrt();
        let sin=I::point(raw.up.z);let cos=I::point(raw.north.z);
        let lat_norm=(sin*sin+cos*cos).sqrt();
        if !lon_norm.valid()||!lat_norm.valid()||lon_norm.lo<0.999999||lat_norm.lo<0.999999{
            return Err("Degenerate canonical scene frame".into());
        }
        let zero=I::point(0.);let east=[ex/lon_norm,ey/lon_norm,zero];
        let radial=[east[1],-east[0],zero];
        let s=sin/lat_norm;let c=cos/lat_norm;
        let up=[c*radial[0],c*radial[1],s];
        let minus_north=[s*radial[0],s*radial[1],-c];
        let radius=I::point(WGS84_A)/(I::point(1.)-I::point(WGS84_E2)*s*s).sqrt();
        let h=I::point(eye.height_m);
        let mut origin=up.map(|x|(radius+h)*x);
        origin[2]=origin[2]-radius*I::point(WGS84_E2)*s;
        if east.into_iter().chain(up).chain(minus_north).chain(origin).any(|x|!x.valid()){
            return Err("Nonfinite ideal scene frame".into());
        }
        Ok(Self{columns:[east.map(bounds),up.map(bounds),minus_north.map(bounds)],origin_ecef:origin.map(bounds)})
    }
    /// Interval local components of a supplied canonical ECEF vector.
    pub fn project(self,vector:VectorBounds)->VectorBounds{
        let v=vector.map(interval);
        self.columns.map(|column|bounds(dot(column.map(interval),v)))
    }
    /// Canonical eye-minus-point in this same ideal local frame. Source points
    /// are exact binary64 coordinates; source model error is a separate issue.
    pub fn eye_minus_point(self,point:[f64;3])->VectorBounds{
        let difference=std::array::from_fn(|i|bounds(interval(self.origin_ecef[i])-I::point(point[i])));
        self.project(difference)
    }
    /// Enclose canonical mapped eye minus separately supplied ECEF eye.
    pub fn origin_residual(self,eye_ecef:[f64;3])->VectorBounds{
        std::array::from_fn(|i|bounds(interval(self.origin_ecef[i])-I::point(eye_ecef[i])))
    }
}

#[cfg(test)]mod tests{
    use super::*;
    use earth_math::{geodetic_to_ecef,Vec3d};
    fn components(v:Vec3d)->[f64;3]{[v.x,v.y,v.z]}
    #[test]fn ideal_frame_and_origin_enclosures_include_known_equatorial_case(){
        let frame=IdealFrame::from_eye(Geodetic::new(0.,0.,100.).unwrap()).unwrap();
        let expected=[[0.,1.,0.],[1.,0.,0.],[0.,0.,-1.]];
        for i in 0..3{for j in 0..3{assert!(frame.columns[i][j][0]<=expected[i][j]&&frame.columns[i][j][1]>=expected[i][j]);}}
        let origin=[WGS84_A+100.,0.,0.];
        for i in 0..3{assert!(frame.origin_ecef[i][0]<=origin[i]&&frame.origin_ecef[i][1]>=origin[i]);}
    }
    #[test]fn finite_frames_poles_and_roundtrip_residual_are_small_not_assumed_zero(){
        for lat in [-90.,-70.,0.,29.305549,89.,90.]{for lon in [-180.,-94.767015,0.,179.9]{
            let eye=Geodetic::new(lat,lon,-26.00552287325263).unwrap();
            let frame=IdealFrame::from_eye(eye).unwrap();
            for interval in frame.columns.into_iter().flatten().chain(frame.origin_ecef){assert!(interval[0]<=interval[1]);}
            let residual=frame.origin_residual(components(geodetic_to_ecef(eye)));
            for component in residual{assert!(component[0].abs().max(component[1].abs())<0.000001);}
        }}
        assert!(IdealFrame::from_eye(Geodetic::new(0.,0.,1e8).unwrap()).is_err());
    }
}
