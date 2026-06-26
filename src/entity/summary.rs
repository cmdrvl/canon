#![forbid(unsafe_code)]

//! Stable operator summaries for entity workbench artifacts.

use crate::entity::{
    EntityDeterministicSummary,
    apply::ApplyRunArtifact,
    block_artifact::BlockCandidateArtifact,
    edge_artifact::EdgeEvidenceArtifact,
    index::EntityIndexArtifact,
    prepare::PrepareRunArtifact,
    run::{EntityRunArtifact, render_run_summary},
    solve::SolveArtifact,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CANON_ENTITY_OPERATOR_SUMMARY_VERSION: &str = "canon_entity_operator_summary.v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRunOperatorSummaryRequest<'a> {
    pub artifact: &'a EntityRunArtifact,
    pub extra_counts: BTreeMap<String, u64>,
    pub cache_status: BTreeMap<String, String>,
    pub top_unresolved_tokens: Vec<EntitySummaryRankedItem>,
    pub top_anti_merge_reasons: Vec<EntitySummaryRankedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRunOperatorSummary {
    pub version: String,
    pub profile_id: String,
    pub registry: EntitySummaryRegistry,
    pub counts: BTreeMap<String, u64>,
    pub labels: BTreeMap<String, String>,
    pub cache_status: BTreeMap<String, String>,
    pub top_unresolved_tokens: Vec<EntitySummaryRankedItem>,
    pub top_anti_merge_reasons: Vec<EntitySummaryRankedItem>,
    pub next_commands: BTreeMap<String, String>,
    pub human_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySummaryRegistry {
    pub id: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityStageOperatorSummaryRequest {
    pub stage: String,
    pub artifact_version: String,
    pub counts: BTreeMap<String, u64>,
    pub labels: BTreeMap<String, String>,
    pub cache_status: BTreeMap<String, String>,
    pub top_unresolved_tokens: Vec<EntitySummaryRankedItem>,
    pub top_anti_merge_reasons: Vec<EntitySummaryRankedItem>,
    pub next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntitySummaryRankedItem {
    pub key: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityStageOperatorSummary {
    pub version: String,
    pub stage: String,
    pub artifact_version: String,
    pub counts: BTreeMap<String, u64>,
    pub labels: BTreeMap<String, String>,
    pub cache_status: BTreeMap<String, String>,
    pub top_unresolved_tokens: Vec<EntitySummaryRankedItem>,
    pub top_anti_merge_reasons: Vec<EntitySummaryRankedItem>,
    pub next_commands: BTreeMap<String, String>,
    pub human_summary: String,
}

impl EntitySummaryRankedItem {
    pub fn new(key: impl Into<String>, count: u64) -> Self {
        Self {
            key: key.into(),
            count,
        }
    }
}

pub fn build_prepare_operator_summary(artifact: &PrepareRunArtifact) -> EntityStageOperatorSummary {
    build_stage_operator_summary(EntityStageOperatorSummaryRequest {
        stage: "prepare".to_string(),
        artifact_version: artifact.version.clone(),
        counts: artifact.summary.clone(),
        labels: BTreeMap::from([
            ("profile_id".to_string(), artifact.profile.id.clone()),
            (
                "profile_version".to_string(),
                artifact.profile.version.clone(),
            ),
            (
                "registry_id".to_string(),
                artifact.registry_snapshot.id.clone(),
            ),
            (
                "registry_version".to_string(),
                artifact.registry_snapshot.version.clone(),
            ),
            ("surfaces_path".to_string(), artifact.surfaces_path.clone()),
        ]),
        cache_status: BTreeMap::new(),
        top_unresolved_tokens: Vec::new(),
        top_anti_merge_reasons: Vec::new(),
        next_commands: BTreeMap::new(),
    })
}

pub fn build_index_operator_summary(artifact: &EntityIndexArtifact) -> EntityStageOperatorSummary {
    let cache_status = artifact
        .summary
        .labels
        .get("cache_status")
        .map(|status| BTreeMap::from([("index".to_string(), status.clone())]))
        .unwrap_or_default();
    build_deterministic_stage_operator_summary(
        "index",
        &artifact.version,
        &artifact.summary,
        cache_status,
        BTreeMap::new(),
    )
}

pub fn build_block_operator_summary(
    artifact: &BlockCandidateArtifact,
) -> EntityStageOperatorSummary {
    build_deterministic_stage_operator_summary(
        "block",
        &artifact.version,
        &artifact.summary,
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

pub fn build_edge_operator_summary(artifact: &EdgeEvidenceArtifact) -> EntityStageOperatorSummary {
    build_deterministic_stage_operator_summary(
        "edge",
        &artifact.version,
        &artifact.summary,
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

pub fn build_solve_operator_summary(artifact: &SolveArtifact) -> EntityStageOperatorSummary {
    build_deterministic_stage_operator_summary(
        "solve",
        &artifact.version,
        &artifact.summary,
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

pub fn build_apply_operator_summary(artifact: &ApplyRunArtifact) -> EntityStageOperatorSummary {
    build_stage_operator_summary(EntityStageOperatorSummaryRequest {
        stage: "apply".to_string(),
        artifact_version: artifact.version.clone(),
        counts: artifact.summary.clone(),
        labels: BTreeMap::from([
            ("registry_id".to_string(), artifact.registry.id.clone()),
            (
                "registry_version".to_string(),
                artifact.registry.version.clone(),
            ),
            ("output_path".to_string(), artifact.output_path.clone()),
        ]),
        cache_status: BTreeMap::new(),
        top_unresolved_tokens: Vec::new(),
        top_anti_merge_reasons: Vec::new(),
        next_commands: BTreeMap::new(),
    })
}

pub fn build_run_operator_summary(
    request: EntityRunOperatorSummaryRequest<'_>,
) -> EntityRunOperatorSummary {
    let mut counts = request.extra_counts;
    counts.extend(request.artifact.summary.counts.clone());
    let labels = request.artifact.summary.labels.clone();
    let next_commands = BTreeMap::from([
        (
            "resume".to_string(),
            request.artifact.next_commands.resume.clone(),
        ),
        (
            "review_export".to_string(),
            request.artifact.next_commands.review_export.clone(),
        ),
        (
            "audit".to_string(),
            request.artifact.next_commands.audit.clone(),
        ),
        (
            "promote".to_string(),
            request.artifact.next_commands.promote.clone(),
        ),
        (
            "apply".to_string(),
            request.artifact.next_commands.apply.clone(),
        ),
    ]);
    let registry = EntitySummaryRegistry {
        id: labels.get("registry_id").cloned().unwrap_or_default(),
        version: labels.get("registry_version").cloned().unwrap_or_default(),
        source: labels.get("registry_source").cloned().unwrap_or_default(),
    };
    let mut summary = EntityRunOperatorSummary {
        version: CANON_ENTITY_OPERATOR_SUMMARY_VERSION.to_string(),
        profile_id: labels.get("profile_id").cloned().unwrap_or_default(),
        registry,
        counts,
        labels,
        cache_status: request.cache_status,
        top_unresolved_tokens: sorted_ranked_items(request.top_unresolved_tokens),
        top_anti_merge_reasons: sorted_ranked_items(request.top_anti_merge_reasons),
        next_commands,
        human_summary: String::new(),
    };
    summary.human_summary = render_operator_summary(&summary, request.artifact);
    summary
}

pub fn build_deterministic_stage_operator_summary(
    stage: &str,
    artifact_version: &str,
    summary: &EntityDeterministicSummary,
    cache_status: BTreeMap<String, String>,
    next_commands: BTreeMap<String, String>,
) -> EntityStageOperatorSummary {
    build_stage_operator_summary(EntityStageOperatorSummaryRequest {
        stage: stage.to_string(),
        artifact_version: artifact_version.to_string(),
        counts: summary.counts.clone(),
        labels: summary.labels.clone(),
        cache_status,
        top_unresolved_tokens: Vec::new(),
        top_anti_merge_reasons: Vec::new(),
        next_commands,
    })
}

pub fn build_stage_operator_summary(
    request: EntityStageOperatorSummaryRequest,
) -> EntityStageOperatorSummary {
    let mut summary = EntityStageOperatorSummary {
        version: CANON_ENTITY_OPERATOR_SUMMARY_VERSION.to_string(),
        stage: request.stage,
        artifact_version: request.artifact_version,
        counts: request.counts,
        labels: request.labels,
        cache_status: request.cache_status,
        top_unresolved_tokens: sorted_ranked_items(request.top_unresolved_tokens),
        top_anti_merge_reasons: sorted_ranked_items(request.top_anti_merge_reasons),
        next_commands: request.next_commands,
        human_summary: String::new(),
    };
    summary.human_summary = render_stage_operator_summary(&summary);
    summary
}

pub fn render_operator_summary(
    summary: &EntityRunOperatorSummary,
    artifact: &EntityRunArtifact,
) -> String {
    let next = summary
        .next_commands
        .keys()
        .filter(|key| key.as_str() != "resume")
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    let cache = render_pairs(&summary.cache_status);
    format!(
        "{} deals={} raw_unique={} promotable={} review_groups={} anti_merge_groups={} cache={} top_unresolved={} top_anti_merge={} next=[{}]",
        render_run_summary(artifact),
        count_any(&summary.counts, &["deals", "deal_count"]),
        count_any(
            &summary.counts,
            &["raw_unique_names", "raw_unique_surfaces"]
        ),
        count_any(&summary.counts, &["promotable_aliases", "promotable_new"]),
        count_any(
            &summary.counts,
            &[
                "review_groups",
                "review_group_count",
                "operator_review_groups"
            ]
        ),
        count_any(&summary.counts, &["anti_merge_groups"]),
        cache,
        render_ranked(&summary.top_unresolved_tokens),
        render_ranked(&summary.top_anti_merge_reasons),
        next
    )
}

pub fn render_stage_operator_summary(summary: &EntityStageOperatorSummary) -> String {
    let profile = summary.labels.get("profile_id").map_or("", String::as_str);
    let registry = summary.labels.get("registry_id").map_or("", String::as_str);
    let next = summary
        .next_commands
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} stage={} artifact={} profile={} registry={} rows={} prepared={} exact_resolved={} promotable={} review_groups={} anti_merge_groups={} cache={} top_unresolved={} top_anti_merge={} next=[{}]",
        summary.version,
        summary.stage,
        summary.artifact_version,
        profile,
        registry,
        count_any(&summary.counts, &["rows", "row_count"]),
        count_any(&summary.counts, &["prepared_surfaces"]),
        count_any(&summary.counts, &["exact_resolved_surfaces"]),
        count_any(&summary.counts, &["promotable_aliases", "promotable_new"]),
        count_any(
            &summary.counts,
            &[
                "review_groups",
                "review_group_count",
                "operator_review_groups"
            ]
        ),
        count_any(&summary.counts, &["anti_merge_groups"]),
        render_pairs(&summary.cache_status),
        render_ranked(&summary.top_unresolved_tokens),
        render_ranked(&summary.top_anti_merge_reasons),
        next
    )
}

fn sorted_ranked_items(mut items: Vec<EntitySummaryRankedItem>) -> Vec<EntitySummaryRankedItem> {
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.key.cmp(&right.key))
    });
    items
}

fn count_any(counts: &BTreeMap<String, u64>, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| counts.get(*key).copied())
        .unwrap_or_default()
}

fn render_pairs(values: &BTreeMap<String, String>) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_ranked(items: &[EntitySummaryRankedItem]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    items
        .iter()
        .map(|item| format!("{}:{}", item.key, item.count))
        .collect::<Vec<_>>()
        .join(",")
}
