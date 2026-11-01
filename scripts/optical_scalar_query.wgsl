// Original MIT. Unregistered evaluator diagnostic, without an acceptance guard.
// The host audits each returned candidate against the actual rounded query.
// It must not be substituted into an integrator based on this cost alone.
struct Query {q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,latitude:vec4<f32>,position:vec4<f32>};
@group(0) @binding(0) var<storage,read> queries:array<Query>;
@group(0) @binding(1) var<storage,read_write> results:array<vec4<u32>>;
fn query_mul(q:Query,p:vec3<f32>)->vec3<f32>{return vec3(dot(q.q0.xyz,p),dot(q.q1.xyz,p),dot(q.q2.xyz,p));}
fn explicit_normal(v:vec3<f32>)->vec3<f32>{return v*inverseSqrt(dot(v,v));}
fn finite4(v:vec4<f32>)->bool{return all(abs(v)<=vec4(3.0e38));}
struct Candidate {value:vec4<f32>,input_residual:vec4<f32>,qf:vec4<f32>,qnv:vec4<f32>,diagnostics:vec4<f32>,gradient_norm:vec4<f32>};
fn scalar_query(q:Query)->Candidate{
    let p=q.position.xyz;let mp=query_mul(q,p);let qp=q.origin.xyz+mp;
    let f=q.origin.w+dot(p,2.0*q.origin.xyz+mp);
    var failed=Candidate(vec4(0.0),vec4(0.0),vec4(qp,f),vec4(0.0),vec4(0.0),vec4(0.0));
    if !finite4(vec4(qp,f)) || !(dot(qp,qp)>0.5625){return failed;}
    let axis_input=vec3(0.0,q.latitude.x,-q.latitude.y);
    if !(dot(axis_input,axis_input)>0.5){return failed;}
    let axis=explicit_normal(axis_input);
    let alpha=1.0/6378137.0;let beta=6378137.0/(6356752.314245179*6356752.314245179);
    let z=dot(qp,axis);let transverse=qp-axis*z;let p2=dot(transverse,transverse);let z2=z*z;
    var mu=f/(2.0*(p2+z2));
    for(var i=0;i<2;i+=1){
        let da=1.0+mu*alpha;let db=1.0+mu*beta;
        if !(da>0.5 && db>0.5) || !finite4(vec4(da,db,mu,0.0)){return failed;}
        let value=f-mu*(p2*(2.0+mu*alpha)/(da*da)+z2*(2.0+mu*beta)/(db*db));
        let derivative=-2.0*(p2/(da*da*da)+z2/(db*db*db));
        if !(derivative<0.0) || !finite4(vec4(value,derivative,0.0,0.0)){return failed;}
        mu-=value/derivative;
    }
    let correction=z*mu*(beta-alpha)/(1.0+mu*beta);
    let seed=qp-axis*correction;
    if !finite4(vec4(seed,mu)) || !(dot(seed,seed)>0.5){return failed;}
    let normal=explicit_normal(seed);let qn=query_mul(q,normal);
    let v=dot(normal,qp);let coefficient=dot(normal,qn);
    let discriminant=v*v-f*coefficient;
    if !(discriminant>=0.0) || !finite4(vec4(v,coefficient,discriminant,0.0)){return failed;}
    let h=f/(v+sqrt(discriminant));let raw=qp-h*qn;
    if !finite4(vec4(raw,h)) || !(dot(raw,raw)>0.5){return failed;}
    let next=explicit_normal(raw);let residual=f-h*(2.0*v-h*coefficient);
    if !finite4(vec4(next,residual)){return failed;}
    return Candidate(vec4(next,h),vec4(normal,residual),vec4(qp,f),vec4(qn,v),vec4(coefficient,length(next-normal),1.0,0.0),vec4(raw,dot(normal,normal)));
}
// query_legacy is injected verbatim from production apart from parameter names.
@compute @workgroup_size(32)
fn legacy_cost(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=917504u{return;}
    results[id.x]=bitcast<vec4<u32>>(query_legacy(queries[id.x%224u]));
}
@compute @workgroup_size(32)
fn candidate_cost(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=917504u{return;}
    results[id.x]=bitcast<vec4<u32>>(scalar_query(queries[id.x%224u]).value);
}
@compute @workgroup_size(32)
fn query_details(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=224u{return;}
    let q=queries[id.x];let c=scalar_query(q);let offset=id.x*7u;
    results[offset]=bitcast<vec4<u32>>(c.value);
    results[offset+1u]=bitcast<vec4<u32>>(c.input_residual);
    results[offset+2u]=bitcast<vec4<u32>>(c.qf);
    results[offset+3u]=bitcast<vec4<u32>>(c.qnv);
    results[offset+4u]=bitcast<vec4<u32>>(c.diagnostics);
    results[offset+5u]=bitcast<vec4<u32>>(c.gradient_norm);
    results[offset+6u]=bitcast<vec4<u32>>(query_legacy(q));
}
