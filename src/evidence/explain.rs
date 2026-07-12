#![forbid(unsafe_code)]

//! Deterministic evidence-waterfall explanations derived from frozen artifacts.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const CANON_ENTITY_EXPLAIN_WATERFALL_VERSION: &str = "canon_entity_explain.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceExplanationOutcome {
    ExactExisting,
    NewCluster,
    Linked,
    Ambiguous,
    Contradictory,
    BlockedByVeto,
    BelowThreshold,
    ReviewOverridden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceWaterfallEntryKind {
    Veto,
    PositiveScoreContribution,
    Context,
    MissingEvidence,
    PolicyClause,
    RegistryFact,
    CandidateContext,
    SolverDecision,
    ReviewOverride,
}

impl EvidenceWaterfallEntryKind {
    const fn lane_id(self) -> &'static str {
        match self {
            Self::Veto => "vetoes",
            Self::PositiveScoreContribution => "positive_score_contributions",
            Self::Context => "context",
            Self::MissingEvidence => "missing_evidence",
            Self::PolicyClause => "policy_clauses",
            Self::RegistryFact => "registry_facts",
            Self::CandidateContext => "candidate_context",
            Self::SolverDecision => "solver_decisions",
            Self::ReviewOverride => "review_overrides",
        }
    }

    const fn sort_rank(self) -> u8 {
        match self {
            Self::Veto => 0,
            Self::ReviewOverride => 1,
            Self::PolicyClause => 2,
            Self::SolverDecision => 3,
            Self::PositiveScoreContribution => 4,
            Self::Context => 5,
            Self::MissingEvidence => 6,
            Self::RegistryFact => 7,
            Self::CandidateContext => 8,
        }
    }
}

impl EvidenceExplanationOutcome {
    const fn summary_id(self) -> &'static str {
        match self {
            Self::ExactExisting => "exact_existing",
            Self::NewCluster => "new_cluster",
            Self::Linked => "linked",
            Self::Ambiguous => "ambiguous",
            Self::Contradictory => "contradictory",
            Self::BlockedByVeto => "blocked_by_veto",
            Self::BelowThreshold => "below_threshold",
            Self::ReviewOverridden => "review_overridden",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceExplainSourceArtifact {
    pub artifact_kind: String,
    pub version: String,
    pub content_hash: String,
}

impl EvidenceExplainSourceArtifact {
    pub fn new(
        artifact_kind: impl Into<String>,
        version: impl Into<String>,
        content_hash: impl Into<String>,
    ) -> Self {
        Self {
            artifact_kind: artifact_kind.into(),
            version: version.into(),
            content_hash: content_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceExplainInputRecord {
    pub source_kind: String,
    pub source_id: String,
    pub entry_kind: EvidenceWaterfallEntryKind,
    #[serde(default)]
    pub target_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub score_units: i64,
    #[serde(default)]
    pub hard_veto: bool,
    pub summary: String,
    #[serde(default)]
    pub sensitive: bool,
}

impl EvidenceExplainInputRecord {
    pub fn new(
        source_kind: impl Into<String>,
        source_id: impl Into<String>,
        entry_kind: EvidenceWaterfallEntryKind,
        score_units: i64,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            source_kind: source_kind.into(),
            source_id: source_id.into(),
            entry_kind,
            target_ids: Vec::new(),
            operator_id: None,
            reason_code: None,
            score_units,
            hard_veto: false,
            summary: summary.into(),
            sensitive: false,
        }
    }

    pub fn with_targets(mut self, target_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.target_ids = target_ids.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_operator(
        mut self,
        operator_id: impl Into<String>,
        reason_code: impl Into<String>,
    ) -> Self {
        self.operator_id = Some(operator_id.into());
        self.reason_code = Some(reason_code.into());
        self
    }

    pub const fn with_hard_veto(mut self) -> Self {
        self.hard_veto = true;
        self
    }

    pub const fn with_sensitive_payload(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceExplainPageRequest {
    pub page: usize,
    pub per_page: usize,
}

impl Default for EvidenceExplainPageRequest {
    fn default() -> Self {
        Self {
            page: 0,
            per_page: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceExplainRedactionPolicy {
    pub include_sensitive_text: bool,
    pub max_summary_bytes: usize,
}

impl Default for EvidenceExplainRedactionPolicy {
    fn default() -> Self {
        Self {
            include_sensitive_text: false,
            max_summary_bytes: 160,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallRequest {
    pub decision_id: String,
    pub outcome: EvidenceExplanationOutcome,
    #[serde(default)]
    pub subject_ids: Vec<String>,
    pub observed_score_units: i64,
    pub threshold_score_units: i64,
    #[serde(default)]
    pub source_artifacts: Vec<EvidenceExplainSourceArtifact>,
    #[serde(default)]
    pub evidence_records: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub candidate_context: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub solver_decisions: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub policy_clauses: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub registry_facts: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub review_overrides: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub missing_evidence: Vec<EvidenceExplainInputRecord>,
    #[serde(default)]
    pub redaction: EvidenceExplainRedactionPolicy,
    #[serde(default)]
    pub page: EvidenceExplainPageRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallArtifact {
    pub version: String,
    pub decision_id: String,
    pub outcome: EvidenceExplanationOutcome,
    pub subject_ids: Vec<String>,
    pub source_artifacts: Vec<EvidenceExplainSourceArtifact>,
    pub summary: EvidenceWaterfallSummary,
    pub pagination: EvidenceWaterfallPagination,
    pub lanes: BTreeMap<String, EvidenceWaterfallLane>,
    pub human_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallSummary {
    pub total_entries: u64,
    pub positive_score_units: i64,
    pub negative_score_units: i64,
    pub observed_score_units: i64,
    pub threshold_score_units: i64,
    pub counterfactual: EvidenceWaterfallCounterfactual,
    pub counts_by_lane: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallCounterfactual {
    pub policy_decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_score_units_to_threshold: Option<i64>,
    pub hard_veto_count: u64,
    pub competing_context_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallPagination {
    pub page: usize,
    pub per_page: usize,
    pub total_entries: usize,
    pub page_entries: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallLane {
    pub lane: String,
    pub entries: Vec<EvidenceWaterfallEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWaterfallEntry {
    pub ordinal: u64,
    pub entry_id: String,
    pub entry_kind: EvidenceWaterfallEntryKind,
    pub source_kind: String,
    pub source_id: String,
    #[serde(default)]
    pub target_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub score_units: i64,
    pub hard_veto: bool,
    pub summary: String,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceExplainError {
    pub field: String,
    pub message: String,
}

impl EvidenceExplainError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for EvidenceExplainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for EvidenceExplainError {}

pub fn render_evidence_waterfall(
    request: EvidenceWaterfallRequest,
) -> Result<EvidenceWaterfallArtifact, EvidenceExplainError> {
    validate_request(&request)?;

    let subject_ids = sorted_unique(request.subject_ids);
    let mut source_artifacts = request.source_artifacts;
    source_artifacts.sort_by(|left, right| {
        left.artifact_kind
            .cmp(&right.artifact_kind)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.content_hash.cmp(&right.content_hash))
    });

    let mut inputs = Vec::new();
    inputs.extend(request.evidence_records);
    inputs.extend(force_kind(
        request.candidate_context,
        EvidenceWaterfallEntryKind::CandidateContext,
    ));
    inputs.extend(force_kind(
        request.solver_decisions,
        EvidenceWaterfallEntryKind::SolverDecision,
    ));
    inputs.extend(force_kind(
        request.policy_clauses,
        EvidenceWaterfallEntryKind::PolicyClause,
    ));
    inputs.extend(force_kind(
        request.registry_facts,
        EvidenceWaterfallEntryKind::RegistryFact,
    ));
    inputs.extend(force_kind(
        request.review_overrides,
        EvidenceWaterfallEntryKind::ReviewOverride,
    ));
    inputs.extend(force_kind(
        request.missing_evidence,
        EvidenceWaterfallEntryKind::MissingEvidence,
    ));

    let mut entries = inputs
        .into_iter()
        .map(|record| entry_from_record(record, request.redaction))
        .collect::<Vec<_>>();
    entries.sort_by(entry_cmp);
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.ordinal = index as u64;
    }

    let summary = build_summary(
        &entries,
        request.outcome,
        request.observed_score_units,
        request.threshold_score_units,
    );
    let pagination = paginate(entries.len(), request.page);
    let page_entries = entries
        .into_iter()
        .skip(pagination.page.saturating_mul(pagination.per_page))
        .take(pagination.per_page)
        .collect::<Vec<_>>();
    let lanes = lanes_from_entries(page_entries);
    let human_summary = render_evidence_waterfall_summary_parts(
        &request.decision_id,
        request.outcome,
        &summary,
        &pagination,
    );

    Ok(EvidenceWaterfallArtifact {
        version: CANON_ENTITY_EXPLAIN_WATERFALL_VERSION.to_string(),
        decision_id: request.decision_id,
        outcome: request.outcome,
        subject_ids,
        source_artifacts,
        summary,
        pagination,
        lanes,
        human_summary,
    })
}

pub fn render_evidence_waterfall_summary(artifact: &EvidenceWaterfallArtifact) -> String {
    render_evidence_waterfall_summary_parts(
        &artifact.decision_id,
        artifact.outcome,
        &artifact.summary,
        &artifact.pagination,
    )
}

fn validate_request(request: &EvidenceWaterfallRequest) -> Result<(), EvidenceExplainError> {
    if request.decision_id.trim().is_empty() {
        return Err(EvidenceExplainError::new(
            "decision_id",
            "must not be empty",
        ));
    }
    if request.source_artifacts.is_empty() {
        return Err(EvidenceExplainError::new(
            "source_artifacts",
            "at least one frozen source artifact is required",
        ));
    }
    if request.page.per_page == 0 {
        return Err(EvidenceExplainError::new("page.per_page", "must be > 0"));
    }
    if request.redaction.max_summary_bytes == 0 {
        return Err(EvidenceExplainError::new(
            "redaction.max_summary_bytes",
            "must be > 0",
        ));
    }
    Ok(())
}

fn force_kind(
    records: Vec<EvidenceExplainInputRecord>,
    kind: EvidenceWaterfallEntryKind,
) -> Vec<EvidenceExplainInputRecord> {
    records
        .into_iter()
        .map(|mut record| {
            record.entry_kind = kind;
            record
        })
        .collect()
}

fn entry_from_record(
    record: EvidenceExplainInputRecord,
    redaction: EvidenceExplainRedactionPolicy,
) -> EvidenceWaterfallEntry {
    let target_ids = sorted_unique(record.target_ids.clone());
    let (summary, redacted) = redacted_summary(&record.summary, record.sensitive, redaction);
    let entry_id = deterministic_entry_id(&record, &target_ids);
    EvidenceWaterfallEntry {
        ordinal: 0,
        entry_id,
        entry_kind: record.entry_kind,
        source_kind: record.source_kind,
        source_id: record.source_id,
        target_ids,
        operator_id: record.operator_id,
        reason_code: record.reason_code,
        score_units: record.score_units,
        hard_veto: record.hard_veto,
        summary,
        redacted,
    }
}

fn deterministic_entry_id(record: &EvidenceExplainInputRecord, target_ids: &[String]) -> String {
    let bytes = serde_json::to_vec(&(
        record.entry_kind,
        &record.source_kind,
        &record.source_id,
        target_ids,
        &record.operator_id,
        &record.reason_code,
        record.score_units,
        record.hard_veto,
    ))
    .expect("entry id tuple serializes");
    format!("explain:{}", blake3::hash(&bytes).to_hex())
}

fn redacted_summary(
    text: &str,
    sensitive: bool,
    redaction: EvidenceExplainRedactionPolicy,
) -> (String, bool) {
    if sensitive && !redaction.include_sensitive_text {
        let hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        return (format!("redacted:blake3:{}", &hash[..16]), true);
    }
    (truncate_utf8(text, redaction.max_summary_bytes), false)
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut out = String::new();
    for character in text.chars() {
        if out.len() + character.len_utf8() > max_bytes {
            break;
        }
        out.push(character);
    }
    out
}

fn build_summary(
    entries: &[EvidenceWaterfallEntry],
    outcome: EvidenceExplanationOutcome,
    observed_score_units: i64,
    threshold_score_units: i64,
) -> EvidenceWaterfallSummary {
    let mut counts_by_lane = initialized_lane_counts();
    let mut positive_score_units = 0_i64;
    let mut negative_score_units = 0_i64;
    let mut hard_veto_count = 0_u64;
    let mut competing_context_count = 0_u64;

    for entry in entries {
        *counts_by_lane
            .entry(entry.entry_kind.lane_id().to_string())
            .or_default() += 1;
        if entry.entry_kind == EvidenceWaterfallEntryKind::PositiveScoreContribution
            && entry.score_units > 0
        {
            positive_score_units = positive_score_units.saturating_add(entry.score_units);
        }
        if matches!(
            entry.entry_kind,
            EvidenceWaterfallEntryKind::Veto | EvidenceWaterfallEntryKind::Context
        ) && entry.score_units < 0
        {
            negative_score_units = negative_score_units.saturating_add(entry.score_units);
        }
        if entry.entry_kind == EvidenceWaterfallEntryKind::Veto && entry.hard_veto {
            hard_veto_count += 1;
        }
        if entry.entry_kind == EvidenceWaterfallEntryKind::CandidateContext {
            competing_context_count += 1;
        }
    }

    let additional_score_units_to_threshold = (observed_score_units < threshold_score_units)
        .then_some(threshold_score_units.saturating_sub(observed_score_units));
    EvidenceWaterfallSummary {
        total_entries: entries.len() as u64,
        positive_score_units,
        negative_score_units,
        observed_score_units,
        threshold_score_units,
        counterfactual: EvidenceWaterfallCounterfactual {
            policy_decision: policy_decision(outcome, hard_veto_count),
            additional_score_units_to_threshold,
            hard_veto_count,
            competing_context_count,
        },
        counts_by_lane,
    }
}

fn policy_decision(outcome: EvidenceExplanationOutcome, hard_veto_count: u64) -> String {
    if hard_veto_count > 0 {
        "blocked_by_veto".to_string()
    } else {
        match outcome {
            EvidenceExplanationOutcome::Ambiguous => "ambiguous".to_string(),
            EvidenceExplanationOutcome::Contradictory => "contradictory".to_string(),
            EvidenceExplanationOutcome::BelowThreshold => "below_threshold".to_string(),
            EvidenceExplanationOutcome::ReviewOverridden => "review_overridden".to_string(),
            EvidenceExplanationOutcome::ExactExisting
            | EvidenceExplanationOutcome::NewCluster
            | EvidenceExplanationOutcome::Linked
            | EvidenceExplanationOutcome::BlockedByVeto => "threshold_satisfied".to_string(),
        }
    }
}

fn initialized_lane_counts() -> BTreeMap<String, u64> {
    [
        "vetoes",
        "positive_score_contributions",
        "context",
        "missing_evidence",
        "policy_clauses",
        "registry_facts",
        "candidate_context",
        "solver_decisions",
        "review_overrides",
    ]
    .into_iter()
    .map(|lane| (lane.to_string(), 0))
    .collect()
}

fn paginate(total_entries: usize, page: EvidenceExplainPageRequest) -> EvidenceWaterfallPagination {
    let start = page.page.saturating_mul(page.per_page);
    let page_entries = total_entries.saturating_sub(start).min(page.per_page);
    let next_page = (start + page_entries < total_entries).then_some(page.page + 1);
    EvidenceWaterfallPagination {
        page: page.page,
        per_page: page.per_page,
        total_entries,
        page_entries,
        next_page,
    }
}

fn lanes_from_entries(
    entries: Vec<EvidenceWaterfallEntry>,
) -> BTreeMap<String, EvidenceWaterfallLane> {
    let mut lanes = initialized_lane_counts()
        .into_keys()
        .map(|lane| {
            (
                lane.clone(),
                EvidenceWaterfallLane {
                    lane,
                    entries: Vec::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for entry in entries {
        let lane = entry.entry_kind.lane_id().to_string();
        lanes
            .entry(lane.clone())
            .or_insert_with(|| EvidenceWaterfallLane {
                lane: lane.clone(),
                entries: Vec::new(),
            })
            .entries
            .push(entry);
    }
    lanes
}

fn render_evidence_waterfall_summary_parts(
    decision_id: &str,
    outcome: EvidenceExplanationOutcome,
    summary: &EvidenceWaterfallSummary,
    pagination: &EvidenceWaterfallPagination,
) -> String {
    format!(
        "{} decision={} outcome={} score={}/{} vetoes={} missing={} page={}/{} entries={}",
        CANON_ENTITY_EXPLAIN_WATERFALL_VERSION,
        decision_id,
        outcome.summary_id(),
        summary.observed_score_units,
        summary.threshold_score_units,
        summary.counterfactual.hard_veto_count,
        summary
            .counts_by_lane
            .get("missing_evidence")
            .copied()
            .unwrap_or(0),
        pagination.page,
        pagination.next_page.unwrap_or(pagination.page),
        pagination.page_entries
    )
}

fn entry_cmp(left: &EvidenceWaterfallEntry, right: &EvidenceWaterfallEntry) -> std::cmp::Ordering {
    left.entry_kind
        .sort_rank()
        .cmp(&right.entry_kind.sort_rank())
        .then_with(|| left.source_kind.cmp(&right.source_kind))
        .then_with(|| left.source_id.cmp(&right.source_id))
        .then_with(|| left.operator_id.cmp(&right.operator_id))
        .then_with(|| left.reason_code.cmp(&right.reason_code))
        .then_with(|| left.target_ids.cmp(&right.target_ids))
        .then_with(|| left.entry_id.cmp(&right.entry_id))
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}
