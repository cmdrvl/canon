#![forbid(unsafe_code)]

//! Residual-aware next-evidence recommendations for Canon Geo.
//!
//! This module is a deterministic controller over already-materialized Geo
//! artifacts. It never acquires data and never mutates a run.

use crate::geo::{
    GeoAcquisitionRequest, GeoCompositionArtifact, GeoCompositionStatus, GeoDecisionPolicyRef,
    GeoExplanationSubjectRef, GeoOutcomeSeparation, GeoResourceBudget, GeoResourceCounter,
    GeoSeparationArtifact, canonical_composition_bytes, canonical_separation_bytes,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION: &str = "canon_geo_next_evidence_request.v0";
pub const CANON_GEO_NEXT_EVIDENCE_VERSION: &str = "canon_geo_next_evidence.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNextActionClass {
    RepairReach,
    DiagnoseConflict,
    SeparateResidual,
    RaiseClaimClass,
    Stop,
}

impl Default for GeoNextActionClass {
    fn default() -> Self {
        Self::SeparateResidual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum GeoNextActionKind {
    Acquire(Box<GeoAcquisitionRequest>),
    Adjudicate(String),
    Observe(String),
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoStopReason {
    ClaimForced,
    AllActionsRedundant,
    GrainUnsupported,
    HonestAmbiguity,
    BudgetExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoDominanceBasis {
    Exact,
    Bounds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextAction {
    pub action_id: String,
    #[serde(default)]
    pub class: GeoNextActionClass,
    pub kind: GeoNextActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<String>,
    pub cost_units: u64,
    pub separation: Vec<GeoOutcomeSeparation>,
    pub worst_case_remaining: u64,
    #[serde(default)]
    pub redundant: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dominated_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<GeoStopReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoLossModelRef {
    pub loss_model_id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextEvidencePolicy {
    pub policy: GeoDecisionPolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_model: Option<GeoLossModelRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextEvidenceRequest {
    pub version: String,
    pub composition_blake3: String,
    pub separation_blake3: String,
    pub candidates: Vec<GeoNextAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<GeoNextEvidencePolicy>,
    pub budget: GeoResourceBudget,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub budget_spent: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextEvidenceRankingAbstention {
    pub code: GeoNextEvidenceErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextEvidenceArtifact {
    pub version: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<GeoExplanationSubjectRef>,
    pub composition_blake3: String,
    pub separation_blake3: String,
    pub frontier: Vec<GeoNextAction>,
    pub dominated: Vec<GeoNextAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ranking: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranking_abstention: Option<GeoNextEvidenceRankingAbstention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<GeoStopReason>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub budget_remaining: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dominance_basis: BTreeMap<String, GeoDominanceBasis>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoNextEvidenceErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    NextEvidenceNoLossModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNextEvidenceError {
    pub code: GeoNextEvidenceErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoNextEvidenceError {
    fn new(
        code: GeoNextEvidenceErrorCode,
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    fn invalid(
        message: impl Into<String>,
        detail: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self::new(GeoNextEvidenceErrorCode::InvalidInput, message, detail)
    }

    fn unsupported_version(expected: &'static str, actual: &str, artifact: &'static str) -> Self {
        Self::new(
            GeoNextEvidenceErrorCode::UnsupportedVersion,
            format!("{artifact} declares an unsupported version"),
            [
                ("expected".to_string(), expected.to_string()),
                ("actual".to_string(), actual.to_string()),
            ],
        )
    }
}

impl fmt::Display for GeoNextEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeoNextEvidenceError {}

pub fn recommend(
    composition: &GeoCompositionArtifact,
    separation: &GeoSeparationArtifact,
    candidates: &[GeoNextAction],
    policy: Option<&GeoNextEvidencePolicy>,
    budget: &GeoResourceBudget,
    spent: &BTreeMap<String, u64>,
) -> Result<GeoNextEvidenceArtifact, GeoNextEvidenceError> {
    validate_composition_input(composition)?;
    validate_separation_input(separation)?;
    validate_budget(budget, spent)?;

    let composition_blake3 =
        prefixed_hash(&canonical_composition_bytes(composition).map_err(|error| {
            GeoNextEvidenceError::invalid(
                "Geo composition artifact could not be serialized",
                [("serde_error", error.to_string())],
            )
        })?);
    let separation_bytes = canonical_separation_bytes(separation).map_err(|error| {
        GeoNextEvidenceError::invalid(
            "Geo separation artifact is not canonical",
            [("source_error", error.to_string())],
        )
    })?;
    let separation_blake3 = prefixed_hash(&separation_bytes);

    let mut normalized = normalize_candidates(candidates, separation)?;
    let budget_remaining = remaining_budget(budget, spent);
    let budget_exceeded = operations_budget_exhausted(budget, spent);

    let forced = claim_forced(composition);
    let explicit_stop = normalized.iter().find_map(explicit_stop_reason);
    let no_actionable_candidates = normalized
        .iter()
        .all(|candidate| candidate.redundant || candidate.class == GeoNextActionClass::Stop);
    let stop = if forced {
        Some(GeoStopReason::ClaimForced)
    } else if budget_exceeded {
        Some(GeoStopReason::BudgetExceeded)
    } else if let Some(reason) = explicit_stop {
        Some(reason)
    } else if !normalized.is_empty() && no_actionable_candidates {
        Some(GeoStopReason::AllActionsRedundant)
    } else if normalized.is_empty() && composition.status == GeoCompositionStatus::Ambiguous {
        Some(GeoStopReason::HonestAmbiguity)
    } else {
        None
    };

    let (frontier, dominated, dominance_basis) = if forced
        || matches!(
            stop,
            Some(GeoStopReason::GrainUnsupported | GeoStopReason::HonestAmbiguity)
        ) {
        (Vec::new(), Vec::new(), BTreeMap::new())
    } else {
        classify_frontier(composition, separation, &mut normalized)
    };

    let total_ranking = policy.and_then(|policy| {
        policy
            .loss_model
            .as_ref()
            .map(|_| ranked_action_ids(&frontier, &dominated))
    });
    let ranking_abstention = if total_ranking.is_none() {
        Some(no_loss_model_abstention(policy))
    } else {
        None
    };

    let mut counters = BTreeMap::new();
    counters.insert("candidate_actions".to_string(), candidates.len() as u64);
    counters.insert("frontier_actions".to_string(), frontier.len() as u64);
    counters.insert("dominated_actions".to_string(), dominated.len() as u64);
    counters.insert("budget_exhausted".to_string(), u64::from(budget_exceeded));

    let run_id = next_evidence_run_id(
        &composition_blake3,
        &separation_blake3,
        candidates,
        policy,
        budget,
        spent,
    )?;
    let artifact = GeoNextEvidenceArtifact {
        version: CANON_GEO_NEXT_EVIDENCE_VERSION.to_string(),
        run_id,
        subject_ref: separation.subject_ref.clone(),
        composition_blake3,
        separation_blake3,
        frontier,
        dominated,
        total_ranking,
        ranking_abstention,
        stop,
        budget_remaining,
        dominance_basis,
        counters,
    };
    validate_next_evidence_artifact(&artifact)?;
    Ok(artifact)
}

pub fn recommend_from_request(
    composition: &GeoCompositionArtifact,
    separation: &GeoSeparationArtifact,
    request: &GeoNextEvidenceRequest,
) -> Result<GeoNextEvidenceArtifact, GeoNextEvidenceError> {
    validate_next_evidence_request(request)?;
    let composition_blake3 =
        prefixed_hash(&canonical_composition_bytes(composition).map_err(|error| {
            GeoNextEvidenceError::invalid(
                "Geo composition artifact could not be serialized",
                [("serde_error", error.to_string())],
            )
        })?);
    if request.composition_blake3 != composition_blake3 {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence request composition digest does not match the supplied artifact",
            [
                ("expected", composition_blake3),
                ("actual", request.composition_blake3.clone()),
            ],
        ));
    }
    let separation_blake3 =
        prefixed_hash(&canonical_separation_bytes(separation).map_err(|error| {
            GeoNextEvidenceError::invalid(
                "Geo separation artifact is not canonical",
                [("source_error", error.to_string())],
            )
        })?);
    if request.separation_blake3 != separation_blake3 {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence request separation digest does not match the supplied artifact",
            [
                ("expected", separation_blake3),
                ("actual", request.separation_blake3.clone()),
            ],
        ));
    }
    recommend(
        composition,
        separation,
        &request.candidates,
        request.policy.as_ref(),
        &request.budget,
        &request.budget_spent,
    )
}

pub fn validate_next_evidence_request(
    request: &GeoNextEvidenceRequest,
) -> Result<(), GeoNextEvidenceError> {
    if request.version != CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION {
        return Err(GeoNextEvidenceError::unsupported_version(
            CANON_GEO_NEXT_EVIDENCE_REQUEST_VERSION,
            &request.version,
            "Geo next-evidence request",
        ));
    }
    validate_prefixed_blake3("composition_blake3", &request.composition_blake3)?;
    validate_prefixed_blake3("separation_blake3", &request.separation_blake3)?;
    validate_budget(&request.budget, &request.budget_spent)?;
    validate_candidate_order(&request.candidates)
}

pub fn canonical_next_evidence_request_bytes(
    request: &GeoNextEvidenceRequest,
) -> Result<Vec<u8>, GeoNextEvidenceError> {
    validate_next_evidence_request(request)?;
    serde_json::to_vec(request).map_err(|error| {
        GeoNextEvidenceError::invalid(
            "Geo next-evidence request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn validate_next_evidence_artifact(
    artifact: &GeoNextEvidenceArtifact,
) -> Result<(), GeoNextEvidenceError> {
    if artifact.version != CANON_GEO_NEXT_EVIDENCE_VERSION {
        return Err(GeoNextEvidenceError::unsupported_version(
            CANON_GEO_NEXT_EVIDENCE_VERSION,
            &artifact.version,
            "Geo next-evidence artifact",
        ));
    }
    if artifact.run_id.is_empty() {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence artifact run_id must be nonempty",
            [("field", "run_id")],
        ));
    }
    validate_prefixed_blake3("composition_blake3", &artifact.composition_blake3)?;
    validate_prefixed_blake3("separation_blake3", &artifact.separation_blake3)?;
    validate_candidate_order(&artifact.frontier)?;
    validate_candidate_order(&artifact.dominated)?;
    reject_overlap(&artifact.frontier, &artifact.dominated)?;
    if let Some(ranking) = &artifact.total_ranking {
        reject_duplicate_strings("total_ranking", ranking)?;
        let known = artifact
            .frontier
            .iter()
            .chain(artifact.dominated.iter())
            .map(|candidate| candidate.action_id.as_str())
            .collect::<BTreeSet<_>>();
        for action_id in ranking {
            if !known.contains(action_id.as_str()) {
                return Err(GeoNextEvidenceError::invalid(
                    "Geo next-evidence ranking names an unknown action",
                    [("action_id", action_id.clone())],
                ));
            }
        }
    }
    let serialized = serde_json::to_value(artifact).map_err(|error| {
        GeoNextEvidenceError::invalid(
            "Geo next-evidence artifact could not be inspected",
            [("serde_error", error.to_string())],
        )
    })?;
    if let Some(field) = forbidden_information_key(&serialized) {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence artifact contains a disallowed information-value field",
            [("field", field)],
        ));
    }
    Ok(())
}

pub fn canonical_next_evidence_bytes(
    artifact: &GeoNextEvidenceArtifact,
) -> Result<Vec<u8>, GeoNextEvidenceError> {
    validate_next_evidence_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoNextEvidenceError::invalid(
            "Geo next-evidence artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

fn validate_composition_input(
    composition: &GeoCompositionArtifact,
) -> Result<(), GeoNextEvidenceError> {
    if composition.version != crate::geo::CANON_GEO_COMPOSITION_VERSION {
        return Err(GeoNextEvidenceError::unsupported_version(
            crate::geo::CANON_GEO_COMPOSITION_VERSION,
            &composition.version,
            "Geo composition artifact",
        ));
    }
    Ok(())
}

fn validate_separation_input(
    separation: &GeoSeparationArtifact,
) -> Result<(), GeoNextEvidenceError> {
    if separation.version != crate::geo::CANON_GEO_SEPARATION_VERSION {
        return Err(GeoNextEvidenceError::unsupported_version(
            crate::geo::CANON_GEO_SEPARATION_VERSION,
            &separation.version,
            "Geo separation artifact",
        ));
    }
    Ok(())
}

fn validate_budget(
    budget: &GeoResourceBudget,
    spent: &BTreeMap<String, u64>,
) -> Result<(), GeoNextEvidenceError> {
    if budget.version != crate::geo::CANON_GEO_RESOURCE_BUDGET_VERSION {
        return Err(GeoNextEvidenceError::unsupported_version(
            crate::geo::CANON_GEO_RESOURCE_BUDGET_VERSION,
            &budget.version,
            "Geo resource budget",
        ));
    }
    let mut seen = BTreeSet::new();
    for bound in &budget.deterministic_bounds {
        if bound.semantic_id.is_empty() {
            return Err(GeoNextEvidenceError::invalid(
                "Geo resource budget bound semantic_id must be nonempty",
                [("field", "budget.deterministic_bounds.semantic_id")],
            ));
        }
        if !seen.insert(bound.semantic_id.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo resource budget bound semantic_id must be unique",
                [("semantic_id", bound.semantic_id.clone())],
            ));
        }
    }
    for semantic_id in spent.keys() {
        if !seen.contains(semantic_id.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence budget_spent names an unknown budget bound",
                [("semantic_id", semantic_id.clone())],
            ));
        }
    }
    Ok(())
}

fn remaining_budget(
    budget: &GeoResourceBudget,
    spent: &BTreeMap<String, u64>,
) -> BTreeMap<String, u64> {
    budget
        .deterministic_bounds
        .iter()
        .map(|bound| {
            let used = spent.get(&bound.semantic_id).copied().unwrap_or_default();
            (bound.semantic_id.clone(), bound.value.saturating_sub(used))
        })
        .collect()
}

fn operations_budget_exhausted(budget: &GeoResourceBudget, spent: &BTreeMap<String, u64>) -> bool {
    budget.deterministic_bounds.iter().any(|bound| {
        bound.counter == GeoResourceCounter::Operations
            && spent.get(&bound.semantic_id).copied().unwrap_or_default() >= bound.value
    })
}

fn normalize_candidates(
    candidates: &[GeoNextAction],
    separation: &GeoSeparationArtifact,
) -> Result<Vec<GeoNextAction>, GeoNextEvidenceError> {
    validate_candidate_order(candidates)?;
    let observations = separation
        .per_observation
        .iter()
        .map(|observation| (observation.observation_id.as_str(), observation))
        .collect::<BTreeMap<_, _>>();
    let mut normalized = Vec::with_capacity(candidates.len());
    let mut first_by_lineage = BTreeMap::<Vec<String>, String>::new();
    for candidate in candidates {
        if candidate.action_id.is_empty() {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence candidate action_id must be nonempty",
                [("field", "action_id")],
            ));
        }
        if candidate.class != GeoNextActionClass::Stop && candidate.separation.is_empty() {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence non-stop action must carry separation outcomes",
                [("action_id", candidate.action_id.clone())],
            ));
        }
        let mut next = candidate.clone();
        validate_action_separation(&next)?;
        if let Some(observation_id) = bound_observation_id(&next)
            && let Some(observation) = observations.get(observation_id)
        {
            if next.separation != observation.per_outcome {
                return Err(GeoNextEvidenceError::invalid(
                    "Geo next-evidence candidate separation does not match the separation artifact",
                    [
                        ("action_id", next.action_id.clone()),
                        ("observation_id", observation_id.to_string()),
                    ],
                ));
            }
            if next.worst_case_remaining != observation.worst_case_remaining {
                return Err(GeoNextEvidenceError::invalid(
                    "Geo next-evidence candidate worst-case count does not match the separation artifact",
                    [
                        ("action_id", next.action_id.clone()),
                        ("observation_id", observation_id.to_string()),
                    ],
                ));
            }
            next.redundant |= observation.redundant;
        }
        if !next.lineage_ids.is_empty() {
            let mut lineage = next.lineage_ids.clone();
            lineage.sort();
            lineage.dedup();
            if lineage.len() != next.lineage_ids.len() {
                return Err(GeoNextEvidenceError::invalid(
                    "Geo next-evidence lineage_ids must be unique",
                    [("action_id", next.action_id.clone())],
                ));
            }
            if let Some(first) = first_by_lineage.insert(lineage, next.action_id.clone()) {
                next.redundant = true;
                if !next.dominated_by.iter().any(|id| id == &first) {
                    next.dominated_by.push(first);
                    next.dominated_by.sort();
                }
            }
        }
        normalized.push(next);
    }
    Ok(normalized)
}

fn validate_action_separation(candidate: &GeoNextAction) -> Result<(), GeoNextEvidenceError> {
    let mut previous: Option<&str> = None;
    let mut worst = 0;
    for outcome in &candidate.separation {
        if outcome.outcome_id.is_empty() {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence outcome_id must be nonempty",
                [("action_id", candidate.action_id.clone())],
            ));
        }
        if previous.is_some_and(|prior| prior >= outcome.outcome_id.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence outcomes must be strictly sorted",
                [
                    ("action_id", candidate.action_id.clone()),
                    ("outcome_id", outcome.outcome_id.clone()),
                ],
            ));
        }
        previous = Some(&outcome.outcome_id);
        worst = worst.max(outcome.residual_model_count);
    }
    if !candidate.separation.is_empty() && candidate.worst_case_remaining != worst {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence worst_case_remaining must equal the largest outcome residual count",
            [
                ("action_id", candidate.action_id.clone()),
                (
                    "declared_worst_case_remaining",
                    candidate.worst_case_remaining.to_string(),
                ),
                ("computed_worst_case_remaining", worst.to_string()),
            ],
        ));
    }
    Ok(())
}

fn bound_observation_id(candidate: &GeoNextAction) -> Option<&str> {
    candidate
        .observation_id
        .as_deref()
        .or(match &candidate.kind {
            GeoNextActionKind::Observe(observation_id) => Some(observation_id.as_str()),
            _ => Some(candidate.action_id.as_str()),
        })
}

fn classify_frontier(
    composition: &GeoCompositionArtifact,
    separation: &GeoSeparationArtifact,
    candidates: &mut [GeoNextAction],
) -> (
    Vec<GeoNextAction>,
    Vec<GeoNextAction>,
    BTreeMap<String, GeoDominanceBasis>,
) {
    let active_class = active_action_class(composition, separation, candidates);
    let mut active = Vec::new();
    let mut dominated = Vec::new();
    for candidate in candidates.iter().cloned() {
        if candidate.class == GeoNextActionClass::Stop {
            continue;
        }
        if candidate.redundant {
            dominated.push(candidate);
            continue;
        }
        if active_class.is_some_and(|class| candidate.class != class) {
            continue;
        }
        active.push(candidate);
    }

    let mut dominance_basis = BTreeMap::new();
    for right in 0..active.len() {
        for left in 0..active.len() {
            if left == right {
                continue;
            }
            let key = dominance_pair_key(&active[left].action_id, &active[right].action_id);
            if !all_counts_exact(&active[left]) || !all_counts_exact(&active[right]) {
                dominance_basis.insert(key, GeoDominanceBasis::Bounds);
                continue;
            }
            dominance_basis.insert(key, GeoDominanceBasis::Exact);
            if dominates_exact(&active[left], &active[right]) {
                let dominator = active[left].action_id.clone();
                if !active[right].dominated_by.iter().any(|id| id == &dominator) {
                    active[right].dominated_by.push(dominator);
                }
            }
        }
    }

    for candidate in &mut active {
        candidate.dominated_by.sort();
        candidate.dominated_by.dedup();
    }

    let mut frontier = Vec::new();
    for mut candidate in active {
        if candidate.class == GeoNextActionClass::Stop
            || candidate.redundant
            || active_class.is_some_and(|class| candidate.class != class)
        {
            continue;
        }
        candidate.dominated_by = dominated_by_for(&candidate, candidates, &dominance_basis);
        if candidate.dominated_by.is_empty() {
            frontier.push(candidate);
        } else {
            dominated.push(candidate);
        }
    }
    frontier.sort_by(|left, right| left.action_id.cmp(&right.action_id));
    dominated.sort_by(|left, right| left.action_id.cmp(&right.action_id));
    dominated.dedup_by(|left, right| left.action_id == right.action_id);
    (frontier, dominated, dominance_basis)
}

fn active_action_class(
    composition: &GeoCompositionArtifact,
    separation: &GeoSeparationArtifact,
    candidates: &[GeoNextAction],
) -> Option<GeoNextActionClass> {
    let has_class = |class| {
        candidates
            .iter()
            .any(|candidate| !candidate.redundant && candidate.class == class)
    };
    if has_class(GeoNextActionClass::RepairReach) {
        return Some(GeoNextActionClass::RepairReach);
    }
    if composition.status == GeoCompositionStatus::Conflict
        && separation.baseline_model_count == 0
        && has_class(GeoNextActionClass::DiagnoseConflict)
    {
        return Some(GeoNextActionClass::DiagnoseConflict);
    }
    if separation.baseline_model_count > 0 && has_class(GeoNextActionClass::SeparateResidual) {
        return Some(GeoNextActionClass::SeparateResidual);
    }
    if has_class(GeoNextActionClass::DiagnoseConflict) {
        return Some(GeoNextActionClass::DiagnoseConflict);
    }
    if has_class(GeoNextActionClass::RaiseClaimClass) {
        return Some(GeoNextActionClass::RaiseClaimClass);
    }
    None
}

fn dominates_exact(left: &GeoNextAction, right: &GeoNextAction) -> bool {
    if left.cost_units > right.cost_units {
        return false;
    }
    if left.worst_case_remaining > right.worst_case_remaining {
        return false;
    }
    let left_outcomes = outcome_count_vector(left);
    let right_outcomes = outcome_count_vector(right);
    if left_outcomes.len() != right_outcomes.len() {
        return false;
    }
    for (left_count, right_count) in left_outcomes.iter().zip(right_outcomes.iter()) {
        if left_count > right_count {
            return false;
        }
    }
    left.cost_units < right.cost_units || left.worst_case_remaining < right.worst_case_remaining
}

fn outcome_count_vector(candidate: &GeoNextAction) -> Vec<u64> {
    let mut counts = candidate
        .separation
        .iter()
        .map(|outcome| outcome.residual_model_count)
        .collect::<Vec<_>>();
    counts.sort_by(|left, right| right.cmp(left));
    counts
}

fn dominated_by_for(
    candidate: &GeoNextAction,
    candidates: &[GeoNextAction],
    dominance_basis: &BTreeMap<String, GeoDominanceBasis>,
) -> Vec<String> {
    let mut dominated_by = candidate.dominated_by.clone();
    for left in candidates {
        if left.action_id == candidate.action_id
            || left.redundant
            || left.class != candidate.class
            || !matches!(
                dominance_basis.get(&dominance_pair_key(&left.action_id, &candidate.action_id)),
                Some(GeoDominanceBasis::Exact)
            )
        {
            continue;
        }
        if dominates_exact(left, candidate) {
            dominated_by.push(left.action_id.clone());
        }
    }
    dominated_by.sort();
    dominated_by.dedup();
    dominated_by
}

fn all_counts_exact(candidate: &GeoNextAction) -> bool {
    !candidate.separation.is_empty()
        && candidate
            .separation
            .iter()
            .all(|outcome| outcome.count_exact)
}

fn ranked_action_ids(frontier: &[GeoNextAction], dominated: &[GeoNextAction]) -> Vec<String> {
    let mut actions = frontier
        .iter()
        .chain(dominated.iter())
        .map(|candidate| {
            (
                action_class_rank(candidate.class),
                candidate.cost_units,
                candidate.worst_case_remaining,
                candidate.action_id.clone(),
            )
        })
        .collect::<Vec<_>>();
    actions.sort();
    actions
        .into_iter()
        .map(|(_, _, _, action_id)| action_id)
        .collect()
}

fn action_class_rank(class: GeoNextActionClass) -> u8 {
    match class {
        GeoNextActionClass::RepairReach => 0,
        GeoNextActionClass::DiagnoseConflict => 1,
        GeoNextActionClass::SeparateResidual => 2,
        GeoNextActionClass::RaiseClaimClass => 3,
        GeoNextActionClass::Stop => 4,
    }
}

fn no_loss_model_abstention(
    policy: Option<&GeoNextEvidencePolicy>,
) -> GeoNextEvidenceRankingAbstention {
    let mut detail = BTreeMap::new();
    detail.insert(
        "policy".to_string(),
        policy
            .map(|policy| policy.policy.policy_id.clone())
            .unwrap_or_else(|| "none".to_string()),
    );
    GeoNextEvidenceRankingAbstention {
        code: GeoNextEvidenceErrorCode::NextEvidenceNoLossModel,
        message: "Total ranking requires a versioned loss model".to_string(),
        detail,
    }
}

fn claim_forced(composition: &GeoCompositionArtifact) -> bool {
    composition.status == GeoCompositionStatus::Resolved
        && composition.backbone_complete
        && (!composition.hard_forced.parcels.is_empty()
            || !composition.hard_forced.buildings.is_empty())
}

fn explicit_stop_reason(candidate: &GeoNextAction) -> Option<GeoStopReason> {
    if candidate.class != GeoNextActionClass::Stop {
        return None;
    }
    Some(
        candidate
            .stop_reason
            .unwrap_or(GeoStopReason::HonestAmbiguity),
    )
}

fn next_evidence_run_id(
    composition_blake3: &str,
    separation_blake3: &str,
    candidates: &[GeoNextAction],
    policy: Option<&GeoNextEvidencePolicy>,
    budget: &GeoResourceBudget,
    spent: &BTreeMap<String, u64>,
) -> Result<String, GeoNextEvidenceError> {
    #[derive(Serialize)]
    struct Seed<'a> {
        composition_blake3: &'a str,
        separation_blake3: &'a str,
        candidates: &'a [GeoNextAction],
        policy: Option<&'a GeoNextEvidencePolicy>,
        budget: &'a GeoResourceBudget,
        spent: &'a BTreeMap<String, u64>,
    }

    let bytes = serde_json::to_vec(&Seed {
        composition_blake3,
        separation_blake3,
        candidates,
        policy,
        budget,
        spent,
    })
    .map_err(|error| {
        GeoNextEvidenceError::invalid(
            "Geo next-evidence run id seed could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!(
        "{CANON_GEO_NEXT_EVIDENCE_VERSION}:{}",
        blake3::hash(&bytes).to_hex()
    ))
}

fn validate_candidate_order(candidates: &[GeoNextAction]) -> Result<(), GeoNextEvidenceError> {
    let mut previous: Option<&str> = None;
    for candidate in candidates {
        if previous.is_some_and(|prior| prior >= candidate.action_id.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence candidates must be strictly sorted by action_id",
                [("action_id", candidate.action_id.clone())],
            ));
        }
        previous = Some(&candidate.action_id);
    }
    Ok(())
}

fn reject_overlap(
    frontier: &[GeoNextAction],
    dominated: &[GeoNextAction],
) -> Result<(), GeoNextEvidenceError> {
    let frontier_ids = frontier
        .iter()
        .map(|candidate| candidate.action_id.as_str())
        .collect::<BTreeSet<_>>();
    for candidate in dominated {
        if frontier_ids.contains(candidate.action_id.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence action cannot be both frontier and dominated",
                [("action_id", candidate.action_id.clone())],
            ));
        }
    }
    Ok(())
}

fn reject_duplicate_strings(field: &str, values: &[String]) -> Result<(), GeoNextEvidenceError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.as_str()) {
            return Err(GeoNextEvidenceError::invalid(
                "Geo next-evidence string list contains duplicates",
                [("field", field.to_string()), ("value", value.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_prefixed_blake3(field: &str, value: &str) -> Result<(), GeoNextEvidenceError> {
    let digest = value.strip_prefix("blake3:").ok_or_else(|| {
        GeoNextEvidenceError::invalid(
            "Geo next-evidence digest must use the blake3: prefix",
            [("field", field.to_string()), ("value", value.to_string())],
        )
    })?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence digest must contain 64 lowercase hex characters",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    if !digest
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(GeoNextEvidenceError::invalid(
            "Geo next-evidence digest must use lowercase hex",
            [("field", field.to_string()), ("value", value.to_string())],
        ));
    }
    Ok(())
}

fn prefixed_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn dominance_pair_key(left: &str, right: &str) -> String {
    format!("{left}>{right}")
}

fn forbidden_information_key(value: &Value) -> Option<String> {
    const FORBIDDEN: [&str; 5] = ["expect", "probab", "voi", "likelihood", "gain"];
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let lower = key.to_ascii_lowercase();
                if FORBIDDEN.iter().any(|needle| lower.contains(needle)) {
                    return Some(key.clone());
                }
                if let Some(found) = forbidden_information_key(nested) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(values) => values.iter().find_map(forbidden_information_key),
        _ => None,
    }
}
