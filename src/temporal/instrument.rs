//! Bounded instrument identifier succession for Proof Case 0.
//!
//! The identity-bearing artifact is a valid-time-bounded
//! `identifies_same_instrument` fact. A companion `superseded_by` succession
//! relation is emitted for review/navigation context only; it references the
//! equality fact and relation connectivity still never grants alias power.

use super::{
    AssertionStatus, FactScope, IdentityFact, IntervalBoundary, RecordedTime, SourceLocator,
    TemporalError, TemporalErrorCode, TimeInterval, finalize_fact, finalize_facts,
    relation::{
        CoreEntityTypeClass, CoreRelationClass, DirectedRelationFact,
        IntervalBoundary as RelationIntervalBoundary, RelationCardinalityConstraints,
        RelationCyclePolicy, RelationEndpoint, RelationEntityTypeRef, RelationError,
        RelationIdentityImplication, RelationIdentityImplicationMode, RelationKindRef,
        RelationOverlapPolicy, RelationProvenance, RelationReview, ReviewDisposition,
        TimeInterval as RelationTimeInterval, finalize_relation, finalize_relations,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const IDENTIFIES_SAME_INSTRUMENT_PREDICATE: &str = "identifies_same_instrument";
pub const INSTRUMENT_SUCCESSION_TRUST_POLICY_REF: &str =
    "instrument_identity.temporal_succession.v0";
pub const INSTRUMENT_SUPERSEDED_BY_RELATION_POLICY_REF: &str =
    "instrument_identity.superseded_by.relation_context.v0";

pub type InstrumentSuccessionResult<T> = Result<T, InstrumentSuccessionError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrumentSuccessionError {
    Temporal(TemporalError),
    Relation(RelationError),
    Serialization(String),
}

impl fmt::Display for InstrumentSuccessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temporal(error) => write!(f, "{error}"),
            Self::Relation(error) => write!(f, "{error}"),
            Self::Serialization(message) => write!(f, "{message}"),
        }
    }
}

impl Error for InstrumentSuccessionError {}

impl From<TemporalError> for InstrumentSuccessionError {
    fn from(error: TemporalError) -> Self {
        Self::Temporal(error)
    }
}

impl From<RelationError> for InstrumentSuccessionError {
    fn from(error: RelationError) -> Self {
        Self::Relation(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentIdentifierObservation {
    pub surface_id: String,
    pub identifier_namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_lei: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annualized_rate_bps: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figi: Option<String>,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub source_locator: SourceLocator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentSuccessionDecisionKind {
    Succession,
    TimeSeriesBreak,
    NoSuccession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentSuccessionReason {
    ValidTimeEqualityWithSupersededByRelation,
    FigiChanged,
    IdentifierUnchanged,
    IdentifierNamespaceChanged,
    MissingIdentifierInput,
    MissingProfileInput,
    MissingPeriodBoundary,
    NoPeriodHandover,
    ProfileMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSuccessionDecision {
    pub kind: InstrumentSuccessionDecisionKind,
    pub reason: InstrumentSuccessionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSeriesBreak {
    pub predecessor_identifier_id: String,
    pub successor_identifier_id: String,
    pub reason: InstrumentSuccessionReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSuccessionSurface {
    pub decision: InstrumentSuccessionDecision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identity_facts: Vec<IdentityFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<DirectedRelationFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<InstrumentSeriesBreak>,
}

pub fn derive_instrument_identifier_succession(
    predecessor: InstrumentIdentifierObservation,
    successor: InstrumentIdentifierObservation,
) -> InstrumentSuccessionResult<InstrumentSuccessionSurface> {
    let predecessor = NormalizedObservation::from_observation(predecessor);
    let successor = NormalizedObservation::from_observation(successor);

    if let Some(field) = first_missing_identifier_input(&predecessor, "predecessor")
        .or_else(|| first_missing_identifier_input(&successor, "successor"))
    {
        return Ok(no_succession(
            InstrumentSuccessionReason::MissingIdentifierInput,
            Some(field),
        ));
    }

    if figi_changed(&predecessor, &successor) {
        return Ok(InstrumentSuccessionSurface {
            decision: InstrumentSuccessionDecision {
                kind: InstrumentSuccessionDecisionKind::TimeSeriesBreak,
                reason: InstrumentSuccessionReason::FigiChanged,
                field: Some("figi".to_string()),
            },
            identity_facts: Vec::new(),
            relations: Vec::new(),
            breaks: vec![InstrumentSeriesBreak {
                predecessor_identifier_id: identifier_id(&predecessor),
                successor_identifier_id: identifier_id(&successor),
                reason: InstrumentSuccessionReason::FigiChanged,
            }],
        });
    }

    if predecessor.identifier_namespace != successor.identifier_namespace {
        return Ok(no_succession(
            InstrumentSuccessionReason::IdentifierNamespaceChanged,
            Some("identifier_namespace".to_string()),
        ));
    }

    if predecessor.identifier_value == successor.identifier_value {
        return Ok(no_succession(
            InstrumentSuccessionReason::IdentifierUnchanged,
            Some("identifier_value".to_string()),
        ));
    }

    if let Some(field) = first_missing_profile_input(&predecessor, "predecessor")
        .or_else(|| first_missing_profile_input(&successor, "successor"))
    {
        return Ok(no_succession(
            InstrumentSuccessionReason::MissingProfileInput,
            Some(field),
        ));
    }

    if !profile_matches(&predecessor, &successor) {
        return Ok(no_succession(
            InstrumentSuccessionReason::ProfileMismatch,
            None,
        ));
    }

    let Some(predecessor_end_at) = predecessor.valid_time.end_at.as_deref() else {
        return Ok(no_succession(
            InstrumentSuccessionReason::MissingPeriodBoundary,
            Some("predecessor.valid_time.end_at".to_string()),
        ));
    };
    let Some(successor_start_at) = successor.valid_time.start_at.as_deref() else {
        return Ok(no_succession(
            InstrumentSuccessionReason::MissingPeriodBoundary,
            Some("successor.valid_time.start_at".to_string()),
        ));
    };
    if !has_period_handover(
        &predecessor.valid_time,
        &successor.valid_time,
        predecessor_end_at,
        successor_start_at,
    )? {
        return Ok(no_succession(
            InstrumentSuccessionReason::NoPeriodHandover,
            None,
        ));
    }

    let fact = finalize_fact(IdentityFact {
        version: String::new(),
        fact_id: String::new(),
        assertion_key: String::new(),
        conflict_key: String::new(),
        subject_id: identifier_id(&predecessor),
        predicate: IDENTIFIES_SAME_INSTRUMENT_PREDICATE.to_string(),
        object_id: identifier_id(&successor),
        valid_time: successor.valid_time.clone(),
        recorded_time: successor.recorded_time.clone(),
        source_locator: successor.source_locator.clone(),
        materialization_digest: materialization_digest(&predecessor, &successor)?,
        assertion_status: AssertionStatus::Accepted,
        trust_policy_ref: INSTRUMENT_SUCCESSION_TRUST_POLICY_REF.to_string(),
        scope: Some(FactScope {
            scope_type: "profile".to_string(),
            scope_id: "instrument_identity".to_string(),
        }),
        supersedes: Vec::new(),
        retracts: Vec::new(),
    })?;
    let relation = finalize_relation(DirectedRelationFact {
        version: String::new(),
        relation_id: String::new(),
        edge_key: String::new(),
        subject: instrument_endpoint(fact.subject_id.clone()),
        relation: RelationKindRef::Core {
            class: CoreRelationClass::Succession,
        },
        object: instrument_endpoint(fact.object_id.clone()),
        valid_time: relation_interval_from_fact(&fact.valid_time),
        provenance: RelationProvenance {
            source_system: fact.source_locator.source_system.clone(),
            locator: fact.source_locator.locator.clone(),
            fragment: fact.source_locator.fragment.clone(),
            observed_at: fact.recorded_time.start_at.clone(),
        },
        policy_ref: INSTRUMENT_SUPERSEDED_BY_RELATION_POLICY_REF.to_string(),
        constraints: RelationCardinalityConstraints {
            max_objects_per_subject: Some(1),
            max_subjects_per_object: None,
            overlap_policy: RelationOverlapPolicy::Disallow,
            cycle_policy: RelationCyclePolicy::Disallow,
        },
        identity_implication: RelationIdentityImplication {
            mode: RelationIdentityImplicationMode::SupportedBySeparateEqualityFact,
            equality_fact_ref: Some(fact.fact_id.clone()),
        },
        review: RelationReview {
            disposition: ReviewDisposition::Related,
            reason_code: "superseded_by_relation_context_only".to_string(),
        },
    })?;

    Ok(InstrumentSuccessionSurface {
        decision: InstrumentSuccessionDecision {
            kind: InstrumentSuccessionDecisionKind::Succession,
            reason: InstrumentSuccessionReason::ValidTimeEqualityWithSupersededByRelation,
            field: None,
        },
        identity_facts: vec![fact],
        relations: vec![relation],
        breaks: Vec::new(),
    })
}

pub fn canonical_instrument_succession_bytes(
    surface: &InstrumentSuccessionSurface,
) -> InstrumentSuccessionResult<Vec<u8>> {
    let mut normalized = surface.clone();
    normalized.identity_facts = finalize_facts(normalized.identity_facts)?;
    normalized.relations = finalize_relations(normalized.relations)?;
    normalized.breaks.sort_by(|left, right| {
        left.predecessor_identifier_id
            .cmp(&right.predecessor_identifier_id)
            .then_with(|| {
                left.successor_identifier_id
                    .cmp(&right.successor_identifier_id)
            })
            .then_with(|| left.reason.cmp(&right.reason))
    });
    serde_json::to_vec(&normalized).map_err(|error| {
        InstrumentSuccessionError::Serialization(format!(
            "failed to serialize instrument succession surface: {error}"
        ))
    })
}

#[derive(Debug, Clone)]
struct NormalizedObservation {
    surface_id: String,
    identifier_namespace: String,
    identifier_value: String,
    issuer_lei: Option<String>,
    maturity_date: Option<String>,
    annualized_rate_bps: Option<i64>,
    balance_profile: Option<String>,
    figi: Option<String>,
    valid_time: TimeInterval,
    recorded_time: RecordedTime,
    source_locator: SourceLocator,
}

impl NormalizedObservation {
    fn from_observation(observation: InstrumentIdentifierObservation) -> Self {
        Self {
            surface_id: observation.surface_id.trim().to_string(),
            identifier_namespace: observation.identifier_namespace.trim().to_string(),
            identifier_value: trim_option(observation.identifier_value).unwrap_or_default(),
            issuer_lei: trim_option(observation.issuer_lei),
            maturity_date: trim_option(observation.maturity_date),
            annualized_rate_bps: observation.annualized_rate_bps,
            balance_profile: trim_option(observation.balance_profile),
            figi: trim_option(observation.figi),
            valid_time: observation.valid_time,
            recorded_time: observation.recorded_time,
            source_locator: observation.source_locator,
        }
    }
}

fn no_succession(
    reason: InstrumentSuccessionReason,
    field: Option<String>,
) -> InstrumentSuccessionSurface {
    InstrumentSuccessionSurface {
        decision: InstrumentSuccessionDecision {
            kind: InstrumentSuccessionDecisionKind::NoSuccession,
            reason,
            field,
        },
        identity_facts: Vec::new(),
        relations: Vec::new(),
        breaks: Vec::new(),
    }
}

fn first_missing_identifier_input(
    observation: &NormalizedObservation,
    prefix: &str,
) -> Option<String> {
    if observation.surface_id.is_empty() {
        return Some(format!("{prefix}.surface_id"));
    }
    if observation.identifier_namespace.is_empty() {
        return Some(format!("{prefix}.identifier_namespace"));
    }
    if observation.identifier_value.is_empty() {
        return Some(format!("{prefix}.identifier_value"));
    }
    None
}

fn first_missing_profile_input(
    observation: &NormalizedObservation,
    prefix: &str,
) -> Option<String> {
    if observation.issuer_lei.is_none() {
        return Some(format!("{prefix}.issuer_lei"));
    }
    if observation.maturity_date.is_none() {
        return Some(format!("{prefix}.maturity_date"));
    }
    if observation.annualized_rate_bps.is_none() {
        return Some(format!("{prefix}.annualized_rate_bps"));
    }
    if observation.balance_profile.is_none() {
        return Some(format!("{prefix}.balance_profile"));
    }
    None
}

fn figi_changed(predecessor: &NormalizedObservation, successor: &NormalizedObservation) -> bool {
    matches!(
        (predecessor.figi.as_deref(), successor.figi.as_deref()),
        (Some(left), Some(right)) if left != right
    )
}

fn profile_matches(predecessor: &NormalizedObservation, successor: &NormalizedObservation) -> bool {
    predecessor.issuer_lei == successor.issuer_lei
        && predecessor.maturity_date == successor.maturity_date
        && predecessor.annualized_rate_bps == successor.annualized_rate_bps
        && predecessor.balance_profile == successor.balance_profile
}

fn has_period_handover(
    predecessor_valid_time: &TimeInterval,
    successor_valid_time: &TimeInterval,
    predecessor_end_at: &str,
    successor_start_at: &str,
) -> InstrumentSuccessionResult<bool> {
    if matches!(predecessor_valid_time.end_bound, IntervalBoundary::Open) {
        return Err(TemporalError::new(
            TemporalErrorCode::ArtifactContract,
            "predecessor.valid_time.end_bound cannot be open when end_at is present",
        )
        .into());
    }
    if matches!(successor_valid_time.start_bound, IntervalBoundary::Open) {
        return Err(TemporalError::new(
            TemporalErrorCode::ArtifactContract,
            "successor.valid_time.start_bound cannot be open when start_at is present",
        )
        .into());
    }

    let predecessor_end = parse_timestamp(predecessor_end_at, "predecessor.valid_time.end_at")?;
    let successor_start = parse_timestamp(successor_start_at, "successor.valid_time.start_at")?;
    if predecessor_end < successor_start {
        return Ok(true);
    }
    if predecessor_end > successor_start {
        return Ok(false);
    }
    Ok(!matches!(
        predecessor_valid_time.end_bound,
        IntervalBoundary::Inclusive
    ) || !matches!(
        successor_valid_time.start_bound,
        IntervalBoundary::Inclusive
    ))
}

fn identifier_id(observation: &NormalizedObservation) -> String {
    format!(
        "instrument_identifier:{}:{}",
        observation.identifier_namespace, observation.identifier_value
    )
}

fn instrument_endpoint(identity_id: String) -> RelationEndpoint {
    RelationEndpoint {
        identity_id,
        entity_type: RelationEntityTypeRef::Core {
            class: CoreEntityTypeClass::Instrument,
        },
    }
}

fn relation_interval_from_fact(interval: &TimeInterval) -> RelationTimeInterval {
    RelationTimeInterval {
        start_at: interval.start_at.clone(),
        start_bound: relation_boundary(interval.start_bound),
        end_at: interval.end_at.clone(),
        end_bound: relation_boundary(interval.end_bound),
    }
}

fn relation_boundary(boundary: IntervalBoundary) -> RelationIntervalBoundary {
    match boundary {
        IntervalBoundary::Inclusive => RelationIntervalBoundary::Inclusive,
        IntervalBoundary::Exclusive => RelationIntervalBoundary::Exclusive,
        IntervalBoundary::Open => RelationIntervalBoundary::Open,
    }
}

fn materialization_digest(
    predecessor: &NormalizedObservation,
    successor: &NormalizedObservation,
) -> InstrumentSuccessionResult<String> {
    #[derive(Serialize)]
    struct DigestPayload<'a> {
        version: &'static str,
        predicate: &'static str,
        predecessor_surface_id: &'a str,
        predecessor_identifier_id: String,
        predecessor_issuer_lei: &'a Option<String>,
        predecessor_maturity_date: &'a Option<String>,
        predecessor_annualized_rate_bps: &'a Option<i64>,
        predecessor_balance_profile: &'a Option<String>,
        predecessor_figi: &'a Option<String>,
        successor_surface_id: &'a str,
        successor_identifier_id: String,
        successor_issuer_lei: &'a Option<String>,
        successor_maturity_date: &'a Option<String>,
        successor_annualized_rate_bps: &'a Option<i64>,
        successor_balance_profile: &'a Option<String>,
        successor_figi: &'a Option<String>,
        valid_time: &'a TimeInterval,
        source_locator: &'a SourceLocator,
    }

    let payload = DigestPayload {
        version: "instrument_identifier_succession.v0",
        predicate: IDENTIFIES_SAME_INSTRUMENT_PREDICATE,
        predecessor_surface_id: &predecessor.surface_id,
        predecessor_identifier_id: identifier_id(predecessor),
        predecessor_issuer_lei: &predecessor.issuer_lei,
        predecessor_maturity_date: &predecessor.maturity_date,
        predecessor_annualized_rate_bps: &predecessor.annualized_rate_bps,
        predecessor_balance_profile: &predecessor.balance_profile,
        predecessor_figi: &predecessor.figi,
        successor_surface_id: &successor.surface_id,
        successor_identifier_id: identifier_id(successor),
        successor_issuer_lei: &successor.issuer_lei,
        successor_maturity_date: &successor.maturity_date,
        successor_annualized_rate_bps: &successor.annualized_rate_bps,
        successor_balance_profile: &successor.balance_profile,
        successor_figi: &successor.figi,
        valid_time: &successor.valid_time,
        source_locator: &successor.source_locator,
    };
    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        InstrumentSuccessionError::Serialization(format!(
            "failed to serialize instrument succession digest: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn parse_timestamp(value: &str, field: &str) -> InstrumentSuccessionResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            TemporalError::new(
                TemporalErrorCode::ArtifactContract,
                format!("invalid RFC3339 timestamp for {field}: {error}"),
            )
            .into()
        })
}

fn trim_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
