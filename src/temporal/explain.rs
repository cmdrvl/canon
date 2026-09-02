// Privacy-neutral temporal identity explanations.
//
// These artifacts explain supplied assertions and policy references. They do
// not infer truth or legal status beyond the facts provided by the caller.

use super::{
    AssertionStatus, FactScope, IdentityFact, IntervalBoundary, RecordedTime, SourceLocator,
    TemporalError, TemporalErrorCode, TemporalResult, TimeInterval, finalize_facts,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_TEMPORAL_EXPLAIN_VERSION: &str = "canon.temporal.explain.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalChangeClass {
    NewFact,
    ExpiredFact,
    Correction,
    Retraction,
    PolicyChange,
    ScopeChange,
    Conflict,
    CanonicalRemap,
    NoChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemporalExplainSubject {
    Surface { subject_id: String },
    CanonicalEntity { canonical_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalSnapshotReference {
    pub snapshot_id: String,
    pub registry_id: String,
    pub registry_version: String,
    pub compiled_snapshot_digest: String,
    pub valid_at: String,
    pub known_as_of: String,
    pub policy_ref: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalRelationshipRef {
    pub relationship_id: String,
    pub subject_id: String,
    pub predicate: String,
    pub object_id: String,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub source_locator: SourceLocator,
    pub materialization_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<FactScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalIdentitySnapshot {
    pub snapshot: TemporalSnapshotReference,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<IdentityFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<TemporalRelationshipRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalExplainRequest {
    pub version: String,
    pub subject: TemporalExplainSubject,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<TemporalIdentitySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chain_facts: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalExplainArtifact {
    pub version: String,
    pub subject: TemporalExplainSubject,
    pub summary: TemporalExplainSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<TemporalExplainSnapshotResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<TemporalTimelineEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporalExplainSummary {
    pub snapshot_count: usize,
    pub mapped_snapshot_count: usize,
    pub conflict_snapshot_count: usize,
    pub no_result_snapshot_count: usize,
    pub supporting_fact_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalExplainSnapshotResult {
    pub snapshot: TemporalSnapshotReference,
    pub exact_result: TemporalExactResult,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causal_chain: Vec<TemporalCausalFactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<TemporalRelationshipRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TemporalExactResult {
    NoExactResult {
        reason: String,
    },
    SurfaceMapping {
        subject_id: String,
        canonical_id: String,
        canonical_type: String,
        fact_ids: Vec<String>,
    },
    EntitySupport {
        canonical_id: String,
        subject_ids: Vec<String>,
        fact_ids: Vec<String>,
    },
    Conflict {
        subject_id: Option<String>,
        canonical_ids: Vec<String>,
        fact_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalCausalFactRef {
    pub fact_id: String,
    pub subject_id: String,
    pub predicate: String,
    pub object_id: String,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub assertion_status: AssertionStatus,
    pub source_locator: SourceLocator,
    pub trust_policy_ref: String,
    pub materialization_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<FactScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalTimelineEvent {
    pub event_id: String,
    pub change_class: TemporalChangeClass,
    pub snapshot_id: String,
    pub valid_at: String,
    pub known_as_of: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fact_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_ids: Vec<String>,
    pub message: String,
}

pub fn explain_temporal_identity(
    request: TemporalExplainRequest,
) -> TemporalResult<TemporalExplainArtifact> {
    let request = finalize_explain_request(request)?;
    let mut results = Vec::with_capacity(request.snapshots.len());
    for snapshot in &request.snapshots {
        results.push(explain_snapshot_result(
            snapshot,
            &request.subject,
            request.max_chain_facts,
        )?);
    }
    results.sort_by(|left, right| {
        left.snapshot
            .valid_at
            .cmp(&right.snapshot.valid_at)
            .then_with(|| left.snapshot.known_as_of.cmp(&right.snapshot.known_as_of))
            .then_with(|| left.snapshot.snapshot_id.cmp(&right.snapshot.snapshot_id))
    });

    let mut summary = TemporalExplainSummary {
        snapshot_count: results.len(),
        ..TemporalExplainSummary::default()
    };
    for result in &results {
        match result.exact_result {
            TemporalExactResult::NoExactResult { .. } => summary.no_result_snapshot_count += 1,
            TemporalExactResult::Conflict { .. } => summary.conflict_snapshot_count += 1,
            TemporalExactResult::SurfaceMapping { .. }
            | TemporalExactResult::EntitySupport { .. } => summary.mapped_snapshot_count += 1,
        }
        summary.supporting_fact_count += result.causal_chain.len();
    }

    let timeline = build_timeline(&request.subject, &results)?;
    Ok(TemporalExplainArtifact {
        version: CANON_TEMPORAL_EXPLAIN_VERSION.to_string(),
        subject: request.subject,
        summary,
        snapshots: results,
        timeline,
    })
}

pub fn explain_snapshot_result(
    snapshot: &TemporalIdentitySnapshot,
    subject: &TemporalExplainSubject,
    max_chain_facts: Option<usize>,
) -> TemporalResult<TemporalExplainSnapshotResult> {
    let snapshot = finalize_identity_snapshot(snapshot.clone())?;
    let known_facts = known_facts(&snapshot);
    let active = active_facts_for_subject(&snapshot, subject, &known_facts);
    let chain = minimal_causal_chain(&known_facts, &active, max_chain_facts);
    let relationships = active_relationships_for_subject(&snapshot, subject)?;
    let exact_result = exact_result_for_subject(subject, &active);

    Ok(TemporalExplainSnapshotResult {
        snapshot: snapshot.snapshot,
        exact_result,
        causal_chain: chain,
        relationships,
    })
}

pub fn finalize_identity_snapshot(
    mut snapshot: TemporalIdentitySnapshot,
) -> TemporalResult<TemporalIdentitySnapshot> {
    snapshot.snapshot = finalize_snapshot_reference(snapshot.snapshot)?;
    snapshot.facts = finalize_facts(snapshot.facts)?;
    snapshot.relationships = snapshot
        .relationships
        .into_iter()
        .map(finalize_relationship)
        .collect::<TemporalResult<Vec<_>>>()?;
    snapshot.relationships.sort_by(|left, right| {
        left.relationship_id
            .cmp(&right.relationship_id)
            .then_with(|| left.subject_id.cmp(&right.subject_id))
            .then_with(|| left.predicate.cmp(&right.predicate))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    Ok(snapshot)
}

pub fn active_surface_ids(snapshot: &TemporalIdentitySnapshot) -> TemporalResult<Vec<String>> {
    let snapshot = finalize_identity_snapshot(snapshot.clone())?;
    let known_facts = known_facts(&snapshot);
    let suppressed = suppressed_fact_ids(&known_facts);
    let mut subject_ids = BTreeSet::new();
    for fact in &known_facts {
        if suppressed.contains(&fact.fact_id) {
            continue;
        }
        if !is_assertive(fact.assertion_status) {
            continue;
        }
        if !recorded_contains(&fact.recorded_time, &snapshot.snapshot.known_as_of) {
            continue;
        }
        subject_ids.insert(fact.subject_id.clone());
    }
    Ok(subject_ids.into_iter().collect())
}

pub fn canonical_explain_bytes(artifact: &TemporalExplainArtifact) -> TemporalResult<Vec<u8>> {
    let mut canonical = artifact.clone();
    canonical.snapshots.sort_by(|left, right| {
        left.snapshot
            .snapshot_id
            .cmp(&right.snapshot.snapshot_id)
            .then_with(|| left.snapshot.valid_at.cmp(&right.snapshot.valid_at))
            .then_with(|| left.snapshot.known_as_of.cmp(&right.snapshot.known_as_of))
    });
    canonical
        .timeline
        .sort_by(|left, right| left.event_id.cmp(&right.event_id));
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize temporal explain artifact: {error}"
        ))
    })
}

pub fn result_canonical_ids(result: &TemporalExactResult) -> Vec<String> {
    match result {
        TemporalExactResult::NoExactResult { .. } => Vec::new(),
        TemporalExactResult::SurfaceMapping { canonical_id, .. } => vec![canonical_id.clone()],
        TemporalExactResult::EntitySupport { canonical_id, .. } => vec![canonical_id.clone()],
        TemporalExactResult::Conflict { canonical_ids, .. } => canonical_ids.clone(),
    }
}

pub fn result_fact_ids(result: &TemporalExactResult) -> Vec<String> {
    match result {
        TemporalExactResult::NoExactResult { .. } => Vec::new(),
        TemporalExactResult::SurfaceMapping { fact_ids, .. }
        | TemporalExactResult::EntitySupport { fact_ids, .. }
        | TemporalExactResult::Conflict { fact_ids, .. } => fact_ids.clone(),
    }
}

pub fn canonical_type_from_id(canonical_id: &str) -> String {
    canonical_id
        .split_once(':')
        .map(|(prefix, _)| prefix)
        .filter(|prefix| !prefix.trim().is_empty())
        .unwrap_or("entity")
        .to_string()
}

pub fn recorded_contains(recorded_time: &RecordedTime, known_as_of: &str) -> bool {
    if let Some(start_at) = recorded_time.start_at.as_deref() {
        if known_as_of < start_at {
            return false;
        }
        if known_as_of == start_at
            && matches!(recorded_time.start_bound, IntervalBoundary::Exclusive)
        {
            return false;
        }
    }
    if let Some(end_at) = recorded_time.end_at.as_deref() {
        if known_as_of > end_at {
            return false;
        }
        if known_as_of == end_at && matches!(recorded_time.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
}

pub fn interval_contains(interval: &TimeInterval, at: &str) -> bool {
    if let Some(start_at) = interval.start_at.as_deref() {
        if at < start_at {
            return false;
        }
        if at == start_at && matches!(interval.start_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    if let Some(end_at) = interval.end_at.as_deref() {
        if at > end_at {
            return false;
        }
        if at == end_at && matches!(interval.end_bound, IntervalBoundary::Exclusive) {
            return false;
        }
    }
    true
}

fn finalize_explain_request(
    mut request: TemporalExplainRequest,
) -> TemporalResult<TemporalExplainRequest> {
    if request.version.trim().is_empty() {
        request.version = CANON_TEMPORAL_EXPLAIN_VERSION.to_string();
    }
    if request.version != CANON_TEMPORAL_EXPLAIN_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported temporal explain version: {}",
            request.version
        )));
    }
    request.subject = normalize_subject(request.subject)?;
    request.snapshots = request
        .snapshots
        .into_iter()
        .map(finalize_identity_snapshot)
        .collect::<TemporalResult<Vec<_>>>()?;
    request.snapshots.sort_by(|left, right| {
        left.snapshot
            .valid_at
            .cmp(&right.snapshot.valid_at)
            .then_with(|| left.snapshot.known_as_of.cmp(&right.snapshot.known_as_of))
            .then_with(|| left.snapshot.snapshot_id.cmp(&right.snapshot.snapshot_id))
    });
    Ok(request)
}

fn finalize_snapshot_reference(
    mut snapshot: TemporalSnapshotReference,
) -> TemporalResult<TemporalSnapshotReference> {
    snapshot.snapshot_id = normalized_non_empty(&snapshot.snapshot_id, "snapshot.snapshot_id")?;
    snapshot.registry_id = normalized_non_empty(&snapshot.registry_id, "snapshot.registry_id")?;
    snapshot.registry_version =
        normalized_non_empty(&snapshot.registry_version, "snapshot.registry_version")?;
    snapshot.compiled_snapshot_digest = normalized_hash(
        &snapshot.compiled_snapshot_digest,
        "snapshot.compiled_snapshot_digest",
    )?;
    snapshot.valid_at = canonical_timestamp(&snapshot.valid_at, "snapshot.valid_at")?;
    snapshot.known_as_of = canonical_timestamp(&snapshot.known_as_of, "snapshot.known_as_of")?;
    snapshot.policy_ref = normalized_non_empty(&snapshot.policy_ref, "snapshot.policy_ref")?;
    snapshot.policy_version =
        normalized_non_empty(&snapshot.policy_version, "snapshot.policy_version")?;
    Ok(snapshot)
}

fn finalize_relationship(
    mut relationship: TemporalRelationshipRef,
) -> TemporalResult<TemporalRelationshipRef> {
    relationship.relationship_id = normalized_non_empty(
        &relationship.relationship_id,
        "relationships.relationship_id",
    )?;
    relationship.subject_id =
        normalized_non_empty(&relationship.subject_id, "relationships.subject_id")?;
    relationship.predicate =
        normalized_non_empty(&relationship.predicate, "relationships.predicate")?;
    relationship.object_id =
        normalized_non_empty(&relationship.object_id, "relationships.object_id")?;
    relationship.valid_time =
        normalize_interval(relationship.valid_time, "relationships.valid_time")?;
    relationship.recorded_time =
        normalize_recorded_time(relationship.recorded_time, "relationships.recorded_time")?;
    relationship.source_locator = normalize_source_locator(relationship.source_locator)?;
    relationship.materialization_digest = normalized_hash(
        &relationship.materialization_digest,
        "relationships.materialization_digest",
    )?;
    relationship.scope = relationship.scope.map(normalize_scope).transpose()?;
    Ok(relationship)
}

fn normalize_subject(subject: TemporalExplainSubject) -> TemporalResult<TemporalExplainSubject> {
    match subject {
        TemporalExplainSubject::Surface { subject_id } => Ok(TemporalExplainSubject::Surface {
            subject_id: normalized_non_empty(&subject_id, "subject.subject_id")?,
        }),
        TemporalExplainSubject::CanonicalEntity { canonical_id } => {
            Ok(TemporalExplainSubject::CanonicalEntity {
                canonical_id: normalized_non_empty(&canonical_id, "subject.canonical_id")?,
            })
        }
    }
}

fn active_facts_for_subject(
    snapshot: &TemporalIdentitySnapshot,
    subject: &TemporalExplainSubject,
    known_facts: &[IdentityFact],
) -> Vec<IdentityFact> {
    let suppressed = suppressed_fact_ids(known_facts);
    let mut facts = known_facts
        .iter()
        .filter(|fact| !suppressed.contains(&fact.fact_id))
        .filter(|fact| is_assertive(fact.assertion_status))
        .filter(|fact| interval_contains(&fact.valid_time, &snapshot.snapshot.valid_at))
        .filter(|fact| match subject {
            TemporalExplainSubject::Surface { subject_id } => fact.subject_id == *subject_id,
            TemporalExplainSubject::CanonicalEntity { canonical_id } => {
                fact.object_id == *canonical_id
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    facts.sort_by(|left, right| {
        left.subject_id
            .cmp(&right.subject_id)
            .then_with(|| left.object_id.cmp(&right.object_id))
            .then_with(|| left.fact_id.cmp(&right.fact_id))
    });
    facts
}

fn active_relationships_for_subject(
    snapshot: &TemporalIdentitySnapshot,
    subject: &TemporalExplainSubject,
) -> TemporalResult<Vec<TemporalRelationshipRef>> {
    let mut relationships = snapshot
        .relationships
        .iter()
        .filter(|relationship| {
            recorded_contains(&relationship.recorded_time, &snapshot.snapshot.known_as_of)
        })
        .filter(|relationship| {
            interval_contains(&relationship.valid_time, &snapshot.snapshot.valid_at)
        })
        .filter(|relationship| match subject {
            TemporalExplainSubject::Surface { subject_id } => {
                relationship.subject_id == *subject_id || relationship.object_id == *subject_id
            }
            TemporalExplainSubject::CanonicalEntity { canonical_id } => {
                relationship.subject_id == *canonical_id || relationship.object_id == *canonical_id
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    relationships.sort_by(|left, right| left.relationship_id.cmp(&right.relationship_id));
    Ok(relationships)
}

fn minimal_causal_chain(
    all_facts: &[IdentityFact],
    active: &[IdentityFact],
    max_chain_facts: Option<usize>,
) -> Vec<TemporalCausalFactRef> {
    let active_ids = active
        .iter()
        .map(|fact| fact.fact_id.clone())
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeMap::<String, IdentityFact>::new();
    for fact in active {
        selected.insert(fact.fact_id.clone(), fact.clone());
    }
    for fact in all_facts {
        let references_active = fact
            .supersedes
            .iter()
            .chain(fact.retracts.iter())
            .any(|fact_id| active_ids.contains(fact_id));
        let referenced_by_active = active.iter().any(|active_fact| {
            active_fact.supersedes.contains(&fact.fact_id)
                || active_fact.retracts.contains(&fact.fact_id)
        });
        if references_active || referenced_by_active {
            selected.insert(fact.fact_id.clone(), fact.clone());
        }
    }
    let mut refs = selected
        .into_values()
        .map(causal_fact_ref)
        .collect::<Vec<_>>();
    refs.sort_by(|left, right| {
        left.recorded_time
            .start_at
            .cmp(&right.recorded_time.start_at)
            .then_with(|| left.fact_id.cmp(&right.fact_id))
    });
    if let Some(limit) = max_chain_facts {
        refs.truncate(limit);
    }
    refs
}

fn exact_result_for_subject(
    subject: &TemporalExplainSubject,
    active: &[IdentityFact],
) -> TemporalExactResult {
    if active.is_empty() {
        return TemporalExactResult::NoExactResult {
            reason: "no supplied assertion is active at this valid-time and known-time".to_string(),
        };
    }

    let canonical_ids = active
        .iter()
        .map(|fact| fact.object_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let fact_ids = active
        .iter()
        .map(|fact| fact.fact_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    match subject {
        TemporalExplainSubject::Surface { subject_id } => {
            if canonical_ids.len() == 1 {
                let canonical_id = canonical_ids[0].clone();
                TemporalExactResult::SurfaceMapping {
                    subject_id: subject_id.clone(),
                    canonical_type: canonical_type_from_id(&canonical_id),
                    canonical_id,
                    fact_ids,
                }
            } else {
                TemporalExactResult::Conflict {
                    subject_id: Some(subject_id.clone()),
                    canonical_ids,
                    fact_ids,
                }
            }
        }
        TemporalExplainSubject::CanonicalEntity { canonical_id } => {
            let subject_ids = active
                .iter()
                .map(|fact| fact.subject_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            TemporalExactResult::EntitySupport {
                canonical_id: canonical_id.clone(),
                subject_ids,
                fact_ids,
            }
        }
    }
}

fn build_timeline(
    subject: &TemporalExplainSubject,
    results: &[TemporalExplainSnapshotResult],
) -> TemporalResult<Vec<TemporalTimelineEvent>> {
    let mut events = Vec::new();
    let mut previous: Option<&TemporalExplainSnapshotResult> = None;
    for result in results {
        let class = previous
            .map(|previous| classify_snapshot_transition(previous, result))
            .unwrap_or_else(|| {
                if matches!(
                    result.exact_result,
                    TemporalExactResult::NoExactResult { .. }
                ) {
                    TemporalChangeClass::NoChange
                } else if matches!(result.exact_result, TemporalExactResult::Conflict { .. }) {
                    TemporalChangeClass::Conflict
                } else {
                    TemporalChangeClass::NewFact
                }
            });
        let fact_ids = result
            .causal_chain
            .iter()
            .map(|fact| fact.fact_id.clone())
            .collect::<Vec<_>>();
        let policy_refs = result
            .causal_chain
            .iter()
            .map(|fact| fact.trust_policy_ref.clone())
            .chain(std::iter::once(result.snapshot.policy_ref.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let relationship_ids = result
            .relationships
            .iter()
            .map(|relationship| relationship.relationship_id.clone())
            .collect::<Vec<_>>();
        let event_id = timeline_event_id(subject, &result.snapshot.snapshot_id, class, &fact_ids)?;
        events.push(TemporalTimelineEvent {
            event_id,
            change_class: class,
            snapshot_id: result.snapshot.snapshot_id.clone(),
            valid_at: result.snapshot.valid_at.clone(),
            known_as_of: result.snapshot.known_as_of.clone(),
            fact_ids,
            policy_refs,
            relationship_ids,
            message: timeline_message(class),
        });
        previous = Some(result);
    }
    events.sort_by(|left, right| {
        left.valid_at
            .cmp(&right.valid_at)
            .then_with(|| left.known_as_of.cmp(&right.known_as_of))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    Ok(events)
}

fn classify_snapshot_transition(
    before: &TemporalExplainSnapshotResult,
    after: &TemporalExplainSnapshotResult,
) -> TemporalChangeClass {
    if matches!(after.exact_result, TemporalExactResult::Conflict { .. }) {
        return TemporalChangeClass::Conflict;
    }
    if retraction_present(&before.causal_chain, &after.causal_chain) {
        return TemporalChangeClass::Retraction;
    }
    if correction_present(&before.causal_chain, &after.causal_chain) {
        return TemporalChangeClass::Correction;
    }

    let before_ids = result_canonical_ids(&before.exact_result);
    let after_ids = result_canonical_ids(&after.exact_result);
    if before_ids.is_empty() && !after_ids.is_empty() {
        return TemporalChangeClass::NewFact;
    }
    if !before_ids.is_empty() && after_ids.is_empty() {
        if before
            .causal_chain
            .iter()
            .all(|fact| !interval_contains(&fact.valid_time, &after.snapshot.valid_at))
        {
            return TemporalChangeClass::ExpiredFact;
        }
        return TemporalChangeClass::Retraction;
    }
    if before_ids != after_ids {
        return TemporalChangeClass::CanonicalRemap;
    }
    if scope_set(&before.causal_chain) != scope_set(&after.causal_chain) {
        return TemporalChangeClass::ScopeChange;
    }
    if before.snapshot.policy_ref != after.snapshot.policy_ref
        || before.snapshot.policy_version != after.snapshot.policy_version
    {
        return TemporalChangeClass::PolicyChange;
    }
    TemporalChangeClass::NoChange
}

fn retraction_present(
    before_chain: &[TemporalCausalFactRef],
    after_chain: &[TemporalCausalFactRef],
) -> bool {
    let before_ids = before_chain
        .iter()
        .map(|fact| fact.fact_id.as_str())
        .collect::<BTreeSet<_>>();
    after_chain.iter().any(|fact| {
        matches!(fact.assertion_status, AssertionStatus::Retracted)
            && fact
                .retracts
                .iter()
                .any(|fact_id| before_ids.contains(fact_id.as_str()))
    })
}

fn correction_present(
    before_chain: &[TemporalCausalFactRef],
    after_chain: &[TemporalCausalFactRef],
) -> bool {
    let before_ids = before_chain
        .iter()
        .map(|fact| fact.fact_id.as_str())
        .collect::<BTreeSet<_>>();
    after_chain.iter().any(|fact| {
        fact.supersedes
            .iter()
            .any(|fact_id| before_ids.contains(fact_id.as_str()))
    })
}

fn suppressed_fact_ids(facts: &[IdentityFact]) -> BTreeSet<String> {
    let mut suppressed = BTreeSet::new();
    for fact in facts {
        suppressed.extend(fact.supersedes.iter().cloned());
        if matches!(fact.assertion_status, AssertionStatus::Retracted) {
            suppressed.extend(fact.retracts.iter().cloned());
        }
    }
    suppressed
}

fn known_facts(snapshot: &TemporalIdentitySnapshot) -> Vec<IdentityFact> {
    snapshot
        .facts
        .iter()
        .filter(|fact| recorded_contains(&fact.recorded_time, &snapshot.snapshot.known_as_of))
        .cloned()
        .collect()
}

fn causal_fact_ref(fact: IdentityFact) -> TemporalCausalFactRef {
    TemporalCausalFactRef {
        fact_id: fact.fact_id,
        subject_id: fact.subject_id,
        predicate: fact.predicate,
        object_id: fact.object_id,
        valid_time: fact.valid_time,
        recorded_time: fact.recorded_time,
        assertion_status: fact.assertion_status,
        source_locator: fact.source_locator,
        trust_policy_ref: fact.trust_policy_ref,
        materialization_digest: fact.materialization_digest,
        scope: fact.scope,
        supersedes: fact.supersedes,
        retracts: fact.retracts,
    }
}

fn is_assertive(status: AssertionStatus) -> bool {
    matches!(
        status,
        AssertionStatus::Asserted | AssertionStatus::Accepted
    )
}

fn scope_set(chain: &[TemporalCausalFactRef]) -> Vec<Option<FactScope>> {
    let mut scopes = chain
        .iter()
        .map(|fact| fact.scope.clone())
        .collect::<Vec<_>>();
    scopes.sort_by(compare_optional_scope);
    scopes.dedup();
    scopes
}

fn compare_optional_scope(
    left: &Option<FactScope>,
    right: &Option<FactScope>,
) -> std::cmp::Ordering {
    match (left, right) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => compare_scope(left, right),
    }
}

fn compare_scope(left: &FactScope, right: &FactScope) -> std::cmp::Ordering {
    left.scope_type
        .cmp(&right.scope_type)
        .then_with(|| left.scope_id.cmp(&right.scope_id))
}

fn timeline_event_id(
    subject: &TemporalExplainSubject,
    snapshot_id: &str,
    class: TemporalChangeClass,
    fact_ids: &[String],
) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct EventKey<'a> {
        subject: &'a TemporalExplainSubject,
        snapshot_id: &'a str,
        class: TemporalChangeClass,
        fact_ids: &'a [String],
    }
    hash_struct(
        &EventKey {
            subject,
            snapshot_id,
            class,
            fact_ids,
        },
        "temporal timeline event id",
    )
}

fn timeline_message(class: TemporalChangeClass) -> String {
    match class {
        TemporalChangeClass::NewFact => "a supplied assertion became active".to_string(),
        TemporalChangeClass::ExpiredFact => {
            "the prior assertion is outside the later valid-time window".to_string()
        }
        TemporalChangeClass::Correction => {
            "a later-known assertion supersedes an earlier assertion".to_string()
        }
        TemporalChangeClass::Retraction => {
            "a later-known assertion retracts an earlier assertion".to_string()
        }
        TemporalChangeClass::PolicyChange => {
            "the compiled policy reference changed without changing the exact result".to_string()
        }
        TemporalChangeClass::ScopeChange => "the supporting assertion scope changed".to_string(),
        TemporalChangeClass::Conflict => {
            "active supplied assertions point to conflicting canonical IDs".to_string()
        }
        TemporalChangeClass::CanonicalRemap => {
            "the exact result maps to a different canonical ID".to_string()
        }
        TemporalChangeClass::NoChange => {
            "the exact result is unchanged for this compiled snapshot".to_string()
        }
    }
}

fn normalize_interval(mut interval: TimeInterval, field: &str) -> TemporalResult<TimeInterval> {
    interval.start_at = canonical_optional_timestamp(interval.start_at.take(), field, "start_at")?;
    interval.end_at = canonical_optional_timestamp(interval.end_at.take(), field, "end_at")?;
    if interval.start_at.is_none() {
        interval.start_bound = IntervalBoundary::Open;
    }
    if interval.end_at.is_none() {
        interval.end_bound = IntervalBoundary::Open;
    }
    Ok(interval)
}

fn normalize_recorded_time(
    mut recorded_time: RecordedTime,
    field: &str,
) -> TemporalResult<RecordedTime> {
    recorded_time.start_at =
        canonical_optional_timestamp(recorded_time.start_at.take(), field, "start_at")?;
    recorded_time.end_at =
        canonical_optional_timestamp(recorded_time.end_at.take(), field, "end_at")?;
    if recorded_time.start_at.is_none() {
        recorded_time.start_bound = IntervalBoundary::Open;
    }
    if recorded_time.end_at.is_none() {
        recorded_time.end_bound = IntervalBoundary::Open;
    }
    Ok(recorded_time)
}

fn normalize_source_locator(mut locator: SourceLocator) -> TemporalResult<SourceLocator> {
    locator.source_system =
        normalized_non_empty(&locator.source_system, "source_locator.source_system")?;
    locator.locator = normalized_non_empty(&locator.locator, "source_locator.locator")?;
    locator.fragment = locator
        .fragment
        .take()
        .map(|fragment| fragment.trim().to_string())
        .filter(|fragment| !fragment.is_empty());
    Ok(locator)
}

fn normalize_scope(mut scope: FactScope) -> TemporalResult<FactScope> {
    scope.scope_type = normalized_non_empty(&scope.scope_type, "scope.scope_type")?;
    scope.scope_id = normalized_non_empty(&scope.scope_id, "scope.scope_id")?;
    Ok(scope)
}

fn canonical_optional_timestamp(
    value: Option<String>,
    field: &str,
    part: &str,
) -> TemporalResult<Option<String>> {
    value
        .map(|timestamp| canonical_timestamp(&timestamp, &format!("{field}.{part}")))
        .transpose()
}

fn canonical_timestamp(value: &str, field: &str) -> TemporalResult<String> {
    let parsed = DateTime::parse_from_rfc3339(value.trim()).map_err(|error| {
        artifact_contract_error(format!("{field} must be RFC3339 timestamp: {error}"))
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn normalized_non_empty(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!("{field} is required")));
    }
    Ok(normalized)
}

fn normalized_hash(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some(hex) = normalized.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<hex> encoding"
        )));
    };
    if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(artifact_contract_error(format!(
            "{field} must contain a 64-character hex digest"
        )));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}

fn hash_struct(value: &impl Serialize, label: &str) -> TemporalResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {label}: {error}"))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::ArtifactContract, message)
}
