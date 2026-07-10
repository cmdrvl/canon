use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn relation_schema_version() -> &'static str {
    concat!("canon.identity.relation", ".v1")
}

pub type RelationResult<T> = Result<T, RelationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelationErrorCode {
    ArtifactContract,
    CorruptReference,
    PolicyConstraint,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationError {
    pub code: RelationErrorCode,
    pub message: String,
}

impl RelationError {
    pub fn new(code: RelationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for RelationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for RelationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntervalBoundary {
    #[default]
    Inclusive,
    Exclusive,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreEntityTypeClass {
    Account,
    Asset,
    Document,
    Event,
    Instrument,
    Location,
    Organization,
    Person,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreRelationClass {
    Hierarchy,
    Role,
    Succession,
    Association,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelationEntityTypeRef {
    Core {
        class: CoreEntityTypeClass,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelationKindRef {
    Core {
        class: CoreRelationClass,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

impl RelationKindRef {
    fn stable_key(&self) -> String {
        match self {
            Self::Core { class } => format!("core:{class:?}").to_ascii_lowercase(),
            Self::Extension {
                package_digest,
                vocabulary,
                value,
            } => format!("extension:{package_digest}:{vocabulary}:{value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationEndpoint {
    pub identity_id: String,
    pub entity_type: RelationEntityTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TimeInterval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default)]
    pub start_bound: IntervalBoundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
    #[serde(default)]
    pub end_bound: IntervalBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RelationProvenance {
    pub source_system: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationCyclePolicy {
    #[serde(rename = "allow_directed_cycle")]
    Allow,
    #[serde(rename = "disallow_directed_cycle")]
    Disallow,
    #[serde(rename = "review_directed_cycle")]
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationOverlapPolicy {
    #[serde(rename = "allow_overlapping_targets")]
    Allow,
    #[serde(rename = "disallow_overlapping_targets")]
    Disallow,
    #[serde(rename = "review_overlapping_targets")]
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationCardinalityConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_objects_per_subject: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subjects_per_object: Option<u32>,
    pub overlap_policy: RelationOverlapPolicy,
    pub cycle_policy: RelationCyclePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelationIdentityImplicationMode {
    #[default]
    None,
    SupportedBySeparateEqualityFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RelationIdentityImplication {
    #[serde(default)]
    pub mode: RelationIdentityImplicationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equality_fact_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDisposition {
    Same,
    Distinct,
    Related,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationReview {
    pub disposition: ReviewDisposition,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectedRelationFact {
    pub version: String,
    pub relation_id: String,
    pub edge_key: String,
    pub subject: RelationEndpoint,
    pub relation: RelationKindRef,
    pub object: RelationEndpoint,
    pub valid_time: TimeInterval,
    pub provenance: RelationProvenance,
    pub policy_ref: String,
    pub constraints: RelationCardinalityConstraints,
    #[serde(default)]
    pub identity_implication: RelationIdentityImplication,
    pub review: RelationReview,
}

pub fn finalize_relation(mut fact: DirectedRelationFact) -> RelationResult<DirectedRelationFact> {
    if fact.version.trim().is_empty() {
        fact.version = relation_schema_version().to_string();
    }
    if fact.version != relation_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported relation version: {}",
            fact.version
        )));
    }

    fact.subject = normalize_endpoint(fact.subject, "subject")?;
    fact.relation = normalize_relation_kind(fact.relation)?;
    fact.object = normalize_endpoint(fact.object, "object")?;
    fact.valid_time = normalize_interval(fact.valid_time, "valid_time")?;
    fact.provenance = normalize_provenance(fact.provenance)?;
    fact.policy_ref = normalized_non_empty(&fact.policy_ref, "policy_ref")?;
    fact.constraints = normalize_constraints(fact.constraints)?;
    fact.identity_implication = normalize_identity_implication(fact.identity_implication)?;
    fact.review = normalize_review(fact.review)?;

    fact.edge_key = compute_edge_key(&fact)?;
    fact.relation_id.clear();
    fact.relation_id = compute_relation_id(&fact)?;
    Ok(fact)
}

pub fn finalize_relations(
    facts: impl IntoIterator<Item = DirectedRelationFact>,
) -> RelationResult<Vec<DirectedRelationFact>> {
    let mut normalized = Vec::new();
    for fact in facts {
        normalized.push(finalize_relation(fact)?);
    }
    normalized.sort_by(relation_cmp);

    let mut deduped: Vec<DirectedRelationFact> = Vec::with_capacity(normalized.len());
    for fact in normalized {
        if let Some(last) = deduped.last()
            && last.relation_id == fact.relation_id
        {
            if last != &fact {
                return Err(policy_constraint_error(
                    "non-identical relation facts collided on the same relation_id",
                ));
            }
            continue;
        }
        deduped.push(fact);
    }

    validate_policy_constraints(&deduped)?;
    Ok(deduped)
}

pub fn canonical_relation_bytes(fact: &DirectedRelationFact) -> RelationResult<Vec<u8>> {
    let fact = finalize_relation(fact.clone())?;
    serde_json::to_vec(&fact)
        .map_err(|error| artifact_contract_error(format!("failed to serialize relation: {error}")))
}

pub fn canonical_relation_set_bytes(facts: &[DirectedRelationFact]) -> RelationResult<Vec<u8>> {
    let facts = finalize_relations(facts.to_vec())?;
    serde_json::to_vec(&facts).map_err(|error| {
        artifact_contract_error(format!("failed to serialize relation set: {error}"))
    })
}

pub fn relation_timeline_for_subject(
    facts: &[DirectedRelationFact],
    subject_id: &str,
) -> RelationResult<Vec<DirectedRelationFact>> {
    let subject_id = normalized_non_empty(subject_id, "subject_id")?;
    let facts = finalize_relations(facts.to_vec())?;
    Ok(facts
        .into_iter()
        .filter(|fact| fact.subject.identity_id == subject_id)
        .collect())
}

pub fn relation_edge_implies_alias(_fact: &DirectedRelationFact) -> bool {
    false
}

pub fn review_concepts_are_distinct(left: &RelationReview, right: &RelationReview) -> bool {
    left.disposition != right.disposition
}

fn normalize_endpoint(
    mut endpoint: RelationEndpoint,
    field: &str,
) -> RelationResult<RelationEndpoint> {
    endpoint.identity_id =
        normalized_non_empty(&endpoint.identity_id, &format!("{field}.identity_id"))?;
    endpoint.entity_type = normalize_entity_type(endpoint.entity_type)?;
    Ok(endpoint)
}

fn normalize_entity_type(
    entity_type: RelationEntityTypeRef,
) -> RelationResult<RelationEntityTypeRef> {
    match entity_type {
        RelationEntityTypeRef::Core { .. } => Ok(entity_type),
        RelationEntityTypeRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(RelationEntityTypeRef::Extension {
            package_digest: normalized_hash(&package_digest, "entity_type.package_digest")?,
            vocabulary: normalized_non_empty(&vocabulary, "entity_type.vocabulary")?,
            value: normalized_non_empty(&value, "entity_type.value")?,
        }),
    }
}

fn normalize_relation_kind(kind: RelationKindRef) -> RelationResult<RelationKindRef> {
    match kind {
        RelationKindRef::Core { .. } => Ok(kind),
        RelationKindRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(RelationKindRef::Extension {
            package_digest: normalized_hash(&package_digest, "relation.package_digest")?,
            vocabulary: normalized_non_empty(&vocabulary, "relation.vocabulary")?,
            value: normalized_non_empty(&value, "relation.value")?,
        }),
    }
}

fn normalize_provenance(mut provenance: RelationProvenance) -> RelationResult<RelationProvenance> {
    provenance.source_system =
        normalized_non_empty(&provenance.source_system, "provenance.source_system")?;
    provenance.locator = normalized_non_empty(&provenance.locator, "provenance.locator")?;
    provenance.fragment = provenance
        .fragment
        .take()
        .map(|fragment| fragment.trim().to_string())
        .filter(|fragment| !fragment.is_empty());
    provenance.observed_at = provenance
        .observed_at
        .take()
        .map(|value| canonical_timestamp(&value, "provenance.observed_at"))
        .transpose()?;
    Ok(provenance)
}

fn normalize_interval(mut interval: TimeInterval, field: &str) -> RelationResult<TimeInterval> {
    interval.start_at = canonical_optional_timestamp(interval.start_at.take(), field, "start_at")?;
    interval.end_at = canonical_optional_timestamp(interval.end_at.take(), field, "end_at")?;

    if interval.start_at.is_none() {
        interval.start_bound = IntervalBoundary::Open;
    } else if matches!(interval.start_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(format!(
            "{field}.start_bound cannot be open when {field}.start_at is present"
        )));
    }

    if interval.end_at.is_none() {
        interval.end_bound = IntervalBoundary::Open;
    } else if matches!(interval.end_bound, IntervalBoundary::Open) {
        return Err(artifact_contract_error(format!(
            "{field}.end_bound cannot be open when {field}.end_at is present"
        )));
    }

    validate_interval_bounds(
        interval.start_at.as_deref(),
        interval.start_bound,
        interval.end_at.as_deref(),
        interval.end_bound,
        field,
    )?;

    Ok(interval)
}

fn normalize_constraints(
    constraints: RelationCardinalityConstraints,
) -> RelationResult<RelationCardinalityConstraints> {
    if matches!(constraints.max_objects_per_subject, Some(0))
        || matches!(constraints.max_subjects_per_object, Some(0))
    {
        return Err(artifact_contract_error(
            "cardinality constraints must be absent or >= 1",
        ));
    }
    Ok(constraints)
}

fn normalize_identity_implication(
    mut implication: RelationIdentityImplication,
) -> RelationResult<RelationIdentityImplication> {
    implication.equality_fact_ref = implication
        .equality_fact_ref
        .take()
        .map(|value| normalized_hash(&value, "identity_implication.equality_fact_ref"))
        .transpose()?;
    match implication.mode {
        RelationIdentityImplicationMode::None => {
            if implication.equality_fact_ref.is_some() {
                return Err(policy_constraint_error(
                    "identity implication cannot reference an equality fact when mode is none",
                ));
            }
        }
        RelationIdentityImplicationMode::SupportedBySeparateEqualityFact => {
            if implication.equality_fact_ref.is_none() {
                return Err(policy_constraint_error(
                    "supported_by_separate_equality_fact requires equality_fact_ref",
                ));
            }
        }
    }
    Ok(implication)
}

fn normalize_review(mut review: RelationReview) -> RelationResult<RelationReview> {
    review.reason_code = normalized_non_empty(&review.reason_code, "review.reason_code")?;
    Ok(review)
}

fn validate_policy_constraints(facts: &[DirectedRelationFact]) -> RelationResult<()> {
    let mut max_by_subject_groups: BTreeMap<(String, String), Vec<&DirectedRelationFact>> =
        BTreeMap::new();
    let mut max_by_object_groups: BTreeMap<(String, String), Vec<&DirectedRelationFact>> =
        BTreeMap::new();
    let mut cycle_groups: BTreeMap<(String, String), Vec<&DirectedRelationFact>> = BTreeMap::new();

    for fact in facts {
        let relation_key = fact.relation.stable_key();
        max_by_subject_groups
            .entry((fact.policy_ref.clone(), relation_key.clone()))
            .or_default()
            .push(fact);
        max_by_object_groups
            .entry((fact.policy_ref.clone(), relation_key.clone()))
            .or_default()
            .push(fact);
        cycle_groups
            .entry((fact.policy_ref.clone(), relation_key))
            .or_default()
            .push(fact);
    }

    for ((_policy_ref, _relation_key), group) in max_by_subject_groups {
        validate_max_objects_per_subject(&group)?;
    }
    for ((_policy_ref, _relation_key), group) in max_by_object_groups {
        validate_max_subjects_per_object(&group)?;
    }
    for ((_policy_ref, _relation_key), group) in cycle_groups {
        validate_cycles(&group)?;
    }

    Ok(())
}

fn validate_max_objects_per_subject(group: &[&DirectedRelationFact]) -> RelationResult<()> {
    let Some(limit) = group
        .iter()
        .filter_map(|fact| fact.constraints.max_objects_per_subject)
        .min()
    else {
        return Ok(());
    };

    let mut by_subject: BTreeMap<&str, Vec<&DirectedRelationFact>> = BTreeMap::new();
    for fact in group {
        by_subject
            .entry(fact.subject.identity_id.as_str())
            .or_default()
            .push(*fact);
    }

    for (subject_id, facts) in by_subject {
        let peak = peak_distinct_counterparty_count(
            facts.as_slice(),
            true,
            facts[0].constraints.overlap_policy,
        )?;
        if peak > limit {
            return Err(policy_constraint_error(format!(
                "subject {subject_id} exceeds max_objects_per_subject={limit}"
            )));
        }
    }
    Ok(())
}

fn validate_max_subjects_per_object(group: &[&DirectedRelationFact]) -> RelationResult<()> {
    let Some(limit) = group
        .iter()
        .filter_map(|fact| fact.constraints.max_subjects_per_object)
        .min()
    else {
        return Ok(());
    };

    let mut by_object: BTreeMap<&str, Vec<&DirectedRelationFact>> = BTreeMap::new();
    for fact in group {
        by_object
            .entry(fact.object.identity_id.as_str())
            .or_default()
            .push(*fact);
    }

    for (object_id, facts) in by_object {
        let peak = peak_distinct_counterparty_count(
            facts.as_slice(),
            false,
            facts[0].constraints.overlap_policy,
        )?;
        if peak > limit {
            return Err(policy_constraint_error(format!(
                "object {object_id} exceeds max_subjects_per_object={limit}"
            )));
        }
    }
    Ok(())
}

fn peak_distinct_counterparty_count(
    facts: &[&DirectedRelationFact],
    by_subject: bool,
    overlap_policy: RelationOverlapPolicy,
) -> RelationResult<u32> {
    let candidate_times = facts
        .iter()
        .flat_map(|fact| {
            [
                fact.valid_time.start_at.as_deref(),
                fact.valid_time.end_at.as_deref(),
            ]
        })
        .flatten()
        .collect::<BTreeSet<_>>();

    if candidate_times.is_empty() {
        let peak = facts
            .iter()
            .map(|fact| {
                if by_subject {
                    fact.object.identity_id.clone()
                } else {
                    fact.subject.identity_id.clone()
                }
            })
            .collect::<BTreeSet<_>>()
            .len() as u32;
        if peak > 1 && matches!(overlap_policy, RelationOverlapPolicy::Disallow) {
            return Err(policy_constraint_error(
                "policy disallows overlapping targets for open-ended relations",
            ));
        }
        return Ok(peak);
    }

    let mut peak = 0u32;
    for timestamp in candidate_times {
        let active = facts
            .iter()
            .filter(|fact| interval_contains(&fact.valid_time, timestamp))
            .map(|fact| {
                if by_subject {
                    fact.object.identity_id.clone()
                } else {
                    fact.subject.identity_id.clone()
                }
            })
            .collect::<BTreeSet<_>>();
        peak = peak.max(active.len() as u32);
        if active.len() > 1 && matches!(overlap_policy, RelationOverlapPolicy::Disallow) {
            return Err(policy_constraint_error(format!(
                "policy disallows overlapping targets at {timestamp}"
            )));
        }
    }
    Ok(peak)
}

fn validate_cycles(group: &[&DirectedRelationFact]) -> RelationResult<()> {
    if group.is_empty()
        || !group
            .iter()
            .any(|fact| matches!(fact.constraints.cycle_policy, RelationCyclePolicy::Disallow))
    {
        return Ok(());
    }

    let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for fact in group {
        adjacency
            .entry(fact.subject.identity_id.as_str())
            .or_default()
            .push(fact.object.identity_id.as_str());
        adjacency
            .entry(fact.object.identity_id.as_str())
            .or_default();
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Done,
    }

    fn dfs<'a>(
        node: &'a str,
        adjacency: &BTreeMap<&'a str, Vec<&'a str>>,
        state: &mut BTreeMap<&'a str, VisitState>,
    ) -> bool {
        state.insert(node, VisitState::Visiting);
        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                match state.get(neighbor).copied() {
                    Some(VisitState::Visiting) => return true,
                    Some(VisitState::Done) => continue,
                    None => {
                        if dfs(neighbor, adjacency, state) {
                            return true;
                        }
                    }
                }
            }
        }
        state.insert(node, VisitState::Done);
        false
    }

    let mut state = BTreeMap::new();
    for node in adjacency.keys().copied() {
        if !state.contains_key(node) && dfs(node, &adjacency, &mut state) {
            return Err(policy_constraint_error(
                "policy disallows directed cycles for this relation set",
            ));
        }
    }

    Ok(())
}

fn interval_contains(interval: &TimeInterval, timestamp: &str) -> bool {
    let after_start = match interval.start_at.as_deref() {
        None => true,
        Some(start) if timestamp > start => true,
        Some(start) if timestamp == start => {
            matches!(interval.start_bound, IntervalBoundary::Inclusive)
        }
        Some(_) => false,
    };
    let before_end = match interval.end_at.as_deref() {
        None => true,
        Some(end) if timestamp < end => true,
        Some(end) if timestamp == end => matches!(interval.end_bound, IntervalBoundary::Inclusive),
        Some(_) => false,
    };
    after_start && before_end
}

fn validate_interval_bounds(
    start_at: Option<&str>,
    start_bound: IntervalBoundary,
    end_at: Option<&str>,
    end_bound: IntervalBoundary,
    field: &str,
) -> RelationResult<()> {
    if let (Some(start_at), Some(end_at)) = (start_at, end_at) {
        if start_at > end_at {
            return Err(artifact_contract_error(format!(
                "{field}.start_at must be <= {field}.end_at"
            )));
        }
        if start_at == end_at
            && (!matches!(start_bound, IntervalBoundary::Inclusive)
                || !matches!(end_bound, IntervalBoundary::Inclusive))
        {
            return Err(artifact_contract_error(format!(
                "{field} cannot be empty when start_at == end_at"
            )));
        }
    }
    Ok(())
}

fn compute_edge_key(fact: &DirectedRelationFact) -> RelationResult<String> {
    #[derive(Serialize)]
    struct EdgeKey<'a> {
        version: &'static str,
        subject: &'a RelationEndpoint,
        relation: &'a RelationKindRef,
        object: &'a RelationEndpoint,
        valid_time: &'a TimeInterval,
        policy_ref: &'a str,
    }

    hash_struct(
        &EdgeKey {
            version: relation_schema_version(),
            subject: &fact.subject,
            relation: &fact.relation,
            object: &fact.object,
            valid_time: &fact.valid_time,
            policy_ref: &fact.policy_ref,
        },
        "edge_key",
    )
}

fn compute_relation_id(fact: &DirectedRelationFact) -> RelationResult<String> {
    #[derive(Serialize)]
    struct RelationId<'a> {
        version: &'static str,
        edge_key: &'a str,
        provenance: &'a RelationProvenance,
        constraints: &'a RelationCardinalityConstraints,
        identity_implication: &'a RelationIdentityImplication,
        review: &'a RelationReview,
    }

    hash_struct(
        &RelationId {
            version: relation_schema_version(),
            edge_key: &fact.edge_key,
            provenance: &fact.provenance,
            constraints: &fact.constraints,
            identity_implication: &fact.identity_implication,
            review: &fact.review,
        },
        "relation_id",
    )
}

fn hash_struct(value: &impl Serialize, label: &str) -> RelationResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {label}: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn canonical_optional_timestamp(
    value: Option<String>,
    field: &str,
    part: &str,
) -> RelationResult<Option<String>> {
    value
        .map(|value| canonical_timestamp(&value, &format!("{field}.{part}")))
        .transpose()
}

fn canonical_timestamp(value: &str, field: &str) -> RelationResult<String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|error| {
            artifact_contract_error(format!("invalid RFC3339 timestamp for {field}: {error}"))
        })
}

fn normalized_non_empty(value: &str, field: &str) -> RelationResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} requires a non-empty string"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> RelationResult<String> {
    let normalized = value.trim();
    let Some((algorithm, digest)) = normalized.split_once(':') else {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 hash"
        )));
    };
    if !algorithm.eq_ignore_ascii_case("blake3")
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(corrupt_reference_error(format!(
            "{field} must be a canonical blake3 digest"
        )));
    }
    Ok(format!("blake3:{}", digest.to_ascii_lowercase()))
}

fn relation_cmp(left: &DirectedRelationFact, right: &DirectedRelationFact) -> std::cmp::Ordering {
    left.edge_key
        .cmp(&right.edge_key)
        .then_with(|| left.valid_time.start_at.cmp(&right.valid_time.start_at))
        .then_with(|| {
            left.valid_time
                .start_bound
                .cmp(&right.valid_time.start_bound)
        })
        .then_with(|| left.valid_time.end_at.cmp(&right.valid_time.end_at))
        .then_with(|| left.valid_time.end_bound.cmp(&right.valid_time.end_bound))
        .then_with(|| {
            left.provenance
                .source_system
                .cmp(&right.provenance.source_system)
        })
        .then_with(|| left.provenance.locator.cmp(&right.provenance.locator))
        .then_with(|| left.review.disposition.cmp(&right.review.disposition))
        .then_with(|| left.relation_id.cmp(&right.relation_id))
}

fn artifact_contract_error(message: impl Into<String>) -> RelationError {
    RelationError::new(RelationErrorCode::ArtifactContract, message)
}

fn corrupt_reference_error(message: impl Into<String>) -> RelationError {
    RelationError::new(RelationErrorCode::CorruptReference, message)
}

fn policy_constraint_error(message: impl Into<String>) -> RelationError {
    RelationError::new(RelationErrorCode::PolicyConstraint, message)
}
