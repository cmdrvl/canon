#![forbid(unsafe_code)]

//! Grouped review-queue export for entity solve artifacts.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_SOLVE_VERSION, EntityArtifactMetadata, EntityArtifactReference,
        EntityDeterministicSummary,
        error::EntityRefusalKind,
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
        solve::{
            SolveArtifact, SolveComponentDiagnostics, SolveEvidenceCut, SolveReconciliationState,
            SolveReviewGroupSeed, validate_solve_artifact_contract,
        },
    },
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewQueueArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub source_solve_hash: String,
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
                format!("{:?}", item.state).to_ascii_lowercase(),
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
