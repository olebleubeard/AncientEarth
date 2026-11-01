// Original MIT. Sufficient-only exclusion for the isolated Cartesian suffix.
// Requires optical_normal_interval.wgsl and the unchanged scene/DP declarations.
// This never accepts a contact. Invalid inputs or any nonseparating root defer
// to the original-eye solver. No atmospheric tolerance is enlarged here.
struct CsI3 {x:vec2<f32>,y:vec2<f32>,z:vec2<f32>};
struct CsControls {b0:CsI3,b1:CsI3,b2:CsI3,b3:CsI3,b4:CsI3};
fn cs_exact3(p:vec3<f32>)->CsI3{return CsI3(nc_interval_exact(p.x),nc_interval_exact(p.y),nc_interval_exact(p.z));}
fn cs_add3(a:CsI3,b:CsI3)->CsI3{return CsI3(nc_interval_add(a.x,b.x),nc_interval_add(a.y,b.y),nc_interval_add(a.z,b.z));}
fn cs_sub3(a:CsI3,b:CsI3)->CsI3{return CsI3(nc_interval_sub(a.x,b.x),nc_interval_sub(a.y,b.y),nc_interval_sub(a.z,b.z));}
fn cs_scale3(a:CsI3,b:vec2<f32>)->CsI3{return CsI3(nc_interval_mul(a.x,b),nc_interval_mul(a.y,b),nc_interval_mul(a.z,b));}
fn cs_valid3(a:CsI3)->bool{return nc_interval_valid(a.x)&&nc_interval_valid(a.y)&&nc_interval_valid(a.z);}
fn cs_union3(a:CsI3,b:CsI3)->CsI3{
    if !cs_valid3(a)||!cs_valid3(b){let bad=nc_interval_invalid();return CsI3(bad,bad,bad);}
    return CsI3(vec2(min(a.x.x,b.x.x),max(a.x.y,b.x.y)),vec2(min(a.y.x,b.y.x),max(a.y.y,b.y.y)),vec2(min(a.z.x,b.z.x),max(a.z.y,b.z.y)));
}
fn cs_l1(a:CsI3)->vec2<f32>{
    if !cs_valid3(a){return nc_interval_invalid();}
    return nc_interval_add(nc_interval_add(vec2(nc_interval_abs_upper(a.x)),vec2(nc_interval_abs_upper(a.y))),vec2(nc_interval_abs_upper(a.z)));
}
fn cs_dot(row:vec3<f32>,p:CsI3)->vec2<f32>{
    return nc_interval_add(nc_interval_add(nc_interval_mul(nc_interval_exact(row.x),p.x),nc_interval_mul(nc_interval_exact(row.y),p.y)),nc_interval_mul(nc_interval_exact(row.z),p.z));
}
fn cs_transform(p:CsI3,base:u32)->CsI3{
    if base==0u{return p;}
    return CsI3(nc_interval_add(nc_interval_exact(optical_frames[base].x),cs_dot(optical_frames[base+1u].xyz,p)),nc_interval_add(nc_interval_exact(optical_frames[base].y),cs_dot(optical_frames[base+2u].xyz,p)),nc_interval_add(nc_interval_exact(optical_frames[base].z),cs_dot(optical_frames[base+3u].xyz,p)));
}
fn cs_box(c:CsControls,base:u32)->CsI3{
    return cs_union3(cs_union3(cs_union3(cs_transform(c.b0,base),cs_transform(c.b1,base)),cs_union3(cs_transform(c.b2,base),cs_transform(c.b3,base))),cs_transform(c.b4,base));
}
fn cs_separate(c:CsControls,base:u32,lo:vec3<f32>,hi:vec3<f32>,tube:vec2<f32>,source:vec2<f32>)->bool{
    let bounds=cs_box(c,base);let lower=cs_exact3(lo);let upper=cs_exact3(hi);
    if !cs_valid3(bounds)||!cs_valid3(lower)||!cs_valid3(upper)||!all(lo<=hi)||!nc_interval_valid(tube)||tube.x<0.0||!nc_interval_valid(source)||source.x<0.0{return false;}
    // L1 bounds Euclidean endpoint norms, including all five controls. Thus
    // the old 2e-6*max(endpoint norms) broadphase allowance is never reduced.
    let frame_rounding=nc_interval_mul(vec2(0.000002),cs_l1(bounds));
    let common_padding=nc_interval_add(source,frame_rounding);
    var gain=vec3(1.0);
    if base!=0u{
        let x=cs_l1(cs_exact3(optical_frames[base+1u].xyz));let y=cs_l1(cs_exact3(optical_frames[base+2u].xyz));let z=cs_l1(cs_exact3(optical_frames[base+3u].xyz));
        if !nc_interval_valid(x)||!nc_interval_valid(y)||!nc_interval_valid(z){return false;}
        gain=max(vec3(1.0),vec3(x.y,y.y,z.y));
    }
    let px=nc_interval_add(common_padding,nc_interval_mul(tube,vec2(gain.x)));
    let py=nc_interval_add(common_padding,nc_interval_mul(tube,vec2(gain.y)));
    let pz=nc_interval_add(common_padding,nc_interval_mul(tube,vec2(gain.z)));
    if !nc_interval_valid(px)||!nc_interval_valid(py)||!nc_interval_valid(pz){return false;}
    let x=nc_interval_add(bounds.x,vec2(-px.y,px.y));let y=nc_interval_add(bounds.y,vec2(-py.y,py.y));let z=nc_interval_add(bounds.z,vec2(-pz.y,pz.y));
    if !nc_interval_valid(x)||!nc_interval_valid(y)||!nc_interval_valid(z){return false;}
    return x.y<lower.x.x||x.x>upper.x.y||y.y<lower.y.x||y.x>upper.y.y||z.y<lower.z.x||z.x>upper.z.y;
}
fn cs_all_roots_clear(c:CsControls,tube:vec2<f32>)->bool{
    let nf=arrayLength(&optical_frames);let ng=arrayLength(&optical_geometry);
    if nf<6u||ng<1u||!nc_interval_valid(tube)||tube.x<0.0||!cs_valid3(c.b0)||!cs_valid3(c.b1)||!cs_valid3(c.b2)||!cs_valid3(c.b3)||!cs_valid3(c.b4){return false;}
    let header=bitcast<vec4<u32>>(optical_frames[0]);let patches=header.y;let markers=header.w;
    // Explicit iteration cap and subtraction-before-multiplication prevent
    // metadata wrap. The frozen source packet provides tree/source identity.
    if patches>8192u||patches>(nf-6u)/4u{return false;}
    let marker_start=6u+4u*patches;
    if bitcast<u32>(optical_frames[1].x)!=marker_start||markers>(nf-marker_start)/8u{return false;}
    if (patches==0u)!=(header.x==0u)||bitcast<u32>(optical_frames[2].w)!=header.x{return false;}
    if patches>0u{
        if header.x>=ng-1u{return false;}
        let stop=bitcast<u32>(optical_geometry[header.x].w);
        if stop<=header.x+1u||stop>ng{return false;}
    }
    for(var i=0u;i<patches;i+=1u){
        let base=6u+4u*i;let root=bitcast<u32>(optical_frames[base].w);
        if root==0u||root>=ng-1u{return false;}
        let stop=bitcast<u32>(optical_geometry[root].w);
        if stop<=root+1u||stop>ng{return false;}
        let source=nc_interval_exact(optical_frames[base+1u].w);
        // A stored zero source error may be flushed; its upper bound is kept,
        // while the actual nonnegative source allowance cannot be negative.
        if !nc_interval_valid(source)||optical_frames[base+1u].w<0.0{return false;}
        if !cs_separate(c,base,optical_geometry[root].xyz,optical_geometry[root+1u].xyz,tube,vec2(0.0,source.y)){return false;}
    }
    if (markers==0u)!=(header.z==0u){return false;}
    if markers>0u{
        let first_tree=marker_start+8u*markers;
        if header.z<first_tree||header.z>=nf-1u{return false;}
        let stop=bitcast<u32>(optical_frames[header.z].w);
        if stop<=header.z+1u||stop>nf{return false;}
        if !cs_separate(c,0u,optical_frames[header.z].xyz,optical_frames[header.z+1u].xyz,tube,vec2(0.0)){return false;}
    }
    return true;
}
fn cs_initial_owner(state:State,g:vec4<f32>)->i32{
    if !dp_finite(g)||!dp_finite(vec4(state.p,0.0))||!dp_finite(vec4(state.u,0.0)){return -2;}
    if !(settings.budget.x>=2.0&&settings.budget.x<=128.0&&floor(settings.budget.x)==settings.budget.x){return -2;}
    let count=u32(settings.budget.x);
    if !dp_finite(settings.scene)||!dp_finite(settings.budget)||settings.scene.y!=settings.layers[0].x||settings.budget.y!=settings.layers[count-1u].x{return -2;}
    // Same packed-field owner as index_gradient, with a finite selection guard.
    // This is deliberately NOT an exact-geodetic/inverse residual certificate.
    let band=max(0.00001,4e-7*length(state.p));
    let h=nc_interval_add(nc_interval_exact(g.w),vec2(-band,band));
    if !nc_interval_valid(h)||h.x<=settings.scene.z||h.y>=settings.budget.y{return -2;}
    var owner=select(-2,-1,h.y<settings.scene.y);
    for(var i=0u;i<count-1u;i+=1u){
        let a=settings.layers[i];let b=settings.layers[i+1u];
        if !dp_finite(a)||!dp_finite(b)||!(a.x<b.x){return -2;}
        if h.x>a.x&&h.y<b.x{owner=i32(i);}
    }
    return owner;
}
fn cs_point_scene_clear(p:vec3<f32>,error:f32)->bool{
    if !(error>=0.0&&error<3e37){return false;}
    let q=cs_exact3(p);let c=CsControls(q,q,q,q,q);
    return cs_all_roots_clear(c,vec2(0.0,nc_interval_exact(error).y));
}
fn cs_curve_scene_clear(start:State,accepted_distance:f32,error:f32)->bool{
    if !(accepted_distance>=0.0&&accepted_distance<=dp_curve_step&&dp_curve_step>=0.000001&&dp_curve_step<=100000.0&&error>=0.0&&error<3e37&&settings.scene.x>=0.0){return false;}
    let alpha=nc_interval_div(nc_interval_exact(accepted_distance),nc_interval_exact(dp_curve_step));
    if !nc_interval_valid(alpha)||alpha.y>1.000002{return false;}
    let a=vec2(max(0.0,alpha.x),alpha.y);let a2=nc_interval_mul(a,a);let a3=nc_interval_mul(a2,a);let a4=nc_interval_mul(a2,a2);
    let p=cs_exact3(start.p);let d=cs_exact3(dp_curve.delta.p);let r3=cs_exact3(dp_curve.r3);let r4=cs_exact3(dp_curve.r4);let r5=cs_exact3(dp_curve.r5);
    let c1=cs_scale3(cs_add3(d,r3),a);let c2=cs_scale3(cs_add3(cs_sub3(r4,r3),r5),a2);
    let c3=cs_scale3(cs_sub3(cs_scale3(r4,vec2(-1.0)),cs_scale3(r5,vec2(2.0))),a3);let c4=cs_scale3(r5,a4);
    let sixth=nc_interval_div(vec2(1.0),vec2(6.0));
    let b1=cs_add3(p,cs_scale3(c1,vec2(0.25)));
    let b2=cs_add3(p,cs_add3(cs_scale3(c1,vec2(0.5)),cs_scale3(c2,sixth)));
    let b3=cs_add3(p,cs_add3(cs_add3(cs_scale3(c1,vec2(0.75)),cs_scale3(c2,vec2(0.5))),cs_scale3(c3,vec2(0.25))));
    let b4=cs_add3(p,cs_add3(cs_add3(c1,c2),cs_add3(c3,c4)));
    let controls=CsControls(p,b1,b2,b3,b4);
    // Keep at least BOTH original broadphase sag allowances. These are NOT
    // an additional derivative proof on the small alpha>1 rounding extension;
    // the Bernstein controls already enclose that entire extension directly.
    let rsum=nc_interval_add(nc_interval_add(nc_interval_mul(vec2(2.0),cs_l1(r3)),nc_interval_mul(vec2(4.0),cs_l1(r4))),nc_interval_mul(vec2(2.0),cs_l1(r5)));
    let polynomial_sag=nc_interval_mul(nc_interval_mul(rsum,a2),vec2(0.125));
    let distance=nc_interval_exact(accepted_distance);
    let physical_sag=nc_interval_mul(nc_interval_mul(nc_interval_mul(nc_interval_exact(settings.scene.x),distance),distance),vec2(0.125));
    if !nc_interval_valid(polynomial_sag)||!nc_interval_valid(physical_sag){return false;}
    let displacement_magnitude=nc_interval_add(cs_l1(d),nc_interval_add(cs_l1(r3),nc_interval_add(cs_l1(r4),cs_l1(r5))));
    let magnitude=nc_interval_add(cs_l1(p),displacement_magnitude);
    if !nc_interval_valid(magnitude)||magnitude.y>1e9{return false;}
    // At alpha<=1.000002, the <=12 basic operations in stored dense position
    // evaluation have error below64*2^-23*sum(abs(inputs)); the unit floor
    // also covers every possible FTZ contribution. Division uncertainty is
    // already enclosed by alpha. This is display evaluation padding only.
    let evaluation=nc_interval_mul(vec2(0.00000762939453125),vec2(max(1.0,magnitude.y)));
    // Source-root allocation has path units. The represented dense derivative
    // is not assumed unit length. On0..1.000002 its four scalar weights are
    // bounded by1.00001, so this outward gain bounds physical metres/path-metre.
    // Scale the whole supplied error conservatively; only exclusion changes.
    let speed=nc_interval_div(nc_interval_mul(vec2(1.00001),displacement_magnitude),nc_interval_exact(dp_curve_step));
    if !nc_interval_valid(speed){return false;}
    let positioned_error=nc_interval_mul(vec2(0.0,nc_interval_exact(error).y),vec2(max(1.0,speed.y)));
    let tube=nc_interval_add(nc_interval_add(positioned_error,vec2(0.0,max(polynomial_sag.y,physical_sag.y))),evaluation);
    return cs_all_roots_clear(controls,tube);
}
