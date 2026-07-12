#![forbid(unsafe_code)]

//! Entity-disjoint and time-forward discovery benchmark contract.
//!
//! This module validates sealed benchmark inputs and compiles deterministic
//! reports for two generalization families:
//! - entity-disjoint trials, where each canonical entity appears exclusively in
//!   tune or holdout observations;
//! - time-forward trials, where build inputs are strictly before the cutoff and
//!   evaluation inputs are strictly after it.
//!
//! Core logic is intentionally domain-neutral. It tracks opaque observation and
//! entity identifiers, rejects holdout/future leakage, refuses severity-critical
//! false merges, and reports stratified results without interpreting domain facts.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_GENERALIZATION_VERSION: &str = "canon.evaluation.generalization.v1";

pub type GeneralizationResult<T> = Result<T, GeneralizationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationErrorCode {
    ArtifactContract,
    MissingReference,
    DuplicateRecord,
    EntityDisjointLeak,
    FutureLeakage,
    TemporalReversal,
    CriticalFalseMerge,
    DirectionalLinkContract,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationError {
    pub code: GeneralizationErrorCode,
    pub message: String,
}

impl GeneralizationError {
    pub fn new(code: GeneralizationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for GeneralizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for GeneralizationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusVisibility {
    PublicFixture,
    PrivateCorpusRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkPartition {
    Tune,
    Holdout,
    Build,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFamily {
    Reference,
    Target,
    PublicFixture,
    PrivateReference,
    PrivateTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetRole {
    Reference,
    Target,
    SingleSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAvailability {
    NameOnly,
    SurfaceAndIdentifier,
    RichEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameDifficulty {
    Easy,
    PunctuationCase,
    LegalSuffix,
    Rename,
    Lookalike,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFrequency {
    Head,
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationClass {
    SameEntity,
    RelatedDistinct,
    Hierarchy,
    Lookalike,
    RenameContinuity,
    ChangedRelationship,
    NewEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyBand {
    Easy,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryDecision {
    AttachExisting,
    ClusterNew,
    Abstain,
    Reject,
    FalseMerge,
}

impl DiscoveryDecision {
    const fn is_abstention(self) -> bool {
        matches!(self, Self::Abstain)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    PromoteCluster,
    PromoteLink,
    DeferReview,
    RejectCandidate,
    RecordCannotLink,
    NoAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakChannel {
    Alias,
    Anchor,
    Threshold,
    Dictionary,
    Patch,
    Cache,
    GeneratedCorpus,
}

impl LeakChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alias => "alias",
            Self::Anchor => "anchor",
            Self::Threshold => "threshold",
            Self::Dictionary => "dictionary",
            Self::Patch => "patch",
            Self::Cache => "cache",
            Self::GeneratedCorpus => "generated_corpus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectedSet {
    HoldoutEntity,
    FutureObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationObservation {
    pub observation_id: String,
    pub canonical_entity_id: String,
    pub dataset_id: String,
    pub dataset_role: DatasetRole,
    pub partition: BenchmarkPartition,
    pub observed_at: String,
    pub surface: String,
    pub evidence_availability: EvidenceAvailability,
    pub source_family: SourceFamily,
    pub name_difficulty: NameDifficulty,
    pub entity_frequency: EntityFrequency,
    pub relation_class: RelationClass,
    pub difficulty_band: DifficultyBand,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceLaneSummary {
    pub lane_id: String,
    pub available: bool,
    pub support_basis_points: u16,
    pub contradiction_basis_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DiscoveryResultRecord {
    pub result_id: String,
    pub observation_ids: Vec<String>,
    pub expected_decision: DiscoveryDecision,
    pub actual_decision: DiscoveryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
    pub evidence_lanes: Vec<EvidenceLaneSummary>,
    pub review_action: ReviewAction,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HardNegativeControl {
    pub control_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub relation_class: RelationClass,
    pub severity: Severity,
    pub false_merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DirectionalCrossSourceLink {
    pub link_id: String,
    pub reference_observation_id: String,
    pub target_observation_id: String,
    pub reference_dataset_id: String,
    pub target_dataset_id: String,
    pub expected_decision: DiscoveryDecision,
    pub actual_decision: DiscoveryDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LeakageProbe {
    pub channel: LeakChannel,
    pub protected_set: ProtectedSet,
    pub locator: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityDisjointTrial {
    pub trial_id: String,
    pub observations: Vec<GeneralizationObservation>,
    pub discovery_results: Vec<DiscoveryResultRecord>,
    pub hard_negatives: Vec<HardNegativeControl>,
    pub directional_links: Vec<DirectionalCrossSourceLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_probes: Vec<LeakageProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TimeForwardTrial {
    pub trial_id: String,
    pub cutoff: String,
    pub observations: Vec<GeneralizationObservation>,
    pub build_observation_ids: Vec<String>,
    pub evaluation_observation_ids: Vec<String>,
    pub event_results: Vec<DiscoveryResultRecord>,
    pub hard_negatives: Vec<HardNegativeControl>,
    pub directional_links: Vec<DirectionalCrossSourceLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leakage_probes: Vec<LeakageProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationBenchmark {
    pub version: String,
    pub benchmark_id: String,
    pub corpus_visibility: CorpusVisibility,
    pub corpus_ref: String,
    pub policy_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_disjoint_trials: Vec<EntityDisjointTrial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub time_forward_trials: Vec<TimeForwardTrial>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationStratumKey {
    pub evidence_availability: EvidenceAvailability,
    pub source_family: SourceFamily,
    pub name_difficulty: NameDifficulty,
    pub entity_frequency: EntityFrequency,
    pub relation_class: RelationClass,
    pub difficulty_band: DifficultyBand,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralizationStratumReport {
    pub key: GeneralizationStratumKey,
    pub result_count: usize,
    pub correct_count: usize,
    pub abstain_count: usize,
    pub false_merge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityDisjointTrialReport {
    pub trial_id: String,
    pub clean_snapshot_digest: String,
    pub protected_holdout_digest: String,
    pub novel_cluster_result_count: usize,
    pub correct_novel_cluster_count: usize,
    pub related_distinct_hard_negative_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeForwardTrialReport {
    pub trial_id: String,
    pub cutoff: String,
    pub build_snapshot_digest: String,
    pub protected_future_digest: String,
    pub evaluation_result_count: usize,
    pub correct_evaluation_count: usize,
    pub renamed_surface_count: usize,
    pub new_entity_count: usize,
    pub changed_relationship_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationAggregate {
    pub entity_disjoint_trial_count: usize,
    pub time_forward_trial_count: usize,
    pub result_count: usize,
    pub correct_count: usize,
    pub abstain_count: usize,
    pub critical_false_merge_count: usize,
    pub directional_cross_source_count: usize,
    pub head_result_count: usize,
    pub tail_result_count: usize,
    pub easy_result_count: usize,
    pub hard_result_count: usize,
    pub strata: Vec<GeneralizationStratumReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralizationReport {
    pub version: String,
    pub benchmark_id: String,
    pub corpus_visibility: CorpusVisibility,
    pub corpus_ref: String,
    pub benchmark_digest: String,
    pub report_digest: String,
    pub entity_disjoint: Vec<EntityDisjointTrialReport>,
    pub time_forward: Vec<TimeForwardTrialReport>,
    pub aggregate: GeneralizationAggregate,
}

pub fn generalization_schema_version() -> &'static str {
    CANON_GENERALIZATION_VERSION
}

pub fn compile_generalization_benchmark(
    benchmark: GeneralizationBenchmark,
) -> GeneralizationResult<GeneralizationReport> {
    let benchmark = finalize_benchmark(benchmark)?;
    let benchmark_digest = generalization_benchmark_digest(&benchmark)?;

    let mut entity_reports = Vec::with_capacity(benchmark.entity_disjoint_trials.len());
    for trial in &benchmark.entity_disjoint_trials {
        entity_reports.push(compile_entity_disjoint_trial(trial)?);
    }
    entity_reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let mut time_reports = Vec::with_capacity(benchmark.time_forward_trials.len());
    for trial in &benchmark.time_forward_trials {
        time_reports.push(compile_time_forward_trial(trial)?);
    }
    time_reports.sort_by(|left, right| left.trial_id.cmp(&right.trial_id));

    let aggregate = aggregate_reports(&entity_reports, &time_reports);
    let mut report = GeneralizationReport {
        version: CANON_GENERALIZATION_VERSION.to_string(),
        benchmark_id: benchmark.benchmark_id,
        corpus_visibility: benchmark.corpus_visibility,
        corpus_ref: benchmark.corpus_ref,
        benchmark_digest,
        report_digest: String::new(),
        entity_disjoint: entity_reports,
        time_forward: time_reports,
        aggregate,
    };
    report.report_digest = generalization_report_digest(&report)?;
    Ok(report)
}

pub fn compile_entity_disjoint_trial(
    trial: &EntityDisjointTrial,
) -> GeneralizationResult<EntityDisjointTrialReport> {
    let trial = canonicalize_entity_disjoint_trial(trial.clone())?;
    validate_entity_disjoint_split(&trial)?;
    validate_discovery_result_refs(&trial.discovery_results, &trial.observations)?;
    validate_hard_negatives(&trial.hard_negatives, &trial.observations)?;
    validate_directional_links(&trial.directional_links, &trial.observations)?;

    let holdout_protected =
        protected_values_for_partition(&trial.observations, BenchmarkPartition::Holdout);
    refuse_leakage(
        &trial.leakage_probes,
        ProtectedSet::HoldoutEntity,
        &holdout_protected,
        &trial.trial_id,
    )?;

    let tune_observations = trial
        .observations
        .iter()
        .filter(|observation| observation.partition == BenchmarkPartition::Tune)
        .cloned()
        .collect::<Vec<_>>();
    let holdout_observations = trial
        .observations
        .iter()
        .filter(|observation| observation.partition == BenchmarkPartition::Holdout)
        .cloned()
        .collect::<Vec<_>>();

    let strata = strata_for_results(&trial.discovery_results, &trial.observations)?;
    let novel_results = trial
        .discovery_results
        .iter()
        .filter(|result| result.expected_decision == DiscoveryDecision::ClusterNew)
        .collect::<Vec<_>>();
    let correct_novel_cluster_count = novel_results
        .iter()
        .filter(|result| result.actual_decision == result.expected_decision)
        .count();
    let critical_false_merge_count = critical_false_merges(&trial.hard_negatives);
    if critical_false_merge_count > 0 {
        return Err(critical_false_merge_error(&trial.trial_id));
    }

    Ok(EntityDisjointTrialReport {
        trial_id: trial.trial_id,
        clean_snapshot_digest: hash_serialized(&tune_observations)?,
        protected_holdout_digest: hash_serialized(&holdout_observations)?,
        novel_cluster_result_count: novel_results.len(),
        correct_novel_cluster_count,
        related_distinct_hard_negative_count: trial
            .hard_negatives
            .iter()
            .filter(|control| {
                matches!(
                    control.relation_class,
                    RelationClass::RelatedDistinct
                        | RelationClass::Hierarchy
                        | RelationClass::Lookalike
                )
            })
            .count(),
        critical_false_merge_count,
        directional_cross_source_count: trial.directional_links.len(),
        strata,
    })
}

pub fn compile_time_forward_trial(
    trial: &TimeForwardTrial,
) -> GeneralizationResult<TimeForwardTrialReport> {
    let trial = canonicalize_time_forward_trial(trial.clone())?;
    let observations_by_id = observations_by_id(&trial.observations)?;
    validate_time_forward_cutoff(&trial, &observations_by_id)?;
    validate_discovery_result_refs(&trial.event_results, &trial.observations)?;
    validate_hard_negatives(&trial.hard_negatives, &trial.observations)?;
    validate_directional_links(&trial.directional_links, &trial.observations)?;

    let future_protected = trial
        .evaluation_observation_ids
        .iter()
        .filter_map(|id| observations_by_id.get(id))
        .flat_map(protected_values_for_observation)
        .collect::<BTreeSet<_>>();
    refuse_leakage(
        &trial.leakage_probes,
        ProtectedSet::FutureObservation,
        &future_protected,
        &trial.trial_id,
    )?;

    let build_observations = trial
        .build_observation_ids
        .iter()
        .map(|id| {
            observations_by_id
                .get(id)
                .expect("build refs validated")
                .clone()
        })
        .collect::<Vec<_>>();
    let eval_observations = trial
        .evaluation_observation_ids
        .iter()
        .map(|id| {
            observations_by_id
                .get(id)
                .expect("eval refs validated")
                .clone()
        })
        .collect::<Vec<_>>();

    let strata = strata_for_results(&trial.event_results, &trial.observations)?;
    let correct_evaluation_count = trial
        .event_results
        .iter()
        .filter(|result| result.actual_decision == result.expected_decision)
        .count();
    let critical_false_merge_count = critical_false_merges(&trial.hard_negatives);
    if critical_false_merge_count > 0 {
        return Err(critical_false_merge_error(&trial.trial_id));
    }

    Ok(TimeForwardTrialReport {
        trial_id: trial.trial_id,
        cutoff: trial.cutoff,
        build_snapshot_digest: hash_serialized(&build_observations)?,
        protected_future_digest: hash_serialized(&eval_observations)?,
        evaluation_result_count: trial.event_results.len(),
        correct_evaluation_count,
        renamed_surface_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::RenameContinuity,
        )?,
        new_entity_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::NewEntity,
        )?,
        changed_relationship_count: count_results_by_relation(
            &trial.event_results,
            &trial.observations,
            RelationClass::ChangedRelationship,
        )?,
        critical_false_merge_count,
        directional_cross_source_count: trial.directional_links.len(),
        strata,
    })
}

pub fn canonical_benchmark_bytes(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<Vec<u8>> {
    let benchmark = finalize_benchmark(benchmark.clone())?;
    serde_json::to_vec(&benchmark).map_err(artifact_error)
}

pub fn canonical_report_bytes(report: &GeneralizationReport) -> GeneralizationResult<Vec<u8>> {
    let mut report = report.clone();
    report
        .entity_disjoint
        .sort_by(|left, right| left.trial_id.cmp(&right.trial_id));
    report
        .time_forward
        .sort_by(|left, right| left.trial_id.cmp(&right.trial_id));
    report.aggregate.strata.sort();
    serde_json::to_vec(&report).map_err(artifact_error)
}

pub fn generalization_benchmark_digest(
    benchmark: &GeneralizationBenchmark,
) -> GeneralizationResult<String> {
    Ok(hash_bytes(&canonical_benchmark_bytes(benchmark)?))
}

pub fn generalization_report_digest(report: &GeneralizationReport) -> GeneralizationResult<String> {
    let mut report = report.clone();
    report.report_digest.clear();
    Ok(hash_bytes(&canonical_report_bytes(&report)?))
}

pub fn finalize_benchmark(
    mut benchmark: GeneralizationBenchmark,
) -> GeneralizationResult<GeneralizationBenchmark> {
    if ascii_trim(&benchmark.version).is_empty() {
        benchmark.version = CANON_GENERALIZATION_VERSION.to_string();
    }
    if benchmark.version != CANON_GENERALIZATION_VERSION {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("unsupported generalization version {}", benchmark.version),
        ));
    }
    benchmark.benchmark_id = normalize_non_empty(benchmark.benchmark_id, "benchmark_id")?;
    benchmark.corpus_ref = normalize_non_empty(benchmark.corpus_ref, "corpus_ref")?;
    benchmark.policy_digest = normalize_digest(benchmark.policy_digest, "policy_digest")?;
    benchmark.entity_disjoint_trials = benchmark
        .entity_disjoint_trials
        .into_iter()
        .map(canonicalize_entity_disjoint_trial)
        .collect::<GeneralizationResult<Vec<_>>>()?;
    benchmark.entity_disjoint_trials.sort();
    benchmark.entity_disjoint_trials = dedup_or_conflict(
        benchmark.entity_disjoint_trials,
        |trial| trial.trial_id.clone(),
        "entity_disjoint_trial",
    )?;
    benchmark.time_forward_trials = benchmark
        .time_forward_trials
        .into_iter()
        .map(canonicalize_time_forward_trial)
        .collect::<GeneralizationResult<Vec<_>>>()?;
    benchmark.time_forward_trials.sort();
    benchmark.time_forward_trials = dedup_or_conflict(
        benchmark.time_forward_trials,
        |trial| trial.trial_id.clone(),
        "time_forward_trial",
    )?;
    if benchmark.entity_disjoint_trials.is_empty() && benchmark.time_forward_trials.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            "generalization benchmark must include at least one trial",
        ));
    }
    Ok(benchmark)
}

fn canonicalize_entity_disjoint_trial(
    mut trial: EntityDisjointTrial,
) -> GeneralizationResult<EntityDisjointTrial> {
    trial.trial_id = normalize_non_empty(trial.trial_id, "trial_id")?;
    trial.observations = normalize_observations(trial.observations)?;
    for observation in &trial.observations {
        if !matches!(
            observation.partition,
            BenchmarkPartition::Tune | BenchmarkPartition::Holdout
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "entity-disjoint observation {} must be tune or holdout",
                    observation.observation_id
                ),
            ));
        }
    }
    trial.discovery_results = normalize_discovery_results(trial.discovery_results)?;
    trial.hard_negatives = normalize_hard_negatives(trial.hard_negatives)?;
    trial.directional_links = normalize_directional_links(trial.directional_links)?;
    trial.leakage_probes = normalize_leakage_probes(trial.leakage_probes)?;
    Ok(trial)
}

fn canonicalize_time_forward_trial(
    mut trial: TimeForwardTrial,
) -> GeneralizationResult<TimeForwardTrial> {
    trial.trial_id = normalize_non_empty(trial.trial_id, "trial_id")?;
    trial.cutoff = normalize_non_empty(trial.cutoff, "cutoff")?;
    trial.observations = normalize_observations(trial.observations)?;
    for observation in &trial.observations {
        if !matches!(
            observation.partition,
            BenchmarkPartition::Build | BenchmarkPartition::Evaluation
        ) {
            return Err(error(
                GeneralizationErrorCode::ArtifactContract,
                format!(
                    "time-forward observation {} must be build or evaluation",
                    observation.observation_id
                ),
            ));
        }
    }
    trial.build_observation_ids =
        normalize_string_vec(trial.build_observation_ids, "build_observation_id")?;
    trial.evaluation_observation_ids = normalize_string_vec(
        trial.evaluation_observation_ids,
        "evaluation_observation_id",
    )?;
    trial.event_results = normalize_discovery_results(trial.event_results)?;
    trial.hard_negatives = normalize_hard_negatives(trial.hard_negatives)?;
    trial.directional_links = normalize_directional_links(trial.directional_links)?;
    trial.leakage_probes = normalize_leakage_probes(trial.leakage_probes)?;
    Ok(trial)
}

fn normalize_observations(
    observations: Vec<GeneralizationObservation>,
) -> GeneralizationResult<Vec<GeneralizationObservation>> {
    let mut observations = observations
        .into_iter()
        .map(|mut observation| {
            observation.observation_id =
                normalize_non_empty(observation.observation_id, "observation_id")?;
            observation.canonical_entity_id =
                normalize_non_empty(observation.canonical_entity_id, "canonical_entity_id")?;
            observation.dataset_id = normalize_non_empty(observation.dataset_id, "dataset_id")?;
            observation.observed_at = normalize_non_empty(observation.observed_at, "observed_at")?;
            observation.surface = normalize_non_empty(observation.surface, "surface")?;
            Ok(observation)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    observations.sort();
    dedup_or_conflict(
        observations,
        |observation| observation.observation_id.clone(),
        "observation",
    )
}

fn normalize_discovery_results(
    results: Vec<DiscoveryResultRecord>,
) -> GeneralizationResult<Vec<DiscoveryResultRecord>> {
    let mut results = results
        .into_iter()
        .map(|mut result| {
            result.result_id = normalize_non_empty(result.result_id, "result_id")?;
            result.observation_ids =
                normalize_string_vec(result.observation_ids, "result_observation_id")?;
            if result.observation_ids.is_empty() {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!(
                        "result {} must reference at least one observation",
                        result.result_id
                    ),
                ));
            }
            if result.candidate_rank == Some(0) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("result {} candidate_rank is 1-based", result.result_id),
                ));
            }
            result.evidence_lanes = normalize_evidence_lanes(result.evidence_lanes)?;
            Ok(result)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    results.sort();
    dedup_or_conflict(
        results,
        |result| result.result_id.clone(),
        "discovery_result",
    )
}

fn normalize_evidence_lanes(
    lanes: Vec<EvidenceLaneSummary>,
) -> GeneralizationResult<Vec<EvidenceLaneSummary>> {
    let mut lanes = lanes
        .into_iter()
        .map(|mut lane| {
            lane.lane_id = normalize_non_empty(lane.lane_id, "lane_id")?;
            Ok(lane)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    lanes.sort();
    dedup_or_conflict(lanes, |lane| lane.lane_id.clone(), "evidence_lane")
}

fn normalize_hard_negatives(
    controls: Vec<HardNegativeControl>,
) -> GeneralizationResult<Vec<HardNegativeControl>> {
    let mut controls = controls
        .into_iter()
        .map(|mut control| {
            control.control_id = normalize_non_empty(control.control_id, "control_id")?;
            control.left_observation_id =
                normalize_non_empty(control.left_observation_id, "left_observation_id")?;
            control.right_observation_id =
                normalize_non_empty(control.right_observation_id, "right_observation_id")?;
            Ok(control)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    controls.sort();
    dedup_or_conflict(
        controls,
        |control| control.control_id.clone(),
        "hard_negative",
    )
}

fn normalize_directional_links(
    links: Vec<DirectionalCrossSourceLink>,
) -> GeneralizationResult<Vec<DirectionalCrossSourceLink>> {
    let mut links = links
        .into_iter()
        .map(|mut link| {
            link.link_id = normalize_non_empty(link.link_id, "link_id")?;
            link.reference_observation_id =
                normalize_non_empty(link.reference_observation_id, "reference_observation_id")?;
            link.target_observation_id =
                normalize_non_empty(link.target_observation_id, "target_observation_id")?;
            link.reference_dataset_id =
                normalize_non_empty(link.reference_dataset_id, "reference_dataset_id")?;
            link.target_dataset_id =
                normalize_non_empty(link.target_dataset_id, "target_dataset_id")?;
            if link.candidate_rank == Some(0) {
                return Err(error(
                    GeneralizationErrorCode::ArtifactContract,
                    format!("link {} candidate_rank is 1-based", link.link_id),
                ));
            }
            Ok(link)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    links.sort();
    dedup_or_conflict(links, |link| link.link_id.clone(), "directional_link")
}

fn normalize_leakage_probes(probes: Vec<LeakageProbe>) -> GeneralizationResult<Vec<LeakageProbe>> {
    let mut probes = probes
        .into_iter()
        .map(|mut probe| {
            probe.locator = normalize_non_empty(probe.locator, "leak_locator")?;
            probe.value = normalize_non_empty(probe.value, "leak_value")?;
            Ok(probe)
        })
        .collect::<GeneralizationResult<Vec<_>>>()?;
    probes.sort();
    probes.dedup();
    Ok(probes)
}

fn validate_entity_disjoint_split(trial: &EntityDisjointTrial) -> GeneralizationResult<()> {
    let mut partitions_by_entity = BTreeMap::<String, BTreeSet<BenchmarkPartition>>::new();
    for observation in &trial.observations {
        partitions_by_entity
            .entry(observation.canonical_entity_id.clone())
            .or_default()
            .insert(observation.partition);
    }
    for (entity_id, partitions) in partitions_by_entity {
        if partitions.contains(&BenchmarkPartition::Tune)
            && partitions.contains(&BenchmarkPartition::Holdout)
        {
            return Err(error(
                GeneralizationErrorCode::EntityDisjointLeak,
                format!("entity {entity_id} appears in both tune and holdout"),
            ));
        }
    }
    Ok(())
}

fn validate_time_forward_cutoff(
    trial: &TimeForwardTrial,
    observations_by_id: &BTreeMap<String, GeneralizationObservation>,
) -> GeneralizationResult<()> {
    if trial.build_observation_ids.is_empty() || trial.evaluation_observation_ids.is_empty() {
        return Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!(
                "time-forward trial {} needs build and evaluation observations",
                trial.trial_id
            ),
        ));
    }

    for id in &trial.build_observation_ids {
        let observation = observations_by_id.get(id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!("build observation {id} is missing in {}", trial.trial_id),
            )
        })?;
        if observation.partition != BenchmarkPartition::Build
            || observation.observed_at >= trial.cutoff
        {
            return Err(error(
                GeneralizationErrorCode::TemporalReversal,
                format!(
                    "build observation {} is not strictly before cutoff {}",
                    observation.observation_id, trial.cutoff
                ),
            ));
        }
    }
    for id in &trial.evaluation_observation_ids {
        let observation = observations_by_id.get(id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "evaluation observation {id} is missing in {}",
                    trial.trial_id
                ),
            )
        })?;
        if observation.partition != BenchmarkPartition::Evaluation
            || observation.observed_at <= trial.cutoff
        {
            return Err(error(
                GeneralizationErrorCode::TemporalReversal,
                format!(
                    "evaluation observation {} is not strictly after cutoff {}",
                    observation.observation_id, trial.cutoff
                ),
            ));
        }
    }
    Ok(())
}

fn validate_discovery_result_refs(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let ids = observations
        .iter()
        .map(|observation| observation.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    for result in results {
        for observation_id in &result.observation_ids {
            if !ids.contains(observation_id.as_str()) {
                return Err(error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "result {} references missing observation {}",
                        result.result_id, observation_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_hard_negatives(
    controls: &[HardNegativeControl],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let ids = observations
        .iter()
        .map(|observation| observation.observation_id.as_str())
        .collect::<BTreeSet<_>>();
    for control in controls {
        if !ids.contains(control.left_observation_id.as_str())
            || !ids.contains(control.right_observation_id.as_str())
        {
            return Err(error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "hard negative {} references missing observation",
                    control.control_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_directional_links(
    links: &[DirectionalCrossSourceLink],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<()> {
    let by_id = observations_by_id(observations)?;
    for link in links {
        let reference = by_id.get(&link.reference_observation_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "directional link {} references missing reference observation",
                    link.link_id
                ),
            )
        })?;
        let target = by_id.get(&link.target_observation_id).ok_or_else(|| {
            error(
                GeneralizationErrorCode::MissingReference,
                format!(
                    "directional link {} references missing target observation",
                    link.link_id
                ),
            )
        })?;
        if link.reference_dataset_id != reference.dataset_id
            || link.target_dataset_id != target.dataset_id
            || link.reference_dataset_id == link.target_dataset_id
            || reference.dataset_role == target.dataset_role
        {
            return Err(error(
                GeneralizationErrorCode::DirectionalLinkContract,
                format!(
                    "directional link {} must connect different reference and target datasets",
                    link.link_id
                ),
            ));
        }
    }
    Ok(())
}

fn refuse_leakage(
    probes: &[LeakageProbe],
    protected_set: ProtectedSet,
    protected_values: &BTreeSet<String>,
    trial_id: &str,
) -> GeneralizationResult<()> {
    let protected_fingerprints = protected_values
        .iter()
        .map(|value| hash_bytes(ascii_trim(value).as_bytes()))
        .collect::<BTreeSet<_>>();
    for probe in probes
        .iter()
        .filter(|probe| probe.protected_set == protected_set)
    {
        let value = ascii_trim(&probe.value);
        if protected_values.contains(value) || protected_fingerprints.contains(value) {
            return Err(error(
                match protected_set {
                    ProtectedSet::HoldoutEntity => GeneralizationErrorCode::EntityDisjointLeak,
                    ProtectedSet::FutureObservation => GeneralizationErrorCode::FutureLeakage,
                },
                format!(
                    "{} leaked protected {:?} value in {} at {}",
                    probe.channel.as_str(),
                    protected_set,
                    trial_id,
                    probe.locator
                ),
            ));
        }
    }
    Ok(())
}

fn strata_for_results(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<Vec<GeneralizationStratumReport>> {
    let observations_by_id = observations_by_id(observations)?;
    let mut strata = BTreeMap::<GeneralizationStratumKey, GeneralizationStratumReport>::new();
    for result in results {
        for observation_id in &result.observation_ids {
            let observation = observations_by_id.get(observation_id).ok_or_else(|| {
                error(
                    GeneralizationErrorCode::MissingReference,
                    format!(
                        "result {} references missing observation {}",
                        result.result_id, observation_id
                    ),
                )
            })?;
            let key = GeneralizationStratumKey {
                evidence_availability: observation.evidence_availability,
                source_family: observation.source_family,
                name_difficulty: observation.name_difficulty,
                entity_frequency: observation.entity_frequency,
                relation_class: observation.relation_class,
                difficulty_band: observation.difficulty_band,
            };
            let entry = strata
                .entry(key.clone())
                .or_insert_with(|| GeneralizationStratumReport {
                    key,
                    result_count: 0,
                    correct_count: 0,
                    abstain_count: 0,
                    false_merge_count: 0,
                });
            entry.result_count += 1;
            if result.actual_decision == result.expected_decision {
                entry.correct_count += 1;
            }
            if result.actual_decision.is_abstention() {
                entry.abstain_count += 1;
            }
            if result.actual_decision == DiscoveryDecision::FalseMerge {
                entry.false_merge_count += 1;
            }
        }
    }
    Ok(strata.into_values().collect())
}

fn aggregate_reports(
    entity_reports: &[EntityDisjointTrialReport],
    time_reports: &[TimeForwardTrialReport],
) -> GeneralizationAggregate {
    let mut strata = BTreeMap::<GeneralizationStratumKey, GeneralizationStratumReport>::new();
    for report in entity_reports {
        merge_strata(&mut strata, &report.strata);
    }
    for report in time_reports {
        merge_strata(&mut strata, &report.strata);
    }

    let result_count = strata.values().map(|entry| entry.result_count).sum();
    let correct_count = strata.values().map(|entry| entry.correct_count).sum();
    let abstain_count = strata.values().map(|entry| entry.abstain_count).sum();
    let head_result_count = strata
        .values()
        .filter(|entry| entry.key.entity_frequency == EntityFrequency::Head)
        .map(|entry| entry.result_count)
        .sum();
    let tail_result_count = strata
        .values()
        .filter(|entry| entry.key.entity_frequency == EntityFrequency::Tail)
        .map(|entry| entry.result_count)
        .sum();
    let easy_result_count = strata
        .values()
        .filter(|entry| entry.key.difficulty_band == DifficultyBand::Easy)
        .map(|entry| entry.result_count)
        .sum();
    let hard_result_count = strata
        .values()
        .filter(|entry| entry.key.difficulty_band == DifficultyBand::Hard)
        .map(|entry| entry.result_count)
        .sum();

    GeneralizationAggregate {
        entity_disjoint_trial_count: entity_reports.len(),
        time_forward_trial_count: time_reports.len(),
        result_count,
        correct_count,
        abstain_count,
        critical_false_merge_count: entity_reports
            .iter()
            .map(|report| report.critical_false_merge_count)
            .sum::<usize>()
            + time_reports
                .iter()
                .map(|report| report.critical_false_merge_count)
                .sum::<usize>(),
        directional_cross_source_count: entity_reports
            .iter()
            .map(|report| report.directional_cross_source_count)
            .sum::<usize>()
            + time_reports
                .iter()
                .map(|report| report.directional_cross_source_count)
                .sum::<usize>(),
        head_result_count,
        tail_result_count,
        easy_result_count,
        hard_result_count,
        strata: strata.into_values().collect(),
    }
}

fn merge_strata(
    target: &mut BTreeMap<GeneralizationStratumKey, GeneralizationStratumReport>,
    source: &[GeneralizationStratumReport],
) {
    for source_entry in source {
        let entry =
            target
                .entry(source_entry.key.clone())
                .or_insert_with(|| GeneralizationStratumReport {
                    key: source_entry.key.clone(),
                    result_count: 0,
                    correct_count: 0,
                    abstain_count: 0,
                    false_merge_count: 0,
                });
        entry.result_count += source_entry.result_count;
        entry.correct_count += source_entry.correct_count;
        entry.abstain_count += source_entry.abstain_count;
        entry.false_merge_count += source_entry.false_merge_count;
    }
}

fn count_results_by_relation(
    results: &[DiscoveryResultRecord],
    observations: &[GeneralizationObservation],
    relation_class: RelationClass,
) -> GeneralizationResult<usize> {
    let observations_by_id = observations_by_id(observations)?;
    let mut count = 0usize;
    for result in results {
        if result.observation_ids.iter().any(|id| {
            observations_by_id
                .get(id)
                .map(|observation| observation.relation_class == relation_class)
                .unwrap_or(false)
        }) {
            count += 1;
        }
    }
    Ok(count)
}

fn critical_false_merges(controls: &[HardNegativeControl]) -> usize {
    controls
        .iter()
        .filter(|control| control.severity == Severity::Critical && control.false_merge)
        .count()
}

fn critical_false_merge_error(trial_id: &str) -> GeneralizationError {
    error(
        GeneralizationErrorCode::CriticalFalseMerge,
        format!("trial {trial_id} has severity-critical false merges"),
    )
}

fn observations_by_id(
    observations: &[GeneralizationObservation],
) -> GeneralizationResult<BTreeMap<String, GeneralizationObservation>> {
    let mut by_id = BTreeMap::new();
    for observation in observations {
        if by_id
            .insert(observation.observation_id.clone(), observation.clone())
            .is_some()
        {
            return Err(error(
                GeneralizationErrorCode::DuplicateRecord,
                format!("duplicate observation {}", observation.observation_id),
            ));
        }
    }
    Ok(by_id)
}

fn protected_values_for_partition(
    observations: &[GeneralizationObservation],
    partition: BenchmarkPartition,
) -> BTreeSet<String> {
    observations
        .iter()
        .filter(|observation| observation.partition == partition)
        .flat_map(protected_values_for_observation)
        .collect()
}

fn protected_values_for_observation(observation: &GeneralizationObservation) -> Vec<String> {
    vec![
        observation.observation_id.clone(),
        observation.canonical_entity_id.clone(),
        observation.surface.clone(),
    ]
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> GeneralizationResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalize_non_empty(value, field))
        .collect::<GeneralizationResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_digest(value: String, field: &str) -> GeneralizationResult<String> {
    let value = normalize_non_empty(value, field)?;
    if is_blake3_digest(&value) {
        Ok(value)
    } else {
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must be a lowercase blake3 digest"),
        ))
    }
}

fn normalize_non_empty(value: String, field: &str) -> GeneralizationResult<String> {
    let trimmed = ascii_trim(&value).to_string();
    if trimmed.is_empty() {
        Err(error(
            GeneralizationErrorCode::ArtifactContract,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(trimmed)
    }
}

fn is_blake3_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hash_serialized<T: Serialize>(value: &T) -> GeneralizationResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| hash_bytes(&bytes))
        .map_err(artifact_error)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn dedup_or_conflict<T, K>(
    values: Vec<T>,
    key: impl Fn(&T) -> K,
    label: &str,
) -> GeneralizationResult<Vec<T>>
where
    T: Clone + PartialEq,
    K: Ord + fmt::Debug,
{
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.iter().find(|previous| key(previous) == key(&value)) {
            if previous != &value {
                return Err(error(
                    GeneralizationErrorCode::DuplicateRecord,
                    format!(
                        "duplicate {label} {:?} has conflicting content",
                        key(&value)
                    ),
                ));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn artifact_error(error: serde_json::Error) -> GeneralizationError {
    GeneralizationError::new(GeneralizationErrorCode::ArtifactContract, error.to_string())
}

fn error(code: GeneralizationErrorCode, message: impl Into<String>) -> GeneralizationError {
    GeneralizationError::new(code, message)
}
