#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_VERSION_V1,
        block::{
            BlockCandidateBudgetConfig, BlockCandidateBudgetObservation, ExactBucketBlockRequest,
            ExactBucketSurface, emit_exact_bucket_hyperedges,
            validate_block_candidate_budget_before_artifact_emission,
        },
        block_artifact::{ExactBucketProfile, ExactBucketUpstream},
        edge::{
            EdgeCandidateArtifactExpectation, EdgeCandidateArtifactRef, EdgeCandidateBudgetProof,
            validate_edge_candidate_artifact_before_scoring,
        },
        graph::{EntityEvidenceGraph, ExactBucketSolveAction},
        solve::{
            SolveBudgetAction, SolveBudgetConfig, SolveComponentBudgetInput, evaluate_solve_budget,
        },
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[test]
fn budget_exact_bucket_readiness_contracts_compose_without_pair_expansion() {
    let block_budget_refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(2, 8, 8),
        &[BlockCandidateBudgetObservation::new(
            "surf:hot",
            "rare_token_overlap:tenant_tokens",
            3,
            0,
        )],
    )
    .expect_err("over-budget block observation refuses");
    assert_eq!(
        block_budget_refusal.code,
        RefusalCode::EEntityCandidateBudget
    );

    let edge_permit = validate_edge_candidate_artifact_before_scoring(
        &EdgeCandidateArtifactRef {
            version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
            profile_id: "cmbs_tenant_label".to_string(),
            profile_version: "0.1.0".to_string(),
            strategy_hash: "blake3:block-strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
            content_hash: "blake3:block".to_string(),
            candidate_record_count: 2,
            candidate_budget: EdgeCandidateBudgetProof::within_run_budget(2, 8),
        },
        &EdgeCandidateArtifactExpectation {
            profile_id: "cmbs_tenant_label".to_string(),
            profile_version: "0.1.0".to_string(),
            strategy_hash: "blake3:block-strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
            content_hash: "blake3:block".to_string(),
            max_edge_records: 8,
        },
    )
    .expect("edge candidate artifact validates before scoring");

    let solve_budget = evaluate_solve_budget(
        &[SolveComponentBudgetInput::new(
            "component:oversized",
            vec![
                "surf:001".to_string(),
                "surf:002".to_string(),
                "surf:003".to_string(),
            ],
        )],
        SolveBudgetConfig::bounded_abstention(2),
    )
    .expect("solve budget evaluates");
    assert_eq!(
        solve_budget.components[0].action,
        SolveBudgetAction::Abstain
    );

    let exact_bucket = emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_profile(),
        upstream: sample_upstream(),
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::new(),
        surfaces: (0..8_000)
            .map(|ordinal| {
                ExactBucketSurface::new(format!("surf:sears:{ordinal:04}"), "sears", 1, 1)
            })
            .collect(),
    })
    .expect("exact bucket emits compact hyperedge assertion");
    let assertion = &exact_bucket.assertions[0];
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(assertion.artifact_membership_record_count(), 8_000);

    let mut graph = EntityEvidenceGraph::from_exact_bucket_assertions(&exact_bucket.assertions);
    graph.add_hard_cannot_link("surf:sears:0000", "surf:sears:7999");
    let graph_report = graph.solve_exact_bucket_hyperedges();
    assert_eq!(graph_report.expanded_pair_count, 0);
    assert_eq!(
        graph_report.decisions[0].action,
        ExactBucketSolveAction::ReviewContradiction
    );

    let actual = json!({
        "block_budget_refusal_code": block_budget_refusal.detail["refusal_code"],
        "edge_permit_candidate_record_count": edge_permit.candidate_record_count,
        "exact_bucket_expanded_pair_count": assertion.expanded_pair_count(),
        "exact_bucket_hyperedge_count": graph_report.hyperedge_count,
        "exact_bucket_membership_record_count": graph_report.membership_record_count,
        "exact_bucket_theoretical_pair_count": graph_report.theoretical_pair_count,
        "graph_action": graph_report.decisions[0].action,
        "graph_expanded_pair_count": graph_report.expanded_pair_count,
        "solve_abstained_component_count": solve_budget.summary["abstained_component_count"]
    });
    assert_eq!(actual, expected_readiness_summary());
}

fn expected_readiness_summary() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/entity/block/exact_bucket_hyperedge/readiness_expected.json"
    ))
    .expect("readiness fixture parses")
}

fn sample_profile() -> ExactBucketProfile {
    ExactBucketProfile {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        content_hash: "blake3:profile".to_string(),
    }
}

fn sample_upstream() -> ExactBucketUpstream {
    ExactBucketUpstream {
        prepare_hash: "blake3:prepare".to_string(),
        index_hash: "blake3:index".to_string(),
        strategy_hash: "blake3:block-strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
    }
}
