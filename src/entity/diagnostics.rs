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
        edge::EdgeEvidenceHit,
        error::EntityRefusalKind,
        evidence::{
            ExactViewSupportRequest, StringSimilaritySupportRequest, exact_view_support_hit,
            string_similarity_support_hit,
        },
        prepare::PreparedSurfaceRecord,
        profile::{EntityOperatorSpec, EntityProfileDocument},
        record_link::{
            RecordLinkFeaturePolicy, RecordLinkFeatureValue, record_link_self_support_feature,
        },
        score::{
            ENTITY_SCORE_SCALE, ScoreBreakdown, ScoreContribution, ScoreLane, ScoreUnits,
            accumulate_score_units,
        },
        tfidf_evidence::{TfidfCosineSupportRequest, tfidf_cosine_support_hit},
    },
    namekit::similarity::SimilarityMetric,
    namekit::tfidf::{SparseTfidfModel, TfidfInputSurface},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{cmp::Ordering, collections::BTreeMap};

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

pub const ENTITY_UNLINKABLES_REPORT_TYPE: &str = "unlinkables";
pub const ENTITY_UNLINKABLES_EVALUATION_MODE: &str = "hypothetical_perfect_twin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesReport {
    pub report_type: String,
    pub evaluation_mode: String,
    pub score_units_scale: u32,
    pub thresholds: EntityUnlinkablesThresholds,
    pub denominator: EntityUnlinkablesDenominator,
    pub distribution: Vec<EntityUnlinkablesScoreBucket>,
    pub surfaces: Vec<EntityUnlinkablesSurfaceCeiling>,
    pub unlinkable_surfaces: Vec<EntityUnlinkableSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesThresholds {
    pub threshold_source: String,
    pub attach_score_min_units: u32,
    pub backbone_score_min_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesDenominator {
    pub subject_surface_count: u64,
    pub unique_prepared_surface_count: u64,
    pub reference_surface_count: u64,
    pub target_surface_count: u64,
    pub unassigned_surface_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesScoreBucket {
    pub max_attainable_support_units: u32,
    pub surface_count: u64,
    pub below_attach_threshold: bool,
    pub below_backbone_threshold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesSurfaceCeiling {
    pub side: EntityUnlinkablesSurfaceSide,
    pub surface_id: String,
    pub profile_id: String,
    pub link_ids: Vec<String>,
    pub max_attainable_support_units: u32,
    pub raw_attainable_support_units: u64,
    pub below_attach_threshold: bool,
    pub below_backbone_threshold: bool,
    pub score_breakdown: ScoreBreakdown,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_field_costs: Vec<EntityUnlinkablesMissingFieldCost>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_operators: Vec<EntityUnlinkablesUnsupportedOperator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkableSurface {
    pub side: EntityUnlinkablesSurfaceSide,
    pub surface_id: String,
    pub profile_id: String,
    pub link_ids: Vec<String>,
    pub max_attainable_support_units: u32,
    pub below_thresholds: Vec<String>,
    pub missing_field_costs: Vec<EntityUnlinkablesMissingFieldCost>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_operators: Vec<EntityUnlinkablesUnsupportedOperator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesMissingFieldCost {
    pub field_id: String,
    pub source: String,
    pub operator_id: String,
    pub reason: String,
    pub cost_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityUnlinkablesUnsupportedOperator {
    pub operator_id: String,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityUnlinkablesSurfaceSide {
    Reference,
    Target,
    Unassigned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityUnlinkablesSurfaceInput {
    pub side: EntityUnlinkablesSurfaceSide,
    pub surface: PreparedSurfaceRecord,
    pub link_ids: Vec<String>,
    pub record_link_features: BTreeMap<String, RecordLinkFeatureValue>,
    pub quarantined_record_link_features: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct EntityUnlinkablesReportRequest<'a> {
    pub profile: &'a EntityProfileDocument,
    pub support_namespace: &'a str,
    pub thresholds: EntityUnlinkablesThresholds,
    pub surfaces: Vec<EntityUnlinkablesSurfaceInput>,
    pub record_link_feature_policies: BTreeMap<String, RecordLinkFeaturePolicy>,
}

pub fn build_entity_unlinkables_report(
    request: EntityUnlinkablesReportRequest<'_>,
) -> Result<EntityUnlinkablesReport, Refusal> {
    let mut surfaces = request
        .surfaces
        .iter()
        .map(|surface| {
            entity_unlinkables_surface_ceiling(
                surface,
                request.profile,
                request.support_namespace,
                &request.thresholds,
                &request.record_link_feature_policies,
            )
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    surfaces.sort_by(entity_unlinkables_surface_ceiling_cmp);

    let distribution = entity_unlinkables_distribution(&surfaces, &request.thresholds);
    let denominator = entity_unlinkables_denominator(&surfaces);
    let unlinkable_surfaces = surfaces
        .iter()
        .filter(|surface| surface.below_attach_threshold || surface.below_backbone_threshold)
        .map(entity_unlinkable_surface)
        .collect::<Vec<_>>();

    let report = EntityUnlinkablesReport {
        report_type: ENTITY_UNLINKABLES_REPORT_TYPE.to_string(),
        evaluation_mode: ENTITY_UNLINKABLES_EVALUATION_MODE.to_string(),
        score_units_scale: ENTITY_SCORE_SCALE,
        thresholds: request.thresholds,
        denominator,
        distribution,
        surfaces,
        unlinkable_surfaces,
    };
    validate_entity_unlinkables_report(&report)?;
    Ok(report)
}

pub fn validate_entity_unlinkables_report(report: &EntityUnlinkablesReport) -> Result<(), Refusal> {
    if report.report_type != ENTITY_UNLINKABLES_REPORT_TYPE {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables report has the wrong report type",
            json!({
                "stage": "link",
                "field": "unlinkables.report_type",
                "expected": ENTITY_UNLINKABLES_REPORT_TYPE,
                "actual": report.report_type,
                "writes_performed": false
            }),
        ));
    }
    if report.evaluation_mode != ENTITY_UNLINKABLES_EVALUATION_MODE {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables report has the wrong evaluation mode",
            json!({
                "stage": "link",
                "field": "unlinkables.evaluation_mode",
                "expected": ENTITY_UNLINKABLES_EVALUATION_MODE,
                "actual": report.evaluation_mode,
                "writes_performed": false
            }),
        ));
    }
    if report.score_units_scale != ENTITY_SCORE_SCALE {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables report uses an unexpected score scale",
            json!({
                "stage": "link",
                "field": "unlinkables.score_units_scale",
                "expected": ENTITY_SCORE_SCALE,
                "actual": report.score_units_scale,
                "writes_performed": false
            }),
        ));
    }
    if report.thresholds.attach_score_min_units > ENTITY_SCORE_SCALE
        || report.thresholds.backbone_score_min_units > ENTITY_SCORE_SCALE
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables thresholds exceed the entity score scale",
            json!({
                "stage": "link",
                "field": "unlinkables.thresholds",
                "score_units_scale": ENTITY_SCORE_SCALE,
                "writes_performed": false
            }),
        ));
    }
    if report
        .surfaces
        .windows(2)
        .any(|pair| entity_unlinkables_surface_ceiling_cmp(&pair[0], &pair[1]).is_gt())
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables surfaces must be sorted deterministically",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces",
                "writes_performed": false
            }),
        ));
    }
    if report
        .distribution
        .windows(2)
        .any(|pair| pair[0].max_attainable_support_units >= pair[1].max_attainable_support_units)
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables distribution buckets must be sorted and unique",
            json!({
                "stage": "link",
                "field": "unlinkables.distribution",
                "writes_performed": false
            }),
        ));
    }
    let expected_denominator = entity_unlinkables_denominator(&report.surfaces);
    if report.denominator != expected_denominator {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables denominator does not match surface rows",
            json!({
                "stage": "link",
                "field": "unlinkables.denominator",
                "expected": expected_denominator,
                "actual": report.denominator,
                "writes_performed": false
            }),
        ));
    }
    let expected_distribution =
        entity_unlinkables_distribution(&report.surfaces, &report.thresholds);
    if report.distribution != expected_distribution {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables distribution does not match surface ceilings",
            json!({
                "stage": "link",
                "field": "unlinkables.distribution",
                "expected": expected_distribution,
                "actual": report.distribution,
                "writes_performed": false
            }),
        ));
    }
    for surface in &report.surfaces {
        validate_entity_unlinkables_surface(surface, &report.thresholds)?;
    }
    let expected_unlinkable_surfaces = report
        .surfaces
        .iter()
        .filter(|surface| surface.below_attach_threshold || surface.below_backbone_threshold)
        .map(entity_unlinkable_surface)
        .collect::<Vec<_>>();
    if report.unlinkable_surfaces != expected_unlinkable_surfaces {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables list does not match threshold comparison",
            json!({
                "stage": "link",
                "field": "unlinkables.unlinkable_surfaces",
                "expected": expected_unlinkable_surfaces,
                "actual": report.unlinkable_surfaces,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn entity_unlinkables_surface_ceiling(
    input: &EntityUnlinkablesSurfaceInput,
    profile: &EntityProfileDocument,
    support_namespace: &str,
    thresholds: &EntityUnlinkablesThresholds,
    record_link_feature_policies: &BTreeMap<String, RecordLinkFeaturePolicy>,
) -> Result<EntityUnlinkablesSurfaceCeiling, Refusal> {
    let mut hits = Vec::new();
    let mut missing_field_costs = Vec::new();
    let mut unsupported_operators = Vec::new();

    for spec in &profile.evidence.support {
        score_profile_support_ceiling(
            spec,
            &input.surface,
            support_namespace,
            &mut hits,
            &mut missing_field_costs,
            &mut unsupported_operators,
        )?;
    }
    for (feature_id, policy) in record_link_feature_policies {
        score_record_link_support_ceiling(
            feature_id,
            policy,
            input,
            &mut hits,
            &mut missing_field_costs,
            &mut unsupported_operators,
        )?;
    }

    missing_field_costs.sort_by(entity_missing_field_cost_cmp);
    unsupported_operators.sort_by(entity_unsupported_operator_cmp);
    let score_breakdown = accumulate_score_units(hits.iter().map(|hit| {
        ScoreContribution::new(
            hit.lane,
            format!("{}:{}", hit.namespace, hit.operator_id),
            hit.reason_code.clone(),
            hit.score_units,
        )
    }));
    let max_attainable_support_units = score_breakdown.total_score_units.as_u32();
    let below_attach_threshold = max_attainable_support_units < thresholds.attach_score_min_units;
    let below_backbone_threshold =
        max_attainable_support_units < thresholds.backbone_score_min_units;

    Ok(EntityUnlinkablesSurfaceCeiling {
        side: input.side,
        surface_id: input.surface.surface_id.clone(),
        profile_id: input.surface.profile_id.clone(),
        link_ids: sorted_deduped(input.link_ids.clone()),
        max_attainable_support_units,
        raw_attainable_support_units: score_breakdown.raw_support_score_units,
        below_attach_threshold,
        below_backbone_threshold,
        score_breakdown,
        missing_field_costs,
        unsupported_operators,
    })
}

fn score_profile_support_ceiling(
    spec: &EntityOperatorSpec,
    surface: &PreparedSurfaceRecord,
    support_namespace: &str,
    hits: &mut Vec<EdgeEvidenceHit>,
    missing_field_costs: &mut Vec<EntityUnlinkablesMissingFieldCost>,
    unsupported_operators: &mut Vec<EntityUnlinkablesUnsupportedOperator>,
) -> Result<(), Refusal> {
    match spec.op.as_str() {
        "exact_view" => {
            score_exact_view_ceiling(spec, surface, support_namespace, hits, missing_field_costs)
        }
        "string_similarity" => score_string_similarity_ceiling(
            spec,
            surface,
            support_namespace,
            hits,
            missing_field_costs,
        ),
        "tfidf_cosine" => {
            score_tfidf_ceiling(spec, surface, support_namespace, hits, missing_field_costs)
        }
        _ => {
            unsupported_operators.push(EntityUnlinkablesUnsupportedOperator {
                operator_id: support_operator_id(spec),
                source: "profile_support".to_string(),
                reason: "operator_not_scored_by_link_evidence_stage".to_string(),
            });
            Ok(())
        }
    }
}

fn score_exact_view_ceiling(
    spec: &EntityOperatorSpec,
    surface: &PreparedSurfaceRecord,
    support_namespace: &str,
    hits: &mut Vec<EdgeEvidenceHit>,
    missing_field_costs: &mut Vec<EntityUnlinkablesMissingFieldCost>,
) -> Result<(), Refusal> {
    let Some(view_name) = support_view_name(spec)? else {
        return Ok(());
    };
    let score_units =
        optional_score_units_param(spec, "score_units", "score")?.unwrap_or(ScoreUnits::MAX);
    if score_units == ScoreUnits::ZERO {
        return Ok(());
    }
    let operator_id = support_operator_id(spec);
    let Some(value) = populated_support_view_value(surface, view_name) else {
        missing_field_costs.push(profile_missing_cost(view_name, &operator_id, score_units));
        return Ok(());
    };
    if let Some(hit) = exact_view_support_hit(ExactViewSupportRequest {
        namespace: support_namespace,
        operator_id: &operator_id,
        reason_code: "exact_view_support",
        view_name,
        left_value: value,
        right_value: value,
        score_units,
    }) {
        hits.push(hit);
    }
    Ok(())
}

fn score_string_similarity_ceiling(
    spec: &EntityOperatorSpec,
    surface: &PreparedSurfaceRecord,
    support_namespace: &str,
    hits: &mut Vec<EdgeEvidenceHit>,
    missing_field_costs: &mut Vec<EntityUnlinkablesMissingFieldCost>,
) -> Result<(), Refusal> {
    let Some(score_cutoff) = positive_support_threshold(spec)? else {
        return Ok(());
    };
    let view_name = required_support_view_name(spec, "string_similarity")?;
    let metric = required_similarity_metric(spec)?;
    let score_hint = optional_score_units_param(spec, "score_hint_units", "score_hint")?;
    let operator_id = support_operator_id(spec);
    let Some(value) = populated_support_view_value(surface, view_name) else {
        missing_field_costs.push(profile_missing_cost(
            view_name,
            &operator_id,
            ScoreUnits::MAX,
        ));
        return Ok(());
    };
    if let Some(hit) = string_similarity_support_hit(StringSimilaritySupportRequest {
        namespace: support_namespace,
        operator_id: &operator_id,
        reason_code: "string_similarity_support",
        metric,
        left_value: value,
        right_value: value,
        score_cutoff: Some(score_cutoff),
        score_hint,
    }) {
        hits.push(hit);
    }
    Ok(())
}

fn score_tfidf_ceiling(
    spec: &EntityOperatorSpec,
    surface: &PreparedSurfaceRecord,
    support_namespace: &str,
    hits: &mut Vec<EdgeEvidenceHit>,
    missing_field_costs: &mut Vec<EntityUnlinkablesMissingFieldCost>,
) -> Result<(), Refusal> {
    let Some(min_score_units) = positive_support_threshold(spec)? else {
        return Ok(());
    };
    let view_name = required_support_view_name(spec, "tfidf_cosine")?;
    let operator_id = support_operator_id(spec);
    let Some(value) = populated_support_view_value(surface, view_name) else {
        missing_field_costs.push(profile_missing_cost(
            view_name,
            &operator_id,
            ScoreUnits::MAX,
        ));
        return Ok(());
    };
    let twin_surface_id = format!("{}#perfect_twin", surface.surface_id);
    let model = SparseTfidfModel::build(&[
        TfidfInputSurface::tokenized(
            surface.surface_id.clone(),
            value.to_string(),
            value.split_whitespace().map(ToOwned::to_owned),
        ),
        TfidfInputSurface::tokenized(
            twin_surface_id.clone(),
            value.to_string(),
            value.split_whitespace().map(ToOwned::to_owned),
        ),
    ]);
    if let Some(hit) = tfidf_cosine_support_hit(TfidfCosineSupportRequest {
        namespace: support_namespace,
        operator_id: &operator_id,
        model: &model,
        left_surface_id: &surface.surface_id,
        right_surface_id: &twin_surface_id,
        min_score_units,
        top_k: positive_usize_param(spec, "top_k", 25)?,
        candidate_cap: Some(positive_usize_param(spec, "candidate_cap", 25)?),
    }) {
        hits.push(hit);
    }
    Ok(())
}

fn score_record_link_support_ceiling(
    feature_id: &str,
    policy: &RecordLinkFeaturePolicy,
    input: &EntityUnlinkablesSurfaceInput,
    hits: &mut Vec<EdgeEvidenceHit>,
    missing_field_costs: &mut Vec<EntityUnlinkablesMissingFieldCost>,
    unsupported_operators: &mut Vec<EntityUnlinkablesUnsupportedOperator>,
) -> Result<(), Refusal> {
    let operator_id = format!("record_link:{feature_id}");
    let Some(value) = input.record_link_features.get(feature_id) else {
        let reason = input
            .quarantined_record_link_features
            .get(feature_id)
            .cloned()
            .unwrap_or_else(|| "record_link_feature_missing".to_string());
        missing_field_costs.push(EntityUnlinkablesMissingFieldCost {
            field_id: feature_id.to_string(),
            source: "record_link_feature".to_string(),
            operator_id,
            reason,
            cost_units: policy.score_units,
        });
        return Ok(());
    };
    let Some(feature) = record_link_self_support_feature(
        feature_id,
        value,
        &BTreeMap::from([(feature_id.to_string(), policy.clone())]),
    )
    .map_err(|error| {
        entity_unlinkables_refusal(
            "Record-link feature self comparison failed for unlinkables diagnostics",
            json!({
                "stage": "link",
                "field": "unlinkables.record_link_features",
                "feature_id": feature_id,
                "record_link_stage": error.stage,
                "reason": error.reason,
                "error": error.message,
                "writes_performed": false
            }),
        )
    })?
    else {
        unsupported_operators.push(EntityUnlinkablesUnsupportedOperator {
            operator_id,
            source: "record_link_feature".to_string(),
            reason: "feature_did_not_support_self_pair".to_string(),
        });
        return Ok(());
    };
    hits.push(EdgeEvidenceHit::new(
        ScoreLane::Support,
        "record_link",
        format!("record_link:{}", feature.feature_id),
        "record_link_feature_support",
        ScoreUnits::saturating_from_units(feature.score_units),
        false,
        format!(
            "record-link feature {} self-support score_units={}",
            feature.feature_id, feature.score_units
        ),
    ));
    Ok(())
}

fn populated_support_view_value<'a>(
    surface: &'a PreparedSurfaceRecord,
    view_name: &str,
) -> Option<&'a str> {
    surface
        .normalized_views
        .get(view_name)
        .map(|view| view.value.trim())
        .filter(|value| !value.is_empty())
}

fn profile_missing_cost(
    view_name: &str,
    operator_id: &str,
    score_units: ScoreUnits,
) -> EntityUnlinkablesMissingFieldCost {
    EntityUnlinkablesMissingFieldCost {
        field_id: view_name.to_string(),
        source: "profile_support_view".to_string(),
        operator_id: operator_id.to_string(),
        reason: "prepared_surface_view_missing_or_empty".to_string(),
        cost_units: u64::from(score_units.as_u32()),
    }
}

fn entity_unlinkables_denominator(
    surfaces: &[EntityUnlinkablesSurfaceCeiling],
) -> EntityUnlinkablesDenominator {
    let mut unique_surface_ids = BTreeMap::<String, ()>::new();
    let mut reference_surface_count = 0_u64;
    let mut target_surface_count = 0_u64;
    let mut unassigned_surface_count = 0_u64;
    for surface in surfaces {
        unique_surface_ids.insert(surface.surface_id.clone(), ());
        match surface.side {
            EntityUnlinkablesSurfaceSide::Reference => reference_surface_count += 1,
            EntityUnlinkablesSurfaceSide::Target => target_surface_count += 1,
            EntityUnlinkablesSurfaceSide::Unassigned => unassigned_surface_count += 1,
        }
    }
    EntityUnlinkablesDenominator {
        subject_surface_count: surfaces.len() as u64,
        unique_prepared_surface_count: unique_surface_ids.len() as u64,
        reference_surface_count,
        target_surface_count,
        unassigned_surface_count,
    }
}

fn entity_unlinkables_distribution(
    surfaces: &[EntityUnlinkablesSurfaceCeiling],
    thresholds: &EntityUnlinkablesThresholds,
) -> Vec<EntityUnlinkablesScoreBucket> {
    let mut counts = BTreeMap::<u32, u64>::new();
    for surface in surfaces {
        *counts
            .entry(surface.max_attainable_support_units)
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(
            |(max_attainable_support_units, surface_count)| EntityUnlinkablesScoreBucket {
                max_attainable_support_units,
                surface_count,
                below_attach_threshold: max_attainable_support_units
                    < thresholds.attach_score_min_units,
                below_backbone_threshold: max_attainable_support_units
                    < thresholds.backbone_score_min_units,
            },
        )
        .collect()
}

fn entity_unlinkable_surface(surface: &EntityUnlinkablesSurfaceCeiling) -> EntityUnlinkableSurface {
    let mut below_thresholds = Vec::new();
    if surface.below_attach_threshold {
        below_thresholds.push("attach".to_string());
    }
    if surface.below_backbone_threshold {
        below_thresholds.push("backbone".to_string());
    }
    EntityUnlinkableSurface {
        side: surface.side,
        surface_id: surface.surface_id.clone(),
        profile_id: surface.profile_id.clone(),
        link_ids: surface.link_ids.clone(),
        max_attainable_support_units: surface.max_attainable_support_units,
        below_thresholds,
        missing_field_costs: surface.missing_field_costs.clone(),
        unsupported_operators: surface.unsupported_operators.clone(),
    }
}

fn validate_entity_unlinkables_surface(
    surface: &EntityUnlinkablesSurfaceCeiling,
    thresholds: &EntityUnlinkablesThresholds,
) -> Result<(), Refusal> {
    if surface.surface_id.trim().is_empty() || surface.profile_id.trim().is_empty() {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables surface row is missing its identity",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces",
                "surface_id": surface.surface_id,
                "profile_id": surface.profile_id,
                "writes_performed": false
            }),
        ));
    }
    if surface.max_attainable_support_units != surface.score_breakdown.total_score_units.as_u32()
        || surface.raw_attainable_support_units != surface.score_breakdown.raw_support_score_units
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables surface score does not match its score breakdown",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces.score_breakdown",
                "surface_id": surface.surface_id,
                "writes_performed": false
            }),
        ));
    }
    if surface.below_attach_threshold
        != (surface.max_attainable_support_units < thresholds.attach_score_min_units)
        || surface.below_backbone_threshold
            != (surface.max_attainable_support_units < thresholds.backbone_score_min_units)
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables surface threshold flags are stale",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces.thresholds",
                "surface_id": surface.surface_id,
                "writes_performed": false
            }),
        ));
    }
    if surface
        .missing_field_costs
        .windows(2)
        .any(|pair| entity_missing_field_cost_cmp(&pair[0], &pair[1]).is_gt())
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables missing field costs must be sorted deterministically",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces.missing_field_costs",
                "surface_id": surface.surface_id,
                "writes_performed": false
            }),
        ));
    }
    if surface
        .unsupported_operators
        .windows(2)
        .any(|pair| entity_unsupported_operator_cmp(&pair[0], &pair[1]).is_gt())
    {
        return Err(entity_unlinkables_refusal(
            "Entity unlinkables unsupported operators must be sorted deterministically",
            json!({
                "stage": "link",
                "field": "unlinkables.surfaces.unsupported_operators",
                "surface_id": surface.surface_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn support_view_name(spec: &EntityOperatorSpec) -> Result<Option<&str>, Refusal> {
    match spec.view.as_deref().map(str::trim) {
        Some("") => Err(entity_unlinkables_refusal(
            "Profile-declared support evidence view must be non-empty",
            json!({
                "stage": "link",
                "field": "unlinkables.profile_support.view",
                "operator": spec.op,
                "writes_performed": false
            }),
        )),
        Some(view_name) => Ok(Some(view_name)),
        None => Ok(None),
    }
}

fn required_support_view_name<'a>(
    spec: &'a EntityOperatorSpec,
    operator: &'static str,
) -> Result<&'a str, Refusal> {
    support_view_name(spec)?.ok_or_else(|| {
        entity_unlinkables_refusal(
            "Profile-declared support evidence requires an explicit view",
            json!({
                "stage": "link",
                "field": "unlinkables.profile_support.view",
                "operator": operator,
                "writes_performed": false
            }),
        )
    })
}

fn positive_support_threshold(spec: &EntityOperatorSpec) -> Result<Option<ScoreUnits>, Refusal> {
    let threshold = optional_score_units_param(spec, "min_score_units", "min_score")?;
    Ok(threshold.filter(|score_units| *score_units > ScoreUnits::ZERO))
}

fn optional_score_units_param(
    spec: &EntityOperatorSpec,
    units_key: &'static str,
    decimal_key: &'static str,
) -> Result<Option<ScoreUnits>, Refusal> {
    if let Some(value) = spec.params.get(units_key) {
        return parse_score_units_param(value, &spec.op, units_key).map(Some);
    }
    if let Some(value) = spec.params.get(decimal_key) {
        return parse_decimal_score_param(value, &spec.op, decimal_key).map(Some);
    }
    Ok(None)
}

fn parse_score_units_param(
    value: &str,
    operator: &str,
    field: &'static str,
) -> Result<ScoreUnits, Refusal> {
    let units = value.trim().parse::<u32>().map_err(|_| {
        entity_unlinkables_refusal(
            "Profile-declared score threshold must be an integer score unit",
            json!({
                "stage": "link",
                "operator": operator,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "writes_performed": false
            }),
        )
    })?;
    ScoreUnits::from_scaled(units).ok_or_else(|| {
        entity_unlinkables_refusal(
            "Profile-declared score threshold is outside the entity score scale",
            json!({
                "stage": "link",
                "operator": operator,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "max": ENTITY_SCORE_SCALE,
                "writes_performed": false
            }),
        )
    })
}

fn parse_decimal_score_param(
    value: &str,
    operator: &str,
    field: &'static str,
) -> Result<ScoreUnits, Refusal> {
    let trimmed = value.trim();
    let Some((whole, fractional)) = trimmed.split_once('.') else {
        return match trimmed {
            "0" => Ok(ScoreUnits::ZERO),
            "1" => Ok(ScoreUnits::MAX),
            _ => parse_score_units_param(trimmed, operator, field),
        };
    };
    if !matches!(whole, "0" | "1")
        || fractional.is_empty()
        || fractional.len() > 4
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(entity_unlinkables_refusal(
            "Profile-declared decimal score threshold must be between 0 and 1 with at most four fractional digits",
            json!({
                "stage": "link",
                "operator": operator,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "writes_performed": false
            }),
        ));
    }
    let mut fractional_units = fractional.parse::<u32>().map_err(|_| {
        entity_unlinkables_refusal(
            "Profile-declared decimal score threshold is malformed",
            json!({
                "stage": "link",
                "operator": operator,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "writes_performed": false
            }),
        )
    })?;
    for _ in fractional.len()..4 {
        fractional_units *= 10;
    }
    let whole_units = if whole == "1" {
        ScoreUnits::MAX.as_u32()
    } else {
        0
    };
    ScoreUnits::from_scaled(whole_units.saturating_add(fractional_units)).ok_or_else(|| {
        entity_unlinkables_refusal(
            "Profile-declared decimal score threshold is outside the entity score scale",
            json!({
                "stage": "link",
                "operator": operator,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "max": "1.0",
                "writes_performed": false
            }),
        )
    })
}

fn positive_usize_param(
    spec: &EntityOperatorSpec,
    field: &'static str,
    default: usize,
) -> Result<usize, Refusal> {
    let Some(value) = spec.params.get(field) else {
        return Ok(default);
    };
    let parsed = value.trim().parse::<usize>().map_err(|_| {
        entity_unlinkables_refusal(
            "Profile-declared support evidence parameter must be a positive integer",
            json!({
                "stage": "link",
                "operator": spec.op,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "writes_performed": false
            }),
        )
    })?;
    if parsed == 0 {
        return Err(entity_unlinkables_refusal(
            "Profile-declared support evidence parameter must be positive",
            json!({
                "stage": "link",
                "operator": spec.op,
                "field": format!("unlinkables.profile_support.{field}"),
                "value": value,
                "writes_performed": false
            }),
        ));
    }
    Ok(parsed)
}

fn required_similarity_metric(spec: &EntityOperatorSpec) -> Result<SimilarityMetric, Refusal> {
    let metric = spec.params.get("metric").ok_or_else(|| {
        entity_unlinkables_refusal(
            "Profile-declared string similarity support requires a metric",
            json!({
                "stage": "link",
                "operator": spec.op,
                "field": "unlinkables.profile_support.metric",
                "writes_performed": false
            }),
        )
    })?;
    match metric.trim() {
        "levenshtein_normalized" => Ok(SimilarityMetric::LevenshteinNormalized),
        "jaro_winkler" => Ok(SimilarityMetric::JaroWinkler),
        "dice_sorensen" => Ok(SimilarityMetric::DiceSorensen),
        "token_sort_ratio" => Ok(SimilarityMetric::TokenSortRatio),
        "token_set_ratio" => Ok(SimilarityMetric::TokenSetRatio),
        _ => Err(entity_unlinkables_refusal(
            "Profile-declared string similarity metric is unsupported",
            json!({
                "stage": "link",
                "operator": spec.op,
                "field": "unlinkables.profile_support.metric",
                "value": metric,
                "writes_performed": false
            }),
        )),
    }
}

fn support_operator_id(spec: &EntityOperatorSpec) -> String {
    spec.view
        .as_deref()
        .map(|view_name| format!("{}:{view_name}", spec.op))
        .unwrap_or_else(|| spec.op.clone())
}

fn sorted_deduped(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn entity_unlinkables_surface_ceiling_cmp(
    left: &EntityUnlinkablesSurfaceCeiling,
    right: &EntityUnlinkablesSurfaceCeiling,
) -> Ordering {
    left.side
        .cmp(&right.side)
        .then_with(|| left.surface_id.cmp(&right.surface_id))
        .then_with(|| left.profile_id.cmp(&right.profile_id))
        .then_with(|| left.link_ids.cmp(&right.link_ids))
}

fn entity_missing_field_cost_cmp(
    left: &EntityUnlinkablesMissingFieldCost,
    right: &EntityUnlinkablesMissingFieldCost,
) -> Ordering {
    right
        .cost_units
        .cmp(&left.cost_units)
        .then_with(|| left.source.cmp(&right.source))
        .then_with(|| left.field_id.cmp(&right.field_id))
        .then_with(|| left.operator_id.cmp(&right.operator_id))
        .then_with(|| left.reason.cmp(&right.reason))
}

fn entity_unsupported_operator_cmp(
    left: &EntityUnlinkablesUnsupportedOperator,
    right: &EntityUnlinkablesUnsupportedOperator,
) -> Ordering {
    left.source
        .cmp(&right.source)
        .then_with(|| left.operator_id.cmp(&right.operator_id))
        .then_with(|| left.reason.cmp(&right.reason))
}

fn entity_unlinkables_refusal(message: impl Into<String>, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Use canon entity link to regenerate link/link.json".to_string()),
    )
}
