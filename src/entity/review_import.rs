#![forbid(unsafe_code)]

//! Review decision import validation and decision-ledger append.

use crate::{
    Refusal,
    entity::{
        EntityArtifactMetadata, EntityArtifactReference,
        error::EntityRefusalKind,
        ledger::{
            DecisionLedgerAppendReceipt, DecisionLedgerEventInput, DecisionLedgerEventType,
            DecisionLedgerRefs, append_decision_ledger_event, build_decision_ledger_event,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
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
