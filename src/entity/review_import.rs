#![forbid(unsafe_code)]

//! Review decision import validation and decision-ledger append.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_AUDIT_VERSION_V1, CANON_ENTITY_REVIEW_VERSION_V1, EntityArtifactMetadata,
        EntityArtifactReference, EntityArtifactStageV1,
        error::EntityRefusalKind,
        ledger::{
            DecisionLedgerAppendReceipt, DecisionLedgerEventInput, DecisionLedgerEventType,
            DecisionLedgerRefs, append_decision_ledger_event, build_decision_ledger_event,
        },
        review::{
            lifecycle_metadata_v1, required_value_string, set_v1_self_hash, source_reference_v1,
            validate_review_v1_artifact, value_string_or, value_u64_or,
        },
        review_export::{
            CANON_ENTITY_NATIVE_REVIEW_VERSION, NativeReviewArtifact as ExportNativeReviewArtifact,
            NativeReviewDecisionAction as ExportNativeReviewDecisionAction,
            NativeReviewMode as ExportNativeReviewMode,
            NativeReviewModeContext as ExportNativeReviewModeContext, native_review_artifact_hash,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
        schema::validate_artifact_v1_core_contract,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewImportAction {
    MergeConfirmed,
    DistinctConfirmed,
    RelationConfirmed,
    OperatorOverrideRequested,
    OperatorOverrideApproved,
}

impl ReviewImportAction {
    const fn ledger_event_type(self) -> DecisionLedgerEventType {
        match self {
            Self::MergeConfirmed => DecisionLedgerEventType::MergeConfirmed,
            Self::DistinctConfirmed => DecisionLedgerEventType::DistinctConfirmed,
            Self::RelationConfirmed => DecisionLedgerEventType::RelationConfirmed,
            Self::OperatorOverrideRequested => DecisionLedgerEventType::OperatorOverrideRequested,
            Self::OperatorOverrideApproved => DecisionLedgerEventType::OperatorOverrideApproved,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportDecision {
    pub review_id: String,
    pub action: ReviewImportAction,
    pub operator_id: String,
    pub source_review_queue_hash: String,
    pub profile_id: String,
    pub profile_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_semantics: Option<String>,
    pub strategy_hash: String,
    pub registry_snapshot_hash: String,
    pub surface_ids: Vec<String>,
    pub reason_code: String,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_approved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewImportContext {
    pub metadata: EntityArtifactMetadata,
    pub source_review_queue_hash: String,
    pub known_review_ids: BTreeSet<String>,
    pub cannot_link_review_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewImportRequest {
    pub context: ReviewImportContext,
    pub decisions: Vec<ReviewImportDecision>,
    pub ledger_path: PathBuf,
    pub timestamp: String,
    pub previous_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewImportReceipt {
    pub accepted_decisions: u64,
    pub ledger_path: PathBuf,
    pub last_event_hash: String,
    pub appended_events: Vec<DecisionLedgerAppendReceipt>,
}

pub fn parse_review_import_json(input: &str) -> Result<Vec<ReviewImportDecision>, Refusal> {
    let value = serde_json::from_str::<serde_json::Value>(input).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review JSON is malformed",
            json!({
                "stage": "review_import",
                "field": "review_json",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if value.is_array() {
        serde_json::from_value(value).map_err(json_shape_refusal)
    } else {
        let decisions = value.get("decisions").cloned().ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review JSON must be an array or an object with decisions",
                json!({
                    "stage": "review_import",
                    "field": "decisions",
                    "writes_performed": false
                }),
            )
        })?;
        serde_json::from_value(decisions).map_err(json_shape_refusal)
    }
}

pub fn parse_review_import_csv(input: &str) -> Result<Vec<ReviewImportDecision>, Refusal> {
    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let mut decisions = Vec::new();
    for result in reader.deserialize::<ReviewImportDecisionCsv>() {
        let record = result.map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review CSV is malformed",
                json!({
                    "stage": "review_import",
                    "field": "review_csv",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        decisions.push(record.into_decision()?);
    }
    Ok(decisions)
}

pub fn import_review_decisions(
    request: ReviewImportRequest,
) -> Result<ReviewImportReceipt, Refusal> {
    validate_review_import_batch(&request.context, &request.decisions)?;
    if request.timestamp.trim().is_empty() || request.previous_event_hash.trim().is_empty() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import requires explicit timestamp and previous ledger hash",
            json!({
                "stage": "review_import",
                "field": "timestamp_or_previous_event_hash",
                "writes_performed": false
            }),
        ));
    }

    let mut previous_event_hash = request.previous_event_hash;
    let mut receipts = Vec::new();
    for decision in request.decisions {
        let event = build_decision_ledger_event(DecisionLedgerEventInput {
            metadata: ledger_metadata(&request.context),
            event_type: decision.action.ledger_event_type(),
            timestamp: request.timestamp.clone(),
            operator_id: decision.operator_id.clone(),
            previous_event_hash: previous_event_hash.clone(),
            source_artifact_hash: request.context.source_review_queue_hash.clone(),
            refs: decision_refs(&decision)?,
            reason_code: decision.reason_code.clone(),
            note: decision_note(&decision),
        })?;
        previous_event_hash = event.event_hash.clone();
        receipts.push(append_decision_ledger_event(&request.ledger_path, &event)?);
    }

    Ok(ReviewImportReceipt {
        accepted_decisions: receipts.len() as u64,
        ledger_path: request.ledger_path,
        last_event_hash: previous_event_hash,
        appended_events: receipts,
    })
}

pub fn validate_review_import_batch(
    context: &ReviewImportContext,
    decisions: &[ReviewImportDecision],
) -> Result<(), Refusal> {
    if decisions.is_empty() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import contains no decisions",
            json!({
                "stage": "review_import",
                "field": "decisions",
                "writes_performed": false
            }),
        ));
    }

    let mut seen = BTreeSet::new();
    for decision in decisions {
        validate_review_import_decision(context, decision)?;
        if !seen.insert(decision.review_id.clone()) {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import contains duplicate decisions",
                json!({
                    "stage": "review_import",
                    "field": "review_id",
                    "review_id": decision.review_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_review_import_decision(
    context: &ReviewImportContext,
    decision: &ReviewImportDecision,
) -> Result<(), Refusal> {
    require_non_empty("review_id", &decision.review_id)?;
    require_non_empty("operator_id", &decision.operator_id)?;
    require_non_empty("reason_code", &decision.reason_code)?;
    if decision.surface_ids.is_empty() || decision.surface_ids.iter().any(|id| id.trim().is_empty())
    {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decisions must include referenced surfaces",
            json!({
                "stage": "review_import",
                "field": "surface_ids",
                "review_id": decision.review_id,
                "writes_performed": false
            }),
        ));
    }
    if !context.known_review_ids.contains(&decision.review_id) {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import references an unknown review item",
            json!({
                "stage": "review_import",
                "field": "review_id",
                "review_id": decision.review_id,
                "writes_performed": false
            }),
        ));
    }
    compare_context_field(
        "source_review_queue_hash",
        &decision.source_review_queue_hash,
        &context.source_review_queue_hash,
        &decision.review_id,
    )?;
    compare_context_field(
        "profile_id",
        &decision.profile_id,
        &context.metadata.profile.id,
        &decision.review_id,
    )?;
    compare_context_field(
        "profile_version",
        &decision.profile_version,
        &context.metadata.profile.version,
        &decision.review_id,
    )?;
    let entity_type =
        require_import_context_field("entity_type", decision.entity_type.as_deref(), decision)?;
    compare_context_field(
        "entity_type",
        entity_type,
        &context.metadata.profile.entity_type,
        &decision.review_id,
    )?;
    let identity_semantics = require_import_context_field(
        "identity_semantics",
        decision.identity_semantics.as_deref(),
        decision,
    )?;
    compare_context_field(
        "identity_semantics",
        identity_semantics,
        &context.metadata.profile.identity_semantics,
        &decision.review_id,
    )?;
    compare_context_field(
        "strategy_hash",
        &decision.strategy_hash,
        &context.metadata.strategy.content_hash,
        &decision.review_id,
    )?;
    compare_context_field(
        "registry_snapshot_hash",
        &decision.registry_snapshot_hash,
        &context.metadata.registry_snapshot.lookup_snapshot_hash,
        &decision.review_id,
    )?;

    if decision.action == ReviewImportAction::MergeConfirmed
        && context.cannot_link_review_ids.contains(&decision.review_id)
        && (decision
            .override_approved_by
            .as_deref()
            .is_none_or(str::is_empty)
            || decision
                .override_reason_code
                .as_deref()
                .is_none_or(str::is_empty))
    {
        return Err(review_import_refusal(
            EntityRefusalKind::CannotLinkOverride,
            "Review import merge would override a hard cannot-link without explicit provenance",
            json!({
                "stage": "review_import",
                "field": "override_approved_by",
                "review_id": decision.review_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn ledger_metadata(context: &ReviewImportContext) -> EntityArtifactMetadata {
    let mut metadata = context.metadata.clone();
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
        content_hash: context.source_review_queue_hash.clone(),
    }];
    metadata
}

fn decision_refs(decision: &ReviewImportDecision) -> Result<DecisionLedgerRefs, Refusal> {
    let mut surface_ids = decision.surface_ids.clone();
    surface_ids.sort();
    surface_ids.dedup();
    match surface_ids.as_slice() {
        [left, right] => Ok(DecisionLedgerRefs::surface_pair(
            left.clone(),
            right.clone(),
        )),
        [_] | [_, _, ..] => Ok(DecisionLedgerRefs::entity_surfaces(
            decision.review_id.clone(),
            surface_ids,
        )),
        [] => Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decisions must include referenced surfaces",
            json!({
                "stage": "review_import",
                "field": "surface_ids",
                "review_id": decision.review_id,
                "writes_performed": false
            }),
        )),
    }
}

fn decision_note(decision: &ReviewImportDecision) -> String {
    match (
        decision.override_approved_by.as_deref(),
        decision.override_reason_code.as_deref(),
    ) {
        (Some(approved_by), Some(reason_code)) => format!(
            "{} override_approved_by={} override_reason_code={}",
            decision.note, approved_by, reason_code
        ),
        _ => decision.note.clone(),
    }
}

fn compare_context_field(
    field: &str,
    actual: &str,
    expected: &str,
    review_id: &str,
) -> Result<(), Refusal> {
    if actual == expected {
        Ok(())
    } else {
        Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decision does not match the exported review context",
            json!({
                "stage": "review_import",
                "field": field,
                "review_id": review_id,
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ))
    }
}

fn require_import_context_field<'a>(
    field: &str,
    value: Option<&'a str>,
    decision: &ReviewImportDecision,
) -> Result<&'a str, Refusal> {
    let Some(value) = value else {
        return Err(missing_import_context_field(field, decision));
    };
    if value.trim().is_empty() {
        Err(missing_import_context_field(field, decision))
    } else {
        Ok(value)
    }
}

fn missing_import_context_field(field: &str, decision: &ReviewImportDecision) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Review import decision is missing exported profile firewall context",
        json!({
            "stage": "review_import",
            "field": field,
            "review_id": decision.review_id,
            "writes_performed": false
        }),
    )
}

fn require_non_empty(field: &str, value: &str) -> Result<(), Refusal> {
    if value.trim().is_empty() {
        Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decision has an empty required field",
            json!({
                "stage": "review_import",
                "field": field,
                "writes_performed": false
            }),
        ))
    } else {
        Ok(())
    }
}

fn json_shape_refusal(error: serde_json::Error) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Review JSON has an invalid decision shape",
        json!({
            "stage": "review_import",
            "field": "decisions",
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn review_import_refusal(
    kind: EntityRefusalKind,
    message: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(
        message,
        detail,
        Some("canon entity review export <SOLVE.json> --emit json > fresh-review.json".to_string()),
    )
}

#[derive(Debug, Deserialize)]
struct ReviewImportDecisionCsv {
    review_id: String,
    action: ReviewImportAction,
    operator_id: String,
    source_review_queue_hash: String,
    profile_id: String,
    profile_version: String,
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    identity_semantics: Option<String>,
    strategy_hash: String,
    registry_snapshot_hash: String,
    surface_ids_json: String,
    reason_code: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    override_approved_by: Option<String>,
    #[serde(default)]
    override_reason_code: Option<String>,
}

impl ReviewImportDecisionCsv {
    fn into_decision(self) -> Result<ReviewImportDecision, Refusal> {
        let surface_ids =
            serde_json::from_str::<Vec<String>>(&self.surface_ids_json).map_err(|error| {
                review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Review CSV surface_ids_json is malformed",
                    json!({
                        "stage": "review_import",
                        "field": "surface_ids_json",
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })?;
        Ok(ReviewImportDecision {
            review_id: self.review_id,
            action: self.action,
            operator_id: self.operator_id,
            source_review_queue_hash: self.source_review_queue_hash,
            profile_id: self.profile_id,
            profile_version: self.profile_version,
            entity_type: self.entity_type.filter(|value| !value.is_empty()),
            identity_semantics: self.identity_semantics.filter(|value| !value.is_empty()),
            strategy_hash: self.strategy_hash,
            registry_snapshot_hash: self.registry_snapshot_hash,
            surface_ids,
            reason_code: self.reason_code,
            note: self.note,
            override_approved_by: self.override_approved_by.filter(|value| !value.is_empty()),
            override_reason_code: self.override_reason_code.filter(|value| !value.is_empty()),
        })
    }
}

pub fn decisions_by_review_id(
    decisions: &[ReviewImportDecision],
) -> BTreeMap<String, ReviewImportAction> {
    decisions
        .iter()
        .map(|decision| (decision.review_id.clone(), decision.action))
        .collect()
}

pub const CANON_ENTITY_NATIVE_REVIEW_IMPORT_VERSION: &str = "canon_entity_native_review_import.v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReviewDecisionMode {
    Cluster,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReviewDecisionAction {
    Alias,
    CannotLink,
    Relation,
    Assignment,
    Defer,
}

impl NativeReviewDecisionAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alias => "alias",
            Self::CannotLink => "cannot_link",
            Self::Relation => "relation",
            Self::Assignment => "assignment",
            Self::Defer => "defer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewDecision {
    pub review_id: String,
    pub mode: NativeReviewDecisionMode,
    pub action: NativeReviewDecisionAction,
    pub operator_id: String,
    pub reason_code: String,
    #[serde(default)]
    pub note: String,
    pub source_review_artifact_hash: String,
    pub decision_binding_hash: String,
    pub run_content_hash: String,
    pub policy_content_hash: String,
    pub registry_snapshot_hash: String,
    pub mode_context: NativeReviewDecisionContext,
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeReviewDecisionContext {
    Cluster {
        cluster_id: String,
        surface_ids: Vec<String>,
    },
    Link {
        left_surface_id: String,
        right_surface_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        relation_hints: Vec<NativeReviewDecisionRelationHint>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relation: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewDecisionRelationHint {
    pub link_id: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub relation: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeReviewExpectedDecision {
    pub mode: NativeReviewDecisionMode,
    pub decision_binding_hash: String,
    pub mode_context: NativeReviewDecisionContext,
    pub surface_ids: Vec<String>,
    pub allowed_actions: BTreeSet<NativeReviewDecisionAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeReviewImportContext {
    pub source_review_artifact_hash: String,
    pub source_review_queue_hash: String,
    pub run_content_hash: String,
    pub policy_content_hash: String,
    pub registry_snapshot_hash: String,
    pub profile_id: String,
    pub profile_version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub strategy_hash: String,
    pub expected_decisions: BTreeMap<String, NativeReviewExpectedDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeReviewPatchBundle {
    pub alias_patches: Vec<NativeAliasPatch>,
    pub cannot_link_patches: Vec<NativeCannotLinkPatch>,
    pub relation_patches: Vec<NativeRelationPatch>,
    pub assignment_patches: Vec<NativeAssignmentPatch>,
    pub defer_patches: Vec<NativeDeferPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAliasPatch {
    pub patch_id: String,
    pub review_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub operator_id: String,
    pub reason_code: String,
    pub canonical_hint: String,
    pub surface_ids: Vec<String>,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCannotLinkPatch {
    pub patch_id: String,
    pub review_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub operator_id: String,
    pub reason_code: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRelationPatch {
    pub patch_id: String,
    pub review_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub operator_id: String,
    pub reason_code: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub relation: String,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAssignmentPatch {
    pub patch_id: String,
    pub review_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub operator_id: String,
    pub reason_code: String,
    pub canonical_id: String,
    pub surface_ids: Vec<String>,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeDeferPatch {
    pub patch_id: String,
    pub review_id: String,
    pub profile_id: String,
    pub identity_semantics: String,
    pub operator_id: String,
    pub reason_code: String,
    pub surface_ids: Vec<String>,
    pub decision_binding_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewImportReceipt {
    pub version: String,
    pub accepted_decisions: u64,
    pub source_review_artifact_hash: String,
    pub source_review_queue_hash: String,
    pub run_content_hash: String,
    pub policy_content_hash: String,
    pub registry_snapshot_hash: String,
    pub profile_id: String,
    pub profile_version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub strategy_hash: String,
    pub patches: NativeReviewPatchBundle,
}

pub fn parse_native_review_import_json(input: &str) -> Result<Vec<NativeReviewDecision>, Refusal> {
    let value = serde_json::from_str::<Value>(input).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Native review decision JSON is malformed",
            json!({
                "stage": "native_review_import",
                "field": "review_json",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    if value.is_array() {
        serde_json::from_value(value).map_err(native_json_shape_refusal)
    } else {
        let decisions = value.get("decisions").cloned().ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Native review JSON must be an array or an object with decisions",
                json!({
                    "stage": "native_review_import",
                    "field": "decisions",
                    "writes_performed": false
                }),
            )
        })?;
        serde_json::from_value(decisions).map_err(native_json_shape_refusal)
    }
}

pub fn parse_native_review_import_csv(input: &str) -> Result<Vec<NativeReviewDecision>, Refusal> {
    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let mut decisions = Vec::new();
    for result in reader.deserialize::<NativeReviewDecisionCsv>() {
        let record = result.map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Native review decision CSV is malformed",
                json!({
                    "stage": "native_review_import",
                    "field": "review_csv",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        decisions.push(record.into_decision()?);
    }
    Ok(decisions)
}

pub fn native_review_import_context_from_artifact(
    artifact: &Value,
) -> Result<NativeReviewImportContext, Refusal> {
    let artifact = validate_native_review_artifact(artifact)?;
    let mut expected_decisions = BTreeMap::new();
    for item in &artifact.review_items {
        let mode = native_mode_from_export(item.mode);
        let mode_context =
            native_decision_context_from_export(&item.mode_context, mode, &item.review_id)?;
        let allowed_actions = item
            .allowed_actions
            .iter()
            .copied()
            .map(native_action_from_export)
            .collect::<BTreeSet<_>>();
        let expected = NativeReviewExpectedDecision {
            mode,
            decision_binding_hash: item.decision_binding_hash.clone(),
            surface_ids: native_mode_context_surface_ids(&mode_context, &item.review_id)?,
            mode_context,
            allowed_actions,
        };
        if expected_decisions
            .insert(item.review_id.clone(), expected)
            .is_some()
        {
            return Err(native_import_refusal(
                "Native review artifact contains duplicate review items",
                json!({
                    "field": "review_items.review_id",
                    "review_id": item.review_id
                }),
            ));
        }
    }
    Ok(NativeReviewImportContext {
        source_review_artifact_hash: artifact.artifact_content_hash,
        source_review_queue_hash: artifact.binding.source_review_queue_hash,
        run_content_hash: artifact.binding.run_content_hash,
        policy_content_hash: artifact.binding.policy_content_hash,
        registry_snapshot_hash: artifact.binding.registry_snapshot_hash,
        profile_id: artifact.binding.profile_id,
        profile_version: artifact.binding.profile_version,
        entity_type: artifact.binding.entity_type,
        identity_semantics: artifact.binding.identity_semantics,
        strategy_hash: artifact.binding.strategy_hash,
        expected_decisions,
    })
}

fn validate_native_review_artifact(
    artifact: &Value,
) -> Result<ExportNativeReviewArtifact, Refusal> {
    let typed = serde_json::from_value::<ExportNativeReviewArtifact>(artifact.clone()).map_err(
        |error| {
            native_import_refusal(
                "Native review artifact has an invalid canonical shape",
                json!({
                    "field": "artifact",
                    "error": error.to_string()
                }),
            )
        },
    )?;
    if typed.version != CANON_ENTITY_NATIVE_REVIEW_VERSION {
        return Err(native_import_refusal(
            "Native review import requires a native review artifact",
            json!({
                "field": "version",
                "expected": CANON_ENTITY_NATIVE_REVIEW_VERSION,
                "actual": typed.version
            }),
        ));
    }
    let canonical = serde_json::to_value(&typed).map_err(|error| {
        native_import_refusal(
            "Native review artifact has an invalid canonical shape",
            json!({
                "field": "artifact",
                "error": error.to_string()
            }),
        )
    })?;
    if &canonical != artifact {
        return Err(native_import_refusal(
            "Native review artifact contains noncanonical fields",
            json!({
                "field": "artifact"
            }),
        ));
    }
    if !typed.artifact_content_hash.starts_with("blake3:")
        || typed.artifact_content_hash.len() <= "blake3:".len()
    {
        return Err(native_import_refusal(
            "Native review artifact hash must use blake3",
            json!({
                "field": "artifact_content_hash",
                "actual": typed.artifact_content_hash
            }),
        ));
    }
    if typed.metadata.artifact_content_hash != typed.artifact_content_hash {
        return Err(native_import_refusal(
            "Native review artifact metadata hash does not match top-level hash",
            json!({
                "field": "metadata.artifact_content_hash",
                "expected": typed.artifact_content_hash,
                "actual": typed.metadata.artifact_content_hash
            }),
        ));
    }
    let expected_hash = native_review_artifact_hash(&typed)?;
    if typed.artifact_content_hash != expected_hash {
        return Err(native_import_refusal(
            "Native review artifact content hash does not match its canonical content",
            json!({
                "field": "artifact_content_hash",
                "expected": expected_hash,
                "actual": typed.artifact_content_hash
            }),
        ));
    }
    Ok(typed)
}

fn native_mode_from_export(mode: ExportNativeReviewMode) -> NativeReviewDecisionMode {
    match mode {
        ExportNativeReviewMode::Cluster => NativeReviewDecisionMode::Cluster,
        ExportNativeReviewMode::Link => NativeReviewDecisionMode::Link,
    }
}

fn native_action_from_export(
    action: ExportNativeReviewDecisionAction,
) -> NativeReviewDecisionAction {
    match action {
        ExportNativeReviewDecisionAction::Alias => NativeReviewDecisionAction::Alias,
        ExportNativeReviewDecisionAction::CannotLink => NativeReviewDecisionAction::CannotLink,
        ExportNativeReviewDecisionAction::Relation => NativeReviewDecisionAction::Relation,
        ExportNativeReviewDecisionAction::Assignment => NativeReviewDecisionAction::Assignment,
        ExportNativeReviewDecisionAction::Defer => NativeReviewDecisionAction::Defer,
    }
}

fn native_decision_context_from_export(
    context: &ExportNativeReviewModeContext,
    mode: NativeReviewDecisionMode,
    review_id: &str,
) -> Result<NativeReviewDecisionContext, Refusal> {
    let converted = match context {
        ExportNativeReviewModeContext::Cluster {
            cluster_id,
            surface_ids,
        } => NativeReviewDecisionContext::Cluster {
            cluster_id: cluster_id.clone(),
            surface_ids: surface_ids.clone(),
        },
        ExportNativeReviewModeContext::Link {
            left_surface_id,
            right_surface_id,
            relation_hints,
        } => NativeReviewDecisionContext::Link {
            left_surface_id: left_surface_id.clone(),
            right_surface_id: right_surface_id.clone(),
            relation_hints: relation_hints
                .iter()
                .map(|hint| NativeReviewDecisionRelationHint {
                    link_id: hint.link_id.clone(),
                    left_surface_id: hint.left_surface_id.clone(),
                    right_surface_id: hint.right_surface_id.clone(),
                    relation: hint.relation.clone(),
                    reason_code: hint.reason_code.clone(),
                })
                .collect(),
            relation: None,
        },
    };
    let context_mode = match &converted {
        NativeReviewDecisionContext::Cluster { .. } => NativeReviewDecisionMode::Cluster,
        NativeReviewDecisionContext::Link { .. } => NativeReviewDecisionMode::Link,
    };
    if context_mode != mode {
        return Err(native_import_refusal(
            "Native review artifact mode_context does not match item mode",
            json!({
                "field": "mode_context",
                "review_id": review_id,
                "expected": native_mode_str(mode),
                "actual": native_mode_str(context_mode)
            }),
        ));
    }
    Ok(converted)
}

fn native_context_value(context: &NativeReviewDecisionContext) -> Value {
    serde_json::to_value(context).expect("native review decision context serializes")
}

pub fn import_native_review_decisions(
    context: NativeReviewImportContext,
    decisions: Vec<NativeReviewDecision>,
) -> Result<NativeReviewImportReceipt, Refusal> {
    validate_native_review_batch(&context, &decisions)?;
    let mut patches = NativeReviewPatchBundle::default();
    for decision in decisions {
        match decision.action {
            NativeReviewDecisionAction::Alias => {
                patches
                    .alias_patches
                    .push(native_alias_patch(&context, &decision)?);
            }
            NativeReviewDecisionAction::CannotLink => {
                patches.cannot_link_patches.extend(
                    native_surface_pairs(&native_decision_surface_ids(&decision)?)?
                        .into_iter()
                        .map(|(left, right)| {
                            native_cannot_link_patch(&context, &decision, left, right)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            NativeReviewDecisionAction::Relation => {
                patches
                    .relation_patches
                    .push(native_relation_patch(&context, &decision)?);
            }
            NativeReviewDecisionAction::Assignment => {
                patches
                    .assignment_patches
                    .push(native_assignment_patch(&context, &decision)?);
            }
            NativeReviewDecisionAction::Defer => {
                patches
                    .defer_patches
                    .push(native_defer_patch(&context, &decision)?);
            }
        }
    }
    sort_native_patch_bundle(&mut patches);
    Ok(NativeReviewImportReceipt {
        version: CANON_ENTITY_NATIVE_REVIEW_IMPORT_VERSION.to_string(),
        accepted_decisions: native_patch_count(&patches),
        source_review_artifact_hash: context.source_review_artifact_hash,
        source_review_queue_hash: context.source_review_queue_hash,
        run_content_hash: context.run_content_hash,
        policy_content_hash: context.policy_content_hash,
        registry_snapshot_hash: context.registry_snapshot_hash,
        profile_id: context.profile_id,
        profile_version: context.profile_version,
        entity_type: context.entity_type,
        identity_semantics: context.identity_semantics,
        strategy_hash: context.strategy_hash,
        patches,
    })
}

pub fn validate_native_review_batch(
    context: &NativeReviewImportContext,
    decisions: &[NativeReviewDecision],
) -> Result<(), Refusal> {
    if decisions.is_empty() {
        return Err(native_import_refusal(
            "Native review import contains no decisions",
            json!({
                "field": "decisions"
            }),
        ));
    }
    let mut seen = BTreeSet::new();
    for decision in decisions {
        validate_native_review_decision(context, decision)?;
        if !seen.insert(decision.review_id.clone()) {
            return Err(native_import_refusal(
                "Native review import contains duplicate decisions",
                json!({
                    "field": "review_id",
                    "review_id": decision.review_id
                }),
            ));
        }
    }
    validate_native_contradictions(decisions)
}

fn validate_native_review_decision(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
) -> Result<(), Refusal> {
    require_non_empty("review_id", &decision.review_id)?;
    require_non_empty("operator_id", &decision.operator_id)?;
    require_non_empty("reason_code", &decision.reason_code)?;
    let expected = context
        .expected_decisions
        .get(&decision.review_id)
        .ok_or_else(|| {
            native_import_refusal(
                "Native review import references an unknown review item",
                json!({
                    "field": "review_id",
                    "review_id": decision.review_id
                }),
            )
        })?;
    native_compare_context_field(
        "source_review_artifact_hash",
        &decision.source_review_artifact_hash,
        &context.source_review_artifact_hash,
        &decision.review_id,
    )?;
    native_compare_context_field(
        "run_content_hash",
        &decision.run_content_hash,
        &context.run_content_hash,
        &decision.review_id,
    )?;
    native_compare_context_field(
        "policy_content_hash",
        &decision.policy_content_hash,
        &context.policy_content_hash,
        &decision.review_id,
    )?;
    native_compare_context_field(
        "registry_snapshot_hash",
        &decision.registry_snapshot_hash,
        &context.registry_snapshot_hash,
        &decision.review_id,
    )?;
    native_compare_context_field(
        "decision_binding_hash",
        &decision.decision_binding_hash,
        &expected.decision_binding_hash,
        &decision.review_id,
    )?;
    if decision.mode != expected.mode {
        return Err(native_import_refusal(
            "Native review decision mode does not match exported context",
            json!({
                "field": "mode",
                "review_id": decision.review_id,
                "expected": native_mode_str(expected.mode),
                "actual": native_mode_str(decision.mode)
            }),
        ));
    }
    if !expected.allowed_actions.contains(&decision.action) {
        return Err(native_import_refusal(
            "Native review decision action was not offered by the exported review item",
            json!({
                "field": "action",
                "review_id": decision.review_id,
                "actual": decision.action.as_str()
            }),
        ));
    }
    validate_native_mode_context(decision)?;
    let surface_ids = native_decision_surface_ids(decision)?;
    if surface_ids != expected.surface_ids {
        return Err(native_import_refusal(
            "Native review decision surfaces do not match exported context",
            json!({
                "field": "surface_ids",
                "review_id": decision.review_id,
                "expected": expected.surface_ids,
                "actual": surface_ids
            }),
        ));
    }
    if decision.mode_context != expected.mode_context {
        return Err(native_import_refusal(
            "Native review decision mode_context does not match exported context",
            json!({
                "field": "mode_context",
                "review_id": decision.review_id,
                "expected": native_context_value(&expected.mode_context),
                "actual": native_context_value(&decision.mode_context)
            }),
        ));
    }
    Ok(())
}

fn validate_native_mode_context(decision: &NativeReviewDecision) -> Result<(), Refusal> {
    match (&decision.mode, &decision.mode_context) {
        (
            NativeReviewDecisionMode::Cluster,
            NativeReviewDecisionContext::Cluster {
                cluster_id,
                surface_ids,
            },
        ) => {
            let singleton_alias =
                decision.action == NativeReviewDecisionAction::Alias && surface_ids.len() == 1;
            let singleton_alias_has_canonical = decision
                .target_canonical_id
                .as_deref()
                .is_some_and(|canonical_id| !canonical_id.trim().is_empty());
            if cluster_id.trim().is_empty()
                || surface_ids.is_empty()
                || (surface_ids.len() < 2 && !singleton_alias)
                || (singleton_alias && !singleton_alias_has_canonical)
            {
                return Err(native_import_refusal(
                    "Native cluster review decision requires cluster context",
                    json!({
                        "field": "mode_context",
                        "review_id": decision.review_id
                    }),
                ));
            }
            Ok(())
        }
        (
            NativeReviewDecisionMode::Link,
            NativeReviewDecisionContext::Link {
                left_surface_id,
                right_surface_id,
                ..
            },
        ) => {
            if left_surface_id.trim().is_empty() {
                return Err(native_import_refusal(
                    "Native link review decision requires a distinct surface pair",
                    json!({
                        "field": "mode_context",
                        "review_id": decision.review_id
                    }),
                ));
            }
            if let Some(right_surface_id) = right_surface_id
                && (right_surface_id.trim().is_empty() || left_surface_id == right_surface_id)
            {
                return Err(native_import_refusal(
                    "Native link review decision requires a distinct surface pair",
                    json!({
                        "field": "mode_context",
                        "review_id": decision.review_id
                    }),
                ));
            }
            if matches!(
                decision.action,
                NativeReviewDecisionAction::Relation | NativeReviewDecisionAction::CannotLink
            ) && right_surface_id.is_none()
            {
                return Err(native_import_refusal(
                    "Native link review decision action requires a candidate surface",
                    json!({
                        "field": "mode_context",
                        "review_id": decision.review_id,
                        "action": decision.action.as_str()
                    }),
                ));
            }
            Ok(())
        }
        _ => Err(native_import_refusal(
            "Native review decision mode-specific context is contradictory",
            json!({
                "field": "mode_context",
                "review_id": decision.review_id
            }),
        )),
    }
}

fn validate_native_contradictions(decisions: &[NativeReviewDecision]) -> Result<(), Refusal> {
    let mut positive_pairs = BTreeMap::<NativePairKey, String>::new();
    let mut negative_pairs = BTreeMap::<NativePairKey, String>::new();
    for decision in decisions {
        match decision.action {
            NativeReviewDecisionAction::Alias => {
                let surface_ids = native_decision_surface_ids(decision)?;
                for pair in if surface_ids.len() < 2 {
                    Vec::new()
                } else {
                    native_surface_pairs(&surface_ids)?
                } {
                    let key = NativePairKey::new(pair.0, pair.1);
                    if let Some(negative_review_id) = negative_pairs.get(&key) {
                        return Err(native_contradiction_refusal(
                            "identity_cannot_link_conflict",
                            &decision.review_id,
                            negative_review_id,
                            &key,
                        ));
                    }
                    positive_pairs.insert(key, decision.review_id.clone());
                }
            }
            NativeReviewDecisionAction::Assignment => {
                for pair in native_surface_pairs(&native_decision_surface_ids(decision)?)? {
                    let key = NativePairKey::new(pair.0, pair.1);
                    if let Some(negative_review_id) = negative_pairs.get(&key) {
                        return Err(native_contradiction_refusal(
                            "identity_cannot_link_conflict",
                            &decision.review_id,
                            negative_review_id,
                            &key,
                        ));
                    }
                    positive_pairs.insert(key, decision.review_id.clone());
                }
            }
            NativeReviewDecisionAction::CannotLink => {
                for pair in native_surface_pairs(&native_decision_surface_ids(decision)?)? {
                    let key = NativePairKey::new(pair.0, pair.1);
                    if let Some(positive_review_id) = positive_pairs.get(&key) {
                        return Err(native_contradiction_refusal(
                            "identity_cannot_link_conflict",
                            positive_review_id,
                            &decision.review_id,
                            &key,
                        ));
                    }
                    negative_pairs.insert(key, decision.review_id.clone());
                }
            }
            NativeReviewDecisionAction::Relation | NativeReviewDecisionAction::Defer => {}
        }
    }
    Ok(())
}

fn native_alias_patch(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
) -> Result<NativeAliasPatch, Refusal> {
    let surface_ids = native_decision_surface_ids(decision)?;
    Ok(NativeAliasPatch {
        patch_id: native_patch_id("alias", decision),
        review_id: decision.review_id.clone(),
        profile_id: context.profile_id.clone(),
        identity_semantics: context.identity_semantics.clone(),
        operator_id: decision.operator_id.clone(),
        reason_code: decision.reason_code.clone(),
        canonical_hint: decision
            .target_canonical_id
            .clone()
            .unwrap_or_else(|| decision.review_id.clone()),
        surface_ids,
        decision_binding_hash: decision.decision_binding_hash.clone(),
    })
}

fn native_cannot_link_patch(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
    left_surface_id: String,
    right_surface_id: String,
) -> Result<NativeCannotLinkPatch, Refusal> {
    Ok(NativeCannotLinkPatch {
        patch_id: format!(
            "{}:{}:{}",
            native_patch_id("cannot_link", decision),
            native_stable_suffix(&left_surface_id),
            native_stable_suffix(&right_surface_id)
        ),
        review_id: decision.review_id.clone(),
        profile_id: context.profile_id.clone(),
        identity_semantics: context.identity_semantics.clone(),
        operator_id: decision.operator_id.clone(),
        reason_code: decision.reason_code.clone(),
        left_surface_id,
        right_surface_id,
        decision_binding_hash: decision.decision_binding_hash.clone(),
    })
}

fn native_relation_patch(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
) -> Result<NativeRelationPatch, Refusal> {
    let (left_surface_id, right_surface_id, context_relation) = match &decision.mode_context {
        NativeReviewDecisionContext::Link {
            left_surface_id,
            right_surface_id,
            relation_hints,
            relation,
        } => {
            let Some(right_surface_id) = right_surface_id.clone() else {
                return Err(native_import_refusal(
                    "Native relation decision requires candidate-backed link context",
                    json!({
                        "field": "mode_context",
                        "review_id": decision.review_id
                    }),
                ));
            };
            (
                left_surface_id.clone(),
                right_surface_id,
                relation
                    .clone()
                    .or_else(|| relation_hints.first().map(|hint| hint.relation.clone())),
            )
        }
        NativeReviewDecisionContext::Cluster { .. } => {
            return Err(native_import_refusal(
                "Native relation decision requires link context",
                json!({
                    "field": "mode_context",
                    "review_id": decision.review_id
                }),
            ));
        }
    };
    let relation = decision
        .relation
        .clone()
        .or(context_relation)
        .unwrap_or_else(|| decision.reason_code.clone());
    Ok(NativeRelationPatch {
        patch_id: native_patch_id("relation", decision),
        review_id: decision.review_id.clone(),
        profile_id: context.profile_id.clone(),
        identity_semantics: context.identity_semantics.clone(),
        operator_id: decision.operator_id.clone(),
        reason_code: decision.reason_code.clone(),
        left_surface_id,
        right_surface_id,
        relation,
        decision_binding_hash: decision.decision_binding_hash.clone(),
    })
}

fn native_assignment_patch(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
) -> Result<NativeAssignmentPatch, Refusal> {
    let canonical_id = decision
        .target_canonical_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            native_import_refusal(
                "Native assignment decision requires target_canonical_id",
                json!({
                    "field": "target_canonical_id",
                    "review_id": decision.review_id
                }),
            )
        })?;
    Ok(NativeAssignmentPatch {
        patch_id: native_patch_id("assignment", decision),
        review_id: decision.review_id.clone(),
        profile_id: context.profile_id.clone(),
        identity_semantics: context.identity_semantics.clone(),
        operator_id: decision.operator_id.clone(),
        reason_code: decision.reason_code.clone(),
        canonical_id: canonical_id.to_string(),
        surface_ids: native_decision_surface_ids(decision)?,
        decision_binding_hash: decision.decision_binding_hash.clone(),
    })
}

fn native_defer_patch(
    context: &NativeReviewImportContext,
    decision: &NativeReviewDecision,
) -> Result<NativeDeferPatch, Refusal> {
    Ok(NativeDeferPatch {
        patch_id: native_patch_id("defer", decision),
        review_id: decision.review_id.clone(),
        profile_id: context.profile_id.clone(),
        identity_semantics: context.identity_semantics.clone(),
        operator_id: decision.operator_id.clone(),
        reason_code: decision.reason_code.clone(),
        surface_ids: native_decision_surface_ids(decision)?,
        decision_binding_hash: decision.decision_binding_hash.clone(),
    })
}

fn native_decision_surface_ids(decision: &NativeReviewDecision) -> Result<Vec<String>, Refusal> {
    let mut ids = native_mode_context_surface_ids(&decision.mode_context, &decision.review_id)?;
    ids.extend(decision.surface_ids.clone());
    ids.sort();
    ids.dedup();
    if ids.iter().any(|id| id.trim().is_empty()) || ids.is_empty() {
        return Err(native_import_refusal(
            "Native review decision requires non-empty surface references",
            json!({
                "field": "surface_ids",
                "review_id": decision.review_id
            }),
        ));
    }
    Ok(ids)
}

fn native_mode_context_surface_ids(
    context: &NativeReviewDecisionContext,
    review_id: &str,
) -> Result<Vec<String>, Refusal> {
    let mut ids = match context {
        NativeReviewDecisionContext::Cluster { surface_ids, .. } => surface_ids.clone(),
        NativeReviewDecisionContext::Link {
            left_surface_id,
            right_surface_id,
            ..
        } => {
            let mut ids = vec![left_surface_id.clone()];
            if let Some(right_surface_id) = right_surface_id {
                ids.push(right_surface_id.clone());
            }
            ids
        }
    };
    ids.sort();
    ids.dedup();
    if ids.iter().any(|id| id.trim().is_empty()) || ids.is_empty() {
        return Err(native_import_refusal(
            "Native review decision requires non-empty surface references",
            json!({
                "field": "surface_ids",
                "review_id": review_id
            }),
        ));
    }
    Ok(ids)
}

fn native_surface_pairs(surface_ids: &[String]) -> Result<Vec<(String, String)>, Refusal> {
    let mut ids = surface_ids.to_vec();
    ids.sort();
    ids.dedup();
    if ids.len() < 2 {
        return Err(native_import_refusal(
            "Native review decision requires at least two surfaces for patch derivation",
            json!({
                "field": "surface_ids"
            }),
        ));
    }
    let mut pairs = Vec::new();
    for left_index in 0..ids.len() {
        for right_index in (left_index + 1)..ids.len() {
            pairs.push((ids[left_index].clone(), ids[right_index].clone()));
        }
    }
    Ok(pairs)
}

fn native_patch_id(prefix: &str, decision: &NativeReviewDecision) -> String {
    format!(
        "{}:{}",
        prefix,
        native_stable_suffix(
            decision
                .review_id
                .strip_prefix("review:")
                .unwrap_or(&decision.review_id)
        )
    )
}

fn native_patch_count(bundle: &NativeReviewPatchBundle) -> u64 {
    (bundle.alias_patches.len()
        + bundle.cannot_link_patches.len()
        + bundle.relation_patches.len()
        + bundle.assignment_patches.len()
        + bundle.defer_patches.len()) as u64
}

fn sort_native_patch_bundle(bundle: &mut NativeReviewPatchBundle) {
    bundle
        .alias_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .cannot_link_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .relation_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .assignment_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
    bundle
        .defer_patches
        .sort_by(|left, right| left.patch_id.cmp(&right.patch_id));
}

fn native_compare_context_field(
    field: &str,
    actual: &str,
    expected: &str,
    review_id: &str,
) -> Result<(), Refusal> {
    if actual == expected {
        Ok(())
    } else {
        Err(native_import_refusal(
            "Native review import decision does not match exported context",
            json!({
                "field": field,
                "review_id": review_id,
                "expected": expected,
                "actual": actual
            }),
        ))
    }
}

fn native_mode_str(mode: NativeReviewDecisionMode) -> &'static str {
    match mode {
        NativeReviewDecisionMode::Cluster => "cluster",
        NativeReviewDecisionMode::Link => "link",
    }
}

fn native_json_shape_refusal(error: serde_json::Error) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Native review JSON has an invalid decision shape",
        json!({
            "stage": "native_review_import",
            "field": "decisions",
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn native_contradiction_refusal(
    reason: &'static str,
    positive_review_id: &str,
    negative_review_id: &str,
    key: &NativePairKey,
) -> Refusal {
    native_import_refusal(
        "Native review import contains contradictory decisions",
        json!({
            "reason": reason,
            "left_surface_id": key.left,
            "right_surface_id": key.right,
            "positive_review_id": positive_review_id,
            "negative_review_id": negative_review_id
        }),
    )
}

fn native_import_refusal(message: &'static str, mut detail: Value) -> Refusal {
    if let Some(object) = detail.as_object_mut() {
        object.insert(
            "stage".to_string(),
            Value::String("native_review_import".to_string()),
        );
        object.insert("writes_performed".to_string(), Value::Bool(false));
    }
    review_import_refusal(EntityRefusalKind::ReviewImport, message, detail)
}

fn native_stable_suffix(value: &str) -> String {
    value.replace([':', '/', ' ', '.'], "_")
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativePairKey {
    left: String,
    right: String,
}

impl NativePairKey {
    fn new(left: String, right: String) -> Self {
        if left <= right {
            Self { left, right }
        } else {
            Self {
                left: right,
                right: left,
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct NativeReviewDecisionCsv {
    review_id: String,
    mode: NativeReviewDecisionMode,
    action: NativeReviewDecisionAction,
    operator_id: String,
    reason_code: String,
    #[serde(default)]
    note: String,
    source_review_artifact_hash: String,
    decision_binding_hash: String,
    run_content_hash: String,
    policy_content_hash: String,
    registry_snapshot_hash: String,
    mode_context_json: String,
    #[serde(default)]
    surface_ids_json: String,
    #[serde(default)]
    target_canonical_id: Option<String>,
    #[serde(default)]
    relation: Option<String>,
}

impl NativeReviewDecisionCsv {
    fn into_decision(self) -> Result<NativeReviewDecision, Refusal> {
        let mode_context = serde_json::from_str::<NativeReviewDecisionContext>(
            &self.mode_context_json,
        )
        .map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Native review CSV mode_context_json is malformed",
                json!({
                    "stage": "native_review_import",
                    "field": "mode_context_json",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        let surface_ids = if self.surface_ids_json.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str::<Vec<String>>(&self.surface_ids_json).map_err(|error| {
                review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Native review CSV surface_ids_json is malformed",
                    json!({
                        "stage": "native_review_import",
                        "field": "surface_ids_json",
                        "error": error.to_string(),
                        "writes_performed": false
                    }),
                )
            })?
        };
        Ok(NativeReviewDecision {
            review_id: self.review_id,
            mode: self.mode,
            action: self.action,
            operator_id: self.operator_id,
            reason_code: self.reason_code,
            note: self.note,
            source_review_artifact_hash: self.source_review_artifact_hash,
            decision_binding_hash: self.decision_binding_hash,
            run_content_hash: self.run_content_hash,
            policy_content_hash: self.policy_content_hash,
            registry_snapshot_hash: self.registry_snapshot_hash,
            mode_context,
            surface_ids,
            target_canonical_id: self.target_canonical_id.filter(|value| !value.is_empty()),
            relation: self.relation.filter(|value| !value.is_empty()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewImportV1Request<'a> {
    pub review_path: &'a Path,
    pub review_bytes: &'a [u8],
    pub registry: &'a Path,
    pub next_version: &'a str,
    pub audit: Option<(&'a Value, &'a [u8])>,
}

pub fn review_import_input_looks_v1(bytes: &[u8]) -> bool {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        return value.get("version").and_then(Value::as_str)
            == Some(CANON_ENTITY_REVIEW_VERSION_V1);
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    text.lines()
        .next()
        .is_some_and(|line| line.contains("review_context_json"))
}

pub fn import_review_v1(request: ReviewImportV1Request<'_>) -> Result<Value, Refusal> {
    let review = parse_review_v1_input(request.review_path, request.review_bytes)?;
    validate_review_v1_artifact(&review)?;
    validate_review_v1_audit(&review, request.audit.map(|(audit, _)| audit))?;
    let registry_before = registry_json_value(request.registry)?;
    let version_before = registry_before
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| review_import_registry_refusal(request.registry, "missing version"))?;
    let registry_id = registry_before
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| review_import_registry_refusal(request.registry, "missing id"))?;
    let next_version = validate_review_import_next_version(version_before, request.next_version)?;
    let source_ref = source_reference_v1(&review)?;
    let mut upstreams = vec![source_ref];
    if let Some((audit, _)) = request.audit {
        upstreams.push(source_reference_v1(audit)?);
    }
    upstreams.sort_by(|left, right| {
        value_string_or(left, &["version"], "")
            .cmp(value_string_or(right, &["version"], ""))
            .then_with(|| {
                value_string_or(left, &["content_hash"], "").cmp(value_string_or(
                    right,
                    &["content_hash"],
                    "",
                ))
            })
    });
    let metadata = lifecycle_metadata_v1(&review, EntityArtifactStageV1::Review, upstreams)?;
    let decisions = reviewed_decisions_from_v1(&review);
    let review_hash = required_value_string(&review, &["artifact_content_hash"], "review hash")?;
    let audit_hash = request
        .audit
        .map(|(audit, _)| required_value_string(audit, &["artifact_content_hash"], "audit hash"))
        .transpose()?;
    let mut artifact = json!({
        "version": CANON_ENTITY_REVIEW_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "reviewed_decisions": decisions.len() as u64,
                "source_review_items": value_u64_or(&review, &["summary", "counts", "review_items"], 0)
            },
            "labels": {
                "stage": "review",
                "operation": "import",
                "status": "accepted"
            }
        },
        "review_queue_path": "review/queue.jsonl",
        "source_review": {
            "path": request.review_path.display().to_string(),
            "content_hash": review_hash
        },
        "audit": audit_hash.map(|hash| json!({
            "version": CANON_ENTITY_AUDIT_VERSION_V1,
            "content_hash": hash
        })),
        "registry": {
            "id": registry_id,
            "version_before": version_before,
            "version_after": next_version,
            "source": request.registry.display().to_string()
        },
        "decisions": decisions
    });
    set_v1_self_hash(&mut artifact)?;
    Ok(artifact)
}

pub fn render_review_import_v1_summary(artifact: &Value) -> String {
    let registry = value_string_or(artifact, &["registry", "id"], "<registry>");
    let before = value_string_or(artifact, &["registry", "version_before"], "<before>");
    let after = value_string_or(artifact, &["registry", "version_after"], "<after>");
    let decisions = value_u64_or(artifact, &["summary", "counts", "reviewed_decisions"], 0);
    format!("{registry} review import v1 {before} -> {after} decisions={decisions}")
}

fn parse_review_v1_input(path: &Path, bytes: &[u8]) -> Result<Value, Refusal> {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        return Ok(value);
    }
    let text = std::str::from_utf8(bytes).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review v1 input is neither JSON nor UTF-8 CSV",
            json!({
                "stage": "review_import",
                "field": "review",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    parse_review_v1_csv(text)
}

fn parse_review_v1_csv(input: &str) -> Result<Value, Refusal> {
    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let headers = reader.headers().map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review v1 CSV headers are malformed",
            json!({
                "stage": "review_import",
                "field": "review_context_json",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let context_index = headers
        .iter()
        .position(|header| header == "review_context_json")
        .ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review v1 CSV is missing review_context_json",
                json!({
                    "stage": "review_import",
                    "field": "review_context_json",
                    "writes_performed": false
                }),
            )
        })?;
    let item_index = headers.iter().position(|header| header == "item_json");
    let decision_index = headers.iter().position(|header| header == "decision");
    let mut context = None;
    let mut decisions = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review v1 CSV record is malformed",
                json!({
                    "stage": "review_import",
                    "field": "review_csv",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        if context.is_none() {
            context = Some(
                serde_json::from_str::<Value>(&record[context_index]).map_err(|error| {
                    review_import_refusal(
                        EntityRefusalKind::ReviewImport,
                        "Review v1 CSV context JSON is malformed",
                        json!({
                            "stage": "review_import",
                            "field": "review_context_json",
                            "error": error.to_string(),
                            "writes_performed": false
                        }),
                    )
                })?,
            );
        }
        if record.get(0) == Some("__context__") {
            continue;
        }
        if let Some(index) = item_index {
            let mut item =
                serde_json::from_str::<Value>(&record[index]).unwrap_or_else(|_| json!({}));
            if let Some(decision_index) = decision_index
                && let Some(decision) = record.get(decision_index).filter(|value| !value.is_empty())
            {
                item["decision"] = Value::String(decision.to_string());
            }
            decisions.push(item);
        }
    }
    let mut artifact = context.ok_or_else(|| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review v1 CSV contains no context row",
            json!({
                "stage": "review_import",
                "field": "review_context_json",
                "writes_performed": false
            }),
        )
    })?;
    artifact["review_items"] = Value::Array(decisions);
    Ok(artifact)
}

fn validate_review_v1_audit(review: &Value, audit: Option<&Value>) -> Result<(), Refusal> {
    let Some(audit) = audit else {
        return Ok(());
    };
    let contract = validate_artifact_v1_core_contract(audit)?;
    if contract.artifact_version != CANON_ENTITY_AUDIT_VERSION_V1 {
        return Err(review_import_refusal(
            EntityRefusalKind::AuditGate,
            "Review import audit must be canon_entity_audit.v1",
            json!({
                "stage": "review_import",
                "field": "audit.version",
                "expected": CANON_ENTITY_AUDIT_VERSION_V1,
                "actual": contract.artifact_version,
                "writes_performed": false
            }),
        ));
    }
    if value_string_or(audit, &["summary", "labels", "status"], "") != "passed" {
        return Err(review_import_refusal(
            EntityRefusalKind::AuditGate,
            "Review import requires a passing audit artifact",
            json!({
                "stage": "review_import",
                "field": "audit.status",
                "expected": "passed",
                "actual": value_string_or(audit, &["summary", "labels", "status"], "<missing>"),
                "writes_performed": false
            }),
        ));
    }
    let review_registry = required_value_string(
        review,
        &["metadata", "registry_snapshot", "lookup_snapshot_hash"],
        "metadata.registry_snapshot.lookup_snapshot_hash",
    )?;
    let audit_registry = required_value_string(
        audit,
        &["metadata", "registry_snapshot", "lookup_snapshot_hash"],
        "audit.metadata.registry_snapshot.lookup_snapshot_hash",
    )?;
    if review_registry != audit_registry {
        return Err(review_import_refusal(
            EntityRefusalKind::RegistrySnapshot,
            "Review import audit registry snapshot does not match review artifact",
            json!({
                "stage": "review_import",
                "field": "registry_snapshot_hash",
                "expected": review_registry,
                "actual": audit_registry,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn reviewed_decisions_from_v1(review: &Value) -> Vec<Value> {
    review
        .get("review_items")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter(|item| {
            item.get("decision")
                .and_then(Value::as_str)
                .is_some_and(|decision| !decision.trim().is_empty())
        })
        .cloned()
        .collect()
}

fn registry_json_value(registry: &Path) -> Result<Value, Refusal> {
    let path = registry.join("registry.json");
    let bytes = fs::read(&path).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import could not read registry.json",
            json!({
                "stage": "review_import",
                "field": "registry",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| review_import_registry_refusal(registry, "registry.json malformed"))
}

fn validate_review_import_next_version<'a>(
    current: &str,
    next: &'a str,
) -> Result<&'a str, Refusal> {
    let trimmed = next.trim_matches(|ch: char| ch.is_ascii_whitespace());
    if trimmed.is_empty() || trimmed != next || trimmed == current {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import requires an explicit changed next version",
            json!({
                "stage": "review_import",
                "field": "next_version",
                "current_version": current,
                "next_version": next,
                "writes_performed": false
            }),
        ));
    }
    Ok(next)
}

fn review_import_registry_refusal(registry: &Path, problem: &str) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Review import registry metadata is malformed",
        json!({
            "stage": "review_import",
            "field": "registry",
            "registry": registry.display().to_string(),
            "problem": problem,
            "writes_performed": false
        }),
    )
}
