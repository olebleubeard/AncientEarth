// Original MIT. Diagnostic replacement only, not registered production code.
// Normal-line coverage with triangle-anchor arithmetic. Bounds are a declared
// finite f32 arithmetic envelope, not a universal shader/hardware proof.
const COVERAGE_U:f32=5.960464477539063e-8;
fn coverage_gamma(n:f32)->f32{return n*COVERAGE_U/(1.0-n*COVERAGE_U);}
fn coverage_finite(x:f32)->bool{return abs(x)<=3e38;}
fn coverage_transform_error(p:vec3<f32>,base:u32,is_vector:bool)->f32{
    let products=vec3(dot(abs(optical_frames[base+1u].xyz),abs(p)),dot(abs(optical_frames[base+2u].xyz),abs(p)),dot(abs(optical_frames[base+3u].xyz),abs(p)));
    // Coefficient quantization, three products and two sums; the translation
    // also has quantization and a final addition. No cancellation assumption.
    var error=products*coverage_gamma(6.0);
    if !is_vector{error+=abs(optical_frames[base].xyz)*coverage_gamma(2.0);}
    return length(error);
}
// Error in a computed cross product with norm-bounded input uncertainty.
fn coverage_cross_error(a:vec3<f32>,b:vec3<f32>,ea:f32,eb:f32)->f32{
    return length(a)*eb+length(b)*ea+ea*eb+2.0*coverage_gamma(2.0)*length(a)*length(b);
}
fn coverage_dot_error(a:vec3<f32>,b:vec3<f32>,ea:f32,eb:f32)->f32{
    return length(a)*eb+length(b)*ea+ea*eb+coverage_gamma(3.0)*length(a)*length(b);
}
// 0 outside, 1 strict interior, 2 finite edge uncertainty, 3 nonfinite.
// Explicit edge calls avoid dynamically indexed private point/error arrays.
fn coverage_edge_local(a:vec3<f32>,b:vec3<f32>,ea:f32,eb:f32,at:vec3<f32>,at_error:f32,n:vec3<f32>,en:f32)->u32{
    let edge=b-a;let w=at-a;
    let edge_error=ea+eb+coverage_gamma(1.0)*(length(a)+length(b));
    let w_error=at_error+ea+coverage_gamma(1.0)*(length(at)+length(a));
    let product=cross(edge,w);let product_error=coverage_cross_error(edge,w,edge_error,w_error);
    let edge_value=dot(product,n);let edge_error_bound=coverage_dot_error(product,n,product_error,en);
    if !coverage_finite(edge_error_bound) || !coverage_finite(edge_value){return 3u;}
    if edge_value < -edge_error_bound{return 0u;}
    return select(1u,2u,edge_value<=edge_error_bound);
}
// 0 definitely outside this finite envelope; 1 strict interior; 2 uncertain.
fn coverage_face_local(x:vec3<f32>,v:vec3<f32>,ex:f32,ev:f32,va:vec3<f32>,vb:vec3<f32>,vc:vec3<f32>,source_error:f32)->u32{
    let a=vec3(0.0);let b=vb-va;let c=vc-va;let q=x-va;
    let eb=2.0*source_error+coverage_gamma(1.0)*(length(vb)+length(va));
    let ec=2.0*source_error+coverage_gamma(1.0)*(length(vc)+length(va));
    let eq=ex+source_error+coverage_gamma(1.0)*(length(x)+length(va));
    let raw=cross(b,c);let area=length(raw);let cross_error=coverage_cross_error(b,c,eb,ec);
    if !(area>cross_error) || !coverage_finite(area){return 2u;}
    let n=raw/area;
    // Normalization perturbation plus a finite sqrt/division allowance. The
    // source-vertex perturbation is not treated as a material/coverage dilation.
    let en=2.0*cross_error/(area-cross_error)+coverage_gamma(16.0);
    let numerator=dot(n,q);let denominator=dot(n,v);
    let numerator_error=coverage_dot_error(n,q,en,eq);
    let denominator_error=coverage_dot_error(n,v,en,ev);
    if !(abs(denominator)>denominator_error) || !coverage_finite(denominator_error){return 2u;}
    let distance=-numerator/denominator;
    let distance_error=(numerator_error+abs(distance)*denominator_error+coverage_gamma(1.0)*abs(distance)*abs(denominator))/(abs(denominator)-denominator_error);
    if !coverage_finite(distance_error) || !coverage_finite(distance){return 2u;}
    if distance+distance_error < -10001.0 || distance-distance_error > 101000.0{return 0u;}
    if distance-distance_error < -10001.0 || distance+distance_error > 101000.0{return 2u;}
    let at=q+v*distance;
    let at_error=eq+abs(distance)*ev+distance_error*(length(v)+ev)+coverage_gamma(2.0)*(length(q)+abs(distance)*length(v));
    let ab=coverage_edge_local(a,b,0.0,eb,at,at_error,n,en);
    if ab==0u{return 0u;}if ab==3u{return 2u;}
    let bc=coverage_edge_local(b,c,eb,ec,at,at_error,n,en);
    if bc==0u{return 0u;}if bc==3u{return 2u;}
    let ca=coverage_edge_local(c,a,ec,0.0,at,at_error,n,en);
    if ca==0u{return 0u;}if ca==3u{return 2u;}
    return select(1u,2u,ab==2u||bc==2u||ca==2u);
}
fn terrain_coverage(p:vec3<f32>,up:vec3<f32>)->u32{
    let root=bitcast<u32>(optical_frames[0].x);if root==0u{return 0u;}
    let a=p+up*101000.0;let b=p-up*10001.0;let ga=frame_point(a,2u);let gb=frame_point(b,2u);
    var index=root;let limit=bitcast<u32>(optical_geometry[root].w);var visited=0u;var uncertain=false;
    loop{
        if index>=limit{break;}visited+=1u;if visited>8192u{return 2u;}
        let lo=optical_geometry[index];let hi=optical_geometry[index+1u];let escape=bitcast<u32>(lo.w);let payload=bitcast<u32>(hi.w);
        if !segment_box(ga,gb,lo.xyz,hi.xyz,2e-6*max(length(ga),length(gb))){index=escape;continue;}
        if (payload&OPTICAL_LEAF)==0u{index+=2u;continue;}
        let base=6u+(payload&OPTICAL_ADDRESS)*4u;let x=frame_point(p,base);let v=frame_vector(up,base);
        let ex=coverage_transform_error(p,base,false);
        // The existing inverse-geodetic normal is supplied in f32. Retain a
        // finite normal allowance as well as matrix/operation rounding.
        let ev=coverage_transform_error(up,base,true)+coverage_gamma(16.0)*length(up);
        let source_error=optical_frames[base+1u].w;
        let line_a=frame_point(a,base);let line_b=frame_point(b,base);
        var node=bitcast<u32>(optical_frames[base].w);let stop=bitcast<u32>(optical_geometry[node].w);
        loop{
            if node>=stop{break;}visited+=1u;if visited>8192u{return 2u;}
            let lower=optical_geometry[node];let upper=optical_geometry[node+1u];let next=bitcast<u32>(lower.w);let leaf=bitcast<u32>(upper.w);
            if !segment_box(line_a,line_b,lower.xyz,upper.xyz,source_error+2e-6*max(length(line_a),length(line_b))){node=next;continue;}
            if (leaf&OPTICAL_LEAF)==0u{node+=2u;continue;}
            let address=leaf&OPTICAL_ADDRESS;let count=((leaf>>24u)&7u)+1u;
            for(var j=0u;j<count;j+=1u){let first=address+3u*j;
                let result=coverage_face_local(x,v,ex,ev,optical_geometry[first].xyz,optical_geometry[first+1u].xyz,optical_geometry[first+2u].xyz,source_error);
                if result==1u{return 1u;}if result==2u{uncertain=true;}
            }
            node=next;
        }
        index=escape;
    }
    return select(0u,2u,uncertain);
}
