#![forbid(unsafe_code)]

//! Native offline review artifact export for cluster and link decisions.

use crate::{
    Refusal,
    entity::{
        EntityArtifactMetadata, EntityDeterministicSummary,
        error::EntityRefusalKind,
        review::{ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem},
        solve::SolveEvidenceCut,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const CANON_ENTITY_NATIVE_REVIEW_VERSION: &str = "canon_entity_native_review.v0";
pub const CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION: &str =
    "canon_entity_native_review_decisions.v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeReviewExportRequest {
    pub review_queue: ReviewQueueArtifact,
    pub run_content_hash: String,
    pub policy_content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub summary: EntityDeterministicSummary,
    pub binding: NativeReviewBinding,
    pub decision_schema: NativeReviewDecisionSchema,
    pub review_items: Vec<NativeReviewItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewBinding {
    pub source_review_queue_hash: String,
    pub run_content_hash: String,
    pub policy_content_hash: String,
    pub registry_snapshot_hash: String,
    pub registry_id: String,
    pub registry_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub strategy_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewDecisionSchema {
    pub required_actions: Vec<NativeReviewDecisionAction>,
    pub required_decision_fields: Vec<String>,
    pub context_binding_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewItem {
    pub review_id: String,
    pub mode: NativeReviewMode,
    pub mode_context: NativeReviewModeContext,
    pub decision_binding_hash: String,
    pub recommended_action: NativeReviewDecisionAction,
    pub allowed_actions: Vec<NativeReviewDecisionAction>,
    pub observations: Vec<NativeReviewObservation>,
    pub candidate_clusters: Vec<NativeCandidateCluster>,
    pub candidate_links: Vec<NativeCandidateLink>,
    pub evidence_waterfall_refs: Vec<NativeEvidenceWaterfallRef>,
    pub conflicts: Vec<NativeReviewConflict>,
    pub related_distinct_cues: Vec<NativeRelatedDistinctCue>,
    pub impact: NativeReviewImpact,
    pub provenance: Vec<NativeReviewProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReviewMode {
    Cluster,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeReviewModeContext {
    Cluster {
        cluster_id: String,
        surface_ids: Vec<String>,
    },
    Link {
        left_surface_id: String,
        right_surface_id: Option<String>,
        relation_hints: Vec<NativeCandidateLink>,
    },
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
pub struct NativeReviewObservation {
    pub observation_id: String,
    pub surface_id: String,
    pub row_id: String,
    pub source: String,
    pub raw_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCandidateCluster {
    pub cluster_id: String,
    pub surface_ids: Vec<String>,
    pub proposed_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCandidateLink {
    pub link_id: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub relation: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEvidenceWaterfallRef {
    pub evidence_ref_id: String,
    pub lane: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub score_units: u32,
    pub evidence_count: u64,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewConflict {
    pub conflict_id: String,
    pub reason_code: String,
    pub surface_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRelatedDistinctCue {
    pub cue_id: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewImpact {
    pub affected_rows: u64,
    pub affected_deals: u64,
    pub review_priority_units: u32,
    pub priority_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewProvenance {
    pub surface_id: String,
    pub row_id: String,
    pub source: String,
    pub raw_value: String,
}

pub fn build_native_review_artifact(
    request: NativeReviewExportRequest,
) -> Result<NativeReviewArtifact, Refusal> {
    require_hash(
        "run_content_hash",
        &request.run_content_hash,
        "Native review export requires a run hash binding",
    )?;
    require_hash(
        "policy_content_hash",
        &request.policy_content_hash,
        "Native review export requires a policy hash binding",
    )?;
    require_hash(
        "source_review_queue_hash",
        &request.review_queue.artifact_content_hash,
        "Native review export requires a hashed review queue",
    )?;

    let binding = NativeReviewBinding {
        source_review_queue_hash: request.review_queue.artifact_content_hash.clone(),
        run_content_hash: request.run_content_hash,
        policy_content_hash: request.policy_content_hash,
        registry_snapshot_hash: request
            .review_queue
            .metadata
            .registry_snapshot
            .lookup_snapshot_hash
            .clone(),
        registry_id: request.review_queue.metadata.registry_snapshot.id.clone(),
        registry_version: request
            .review_queue
            .metadata
            .registry_snapshot
            .version
            .clone(),
        profile_id: request.review_queue.metadata.profile.id.clone(),
        profile_version: request.review_queue.metadata.profile.version.clone(),
        entity_type: request.review_queue.metadata.profile.entity_type.clone(),
        identity_semantics: request
            .review_queue
            .metadata
            .profile
            .identity_semantics
            .clone(),
        strategy_hash: request.review_queue.metadata.strategy.content_hash.clone(),
    };

    let mut metadata = request.review_queue.metadata.clone();
    metadata.artifact_content_hash.clear();
    let force_link_mode = request.review_queue.source_link_hash.is_some();
    let mut review_items = request
        .review_queue
        .review_items
        .iter()
        .map(|item| native_review_item(item, &binding, force_link_mode))
        .collect::<Result<Vec<_>, _>>()?;
    review_items.sort_by(|left, right| left.review_id.cmp(&right.review_id));

    let summary = native_review_summary(&review_items);
    let mut artifact = NativeReviewArtifact {
        version: CANON_ENTITY_NATIVE_REVIEW_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        binding,
        decision_schema: native_decision_schema(),
        review_items,
    };
    artifact.artifact_content_hash = hash_native_review_artifact(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn render_native_review_json(artifact: &NativeReviewArtifact) -> Result<String, Refusal> {
    serde_json::to_string_pretty(artifact).map_err(json_refusal)
}

pub fn render_native_review_csv(artifact: &NativeReviewArtifact) -> Result<String, Refusal> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record([
            "review_id",
            "mode",
            "recommended_action",
            "allowed_actions_json",
            "decision_binding_hash",
            "source_review_artifact_hash",
            "run_content_hash",
            "policy_content_hash",
            "registry_snapshot_hash",
            "surface_ids_json",
            "mode_context_json",
            "observations_json",
            "candidate_clusters_json",
            "candidate_links_json",
            "evidence_waterfall_refs_json",
            "conflicts_json",
            "related_distinct_cues_json",
            "impact_json",
            "provenance_json",
        ])
        .map_err(csv_refusal)?;

    for item in &artifact.review_items {
        writer
            .write_record([
                item.review_id.clone(),
                mode_string(item.mode),
                item.recommended_action.as_str().to_string(),
                serde_json::to_string(&item.allowed_actions).map_err(json_refusal)?,
                item.decision_binding_hash.clone(),
                artifact.artifact_content_hash.clone(),
                artifact.binding.run_content_hash.clone(),
                artifact.binding.policy_content_hash.clone(),
                artifact.binding.registry_snapshot_hash.clone(),
                serde_json::to_string(&surface_ids(item)).map_err(json_refusal)?,
                serde_json::to_string(&item.mode_context).map_err(json_refusal)?,
                serde_json::to_string(&item.observations).map_err(json_refusal)?,
                serde_json::to_string(&item.candidate_clusters).map_err(json_refusal)?,
                serde_json::to_string(&item.candidate_links).map_err(json_refusal)?,
                serde_json::to_string(&item.evidence_waterfall_refs).map_err(json_refusal)?,
                serde_json::to_string(&item.conflicts).map_err(json_refusal)?,
                serde_json::to_string(&item.related_distinct_cues).map_err(json_refusal)?,
                serde_json::to_string(&item.impact).map_err(json_refusal)?,
                serde_json::to_string(&item.provenance).map_err(json_refusal)?,
            ])
            .map_err(csv_refusal)?;
    }

    let bytes = writer.into_inner().map_err(|error| {
        native_review_refusal(
            EntityRefusalKind::ReviewImport,
            "Failed to finalize native review CSV",
            json!({
                "stage": "native_review_export",
                "error": error.to_string()
            }),
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        native_review_refusal(
            EntityRefusalKind::ReviewImport,
            "Native review CSV was not UTF-8",
            json!({
                "stage": "native_review_export",
                "error": error.to_string()
            }),
        )
    })
}

pub fn render_native_review_html(artifact: &NativeReviewArtifact) -> Result<String, Refusal> {
    let json = serde_json::to_string(artifact)
        .map_err(json_refusal)?
        .replace("</", "<\\/");
    Ok(include_str!("../../assets/entity_review.html")
        .replace("__CANON_NATIVE_REVIEW_JSON__", &json)
        .replace(
            "__CANON_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION__",
            CANON_ENTITY_NATIVE_REVIEW_DECISION_ENVELOPE_VERSION,
        ))
}

pub fn native_review_artifact_hash(artifact: &NativeReviewArtifact) -> Result<String, Refusal> {
    hash_native_review_artifact(artifact)
}

fn native_review_item(
    item: &ReviewQueueItem,
    binding: &NativeReviewBinding,
    force_link_mode: bool,
) -> Result<NativeReviewItem, Refusal> {
    let mode = item_mode(item, force_link_mode);
    let candidate_links = native_candidate_links(item);
    let mode_context = match mode {
        NativeReviewMode::Cluster => NativeReviewModeContext::Cluster {
            cluster_id: format!("cluster:{}", stable_suffix(&item.review_id)),
            surface_ids: sorted_unique(item.surface_ids.clone()),
        },
        NativeReviewMode::Link => link_mode_context(item, &candidate_links, force_link_mode)?,
    };
    let has_candidate = mode != NativeReviewMode::Link || !candidate_links.is_empty();
    let recommended_action = recommended_action(item, mode, has_candidate);
    let allowed_actions = allowed_actions(mode, has_candidate);
    let observations = native_observations(&item.provenance_samples);
    let candidate_clusters = native_candidate_clusters(item, mode);
    let evidence_waterfall_refs = native_evidence_waterfall_refs(item);
    let conflicts = native_conflicts(item);
    let related_distinct_cues = native_related_distinct_cues(item);
    let impact = NativeReviewImpact {
        affected_rows: item.affected_rows,
        affected_deals: item.affected_deals,
        review_priority_units: item.review_priority_units,
        priority_reasons: item.priority_reasons.clone(),
    };
    let provenance = item
        .provenance_samples
        .iter()
        .map(|sample| NativeReviewProvenance {
            surface_id: sample.surface_id.clone(),
            row_id: sample.row_id.clone(),
            source: sample.source.clone(),
            raw_value: sample.raw_value.clone(),
        })
        .collect();

    let mut native = NativeReviewItem {
        review_id: item.review_id.clone(),
        mode,
        mode_context,
        decision_binding_hash: String::new(),
        recommended_action,
        allowed_actions,
        observations,
        candidate_clusters,
        candidate_links,
        evidence_waterfall_refs,
        conflicts,
        related_distinct_cues,
        impact,
        provenance,
    };
    native.decision_binding_hash = hash_decision_binding(&native, binding)?;
    Ok(native)
}

fn item_mode(item: &ReviewQueueItem, force_link_mode: bool) -> NativeReviewMode {
    if force_link_mode || !item.relation_hints.is_empty() {
        NativeReviewMode::Link
    } else {
        NativeReviewMode::Cluster
    }
}

fn link_mode_context(
    item: &ReviewQueueItem,
    candidate_links: &[NativeCandidateLink],
    force_link_mode: bool,
) -> Result<NativeReviewModeContext, Refusal> {
    if force_link_mode {
        let left_surface_id = source_link_target_surface(item)?;
        let right_surface_id = candidate_links
            .first()
            .map(|link| source_link_candidate_surface(&left_surface_id, link));
        return Ok(NativeReviewModeContext::Link {
            left_surface_id,
            right_surface_id,
            relation_hints: candidate_links.to_vec(),
        });
    }

    let link = candidate_links.first().ok_or_else(|| {
        native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Native link review item requires a candidate link",
            json!({
                "stage": "native_review_export",
                "review_id": item.review_id
            }),
        )
    })?;
    Ok(NativeReviewModeContext::Link {
        left_surface_id: link.left_surface_id.clone(),
        right_surface_id: Some(link.right_surface_id.clone()),
        relation_hints: candidate_links.to_vec(),
    })
}

fn source_link_target_surface(item: &ReviewQueueItem) -> Result<String, Refusal> {
    item.surface_ids
        .iter()
        .find(|surface_id| !surface_id.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native source-link review item requires a target surface",
                json!({
                    "stage": "native_review_export",
                    "review_id": item.review_id
                }),
            )
        })
}

fn source_link_candidate_surface(target_surface_id: &str, link: &NativeCandidateLink) -> String {
    if link.left_surface_id == target_surface_id {
        link.right_surface_id.clone()
    } else if link.right_surface_id == target_surface_id {
        link.left_surface_id.clone()
    } else {
        link.right_surface_id.clone()
    }
}

fn recommended_action(
    item: &ReviewQueueItem,
    mode: NativeReviewMode,
    has_candidate: bool,
) -> NativeReviewDecisionAction {
    if mode == NativeReviewMode::Link && !has_candidate {
        NativeReviewDecisionAction::Defer
    } else if item.strongest_negative_cut.is_some() {
        NativeReviewDecisionAction::CannotLink
    } else if mode == NativeReviewMode::Link {
        NativeReviewDecisionAction::Relation
    } else {
        NativeReviewDecisionAction::Alias
    }
}

fn allowed_actions(mode: NativeReviewMode, has_candidate: bool) -> Vec<NativeReviewDecisionAction> {
    match mode {
        NativeReviewMode::Cluster => vec![
            NativeReviewDecisionAction::Alias,
            NativeReviewDecisionAction::CannotLink,
            NativeReviewDecisionAction::Assignment,
            NativeReviewDecisionAction::Defer,
        ],
        NativeReviewMode::Link if has_candidate => vec![
            NativeReviewDecisionAction::Relation,
            NativeReviewDecisionAction::CannotLink,
            NativeReviewDecisionAction::Defer,
        ],
        NativeReviewMode::Link => vec![NativeReviewDecisionAction::Defer],
    }
}

fn native_observations(samples: &[ReviewProvenanceSample]) -> Vec<NativeReviewObservation> {
    let mut observations = samples
        .iter()
        .map(|sample| NativeReviewObservation {
            observation_id: format!(
                "observation:{}:{}",
                stable_suffix(&sample.surface_id),
                stable_suffix(&sample.row_id)
            ),
            surface_id: sample.surface_id.clone(),
            row_id: sample.row_id.clone(),
            source: sample.source.clone(),
            raw_value: sample.raw_value.clone(),
        })
        .collect::<Vec<_>>();
    observations.sort_by(|left, right| {
        left.surface_id
            .cmp(&right.surface_id)
            .then_with(|| left.row_id.cmp(&right.row_id))
            .then_with(|| left.source.cmp(&right.source))
    });
    observations
}

fn native_candidate_clusters(
    item: &ReviewQueueItem,
    mode: NativeReviewMode,
) -> Vec<NativeCandidateCluster> {
    if mode != NativeReviewMode::Cluster {
        return Vec::new();
    }
    vec![NativeCandidateCluster {
        cluster_id: format!("cluster:{}", stable_suffix(&item.review_id)),
        surface_ids: sorted_unique(item.surface_ids.clone()),
        proposed_action: item.proposed_action.clone(),
    }]
}

fn native_candidate_links(item: &ReviewQueueItem) -> Vec<NativeCandidateLink> {
    let mut links = item
        .relation_hints
        .iter()
        .map(|hint| NativeCandidateLink {
            link_id: format!(
                "link:{}:{}:{}",
                stable_suffix(&hint.left_surface_id),
                stable_suffix(&hint.right_surface_id),
                stable_suffix(&hint.relation)
            ),
            left_surface_id: hint.left_surface_id.clone(),
            right_surface_id: hint.right_surface_id.clone(),
            relation: hint.relation.clone(),
            reason_code: hint.reason_code.clone(),
        })
        .collect::<Vec<_>>();
    links.sort_by(|left, right| {
        left.left_surface_id
            .cmp(&right.left_surface_id)
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
            .then_with(|| left.relation.cmp(&right.relation))
            .then_with(|| left.reason_code.cmp(&right.reason_code))
    });
    links
}

fn native_evidence_waterfall_refs(item: &ReviewQueueItem) -> Vec<NativeEvidenceWaterfallRef> {
    let mut refs = Vec::new();
    if let Some(cut) = &item.strongest_positive_cut {
        refs.push(evidence_ref("support", cut));
    }
    if let Some(cut) = &item.strongest_negative_cut {
        refs.push(evidence_ref("anti_merge", cut));
    }
    refs.sort_by(|left, right| left.evidence_ref_id.cmp(&right.evidence_ref_id));
    refs
}

fn evidence_ref(lane: &str, cut: &SolveEvidenceCut) -> NativeEvidenceWaterfallRef {
    NativeEvidenceWaterfallRef {
        evidence_ref_id: format!(
            "evidence:{}:{}:{}",
            lane,
            stable_suffix(&cut.left_surface_id),
            stable_suffix(&cut.right_surface_id)
        ),
        lane: lane.to_string(),
        left_surface_id: cut.left_surface_id.clone(),
        right_surface_id: cut.right_surface_id.clone(),
        score_units: cut.score_units.as_u32(),
        evidence_count: cut.evidence_count,
        reason_codes: cut.evidence_reason_codes.clone(),
    }
}

fn native_conflicts(item: &ReviewQueueItem) -> Vec<NativeReviewConflict> {
    let mut conflicts = Vec::new();
    if item.strongest_positive_cut.is_some() && item.strongest_negative_cut.is_some() {
        conflicts.push(NativeReviewConflict {
            conflict_id: format!("conflict:{}", stable_suffix(&item.review_id)),
            reason_code: "support_and_cannot_link".to_string(),
            surface_ids: sorted_unique(item.surface_ids.clone()),
        });
    }
    for reason in &item.priority_reasons {
        if reason.contains("conflict") || reason.contains("cannot_link") {
            conflicts.push(NativeReviewConflict {
                conflict_id: format!("conflict:{}:{}", stable_suffix(&item.review_id), reason),
                reason_code: reason.clone(),
                surface_ids: sorted_unique(item.surface_ids.clone()),
            });
        }
    }
    conflicts.sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));
    conflicts.dedup_by(|left, right| left.conflict_id == right.conflict_id);
    conflicts
}

fn native_related_distinct_cues(item: &ReviewQueueItem) -> Vec<NativeRelatedDistinctCue> {
    let mut cues = item
        .relation_hints
        .iter()
        .map(|hint| NativeRelatedDistinctCue {
            cue_id: format!(
                "cue:{}:{}:{}",
                stable_suffix(&hint.left_surface_id),
                stable_suffix(&hint.right_surface_id),
                stable_suffix(&hint.reason_code)
            ),
            left_surface_id: hint.left_surface_id.clone(),
            right_surface_id: hint.right_surface_id.clone(),
            reason_code: hint.reason_code.clone(),
        })
        .collect::<Vec<_>>();
    if let Some(cut) = &item.strongest_negative_cut {
        cues.push(NativeRelatedDistinctCue {
            cue_id: format!(
                "cue:{}:{}:negative",
                stable_suffix(&cut.left_surface_id),
                stable_suffix(&cut.right_surface_id)
            ),
            left_surface_id: cut.left_surface_id.clone(),
            right_surface_id: cut.right_surface_id.clone(),
            reason_code: cut
                .evidence_reason_codes
                .first()
                .cloned()
                .unwrap_or_else(|| "negative_identity_evidence".to_string()),
        });
    }
    cues.sort_by(|left, right| left.cue_id.cmp(&right.cue_id));
    cues.dedup_by(|left, right| left.cue_id == right.cue_id);
    cues
}

fn native_review_summary(review_items: &[NativeReviewItem]) -> EntityDeterministicSummary {
    let cluster_count = review_items
        .iter()
        .filter(|item| item.mode == NativeReviewMode::Cluster)
        .count() as u64;
    let link_count = review_items
        .iter()
        .filter(|item| item.mode == NativeReviewMode::Link)
        .count() as u64;
    EntityDeterministicSummary {
        counts: BTreeMap::from([
            ("review_items".to_string(), review_items.len() as u64),
            ("candidate_clusters".to_string(), cluster_count),
            ("candidate_links".to_string(), link_count),
            (
                "review_rows_covered".to_string(),
                review_items
                    .iter()
                    .map(|item| item.impact.affected_rows)
                    .sum(),
            ),
            (
                "review_deals_covered".to_string(),
                review_items
                    .iter()
                    .map(|item| item.impact.affected_deals)
                    .sum(),
            ),
        ]),
        labels: BTreeMap::from([
            ("stage".to_string(), "native_review".to_string()),
            ("projection".to_string(), "offline_static_html".to_string()),
        ]),
    }
}

fn native_decision_schema() -> NativeReviewDecisionSchema {
    NativeReviewDecisionSchema {
        required_actions: vec![
            NativeReviewDecisionAction::Alias,
            NativeReviewDecisionAction::CannotLink,
            NativeReviewDecisionAction::Relation,
            NativeReviewDecisionAction::Assignment,
            NativeReviewDecisionAction::Defer,
        ],
        required_decision_fields: vec![
            "review_id".to_string(),
            "mode".to_string(),
            "action".to_string(),
            "operator_id".to_string(),
            "reason_code".to_string(),
            "source_review_artifact_hash".to_string(),
            "decision_binding_hash".to_string(),
            "run_content_hash".to_string(),
            "policy_content_hash".to_string(),
            "registry_snapshot_hash".to_string(),
            "mode_context".to_string(),
        ],
        context_binding_fields: vec![
            "source_review_queue_hash".to_string(),
            "run_content_hash".to_string(),
            "policy_content_hash".to_string(),
            "registry_snapshot_hash".to_string(),
            "profile_id".to_string(),
            "strategy_hash".to_string(),
        ],
    }
}

fn hash_decision_binding(
    item: &NativeReviewItem,
    binding: &NativeReviewBinding,
) -> Result<String, Refusal> {
    let mut hashable = item.clone();
    hashable.decision_binding_hash.clear();
    let value = json!({
        "binding": binding,
        "item": hashable
    });
    Ok(format!(
        "blake3:{}",
        blake3::hash(canonical_json(&value).as_bytes()).to_hex()
    ))
}

fn hash_native_review_artifact(artifact: &NativeReviewArtifact) -> Result<String, Refusal> {
    let mut value = serde_json::to_value(artifact).map_err(json_refusal)?;
    clear_native_self_hash_fields(&mut value)?;
    Ok(format!(
        "blake3:{}",
        blake3::hash(canonical_json(&value).as_bytes()).to_hex()
    ))
}

fn clear_native_self_hash_fields(value: &mut Value) -> Result<(), Refusal> {
    let object = value.as_object_mut().ok_or_else(|| {
        native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Native review artifact must be a JSON object",
            json!({
                "stage": "native_review_export",
                "field": "$"
            }),
        )
    })?;
    object.insert(
        "artifact_content_hash".to_string(),
        Value::String(String::new()),
    );
    if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.insert(
            "artifact_content_hash".to_string(),
            Value::String(String::new()),
        );
    }
    Ok(())
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => serde_json::to_string(text).expect("string serializes"),
        Value::Array(array) => {
            let mut rendered = String::from("[");
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&canonical_json(item));
            }
            rendered.push(']');
            rendered
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut rendered = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&serde_json::to_string(key).expect("key serializes"));
                rendered.push(':');
                rendered.push_str(&canonical_json(&object[*key]));
            }
            rendered.push('}');
            rendered
        }
    }
}

fn surface_ids(item: &NativeReviewItem) -> Vec<String> {
    match &item.mode_context {
        NativeReviewModeContext::Cluster { surface_ids, .. } => surface_ids.clone(),
        NativeReviewModeContext::Link {
            left_surface_id,
            right_surface_id,
            ..
        } => {
            let mut ids = vec![left_surface_id.clone()];
            if let Some(right_surface_id) = right_surface_id {
                ids.push(right_surface_id.clone());
            }
            sorted_unique(ids)
        }
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn stable_suffix(value: &str) -> String {
    value.replace([':', '/', ' ', '.'], "_")
}

fn mode_string(mode: NativeReviewMode) -> String {
    match mode {
        NativeReviewMode::Cluster => "cluster",
        NativeReviewMode::Link => "link",
    }
    .to_string()
}

fn require_hash(field: &str, value: &str, message: &'static str) -> Result<(), Refusal> {
    if value.starts_with("blake3:") && value.len() > "blake3:".len() {
        Ok(())
    } else {
        Err(native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            message,
            json!({
                "stage": "native_review_export",
                "field": field,
                "actual": value
            }),
        ))
    }
}

fn json_refusal(error: serde_json::Error) -> Refusal {
    native_review_refusal(
        EntityRefusalKind::ReviewImport,
        "Failed to serialize native review field",
        json!({
            "stage": "native_review_export",
            "error": error.to_string()
        }),
    )
}

fn csv_refusal(error: csv::Error) -> Refusal {
    native_review_refusal(
        EntityRefusalKind::ReviewImport,
        "Failed to write native review CSV",
        json!({
            "stage": "native_review_export",
            "error": error.to_string()
        }),
    )
}

fn native_review_refusal(
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
