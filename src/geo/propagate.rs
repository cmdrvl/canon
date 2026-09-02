#![forbid(unsafe_code)]

//! Sound domain propagators for bounded Geo composition requests.
//!
//! Propagation is a pre-search filter over the existing extensional solver
//! contract. It emits only values entailed by the current hard constraints and
//! represents every pruning as another ordinary `Require` or `Forbid` when the
//! narrowed request is handed back to the exact backend.

use super::{
    GeoCompositionError, GeoCompositionModel, GeoCompositionRequest, GeoEntityLevel, GeoEntityRef,
    GeoEvidenceCompilationArtifact, GeoEvidenceDisposition, GeoHardConstraint,
    GeoHardConstraintKind, canonicalize_composition_request, solve_composition,
    validate_evidence_compilation_artifact,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GEO_PROPAGATION_VERSION: &str = "canon_geo_propagation.v0";

const DEFAULT_MAX_FIXPOINT_ROUNDS: u64 = 64;
const DEFAULT_MAX_HALL_SUBSET_SIZE: usize = 8;
const DEFAULT_MAX_SUBSET_SUM_STATES: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropagatorKind {
    AdditiveBand,
    Cardinality,
    SourceExclusivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropagationBudget {
    pub max_fixpoint_rounds: u64,
    pub max_hall_subset_size: usize,
    pub max_subset_sum_states: u64,
}

impl Default for GeoPropagationBudget {
    fn default() -> Self {
        Self {
            max_fixpoint_rounds: DEFAULT_MAX_FIXPOINT_ROUNDS,
            max_hall_subset_size: DEFAULT_MAX_HALL_SUBSET_SIZE,
            max_subset_sum_states: DEFAULT_MAX_SUBSET_SUM_STATES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPrunedValue {
    Excluded,
    Forced,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPruning {
    pub member: GeoEntityRef,
    pub value: GeoPrunedValue,
    pub propagator: GeoPropagatorKind,
    pub constraint_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropagationFallback {
    pub propagator: GeoPropagatorKind,
    pub counter: String,
    pub configured: u64,
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropagationArtifact {
    pub version: String,
    pub request_blake3: String,
    pub prunings: Vec<GeoPruning>,
    pub rounds: u64,
    pub fixpoint_reached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_fallback: Option<GeoPropagationFallback>,
    pub counters: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoSoundnessReport {
    pub sound: bool,
    pub model_count_before: u64,
    pub model_count_after: u64,
    pub differing_models: Vec<GeoCompositionModel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoPropagationErrorCode {
    UnsupportedVersion,
    InvalidInput,
    BudgetExceeded,
    ArithmeticOverflow,
    PropagationUnsoundDetected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoPropagationError {
    pub code: GeoPropagationErrorCode,
    pub message: String,
    pub detail: BTreeMap<String, String>,
}

impl GeoPropagationError {
    fn new(
        code: GeoPropagationErrorCode,
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
        Self::new(GeoPropagationErrorCode::InvalidInput, message, detail)
    }

    fn overflow(context: &str) -> Self {
        Self::new(
            GeoPropagationErrorCode::ArithmeticOverflow,
            "Geo propagation arithmetic overflowed",
            [("context", context)],
        )
    }
}

impl fmt::Display for GeoPropagationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.message, self.code)
    }
}

impl Error for GeoPropagationError {}

impl From<GeoCompositionError> for GeoPropagationError {
    fn from(error: GeoCompositionError) -> Self {
        let mut detail = error.detail;
        detail.insert("composition_code".to_string(), format!("{:?}", error.code));
        Self {
            code: GeoPropagationErrorCode::InvalidInput,
            message: error.message,
            detail,
        }
    }
}

pub fn propagate(
    request: &GeoCompositionRequest,
    evidence: Option<&GeoEvidenceCompilationArtifact>,
    budget: &GeoPropagationBudget,
) -> Result<GeoPropagationArtifact, GeoPropagationError> {
    propagate_with_order(request, evidence, budget, &default_propagator_order())
}

pub fn propagate_with_order(
    request: &GeoCompositionRequest,
    evidence: Option<&GeoEvidenceCompilationArtifact>,
    budget: &GeoPropagationBudget,
    order: &[GeoPropagatorKind],
) -> Result<GeoPropagationArtifact, GeoPropagationError> {
    validate_budget(budget)?;
    validate_order(order)?;
    let request = canonicalize_composition_request(request)?;
    validate_evidence_matches_request(&request, evidence)?;
    let evidence_index = ConstraintEvidenceIndex::from_evidence(evidence);
    let mut domain = Domain::from_request(&request, &evidence_index)?;
    let mut counters = base_counters(&request, &domain, order);
    let mut all_prunings = Vec::new();
    let mut rounds = 0_u64;
    let mut fallback = None;

    for _ in 0..budget.max_fixpoint_rounds {
        rounds = rounds
            .checked_add(1)
            .ok_or_else(|| GeoPropagationError::overflow("fixpoint rounds"))?;
        let previous_count = all_prunings.len();
        for kind in order {
            let outcome = match kind {
                GeoPropagatorKind::AdditiveBand => {
                    additive_band_propagator(&request, &mut domain, &evidence_index, budget)?
                }
                GeoPropagatorKind::Cardinality => {
                    cardinality_propagator(&request, &mut domain, &evidence_index)?
                }
                GeoPropagatorKind::SourceExclusivity => {
                    source_exclusivity_propagator(&request, &mut domain, &evidence_index, budget)?
                }
            };
            if let Some(limit) = outcome.fallback {
                fallback = Some(limit);
                break;
            }
            all_prunings.extend(outcome.prunings);
        }
        if fallback.is_some() {
            break;
        }
        if all_prunings.len() == previous_count {
            all_prunings.sort_by(pruning_sort_key);
            counters.insert("pruning_count".to_string(), all_prunings.len() as u64);
            counters.insert("rounds".to_string(), rounds);
            let artifact = GeoPropagationArtifact {
                version: CANON_GEO_PROPAGATION_VERSION.to_string(),
                request_blake3: propagation_request_blake3(&request)?,
                prunings: all_prunings,
                rounds,
                fixpoint_reached: true,
                budget_fallback: None,
                counters,
            };
            validate_propagation_artifact(&artifact)?;
            return Ok(artifact);
        }
    }

    let fallback = fallback.unwrap_or_else(|| GeoPropagationFallback {
        propagator: GeoPropagatorKind::AdditiveBand,
        counter: "max_fixpoint_rounds".to_string(),
        configured: budget.max_fixpoint_rounds,
        guidance: "raise max_fixpoint_rounds or accept retained individually justified prunings before exact solving".to_string(),
    });
    all_prunings.sort_by(pruning_sort_key);
    counters.insert("pruning_count".to_string(), all_prunings.len() as u64);
    counters.insert("rounds".to_string(), rounds);
    counters.insert(format!("fallback.{}", fallback.counter), 1);
    let artifact = GeoPropagationArtifact {
        version: CANON_GEO_PROPAGATION_VERSION.to_string(),
        request_blake3: propagation_request_blake3(&request)?,
        prunings: all_prunings,
        rounds,
        fixpoint_reached: false,
        budget_fallback: Some(fallback),
        counters,
    };
    validate_propagation_artifact(&artifact)?;
    Ok(artifact)
}

pub fn apply_prunings(
    request: &GeoCompositionRequest,
    artifact: &GeoPropagationArtifact,
) -> Result<GeoCompositionRequest, GeoPropagationError> {
    validate_propagation_artifact(artifact)?;
    let canonical = canonicalize_composition_request(request)?;
    let request_blake3 = propagation_request_blake3(&canonical)?;
    if artifact.request_blake3 != request_blake3 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation artifact was produced for a different composition request",
            [
                ("field", "request_blake3".to_string()),
                ("expected", request_blake3),
                ("actual", artifact.request_blake3.clone()),
            ],
        ));
    }
    let mut narrowed = canonical;
    for pruning in &artifact.prunings {
        narrowed.hard_constraints.push(GeoHardConstraint {
            id: pruning_constraint_id(pruning),
            constraint: match pruning.value {
                GeoPrunedValue::Excluded => GeoHardConstraintKind::Forbid {
                    member: pruning.member.clone(),
                },
                GeoPrunedValue::Forced => GeoHardConstraintKind::Require {
                    member: pruning.member.clone(),
                },
            },
        });
    }
    Ok(canonicalize_composition_request(&narrowed)?)
}

pub fn check_soundness(
    request: &GeoCompositionRequest,
    artifact: &GeoPropagationArtifact,
) -> Result<GeoSoundnessReport, GeoPropagationError> {
    let before = solve_composition(request)?;
    let narrowed = apply_prunings(request, artifact)?;
    let after = solve_composition(&narrowed)?;
    let before_models = before
        .residual_models
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let after_models = after
        .residual_models
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let differing_models = before_models
        .symmetric_difference(&after_models)
        .cloned()
        .collect::<Vec<_>>();
    let counts_equal = before.summary.residual_model_count == after.summary.residual_model_count
        && before.summary.residual_model_count_complete
            == after.summary.residual_model_count_complete
        && before.summary.residual_model_count_saturated
            == after.summary.residual_model_count_saturated;
    if counts_equal && differing_models.is_empty() {
        return Ok(GeoSoundnessReport {
            sound: true,
            model_count_before: before.summary.residual_model_count,
            model_count_after: after.summary.residual_model_count,
            differing_models,
        });
    }
    Err(GeoPropagationError::new(
        GeoPropagationErrorCode::PropagationUnsoundDetected,
        "Geo propagation changed the exact residual model set",
        [
            (
                "model_count_before".to_string(),
                before.summary.residual_model_count.to_string(),
            ),
            (
                "model_count_after".to_string(),
                after.summary.residual_model_count.to_string(),
            ),
            (
                "member".to_string(),
                first_differing_member(&differing_models)
                    .unwrap_or("residual_model_count")
                    .to_string(),
            ),
        ],
    ))
}

pub fn validate_propagation_artifact(
    artifact: &GeoPropagationArtifact,
) -> Result<(), GeoPropagationError> {
    if artifact.version != CANON_GEO_PROPAGATION_VERSION {
        return Err(GeoPropagationError::new(
            GeoPropagationErrorCode::UnsupportedVersion,
            "Unsupported Geo propagation artifact version",
            [
                ("actual", artifact.version.as_str()),
                ("expected", CANON_GEO_PROPAGATION_VERSION),
            ],
        ));
    }
    validate_blake3_ref("request_blake3", &artifact.request_blake3)?;
    if artifact.rounds == 0 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation artifact must record at least one fixpoint round",
            [("field", "rounds")],
        ));
    }
    if artifact.fixpoint_reached && artifact.budget_fallback.is_some() {
        return Err(GeoPropagationError::invalid(
            "Geo propagation fixpoint artifacts cannot carry a budget fallback",
            [("field", "budget_fallback")],
        ));
    }
    if !artifact.fixpoint_reached && artifact.budget_fallback.is_none() {
        return Err(GeoPropagationError::invalid(
            "Geo propagation non-fixpoint artifacts require a budget fallback",
            [("field", "budget_fallback")],
        ));
    }
    if let Some(fallback) = &artifact.budget_fallback {
        validate_fallback(fallback)?;
    }
    validate_prunings(&artifact.prunings)?;
    validate_counters(&artifact.counters)
}

pub fn canonical_propagation_bytes(
    artifact: &GeoPropagationArtifact,
) -> Result<Vec<u8>, GeoPropagationError> {
    validate_propagation_artifact(artifact)?;
    serde_json::to_vec(artifact).map_err(|error| {
        GeoPropagationError::invalid(
            "Geo propagation artifact could not be serialized",
            [("serde_error", error.to_string())],
        )
    })
}

pub fn propagation_request_blake3(
    request: &GeoCompositionRequest,
) -> Result<String, GeoPropagationError> {
    let canonical = canonicalize_composition_request(request)?;
    let request_bytes = serde_json::to_vec(&canonical).map_err(|error| {
        GeoPropagationError::invalid(
            "Geo propagation request could not be serialized",
            [("serde_error", error.to_string())],
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&request_bytes).to_hex()))
}

fn default_propagator_order() -> [GeoPropagatorKind; 3] {
    [
        GeoPropagatorKind::AdditiveBand,
        GeoPropagatorKind::Cardinality,
        GeoPropagatorKind::SourceExclusivity,
    ]
}

fn validate_budget(budget: &GeoPropagationBudget) -> Result<(), GeoPropagationError> {
    if budget.max_fixpoint_rounds == 0 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation max_fixpoint_rounds must be positive",
            [("field", "max_fixpoint_rounds")],
        ));
    }
    if budget.max_hall_subset_size == 0 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation max_hall_subset_size must be positive",
            [("field", "max_hall_subset_size")],
        ));
    }
    if budget.max_subset_sum_states == 0 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation max_subset_sum_states must be positive",
            [("field", "max_subset_sum_states")],
        ));
    }
    Ok(())
}

fn validate_order(order: &[GeoPropagatorKind]) -> Result<(), GeoPropagationError> {
    let expected = default_propagator_order()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual = order.iter().copied().collect::<BTreeSet<_>>();
    if order.len() != expected.len() || actual != expected {
        return Err(GeoPropagationError::invalid(
            "Geo propagation order must contain each propagator exactly once",
            [("field", "order")],
        ));
    }
    Ok(())
}

fn validate_evidence_matches_request(
    request: &GeoCompositionRequest,
    evidence: Option<&GeoEvidenceCompilationArtifact>,
) -> Result<(), GeoPropagationError> {
    let Some(evidence) = evidence else {
        return Ok(());
    };
    validate_evidence_compilation_artifact(evidence).map_err(|error| {
        GeoPropagationError::invalid(
            "Geo propagation evidence compilation artifact is invalid",
            [
                ("evidence_code".to_string(), format!("{:?}", error.code)),
                ("evidence_message".to_string(), error.message),
            ],
        )
    })?;
    let evidence_request = canonicalize_composition_request(&evidence.composition_request)?;
    if &evidence_request != request {
        return Err(GeoPropagationError::invalid(
            "Geo propagation evidence compilation request does not match the input request",
            [("field", "evidence.composition_request")],
        ));
    }
    Ok(())
}

fn base_counters(
    request: &GeoCompositionRequest,
    domain: &Domain,
    order: &[GeoPropagatorKind],
) -> BTreeMap<String, u64> {
    let mut counters = BTreeMap::new();
    counters.insert(
        "hard_constraint_count".to_string(),
        request.hard_constraints.len() as u64,
    );
    counters.insert("member_count".to_string(), domain.members.len() as u64);
    counters.insert("propagator_count".to_string(), order.len() as u64);
    counters.insert(
        "additive_band_constraint_count".to_string(),
        request
            .hard_constraints
            .iter()
            .filter(|constraint| {
                matches!(
                    constraint.constraint,
                    GeoHardConstraintKind::IntegerSumBand { .. }
                )
            })
            .count() as u64,
    );
    counters.insert(
        "cardinality_constraint_count".to_string(),
        request
            .hard_constraints
            .iter()
            .filter(|constraint| {
                matches!(
                    constraint.constraint,
                    GeoHardConstraintKind::Cardinality { .. }
                )
            })
            .count() as u64,
    );
    counters.insert(
        "source_exclusivity_constraint_count".to_string(),
        request
            .hard_constraints
            .iter()
            .filter(|constraint| {
                matches!(
                    constraint.constraint,
                    GeoHardConstraintKind::AllowedSets { .. }
                )
            })
            .count() as u64,
    );
    counters
}

#[derive(Default)]
struct PropagatorOutcome {
    prunings: Vec<GeoPruning>,
    fallback: Option<GeoPropagationFallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DomainValue {
    Unknown,
    Excluded,
    Forced,
    Contradictory,
}

#[derive(Debug, Clone)]
struct DomainMember {
    value: DomainValue,
    constraint_ids: BTreeSet<String>,
    evidence_ids: BTreeSet<String>,
}

impl Default for DomainMember {
    fn default() -> Self {
        Self {
            value: DomainValue::Unknown,
            constraint_ids: BTreeSet::new(),
            evidence_ids: BTreeSet::new(),
        }
    }
}

struct Domain {
    members: BTreeMap<GeoEntityRef, DomainMember>,
}

impl Domain {
    fn from_request(
        request: &GeoCompositionRequest,
        evidence_index: &ConstraintEvidenceIndex,
    ) -> Result<Self, GeoPropagationError> {
        let mut members = BTreeMap::new();
        for parcel in &request.universe.parcels {
            members.insert(
                GeoEntityRef::new(GeoEntityLevel::Parcel, parcel.clone()),
                DomainMember::default(),
            );
        }
        for building in &request.universe.buildings {
            members.insert(
                GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()),
                DomainMember::default(),
            );
        }
        let mut domain = Self { members };
        for constraint in &request.hard_constraints {
            match &constraint.constraint {
                GeoHardConstraintKind::Require { member } => {
                    let reason = evidence_index.reason_for_constraint(&constraint.id);
                    domain.seed(member, GeoPrunedValue::Forced, reason)?;
                }
                GeoHardConstraintKind::Forbid { member } => {
                    let reason = evidence_index.reason_for_constraint(&constraint.id);
                    domain.seed(member, GeoPrunedValue::Excluded, reason)?;
                }
                _ => {}
            }
        }
        Ok(domain)
    }

    fn seed(
        &mut self,
        member: &GeoEntityRef,
        value: GeoPrunedValue,
        reason: ReasonAccumulator,
    ) -> Result<(), GeoPropagationError> {
        let Some(slot) = self.members.get_mut(member) else {
            return Err(GeoPropagationError::invalid(
                "Geo propagation seed references an unknown member",
                [
                    ("level", level_name(member.level)),
                    ("member", member.id.as_str()),
                ],
            ));
        };
        apply_domain_value(slot, value, reason);
        Ok(())
    }

    fn value(&self, member: &GeoEntityRef) -> DomainValue {
        self.members
            .get(member)
            .map(|slot| slot.value)
            .unwrap_or(DomainValue::Unknown)
    }

    fn is_forced(&self, member: &GeoEntityRef) -> bool {
        self.value(member) == DomainValue::Forced
    }

    fn is_excluded(&self, member: &GeoEntityRef) -> bool {
        self.value(member) == DomainValue::Excluded
    }

    fn reason_for_member(&self, member: &GeoEntityRef) -> ReasonAccumulator {
        self.members
            .get(member)
            .map(|slot| ReasonAccumulator {
                constraint_ids: slot.constraint_ids.clone(),
                evidence_ids: slot.evidence_ids.clone(),
            })
            .unwrap_or_default()
    }

    fn add_pruning(
        &mut self,
        member: GeoEntityRef,
        value: GeoPrunedValue,
        propagator: GeoPropagatorKind,
        reason: ReasonAccumulator,
    ) -> Option<GeoPruning> {
        let slot = self.members.get_mut(&member)?;
        match (slot.value, value) {
            (DomainValue::Contradictory, _) => None,
            (DomainValue::Forced, GeoPrunedValue::Forced)
            | (DomainValue::Excluded, GeoPrunedValue::Excluded) => None,
            (DomainValue::Forced, GeoPrunedValue::Excluded)
            | (DomainValue::Excluded, GeoPrunedValue::Forced) => {
                slot.value = DomainValue::Contradictory;
                slot.constraint_ids.extend(reason.constraint_ids);
                slot.evidence_ids.extend(reason.evidence_ids);
                None
            }
            (DomainValue::Unknown, _) => {
                apply_domain_value(slot, value, reason.clone());
                Some(GeoPruning {
                    member,
                    value,
                    propagator,
                    constraint_ids: reason.constraint_ids.into_iter().collect(),
                    evidence_ids: reason.evidence_ids.into_iter().collect(),
                })
            }
        }
    }
}

fn apply_domain_value(slot: &mut DomainMember, value: GeoPrunedValue, reason: ReasonAccumulator) {
    let incoming = match value {
        GeoPrunedValue::Excluded => DomainValue::Excluded,
        GeoPrunedValue::Forced => DomainValue::Forced,
    };
    slot.value = match (slot.value, incoming) {
        (DomainValue::Unknown, incoming) => incoming,
        (current, incoming) if current == incoming => current,
        _ => DomainValue::Contradictory,
    };
    slot.constraint_ids.extend(reason.constraint_ids);
    slot.evidence_ids.extend(reason.evidence_ids);
}

#[derive(Debug, Clone, Default)]
struct ReasonAccumulator {
    constraint_ids: BTreeSet<String>,
    evidence_ids: BTreeSet<String>,
}

impl ReasonAccumulator {
    fn with_constraint(
        constraint_id: &str,
        evidence_index: &ConstraintEvidenceIndex,
    ) -> ReasonAccumulator {
        let mut reason = Self::default();
        reason.add_constraint(constraint_id, evidence_index);
        reason
    }

    fn add_constraint(&mut self, constraint_id: &str, evidence_index: &ConstraintEvidenceIndex) {
        self.constraint_ids.insert(constraint_id.to_string());
        if let Some(evidence_ids) = evidence_index.evidence_ids.get(constraint_id) {
            self.evidence_ids.extend(evidence_ids.iter().cloned());
        }
    }

    fn merge(&mut self, other: ReasonAccumulator) {
        self.constraint_ids.extend(other.constraint_ids);
        self.evidence_ids.extend(other.evidence_ids);
    }
}

struct ConstraintEvidenceIndex {
    evidence_ids: BTreeMap<String, BTreeSet<String>>,
}

impl ConstraintEvidenceIndex {
    fn from_evidence(evidence: Option<&GeoEvidenceCompilationArtifact>) -> Self {
        let mut evidence_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if let Some(evidence) = evidence {
            for admission in &evidence.admissions {
                if admission.disposition != GeoEvidenceDisposition::HardConstraint {
                    continue;
                }
                for generated_id in &admission.generated_ids {
                    evidence_ids
                        .entry(generated_id.clone())
                        .or_default()
                        .insert(admission.observation_id.clone());
                }
            }
        }
        Self { evidence_ids }
    }

    fn reason_for_constraint(&self, constraint_id: &str) -> ReasonAccumulator {
        ReasonAccumulator::with_constraint(constraint_id, self)
    }
}

fn additive_band_propagator(
    request: &GeoCompositionRequest,
    domain: &mut Domain,
    evidence_index: &ConstraintEvidenceIndex,
    budget: &GeoPropagationBudget,
) -> Result<PropagatorOutcome, GeoPropagationError> {
    let mut outcome = PropagatorOutcome::default();
    for constraint in &request.hard_constraints {
        let GeoHardConstraintKind::IntegerSumBand {
            level,
            values,
            min,
            max,
            ..
        } = &constraint.constraint
        else {
            continue;
        };
        if values.len() as u64 > budget.max_subset_sum_states {
            outcome.fallback = Some(GeoPropagationFallback {
                propagator: GeoPropagatorKind::AdditiveBand,
                counter: "max_subset_sum_states".to_string(),
                configured: budget.max_subset_sum_states,
                guidance:
                    "raise max_subset_sum_states to allow additive-band domain feasibility checks"
                        .to_string(),
            });
            return Ok(outcome);
        }
        let mut forced_sum = 0_u128;
        let mut free_sum = 0_u128;
        let mut free_values = Vec::new();
        let mut reason_all = evidence_index.reason_for_constraint(&constraint.id);
        for value in values {
            let member = GeoEntityRef::new(*level, value.id.clone());
            match domain.value(&member) {
                DomainValue::Forced => {
                    forced_sum = forced_sum
                        .checked_add(u128::from(value.value))
                        .ok_or_else(|| GeoPropagationError::overflow("additive forced sum"))?;
                    reason_all.merge(domain.reason_for_member(&member));
                }
                DomainValue::Excluded => {
                    reason_all.merge(domain.reason_for_member(&member));
                }
                DomainValue::Unknown => {
                    free_sum = free_sum
                        .checked_add(u128::from(value.value))
                        .ok_or_else(|| GeoPropagationError::overflow("additive free sum"))?;
                    free_values.push((member, u128::from(value.value)));
                }
                DomainValue::Contradictory => return Ok(outcome),
            }
        }
        let min = u128::from(*min);
        let max = u128::from(*max);
        if forced_sum > max || forced_sum + free_sum < min {
            continue;
        }
        for (member, value) in free_values {
            if forced_sum + value > max {
                let mut reason = evidence_index.reason_for_constraint(&constraint.id);
                for value_ref in values {
                    let other = GeoEntityRef::new(*level, value_ref.id.clone());
                    if domain.is_forced(&other) {
                        reason.merge(domain.reason_for_member(&other));
                    }
                }
                if let Some(pruning) = domain.add_pruning(
                    member,
                    GeoPrunedValue::Excluded,
                    GeoPropagatorKind::AdditiveBand,
                    reason,
                ) {
                    outcome.prunings.push(pruning);
                }
            } else if forced_sum + free_sum - value < min {
                let mut reason = reason_all.clone();
                for value_ref in values {
                    let other = GeoEntityRef::new(*level, value_ref.id.clone());
                    if domain.is_excluded(&other) {
                        reason.merge(domain.reason_for_member(&other));
                    }
                }
                if let Some(pruning) = domain.add_pruning(
                    member,
                    GeoPrunedValue::Forced,
                    GeoPropagatorKind::AdditiveBand,
                    reason,
                ) {
                    outcome.prunings.push(pruning);
                }
            }
        }
    }
    Ok(outcome)
}

fn cardinality_propagator(
    request: &GeoCompositionRequest,
    domain: &mut Domain,
    evidence_index: &ConstraintEvidenceIndex,
) -> Result<PropagatorOutcome, GeoPropagationError> {
    let mut outcome = PropagatorOutcome::default();
    for constraint in &request.hard_constraints {
        let GeoHardConstraintKind::Cardinality { level, min, max } = &constraint.constraint else {
            continue;
        };
        if *level == GeoEntityLevel::Building {
            for building in &request.universe.buildings {
                if building.parcel_ids.is_empty() {
                    continue;
                }
                let member = GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone());
                if domain.value(&member) != DomainValue::Unknown {
                    continue;
                }
                if building.parcel_ids.iter().all(|parcel_id| {
                    domain.is_excluded(&GeoEntityRef::new(
                        GeoEntityLevel::Parcel,
                        parcel_id.clone(),
                    ))
                }) {
                    let mut reason = evidence_index.reason_for_constraint(&constraint.id);
                    for parcel_id in &building.parcel_ids {
                        reason.merge(domain.reason_for_member(&GeoEntityRef::new(
                            GeoEntityLevel::Parcel,
                            parcel_id.clone(),
                        )));
                    }
                    if let Some(pruning) = domain.add_pruning(
                        member,
                        GeoPrunedValue::Excluded,
                        GeoPropagatorKind::Cardinality,
                        reason,
                    ) {
                        outcome.prunings.push(pruning);
                    }
                }
            }
        }

        let members = members_at_level(request, *level)?;
        let forced = members
            .iter()
            .filter(|member| domain.is_forced(member))
            .collect::<Vec<_>>();
        let free = members
            .iter()
            .filter(|member| domain.value(member) == DomainValue::Unknown)
            .cloned()
            .collect::<Vec<_>>();
        if forced.len() > *max || forced.len() + free.len() < *min {
            continue;
        }
        if forced.len() == *max {
            let mut reason = evidence_index.reason_for_constraint(&constraint.id);
            for member in forced {
                reason.merge(domain.reason_for_member(member));
            }
            for member in free {
                if let Some(pruning) = domain.add_pruning(
                    member,
                    GeoPrunedValue::Excluded,
                    GeoPropagatorKind::Cardinality,
                    reason.clone(),
                ) {
                    outcome.prunings.push(pruning);
                }
            }
        } else if forced.len() + free.len() == *min {
            let mut reason = evidence_index.reason_for_constraint(&constraint.id);
            for member in members {
                if domain.is_excluded(&member) {
                    reason.merge(domain.reason_for_member(&member));
                }
            }
            for member in free {
                if let Some(pruning) = domain.add_pruning(
                    member,
                    GeoPrunedValue::Forced,
                    GeoPropagatorKind::Cardinality,
                    reason.clone(),
                ) {
                    outcome.prunings.push(pruning);
                }
            }
        }
    }
    Ok(outcome)
}

fn source_exclusivity_propagator(
    request: &GeoCompositionRequest,
    domain: &mut Domain,
    evidence_index: &ConstraintEvidenceIndex,
    budget: &GeoPropagationBudget,
) -> Result<PropagatorOutcome, GeoPropagationError> {
    let mut outcome = PropagatorOutcome::default();
    for constraint in &request.hard_constraints {
        let GeoHardConstraintKind::AllowedSets { level, sets } = &constraint.constraint else {
            continue;
        };
        if sets.len() > budget.max_hall_subset_size {
            outcome.fallback = Some(GeoPropagationFallback {
                propagator: GeoPropagatorKind::SourceExclusivity,
                counter: "max_hall_subset_size".to_string(),
                configured: usize_to_u64(budget.max_hall_subset_size)?,
                guidance:
                    "raise max_hall_subset_size to allow source-exclusivity allowed-set pruning"
                        .to_string(),
            });
            return Ok(outcome);
        }
        let members = members_at_level(request, *level)?;
        let compatible = sets
            .iter()
            .filter(|set| {
                members.iter().all(|member| {
                    let in_set = set.binary_search(&member.id).is_ok();
                    match domain.value(member) {
                        DomainValue::Forced => in_set,
                        DomainValue::Excluded => !in_set,
                        DomainValue::Unknown => true,
                        DomainValue::Contradictory => false,
                    }
                })
            })
            .collect::<Vec<_>>();
        if compatible.is_empty() {
            continue;
        }
        for member in members {
            if domain.value(&member) != DomainValue::Unknown {
                continue;
            }
            let appears = compatible
                .iter()
                .filter(|set| set.binary_search(&member.id).is_ok())
                .count();
            let value = if appears == 0 {
                Some(GeoPrunedValue::Excluded)
            } else if appears == compatible.len() {
                Some(GeoPrunedValue::Forced)
            } else {
                None
            };
            if let Some(value) = value {
                let mut reason = evidence_index.reason_for_constraint(&constraint.id);
                for other in members_at_level(request, *level)? {
                    if domain.value(&other) != DomainValue::Unknown {
                        reason.merge(domain.reason_for_member(&other));
                    }
                }
                if let Some(pruning) =
                    domain.add_pruning(member, value, GeoPropagatorKind::SourceExclusivity, reason)
                {
                    outcome.prunings.push(pruning);
                }
            }
        }
    }
    Ok(outcome)
}

fn members_at_level(
    request: &GeoCompositionRequest,
    level: GeoEntityLevel,
) -> Result<Vec<GeoEntityRef>, GeoPropagationError> {
    match level {
        GeoEntityLevel::Parcel => Ok(request
            .universe
            .parcels
            .iter()
            .map(|id| GeoEntityRef::new(GeoEntityLevel::Parcel, id.clone()))
            .collect()),
        GeoEntityLevel::Building => Ok(request
            .universe
            .buildings
            .iter()
            .map(|building| GeoEntityRef::new(GeoEntityLevel::Building, building.id.clone()))
            .collect()),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => Err(GeoPropagationError::invalid(
            "Geo propagation supports only parcel and building levels",
            [("level", level_name(level))],
        )),
    }
}

fn validate_prunings(prunings: &[GeoPruning]) -> Result<(), GeoPropagationError> {
    let mut previous: Option<&GeoPruning> = None;
    let mut values_by_member: BTreeMap<&GeoEntityRef, GeoPrunedValue> = BTreeMap::new();
    for pruning in prunings {
        if let Some(previous) = previous
            && pruning_sort_key(previous, pruning) != std::cmp::Ordering::Less
        {
            return Err(GeoPropagationError::invalid(
                "Geo propagation prunings must be strictly sorted by member and value",
                [("field", "prunings")],
            ));
        }
        previous = Some(pruning);
        validate_member_ref("prunings[].member", &pruning.member)?;
        validate_sorted_nonempty_ids("constraint_ids", &pruning.constraint_ids)?;
        validate_sorted_ids("evidence_ids", &pruning.evidence_ids)?;
        if let Some(previous_value) = values_by_member.insert(&pruning.member, pruning.value)
            && previous_value != pruning.value
        {
            return Err(GeoPropagationError::invalid(
                "Geo propagation cannot both force and exclude the same member",
                [
                    ("level", level_name(pruning.member.level)),
                    ("member", pruning.member.id.as_str()),
                ],
            ));
        }
    }
    Ok(())
}

fn validate_fallback(fallback: &GeoPropagationFallback) -> Result<(), GeoPropagationError> {
    match fallback.counter.as_str() {
        "max_fixpoint_rounds" | "max_hall_subset_size" | "max_subset_sum_states" => {}
        _ => {
            return Err(GeoPropagationError::invalid(
                "Geo propagation fallback counter is not a declared propagation budget field",
                [("counter", fallback.counter.as_str())],
            ));
        }
    }
    if fallback.configured == 0 {
        return Err(GeoPropagationError::invalid(
            "Geo propagation fallback must record a positive configured budget",
            [("field", "configured")],
        ));
    }
    validate_identifier("guidance", &fallback.guidance)
}

fn validate_counters(counters: &BTreeMap<String, u64>) -> Result<(), GeoPropagationError> {
    if !counters.contains_key("pruning_count")
        || !counters.contains_key("rounds")
        || !counters.contains_key("propagator_count")
    {
        return Err(GeoPropagationError::invalid(
            "Geo propagation counters must include pruning_count, rounds, and propagator_count",
            [("field", "counters")],
        ));
    }
    for key in counters.keys() {
        validate_identifier("counters.key", key)?;
    }
    Ok(())
}

fn validate_member_ref(field: &str, member: &GeoEntityRef) -> Result<(), GeoPropagationError> {
    validate_identifier(field, &member.id)?;
    match member.level {
        GeoEntityLevel::Parcel | GeoEntityLevel::Building => Ok(()),
        GeoEntityLevel::PoiUnit | GeoEntityLevel::Property => Err(GeoPropagationError::invalid(
            "Geo propagation pruning references an unsupported member level",
            [("level", level_name(member.level))],
        )),
    }
}

fn validate_sorted_nonempty_ids(field: &str, values: &[String]) -> Result<(), GeoPropagationError> {
    if values.is_empty() {
        return Err(GeoPropagationError::invalid(
            "Geo propagation pruning reason must name at least one constraint",
            [("field", field)],
        ));
    }
    if field == "constraint_ids" {
        for value in values {
            if value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(GeoPropagationError::invalid(
                    "Geo propagation pruning reason must name stable constraint ids, not numeric indices",
                    [("field", field), ("value", value.as_str())],
                ));
            }
        }
    }
    validate_sorted_ids(field, values)
}

fn validate_sorted_ids(field: &str, values: &[String]) -> Result<(), GeoPropagationError> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_identifier(field, value)?;
        if previous.is_some_and(|previous| previous >= value.as_str()) {
            return Err(GeoPropagationError::invalid(
                "Geo propagation reason ids must be strictly sorted and distinct",
                [("field", field), ("value", value.as_str())],
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_blake3_ref(field: &str, value: &str) -> Result<(), GeoPropagationError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(GeoPropagationError::invalid(
            "Geo propagation digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GeoPropagationError::invalid(
            "Geo propagation digest must be blake3-prefixed lowercase hex",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), GeoPropagationError> {
    if value.is_empty() || value.trim() != value {
        return Err(GeoPropagationError::invalid(
            "Geo propagation identifiers must be non-empty and already canonical",
            [("field", field), ("value", value)],
        ));
    }
    Ok(())
}

fn pruning_constraint_id(pruning: &GeoPruning) -> String {
    format!(
        "prune:{}:{}:{}",
        pruned_value_name(pruning.value),
        level_name(pruning.member.level),
        pruning.member.id
    )
}

fn pruning_sort_key(left: &GeoPruning, right: &GeoPruning) -> std::cmp::Ordering {
    left.member
        .cmp(&right.member)
        .then_with(|| left.value.cmp(&right.value))
        .then_with(|| left.propagator.cmp(&right.propagator))
        .then_with(|| left.constraint_ids.cmp(&right.constraint_ids))
        .then_with(|| left.evidence_ids.cmp(&right.evidence_ids))
}

fn first_differing_member(models: &[GeoCompositionModel]) -> Option<&str> {
    models
        .first()
        .and_then(|model| model.parcels.first().or_else(|| model.buildings.first()))
        .map(String::as_str)
}

fn usize_to_u64(value: usize) -> Result<u64, GeoPropagationError> {
    u64::try_from(value).map_err(|_| GeoPropagationError::overflow("usize to u64"))
}

fn pruned_value_name(value: GeoPrunedValue) -> &'static str {
    match value {
        GeoPrunedValue::Excluded => "excluded",
        GeoPrunedValue::Forced => "forced",
    }
}

fn level_name(level: GeoEntityLevel) -> &'static str {
    match level {
        GeoEntityLevel::PoiUnit => "poi_unit",
        GeoEntityLevel::Building => "building",
        GeoEntityLevel::Parcel => "parcel",
        GeoEntityLevel::Property => "property",
    }
}
