#![forbid(unsafe_code)]

//! Deterministic unresolved-coverage evidence.
//!
//! This contract is intentionally not a registry surface. It records missing
//! coverage, abstentions, and rejected candidates so later workflow beads can
//! ingest the evidence without making identity assertions.

pub mod cli;
pub mod context;
pub mod group;
pub mod rank;
mod types;

pub use types::*;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub fn finalize_artifact(
    mut artifact: UnresolvedInboxArtifact,
) -> InboxResult<UnresolvedInboxArtifact> {
    if artifact.version.trim().is_empty() {
        artifact.version = CANON_UNRESOLVED_INBOX_VERSION.to_string();
    }
    if artifact.version != CANON_UNRESOLVED_INBOX_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported inbox artifact version: {}",
            artifact.version
        )));
    }

    normalize_policy(&artifact.policy, artifact.view)?;

    let mut items = Vec::with_capacity(artifact.items.len());
    for item in artifact.items {
        items.push(normalize_item(item, &artifact.policy, artifact.view)?);
    }
    items.sort_by(item_cmp);

    let mut merged: Vec<UnresolvedInboxItem> = Vec::new();
    for item in items {
        if let Some(last) = merged.last_mut()
            && last.event_key == item.event_key
        {
            merge_item(last, item, artifact.view)?;
        } else {
            merged.push(item);
        }
    }

    artifact.items = merged;
    artifact.summary = build_summary(&artifact.items);
    artifact.artifact_content_hash.clear();
    artifact.artifact_content_hash = hash_without_self(&artifact)?;
    Ok(artifact)
}

pub fn merge_artifacts(
    artifacts: impl IntoIterator<Item = UnresolvedInboxArtifact>,
) -> InboxResult<UnresolvedInboxArtifact> {
    let mut iter = artifacts.into_iter();
    let Some(first) = iter.next() else {
        return finalize_artifact(UnresolvedInboxArtifact::default());
    };
    let first = finalize_artifact(first)?;
    let mut merged = first.clone();
    merged.artifact_content_hash.clear();
    merged.summary = InboxSummary::default();

    let policy = first.policy.clone();
    let view = first.view;
    let version = first.version.clone();
    let mut items = first.items;

    for artifact in iter {
        let artifact = finalize_artifact(artifact)?;
        if artifact.version != version {
            return Err(privacy_policy_error(
                "cannot merge inbox shards with different versions",
            ));
        }
        if artifact.policy != policy {
            return Err(privacy_policy_error(
                "cannot merge inbox shards with different privacy policies",
            ));
        }
        if artifact.view != view {
            return Err(privacy_policy_error(
                "cannot merge inbox shards with different export views",
            ));
        }
        items.extend(artifact.items);
    }

    merged.items = items;
    finalize_artifact(merged)
}

pub fn export_artifact(
    artifact: &UnresolvedInboxArtifact,
    view: InboxExportMode,
) -> InboxResult<UnresolvedInboxArtifact> {
    let mut export = finalize_artifact(artifact.clone())?;

    if matches!(view, InboxExportMode::Retained)
        && !matches!(
            export.policy.raw_value_retention,
            RawValueRetention::ExternalReference
        )
    {
        return Err(privacy_policy_error(
            "retained export requires external-reference raw retention",
        ));
    }
    if matches!(view, InboxExportMode::Retained)
        && !matches!(export.view, InboxExportMode::Retained)
    {
        return Err(privacy_policy_error(
            "retained export cannot be reconstructed from a non-retained materialization",
        ));
    }

    export.view = view;
    for item in &mut export.items {
        match view {
            InboxExportMode::Retained => {
                item.raw_values_redacted = false;
            }
            InboxExportMode::Redacted => {
                if !item.raw_values.is_empty() {
                    item.raw_values.clear();
                    item.raw_values_redacted = true;
                }
            }
            InboxExportMode::FingerprintsOnly => {
                item.raw_values.clear();
                item.raw_values_redacted = false;
            }
        }
    }

    finalize_artifact(export)
}

pub fn canonical_json_bytes(artifact: &UnresolvedInboxArtifact) -> InboxResult<Vec<u8>> {
    let artifact = finalize_artifact(artifact.clone())?;
    serde_json::to_vec(&artifact).map_err(|error| {
        artifact_contract_error(format!("failed to serialize inbox artifact: {error}"))
    })
}

fn normalize_policy(policy: &InboxPrivacyPolicy, view: InboxExportMode) -> InboxResult<()> {
    if policy.policy_id.trim().is_empty() {
        return Err(artifact_contract_error(
            "inbox privacy policy requires a non-empty policy_id",
        ));
    }
    if matches!(policy.raw_value_retention, RawValueRetention::Omit)
        && matches!(view, InboxExportMode::Retained)
    {
        return Err(privacy_policy_error(
            "retained inbox view requires external-reference raw retention",
        ));
    }
    Ok(())
}

fn normalize_item(
    mut item: UnresolvedInboxItem,
    policy: &InboxPrivacyPolicy,
    view: InboxExportMode,
) -> InboxResult<UnresolvedInboxItem> {
    if item.field_name.trim().is_empty() {
        return Err(artifact_contract_error(
            "inbox items require a non-empty field_name",
        ));
    }
    if item.surface_fingerprints.is_empty() {
        return Err(artifact_contract_error(
            "inbox items require at least one normalized surface fingerprint",
        ));
    }
    if item.occurrences.is_empty() {
        return Err(artifact_contract_error(
            "inbox items require at least one source/project/run occurrence reference",
        ));
    }
    if let Some(profile_ref) = &item.profile_ref
        && (profile_ref.profile_id.trim().is_empty()
            || profile_ref.profile_version.trim().is_empty())
    {
        return Err(artifact_contract_error(
            "profile_ref requires non-empty profile_id and profile_version",
        ));
    }

    for fingerprint in &mut item.surface_fingerprints {
        fingerprint.normalizer_id = fingerprint.normalizer_id.trim().to_string();
        fingerprint.surface_role = fingerprint.surface_role.trim().to_string();
        fingerprint.fingerprint = normalized_hash(&fingerprint.fingerprint, "surface_fingerprint")?;
        if fingerprint.normalizer_id.is_empty() || fingerprint.surface_role.is_empty() {
            return Err(artifact_contract_error(
                "surface fingerprints require non-empty normalizer_id and surface_role",
            ));
        }
    }
    item.surface_fingerprints.sort_by(fingerprint_cmp);
    item.surface_fingerprints.dedup();

    for hint in &mut item.namespace_hints {
        hint.namespace = hint.namespace.trim().to_string();
        hint.source = hint.source.trim().to_string();
        if hint.namespace.is_empty() || hint.source.is_empty() {
            return Err(artifact_contract_error(
                "namespace hints require non-empty namespace and source",
            ));
        }
    }
    item.namespace_hints.sort_by(namespace_hint_cmp);
    item.namespace_hints.dedup();

    item.candidate_summary.rejection_reasons = item
        .candidate_summary
        .rejection_reasons
        .into_iter()
        .map(|reason| reason.trim().to_string())
        .filter(|reason| !reason.is_empty())
        .collect();
    item.candidate_summary.rejection_reasons.sort();
    item.candidate_summary.rejection_reasons.dedup();
    item.candidate_summary.best_score_band = item
        .candidate_summary
        .best_score_band
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(scope) = &mut item.temporal_scope {
        scope.start_at = canonical_optional_timestamp(scope.start_at.take())?;
        scope.end_at = canonical_optional_timestamp(scope.end_at.take())?;
        if let (Some(start), Some(end)) = (&scope.start_at, &scope.end_at)
            && start > end
        {
            return Err(artifact_contract_error(
                "temporal_scope.start_at must be <= temporal_scope.end_at",
            ));
        }
    }

    for occurrence in &mut item.occurrences {
        occurrence.project_ref = occurrence.project_ref.trim().to_string();
        occurrence.run_ref = occurrence.run_ref.trim().to_string();
        occurrence.source_ref = occurrence.source_ref.trim().to_string();
        occurrence.record_ref = occurrence
            .record_ref
            .take()
            .map(|record_ref| record_ref.trim().to_string())
            .filter(|record_ref| !record_ref.is_empty());
        occurrence.seen_at = canonical_timestamp(&occurrence.seen_at, "occurrence.seen_at")?;
        if occurrence.project_ref.is_empty()
            || occurrence.run_ref.is_empty()
            || occurrence.source_ref.is_empty()
        {
            return Err(artifact_contract_error(
                "occurrences require non-empty project_ref, run_ref, and source_ref",
            ));
        }
    }
    item.occurrences.sort_by(occurrence_cmp);
    item.occurrences.dedup();

    set_occurrence_bounds(&mut item);
    item.occurrence_summary = build_occurrence_summary(&item.occurrences);

    for raw_value in &mut item.raw_values {
        raw_value.store = raw_value.store.trim().to_string();
        raw_value.locator = raw_value.locator.trim().to_string();
        raw_value.content_hash =
            normalized_hash(&raw_value.content_hash, "raw_value.content_hash")?;
        if raw_value.store.is_empty() || raw_value.locator.is_empty() {
            return Err(corrupt_reference_error(
                "raw value references require non-empty store and locator",
            ));
        }
    }
    item.raw_values.sort_by(raw_value_cmp);
    item.raw_values.dedup();

    match policy.raw_value_retention {
        RawValueRetention::Omit => {
            if !item.raw_values.is_empty() || item.raw_values_redacted {
                return Err(privacy_policy_error(
                    "omit policy forbids raw value references and redaction markers",
                ));
            }
        }
        RawValueRetention::ExternalReference => match view {
            InboxExportMode::Retained => {
                if item.raw_values_redacted {
                    return Err(privacy_policy_error(
                        "retained view cannot carry redaction markers",
                    ));
                }
            }
            InboxExportMode::Redacted => {
                if !item.raw_values.is_empty() {
                    return Err(privacy_policy_error(
                        "redacted view cannot carry retained raw value references",
                    ));
                }
            }
            InboxExportMode::FingerprintsOnly => {
                if !item.raw_values.is_empty() || item.raw_values_redacted {
                    return Err(privacy_policy_error(
                        "fingerprints-only view must omit raw references and redaction markers",
                    ));
                }
            }
        },
    }

    item.event_key = compute_event_key(&item)?;
    Ok(item)
}

fn merge_item(
    existing: &mut UnresolvedInboxItem,
    incoming: UnresolvedInboxItem,
    view: InboxExportMode,
) -> InboxResult<()> {
    if existing.event_key != incoming.event_key {
        return Err(artifact_contract_error(
            "cannot merge inbox items with different event keys",
        ));
    }

    existing.namespace_hints.extend(incoming.namespace_hints);
    existing.namespace_hints.sort_by(namespace_hint_cmp);
    existing.namespace_hints.dedup();

    existing.occurrences.extend(incoming.occurrences);
    existing.occurrences.sort_by(occurrence_cmp);
    existing.occurrences.dedup();

    existing.raw_values.extend(incoming.raw_values);
    existing.raw_values.sort_by(raw_value_cmp);
    existing.raw_values.dedup();

    existing.raw_values_redacted |= incoming.raw_values_redacted;
    existing.privacy_class = match (existing.privacy_class, incoming.privacy_class) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };
    existing.candidate_summary = merge_candidate_summary(
        existing.candidate_summary.clone(),
        incoming.candidate_summary,
    );
    existing.temporal_scope =
        merge_temporal_scope(existing.temporal_scope.take(), incoming.temporal_scope);

    set_occurrence_bounds(existing);
    existing.occurrence_summary = build_occurrence_summary(&existing.occurrences);

    if matches!(view, InboxExportMode::FingerprintsOnly) {
        existing.raw_values.clear();
        existing.raw_values_redacted = false;
    }

    Ok(())
}

fn build_summary(items: &[UnresolvedInboxItem]) -> InboxSummary {
    let mut summary = InboxSummary {
        total_items: items.len() as u64,
        total_occurrences: items
            .iter()
            .map(|item| item.occurrence_summary.total_occurrences)
            .sum(),
        redacted_items: items.iter().filter(|item| item.raw_values_redacted).count() as u64,
        retained_raw_reference_count: items.iter().map(|item| item.raw_values.len() as u64).sum(),
        by_reason_code: BTreeMap::new(),
        by_event_kind: BTreeMap::new(),
        by_privacy_class: BTreeMap::new(),
    };

    for item in items {
        *summary
            .by_reason_code
            .entry(enum_name(item.reason_code))
            .or_default() += 1;
        *summary
            .by_event_kind
            .entry(enum_name(item.event_kind))
            .or_default() += 1;
        if let Some(privacy_class) = item.privacy_class {
            *summary
                .by_privacy_class
                .entry(enum_name(privacy_class))
                .or_default() += 1;
        }
    }

    summary
}

fn compute_event_key(item: &UnresolvedInboxItem) -> InboxResult<String> {
    #[derive(Serialize)]
    struct EventKey<'a> {
        version: &'static str,
        event_kind: &'a InboxEventKind,
        reason_code: &'a InboxReasonCode,
        field_name: &'a str,
        field_role: &'a InboxFieldRole,
        profile_ref: &'a Option<ProfileFieldRef>,
        surface_fingerprints: &'a [NormalizedSurfaceFingerprint],
    }

    let key = EventKey {
        version: CANON_UNRESOLVED_INBOX_VERSION,
        event_kind: &item.event_kind,
        reason_code: &item.reason_code,
        field_name: &item.field_name,
        field_role: &item.field_role,
        profile_ref: &item.profile_ref,
        surface_fingerprints: &item.surface_fingerprints,
    };
    let bytes = serde_json::to_vec(&key).map_err(|error| {
        artifact_contract_error(format!("failed to hash inbox event key: {error}"))
    })?;
    Ok(hash_bytes(&bytes))
}

fn merge_candidate_summary(left: CandidateSummary, right: CandidateSummary) -> CandidateSummary {
    let mut rejection_reasons = left.rejection_reasons;
    rejection_reasons.extend(right.rejection_reasons);
    rejection_reasons.sort();
    rejection_reasons.dedup();

    CandidateSummary {
        status: left.status.max(right.status),
        candidate_count: left.candidate_count.max(right.candidate_count),
        best_score_band: match (left.best_score_band, right.best_score_band) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        },
        rejection_reasons,
    }
}

fn merge_temporal_scope(
    left: Option<TemporalScope>,
    right: Option<TemporalScope>,
) -> Option<TemporalScope> {
    match (left, right) {
        (Some(left), Some(right)) => Some(TemporalScope {
            start_at: match (left.start_at, right.start_at) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(left), None) => Some(left),
                (None, Some(right)) => Some(right),
                (None, None) => None,
            },
            end_at: match (left.end_at, right.end_at) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(left), None) => Some(left),
                (None, Some(right)) => Some(right),
                (None, None) => None,
            },
        }),
        (Some(scope), None) | (None, Some(scope)) => Some(scope),
        (None, None) => None,
    }
}

fn build_occurrence_summary(occurrences: &[InboxOccurrenceRef]) -> OccurrenceSummary {
    let mut projects = BTreeSet::new();
    let mut runs = BTreeSet::new();
    let mut sources = BTreeSet::new();

    for occurrence in occurrences {
        projects.insert(occurrence.project_ref.clone());
        runs.insert(occurrence.run_ref.clone());
        sources.insert(occurrence.source_ref.clone());
    }

    OccurrenceSummary {
        total_occurrences: occurrences.len() as u64,
        distinct_projects: projects.len() as u64,
        distinct_runs: runs.len() as u64,
        distinct_sources: sources.len() as u64,
    }
}

fn set_occurrence_bounds(item: &mut UnresolvedInboxItem) {
    item.first_seen_at = item
        .occurrences
        .iter()
        .map(|occurrence| occurrence.seen_at.as_str())
        .min()
        .expect("occurrences are never empty")
        .to_string();
    item.last_seen_at = item
        .occurrences
        .iter()
        .map(|occurrence| occurrence.seen_at.as_str())
        .max()
        .expect("occurrences are never empty")
        .to_string();
}

fn hash_without_self(artifact: &UnresolvedInboxArtifact) -> InboxResult<String> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        artifact_contract_error(format!("failed to hash inbox artifact: {error}"))
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn canonical_optional_timestamp(value: Option<String>) -> InboxResult<Option<String>> {
    value
        .map(|value| canonical_timestamp(&value, "timestamp"))
        .transpose()
}

fn canonical_timestamp(value: &str, field: &str) -> InboxResult<String> {
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

fn normalized_hash(value: &str, field: &str) -> InboxResult<String> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 digest with 64 lowercase hex characters"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(corrupt_reference_error(format!(
            "{field} must be a blake3 digest with 64 lowercase hex characters"
        )));
    }
    Ok(value.to_string())
}

fn item_cmp(left: &UnresolvedInboxItem, right: &UnresolvedInboxItem) -> std::cmp::Ordering {
    left.event_key
        .cmp(&right.event_key)
        .then_with(|| left.field_name.cmp(&right.field_name))
        .then_with(|| left.first_seen_at.cmp(&right.first_seen_at))
}

fn fingerprint_cmp(
    left: &NormalizedSurfaceFingerprint,
    right: &NormalizedSurfaceFingerprint,
) -> std::cmp::Ordering {
    left.normalizer_id
        .cmp(&right.normalizer_id)
        .then_with(|| left.surface_role.cmp(&right.surface_role))
        .then_with(|| left.fingerprint.cmp(&right.fingerprint))
}

fn namespace_hint_cmp(left: &NamespaceHint, right: &NamespaceHint) -> std::cmp::Ordering {
    left.namespace
        .cmp(&right.namespace)
        .then_with(|| left.source.cmp(&right.source))
}

fn occurrence_cmp(left: &InboxOccurrenceRef, right: &InboxOccurrenceRef) -> std::cmp::Ordering {
    left.project_ref
        .cmp(&right.project_ref)
        .then_with(|| left.run_ref.cmp(&right.run_ref))
        .then_with(|| left.source_ref.cmp(&right.source_ref))
        .then_with(|| left.record_ref.cmp(&right.record_ref))
        .then_with(|| left.seen_at.cmp(&right.seen_at))
}

fn raw_value_cmp(
    left: &ExternalRawValueReference,
    right: &ExternalRawValueReference,
) -> std::cmp::Ordering {
    left.store
        .cmp(&right.store)
        .then_with(|| left.locator.cmp(&right.locator))
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn artifact_contract_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::ArtifactContract, message)
}

fn privacy_policy_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::PrivacyPolicy, message)
}

fn corrupt_reference_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::CorruptReference, message)
}
