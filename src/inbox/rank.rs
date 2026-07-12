#![forbid(unsafe_code)]

//! Deterministic priority ranking for unresolved inbox work.
//!
//! Ranking is queue triage only. It emits expected coverage value and
//! explanations; it never selects a canonical identity or rewrites inbox events.

use crate::inbox::{
    CandidateStatus, InboxError, InboxErrorCode, InboxFieldRole, InboxResult, PrivacyClass,
    UnresolvedInboxArtifact, UnresolvedInboxItem, finalize_artifact,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

pub const CANON_INBOX_PRIORITY_POLICY_VERSION: &str = "canon.inbox.priority_policy.v1";
pub const CANON_INBOX_PRIORITY_RANKING_VERSION: &str = "canon.inbox.priority_ranking.v1";
pub const RANKING_IDENTITY_STATUS: &str = "rank_only_no_identity_assertion";

const COMPONENTS: [&str; 12] = [
    "recurrence",
    "distinct_sources",
    "distinct_subjects",
    "distinct_projects",
    "role_criticality",
    "exposure_band",
    "downstream_consumers",
    "candidate_readiness",
    "ambiguity_cost",
    "review_effort",
    "age",
    "drift",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub revision: String,
    pub as_of: String,
    #[serde(default)]
    pub component_weights: BTreeMap<String, i64>,
    #[serde(default)]
    pub caps: BTreeMap<String, u64>,
    #[serde(default)]
    pub role_criticality: BTreeMap<String, i64>,
    #[serde(default)]
    pub exposure_bands: Vec<ExposureBand>,
    #[serde(default)]
    pub candidate_readiness: BTreeMap<String, i64>,
    #[serde(default)]
    pub event_signals: BTreeMap<String, PrioritySignalOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposureBand {
    pub band: String,
    pub min_occurrences: u64,
    pub score_units: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PrioritySignalOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_partition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distinct_subjects: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downstream_consumers: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_effort_units: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_events: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PriorityRankingSummary {
    pub total_items: u64,
    pub uncertain_items: u64,
    pub capped_outlier_items: u64,
    pub highest_expected_coverage_value_units: i64,
    #[serde(default)]
    pub by_partition: BTreeMap<String, u64>,
    #[serde(default)]
    pub by_uncertainty_flag: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxPriorityRankingArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub identity_status: String,
    pub source_inbox_artifact_hash: String,
    pub policy: PriorityPolicy,
    pub policy_content_hash: String,
    pub summary: PriorityRankingSummary,
    #[serde(default)]
    pub ranked_items: Vec<RankedInboxItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankedInboxItem {
    pub rank: u64,
    pub event_key: String,
    pub identity_status: String,
    pub expected_coverage_value_units: i64,
    pub queue_partition: QueuePartition,
    pub components: Vec<PriorityComponentScore>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncertainty_flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capped_outliers: Vec<CappedOutlier>,
    pub sensitivity: Vec<PrioritySensitivity>,
    pub tie_breaker: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuePartition {
    pub profile: String,
    pub registry: String,
    pub role: String,
    pub source: String,
    pub privacy_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityComponentScore {
    pub component: String,
    pub raw_value: Option<u64>,
    pub effective_value: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<u64>,
    pub weight: i64,
    pub contribution_units: i64,
    pub missing_signal: bool,
    pub capped: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CappedOutlier {
    pub component: String,
    pub raw_value: u64,
    pub cap: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrioritySensitivity {
    pub component: String,
    pub contribution_units: i64,
    pub score_without_component: i64,
    pub rank_without_component: u64,
}

#[derive(Debug, Clone)]
struct ScoredItem {
    item: RankedInboxItem,
    first_seen_at: String,
    field_name: String,
}

impl PriorityPolicy {
    pub fn baseline(
        policy_id: impl Into<String>,
        revision: impl Into<String>,
        as_of: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: CANON_INBOX_PRIORITY_POLICY_VERSION.to_string(),
            policy_id: policy_id.into(),
            revision: revision.into(),
            as_of: as_of.into(),
            component_weights: BTreeMap::from([
                ("recurrence".to_string(), 10),
                ("distinct_sources".to_string(), 12),
                ("distinct_subjects".to_string(), 14),
                ("distinct_projects".to_string(), 16),
                ("role_criticality".to_string(), 10),
                ("exposure_band".to_string(), 8),
                ("downstream_consumers".to_string(), 18),
                ("candidate_readiness".to_string(), 8),
                ("ambiguity_cost".to_string(), 6),
                ("review_effort".to_string(), -4),
                ("age".to_string(), 2),
                ("drift".to_string(), 3),
            ]),
            caps: BTreeMap::from([
                ("recurrence".to_string(), 100),
                ("distinct_sources".to_string(), 25),
                ("distinct_subjects".to_string(), 25),
                ("distinct_projects".to_string(), 25),
                ("downstream_consumers".to_string(), 50),
                ("ambiguity_cost".to_string(), 40),
                ("review_effort".to_string(), 50),
                ("age".to_string(), 180),
                ("drift".to_string(), 180),
            ]),
            role_criticality: BTreeMap::from([
                ("lookup_input".to_string(), 4),
                ("name_field".to_string(), 10),
                ("anchor_field".to_string(), 14),
                ("context_field".to_string(), 2),
                ("candidate_pair".to_string(), 8),
            ]),
            exposure_bands: vec![
                ExposureBand {
                    band: "low".to_string(),
                    min_occurrences: 1,
                    score_units: 1,
                },
                ExposureBand {
                    band: "medium".to_string(),
                    min_occurrences: 5,
                    score_units: 5,
                },
                ExposureBand {
                    band: "high".to_string(),
                    min_occurrences: 20,
                    score_units: 12,
                },
                ExposureBand {
                    band: "critical".to_string(),
                    min_occurrences: 50,
                    score_units: 20,
                },
            ],
            candidate_readiness: BTreeMap::from([
                ("none".to_string(), 0),
                ("ambiguous".to_string(), 8),
                ("rejected".to_string(), 4),
                ("budget_limited".to_string(), 2),
            ]),
            event_signals: BTreeMap::new(),
        }
    }
}

pub fn rank_inbox(
    inbox: &UnresolvedInboxArtifact,
    policy: PriorityPolicy,
) -> InboxResult<InboxPriorityRankingArtifact> {
    let inbox = finalize_artifact(inbox.clone())?;
    let policy = normalize_policy(policy)?;
    let as_of = parse_timestamp(&policy.as_of, "policy.as_of")?;
    let policy_content_hash = hash_serialized(&policy, "priority policy")?;

    let mut scored = inbox
        .items
        .iter()
        .map(|item| score_item(item, &policy, as_of))
        .collect::<InboxResult<Vec<_>>>()?;
    apply_ranks(&mut scored, None);

    let rank_maps = sensitivity_rank_maps(&scored);
    for scored_item in &mut scored {
        let components = scored_item.item.components.clone();
        scored_item.item.sensitivity = components
            .iter()
            .map(|component| PrioritySensitivity {
                component: component.component.clone(),
                contribution_units: component.contribution_units,
                score_without_component: scored_item.item.expected_coverage_value_units
                    - component.contribution_units,
                rank_without_component: rank_maps
                    .get(&component.component)
                    .and_then(|ranks| ranks.get(&scored_item.item.event_key))
                    .copied()
                    .unwrap_or(scored_item.item.rank),
            })
            .collect();
    }

    let ranked_items = scored.into_iter().map(|item| item.item).collect::<Vec<_>>();
    let mut artifact = InboxPriorityRankingArtifact {
        version: CANON_INBOX_PRIORITY_RANKING_VERSION.to_string(),
        artifact_content_hash: String::new(),
        identity_status: RANKING_IDENTITY_STATUS.to_string(),
        source_inbox_artifact_hash: inbox.artifact_content_hash,
        policy,
        policy_content_hash,
        summary: PriorityRankingSummary::default(),
        ranked_items,
    };
    artifact.summary = build_summary(&artifact.ranked_items);
    artifact.artifact_content_hash = hash_without_self(&artifact)?;
    Ok(artifact)
}

pub fn canonical_priority_json_bytes(
    artifact: &InboxPriorityRankingArtifact,
) -> InboxResult<Vec<u8>> {
    serde_json::to_vec(artifact).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize inbox priority ranking artifact: {error}"
        ))
    })
}

fn score_item(
    item: &UnresolvedInboxItem,
    policy: &PriorityPolicy,
    as_of: DateTime<Utc>,
) -> InboxResult<ScoredItem> {
    let signal = policy.event_signals.get(&item.event_key);
    let mut components = Vec::new();
    let mut uncertainty_flags = BTreeSet::new();
    let mut capped_outliers = Vec::new();

    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "recurrence",
        Some(item.occurrence_summary.total_occurrences),
        policy,
        "more repeated unresolved occurrences indicate broader missing coverage",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "distinct_sources",
        Some(item.occurrence_summary.distinct_sources),
        policy,
        "more independent sources indicate broader missing coverage",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "distinct_subjects",
        signal.and_then(|signal| signal.distinct_subjects),
        policy,
        "distinct domain subjects require supplied usage context",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "distinct_projects",
        Some(item.occurrence_summary.distinct_projects),
        policy,
        "more projects indicate wider downstream reuse",
    );
    push_mapped_component(
        &mut components,
        "role_criticality",
        role_key(item.field_role),
        policy
            .role_criticality
            .get(role_key(item.field_role))
            .copied(),
        weight(policy, "role_criticality"),
        "field role criticality is policy-defined and domain neutral",
    );
    push_exposure_component(&mut components, item, signal, policy)?;
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "downstream_consumers",
        signal.and_then(|signal| signal.downstream_consumers),
        policy,
        "downstream consumer count requires supplied usage context",
    );
    push_mapped_component(
        &mut components,
        "candidate_readiness",
        candidate_status_key(item.candidate_summary.status),
        policy
            .candidate_readiness
            .get(candidate_status_key(item.candidate_summary.status))
            .copied(),
        weight(policy, "candidate_readiness"),
        "candidate readiness raises review priority without making a decision",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "ambiguity_cost",
        Some(ambiguity_units(item)),
        policy,
        "ambiguous or rejected candidates may justify review even without auto-deciding",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "review_effort",
        Some(review_effort_units(item, signal)),
        policy,
        "review effort is a policy-weighted cost component, not an identity decision",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "age",
        Some(age_days(&item.first_seen_at, as_of)?),
        policy,
        "older unresolved work compounds across runs",
    );
    push_capped_component(
        &mut components,
        &mut capped_outliers,
        "drift",
        Some(drift_units(item, signal, as_of)?),
        policy,
        "items that continue changing or recurring over time are more urgent",
    );

    for component in &components {
        if component.missing_signal {
            uncertainty_flags.insert(format!("missing_{}", component.component));
        }
        if component.capped {
            uncertainty_flags.insert(format!("capped_{}", component.component));
        }
    }
    if item.profile_ref.is_none() {
        uncertainty_flags.insert("missing_profile".to_string());
    }
    if item.privacy_class.is_none() {
        uncertainty_flags.insert("missing_privacy_class".to_string());
    }

    let expected_coverage_value_units = components
        .iter()
        .map(|component| component.contribution_units)
        .sum::<i64>();
    let partition = queue_partition(item, signal);
    let tie_breaker = vec![
        item.first_seen_at.clone(),
        item.event_key.clone(),
        item.field_name.clone(),
    ];

    Ok(ScoredItem {
        item: RankedInboxItem {
            rank: 0,
            event_key: item.event_key.clone(),
            identity_status: RANKING_IDENTITY_STATUS.to_string(),
            expected_coverage_value_units,
            queue_partition: partition,
            components,
            uncertainty_flags: uncertainty_flags.into_iter().collect(),
            capped_outliers,
            sensitivity: Vec::new(),
            tie_breaker,
        },
        first_seen_at: item.first_seen_at.clone(),
        field_name: item.field_name.clone(),
    })
}

fn normalize_policy(mut policy: PriorityPolicy) -> InboxResult<PriorityPolicy> {
    policy.schema_version = policy.schema_version.trim().to_string();
    policy.policy_id = policy.policy_id.trim().to_string();
    policy.revision = policy.revision.trim().to_string();
    policy.as_of = canonical_timestamp(&policy.as_of, "policy.as_of")?;

    if policy.schema_version != CANON_INBOX_PRIORITY_POLICY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported priority policy schema_version {}",
            policy.schema_version
        )));
    }
    if policy.policy_id.is_empty() || policy.revision.is_empty() {
        return Err(artifact_contract_error(
            "priority policy requires non-empty policy_id and revision",
        ));
    }

    for component in COMPONENTS {
        policy
            .component_weights
            .entry(component.to_string())
            .or_insert(0);
    }
    for component in [
        "recurrence",
        "distinct_sources",
        "distinct_subjects",
        "distinct_projects",
        "downstream_consumers",
        "ambiguity_cost",
        "review_effort",
        "age",
        "drift",
    ] {
        policy.caps.entry(component.to_string()).or_insert(u64::MAX);
    }

    for band in &mut policy.exposure_bands {
        band.band = band.band.trim().to_string();
        if band.band.is_empty() {
            return Err(artifact_contract_error(
                "priority policy exposure bands require non-empty band names",
            ));
        }
    }
    policy.exposure_bands.sort_by(|left, right| {
        left.min_occurrences
            .cmp(&right.min_occurrences)
            .then_with(|| left.band.cmp(&right.band))
    });
    policy.exposure_bands.dedup_by(|left, right| {
        left.band == right.band
            && left.min_occurrences == right.min_occurrences
            && left.score_units == right.score_units
    });

    let mut normalized_signals = BTreeMap::new();
    for (event_key, mut signal) in policy.event_signals {
        let event_key = normalized_event_key(&event_key)?;
        signal.registry_id = clean_optional(signal.registry_id);
        signal.source_partition = clean_optional(signal.source_partition);
        signal.exposure_band = clean_optional(signal.exposure_band);
        normalized_signals.insert(event_key, signal);
    }
    policy.event_signals = normalized_signals;

    Ok(policy)
}

fn apply_ranks(items: &mut [ScoredItem], omitted_component: Option<&str>) {
    items.sort_by(|left, right| score_cmp(left, right, omitted_component));
    for (index, item) in items.iter_mut().enumerate() {
        if omitted_component.is_none() {
            item.item.rank = (index + 1) as u64;
        }
    }
}

fn sensitivity_rank_maps(items: &[ScoredItem]) -> BTreeMap<String, BTreeMap<String, u64>> {
    let mut maps = BTreeMap::new();
    for component in COMPONENTS {
        let mut adjusted = items.to_vec();
        apply_ranks(&mut adjusted, Some(component));
        maps.insert(
            component.to_string(),
            adjusted
                .into_iter()
                .enumerate()
                .map(|(index, item)| (item.item.event_key, (index + 1) as u64))
                .collect(),
        );
    }
    maps
}

fn score_cmp(left: &ScoredItem, right: &ScoredItem, omitted_component: Option<&str>) -> Ordering {
    adjusted_score(right, omitted_component)
        .cmp(&adjusted_score(left, omitted_component))
        .then_with(|| left.first_seen_at.cmp(&right.first_seen_at))
        .then_with(|| left.item.event_key.cmp(&right.item.event_key))
        .then_with(|| left.field_name.cmp(&right.field_name))
}

fn adjusted_score(item: &ScoredItem, omitted_component: Option<&str>) -> i64 {
    let mut score = item.item.expected_coverage_value_units;
    if let Some(component) = omitted_component
        && let Some(component_score) = item
            .item
            .components
            .iter()
            .find(|candidate| candidate.component == component)
    {
        score -= component_score.contribution_units;
    }
    score
}

fn build_summary(items: &[RankedInboxItem]) -> PriorityRankingSummary {
    let mut summary = PriorityRankingSummary {
        total_items: items.len() as u64,
        uncertain_items: items
            .iter()
            .filter(|item| !item.uncertainty_flags.is_empty())
            .count() as u64,
        capped_outlier_items: items
            .iter()
            .filter(|item| !item.capped_outliers.is_empty())
            .count() as u64,
        highest_expected_coverage_value_units: items
            .iter()
            .map(|item| item.expected_coverage_value_units)
            .max()
            .unwrap_or(0),
        by_partition: BTreeMap::new(),
        by_uncertainty_flag: BTreeMap::new(),
    };

    for item in items {
        *summary
            .by_partition
            .entry(partition_key(&item.queue_partition))
            .or_default() += 1;
        for flag in &item.uncertainty_flags {
            *summary.by_uncertainty_flag.entry(flag.clone()).or_default() += 1;
        }
    }

    summary
}

fn push_capped_component(
    components: &mut Vec<PriorityComponentScore>,
    capped_outliers: &mut Vec<CappedOutlier>,
    component: &str,
    raw_value: Option<u64>,
    policy: &PriorityPolicy,
    rationale: &str,
) {
    let cap = policy.caps.get(component).copied();
    let weight = weight(policy, component);
    let (effective_u64, capped) = match (raw_value, cap) {
        (Some(raw), Some(cap)) => (raw.min(cap), raw > cap),
        (Some(raw), None) => (raw, false),
        (None, _) => (0, false),
    };
    if let (Some(raw), Some(cap)) = (raw_value, cap)
        && raw > cap
    {
        capped_outliers.push(CappedOutlier {
            component: component.to_string(),
            raw_value: raw,
            cap,
        });
    }
    components.push(PriorityComponentScore {
        component: component.to_string(),
        raw_value,
        effective_value: effective_u64 as i64,
        cap,
        weight,
        contribution_units: (effective_u64 as i64) * weight,
        missing_signal: raw_value.is_none(),
        capped,
        rationale: rationale.to_string(),
    });
}

fn push_mapped_component(
    components: &mut Vec<PriorityComponentScore>,
    component: &str,
    key: &str,
    mapped_value: Option<i64>,
    weight: i64,
    rationale: &str,
) {
    let effective_value = mapped_value.unwrap_or_default();
    components.push(PriorityComponentScore {
        component: component.to_string(),
        raw_value: None,
        effective_value,
        cap: None,
        weight,
        contribution_units: effective_value * weight,
        missing_signal: mapped_value.is_none(),
        capped: false,
        rationale: format!("{rationale}; policy key={key}"),
    });
}

fn push_exposure_component(
    components: &mut Vec<PriorityComponentScore>,
    item: &UnresolvedInboxItem,
    signal: Option<&PrioritySignalOverride>,
    policy: &PriorityPolicy,
) -> InboxResult<()> {
    let (band, score_units, missing_signal) =
        if let Some(band) = signal.and_then(|signal| signal.exposure_band.as_deref()) {
            let Some(band_score) = policy
                .exposure_bands
                .iter()
                .find(|candidate| candidate.band == band)
                .map(|candidate| candidate.score_units)
            else {
                return Err(artifact_contract_error(format!(
                    "event signal references unknown exposure band {band}"
                )));
            };
            (band.to_string(), band_score, false)
        } else {
            let inferred = policy
                .exposure_bands
                .iter()
                .rfind(|band| item.occurrence_summary.total_occurrences >= band.min_occurrences);
            match inferred {
                Some(band) => (band.band.clone(), band.score_units, true),
                None => ("unknown".to_string(), 0, true),
            }
        };
    let weight = weight(policy, "exposure_band");
    components.push(PriorityComponentScore {
        component: "exposure_band".to_string(),
        raw_value: Some(item.occurrence_summary.total_occurrences),
        effective_value: score_units,
        cap: None,
        weight,
        contribution_units: score_units * weight,
        missing_signal,
        capped: false,
        rationale: format!(
            "exposure band {band} is policy-defined from explicit signal or recurrence"
        ),
    });
    Ok(())
}

fn queue_partition(
    item: &UnresolvedInboxItem,
    signal: Option<&PrioritySignalOverride>,
) -> QueuePartition {
    QueuePartition {
        profile: item
            .profile_ref
            .as_ref()
            .map(|profile| profile.profile_id.clone())
            .unwrap_or_else(|| "unknown_profile".to_string()),
        registry: signal
            .and_then(|signal| signal.registry_id.clone())
            .or_else(|| {
                item.namespace_hints
                    .iter()
                    .map(|hint| hint.namespace.clone())
                    .next()
            })
            .unwrap_or_else(|| "unknown_registry".to_string()),
        role: role_key(item.field_role).to_string(),
        source: signal
            .and_then(|signal| signal.source_partition.clone())
            .or_else(|| {
                item.occurrences
                    .iter()
                    .map(|occurrence| occurrence.source_ref.clone())
                    .next()
            })
            .unwrap_or_else(|| "unknown_source".to_string()),
        privacy_class: item
            .privacy_class
            .map(privacy_key)
            .unwrap_or("unknown_privacy")
            .to_string(),
    }
}

fn partition_key(partition: &QueuePartition) -> String {
    format!(
        "profile={}|registry={}|role={}|source={}|privacy={}",
        partition.profile,
        partition.registry,
        partition.role,
        partition.source,
        partition.privacy_class
    )
}

fn role_key(role: InboxFieldRole) -> &'static str {
    match role {
        InboxFieldRole::LookupInput => "lookup_input",
        InboxFieldRole::NameField => "name_field",
        InboxFieldRole::AnchorField => "anchor_field",
        InboxFieldRole::ContextField => "context_field",
        InboxFieldRole::CandidatePair => "candidate_pair",
    }
}

fn privacy_key(privacy: PrivacyClass) -> &'static str {
    match privacy {
        PrivacyClass::Public => "public",
        PrivacyClass::Internal => "internal",
        PrivacyClass::Restricted => "restricted",
        PrivacyClass::Secret => "secret",
    }
}

fn candidate_status_key(status: CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::None => "none",
        CandidateStatus::Ambiguous => "ambiguous",
        CandidateStatus::Rejected => "rejected",
        CandidateStatus::BudgetLimited => "budget_limited",
    }
}

fn ambiguity_units(item: &UnresolvedInboxItem) -> u64 {
    match item.candidate_summary.status {
        CandidateStatus::Ambiguous => item.candidate_summary.candidate_count.max(1) as u64,
        CandidateStatus::Rejected | CandidateStatus::BudgetLimited => {
            item.candidate_summary.candidate_count as u64
        }
        CandidateStatus::None => 0,
    }
}

fn review_effort_units(item: &UnresolvedInboxItem, signal: Option<&PrioritySignalOverride>) -> u64 {
    signal
        .and_then(|signal| signal.review_effort_units)
        .unwrap_or_else(|| {
            item.surface_fingerprints.len() as u64
                + u64::from(item.candidate_summary.candidate_count)
                + item.candidate_summary.rejection_reasons.len() as u64
        })
}

fn age_days(first_seen_at: &str, as_of: DateTime<Utc>) -> InboxResult<u64> {
    let first_seen_at = parse_timestamp(first_seen_at, "item.first_seen_at")?;
    Ok(nonnegative_days(as_of.signed_duration_since(first_seen_at)))
}

fn drift_units(
    item: &UnresolvedInboxItem,
    signal: Option<&PrioritySignalOverride>,
    as_of: DateTime<Utc>,
) -> InboxResult<u64> {
    if let Some(drift_events) = signal.and_then(|signal| signal.drift_events) {
        return Ok(drift_events);
    }
    let first_seen_at = parse_timestamp(&item.first_seen_at, "item.first_seen_at")?;
    let last_seen_at = parse_timestamp(&item.last_seen_at, "item.last_seen_at")?;
    let observed_days = nonnegative_days(last_seen_at.signed_duration_since(first_seen_at));
    let open_days = nonnegative_days(as_of.signed_duration_since(last_seen_at));
    Ok(observed_days + item.occurrence_summary.distinct_runs + (open_days / 30))
}

fn nonnegative_days(duration: chrono::Duration) -> u64 {
    duration.num_days().max(0) as u64
}

fn weight(policy: &PriorityPolicy, component: &str) -> i64 {
    policy
        .component_weights
        .get(component)
        .copied()
        .unwrap_or(0)
}

fn normalized_event_key(value: &str) -> InboxResult<String> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(
            "priority event_signals keys must be blake3 event keys",
        ));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(artifact_contract_error(
            "priority event_signals keys must contain 64 lowercase hex characters",
        ));
    }
    Ok(value.to_string())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_timestamp(value: &str, field: &str) -> InboxResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            artifact_contract_error(format!("invalid RFC3339 timestamp for {field}: {error}"))
        })
}

fn canonical_timestamp(value: &str, field: &str) -> InboxResult<String> {
    Ok(parse_timestamp(value, field)?.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

fn hash_without_self(artifact: &InboxPriorityRankingArtifact) -> InboxResult<String> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hash_serialized(&hashable, "inbox priority ranking artifact")
}

fn hash_serialized(value: &impl Serialize, label: &str) -> InboxResult<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| artifact_contract_error(format!("failed to hash {label}: {error}")))?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn artifact_contract_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::ArtifactContract, message)
}
