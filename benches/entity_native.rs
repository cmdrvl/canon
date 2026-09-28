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

#[test]
#[ignore = "synthetic shared-posting stress tier; not live-corpus evidence"]
fn entity_ngram_early_budget_metrics_tier() {
    use canon::entity::{
        block::{
            BlockCandidateBudgetConfig,
            declared::{Blocking, generate},
        },
        prepare::{PreparedSurfaceRecord, load_prepare_profile},
    };
    let mut profile = load_prepare_profile("instrument_identity").unwrap();
    profile.blocking = Some(serde_json::from_value::<Blocking>(serde_json::json!({"operators":[{"op":"ngram_topk","view":"core","k":25,"candidate_cap":25}]})).unwrap());
    let surfaces = (0..5_000).map(|ordinal| serde_json::from_value::<PreparedSurfaceRecord>(serde_json::json!({
        "surface_id":format!("s{ordinal:05}"), "profile_id":"instrument_identity", "surface_key":format!("k{ordinal}"),
        "primary_surface":"common shared title", "normalized_views":{"core":{"value":"common shared title","reason_codes":["surface_id_view"]}},
        "exact_lookup":{"status":"unresolved","canonical_id":null,"canonical_type":null,"rule_id":null,"matched_input":null},
        "raw_variants":[],"alias_surfaces":[],"mention_surfaces":[],"row_count":1,"deal_count":0,"provenance_samples":[]
    })).unwrap()).collect::<Vec<_>>();
    let start = Instant::now();
    let refusal = generate(
        &profile,
        &surfaces,
        BlockCandidateBudgetConfig::new(100, 25_000, 25_000),
    )
    .unwrap_err();
    assert_eq!(refusal.detail["surfaces_visited"], 1001);
    assert_eq!(refusal.detail["observed_at_stop"], 25025);
    eprintln!(
        "entity_ngram synthetic=true surfaces={} visited={} wall_ms={} observed_at_stop={} candidate_artifact_written={}",
        surfaces.len(),
        refusal.detail["surfaces_visited"],
        start.elapsed().as_millis(),
        refusal.detail["observed_at_stop"],
        refusal.detail["candidate_artifact_written"]
    );
}
