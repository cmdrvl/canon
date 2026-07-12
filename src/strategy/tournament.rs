use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const STRATEGY_TOURNAMENT_SCHEMA_VERSION: &str = "canon.strategy.tournament.v1";

pub fn strategy_tournament_schema_version() -> &'static str {
    STRATEGY_TOURNAMENT_SCHEMA_VERSION
}

pub type StrategyTournamentResult<T> = Result<T, StrategyTournamentError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyTournamentErrorKind {
    UnsupportedVersion,
    EmptyField,
    InvalidDigest,
    DuplicatePartition,
    DuplicateCandidate,
    DuplicateEvaluation,
    UnknownCandidate,
    EmptyRankingPolicy,
    HoldoutGenerationAccess,
    HoldoutRanking,
    PackageBoundary,
    MissingEvaluation,
    MissingRankingMetric,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentError {
    pub kind: StrategyTournamentErrorKind,
    pub message: String,
}

impl StrategyTournamentError {
    fn new(kind: StrategyTournamentErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for StrategyTournamentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for StrategyTournamentError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentInput {
    pub version: String,
    pub tournament_id: String,
    pub strategy_kind: String,
    pub package_digest: String,
    pub partitions: StrategyTournamentPartitions,
    pub candidates: Vec<StrategyTournamentCandidate>,
    pub evaluations: Vec<StrategyTournamentEvaluation>,
    pub ranking_policy: StrategyTournamentRankingPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentPartitions {
    pub train: StrategyTournamentPartition,
    pub tune: StrategyTournamentPartition,
    pub holdout: StrategyTournamentPartition,
}

impl StrategyTournamentPartitions {
    fn validate(&mut self) -> StrategyTournamentResult<()> {
        self.train.normalize("partitions.train")?;
        self.tune.normalize("partitions.tune")?;
        self.holdout.normalize("partitions.holdout")?;

        let mut ids = BTreeSet::new();
        let mut digests = BTreeSet::new();
        for partition in [&self.train, &self.tune, &self.holdout] {
            if !ids.insert(partition.partition_id.clone()) {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::DuplicatePartition,
                    "partition ids must be unique",
                ));
            }
            if !digests.insert(partition.corpus_digest.clone()) {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::DuplicatePartition,
                    "partition corpus digests must be unique",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentPartition {
    pub partition_id: String,
    pub corpus_digest: String,
    pub labels_digest: String,
    pub row_count: u64,
}

impl StrategyTournamentPartition {
    fn normalize(&mut self, field: &str) -> StrategyTournamentResult<()> {
        self.partition_id = normalized_non_empty(&self.partition_id, field, "partition_id")?;
        self.corpus_digest = normalized_digest(&self.corpus_digest, field, "corpus_digest")?;
        self.labels_digest = normalized_digest(&self.labels_digest, field, "labels_digest")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentPartitionRole {
    Train,
    Tune,
    Holdout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentAccessKind {
    Features,
    Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TournamentGenerationAccess {
    pub partition_role: TournamentPartitionRole,
    pub access_kind: TournamentAccessKind,
    pub package_digest: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentCandidate {
    pub candidate_id: String,
    pub package_digest: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    pub generation_inputs: Vec<TournamentGenerationAccess>,
}

impl StrategyTournamentCandidate {
    fn normalize(&mut self, tournament_package_digest: &str) -> StrategyTournamentResult<()> {
        self.candidate_id = normalized_non_empty(&self.candidate_id, "candidate", "candidate_id")?;
        self.package_digest =
            normalized_digest(&self.package_digest, "candidate", "package_digest")?;
        if self.package_digest != tournament_package_digest {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::PackageBoundary,
                "candidate package digest must match the declared tournament package",
            ));
        }

        let mut parameters = BTreeMap::new();
        for (key, value) in std::mem::take(&mut self.parameters) {
            let key = normalized_non_empty(&key, "candidate.parameters", "key")?;
            let value = value.trim().to_string();
            parameters.insert(key, value);
        }
        self.parameters = parameters;

        for access in &mut self.generation_inputs {
            access.package_digest = normalized_digest(
                &access.package_digest,
                "generation_inputs",
                "package_digest",
            )?;
            access.source_digest =
                normalized_digest(&access.source_digest, "generation_inputs", "source_digest")?;
            if access.package_digest != self.package_digest {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::PackageBoundary,
                    "candidate generation input references a package outside the declared candidate package",
                ));
            }
            if matches!(access.partition_role, TournamentPartitionRole::Holdout) {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::HoldoutGenerationAccess,
                    "candidate generation cannot read holdout features or labels",
                ));
            }
        }
        self.generation_inputs.sort();
        self.generation_inputs.dedup();

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentEvaluationStatus {
    Passed,
    ResourceFailure,
    PolicyDenied,
    RunnerFailure,
}

impl TournamentEvaluationStatus {
    fn rank(self) -> u8 {
        match self {
            Self::Passed => 0,
            Self::ResourceFailure => 1,
            Self::PolicyDenied => 2,
            Self::RunnerFailure => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentResourceCost {
    pub wall_ms: u64,
    pub peak_memory_bytes: u64,
    pub output_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentUncertainty {
    pub metric: String,
    pub lower: i64,
    pub upper: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentEvaluation {
    pub candidate_id: String,
    pub partition_role: TournamentPartitionRole,
    pub run_digest: String,
    pub status: TournamentEvaluationStatus,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, i64>,
    pub hard_negative_failures: u64,
    pub resource_cost: TournamentResourceCost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regressions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncertainty: Vec<TournamentUncertainty>,
}

impl StrategyTournamentEvaluation {
    fn normalize(&mut self) -> StrategyTournamentResult<()> {
        self.candidate_id = normalized_non_empty(&self.candidate_id, "evaluation", "candidate_id")?;
        self.run_digest = normalized_digest(&self.run_digest, "evaluation", "run_digest")?;

        let mut metrics = BTreeMap::new();
        for (key, value) in std::mem::take(&mut self.metrics) {
            metrics.insert(
                normalized_non_empty(&key, "evaluation.metrics", "key")?,
                value,
            );
        }
        self.metrics = metrics;

        for regression in &mut self.regressions {
            *regression = regression.trim().to_string();
        }
        self.regressions.retain(|value| !value.is_empty());
        self.regressions.sort();
        self.regressions.dedup();

        for uncertainty in &mut self.uncertainty {
            uncertainty.metric =
                normalized_non_empty(&uncertainty.metric, "evaluation.uncertainty", "metric")?;
            if uncertainty.lower > uncertainty.upper {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::EmptyField,
                    "uncertainty lower bound must be <= upper bound",
                ));
            }
        }
        self.uncertainty
            .sort_by(|left, right| left.metric.cmp(&right.metric));
        self.uncertainty
            .dedup_by(|left, right| left.metric == right.metric);

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentMetricGoal {
    Maximize,
    Minimize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentMetricRule {
    pub metric: String,
    pub goal: TournamentMetricGoal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentRankingPolicy {
    pub partition_role: TournamentPartitionRole,
    pub metric_order: Vec<TournamentMetricRule>,
}

impl StrategyTournamentRankingPolicy {
    fn normalize(&mut self) -> StrategyTournamentResult<()> {
        if matches!(self.partition_role, TournamentPartitionRole::Holdout) {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::HoldoutRanking,
                "holdout metrics cannot be used to rank tournament candidates",
            ));
        }
        if self.metric_order.is_empty() {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::EmptyRankingPolicy,
                "ranking policy must contain at least one metric rule",
            ));
        }
        for rule in &mut self.metric_order {
            rule.metric = normalized_non_empty(&rule.metric, "ranking_policy", "metric")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentSummary {
    pub candidate_count: usize,
    pub ranking_partition: TournamentPartitionRole,
    pub ranking_metrics: Vec<String>,
    pub holdout_evaluations: usize,
    pub failed_candidates: usize,
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentCandidateReport {
    pub rank: usize,
    pub candidate_id: String,
    pub package_digest: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    pub ranking_status: TournamentEvaluationStatus,
    pub ranking_metrics: BTreeMap<String, i64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub holdout_metrics: BTreeMap<String, i64>,
    pub hard_negative_failures: u64,
    pub resource_cost: TournamentResourceCost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regressions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncertainty: Vec<TournamentUncertainty>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyTournamentReport {
    pub version: String,
    pub tournament_id: String,
    pub strategy_kind: String,
    pub package_digest: String,
    pub partitions: StrategyTournamentPartitions,
    pub ranking_policy: StrategyTournamentRankingPolicy,
    pub summary: StrategyTournamentSummary,
    pub candidates: Vec<StrategyTournamentCandidateReport>,
    pub content_hash: String,
}

pub fn run_strategy_tournament(
    mut input: StrategyTournamentInput,
) -> StrategyTournamentResult<StrategyTournamentReport> {
    input.version = input.version.trim().to_string();
    if input.version != STRATEGY_TOURNAMENT_SCHEMA_VERSION {
        return Err(StrategyTournamentError::new(
            StrategyTournamentErrorKind::UnsupportedVersion,
            "unsupported strategy tournament version",
        ));
    }

    input.tournament_id =
        normalized_non_empty(&input.tournament_id, "tournament", "tournament_id")?;
    input.strategy_kind =
        normalized_non_empty(&input.strategy_kind, "tournament", "strategy_kind")?;
    input.package_digest =
        normalized_digest(&input.package_digest, "tournament", "package_digest")?;
    input.partitions.validate()?;
    input.ranking_policy.normalize()?;

    let mut candidate_ids = BTreeSet::new();
    for candidate in &mut input.candidates {
        candidate.normalize(&input.package_digest)?;
        if !candidate_ids.insert(candidate.candidate_id.clone()) {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::DuplicateCandidate,
                "candidate ids must be unique",
            ));
        }
    }
    input
        .candidates
        .sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));

    let mut evaluations = BTreeMap::new();
    for evaluation in &mut input.evaluations {
        evaluation.normalize()?;
        if !candidate_ids.contains(&evaluation.candidate_id) {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::UnknownCandidate,
                "evaluation references an unknown candidate",
            ));
        }
        let key = (evaluation.candidate_id.clone(), evaluation.partition_role);
        if evaluations.insert(key, evaluation.clone()).is_some() {
            return Err(StrategyTournamentError::new(
                StrategyTournamentErrorKind::DuplicateEvaluation,
                "candidate evaluations must be unique per partition",
            ));
        }
    }

    let mut scored = Vec::new();
    for candidate in &input.candidates {
        let ranking_eval = evaluation_for(
            &evaluations,
            &candidate.candidate_id,
            input.ranking_policy.partition_role,
        )?;
        for rule in &input.ranking_policy.metric_order {
            if !ranking_eval.metrics.contains_key(&rule.metric) {
                return Err(StrategyTournamentError::new(
                    StrategyTournamentErrorKind::MissingRankingMetric,
                    format!(
                        "candidate {} is missing ranking metric {}",
                        candidate.candidate_id, rule.metric
                    ),
                ));
            }
        }

        let holdout_eval = evaluations
            .get(&(
                candidate.candidate_id.clone(),
                TournamentPartitionRole::Holdout,
            ))
            .cloned();
        scored.push(ScoredCandidate {
            candidate: candidate.clone(),
            ranking_eval,
            holdout_eval,
        });
    }

    scored.sort_by(|left, right| compare_scored(left, right, &input.ranking_policy));

    let mut reports = Vec::with_capacity(scored.len());
    let mut failed_candidates = 0usize;
    let mut holdout_evaluations = 0usize;
    for (index, scored_candidate) in scored.into_iter().enumerate() {
        if scored_candidate.ranking_eval.status != TournamentEvaluationStatus::Passed {
            failed_candidates += 1;
        }
        let holdout_metrics = scored_candidate
            .holdout_eval
            .as_ref()
            .map(|evaluation| evaluation.metrics.clone())
            .unwrap_or_default();
        if scored_candidate.holdout_eval.is_some() {
            holdout_evaluations += 1;
        }

        reports.push(StrategyTournamentCandidateReport {
            rank: index + 1,
            candidate_id: scored_candidate.candidate.candidate_id,
            package_digest: scored_candidate.candidate.package_digest,
            parameters: scored_candidate.candidate.parameters,
            ranking_status: scored_candidate.ranking_eval.status,
            ranking_metrics: scored_candidate.ranking_eval.metrics.clone(),
            holdout_metrics,
            hard_negative_failures: scored_candidate.ranking_eval.hard_negative_failures,
            resource_cost: scored_candidate.ranking_eval.resource_cost,
            regressions: scored_candidate.ranking_eval.regressions,
            uncertainty: scored_candidate.ranking_eval.uncertainty,
            recommendation: if index == 0 {
                "candidate_for_operator_review".to_string()
            } else {
                "not_selected".to_string()
            },
        });
    }

    let ranking_metrics = input
        .ranking_policy
        .metric_order
        .iter()
        .map(|rule| rule.metric.clone())
        .collect::<Vec<_>>();

    let ranking_partition = input.ranking_policy.partition_role;
    let mut report = StrategyTournamentReport {
        version: STRATEGY_TOURNAMENT_SCHEMA_VERSION.to_string(),
        tournament_id: input.tournament_id,
        strategy_kind: input.strategy_kind,
        package_digest: input.package_digest,
        partitions: input.partitions,
        ranking_policy: input.ranking_policy,
        summary: StrategyTournamentSummary {
            candidate_count: reports.len(),
            ranking_partition,
            ranking_metrics,
            holdout_evaluations,
            failed_candidates,
            decision: "recommend_review_no_auto_promotion".to_string(),
        },
        candidates: reports,
        content_hash: String::new(),
    };
    report.content_hash = report_hash(&report)?;
    Ok(report)
}

pub fn canonical_tournament_report_bytes(
    report: &StrategyTournamentReport,
) -> StrategyTournamentResult<Vec<u8>> {
    serde_json::to_vec(report).map_err(|error| {
        StrategyTournamentError::new(
            StrategyTournamentErrorKind::Serialization,
            format!("failed to serialize tournament report: {error}"),
        )
    })
}

fn report_hash(report: &StrategyTournamentReport) -> StrategyTournamentResult<String> {
    let mut canonical = report.clone();
    canonical.content_hash.clear();
    let bytes = canonical_tournament_report_bytes(&canonical)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

#[derive(Debug, Clone)]
struct ScoredCandidate {
    candidate: StrategyTournamentCandidate,
    ranking_eval: StrategyTournamentEvaluation,
    holdout_eval: Option<StrategyTournamentEvaluation>,
}

fn compare_scored(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    policy: &StrategyTournamentRankingPolicy,
) -> Ordering {
    let status = left
        .ranking_eval
        .status
        .rank()
        .cmp(&right.ranking_eval.status.rank());
    if status != Ordering::Equal {
        return status;
    }

    for rule in &policy.metric_order {
        let left_value = left.ranking_eval.metrics[&rule.metric];
        let right_value = right.ranking_eval.metrics[&rule.metric];
        let ordering = match rule.goal {
            TournamentMetricGoal::Maximize => right_value.cmp(&left_value),
            TournamentMetricGoal::Minimize => left_value.cmp(&right_value),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left.candidate
        .candidate_id
        .cmp(&right.candidate.candidate_id)
}

fn evaluation_for(
    evaluations: &BTreeMap<(String, TournamentPartitionRole), StrategyTournamentEvaluation>,
    candidate_id: &str,
    partition_role: TournamentPartitionRole,
) -> StrategyTournamentResult<StrategyTournamentEvaluation> {
    evaluations
        .get(&(candidate_id.to_string(), partition_role))
        .cloned()
        .ok_or_else(|| {
            StrategyTournamentError::new(
                StrategyTournamentErrorKind::MissingEvaluation,
                format!("candidate {candidate_id} is missing ranking partition evaluation"),
            )
        })
}

fn normalized_non_empty(value: &str, owner: &str, field: &str) -> StrategyTournamentResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(StrategyTournamentError::new(
            StrategyTournamentErrorKind::EmptyField,
            format!("{owner}.{field} cannot be empty"),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalized_digest(value: &str, owner: &str, field: &str) -> StrategyTournamentResult<String> {
    let digest = normalized_non_empty(value, owner, field)?;
    let Some(hex) = digest.strip_prefix("blake3:") else {
        return Err(StrategyTournamentError::new(
            StrategyTournamentErrorKind::InvalidDigest,
            format!("{owner}.{field} must be a blake3 digest"),
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StrategyTournamentError::new(
            StrategyTournamentErrorKind::InvalidDigest,
            format!("{owner}.{field} must be a 64-character blake3 hex digest"),
        ));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}
