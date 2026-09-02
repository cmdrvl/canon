// Deterministic temporal identity snapshot diffs.

use super::explain::{
    TemporalCausalFactRef, TemporalChangeClass, TemporalExactResult, TemporalIdentitySnapshot,
    TemporalSnapshotReference, active_surface_ids, canonical_type_from_id, explain_snapshot_result,
    finalize_identity_snapshot, interval_contains, result_canonical_ids,
};
use super::{FactScope, TemporalError, TemporalErrorCode, TemporalResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_TEMPORAL_DIFF_VERSION: &str = "canon.temporal.diff.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalDiffRequest {
    pub version: String,
    pub before: TemporalIdentitySnapshot,
    pub after: TemporalIdentitySnapshot,
    #[serde(default)]
    pub filter: TemporalDiffFilter,
    #[serde(default)]
    pub page: TemporalDiffPageRequest,
    #[serde(default)]
    pub include_unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporalDiffFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<FactScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_systems: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_classes: Vec<TemporalChangeClass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalDiffPageRequest {
    #[serde(default = "default_page_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_cursor: Option<String>,
}

impl Default for TemporalDiffPageRequest {
    fn default() -> Self {
        Self {
            limit: default_page_limit(),
            after_cursor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalDiffArtifact {
    pub version: String,
    pub before_snapshot: TemporalSnapshotReference,
    pub after_snapshot: TemporalSnapshotReference,
    pub summary: TemporalDiffSummary,
    pub page: TemporalDiffPage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<TemporalDiffChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporalDiffSummary {
    pub compared_subject_count: usize,
    pub changed_subject_count: usize,
    pub total_matching_change_count: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub by_change_class: BTreeMap<TemporalChangeClass, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalDiffPage {
    pub limit: usize,
    pub returned: usize,
    pub total_matching: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalDiffChange {
    pub change_id: String,
    pub change_class: TemporalChangeClass,
    pub subject_id: String,
    pub entity_type: String,
    pub before: TemporalExactResult,
    pub after: TemporalExactResult,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causal_chain: Vec<TemporalCausalFactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_systems: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<FactScope>,
    pub message: String,
}

pub fn diff_temporal_snapshots(
    request: TemporalDiffRequest,
) -> TemporalResult<TemporalDiffArtifact> {
    let request = finalize_diff_request(request)?;
    let subject_ids = compared_subject_ids(&request.before, &request.after)?;
    let mut all_changes = Vec::new();
    for subject_id in &subject_ids {
        let subject = super::explain::TemporalExplainSubject::Surface {
            subject_id: subject_id.clone(),
        };
        let before = explain_snapshot_result(&request.before, &subject, None)?;
        let after = explain_snapshot_result(&request.after, &subject, None)?;
        let Some(change_class) = classify_change(&request.before, &request.after, &before, &after)
        else {
            continue;
        };
        if change_class == TemporalChangeClass::NoChange && !request.include_unchanged {
            continue;
        }

        let mut causal_chain = before.causal_chain.clone();
        causal_chain.extend(after.causal_chain.clone());
        causal_chain.sort_by(|left, right| left.fact_id.cmp(&right.fact_id));
        causal_chain.dedup_by(|left, right| left.fact_id == right.fact_id);

        let canonical_ids = result_canonical_ids(&after.exact_result);
        let fallback_ids = result_canonical_ids(&before.exact_result);
        let entity_type = canonical_ids
            .first()
            .or_else(|| fallback_ids.first())
            .map(|id| canonical_type_from_id(id))
            .unwrap_or_else(|| "entity".to_string());
        let source_systems = source_systems(&causal_chain);
        let scopes = scopes(&causal_chain);
        let policy_refs = policy_refs(
            &request.before.snapshot,
            &request.after.snapshot,
            &causal_chain,
        );
        let change_id = change_id(
            &request.before.snapshot.compiled_snapshot_digest,
            &request.after.snapshot.compiled_snapshot_digest,
            subject_id,
            change_class,
            &before.exact_result,
            &after.exact_result,
        )?;
        let change = TemporalDiffChange {
            change_id,
            change_class,
            subject_id: subject_id.clone(),
            entity_type,
            before: before.exact_result,
            after: after.exact_result,
            causal_chain,
            policy_refs,
            source_systems,
            scopes,
            message: change_message(change_class),
        };
        if filter_matches(&request.filter, &change) {
            all_changes.push(change);
        }
    }

    all_changes.sort_by(|left, right| {
        left.subject_id
            .cmp(&right.subject_id)
            .then_with(|| left.change_class.cmp(&right.change_class))
            .then_with(|| left.change_id.cmp(&right.change_id))
    });

    let mut summary = TemporalDiffSummary {
        compared_subject_count: subject_ids.len(),
        changed_subject_count: all_changes
            .iter()
            .map(|change| change.subject_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        total_matching_change_count: all_changes.len(),
        by_change_class: BTreeMap::new(),
    };
    for change in &all_changes {
        *summary
            .by_change_class
            .entry(change.change_class)
            .or_insert(0) += 1;
    }

    let paged = paginate_changes(all_changes, &request.page)?;
    Ok(TemporalDiffArtifact {
        version: CANON_TEMPORAL_DIFF_VERSION.to_string(),
        before_snapshot: request.before.snapshot,
        after_snapshot: request.after.snapshot,
        summary,
        page: TemporalDiffPage {
            limit: request.page.limit,
            returned: paged.items.len(),
            total_matching: paged.total_matching,
            next_cursor: paged.next_cursor,
        },
        changes: paged.items,
    })
}

pub fn canonical_diff_bytes(artifact: &TemporalDiffArtifact) -> TemporalResult<Vec<u8>> {
    let mut canonical = artifact.clone();
    canonical.changes.sort_by(|left, right| {
        left.subject_id
            .cmp(&right.subject_id)
            .then_with(|| left.change_class.cmp(&right.change_class))
            .then_with(|| left.change_id.cmp(&right.change_id))
    });
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize temporal diff artifact: {error}"
        ))
    })
}

fn finalize_diff_request(mut request: TemporalDiffRequest) -> TemporalResult<TemporalDiffRequest> {
    if request.version.trim().is_empty() {
        request.version = CANON_TEMPORAL_DIFF_VERSION.to_string();
    }
    if request.version != CANON_TEMPORAL_DIFF_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported temporal diff version: {}",
            request.version
        )));
    }
    request.before = finalize_identity_snapshot(request.before)?;
    request.after = finalize_identity_snapshot(request.after)?;
    request.filter = normalize_filter(request.filter)?;
    if request.page.limit == 0 {
        return Err(artifact_contract_error(
            "page.limit must be greater than zero",
        ));
    }
    if let Some(cursor) = &request.page.after_cursor {
        normalized_non_empty(cursor, "page.after_cursor")?;
    }
    Ok(request)
}

fn normalize_filter(mut filter: TemporalDiffFilter) -> TemporalResult<TemporalDiffFilter> {
    filter.entity_types = normalize_strings(filter.entity_types, "filter.entity_types")?;
    filter.source_systems = normalize_strings(filter.source_systems, "filter.source_systems")?;
    filter.scopes = filter
        .scopes
        .into_iter()
        .map(normalize_scope)
        .collect::<TemporalResult<Vec<_>>>()?;
    sort_scopes(&mut filter.scopes);
    filter.scopes.dedup();
    filter.change_classes.sort();
    filter.change_classes.dedup();
    Ok(filter)
}

fn compared_subject_ids(
    before: &TemporalIdentitySnapshot,
    after: &TemporalIdentitySnapshot,
) -> TemporalResult<Vec<String>> {
    let mut subjects = BTreeSet::new();
    subjects.extend(active_surface_ids(before)?);
    subjects.extend(active_surface_ids(after)?);
    subjects.extend(before.facts.iter().map(|fact| fact.subject_id.clone()));
    subjects.extend(after.facts.iter().map(|fact| fact.subject_id.clone()));
    Ok(subjects.into_iter().collect())
}

fn classify_change(
    before_snapshot: &TemporalIdentitySnapshot,
    after_snapshot: &TemporalIdentitySnapshot,
    before: &super::explain::TemporalExplainSnapshotResult,
    after: &super::explain::TemporalExplainSnapshotResult,
) -> Option<TemporalChangeClass> {
    if matches!(after.exact_result, TemporalExactResult::Conflict { .. }) {
        return Some(TemporalChangeClass::Conflict);
    }
    if retraction_present(&before.causal_chain, &after.causal_chain) {
        return Some(TemporalChangeClass::Retraction);
    }
    if correction_present(&before.causal_chain, &after.causal_chain) {
        return Some(TemporalChangeClass::Correction);
    }

    let before_ids = result_canonical_ids(&before.exact_result);
    let after_ids = result_canonical_ids(&after.exact_result);
    if before_ids.is_empty() && after_ids.is_empty() {
        if policy_changed(before_snapshot, after_snapshot) {
            return Some(TemporalChangeClass::PolicyChange);
        }
        return Some(TemporalChangeClass::NoChange);
    }
    if before_ids.is_empty() && !after_ids.is_empty() {
        return Some(TemporalChangeClass::NewFact);
    }
    if !before_ids.is_empty() && after_ids.is_empty() {
        if before
            .causal_chain
            .iter()
            .all(|fact| !interval_contains(&fact.valid_time, &after_snapshot.snapshot.valid_at))
        {
            return Some(TemporalChangeClass::ExpiredFact);
        }
        return Some(TemporalChangeClass::Retraction);
    }
    if before_ids != after_ids {
        return Some(TemporalChangeClass::CanonicalRemap);
    }
    if scopes(&before.causal_chain) != scopes(&after.causal_chain) {
        return Some(TemporalChangeClass::ScopeChange);
    }
    if policy_changed(before_snapshot, after_snapshot) {
        return Some(TemporalChangeClass::PolicyChange);
    }
    Some(TemporalChangeClass::NoChange)
}

fn policy_changed(
    before_snapshot: &TemporalIdentitySnapshot,
    after_snapshot: &TemporalIdentitySnapshot,
) -> bool {
    before_snapshot.snapshot.policy_ref != after_snapshot.snapshot.policy_ref
        || before_snapshot.snapshot.policy_version != after_snapshot.snapshot.policy_version
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
        fact.assertion_status == super::AssertionStatus::Retracted
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

fn filter_matches(filter: &TemporalDiffFilter, change: &TemporalDiffChange) -> bool {
    if !filter.change_classes.is_empty() && !filter.change_classes.contains(&change.change_class) {
        return false;
    }
    if !filter.entity_types.is_empty() && !filter.entity_types.contains(&change.entity_type) {
        return false;
    }
    if !filter.source_systems.is_empty()
        && change
            .source_systems
            .iter()
            .all(|source| !filter.source_systems.contains(source))
    {
        return false;
    }
    if !filter.scopes.is_empty()
        && change
            .scopes
            .iter()
            .all(|scope| !filter.scopes.contains(scope))
    {
        return false;
    }
    true
}

struct PagedChanges {
    items: Vec<TemporalDiffChange>,
    total_matching: usize,
    next_cursor: Option<String>,
}

fn paginate_changes(
    changes: Vec<TemporalDiffChange>,
    page: &TemporalDiffPageRequest,
) -> TemporalResult<PagedChanges> {
    let start = page
        .after_cursor
        .as_ref()
        .and_then(|cursor| {
            changes
                .iter()
                .position(|change| change.change_id == *cursor)
                .map(|index| index + 1)
        })
        .unwrap_or(0);
    let total_matching = changes.len();
    let items = changes
        .into_iter()
        .skip(start)
        .take(page.limit)
        .collect::<Vec<_>>();
    let next_cursor = if start + items.len() < total_matching {
        items.last().map(|change| change.change_id.clone())
    } else {
        None
    };
    Ok(PagedChanges {
        items,
        total_matching,
        next_cursor,
    })
}

fn source_systems(chain: &[TemporalCausalFactRef]) -> Vec<String> {
    chain
        .iter()
        .map(|fact| fact.source_locator.source_system.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn scopes(chain: &[TemporalCausalFactRef]) -> Vec<FactScope> {
    let mut scopes = chain
        .iter()
        .filter_map(|fact| fact.scope.clone())
        .collect::<Vec<_>>();
    sort_scopes(&mut scopes);
    scopes.dedup();
    scopes
}

fn policy_refs(
    before: &TemporalSnapshotReference,
    after: &TemporalSnapshotReference,
    chain: &[TemporalCausalFactRef],
) -> Vec<String> {
    chain
        .iter()
        .map(|fact| fact.trust_policy_ref.clone())
        .chain([before.policy_ref.clone(), after.policy_ref.clone()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn change_id(
    before_digest: &str,
    after_digest: &str,
    subject_id: &str,
    change_class: TemporalChangeClass,
    before: &TemporalExactResult,
    after: &TemporalExactResult,
) -> TemporalResult<String> {
    #[derive(Serialize)]
    struct ChangeKey<'a> {
        before_digest: &'a str,
        after_digest: &'a str,
        subject_id: &'a str,
        change_class: TemporalChangeClass,
        before: &'a TemporalExactResult,
        after: &'a TemporalExactResult,
    }
    let bytes = serde_json::to_vec(&ChangeKey {
        before_digest,
        after_digest,
        subject_id,
        change_class,
        before,
        after,
    })
    .map_err(|error| artifact_contract_error(format!("failed to serialize change id: {error}")))?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn change_message(change_class: TemporalChangeClass) -> String {
    match change_class {
        TemporalChangeClass::NewFact => "an exact result appears in the later snapshot".to_string(),
        TemporalChangeClass::ExpiredFact => {
            "the earlier exact result is outside the later valid-time window".to_string()
        }
        TemporalChangeClass::Correction => {
            "the later snapshot knows a superseding assertion".to_string()
        }
        TemporalChangeClass::Retraction => {
            "the later snapshot knows a retraction assertion".to_string()
        }
        TemporalChangeClass::PolicyChange => "the compiled policy reference changed".to_string(),
        TemporalChangeClass::ScopeChange => "the supporting assertion scope changed".to_string(),
        TemporalChangeClass::Conflict => {
            "the later snapshot has conflicting active exact assertions".to_string()
        }
        TemporalChangeClass::CanonicalRemap => {
            "the exact result maps to a different canonical ID".to_string()
        }
        TemporalChangeClass::NoChange => "the exact result is unchanged".to_string(),
    }
}

fn normalize_strings(values: Vec<String>, field: &str) -> TemporalResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<TemporalResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_scope(mut scope: FactScope) -> TemporalResult<FactScope> {
    scope.scope_type = normalized_non_empty(&scope.scope_type, "filter.scopes.scope_type")?;
    scope.scope_id = normalized_non_empty(&scope.scope_id, "filter.scopes.scope_id")?;
    Ok(scope)
}

fn sort_scopes(scopes: &mut [FactScope]) {
    scopes.sort_by(|left, right| {
        left.scope_type
            .cmp(&right.scope_type)
            .then_with(|| left.scope_id.cmp(&right.scope_id))
    });
}

fn normalized_non_empty(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!("{field} is required")));
    }
    Ok(normalized)
}

fn default_page_limit() -> usize {
    100
}

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError::new(TemporalErrorCode::ArtifactContract, message)
}
