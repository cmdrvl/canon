#![forbid(unsafe_code)]

//! Immutable decision-ledger events for reviewed entity decisions.
//!
//! The ledger is intentionally append-only. Callers provide explicit timestamp
//! and operator provenance; this module supplies deterministic decision IDs,
//! event hashes, stale-context validation, and JSONL append behavior.

use crate::{
    Refusal,
    entity::{
        EntityArtifactMetadata, EntityDeterministicSummary,
        contracts::CANON_ENTITY_DECISION_LEDGER_VERSION, error::EntityRefusalKind,
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

pub const DECISION_LEDGER_EVENT_VERSION: &str = "decision_event.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionLedgerEventType {
    MergeConfirmed,
    DistinctConfirmed,
    RelationConfirmed,
    AliasPatchAdded,
    CannotLinkAdded,
    OperatorOverrideRequested,
    OperatorOverrideApproved,
    PromotionApplied,
    PromotionReverted,
}

impl DecisionLedgerEventType {
    pub const fn all() -> &'static [Self] {
        &[
            Self::MergeConfirmed,
            Self::DistinctConfirmed,
            Self::RelationConfirmed,
            Self::AliasPatchAdded,
            Self::CannotLinkAdded,
            Self::OperatorOverrideRequested,
            Self::OperatorOverrideApproved,
            Self::PromotionApplied,
            Self::PromotionReverted,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MergeConfirmed => "merge_confirmed",
            Self::DistinctConfirmed => "distinct_confirmed",
            Self::RelationConfirmed => "relation_confirmed",
            Self::AliasPatchAdded => "alias_patch_added",
            Self::CannotLinkAdded => "cannot_link_added",
            Self::OperatorOverrideRequested => "operator_override_requested",
            Self::OperatorOverrideApproved => "operator_override_approved",
            Self::PromotionApplied => "promotion_applied",
            Self::PromotionReverted => "promotion_reverted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DecisionLedgerRefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
}

impl DecisionLedgerRefs {
    pub fn surface_pair(
        left_surface_id: impl Into<String>,
        right_surface_id: impl Into<String>,
    ) -> Self {
        let (left_surface_id, right_surface_id) =
            ordered_pair(left_surface_id.into(), right_surface_id.into());
        Self {
            left_surface_id: Some(left_surface_id),
            right_surface_id: Some(right_surface_id),
            ..Self::default()
        }
    }

    pub fn entity_pair(
        left_entity_id: impl Into<String>,
        right_entity_id: impl Into<String>,
    ) -> Self {
        let (left_entity_id, right_entity_id) =
            ordered_pair(left_entity_id.into(), right_entity_id.into());
        Self {
            left_entity_id: Some(left_entity_id),
            right_entity_id: Some(right_entity_id),
            ..Self::default()
        }
    }

    pub fn entity_surfaces(entity_id: impl Into<String>, surface_ids: Vec<String>) -> Self {
        let mut surface_ids = surface_ids;
        surface_ids.sort();
        surface_ids.dedup();
        Self {
            entity_id: Some(entity_id.into()),
            surface_ids,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionLedgerEventInput {
    pub metadata: EntityArtifactMetadata,
    pub event_type: DecisionLedgerEventType,
    pub timestamp: String,
    pub operator_id: String,
    pub previous_event_hash: String,
    pub source_artifact_hash: String,
    pub refs: DecisionLedgerRefs,
    pub reason_code: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLedgerEvent {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub decision_id: String,
    pub event_hash: String,
    pub event_type: DecisionLedgerEventType,
    pub event_version: String,
    pub timestamp: String,
    pub operator_id: String,
    pub previous_event_hash: String,
    pub source_artifact_hash: String,
    pub refs: DecisionLedgerRefs,
    pub decision: String,
    pub reason_code: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionLedgerExpectedContext {
    pub profile_id: String,
    pub profile_version: String,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub source_artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionLedgerAppendReceipt {
    pub path: PathBuf,
    pub decision_id: String,
    pub event_hash: String,
    pub bytes_written: u64,
}

pub fn build_decision_ledger_event(
    input: DecisionLedgerEventInput,
) -> Result<DecisionLedgerEvent, Refusal> {
    validate_event_input(&input)?;

    let mut metadata = input.metadata;
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts.sort_by(|left, right| {
        left.version
            .cmp(&right.version)
            .then_with(|| left.content_hash.cmp(&right.content_hash))
    });

    let event_type = input.event_type;
    let decision_id = decision_id_for_input(
        &metadata,
        event_type,
        &input.source_artifact_hash,
        &input.refs,
        &input.reason_code,
        &input.note,
    )?;
    let mut event = DecisionLedgerEvent {
        version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary: decision_ledger_summary(),
        decision_id,
        event_hash: String::new(),
        event_type,
        event_version: DECISION_LEDGER_EVENT_VERSION.to_string(),
        timestamp: input.timestamp,
        operator_id: input.operator_id,
        previous_event_hash: input.previous_event_hash,
        source_artifact_hash: input.source_artifact_hash,
        refs: input.refs,
        decision: event_type.as_str().to_string(),
        reason_code: input.reason_code,
        note: input.note,
    };
    let event_hash = hash_event_without_self(&event)?;
    event.event_hash = event_hash.clone();
    event.artifact_content_hash = event_hash.clone();
    event.metadata.artifact_content_hash = event_hash;
    validate_decision_ledger_event(&event)?;
    Ok(event)
}

pub fn append_decision_ledger_event(
    path: &Path,
    event: &DecisionLedgerEvent,
) -> Result<DecisionLedgerAppendReceipt, Refusal> {
    validate_decision_ledger_event(event)?;
    let mut line = serde_json::to_vec(event).map_err(|error| {
        ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to serialize decision ledger event",
            json!({
                "stage": "review_import",
                "field": "decision_ledger_event",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    line.push(b'\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| ledger_io_refusal(path, error))?;
    file.write_all(&line)
        .map_err(|error| ledger_io_refusal(path, error))?;

    Ok(DecisionLedgerAppendReceipt {
        path: path.to_path_buf(),
        decision_id: event.decision_id.clone(),
        event_hash: event.event_hash.clone(),
        bytes_written: line.len() as u64,
    })
}

pub fn validate_decision_ledger_event(event: &DecisionLedgerEvent) -> Result<(), Refusal> {
    if event.version != CANON_ENTITY_DECISION_LEDGER_VERSION {
        return Err(ledger_refusal(
            EntityRefusalKind::ArtifactContract,
            "Decision ledger event has the wrong contract version",
            json!({
                "stage": "review_import",
                "field": "version",
                "expected": CANON_ENTITY_DECISION_LEDGER_VERSION,
                "actual": event.version
            }),
        ));
    }
    if event.event_version != DECISION_LEDGER_EVENT_VERSION {
        return Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event has the wrong event version",
            json!({
                "stage": "review_import",
                "field": "event_version",
                "expected": DECISION_LEDGER_EVENT_VERSION,
                "actual": event.event_version
            }),
        ));
    }
    validate_metadata(&event.metadata)?;
    validate_refs(&event.refs)?;
    require_non_empty("timestamp", &event.timestamp)?;
    require_non_empty("operator_id", &event.operator_id)?;
    require_non_empty("previous_event_hash", &event.previous_event_hash)?;
    require_non_empty("source_artifact_hash", &event.source_artifact_hash)?;
    require_non_empty("reason_code", &event.reason_code)?;
    if event.decision != event.event_type.as_str() {
        return Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event decision must match event_type",
            json!({
                "stage": "review_import",
                "field": "decision",
                "expected": event.event_type.as_str(),
                "actual": event.decision
            }),
        ));
    }
    require_source_artifact_hash(&event.metadata, &event.source_artifact_hash)?;
    if event.summary != decision_ledger_summary() {
        return Err(ledger_refusal(
            EntityRefusalKind::ArtifactContract,
            "Decision ledger event summary must identify one append-only event",
            json!({
                "stage": "review_import",
                "field": "summary"
            }),
        ));
    }
    let expected_decision_id = decision_id_for_input(
        &event.metadata,
        event.event_type,
        &event.source_artifact_hash,
        &event.refs,
        &event.reason_code,
        &event.note,
    )?;
    if event.decision_id != expected_decision_id {
        return Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event decision_id is not canonical",
            json!({
                "stage": "review_import",
                "field": "decision_id",
                "expected": expected_decision_id,
                "actual": event.decision_id
            }),
        ));
    }
    let expected_hash = hash_event_without_self(event)?;
    if event.event_hash != expected_hash || event.artifact_content_hash != expected_hash {
        return Err(ledger_refusal(
            EntityRefusalKind::ArtifactContract,
            "Decision ledger event hash is stale",
            json!({
                "stage": "review_import",
                "field": "event_hash",
                "expected": expected_hash,
                "event_hash": event.event_hash,
                "artifact_content_hash": event.artifact_content_hash
            }),
        ));
    }
    if event.metadata.artifact_content_hash != event.artifact_content_hash {
        return Err(ledger_refusal(
            EntityRefusalKind::ArtifactContract,
            "Decision ledger metadata hash must match artifact hash",
            json!({
                "stage": "review_import",
                "field": "metadata.artifact_content_hash"
            }),
        ));
    }
    Ok(())
}

pub fn validate_decision_ledger_event_context(
    event: &DecisionLedgerEvent,
    expected: &DecisionLedgerExpectedContext,
) -> Result<(), Refusal> {
    validate_decision_ledger_event(event)?;
    compare_context_field(
        "profile.id",
        &event.metadata.profile.id,
        &expected.profile_id,
    )?;
    compare_context_field(
        "profile.version",
        &event.metadata.profile.version,
        &expected.profile_version,
    )?;
    compare_context_field(
        "strategy.content_hash",
        &event.metadata.strategy.content_hash,
        &expected.strategy_hash,
    )?;
    compare_context_field(
        "registry_snapshot.lookup_snapshot_hash",
        &event.metadata.registry_snapshot.lookup_snapshot_hash,
        &expected.registry_snapshot_hash,
    )?;
    compare_context_field(
        "source_artifact_hash",
        &event.source_artifact_hash,
        &expected.source_artifact_hash,
    )
}

fn validate_event_input(input: &DecisionLedgerEventInput) -> Result<(), Refusal> {
    validate_metadata(&input.metadata)?;
    validate_refs(&input.refs)?;
    require_non_empty("timestamp", &input.timestamp)?;
    require_non_empty("operator_id", &input.operator_id)?;
    require_non_empty("previous_event_hash", &input.previous_event_hash)?;
    require_non_empty("source_artifact_hash", &input.source_artifact_hash)?;
    require_non_empty("reason_code", &input.reason_code)?;
    require_source_artifact_hash(&input.metadata, &input.source_artifact_hash)
}

fn validate_metadata(metadata: &EntityArtifactMetadata) -> Result<(), Refusal> {
    if !metadata.profile.is_complete() {
        return Err(ledger_refusal(
            EntityRefusalKind::ArtifactContract,
            "Decision ledger profile metadata is incomplete",
            json!({
                "stage": "review_import",
                "field": "metadata.profile"
            }),
        ));
    }
    require_non_empty("metadata.strategy.id", &metadata.strategy.id)?;
    require_non_empty("metadata.strategy.version", &metadata.strategy.version)?;
    require_non_empty(
        "metadata.strategy.content_hash",
        &metadata.strategy.content_hash,
    )?;
    require_non_empty(
        "metadata.registry_snapshot.id",
        &metadata.registry_snapshot.id,
    )?;
    require_non_empty(
        "metadata.registry_snapshot.version",
        &metadata.registry_snapshot.version,
    )?;
    require_non_empty(
        "metadata.registry_snapshot.lookup_snapshot_hash",
        &metadata.registry_snapshot.lookup_snapshot_hash,
    )?;
    require_non_empty("metadata.patch_namespace", &metadata.patch_namespace)?;
    Ok(())
}

fn validate_refs(refs: &DecisionLedgerRefs) -> Result<(), Refusal> {
    for (field, value) in [
        ("refs.left_surface_id", refs.left_surface_id.as_deref()),
        ("refs.right_surface_id", refs.right_surface_id.as_deref()),
        ("refs.left_entity_id", refs.left_entity_id.as_deref()),
        ("refs.right_entity_id", refs.right_entity_id.as_deref()),
        ("refs.entity_id", refs.entity_id.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(empty_field_refusal(field));
        }
    }
    for surface_id in &refs.surface_ids {
        if surface_id.trim().is_empty() {
            return Err(empty_field_refusal("refs.surface_ids"));
        }
    }
    let has_surface_pair = refs.left_surface_id.is_some() && refs.right_surface_id.is_some();
    let has_partial_surface_pair = refs.left_surface_id.is_some() ^ refs.right_surface_id.is_some();
    let has_entity_pair = refs.left_entity_id.is_some() && refs.right_entity_id.is_some();
    let has_partial_entity_pair = refs.left_entity_id.is_some() ^ refs.right_entity_id.is_some();
    let has_entity_surfaces = refs.entity_id.is_some() || !refs.surface_ids.is_empty();
    if has_partial_surface_pair || has_partial_entity_pair {
        return Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger pair references must include both sides",
            json!({
                "stage": "review_import",
                "field": "refs"
            }),
        ));
    }
    if !(has_surface_pair || has_entity_pair || has_entity_surfaces) {
        return Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event must reference at least one surface or entity",
            json!({
                "stage": "review_import",
                "field": "refs"
            }),
        ));
    }
    Ok(())
}

fn require_source_artifact_hash(
    metadata: &EntityArtifactMetadata,
    source_artifact_hash: &str,
) -> Result<(), Refusal> {
    if metadata
        .upstream_artifacts
        .iter()
        .any(|reference| reference.content_hash == source_artifact_hash)
    {
        Ok(())
    } else {
        Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger source artifact hash is not present in metadata upstream artifacts",
            json!({
                "stage": "review_import",
                "field": "source_artifact_hash",
                "source_artifact_hash": source_artifact_hash
            }),
        ))
    }
}

fn compare_context_field(field: &str, actual: &str, expected: &str) -> Result<(), Refusal> {
    if actual == expected {
        Ok(())
    } else {
        Err(ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Decision ledger event does not match the expected review context",
            json!({
                "stage": "review_import",
                "field": field,
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ))
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), Refusal> {
    if value.trim().is_empty() {
        Err(empty_field_refusal(field))
    } else {
        Ok(())
    }
}

fn empty_field_refusal(field: &str) -> Refusal {
    ledger_refusal(
        EntityRefusalKind::ReviewImport,
        "Decision ledger event contains an empty required field",
        json!({
            "stage": "review_import",
            "field": field,
            "writes_performed": false
        }),
    )
}

fn decision_id_for_input(
    metadata: &EntityArtifactMetadata,
    event_type: DecisionLedgerEventType,
    source_artifact_hash: &str,
    refs: &DecisionLedgerRefs,
    reason_code: &str,
    note: &str,
) -> Result<String, Refusal> {
    #[derive(Serialize)]
    struct DecisionIdMaterial<'a> {
        version: &'a str,
        profile_id: &'a str,
        profile_version: &'a str,
        identity_semantics: &'a str,
        strategy_hash: &'a str,
        registry_snapshot_hash: &'a str,
        event_type: DecisionLedgerEventType,
        source_artifact_hash: &'a str,
        refs: &'a DecisionLedgerRefs,
        reason_code: &'a str,
        note: &'a str,
    }

    let material = DecisionIdMaterial {
        version: CANON_ENTITY_DECISION_LEDGER_VERSION,
        profile_id: &metadata.profile.id,
        profile_version: &metadata.profile.version,
        identity_semantics: &metadata.profile.identity_semantics,
        strategy_hash: &metadata.strategy.content_hash,
        registry_snapshot_hash: &metadata.registry_snapshot.lookup_snapshot_hash,
        event_type,
        source_artifact_hash,
        refs,
        reason_code,
        note,
    };
    let bytes = serde_json::to_vec(&material).map_err(|error| {
        ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to build decision ledger decision_id material",
            json!({
                "stage": "review_import",
                "field": "decision_id",
                "error": error.to_string()
            }),
        )
    })?;
    Ok(format!(
        "decision:{}",
        witness::hash_bytes(&bytes)
            .strip_prefix("blake3:")
            .unwrap_or("hash")
    ))
}

fn hash_event_without_self(event: &DecisionLedgerEvent) -> Result<String, Refusal> {
    let mut hashable = event.clone();
    hashable.artifact_content_hash.clear();
    hashable.event_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        ledger_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to hash decision ledger event",
            json!({
                "stage": "review_import",
                "field": "event_hash",
                "error": error.to_string()
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn decision_ledger_summary() -> EntityDeterministicSummary {
    EntityDeterministicSummary {
        counts: BTreeMap::from([("events".to_string(), 1)]),
        labels: BTreeMap::from([("ledger".to_string(), "append_only".to_string())]),
    }
}

fn ordered_pair(left: String, right: String) -> (String, String) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn ledger_io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    ledger_refusal(
        EntityRefusalKind::ReviewImport,
        "Failed to append decision ledger event",
        json!({
            "stage": "review_import",
            "field": "decision_ledger_path",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn ledger_refusal(
    kind: EntityRefusalKind,
    message: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(
        message,
        detail,
        Some("canon entity review import <REVIEW.json|csv> --registry <REGISTRY_DIR>".to_string()),
    )
}
