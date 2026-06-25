use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_VERSION,
        edge::{
            EdgeCandidateArtifactExpectation, EdgeCandidateArtifactRef, EdgeCandidateBudgetProof,
            validate_edge_candidate_artifact_before_scoring,
        },
    },
};
use serde_json::json;

#[test]
fn edge_budget_refusal_rejects_over_budget_candidate_artifact_before_scoring() {
    let mut artifact = valid_artifact();
    artifact.candidate_record_count = 101;
    let mut expected = valid_expectation();
    expected.max_edge_records = 100;

    let refusal = validate_edge_candidate_artifact_before_scoring(&artifact, &expected)
        .expect_err("edge record cap refuses before scoring");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "edge");
    assert_eq!(refusal.detail["reason"], "edge_record_budget_exceeded");
    assert_eq!(
        refusal.detail["budget"]["policy_id"],
        "edge.max_edge_records"
    );
    assert_eq!(refusal.detail["budget"]["observed"], 101);
    assert_eq!(refusal.detail["budget"]["configured"], 100);
    assert_eq!(
        refusal.detail["partial_edge_artifact_written"],
        json!(false)
    );
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("canon entity edge"))
    );
}

#[test]
fn edge_budget_refusal_rejects_wrong_or_stale_candidate_artifact() {
    let mut wrong_version = valid_artifact();
    wrong_version.version = "canon_entity_prepare.v0".to_string();
    let refusal =
        validate_edge_candidate_artifact_before_scoring(&wrong_version, &valid_expectation())
            .expect_err("wrong artifact version refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["reason"], "wrong_version");
    assert_eq!(
        refusal.detail["expected_version"],
        CANON_ENTITY_BLOCK_VERSION
    );
    assert_eq!(
        refusal.detail["partial_edge_artifact_written"],
        json!(false)
    );

    let mut stale = valid_artifact();
    stale.registry_snapshot_hash = "blake3:old-registry".to_string();
    let refusal = validate_edge_candidate_artifact_before_scoring(&stale, &valid_expectation())
        .expect_err("stale registry snapshot refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["reason"], "stale_artifact");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["expected"], "blake3:registry");
    assert_eq!(refusal.detail["actual"], "blake3:old-registry");
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_CANDIDATE_BUDGET_refuses_unvalidated_block_budget_before_edge_scoring() {
    let mut artifact = valid_artifact();
    artifact.candidate_budget = EdgeCandidateBudgetProof {
        validated: false,
        policy_id: "block.max_candidates_per_run".to_string(),
        observed: 0,
        configured: 0,
    };

    let refusal = validate_edge_candidate_artifact_before_scoring(&artifact, &valid_expectation())
        .expect_err("missing block budget proof refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["stage"], "edge");
    assert_eq!(refusal.detail["upstream_stage"], "block");
    assert_eq!(refusal.detail["reason"], "candidate_budget_not_validated");
    assert_eq!(refusal.detail["policy_id"], "block.max_candidates_per_run");
    assert_eq!(
        refusal.detail["partial_edge_artifact_written"],
        json!(false)
    );
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("canon entity block"))
    );
}

#[test]
fn edge_budget_validation_allows_matching_candidate_artifact_within_caps() {
    let permit =
        validate_edge_candidate_artifact_before_scoring(&valid_artifact(), &valid_expectation())
            .expect("matching candidate artifact may be scored");

    assert_eq!(permit.candidate_record_count, 42);
    assert_eq!(permit.max_edge_records, 100);
    assert!(!permit.partial_edge_artifact_written);
}

fn valid_artifact() -> EdgeCandidateArtifactRef {
    EdgeCandidateArtifactRef {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        content_hash: "blake3:block-artifact".to_string(),
        candidate_record_count: 42,
        candidate_budget: EdgeCandidateBudgetProof::within_run_budget(42, 100),
    }
}

fn valid_expectation() -> EdgeCandidateArtifactExpectation {
    EdgeCandidateArtifactExpectation {
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        content_hash: "blake3:block-artifact".to_string(),
        max_edge_records: 100,
    }
}
