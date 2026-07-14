//! Solve-stage budget contracts.
//!
//! Full signed-graph solving is implemented by later ENT-P07 beads. This module
//! owns the stage-local budget decision for oversized components so callers
//! either get an explicit bounded abstention or a structured refusal.

use crate::Refusal;
use crate::entity::{
    budget::{BudgetEnforcement, BudgetLimit, BudgetStage, find_budget_policy},
    contracts::{
        CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1,
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactMetadata, EntityArtifactReference,
        EntityDeterministicSummary,
    },
    error::EntityRefusalKind,
    graph::{CannotLinkEvidenceEdge, EntityEvidenceGraph, SignedEvidenceEdge},
    score::ScoreUnits,
};
use crate::witness;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveOversizedComponentPolicy {
    BoundedAbstention,
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetConfig {
    pub max_component_size: u64,
    pub oversized_component_policy: SolveOversizedComponentPolicy,
}

impl SolveBudgetConfig {
    pub const fn bounded_abstention(max_component_size: u64) -> Self {
        Self {
            max_component_size,
            oversized_component_policy: SolveOversizedComponentPolicy::BoundedAbstention,
        }
    }

    pub const fn refuse(max_component_size: u64) -> Self {
        Self {
            max_component_size,
            oversized_component_policy: SolveOversizedComponentPolicy::Refuse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentBudgetInput {
    pub component_id: String,
    pub surface_ids: Vec<String>,
}

impl SolveComponentBudgetInput {
    pub fn new(component_id: impl Into<String>, surface_ids: Vec<String>) -> Self {
        Self {
            component_id: component_id.into(),
            surface_ids,
        }
    }

    pub fn observed_size(&self) -> u64 {
        u64::try_from(self.surface_ids.len()).expect("component size fits u64")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveBudgetAction {
    Solve,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetComponentDecision {
    pub component_id: String,
    pub action: SolveBudgetAction,
    pub policy_id: String,
    pub enforcement: BudgetEnforcement,
    pub observed: u64,
    pub configured: u64,
    pub surface_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveBudgetReport {
    pub policy_id: String,
    pub enforcement: BudgetEnforcement,
    pub configured: u64,
    pub summary: BTreeMap<String, u64>,
    pub components: Vec<SolveBudgetComponentDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveComponentAction {
    AutoMergeCandidate,
    Review,
    Contradiction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveEvidenceCut {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub evidence_count: u64,
    pub evidence_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveCannotLinkViolation {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: ScoreUnits,
    pub hard_cannot_link: bool,
    pub evidence_count: u64,
    pub evidence_reason_codes: Vec<String>,
    pub evidence_operator_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentConstraintDecision {
    pub component_id: String,
    pub action: SolveComponentAction,
    pub reason: String,
    pub surface_ids: Vec<String>,
    pub support_edge_count: u64,
    pub hard_cannot_link_violations: Vec<SolveCannotLinkViolation>,
    pub soft_anti_merge_warnings: Vec<SolveCannotLinkViolation>,
    pub strongest_positive_cut: Option<SolveEvidenceCut>,
    pub strongest_negative_cut: Option<SolveEvidenceCut>,
    pub raw_support_score_units: ScoreUnits,
    pub adjusted_support_score_units: ScoreUnits,
    pub review_priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentConstraintReport {
    pub summary: BTreeMap<String, u64>,
    pub components: Vec<SolveComponentConstraintDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveNewIdPolicy {
    DelegateToPromotion,
    EscrowOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveReconciliationConfig {
    pub minimum_adjusted_support_units: ScoreUnits,
    pub new_id_policy: SolveNewIdPolicy,
}

impl SolveReconciliationConfig {
    pub const fn delegate_new_ids(minimum_adjusted_support_units: ScoreUnits) -> Self {
        Self {
            minimum_adjusted_support_units,
            new_id_policy: SolveNewIdPolicy::DelegateToPromotion,
        }
    }

    pub const fn escrow_only(minimum_adjusted_support_units: ScoreUnits) -> Self {
        Self {
            minimum_adjusted_support_units,
            new_id_policy: SolveNewIdPolicy::EscrowOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveReconciliationState {
    ResolvedExisting,
    PromotableNew,
    Escrow,
    Conflict,
    Contradiction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveReconciliationDecision {
    pub component_id: String,
    pub state: SolveReconciliationState,
    pub reason: String,
    pub surface_ids: Vec<String>,
    pub incumbent_canonical_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    pub constraint_action: SolveComponentAction,
    pub support_score_units: ScoreUnits,
    pub adjusted_support_score_units: ScoreUnits,
    pub hard_cannot_link_count: u64,
    pub soft_anti_merge_warning_count: u64,
    pub review_priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveReconciliationReport {
    pub summary: BTreeMap<String, u64>,
    pub decisions: Vec<SolveReconciliationDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveSurfaceProvenance {
    pub surface_id: String,
    pub row_count: u64,
    pub deal_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveComponentDiagnostics {
    pub component_id: String,
    pub state: SolveReconciliationState,
    pub reason: String,
    pub surface_ids: Vec<String>,
    pub support_score_units: ScoreUnits,
    pub adjusted_support_score_units: ScoreUnits,
    pub negative_score_units: ScoreUnits,
    pub score_margin_units: ScoreUnits,
    pub strongest_positive_cut: Option<SolveEvidenceCut>,
    pub strongest_negative_cut: Option<SolveEvidenceCut>,
    pub affected_rows: u64,
    pub affected_deals: u64,
    pub review_priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveReviewGroupSeed {
    pub review_group_id: String,
    pub ambiguity_key: String,
    pub component_id: String,
    pub state: SolveReconciliationState,
    pub priority_reasons: Vec<String>,
    pub affected_rows: u64,
    pub affected_deals: u64,
    pub surface_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveDiagnosticsReport {
    pub summary: BTreeMap<String, u64>,
    pub components: Vec<SolveComponentDiagnostics>,
    pub review_group_seeds: Vec<SolveReviewGroupSeed>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveArtifactRequest {
    pub metadata: EntityArtifactMetadata,
    pub graph: EntityEvidenceGraph,
    pub config: SolveReconciliationConfig,
    pub provenance: Vec<SolveSurfaceProvenance>,
    pub decision_ledger_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntitySolveStageRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub evidence: &'a Path,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySolveStageOutput {
    pub artifact: SolveArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveEntityRecord {
    pub component_id: String,
    pub state: SolveReconciliationState,
    pub reason: String,
    pub surface_ids: Vec<String>,
    pub incumbent_canonical_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    pub support_score_units: ScoreUnits,
    pub adjusted_support_score_units: ScoreUnits,
    pub hard_cannot_link_count: u64,
    pub soft_anti_merge_warning_count: u64,
    pub review_priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolveArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub upstream_artifacts: Vec<EntityArtifactReference>,
    pub entities: Vec<SolveEntityRecord>,
    pub review_groups: Vec<SolveReviewGroupSeed>,
    pub diagnostics: SolveDiagnosticsReport,
    pub decision_ledger_path: String,
}

pub fn build_solve_artifact_contract(
    request: SolveArtifactRequest,
) -> Result<SolveArtifact, Refusal> {
    validate_solve_metadata(&request.metadata)?;
    if request.decision_ledger_path.trim().is_empty() {
        return Err(solve_artifact_refusal(
            "Solve artifact decision ledger path is required",
            json!({
                "stage": "solve",
                "field": "decision_ledger_path",
                "writes_performed": false
            }),
        ));
    }

    let upstream_artifacts = required_solve_upstream_artifacts(&request.metadata)?;
    let diagnostics = build_solve_diagnostics(&request.graph, request.config, &request.provenance);
    let reconciliation = reconcile_signed_graph_components(&request.graph, request.config);
    let entities = solve_entity_records(reconciliation.decisions);
    let summary = solve_artifact_summary(&entities, &diagnostics);
    let mut metadata = request.metadata;
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = upstream_artifacts.clone();

    let mut artifact = SolveArtifact {
        version: CANON_ENTITY_SOLVE_VERSION_V1.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        upstream_artifacts,
        entities,
        review_groups: diagnostics.review_group_seeds.clone(),
        diagnostics,
        decision_ledger_path: request.decision_ledger_path,
    };
    artifact.artifact_content_hash = hash_solve_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn validate_solve_artifact_contract(artifact: &SolveArtifact) -> Result<(), Refusal> {
    validate_solve_artifact_contract_inner(artifact, true)
}

pub fn validate_solve_artifact_envelope_contract(artifact: &SolveArtifact) -> Result<(), Refusal> {
    validate_solve_artifact_contract_inner(artifact, false)
}

fn validate_solve_artifact_contract_inner(
    artifact: &SolveArtifact,
    validate_typed_self_hash: bool,
) -> Result<(), Refusal> {
    if artifact.version != CANON_ENTITY_SOLVE_VERSION_V1 {
        return Err(solve_artifact_refusal(
            "Solve artifact has the wrong contract version",
            json!({
                "stage": "solve",
                "reason": "wrong_version",
                "expected": CANON_ENTITY_SOLVE_VERSION_V1,
                "actual": artifact.version
            }),
        ));
    }
    validate_solve_metadata(&artifact.metadata)?;
    required_solve_upstream_artifacts(&artifact.metadata)?;
    if artifact.upstream_artifacts != artifact.metadata.upstream_artifacts {
        return Err(solve_artifact_refusal(
            "Solve artifact upstream references must match metadata",
            json!({
                "stage": "solve",
                "field": "upstream_artifacts",
                "writes_performed": false
            }),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(solve_artifact_refusal(
            "Solve artifact metadata hash does not match artifact hash",
            json!({
                "stage": "solve",
                "field": "metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }
    if validate_typed_self_hash {
        let expected_artifact_hash = hash_solve_artifact_without_self(artifact)?;
        if artifact.artifact_content_hash != expected_artifact_hash {
            return Err(solve_artifact_refusal(
                "Solve artifact content hash does not match canonical bytes",
                json!({
                    "stage": "solve",
                    "field": "artifact_content_hash",
                    "expected": expected_artifact_hash,
                    "actual": artifact.artifact_content_hash,
                    "writes_performed": false
                }),
            ));
        }
    }
    validate_review_groups_reference_entities(artifact)?;
    Ok(())
}

pub fn evaluate_solve_budget(
    components: &[SolveComponentBudgetInput],
    config: SolveBudgetConfig,
) -> Result<SolveBudgetReport, Refusal> {
    let policy = find_budget_policy(BudgetStage::Solve, BudgetLimit::MaxComponentSize)
        .ok_or_else(missing_solve_budget_policy_refusal)?;
    let mut ordered_components = components.to_vec();
    ordered_components.sort_by(|left, right| left.component_id.cmp(&right.component_id));

    let mut decisions = Vec::with_capacity(ordered_components.len());
    for component in &ordered_components {
        let observed = component.observed_size();
        let mut surface_ids = component.surface_ids.clone();
        surface_ids.sort();

        if observed <= config.max_component_size {
            decisions.push(SolveBudgetComponentDecision {
                component_id: component.component_id.clone(),
                action: SolveBudgetAction::Solve,
                policy_id: policy.id.to_string(),
                enforcement: policy.enforcement,
                observed,
                configured: config.max_component_size,
                surface_ids,
                reason: "within_component_budget".to_string(),
            });
            continue;
        }

        match config.oversized_component_policy {
            SolveOversizedComponentPolicy::BoundedAbstention => {
                decisions.push(SolveBudgetComponentDecision {
                    component_id: component.component_id.clone(),
                    action: SolveBudgetAction::Abstain,
                    policy_id: policy.id.to_string(),
                    enforcement: BudgetEnforcement::BoundedAbstention,
                    observed,
                    configured: config.max_component_size,
                    surface_ids,
                    reason: "component_size_exceeds_budget".to_string(),
                });
            }
            SolveOversizedComponentPolicy::Refuse => {
                return Err(solve_component_budget_refusal(
                    &component.component_id,
                    &surface_ids,
                    observed,
                    config.max_component_size,
                    policy.id,
                    policy.next_command,
                ));
            }
        }
    }

    Ok(SolveBudgetReport {
        policy_id: policy.id.to_string(),
        enforcement: policy.enforcement,
        configured: config.max_component_size,
        summary: solve_budget_summary(&decisions),
        components: decisions,
    })
}

pub fn evaluate_solve_component_budget(
    component: SolveComponentBudgetInput,
    config: SolveBudgetConfig,
) -> Result<SolveBudgetComponentDecision, Refusal> {
    let report = evaluate_solve_budget(&[component], config)?;
    Ok(report
        .components
        .into_iter()
        .next()
        .expect("single component report has one decision"))
}

pub fn evaluate_signed_graph_components(
    graph: &EntityEvidenceGraph,
) -> SolveComponentConstraintReport {
    let components = positive_components(graph);
    let mut decisions = components
        .into_iter()
        .map(|component| evaluate_component_constraints(graph, component))
        .collect::<Vec<_>>();
    decisions.sort_by(component_constraint_decision_cmp);

    SolveComponentConstraintReport {
        summary: component_constraint_summary(&decisions),
        components: decisions,
    }
}

pub fn reconcile_signed_graph_components(
    graph: &EntityEvidenceGraph,
    config: SolveReconciliationConfig,
) -> SolveReconciliationReport {
    let constraint_report = evaluate_signed_graph_components(graph);
    let incumbent_ids = graph
        .surface_nodes
        .iter()
        .filter_map(|node| {
            node.incumbent_canonical_id
                .as_ref()
                .map(|canonical_id| (node.surface_id.clone(), canonical_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    let mut decisions = constraint_report
        .components
        .into_iter()
        .map(|component| reconcile_component(component, &incumbent_ids, config))
        .collect::<Vec<_>>();
    decisions.sort_by(reconciliation_decision_cmp);

    SolveReconciliationReport {
        summary: reconciliation_summary(&decisions),
        decisions,
    }
}

pub fn build_solve_diagnostics(
    graph: &EntityEvidenceGraph,
    config: SolveReconciliationConfig,
    provenance: &[SolveSurfaceProvenance],
) -> SolveDiagnosticsReport {
    let constraint_report = evaluate_signed_graph_components(graph);
    let reconciliation_report = reconcile_signed_graph_components(graph, config);
    let constraints_by_component = constraint_report
        .components
        .into_iter()
        .map(|component| (component.component_id.clone(), component))
        .collect::<BTreeMap<_, _>>();
    let provenance_by_surface = provenance_by_surface_id(provenance);

    let mut components = reconciliation_report
        .decisions
        .into_iter()
        .map(|decision| {
            let constraint = constraints_by_component
                .get(&decision.component_id)
                .expect("reconciliation decision has a matching constraint decision");
            solve_component_diagnostics(decision, constraint, &provenance_by_surface)
        })
        .collect::<Vec<_>>();
    components.sort_by(solve_component_diagnostics_cmp);

    let mut review_group_seeds = components
        .iter()
        .filter(|component| emits_review_group_seed(component.state))
        .map(review_group_seed)
        .collect::<Vec<_>>();
    review_group_seeds.sort_by(review_group_seed_cmp);

    SolveDiagnosticsReport {
        summary: solve_diagnostics_summary(&components, &review_group_seeds),
        components,
        review_group_seeds,
    }
}

fn validate_solve_metadata(metadata: &EntityArtifactMetadata) -> Result<(), Refusal> {
    if !metadata.profile.is_complete() {
        return Err(solve_artifact_refusal(
            "Solve artifact profile metadata is incomplete",
            json!({
                "stage": "solve",
                "field": "metadata.profile"
            }),
        ));
    }
    if metadata.strategy.id.trim().is_empty()
        || metadata.strategy.version.trim().is_empty()
        || metadata.strategy.content_hash.trim().is_empty()
    {
        return Err(solve_artifact_refusal(
            "Solve artifact strategy metadata is incomplete",
            json!({
                "stage": "solve",
                "field": "metadata.strategy"
            }),
        ));
    }
    if metadata.registry_snapshot.id.trim().is_empty()
        || metadata.registry_snapshot.version.trim().is_empty()
        || metadata
            .registry_snapshot
            .lookup_snapshot_hash
            .trim()
            .is_empty()
    {
        return Err(solve_artifact_refusal(
            "Solve artifact registry snapshot metadata is incomplete",
            json!({
                "stage": "solve",
                "field": "metadata.registry_snapshot"
            }),
        ));
    }
    Ok(())
}

fn required_solve_upstream_artifacts(
    metadata: &EntityArtifactMetadata,
) -> Result<Vec<EntityArtifactReference>, Refusal> {
    require_upstream_artifact(metadata, CANON_ENTITY_BLOCK_VERSION_V1)?;
    require_upstream_artifact(metadata, CANON_ENTITY_EVIDENCE_VERSION_V1)?;
    let mut upstream_artifacts = metadata.upstream_artifacts.clone();
    upstream_artifacts.sort_by(upstream_artifact_cmp);
    Ok(upstream_artifacts)
}

fn require_upstream_artifact(
    metadata: &EntityArtifactMetadata,
    version: &str,
) -> Result<(), Refusal> {
    let Some(reference) = metadata
        .upstream_artifacts
        .iter()
        .find(|reference| reference.version == version)
    else {
        return Err(solve_artifact_refusal(
            "Solve artifact requires upstream block and evidence artifact hashes",
            json!({
                "stage": "solve",
                "field": "metadata.upstream_artifacts",
                "missing_version": version
            }),
        ));
    };
    if reference.content_hash.trim().is_empty() {
        return Err(solve_artifact_refusal(
            "Solve artifact upstream artifact hash is required",
            json!({
                "stage": "solve",
                "field": "metadata.upstream_artifacts.content_hash",
                "version": version
            }),
        ));
    }
    Ok(())
}

fn solve_entity_records(decisions: Vec<SolveReconciliationDecision>) -> Vec<SolveEntityRecord> {
    let mut entities = decisions
        .into_iter()
        .map(|decision| SolveEntityRecord {
            component_id: decision.component_id,
            state: decision.state,
            reason: decision.reason,
            surface_ids: decision.surface_ids,
            incumbent_canonical_ids: decision.incumbent_canonical_ids,
            canonical_id: decision.canonical_id,
            candidate_id: decision.candidate_id,
            support_score_units: decision.support_score_units,
            adjusted_support_score_units: decision.adjusted_support_score_units,
            hard_cannot_link_count: decision.hard_cannot_link_count,
            soft_anti_merge_warning_count: decision.soft_anti_merge_warning_count,
            review_priority_reasons: decision.review_priority_reasons,
        })
        .collect::<Vec<_>>();
    entities.sort_by(solve_entity_record_cmp);
    entities
}

fn solve_artifact_summary(
    entities: &[SolveEntityRecord],
    diagnostics: &SolveDiagnosticsReport,
) -> EntityDeterministicSummary {
    let mut counts = BTreeMap::from([
        ("entity_count".to_string(), entities.len() as u64),
        (
            "resolved_existing".to_string(),
            entity_state_count(entities, SolveReconciliationState::ResolvedExisting),
        ),
        (
            "promotable_new".to_string(),
            entity_state_count(entities, SolveReconciliationState::PromotableNew),
        ),
        (
            "escrow".to_string(),
            entity_state_count(entities, SolveReconciliationState::Escrow),
        ),
        (
            "contradictions".to_string(),
            entity_state_count(entities, SolveReconciliationState::Contradiction),
        ),
        (
            "conflicts".to_string(),
            entity_state_count(entities, SolveReconciliationState::Conflict),
        ),
        (
            "review_group_count".to_string(),
            diagnostics.review_group_seeds.len() as u64,
        ),
    ]);
    for (key, value) in &diagnostics.summary {
        counts.insert(format!("diagnostics_{key}"), *value);
    }

    EntityDeterministicSummary {
        counts,
        labels: BTreeMap::from([(
            "decision_ledger".to_string(),
            "required_before_review_import_or_promotion".to_string(),
        )]),
    }
}

fn entity_state_count(entities: &[SolveEntityRecord], state: SolveReconciliationState) -> u64 {
    entities
        .iter()
        .filter(|entity| entity.state == state)
        .count() as u64
}

fn validate_review_groups_reference_entities(artifact: &SolveArtifact) -> Result<(), Refusal> {
    let component_ids = artifact
        .entities
        .iter()
        .map(|entity| entity.component_id.as_str())
        .collect::<BTreeSet<_>>();
    for seed in &artifact.review_groups {
        if !component_ids.contains(seed.component_id.as_str()) {
            return Err(solve_artifact_refusal(
                "Solve artifact review group references an unknown component",
                json!({
                    "stage": "solve",
                    "field": "review_groups.component_id",
                    "component_id": seed.component_id
                }),
            ));
        }
    }
    Ok(())
}

fn hash_solve_artifact_without_self(artifact: &SolveArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        solve_artifact_refusal(
            "Failed to hash solve artifact",
            json!({
                "stage": "solve",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn upstream_artifact_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn solve_entity_record_cmp(
    left: &SolveEntityRecord,
    right: &SolveEntityRecord,
) -> std::cmp::Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
        .then_with(|| {
            left.incumbent_canonical_ids
                .cmp(&right.incumbent_canonical_ids)
        })
}

fn solve_artifact_refusal(message: &'static str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some(
            "canon entity solve <ROWS> --evidence <EVIDENCE_ARTIFACT.json> --registry <REGISTRY_DIR>"
                .to_string(),
        ),
    )
}

fn solve_component_diagnostics(
    decision: SolveReconciliationDecision,
    constraint: &SolveComponentConstraintDecision,
    provenance_by_surface: &BTreeMap<String, (u64, u64)>,
) -> SolveComponentDiagnostics {
    let negative_score_units = constraint
        .strongest_negative_cut
        .as_ref()
        .map(|cut| cut.score_units)
        .unwrap_or(ScoreUnits::ZERO);
    let score_margin_units =
        subtract_score_units(decision.adjusted_support_score_units, negative_score_units);
    let (affected_rows, affected_deals) =
        affected_counts(&decision.surface_ids, provenance_by_surface);
    let review_priority_reasons = review_priority_reasons_for_diagnostics(&decision);

    SolveComponentDiagnostics {
        component_id: decision.component_id,
        state: decision.state,
        reason: decision.reason,
        surface_ids: decision.surface_ids,
        support_score_units: decision.support_score_units,
        adjusted_support_score_units: decision.adjusted_support_score_units,
        negative_score_units,
        score_margin_units,
        strongest_positive_cut: constraint.strongest_positive_cut.clone(),
        strongest_negative_cut: constraint.strongest_negative_cut.clone(),
        affected_rows,
        affected_deals,
        review_priority_reasons,
    }
}

fn review_group_seed(component: &SolveComponentDiagnostics) -> SolveReviewGroupSeed {
    SolveReviewGroupSeed {
        review_group_id: format!(
            "review:{}",
            stable_component_suffix(&component.component_id)
        ),
        ambiguity_key: format!("{:?}:{}", component.state, component.reason).to_ascii_lowercase(),
        component_id: component.component_id.clone(),
        state: component.state,
        priority_reasons: component.review_priority_reasons.clone(),
        affected_rows: component.affected_rows,
        affected_deals: component.affected_deals,
        surface_ids: component.surface_ids.clone(),
    }
}

fn emits_review_group_seed(state: SolveReconciliationState) -> bool {
    matches!(
        state,
        SolveReconciliationState::Escrow
            | SolveReconciliationState::Conflict
            | SolveReconciliationState::Contradiction
    )
}

fn review_priority_reasons_for_diagnostics(decision: &SolveReconciliationDecision) -> Vec<String> {
    let mut reasons = decision.review_priority_reasons.clone();
    match decision.state {
        SolveReconciliationState::Conflict => reasons.push("incumbent_conflict".to_string()),
        SolveReconciliationState::Contradiction => reasons.push("hard_cannot_link".to_string()),
        SolveReconciliationState::Escrow => reasons.push(decision.reason.clone()),
        SolveReconciliationState::ResolvedExisting | SolveReconciliationState::PromotableNew => {}
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn provenance_by_surface_id(provenance: &[SolveSurfaceProvenance]) -> BTreeMap<String, (u64, u64)> {
    let mut by_surface = BTreeMap::<String, (u64, u64)>::new();
    for record in provenance {
        let counts = by_surface.entry(record.surface_id.clone()).or_default();
        counts.0 = counts.0.saturating_add(record.row_count);
        counts.1 = counts.1.saturating_add(record.deal_count);
    }
    by_surface
}

fn affected_counts(
    surface_ids: &[String],
    provenance_by_surface: &BTreeMap<String, (u64, u64)>,
) -> (u64, u64) {
    surface_ids
        .iter()
        .filter_map(|surface_id| provenance_by_surface.get(surface_id))
        .fold((0u64, 0u64), |(rows, deals), (row_count, deal_count)| {
            (
                rows.saturating_add(*row_count),
                deals.saturating_add(*deal_count),
            )
        })
}

fn solve_diagnostics_summary(
    components: &[SolveComponentDiagnostics],
    review_group_seeds: &[SolveReviewGroupSeed],
) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("component_count".to_string(), components.len() as u64),
        (
            "review_group_count".to_string(),
            review_group_seeds.len() as u64,
        ),
        (
            "affected_rows".to_string(),
            components
                .iter()
                .map(|component| component.affected_rows)
                .sum(),
        ),
        (
            "affected_deals".to_string(),
            components
                .iter()
                .map(|component| component.affected_deals)
                .sum(),
        ),
        (
            "contradiction_count".to_string(),
            components
                .iter()
                .filter(|component| component.state == SolveReconciliationState::Contradiction)
                .count() as u64,
        ),
        (
            "escrow_count".to_string(),
            components
                .iter()
                .filter(|component| component.state == SolveReconciliationState::Escrow)
                .count() as u64,
        ),
        (
            "conflict_count".to_string(),
            components
                .iter()
                .filter(|component| component.state == SolveReconciliationState::Conflict)
                .count() as u64,
        ),
    ])
}

fn solve_component_diagnostics_cmp(
    left: &SolveComponentDiagnostics,
    right: &SolveComponentDiagnostics,
) -> std::cmp::Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
}

fn review_group_seed_cmp(
    left: &SolveReviewGroupSeed,
    right: &SolveReviewGroupSeed,
) -> std::cmp::Ordering {
    left.ambiguity_key
        .cmp(&right.ambiguity_key)
        .then_with(|| right.affected_rows.cmp(&left.affected_rows))
        .then_with(|| right.affected_deals.cmp(&left.affected_deals))
        .then_with(|| left.component_id.cmp(&right.component_id))
}

fn stable_component_suffix(component_id: &str) -> String {
    component_id
        .strip_prefix("component:")
        .unwrap_or(component_id)
        .replace(':', "_")
}

fn reconcile_component(
    component: SolveComponentConstraintDecision,
    incumbent_ids: &BTreeMap<String, String>,
    config: SolveReconciliationConfig,
) -> SolveReconciliationDecision {
    let component_incumbent_ids = component_incumbent_ids(&component.surface_ids, incumbent_ids);
    let hard_cannot_link_count = component.hard_cannot_link_violations.len() as u64;
    let soft_anti_merge_warning_count = component.soft_anti_merge_warnings.len() as u64;

    let (state, reason, canonical_id, candidate_id) =
        if component.action == SolveComponentAction::Contradiction {
            (
                SolveReconciliationState::Contradiction,
                "hard_cannot_link_constraint",
                None,
                None,
            )
        } else if component_incumbent_ids.len() > 1 {
            (
                SolveReconciliationState::Conflict,
                "multiple_incumbent_canonical_ids",
                None,
                None,
            )
        } else if let Some(canonical_id) = component_incumbent_ids.first() {
            if component.action == SolveComponentAction::Review {
                (
                    SolveReconciliationState::Escrow,
                    "incumbent_component_requires_review",
                    None,
                    None,
                )
            } else {
                (
                    SolveReconciliationState::ResolvedExisting,
                    "single_incumbent_inherits_existing_id",
                    Some(canonical_id.clone()),
                    None,
                )
            }
        } else if component.adjusted_support_score_units < config.minimum_adjusted_support_units {
            (
                SolveReconciliationState::Escrow,
                "below_support_threshold",
                None,
                None,
            )
        } else if component.action == SolveComponentAction::Review {
            (
                SolveReconciliationState::Escrow,
                "soft_anti_merge_requires_review",
                None,
                None,
            )
        } else {
            match config.new_id_policy {
                SolveNewIdPolicy::DelegateToPromotion => (
                    SolveReconciliationState::PromotableNew,
                    "new_id_delegated_to_promotion_policy",
                    None,
                    Some(promotable_candidate_id(&component.component_id)),
                ),
                SolveNewIdPolicy::EscrowOnly => (
                    SolveReconciliationState::Escrow,
                    "new_id_promotion_deferred_by_policy",
                    None,
                    None,
                ),
            }
        };

    SolveReconciliationDecision {
        component_id: component.component_id,
        state,
        reason: reason.to_string(),
        surface_ids: component.surface_ids,
        incumbent_canonical_ids: component_incumbent_ids,
        canonical_id,
        candidate_id,
        constraint_action: component.action,
        support_score_units: component.raw_support_score_units,
        adjusted_support_score_units: component.adjusted_support_score_units,
        hard_cannot_link_count,
        soft_anti_merge_warning_count,
        review_priority_reasons: component.review_priority_reasons,
    }
}

fn component_incumbent_ids(
    surface_ids: &[String],
    incumbent_ids: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut ids = surface_ids
        .iter()
        .filter_map(|surface_id| incumbent_ids.get(surface_id).cloned())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn promotable_candidate_id(component_id: &str) -> String {
    let suffix = component_id
        .strip_prefix("component:")
        .unwrap_or(component_id)
        .replace(':', "_");
    format!("candidate:{suffix}")
}

fn reconciliation_summary(decisions: &[SolveReconciliationDecision]) -> BTreeMap<String, u64> {
    let component_count = decisions.len() as u64;
    let resolved_existing_count =
        reconciliation_state_count(decisions, SolveReconciliationState::ResolvedExisting);
    let promotable_new_count =
        reconciliation_state_count(decisions, SolveReconciliationState::PromotableNew);
    let escrow_count = reconciliation_state_count(decisions, SolveReconciliationState::Escrow);
    let conflict_count = reconciliation_state_count(decisions, SolveReconciliationState::Conflict);
    let contradiction_count =
        reconciliation_state_count(decisions, SolveReconciliationState::Contradiction);

    BTreeMap::from([
        ("component_count".to_string(), component_count),
        (
            "resolved_existing_count".to_string(),
            resolved_existing_count,
        ),
        ("promotable_new_count".to_string(), promotable_new_count),
        ("escrow_count".to_string(), escrow_count),
        ("conflict_count".to_string(), conflict_count),
        ("contradiction_count".to_string(), contradiction_count),
    ])
}

fn reconciliation_state_count(
    decisions: &[SolveReconciliationDecision],
    state: SolveReconciliationState,
) -> u64 {
    decisions
        .iter()
        .filter(|decision| decision.state == state)
        .count() as u64
}

fn reconciliation_decision_cmp(
    left: &SolveReconciliationDecision,
    right: &SolveReconciliationDecision,
) -> std::cmp::Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
        .then_with(|| {
            left.incumbent_canonical_ids
                .cmp(&right.incumbent_canonical_ids)
        })
}

fn evaluate_component_constraints(
    graph: &EntityEvidenceGraph,
    component: PositiveSolveComponent,
) -> SolveComponentConstraintDecision {
    let surface_set = component
        .surface_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let support_edges = graph
        .support_edges
        .iter()
        .filter(|edge| edge_inside_component(edge, &surface_set))
        .cloned()
        .collect::<Vec<_>>();
    let cannot_link_edges = graph
        .cannot_link_edges
        .iter()
        .filter(|edge| cannot_link_inside_component(edge, &surface_set))
        .cloned()
        .collect::<Vec<_>>();

    let mut hard_cannot_link_violations = cannot_link_edges
        .iter()
        .filter(|edge| edge.hard_cannot_link)
        .map(cannot_link_violation)
        .collect::<Vec<_>>();
    hard_cannot_link_violations.sort_by(cannot_link_violation_cmp);

    let mut soft_anti_merge_warnings = cannot_link_edges
        .iter()
        .filter(|edge| !edge.hard_cannot_link)
        .map(cannot_link_violation)
        .collect::<Vec<_>>();
    soft_anti_merge_warnings.sort_by(cannot_link_violation_cmp);

    let strongest_positive_cut = support_edges
        .iter()
        .max_by(|left, right| signed_edge_strength_cmp(left, right))
        .map(signed_evidence_cut);
    let strongest_negative_cut = cannot_link_edges
        .iter()
        .max_by(|left, right| cannot_link_strength_cmp(left, right))
        .map(cannot_link_evidence_cut);
    let raw_support_score_units = strongest_positive_cut
        .as_ref()
        .map(|cut| cut.score_units)
        .unwrap_or(ScoreUnits::ZERO);
    let soft_penalty = ScoreUnits::saturating_from_units(
        soft_anti_merge_warnings
            .iter()
            .map(|warning| u64::from(warning.score_units.as_u32()))
            .sum(),
    );
    let adjusted_support_score_units = subtract_score_units(raw_support_score_units, soft_penalty);

    let mut review_priority_reasons = Vec::new();
    let (action, reason) = if !hard_cannot_link_violations.is_empty() {
        review_priority_reasons.push("hard_cannot_link".to_string());
        (
            SolveComponentAction::Contradiction,
            "hard_cannot_link_inside_positive_component",
        )
    } else if !soft_anti_merge_warnings.is_empty() {
        review_priority_reasons.push("soft_anti_merge".to_string());
        (
            SolveComponentAction::Review,
            "soft_anti_merge_inside_positive_component",
        )
    } else {
        (
            SolveComponentAction::AutoMergeCandidate,
            "positive_component_without_cannot_link",
        )
    };

    SolveComponentConstraintDecision {
        component_id: component.component_id,
        action,
        reason: reason.to_string(),
        surface_ids: component.surface_ids,
        support_edge_count: support_edges.len() as u64,
        hard_cannot_link_violations,
        soft_anti_merge_warnings,
        strongest_positive_cut,
        strongest_negative_cut,
        raw_support_score_units,
        adjusted_support_score_units,
        review_priority_reasons,
    }
}

fn positive_components(graph: &EntityEvidenceGraph) -> Vec<PositiveSolveComponent> {
    let mut union_find = SurfaceUnionFind::default();
    for node in &graph.surface_nodes {
        if node.incumbent_canonical_id.is_some() {
            union_find.insert(node.surface_id.clone());
        }
    }
    for edge in &graph.support_edges {
        union_find.union(&edge.left_surface_id, &edge.right_surface_id);
    }
    for hyperedge in &graph.exact_bucket_hyperedges {
        let mut explicit_surface_ids = hyperedge
            .explicit_surface_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        explicit_surface_ids.sort();
        union_all(&mut union_find, &explicit_surface_ids);

        for range in &hyperedge.surface_ranges {
            union_find.union(&range.start_surface_id, &range.end_surface_id);
        }
    }

    union_find.into_components()
}

fn union_all(union_find: &mut SurfaceUnionFind, surface_ids: &[String]) {
    if let Some(first) = surface_ids.first() {
        for surface_id in surface_ids.iter().skip(1) {
            union_find.union(first, surface_id);
        }
    }
}

fn edge_inside_component(edge: &SignedEvidenceEdge, surface_ids: &BTreeSet<String>) -> bool {
    surface_ids.contains(&edge.left_surface_id) && surface_ids.contains(&edge.right_surface_id)
}

fn cannot_link_inside_component(
    edge: &CannotLinkEvidenceEdge,
    surface_ids: &BTreeSet<String>,
) -> bool {
    surface_ids.contains(&edge.left_surface_id) && surface_ids.contains(&edge.right_surface_id)
}

fn signed_evidence_cut(edge: &SignedEvidenceEdge) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
    }
}

fn cannot_link_evidence_cut(edge: &CannotLinkEvidenceEdge) -> SolveEvidenceCut {
    SolveEvidenceCut {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
    }
}

fn cannot_link_violation(edge: &CannotLinkEvidenceEdge) -> SolveCannotLinkViolation {
    SolveCannotLinkViolation {
        left_surface_id: edge.left_surface_id.clone(),
        right_surface_id: edge.right_surface_id.clone(),
        score_units: edge.score_units,
        hard_cannot_link: edge.hard_cannot_link,
        evidence_count: edge.evidence.len() as u64,
        evidence_reason_codes: evidence_reason_codes(
            edge.evidence.iter().map(|hit| hit.reason_code.as_str()),
        ),
        evidence_operator_ids: evidence_operator_ids(
            edge.evidence.iter().map(|hit| hit.operator_id.as_str()),
        ),
    }
}

fn evidence_reason_codes<'a>(codes: impl Iterator<Item = &'a str>) -> Vec<String> {
    dedup_sorted_strings(codes)
}

fn evidence_operator_ids<'a>(operator_ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    dedup_sorted_strings(operator_ids)
}

fn dedup_sorted_strings<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values = values.map(str::to_string).collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn subtract_score_units(left: ScoreUnits, right: ScoreUnits) -> ScoreUnits {
    ScoreUnits::from_scaled(left.as_u32().saturating_sub(right.as_u32()))
        .expect("subtracting bounded score units remains inside score scale")
}

fn component_constraint_summary(
    decisions: &[SolveComponentConstraintDecision],
) -> BTreeMap<String, u64> {
    let component_count = decisions.len() as u64;
    let auto_merge_candidate_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::AutoMergeCandidate)
        .count() as u64;
    let review_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::Review)
        .count() as u64;
    let contradiction_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveComponentAction::Contradiction)
        .count() as u64;
    let hard_cannot_link_count = decisions
        .iter()
        .map(|decision| decision.hard_cannot_link_violations.len() as u64)
        .sum();
    let soft_anti_merge_warning_count = decisions
        .iter()
        .map(|decision| decision.soft_anti_merge_warnings.len() as u64)
        .sum();

    BTreeMap::from([
        ("component_count".to_string(), component_count),
        (
            "auto_merge_candidate_count".to_string(),
            auto_merge_candidate_count,
        ),
        ("review_component_count".to_string(), review_component_count),
        ("contradiction_count".to_string(), contradiction_count),
        ("hard_cannot_link_count".to_string(), hard_cannot_link_count),
        (
            "soft_anti_merge_warning_count".to_string(),
            soft_anti_merge_warning_count,
        ),
    ])
}

fn component_constraint_decision_cmp(
    left: &SolveComponentConstraintDecision,
    right: &SolveComponentConstraintDecision,
) -> std::cmp::Ordering {
    left.component_id
        .cmp(&right.component_id)
        .then_with(|| left.surface_ids.cmp(&right.surface_ids))
}

fn signed_edge_strength_cmp(
    left: &SignedEvidenceEdge,
    right: &SignedEvidenceEdge,
) -> std::cmp::Ordering {
    left.score_units
        .cmp(&right.score_units)
        .then_with(|| right.left_surface_id.cmp(&left.left_surface_id))
        .then_with(|| right.right_surface_id.cmp(&left.right_surface_id))
}

fn cannot_link_strength_cmp(
    left: &CannotLinkEvidenceEdge,
    right: &CannotLinkEvidenceEdge,
) -> std::cmp::Ordering {
    left.hard_cannot_link
        .cmp(&right.hard_cannot_link)
        .then_with(|| left.score_units.cmp(&right.score_units))
        .then_with(|| right.left_surface_id.cmp(&left.left_surface_id))
        .then_with(|| right.right_surface_id.cmp(&left.right_surface_id))
}

fn cannot_link_violation_cmp(
    left: &SolveCannotLinkViolation,
    right: &SolveCannotLinkViolation,
) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        .then_with(|| right.hard_cannot_link.cmp(&left.hard_cannot_link))
        .then_with(|| right.score_units.cmp(&left.score_units))
}

#[derive(Debug, Clone)]
struct PositiveSolveComponent {
    component_id: String,
    surface_ids: Vec<String>,
}

#[derive(Debug, Default)]
struct SurfaceUnionFind {
    parent: BTreeMap<String, String>,
}

impl SurfaceUnionFind {
    fn insert(&mut self, surface_id: String) {
        self.parent.entry(surface_id.clone()).or_insert(surface_id);
    }

    fn union(&mut self, left_surface_id: &str, right_surface_id: &str) {
        self.insert(left_surface_id.to_string());
        self.insert(right_surface_id.to_string());

        let left_root = self.find(left_surface_id);
        let right_root = self.find(right_surface_id);
        if left_root == right_root {
            return;
        }

        if left_root < right_root {
            self.parent.insert(right_root, left_root);
        } else {
            self.parent.insert(left_root, right_root);
        }
    }

    fn find(&mut self, surface_id: &str) -> String {
        let mut current = self
            .parent
            .get(surface_id)
            .cloned()
            .unwrap_or_else(|| surface_id.to_string());
        loop {
            let parent = self
                .parent
                .get(&current)
                .cloned()
                .unwrap_or_else(|| current.clone());
            if parent == current {
                break;
            }
            current = parent;
        }

        let root = current;
        let mut current = surface_id.to_string();
        while let Some(parent) = self.parent.get(&current).cloned() {
            if parent == root {
                break;
            }
            self.parent.insert(current.clone(), root.clone());
            current = parent;
        }
        root
    }

    fn into_components(mut self) -> Vec<PositiveSolveComponent> {
        let surface_ids = self.parent.keys().cloned().collect::<Vec<_>>();
        let mut by_root = BTreeMap::<String, Vec<String>>::new();
        for surface_id in surface_ids {
            let root = self.find(&surface_id);
            by_root.entry(root).or_default().push(surface_id);
        }

        by_root
            .into_iter()
            .map(|(root, mut surface_ids)| {
                surface_ids.sort();
                PositiveSolveComponent {
                    component_id: format!("component:{root}"),
                    surface_ids,
                }
            })
            .collect()
    }
}

fn solve_budget_summary(decisions: &[SolveBudgetComponentDecision]) -> BTreeMap<String, u64> {
    let component_count = u64::try_from(decisions.len()).expect("component count fits u64");
    let solved_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Solve)
        .count();
    let abstained_component_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Abstain)
        .count();
    let surface_count = decisions
        .iter()
        .map(|decision| decision.observed)
        .sum::<u64>();
    let abstained_surface_count = decisions
        .iter()
        .filter(|decision| decision.action == SolveBudgetAction::Abstain)
        .map(|decision| decision.observed)
        .sum::<u64>();
    let largest_component_size = decisions
        .iter()
        .map(|decision| decision.observed)
        .max()
        .unwrap_or_default();

    BTreeMap::from([
        ("component_count".to_string(), component_count),
        (
            "solved_component_count".to_string(),
            u64::try_from(solved_component_count).expect("solved component count fits u64"),
        ),
        (
            "abstained_component_count".to_string(),
            u64::try_from(abstained_component_count).expect("abstained component count fits u64"),
        ),
        ("surface_count".to_string(), surface_count),
        (
            "abstained_surface_count".to_string(),
            abstained_surface_count,
        ),
        ("largest_component_size".to_string(), largest_component_size),
    ])
}

fn solve_component_budget_refusal(
    component_id: &str,
    surface_ids: &[String],
    observed: u64,
    configured: u64,
    policy_id: &str,
    next_command: &str,
) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Solve component budget exceeded before safe solving",
        json!({
            "stage": "solve",
            "policy_id": policy_id,
            "component_id": component_id,
            "observed": observed,
            "configured": configured,
            "surface_ids": surface_ids,
            "recovery": "Keep the oversized component in escrow or explicitly configure a larger solve component cap"
        }),
        Some(next_command.to_string()),
    )
}

fn missing_solve_budget_policy_refusal() -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Solve component budget policy is missing",
        json!({
            "stage": "solve",
            "policy_id": "solve.max_component_size",
            "recovery": "Restore the shared entity budget policy table before running solve"
        }),
        Some("canon entity solve <ROWS> --strategy <STRATEGY.yaml> --evidence <EVIDENCE_ARTIFACT.json> --registry <REGISTRY_DIR>".to_string()),
    )
}
