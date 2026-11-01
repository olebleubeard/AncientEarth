//! Unregistered immutable canonical terrain-face certificates.
//!
//! A fixed patch uses the SAME exact binary64 rows/origin as its packed BVH.
//! It need not be orthonormal: a positive interval determinant proves the
//! coordinate map nonsingular and orientation preserving. Canonical source
//! ECEF points are never recovered from quantized display vertices.
//!
//! Only nine vec4 records per patch depend on the eye. Face records depend on
//! source vertices and patch identity, so a camera move does not rebuild them.
//! This host representation does not grant a GPU contact or numerical budget.
#[path="optical_taylor.rs"]
mod arithmetic;
#[path="optical_normal_ideal_frame.rs"]
mod ideal_frame;
use arithmetic::Interval as I;
use earth_math::rendering::LocalFrame;
pub use ideal_frame::IdealFrame;
pub const FACE_VEC4:usize=7;
pub const EYE_VEC4:usize=9;
pub type VectorBounds=[[f64;2];3];
#[derive(Clone,Copy,Debug)]
pub struct PatchBasis{origin:[f64;3],rows:[[f64;3];3],determinant:[f64;2]}
#[derive(Clone,Copy,Debug)]
pub struct FaceCertificate{pub words:[[f32;4];FACE_VEC4]}
#[derive(Clone,Copy,Debug)]
pub struct EyeCertificate{pub words:[[f32;4];EYE_VEC4]}
fn interval(v:[f64;2])->I{I{lo:v[0],hi:v[1]}}
fn dot(a:[I;3],b:[I;3])->I{a.into_iter().zip(b).fold(I::point(0.),|s,(x,y)|s+x*y)}
fn cross(a:[I;3],b:[I;3])->[I;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn difference(a:[f64;3],b:[f64;3])->[I;3]{std::array::from_fn(|i|I::point(a[i])-I::point(b[i]))}
fn pack(v:I)->Result<[f32;2],String>{
    if !v.valid(){return Err("Invalid source/frame enclosure".into());}
    let mut lo=v.lo as f32;let mut hi=v.hi as f32;
    if !lo.is_finite()||!hi.is_finite(){return Err("Source/frame enclosure exceeds f32 range".into());}
    if f64::from(lo)>v.lo{lo=lo.next_down();}if f64::from(hi)<v.hi{hi=hi.next_up();}
    // Include both signed FTZ behavior and the exact nonzero real endpoint.
    if v.lo<0.&&lo.abs()<f32::MIN_POSITIVE{lo=-f32::MIN_POSITIVE;}
    if v.lo>=0.&&lo!=0.&&lo<f32::MIN_POSITIVE{lo=0.;}
    if v.hi>0.&&hi.abs()<f32::MIN_POSITIVE{hi=f32::MIN_POSITIVE;}
    if v.hi<=0.&&hi!=0.&&hi.abs()<f32::MIN_POSITIVE{hi=0.;}
    if !lo.is_finite()||!hi.is_finite()||f64::from(lo)>v.lo||f64::from(hi)<v.hi{return Err("Invalid outward f32 packing".into());}
    Ok([lo,hi])
}
fn pair(v:[I;3])->Result<[[f32;4];2],String>{
    let p=[pack(v[0])?,pack(v[1])?,pack(v[2])?];
    Ok([[p[0][0],p[1][0],p[2][0],0.],[p[0][1],p[1][1],p[2][1],0.]])
}
fn split(v:[I;3])->Result<[[f32;4];3],String>{
    if v.iter().any(|x|!x.valid()){return Err("Invalid split source/frame enclosure".into());}
    // The chosen high word is a coordinate origin, not an exact midpoint.
    // Its complete residual is independently enclosed; no low bit is dropped.
    let high=v.map(|x|{
        let chosen=(0.5*x.lo+0.5*x.hi)as f32;
        // A stored subnormal origin could flush independently when loaded by
        // the GPU. Choose exact zero instead and retain the entire small
        // coordinate in the separately outward, FTZ-safe residual.
        if chosen.abs()<f32::MIN_POSITIVE{0.0}else{chosen}
    });
    if high.iter().any(|x|!x.is_finite()){return Err("Nonfinite split coordinate origin".into());}
    let residual=std::array::from_fn(|i|v[i]-I::point(f64::from(high[i])));
    let low=pair(residual)?;
    Ok([[high[0],high[1],high[2],0.],low[0],low[1]])
}
fn packed_pair(words:&[[f32;4]],offset:usize)->[I;3]{
    std::array::from_fn(|i|I{lo:f64::from(words[offset][i]),hi:f64::from(words[offset+1][i])})
}
impl PatchBasis{
    pub fn origin(self)->[f64;3]{self.origin}
    pub fn rows(self)->[[f64;3];3]{self.rows}
    pub fn determinant(self)->[f64;2]{self.determinant}
    pub fn new(origin:[f64;3],rows:[[f64;3];3])->Result<Self,String>{
        if origin.iter().chain(rows.iter().flatten()).any(|v|!v.is_finite()){return Err("Nonfinite source patch frame".into());}
        let r=rows.map(|v|v.map(I::point));let d=dot(r[0],cross(r[1],r[2]));
        if !d.valid()||d.lo<=0.{return Err("Canonical patch coordinate map must have certified positive determinant".into());}
        Ok(Self{origin,rows,determinant:[d.lo,d.hi]})
    }
    pub fn from_frame(frame:LocalFrame)->Result<Self,String>{
        let xyz=|v:earth_math::Vec3d|[v.x,v.y,v.z];
        Self::new(xyz(frame.origin),[xyz(frame.basis.east),xyz(frame.basis.up),xyz(-frame.basis.north)])
    }
    fn project(self,v:[I;3])->[I;3]{self.rows.map(|r|dot(r.map(I::point),v))}
    /// Static face ABI:0 negativeA high;1..2 negativeA residual lower/upper;
    ///3..4 canonical B−A lower/upper;5..6 canonical C−A lower/upper.
    /// All coordinates use the fixed patch rows and exact source vertices.
    pub fn face(self,vertices:[[f64;3];3])->Result<FaceCertificate,String>{
        if vertices.iter().flatten().any(|v|!v.is_finite()){return Err("Nonfinite canonical source face".into());}
        let [a,b,c]=vertices;let negative_a=self.project(difference(self.origin,a));
        let origin=split(negative_a)?;let ba=pair(self.project(difference(b,a)))?;let ca=pair(self.project(difference(c,a)))?;
        Ok(FaceCertificate{words:[origin[0],origin[1],origin[2],ba[0],ba[1],ca[0],ca[1]]})
    }
    /// Dynamic eye ABI:0 eye−patch-origin high;1..2 residual lower/upper;
    ///3..8 three eye-to-patch matrix rows, each lower xyz then upper xyz.
    /// The exact eye/basis convention is shared with the canonical curve map.
    pub fn eye(self,eye:IdealFrame)->Result<EyeCertificate,String>{
        let offset=std::array::from_fn(|i|interval(eye.origin_ecef[i])-I::point(self.origin[i]));
        let shift=split(self.project(offset))?;
        let mut words=[[0.;4];EYE_VEC4];words[..3].copy_from_slice(&shift);
        for i in 0..3{
            let row=self.rows[i].map(I::point);
            let matrix=eye.columns.map(|column|dot(row,column.map(interval)));
            words[3+2*i..5+2*i].copy_from_slice(&pair(matrix)?);
        }
        Ok(EyeCertificate{words})
    }
}
/// CPU enclosure of the consumer operation, for independent fixture use.
/// Production WGSL must use its own reviewed outward f32 operations. Fold
/// the two exact high coordinates BEFORE adding either residual to preserve
/// useful cancellation; never expand a split source coordinate prematurely.
pub fn face_frame(eye:EyeCertificate,face:FaceCertificate)->(VectorBounds,[VectorBounds;3],VectorBounds,VectorBounds){
    let eye_low=packed_pair(&eye.words,1);let face_low=packed_pair(&face.words,1);
    let shift=std::array::from_fn(|i|{
        let high=I::point(f64::from(eye.words[0][i]))+I::point(f64::from(face.words[0][i]));
        let v=high+eye_low[i]+face_low[i];[v.lo,v.hi]
    });
    let rows=std::array::from_fn(|i|packed_pair(&eye.words,3+2*i).map(|v|[v.lo,v.hi]));
    let b=packed_pair(&face.words,3).map(|v|[v.lo,v.hi]);let c=packed_pair(&face.words,5).map(|v|[v.lo,v.hi]);
    (shift,rows,b,c)
}

#[cfg(test)]
#[path="optical_patch_certificates_review.rs"]
mod independent_review;

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn fixed_faces_survive_eye_motion_and_split_large_origin(){
        let origin=[6378137.,100000.,-200000.];
        let patch=PatchBasis::new(origin,[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]).unwrap();
        let a=[origin[0]+12345.123456789,origin[1]-23456.23456789,origin[2]+10.];
        let vertices=[a,[a[0]+3.,a[1]+1.,a[2]],[a[0],a[1]+2.,a[2]+4.]];
        let face=patch.face(vertices).unwrap();
        for offset in [0.,0.01,1000.]{
            let o=std::array::from_fn(|i|[a[i]+offset,a[i]+offset]);
            let eye=IdealFrame{origin_ecef:o,columns:[[[1.,1.],[0.,0.],[0.,0.]],[[0.,0.],[1.,1.],[0.,0.]],[[0.,0.],[0.,0.],[1.,1.]]]};
            let (shift,_,b,c)=face_frame(patch.eye(eye).unwrap(),face);
            for i in 0..3{let exact=(a[i]+offset)-a[i];assert!(shift[i][0]<=exact&&shift[i][1]>=exact);assert!(shift[i][1]-shift[i][0]<1e-6);}
            for (v,truth) in [(b,[3.,1.,0.]),(c,[0.,2.,4.])]{for i in 0..3{assert!(v[i][0]<=truth[i]&&v[i][1]>=truth[i]);}}
            assert_eq!(face.words,patch.face(vertices).unwrap().words);
        }
    }
    #[test]fn basis_invertibility_is_proved_not_assumed(){
        assert!(PatchBasis::new([0.;3],[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]).is_ok());
        assert!(PatchBasis::new([0.;3],[[1.,0.,0.],[0.,1.,0.],[0.,0.,-1.]]).is_err());
        assert!(PatchBasis::new([0.;3],[[1.,0.,0.],[1.,0.,0.],[0.,0.,1.]]).is_err());
        assert!(PatchBasis::new([0.;3],[[1.,1e-10,0.],[0.,1.,0.],[0.,0.,1.]]).is_ok());
    }
    #[test]fn split_high_never_depends_on_subnormal_preservation(){
        let tiny=f64::from(f32::from_bits(17));
        let packed=split([I::point(tiny),I::point(-tiny),I::point(0.)]).unwrap();
        assert_eq!(packed[0],[0.;4]);
        for (axis,truth) in [tiny,-tiny,0.].into_iter().enumerate(){
            assert!(f64::from(packed[1][axis])<=truth&&truth<=f64::from(packed[2][axis]));
            for row in &packed{assert!(row[axis]==0.||row[axis].abs()>=f32::MIN_POSITIVE);}
        }
    }
}
