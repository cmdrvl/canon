#![forbid(unsafe_code)]

//! Pure answer projection over retained, digest-bound geographic artifacts.
//! Preference and evidence provenance are separate from acceptance and extent.

use super::{
    CANON_GEO_COMPOSITION_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
    CANON_GEO_EXPLANATION_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION, GeoActionDiscoveryState,
    GeoAnswerScope, GeoAsOf, GeoClaimClass, GeoCompositionArtifact, GeoCompositionBackbone,
    GeoCompositionModel, GeoCompositionStatus, GeoControlEntityLevel, GeoDecisionPolicyRef,
    GeoEntityLevel, GeoEntityRef, GeoEvidenceAdmission, GeoEvidenceCompilationArtifact,
    GeoEvidenceDisposition, GeoEvidenceRecordRef, GeoExplanationArtifact, GeoHardConstraintKind,
    GeoNextEvidenceArtifact, GeoPlanStage, GeoRhoObservationKind, GeoRun, GeoRunBlockerKind,
    GeoRunNextActionKind, GeoRunPhase, GeoRunStatus, GeoStopReason, GeoTileCandidateReachStatus,
    GeoTileTruthReachStatus, GeoTileWorkUnitArtifact, canonical_composition_bytes,
    canonical_evidence_compilation_bytes, canonical_explanation_bytes, canonical_geo_run_bytes,
    canonical_next_evidence_bytes, canonical_tile_work_unit_bytes,
    canonicalize_composition_request, validate_evidence_compilation_artifact, validate_geo_run,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_ANSWER_VERSION: &str = "canon_geo_answer.v0";
pub const GEO_ANSWER_GROUPED_RULE_ID: &str = "contract_lineage_dedupe.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerConfig {
    pub entity_level: GeoControlEntityLevel,
    pub answer_scope: GeoAnswerScope,
    pub requested_claim_classes: Vec<GeoClaimClass>,
    pub decision_policy: Option<GeoDecisionPolicyRef>,
    pub question_hash: String,
    pub question_id: String,
    pub query_as_of: Option<GeoAsOf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerLimits {
    pub max_support_members: u64,
    pub max_preferred_models: u64,
}

impl Default for GeoAnswerLimits {
    fn default() -> Self {
        Self {
            max_support_members: 64,
            max_preferred_models: 16,
        }
    }
}

pub struct GeoAnswerClaimInputs<'a> {
    pub section: &'a GeoTileWorkUnitArtifact,
    pub compilation: &'a GeoEvidenceCompilationArtifact,
    pub composition: &'a GeoCompositionArtifact,
    pub explanation: &'a GeoExplanationArtifact,
}

pub struct GeoAnswerInputs<'a> {
    pub section: &'a GeoTileWorkUnitArtifact,
    pub compilation: &'a GeoEvidenceCompilationArtifact,
    pub composition: &'a GeoCompositionArtifact,
    pub explanation: &'a GeoExplanationArtifact,
    pub next_evidence: Option<&'a GeoNextEvidenceArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoAnswerClaimLabel {
    SingleMemberAssociation,
    Composition { claim_class: GeoClaimClass },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerInputDigests {
    pub section_blake3: String,
    pub evidence_compilation_blake3: String,
    pub composition_blake3: String,
    pub explanation_blake3: String,
    pub next_evidence_blake3: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerExecutionSource {
    SuppliedArtifactProjection,
    VerifiedRunSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerAwaitingInput {
    pub blocker_id: String,
    pub project_node_id: Option<String>,
    pub reason: String,
    pub required_artifact_ids: Vec<String>,
    pub expected_contracts: Vec<String>,
    pub next_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerRunExecution {
    pub run_id: String,
    pub run_semantic_hash: String,
    pub run_artifact_blake3: String,
    pub plan_id: String,
    pub plan_semantic_hash: String,
    pub status: GeoRunStatus,
    pub phase: GeoRunPhase,
    pub completed_project_node_ids: Vec<String>,
    pub executed_project_node_ids: Vec<String>,
    pub resumed_project_node_ids: Vec<String>,
    pub failed_project_node_ids: Vec<String>,
    pub cancelled_project_node_ids: Vec<String>,
    pub blocked_project_node_ids: Vec<String>,
    /// Nodes whose full receipt bodies were supplied and hash-validated.
    pub receipt_body_verified_project_node_ids: Vec<String>,
    /// Terminal outcomes backed by report metadata, without a supplied body.
    pub report_only_terminal_project_node_ids: Vec<String>,
    pub awaiting_inputs: Vec<GeoAnswerAwaitingInput>,
    pub run_budget_fallback: bool,
    pub composition_budget_fallback: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerExecution {
    pub source: GeoAnswerExecutionSource,
    pub consumed_stages: Vec<GeoPlanStage>,
    pub omitted_by_scope: Vec<GeoPlanStage>,
    pub composition_status: GeoCompositionStatus,
    pub actual_runner_history_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_snapshot: Option<GeoAnswerRunExecution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerSourceDerivation {
    NotDeclared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerSupportOrigin {
    RecordedObservation,
    LogicalConsequence,
    ObservationAndConsequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerEvidenceEffect {
    MembershipRequired,
    MembershipForbidden,
    CandidateSetRestriction,
    SoftAgreement,
    WholeLevelRestriction,
    DiagnosticOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerEvidenceRef {
    pub observation_id: String,
    pub rho_contract_id: String,
    pub rho_contract_version: String,
    pub disposition: GeoEvidenceDisposition,
    pub generated_id: Option<String>,
    pub admission_reason: Option<String>,
    pub source_dataset: String,
    pub source_release: String,
    pub source_record_ids: Vec<String>,
    pub source_vintages: Vec<String>,
    pub source_records: Vec<GeoEvidenceRecordRef>,
    pub lineage_ids: Vec<String>,
    pub lineage_group_id: String,
    pub source_derivation: GeoAnswerSourceDerivation,
    pub effect: GeoAnswerEvidenceEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerLineageGroup {
    pub group_id: String,
    pub observation_ids: Vec<String>,
    pub shared_lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerMemberSupport {
    pub member: GeoEntityRef,
    pub hard: Vec<GeoAnswerEvidenceRef>,
    pub soft: Vec<GeoAnswerEvidenceRef>,
    pub diagnostic: Vec<GeoAnswerEvidenceRef>,
    pub forced: bool,
    pub independent_support_groups: u64,
    pub support_origin: GeoAnswerSupportOrigin,
    pub conflicting_observation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerSupport {
    pub members: Vec<GeoAnswerMemberSupport>,
    pub global_constraints: Vec<GeoAnswerEvidenceRef>,
    pub diagnostic_observations: Vec<GeoAnswerEvidenceRef>,
    pub lineage_groups: Vec<GeoAnswerLineageGroup>,
    pub members_total: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerCalibration {
    Uncalibrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerPreferenceUnavailable {
    ResidualModelsNotMaterialized,
    CompositionConflict,
    CompositionBudgetFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeoAnswerPreferenceState {
    Ranked,
    NoSoftEvidence,
    Unavailable {
        reason: GeoAnswerPreferenceUnavailable,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerPreferredModel {
    pub members: GeoCompositionModel,
    pub cost: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerCostBucket {
    pub cost: u128,
    pub model_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerPreferenceView {
    pub preferred: Vec<GeoAnswerPreferredModel>,
    pub tie_count: u64,
    pub next_best_cost: Option<u128>,
    pub next_best_tie_count: u64,
    pub ranked_models: u64,
    pub cost_histogram: Vec<GeoAnswerCostBucket>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerGroupedPreferenceView {
    pub rule_id: String,
    pub preferred: Vec<GeoAnswerPreferredModel>,
    pub tie_count: u64,
    pub next_best_cost: Option<u128>,
    pub next_best_tie_count: u64,
    pub ranked_models: u64,
    pub cost_histogram: Vec<GeoAnswerCostBucket>,
    pub truncated: bool,
}

impl From<GeoAnswerPreferenceView> for GeoAnswerGroupedPreferenceView {
    fn from(view: GeoAnswerPreferenceView) -> Self {
        Self {
            rule_id: GEO_ANSWER_GROUPED_RULE_ID.into(),
            preferred: view.preferred,
            tie_count: view.tie_count,
            next_best_cost: view.next_best_cost,
            next_best_tie_count: view.next_best_tie_count,
            ranked_models: view.ranked_models,
            cost_histogram: view.cost_histogram,
            truncated: view.truncated,
        }
    }
}

impl GeoAnswerGroupedPreferenceView {
    fn preference_view(&self) -> GeoAnswerPreferenceView {
        GeoAnswerPreferenceView {
            preferred: self.preferred.clone(),
            tie_count: self.tie_count,
            next_best_cost: self.next_best_cost,
            next_best_tie_count: self.next_best_tie_count,
            ranked_models: self.ranked_models,
            cost_histogram: self.cost_histogram.clone(),
            truncated: self.truncated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerPreferredSupport {
    pub members: GeoCompositionModel,
    pub observation_ids: Vec<String>,
    pub lineage_group_ids: Vec<String>,
    pub shared_lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerPreference {
    pub state: GeoAnswerPreferenceState,
    pub calibration: GeoAnswerCalibration,
    pub composition_view: GeoAnswerPreferenceView,
    pub grouped_view: GeoAnswerGroupedPreferenceView,
    pub lineage_dependent: bool,
    pub supporting: Vec<GeoAnswerPreferredSupport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerAcceptanceStatus {
    NoPolicyDeclared,
    PolicyNotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerAcceptance {
    pub policy: Option<GeoDecisionPolicyRef>,
    pub status: GeoAnswerAcceptanceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerConsequence {
    pub residual_model_count: Option<u64>,
    pub residual_model_count_complete: bool,
    pub residual_model_count_saturated: bool,
    pub hard_forced: GeoCompositionBackbone,
    pub backbone_complete: bool,
    pub conflict_constraint_ids: Vec<String>,
    pub conflict_core_complete: Option<bool>,
    pub budget_fallback_component_keys: Vec<String>,
    pub exactness_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerExtent {
    pub candidate_reach: GeoTileCandidateReachStatus,
    pub truth_reach: GeoTileTruthReachStatus,
    pub candidate_count: u64,
    pub missing_reference_count: u64,
    pub complete_extent_claimed: bool,
    pub complete_extent_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerFurtherWorkSource {
    NextEvidenceArtifact,
    NotInScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerFurtherWork {
    pub source: GeoAnswerFurtherWorkSource,
    pub action_discovery: GeoActionDiscoveryState,
    pub frontier_action_ids: Vec<String>,
    pub stop: Option<GeoStopReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerClaimProjection {
    pub claim_label: GeoAnswerClaimLabel,
    pub association_support: GeoAnswerSupport,
    pub preference: GeoAnswerPreference,
    pub logical_consequence: GeoAnswerConsequence,
    pub extent_and_reach: GeoAnswerExtent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerArtifact {
    pub version: String,
    pub question_hash: String,
    pub question_id: String,
    pub query_as_of: Option<GeoAsOf>,
    pub entity_level: GeoControlEntityLevel,
    pub answer_scope: GeoAnswerScope,
    pub claim_label: GeoAnswerClaimLabel,
    pub inputs: GeoAnswerInputDigests,
    pub execution: GeoAnswerExecution,
    pub association_support: GeoAnswerSupport,
    pub preference: GeoAnswerPreference,
    pub acceptance: GeoAnswerAcceptance,
    pub logical_consequence: GeoAnswerConsequence,
    pub extent_and_reach: GeoAnswerExtent,
    pub further_work: GeoAnswerFurtherWork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoAnswerErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    AnswerInputMismatch,
    AnswerConfigMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoAnswerError {
    pub code: GeoAnswerErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl fmt::Display for GeoAnswerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl Error for GeoAnswerError {}

fn invalid(field: &str, message: impl Into<String>) -> GeoAnswerError {
    GeoAnswerError {
        code: GeoAnswerErrorCode::InvalidInput,
        message: message.into(),
        detail: BTreeMap::from([("field".into(), field.into())]),
    }
}

fn mismatch(artifact: &str, expected: &str, actual: &str) -> GeoAnswerError {
    GeoAnswerError {
        code: GeoAnswerErrorCode::AnswerInputMismatch,
        message: "Answer inputs do not bind the same retained evidence".into(),
        detail: BTreeMap::from([
            ("artifact".into(), artifact.into()),
            ("expected_blake3".into(), expected.into()),
            ("actual_blake3".into(), actual.into()),
        ]),
    }
}

fn prefixed_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn normalized_compilation(
    input: &GeoEvidenceCompilationArtifact,
) -> Result<GeoEvidenceCompilationArtifact, GeoAnswerError> {
    let mut value = input.clone();
    value.composition_request = canonicalize_composition_request(&value.composition_request)
        .map_err(|e| invalid("compilation", e.to_string()))?;
    value
        .admissions
        .sort_by(|a, b| a.observation_id.cmp(&b.observation_id));
    for admission in &mut value.admissions {
        admission.source_records.sort();
        admission.generated_ids.sort();
        admission.contract.source_lineage_ids.sort();
    }
    validate_evidence_compilation_artifact(&value)
        .map_err(|e| invalid("compilation", e.to_string()))?;
    Ok(value)
}

fn validate_config(
    config: &GeoAnswerConfig,
    limits: &GeoAnswerLimits,
) -> Result<GeoEntityLevel, GeoAnswerError> {
    validate_answer_config(config)?;
    if limits.max_support_members == 0 || limits.max_preferred_models == 0 {
        return Err(invalid(
            "limits",
            "Answer presentation limits must be positive",
        ));
    }
    match config.entity_level {
        GeoControlEntityLevel::Parcel => Ok(GeoEntityLevel::Parcel),
        GeoControlEntityLevel::Building => Ok(GeoEntityLevel::Building),
        _ => Err(invalid(
            "entity_level",
            "The supplied composition profile supports parcel/building projection only",
        )),
    }
}

pub fn validate_answer_config(config: &GeoAnswerConfig) -> Result<(), GeoAnswerError> {
    validate_digest("question_hash", &config.question_hash)?;
    if config.question_id.trim().is_empty() || config.question_id.len() > 256 {
        return Err(invalid(
            "question_id",
            "Question identifier must be nonempty and bounded",
        ));
    }
    if !matches!(
        config.entity_level,
        GeoControlEntityLevel::Parcel | GeoControlEntityLevel::Building
    ) {
        return Err(invalid(
            "entity_level",
            "Answer v0 projects the supported parcel/building composition profiles",
        ));
    }
    if let Some(as_of) = &config.query_as_of
        && (chrono::NaiveDate::parse_from_str(&as_of.utc_day, "%Y-%m-%d").is_err()
            || as_of.utc_day.len() != 10
            || as_of.unit != "utc_day"
            || as_of.semantic_id.trim().is_empty())
    {
        return Err(invalid(
            "query_as_of",
            "Answer as-of must preserve an explicit UTC-day declaration",
        ));
    }
    if let Some(policy) = &config.decision_policy {
        if policy.policy_id.trim().is_empty() || policy.version.trim().is_empty() {
            return Err(invalid(
                "decision_policy",
                "Decision policy references require their versioned identifier",
            ));
        }
        validate_digest("decision_policy.content_hash", &policy.content_hash)?;
    }
    let unique: BTreeSet<_> = config.requested_claim_classes.iter().copied().collect();
    if unique.len() != config.requested_claim_classes.len() {
        return Err(invalid(
            "requested_claim_classes",
            "Requested claim classes must be distinct",
        ));
    }
    Ok(())
}

pub fn canonicalize_answer_config(config: &GeoAnswerConfig) -> GeoAnswerConfig {
    let mut canonical = config.clone();
    canonical.requested_claim_classes.sort();
    canonical
}

fn input_digests(
    inputs: &GeoAnswerClaimInputs<'_>,
    compilation: &GeoEvidenceCompilationArtifact,
) -> Result<GeoAnswerInputDigests, GeoAnswerError> {
    if inputs.section.version != CANON_GEO_TILE_WORK_UNIT_VERSION
        || inputs.composition.version != CANON_GEO_COMPOSITION_VERSION
        || inputs.explanation.version != CANON_GEO_EXPLANATION_VERSION
        || compilation.version != CANON_GEO_EVIDENCE_COMPILATION_VERSION
    {
        return Err(GeoAnswerError {
            code: GeoAnswerErrorCode::UnsupportedVersion,
            message: "Unsupported answer input artifact version".into(),
            detail: BTreeMap::new(),
        });
    }
    let compiled_bytes = canonical_evidence_compilation_bytes(compilation)
        .map_err(|e| invalid("compilation", e.to_string()))?;
    let evidence_hash = prefixed_hash(&compiled_bytes);
    let raw_hash = blake3::hash(&compiled_bytes).to_hex().to_string();
    let actual = inputs
        .composition
        .evidence_compilation
        .as_ref()
        .map(|r| r.blake3.as_str())
        .unwrap_or("");
    if actual != raw_hash {
        return Err(mismatch("composition", &raw_hash, actual));
    }
    if inputs.explanation.evidence_blake3 != evidence_hash {
        return Err(mismatch(
            "explanation",
            &evidence_hash,
            &inputs.explanation.evidence_blake3,
        ));
    }
    Ok(GeoAnswerInputDigests {
        section_blake3: prefixed_hash(
            &canonical_tile_work_unit_bytes(inputs.section)
                .map_err(|e| invalid("section", e.to_string()))?,
        ),
        evidence_compilation_blake3: evidence_hash,
        composition_blake3: prefixed_hash(
            &canonical_composition_bytes(inputs.composition)
                .map_err(|e| invalid("composition", e.to_string()))?,
        ),
        explanation_blake3: prefixed_hash(
            &canonical_explanation_bytes(inputs.explanation)
                .map_err(|e| invalid("explanation", e.to_string()))?,
        ),
        next_evidence_blake3: None,
    })
}

/// Project the current claim without requiring prospective actions or a final answer.
pub fn project_current_claim(
    inputs: &GeoAnswerClaimInputs<'_>,
    config: &GeoAnswerConfig,
    limits: &GeoAnswerLimits,
) -> Result<GeoAnswerClaimProjection, GeoAnswerError> {
    let level = validate_config(config, limits)?;
    let compilation = normalized_compilation(inputs.compilation)?;
    input_digests(inputs, &compilation)?;
    claim_projection(inputs, &compilation, config, limits, level)
}

/// Assemble a terminal answer from supplied artifacts, never from ambient state.
pub fn project_answer(
    inputs: &GeoAnswerInputs<'_>,
    config: &GeoAnswerConfig,
    limits: &GeoAnswerLimits,
) -> Result<GeoAnswerArtifact, GeoAnswerError> {
    let level = validate_config(config, limits)?;
    let compilation = normalized_compilation(inputs.compilation)?;
    let claim_inputs = GeoAnswerClaimInputs {
        section: inputs.section,
        compilation: &compilation,
        composition: inputs.composition,
        explanation: inputs.explanation,
    };
    let mut digests = input_digests(&claim_inputs, &compilation)?;
    let claim = claim_projection(&claim_inputs, &compilation, config, limits, level)?;
    let further_work = match inputs.next_evidence {
        Some(next) => {
            if next.composition_blake3 != digests.composition_blake3 {
                return Err(mismatch(
                    "next_evidence",
                    &digests.composition_blake3,
                    &next.composition_blake3,
                ));
            }
            let bytes = canonical_next_evidence_bytes(next)
                .map_err(|e| invalid("next_evidence", e.to_string()))?;
            digests.next_evidence_blake3 = Some(prefixed_hash(&bytes));
            GeoAnswerFurtherWork {
                source: GeoAnswerFurtherWorkSource::NextEvidenceArtifact,
                action_discovery: next.action_discovery,
                frontier_action_ids: next.frontier.iter().map(|a| a.action_id.clone()).collect(),
                stop: next.stop,
            }
        }
        None if config.answer_scope == GeoAnswerScope::FullWorkflow => {
            return Err(invalid(
                "next_evidence",
                "Full workflow answer requires its next-evidence artifact",
            ));
        }
        None => GeoAnswerFurtherWork {
            source: GeoAnswerFurtherWorkSource::NotInScope,
            action_discovery: GeoActionDiscoveryState::NotAttempted,
            frontier_action_ids: Vec::new(),
            stop: None,
        },
    };
    let mut stages = vec![
        GeoPlanStage::BuildBoundedSection,
        GeoPlanStage::CompileEvidence,
        GeoPlanStage::FactorAndSolveExactResidual,
        GeoPlanStage::ExplainResidual,
    ];
    if inputs.next_evidence.is_some() {
        stages.push(GeoPlanStage::SelectNextEvidence);
    }
    stages.sort();
    let answer = GeoAnswerArtifact {
        version: CANON_GEO_ANSWER_VERSION.into(),
        question_hash: config.question_hash.clone(),
        question_id: config.question_id.clone(),
        query_as_of: config.query_as_of.clone(),
        entity_level: config.entity_level,
        answer_scope: config.answer_scope,
        claim_label: claim.claim_label,
        inputs: digests,
        execution: GeoAnswerExecution {
            source: GeoAnswerExecutionSource::SuppliedArtifactProjection,
            consumed_stages: stages,
            omitted_by_scope: if inputs.next_evidence.is_none() {
                vec![
                    GeoPlanStage::SeparateResidual,
                    GeoPlanStage::SelectNextEvidence,
                ]
            } else {
                Vec::new()
            },
            composition_status: inputs.composition.status,
            actual_runner_history_available: false,
            run_snapshot: None,
        },
        association_support: claim.association_support,
        preference: claim.preference,
        acceptance: GeoAnswerAcceptance {
            policy: config.decision_policy.clone(),
            status: if config.decision_policy.is_some() {
                GeoAnswerAcceptanceStatus::PolicyNotEvaluated
            } else {
                GeoAnswerAcceptanceStatus::NoPolicyDeclared
            },
        },
        logical_consequence: claim.logical_consequence,
        extent_and_reach: claim.extent_and_reach,
        further_work,
    };
    validate_answer_artifact(&answer)?;
    Ok(answer)
}

/// Project a validated prior run snapshot, including legitimate preflight waits.
/// This API does not require or invent a current association answer.
pub fn project_answer_execution(run: &GeoRun) -> Result<GeoAnswerRunExecution, GeoAnswerError> {
    validate_geo_run(run).map_err(|e| invalid("execution.run", e.to_string()))?;
    let mut snapshot = GeoAnswerRunExecution {
        run_id: run.run_id.clone(),
        run_semantic_hash: run.semantic_hash.clone(),
        run_artifact_blake3: prefixed_hash(
            &canonical_geo_run_bytes(run).map_err(|e| invalid("execution.run", e.to_string()))?,
        ),
        plan_id: run.plan_ref.plan_id.clone(),
        plan_semantic_hash: run.plan_ref.semantic_hash.clone(),
        status: run.status,
        phase: run.phase,
        completed_project_node_ids: Vec::new(),
        executed_project_node_ids: Vec::new(),
        resumed_project_node_ids: Vec::new(),
        failed_project_node_ids: Vec::new(),
        cancelled_project_node_ids: Vec::new(),
        blocked_project_node_ids: Vec::new(),
        receipt_body_verified_project_node_ids: Vec::new(),
        report_only_terminal_project_node_ids: Vec::new(),
        awaiting_inputs: Vec::new(),
        run_budget_fallback: run.status == GeoRunStatus::BudgetFallback,
        composition_budget_fallback: None,
    };
    if let Some(report) = &run.project_run_report {
        if report.project_id != run.plan_ref.project_id
            || report.plan_graph_hash != run.plan_ref.project_graph_hash
            || report.receipt.project_id != report.project_id
            || report.receipt.plan_graph_hash != report.plan_graph_hash
            || report.run_receipt_hash != report.receipt.receipt_hash
        {
            return Err(invalid(
                "execution.report",
                "Run and project receipt identities must agree",
            ));
        }
        crate::project::canonical_run_receipt_bytes(&report.receipt)
            .map_err(|e| invalid("execution.report.receipt", e.to_string()))?;
        let mut receipts = BTreeMap::new();
        for receipt in &report.receipt.node_receipts {
            crate::project::validate_node_receipt(receipt.clone())
                .map_err(|e| invalid("execution.report.node_receipt", e.to_string()))?;
            if receipt.project_id != report.project_id
                || receipt.outcome != crate::project::ProjectRunNodeOutcome::Completed
                || (receipt.plan_graph_hash != report.plan_graph_hash
                    && !report.resumed_nodes.contains(&receipt.node_id))
                || receipts.insert(receipt.node_id.as_str(), receipt).is_some()
            {
                return Err(invalid(
                    "execution.report.node_receipt",
                    "Embedded receipts must be unique completed outcomes for this run's project",
                ));
            }
        }
        snapshot.executed_project_node_ids = sorted_unique(report.executed_nodes.clone());
        snapshot.resumed_project_node_ids = sorted_unique(report.resumed_nodes.clone());
        snapshot.completed_project_node_ids = sorted_unique(
            report
                .executed_nodes
                .iter()
                .chain(&report.resumed_nodes)
                .cloned()
                .collect(),
        );
        snapshot.failed_project_node_ids = sorted_unique(report.failed_nodes.clone());
        snapshot.cancelled_project_node_ids = sorted_unique(report.cancelled_nodes.clone());
        snapshot.blocked_project_node_ids = sorted_unique(report.blocked_nodes.clone());
        if snapshot.completed_project_node_ids
            != sorted_unique(report.receipt.completed_nodes.clone())
            || snapshot.failed_project_node_ids
                != sorted_unique(report.receipt.failed_nodes.clone())
            || snapshot.cancelled_project_node_ids
                != sorted_unique(report.receipt.cancelled_nodes.clone())
            || snapshot.blocked_project_node_ids
                != sorted_unique(report.receipt.blocked_nodes.clone())
        {
            return Err(invalid(
                "execution.report",
                "Reported outcomes must agree with the retained receipt",
            ));
        }
        if snapshot
            .executed_project_node_ids
            .iter()
            .any(|id| snapshot.resumed_project_node_ids.contains(id))
        {
            return Err(invalid(
                "execution.report",
                "Execution and reuse are distinct outcomes",
            ));
        }
        for (ids, outcome) in [
            (
                &snapshot.completed_project_node_ids,
                crate::project::ProjectRunNodeOutcome::Completed,
            ),
            (
                &snapshot.failed_project_node_ids,
                crate::project::ProjectRunNodeOutcome::Failed,
            ),
            (
                &snapshot.cancelled_project_node_ids,
                crate::project::ProjectRunNodeOutcome::Cancelled,
            ),
        ] {
            for id in ids {
                match receipts.get(id.as_str()) {
                    Some(receipt) if receipt.outcome == outcome => {
                        snapshot
                            .receipt_body_verified_project_node_ids
                            .push(id.clone());
                    }
                    None if outcome != crate::project::ProjectRunNodeOutcome::Completed => {
                        snapshot
                            .report_only_terminal_project_node_ids
                            .push(id.clone());
                    }
                    _ => {
                        return Err(invalid(
                            "execution.report",
                            "Completed outcomes need their receipt bodies; terminal outcomes use report metadata",
                        ));
                    }
                }
            }
        }
        snapshot.receipt_body_verified_project_node_ids.sort();
        snapshot.report_only_terminal_project_node_ids.sort();
        if receipts.len() != snapshot.receipt_body_verified_project_node_ids.len() {
            return Err(invalid(
                "execution.report.node_receipt",
                "Receipt body has no declared node outcome",
            ));
        }
        let mut reported = BTreeSet::new();
        for node in &report.node_reports {
            let expected_outcome = if snapshot.completed_project_node_ids.contains(&node.node_id) {
                Some(crate::project::ProjectRunNodeOutcome::Completed)
            } else if snapshot.failed_project_node_ids.contains(&node.node_id) {
                Some(crate::project::ProjectRunNodeOutcome::Failed)
            } else if snapshot.cancelled_project_node_ids.contains(&node.node_id) {
                Some(crate::project::ProjectRunNodeOutcome::Cancelled)
            } else {
                None
            };
            let hash = node.receipt_hash.as_deref().ok_or_else(|| {
                invalid(
                    "execution.report.node_reports",
                    "Reported execution needs its receipt hash",
                )
            })?;
            validate_digest("execution.report.node_reports.receipt_hash", hash)?;
            if !reported.insert(&node.node_id)
                || expected_outcome != Some(node.outcome)
                || receipts.get(node.node_id.as_str()).is_some_and(|receipt| {
                    receipt.outcome != node.outcome
                        || node.receipt_hash.as_ref() != Some(&receipt.receipt_hash)
                })
            {
                return Err(invalid(
                    "execution.report.node_reports",
                    "Node report must identify its actual unique receipt outcome",
                ));
            }
        }
        if snapshot
            .executed_project_node_ids
            .iter()
            .chain(&snapshot.failed_project_node_ids)
            .chain(&snapshot.cancelled_project_node_ids)
            .any(|id| !reported.contains(id))
        {
            return Err(invalid(
                "execution.report.node_reports",
                "Executed and terminal nodes need their unique outcome reports",
            ));
        }
        let mut outputs = BTreeSet::new();
        for output in &run.output_refs {
            if !outputs.insert((&output.project_node_id, &output.output_id)) {
                return Err(invalid(
                    "execution.output_refs",
                    "Duplicate declared run output",
                ));
            }
            if !snapshot
                .completed_project_node_ids
                .contains(&output.project_node_id)
            {
                return Err(invalid(
                    "execution.output_refs",
                    "A failed, cancelled or blocked node cannot publish a completed consumed output",
                ));
            }
            let receipt = receipts
                .get(output.project_node_id.as_str())
                .ok_or_else(|| invalid("execution.output_refs", "Missing producer receipt"))?;
            let matching: Vec<_> = receipt
                .outputs
                .iter()
                .filter(|item| item.output_id == output.output_id)
                .collect();
            if matching.len() != 1 {
                return Err(invalid(
                    "execution.output_refs",
                    "Output must name exactly one producer receipt output",
                ));
            }
            if matching[0].content_digest != output.content_digest
                || matching[0].byte_count != output.byte_count
            {
                let mut error = mismatch(
                    &format!("execution.output.{}", output.output_id),
                    &matching[0].content_digest,
                    &output.content_digest,
                );
                error.detail.insert(
                    "expected_byte_count".into(),
                    matching[0].byte_count.to_string(),
                );
                error
                    .detail
                    .insert("actual_byte_count".into(), output.byte_count.to_string());
                return Err(error);
            }
        }
    } else if !run.output_refs.is_empty() {
        return Err(invalid(
            "execution.output_refs",
            "Outputs need their retained project run report",
        ));
    }
    for blocker in &run.blockers {
        if blocker.kind != GeoRunBlockerKind::WaitingForInput {
            continue;
        }
        let actions: Vec<_> = run
            .next_actions
            .iter()
            .filter(|action| {
                action.project_node_id == blocker.project_node_id
                    && matches!(
                        action.kind,
                        GeoRunNextActionKind::SupplyLocalArtifact
                            | GeoRunNextActionKind::SatisfyAcquisition
                            | GeoRunNextActionKind::SatisfyDiscovery
                    )
            })
            .collect();
        snapshot.awaiting_inputs.push(GeoAnswerAwaitingInput {
            blocker_id: blocker.blocker_id.clone(),
            project_node_id: blocker.project_node_id.clone(),
            reason: blocker.reason.clone(),
            required_artifact_ids: sorted_unique(
                actions
                    .iter()
                    .filter_map(|a| a.artifact_id.clone())
                    .collect(),
            ),
            expected_contracts: sorted_unique(
                actions
                    .iter()
                    .filter_map(|a| a.expected_contract.clone())
                    .collect(),
            ),
            next_commands: sorted_unique(
                actions.iter().filter_map(|a| a.command.clone()).collect(),
            ),
        });
    }
    snapshot.awaiting_inputs.sort();
    validate_run_execution(&snapshot)?;
    Ok(snapshot)
}

/// Attach an independently retained prior execution record after binding every
/// consumed output to that record. The snapshot predates this answer assembly.
pub fn project_answer_with_execution(
    inputs: &GeoAnswerInputs<'_>,
    config: &GeoAnswerConfig,
    limits: &GeoAnswerLimits,
    run: &GeoRun,
) -> Result<GeoAnswerArtifact, GeoAnswerError> {
    if run.plan_ref.question_hash != config.question_hash {
        return Err(mismatch(
            "execution.question",
            &config.question_hash,
            &run.plan_ref.question_hash,
        ));
    }
    let mut answer = project_answer(inputs, config, limits)?;
    let mut snapshot = project_answer_execution(run)?;
    let compilation = normalized_compilation(inputs.compilation)?;
    let mut required = vec![
        (
            "section",
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            canonical_tile_work_unit_bytes(inputs.section)
                .map_err(|e| invalid("section", e.to_string()))?,
        ),
        (
            "compile_evidence",
            CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            canonical_evidence_compilation_bytes(&compilation)
                .map_err(|e| invalid("compilation", e.to_string()))?,
        ),
        (
            "solve",
            CANON_GEO_COMPOSITION_VERSION,
            canonical_composition_bytes(inputs.composition)
                .map_err(|e| invalid("composition", e.to_string()))?,
        ),
        (
            "explanation",
            CANON_GEO_EXPLANATION_VERSION,
            canonical_explanation_bytes(inputs.explanation)
                .map_err(|e| invalid("explanation", e.to_string()))?,
        ),
    ];
    if let Some(next) = inputs.next_evidence {
        required.push((
            "next_evidence",
            super::CANON_GEO_NEXT_EVIDENCE_VERSION,
            canonical_next_evidence_bytes(next)
                .map_err(|error| invalid("next_evidence", error.to_string()))?,
        ));
    }
    for (output_id, contract, bytes) in &required {
        let digest = prefixed_hash(bytes);
        let matches: Vec<_> = run
            .output_refs
            .iter()
            .filter(|output| {
                output.output_id == *output_id
                    && output.contract_version == *contract
                    && output.content_digest == digest
            })
            .collect();
        if matches.len() != 1 || matches[0].byte_count != bytes.len() as u64 {
            let actual = run
                .output_refs
                .iter()
                .filter(|output| {
                    output.output_id == *output_id && output.contract_version == *contract
                })
                .map(|output| output.content_digest.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let mut error = mismatch(&format!("execution.output.{output_id}"), &digest, &actual);
            error
                .detail
                .insert("expected_byte_count".into(), bytes.len().to_string());
            return Err(error);
        }
        if !snapshot
            .completed_project_node_ids
            .contains(&matches[0].project_node_id)
        {
            return Err(invalid(
                "execution.output_refs",
                "Consumed answer input is not from a completed producer",
            ));
        }
    }
    snapshot.composition_budget_fallback =
        Some(inputs.composition.status == GeoCompositionStatus::BudgetFallback);
    answer.execution.source = GeoAnswerExecutionSource::VerifiedRunSnapshot;
    answer.execution.actual_runner_history_available = true;
    answer.execution.run_snapshot = Some(snapshot);
    validate_answer_artifact(&answer)?;
    Ok(answer)
}

fn validate_run_execution(snapshot: &GeoAnswerRunExecution) -> Result<(), GeoAnswerError> {
    for (field, digest) in [
        ("execution.run_semantic_hash", &snapshot.run_semantic_hash),
        (
            "execution.run_artifact_blake3",
            &snapshot.run_artifact_blake3,
        ),
        ("execution.plan_semantic_hash", &snapshot.plan_semantic_hash),
    ] {
        validate_digest(field, digest)?;
    }
    if snapshot.run_id
        != format!(
            "canon_geo_run.v0:{}",
            snapshot.run_semantic_hash.trim_start_matches("blake3:")
        )
        || snapshot.plan_id
            != format!(
                "canon_geo_plan.v0:{}",
                snapshot.plan_semantic_hash.trim_start_matches("blake3:")
            )
    {
        return Err(invalid(
            "execution.run_id",
            "Execution identifiers must match their semantic digests",
        ));
    }
    let states = [
        &snapshot.completed_project_node_ids,
        &snapshot.failed_project_node_ids,
        &snapshot.cancelled_project_node_ids,
        &snapshot.blocked_project_node_ids,
    ];
    let mut all = BTreeSet::new();
    for ids in states {
        require_sorted_distinct("execution.node_ids", ids)?;
        for id in ids {
            if !all.insert(id) {
                return Err(invalid(
                    "execution.node_ids",
                    "Completed and stopped node states must be disjoint",
                ));
            }
        }
    }
    require_sorted_distinct(
        "execution.executed_project_node_ids",
        &snapshot.executed_project_node_ids,
    )?;
    require_sorted_distinct(
        "execution.receipt_body_verified_project_node_ids",
        &snapshot.receipt_body_verified_project_node_ids,
    )?;
    require_sorted_distinct(
        "execution.report_only_terminal_project_node_ids",
        &snapshot.report_only_terminal_project_node_ids,
    )?;
    let terminal_ids: BTreeSet<_> = snapshot
        .failed_project_node_ids
        .iter()
        .chain(&snapshot.cancelled_project_node_ids)
        .collect();
    let report_only: BTreeSet<_> = snapshot
        .report_only_terminal_project_node_ids
        .iter()
        .collect();
    if snapshot.receipt_body_verified_project_node_ids != snapshot.completed_project_node_ids
        || report_only != terminal_ids
    {
        return Err(invalid(
            "execution.receipt_body_verified_project_node_ids",
            "Body-verified scope must equal completed nodes; report-only scope must equal failed and cancelled nodes",
        ));
    }
    require_sorted_distinct(
        "execution.resumed_project_node_ids",
        &snapshot.resumed_project_node_ids,
    )?;
    if snapshot
        .executed_project_node_ids
        .iter()
        .any(|id| snapshot.resumed_project_node_ids.contains(id))
        || sorted_unique(
            snapshot
                .executed_project_node_ids
                .iter()
                .chain(&snapshot.resumed_project_node_ids)
                .cloned()
                .collect(),
        ) != snapshot.completed_project_node_ids
    {
        return Err(invalid(
            "execution.completed_project_node_ids",
            "Completed nodes must be exactly successful execution and reuse",
        ));
    }
    if (!snapshot.failed_project_node_ids.is_empty()) != (snapshot.status == GeoRunStatus::Failed)
        || (snapshot.status == GeoRunStatus::Cancelled
            && snapshot.cancelled_project_node_ids.is_empty())
    {
        return Err(invalid(
            "execution.status",
            "Execution status contradicts reported terminal failures",
        ));
    }
    if snapshot.status == GeoRunStatus::Completed
        && (!snapshot.blocked_project_node_ids.is_empty()
            || !snapshot.cancelled_project_node_ids.is_empty()
            || !snapshot.awaiting_inputs.is_empty())
    {
        return Err(invalid(
            "execution.status",
            "Whole-run completion cannot contain stopped or waiting stages",
        ));
    }
    if snapshot.run_budget_fallback != (snapshot.status == GeoRunStatus::BudgetFallback) {
        return Err(invalid(
            "execution.run_budget_fallback",
            "Budget flag must preserve the actual run status",
        ));
    }
    require_sorted_distinct("execution.awaiting_inputs", &snapshot.awaiting_inputs)?;
    Ok(())
}

fn claim_projection(
    inputs: &GeoAnswerClaimInputs<'_>,
    compilation: &GeoEvidenceCompilationArtifact,
    config: &GeoAnswerConfig,
    limits: &GeoAnswerLimits,
    level: GeoEntityLevel,
) -> Result<GeoAnswerClaimProjection, GeoAnswerError> {
    if compilation.composition_request.profile.selection_level != level
        || inputs.composition.profile.selection_level != level
    {
        return Err(invalid(
            "entity_level",
            "Answer grain must match the supplied composition profile",
        ));
    }
    let universe = &compilation.composition_request.universe;
    if inputs.composition.summary.parcel_candidates != universe.parcels.len()
        || inputs.composition.summary.building_candidates != universe.buildings.len()
    {
        return Err(invalid(
            "composition.summary",
            "Composition candidate counts disagree with compiled evidence",
        ));
    }
    validate_composition_projection_inputs(compilation, inputs.composition)?;
    let support = association_support(compilation, inputs.composition, inputs.explanation, limits)?;
    let preference = preference(compilation, inputs.composition, &support, limits)?;
    let single = compilation.composition_request.hard_constraints.iter().any(|constraint| matches!(&constraint.constraint, GeoHardConstraintKind::Cardinality {level: candidate_level, min: 1, max: 1} if *candidate_level == level));
    let summary = &inputs.composition.summary;
    let count = (summary.residual_model_count_complete && !summary.residual_model_count_saturated)
        .then_some(summary.residual_model_count);
    Ok(GeoAnswerClaimProjection {
        claim_label: if single {
            GeoAnswerClaimLabel::SingleMemberAssociation
        } else {
            GeoAnswerClaimLabel::Composition {
                claim_class: config
                    .requested_claim_classes
                    .iter()
                    .min()
                    .copied()
                    .unwrap_or(GeoClaimClass::CollateralComposition),
            }
        },
        association_support: support,
        preference,
        logical_consequence: GeoAnswerConsequence {
            residual_model_count: count,
            residual_model_count_complete: summary.residual_model_count_complete,
            residual_model_count_saturated: summary.residual_model_count_saturated,
            hard_forced: inputs.composition.hard_forced.clone(),
            backbone_complete: inputs.composition.backbone_complete,
            conflict_constraint_ids: sorted_unique(
                inputs.composition.conflict_constraint_ids.clone(),
            ),
            conflict_core_complete: inputs.composition.conflict_core_complete,
            budget_fallback_component_keys: inputs
                .composition
                .budget_fallback
                .as_ref()
                .map(|b| sorted_unique(b.component_keys.clone()))
                .unwrap_or_default(),
            exactness_scope: "representation_relative".into(),
        },
        extent_and_reach: GeoAnswerExtent {
            candidate_reach: inputs.section.candidate_reach.status,
            truth_reach: inputs.section.candidate_reach.truth_reach_status,
            candidate_count: inputs.section.candidate_reach.candidate_count,
            missing_reference_count: inputs.section.candidate_reach.missing_reference_count,
            complete_extent_claimed: false,
            complete_extent_reason:
                "complete property/collateral membership is not established by association evidence"
                    .into(),
        },
    })
}

fn validate_composition_projection_inputs(
    compilation: &GeoEvidenceCompilationArtifact,
    composition: &GeoCompositionArtifact,
) -> Result<(), GeoAnswerError> {
    let universe = &compilation.composition_request.universe;
    let parcels: BTreeSet<_> = universe.parcels.iter().map(String::as_str).collect();
    let buildings: BTreeSet<_> = universe
        .buildings
        .iter()
        .map(|building| building.id.as_str())
        .collect();
    for (field, forced, candidates) in [
        (
            "composition.hard_forced.parcels",
            &composition.hard_forced.parcels,
            &parcels,
        ),
        (
            "composition.hard_forced.buildings",
            &composition.hard_forced.buildings,
            &buildings,
        ),
    ] {
        if forced.iter().any(|id| !candidates.contains(id.as_str())) {
            return Err(invalid(
                field,
                "Hard-forced members must belong to the bound candidate universe",
            ));
        }
        require_sorted_distinct(field, forced)?;
    }
    let mut unique = BTreeSet::new();
    for model in &composition.residual_models {
        validate_model(model)?;
        if model
            .parcels
            .iter()
            .any(|id| !parcels.contains(id.as_str()))
            || model
                .buildings
                .iter()
                .any(|id| !buildings.contains(id.as_str()))
        {
            return Err(invalid(
                "composition.residual_models",
                "Stored model members must belong to the bound candidate universe",
            ));
        }
        if !unique.insert(model) {
            return Err(invalid(
                "composition.residual_models",
                "Stored residual models must be distinct",
            ));
        }
    }
    let summary = &composition.summary;
    if summary.residual_models_materialized
        && summary.residual_model_count_complete
        && !summary.residual_model_count_saturated
        && summary.residual_model_count != composition.residual_models.len() as u64
    {
        return Err(invalid(
            "composition.summary.residual_model_count",
            "Exact materialized residual count must equal the number of distinct stored models",
        ));
    }
    Ok(())
}

fn sorted_unique<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort();
    values.dedup();
    values
}

fn root(parents: &[usize], mut index: usize) -> usize {
    while parents[index] != index {
        index = parents[index];
    }
    index
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left = root(parents, left);
    let right = root(parents, right);
    if left != right {
        parents[left.max(right)] = left.min(right);
    }
}

fn lineage_groups(
    compilation: &GeoEvidenceCompilationArtifact,
) -> (Vec<GeoAnswerLineageGroup>, BTreeMap<String, String>) {
    let admissions = &compilation.admissions;
    let mut parents: Vec<_> = (0..admissions.len()).collect();
    let mut first: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, admission) in admissions.iter().enumerate() {
        if admission.disposition == GeoEvidenceDisposition::DiagnosticOnly {
            continue;
        }
        for lineage in &admission.contract.source_lineage_ids {
            if let Some(previous) = first.insert(lineage, index) {
                union(&mut parents, index, previous);
            }
        }
    }
    let mut by_root: BTreeMap<usize, Vec<&GeoEvidenceAdmission>> = BTreeMap::new();
    for (index, admission) in admissions.iter().enumerate() {
        by_root
            .entry(root(&parents, index))
            .or_default()
            .push(admission);
    }
    let mut groups = Vec::new();
    let mut ids = BTreeMap::new();
    for group in by_root.values() {
        let observation_ids =
            sorted_unique(group.iter().map(|a| a.observation_id.clone()).collect());
        let group_id = format!("lg:{}", observation_ids[0]);
        let mut frequencies: BTreeMap<&str, usize> = BTreeMap::new();
        for admission in group {
            for lineage in &admission.contract.source_lineage_ids {
                *frequencies.entry(lineage).or_default() += 1;
            }
        }
        let shared_lineage_ids = frequencies
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(id, _)| id.to_owned())
            .collect();
        for observation in &observation_ids {
            ids.insert(observation.clone(), group_id.clone());
        }
        groups.push(GeoAnswerLineageGroup {
            group_id,
            observation_ids,
            shared_lineage_ids,
        });
    }
    groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    (groups, ids)
}

fn evidence_ref(
    admission: &GeoEvidenceAdmission,
    generated_id: Option<&str>,
    group_id: &str,
    effect: GeoAnswerEvidenceEffect,
) -> GeoAnswerEvidenceRef {
    GeoAnswerEvidenceRef {
        observation_id: admission.observation_id.clone(),
        rho_contract_id: admission.contract.id.clone(),
        rho_contract_version: admission.contract.version.clone(),
        disposition: admission.disposition,
        generated_id: generated_id.map(str::to_owned),
        admission_reason: admission.admission_reason.clone(),
        source_dataset: admission.contract.source_dataset.clone(),
        source_release: admission.contract.source_release.clone(),
        source_record_ids: sorted_unique(
            admission
                .source_records
                .iter()
                .map(|r| r.source_record_id.clone())
                .collect(),
        ),
        source_vintages: sorted_unique(
            admission
                .source_records
                .iter()
                .map(|r| r.source_vintage.clone())
                .collect(),
        ),
        source_records: admission.source_records.clone(),
        lineage_ids: admission.contract.source_lineage_ids.clone(),
        lineage_group_id: group_id.into(),
        source_derivation: GeoAnswerSourceDerivation::NotDeclared,
        effect,
    }
}

fn new_member(member: GeoEntityRef) -> GeoAnswerMemberSupport {
    GeoAnswerMemberSupport {
        member,
        hard: Vec::new(),
        soft: Vec::new(),
        diagnostic: Vec::new(),
        forced: false,
        independent_support_groups: 0,
        support_origin: GeoAnswerSupportOrigin::RecordedObservation,
        conflicting_observation_ids: Vec::new(),
    }
}

fn constraint_members(constraint: &GeoHardConstraintKind, bound: u64) -> Vec<GeoEntityRef> {
    let members = match constraint {
        GeoHardConstraintKind::Require { member } | GeoHardConstraintKind::Forbid { member } => {
            vec![member.clone()]
        }
        GeoHardConstraintKind::AnyOf { members }
        | GeoHardConstraintKind::AllOf { members }
        | GeoHardConstraintKind::AllOrNone { members } => members.clone(),
        GeoHardConstraintKind::AllowedSets { level, sets } => sorted_unique(
            sets.iter()
                .flatten()
                .map(|id| GeoEntityRef::new(*level, id))
                .collect(),
        ),
        GeoHardConstraintKind::Requires {
            if_member,
            then_member,
        } => vec![if_member.clone(), then_member.clone()],
        GeoHardConstraintKind::Cardinality { .. }
        | GeoHardConstraintKind::IntegerSumBand { .. } => Vec::new(),
    };
    if matches!(
        constraint,
        GeoHardConstraintKind::AnyOf { .. }
            | GeoHardConstraintKind::AllowedSets { .. }
            | GeoHardConstraintKind::AllOrNone { .. }
    ) && members.len() as u64 > bound
    {
        Vec::new()
    } else {
        sorted_unique(members)
    }
}

fn observation_members(observation: &GeoRhoObservationKind) -> Vec<GeoEntityRef> {
    match observation {
        GeoRhoObservationKind::PreferMember { member, .. } => vec![member.clone()],
        GeoRhoObservationKind::ExactSets { level, sets } => sorted_unique(
            sets.iter()
                .flatten()
                .map(|id| GeoEntityRef::new(*level, id))
                .collect(),
        ),
        GeoRhoObservationKind::ExistentialMembership { members }
        | GeoRhoObservationKind::AllOf { members } => members.clone(),
        GeoRhoObservationKind::IntegerSumBand { .. }
        | GeoRhoObservationKind::ExactCardinality { .. } => Vec::new(),
    }
}

fn association_support(
    compilation: &GeoEvidenceCompilationArtifact,
    composition: &GeoCompositionArtifact,
    explanation: &GeoExplanationArtifact,
    limits: &GeoAnswerLimits,
) -> Result<GeoAnswerSupport, GeoAnswerError> {
    let (groups, group_ids) = lineage_groups(compilation);
    let by_generated: BTreeMap<_, _> = compilation
        .admissions
        .iter()
        .flat_map(|admission| {
            admission
                .generated_ids
                .iter()
                .map(move |id| (id.as_str(), admission))
        })
        .collect();
    let mut members: BTreeMap<GeoEntityRef, GeoAnswerMemberSupport> = BTreeMap::new();
    let mut global = Vec::new();
    let mut diagnostic = Vec::new();
    for preference in &compilation.composition_request.soft_preferences {
        let admission = by_generated.get(preference.id.as_str()).ok_or_else(|| {
            invalid(
                "soft_preferences.id",
                "Soft preference has no bound admission",
            )
        })?;
        let reference = evidence_ref(
            admission,
            Some(&preference.id),
            &group_ids[&admission.observation_id],
            GeoAnswerEvidenceEffect::SoftAgreement,
        );
        members
            .entry(preference.member.clone())
            .or_insert_with(|| new_member(preference.member.clone()))
            .soft
            .push(reference);
    }
    for constraint in &compilation.composition_request.hard_constraints {
        let admission = by_generated.get(constraint.id.as_str()).ok_or_else(|| {
            invalid(
                "hard_constraints.id",
                "Hard constraint has no bound admission",
            )
        })?;
        let named = constraint_members(&constraint.constraint, limits.max_support_members);
        let effect = match constraint.constraint {
            GeoHardConstraintKind::Require { .. } | GeoHardConstraintKind::AllOf { .. } => {
                GeoAnswerEvidenceEffect::MembershipRequired
            }
            GeoHardConstraintKind::Forbid { .. } => GeoAnswerEvidenceEffect::MembershipForbidden,
            GeoHardConstraintKind::Cardinality { .. }
            | GeoHardConstraintKind::IntegerSumBand { .. } => {
                GeoAnswerEvidenceEffect::WholeLevelRestriction
            }
            _ => GeoAnswerEvidenceEffect::CandidateSetRestriction,
        };
        let reference = evidence_ref(
            admission,
            Some(&constraint.id),
            &group_ids[&admission.observation_id],
            effect,
        );
        if named.is_empty() {
            global.push(reference);
        } else {
            for member in named {
                members
                    .entry(member.clone())
                    .or_insert_with(|| new_member(member))
                    .hard
                    .push(reference.clone());
            }
        }
    }
    for (level, ids) in [
        (GeoEntityLevel::Parcel, &composition.hard_forced.parcels),
        (GeoEntityLevel::Building, &composition.hard_forced.buildings),
    ] {
        for id in ids {
            let member = GeoEntityRef::new(level, id);
            members
                .entry(member.clone())
                .or_insert_with(|| new_member(member))
                .forced = true;
        }
    }
    for admission in &compilation.admissions {
        if admission.disposition != GeoEvidenceDisposition::DiagnosticOnly {
            continue;
        }
        let reference = evidence_ref(
            admission,
            None,
            &group_ids[&admission.observation_id],
            GeoAnswerEvidenceEffect::DiagnosticOnly,
        );
        for member in observation_members(&admission.observation) {
            if let Some(support) = members.get_mut(&member) {
                support.diagnostic.push(reference.clone());
            }
        }
        diagnostic.push(reference);
    }
    for support in members.values_mut() {
        let admitted = support.hard.iter().chain(&support.soft);
        support.independent_support_groups = admitted
            .clone()
            .map(|r| &r.lineage_group_id)
            .collect::<BTreeSet<_>>()
            .len() as u64;
        let has_observation = admitted.clone().next().is_some();
        support.support_origin = match (has_observation, support.forced) {
            (true, true) => GeoAnswerSupportOrigin::ObservationAndConsequence,
            (false, true) => GeoAnswerSupportOrigin::LogicalConsequence,
            _ => GeoAnswerSupportOrigin::RecordedObservation,
        };
        support.conflicting_observation_ids = sorted_unique(
            explanation
                .cores
                .iter()
                .flat_map(|core| core.observation_ids.iter())
                .filter(|id| admitted.clone().any(|r| &r.observation_id == *id))
                .cloned()
                .collect(),
        );
        support.hard.sort_by(reference_order);
        support.soft.sort_by(reference_order);
        support.diagnostic.sort_by(reference_order);
    }
    let members_total = members.len() as u64;
    let mut listed: Vec<_> = members.into_values().collect();
    listed.sort_by(|a, b| {
        b.forced
            .cmp(&a.forced)
            .then_with(|| {
                b.independent_support_groups
                    .cmp(&a.independent_support_groups)
            })
            .then_with(|| a.member.cmp(&b.member))
    });
    listed.truncate(usize::try_from(limits.max_support_members).unwrap_or(usize::MAX));
    global.sort_by(reference_order);
    diagnostic.sort_by(reference_order);
    Ok(GeoAnswerSupport {
        members: listed,
        global_constraints: global,
        diagnostic_observations: diagnostic,
        lineage_groups: groups,
        members_total,
        truncated: members_total > limits.max_support_members,
    })
}

fn reference_order(a: &GeoAnswerEvidenceRef, b: &GeoAnswerEvidenceRef) -> std::cmp::Ordering {
    a.observation_id
        .cmp(&b.observation_id)
        .then_with(|| a.generated_id.cmp(&b.generated_id))
}

fn contains(model: &GeoCompositionModel, member: &GeoEntityRef) -> bool {
    match member.level {
        GeoEntityLevel::Parcel => model.parcels.contains(&member.id),
        GeoEntityLevel::Building => model.buildings.contains(&member.id),
        _ => false,
    }
}

fn soft_groups(
    compilation: &GeoEvidenceCompilationArtifact,
) -> Result<Vec<Vec<&super::GeoSoftPreference>>, GeoAnswerError> {
    let soft: Vec<_> = compilation
        .admissions
        .iter()
        .filter(|a| a.disposition == GeoEvidenceDisposition::SoftPreference)
        .collect();
    let mut parents: Vec<_> = (0..soft.len()).collect();
    let mut first: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (index, admission) in soft.iter().enumerate() {
        for lineage in &admission.contract.source_lineage_ids {
            if let Some(previous) = first.insert((&admission.contract.id, lineage), index) {
                union(&mut parents, index, previous);
            }
        }
    }
    let mut index = BTreeMap::new();
    for (position, admission) in soft.iter().enumerate() {
        for id in &admission.generated_ids {
            index.insert(id.as_str(), root(&parents, position));
        }
    }
    let mut groups: BTreeMap<usize, Vec<_>> = BTreeMap::new();
    for preference in &compilation.composition_request.soft_preferences {
        let key = index
            .get(preference.id.as_str())
            .ok_or_else(|| invalid("soft_preferences", "Missing soft admission group"))?;
        groups.entry(*key).or_default().push(preference);
    }
    Ok(groups.into_values().collect())
}

fn checked_cost(left: u128, right: u128) -> Result<u128, GeoAnswerError> {
    left.checked_add(right).ok_or_else(|| GeoAnswerError {
        code: GeoAnswerErrorCode::ArithmeticOverflow,
        message: "Answer grouped cost overflow".into(),
        detail: BTreeMap::from([("field".into(), "grouped_cost".into())]),
    })
}

fn preference_view(costs: &[(u128, GeoCompositionModel)], limit: u64) -> GeoAnswerPreferenceView {
    let mut histogram: BTreeMap<u128, u64> = BTreeMap::new();
    for (cost, _) in costs {
        *histogram.entry(*cost).or_default() += 1;
    }
    let minimum = costs.first().map(|(cost, _)| *cost);
    let tie_count = minimum
        .and_then(|cost| histogram.get(&cost).copied())
        .unwrap_or(0);
    let mut buckets = histogram.iter();
    let _ = buckets.next();
    let next = buckets.next();
    GeoAnswerPreferenceView {
        preferred: costs
            .iter()
            .take_while(|(cost, _)| Some(*cost) == minimum)
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|(cost, model)| GeoAnswerPreferredModel {
                members: model.clone(),
                cost: *cost,
            })
            .collect(),
        tie_count,
        next_best_cost: next.map(|(cost, _)| *cost),
        next_best_tie_count: next.map(|(_, count)| *count).unwrap_or(0),
        ranked_models: costs.len() as u64,
        cost_histogram: histogram
            .into_iter()
            .map(|(cost, model_count)| GeoAnswerCostBucket { cost, model_count })
            .collect(),
        truncated: tie_count > limit,
    }
}

fn preference(
    compilation: &GeoEvidenceCompilationArtifact,
    composition: &GeoCompositionArtifact,
    support: &GeoAnswerSupport,
    limits: &GeoAnswerLimits,
) -> Result<GeoAnswerPreference, GeoAnswerError> {
    let state = if compilation.composition_request.soft_preferences.is_empty() {
        GeoAnswerPreferenceState::NoSoftEvidence
    } else if composition.status == GeoCompositionStatus::Conflict {
        GeoAnswerPreferenceState::Unavailable {
            reason: GeoAnswerPreferenceUnavailable::CompositionConflict,
        }
    } else if composition.status == GeoCompositionStatus::BudgetFallback {
        GeoAnswerPreferenceState::Unavailable {
            reason: GeoAnswerPreferenceUnavailable::CompositionBudgetFallback,
        }
    } else if !composition.summary.residual_models_materialized {
        GeoAnswerPreferenceState::Unavailable {
            reason: GeoAnswerPreferenceUnavailable::ResidualModelsNotMaterialized,
        }
    } else {
        GeoAnswerPreferenceState::Ranked
    };
    if state != GeoAnswerPreferenceState::Ranked {
        return Ok(GeoAnswerPreference {
            state,
            calibration: GeoAnswerCalibration::Uncalibrated,
            composition_view: GeoAnswerPreferenceView::default(),
            grouped_view: GeoAnswerPreferenceView::default().into(),
            lineage_dependent: false,
            supporting: Vec::new(),
        });
    }
    if !composition.summary.residual_model_count_complete
        || composition.summary.residual_model_count_saturated
        || composition.residual_models.is_empty()
        || composition.residual_models.len() as u64 != composition.summary.residual_model_count
    {
        return Err(invalid(
            "composition.residual_models",
            "Ranked projection needs the complete materialized residual",
        ));
    }
    let residual: BTreeSet<_> = composition.residual_models.iter().cloned().collect();
    let ranked: BTreeSet<_> = composition
        .soft_ranked
        .iter()
        .map(|r| r.model.clone())
        .collect();
    if ranked != residual
        || ranked.len() != composition.soft_ranked.len()
        || residual.len() != composition.residual_models.len()
    {
        return Err(invalid(
            "composition.soft_ranked",
            "Ranked models must cover the exact materialized residual once",
        ));
    }
    for ranked in &composition.soft_ranked {
        let mut expected = 0;
        for preference in &compilation.composition_request.soft_preferences {
            if !contains(&ranked.model, &preference.member) {
                expected = checked_cost(expected, u128::from(preference.cost_if_absent))?;
            }
        }
        if ranked.cost != expected {
            return Err(invalid(
                "composition.soft_ranked.cost",
                "Kernel presentation cost disagrees with the bound soft preferences",
            ));
        }
    }
    let mut primary: Vec<_> = composition
        .soft_ranked
        .iter()
        .map(|r| (r.cost, r.model.clone()))
        .collect();
    primary.sort();
    let groups = soft_groups(compilation)?;
    let mut grouped = Vec::new();
    for model in &residual {
        let mut total = 0;
        for group in &groups {
            let cost = if group.iter().any(|p| contains(model, &p.member)) {
                0
            } else {
                group
                    .iter()
                    .map(|p| u128::from(p.cost_if_absent))
                    .max()
                    .unwrap_or(0)
            };
            total = checked_cost(total, cost)?;
        }
        grouped.push((total, model.clone()));
    }
    grouped.sort();
    let preferred_sets = |costs: &[(u128, GeoCompositionModel)]| {
        costs
            .iter()
            .take_while(|(cost, _)| Some(*cost) == costs.first().map(|(cost, _)| *cost))
            .map(|(_, model)| model.clone())
            .collect::<BTreeSet<_>>()
    };
    let lineage_dependent = preferred_sets(&primary) != preferred_sets(&grouped);
    let composition_view = preference_view(&primary, limits.max_preferred_models);
    let grouped_view: GeoAnswerGroupedPreferenceView =
        preference_view(&grouped, limits.max_preferred_models).into();
    let visible_models: BTreeSet<_> = composition_view
        .preferred
        .iter()
        .chain(&grouped_view.preferred)
        .map(|p| p.members.clone())
        .collect();
    let mut supporting = Vec::new();
    for model in visible_models {
        let generated: BTreeSet<_> = compilation
            .composition_request
            .soft_preferences
            .iter()
            .filter(|p| contains(&model, &p.member))
            .map(|p| p.id.as_str())
            .collect();
        let observations = sorted_unique(
            compilation
                .admissions
                .iter()
                .filter(|a| {
                    a.disposition == GeoEvidenceDisposition::SoftPreference
                        && a.generated_ids
                            .iter()
                            .any(|id| generated.contains(id.as_str()))
                })
                .map(|a| a.observation_id.clone())
                .collect(),
        );
        let lineage_group_ids = sorted_unique(
            support
                .lineage_groups
                .iter()
                .filter(|group| {
                    group
                        .observation_ids
                        .iter()
                        .any(|id| observations.contains(id))
                })
                .map(|g| g.group_id.clone())
                .collect(),
        );
        supporting.push(GeoAnswerPreferredSupport {
            members: model,
            observation_ids: observations,
            shared_lineage_ids: sorted_unique(
                support
                    .lineage_groups
                    .iter()
                    .filter(|g| lineage_group_ids.contains(&g.group_id))
                    .flat_map(|g| g.shared_lineage_ids.clone())
                    .collect(),
            ),
            lineage_group_ids,
        });
    }
    Ok(GeoAnswerPreference {
        state,
        calibration: GeoAnswerCalibration::Uncalibrated,
        composition_view,
        grouped_view,
        lineage_dependent,
        supporting,
    })
}

fn validate_digest(field: &str, value: &str) -> Result<(), GeoAnswerError> {
    if !value.strip_prefix("blake3:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err(invalid(
            field,
            "Expected a lowercase, prefixed BLAKE3 digest",
        ));
    }
    Ok(())
}

fn require_sorted_distinct<T: Ord>(field: &str, values: &[T]) -> Result<(), GeoAnswerError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(
            field,
            "Answer collections must be sorted and distinct",
        ));
    }
    Ok(())
}

fn validate_model(model: &GeoCompositionModel) -> Result<(), GeoAnswerError> {
    require_sorted_distinct("model.parcels", &model.parcels)?;
    require_sorted_distinct("model.buildings", &model.buildings)?;
    if model
        .parcels
        .iter()
        .chain(&model.buildings)
        .any(|id| id.trim().is_empty())
        || (model.parcels.is_empty() && model.buildings.is_empty())
    {
        return Err(invalid(
            "model",
            "A preferred model must contain named members",
        ));
    }
    Ok(())
}

fn validate_view(view: &GeoAnswerPreferenceView, ranked: bool) -> Result<(), GeoAnswerError> {
    if !ranked {
        if view != &GeoAnswerPreferenceView::default() {
            return Err(invalid(
                "preference",
                "Unavailable or absent soft evidence cannot carry a fabricated ordering",
            ));
        }
        return Ok(());
    }
    if view.preferred.is_empty()
        || view.tie_count == 0
        || view.ranked_models == 0
        || view.cost_histogram.is_empty()
    {
        return Err(invalid(
            "preference",
            "A ranked view requires preferred models and exact counts",
        ));
    }
    if view
        .cost_histogram
        .windows(2)
        .any(|pair| pair[0].cost >= pair[1].cost)
        || view
            .cost_histogram
            .iter()
            .any(|bucket| bucket.model_count == 0)
    {
        return Err(invalid(
            "preference.cost_histogram",
            "Cost histogram must be ordered, distinct and nonempty",
        ));
    }
    let total = view
        .cost_histogram
        .iter()
        .try_fold(0_u64, |sum, bucket| sum.checked_add(bucket.model_count))
        .ok_or_else(|| invalid("preference.ranked_models", "Model count overflow"))?;
    if total != view.ranked_models
        || view.tie_count != view.cost_histogram[0].model_count
        || view.preferred.len() as u64 > view.tie_count
        || view.truncated != ((view.preferred.len() as u64) < view.tie_count)
    {
        return Err(invalid(
            "preference.tie_count",
            "Preference totals/truncation disagree with its exact histogram",
        ));
    }
    let second = view.cost_histogram.get(1);
    if view.next_best_cost != second.map(|bucket| bucket.cost)
        || view.next_best_tie_count != second.map(|bucket| bucket.model_count).unwrap_or(0)
    {
        return Err(invalid(
            "preference.next_best_cost",
            "Next-best cost and ties must follow the histogram",
        ));
    }
    let mut previous = None;
    for model in &view.preferred {
        validate_model(&model.members)?;
        if model.cost != view.cost_histogram[0].cost
            || previous.is_some_and(|prior: &GeoCompositionModel| prior >= &model.members)
        {
            return Err(invalid(
                "preference.preferred",
                "Preferred members must be ordered distinct models at the minimum cost",
            ));
        }
        previous = Some(&model.members);
    }
    Ok(())
}

fn validate_reference(
    reference: &GeoAnswerEvidenceRef,
    groups: &BTreeMap<&str, &GeoAnswerLineageGroup>,
) -> Result<(), GeoAnswerError> {
    let Some(group) = groups.get(reference.lineage_group_id.as_str()) else {
        return Err(invalid(
            "association_support.lineage_group_id",
            "Evidence refers to an unknown lineage group",
        ));
    };
    if group
        .observation_ids
        .binary_search(&reference.observation_id)
        .is_err()
    {
        return Err(invalid(
            "association_support.observation_id",
            "Evidence observation does not belong to its lineage group",
        ));
    }
    if reference.observation_id.trim().is_empty()
        || reference.rho_contract_id.trim().is_empty()
        || reference.rho_contract_version.trim().is_empty()
        || reference.source_dataset.trim().is_empty()
        || reference.source_release.trim().is_empty()
        || reference.source_records.is_empty()
    {
        return Err(invalid(
            "association_support.evidence",
            "Evidence identity, version and retained provenance are required",
        ));
    }
    require_sorted_distinct("source_record_ids", &reference.source_record_ids)?;
    require_sorted_distinct("source_vintages", &reference.source_vintages)?;
    require_sorted_distinct("lineage_ids", &reference.lineage_ids)?;
    require_sorted_distinct("source_records", &reference.source_records)?;
    if reference.source_record_ids
        != sorted_unique(
            reference
                .source_records
                .iter()
                .map(|r| r.source_record_id.clone())
                .collect(),
        )
        || reference.source_vintages
            != sorted_unique(
                reference
                    .source_records
                    .iter()
                    .map(|r| r.source_vintage.clone())
                    .collect(),
            )
    {
        return Err(invalid(
            "association_support.source_records",
            "Source summaries must preserve actual record references",
        ));
    }
    if (reference.disposition == GeoEvidenceDisposition::DiagnosticOnly)
        != reference.generated_id.is_none()
    {
        return Err(invalid(
            "association_support.generated_id",
            "Diagnostic records have no generated solver identifier; admitted records require one",
        ));
    }
    if reference.disposition == GeoEvidenceDisposition::DiagnosticOnly
        && reference.effect != GeoAnswerEvidenceEffect::DiagnosticOnly
    {
        return Err(invalid(
            "association_support.effect",
            "Diagnostic evidence cannot claim an admitted membership effect",
        ));
    }
    Ok(())
}

/// Validate projection invariants without reconstructing any feasible set.
pub fn validate_answer_artifact(answer: &GeoAnswerArtifact) -> Result<(), GeoAnswerError> {
    if answer.version != CANON_GEO_ANSWER_VERSION {
        return Err(GeoAnswerError {
            code: GeoAnswerErrorCode::UnsupportedVersion,
            message: "Unsupported answer artifact version".into(),
            detail: BTreeMap::from([("actual".into(), answer.version.clone())]),
        });
    }
    validate_digest("question_hash", &answer.question_hash)?;
    if answer.question_id.trim().is_empty() {
        return Err(invalid("question_id", "Question identifier is required"));
    }
    for (field, digest) in [
        ("section_blake3", &answer.inputs.section_blake3),
        (
            "evidence_compilation_blake3",
            &answer.inputs.evidence_compilation_blake3,
        ),
        ("composition_blake3", &answer.inputs.composition_blake3),
        ("explanation_blake3", &answer.inputs.explanation_blake3),
    ] {
        validate_digest(field, digest)?;
    }
    if let Some(digest) = &answer.inputs.next_evidence_blake3 {
        validate_digest("next_evidence_blake3", digest)?;
    }
    match (
        &answer.execution.run_snapshot,
        answer.execution.source,
        answer.execution.actual_runner_history_available,
    ) {
        (None, GeoAnswerExecutionSource::SuppliedArtifactProjection, false) => {}
        (Some(snapshot), GeoAnswerExecutionSource::VerifiedRunSnapshot, true) => {
            validate_run_execution(snapshot)?;
            if snapshot.composition_budget_fallback
                != Some(answer.execution.composition_status == GeoCompositionStatus::BudgetFallback)
            {
                return Err(invalid(
                    "execution.composition_budget_fallback",
                    "Composition budget state must come from the bound composition",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "execution.actual_runner_history_available",
                "Runner history availability must match its verified retained snapshot source",
            ));
        }
    }
    require_sorted_distinct(
        "execution.consumed_stages",
        &answer.execution.consumed_stages,
    )?;
    require_sorted_distinct(
        "execution.omitted_by_scope",
        &answer.execution.omitted_by_scope,
    )?;
    if answer.extent_and_reach.complete_extent_claimed
        || answer
            .extent_and_reach
            .complete_extent_reason
            .trim()
            .is_empty()
    {
        return Err(invalid(
            "extent_and_reach.complete_extent_claimed",
            "Association evidence does not establish complete property extent",
        ));
    }
    if answer.logical_consequence.exactness_scope != "representation_relative" {
        return Err(invalid(
            "logical_consequence.exactness_scope",
            "Exactness is relative to the retained representation",
        ));
    }
    let consequence = &answer.logical_consequence;
    if consequence.residual_model_count.is_some()
        != (consequence.residual_model_count_complete
            && !consequence.residual_model_count_saturated)
    {
        return Err(invalid(
            "logical_consequence.residual_model_count",
            "Incomplete or saturated counts must not be printed as exact numbers",
        ));
    }
    require_sorted_distinct("hard_forced.parcels", &consequence.hard_forced.parcels)?;
    require_sorted_distinct("hard_forced.buildings", &consequence.hard_forced.buildings)?;
    require_sorted_distinct(
        "conflict_constraint_ids",
        &consequence.conflict_constraint_ids,
    )?;
    require_sorted_distinct(
        "budget_fallback_component_keys",
        &consequence.budget_fallback_component_keys,
    )?;
    if (answer.acceptance.policy.is_none()
        && answer.acceptance.status != GeoAnswerAcceptanceStatus::NoPolicyDeclared)
        || (answer.acceptance.policy.is_some()
            && answer.acceptance.status != GeoAnswerAcceptanceStatus::PolicyNotEvaluated)
    {
        return Err(invalid(
            "acceptance.status",
            "Policy references do not establish evaluated acceptance",
        ));
    }
    let support = &answer.association_support;
    if support.members_total < support.members.len() as u64
        || support.truncated != (support.members_total > support.members.len() as u64)
    {
        return Err(invalid(
            "association_support.members_total",
            "Member totals must preserve presentation truncation",
        ));
    }
    let mut groups = BTreeMap::new();
    let mut observed = BTreeSet::new();
    let mut prior_group = None;
    for group in &support.lineage_groups {
        require_sorted_distinct("lineage_groups.observation_ids", &group.observation_ids)?;
        require_sorted_distinct(
            "lineage_groups.shared_lineage_ids",
            &group.shared_lineage_ids,
        )?;
        if group.observation_ids.is_empty()
            || group.group_id != format!("lg:{}", group.observation_ids[0])
            || prior_group.is_some_and(|prior: &String| prior >= &group.group_id)
        {
            return Err(invalid(
                "association_support.lineage_groups",
                "Lineage identifiers must be canonical observation-based groups",
            ));
        }
        for observation in &group.observation_ids {
            if !observed.insert(observation) {
                return Err(invalid(
                    "association_support.lineage_groups",
                    "An observation cannot occur in two lineage groups",
                ));
            }
        }
        groups.insert(group.group_id.as_str(), group);
        prior_group = Some(&group.group_id);
    }
    let mut member_ids = BTreeSet::new();
    for member in &support.members {
        if !member_ids.insert(&member.member) {
            return Err(invalid(
                "association_support.members",
                "Duplicate member details",
            ));
        }
        let forced = match member.member.level {
            GeoEntityLevel::Parcel => consequence.hard_forced.parcels.contains(&member.member.id),
            GeoEntityLevel::Building => consequence
                .hard_forced
                .buildings
                .contains(&member.member.id),
            _ => false,
        };
        if member.forced != forced {
            return Err(invalid(
                "association_support.forced",
                "Member forcing must come from the supplied hard backbone",
            ));
        }
        let independent = member
            .hard
            .iter()
            .chain(&member.soft)
            .map(|r| &r.lineage_group_id)
            .collect::<BTreeSet<_>>()
            .len() as u64;
        if independent != member.independent_support_groups {
            return Err(invalid(
                "association_support.independent_support_groups",
                "Source counts are not independent evidence counts",
            ));
        }
        for (references, disposition) in [
            (&member.hard, GeoEvidenceDisposition::HardConstraint),
            (&member.soft, GeoEvidenceDisposition::SoftPreference),
            (&member.diagnostic, GeoEvidenceDisposition::DiagnosticOnly),
        ] {
            for reference in references {
                validate_reference(reference, &groups)?;
                if reference.disposition != disposition {
                    return Err(invalid(
                        "association_support.disposition",
                        "Evidence authority must remain in its admitted class",
                    ));
                }
            }
        }
    }
    for reference in support
        .global_constraints
        .iter()
        .chain(&support.diagnostic_observations)
    {
        validate_reference(reference, &groups)?;
    }
    let ranked = answer.preference.state == GeoAnswerPreferenceState::Ranked;
    validate_view(&answer.preference.composition_view, ranked)?;
    validate_view(&answer.preference.grouped_view.preference_view(), ranked)?;
    if answer.preference.grouped_view.rule_id != GEO_ANSWER_GROUPED_RULE_ID {
        return Err(invalid(
            "preference.grouped_view.rule_id",
            "Unknown diagnostic grouping rule",
        ));
    }
    if ranked {
        if answer.preference.composition_view.ranked_models
            != answer.preference.grouped_view.ranked_models
            || consequence.residual_model_count
                != Some(answer.preference.composition_view.ranked_models)
        {
            return Err(invalid(
                "preference.ranked_models",
                "Ranked views must cover the complete reported residual",
            ));
        }
    } else if answer.preference.lineage_dependent || !answer.preference.supporting.is_empty() {
        return Err(invalid(
            "preference",
            "Unavailable ranking cannot invent preference support or lineage effects",
        ));
    }
    match &answer.preference.state {
        GeoAnswerPreferenceState::Unavailable {
            reason: GeoAnswerPreferenceUnavailable::CompositionConflict,
        } if answer.execution.composition_status != GeoCompositionStatus::Conflict => {
            return Err(invalid(
                "preference.state",
                "Conflict reason disagrees with composition",
            ));
        }
        GeoAnswerPreferenceState::Unavailable {
            reason: GeoAnswerPreferenceUnavailable::CompositionBudgetFallback,
        } if answer.execution.composition_status != GeoCompositionStatus::BudgetFallback => {
            return Err(invalid(
                "preference.state",
                "Budget reason disagrees with composition",
            ));
        }
        GeoAnswerPreferenceState::Ranked
            if matches!(
                answer.execution.composition_status,
                GeoCompositionStatus::Conflict | GeoCompositionStatus::BudgetFallback
            ) =>
        {
            return Err(invalid(
                "preference.state",
                "Conflict or budget fallback cannot claim an available ordering",
            ));
        }
        _ => {}
    }
    if answer.further_work.source == GeoAnswerFurtherWorkSource::NotInScope {
        if answer.inputs.next_evidence_blake3.is_some()
            || answer.further_work.action_discovery != GeoActionDiscoveryState::NotAttempted
            || !answer.further_work.frontier_action_ids.is_empty()
            || answer.further_work.stop.is_some()
            || answer.answer_scope == GeoAnswerScope::FullWorkflow
        {
            return Err(invalid(
                "further_work",
                "Absent prospective artifacts cannot establish discovery coverage or a stop",
            ));
        }
    } else if answer.inputs.next_evidence_blake3.is_none() {
        return Err(invalid(
            "further_work",
            "Further work requires its retained artifact digest",
        ));
    }
    if answer.further_work.stop == Some(GeoStopReason::HonestAmbiguity)
        && answer.further_work.action_discovery
            != GeoActionDiscoveryState::ExhaustedWithinDeclaredCapabilitiesAndBudget
    {
        return Err(invalid(
            "further_work.action_discovery",
            "Ambiguity does not prove exhausted action discovery",
        ));
    }
    Ok(())
}

pub fn canonical_answer_bytes(answer: &GeoAnswerArtifact) -> Result<Vec<u8>, GeoAnswerError> {
    validate_answer_artifact(answer)?;
    serde_json::to_vec(answer).map_err(|e| invalid("answer", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_view_roundtrips_full_width_integer_costs_without_content_buffering() {
        let cost = u128::from(u64::MAX) + 1;
        let view: GeoAnswerGroupedPreferenceView = GeoAnswerPreferenceView {
            preferred: vec![GeoAnswerPreferredModel {
                members: GeoCompositionModel {
                    parcels: vec!["member-a".into()],
                    buildings: Vec::new(),
                },
                cost,
            }],
            tie_count: 1,
            next_best_cost: None,
            next_best_tie_count: 0,
            ranked_models: 1,
            cost_histogram: vec![GeoAnswerCostBucket {
                cost,
                model_count: 1,
            }],
            truncated: false,
        }
        .into();
        let bytes = serde_json::to_vec(&view).expect("serialize exact wide cost");
        let decoded: GeoAnswerGroupedPreferenceView =
            serde_json::from_slice(&bytes).expect("direct integer deserialization supports u128");
        assert_eq!(view, decoded);
        validate_view(&decoded.preference_view(), true).expect("retained counts and costs agree");
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
        let mut bad = String::from_utf8(bytes).unwrap();
        bad.insert_str(1, "\"unexpected\":true,");
        assert!(
            serde_json::from_str::<GeoAnswerGroupedPreferenceView>(&bad).is_err(),
            "strict grouped view cannot accept unknown fields"
        );
    }
}
