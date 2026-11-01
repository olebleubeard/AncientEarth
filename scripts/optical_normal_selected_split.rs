//! Original MIT. Unregistered compiler-stage split of selected continuation.
//! The accepted-trial and commit expressions, certificate, scene query and
//! record ABI are unchanged. Only the phase scheduler is separated so that
//! the physical integrator and interval-map certificate compile independently.
use super::{SelectedFixture, function, remove_entry};

const PRODUCE: &str = r#"fn normal_selected_produce(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=NS_COUNT{return;}ns_load(id.x);
    if ns.phase==1u||ns.phase==2u||ns.phase==4u{return;}
    if ns.phase==3u{
        if ns.event.kind==0u{
            let next_piece=ns.piece_index+1u;
            if next_piece<ns.pieces&&ns_piece_range(next_piece,ns.pieces).x<ns.curve.theta_end{
                ns.piece_index=next_piece;ns.phase=1u;ns_store();return;
            }
        }
        let result=ns_commit(ns.event);
        if result.status!=4u{ns_publish(result);return;}
        ns.phase=0u;
    }
    if ns.phase==0u{
        let result=ns_attempt();
        if result.status!=5u{ns_publish(result);return;}
        // A new accepted interval owns a new partition. Zero is a private
        // scheduling sentinel; no ns_piece_range call receives this count.
        ns.pieces=0u;ns.piece_index=0u;
    }
    // Phase1 is handled by the separate certificate module. The source and
    // physical state remain uncommitted until every prefix piece is clear.
    ns_store();
}"#;

const CERTIFY: &str = r#"
@compute @workgroup_size(32)
fn normal_selected_certify(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=NS_COUNT{return;}ns_load(id.x);if ns.phase!=1u{return;}
    // First admission searches an entire dyadic partition; later admission
    // recomputes only the next required piece using the identical helper.
    if !ns_prepare_piece(ns.pieces==0u){
        ns.event=nc_scene_uncertain();ns.phase=3u;
    }
    ns_store();
}
"#;

pub fn build() -> SelectedFixture {
    let mut result = super::build();
    assert!(result.certificate_shader.is_none());
    let span = function(&result.producer_shader, "normal_selected_produce");
    result.producer_shader.replace_range(span, PRODUCE);
    let mut certificate = result.producer_shader.clone();
    for name in [
        "normal_selected_init",
        "normal_selected_produce",
        "normal_selected_finalize",
    ] {
        remove_entry(&mut certificate, name);
    }
    certificate.push_str(CERTIFY);
    result.certificate_shader = Some(certificate);
    result.scene.cases["selected_continuation"]["certificate_stage"] = serde_json::json!({
        "entry":"normal_selected_certify",
        "workgroup_size":32,
        "sequence":"init; (produce, certify, consume) repeated; finalize",
        "partition_sentinel":"pieces==0 only in phase1 means a new accepted trial; positive counts retain the admitted partition for the next chronological piece",
        "scope":"Same trial, commit, Cartesian certificate and scene predicates in separate modules. New accepted trials reset partition state. Certificate failure delivers the same uncertainty to unchanged commit/retry policy. Fixed rounds remain operational and may leave an invalid unfinished header; they never reset the4096 attempt budget. No compilation-time, numerical or throughput result is inferred."
    });
    result
}
