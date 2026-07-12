#![forbid(unsafe_code)]

use canon::entity::run::{NativeScaleProofConfig, prove_native_engine_scale_offline};
use std::time::Instant;

#[test]
fn entity_native_smoke_tier_compiles_and_runs() {
    let proof = prove_native_engine_scale_offline(NativeScaleProofConfig::smoke())
        .expect("native smoke proof");

    assert_eq!(proof.intake.observation_count, 50_000);
    assert!(proof.intake.unique_surface_count > 0);
    assert!(proof.index.token_count > 0);
    assert!(proof.block.candidate_record_count > 0);
    assert!(proof.edge_record_count > 0);
}

#[test]
#[ignore = "records local wall-time metrics for the published 500k tier"]
fn entity_native_500k_metrics_tier() {
    let start = Instant::now();
    let proof = prove_native_engine_scale_offline(NativeScaleProofConfig::offline_500k())
        .expect("native 500k proof");
    let wall_ms = start.elapsed().as_millis();

    eprintln!(
        "entity_native tier=500k wall_ms={} observations={} surfaces={} candidates={} artifact_bytes={} hash={}",
        wall_ms,
        proof.intake.observation_count,
        proof.intake.unique_surface_count,
        proof.block.candidate_record_count,
        proof.artifact_publication.artifact_bytes,
        proof.artifact_content_hash
    );
}
