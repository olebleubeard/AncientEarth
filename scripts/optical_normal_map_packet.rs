//! Unused canonical map metadata authoring. No renderer registration.
//! Intervals contain exact real values of the canonical binary64 inputs;
//! consumers must retain interval arithmetic, not reconstruct center values.
#[path = "optical_taylor.rs"]
mod arithmetic;
use arithmetic::Interval as I;
use earth_math::{EnuBasis,Geodetic,WGS84_E2,atmosphere::AtmosphereProfile};

#[derive(Clone,Debug)]
pub struct MapPacket {
    /// Components in the same east/up/-north eye frame as scene vertices.
    /// The interval normalization encloses a true unit vector, not a claim
    /// that a rounded GPU axis is exactly unit.
    pub axis_lower:[f32;3],
    pub axis_upper:[f32;3],
    pub eccentricity:[f32;2],
    pub eye_residual:[f32;2],
    /// Initial anchor is the eye. Source anchor i uses source_residuals[i].
    pub source_residuals:Vec<[f32;2]>,
}
pub fn pack_interval(value:I)->Result<[f32;2],String>{
    if !value.valid()||value.lo.abs().max(value.hi.abs())>1.0e8{return Err("Map metadata interval outside finite packing domain".into());}
    let mut low=value.lo as f32;let mut high=value.hi as f32;
    if f64::from(low)>value.lo{low=low.next_down();}
    if f64::from(high)<value.hi{high=high.next_up();}
    // Metadata endpoints themselves must survive permitted FTZ. Authored
    // exact zero remains zero; all other subnormal ranges widen to normals.
    if low!=0. && low.abs()<f32::MIN_POSITIVE{low=if low>0.{0.}else{-f32::MIN_POSITIVE};}
    if high!=0. && high.abs()<f32::MIN_POSITIVE{high=if high>0.{f32::MIN_POSITIVE}else{0.};}
    if value.lo<0. && low==0.{low=-f32::MIN_POSITIVE;}
    if value.hi>0. && high==0.{high=f32::MIN_POSITIVE;}
    Ok([low,high])
}
pub fn split_residual(value:f64)->Result<[f32;2],String>{
    if !value.is_finite()||value.abs()>1.0e8{return Err("Map source height outside finite packing domain".into());}
    let high=value as f32;let low=(value-f64::from(high)) as f32;
    residual_for_split(value,[high,low])
}
/// Encloses the residual against the ACTUAL uploaded words. `split_residual`
/// is only a convenience for the shared high/low split used by FieldSettings;
/// using a different pair requires calling this function with that pair.
pub fn residual_for_split(value:f64,pair:[f32;2])->Result<[f32;2],String>{
    if !value.is_finite()||value.abs()>1.0e8||pair.iter().any(|x|!x.is_finite()||x.abs()>1.0e8){return Err("Map source height or split outside finite packing domain".into());}
    let encoded=I::point(f64::from(pair[0]))+I::point(f64::from(pair[1]));
    pack_interval(I::point(value)-encoded)
}
impl MapPacket {
    pub fn from_canonical(eye:Geodetic,profile:&AtmosphereProfile)->Result<Self,String>{
        let (axis_lower,axis_upper)=axis_intervals(eye)?;
        let source_residuals=profile.layers().iter().map(|node|split_residual(profile.base_height_m()+node.height_m)).collect::<Result<_,_>>()?;
        Ok(Self{axis_lower,axis_upper,
            eccentricity:eccentricity_interval()?,eye_residual:split_residual(eye.height_m)?,source_residuals,
        })
    }
}
pub fn eccentricity_interval()->Result<[f32;2],String>{pack_interval(I::point(WGS84_E2))}
pub fn axis_intervals(eye:Geodetic)->Result<([f32;3],[f32;3]),String>{
        if !eye.lat_deg.is_finite()||!eye.lon_deg.is_finite()||!(-90.0..=90.0).contains(&eye.lat_deg){return Err("Invalid canonical map eye".into());}
        let basis=EnuBasis::at(eye);
        let raw=[basis.east.z,basis.up.z,-basis.north.z].map(I::point);
        let squared=raw.iter().fold(I::point(0.),|s,x|s+*x**x);
        let norm=squared.sqrt();
        if !norm.valid()||norm.lo<0.999999||norm.hi>1.000001{return Err("Invalid canonical polar axis".into());}
        let packed:Vec<_>=raw.into_iter().map(|x|pack_interval(x/norm)).collect::<Result<_,_>>()?;
        Ok((std::array::from_fn(|i|packed[i][0]),std::array::from_fn(|i|packed[i][1])))
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn endpoint_packing_covers_signed_values_and_ftz(){
        for x in [-1e8,-1.,-1e-40,-1e-300,0.,1e-300,1e-40,1.,1e8]{
            let p=pack_interval(I::point(x)).unwrap();
            assert!(f64::from(p[0])<=x&&f64::from(p[1])>=x);
            for v in p{assert!(v==0.||v.abs()>=f32::MIN_POSITIVE);}
        }
    }
    #[test]fn split_residual_contains_canonical_height(){
        for h in [-10000.,-26.00552287325263,0.,78.084735,3757.2,120000.123456789,1e8]{
            let high=h as f32;let low=(h-f64::from(high)) as f32;
            let residual=split_residual(h).unwrap();
            let sum=f64::from(high)+f64::from(low);
            assert!(sum+f64::from(residual[0])<=h);
            assert!(sum+f64::from(residual[1])>=h);
        }
    }
}
