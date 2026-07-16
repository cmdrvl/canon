#![forbid(unsafe_code)]

//! Grouped review-queue export for entity solve artifacts.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_REVIEW_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION,
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactMetadata, EntityArtifactReference,
        EntityArtifactStageV1, EntityDeterministicSummary,
        error::EntityRefusalKind,
        run::link::{
            ENTITY_LINK_VERSION, EntityLinkArtifact, validate_entity_link_artifact_contract,
        },
        schema::{
            CANON_ENTITY_REVIEW_QUEUE_VERSION, entity_v1_artifact_reference,
            entity_v1_lifecycle_metadata_from_source, finalize_entity_v1_self_hash,
            validate_artifact_v1_core_contract,
        },
        solve::{
            SolveArtifact, SolveComponentDiagnostics, SolveEvidenceCut, SolveReconciliationState,
            SolveReviewGroupSeed, validate_solve_artifact_contract,
            validate_solve_artifact_envelope_contract,
        },
    },
    resolve::{AmbiguousRecord, CandidateScore, MatchRecord, UnmatchedRecord},
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewExportInclude {
    Resolved,
    Escrow,
    Contradictions,
    All,
}

impl ReviewExportInclude {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Escrow => "escrow",
            Self::Contradictions => "contradictions",
            Self::All => "all",
        }
    }

    const fn includes(self, state: SolveReconciliationState) -> bool {
        match self {
            Self::Resolved => matches!(
                state,
                SolveReconciliationState::ResolvedExisting
                    | SolveReconciliationState::PromotableNew
            ),
            Self::Escrow => matches!(state, SolveReconciliationState::Escrow),
            Self::Contradictions => matches!(state, SolveReconciliationState::Contradiction),
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewQueueRequest {
    pub solve_artifact: SolveArtifact,
    pub include: ReviewExportInclude,
    pub provenance_samples: Vec<ReviewProvenanceSample>,
    pub relation_hints: Vec<ReviewRelationHint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkReviewQueueRequest {
    pub link_artifact: EntityLinkArtifact,
    pub include: ReviewExportInclude,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewQueueArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub source_solve_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_link_hash: Option<String>,
    pub review_items: Vec<ReviewQueueItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewQueueItem {
    pub review_id: String,
    pub ambiguity_key: String,
    pub component_id: String,
    pub state: SolveReconciliationState,
    pub proposed_action: String,
    pub review_priority_units: u32,
    pub priority_reasons: Vec<String>,
    pub affected_rows: u64,
    pub affected_deals: u64,
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strongest_positive_cut: Option<SolveEvidenceCut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strongest_negative_cut: Option<SolveEvidenceCut>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_hints: Vec<ReviewRelationHint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_samples: Vec<ReviewProvenanceSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewProvenanceSample {
    pub surface_id: String,
    pub row_id: String,
    pub source: String,
    pub raw_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRelationHint {
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub relation: String,
    pub reason_code: String,
}

pub fn build_review_queue_artifact(
    request: ReviewQueueRequest,
) -> Result<ReviewQueueArtifact, Refusal> {
    validate_solve_artifact_contract(&request.solve_artifact)?;
    if request
        .solve_artifact
        .artifact_content_hash
        .trim()
        .is_empty()
    {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Review export requires a hashed solve artifact",
            json!({
                "stage": "review_export",
                "field": "source_solve_hash"
            }),
        ));
    }

    let source_solve_hash = request.solve_artifact.artifact_content_hash.clone();
    let mut metadata = request.solve_artifact.metadata.clone();
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        content_hash: source_solve_hash.clone(),
    }];

    let mut review_items = review_items_from_solve(
        &request.solve_artifact,
        request.include,
        &request.provenance_samples,
        &request.relation_hints,
    );
    review_items.sort_by(review_item_cmp);

    let summary = review_queue_summary(&review_items, request.include);
    let mut artifact = ReviewQueueArtifact {
        version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        source_solve_hash,
        source_link_hash: None,
        review_items,
    };
    artifact.artifact_content_hash = hash_review_queue_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn build_link_review_queue_artifact(
    request: LinkReviewQueueRequest,
) -> Result<ReviewQueueArtifact, Refusal> {
    validate_entity_link_artifact_contract(&request.link_artifact)?;
    if request
        .link_artifact
        .shared_solve_artifact
        .content_hash
        .trim()
        .is_empty()
    {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Link review export requires a hashed solve source",
            json!({
                "stage": "review_export",
                "field": "source_solve_hash"
            }),
        ));
    }
    let source_solve_hash = request
        .link_artifact
        .shared_solve_artifact
        .content_hash
        .clone();
    let source_link_hash = request.link_artifact.artifact_content_hash.clone();
    let mut metadata = request.link_artifact.metadata.clone();
    metadata.artifact_content_hash.clear();
    metadata.upstream_artifacts = vec![
        EntityArtifactReference {
            version: ENTITY_LINK_VERSION.to_string(),
            content_hash: source_link_hash.clone(),
        },
        request.link_artifact.shared_solve_artifact.clone(),
    ];
    metadata.upstream_artifacts.sort_by(artifact_ref_cmp);

    let mut review_items = review_items_from_link(&request.link_artifact, request.include);
    review_items.sort_by(review_item_cmp);
    let summary = review_queue_summary(&review_items, request.include);
    let mut artifact = ReviewQueueArtifact {
        version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        source_solve_hash,
        source_link_hash: Some(source_link_hash),
        review_items,
    };
    artifact.artifact_content_hash = hash_review_queue_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn render_review_queue_csv(artifact: &ReviewQueueArtifact) -> Result<String, Refusal> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "review_id",
            "review_priority_units",
            "priority_reasons_json",
            "affected_rows",
            "affected_deals",
            "component_id",
            "state",
            "proposed_action",
            "surface_ids_json",
            "positive_evidence_json",
            "anti_merge_evidence_json",
            "relation_hints_json",
            "provenance_samples_json",
        ])
        .map_err(csv_refusal)?;

    for item in &artifact.review_items {
        writer
            .write_record([
                item.review_id.clone(),
                item.review_priority_units.to_string(),
                serde_json::to_string(&item.priority_reasons).map_err(json_refusal)?,
                item.affected_rows.to_string(),
                item.affected_deals.to_string(),
                item.component_id.clone(),
                review_state_label(item.state).to_string(),
                item.proposed_action.clone(),
                serde_json::to_string(&item.surface_ids).map_err(json_refusal)?,
                serde_json::to_string(&item.strongest_positive_cut).map_err(json_refusal)?,
                serde_json::to_string(&item.strongest_negative_cut).map_err(json_refusal)?,
                serde_json::to_string(&item.relation_hints).map_err(json_refusal)?,
                serde_json::to_string(&item.provenance_samples).map_err(json_refusal)?,
            ])
            .map_err(csv_refusal)?;
    }

    let bytes = writer.into_inner().map_err(|error| {
        review_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to finalize review queue CSV",
            json!({
                "stage": "review_export",
                "error": error.to_string()
            }),
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        review_refusal(
            EntityRefusalKind::ReviewImport,
            "Review queue CSV was not UTF-8",
            json!({
                "stage": "review_export",
                "error": error.to_string()
            }),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewV1ExportRequest {
    pub result_artifact: Value,
    pub include: ReviewExportInclude,
}

pub fn build_review_v1_artifact(request: ReviewV1ExportRequest) -> Result<Value, Refusal> {
    validate_review_v1_source(&request.result_artifact)?;
    let source_hash = required_value_string(
        &request.result_artifact,
        &["artifact_content_hash"],
        "artifact_content_hash",
    )?;
    let source_version = required_value_string(&request.result_artifact, &["version"], "version")?;
    let source_ref = entity_v1_artifact_reference(&request.result_artifact)?;
    let metadata = entity_v1_lifecycle_metadata_from_source(
        &request.result_artifact,
        EntityArtifactStageV1::Review,
        vec![source_ref],
    )?;
    let review_items = review_items_from_v1_result(&request.result_artifact, request.include)?;
    let review_item_count = review_items.len() as u64;
    let source_review_groups = summary_count_any(
        &request.result_artifact,
        &["review_groups", "review_group_count", "review_items"],
    );
    let effective_review_groups = review_item_count.max(source_review_groups);
    let rows = required_value_u64(
        &request.result_artifact,
        &["metadata", "input", "row_count"],
    )
    .unwrap_or(0);

    let mut artifact = json!({
        "version": CANON_ENTITY_REVIEW_VERSION_V1,
        "artifact_content_hash": "",
        "metadata": metadata,
        "summary": {
            "counts": {
                "review_items": review_item_count,
                "review_group_count": effective_review_groups,
                "review_rows_covered": rows
            },
            "labels": {
                "stage": "review",
                "include": request.include.as_str(),
                "source_version": source_version
            }
        },
        "review_queue_path": "review/queue.jsonl",
        "source_result": {
            "version": source_version,
            "content_hash": source_hash
        },
        "include": request.include.as_str(),
        "review_items": review_items,
        "next_commands": {
            "audit": "canon entity audit <RESULT.json> --suite <SUITE_DIR>",
            "review_import": "canon entity review import <REVIEW.json|csv> --registry <REGISTRY> --next-version <VER>",
            "promote": "canon entity promote <RESULT.json> --audit <AUDIT.json> --registry <REGISTRY> --next-version <VER>"
        }
    });
    finalize_entity_v1_self_hash(&mut artifact)?;
    Ok(artifact)
}

pub fn render_review_v1_csv(artifact: &Value) -> Result<String, Refusal> {
    validate_review_v1_artifact(artifact)?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "review_id",
            "decision",
            "operator_id",
            "reason_code",
            "surface_ids_json",
            "review_context_json",
            "item_json",
        ])
        .map_err(csv_refusal)?;

    let context = review_context_for_csv(artifact)?;
    let items = artifact
        .get("review_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        writer
            .write_record([
                "__context__".to_string(),
                String::new(),
                String::new(),
                String::new(),
                "[]".to_string(),
                context,
                "{}".to_string(),
            ])
            .map_err(csv_refusal)?;
    } else {
        for item in items {
            let review_id = item
                .get("review_id")
                .and_then(Value::as_str)
                .unwrap_or("review:unknown")
                .to_string();
            let surface_ids = item
                .get("surface_ids")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            writer
                .write_record([
                    review_id,
                    item.get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    item.get("operator_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    item.get("reason_code")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    serde_json::to_string(&surface_ids).map_err(json_refusal)?,
                    context.clone(),
                    serde_json::to_string(&item).map_err(json_refusal)?,
                ])
                .map_err(csv_refusal)?;
        }
    }

    let bytes = writer.into_inner().map_err(|error| {
        review_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to finalize review v1 CSV",
            json!({
                "stage": "review",
                "error": error.to_string()
            }),
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        review_refusal(
            EntityRefusalKind::ReviewImport,
            "Review v1 CSV was not UTF-8",
            json!({
                "stage": "review",
                "error": error.to_string()
            }),
        )
    })
}

pub fn render_review_v1_summary(artifact: &Value) -> String {
    let profile = value_string_or(artifact, &["metadata", "profile", "id"], "<profile>");
    let registry_id = value_string_or(
        artifact,
        &["metadata", "registry_snapshot", "id"],
        "<registry>",
    );
    let registry_version = value_string_or(
        artifact,
        &["metadata", "registry_snapshot", "version"],
        "<version>",
    );
    let items = value_u64_or(artifact, &["summary", "counts", "review_items"], 0);
    let groups = value_u64_or(artifact, &["summary", "counts", "review_group_count"], 0);
    format!(
        "{} review v1 registry={}@{} items={} groups={}",
        profile, registry_id, registry_version, items, groups
    )
}

pub fn validate_review_v1_artifact(artifact: &Value) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(artifact)?;
    if contract.artifact_version != CANON_ENTITY_REVIEW_VERSION_V1 {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Review import requires a canon_entity_review.v1 artifact",
            json!({
                "stage": "review",
                "field": "version",
                "expected": CANON_ENTITY_REVIEW_VERSION_V1,
                "actual": contract.artifact_version
            }),
        ));
    }
    Ok(())
}

fn review_items_from_solve(
    solve_artifact: &SolveArtifact,
    include: ReviewExportInclude,
    provenance_samples: &[ReviewProvenanceSample],
    relation_hints: &[ReviewRelationHint],
) -> Vec<ReviewQueueItem> {
    let seeds_by_component = solve_artifact
        .review_groups
        .iter()
        .map(|seed| (seed.component_id.as_str(), seed))
        .collect::<BTreeMap<_, _>>();

    solve_artifact
        .diagnostics
        .components
        .iter()
        .filter(|component| include.includes(component.state))
        .map(|component| {
            let seed = seeds_by_component
                .get(component.component_id.as_str())
                .copied();
            review_item(component, seed, provenance_samples, relation_hints)
        })
        .collect()
}

fn review_items_from_link(
    link_artifact: &EntityLinkArtifact,
    include: ReviewExportInclude,
) -> Vec<ReviewQueueItem> {
    let resolved = matches!(
        include,
        ReviewExportInclude::Resolved | ReviewExportInclude::All
    )
    .then(|| {
        link_artifact
            .decision_artifact
            .matches
            .iter()
            .map(review_item_from_match)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let escrow = matches!(
        include,
        ReviewExportInclude::Escrow | ReviewExportInclude::All
    )
    .then(|| {
        link_artifact
            .decision_artifact
            .ambiguous
            .iter()
            .map(review_item_from_ambiguous)
            .chain(
                link_artifact
                    .decision_artifact
                    .unmatched
                    .iter()
                    .map(review_item_from_unmatched),
            )
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    resolved.into_iter().chain(escrow).collect()
}

fn review_item_from_match(record: &MatchRecord) -> ReviewQueueItem {
    let relation_hints = link_relation_hints(
        &record.target_id,
        [record.reference_id.as_str()].into_iter(),
        "directional_match",
    );
    let surface_ids = link_surface_ids(
        &record.target_id,
        [record.reference_id.as_str()].into_iter(),
    );
    ReviewQueueItem {
        review_id: link_review_id(
            "resolved",
            &record.target_id,
            "directional_match",
            [record.reference_id.as_str()].into_iter(),
        ),
        ambiguity_key: format!("link:resolved:{}", record.target_id),
        component_id: record.target_id.clone(),
        state: SolveReconciliationState::ResolvedExisting,
        proposed_action: "audit_directional_match".to_string(),
        review_priority_units: 500,
        priority_reasons: vec!["directional_match".to_string()],
        affected_rows: 1,
        affected_deals: 0,
        surface_ids,
        strongest_positive_cut: None,
        strongest_negative_cut: None,
        relation_hints,
        provenance_samples: Vec::new(),
    }
}

fn review_item_from_ambiguous(record: &AmbiguousRecord) -> ReviewQueueItem {
    let candidate_ids = sorted_candidate_reference_ids(&record.candidates);
    let relation_hints = link_relation_hints(
        &record.target_id,
        candidate_ids.iter().map(String::as_str),
        "directional_candidate",
    );
    let surface_ids = link_surface_ids(&record.target_id, candidate_ids.iter().map(String::as_str));
    ReviewQueueItem {
        review_id: link_review_id(
            "escrow",
            &record.target_id,
            "ambiguous",
            candidate_ids.iter().map(String::as_str),
        ),
        ambiguity_key: format!("link:ambiguous:{}", record.target_id),
        component_id: record.target_id.clone(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "review_directional_abstention".to_string(),
        review_priority_units: 2_000,
        priority_reasons: vec!["ambiguous".to_string()],
        affected_rows: 1,
        affected_deals: 0,
        surface_ids,
        strongest_positive_cut: None,
        strongest_negative_cut: None,
        relation_hints,
        provenance_samples: Vec::new(),
    }
}

fn review_item_from_unmatched(record: &UnmatchedRecord) -> ReviewQueueItem {
    let candidate_ids = record
        .best_candidate
        .as_ref()
        .map(|candidate| vec![candidate.reference_id.clone()])
        .unwrap_or_default();
    let relation_hints = link_relation_hints(
        &record.target_id,
        candidate_ids.iter().map(String::as_str),
        "directional_near_miss",
    );
    let surface_ids = link_surface_ids(&record.target_id, candidate_ids.iter().map(String::as_str));
    ReviewQueueItem {
        review_id: link_review_id(
            "escrow",
            &record.target_id,
            "unmatched",
            candidate_ids.iter().map(String::as_str),
        ),
        ambiguity_key: format!("link:unmatched:{}", record.target_id),
        component_id: record.target_id.clone(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "review_directional_abstention".to_string(),
        review_priority_units: 2_000,
        priority_reasons: vec!["unmatched".to_string()],
        affected_rows: 1,
        affected_deals: 0,
        surface_ids,
        strongest_positive_cut: None,
        strongest_negative_cut: None,
        relation_hints,
        provenance_samples: Vec::new(),
    }
}

fn sorted_candidate_reference_ids(candidates: &[CandidateScore]) -> Vec<String> {
    let mut ids = candidates
        .iter()
        .map(|candidate| candidate.reference_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn link_relation_hints<'a>(
    target_id: &str,
    reference_ids: impl Iterator<Item = &'a str>,
    reason_code: &str,
) -> Vec<ReviewRelationHint> {
    let mut hints = reference_ids
        .map(|reference_id| ReviewRelationHint {
            left_surface_id: target_id.to_string(),
            right_surface_id: reference_id.to_string(),
            relation: "candidate".to_string(),
            reason_code: reason_code.to_string(),
        })
        .collect::<Vec<_>>();
    hints.sort_by(relation_hint_cmp);
    hints
}

fn link_surface_ids<'a>(
    target_id: &str,
    reference_ids: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let mut surface_ids = std::iter::once(target_id.to_string())
        .chain(reference_ids.map(str::to_string))
        .collect::<Vec<_>>();
    surface_ids.sort();
    surface_ids.dedup();
    surface_ids
}

fn link_review_id<'a>(
    partition: &str,
    target_id: &str,
    reason: &str,
    reference_ids: impl Iterator<Item = &'a str>,
) -> String {
    let mut references = reference_ids.map(str::to_string).collect::<Vec<_>>();
    references.sort();
    references.dedup();
    let material = BTreeMap::from([
        ("partition", json!(partition)),
        ("target_id", json!(target_id)),
        ("reason", json!(reason)),
        ("reference_ids", json!(references)),
    ]);
    let bytes = serde_json::to_vec(&material).expect("semantic review ID material serializes");
    format!("review:link:{}", witness::hash_bytes(&bytes))
}

fn validate_review_v1_source(artifact: &Value) -> Result<(), Refusal> {
    let contract = validate_artifact_v1_core_contract(artifact)?;
    if !matches!(
        contract.artifact_version,
        CANON_ENTITY_RUN_VERSION_V1 | CANON_ENTITY_SOLVE_VERSION_V1
    ) {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Review export requires a canon_entity_run.v1 or canon_entity_solve.v1 artifact",
            json!({
                "stage": "review",
                "field": "version",
                "expected": [CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1],
                "actual": contract.artifact_version
            }),
        ));
    }
    if contract.artifact_version == CANON_ENTITY_SOLVE_VERSION_V1 {
        let solve = serde_json::from_value::<SolveArtifact>(artifact.clone()).map_err(|error| {
            review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Review export solve artifact is malformed",
                json!({
                    "stage": "review",
                    "field": "solve_artifact",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        validate_solve_artifact_envelope_contract(&solve)?;
    }
    Ok(())
}

pub(crate) fn required_value_string<'a>(
    value: &'a Value,
    path: &[&str],
    rendered_path: &str,
) -> Result<&'a str, Refusal> {
    descend_value(value, path)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| missing_v1_field(rendered_path))
}

pub(crate) fn required_value_u64(value: &Value, path: &[&str]) -> Result<u64, Refusal> {
    descend_value(value, path)
        .and_then(Value::as_u64)
        .ok_or_else(|| missing_v1_field(&path.join(".")))
}

pub(crate) fn value_string_or<'a>(value: &'a Value, path: &[&str], fallback: &'a str) -> &'a str {
    descend_value(value, path)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

pub(crate) fn value_u64_or(value: &Value, path: &[&str], fallback: u64) -> u64 {
    descend_value(value, path)
        .and_then(Value::as_u64)
        .unwrap_or(fallback)
}

pub(crate) fn descend_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

pub(crate) fn missing_v1_field(path: &str) -> Refusal {
    review_refusal(
        EntityRefusalKind::ArtifactContract,
        "Entity v1 lifecycle artifact is missing required context",
        json!({
            "stage": "review",
            "field": path,
            "writes_performed": false
        }),
    )
}

fn summary_count_any(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| descend_value(value, &["summary", "counts", key]).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn review_items_from_v1_result(
    result: &Value,
    include: ReviewExportInclude,
) -> Result<Vec<Value>, Refusal> {
    let mut items = review_items_from_v1_alias_proposals(result, include)?;
    items.extend(
        [
            "review_items",
            "review_groups",
            "entities",
            "abstentions",
            "contradictions",
        ]
        .into_iter()
        .filter_map(|field| result.get(field).and_then(Value::as_array))
        .flat_map(|items| items.iter().cloned())
        .filter(|item| v1_item_included(item, include))
        .map(normalize_v1_review_item)
        .collect::<Vec<_>>(),
    );
    items.sort_by(|left, right| {
        value_string_or(left, &["review_id"], "").cmp(value_string_or(right, &["review_id"], ""))
    });
    Ok(items)
}

fn review_items_from_v1_alias_proposals(
    result: &Value,
    include: ReviewExportInclude,
) -> Result<Vec<Value>, Refusal> {
    if !matches!(
        include,
        ReviewExportInclude::Resolved | ReviewExportInclude::All
    ) {
        return Ok(Vec::new());
    }
    let Some(proposals) = result.get("promotable_aliases").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    proposals
        .iter()
        .map(alias_proposal_review_item)
        .collect::<Result<Vec<_>, _>>()
}

fn alias_proposal_review_item(proposal: &Value) -> Result<Value, Refusal> {
    let proposal_id = alias_proposal_string(proposal, "proposal_id")?;
    alias_proposal_string(proposal, "version")?;
    alias_proposal_string(proposal, "content_hash")?;
    alias_proposal_string(proposal, "input")?;
    alias_proposal_string(proposal, "canonical_id")?;
    alias_proposal_string(proposal, "canonical_type")?;
    alias_proposal_string(proposal, "rule_id")?;
    let component_id = alias_proposal_string(proposal, "component_id")?;
    let source_surface_ids = alias_proposal_string_array(proposal, "source_surface_ids")?;
    let allowed_actions = alias_proposal_string_array(proposal, "allowed_actions")?;
    if !allowed_actions
        .iter()
        .any(|action| action == "accept_alias")
    {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Solve alias proposal is missing accept_alias action",
            json!({
                "stage": "review",
                "field": "promotable_aliases.allowed_actions",
                "proposal_id": proposal_id,
                "writes_performed": false
            }),
        ));
    }

    Ok(json!({
        "review_id": proposal_id.clone(),
        "decision": "",
        "proposed_action": "accept_or_reject_alias_proposal",
        "reason_code": "solve_alias_proposal",
        "state": "resolved_existing",
        "component_id": component_id.clone(),
        "surface_ids": source_surface_ids.clone(),
        "alias_proposal": proposal.clone()
    }))
}

fn alias_proposal_string(proposal: &Value, field: &'static str) -> Result<String, Refusal> {
    proposal
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !ascii_trim(value).is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Solve alias proposal is missing required review binding",
                json!({
                    "stage": "review",
                    "field": format!("promotable_aliases.{field}"),
                    "writes_performed": false
                }),
            )
        })
}

fn alias_proposal_string_array(
    proposal: &Value,
    field: &'static str,
) -> Result<Vec<String>, Refusal> {
    let values = proposal
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Solve alias proposal is missing required review binding",
                json!({
                    "stage": "review",
                    "field": format!("promotable_aliases.{field}"),
                    "writes_performed": false
                }),
            )
        })?;
    let parsed = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|text| !ascii_trim(text).is_empty())
                .ok_or_else(|| {
                    review_refusal(
                        EntityRefusalKind::ArtifactContract,
                        "Solve alias proposal review binding must contain non-empty strings",
                        json!({
                            "stage": "review",
                            "field": format!("promotable_aliases.{field}"),
                            "writes_performed": false
                        }),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.is_empty() {
        return Err(review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Solve alias proposal review binding must not be empty",
            json!({
                "stage": "review",
                "field": format!("promotable_aliases.{field}"),
                "writes_performed": false
            }),
        ));
    }
    Ok(parsed.into_iter().map(str::to_string).collect())
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn v1_item_included(item: &Value, include: ReviewExportInclude) -> bool {
    let state = value_string_or(item, &["state"], "");
    match include {
        ReviewExportInclude::Resolved => {
            matches!(state, "resolved" | "resolved_existing" | "promotable_new")
        }
        ReviewExportInclude::Escrow => matches!(state, "escrow" | "pending" | "abstained"),
        ReviewExportInclude::Contradictions => matches!(state, "contradiction" | "conflict"),
        ReviewExportInclude::All => true,
    }
}

fn normalize_v1_review_item(item: Value) -> Value {
    let mut object = item.as_object().cloned().unwrap_or_else(Map::new);
    let review_id = object
        .get("review_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            let source = object
                .get("component_id")
                .or_else(|| object.get("canonical_id"))
                .or_else(|| object.get("escrow_id"))
                .and_then(Value::as_str)
                .unwrap_or("item");
            format!("review:{}", source.replace([':', '/'], "_"))
        });
    object.insert("review_id".to_string(), Value::String(review_id));
    object
        .entry("decision".to_string())
        .or_insert_with(|| Value::String(String::new()));
    object
        .entry("reason_code".to_string())
        .or_insert_with(|| Value::String("operator_review_required".to_string()));
    Value::Object(object)
}

fn review_context_for_csv(artifact: &Value) -> Result<String, Refusal> {
    let context = json!({
        "version": artifact.get("version").cloned().unwrap_or(Value::Null),
        "artifact_content_hash": artifact.get("artifact_content_hash").cloned().unwrap_or(Value::Null),
        "metadata": artifact.get("metadata").cloned().unwrap_or(Value::Null),
        "summary": artifact.get("summary").cloned().unwrap_or(Value::Null),
        "review_queue_path": artifact.get("review_queue_path").cloned().unwrap_or(Value::Null),
        "source_result": artifact.get("source_result").cloned().unwrap_or(Value::Null),
        "include": artifact.get("include").cloned().unwrap_or(Value::Null),
        "next_commands": artifact.get("next_commands").cloned().unwrap_or(Value::Null)
    });
    serde_json::to_string(&context).map_err(json_refusal)
}

fn review_item(
    component: &SolveComponentDiagnostics,
    seed: Option<&SolveReviewGroupSeed>,
    provenance_samples: &[ReviewProvenanceSample],
    relation_hints: &[ReviewRelationHint],
) -> ReviewQueueItem {
    let priority_reasons = review_priority_reasons(component, seed);
    ReviewQueueItem {
        review_id: seed
            .map(|seed| seed.review_group_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "review:{}",
                    stable_component_suffix(&component.component_id)
                )
            }),
        ambiguity_key: seed
            .map(|seed| seed.ambiguity_key.clone())
            .unwrap_or_else(|| {
                format!("{:?}:{}", component.state, component.reason).to_ascii_lowercase()
            }),
        component_id: component.component_id.clone(),
        state: component.state,
        proposed_action: proposed_review_action(component.state),
        review_priority_units: review_priority_units(component, &priority_reasons),
        priority_reasons,
        affected_rows: component.affected_rows,
        affected_deals: component.affected_deals,
        surface_ids: component.surface_ids.clone(),
        strongest_positive_cut: component.strongest_positive_cut.clone(),
        strongest_negative_cut: component.strongest_negative_cut.clone(),
        relation_hints: relation_hints_for_component(&component.surface_ids, relation_hints),
        provenance_samples: provenance_for_component(&component.surface_ids, provenance_samples),
    }
}

fn review_priority_reasons(
    component: &SolveComponentDiagnostics,
    seed: Option<&SolveReviewGroupSeed>,
) -> Vec<String> {
    let mut reasons = seed
        .map(|seed| seed.priority_reasons.clone())
        .unwrap_or_else(|| component.review_priority_reasons.clone());
    match component.state {
        SolveReconciliationState::Contradiction => reasons.push("hard_cannot_link".to_string()),
        SolveReconciliationState::Conflict => reasons.push("incumbent_conflict".to_string()),
        SolveReconciliationState::Escrow => reasons.push(component.reason.clone()),
        SolveReconciliationState::ResolvedExisting | SolveReconciliationState::PromotableNew => {}
    }
    if component.affected_rows >= 50 {
        reasons.push("high_row_count".to_string());
    }
    if component.affected_deals >= 10 {
        reasons.push("high_deal_count".to_string());
    }
    if component.strongest_positive_cut.is_some() && component.strongest_negative_cut.is_some() {
        reasons.push("support_and_cannot_link".to_string());
    }
    reasons.extend(regab_priority_reason_codes(component));
    reasons.sort();
    reasons.dedup();
    reasons
}

fn regab_priority_reason_codes(component: &SolveComponentDiagnostics) -> Vec<String> {
    let mut codes = Vec::new();
    for cut in [
        &component.strongest_positive_cut,
        &component.strongest_negative_cut,
    ]
    .into_iter()
    .flatten()
    {
        codes.extend(
            cut.evidence_reason_codes
                .iter()
                .filter(|code| code.starts_with("regab_"))
                .cloned(),
        );
    }
    codes
}

fn review_priority_units(component: &SolveComponentDiagnostics, reasons: &[String]) -> u32 {
    let state_units: u32 = match component.state {
        SolveReconciliationState::Contradiction => 4_000,
        SolveReconciliationState::Conflict => 3_500,
        SolveReconciliationState::Escrow => 2_000,
        SolveReconciliationState::PromotableNew => 1_000,
        SolveReconciliationState::ResolvedExisting => 500,
    };
    let row_units = component.affected_rows.min(500) as u32 * 6;
    let deal_units = component.affected_deals.min(100) as u32 * 20;
    let reason_units = reasons.len().min(8) as u32 * 250;
    state_units
        .saturating_add(row_units)
        .saturating_add(deal_units)
        .saturating_add(reason_units)
        .min(10_000)
}

fn proposed_review_action(state: SolveReconciliationState) -> String {
    match state {
        SolveReconciliationState::ResolvedExisting => "audit_existing_resolution",
        SolveReconciliationState::PromotableNew => "confirm_promotion_or_escrow",
        SolveReconciliationState::Escrow => "confirm_merge_distinct_or_relation",
        SolveReconciliationState::Conflict => "choose_incumbent_or_distinct",
        SolveReconciliationState::Contradiction => "confirm_distinct_or_request_override",
    }
    .to_string()
}

const fn review_state_label(state: SolveReconciliationState) -> &'static str {
    match state {
        SolveReconciliationState::ResolvedExisting => "resolved_existing",
        SolveReconciliationState::PromotableNew => "promotable_new",
        SolveReconciliationState::Escrow => "escrow",
        SolveReconciliationState::Conflict => "conflict",
        SolveReconciliationState::Contradiction => "contradiction",
    }
}

fn relation_hints_for_component(
    surface_ids: &[String],
    relation_hints: &[ReviewRelationHint],
) -> Vec<ReviewRelationHint> {
    let surface_set = surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut hints = relation_hints
        .iter()
        .filter(|hint| {
            surface_set.contains(hint.left_surface_id.as_str())
                || surface_set.contains(hint.right_surface_id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    hints.sort_by(relation_hint_cmp);
    hints
}

fn provenance_for_component(
    surface_ids: &[String],
    provenance_samples: &[ReviewProvenanceSample],
) -> Vec<ReviewProvenanceSample> {
    let surface_set = surface_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut samples = provenance_samples
        .iter()
        .filter(|sample| surface_set.contains(sample.surface_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    samples.sort_by(provenance_sample_cmp);
    samples
}

fn review_queue_summary(
    review_items: &[ReviewQueueItem],
    include: ReviewExportInclude,
) -> EntityDeterministicSummary {
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("review_items".to_string(), review_items.len() as u64),
            ("review_group_count".to_string(), review_items.len() as u64),
            (
                "review_rows_covered".to_string(),
                review_items.iter().map(|item| item.affected_rows).sum(),
            ),
            (
                "review_deals_covered".to_string(),
                review_items.iter().map(|item| item.affected_deals).sum(),
            ),
        ]),
        labels: BTreeMap::from([
            ("grouping".to_string(), "ambiguity_pattern".to_string()),
            ("include".to_string(), include.as_str().to_string()),
        ]),
    }
}

fn hash_review_queue_without_self(artifact: &ReviewQueueArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Failed to hash review queue artifact",
            json!({
                "stage": "review_export",
                "error": error.to_string()
            }),
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn stable_component_suffix(component_id: &str) -> String {
    component_id
        .strip_prefix("component:")
        .unwrap_or(component_id)
        .replace(':', "_")
}

fn review_item_cmp(left: &ReviewQueueItem, right: &ReviewQueueItem) -> std::cmp::Ordering {
    right
        .review_priority_units
        .cmp(&left.review_priority_units)
        .then_with(|| right.affected_rows.cmp(&left.affected_rows))
        .then_with(|| right.affected_deals.cmp(&left.affected_deals))
        .then_with(|| left.review_id.cmp(&right.review_id))
}

fn relation_hint_cmp(left: &ReviewRelationHint, right: &ReviewRelationHint) -> std::cmp::Ordering {
    left.left_surface_id
        .cmp(&right.left_surface_id)
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        .then_with(|| left.relation.cmp(&right.relation))
        .then_with(|| left.reason_code.cmp(&right.reason_code))
}

fn provenance_sample_cmp(
    left: &ReviewProvenanceSample,
    right: &ReviewProvenanceSample,
) -> std::cmp::Ordering {
    left.surface_id
        .cmp(&right.surface_id)
        .then_with(|| left.row_id.cmp(&right.row_id))
        .then_with(|| left.source.cmp(&right.source))
}

fn json_refusal(error: serde_json::Error) -> Refusal {
    review_refusal(
        EntityRefusalKind::ReviewImport,
        "Failed to serialize review queue field",
        json!({
            "stage": "review_export",
            "error": error.to_string()
        }),
    )
}

fn csv_refusal(error: csv::Error) -> Refusal {
    review_refusal(
        EntityRefusalKind::ReviewImport,
        "Failed to write review queue CSV",
        json!({
            "stage": "review_export",
            "error": error.to_string()
        }),
    )
}

fn review_refusal(
    kind: EntityRefusalKind,
    message: &'static str,
    detail: serde_json::Value,
) -> Refusal {
    kind.to_refusal(
        message,
        detail,
        Some("canon entity review export <SOLVE.json> --emit json".to_string()),
    )
}
