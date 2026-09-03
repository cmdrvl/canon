#![forbid(unsafe_code)]

//! Native offline review artifact export for cluster and link decisions.

use crate::{
    Refusal,
    entity::{
        EntityArtifactMetadata, EntityDeterministicSummary,
        error::EntityRefusalKind,
        review::{ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem},
        score::{ENTITY_SCORE_SCALE, ScoreLane},
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_groups: Vec<NativeReviewEvidenceSignatureGroup>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_group_decision_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_signature_fields: Vec<String>,
    pub context_binding_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewItem {
    pub review_id: String,
    pub mode: NativeReviewMode,
    pub mode_context: NativeReviewModeContext,
    #[serde(
        default,
        skip_serializing_if = "NativeReviewEvidenceSignature::is_empty"
    )]
    pub evidence_signature: NativeReviewEvidenceSignature,
    pub decision_binding_hash: String,
    pub recommended_action: NativeReviewDecisionAction,
    pub allowed_actions: Vec<NativeReviewDecisionAction>,
    pub observations: Vec<NativeReviewObservation>,
    pub candidate_clusters: Vec<NativeCandidateCluster>,
    pub candidate_links: Vec<NativeCandidateLink>,
    #[serde(default)]
    pub evidence_waterfall: NativeEvidenceWaterfall,
    pub evidence_waterfall_refs: Vec<NativeEvidenceWaterfallRef>,
    pub conflicts: Vec<NativeReviewConflict>,
    pub related_distinct_cues: Vec<NativeRelatedDistinctCue>,
    pub impact: NativeReviewImpact,
    pub provenance: Vec<NativeReviewProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeReviewEvidenceSignature {
    pub signature_id: String,
    pub mode: Option<NativeReviewMode>,
    pub recommended_action: Option<NativeReviewDecisionAction>,
    pub allowed_actions: Vec<NativeReviewDecisionAction>,
    pub score_band: String,
    pub contradiction_class: String,
    pub mode_context_class: String,
    pub hit_vector: Vec<NativeReviewEvidenceSignatureHit>,
}

impl NativeReviewEvidenceSignature {
    fn is_empty(&self) -> bool {
        self.signature_id.is_empty()
            && self.mode.is_none()
            && self.recommended_action.is_none()
            && self.allowed_actions.is_empty()
            && self.score_band.is_empty()
            && self.contradiction_class.is_empty()
            && self.mode_context_class.is_empty()
            && self.hit_vector.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewEvidenceSignatureHit {
    pub lane: String,
    pub operator: String,
    pub view_field: String,
    pub score_band: String,
    pub evidence_count: u64,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewEvidenceSignatureGroup {
    pub signature_id: String,
    pub signature: NativeReviewEvidenceSignature,
    pub member_count: u64,
    pub sample_review_ids: Vec<String>,
    pub score_stats: NativeReviewEvidenceSignatureScoreStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReviewEvidenceSignatureScoreStats {
    pub min_review_priority_units: u32,
    pub max_review_priority_units: u32,
    pub total_review_priority_units: u64,
    pub min_evidence_score_units: u32,
    pub max_evidence_score_units: u32,
    pub total_evidence_score_units: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeEvidenceWaterfall {
    pub score_total_units: u32,
    pub raw_support_score_units: u64,
    pub threshold_lines: Vec<NativeEvidenceWaterfallThresholdLine>,
    pub contributions: Vec<NativeEvidenceWaterfallContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEvidenceWaterfallThresholdLine {
    pub threshold_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_units: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_units: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEvidenceWaterfallContribution {
    pub evidence_ref_id: String,
    pub lane: String,
    pub operator: String,
    pub view_field: String,
    pub left_surface_id: String,
    pub right_surface_id: String,
    pub evidence_count: u64,
    pub reason_codes: Vec<String>,
    pub source_score_units: u32,
    pub score_units: u32,
    pub running_total_units: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_frequency: Option<NativeEvidenceWaterfallValueFrequency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEvidenceWaterfallValueFrequency {
    pub table_hash: String,
    pub view_field: String,
    pub count: u64,
    pub band: String,
    pub floor_applied: bool,
    pub multiplier_basis_points: u32,
    pub original_score_units: u32,
    pub adjusted_score_units: u32,
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

    let review_groups = build_native_review_signature_groups(&review_items);
    let summary = native_review_summary(&review_items, &review_groups);
    let mut artifact = NativeReviewArtifact {
        version: CANON_ENTITY_NATIVE_REVIEW_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        summary,
        binding,
        decision_schema: native_decision_schema(),
        review_groups,
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
            "evidence_signature_id",
            "evidence_signature_json",
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
            "evidence_waterfall_json",
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
                item.evidence_signature.signature_id.clone(),
                serde_json::to_string(&item.evidence_signature).map_err(json_refusal)?,
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
                serde_json::to_string(&item.evidence_waterfall).map_err(json_refusal)?,
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
    let evidence_waterfall = native_evidence_waterfall(item);
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
        evidence_signature: NativeReviewEvidenceSignature::default(),
        decision_binding_hash: String::new(),
        recommended_action,
        allowed_actions,
        observations,
        candidate_clusters,
        candidate_links,
        evidence_waterfall,
        evidence_waterfall_refs,
        conflicts,
        related_distinct_cues,
        impact,
        provenance,
    };
    native.evidence_signature = native_evidence_signature_for_item(&native)?;
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

fn native_evidence_waterfall(item: &ReviewQueueItem) -> NativeEvidenceWaterfall {
    let mut sources = Vec::new();
    if let Some(cut) = &item.strongest_positive_cut {
        sources.extend(waterfall_sources_from_cut("support", cut));
    }
    if let Some(cut) = &item.strongest_negative_cut {
        sources.extend(waterfall_sources_from_cut("anti_merge", cut));
    }
    sources.sort_by(waterfall_source_cmp);

    let mut remaining_support_units = u64::from(ENTITY_SCORE_SCALE);
    let mut contributions = Vec::new();
    for source in sources {
        let score_units = if source.lane == "support" {
            let score_units = u64::from(source.source_score_units).min(remaining_support_units);
            remaining_support_units -= score_units;
            score_units as u32
        } else {
            0
        };
        contributions.push(NativeEvidenceWaterfallContribution {
            evidence_ref_id: source.evidence_ref_id,
            lane: source.lane,
            operator: source.operator,
            view_field: source.view_field,
            left_surface_id: source.left_surface_id,
            right_surface_id: source.right_surface_id,
            evidence_count: source.evidence_count,
            reason_codes: source.reason_codes,
            source_score_units: source.source_score_units,
            score_units,
            running_total_units: 0,
            value_frequency: source.value_frequency,
        });
    }
    contributions.sort_by(native_waterfall_contribution_cmp);

    let mut running_total_units = 0u64;
    let mut raw_support_score_units = 0u64;
    for contribution in &mut contributions {
        if contribution.lane == "support" {
            raw_support_score_units += u64::from(contribution.source_score_units);
        }
        running_total_units += u64::from(contribution.score_units);
        contribution.running_total_units = running_total_units as u32;
    }
    let score_total_units = running_total_units as u32;

    NativeEvidenceWaterfall {
        score_total_units,
        raw_support_score_units,
        threshold_lines: unavailable_threshold_lines(score_total_units),
        contributions,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeEvidenceWaterfallSource {
    evidence_ref_id: String,
    lane: String,
    operator: String,
    view_field: String,
    left_surface_id: String,
    right_surface_id: String,
    evidence_count: u64,
    reason_codes: Vec<String>,
    source_score_units: u32,
    value_frequency: Option<NativeEvidenceWaterfallValueFrequency>,
}

fn waterfall_sources_from_cut(
    lane: &str,
    cut: &SolveEvidenceCut,
) -> Vec<NativeEvidenceWaterfallSource> {
    let evidence_ref_id = format!(
        "evidence:{}:{}:{}",
        lane,
        stable_suffix(&cut.left_surface_id),
        stable_suffix(&cut.right_surface_id)
    );
    if !cut.evidence_hits.is_empty() {
        let mut sources = cut
            .evidence_hits
            .iter()
            .map(|hit| NativeEvidenceWaterfallSource {
                evidence_ref_id: evidence_ref_id.clone(),
                lane: score_lane_string(hit.lane),
                operator: normalized_signature_part(&hit.operator_id, "unattributed", "operator"),
                view_field: normalized_signature_part(&hit.namespace, "unspecified", "view_field"),
                left_surface_id: cut.left_surface_id.clone(),
                right_surface_id: cut.right_surface_id.clone(),
                evidence_count: 1,
                reason_codes: vec![hit.reason_code.clone()],
                source_score_units: hit.score_units.as_u32(),
                value_frequency: value_frequency_from_explanation(&hit.explanation),
            })
            .collect::<Vec<_>>();
        sources.sort_by(waterfall_source_cmp);
        return sources;
    }

    let reason_codes = sorted_unique(cut.evidence_reason_codes.clone());
    let (operator, view_field) = fallback_operator_view_field(&reason_codes);
    vec![NativeEvidenceWaterfallSource {
        evidence_ref_id,
        lane: lane.to_string(),
        operator,
        view_field,
        left_surface_id: cut.left_surface_id.clone(),
        right_surface_id: cut.right_surface_id.clone(),
        evidence_count: cut.evidence_count,
        reason_codes,
        source_score_units: cut.score_units.as_u32(),
        value_frequency: None,
    }]
}

fn fallback_operator_view_field(reason_codes: &[String]) -> (String, String) {
    match reason_codes {
        [reason_code] => signature_operator_view_field(reason_code),
        [] => ("unattributed".to_string(), "unspecified".to_string()),
        _ => ("operator:mixed".to_string(), "view_field:mixed".to_string()),
    }
}

fn value_frequency_from_explanation(
    explanation: &str,
) -> Option<NativeEvidenceWaterfallValueFrequency> {
    if !explanation.contains("value_frequency") {
        return None;
    }
    let params = explanation
        .split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect::<BTreeMap<_, _>>();
    Some(NativeEvidenceWaterfallValueFrequency {
        table_hash: params.get("table_hash")?.to_string(),
        view_field: params.get("view")?.to_string(),
        count: params.get("count")?.parse().ok()?,
        band: params.get("band")?.to_string(),
        floor_applied: params.get("floor_applied")?.parse().ok()?,
        multiplier_basis_points: params.get("multiplier_basis_points")?.parse().ok()?,
        original_score_units: params.get("original_score_units")?.parse().ok()?,
        adjusted_score_units: params.get("adjusted_score_units")?.parse().ok()?,
    })
}

fn unavailable_threshold_lines(
    score_total_units: u32,
) -> Vec<NativeEvidenceWaterfallThresholdLine> {
    ["backbone_score_min", "attach_score_min", "abstain_margin"]
        .into_iter()
        .map(|threshold_id| NativeEvidenceWaterfallThresholdLine {
            threshold_id: threshold_id.to_string(),
            score_units: None,
            delta_units: None,
            source: format!("not_bound_in_review_queue_v0; pair_score_total={score_total_units}"),
        })
        .collect()
}

fn score_lane_string(lane: ScoreLane) -> String {
    match lane {
        ScoreLane::Support => "support",
        ScoreLane::AntiMerge => "anti_merge",
        ScoreLane::RelationHint => "relation_hint",
    }
    .to_string()
}

fn waterfall_source_cmp(
    left: &NativeEvidenceWaterfallSource,
    right: &NativeEvidenceWaterfallSource,
) -> std::cmp::Ordering {
    right
        .source_score_units
        .cmp(&left.source_score_units)
        .then_with(|| left.operator.cmp(&right.operator))
        .then_with(|| left.view_field.cmp(&right.view_field))
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.reason_codes.cmp(&right.reason_codes))
        .then_with(|| left.evidence_ref_id.cmp(&right.evidence_ref_id))
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

fn native_waterfall_contribution_cmp(
    left: &NativeEvidenceWaterfallContribution,
    right: &NativeEvidenceWaterfallContribution,
) -> std::cmp::Ordering {
    right
        .score_units
        .cmp(&left.score_units)
        .then_with(|| left.operator.cmp(&right.operator))
        .then_with(|| left.view_field.cmp(&right.view_field))
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.reason_codes.cmp(&right.reason_codes))
        .then_with(|| left.evidence_ref_id.cmp(&right.evidence_ref_id))
        .then_with(|| left.left_surface_id.cmp(&right.left_surface_id))
        .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
}

pub fn validate_native_review_waterfalls(artifact: &NativeReviewArtifact) -> Result<(), Refusal> {
    for item in &artifact.review_items {
        validate_native_review_item_waterfall(item)?;
    }
    Ok(())
}

fn validate_native_review_item_waterfall(item: &NativeReviewItem) -> Result<(), Refusal> {
    let expected_score_total_units = support_ref_score_total(&item.evidence_waterfall_refs);
    if item.evidence_waterfall.score_total_units != expected_score_total_units {
        return Err(native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Native review evidence waterfall score does not match evidence refs",
            json!({
                "stage": "native_review_export",
                "field": "review_items.evidence_waterfall.score_total_units",
                "review_id": item.review_id,
                "expected": expected_score_total_units,
                "actual": item.evidence_waterfall.score_total_units
            }),
        ));
    }

    let refs_by_id = item
        .evidence_waterfall_refs
        .iter()
        .map(|reference| (reference.evidence_ref_id.as_str(), reference))
        .collect::<BTreeMap<_, _>>();
    let mut running_total_units = 0u64;
    let mut raw_support_score_units = 0u64;
    let mut previous: Option<&NativeEvidenceWaterfallContribution> = None;
    for contribution in &item.evidence_waterfall.contributions {
        validate_waterfall_contribution_shape(item, contribution, &refs_by_id)?;
        if let Some(previous) = previous
            && native_waterfall_contribution_cmp(previous, contribution).is_gt()
        {
            return Err(native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall contributions are not deterministically ordered",
                json!({
                    "stage": "native_review_export",
                    "field": "review_items.evidence_waterfall.contributions",
                    "review_id": item.review_id,
                    "previous": previous,
                    "actual": contribution
                }),
            ));
        }
        if contribution.lane == "support" {
            raw_support_score_units += u64::from(contribution.source_score_units);
        } else if contribution.score_units != 0 {
            return Err(native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall non-support evidence cannot add to pair score",
                json!({
                    "stage": "native_review_export",
                    "field": "review_items.evidence_waterfall.contributions.score_units",
                    "review_id": item.review_id,
                    "evidence_ref_id": contribution.evidence_ref_id,
                    "lane": contribution.lane,
                    "actual": contribution.score_units
                }),
            ));
        }
        if contribution.score_units > contribution.source_score_units {
            return Err(native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall contribution exceeds its source score",
                json!({
                    "stage": "native_review_export",
                    "field": "review_items.evidence_waterfall.contributions.score_units",
                    "review_id": item.review_id,
                    "evidence_ref_id": contribution.evidence_ref_id,
                    "source_score_units": contribution.source_score_units,
                    "actual": contribution.score_units
                }),
            ));
        }
        running_total_units += u64::from(contribution.score_units);
        if running_total_units > u64::from(ENTITY_SCORE_SCALE)
            || contribution.running_total_units != running_total_units as u32
        {
            return Err(native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall running total is inconsistent",
                json!({
                    "stage": "native_review_export",
                    "field": "review_items.evidence_waterfall.contributions.running_total_units",
                    "review_id": item.review_id,
                    "evidence_ref_id": contribution.evidence_ref_id,
                    "expected": running_total_units,
                    "actual": contribution.running_total_units
                }),
            ));
        }
        previous = Some(contribution);
    }

    if running_total_units != u64::from(item.evidence_waterfall.score_total_units) {
        return Err(native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Native review evidence waterfall contributions do not sum to the pair score",
            json!({
                "stage": "native_review_export",
                "field": "review_items.evidence_waterfall.contributions",
                "review_id": item.review_id,
                "expected": item.evidence_waterfall.score_total_units,
                "actual": running_total_units
            }),
        ));
    }
    if raw_support_score_units != item.evidence_waterfall.raw_support_score_units {
        return Err(native_review_refusal(
            EntityRefusalKind::ArtifactContract,
            "Native review evidence waterfall raw support total is inconsistent",
            json!({
                "stage": "native_review_export",
                "field": "review_items.evidence_waterfall.raw_support_score_units",
                "review_id": item.review_id,
                "expected": raw_support_score_units,
                "actual": item.evidence_waterfall.raw_support_score_units
            }),
        ));
    }
    validate_waterfall_threshold_lines(item)
}

fn validate_waterfall_contribution_shape(
    item: &NativeReviewItem,
    contribution: &NativeEvidenceWaterfallContribution,
    refs_by_id: &BTreeMap<&str, &NativeEvidenceWaterfallRef>,
) -> Result<(), Refusal> {
    let reference = refs_by_id
        .get(contribution.evidence_ref_id.as_str())
        .ok_or_else(|| {
            native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall contribution references an unknown evidence ref",
                json!({
                    "stage": "native_review_export",
                    "field": "review_items.evidence_waterfall.contributions.evidence_ref_id",
                    "review_id": item.review_id,
                    "actual": contribution.evidence_ref_id
                }),
            )
        })?;
    for (field, expected, actual) in [
        ("lane", reference.lane.as_str(), contribution.lane.as_str()),
        (
            "left_surface_id",
            reference.left_surface_id.as_str(),
            contribution.left_surface_id.as_str(),
        ),
        (
            "right_surface_id",
            reference.right_surface_id.as_str(),
            contribution.right_surface_id.as_str(),
        ),
    ] {
        if expected != actual {
            return Err(native_review_refusal(
                EntityRefusalKind::ArtifactContract,
                "Native review evidence waterfall contribution does not match its evidence ref",
                json!({
                    "stage": "native_review_export",
                    "field": format!("review_items.evidence_waterfall.contributions.{field}"),
                    "review_id": item.review_id,
                    "evidence_ref_id": contribution.evidence_ref_id,
                    "expected": expected,
                    "actual": actual
                }),
            ));
        }
    }
    Ok(())
}

fn validate_waterfall_threshold_lines(item: &NativeReviewItem) -> Result<(), Refusal> {
    for threshold in &item.evidence_waterfall.threshold_lines {
        if let Some(score_units) = threshold.score_units {
            let expected_delta =
                i64::from(score_units) - i64::from(item.evidence_waterfall.score_total_units);
            if threshold.delta_units != Some(expected_delta) {
                return Err(native_review_refusal(
                    EntityRefusalKind::ArtifactContract,
                    "Native review evidence waterfall threshold delta is inconsistent",
                    json!({
                        "stage": "native_review_export",
                        "field": "review_items.evidence_waterfall.threshold_lines.delta_units",
                        "review_id": item.review_id,
                        "threshold_id": threshold.threshold_id,
                        "expected": expected_delta,
                        "actual": threshold.delta_units
                    }),
                ));
            }
        }
    }
    Ok(())
}

fn support_ref_score_total(refs: &[NativeEvidenceWaterfallRef]) -> u32 {
    let support_units = refs
        .iter()
        .filter(|reference| reference.lane == "support")
        .map(|reference| u64::from(reference.score_units))
        .sum::<u64>();
    support_units.min(u64::from(ENTITY_SCORE_SCALE)) as u32
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

pub fn build_native_review_signature_groups(
    review_items: &[NativeReviewItem],
) -> Vec<NativeReviewEvidenceSignatureGroup> {
    const SAMPLE_LIMIT: usize = 5;

    let mut members_by_signature = BTreeMap::<String, Vec<&NativeReviewItem>>::new();
    for item in review_items {
        if !item.evidence_signature.signature_id.is_empty() {
            members_by_signature
                .entry(item.evidence_signature.signature_id.clone())
                .or_default()
                .push(item);
        }
    }

    members_by_signature
        .into_iter()
        .map(|(signature_id, mut members)| {
            members.sort_by(|left, right| left.review_id.cmp(&right.review_id));
            let sample_review_ids = members
                .iter()
                .take(SAMPLE_LIMIT)
                .map(|item| item.review_id.clone())
                .collect::<Vec<_>>();
            let score_stats = native_signature_group_score_stats(&members);
            NativeReviewEvidenceSignatureGroup {
                signature_id,
                signature: members[0].evidence_signature.clone(),
                member_count: members.len() as u64,
                sample_review_ids,
                score_stats,
            }
        })
        .collect()
}

pub fn native_evidence_signature_for_item(
    item: &NativeReviewItem,
) -> Result<NativeReviewEvidenceSignature, Refusal> {
    let mut allowed_actions = item.allowed_actions.clone();
    allowed_actions.sort();
    allowed_actions.dedup();
    let mut signature = NativeReviewEvidenceSignature {
        signature_id: String::new(),
        mode: Some(item.mode),
        recommended_action: Some(item.recommended_action),
        allowed_actions,
        score_band: score_band(item_signature_score_units(item)),
        contradiction_class: native_contradiction_class(item),
        mode_context_class: native_mode_context_class(item),
        hit_vector: native_signature_hit_vector(item),
    };
    let value = serde_json::to_value(&signature).map_err(json_refusal)?;
    signature.signature_id = format!(
        "signature:blake3:{}",
        blake3::hash(canonical_json(&value).as_bytes()).to_hex()
    );
    Ok(signature)
}

fn native_review_summary(
    review_items: &[NativeReviewItem],
    review_groups: &[NativeReviewEvidenceSignatureGroup],
) -> EntityDeterministicSummary {
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
            ("review_group_count".to_string(), review_groups.len() as u64),
            (
                "evidence_signature_groups".to_string(),
                review_groups.len() as u64,
            ),
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
            ("grouping".to_string(), "evidence_signature".to_string()),
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
        required_group_decision_fields: vec![
            "evidence_signature_id".to_string(),
            "action".to_string(),
            "operator_id".to_string(),
            "reason_code".to_string(),
            "source_review_artifact_hash".to_string(),
            "run_content_hash".to_string(),
            "policy_content_hash".to_string(),
            "registry_snapshot_hash".to_string(),
        ],
        evidence_signature_fields: vec![
            "signature_id".to_string(),
            "mode".to_string(),
            "recommended_action".to_string(),
            "allowed_actions".to_string(),
            "score_band".to_string(),
            "contradiction_class".to_string(),
            "mode_context_class".to_string(),
            "hit_vector".to_string(),
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

fn native_signature_group_score_stats(
    members: &[&NativeReviewItem],
) -> NativeReviewEvidenceSignatureScoreStats {
    let review_scores = members
        .iter()
        .map(|item| item.impact.review_priority_units)
        .collect::<Vec<_>>();
    let evidence_scores = members
        .iter()
        .map(|item| item_signature_score_units(item))
        .collect::<Vec<_>>();
    NativeReviewEvidenceSignatureScoreStats {
        min_review_priority_units: review_scores.iter().copied().min().unwrap_or(0),
        max_review_priority_units: review_scores.iter().copied().max().unwrap_or(0),
        total_review_priority_units: review_scores.iter().map(|score| u64::from(*score)).sum(),
        min_evidence_score_units: evidence_scores.iter().copied().min().unwrap_or(0),
        max_evidence_score_units: evidence_scores.iter().copied().max().unwrap_or(0),
        total_evidence_score_units: evidence_scores.iter().map(|score| u64::from(*score)).sum(),
    }
}

fn item_signature_score_units(item: &NativeReviewItem) -> u32 {
    if item.evidence_waterfall.contributions.is_empty() {
        return item.evidence_waterfall.score_total_units;
    }
    item.evidence_waterfall.score_total_units
}

fn native_contradiction_class(item: &NativeReviewItem) -> String {
    if item
        .conflicts
        .iter()
        .any(|conflict| conflict.reason_code == "support_and_cannot_link")
    {
        "support_and_cannot_link".to_string()
    } else if !item.conflicts.is_empty() {
        "conflict".to_string()
    } else if item
        .evidence_waterfall_refs
        .iter()
        .any(|evidence| evidence.lane == "anti_merge")
    {
        "cannot_link_evidence".to_string()
    } else {
        "none".to_string()
    }
}

fn native_mode_context_class(item: &NativeReviewItem) -> String {
    match &item.mode_context {
        NativeReviewModeContext::Cluster { surface_ids, .. } if surface_ids.len() <= 1 => {
            "cluster_singleton".to_string()
        }
        NativeReviewModeContext::Cluster { .. } => "cluster_multi_surface".to_string(),
        NativeReviewModeContext::Link {
            right_surface_id, ..
        } if right_surface_id.is_some() => "link_candidate_backed".to_string(),
        NativeReviewModeContext::Link { .. } => "link_candidate_free".to_string(),
    }
}

fn native_signature_hit_vector(item: &NativeReviewItem) -> Vec<NativeReviewEvidenceSignatureHit> {
    let mut hits = Vec::new();
    for contribution in &item.evidence_waterfall.contributions {
        let reason_codes = sorted_unique(contribution.reason_codes.clone());
        if reason_codes.is_empty() {
            hits.push(NativeReviewEvidenceSignatureHit {
                lane: contribution.lane.clone(),
                operator: contribution.operator.clone(),
                view_field: contribution.view_field.clone(),
                score_band: score_band(contribution.score_units),
                evidence_count: contribution.evidence_count,
                reason_codes: Vec::new(),
            });
            continue;
        }
        for reason_code in reason_codes {
            hits.push(NativeReviewEvidenceSignatureHit {
                lane: contribution.lane.clone(),
                operator: contribution.operator.clone(),
                view_field: contribution.view_field.clone(),
                score_band: score_band(contribution.score_units),
                evidence_count: contribution.evidence_count,
                reason_codes: vec![reason_code],
            });
        }
    }
    hits.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then_with(|| left.operator.cmp(&right.operator))
            .then_with(|| left.view_field.cmp(&right.view_field))
            .then_with(|| left.score_band.cmp(&right.score_band))
            .then_with(|| left.reason_codes.cmp(&right.reason_codes))
    });
    hits
}

fn signature_operator_view_field(reason_code: &str) -> (String, String) {
    if let Some((operator, view_field)) = reason_code.split_once(':') {
        (
            normalized_signature_part(operator, "unattributed", "operator"),
            normalized_signature_part(view_field, "unspecified", "view_field"),
        )
    } else if let Some((operator, view_field)) = reason_code.split_once('.') {
        (
            normalized_signature_part(operator, "unattributed", "operator"),
            normalized_signature_part(view_field, "unspecified", "view_field"),
        )
    } else if let Some((operator, view_field)) = reason_code.split_once('/') {
        (
            normalized_signature_part(operator, "unattributed", "operator"),
            normalized_signature_part(view_field, "unspecified", "view_field"),
        )
    } else {
        (
            "unattributed".to_string(),
            normalized_signature_part(reason_code, "unspecified", "view_field"),
        )
    }
}

fn normalized_signature_part(value: &str, empty_value: &str, kind: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return empty_value.to_string();
    }
    format!("{kind}:{trimmed}")
}

fn score_band(score_units: u32) -> String {
    let floor = (score_units / 1_000) * 1_000;
    let upper = floor.saturating_add(999).min(10_000);
    format!("{floor:05}-{upper:05}")
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
