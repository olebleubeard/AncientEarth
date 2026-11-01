//! Original MIT. Unused source-vertex anchor certificate.
//! An approximate inverse selects a nearby normal-coordinate point only.
//! Outward evaluation of that point minus the exact source vertex explicitly
//! retains the inverse/frame/roundtrip discrepancy. No inverse is certified by
//! a sample, and the residual must not be dropped by the GPU consumer.
#[path="optical_taylor.rs"]mod arithmetic;
#[path="optical_normal_ideal_frame.rs"]mod ideal_frame;
#[path="optical_normal_map_packet.rs"]mod map_packet;
use arithmetic::Interval as I;
use earth_math::{Vec3d,Geodetic,EnuBasis,ecef_to_geodetic,WGS84_A_M,WGS84_E2};

#[derive(Clone,Debug)]pub struct AnchorPacket{
    pub chart_hi:[f32;2],pub chart_lo:[f32;2],
    pub chart_residual:[[f32;2];2],
    pub height:[f32;2],pub height_residual:[f32;2],
    pub mapped_minus_vertex_lower:[f32;3],pub mapped_minus_vertex_upper:[f32;3],
    pub canonical_chart:[f64;2],pub canonical_height:f64,
}
fn i(b:[f64;2])->I{I{lo:b[0],hi:b[1]}}
fn pack(x:I)->Result<[f32;2],String>{
    // Same FTZ-aware outward endpoint policy as map_packet; the two modules
    // use distinct instances of the diagnostic Interval type.
    let mut lo=x.lo as f32;let mut hi=x.hi as f32;
    if !x.valid()||x.abs_upper()>1e8{return Err("Anchor interval outside packing domain".into());}
    if f64::from(lo)>x.lo{lo=lo.next_down();}if f64::from(hi)<x.hi{hi=hi.next_up();}
    if lo!=0.&&lo.abs()<f32::MIN_POSITIVE{lo=if lo<0.{-f32::MIN_POSITIVE}else{0.};}
    if hi!=0.&&hi.abs()<f32::MIN_POSITIVE{hi=if hi>0.{f32::MIN_POSITIVE}else{0.};}
    if x.lo<0.&&lo==0.{lo=-f32::MIN_POSITIVE;}if x.hi>0.&&hi==0.{hi=f32::MIN_POSITIVE;}
    Ok([lo,hi])
}
fn split(x:f64)->([f32;2],[f32;2]){
    let hi=x as f32;let lo=(x-f64::from(hi))as f32;
    let residual=map_packet::residual_for_split(x,[hi,lo]).expect("Validated finite anchor");
    ([hi,lo],residual)
}
impl AnchorPacket{
    pub fn from_vertex(eye:Geodetic,vertex:Vec3d)->Result<Self,String>{
        let frame=ideal_frame::IdealFrame::from_eye(eye)?;
        let approximate=ecef_to_geodetic(vertex).map_err(|_|"Invalid source vertex")?;
        let raw_normal=EnuBasis::at(approximate).up;
        let raw=[raw_normal.x,raw_normal.y,raw_normal.z];
        let local:[f64;3]=std::array::from_fn(|j|frame.columns[j].iter().zip(raw).map(|(x,y)|(x[0]+(x[1]-x[0])*0.5)*y).sum());
        if !(local[1]>0.5)||!local.iter().all(|x|x.is_finite()){
            return Err("Source anchor outside forward chart".into());
        }
        let chart=[local[0]/local[1],-local[2]/local[1]];
        if chart[0]*chart[0]+chart[1]*chart[1]>1.||!(-10000.0..=120001.0).contains(&approximate.height_m){
            return Err("Source anchor outside finite chart".into());
        }
        // The chosen binary64 chart/H values now define the EXACT anchor.
        // Any error of their approximate construction is captured below.
        let v=[I::point(chart[0]),I::point(1.),I::point(-chart[1])];
        let norm=v.into_iter().fold(I::point(0.),|sum,x|sum+x*x).sqrt();
        let n=v.map(|x|x/norm);
        let axis=frame.columns.map(|column|i(column[2]));
        let z=n.into_iter().zip(axis).fold(I::point(0.),|sum,(x,y)|sum+x*y);
        let e=I::point(WGS84_E2);
        let radius=I::point(WGS84_A_M)/(I::point(1.)-e*z*z).sqrt();
        let h=I::point(approximate.height_m);
        let mapped:[I;3]=std::array::from_fn(|j|radius*(n[j]-e*z*axis[j])+h*n[j]);
        let source=frame.project([[vertex.x,vertex.x],[vertex.y,vertex.y],[vertex.z,vertex.z]]).map(i);
        let residual:Vec<_>=(0..3).map(|j|pack(mapped[j]-source[j])).collect::<Result<_,_>>()?;
        let pairs=chart.map(split);let (height,height_residual)=split(approximate.height_m);
        Ok(Self{chart_hi:pairs.map(|x|x.0[0]),chart_lo:pairs.map(|x|x.0[1]),
            chart_residual:pairs.map(|x|x.1),height,height_residual,
            mapped_minus_vertex_lower:std::array::from_fn(|j|residual[j][0]),
            mapped_minus_vertex_upper:std::array::from_fn(|j|residual[j][1]),
            canonical_chart:chart,canonical_height:approximate.height_m})
    }
}
#[cfg(test)]mod tests{
    use super::*;use earth_math::geodetic_to_ecef;
    #[test]fn real_source_anchor_retains_small_explicit_residual(){
        let eye=Geodetic::new(29.305549,-94.767015,-26.00552287325263).unwrap();
        for location in [(29.3056,-94.7671,-27.),(30.,-95.,166.),(29.,-94.,-30.)]{
            let point=geodetic_to_ecef(Geodetic::new(location.0,location.1,location.2).unwrap());
            let p=AnchorPacket::from_vertex(eye,point).unwrap();
            for j in 0..3{
                assert!(p.mapped_minus_vertex_lower[j]<=p.mapped_minus_vertex_upper[j]);
                assert!(p.mapped_minus_vertex_lower[j].abs().max(p.mapped_minus_vertex_upper[j].abs())<1e-6);
            }
        }
    }
}
