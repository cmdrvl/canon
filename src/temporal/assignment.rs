#![forbid(unsafe_code)]

//! Domain-neutral typed assignment facts that stay separate from entity identity.
//!
//! Assignment facts describe who held a typed role for a subject during a valid
//! and known interval. They may point at a resolved entity or preserve an
//! unresolved disclosed observation. They never create aliases or same-as
//! evidence on their own.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn assignment_schema_version() -> &'static str {
    concat!("canon.identity.assignment", ".v1")
}

pub type AssignmentResult<T> = Result<T, AssignmentError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentErrorCode {
    ArtifactContract,
    CorruptReference,
    PolicyConstraint,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentError {
    pub code: AssignmentErrorCode,
    pub message: String,
}

impl AssignmentError {
    pub fn new(code: AssignmentErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for AssignmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for AssignmentError {}

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssignmentEntityTypeRef {
    Core {
        class: CoreEntityTypeClass,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

impl AssignmentEntityTypeRef {
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
pub struct AssignmentSubject {
    pub identity_id: String,
    pub entity_type: AssignmentEntityTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssignmentRoleRef {
    pub package_digest: String,
    pub term_id: String,
}

impl AssignmentRoleRef {
    fn stable_key(&self) -> String {
        format!("{}:{}", self.package_digest, self.term_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssignmentPolicyRef {
    pub package_digest: String,
    pub policy_id: String,
}

impl AssignmentPolicyRef {
    fn stable_key(&self) -> String {
        format!("{}:{}", self.package_digest, self.policy_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssignmentAssignee {
    Entity {
        identity_id: String,
        entity_type: AssignmentEntityTypeRef,
    },
    Observation {
        disclosed_value: String,
        entity_type: AssignmentEntityTypeRef,
    },
}

impl AssignmentAssignee {
    fn stable_key(&self) -> String {
        match self {
            Self::Entity {
                identity_id,
                entity_type,
            } => format!("entity:{identity_id}:{}", entity_type.stable_key()),
            Self::Observation {
                disclosed_value,
                entity_type,
            } => format!("observation:{disclosed_value}:{}", entity_type.stable_key()),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AssignmentConflictPolicy {
    #[serde(rename = "allow_conflicting_claims")]
    Allow,
    #[serde(rename = "disallow_conflicting_claims")]
    Disallow,
    #[serde(rename = "review_conflicting_claims")]
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AssignmentSuccessorPolicy {
    #[serde(rename = "allow_parallel_assignments")]
    AllowParallel,
    #[serde(rename = "require_non_overlapping_successors")]
    RequireNonOverlapping,
    #[serde(rename = "review_overlapping_successors")]
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_active_assignees_per_subject_role: Option<u32>,
    #[serde(default)]
    pub allow_unresolved_assignee: bool,
    pub conflict_policy: AssignmentConflictPolicy,
    pub successor_policy: AssignmentSuccessorPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatusCode {
    Asserted,
    Disputed,
    Retracted,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentStatus {
    pub code: AssignmentStatusCode,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_assignment_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AssignmentProvenance {
    pub source_system: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedAssignmentFact {
    pub version: String,
    pub assignment_id: String,
    pub assignment_key: String,
    pub subject: AssignmentSubject,
    pub role: AssignmentRoleRef,
    pub assignee: AssignmentAssignee,
    pub valid_time: TimeInterval,
    pub known_time: TimeInterval,
    pub policy_ref: AssignmentPolicyRef,
    pub constraints: AssignmentConstraints,
    pub status: AssignmentStatus,
    pub provenance: AssignmentProvenance,
}

pub fn finalize_assignment(mut fact: TypedAssignmentFact) -> AssignmentResult<TypedAssignmentFact> {
    if fact.version.trim().is_empty() {
        fact.version = assignment_schema_version().to_string();
    }
    if fact.version != assignment_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported assignment version: {}",
            fact.version
        )));
    }

    fact.subject = normalize_subject(fact.subject)?;
    fact.role = normalize_role_ref(fact.role)?;
    fact.valid_time = normalize_interval(fact.valid_time, "valid_time")?;
    fact.known_time = normalize_interval(fact.known_time, "known_time")?;
    fact.policy_ref = normalize_policy_ref(fact.policy_ref)?;
    fact.constraints = normalize_constraints(fact.constraints)?;
    fact.status = normalize_status(fact.status)?;
    fact.provenance = normalize_provenance(fact.provenance)?;
    fact.assignee = normalize_assignee(fact.assignee, fact.constraints.allow_unresolved_assignee)?;

    fact.assignment_key = compute_assignment_key(&fact)?;
    fact.assignment_id.clear();
    fact.assignment_id = compute_assignment_id(&fact)?;
    Ok(fact)
}

pub fn finalize_assignments(
    facts: impl IntoIterator<Item = TypedAssignmentFact>,
) -> AssignmentResult<Vec<TypedAssignmentFact>> {
    let mut normalized = Vec::new();
    for fact in facts {
        normalized.push(finalize_assignment(fact)?);
    }
    normalized.sort_by(assignment_cmp);

    let mut deduped: Vec<TypedAssignmentFact> = Vec::with_capacity(normalized.len());
    for fact in normalized {
        if let Some(last) = deduped.last()
            && last.assignment_id == fact.assignment_id
        {
            if last != &fact {
                return Err(policy_constraint_error(
                    "non-identical assignment facts collided on the same assignment_id",
                ));
            }
            continue;
        }
        deduped.push(fact);
    }

    validate_policy_constraints(&deduped)?;
    Ok(deduped)
}

pub fn canonical_assignment_bytes(fact: &TypedAssignmentFact) -> AssignmentResult<Vec<u8>> {
    let fact = finalize_assignment(fact.clone())?;
    serde_json::to_vec(&fact).map_err(|error| {
        artifact_contract_error(format!("failed to serialize assignment: {error}"))
    })
}

pub fn canonical_assignment_set_bytes(facts: &[TypedAssignmentFact]) -> AssignmentResult<Vec<u8>> {
    let facts = finalize_assignments(facts.to_vec())?;
    serde_json::to_vec(&facts).map_err(|error| {
        artifact_contract_error(format!("failed to serialize assignment set: {error}"))
    })
}

pub fn assignment_projection_fields(
    fact: &TypedAssignmentFact,
) -> AssignmentResult<BTreeMap<String, String>> {
    let fact = finalize_assignment(fact.clone())?;
    let mut projection = BTreeMap::from([
        ("subject_id".to_string(), fact.subject.identity_id.clone()),
        ("role_term_id".to_string(), fact.role.term_id.clone()),
        (
            "source_system".to_string(),
            fact.provenance.source_system.clone(),
        ),
        (
            "source_locator".to_string(),
            fact.provenance.locator.clone(),
        ),
        (
            "status".to_string(),
            assignment_status_label(fact.status.code),
        ),
    ]);
    match fact.assignee {
        AssignmentAssignee::Entity { identity_id, .. } => {
            projection.insert("assignee_kind".to_string(), "entity".to_string());
            projection.insert("assignee_identity_id".to_string(), identity_id);
        }
        AssignmentAssignee::Observation {
            disclosed_value, ..
        } => {
            projection.insert(
                "assignee_kind".to_string(),
                "unresolved_observation".to_string(),
            );
            projection.insert("assignee_disclosed_value".to_string(), disclosed_value);
        }
    }
    for (key, value) in fact.provenance.raw_fields {
        projection.insert(format!("raw.{key}"), value);
    }
    Ok(projection)
}

pub fn assignment_fact_implies_alias(_fact: &TypedAssignmentFact) -> bool {
    false
}

fn normalize_subject(mut subject: AssignmentSubject) -> AssignmentResult<AssignmentSubject> {
    subject.identity_id = normalized_non_empty(&subject.identity_id, "subject.identity_id")?;
    subject.entity_type = normalize_entity_type(subject.entity_type, "subject.entity_type")?;
    Ok(subject)
}

fn normalize_role_ref(mut role: AssignmentRoleRef) -> AssignmentResult<AssignmentRoleRef> {
    role.package_digest = normalized_hash(&role.package_digest, "role.package_digest")?;
    role.term_id = normalized_opaque_ref(&role.term_id, "role.term_id")?;
    Ok(role)
}

fn normalize_policy_ref(
    mut policy_ref: AssignmentPolicyRef,
) -> AssignmentResult<AssignmentPolicyRef> {
    policy_ref.package_digest =
        normalized_hash(&policy_ref.package_digest, "policy_ref.package_digest")?;
    policy_ref.policy_id = normalized_non_empty(&policy_ref.policy_id, "policy_ref.policy_id")?;
    Ok(policy_ref)
}

fn normalize_assignee(
    assignee: AssignmentAssignee,
    allow_unresolved_assignee: bool,
) -> AssignmentResult<AssignmentAssignee> {
    match assignee {
        AssignmentAssignee::Entity {
            identity_id,
            entity_type,
        } => Ok(AssignmentAssignee::Entity {
            identity_id: normalized_non_empty(&identity_id, "assignee.identity_id")?,
            entity_type: normalize_entity_type(entity_type, "assignee.entity_type")?,
        }),
        AssignmentAssignee::Observation {
            disclosed_value,
            entity_type,
        } => {
            if !allow_unresolved_assignee {
                return Err(policy_constraint_error(
                    "policy does not allow unresolved assignee observations",
                ));
            }
            Ok(AssignmentAssignee::Observation {
                disclosed_value: normalized_non_empty(
                    &disclosed_value,
                    "assignee.disclosed_value",
                )?,
                entity_type: normalize_entity_type(entity_type, "assignee.entity_type")?,
            })
        }
    }
}

fn normalize_entity_type(
    entity_type: AssignmentEntityTypeRef,
    field: &str,
) -> AssignmentResult<AssignmentEntityTypeRef> {
    match entity_type {
        AssignmentEntityTypeRef::Core { .. } => Ok(entity_type),
        AssignmentEntityTypeRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(AssignmentEntityTypeRef::Extension {
            package_digest: normalized_hash(&package_digest, &format!("{field}.package_digest"))?,
            vocabulary: normalized_non_empty(&vocabulary, &format!("{field}.vocabulary"))?,
            value: normalized_non_empty(&value, &format!("{field}.value"))?,
        }),
    }
}

fn normalize_interval(mut interval: TimeInterval, field: &str) -> AssignmentResult<TimeInterval> {
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
    constraints: AssignmentConstraints,
) -> AssignmentResult<AssignmentConstraints> {
    if matches!(constraints.max_active_assignees_per_subject_role, Some(0)) {
        return Err(artifact_contract_error(
            "max_active_assignees_per_subject_role must be absent or >= 1",
        ));
    }
    Ok(constraints)
}

fn normalize_status(mut status: AssignmentStatus) -> AssignmentResult<AssignmentStatus> {
    status.reason_code = normalized_non_empty(&status.reason_code, "status.reason_code")?;
    status.supersedes_assignment_id = status
        .supersedes_assignment_id
        .take()
        .map(|value| normalized_hash(&value, "status.supersedes_assignment_id"))
        .transpose()?;
    match status.code {
        AssignmentStatusCode::Corrected => {
            if status.supersedes_assignment_id.is_none() {
                return Err(policy_constraint_error(
                    "corrected assignments require supersedes_assignment_id",
                ));
            }
        }
        _ => {
            if status.supersedes_assignment_id.is_some() {
                return Err(policy_constraint_error(
                    "only corrected assignments may set supersedes_assignment_id",
                ));
            }
        }
    }
    Ok(status)
}

fn normalize_provenance(
    mut provenance: AssignmentProvenance,
) -> AssignmentResult<AssignmentProvenance> {
    provenance.source_system =
        normalized_non_empty(&provenance.source_system, "provenance.source_system")?;
    provenance.locator = normalized_non_empty(&provenance.locator, "provenance.locator")?;
    provenance.fragment = provenance
        .fragment
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    provenance.observed_at = provenance
        .observed_at
        .take()
        .map(|value| canonical_timestamp(&value, "provenance.observed_at"))
        .transpose()?;
    provenance.raw_fields = provenance
        .raw_fields
        .into_iter()
        .map(|(key, value)| {
            Ok((
                normalized_non_empty(&key, "provenance.raw_fields key")?,
                normalized_non_empty(&value, &format!("provenance.raw_fields.{key}"))?,
            ))
        })
        .collect::<AssignmentResult<BTreeMap<_, _>>>()?;
    Ok(provenance)
}

fn validate_policy_constraints(facts: &[TypedAssignmentFact]) -> AssignmentResult<()> {
    let mut groups: BTreeMap<(String, String, String), Vec<&TypedAssignmentFact>> = BTreeMap::new();
    for fact in facts {
        groups
            .entry((
                fact.subject.identity_id.clone(),
                fact.role.stable_key(),
                fact.policy_ref.stable_key(),
            ))
            .or_default()
            .push(fact);
    }

    for ((_subject_id, _role_key, _policy_key), group) in groups {
        validate_max_active_assignees(&group)?;
        validate_conflicting_claims(&group)?;
        validate_successor_constraints(&group)?;
    }

    Ok(())
}

fn validate_max_active_assignees(group: &[&TypedAssignmentFact]) -> AssignmentResult<()> {
    let Some(limit) = group
        .iter()
        .filter_map(|fact| fact.constraints.max_active_assignees_per_subject_role)
        .min()
    else {
        return Ok(());
    };

    let active_facts = group
        .iter()
        .copied()
        .filter(|fact| assignment_status_counts_as_active(fact.status.code))
        .collect::<Vec<_>>();
    if active_facts.is_empty() {
        return Ok(());
    }

    let peak = peak_distinct_assignee_count(&active_facts)?;
    if peak > limit {
        return Err(policy_constraint_error(format!(
            "subject {} role {} exceeds max_active_assignees_per_subject_role={limit}",
            active_facts[0].subject.identity_id, active_facts[0].role.term_id
        )));
    }
    Ok(())
}

fn validate_conflicting_claims(group: &[&TypedAssignmentFact]) -> AssignmentResult<()> {
    if !has_overlapping_distinct_assignees(group)? {
        return Ok(());
    }

    if group.iter().any(|fact| {
        matches!(
            fact.constraints.conflict_policy,
            AssignmentConflictPolicy::Disallow
        )
    }) {
        return Err(policy_constraint_error(
            "policy disallows conflicting assignment claims for the same subject and role",
        ));
    }

    Ok(())
}

fn validate_successor_constraints(group: &[&TypedAssignmentFact]) -> AssignmentResult<()> {
    if !has_overlapping_distinct_assignees(group)? {
        return Ok(());
    }

    if group.iter().any(|fact| {
        matches!(
            fact.constraints.successor_policy,
            AssignmentSuccessorPolicy::RequireNonOverlapping
        )
    }) {
        return Err(policy_constraint_error(
            "policy requires non-overlapping successor assignments for this subject and role",
        ));
    }

    Ok(())
}

fn has_overlapping_distinct_assignees(group: &[&TypedAssignmentFact]) -> AssignmentResult<bool> {
    let active_facts = group
        .iter()
        .copied()
        .filter(|fact| assignment_status_counts_as_active(fact.status.code))
        .collect::<Vec<_>>();
    if active_facts.len() < 2 {
        return Ok(false);
    }
    Ok(peak_distinct_assignee_count(&active_facts)? > 1)
}

fn peak_distinct_assignee_count(facts: &[&TypedAssignmentFact]) -> AssignmentResult<u32> {
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
        return Ok(facts
            .iter()
            .map(|fact| fact.assignee.stable_key())
            .collect::<BTreeSet<_>>()
            .len() as u32);
    }

    let mut peak = 0u32;
    for timestamp in candidate_times {
        let active = facts
            .iter()
            .filter(|fact| interval_contains(&fact.valid_time, timestamp))
            .map(|fact| fact.assignee.stable_key())
            .collect::<BTreeSet<_>>();
        peak = peak.max(active.len() as u32);
    }
    Ok(peak)
}

fn assignment_status_counts_as_active(code: AssignmentStatusCode) -> bool {
    !matches!(code, AssignmentStatusCode::Retracted)
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
) -> AssignmentResult<()> {
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

fn compute_assignment_key(fact: &TypedAssignmentFact) -> AssignmentResult<String> {
    #[derive(Serialize)]
    struct AssignmentKey<'a> {
        version: &'static str,
        subject: &'a AssignmentSubject,
        role: &'a AssignmentRoleRef,
        assignee: &'a AssignmentAssignee,
        valid_time: &'a TimeInterval,
        policy_ref: &'a AssignmentPolicyRef,
    }

    hash_struct(
        &AssignmentKey {
            version: assignment_schema_version(),
            subject: &fact.subject,
            role: &fact.role,
            assignee: &fact.assignee,
            valid_time: &fact.valid_time,
            policy_ref: &fact.policy_ref,
        },
        "assignment_key",
    )
}

fn compute_assignment_id(fact: &TypedAssignmentFact) -> AssignmentResult<String> {
    #[derive(Serialize)]
    struct AssignmentId<'a> {
        version: &'static str,
        assignment_key: &'a str,
        known_time: &'a TimeInterval,
        constraints: &'a AssignmentConstraints,
        status: &'a AssignmentStatus,
        provenance: &'a AssignmentProvenance,
    }

    hash_struct(
        &AssignmentId {
            version: assignment_schema_version(),
            assignment_key: &fact.assignment_key,
            known_time: &fact.known_time,
            constraints: &fact.constraints,
            status: &fact.status,
            provenance: &fact.provenance,
        },
        "assignment_id",
    )
}

fn hash_struct(value: &impl Serialize, label: &str) -> AssignmentResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {label}: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn canonical_optional_timestamp(
    value: Option<String>,
    field: &str,
    part: &str,
) -> AssignmentResult<Option<String>> {
    value
        .map(|value| canonical_timestamp(&value, &format!("{field}.{part}")))
        .transpose()
}

fn canonical_timestamp(value: &str, field: &str) -> AssignmentResult<String> {
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

fn normalized_non_empty(value: &str, field: &str) -> AssignmentResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} requires a non-empty string"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> AssignmentResult<String> {
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

fn normalized_opaque_ref(value: &str, field: &str) -> AssignmentResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some((namespace, local)) = normalized.split_once(':') else {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    };
    if namespace.is_empty()
        || local.is_empty()
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        || !local.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    }
    Ok(normalized)
}

fn assignment_cmp(left: &TypedAssignmentFact, right: &TypedAssignmentFact) -> std::cmp::Ordering {
    left.assignment_key
        .cmp(&right.assignment_key)
        .then_with(|| left.valid_time.start_at.cmp(&right.valid_time.start_at))
        .then_with(|| left.known_time.start_at.cmp(&right.known_time.start_at))
        .then_with(|| {
            left.provenance
                .source_system
                .cmp(&right.provenance.source_system)
        })
        .then_with(|| left.provenance.locator.cmp(&right.provenance.locator))
        .then_with(|| left.assignment_id.cmp(&right.assignment_id))
}

fn assignment_status_label(code: AssignmentStatusCode) -> String {
    match code {
        AssignmentStatusCode::Asserted => "asserted".to_string(),
        AssignmentStatusCode::Disputed => "disputed".to_string(),
        AssignmentStatusCode::Retracted => "retracted".to_string(),
        AssignmentStatusCode::Corrected => "corrected".to_string(),
    }
}

fn artifact_contract_error(message: impl Into<String>) -> AssignmentError {
    AssignmentError::new(AssignmentErrorCode::ArtifactContract, message)
}

fn corrupt_reference_error(message: impl Into<String>) -> AssignmentError {
    AssignmentError::new(AssignmentErrorCode::CorruptReference, message)
}

fn policy_constraint_error(message: impl Into<String>) -> AssignmentError {
    AssignmentError::new(AssignmentErrorCode::PolicyConstraint, message)
}
