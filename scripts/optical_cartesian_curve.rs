//! Unregistered CPU certificate for a Cartesian cubic approximation to the
//! SAME canonical normal-coordinate dense curve. This does not integrate a
//! ray, certify an ODE global error, or admit any scene contact.
//! Factorial-scaled interval jets enclose P^(4)/4! over a closed interval.
//! Exact endpoint Hermite interpolation then has component error at most
//! sup|P^(4)| L^4/384 = sup|jet4| L^4/16. Endpoint/encoding uncertainty is
//! separate and must join the existing numerical/contact allocation.
mod optical_taylor;
use optical_taylor::Interval as I;
pub use optical_taylor::Interval as Bounds;
use serde_json::{Value,json};
use std::ops::{Add,Sub,Neg,Mul};
const E2:f64=0.0066943799901413165;
#[derive(Clone,Copy,Debug)]
pub struct Jet(pub [I;5]);
impl Jet {
    pub fn constant(v:I)->Self{let mut c=[I::point(0.);5];c[0]=v;Self(c)}
    pub fn variable(v:I)->Self{let mut out=Self::constant(v);out.0[1]=I::point(1.);out}
    pub fn valid(self)->bool{self.0.iter().all(|x|x.valid())}
    pub fn inverse_sqrt(self)->Self{
        if !self.valid()||self.0[0].lo<=0.{return Self([I::invalid();5]);}
        let mut b=[I::point(0.);5];b[0]=I::point(1.)/self.0[0].sqrt();
        // 2 A B' + A' B = 0, coefficient n-1. This recurrence
        // encloses derivatives at every point in the supplied base interval.
        for n in 1..5{
            let sum=(1..=n).fold(I::point(0.),|s,k|s+I::point((2*n-k) as f64)*self.0[k]*b[n-k]);
            b[n]=-sum/(I::point((2*n) as f64)*self.0[0]);
        }
        Self(b)
    }
}
impl Add for Jet{type Output=Self;fn add(self,b:Self)->Self{Self(std::array::from_fn(|i|self.0[i]+b.0[i]))}}
impl Neg for Jet{type Output=Self;fn neg(self)->Self{Self(self.0.map(|v|-v))}}
impl Sub for Jet{type Output=Self;fn sub(self,b:Self)->Self{self+-b}}
impl Mul for Jet{type Output=Self;fn mul(self,b:Self)->Self{Self(std::array::from_fn(|i|(0..=i).fold(I::point(0.),|s,k|s+self.0[k]*b.0[i-k])))}}
fn k(v:f64)->Jet{Jet::constant(I::point(v))}
fn dot(a:[Jet;3],b:[Jet;3])->Jet{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
#[derive(Clone,Copy,Debug)]
pub struct Curve {
    /// H offset,a,b at the start; four original dense basis coefficients.
    pub start:[f64;3],pub r:[[f64;3];4],
    pub anchor:I,pub eye:I,pub axis:[I;3],pub eccentricity:I,
}
impl Curve {
    fn coordinate_jets(self,theta:I)->[Jet;3]{
        let t=Jet::variable(theta);let z=k(1.)-t;
        std::array::from_fn(|i|k(self.start[i])+t*(k(self.r[0][i])+z*(k(self.r[1][i])+t*(k(self.r[2][i])+z*k(self.r[3][i])))))
    }
    pub fn position_jet(self,theta:I)->Option<[Jet;3]>{
        if !theta.valid()||theta.lo<0.||theta.hi>1.||!self.anchor.valid()||!self.eye.valid()
            ||self.axis.iter().any(|v|!v.valid())||!self.eccentricity.valid()
            ||self.eccentricity.lo<0.||self.eccentricity.hi>0.007{return None;}
        let one=k(1.);let q=self.coordinate_jets(theta);
        let squared=q[1]*q[1]+q[2]*q[2];let inv=(one+squared).inverse_sqrt();
        // Stable near-eye n-up, with 1/s-1=-q²/[s(s+1)].
        let inv_plus=one+inv;
        // Reciprocal is expressed through rsqrt squared, retaining all jets.
        let reciprocal=inv_plus.inverse_sqrt();let reciprocal=reciprocal*reciprocal;
        let dn=[q[1]*inv,-squared*inv*inv*reciprocal,-q[2]*inv];
        let axis=self.axis.map(Jet::constant);let e=Jet::constant(self.eccentricity);
        let z0=axis[1];let dz=dot(axis,dn);let zz=z0+dz;
        let d0=one-e*z0*z0;let d=one-e*zz*zz;
        let ir0=d0.inverse_sqrt();let ir=d.inverse_sqrt();let radius=k(6378137.)*ir;
        // N-N0 rationalized. Constant map origin never enters derivative4.
        let sum=ir+ir0;let suminv=sum.inverse_sqrt();let suminv=suminv*suminv;
        let product=ir*ir0;
        let dr=k(6378137.)*e*dz*(zz+z0)*product*product*suminv;
        let h=Jet::constant(self.anchor)+q[0];let dh=Jet::constant(self.anchor-self.eye)+q[0];
        let p=std::array::from_fn(|i|{
            let up=if i==1{one}else{k(0.)};
            (dn[i]-axis[i]*e*dz)*radius+(up-axis[i]*e*z0)*dr+dn[i]*h+up*dh
        });
        p.iter().all(|x|x.valid()).then_some(p)
    }
    /// Scalar norm majorant for ||P^(4)/4!||. The polar axis must be a true
    /// unit vector enclosed by metadata. Its rounded center is not substituted.
    /// Unlike the reference jet this needs no vector sqrt/div derivative graph.
    pub fn fourth_coefficient_upper(self,theta:I)->Option<f64>{
        if !theta.valid()||theta.lo<0.||theta.hi>1.||!self.anchor.valid()
            ||!self.eccentricity.valid()||self.eccentricity.lo<0.||self.eccentricity.hi>=0.007{return None;}
        let q=self.coordinate_jets(theta);if q.iter().any(|v|!v.valid()){return None;}
        let mut v=[I::point(0.);5];
        for j in 0..5{v[j]=I::point(q[1].0[j].abs_upper())+I::point(q[2].0[j].abs_upper());}
        if !v[0].valid()||v[0].hi>1.{return None;}
        v[0]=I::point(2.); // ||(a,1,-b)|| <=sqrt2<2 in the admitted chart.
        let mut xx=[I::point(0.);5];
        for j in 1..5{xx[j]=(0..=j).fold(I::point(0.),|s,i|s+v[i]*v[j-i]);}
        let mut inv=[I::point(0.);5];inv[0]=I::point(1.);
        for j in 1..5{inv[j]=(1..=j).fold(I::point(0.),|s,i|s+I::point((2*j-i)as f64)*xx[i]*inv[j-i])/I::point((2*j)as f64);}
        let mut normal=[I::point(0.);5];normal[0]=I::point(1.);
        for j in 1..5{normal[j]=(0..=j).fold(I::point(0.),|s,i|s+v[i]*inv[j-i]);}
        let e=I::point(self.eccentricity.hi);let d=I::point(1.)-e;
        let mut dd=[I::point(0.);5];
        for j in 1..5{dd[j]=e*(0..=j).fold(I::point(0.),|s,i|s+normal[i]*normal[j-i]);}
        let mut radius=[I::point(0.);5];radius[0]=I::point(6378137.)/d.sqrt();
        for j in 1..5{radius[j]=(1..=j).fold(I::point(0.),|s,i|s+I::point((2*j-i)as f64)*dd[i]*radius[j-i])/(I::point((2*j)as f64)*d);}
        let height: [I;5]=std::array::from_fn(|j|I::point(if j==0{(self.anchor+q[0].0[0]).abs_upper()}else{q[0].0[j].abs_upper()}));
        let result=(I::point(1.)+e)*(0..5).fold(I::point(0.),|s,i|s+radius[i]*normal[4-i])
            +(0..5).fold(I::point(0.),|s,i|s+height[i]*normal[4-i]);
        if !result.valid()||result.lo<0.{None}else{Some(result.hi)}
    }
}
#[derive(Clone,Debug)]
pub struct Certificate {
    pub component_fit:[f64;3],pub endpoint_uncertainty:f64,pub total_map_fit:f64,
    /// Remainder for dP/d(original theta), beyond the interval cubic's own
    /// derivative. This is not inferred by differentiating an unknown xi.
    pub component_derivative_fit:[f64;3],
    /// Bernstein controls in local piece parameter x=(theta-a)/(b-a).
    pub controls:[[I;3];4],
    /// Quadratic Bernstein controls for dP/d(original theta).
    pub derivative_controls:[[I;3];3],
    pub endpoint_positions:[[I;3];2],pub endpoint_derivatives:[[I;3];2],
}
fn norm_l1_upper(v:impl IntoIterator<Item=f64>)->Option<f64>{
    let sum=v.into_iter().fold(I::point(0.),|s,x|s+I::point(x));sum.valid().then_some(sum.hi)
}
fn pack_interval(v:I)->Option<I>{
    if !v.valid(){return None;}let mut lo=v.lo as f32;let mut hi=v.hi as f32;
    if !lo.is_finite()||!hi.is_finite(){return None;}
    if f64::from(lo)>v.lo{lo=lo.next_down();}if f64::from(hi)<v.hi{hi=hi.next_up();}
    if v.lo<0. && lo.abs()<f32::MIN_POSITIVE{lo=-f32::MIN_POSITIVE;}
    if v.lo>=0. && lo!=0. && lo<f32::MIN_POSITIVE{lo=0.;}
    if v.hi>0. && hi.abs()<f32::MIN_POSITIVE{hi=f32::MIN_POSITIVE;}
    if v.hi<=0. && hi!=0. && hi.abs()<f32::MIN_POSITIVE{hi=0.;}
    let out=I{lo:f64::from(lo),hi:f64::from(hi)};out.valid().then_some(out)
}
#[derive(Clone,Debug)]
pub struct Packed {
    pub origin_hi:[f32;3],pub origin_lo:[f32;3],pub controls:[[I;3];4],
    pub derivative_controls:[[I;3];3],pub position_remainder:[f32;3],pub derivative_remainder:[f32;3],
    pub maximum_control_radius:f64,pub position_radius:f64,
}
pub fn pack_certificate(c:&Certificate)->Option<Packed>{
    if c.component_fit.iter().chain(c.component_derivative_fit.iter()).any(|v|!v.is_finite()||*v<0.)
        ||c.endpoint_positions.iter().flatten().any(|v|!v.valid()){
        return None;
    }
    let center: [f64;3]=c.endpoint_positions[0].map(|v|v.lo+(v.hi-v.lo)*0.5);
    let origin_hi=center.map(|v|v as f32);let origin_lo=std::array::from_fn(|i|(center[i]-f64::from(origin_hi[i]))as f32);
    if origin_hi.iter().chain(origin_lo.iter()).any(|x|!x.is_finite()){return None;}
    let origin=std::array::from_fn::<_,3,_>(|i|I::point(f64::from(origin_hi[i]))+I::point(f64::from(origin_lo[i])));
    let mut controls=[[I::point(0.);3];4];let mut derivative_controls=[[I::point(0.);3];3];
    for j in 0..4{for i in 0..3{controls[j][i]=pack_interval(c.controls[j][i]-origin[i])?;}}
    for j in 0..3{for i in 0..3{derivative_controls[j][i]=pack_interval(c.derivative_controls[j][i])?;}}
    let mut position_remainder=[0.;3];let mut derivative_remainder=[0.;3];
    for i in 0..3{position_remainder[i]=pack_interval(I::point(c.component_fit[i]))?.hi as f32;derivative_remainder[i]=pack_interval(I::point(c.component_derivative_fit[i]))?.hi as f32;}
    let mut maximum_control_radius=0_f64;
    for row in controls{
        let mut radius=I::point(0.);
        for value in row{let midpoint=value.lo+(value.hi-value.lo)*0.5;let error=value-I::point(midpoint);if !error.valid(){return None;}radius=radius+I::point(error.abs_upper());}
        if !radius.valid(){return None;}maximum_control_radius=maximum_control_radius.max(radius.hi);
    }
    let position_radius=(I::point(maximum_control_radius)+I::point(norm_l1_upper(position_remainder.map(f64::from))?)).hi;
    if !position_radius.is_finite(){return None;}
    Some(Packed{origin_hi,origin_lo,controls,derivative_controls,position_remainder,derivative_remainder,maximum_control_radius,position_radius})
}
pub fn certify(curve:Curve,range:I)->Option<Certificate>{
    if !range.valid()||range.lo<0.||range.hi>1.||range.lo>=range.hi{return None;}
    let whole=curve.position_jet(range)?;
    let left=curve.position_jet(I::point(range.lo))?;let right=curve.position_jet(I::point(range.hi))?;
    let width=I::point(range.hi)-I::point(range.lo);let width2=width*width;
    let scale=width2*width2/I::point(16.);
    let fit_intervals:[I;3]=std::array::from_fn(|i|I::point(whole[i].0[4].abs_upper())*scale);
    // Taylor about the nearer endpoint gives derivative remainder <=
    // M4 L^3(1/48+1/16+1/18)=5 M4 L^3/36. Since jet4=M4/24,
    // the coefficient is10/3. Componentwise bounds remain independent.
    let derivative_scale=width2*width*I::point(10.)/I::point(3.);
    let derivative_intervals:[I;3]=std::array::from_fn(|i|I::point(whole[i].0[4].abs_upper())*derivative_scale);
    if fit_intervals.iter().chain(derivative_intervals.iter()).any(|v|!v.valid()||v.lo<0.){return None;}
    let component_fit=fit_intervals.map(|v|v.hi);
    let component_derivative_fit=derivative_intervals.map(|v|v.hi);
    let endpoint_positions=[left.map(|v|v.0[0]),right.map(|v|v.0[0])];
    let endpoint_derivatives=[left.map(|v|v.0[1]),right.map(|v|v.0[1])];
    let controls=[endpoint_positions[0],
        std::array::from_fn(|i|endpoint_positions[0][i]+width*endpoint_derivatives[0][i]/I::point(3.)),
        std::array::from_fn(|i|endpoint_positions[1][i]-width*endpoint_derivatives[1][i]/I::point(3.)),endpoint_positions[1]];
    let derivative_controls=[endpoint_derivatives[0],
        std::array::from_fn(|i|I::point(3.)*(endpoint_positions[1][i]-endpoint_positions[0][i])/width-endpoint_derivatives[0][i]-endpoint_derivatives[1][i]),endpoint_derivatives[1]];
    // A midpoint is not implicitly exact: its complete endpoint interval
    // radius is retained. Hermite |h10|+|h11|<=1/4; h00+h01=1.
    let radius=|v:I|->Option<f64>{if !v.valid(){return None;}let center=v.lo+(v.hi-v.lo)*0.5;let error=v-I::point(center);error.valid().then_some(error.abs_upper())};
    let rpos=endpoint_positions.map(|row|norm_l1_upper(row.map(radius).into_iter().collect::<Option<Vec<_>>>()?));
    let rvel=endpoint_derivatives.map(|row|norm_l1_upper(row.map(radius).into_iter().collect::<Option<Vec<_>>>()?));
    let ep=I::point(rpos[0]?.max(rpos[1]?))+width/I::point(4.)*I::point(rvel[0]?.max(rvel[1]?));
    let total=ep+I::point(norm_l1_upper(component_fit)?);
    if !ep.valid()||!total.valid(){return None;}
    if controls.iter().flatten().chain(derivative_controls.iter().flatten()).any(|v|!v.valid()){
        return None;
    }
    Some(Certificate{component_fit,component_derivative_fit,controls,derivative_controls,endpoint_uncertainty:ep.hi,total_map_fit:total.hi,endpoint_positions,endpoint_derivatives})
}
fn words(path:&str)->Vec<u32>{let b=std::fs::read(path).unwrap();assert!(b.len().is_multiple_of(4));b.chunks_exact(4).map(|w|u32::from_le_bytes(w.try_into().unwrap())).collect()}
fn f(v:u32)->f64{f64::from(f32::from_bits(v))}
fn pair(lo:u32,hi:u32)->I{I{lo:f(lo),hi:f(hi)}}
fn scalar_certificate(curve:Curve,range:I)->Option<Certificate>{
    let mut c=certify(curve,range)?;let bound=curve.fourth_coefficient_upper(range)?;
    let w=I::point(range.hi)-I::point(range.lo);let w2=w*w;
    let p=I::point(bound)*w2*w2/I::point(16.);let d=I::point(bound)*w2*w*I::point(10.)/I::point(3.);
    if !p.valid()||!d.valid()||p.lo<0.||d.lo<0.{return None;}
    c.component_fit=[p.hi;3];c.component_derivative_fit=[d.hi;3];
    let total=I::point(c.endpoint_uncertainty)+I::point(3.)*p;
    if !total.valid(){return None;}c.total_map_fit=total.hi;Some(c)
}
pub fn append_piece(data:&mut Vec<u32>,packed:&Packed,range:I,owner:usize,piece:usize,count:usize,scalar:bool){
    let mut v=[[0_u32;4];20];let span=(range.hi-range.lo)as f32;let inverse_span=1_f32/span;
    assert!(span>0.&&f64::from(span)==range.hi-range.lo&&f64::from(inverse_span)*(range.hi-range.lo)==1.);
    v[0]=[(range.lo as f32).to_bits(),(range.hi as f32).to_bits(),inverse_span.to_bits(),(pack_interval(I::point(packed.position_radius)).unwrap().hi as f32).to_bits()];
    for i in 0..3{v[1][i]=packed.origin_hi[i].to_bits();v[2][i]=packed.origin_lo[i].to_bits();}
    for j in 0..4{for i in 0..3{v[3+2*j][i]=(packed.controls[j][i].lo as f32).to_bits();v[4+2*j][i]=(packed.controls[j][i].hi as f32).to_bits();}}
    for j in 0..3{for i in 0..3{v[11+2*j][i]=(packed.derivative_controls[j][i].lo as f32).to_bits();v[12+2*j][i]=(packed.derivative_controls[j][i].hi as f32).to_bits();}}
    for i in 0..3{v[17][i]=packed.position_remainder[i].to_bits();v[18][i]=packed.derivative_remainder[i].to_bits();}
    v[19]=[u32::try_from(owner).unwrap(),u32::try_from(piece).unwrap(),u32::try_from(count).unwrap(),u32::from(scalar)];
    data.extend(v.into_iter().flatten());
}
fn main(){
    let args:Vec<_>=std::env::args().collect();let output=args.get(1).expect("new output JSON path");assert!(!std::path::Path::new(output).exists());
    let packet_prefix=args.iter().position(|v|v=="--packet-prefix").map(|i|args.get(i+1).expect("packet prefix"));
    if let Some(prefix)=packet_prefix{for suffix in [".json","-pieces.bin"]{assert!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists());}}
    let scalar_packet=args.iter().any(|v|v=="--scalar-majorant");let mut packet_words=Vec::new();let mut packet_rows=Vec::new();
    let raw=words("artifacts/optical-height-trial-capture-readback.bin");
    let uniform=words("artifacts/optical-height-trial-capture-uniform.bin");
    let cert=words("artifacts/optical-normal-scene-packet-certificates.bin");
    let packed_axis=std::array::from_fn(|i|pair(cert[8+i],cert[12+i]));let packed_e=pair(cert[11],cert[15]);
    let packet:Value=serde_json::from_slice(&std::fs::read("artifacts/optical-normal-scene-packet.json").unwrap()).unwrap();
    let ideal_axis=std::array::from_fn(|i|I{lo:packet["ideal_frame_columns"][i][2][0].as_f64().unwrap(),hi:packet["ideal_frame_columns"][i][2][1].as_f64().unwrap()});
    let eye=I::point(packet["canonical_eye_geodetic"][2].as_f64().unwrap());
    let layers=cert[2] as usize;let source_start=cert[5] as usize;let mut rows=Vec::new();let begin=std::time::Instant::now();
    assert_eq!(raw.len(),7*13864);
    for ray in 0..7 {let record=&raw[ray*13864..(ray+1)*13864];assert_eq!(record[38],0);let count=record[36] as usize;assert!(count<=128);
        for trial in 0..count{let v=&record[40+108*trial..40+108*(trial+1)];if v[96]!=2{continue;}
            let matches:Vec<_>=(0..layers).filter(|i|v[8..10]==uniform[20+4*i..22+4*i]).collect();
            let residual=if v[8..10]==uniform[2..4]{assert!(matches.is_empty());pair(cert[16],cert[17])}else{assert_eq!(matches.len(),1);pair(cert[4*(source_start+matches[0])],cert[4*(source_start+matches[0])+1])};
            let anchor=I::point(f(v[8]))+I::point(f(v[9]))+residual;
            let start=std::array::from_fn(|i|f(v[i]));let mut r=std::array::from_fn(|j|std::array::from_fn(|i|f(v[12+4*j+i])));
            r[0][0]=f(v[3]);for row in &mut r[1..]{row[0]=0.;}
            let numerical=(I::point(f(v[48]))+I::point(f(v[97]))).hi;let tolerance=f(v[98]);let remaining=(I::point(tolerance)-I::point(numerical)).lo;
            if packet_prefix.is_some(){
                let curve=Curve{start,r,anchor,eye,axis:packed_axis,eccentricity:packed_e};let mut accepted=None;
                for pieces in [1,2,4,8,16,32,64]{let mut blocks=Vec::new();let mut valid=true;
                    for piece in 0..pieces{let range=I{lo:piece as f64/pieces as f64,hi:(piece+1)as f64/pieces as f64};
                        let value=if scalar_packet{scalar_certificate(curve,range)}else{certify(curve,range)};
                        let Some(certificate)=value else{valid=false;break;};let Some(packed)=pack_certificate(&certificate)else{valid=false;break;};
                        let stored_radius=pack_interval(I::point(packed.position_radius)).unwrap().hi;
                        if stored_radius>remaining*0.25{valid=false;break;}
                        blocks.push((range,packed));
                    }
                    if valid{accepted=Some(blocks);break;}
                }
                let blocks=accepted.expect("Retained curve needs unrepresented fallback; do not silently omit it");let first_piece=packet_words.len()/80;let pieces=blocks.len();
                for (piece,(range,packed)) in blocks.iter().enumerate(){append_piece(&mut packet_words,packed,*range,rows.len(),piece,pieces,scalar_packet);}
                packet_rows.push(json!({"curve_index":rows.len(),"capture_ray":ray,"capture_trial":trial,"first_piece":first_piece,"pieces":pieces,"numerical_source_error_m":numerical,"tolerance_m":tolerance,"remaining_m":remaining}));
            }
            let mut results=Vec::new();
            for (name,axis,eccentricity,scalar) in [("canonical_f64_axis",ideal_axis,I::point(E2),false),("uploaded_f32_axis_interval",packed_axis,packed_e,false),("uploaded_f32_axis_scalar_majorant",packed_axis,packed_e,true)]{
                let curve=Curve{start,r,anchor,eye,axis,eccentricity};let mut attempts=Vec::new();
                for pieces in [1,2,4,8,16,32,64]{let mut fit=0_f64;let mut floor=0_f64;let mut packed_fit=0_f64;let mut derivative_fit=0_f64;let mut valid=true;
                    for piece in 0..pieces{let range=I{lo:piece as f64/pieces as f64,hi:(piece+1)as f64/pieces as f64};
                        if let Some(mut c)=certify(curve,range){
                            if scalar{let Some(value)=scalar_certificate(curve,range)else{valid=false;break;};c=value;}
                            let Some(packed)=pack_certificate(&c)else{valid=false;break;};
                            fit=fit.max(c.total_map_fit);floor=floor.max(c.endpoint_uncertainty);packed_fit=packed_fit.max(packed.position_radius);derivative_fit=derivative_fit.max(norm_l1_upper(c.component_derivative_fit).unwrap_or(f64::INFINITY));
                        }else{valid=false;break;}}
                    attempts.push(json!({"pieces":pieces,"valid":valid,"maximum_map_fit_m":fit,"maximum_endpoint_interval_m":floor,"maximum_split_packed_control_plus_fit_m":packed_fit,"maximum_derivative_fit_m_per_theta":derivative_fit,"fits_remaining_numerical_budget":valid&&packed_fit<=remaining,"fits_quarter_remaining_budget":valid&&packed_fit<=remaining*0.25}));
                    if valid&&packed_fit<=remaining*0.25{break;}
                }
                results.push(json!({"metadata_scope":name,"subdivision_trials":attempts}));
            }
            rows.push(json!({"ray":ray,"trial":trial,"numerical_and_source_m":numerical,"tolerance_m":tolerance,"remaining_m":remaining,"start_height_interval_m":[(anchor+I::point(start[0])).lo,(anchor+I::point(start[0])).hi],"height_delta_m":r[0][0],"results":results}));
        }
    }
    assert_eq!(rows.len(),495);
    if let Some(prefix)=packet_prefix{
        assert_eq!(packet_rows.len(),495);assert!(packet_words.len().is_multiple_of(80));
        std::fs::write(format!("{prefix}-pieces.bin"),packet_words.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>()).unwrap();
        let metadata=json!({"schema":"ancientearth.cartesian-scene-curves.v1","scope":"Static retained-curve diagnostic only. Every one of495 original accepted GPU height intervals is represented chronologically. This is not a current-frame GPU producer or a scene-contact result. Shared input capture/source geometry unchanged; interval controls retain source/axis and packing uncertainty. Consumer interval evaluation, canonical face uncertainty and original numerical/source charge must all fit the same total tolerance.","scalar_majorant":scalar_packet,"capture":"artifacts/optical-height-trial-capture","source_packet":"artifacts/optical-normal-scene-packet","record_words":80,"record_vec4":20,"piece_count":packet_words.len()/80,"curve_count":495,
            "layout":{"0":"theta0,theta1,exact power-of-two inverse span, upper packed-control+position-fit radius (excludes consumer evaluation)","1":"split Cartesian origin_hi xyz,unused0","2":"origin_lo xyz,unused0","3..10":"four local position Bernstein controls: lower xyz then upper xyz; w0","11..16":"three derivative Bernstein controls in ORIGINAL theta units: lower xyz then upper xyz; w0","17":"component position fit remainder upper xyz;w0","18":"component derivative fit remainder upper xyz;w0","19":"u32 original curve index,piece index,piece count,scalar-majorant flag"},
            "consumer_contract":"x=(theta-theta0)*inverse_span with outward arithmetic and strict stored range eligibility. Evaluate position cubic and derivative quadratic in x with interval De Casteljau; add separate component remainders. Fold origin_hi then origin_lo into canonical eye-minus-faceA before adding local position. Derivative controls enclose the true cubic derivative, not necessarily the derivative of arbitrarily rounded midpoint position controls. Original H/path/direction curves remain unmodified and evaluated at original theta. Ordered pieces do not authorize state commit until all earlier events/uncertainties are resolved. No proximity-only hit or uniform residual omission.","curves":packet_rows});
        std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&metadata).unwrap()).unwrap();
    }
    let report=json!({"scope":"Unused CPU feasibility certificate for Cartesian cubic Hermite maps of495 retained accepted GPU height curves. Factorial interval jets or a scalar norm majorant bound canonical fourth derivatives; endpoint metadata uncertainty and split-origin/local-control f32 packing are retained. Quarter of remaining numerical budget is only a diagnostic subdivision target; no scene ownership/ODE/global/GPU f32 evaluation or native performance acceptance. Actual source/frame/contact and consumer evaluation uncertainty remain additional. Derivative remainder is separately bounded for original-theta transversality; no differentiated-unknown-xi claim. Exact stored curve coefficients are unchanged.","elapsed_seconds":begin.elapsed().as_secs_f64(),"curves":rows});
    std::fs::write(output,serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn inverse_root_jet_matches_exact_binomial_derivatives(){
        for a in [0.25,1.,4.,10.]{let got=Jet::variable(I::point(a)).inverse_sqrt();let mut coefficient=1.;for n in 0..5{if n>0{coefficient*=(-0.5-(n-1)as f64)/n as f64;}let expected=coefficient*a.powf(-0.5-n as f64);assert!(got.0[n].lo<=expected&&expected<=got.0[n].hi);}}
    }
    #[test]fn constant_chart_linear_height_has_no_fourth_derivative(){
        let c=Curve{start:[0.,0.,0.],r:[[100.,0.,0.],[0.;3],[0.;3],[0.;3]],anchor:I::point(-26.),eye:I::point(-26.),axis:[I::point(0.),I::point(0.5),I::point(-3_f64.sqrt()/2.)],eccentricity:I::point(E2)};
        let got=certify(c,I{lo:0.,hi:1.}).unwrap();assert!(got.component_fit.iter().all(|v|*v==0.));assert!(got.total_map_fit<1e-10);
    }
}
#[cfg(test)]
#[path="optical_cartesian_curve_review.rs"]
mod independent_review;
