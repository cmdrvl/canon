#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::block::{
        BlockCandidateBudgetConfig, BlockCandidateBudgetObservation,
        validate_block_candidate_budget_before_artifact_emission,
    },
};
use serde_json::json;

#[test]
fn block_candidate_budget_matrix_allows_within_caps_and_reports_diagnostics() {
    let diagnostics = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 13, 16),
        &[
            observation("surface-a", "exact_view", 2, 0),
            observation("surface-a", "rare_token", 3, 1),
            observation("surface-b", "exact_view", 1, 0),
            observation("surface-c", "rare_token", 10, 4),
        ],
    )
    .expect("within-budget block may emit a candidate artifact");

    assert_eq!(diagnostics.candidate_pairs_emitted, 16);
    assert_eq!(diagnostics.candidate_pairs_suppressed_by_cap, 5);
    assert_eq!(diagnostics.suppressed_candidate_count, 5);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p50, 5);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p95, 10);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p99, 10);
    assert_eq!(diagnostics.max_candidates_for_surface, 10);
    assert_eq!(diagnostics.max_candidates_for_operator, 13);
    assert!(diagnostics.candidate_budget.validated);
    assert_eq!(
        diagnostics.candidate_budget.policy_id,
        "block.max_candidates_per_run"
    );
    assert_eq!(diagnostics.candidate_budget.observed, 16);
    assert_eq!(diagnostics.candidate_budget.configured, 16);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

#[test]
fn block_candidate_budget_matrix_refuses_before_artifact_write_on_surface_breach() {
    let refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(5, 100, 100),
        &[
            observation("surface-z", "rare_token", 6, 2),
            observation("surface-a", "rare_token", 6, 1),
        ],
    )
    .expect_err("over-budget surface refuses before candidate artifact emission");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["artifact"], "candidate_artifact");
    assert_eq!(refusal.detail["reason"], "candidate_budget_exceeded");
    assert_eq!(
        refusal.detail["policy_id"],
        "block.max_candidates_per_surface"
    );
    assert_eq!(
        refusal.detail["budget"]["policy_id"],
        "block.max_candidates_per_surface"
    );
    assert_eq!(refusal.detail["subject_kind"], "surface");
    assert_eq!(refusal.detail["subject_id"], "surface-a");
    assert_eq!(refusal.detail["observed"], 6);
    assert_eq!(refusal.detail["configured"], 5);
    assert_eq!(refusal.detail["candidate_pairs_per_surface_p95"], 6);
    assert_eq!(refusal.detail["candidate_pairs_per_surface_p99"], 6);
    assert_eq!(refusal.detail["suppressed_candidate_count"], 3);
    assert_eq!(
        refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|command| command.contains("per-surface cap"))
    );
}

#[test]
fn block_candidate_budget_matrix_refuses_operator_and_run_breaches_deterministically() {
    let operator_refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 5, 100),
        &[
            observation("surface-a", "operator-z", 4, 0),
            observation("surface-b", "operator-a", 4, 0),
            observation("surface-c", "operator-a", 2, 0),
        ],
    )
    .expect_err("over-budget operator refuses");

    assert_eq!(
        operator_refusal.detail["policy_id"],
        "block.max_candidates_per_operator"
    );
    assert_eq!(operator_refusal.detail["subject_kind"], "operator");
    assert_eq!(operator_refusal.detail["subject_id"], "operator-a");
    assert_eq!(operator_refusal.detail["observed"], 6);
    assert_eq!(operator_refusal.detail["configured"], 5);
    assert_eq!(
        operator_refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );

    let run_refusal = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(10, 10, 5),
        &[
            observation("surface-a", "operator-z", 3, 0),
            observation("surface-b", "operator-a", 3, 0),
        ],
    )
    .expect_err("over-budget run refuses");

    assert_eq!(
        run_refusal.detail["policy_id"],
        "block.max_candidates_per_run"
    );
    assert_eq!(run_refusal.detail["subject_kind"], "run");
    assert_eq!(run_refusal.detail["subject_id"], json!(null));
    assert_eq!(run_refusal.detail["observed"], 6);
    assert_eq!(run_refusal.detail["configured"], 5);
    assert_eq!(
        run_refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
}

#[test]
fn block_candidate_budget_matrix_empty_observations_report_zeroes() {
    let diagnostics = validate_block_candidate_budget_before_artifact_emission(
        &BlockCandidateBudgetConfig::new(1, 1, 1),
        &[],
    )
    .expect("empty candidate stream is within budget");

    assert_eq!(diagnostics.candidate_pairs_emitted, 0);
    assert_eq!(diagnostics.suppressed_candidate_count, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p50, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p95, 0);
    assert_eq!(diagnostics.candidate_pairs_per_surface_p99, 0);
    assert_eq!(diagnostics.max_candidates_for_surface, 0);
    assert_eq!(diagnostics.max_candidates_for_operator, 0);
    assert!(diagnostics.candidate_budget.validated);
    assert_eq!(diagnostics.candidate_budget.observed, 0);
    assert_eq!(diagnostics.candidate_budget.configured, 1);
    assert!(!diagnostics.partial_candidate_artifact_written);
}

fn observation(
    surface_id: &str,
    operator_id: &str,
    emitted_candidate_count: u64,
    suppressed_candidate_count: u64,
) -> BlockCandidateBudgetObservation {
    BlockCandidateBudgetObservation::new(
        surface_id,
        operator_id,
        emitted_candidate_count,
        suppressed_candidate_count,
    )
}
