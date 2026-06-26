#![forbid(unsafe_code)]

//! Stable operator summaries for entity workbench artifacts.

use crate::entity::{
    EntityDeterministicSummary,
    apply::ApplyRunArtifact,
    block_artifact::BlockCandidateArtifact,
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
    build_stage_operator_summary(
        "prepare",
        &artifact.version,
        artifact.summary.clone(),
        BTreeMap::from([
            ("profile_id".to_string(), artifact.profile.id.clone()),
            (
                "registry_id".to_string(),
                artifact.registry_snapshot.id.clone(),
            ),
            (
                "registry_version".to_string(),
                artifact.registry_snapshot.version.clone(),
            ),
            (
                "surfaces_path".to_string(),
                artifact.surfaces_path.clone(),
            ),
        ]),
    )
}

pub fn build_block_operator_summary(
    artifact: &BlockCandidateArtifact,
) -> EntityStageOperatorSummary {
    build_deterministic_stage_operator_summary("block", &artifact.version, &artifact.summary)
}

pub fn build_solve_operator_summary(artifact: &SolveArtifact) -> EntityStageOperatorSummary {
    build_deterministic_stage_operator_summary("solve", &artifact.version, &artifact.summary)
}

pub fn build_apply_operator_summary(artifact: &ApplyRunArtifact) -> EntityStageOperatorSummary {
    build_stage_operator_summary(
        "apply",
        &artifact.version,
        artifact.summary.clone(),
        BTreeMap::from([
            ("registry_id".to_string(), artifact.registry.id.clone()),
            ("registry_version".to_string(), artifact.registry.version.clone()),
            ("output_path".to_string(), artifact.output_path.clone()),
        ]),
    )
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
) -> EntityStageOperatorSummary {
    build_stage_operator_summary(
        stage,
        artifact_version,
        summary.counts.clone(),
        summary.labels.clone(),
    )
}

pub fn build_stage_operator_summary(
    stage: &str,
    artifact_version: &str,
    counts: BTreeMap<String, u64>,
    labels: BTreeMap<String, String>,
) -> EntityStageOperatorSummary {
    let mut summary = EntityStageOperatorSummary {
        version: CANON_ENTITY_OPERATOR_SUMMARY_VERSION.to_string(),
        stage: stage.to_string(),
        artifact_version: artifact_version.to_string(),
        counts,
        labels,
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
    let cache = summary
        .cache_status
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} deals={} raw_unique={} review_groups={} anti_merge_groups={} top_unresolved={} top_anti_merge={} cache={} next=[{}]",
        render_run_summary(artifact),
        count(&summary.counts, "deal_count"),
        count(&summary.counts, "raw_unique_names"),
        count(&summary.counts, "operator_review_groups"),
        count(&summary.counts, "anti_merge_groups"),
        render_ranked_keys(&summary.top_unresolved_tokens),
        render_ranked_keys(&summary.top_anti_merge_reasons),
        cache,
        next
    )
}

pub fn render_stage_operator_summary(summary: &EntityStageOperatorSummary) -> String {
    let counts = summary
        .counts
        .iter()
        .take(8)
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let labels = summary
        .labels
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "profile_id" | "registry_id" | "registry_version" | "cache_status"
            )
        })
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{} stage={} artifact={} {} {}",
        summary.version, summary.stage, summary.artifact_version, labels, counts
    )
    .trim()
    .to_string()
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

fn count(counts: &BTreeMap<String, u64>, key: &str) -> u64 {
    counts.get(key).copied().unwrap_or_default()
}

fn render_ranked_keys(items: &[EntitySummaryRankedItem]) -> String {
    if items.is_empty() {
        return "-".to_string();
    }
    items
        .iter()
        .map(|item| item.key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
