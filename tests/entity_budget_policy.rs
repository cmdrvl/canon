#![forbid(unsafe_code)]

#[path = "../src/entity/budget.rs"]
mod budget;

use budget::{
    BudgetEnforcement, BudgetLimit, BudgetStage, EntityBudgetRefusalCode, default_budget_policies,
    find_budget_policy,
};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn entity_budget_policy_covers_required_limits() {
    let policies = default_budget_policies();
    let ids = policies
        .iter()
        .map(|policy| policy.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        policies.len(),
        "budget policy ids must be unique"
    );

    for required in [
        "index.max_posting_list_entries",
        "block.max_candidates_per_surface",
        "block.max_candidates_per_operator",
        "block.max_candidates_per_run",
        "block.max_exact_bucket_size",
        "edge.max_edge_records",
        "solve.max_component_size",
        "review.max_review_groups",
        "all_large_stages.max_artifact_bytes",
        "all_large_stages.max_rows",
        "all_large_stages.max_bytes",
        "apply.require_full_resolution",
    ] {
        assert!(ids.contains(required), "missing budget policy {required}");
    }
}

#[test]
fn entity_budget_policy_maps_stage_breaches_to_stable_refusals() {
    assert_policy(
        BudgetStage::Index,
        BudgetLimit::MaxPostingListEntries,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::IndexLimit,
    );
    assert_policy(
        BudgetStage::Block,
        BudgetLimit::MaxCandidatesPerRun,
        BudgetEnforcement::RefuseBeforeEmission,
        EntityBudgetRefusalCode::CandidateBudget,
    );
    assert_policy(
        BudgetStage::Edge,
        BudgetLimit::MaxEdgeRecords,
        BudgetEnforcement::RefuseBeforeScoring,
        EntityBudgetRefusalCode::ArtifactContract,
    );
    assert_policy(
        BudgetStage::Solve,
        BudgetLimit::MaxComponentSize,
        BudgetEnforcement::BoundedAbstention,
        EntityBudgetRefusalCode::ArtifactContract,
    );
    assert_policy(
        BudgetStage::Apply,
        BudgetLimit::RequireFullResolutionApply,
        BudgetEnforcement::RefuseBeforeOutput,
        EntityBudgetRefusalCode::ApplyUnresolved,
    );
}

#[test]
fn entity_budget_policy_breach_detail_names_observed_configured_and_recovery() {
    let policy = find_budget_policy(BudgetStage::Block, BudgetLimit::MaxCandidatesPerSurface)
        .expect("block candidate budget policy exists");
    let breach = policy.breach(101, 100);

    assert_eq!(breach.policy_id, "block.max_candidates_per_surface");
    assert_eq!(breach.observed, 101);
    assert_eq!(breach.configured, 100);
    assert_eq!(breach.refusal_code.as_str(), "E_ENTITY_CANDIDATE_BUDGET");
    assert!(breach.next_command.contains("canon entity block"));
}

#[test]
fn entity_budget_policy_serialization_is_stable_and_operator_readable() {
    let policy = find_budget_policy(BudgetStage::Apply, BudgetLimit::RequireFullResolutionApply)
        .expect("apply full-resolution policy exists");
    let value = serde_json::to_value(policy).expect("budget policy serializes");

    assert_eq!(
        value,
        json!({
            "id": "apply.require_full_resolution",
            "stage": "apply",
            "limit": "require_full_resolution_apply",
            "enforcement": "refuse_before_output",
            "refusal_code": "E_ENTITY_APPLY_UNRESOLVED",
            "next_command": "Promote more exact aliases or rerun canon entity apply without full-resolution mode"
        })
    );

    let breach_value = serde_json::to_value(policy.breach(7, 0)).expect("budget breach serializes");
    assert_eq!(breach_value["observed"], 7);
    assert_eq!(breach_value["configured"], 0);
    assert_eq!(breach_value["refusal_code"], "E_ENTITY_APPLY_UNRESOLVED");
}

fn assert_policy(
    stage: BudgetStage,
    limit: BudgetLimit,
    enforcement: BudgetEnforcement,
    refusal_code: EntityBudgetRefusalCode,
) {
    let policy = find_budget_policy(stage, limit).expect("budget policy exists");
    assert_eq!(policy.enforcement, enforcement);
    assert_eq!(policy.refusal_code, refusal_code);
    assert!(!policy.next_command.is_empty());
}
