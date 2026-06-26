#![forbid(unsafe_code)]

//! Operator-facing diagnostics for entity block generation.
//!
//! Candidate generation and budget enforcement live in `entity::block`; this
//! module turns those counters into stable summaries and refusal boundaries
//! that operators can act on.

use crate::{
    Refusal,
    entity::{
        block::{
            BlockCandidateBudgetConfig, BlockCandidateGenerationResult,
            BlockOperatorCandidateDiagnostics,
        },
        budget::{BudgetLimit, BudgetStage, find_budget_policy},
        error::EntityRefusalKind,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDiagnosticsSummary {
    pub stage: String,
    pub configured: BlockConfiguredLimits,
    pub observed: BlockObservedDiagnostics,
    pub boundary_refusals: Vec<BlockBoundaryRefusalDiagnostic>,
    pub top_blocking_operators_by_yield: Vec<BlockOperatorYieldDiagnostic>,
    pub top_large_postings: Vec<BlockLargePostingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockConfiguredLimits {
    pub max_candidates_per_surface: u64,
    pub max_candidates_per_operator: u64,
    pub max_candidates_per_run: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockObservedDiagnostics {
    pub candidate_record_count: u64,
    pub candidate_pairs_emitted: u64,
    pub candidate_pairs_suppressed_by_cap: u64,
    pub suppressed_candidate_count: u64,
    pub pairs_per_surface_p50: u64,
    pub pairs_per_surface_p95: u64,
    pub pairs_per_surface_p99: u64,
    pub max_candidates_for_surface: u64,
    pub max_candidates_for_operator: u64,
    pub large_buckets_suppressed: u64,
    pub candidate_artifact_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockOperatorYieldDiagnostic {
    pub operator_id: String,
    pub input_candidate_count: u64,
    pub emitted_candidate_count: u64,
    pub suppressed_candidate_count: u64,
    pub yield_per_mille: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockLargePostingDiagnostic {
    pub operator_id: String,
    pub suppressed_posting_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockBoundaryRefusalDiagnostic {
    pub policy_id: String,
    pub boundary: BudgetLimit,
    pub refusal_code: String,
}

pub fn summarize_block_candidate_diagnostics(
    config: &BlockCandidateBudgetConfig,
    result: &BlockCandidateGenerationResult,
    candidate_artifact_bytes: u64,
) -> BlockDiagnosticsSummary {
    let mut top_blocking_operators_by_yield = result
        .diagnostics
        .operator_diagnostics
        .iter()
        .map(operator_yield)
        .collect::<Vec<_>>();
    top_blocking_operators_by_yield.sort_by(operator_yield_cmp);

    let mut top_large_postings = result
        .diagnostics
        .operator_diagnostics
        .iter()
        .filter(|operator| operator.large_posting_suppressed_count > 0)
        .map(|operator| BlockLargePostingDiagnostic {
            operator_id: operator.operator_id.clone(),
            suppressed_posting_count: operator.large_posting_suppressed_count,
        })
        .collect::<Vec<_>>();
    top_large_postings.sort_by(large_posting_cmp);

    BlockDiagnosticsSummary {
        stage: "block".to_string(),
        configured: BlockConfiguredLimits {
            max_candidates_per_surface: config.max_candidates_per_surface,
            max_candidates_per_operator: config.max_candidates_per_operator,
            max_candidates_per_run: config.max_candidates_per_run,
        },
        observed: BlockObservedDiagnostics {
            candidate_record_count: result.diagnostics.candidate_record_count,
            candidate_pairs_emitted: result.diagnostics.candidate_pairs_emitted,
            candidate_pairs_suppressed_by_cap: result.diagnostics.candidate_pairs_suppressed_by_cap,
            suppressed_candidate_count: result.diagnostics.suppressed_candidate_count,
            pairs_per_surface_p50: result.diagnostics.candidate_pairs_per_surface_p50,
            pairs_per_surface_p95: result.diagnostics.candidate_pairs_per_surface_p95,
            pairs_per_surface_p99: result.diagnostics.candidate_pairs_per_surface_p99,
            max_candidates_for_surface: result.diagnostics.max_candidates_for_surface,
            max_candidates_for_operator: result.diagnostics.max_candidates_for_operator,
            large_buckets_suppressed: result.diagnostics.large_buckets_suppressed,
            candidate_artifact_bytes,
        },
        boundary_refusals: block_boundary_refusals(),
        top_blocking_operators_by_yield,
        top_large_postings,
    }
}

pub fn block_index_limit_refusal(
    operator_id: impl Into<String>,
    subject_kind: impl Into<String>,
    subject_id: impl Into<String>,
    observed: u64,
    configured: u64,
    candidate_artifact_bytes: u64,
) -> Refusal {
    let policy = find_budget_policy(BudgetStage::Block, BudgetLimit::MaxExactBucketSize)
        .expect("block exact bucket size policy is defined");
    let breach = policy.breach(observed, configured);
    EntityRefusalKind::IndexLimit.to_refusal(
        "Block index limit exceeded before candidate artifact emission",
        json!({
            "stage": "block",
            "artifact": "candidate_artifact",
            "reason": "index_limit_exceeded",
            "refusal_code": breach.refusal_code.as_str(),
            "policy_id": breach.policy_id,
            "operator_id": operator_id.into(),
            "subject_kind": subject_kind.into(),
            "subject_id": subject_id.into(),
            "observed": breach.observed,
            "configured": breach.configured,
            "budget": breach,
            "candidate_artifact_bytes": candidate_artifact_bytes,
            "partial_candidate_artifact_written": false,
            "candidate_artifact_written": false
        }),
        Some(policy.next_command.to_string()),
    )
}

fn block_boundary_refusals() -> Vec<BlockBoundaryRefusalDiagnostic> {
    [
        BudgetLimit::MaxCandidatesPerSurface,
        BudgetLimit::MaxCandidatesPerOperator,
        BudgetLimit::MaxCandidatesPerRun,
        BudgetLimit::MaxExactBucketSize,
    ]
    .into_iter()
    .map(|boundary| {
        let policy = find_budget_policy(BudgetStage::Block, boundary)
            .expect("block budget boundary policy is defined");
        BlockBoundaryRefusalDiagnostic {
            policy_id: policy.id.to_string(),
            boundary,
            refusal_code: policy.refusal_code.as_str().to_string(),
        }
    })
    .collect()
}

fn operator_yield(operator: &BlockOperatorCandidateDiagnostics) -> BlockOperatorYieldDiagnostic {
    BlockOperatorYieldDiagnostic {
        operator_id: operator.operator_id.clone(),
        input_candidate_count: operator.input_candidate_count,
        emitted_candidate_count: operator.emitted_candidate_count,
        suppressed_candidate_count: operator.suppressed_candidate_count,
        yield_per_mille: if operator.input_candidate_count == 0 {
            0
        } else {
            operator
                .emitted_candidate_count
                .saturating_mul(1_000)
                .checked_div(operator.input_candidate_count)
                .unwrap_or(0)
        },
    }
}

fn operator_yield_cmp(
    left: &BlockOperatorYieldDiagnostic,
    right: &BlockOperatorYieldDiagnostic,
) -> std::cmp::Ordering {
    right
        .emitted_candidate_count
        .cmp(&left.emitted_candidate_count)
        .then_with(|| right.yield_per_mille.cmp(&left.yield_per_mille))
        .then_with(|| left.operator_id.cmp(&right.operator_id))
}

fn large_posting_cmp(
    left: &BlockLargePostingDiagnostic,
    right: &BlockLargePostingDiagnostic,
) -> std::cmp::Ordering {
    right
        .suppressed_posting_count
        .cmp(&left.suppressed_posting_count)
        .then_with(|| left.operator_id.cmp(&right.operator_id))
}
