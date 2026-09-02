#![forbid(unsafe_code)]

//! Review decision import validation and decision-ledger append.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_AUDIT_VERSION_V1, CANON_ENTITY_REVIEW_VERSION_V1, EntityArtifactMetadata,
        EntityArtifactReference,
        error::EntityRefusalKind,
        ledger::{
            DecisionLedgerAppendReceipt, DecisionLedgerEventInput, DecisionLedgerEventType,
            DecisionLedgerRefs, append_decision_ledger_event, build_decision_ledger_event,
        },
        review::{
            required_value_string, validate_review_v1_artifact, value_string_or, value_u64_or,
        },
        review_export::{
            CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION,
            CANON_ENTITY_NATIVE_REVIEW_VERSION, NativeReviewArtifact as ExportNativeReviewArtifact,
            NativeReviewDecisionAction as ExportNativeReviewDecisionAction,
            NativeReviewEvidenceSignatureGroup as ExportNativeReviewEvidenceSignatureGroup,
            NativeReviewItem as ExportNativeReviewItem, NativeReviewMode as ExportNativeReviewMode,
            NativeReviewModeContext as ExportNativeReviewModeContext,
            build_native_review_signature_groups, native_evidence_signature_for_item,
            native_review_artifact_hash,
        },
        schema::{
            CANON_ENTITY_REVIEW_IMPORT_VERSION, CANON_ENTITY_REVIEW_QUEUE_VERSION,
            validate_artifact_core_contract, validate_artifact_v1_core_contract,
            validate_entity_v1_self_hash,
        },
        solve::CANON_ENTITY_ALIAS_PROPOSAL_VERSION,
    },
    registry::{PlannedMutationState, acquire_registry_mutation_guard},
    witness,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
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
const REVIEW_IMPORT_ALIAS_PROPOSAL_ALLOWED_ACTIONS: &[&str] = &["accept_alias", "reject_alias"];

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
pub struct NativeReviewGroupDecision {
    pub evidence_signature_id: String,
    pub action: NativeReviewDecisionAction,
    pub operator_id: String,
    pub reason_code: String,
    #[serde(default)]
    pub note: String,
    pub source_review_artifact_hash: String,
    pub run_content_hash: String,
    pub policy_content_hash: String,
    pub registry_snapshot_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_canonical_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewDecisionEnvelope {
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_decisions: Vec<NativeReviewGroupDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<NativeReviewDecision>,
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
        if let Some(group_decisions) = value.get("group_decisions") {
            let empty_group_decisions = group_decisions
                .as_array()
                .is_some_and(|decisions| decisions.is_empty());
            if empty_group_decisions {
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
                return serde_json::from_value(decisions).map_err(native_json_shape_refusal);
            }
            return Err(native_import_refusal(
                "Native review group decisions require the source review artifact for expansion",
                json!({
                    "field": "group_decisions"
                }),
            ));
        }
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

pub fn parse_native_review_import_json_with_source(
    input: &str,
    source_review_artifact: &Value,
) -> Result<Vec<NativeReviewDecision>, Refusal> {
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
        return serde_json::from_value(value).map_err(native_json_shape_refusal);
    }
    if let Some(version) = value.get("version").and_then(Value::as_str)
        && version != CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION
    {
        return Err(native_import_refusal(
            "Native review decision envelope has an unsupported version",
            json!({
                "field": "version",
                "expected": CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION,
                "actual": version
            }),
        ));
    }
    let decisions = value
        .get("decisions")
        .cloned()
        .map(serde_json::from_value::<Vec<NativeReviewDecision>>)
        .transpose()
        .map_err(native_json_shape_refusal)?
        .unwrap_or_default();
    let group_decisions = value
        .get("group_decisions")
        .cloned()
        .map(serde_json::from_value::<Vec<NativeReviewGroupDecision>>)
        .transpose()
        .map_err(native_group_json_shape_refusal)?
        .unwrap_or_default();
    if decisions.is_empty() && group_decisions.is_empty() {
        return Err(native_import_refusal(
            "Native review import contains no decisions",
            json!({
                "field": "decisions"
            }),
        ));
    }
    expand_native_review_group_decisions(source_review_artifact, group_decisions, decisions)
}

pub fn expand_native_review_group_decisions(
    source_review_artifact: &Value,
    group_decisions: Vec<NativeReviewGroupDecision>,
    per_member_decisions: Vec<NativeReviewDecision>,
) -> Result<Vec<NativeReviewDecision>, Refusal> {
    if group_decisions.is_empty() {
        return Ok(per_member_decisions);
    }
    let artifact = validate_native_review_artifact(source_review_artifact)?;
    let members_by_signature = native_members_by_signature(&artifact.review_items);
    let mut group_decisions_by_signature = BTreeMap::new();
    for group_decision in group_decisions {
        validate_native_group_decision_context(&artifact, &group_decision)?;
        if group_decisions_by_signature
            .insert(group_decision.evidence_signature_id.clone(), group_decision)
            .is_some()
        {
            return Err(native_import_refusal(
                "Native review import contains duplicate group decisions",
                json!({
                    "field": "evidence_signature_id"
                }),
            ));
        }
    }

    let mut explicit_decisions_by_review_id = BTreeMap::new();
    for decision in per_member_decisions {
        if explicit_decisions_by_review_id
            .insert(decision.review_id.clone(), decision)
            .is_some()
        {
            return Err(native_import_refusal(
                "Native review import contains duplicate decisions",
                json!({
                    "field": "review_id"
                }),
            ));
        }
    }

    let mut expanded_decisions = BTreeMap::new();
    for (signature_id, group_decision) in group_decisions_by_signature {
        let members = members_by_signature.get(&signature_id).ok_or_else(|| {
            native_import_refusal(
                "Native review group decision references an unknown evidence signature",
                json!({
                    "field": "evidence_signature_id",
                    "evidence_signature_id": signature_id
                }),
            )
        })?;
        for member in members {
            if explicit_decisions_by_review_id.contains_key(&member.review_id) {
                continue;
            }
            let decision = native_decision_from_group_member(&artifact, &group_decision, member)?;
            expanded_decisions.insert(decision.review_id.clone(), decision);
        }
    }

    expanded_decisions.extend(explicit_decisions_by_review_id);
    Ok(expanded_decisions.into_values().collect())
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

fn native_members_by_signature(
    items: &[ExportNativeReviewItem],
) -> BTreeMap<String, Vec<&ExportNativeReviewItem>> {
    let mut members_by_signature = BTreeMap::<String, Vec<&ExportNativeReviewItem>>::new();
    for item in items {
        if item.evidence_signature.signature_id.is_empty() {
            continue;
        }
        members_by_signature
            .entry(item.evidence_signature.signature_id.clone())
            .or_default()
            .push(item);
    }
    for members in members_by_signature.values_mut() {
        members.sort_by(|left, right| left.review_id.cmp(&right.review_id));
    }
    members_by_signature
}

fn validate_native_group_decision_context(
    artifact: &ExportNativeReviewArtifact,
    decision: &NativeReviewGroupDecision,
) -> Result<(), Refusal> {
    require_non_empty("evidence_signature_id", &decision.evidence_signature_id)?;
    require_non_empty("operator_id", &decision.operator_id)?;
    require_non_empty("reason_code", &decision.reason_code)?;
    native_compare_context_field(
        "source_review_artifact_hash",
        &decision.source_review_artifact_hash,
        &artifact.artifact_content_hash,
        &decision.evidence_signature_id,
    )?;
    native_compare_context_field(
        "run_content_hash",
        &decision.run_content_hash,
        &artifact.binding.run_content_hash,
        &decision.evidence_signature_id,
    )?;
    native_compare_context_field(
        "policy_content_hash",
        &decision.policy_content_hash,
        &artifact.binding.policy_content_hash,
        &decision.evidence_signature_id,
    )?;
    native_compare_context_field(
        "registry_snapshot_hash",
        &decision.registry_snapshot_hash,
        &artifact.binding.registry_snapshot_hash,
        &decision.evidence_signature_id,
    )
}

fn native_decision_from_group_member(
    artifact: &ExportNativeReviewArtifact,
    group_decision: &NativeReviewGroupDecision,
    member: &ExportNativeReviewItem,
) -> Result<NativeReviewDecision, Refusal> {
    let mode = native_mode_from_export(member.mode);
    let mode_context =
        native_decision_context_from_export(&member.mode_context, mode, &member.review_id)?;
    Ok(NativeReviewDecision {
        review_id: member.review_id.clone(),
        mode,
        action: group_decision.action,
        operator_id: group_decision.operator_id.clone(),
        reason_code: group_decision.reason_code.clone(),
        note: group_decision.note.clone(),
        source_review_artifact_hash: artifact.artifact_content_hash.clone(),
        decision_binding_hash: member.decision_binding_hash.clone(),
        run_content_hash: artifact.binding.run_content_hash.clone(),
        policy_content_hash: artifact.binding.policy_content_hash.clone(),
        registry_snapshot_hash: artifact.binding.registry_snapshot_hash.clone(),
        mode_context,
        surface_ids: Vec::new(),
        target_canonical_id: group_decision.target_canonical_id.clone(),
        relation: group_decision.relation.clone(),
    })
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
    validate_native_review_signature_derivations(&typed)?;
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

fn validate_native_review_signature_derivations(
    artifact: &ExportNativeReviewArtifact,
) -> Result<(), Refusal> {
    let legacy_without_signatures = artifact.review_groups.is_empty()
        && artifact
            .review_items
            .iter()
            .all(|item| item.evidence_signature.signature_id.is_empty());
    if legacy_without_signatures {
        return Ok(());
    }
    for item in &artifact.review_items {
        let expected = native_evidence_signature_for_item(item).map_err(|refusal| {
            native_import_refusal(
                "Native review artifact evidence signature could not be recomputed",
                json!({
                    "field": "review_items.evidence_signature",
                    "review_id": item.review_id,
                    "source_code": format!("{:?}", refusal.code),
                    "source_detail": refusal.detail
                }),
            )
        })?;
        if item.evidence_signature != expected {
            return Err(native_import_refusal(
                "Native review artifact evidence signature does not match item content",
                json!({
                    "field": "review_items.evidence_signature",
                    "review_id": item.review_id,
                    "expected": expected,
                    "actual": item.evidence_signature.clone()
                }),
            ));
        }
    }
    let expected_groups = build_native_review_signature_groups(&artifact.review_items);
    if artifact.review_groups != expected_groups {
        return Err(native_import_refusal(
            "Native review artifact review_groups do not match deterministic item signatures",
            json!({
                "field": "review_groups",
                "expected": expected_groups,
                "actual": artifact.review_groups.clone()
            }),
        ));
    }
    validate_native_review_signature_group_ids(&artifact.review_groups)
}

fn validate_native_review_signature_group_ids(
    groups: &[ExportNativeReviewEvidenceSignatureGroup],
) -> Result<(), Refusal> {
    let mut seen = BTreeSet::new();
    for group in groups {
        require_non_empty("review_groups.signature_id", &group.signature_id)?;
        if group.signature_id != group.signature.signature_id {
            return Err(native_import_refusal(
                "Native review artifact review group signature id is inconsistent",
                json!({
                    "field": "review_groups.signature_id",
                    "expected": group.signature.signature_id.clone(),
                    "actual": group.signature_id.clone()
                }),
            ));
        }
        if !seen.insert(group.signature_id.clone()) {
            return Err(native_import_refusal(
                "Native review artifact contains duplicate evidence signature groups",
                json!({
                    "field": "review_groups.signature_id",
                    "signature_id": group.signature_id.clone()
                }),
            ));
        }
        if group.member_count == 0 || group.sample_review_ids.is_empty() {
            return Err(native_import_refusal(
                "Native review artifact evidence signature group requires members and samples",
                json!({
                    "field": "review_groups.member_count",
                    "signature_id": group.signature_id.clone()
                }),
            ));
        }
    }
    Ok(())
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

fn native_group_json_shape_refusal(error: serde_json::Error) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Native review JSON has an invalid group decision shape",
        json!({
            "stage": "native_review_import",
            "field": "group_decisions",
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

struct ParsedReviewV1Input {
    source: Value,
    decisions: Vec<Value>,
}

pub fn import_review_v1(request: ReviewImportV1Request<'_>) -> Result<Value, Refusal> {
    let parsed = parse_review_v1_input(request.review_path, request.review_bytes)?;
    let review = parsed.source;
    validate_review_v1_artifact(&review)?;
    validate_entity_v1_self_hash(&review)?;
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
    let mut upstreams = vec![review_import_v0_artifact_reference(&review)?];
    if let Some((audit, _)) = request.audit {
        upstreams.push(review_import_v0_artifact_reference(audit)?);
    }
    let metadata = review_import_v0_metadata_from_review(&review, upstreams)?;
    let decisions = if parsed.decisions.is_empty() {
        reviewed_decisions_from_v1(&review)
    } else {
        parsed.decisions
    };
    let registry_snapshot_before = review_import_registry_snapshot_hash(request.registry)?;
    validate_review_import_registry_binding(
        request.registry,
        &review,
        registry_id,
        version_before,
        &registry_snapshot_before,
    )?;
    let plan = review_import_default_queue_plan_from_v1_decisions(&review, &decisions)?;
    validate_review_v1_audit(
        &review,
        request.audit.map(|(audit, _)| audit),
        plan.requires_audit(),
    )?;
    let mutation = build_review_import_default_queue_mutation(
        request.registry,
        registry_before.clone(),
        next_version,
        &review,
        plan,
        &registry_snapshot_before,
    )?;
    let review_hash = required_value_string(&review, &["artifact_content_hash"], "review hash")?;
    let audit_hash = request
        .audit
        .map(|(audit, _)| required_value_string(audit, &["artifact_content_hash"], "audit hash"))
        .transpose()?;
    let audit_input_hash = request.audit.map(|(_, bytes)| witness::hash_bytes(bytes));
    let registry_snapshot_after = mutation.registry_snapshot_after.clone();
    let registry_version_after = if mutation.write_count > 0 {
        next_version
    } else {
        version_before
    };
    let mut artifact = json!({
        "version": CANON_ENTITY_REVIEW_IMPORT_VERSION,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "decisions": decisions.len() as u64,
                "reviewed_decisions": decisions.len() as u64,
                "source_review_items": value_u64_or(&review, &["summary", "counts", "review_items"], 0),
                "accepted_aliases": mutation.alias_count,
                "trusted_anchors": mutation.anchor_count,
                "pending_escrows": mutation.pending_count,
                "cannot_links": mutation.cannot_link_count,
                "no_write_decisions": mutation.no_write_decisions,
                "registry_writes": mutation.write_count,
                "registry_entries_before": mutation.entry_count_after.saturating_sub(mutation.alias_count),
                "registry_entries_after": mutation.entry_count_after
            },
            "labels": {
                "stage": "review_import",
                "operation": "default_queue_import",
                "import_status": if mutation.write_count > 0 { "applied" } else { "accepted" },
                "registry_id": registry_id,
                "registry_version_before": version_before,
                "registry_version_after": registry_version_after,
                "registry_snapshot_hash_before": registry_snapshot_before,
                "registry_snapshot_hash_after": registry_snapshot_after,
                "review_input_hash": witness::hash_bytes(request.review_bytes),
                "audit_input_hash": audit_input_hash.unwrap_or_else(|| "none".to_string()),
                "audit_artifact_hash": audit_hash.unwrap_or("none"),
                "alias_patch_hash_before": mutation.alias_hash_before.clone(),
                "alias_patch_hash_after": mutation.alias_hash_after.clone(),
                "anchor_patch_hash_before": mutation.anchor_hash_before.clone(),
                "anchor_patch_hash_after": mutation.anchor_hash_after.clone(),
                "pending_escrow_hash_before": mutation.pending_hash_before.clone(),
                "pending_escrow_hash_after": mutation.pending_hash_after.clone(),
                "cannot_link_hash_before": mutation.cannot_link_hash_before.clone(),
                "cannot_link_hash_after": mutation.cannot_link_hash_after.clone()
            }
        },
        "source_review_queue_hash": review_hash,
        "decisions": decisions
    });
    finalize_review_import_receipt_hash(&mut artifact)?;
    validate_review_import_receipt_contract(&artifact)?;
    if mutation.write_count > 0 {
        commit_review_import_default_queue_mutation(&mutation)?;
    }
    Ok(artifact)
}

pub fn render_review_import_v1_summary(artifact: &Value) -> String {
    let registry = value_string_or(
        artifact,
        &["summary", "labels", "registry_id"],
        "<registry>",
    );
    let before = value_string_or(
        artifact,
        &["summary", "labels", "registry_version_before"],
        "<before>",
    );
    let after = value_string_or(
        artifact,
        &["summary", "labels", "registry_version_after"],
        "<after>",
    );
    let decisions = value_u64_or(artifact, &["summary", "counts", "reviewed_decisions"], 0);
    format!("{registry} review import {before} -> {after} decisions={decisions}")
}

fn review_import_v0_artifact_reference(artifact: &Value) -> Result<Value, Refusal> {
    Ok(json!({
        "version": required_value_string(artifact, &["version"], "version")?,
        "content_hash": required_value_string(
            artifact,
            &["artifact_content_hash"],
            "artifact_content_hash",
        )?
    }))
}

fn review_import_v0_metadata_from_review(
    review: &Value,
    upstream_artifacts: Vec<Value>,
) -> Result<Value, Refusal> {
    let source = review
        .get("metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import source artifact is missing metadata",
                json!({
                    "stage": "review_import",
                    "field": "metadata",
                    "writes_performed": false
                }),
            )
        })?;
    let mut metadata = serde_json::Map::new();
    for field in [
        "profile",
        "strategy",
        "registry_snapshot",
        "input",
        "patch_namespace",
        "patch_set",
        "namekit",
    ] {
        let value = source.get(field).cloned().ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import source metadata is missing a required field",
                json!({
                    "stage": "review_import",
                    "field": format!("metadata.{field}"),
                    "writes_performed": false
                }),
            )
        })?;
        metadata.insert(field.to_string(), value);
    }
    metadata.insert(
        "upstream_artifacts".to_string(),
        Value::Array(upstream_artifacts),
    );
    metadata.insert(
        "artifact_content_hash".to_string(),
        Value::String(String::new()),
    );
    Ok(Value::Object(metadata))
}

fn finalize_review_import_receipt_hash(artifact: &mut Value) -> Result<String, Refusal> {
    artifact["artifact_content_hash"] = Value::String(String::new());
    artifact["metadata"]["artifact_content_hash"] = Value::String(String::new());
    let bytes = serde_json::to_vec(artifact).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import could not serialize receipt for hashing",
            json!({
                "stage": "review_import",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let hash = witness::hash_bytes(&bytes);
    artifact["artifact_content_hash"] = Value::String(hash.clone());
    artifact["metadata"]["artifact_content_hash"] = Value::String(hash.clone());
    Ok(hash)
}

fn validate_review_import_receipt_contract(artifact: &Value) -> Result<(), Refusal> {
    let object = artifact.as_object().ok_or_else(|| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import receipt must be a JSON object",
            json!({
                "stage": "review_import",
                "field": "$",
                "writes_performed": false
            }),
        )
    })?;
    let expected = BTreeSet::from([
        "artifact_content_hash",
        "decisions",
        "metadata",
        "source_review_queue_hash",
        "summary",
        "version",
    ]);
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import receipt does not match the advertised schema top-level fields",
            json!({
                "stage": "review_import",
                "field": "$",
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ));
    }
    let schema = validate_artifact_core_contract(artifact)?;
    if schema.artifact_version != CANON_ENTITY_REVIEW_IMPORT_VERSION {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import receipt has the wrong contract version",
            json!({
                "stage": "review_import",
                "field": "version",
                "expected": CANON_ENTITY_REVIEW_IMPORT_VERSION,
                "actual": schema.artifact_version,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn parse_review_v1_input(path: &Path, bytes: &[u8]) -> Result<ParsedReviewV1Input, Refusal> {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        return Ok(ParsedReviewV1Input {
            decisions: reviewed_decisions_from_v1(&value),
            source: value,
        });
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

fn parse_review_v1_csv(input: &str) -> Result<ParsedReviewV1Input, Refusal> {
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
    let operator_index = headers.iter().position(|header| header == "operator_id");
    let reason_index = headers.iter().position(|header| header == "reason_code");
    let surfaces_index = headers
        .iter()
        .position(|header| header == "surface_ids_json");
    let mut context = None;
    let mut source_items = Vec::new();
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
            let source_item =
                serde_json::from_str::<Value>(&record[index]).unwrap_or_else(|_| json!({}));
            source_items.push(source_item.clone());
            let mut item = source_item;
            if let Some(surface_index) = surfaces_index
                && let Some(surfaces) = record.get(surface_index).filter(|value| !value.is_empty())
                && let Ok(surface_ids) = serde_json::from_str::<Value>(surfaces)
            {
                item["surface_ids"] = surface_ids;
            }
            if let Some(decision_index) = decision_index
                && let Some(decision) = record.get(decision_index).filter(|value| !value.is_empty())
            {
                item["decision"] = Value::String(decision.to_string());
            }
            if let Some(operator_index) = operator_index
                && let Some(operator_id) =
                    record.get(operator_index).filter(|value| !value.is_empty())
            {
                item["operator_id"] = Value::String(operator_id.to_string());
            }
            if let Some(reason_index) = reason_index
                && let Some(reason_code) =
                    record.get(reason_index).filter(|value| !value.is_empty())
            {
                item["reason_code"] = Value::String(reason_code.to_string());
            }
            if item
                .get("decision")
                .and_then(Value::as_str)
                .is_some_and(|decision| !decision.trim().is_empty())
            {
                decisions.push(item);
            }
        }
    }
    let mut source = context.ok_or_else(|| {
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
    if source.get("artifact").is_some() || source.get("review_items").is_some() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review v1 CSV context must not embed the full artifact or review items",
            json!({
                "stage": "review_import",
                "field": "review_context_json",
                "writes_performed": false
            }),
        ));
    }
    source["review_items"] = Value::Array(source_items);
    Ok(ParsedReviewV1Input { source, decisions })
}

fn validate_review_v1_audit(
    review: &Value,
    audit: Option<&Value>,
    required: bool,
) -> Result<(), Refusal> {
    let Some(audit) = audit else {
        if required {
            return Err(review_import_refusal(
                EntityRefusalKind::AuditGate,
                "Review import alias decisions require a passing audit artifact",
                json!({
                    "stage": "review_import",
                    "field": "audit",
                    "writes_performed": false
                }),
            ));
        }
        return Ok(());
    };
    let contract = validate_artifact_v1_core_contract(audit)?;
    validate_entity_v1_self_hash(audit)?;
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
    let review_source_hash = required_value_string(
        review,
        &["source_result", "content_hash"],
        "source_result.content_hash",
    )?;
    let audited_hash = required_value_string(
        audit,
        &["audited_artifact", "content_hash"],
        "audit.audited_artifact.content_hash",
    )?;
    if review_source_hash != audited_hash {
        return Err(review_import_refusal(
            EntityRefusalKind::AuditGate,
            "Review import audit does not match the reviewed source result",
            json!({
                "stage": "review_import",
                "field": "audited_artifact.content_hash",
                "expected": review_source_hash,
                "actual": audited_hash,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ReviewImportAliasEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ReviewImportTrustedAnchorRecord {
    canonical_id: String,
    namespace: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ReviewImportPendingEscrowRecord {
    escrow_id: String,
    profile_id: String,
    identity_semantics: String,
    surface_ids: Vec<String>,
    reason: String,
    source_decision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ReviewImportCannotLinkRecord {
    sidecar_id: String,
    profile_id: String,
    identity_semantics: String,
    left: String,
    right: String,
    hard_cannot_link: bool,
    reason: String,
    review_decision_id: String,
    source_event_hash: String,
}

#[derive(Debug, Clone, Default)]
struct ReviewImportDefaultQueuePlan {
    aliases: Vec<ReviewImportAliasEntry>,
    anchors: Vec<ReviewImportTrustedAnchorRecord>,
    pending_escrows: Vec<ReviewImportPendingEscrowRecord>,
    cannot_links: Vec<ReviewImportCannotLinkRecord>,
    no_write_decisions: u64,
}

impl ReviewImportDefaultQueuePlan {
    fn alias_count(&self) -> u64 {
        self.aliases.len() as u64
    }

    fn anchor_count(&self) -> u64 {
        self.anchors.len() as u64
    }

    fn pending_count(&self) -> u64 {
        self.pending_escrows.len() as u64
    }

    fn cannot_link_count(&self) -> u64 {
        self.cannot_links.len() as u64
    }

    fn requires_audit(&self) -> bool {
        !self.aliases.is_empty() || !self.anchors.is_empty()
    }
}

#[derive(Debug)]
struct ReviewImportPlannedFile {
    path: PathBuf,
    existed_before: bool,
    bytes_before: Vec<u8>,
    bytes_after: Vec<u8>,
    hash_before: String,
    hash_after: String,
}

#[derive(Debug)]
struct ReviewImportDefaultQueueMutation {
    registry_dir: PathBuf,
    registry_path: PathBuf,
    registry_snapshot_before: String,
    registry_snapshot_after: String,
    files: Vec<ReviewImportPlannedFile>,
    alias_hash_before: String,
    alias_hash_after: String,
    anchor_hash_before: String,
    anchor_hash_after: String,
    pending_hash_before: String,
    pending_hash_after: String,
    cannot_link_hash_before: String,
    cannot_link_hash_after: String,
    alias_count: u64,
    anchor_count: u64,
    pending_count: u64,
    cannot_link_count: u64,
    no_write_decisions: u64,
    write_count: u64,
    entry_count_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewImportAliasProposal {
    proposal_id: String,
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
    component_id: String,
    source_surface_ids: Vec<String>,
}

#[cfg(test)]
fn reviewed_aliases_from_v1_decisions(
    review: &Value,
    decisions: &[Value],
) -> Result<Vec<ReviewImportAliasEntry>, Refusal> {
    Ok(review_import_default_queue_plan_from_v1_decisions(review, decisions)?.aliases)
}

fn review_import_default_queue_plan_from_v1_decisions(
    review: &Value,
    decisions: &[Value],
) -> Result<ReviewImportDefaultQueuePlan, Refusal> {
    let items = review_items_by_review_id(review)?;
    let canonical_type = required_value_string(
        review,
        &["metadata", "profile", "canonical_type"],
        "metadata.profile.canonical_type",
    )?;
    let profile_id = required_value_string(
        review,
        &["metadata", "profile", "id"],
        "metadata.profile.id",
    )?;
    let identity_semantics = required_value_string(
        review,
        &["metadata", "profile", "identity_semantics"],
        "metadata.profile.identity_semantics",
    )?;
    let mut seen_reviews = BTreeSet::new();
    let mut seen_inputs = BTreeSet::new();
    let mut seen_anchor_keys = BTreeMap::<(String, String), String>::new();
    let mut plan = ReviewImportDefaultQueuePlan::default();
    for decision in decisions {
        let action = value_string(decision, "decision").unwrap_or_default();
        let normalized_action = normalize_review_import_action(&action);
        if normalized_action == "accept_aliases" {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import does not support plural accept_aliases for this contract",
                json!({
                    "stage": "review_import",
                    "field": "decision",
                    "decision": action,
                    "expected": "accept_alias",
                    "writes_performed": false
                }),
            ));
        }
        let review_id = required_decision_string(decision, "review_id")?;
        if !seen_reviews.insert(review_id.clone()) {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import contains duplicate decisions",
                json!({
                    "stage": "review_import",
                    "field": "review_id",
                    "review_id": review_id,
                    "writes_performed": false
                }),
            ));
        }
        let item = items.get(&review_id).ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import references an unknown review item",
                json!({
                    "stage": "review_import",
                    "field": "review_id",
                    "review_id": review_id,
                    "writes_performed": false
                }),
            )
        })?;
        validate_decision_surface_binding(&review_id, decision, item)?;
        require_decision_string(decision, "operator_id", &review_id)?;
        require_decision_string(decision, "reason_code", &review_id)?;

        match normalized_action.as_str() {
            "accept_alias" => {
                validate_accept_alias_review_item(&review_id, item)?;
                let proposals =
                    alias_proposals_from_review_item(&review_id, item, canonical_type, &action)?;
                validate_decision_alias_fields_match_item(&review_id, decision, &proposals)?;
                for proposal in &proposals {
                    if !seen_inputs.insert(proposal.input.clone()) {
                        return Err(review_import_refusal(
                            EntityRefusalKind::ReviewImport,
                            "Review import contains duplicate alias inputs",
                            json!({
                                "stage": "review_import",
                                "field": "alias_input",
                                "input": proposal.input,
                                "writes_performed": false
                            }),
                        ));
                    }
                    plan.aliases.push(ReviewImportAliasEntry {
                        input: proposal.input.clone(),
                        canonical_id: proposal.canonical_id.clone(),
                        canonical_type: proposal.canonical_type.clone(),
                        rule_id: proposal.rule_id.clone(),
                    });
                }
                let canonical_id = single_alias_canonical_id(&review_id, &proposals)?;
                extend_trusted_anchor_plan(
                    &mut plan,
                    &mut seen_anchor_keys,
                    &review_id,
                    item,
                    &canonical_id,
                )?;
            }
            "accept_anchor" | "accept_trusted_anchor" => {
                let anchor_start = plan.anchors.len();
                let canonical_id = decision_canonical_id(&review_id, decision, item)?;
                extend_trusted_anchor_plan(
                    &mut plan,
                    &mut seen_anchor_keys,
                    &review_id,
                    item,
                    &canonical_id,
                )?;
                if plan.anchors.len() == anchor_start {
                    return Err(review_import_refusal(
                        EntityRefusalKind::ReviewImport,
                        "Trusted-anchor decision does not carry anchor records",
                        json!({
                            "stage": "review_import",
                            "field": "anchors",
                            "review_id": review_id,
                            "writes_performed": false
                        }),
                    ));
                }
            }
            "create_pending" => {
                plan.pending_escrows
                    .push(pending_escrow_record_from_decision(
                        profile_id,
                        identity_semantics,
                        &review_id,
                        decision,
                        item,
                    )?);
            }
            "emit_cannot_link" => {
                plan.cannot_links.push(cannot_link_record_from_decision(
                    profile_id,
                    identity_semantics,
                    &review_id,
                    decision,
                    item,
                )?);
            }
            "reject_alias" | "defer" | "no_action" => {
                plan.no_write_decisions += 1;
            }
            _ => {
                return Err(review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Review import decision action is not supported by the default queue importer",
                    json!({
                        "stage": "review_import",
                        "field": "decision",
                        "review_id": review_id,
                        "decision": action,
                        "supported": [
                            "accept_alias",
                            "accept_anchor",
                            "accept_trusted_anchor",
                            "create_pending",
                            "emit_cannot_link",
                            "reject_alias",
                            "defer",
                            "no_action"
                        ],
                        "writes_performed": false
                    }),
                ));
            }
        }
    }
    plan.aliases.sort();
    plan.anchors.sort();
    plan.pending_escrows.sort();
    plan.cannot_links.sort();
    Ok(plan)
}

fn single_alias_canonical_id(
    review_id: &str,
    proposals: &[ReviewImportAliasProposal],
) -> Result<String, Refusal> {
    let values = unique_proposal_values(proposals, |proposal| &proposal.canonical_id);
    if values.len() == 1 {
        Ok(values[0].clone())
    } else {
        Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item must resolve to one canonical ID before anchor promotion",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.canonical_id",
                "review_id": review_id,
                "actual": values,
                "writes_performed": false
            }),
        ))
    }
}

fn extend_trusted_anchor_plan(
    plan: &mut ReviewImportDefaultQueuePlan,
    seen_anchor_keys: &mut BTreeMap<(String, String), String>,
    review_id: &str,
    item: &Value,
    canonical_id: &str,
) -> Result<(), Refusal> {
    for anchor in trusted_anchor_records_from_item(review_id, item, canonical_id)? {
        let key = (anchor.namespace.clone(), anchor.value.clone());
        if let Some(existing_canonical_id) = seen_anchor_keys.get(&key) {
            if existing_canonical_id != &anchor.canonical_id {
                return Err(review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Review import would emit conflicting trusted anchors in one batch",
                    json!({
                        "stage": "review_import",
                        "field": "anchors",
                        "review_id": review_id,
                        "namespace": key.0,
                        "value": key.1,
                        "existing_canonical_id": existing_canonical_id,
                        "new_canonical_id": anchor.canonical_id,
                        "writes_performed": false
                    }),
                ));
            }
            continue;
        }
        seen_anchor_keys.insert(key, anchor.canonical_id.clone());
        plan.anchors.push(anchor);
    }
    Ok(())
}

fn trusted_anchor_records_from_item(
    review_id: &str,
    item: &Value,
    canonical_id: &str,
) -> Result<Vec<ReviewImportTrustedAnchorRecord>, Refusal> {
    let mut anchors = Vec::new();
    anchors.extend(trusted_anchor_records_from_array_field(
        review_id,
        item,
        "anchors",
        canonical_id,
    )?);
    anchors.extend(trusted_anchor_records_from_array_field(
        review_id,
        item,
        "trusted_anchors",
        canonical_id,
    )?);
    if let (Some(namespace), Some(value)) = (
        value_string(item, "anchor_namespace"),
        value_string(item, "anchor_value"),
    ) {
        anchors.push(ReviewImportTrustedAnchorRecord {
            canonical_id: canonical_id.to_string(),
            namespace,
            value,
        });
    }
    anchors.sort();
    anchors.dedup();
    Ok(anchors)
}

fn trusted_anchor_records_from_array_field(
    review_id: &str,
    item: &Value,
    field: &str,
    canonical_id: &str,
) -> Result<Vec<ReviewImportTrustedAnchorRecord>, Refusal> {
    let Some(values) = item.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = values.as_array() else {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import trusted anchors must be arrays",
            json!({
                "stage": "review_import",
                "field": field,
                "review_id": review_id,
                "writes_performed": false
            }),
        ));
    };
    values
        .iter()
        .map(|value| {
            let namespace = value_string(value, "namespace").ok_or_else(|| {
                review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Review import trusted anchor is missing namespace",
                    json!({
                        "stage": "review_import",
                        "field": format!("{field}.namespace"),
                        "review_id": review_id,
                        "writes_performed": false
                    }),
                )
            })?;
            let anchor_value = value_string(value, "value")
                .or_else(|| value_string(value, "anchor"))
                .ok_or_else(|| {
                    review_import_refusal(
                        EntityRefusalKind::ReviewImport,
                        "Review import trusted anchor is missing value",
                        json!({
                            "stage": "review_import",
                            "field": format!("{field}.value"),
                            "review_id": review_id,
                            "writes_performed": false
                        }),
                    )
                })?;
            Ok(ReviewImportTrustedAnchorRecord {
                canonical_id: canonical_id.to_string(),
                namespace,
                value: anchor_value,
            })
        })
        .collect()
}

fn decision_canonical_id(
    review_id: &str,
    decision: &Value,
    item: &Value,
) -> Result<String, Refusal> {
    value_string(decision, "target_canonical_id")
        .or_else(|| value_string(decision, "canonical_id"))
        .or_else(|| value_string(item, "target_canonical_id"))
        .or_else(|| value_string(item, "canonical_id"))
        .ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Trusted-anchor decision requires a canonical ID",
                json!({
                    "stage": "review_import",
                    "field": "canonical_id",
                    "review_id": review_id,
                    "writes_performed": false
                }),
            )
        })
}

fn pending_escrow_record_from_decision(
    profile_id: &str,
    identity_semantics: &str,
    review_id: &str,
    decision: &Value,
    item: &Value,
) -> Result<ReviewImportPendingEscrowRecord, Refusal> {
    let mut surface_ids = string_array_field(item, "surface_ids")?;
    surface_ids.sort();
    surface_ids.dedup();
    if surface_ids.is_empty() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Pending-escrow decision requires bound surfaces",
            json!({
                "stage": "review_import",
                "field": "surface_ids",
                "review_id": review_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(ReviewImportPendingEscrowRecord {
        escrow_id: value_string(decision, "escrow_id")
            .or_else(|| value_string(item, "escrow_id"))
            .unwrap_or_else(|| review_import_stable_id("pending", &[review_id])),
        profile_id: profile_id.to_string(),
        identity_semantics: identity_semantics.to_string(),
        surface_ids,
        reason: value_string(decision, "reason_code").unwrap_or_else(|| "review".to_string()),
        source_decision_id: review_id.to_string(),
    })
}

fn cannot_link_record_from_decision(
    profile_id: &str,
    identity_semantics: &str,
    review_id: &str,
    decision: &Value,
    item: &Value,
) -> Result<ReviewImportCannotLinkRecord, Refusal> {
    let (left, right) = cannot_link_surfaces(review_id, decision, item)?;
    Ok(ReviewImportCannotLinkRecord {
        sidecar_id: review_import_stable_id("cannot_link", &[review_id, &left, &right]),
        profile_id: profile_id.to_string(),
        identity_semantics: identity_semantics.to_string(),
        left,
        right,
        hard_cannot_link: true,
        reason: value_string(decision, "reason_code").unwrap_or_else(|| "review".to_string()),
        review_decision_id: review_id.to_string(),
        source_event_hash: value_string(decision, "decision_binding_hash")
            .unwrap_or_else(|| review_import_value_hash(decision)),
    })
}

fn cannot_link_surfaces(
    review_id: &str,
    decision: &Value,
    item: &Value,
) -> Result<(String, String), Refusal> {
    let explicit_left = value_string(decision, "left_surface_id")
        .or_else(|| value_string(decision, "left"))
        .or_else(|| value_string(item, "left_surface_id"))
        .or_else(|| value_string(item, "left"));
    let explicit_right = value_string(decision, "right_surface_id")
        .or_else(|| value_string(decision, "right"))
        .or_else(|| value_string(item, "right_surface_id"))
        .or_else(|| value_string(item, "right"));
    if let (Some(left), Some(right)) = (explicit_left, explicit_right) {
        if left == right {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Cannot-link decision cannot bind a surface to itself",
                json!({
                    "stage": "review_import",
                    "field": "surface_ids",
                    "review_id": review_id,
                    "surface_id": left,
                    "writes_performed": false
                }),
            ));
        }
        return Ok(sorted_pair(left, right));
    }
    let mut surface_ids = string_array_field(item, "surface_ids")?;
    surface_ids.sort();
    surface_ids.dedup();
    if surface_ids.len() != 2 {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Cannot-link decision requires explicit left/right surfaces or exactly two bound surfaces",
            json!({
                "stage": "review_import",
                "field": "surface_ids",
                "review_id": review_id,
                "actual_count": surface_ids.len(),
                "writes_performed": false
            }),
        ));
    }
    Ok((surface_ids[0].clone(), surface_ids[1].clone()))
}

fn sorted_pair(left: String, right: String) -> (String, String) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn review_import_stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    let digest = hasher.finalize().to_hex();
    format!("{prefix}:{}", &digest[..16])
}

fn review_import_value_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    witness::hash_bytes(&bytes)
}

fn review_items_by_review_id(review: &Value) -> Result<BTreeMap<String, Value>, Refusal> {
    let mut items = BTreeMap::new();
    for item in review
        .get("review_items")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
    {
        let review_id = value_string(item, "review_id").ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review artifact item is missing a review_id",
                json!({
                    "stage": "review_import",
                    "field": "review_id",
                    "writes_performed": false
                }),
            )
        })?;
        if items.insert(review_id.clone(), item.clone()).is_some() {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review artifact contains duplicate review IDs",
                json!({
                    "stage": "review_import",
                    "field": "review_id",
                    "review_id": review_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(items)
}

fn normalize_review_import_action(action: &str) -> String {
    action.trim().replace('-', "_")
}

fn validate_accept_alias_review_item(review_id: &str, item: &Value) -> Result<(), Refusal> {
    let state = value_string(item, "state").unwrap_or_default();
    if matches!(state.as_str(), "conflict" | "contradiction") {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias decision cannot be applied to conflict or contradiction review items",
            json!({
                "stage": "review_import",
                "field": "state",
                "review_id": review_id,
                "actual": state,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn alias_proposals_from_review_item(
    review_id: &str,
    item: &Value,
    profile_canonical_type: &str,
    action: &str,
) -> Result<Vec<ReviewImportAliasProposal>, Refusal> {
    if item.get("alias_proposals").is_some() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item must use singular alias_proposal for this contract",
            json!({
                "stage": "review_import",
                "field": "alias_proposals",
                "review_id": review_id,
                "writes_performed": false
            }),
        ));
    }
    let value = item.get("alias_proposal").ok_or_else(|| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item does not carry a hash-bound alias proposal",
            json!({
                "stage": "review_import",
                "field": "alias_proposal",
                "review_id": review_id,
                "writes_performed": false
            }),
        )
    })?;
    let item_surfaces = string_array_field(item, "surface_ids")?;
    let proposal = alias_proposal_from_value(review_id, value, profile_canonical_type, action)?;
    validate_alias_proposal_surface_binding(review_id, &proposal, &item_surfaces)?;
    Ok(vec![proposal])
}

fn alias_proposal_from_value(
    review_id: &str,
    value: &Value,
    profile_canonical_type: &str,
    action: &str,
) -> Result<ReviewImportAliasProposal, Refusal> {
    validate_alias_proposal_hash(review_id, value)?;
    let version = value_string(value, "version")
        .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.version"))?;
    if version != CANON_ENTITY_ALIAS_PROPOSAL_VERSION {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item uses an unsupported alias proposal version",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.version",
                "review_id": review_id,
                "expected": CANON_ENTITY_ALIAS_PROPOSAL_VERSION,
                "actual": version,
                "writes_performed": false
            }),
        ));
    }
    validate_alias_proposal_allowed_action(review_id, value, action)?;
    let proposal_id = validate_review_import_alias_text(
        "alias_proposal.proposal_id",
        &value_string(value, "proposal_id")
            .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.proposal_id"))?,
        false,
    )?;
    let input = validate_review_import_alias_text(
        "alias_proposal.input",
        &value_string(value, "input")
            .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.input"))?,
        true,
    )?;
    let canonical_id = validate_review_import_alias_text(
        "alias_proposal.canonical_id",
        &value_string(value, "canonical_id").ok_or_else(|| {
            missing_alias_proposal_field(review_id, "alias_proposal.canonical_id")
        })?,
        false,
    )?;
    let canonical_type = validate_review_import_alias_text(
        "alias_proposal.canonical_type",
        &value_string(value, "canonical_type").ok_or_else(|| {
            missing_alias_proposal_field(review_id, "alias_proposal.canonical_type")
        })?,
        false,
    )?;
    if canonical_type != profile_canonical_type {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias proposal canonical_type conflicts with the review profile",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.canonical_type",
                "review_id": review_id,
                "expected": profile_canonical_type,
                "actual": canonical_type,
                "writes_performed": false
            }),
        ));
    }
    let rule_id = validate_review_import_alias_text(
        "alias_proposal.rule_id",
        &value_string(value, "rule_id")
            .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.rule_id"))?,
        false,
    )?;
    let component_id = validate_review_import_alias_text(
        "alias_proposal.component_id",
        &value_string(value, "component_id").ok_or_else(|| {
            missing_alias_proposal_field(review_id, "alias_proposal.component_id")
        })?,
        false,
    )?;
    let source_surface_ids = string_array_field(value, "source_surface_ids")?;
    if source_surface_ids.is_empty() {
        return Err(missing_alias_proposal_field(
            review_id,
            "alias_proposal.source_surface_ids",
        ));
    }
    Ok(ReviewImportAliasProposal {
        proposal_id,
        input,
        canonical_id,
        canonical_type,
        rule_id,
        component_id,
        source_surface_ids,
    })
}

fn missing_alias_proposal_field(review_id: &str, field: &str) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Accept-alias review item alias proposal is missing a required field",
        json!({
            "stage": "review_import",
            "field": field,
            "review_id": review_id,
            "writes_performed": false
        }),
    )
}

fn validate_alias_proposal_hash(review_id: &str, proposal: &Value) -> Result<(), Refusal> {
    let actual = value_string(proposal, "content_hash")
        .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.content_hash"))?;
    let expected = alias_proposal_content_hash(proposal)?;
    if actual != expected {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item alias proposal hash is stale or tampered",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.content_hash",
                "review_id": review_id,
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ));
    }
    let proposal_id = value_string(proposal, "proposal_id")
        .ok_or_else(|| missing_alias_proposal_field(review_id, "alias_proposal.proposal_id"))?;
    let expected_id = format!("alias_proposal:{expected}");
    if proposal_id != expected_id {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item alias proposal ID does not match its content hash",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.proposal_id",
                "review_id": review_id,
                "expected": expected_id,
                "actual": proposal_id,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn alias_proposal_content_hash(proposal: &Value) -> Result<String, Refusal> {
    if !proposal.is_object() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias review item alias proposal must be an object",
            json!({
                "stage": "review_import",
                "field": "alias_proposal",
                "writes_performed": false
            }),
        ));
    }
    let hashable = json!({
        "version": value_string(proposal, "version").unwrap_or_default(),
        "input": value_string(proposal, "input").unwrap_or_default(),
        "canonical_id": value_string(proposal, "canonical_id").unwrap_or_default(),
        "canonical_type": value_string(proposal, "canonical_type").unwrap_or_default(),
        "rule_id": value_string(proposal, "rule_id").unwrap_or_default(),
        "component_id": value_string(proposal, "component_id").unwrap_or_default(),
        "source_surface_ids": string_array_field(proposal, "source_surface_ids")?,
        "allowed_actions": string_array_field(proposal, "allowed_actions")?
    });
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import could not hash alias proposal",
            json!({
                "stage": "review_import",
                "field": "alias_proposal",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn validate_alias_proposal_allowed_action(
    review_id: &str,
    proposal: &Value,
    action: &str,
) -> Result<(), Refusal> {
    let normalized_action = normalize_review_import_action(action);
    let allowed_values = string_array_field(proposal, "allowed_actions")?;
    let expected_allowed = REVIEW_IMPORT_ALIAS_PROPOSAL_ALLOWED_ACTIONS
        .iter()
        .map(|action| (*action).to_string())
        .collect::<Vec<_>>();
    if allowed_values != expected_allowed {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias proposal allowed_actions do not match the canonical contract",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.allowed_actions",
                "review_id": review_id,
                "expected": expected_allowed,
                "actual": allowed_values,
                "writes_performed": false
            }),
        ));
    }
    if !allowed_values
        .iter()
        .any(|action| action == &normalized_action)
    {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias decision is not allowed by the exported alias proposal",
            json!({
                "stage": "review_import",
                "field": "alias_proposal.allowed_actions",
                "review_id": review_id,
                "decision": normalized_action,
                "allowed_actions": allowed_values,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_alias_proposal_surface_binding(
    review_id: &str,
    proposal: &ReviewImportAliasProposal,
    item_surfaces: &[String],
) -> Result<(), Refusal> {
    let item_surface_set = item_surfaces.iter().collect::<BTreeSet<_>>();
    let mut source_surface_set = BTreeSet::new();
    for surface_id in &proposal.source_surface_ids {
        if !item_surface_set.contains(surface_id) || !source_surface_set.insert(surface_id) {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Accept-alias proposal source surfaces must be unique and bound to the review item",
                json!({
                    "stage": "review_import",
                    "field": "alias_proposal.source_surface_ids",
                    "review_id": review_id,
                    "proposal_id": proposal.proposal_id,
                    "source_surface_id": surface_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn validate_decision_alias_fields_match_item(
    review_id: &str,
    decision: &Value,
    proposals: &[ReviewImportAliasProposal],
) -> Result<(), Refusal> {
    let proposal_ids = unique_proposal_values(proposals, |proposal| &proposal.proposal_id);
    if let Some(proposal_id) = value_string(decision, "alias_proposal_id") {
        validate_decision_single_value_match(
            review_id,
            "alias_proposal_id",
            proposal_id,
            &proposal_ids,
        )?;
    }
    if let Some(value) = decision.get("alias_proposal_ids") {
        let actual = alias_inputs_from_value(value, "alias_proposal_ids", review_id)?;
        validate_decision_value_set_match(review_id, "alias_proposal_ids", actual, &proposal_ids)?;
    }
    let canonical_ids = unique_proposal_values(proposals, |proposal| &proposal.canonical_id);
    validate_optional_decision_field_matches_values(
        review_id,
        decision,
        "target_canonical_id",
        &canonical_ids,
    )?;
    validate_optional_decision_field_matches_values(
        review_id,
        decision,
        "canonical_id",
        &canonical_ids,
    )?;
    let canonical_types = unique_proposal_values(proposals, |proposal| &proposal.canonical_type);
    validate_optional_decision_field_matches_values(
        review_id,
        decision,
        "canonical_type",
        &canonical_types,
    )?;
    let rule_ids = unique_proposal_values(proposals, |proposal| &proposal.rule_id);
    validate_optional_decision_field_matches_values(review_id, decision, "rule_id", &rule_ids)?;
    let proposal_inputs = unique_proposal_values(proposals, |proposal| &proposal.input);
    if let Some(input) = value_string(decision, "alias_input") {
        let actual = validate_review_import_alias_text("alias_input", &input, true)?;
        validate_decision_single_value_match(review_id, "alias_input", actual, &proposal_inputs)?;
    }
    if let Some(value) = decision.get("alias_inputs") {
        let actual = alias_inputs_from_value(value, "alias_inputs", review_id)?
            .into_iter()
            .map(|input| validate_review_import_alias_text("alias_input", &input, true))
            .collect::<Result<Vec<_>, _>>()?;
        validate_decision_value_set_match(review_id, "alias_inputs", actual, &proposal_inputs)?;
    }
    Ok(())
}

fn unique_proposal_values(
    proposals: &[ReviewImportAliasProposal],
    value: impl Fn(&ReviewImportAliasProposal) -> &String,
) -> Vec<String> {
    proposals
        .iter()
        .map(value)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validate_optional_decision_field_matches_values(
    review_id: &str,
    decision: &Value,
    field: &str,
    expected: &[String],
) -> Result<(), Refusal> {
    let Some(actual) = value_string(decision, field) else {
        return Ok(());
    };
    validate_decision_single_value_match(review_id, field, actual, expected)
}

fn validate_decision_single_value_match(
    review_id: &str,
    field: &str,
    actual: String,
    expected: &[String],
) -> Result<(), Refusal> {
    if expected.len() != 1 || expected.first() != Some(&actual) {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias decision field does not match the exported review item",
            json!({
                "stage": "review_import",
                "field": field,
                "review_id": review_id,
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_decision_value_set_match(
    review_id: &str,
    field: &str,
    mut actual: Vec<String>,
    expected: &[String],
) -> Result<(), Refusal> {
    actual.sort();
    actual.dedup();
    if actual != expected {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Accept-alias decision alias inputs do not match the exported review item",
            json!({
                "stage": "review_import",
                "field": field,
                "review_id": review_id,
                "expected": expected,
                "actual": actual,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn alias_inputs_from_value(
    value: &Value,
    field: &str,
    review_id: &str,
) -> Result<Vec<String>, Refusal> {
    match value {
        Value::String(input) => Ok(vec![input.clone()]),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_string).ok_or_else(|| {
                    review_import_refusal(
                        EntityRefusalKind::ReviewImport,
                        "Review import alias input entries must be strings",
                        json!({
                            "stage": "review_import",
                            "field": field,
                            "review_id": review_id,
                            "writes_performed": false
                        }),
                    )
                })
            })
            .collect(),
        _ => Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import alias inputs must be a string or string array",
            json!({
                "stage": "review_import",
                "field": field,
                "review_id": review_id,
                "writes_performed": false
            }),
        )),
    }
}

fn validate_decision_surface_binding(
    review_id: &str,
    decision: &Value,
    item: &Value,
) -> Result<(), Refusal> {
    let mut decision_surfaces = string_array_field(decision, "surface_ids")?;
    let mut item_surfaces = string_array_field(item, "surface_ids")?;
    decision_surfaces.sort();
    decision_surfaces.dedup();
    item_surfaces.sort();
    item_surfaces.dedup();
    if decision_surfaces.is_empty() || decision_surfaces != item_surfaces {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decision surface binding does not match the exported review item",
            json!({
                "stage": "review_import",
                "field": "surface_ids",
                "review_id": review_id,
                "expected": item_surfaces,
                "actual": decision_surfaces,
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn required_decision_string(decision: &Value, field: &str) -> Result<String, Refusal> {
    value_string(decision, field).ok_or_else(|| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import decision is missing a required field",
            json!({
                "stage": "review_import",
                "field": field,
                "writes_performed": false
            }),
        )
    })
}

fn require_decision_string(decision: &Value, field: &str, review_id: &str) -> Result<(), Refusal> {
    value_string(decision, field)
        .filter(|value| !value.trim().is_empty())
        .map(|_| ())
        .ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import decision is missing a required field",
                json!({
                    "stage": "review_import",
                    "field": field,
                    "review_id": review_id,
                    "writes_performed": false
                }),
            )
        })
}

fn value_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_array_field(value: &Value, field: &str) -> Result<Vec<String>, Refusal> {
    let Some(values) = value.get(field).and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                review_import_refusal(
                    EntityRefusalKind::ReviewImport,
                    "Review import surface IDs must be strings",
                    json!({
                        "stage": "review_import",
                        "field": field,
                        "writes_performed": false
                    }),
                )
            })
        })
        .collect()
}

fn validate_review_import_alias_text(
    field: &str,
    value: &str,
    require_already_trimmed: bool,
) -> Result<String, Refusal> {
    let trimmed = value.trim_matches(|ch: char| ch.is_ascii_whitespace());
    if trimmed.is_empty() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import alias field must not be empty",
            json!({
                "stage": "review_import",
                "field": field,
                "writes_performed": false
            }),
        ));
    }
    if require_already_trimmed && trimmed != value {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import alias input must already be ASCII-trimmed",
            json!({
                "stage": "review_import",
                "field": field,
                "input": value,
                "trimmed": trimmed,
                "writes_performed": false
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_review_import_registry_binding(
    registry: &Path,
    review: &Value,
    registry_id: &str,
    version_before: &str,
    registry_snapshot_before: &str,
) -> Result<(), Refusal> {
    let review_registry_id = required_value_string(
        review,
        &["metadata", "registry_snapshot", "id"],
        "registry id",
    )?;
    let review_registry_version = required_value_string(
        review,
        &["metadata", "registry_snapshot", "version"],
        "registry version",
    )?;
    let review_registry_hash = required_value_string(
        review,
        &["metadata", "registry_snapshot", "lookup_snapshot_hash"],
        "registry snapshot hash",
    )?;
    if review_registry_id != registry_id {
        return Err(review_import_registry_snapshot_refusal(
            registry,
            "metadata.registry_snapshot.id",
            review_registry_id,
            registry_id,
        ));
    }
    if review_registry_version != version_before {
        return Err(review_import_registry_snapshot_refusal(
            registry,
            "metadata.registry_snapshot.version",
            review_registry_version,
            version_before,
        ));
    }
    if review_registry_hash != registry_snapshot_before {
        return Err(review_import_registry_snapshot_refusal(
            registry,
            "metadata.registry_snapshot.lookup_snapshot_hash",
            review_registry_hash,
            registry_snapshot_before,
        ));
    }
    Ok(())
}

fn build_review_import_default_queue_mutation(
    registry: &Path,
    registry_before: Value,
    next_version: &str,
    review: &Value,
    plan: ReviewImportDefaultQueuePlan,
    registry_snapshot_before: &str,
) -> Result<ReviewImportDefaultQueueMutation, Refusal> {
    let registry_path = registry.join("registry.json");
    let alias_path = registry.join("aliases.json");
    let anchor_path = review_import_anchor_path(registry, next_version);
    let pending_path = registry.join("_escrow").join("pending.jsonl");
    let cannot_link_path = registry.join("_escrow").join("cannot_link.jsonl");
    let alias_original = read_alias_bytes_or_empty(&alias_path)?;
    let anchor_original = read_file_bytes_or_empty(&anchor_path)?;
    let pending_original = read_file_bytes_or_empty(&pending_path)?;
    let cannot_link_original = read_file_bytes_or_empty(&cannot_link_path)?;
    let alias_hash_before = witness::hash_bytes(&alias_original);
    let anchor_hash_before = witness::hash_bytes(&anchor_original);
    let pending_hash_before = witness::hash_bytes(&pending_original);
    let cannot_link_hash_before = witness::hash_bytes(&cannot_link_original);
    let write_required =
        plan.alias_count() + plan.anchor_count() + plan.pending_count() + plan.cannot_link_count()
            > 0;
    let entry_count_before = review_import_mapping_entry_count(registry)?;
    if !write_required {
        return Ok(ReviewImportDefaultQueueMutation {
            registry_dir: registry.to_path_buf(),
            registry_path,
            registry_snapshot_before: registry_snapshot_before.to_string(),
            registry_snapshot_after: registry_snapshot_before.to_string(),
            files: Vec::new(),
            alias_hash_before: alias_hash_before.clone(),
            alias_hash_after: alias_hash_before,
            anchor_hash_before: anchor_hash_before.clone(),
            anchor_hash_after: anchor_hash_before,
            pending_hash_before: pending_hash_before.clone(),
            pending_hash_after: pending_hash_before,
            cannot_link_hash_before: cannot_link_hash_before.clone(),
            cannot_link_hash_after: cannot_link_hash_before,
            alias_count: 0,
            anchor_count: 0,
            pending_count: 0,
            cannot_link_count: 0,
            no_write_decisions: plan.no_write_decisions,
            write_count: 0,
            entry_count_after: entry_count_before,
        });
    }
    if !plan.aliases.is_empty() && !alias_path.is_file() {
        return Err(review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import requires an existing aliases.json file for alias mutation",
            json!({
                "stage": "review_import",
                "field": "aliases.json",
                "path": alias_path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    validate_alias_conflicts(registry, &plan.aliases)?;
    validate_trusted_anchor_conflicts(registry, &plan.anchors)?;

    let mut replacements = BTreeMap::new();
    let mut files = Vec::new();
    let alias_bytes = if plan.aliases.is_empty() {
        alias_original.clone()
    } else {
        build_review_import_alias_bytes(&alias_original, &plan.aliases)?
    };
    if !plan.aliases.is_empty() {
        files.push(review_import_planned_file(
            &alias_path,
            alias_original.clone(),
            alias_bytes.clone(),
        ));
        replacements.insert(alias_path.clone(), alias_bytes.clone());
    }

    let anchor_bytes = if plan.anchors.is_empty() {
        anchor_original.clone()
    } else {
        build_review_import_jsonl_sidecar_bytes(
            &anchor_path,
            &anchor_original,
            &plan.anchors,
            review_import_trusted_anchor_key,
        )?
    };
    if !plan.anchors.is_empty() {
        files.push(review_import_planned_file(
            &anchor_path,
            anchor_original.clone(),
            anchor_bytes.clone(),
        ));
    }

    let pending_bytes = if plan.pending_escrows.is_empty() {
        pending_original.clone()
    } else {
        build_review_import_jsonl_sidecar_bytes(
            &pending_path,
            &pending_original,
            &plan.pending_escrows,
            |record| record.escrow_id.clone(),
        )?
    };
    if !plan.pending_escrows.is_empty() {
        files.push(review_import_planned_file(
            &pending_path,
            pending_original.clone(),
            pending_bytes.clone(),
        ));
    }

    let cannot_link_bytes = if plan.cannot_links.is_empty() {
        cannot_link_original.clone()
    } else {
        build_review_import_jsonl_sidecar_bytes(
            &cannot_link_path,
            &cannot_link_original,
            &plan.cannot_links,
            |record| record.sidecar_id.clone(),
        )?
    };
    if !plan.cannot_links.is_empty() {
        files.push(review_import_planned_file(
            &cannot_link_path,
            cannot_link_original.clone(),
            cannot_link_bytes.clone(),
        ));
    }

    let entry_count_after = entry_count_before + plan.alias_count();
    let registry_original = fs::read(registry_path.as_path())
        .map_err(|error| review_import_io_refusal(registry_path.as_path(), error))?;
    let registry_bytes = build_review_import_registry_bytes(
        registry_before,
        next_version,
        entry_count_after,
        review,
    )?;
    files.push(review_import_planned_file(
        &registry_path,
        registry_original,
        registry_bytes.clone(),
    ));
    replacements.insert(registry_path.clone(), registry_bytes);
    let registry_snapshot_after =
        review_import_registry_snapshot_hash_with_replacements(registry, &replacements)?;
    Ok(ReviewImportDefaultQueueMutation {
        registry_dir: registry.to_path_buf(),
        registry_path,
        registry_snapshot_before: registry_snapshot_before.to_string(),
        registry_snapshot_after,
        write_count: files.len() as u64,
        files,
        alias_hash_before,
        alias_hash_after: witness::hash_bytes(&alias_bytes),
        anchor_hash_before,
        anchor_hash_after: witness::hash_bytes(&anchor_bytes),
        pending_hash_before,
        pending_hash_after: witness::hash_bytes(&pending_bytes),
        cannot_link_hash_before,
        cannot_link_hash_after: witness::hash_bytes(&cannot_link_bytes),
        alias_count: plan.alias_count(),
        anchor_count: plan.anchor_count(),
        pending_count: plan.pending_count(),
        cannot_link_count: plan.cannot_link_count(),
        no_write_decisions: plan.no_write_decisions,
        entry_count_after,
    })
}

fn commit_review_import_default_queue_mutation(
    mutation: &ReviewImportDefaultQueueMutation,
) -> Result<(), Refusal> {
    commit_review_import_default_queue_mutation_with_hook(mutation, || Ok(()))
}

fn commit_review_import_default_queue_mutation_with_hook(
    mutation: &ReviewImportDefaultQueueMutation,
    before_registry_publish: impl FnOnce() -> Result<(), std::io::Error>,
) -> Result<(), Refusal> {
    if mutation.write_count == 0 {
        return Ok(());
    }
    let _guard = acquire_registry_mutation_guard(&mutation.registry_dir)
        .map_err(|error| review_import_io_refusal(&mutation.registry_dir, error))?;
    let current_snapshot = review_import_registry_snapshot_hash(&mutation.registry_dir)?;
    if current_snapshot != mutation.registry_snapshot_before {
        return Err(review_import_registry_snapshot_refusal(
            &mutation.registry_dir,
            "registry_snapshot_before_commit",
            &mutation.registry_snapshot_before,
            &current_snapshot,
        ));
    }
    match validate_review_import_default_queue_planned_files(&mutation.files)? {
        PlannedMutationState::Ready => {}
        PlannedMutationState::AlreadyApplied => return Ok(()),
        PlannedMutationState::Stale {
            path,
            expected_hash,
            actual_hash,
        } => {
            return Err(review_import_refusal(
                EntityRefusalKind::RegistrySnapshot,
                "Review import registry file changed before commit",
                json!({
                    "stage": "review_import",
                    "field": "planned_mutation",
                    "path": path.display().to_string(),
                    "expected_hash": expected_hash,
                    "actual_hash": actual_hash,
                    "writes_performed": false
                }),
            ));
        }
    }
    let mut published = Vec::<&ReviewImportPlannedFile>::new();
    let mut tmp_paths = Vec::<PathBuf>::new();
    for file in &mutation.files {
        if let Some(parent) = file.path.parent() {
            fs::create_dir_all(parent).map_err(|error| review_import_io_refusal(parent, error))?;
        }
        let tmp = review_import_tmp_path(&file.path);
        if let Err(error) = write_review_import_tmp_file(&tmp, &file.bytes_after) {
            let _ = remove_review_import_tmp_files(&tmp_paths);
            return Err(review_import_io_refusal(&tmp, error));
        }
        tmp_paths.push(tmp);
    }
    for file in mutation
        .files
        .iter()
        .filter(|file| file.path != mutation.registry_path)
    {
        let tmp = review_import_tmp_path(&file.path);
        if let Err(error) = fs::rename(tmp.as_path(), file.path.as_path()) {
            let mut rollback_errors = rollback_review_import_files(&published);
            rollback_errors.extend(remove_review_import_tmp_files(&tmp_paths));
            return Err(review_import_commit_refusal_with_errors(
                "Review import failed while publishing sidecar files",
                &file.path,
                error,
                rollback_errors,
            ));
        }
        published.push(file);
    }
    let registry_file = mutation
        .files
        .iter()
        .find(|file| file.path == mutation.registry_path)
        .ok_or_else(|| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import mutation is missing registry.json",
                json!({
                    "stage": "review_import",
                    "field": "registry.json",
                    "writes_performed": false
                }),
            )
        })?;
    let registry_tmp = review_import_tmp_path(&registry_file.path);
    if let Err(error) = before_registry_publish() {
        let mut rollback_errors = rollback_review_import_files(&published);
        rollback_errors.extend(remove_review_import_tmp_files(&tmp_paths));
        return Err(review_import_commit_refusal_with_errors(
            "Review import failed before publishing registry.json after sidecars",
            &registry_file.path,
            error,
            rollback_errors,
        ));
    }
    if let Err(error) = fs::rename(registry_tmp.as_path(), registry_file.path.as_path()) {
        let mut rollback_errors = rollback_review_import_files(&published);
        rollback_errors.extend(remove_review_import_tmp_files(&tmp_paths));
        return Err(review_import_commit_refusal_with_errors(
            "Review import failed to publish registry.json after sidecars",
            &registry_file.path,
            error,
            rollback_errors,
        ));
    }
    published.push(registry_file);
    let _ = remove_review_import_tmp_files(&tmp_paths);
    if let Some((path, expected_hash, actual_hash)) =
        first_default_queue_file_hash_mismatch(&mutation.files)
    {
        let rollback_errors = rollback_review_import_files(&published);
        return Err(review_import_refusal(
            EntityRefusalKind::RegistrySnapshot,
            "Review import final file hash did not match the planned mutation",
            json!({
                "stage": "review_import",
                "field": "planned_mutation",
                "path": path.display().to_string(),
                "expected_hash": expected_hash,
                "actual_hash": actual_hash,
                "rollback_status": if rollback_errors.is_empty() { "rolled_back" } else { "rollback_failed" },
                "rollback_errors": rollback_errors,
                "writes_performed": true
            }),
        ));
    }
    let final_snapshot = review_import_registry_snapshot_hash(&mutation.registry_dir)?;
    if final_snapshot != mutation.registry_snapshot_after {
        let rollback_errors = rollback_review_import_files(&published);
        return Err(review_import_refusal(
            EntityRefusalKind::RegistrySnapshot,
            "Review import final registry snapshot did not match the planned mutation",
            json!({
                "stage": "review_import",
                "field": "registry_snapshot_after",
                "expected": mutation.registry_snapshot_after,
                "actual": final_snapshot,
                "rollback_status": if rollback_errors.is_empty() { "rolled_back" } else { "rollback_failed" },
                "rollback_errors": rollback_errors,
                "writes_performed": true
            }),
        ));
    }
    Ok(())
}

fn review_import_planned_file(
    path: &Path,
    bytes_before: Vec<u8>,
    bytes_after: Vec<u8>,
) -> ReviewImportPlannedFile {
    ReviewImportPlannedFile {
        path: path.to_path_buf(),
        existed_before: path.is_file(),
        hash_before: witness::hash_bytes(&bytes_before),
        hash_after: witness::hash_bytes(&bytes_after),
        bytes_before,
        bytes_after,
    }
}

fn validate_review_import_default_queue_planned_files(
    files: &[ReviewImportPlannedFile],
) -> Result<PlannedMutationState, Refusal> {
    let mut already_applied = true;
    for file in files {
        let current = match fs::read(file.path.as_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !file.existed_before => {
                Vec::new()
            }
            Err(error) => return Err(review_import_io_refusal(&file.path, error)),
        };
        let actual_hash = witness::hash_bytes(&current);
        if actual_hash != file.hash_before && actual_hash != file.hash_after {
            return Ok(PlannedMutationState::Stale {
                path: file.path.clone(),
                expected_hash: file.hash_before.clone(),
                actual_hash,
            });
        }
        if actual_hash != file.hash_after {
            already_applied = false;
        }
    }
    if already_applied {
        Ok(PlannedMutationState::AlreadyApplied)
    } else {
        Ok(PlannedMutationState::Ready)
    }
}

fn first_default_queue_file_hash_mismatch(
    files: &[ReviewImportPlannedFile],
) -> Option<(PathBuf, String, String)> {
    files.iter().find_map(|file| {
        let bytes = fs::read(file.path.as_path()).ok()?;
        let actual = witness::hash_bytes(&bytes);
        (actual != file.hash_after).then(|| (file.path.clone(), file.hash_after.clone(), actual))
    })
}

fn rollback_review_import_files(files: &[&ReviewImportPlannedFile]) -> Vec<String> {
    let mut errors = Vec::new();
    for file in files.iter().rev() {
        if let Err(error) =
            rollback_review_import_file(&file.path, &file.bytes_before, file.existed_before)
        {
            errors.push(error);
        }
    }
    errors
}

fn review_import_commit_refusal_with_errors(
    message: &'static str,
    path: &Path,
    error: std::io::Error,
    rollback_errors: Vec<String>,
) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        message,
        json!({
            "stage": "review_import",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "rollback_status": if rollback_errors.is_empty() { "rolled_back" } else { "rollback_failed" },
            "rollback_errors": rollback_errors,
            "writes_performed": true
        }),
    )
}

fn review_import_anchor_path(registry: &Path, next_version: &str) -> PathBuf {
    registry.join("_anchors").join(format!(
        "{}.anchors.jsonl",
        review_import_version_stem(next_version)
    ))
}

fn review_import_version_stem(next_version: &str) -> String {
    let stem = next_version
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if stem.is_empty() {
        "next".to_string()
    } else {
        stem
    }
}

fn review_import_tmp_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".review-import.tmp");
    PathBuf::from(tmp)
}

fn write_review_import_tmp_file(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn remove_review_import_tmp_files(paths: &[PathBuf]) -> Vec<String> {
    let mut errors = Vec::new();
    for path in paths {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => errors.push(error.to_string()),
        }
    }
    errors
}

fn read_file_bytes_or_empty(path: &Path) -> Result<Vec<u8>, Refusal> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(review_import_io_refusal(path, error)),
    }
}

fn build_review_import_jsonl_sidecar_bytes<T>(
    path: &Path,
    original: &[u8],
    additions: &[T],
    key: impl Fn(&T) -> String,
) -> Result<Vec<u8>, Refusal>
where
    T: Clone + DeserializeOwned + Serialize + Ord,
{
    let mut records = read_review_import_jsonl_records::<T>(path, original)?;
    let mut keys = records.iter().map(&key).collect::<BTreeSet<_>>();
    for addition in additions {
        let addition_key = key(addition);
        if !keys.insert(addition_key.clone()) {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import sidecar record duplicates an existing record",
                json!({
                    "stage": "review_import",
                    "field": "sidecar_id",
                    "path": path.display().to_string(),
                    "sidecar_id": addition_key,
                    "writes_performed": false
                }),
            ));
        }
        records.push(addition.clone());
    }
    records.sort();
    review_import_jsonl_bytes(&records)
}

fn read_review_import_jsonl_records<T>(path: &Path, bytes: &[u8]) -> Result<Vec<T>, Refusal>
where
    T: DeserializeOwned,
{
    let mut records = Vec::new();
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        records.push(serde_json::from_slice::<T>(line).map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import sidecar JSONL is malformed",
                json!({
                    "stage": "review_import",
                    "field": "sidecar_jsonl",
                    "path": path.display().to_string(),
                    "line": index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?);
    }
    Ok(records)
}

fn review_import_jsonl_bytes<T: Serialize>(records: &[T]) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| {
            review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import could not serialize sidecar update",
                json!({
                    "stage": "review_import",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn review_import_trusted_anchor_key(record: &ReviewImportTrustedAnchorRecord) -> String {
    format!("{}\u{1f}{}", record.namespace, record.value)
}

fn validate_trusted_anchor_conflicts(
    registry: &Path,
    anchors: &[ReviewImportTrustedAnchorRecord],
) -> Result<(), Refusal> {
    if anchors.is_empty() {
        return Ok(());
    }
    let mut existing = BTreeMap::<(String, String), String>::new();
    let anchors_dir = registry.join("_anchors");
    if anchors_dir.is_dir() {
        for entry in fs::read_dir(&anchors_dir)
            .map_err(|error| review_import_io_refusal(&anchors_dir, error))?
        {
            let path = entry
                .map_err(|error| review_import_io_refusal(&anchors_dir, error))?
                .path();
            if path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            {
                let bytes = fs::read(path.as_path())
                    .map_err(|error| review_import_io_refusal(path.as_path(), error))?;
                for record in read_review_import_jsonl_records::<ReviewImportTrustedAnchorRecord>(
                    &path, &bytes,
                )? {
                    existing.insert((record.namespace, record.value), record.canonical_id);
                }
            }
        }
    }
    for anchor in anchors {
        if let Some(existing_canonical_id) =
            existing.get(&(anchor.namespace.clone(), anchor.value.clone()))
            && existing_canonical_id != &anchor.canonical_id
        {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import would create a trusted-anchor conflict",
                json!({
                    "stage": "review_import",
                    "field": "anchors",
                    "namespace": anchor.namespace,
                    "value": anchor.value,
                    "existing_canonical_id": existing_canonical_id,
                    "new_canonical_id": anchor.canonical_id,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn rollback_review_import_file(
    path: &Path,
    bytes: &[u8],
    existed_before: bool,
) -> Result<(), String> {
    if !existed_before {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        }
    }
    let rollback_tmp = path.with_extension("json.review-import.rollback.tmp");
    fs::write(rollback_tmp.as_path(), bytes)
        .and_then(|_| fs::rename(rollback_tmp.as_path(), path))
        .map_err(|error| error.to_string())
}

fn read_alias_bytes_or_empty(path: &Path) -> Result<Vec<u8>, Refusal> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            review_import_pretty_bytes(&Vec::<Value>::new())
        }
        Err(error) => Err(review_import_io_refusal(path, error)),
    }
}

fn validate_alias_conflicts(
    registry: &Path,
    aliases: &[ReviewImportAliasEntry],
) -> Result<(), Refusal> {
    let existing = review_import_existing_aliases(registry)?;
    for alias in aliases {
        if let Some(existing) = existing.get(&alias.input) {
            return Err(review_import_refusal(
                EntityRefusalKind::ReviewImport,
                "Review import alias input already exists in the registry",
                json!({
                    "stage": "review_import",
                    "field": "alias_input",
                    "input": alias.input.as_str(),
                    "existing": {
                        "canonical_id": existing.canonical_id.as_str(),
                        "canonical_type": existing.canonical_type.as_str()
                    },
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn review_import_existing_aliases(
    registry: &Path,
) -> Result<BTreeMap<String, ReviewImportAliasEntry>, Refusal> {
    let mut aliases = BTreeMap::new();
    for path in review_import_mapping_file_paths(registry)? {
        let bytes = fs::read(path.as_path())
            .map_err(|error| review_import_io_refusal(path.as_path(), error))?;
        for entry in parse_review_import_alias_entries(&path, &bytes)? {
            aliases.insert(entry.input.clone(), entry);
        }
    }
    Ok(aliases)
}

fn review_import_mapping_entry_count(registry: &Path) -> Result<u64, Refusal> {
    let mut count = 0u64;
    for path in review_import_mapping_file_paths(registry)? {
        let bytes = fs::read(path.as_path())
            .map_err(|error| review_import_io_refusal(path.as_path(), error))?;
        count += parse_review_import_alias_entries(&path, &bytes)?.len() as u64;
    }
    Ok(count)
}

fn review_import_mapping_file_paths(registry: &Path) -> Result<Vec<PathBuf>, Refusal> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(registry).map_err(|error| review_import_io_refusal(registry, error))?
    {
        let path = entry
            .map_err(|error| review_import_io_refusal(registry, error))?
            .path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
            && path.file_name().and_then(|name| name.to_str()) != Some("registry.json")
            && path.file_name().and_then(|name| name.to_str()) != Some("_build.json")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn parse_review_import_alias_entries(
    path: &Path,
    bytes: &[u8],
) -> Result<Vec<ReviewImportAliasEntry>, Refusal> {
    serde_json::from_slice::<Vec<ReviewImportAliasEntry>>(bytes).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import registry alias file is malformed",
            json!({
                "stage": "review_import",
                "field": "aliases",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn build_review_import_alias_bytes(
    original: &[u8],
    aliases: &[ReviewImportAliasEntry],
) -> Result<Vec<u8>, Refusal> {
    let mut entries = serde_json::from_slice::<Vec<Value>>(original).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import alias file is malformed",
            json!({
                "stage": "review_import",
                "field": "aliases",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    for alias in aliases {
        entries.push(json!({
            "input": alias.input.as_str(),
            "canonical_id": alias.canonical_id.as_str(),
            "canonical_type": alias.canonical_type.as_str(),
            "rule_id": alias.rule_id.as_str()
        }));
    }
    review_import_pretty_bytes(&entries)
}

fn build_review_import_registry_bytes(
    mut registry: Value,
    next_version: &str,
    entry_count_after: u64,
    review: &Value,
) -> Result<Vec<u8>, Refusal> {
    let Some(object) = registry.as_object_mut() else {
        return Err(review_import_registry_refusal(
            Path::new("registry.json"),
            "registry.json must be an object",
        ));
    };
    object.insert(
        "version".to_string(),
        Value::String(next_version.to_string()),
    );
    object.insert(
        "entry_count".to_string(),
        Value::Number(serde_json::Number::from(entry_count_after)),
    );
    if let Some(profile) = review
        .get("metadata")
        .and_then(|metadata| metadata.get("profile"))
    {
        object.insert("entity_profile".to_string(), profile.clone());
    }
    review_import_pretty_bytes(&registry)
}

fn review_import_registry_snapshot_hash(registry: &Path) -> Result<String, Refusal> {
    review_import_registry_snapshot_hash_with_replacements(registry, &BTreeMap::new())
}

fn review_import_registry_snapshot_hash_with_replacements(
    registry: &Path,
    replacements: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<String, Refusal> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(registry).map_err(|error| review_import_io_refusal(registry, error))?
    {
        let path = entry
            .map_err(|error| review_import_io_refusal(registry, error))?
            .path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            files.push(path);
        }
    }
    files.extend(replacements.keys().cloned());
    files.sort();
    files.dedup();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let bytes = if let Some(bytes) = replacements.get(&path) {
            bytes.clone()
        } else {
            fs::read(path.as_path())
                .map_err(|error| review_import_io_refusal(path.as_path(), error))?
        };
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn review_import_pretty_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        review_import_refusal(
            EntityRefusalKind::ReviewImport,
            "Review import could not serialize registry update",
            json!({
                "stage": "review_import",
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn review_import_registry_snapshot_refusal(
    registry: &Path,
    field: &str,
    expected: &str,
    actual: &str,
) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::RegistrySnapshot,
        "Current registry snapshot does not match the reviewed artifact",
        json!({
            "stage": "review_import",
            "field": field,
            "registry": registry.display().to_string(),
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
    )
}

fn review_import_io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    review_import_refusal(
        EntityRefusalKind::ReviewImport,
        "Review import could not access a registry file",
        json!({
            "stage": "review_import",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn registry_json_value(registry: &Path) -> Result<Value, Refusal> {
    let path = registry.join("registry.json");
    let bytes = fs::read(path.as_path()).map_err(|error| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefusalCode;
    use crate::entity::{
        CANON_ENTITY_REVIEW_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
        audit::{EntityAuditV1Request, run_entity_audit_v1},
        entity_artifact_v1_contract_for_version,
        review::render_review_v1_csv,
        schema::{
            entity_v1_schema_content_hash, finalize_entity_v1_self_hash,
            validate_entity_v1_self_hash,
        },
    };
    use std::fs;

    #[test]
    fn review_v1_csv_context_is_compact_and_round_trips_multi_item_source() {
        let review = sample_review_v1_artifact();
        let csv = render_review_v1_csv(&review).expect("review csv renders");
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let headers = reader.headers().expect("headers").clone();
        let context_index = headers
            .iter()
            .position(|header| header == "review_context_json")
            .expect("context column");
        let mut row_count = 0usize;
        for record in reader.records() {
            let record = record.expect("record");
            let context: Value =
                serde_json::from_str(&record[context_index]).expect("context json");
            assert!(
                context.get("artifact").is_none(),
                "CSV context must not embed the full artifact"
            );
            assert!(
                context.get("review_items").is_none(),
                "CSV context must not repeat review_items"
            );
            assert_eq!(context["include"], "all");
            assert!(context.get("next_commands").is_some());
            row_count += 1;
        }
        assert_eq!(row_count, 2);

        let parsed = parse_review_v1_csv(&csv).expect("csv parses");
        assert_eq!(parsed.source, review);
        validate_entity_v1_self_hash(&parsed.source).expect("source self-hash validates");
        assert_eq!(parsed.decisions.len(), 1);
        assert_eq!(parsed.decisions[0]["review_id"], "review:1");
    }

    #[test]
    fn review_v1_import_accept_alias_uses_exported_item_authority_and_updates_registry() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        let review_hash = required_value_string(&review, &["artifact_content_hash"], "review hash")
            .expect("review hash");
        let audit = sample_review_import_audit(temp.path(), run);
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let audit_bytes = serde_json::to_vec_pretty(&audit).expect("audit bytes");

        let receipt = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: Some((&audit, &audit_bytes)),
        })
        .expect("review import succeeds");

        assert_eq!(receipt["version"], CANON_ENTITY_REVIEW_IMPORT_VERSION);
        assert!(
            receipt["artifact_content_hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("blake3:"))
        );
        assert_eq!(receipt["summary"]["counts"]["accepted_aliases"], 1);
        assert_eq!(receipt["summary"]["counts"]["registry_writes"], 2);
        assert_eq!(receipt["summary"]["labels"]["stage"], "review_import");
        assert_eq!(
            receipt["summary"]["labels"]["registry_version_after"],
            "2026.06.26"
        );
        assert_eq!(receipt["source_review_queue_hash"], review_hash);
        let aliases: Vec<Value> =
            serde_json::from_slice(&fs::read(registry.join("aliases.json")).expect("aliases"))
                .expect("aliases json");
        assert!(aliases.iter().any(|entry| {
            entry["input"] == "Sears Holdings"
                && entry["canonical_id"] == "TNT-SEARS"
                && entry["canonical_type"] == "tenant_label"
                && entry["rule_id"] == "ENTITY_REVIEW_IMPORT"
        }));
        let registry_json: Value =
            serde_json::from_slice(&fs::read(registry.join("registry.json")).expect("registry"))
                .expect("registry json");
        assert_eq!(registry_json["version"], "2026.06.26");
        assert_eq!(registry_json["entry_count"], 2);
    }

    #[test]
    fn review_v1_accept_alias_refuses_caller_authored_alias_drift() {
        let review = sample_review_v1_artifact_for_import("blake3:registry", "blake3:run");
        let refusal = reviewed_aliases_from_v1_decisions(
            &review,
            &[json!({
                "review_id": "review:1",
                "decision": "accept_alias",
                "operator_id": "operator-1",
                "reason_code": "confirmed",
                "surface_ids": ["surface:1"],
                "target_canonical_id": "TNT-OTHER",
                "canonical_type": "tenant_label",
                "rule_id": "ENTITY_REVIEW_IMPORT",
                "alias_inputs": ["Sears Holdings"]
            })],
        )
        .expect_err("decision drift refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(refusal.detail["field"], "target_canonical_id");
        assert_eq!(refusal.detail["writes_performed"], false);
    }

    #[test]
    fn review_v1_accept_alias_refuses_plural_authority() {
        let mut review = sample_review_v1_artifact_for_import("blake3:registry", "blake3:run");
        let plural_action_refusal = reviewed_aliases_from_v1_decisions(
            &review,
            &[json!({
                "review_id": "review:1",
                "decision": "accept_aliases",
                "operator_id": "operator-1",
                "reason_code": "confirmed",
                "surface_ids": ["surface:1"]
            })],
        )
        .expect_err("plural accept_aliases refuses");
        assert_eq!(plural_action_refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(plural_action_refusal.detail["field"], "decision");
        assert_eq!(plural_action_refusal.detail["writes_performed"], false);

        let item = review["review_items"][0]
            .as_object_mut()
            .expect("review item object");
        let proposal = item.remove("alias_proposal").expect("alias proposal");
        item.insert("alias_proposals".to_string(), json!([proposal]));
        let plural_proposal_refusal =
            reviewed_aliases_from_v1_decisions(&review, &reviewed_decisions_from_v1(&review))
                .expect_err("plural alias_proposals refuses");
        assert_eq!(
            plural_proposal_refusal.code,
            RefusalCode::EEntityReviewImport
        );
        assert_eq!(plural_proposal_refusal.detail["field"], "alias_proposals");
        assert_eq!(plural_proposal_refusal.detail["writes_performed"], false);
    }

    #[test]
    fn review_v1_import_zero_alias_reports_no_registry_state_change() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        review["review_items"][0]["decision"] = Value::String("reject_alias".to_string());
        finalize_entity_v1_self_hash(&mut review).expect("review self hash finalizes");
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let registry_before = registry_snapshot_for_test(&registry);

        let receipt = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect("zero-alias review import succeeds");

        assert_eq!(receipt["summary"]["counts"]["accepted_aliases"], 0);
        assert_eq!(receipt["summary"]["counts"]["registry_writes"], 0);
        assert_eq!(
            receipt["summary"]["labels"]["registry_version_after"],
            "2026.06.25"
        );
        assert_eq!(
            receipt["summary"]["labels"]["registry_snapshot_hash_after"],
            registry_hash
        );
        assert_eq!(registry_snapshot_for_test(&registry), registry_before);
    }

    #[test]
    fn review_v1_import_writes_anchor_pending_and_cannot_link_sidecars() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        {
            let items = review["review_items"].as_array_mut().expect("review items");
            items[0]["anchors"] = json!([{
                "namespace": "sec_cik",
                "value": "0000320193"
            }]);
            items.push(json!({
                "review_id": "review:pending",
                "state": "needs_review",
                "surface_ids": ["surface:2"],
                "decision": "create_pending",
                "operator_id": "operator-1",
                "reason_code": "needs_more_evidence"
            }));
            items.push(json!({
                "review_id": "review:cannot",
                "state": "contradiction",
                "surface_ids": ["surface:3", "surface:4"],
                "decision": "emit_cannot_link",
                "operator_id": "operator-1",
                "reason_code": "hard_conflict"
            }));
        }
        finalize_entity_v1_self_hash(&mut review).expect("review self hash finalizes");
        let audit = sample_review_import_audit(temp.path(), run);
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let audit_bytes = serde_json::to_vec_pretty(&audit).expect("audit bytes");
        let alias_before_bytes = fs::read(registry.join("aliases.json")).expect("alias before");
        let empty_sidecar_hash = witness::hash_bytes(&[]);

        let receipt = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: Some((&audit, &audit_bytes)),
        })
        .expect("sidecar import succeeds");

        assert_eq!(receipt["summary"]["counts"]["accepted_aliases"], 1);
        assert_eq!(receipt["summary"]["counts"]["trusted_anchors"], 1);
        assert_eq!(receipt["summary"]["counts"]["pending_escrows"], 1);
        assert_eq!(receipt["summary"]["counts"]["cannot_links"], 1);
        assert_eq!(receipt["summary"]["counts"]["registry_writes"], 5);
        assert_eq!(
            receipt["summary"]["labels"]["registry_version_after"],
            "2026.06.26"
        );
        assert_eq!(
            receipt["summary"]["labels"]["alias_patch_hash_before"],
            witness::hash_bytes(&alias_before_bytes)
        );
        assert_eq!(
            receipt["summary"]["labels"]["anchor_patch_hash_before"],
            empty_sidecar_hash
        );
        assert_eq!(
            receipt["summary"]["labels"]["pending_escrow_hash_before"],
            empty_sidecar_hash
        );
        assert_eq!(
            receipt["summary"]["labels"]["cannot_link_hash_before"],
            empty_sidecar_hash
        );
        let alias_after_bytes = fs::read(registry.join("aliases.json")).expect("alias after");
        let anchor_path = registry.join("_anchors/20260626.anchors.jsonl");
        let anchor_after_bytes = fs::read(anchor_path.as_path()).expect("anchor after");
        let pending_path = registry.join("_escrow/pending.jsonl");
        let pending_after_bytes = fs::read(pending_path.as_path()).expect("pending after");
        let cannot_link_path = registry.join("_escrow/cannot_link.jsonl");
        let cannot_link_after_bytes =
            fs::read(cannot_link_path.as_path()).expect("cannot link after");
        assert_eq!(
            receipt["summary"]["labels"]["alias_patch_hash_after"],
            witness::hash_bytes(&alias_after_bytes)
        );
        assert_eq!(
            receipt["summary"]["labels"]["anchor_patch_hash_after"],
            witness::hash_bytes(&anchor_after_bytes)
        );
        assert_eq!(
            receipt["summary"]["labels"]["pending_escrow_hash_after"],
            witness::hash_bytes(&pending_after_bytes)
        );
        assert_eq!(
            receipt["summary"]["labels"]["cannot_link_hash_after"],
            witness::hash_bytes(&cannot_link_after_bytes)
        );
        let anchors = read_jsonl_values(&anchor_path);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0]["canonical_id"], "TNT-SEARS");
        assert_eq!(anchors[0]["namespace"], "sec_cik");
        assert_eq!(anchors[0]["value"], "0000320193");
        let pending = read_jsonl_values(&pending_path);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["profile_id"], "cmbs_tenant_label");
        assert_eq!(pending[0]["source_decision_id"], "review:pending");
        assert_eq!(pending[0]["surface_ids"], json!(["surface:2"]));
        let cannot_link = read_jsonl_values(&cannot_link_path);
        assert_eq!(cannot_link.len(), 1);
        assert_eq!(cannot_link[0]["profile_id"], "cmbs_tenant_label");
        assert_eq!(cannot_link[0]["review_decision_id"], "review:cannot");
        assert_eq!(cannot_link[0]["left"], "surface:3");
        assert_eq!(cannot_link[0]["right"], "surface:4");
        assert_eq!(cannot_link[0]["hard_cannot_link"], true);
        let registry_json: Value =
            serde_json::from_slice(&fs::read(registry.join("registry.json")).expect("registry"))
                .expect("registry json");
        assert_eq!(registry_json["version"], "2026.06.26");
        assert_eq!(registry_json["entry_count"], 2);
    }

    #[test]
    fn review_v1_import_pending_and_cannot_link_without_audit_bumps_truthfully() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        {
            let items = review["review_items"].as_array_mut().expect("review items");
            items[0]
                .as_object_mut()
                .expect("first item")
                .remove("alias_proposal");
            items[0]["decision"] = Value::String("create_pending".to_string());
            items[0]["reason_code"] = Value::String("needs_more_evidence".to_string());
            items.push(json!({
                "review_id": "review:cannot",
                "state": "contradiction",
                "surface_ids": ["surface:3", "surface:4"],
                "decision": "emit_cannot_link",
                "operator_id": "operator-1",
                "reason_code": "hard_conflict"
            }));
        }
        finalize_entity_v1_self_hash(&mut review).expect("review self hash finalizes");
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let aliases_before = fs::read(registry.join("aliases.json")).expect("aliases before");

        let receipt = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect("pending/cannot-link import succeeds without audit");

        assert_eq!(receipt["summary"]["counts"]["accepted_aliases"], 0);
        assert_eq!(receipt["summary"]["counts"]["trusted_anchors"], 0);
        assert_eq!(receipt["summary"]["counts"]["pending_escrows"], 1);
        assert_eq!(receipt["summary"]["counts"]["cannot_links"], 1);
        assert_eq!(receipt["summary"]["counts"]["registry_writes"], 3);
        assert_eq!(
            receipt["summary"]["labels"]["registry_version_after"],
            "2026.06.26"
        );
        assert_eq!(
            fs::read(registry.join("aliases.json")).expect("aliases after"),
            aliases_before
        );
        let registry_json: Value =
            serde_json::from_slice(&fs::read(registry.join("registry.json")).expect("registry"))
                .expect("registry json");
        assert_eq!(registry_json["version"], "2026.06.26");
        assert_eq!(registry_json["entry_count"], 1);
        assert_eq!(
            read_jsonl_values(&registry.join("_escrow/pending.jsonl")).len(),
            1
        );
        assert_eq!(
            read_jsonl_values(&registry.join("_escrow/cannot_link.jsonl")).len(),
            1
        );
    }

    #[test]
    fn review_v1_import_alias_and_anchor_refuse_without_audit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let alias_registry = temp.path().join("alias-registry");
        write_review_import_test_registry(&alias_registry, "2026.06.25", 1);
        let alias_registry_hash =
            review_import_registry_snapshot_hash(&alias_registry).expect("registry hash");
        let run = sample_run_v1_artifact(&alias_registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let alias_review = sample_review_v1_artifact_for_import(&alias_registry_hash, run_hash);
        let alias_bytes = serde_json::to_vec_pretty(&alias_review).expect("review bytes");
        let alias_before = registry_tree_snapshot_for_test(&alias_registry);

        let alias_refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("alias-review.json"),
            review_bytes: &alias_bytes,
            registry: &alias_registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect_err("alias without audit refuses");
        assert_eq!(alias_refusal.code, RefusalCode::EEntityAuditGate);
        assert_eq!(alias_refusal.detail["field"], "audit");
        assert_eq!(
            registry_tree_snapshot_for_test(&alias_registry),
            alias_before
        );

        let anchor_registry = temp.path().join("anchor-registry");
        write_review_import_test_registry(&anchor_registry, "2026.06.25", 1);
        let anchor_registry_hash =
            review_import_registry_snapshot_hash(&anchor_registry).expect("registry hash");
        let run = sample_run_v1_artifact(&anchor_registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut anchor_review =
            sample_review_v1_artifact_for_import(&anchor_registry_hash, run_hash);
        {
            let item = anchor_review["review_items"][0]
                .as_object_mut()
                .expect("review item");
            item.remove("alias_proposal");
            item.insert(
                "decision".to_string(),
                Value::String("accept_anchor".to_string()),
            );
            item.insert(
                "canonical_id".to_string(),
                Value::String("TNT-SEARS".to_string()),
            );
            item.insert(
                "anchors".to_string(),
                json!([{ "namespace": "sec_cik", "value": "0000320193" }]),
            );
        }
        finalize_entity_v1_self_hash(&mut anchor_review).expect("review self hash finalizes");
        let anchor_bytes = serde_json::to_vec_pretty(&anchor_review).expect("review bytes");
        let anchor_before = registry_tree_snapshot_for_test(&anchor_registry);

        let anchor_refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("anchor-review.json"),
            review_bytes: &anchor_bytes,
            registry: &anchor_registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect_err("anchor without audit refuses");
        assert_eq!(anchor_refusal.code, RefusalCode::EEntityAuditGate);
        assert_eq!(anchor_refusal.detail["field"], "audit");
        assert_eq!(
            registry_tree_snapshot_for_test(&anchor_registry),
            anchor_before
        );
    }

    #[test]
    fn review_v1_import_refuses_unknown_action_before_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        review["review_items"][0]["decision"] = Value::String("merge_confirmed".to_string());
        finalize_entity_v1_self_hash(&mut review).expect("review self hash finalizes");
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let registry_before = registry_snapshot_for_test(&registry);

        let refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect_err("unknown action refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(refusal.detail["field"], "decision");
        assert_eq!(refusal.detail["writes_performed"], false);
        assert_eq!(registry_snapshot_for_test(&registry), registry_before);
    }

    #[test]
    fn review_v1_import_duplicate_and_anchor_conflicts_refuse_without_writes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let duplicate_registry = temp.path().join("duplicate-registry");
        write_review_import_test_registry(&duplicate_registry, "2026.06.25", 1);
        let duplicate_registry_hash =
            review_import_registry_snapshot_hash(&duplicate_registry).expect("registry hash");
        let run = sample_run_v1_artifact(&duplicate_registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut duplicate_review =
            sample_review_v1_artifact_for_import(&duplicate_registry_hash, run_hash);
        let duplicate_item = duplicate_review["review_items"][0].clone();
        duplicate_review["review_items"]
            .as_array_mut()
            .expect("review items")
            .push(duplicate_item);
        finalize_entity_v1_self_hash(&mut duplicate_review).expect("review self hash finalizes");
        let duplicate_bytes = serde_json::to_vec_pretty(&duplicate_review).expect("review bytes");
        let duplicate_before = registry_tree_snapshot_for_test(&duplicate_registry);

        let duplicate_refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("duplicate-review.json"),
            review_bytes: &duplicate_bytes,
            registry: &duplicate_registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect_err("duplicate review id refuses");
        assert_eq!(duplicate_refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(duplicate_refusal.detail["field"], "review_id");
        assert_eq!(
            registry_tree_snapshot_for_test(&duplicate_registry),
            duplicate_before
        );

        let batch_registry = temp.path().join("batch-anchor-registry");
        write_review_import_test_registry(&batch_registry, "2026.06.25", 1);
        let batch_registry_hash =
            review_import_registry_snapshot_hash(&batch_registry).expect("registry hash");
        let run = sample_run_v1_artifact(&batch_registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut batch_review = sample_review_v1_artifact_for_import(&batch_registry_hash, run_hash);
        {
            let items = batch_review["review_items"]
                .as_array_mut()
                .expect("review items");
            let first = items[0].as_object_mut().expect("first item");
            first.remove("alias_proposal");
            first.insert(
                "decision".to_string(),
                Value::String("accept_anchor".to_string()),
            );
            first.insert(
                "canonical_id".to_string(),
                Value::String("TNT-SEARS".to_string()),
            );
            first.insert(
                "anchors".to_string(),
                json!([{ "namespace": "sec_cik", "value": "0000320193" }]),
            );
            items.push(json!({
                "review_id": "review:anchor-conflict",
                "state": "promotable_new",
                "surface_ids": ["surface:2"],
                "decision": "accept_anchor",
                "operator_id": "operator-1",
                "reason_code": "confirmed",
                "canonical_id": "TNT-OTHER",
                "anchors": [{ "namespace": "sec_cik", "value": "0000320193" }]
            }));
        }
        finalize_entity_v1_self_hash(&mut batch_review).expect("review self hash finalizes");
        let batch_bytes = serde_json::to_vec_pretty(&batch_review).expect("review bytes");
        let batch_before = registry_tree_snapshot_for_test(&batch_registry);

        let batch_refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("batch-review.json"),
            review_bytes: &batch_bytes,
            registry: &batch_registry,
            next_version: "2026.06.26",
            audit: None,
        })
        .expect_err("in-batch anchor conflict refuses");
        assert_eq!(batch_refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(batch_refusal.detail["field"], "anchors");
        assert_eq!(
            registry_tree_snapshot_for_test(&batch_registry),
            batch_before
        );

        let existing_registry = temp.path().join("existing-anchor-registry");
        write_review_import_test_registry(&existing_registry, "2026.06.25", 1);
        fs::create_dir_all(existing_registry.join("_anchors")).expect("anchors dir");
        fs::write(
            existing_registry.join("_anchors/existing.anchors.jsonl"),
            b"{\"canonical_id\":\"TNT-OTHER\",\"namespace\":\"sec_cik\",\"value\":\"0000320193\"}\n",
        )
        .expect("existing anchor");
        let existing_registry_hash =
            review_import_registry_snapshot_hash(&existing_registry).expect("registry hash");
        let run = sample_run_v1_artifact(&existing_registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut existing_review =
            sample_review_v1_artifact_for_import(&existing_registry_hash, run_hash);
        existing_review["review_items"][0]["anchors"] =
            json!([{ "namespace": "sec_cik", "value": "0000320193" }]);
        finalize_entity_v1_self_hash(&mut existing_review).expect("review self hash finalizes");
        let audit = sample_review_import_audit(temp.path(), run);
        let existing_bytes = serde_json::to_vec_pretty(&existing_review).expect("review bytes");
        let audit_bytes = serde_json::to_vec_pretty(&audit).expect("audit bytes");
        let existing_before = registry_tree_snapshot_for_test(&existing_registry);

        let existing_refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("existing-review.json"),
            review_bytes: &existing_bytes,
            registry: &existing_registry,
            next_version: "2026.06.26",
            audit: Some((&audit, &audit_bytes)),
        })
        .expect_err("existing anchor conflict refuses");
        assert_eq!(existing_refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(existing_refusal.detail["field"], "anchors");
        assert_eq!(
            registry_tree_snapshot_for_test(&existing_registry),
            existing_before
        );
    }

    #[test]
    fn review_v1_import_failure_after_sidecar_publish_rolls_back_all_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let mut review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        {
            let items = review["review_items"].as_array_mut().expect("review items");
            items[0]["anchors"] = json!([{
                "namespace": "sec_cik",
                "value": "0000320193"
            }]);
            items.push(json!({
                "review_id": "review:pending",
                "state": "needs_review",
                "surface_ids": ["surface:2"],
                "decision": "create_pending",
                "operator_id": "operator-1",
                "reason_code": "needs_more_evidence"
            }));
            items.push(json!({
                "review_id": "review:cannot",
                "state": "contradiction",
                "surface_ids": ["surface:3", "surface:4"],
                "decision": "emit_cannot_link",
                "operator_id": "operator-1",
                "reason_code": "hard_conflict"
            }));
        }
        finalize_entity_v1_self_hash(&mut review).expect("review self hash finalizes");
        let before = registry_tree_snapshot_for_test(&registry);
        let decisions = reviewed_decisions_from_v1(&review);
        let plan = review_import_default_queue_plan_from_v1_decisions(&review, &decisions)
            .expect("plan derives");
        let mutation = build_review_import_default_queue_mutation(
            &registry,
            registry_json_value(&registry).expect("registry json"),
            "2026.06.26",
            &review,
            plan,
            &registry_hash,
        )
        .expect("mutation builds");

        let refusal = commit_review_import_default_queue_mutation_with_hook(&mutation, || {
            Err(std::io::Error::other("injected registry publish failure"))
        })
        .expect_err("injected failure refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(refusal.detail["writes_performed"], true);
        assert_eq!(refusal.detail["rollback_status"], "rolled_back");
        assert_eq!(registry_tree_snapshot_for_test(&registry), before);
    }

    #[test]
    fn review_v1_import_refuses_stale_registry_snapshot_before_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        let audit = sample_review_import_audit(temp.path(), run);
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let audit_bytes = serde_json::to_vec_pretty(&audit).expect("audit bytes");
        fs::write(registry.join("_build.json"), r#"{"changed":true}"#).expect("stale registry");
        let aliases_before = fs::read(registry.join("aliases.json")).expect("aliases before");
        let registry_before = fs::read(registry.join("registry.json")).expect("registry before");

        let refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: Some((&audit, &audit_bytes)),
        })
        .expect_err("stale registry refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
        assert_eq!(
            refusal.detail["field"],
            "metadata.registry_snapshot.lookup_snapshot_hash"
        );
        assert_eq!(refusal.detail["writes_performed"], false);
        assert_eq!(
            fs::read(registry.join("aliases.json")).expect("aliases after"),
            aliases_before
        );
        assert_eq!(
            fs::read(registry.join("registry.json")).expect("registry after"),
            registry_before
        );
    }

    #[test]
    fn review_v1_import_refuses_tampered_audit_self_hash_before_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let registry_hash = review_import_registry_snapshot_hash(&registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        let mut audit = sample_review_import_audit(temp.path(), run);
        audit["summary"]["labels"]["suite_id"] = Value::String("tampered".to_string());
        let review_bytes = serde_json::to_vec_pretty(&review).expect("review bytes");
        let audit_bytes = serde_json::to_vec_pretty(&audit).expect("audit bytes");
        let aliases_before = fs::read(registry.join("aliases.json")).expect("aliases before");
        let registry_before = fs::read(registry.join("registry.json")).expect("registry before");

        let refusal = import_review_v1(ReviewImportV1Request {
            review_path: &temp.path().join("review.json"),
            review_bytes: &review_bytes,
            registry: &registry,
            next_version: "2026.06.26",
            audit: Some((&audit, &audit_bytes)),
        })
        .expect_err("tampered audit refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
        assert_eq!(refusal.detail["field"], "artifact_content_hash");
        assert_eq!(
            fs::read(registry.join("aliases.json")).expect("aliases after"),
            aliases_before
        );
        assert_eq!(
            fs::read(registry.join("registry.json")).expect("registry after"),
            registry_before
        );
    }

    #[test]
    fn review_v1_import_second_publish_failure_reports_write_and_rolls_back() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let mutation = sample_review_import_default_queue_mutation(&registry);
        let aliases_before = fs::read(registry.join("aliases.json")).expect("aliases before");
        let registry_before = fs::read(registry.join("registry.json")).expect("registry before");

        let refusal = commit_review_import_default_queue_mutation_with_hook(&mutation, || {
            Err(std::io::Error::other("injected registry publish failure"))
        })
        .expect_err("second publish failure refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityReviewImport);
        assert_eq!(refusal.detail["writes_performed"], true);
        assert_eq!(refusal.detail["rollback_status"], "rolled_back");
        assert_eq!(
            fs::read(registry.join("aliases.json")).expect("aliases after"),
            aliases_before
        );
        assert_eq!(
            fs::read(registry.join("registry.json")).expect("registry after"),
            registry_before
        );
    }

    #[test]
    fn review_v1_import_final_verification_failure_reports_write_and_rolls_back() {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        write_review_import_test_registry(&registry, "2026.06.25", 1);
        let mut mutation = sample_review_import_default_queue_mutation(&registry);
        mutation.registry_snapshot_after = "blake3:not-the-planned-snapshot".to_string();
        let aliases_before = fs::read(registry.join("aliases.json")).expect("aliases before");
        let registry_before = fs::read(registry.join("registry.json")).expect("registry before");

        let refusal = commit_review_import_default_queue_mutation(&mutation)
            .expect_err("final verification failure refuses");

        assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
        assert_eq!(refusal.detail["writes_performed"], true);
        assert_eq!(refusal.detail["rollback_status"], "rolled_back");
        assert_eq!(
            fs::read(registry.join("aliases.json")).expect("aliases after"),
            aliases_before
        );
        assert_eq!(
            fs::read(registry.join("registry.json")).expect("registry after"),
            registry_before
        );
    }

    fn sample_review_v1_artifact() -> Value {
        let contract = entity_artifact_v1_contract_for_version(CANON_ENTITY_REVIEW_VERSION_V1)
            .expect("review v1 contract");
        let mut artifact = json!({
            "version": CANON_ENTITY_REVIEW_VERSION_V1,
            "artifact_content_hash": "",
            "metadata": {
                "profile": {
                    "id": "cmbs_tenant_label",
                    "version": "0.1.0",
                    "entity_type": "tenant_label",
                    "identity_semantics": "canonical_display_label",
                    "canonical_type": "tenant_label",
                    "patch_namespaces": {
                        "aliases": "cmbs_tenant_label.aliases",
                        "distinct": "cmbs_tenant_label.distinct",
                        "relations": "cmbs_tenant_label.relations"
                    },
                    "content_hash": "blake3:profile"
                },
                "strategy": {
                    "id": "cmbs_tenant_label.v1",
                    "version": "0.1.0",
                    "content_hash": "blake3:strategy"
                },
                "registry_snapshot": {
                    "id": "cmbs-tenants",
                    "version": "2026.06.25",
                    "source": "registry",
                    "lookup_snapshot_hash": "blake3:registry"
                },
                "input": {
                    "row_count": 2,
                    "content_hash": "blake3:input"
                },
                "patch_namespace": "cmbs_tenant_label.aliases",
                "schema": {
                    "key": contract.schema_key,
                    "content_hash": entity_v1_schema_content_hash(contract).expect("schema hash")
                },
                "workdir": {
                    "root_dir": "target/entity-work/test",
                    "stage_dir": contract.stage_dir,
                    "artifact_relpath": contract.artifact_relpath,
                    "payload_relpath": contract.payload_relpath
                },
                "upstream_artifacts": [],
                "patch_set": {
                    "content_hash": "blake3:patch",
                    "paths": []
                },
                "namekit": {
                    "version": "namekit.v0",
                    "content_hash": "blake3:namekit"
                },
                "artifact_content_hash": ""
            },
            "summary": {
                "counts": {
                    "review_items": 2,
                    "review_group_count": 2,
                    "review_rows_covered": 2
                },
                "labels": {
                    "stage": "review",
                    "include": "all"
                }
            },
            "review_queue_path": "review/queue.jsonl",
            "source_result": {
                "version": "canon_entity_run.v1",
                "content_hash": "blake3:run"
            },
            "include": "all",
            "review_items": [
                {
                    "review_id": "review:1",
                    "state": "promotable_new",
                    "surface_ids": ["surface:1"],
                    "decision": "accept_alias",
                    "operator_id": "operator-1",
                    "reason_code": "confirmed"
                },
                {
                    "review_id": "review:2",
                    "state": "needs_review",
                    "surface_ids": ["surface:2"]
                }
            ],
            "next_commands": {
                "audit": "canon entity audit <RESULT.json> --suite <SUITE_DIR>",
                "review_import": "canon entity review import <REVIEW.json|csv> --registry <REGISTRY> --next-version <VER>",
                "promote": "canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY> --next-version <VER>"
            }
        });
        finalize_entity_v1_self_hash(&mut artifact).expect("self hash finalizes");
        artifact
    }

    fn sample_review_v1_artifact_for_import(registry_hash: &str, source_hash: &str) -> Value {
        let mut artifact = sample_review_v1_artifact();
        artifact["metadata"]["registry_snapshot"]["lookup_snapshot_hash"] =
            Value::String(registry_hash.to_string());
        artifact["source_result"] = json!({
            "version": CANON_ENTITY_RUN_VERSION_V1,
            "content_hash": source_hash
        });
        let item = artifact["review_items"][0]
            .as_object_mut()
            .expect("review item object");
        item.insert(
            "decision".to_string(),
            Value::String("accept_alias".to_string()),
        );
        item.insert(
            "alias_proposal".to_string(),
            sample_alias_proposal("Sears Holdings"),
        );
        finalize_entity_v1_self_hash(&mut artifact).expect("review self hash finalizes");
        artifact
    }

    fn sample_alias_proposal(input: &str) -> Value {
        let mut proposal = json!({
            "version": CANON_ENTITY_ALIAS_PROPOSAL_VERSION,
            "proposal_id": "",
            "content_hash": "",
            "allowed_actions": ["accept_alias", "reject_alias"],
            "input": input,
            "canonical_id": "TNT-SEARS",
            "canonical_type": "tenant_label",
            "rule_id": "ENTITY_REVIEW_IMPORT",
            "component_id": "component:sears",
            "source_surface_ids": ["surface:1"]
        });
        let hash = alias_proposal_content_hash(&proposal).expect("proposal hash");
        proposal["proposal_id"] = Value::String(format!("alias_proposal:{hash}"));
        proposal["content_hash"] = Value::String(hash);
        proposal
    }

    fn sample_review_import_default_queue_mutation(
        registry: &Path,
    ) -> ReviewImportDefaultQueueMutation {
        let registry_hash = review_import_registry_snapshot_hash(registry).expect("registry hash");
        let run = sample_run_v1_artifact(&registry_hash);
        let run_hash =
            required_value_string(&run, &["artifact_content_hash"], "run hash").expect("run hash");
        let review = sample_review_v1_artifact_for_import(&registry_hash, run_hash);
        let decisions = reviewed_decisions_from_v1(&review);
        let plan = review_import_default_queue_plan_from_v1_decisions(&review, &decisions)
            .expect("default queue plan derives");
        build_review_import_default_queue_mutation(
            registry,
            registry_json_value(registry).expect("registry json"),
            "2026.06.26",
            &review,
            plan,
            &registry_hash,
        )
        .expect("mutation builds")
    }

    fn sample_run_v1_artifact(registry_hash: &str) -> Value {
        let contract = entity_artifact_v1_contract_for_version(CANON_ENTITY_RUN_VERSION_V1)
            .expect("run v1 contract");
        let mut artifact = json!({
            "version": CANON_ENTITY_RUN_VERSION_V1,
            "artifact_content_hash": "",
            "metadata": {
                "profile": {
                    "id": "cmbs_tenant_label",
                    "version": "0.1.0",
                    "entity_type": "tenant_label",
                    "identity_semantics": "canonical_display_label",
                    "canonical_type": "tenant_label",
                    "patch_namespaces": {
                        "aliases": "cmbs_tenant_label.aliases",
                        "distinct": "cmbs_tenant_label.distinct",
                        "relations": "cmbs_tenant_label.relations"
                    },
                    "content_hash": "blake3:profile"
                },
                "strategy": {
                    "id": "cmbs_tenant_label.v1",
                    "version": "0.1.0",
                    "content_hash": "blake3:strategy"
                },
                "registry_snapshot": {
                    "id": "cmbs-tenants",
                    "version": "2026.06.25",
                    "source": "registry",
                    "lookup_snapshot_hash": registry_hash
                },
                "input": {
                    "row_count": 2,
                    "content_hash": "blake3:input"
                },
                "patch_namespace": "cmbs_tenant_label.aliases",
                "schema": {
                    "key": contract.schema_key,
                    "content_hash": entity_v1_schema_content_hash(contract)
                        .expect("run schema hash")
                },
                "workdir": {
                    "root_dir": "target/entity-work/test",
                    "stage_dir": contract.stage_dir,
                    "artifact_relpath": contract.artifact_relpath,
                    "payload_relpath": contract.payload_relpath
                },
                "upstream_artifacts": [],
                "artifact_content_hash": ""
            },
            "summary": {
                "counts": {
                    "rows": 2
                },
                "labels": {
                    "stage": "run"
                }
            },
            "run_manifest_path": "run/manifest.json"
        });
        finalize_entity_v1_self_hash(&mut artifact).expect("run self hash finalizes");
        artifact
    }

    fn sample_review_import_audit(root: &Path, run: Value) -> Value {
        let suite_dir = root.join("audit-suite");
        fs::create_dir_all(&suite_dir).expect("audit suite");
        run_entity_audit_v1(EntityAuditV1Request {
            result_artifact: run,
            suite_dir: &suite_dir,
        })
        .expect("audit v1 succeeds")
    }

    fn write_review_import_test_registry(path: &Path, version: &str, entry_count: u64) {
        fs::create_dir_all(path).expect("registry dir");
        fs::write(
            path.join("registry.json"),
            format!(
                r#"{{
  "id": "cmbs-tenants",
  "version": "{version}",
  "description": "review import test registry",
  "updated": "2026-07-11",
  "entry_count": {entry_count},
  "entity_profile": {{
    "id": "cmbs_tenant_label",
    "identity_semantics": "canonical_display_label"
  }}
}}"#
            ),
        )
        .expect("registry.json");
        fs::write(
            path.join("aliases.json"),
            r#"[
  {
    "input": "Sears",
    "canonical_id": "TNT-SEARS",
    "canonical_type": "tenant_label",
    "rule_id": "ENTITY_REVIEW_PROMOTE"
  }
]"#,
        )
        .expect("aliases.json");
    }

    fn registry_snapshot_for_test(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        for entry in fs::read_dir(path).expect("registry dir") {
            let entry = entry.expect("registry entry");
            let file_path = entry.path();
            if file_path.is_file() {
                snapshot.insert(
                    entry.file_name().to_string_lossy().to_string(),
                    fs::read(file_path).expect("registry file bytes"),
                );
            }
        }
        snapshot
    }

    fn registry_tree_snapshot_for_test(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        collect_registry_tree_snapshot_for_test(path, path, &mut snapshot);
        snapshot
    }

    fn collect_registry_tree_snapshot_for_test(
        root: &Path,
        dir: &Path,
        snapshot: &mut BTreeMap<String, Vec<u8>>,
    ) {
        for entry in fs::read_dir(dir).expect("registry tree") {
            let entry = entry.expect("registry entry");
            let file_path = entry.path();
            if file_path.is_dir() {
                collect_registry_tree_snapshot_for_test(root, &file_path, snapshot);
            } else if file_path.is_file() {
                let relative = file_path
                    .strip_prefix(root)
                    .expect("relative registry path")
                    .to_string_lossy()
                    .replace('\\', "/");
                snapshot.insert(relative, fs::read(file_path).expect("registry file bytes"));
            }
        }
    }

    fn read_jsonl_values(path: &Path) -> Vec<Value> {
        let text = fs::read_to_string(path).expect("jsonl file");
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("jsonl value"))
            .collect()
    }
}
