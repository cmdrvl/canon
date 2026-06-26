#![forbid(unsafe_code)]

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION,
        block_artifact::{
            CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
            EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketContractError,
            ExactBucketDiagnostics, ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
        },
        budget::{BudgetBreach, BudgetLimit, BudgetStage, find_budget_policy},
        edge::EdgeCandidateBudgetProof,
        error::EntityRefusalKind,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const BLOCK_STAGE: &str = "block";
pub const BLOCK_CANDIDATE_ARTIFACT: &str = "candidate_artifact";
pub const BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL: bool = false;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateBudgetConfig {
    pub max_candidates_per_surface: u64,
    pub max_candidates_per_operator: u64,
    pub max_candidates_per_run: u64,
}

impl BlockCandidateBudgetConfig {
    pub const fn new(
        max_candidates_per_surface: u64,
        max_candidates_per_operator: u64,
        max_candidates_per_run: u64,
    ) -> Self {
        Self {
            max_candidates_per_surface,
            max_candidates_per_operator,
            max_candidates_per_run,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCandidateBudgetObservation {
    pub surface_id: String,
    pub operator_id: String,
    pub emitted_candidate_count: u64,
    pub suppressed_candidate_count: u64,
}

impl BlockCandidateBudgetObservation {
    pub fn new(
        surface_id: impl Into<String>,
        operator_id: impl Into<String>,
        emitted_candidate_count: u64,
        suppressed_candidate_count: u64,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            operator_id: operator_id.into(),
            emitted_candidate_count,
            suppressed_candidate_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockCandidateBudgetDiagnostics {
    pub candidate_pairs_emitted: u64,
    pub candidate_pairs_suppressed_by_cap: u64,
    pub suppressed_candidate_count: u64,
    pub candidate_pairs_per_surface_p50: u64,
    pub candidate_pairs_per_surface_p95: u64,
    pub candidate_pairs_per_surface_p99: u64,
    pub max_candidates_for_surface: u64,
    pub max_candidates_for_operator: u64,
    pub candidate_budget: EdgeCandidateBudgetProof,
    pub partial_candidate_artifact_written: bool,
}

pub fn validate_block_candidate_budget_before_artifact_emission(
    config: &BlockCandidateBudgetConfig,
    observations: &[BlockCandidateBudgetObservation],
) -> Result<BlockCandidateBudgetDiagnostics, Refusal> {
    let summary = summarize_candidate_budget(config, observations);

    if let Some(breach) = first_candidate_budget_breach(
        config,
        &summary.surface_totals,
        &summary.operator_totals,
        summary.diagnostics.candidate_pairs_emitted,
    ) {
        return Err(block_candidate_budget_refusal(
            &breach,
            &summary.diagnostics,
        ));
    }

    Ok(summary.diagnostics)
}

fn summarize_candidate_budget(
    config: &BlockCandidateBudgetConfig,
    observations: &[BlockCandidateBudgetObservation],
) -> BlockCandidateBudgetSummary {
    let mut surface_totals = BTreeMap::<String, u64>::new();
    let mut operator_totals = BTreeMap::<String, u64>::new();
    let mut candidate_pairs_emitted = 0_u64;
    let mut suppressed_candidate_count = 0_u64;

    for observation in observations {
        candidate_pairs_emitted =
            candidate_pairs_emitted.saturating_add(observation.emitted_candidate_count);
        suppressed_candidate_count =
            suppressed_candidate_count.saturating_add(observation.suppressed_candidate_count);
        let surface_total = surface_totals
            .entry(observation.surface_id.clone())
            .or_default();
        *surface_total = (*surface_total).saturating_add(observation.emitted_candidate_count);
        let operator_total = operator_totals
            .entry(observation.operator_id.clone())
            .or_default();
        *operator_total = (*operator_total).saturating_add(observation.emitted_candidate_count);
    }

    let mut per_surface_counts = surface_totals.values().copied().collect::<Vec<_>>();
    per_surface_counts.sort_unstable();

    BlockCandidateBudgetSummary {
        diagnostics: BlockCandidateBudgetDiagnostics {
            candidate_pairs_emitted,
            candidate_pairs_suppressed_by_cap: suppressed_candidate_count,
            suppressed_candidate_count,
            candidate_pairs_per_surface_p50: nearest_rank_percentile(&per_surface_counts, 50),
            candidate_pairs_per_surface_p95: nearest_rank_percentile(&per_surface_counts, 95),
            candidate_pairs_per_surface_p99: nearest_rank_percentile(&per_surface_counts, 99),
            max_candidates_for_surface: surface_totals.values().copied().max().unwrap_or(0),
            max_candidates_for_operator: operator_totals.values().copied().max().unwrap_or(0),
            candidate_budget: EdgeCandidateBudgetProof::within_run_budget(
                candidate_pairs_emitted,
                config.max_candidates_per_run,
            ),
            partial_candidate_artifact_written: false,
        },
        surface_totals,
        operator_totals,
    }
}

fn first_candidate_budget_breach(
    config: &BlockCandidateBudgetConfig,
    surface_totals: &BTreeMap<String, u64>,
    operator_totals: &BTreeMap<String, u64>,
    candidate_pairs_emitted: u64,
) -> Option<BlockCandidateBudgetBreach> {
    if let Some((surface_id, observed)) =
        largest_over_limit(surface_totals, config.max_candidates_per_surface)
    {
        return Some(BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerSurface,
            observed,
            config.max_candidates_per_surface,
            "surface",
            Some(surface_id),
        ));
    }

    if let Some((operator_id, observed)) =
        largest_over_limit(operator_totals, config.max_candidates_per_operator)
    {
        return Some(BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerOperator,
            observed,
            config.max_candidates_per_operator,
            "operator",
            Some(operator_id),
        ));
    }

    (candidate_pairs_emitted > config.max_candidates_per_run).then(|| {
        BlockCandidateBudgetBreach::new(
            BudgetLimit::MaxCandidatesPerRun,
            candidate_pairs_emitted,
            config.max_candidates_per_run,
            "run",
            None,
        )
    })
}

fn largest_over_limit(counts: &BTreeMap<String, u64>, limit: u64) -> Option<(String, u64)> {
    let mut best = None::<(&String, u64)>;
    for (id, count) in counts {
        if *count <= limit {
            continue;
        }
        let replace = best.is_none_or(|(best_id, best_count)| {
            *count > best_count || (*count == best_count && id < best_id)
        });
        if replace {
            best = Some((id, *count));
        }
    }
    best.map(|(id, count)| (id.clone(), count))
}

fn nearest_rank_percentile(sorted_counts: &[u64], percentile: u64) -> u64 {
    if sorted_counts.is_empty() {
        return 0;
    }
    let rank = ((sorted_counts.len() as u64) * percentile)
        .div_ceil(100)
        .max(1);
    let index = (rank as usize)
        .saturating_sub(1)
        .min(sorted_counts.len() - 1);
    sorted_counts[index]
}

fn block_candidate_budget_refusal(
    breach: &BlockCandidateBudgetBreach,
    diagnostics: &BlockCandidateBudgetDiagnostics,
) -> Refusal {
    EntityRefusalKind::CandidateBudget.to_refusal(
        "Block candidate budget exceeded before candidate artifact emission",
        json!({
            "stage": BLOCK_STAGE,
            "artifact": BLOCK_CANDIDATE_ARTIFACT,
            "reason": "candidate_budget_exceeded",
            "policy_id": breach.budget.policy_id,
            "subject_kind": breach.subject_kind,
            "subject_id": breach.subject_id,
            "observed": breach.budget.observed,
            "configured": breach.budget.configured,
            "budget": breach.budget,
            "candidate_pairs_emitted": diagnostics.candidate_pairs_emitted,
            "candidate_pairs_suppressed_by_cap": diagnostics.candidate_pairs_suppressed_by_cap,
            "suppressed_candidate_count": diagnostics.suppressed_candidate_count,
            "candidate_pairs_per_surface_p50": diagnostics.candidate_pairs_per_surface_p50,
            "candidate_pairs_per_surface_p95": diagnostics.candidate_pairs_per_surface_p95,
            "candidate_pairs_per_surface_p99": diagnostics.candidate_pairs_per_surface_p99,
            "candidate_artifact_written": BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL,
            "partial_candidate_artifact_written": BLOCK_PARTIAL_CANDIDATE_ARTIFACT_WRITTEN_ON_REFUSAL
        }),
        Some(breach.budget.next_command.to_string()),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockCandidateBudgetSummary {
    diagnostics: BlockCandidateBudgetDiagnostics,
    surface_totals: BTreeMap<String, u64>,
    operator_totals: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockCandidateBudgetBreach {
    budget: BudgetBreach,
    subject_kind: &'static str,
    subject_id: Option<String>,
}

impl BlockCandidateBudgetBreach {
    fn new(
        limit: BudgetLimit,
        observed: u64,
        configured: u64,
        subject_kind: &'static str,
        subject_id: Option<String>,
    ) -> Self {
        let policy = find_budget_policy(BudgetStage::Block, limit)
            .expect("block candidate budget policy is defined");
        Self {
            budget: policy.breach(observed, configured),
            subject_kind,
            subject_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockRequest {
    pub profile: ExactBucketProfile,
    pub upstream: ExactBucketUpstream,
    pub operator_id: String,
    pub identity_view: String,
    pub placeholder_values: BTreeSet<String>,
    pub surfaces: Vec<ExactBucketSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketSurface {
    pub surface_id: String,
    pub bucket_value: String,
    pub row_count: u64,
    pub deal_count: u64,
}

impl ExactBucketSurface {
    pub fn new(
        surface_id: impl Into<String>,
        bucket_value: impl Into<String>,
        row_count: u64,
        deal_count: u64,
    ) -> Self {
        Self {
            surface_id: surface_id.into(),
            bucket_value: bucket_value.into(),
            row_count,
            deal_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBucketBlockResult {
    pub assertions: Vec<ExactBucketAssertion>,
    pub diagnostics: ExactBucketBlockDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExactBucketBlockDiagnostics {
    pub exact_bucket_count: u64,
    pub emitted_bucket_count: u64,
    pub excluded_placeholder_bucket_count: u64,
    pub expanded_pair_count: u64,
    pub suppressed_pair_count: u64,
    pub largest_bucket_size: u64,
    pub membership_record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactBucketEmissionError {
    Contract(ExactBucketContractError),
}

pub fn emit_exact_bucket_hyperedges(
    request: ExactBucketBlockRequest,
) -> Result<ExactBucketBlockResult, ExactBucketEmissionError> {
    let mut groups = BTreeMap::<String, ExactBucketGroup>::new();
    let mut excluded_placeholder_values = BTreeSet::<String>::new();

    for surface in request.surfaces {
        let bucket_value = surface.bucket_value.trim();
        if bucket_value.is_empty() {
            continue;
        }
        if request.placeholder_values.contains(bucket_value) {
            excluded_placeholder_values.insert(bucket_value.to_string());
            continue;
        }
        let group = groups.entry(bucket_value.to_string()).or_default();
        group.surface_ids.insert(surface.surface_id);
        group.row_count = group.row_count.saturating_add(surface.row_count);
        group.deal_count = group.deal_count.saturating_add(surface.deal_count);
    }

    let mut diagnostics = ExactBucketBlockDiagnostics {
        excluded_placeholder_bucket_count: excluded_placeholder_values.len() as u64,
        ..ExactBucketBlockDiagnostics::default()
    };
    let mut assertions = Vec::with_capacity(groups.len());

    for (bucket_value, group) in groups {
        let surface_ids = group.surface_ids.into_iter().collect::<Vec<_>>();
        let suppressed_pair_count = suppressed_pair_count(group.row_count);
        let assertion = ExactBucketAssertion {
            version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
            bucket_id: format!("bucket:{}:{bucket_value}", request.identity_view),
            operator_id: request.operator_id.clone(),
            profile: request.profile.clone(),
            upstream: request.upstream.clone(),
            membership: ExactBucketMembership {
                surface_ids,
                surface_ranges: Vec::new(),
            },
            row_count: group.row_count,
            deal_count: group.deal_count,
            pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
            diagnostics: ExactBucketDiagnostics {
                largest_bucket_size: group.row_count,
                suppressed_pair_count,
                labels: BTreeMap::from([
                    ("identity_view".to_string(), request.identity_view.clone()),
                    ("bucket_value".to_string(), bucket_value),
                ]),
            },
            cannot_link_validation: CannotLinkValidationHook {
                status: CannotLinkValidationStatus::NotChecked,
                checked_fact_count: 0,
                hard_cannot_link_count: 0,
                action: CannotLinkAction::RequireReview,
            },
        };
        assertion
            .validate()
            .map_err(ExactBucketEmissionError::Contract)?;

        diagnostics.exact_bucket_count += 1;
        diagnostics.emitted_bucket_count += 1;
        diagnostics.expanded_pair_count += assertion.expanded_pair_count();
        diagnostics.suppressed_pair_count = diagnostics
            .suppressed_pair_count
            .saturating_add(suppressed_pair_count);
        diagnostics.largest_bucket_size = diagnostics.largest_bucket_size.max(assertion.row_count);
        diagnostics.membership_record_count = diagnostics
            .membership_record_count
            .saturating_add(assertion.artifact_membership_record_count());
        assertions.push(assertion);
    }

    Ok(ExactBucketBlockResult {
        assertions,
        diagnostics,
    })
}

fn suppressed_pair_count(row_count: u64) -> u64 {
    row_count.saturating_mul(row_count.saturating_sub(1)) / 2
}

#[derive(Debug, Clone, Default)]
struct ExactBucketGroup {
    surface_ids: BTreeSet<String>,
    row_count: u64,
    deal_count: u64,
}
