#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn evaluation_corpus_schema_version() -> &'static str {
    concat!("canon.evaluation.corpus", ".v1")
}

pub type EvaluationResult<T> = Result<T, EvaluationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationErrorCode {
    ArtifactContract,
    MissingReference,
    DuplicateRecord,
    PartitionLeakage,
    InconsistentLabel,
    LicenseGap,
    RedactionGap,
    CompatibilityPolicy,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationError {
    pub code: EvaluationErrorCode,
    pub message: String,
}

impl EvaluationError {
    pub fn new(code: EvaluationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for EvaluationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCorpus {
    pub version: String,
    pub corpus_id: String,
    pub corpus_version: String,
    pub provenance: CorpusProvenance,
    pub licenses: Vec<CorpusLicenseGrant>,
    pub redaction_classes: Vec<CorpusRedactionClass>,
    pub execution_policy: CorpusExecutionPolicy,
    pub datasets: Vec<CorpusDataset>,
    pub observations: Vec<CorpusObservation>,
    pub adjudications: Vec<AdjudicationRecord>,
    pub cluster_labels: Vec<ClusterLabel>,
    pub cross_dataset_pairs: Vec<CrossDatasetPair>,
    pub identifiers: Vec<IdentifierEvaluationLabel>,
    pub hard_negatives: Vec<HardNegativeLabel>,
    pub relationships: Vec<RelationshipLabel>,
    pub assignments: Vec<AssignmentLabel>,
    pub temporal_changes: Vec<TemporalChangeLabel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusPartition {
    Train,
    Tune,
    Holdout,
    ExactReplay,
}

impl CorpusPartition {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Tune => "tune",
            Self::Holdout => "holdout",
            Self::ExactReplay => "exact_replay",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetStorageKind {
    PublicFixture,
    PrivatePathRef,
    RemoteArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusLicenseGrant {
    pub license_id: String,
    pub license_expression: String,
    pub redistributable: bool,
    pub attribution_required: bool,
    pub usage_notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusRedactionClass {
    pub redaction_id: String,
    pub raw_content_retained: bool,
    pub export_surface_fingerprints_only: bool,
    pub private_path_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusExecutionPolicy {
    pub tuning_partitions: Vec<CorpusPartition>,
    pub scoring_partitions: Vec<CorpusPartition>,
    pub holdout_labels_sealed_from_tuning: bool,
    pub exact_replay_partition_separate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusDataset {
    pub dataset_id: String,
    pub partition: CorpusPartition,
    pub storage_kind: DatasetStorageKind,
    pub source_locator: String,
    pub content_digest: String,
    pub license_id: String,
    pub redaction_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusObservation {
    pub observation_id: String,
    pub dataset_id: String,
    pub subject_key: String,
    pub split_group_id: String,
    pub surface_fingerprint: String,
    pub locator: ObservationLocator,
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationLocatorKind {
    CsvRow,
    JsonPointer,
    OpaqueRecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationLocator {
    pub kind: ObservationLocatorKind,
    pub locator: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjudicationDecision {
    Accepted,
    Rejected,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjudicationRecord {
    pub adjudication_id: String,
    pub decision: AdjudicationDecision,
    pub confidence_basis_points: u16,
    pub reviewer_set_digest: String,
    pub note_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterLabel {
    pub cluster_id: String,
    pub observation_ids: Vec<String>,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairDisposition {
    SameEntity,
    DistinctEntity,
    RelatedEntity,
    Abstain,
}

impl PairDisposition {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SameEntity => "same_entity",
            Self::DistinctEntity => "distinct_entity",
            Self::RelatedEntity => "related_entity",
            Self::Abstain => "abstain",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossDatasetPair {
    pub pair_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub disposition: PairDisposition,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierEvaluationLabel {
    pub identifier_id: String,
    pub observation_id: String,
    pub namespace_id: String,
    pub value_fingerprint: String,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardNegativeSeverity {
    Low,
    High,
    Critical,
}

impl HardNegativeSeverity {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardNegativeLabel {
    pub hard_negative_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub severity: HardNegativeSeverity,
    pub reason_code: String,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipDisposition {
    Present,
    Absent,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipLabel {
    pub relationship_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub ontology_id: String,
    pub role_id: String,
    pub disposition: RelationshipDisposition,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentLabel {
    pub assignment_id: String,
    pub observation_id: String,
    pub assignee_key: String,
    pub ontology_id: String,
    pub role_id: String,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalDisposition {
    Stable,
    Changed,
    Ended,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalChangeLabel {
    pub change_id: String,
    pub previous_observation_id: String,
    pub next_observation_id: String,
    pub ontology_id: String,
    pub change_kind_id: String,
    pub disposition: TemporalDisposition,
    pub adjudication_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusProvenance {
    pub manifest_locator: String,
    pub source_snapshot_locator: String,
    pub source_snapshot_digest: String,
    pub generated_at: String,
    pub exact_replay_runner_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactReplayCoverage {
    pub dataset_count: usize,
    pub observation_count: usize,
    pub cluster_label_count: usize,
    pub pair_label_count: usize,
    pub hard_negative_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationMetricSnapshot {
    pub corpus_digest: String,
    pub observation_count: usize,
    pub cluster_label_count: usize,
    pub dataset_counts_by_partition: BTreeMap<String, usize>,
    pub pair_counts_by_disposition: BTreeMap<String, usize>,
    pub hard_negative_counts_by_severity: BTreeMap<String, usize>,
    pub relationship_counts_by_type: BTreeMap<String, usize>,
    pub assignment_counts_by_type: BTreeMap<String, usize>,
    pub temporal_change_counts_by_type: BTreeMap<String, usize>,
    pub adjudication_confidence_bands: BTreeMap<String, usize>,
    pub exact_replay_coverage: ExactReplayCoverage,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusQualityFixture {
    #[serde(flatten)]
    pub corpus: EvaluationCorpus,
    pub quality_harness: SealedCorpusQualityHarness,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusQualityHarness {
    pub labels_sealed: bool,
    pub cases: Vec<SealedCorpusQualityCase>,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryQualityStratum {
    ExactKnownReplay,
    WithheldAliasIncumbent,
    NovelMultiObservation,
    DirectionalCrossSource,
    RelatedOrHierarchyDistinct,
    GenuinelyUnresolved,
}

impl DiscoveryQualityStratum {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ExactKnownReplay => "exact_known_replay",
            Self::WithheldAliasIncumbent => "withheld_alias_incumbent",
            Self::NovelMultiObservation => "novel_multi_observation",
            Self::DirectionalCrossSource => "directional_cross_source",
            Self::RelatedOrHierarchyDistinct => "related_or_hierarchy_distinct",
            Self::GenuinelyUnresolved => "genuinely_unresolved",
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SealedCaseOutcome {
    Correct,
    Review,
    ExplicitRefusal,
    MeasuredMiss,
}

impl SealedCaseOutcome {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Review => "review",
            Self::ExplicitRefusal => "explicit_refusal",
            Self::MeasuredMiss => "measured_miss",
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissStage {
    CandidateGeneration,
    EvidenceScoring,
    Solver,
}

impl MissStage {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CandidateGeneration => "candidate_generation",
            Self::EvidenceScoring => "evidence_scoring",
            Self::Solver => "solver",
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusQualityCase {
    pub case_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub stratum: DiscoveryQualityStratum,
    pub label_disposition: PairDisposition,
    pub outcome: SealedCaseOutcome,
    pub miss_stage: Option<MissStage>,
    pub ablation_id: Option<String>,
    pub evidence_locator: String,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusQualityReport {
    pub version: String,
    pub corpus_id: String,
    pub corpus_version: String,
    pub corpus_digest: String,
    pub quality_harness_digest: String,
    pub labels_sealed: bool,
    pub discovery_case_count: usize,
    pub exact_replay_case_count: usize,
    pub correct_discovery_case_count: usize,
    pub discovery_success_basis_points: Option<u16>,
    pub outcome_counts: BTreeMap<String, usize>,
    pub stratum_counts: BTreeMap<String, usize>,
    pub miss_stage_counts: BTreeMap<String, usize>,
    pub miss_evidence: Vec<SealedCorpusMissEvidence>,
    pub ablation_evidence: Vec<SealedCorpusAblationEvidence>,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusMissEvidence {
    pub case_id: String,
    pub left_observation_id: String,
    pub right_observation_id: String,
    pub stratum: DiscoveryQualityStratum,
    pub miss_stage: MissStage,
    pub ablation_id: Option<String>,
    pub evidence_locator: String,
}

#[cfg_attr(test, allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedCorpusAblationEvidence {
    pub ablation_id: String,
    pub affected_case_count: usize,
    pub measured_miss_count: usize,
    pub case_ids: Vec<String>,
}

pub fn finalize_corpus(mut corpus: EvaluationCorpus) -> EvaluationResult<EvaluationCorpus> {
    if corpus.version.trim().is_empty() {
        corpus.version = evaluation_corpus_schema_version().to_string();
    }
    if corpus.version != evaluation_corpus_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported evaluation corpus version: {}",
            corpus.version
        )));
    }

    corpus.corpus_id = normalized_package_id(&corpus.corpus_id, "corpus_id")?;
    corpus.corpus_version = normalized_semver(&corpus.corpus_version, "corpus_version")?;
    corpus.provenance = normalize_provenance(corpus.provenance)?;
    corpus.execution_policy = normalize_execution_policy(corpus.execution_policy)?;

    corpus.licenses = dedupe_components(
        corpus
            .licenses
            .into_iter()
            .map(normalize_license)
            .collect::<EvaluationResult<Vec<_>>>()?,
        |license| license.license_id.clone(),
        "license",
    )?;
    if corpus.licenses.is_empty() {
        return Err(license_gap_error(
            "evaluation corpus must declare at least one license grant",
        ));
    }
    let license_map = corpus
        .licenses
        .iter()
        .map(|license| (license.license_id.clone(), license))
        .collect::<BTreeMap<_, _>>();

    corpus.redaction_classes = dedupe_components(
        corpus
            .redaction_classes
            .into_iter()
            .map(normalize_redaction_class)
            .collect::<EvaluationResult<Vec<_>>>()?,
        |redaction| redaction.redaction_id.clone(),
        "redaction class",
    )?;
    if corpus.redaction_classes.is_empty() {
        return Err(redaction_gap_error(
            "evaluation corpus must declare at least one redaction class",
        ));
    }
    let redaction_map = corpus
        .redaction_classes
        .iter()
        .map(|redaction| (redaction.redaction_id.clone(), redaction))
        .collect::<BTreeMap<_, _>>();

    corpus.datasets = dedupe_components(
        corpus
            .datasets
            .into_iter()
            .map(|dataset| normalize_dataset(dataset, &license_map, &redaction_map))
            .collect::<EvaluationResult<Vec<_>>>()?,
        |dataset| dataset.dataset_id.clone(),
        "dataset",
    )?;
    let dataset_map = corpus
        .datasets
        .iter()
        .map(|dataset| (dataset.dataset_id.clone(), dataset))
        .collect::<BTreeMap<_, _>>();
    validate_dataset_partitions(&corpus.execution_policy, &dataset_map)?;

    corpus.observations = dedupe_components(
        corpus
            .observations
            .into_iter()
            .map(|observation| normalize_observation(observation, &dataset_map))
            .collect::<EvaluationResult<Vec<_>>>()?,
        |observation| observation.observation_id.clone(),
        "observation",
    )?;
    let observation_map = corpus
        .observations
        .iter()
        .map(|observation| (observation.observation_id.clone(), observation))
        .collect::<BTreeMap<_, _>>();
    validate_partition_leakage(&observation_map, &dataset_map)?;

    corpus.adjudications = dedupe_components(
        corpus
            .adjudications
            .into_iter()
            .map(normalize_adjudication)
            .collect::<EvaluationResult<Vec<_>>>()?,
        |adjudication| adjudication.adjudication_id.clone(),
        "adjudication",
    )?;
    let adjudication_ids = corpus
        .adjudications
        .iter()
        .map(|adjudication| adjudication.adjudication_id.clone())
        .collect::<BTreeSet<_>>();

    corpus.cluster_labels = dedupe_components(
        corpus
            .cluster_labels
            .into_iter()
            .map(|cluster| normalize_cluster_label(cluster, &observation_map, &adjudication_ids))
            .collect::<EvaluationResult<Vec<_>>>()?,
        |cluster| cluster.cluster_id.clone(),
        "cluster label",
    )?;
    validate_cluster_membership(&corpus.cluster_labels)?;
    let mut positive_pairs = positive_pairs_from_clusters(&corpus.cluster_labels);

    corpus.cross_dataset_pairs = dedupe_components(
        corpus
            .cross_dataset_pairs
            .into_iter()
            .map(|pair| {
                normalize_cross_dataset_pair(
                    pair,
                    &observation_map,
                    &dataset_map,
                    &adjudication_ids,
                )
            })
            .collect::<EvaluationResult<Vec<_>>>()?,
        |pair| pair.pair_id.clone(),
        "cross dataset pair",
    )?;
    for pair in &corpus.cross_dataset_pairs {
        if pair.disposition == PairDisposition::SameEntity {
            positive_pairs.insert(pair_key(
                &pair.left_observation_id,
                &pair.right_observation_id,
            ));
        }
    }

    corpus.identifiers = dedupe_components(
        corpus
            .identifiers
            .into_iter()
            .map(|identifier| {
                normalize_identifier_label(identifier, &observation_map, &adjudication_ids)
            })
            .collect::<EvaluationResult<Vec<_>>>()?,
        |identifier| identifier.identifier_id.clone(),
        "identifier label",
    )?;

    corpus.relationships = dedupe_components(
        corpus
            .relationships
            .into_iter()
            .map(|relationship| {
                normalize_relationship_label(relationship, &observation_map, &adjudication_ids)
            })
            .collect::<EvaluationResult<Vec<_>>>()?,
        |relationship| relationship.relationship_id.clone(),
        "relationship label",
    )?;

    corpus.assignments = dedupe_components(
        corpus
            .assignments
            .into_iter()
            .map(|assignment| {
                normalize_assignment_label(assignment, &observation_map, &adjudication_ids)
            })
            .collect::<EvaluationResult<Vec<_>>>()?,
        |assignment| assignment.assignment_id.clone(),
        "assignment label",
    )?;

    corpus.temporal_changes = dedupe_components(
        corpus
            .temporal_changes
            .into_iter()
            .map(|change| normalize_temporal_change(change, &observation_map, &adjudication_ids))
            .collect::<EvaluationResult<Vec<_>>>()?,
        |change| change.change_id.clone(),
        "temporal change label",
    )?;

    corpus.hard_negatives = dedupe_components(
        corpus
            .hard_negatives
            .into_iter()
            .map(|hard_negative| {
                normalize_hard_negative(hard_negative, &observation_map, &adjudication_ids)
            })
            .collect::<EvaluationResult<Vec<_>>>()?,
        |hard_negative| hard_negative.hard_negative_id.clone(),
        "hard negative",
    )?;
    validate_hard_negatives(&corpus.hard_negatives, &positive_pairs)?;

    Ok(corpus)
}

pub fn canonical_corpus_bytes(corpus: &EvaluationCorpus) -> EvaluationResult<Vec<u8>> {
    let corpus = finalize_corpus(corpus.clone())?;
    serde_json::to_vec(&corpus)
        .map_err(|error| artifact_contract_error(format!("failed to serialize corpus: {error}")))
}

pub fn corpus_digest(corpus: &EvaluationCorpus) -> EvaluationResult<String> {
    let bytes = canonical_corpus_bytes(corpus)?;
    Ok(blake3_digest(&bytes))
}

pub fn deterministic_metrics(
    corpus: &EvaluationCorpus,
) -> EvaluationResult<EvaluationMetricSnapshot> {
    let corpus = finalize_corpus(corpus.clone())?;
    let corpus_digest = corpus_digest(&corpus)?;
    let dataset_map = corpus
        .datasets
        .iter()
        .map(|dataset| (dataset.dataset_id.clone(), dataset))
        .collect::<BTreeMap<_, _>>();
    let exact_replay_ids = corpus
        .observations
        .iter()
        .filter(|observation| {
            dataset_map
                .get(&observation.dataset_id)
                .is_some_and(|dataset| dataset.partition == CorpusPartition::ExactReplay)
        })
        .map(|observation| observation.observation_id.clone())
        .collect::<BTreeSet<_>>();

    let mut dataset_counts_by_partition = BTreeMap::new();
    for dataset in &corpus.datasets {
        increment_count(&mut dataset_counts_by_partition, dataset.partition.as_str());
    }

    let mut pair_counts_by_disposition = BTreeMap::new();
    for pair in &corpus.cross_dataset_pairs {
        increment_count(&mut pair_counts_by_disposition, pair.disposition.as_str());
    }

    let mut hard_negative_counts_by_severity = BTreeMap::new();
    for hard_negative in &corpus.hard_negatives {
        increment_count(
            &mut hard_negative_counts_by_severity,
            hard_negative.severity.as_str(),
        );
    }

    let mut relationship_counts_by_type = BTreeMap::new();
    for relationship in &corpus.relationships {
        increment_count(
            &mut relationship_counts_by_type,
            &typed_metric_key(&relationship.ontology_id, &relationship.role_id),
        );
    }

    let mut assignment_counts_by_type = BTreeMap::new();
    for assignment in &corpus.assignments {
        increment_count(
            &mut assignment_counts_by_type,
            &typed_metric_key(&assignment.ontology_id, &assignment.role_id),
        );
    }

    let mut temporal_change_counts_by_type = BTreeMap::new();
    for change in &corpus.temporal_changes {
        increment_count(
            &mut temporal_change_counts_by_type,
            &typed_metric_key(&change.ontology_id, &change.change_kind_id),
        );
    }

    let mut adjudication_confidence_bands = BTreeMap::new();
    for adjudication in &corpus.adjudications {
        increment_count(
            &mut adjudication_confidence_bands,
            confidence_band(adjudication.confidence_basis_points),
        );
    }

    let exact_replay_coverage = ExactReplayCoverage {
        dataset_count: corpus
            .datasets
            .iter()
            .filter(|dataset| dataset.partition == CorpusPartition::ExactReplay)
            .count(),
        observation_count: exact_replay_ids.len(),
        cluster_label_count: corpus
            .cluster_labels
            .iter()
            .filter(|cluster| {
                cluster
                    .observation_ids
                    .iter()
                    .any(|observation_id| exact_replay_ids.contains(observation_id))
            })
            .count(),
        pair_label_count: corpus
            .cross_dataset_pairs
            .iter()
            .filter(|pair| {
                exact_replay_ids.contains(&pair.left_observation_id)
                    || exact_replay_ids.contains(&pair.right_observation_id)
            })
            .count(),
        hard_negative_count: corpus
            .hard_negatives
            .iter()
            .filter(|hard_negative| {
                exact_replay_ids.contains(&hard_negative.left_observation_id)
                    || exact_replay_ids.contains(&hard_negative.right_observation_id)
            })
            .count(),
    };

    Ok(EvaluationMetricSnapshot {
        corpus_digest,
        observation_count: corpus.observations.len(),
        cluster_label_count: corpus.cluster_labels.len(),
        dataset_counts_by_partition,
        pair_counts_by_disposition,
        hard_negative_counts_by_severity,
        relationship_counts_by_type,
        assignment_counts_by_type,
        temporal_change_counts_by_type,
        adjudication_confidence_bands,
        exact_replay_coverage,
    })
}

#[cfg_attr(test, allow(dead_code))]
pub fn finalize_sealed_corpus_quality_fixture(
    fixture: SealedCorpusQualityFixture,
) -> EvaluationResult<SealedCorpusQualityFixture> {
    let corpus = finalize_corpus(fixture.corpus)?;
    let dataset_map = corpus
        .datasets
        .iter()
        .map(|dataset| (dataset.dataset_id.clone(), dataset))
        .collect::<BTreeMap<_, _>>();
    let observation_map = corpus
        .observations
        .iter()
        .map(|observation| (observation.observation_id.clone(), observation))
        .collect::<BTreeMap<_, _>>();
    let quality_harness =
        normalize_quality_harness(fixture.quality_harness, &observation_map, &dataset_map)?;
    Ok(SealedCorpusQualityFixture {
        corpus,
        quality_harness,
    })
}

#[cfg_attr(test, allow(dead_code))]
pub fn sealed_corpus_quality_report(
    fixture: &SealedCorpusQualityFixture,
) -> EvaluationResult<SealedCorpusQualityReport> {
    let fixture = finalize_sealed_corpus_quality_fixture(fixture.clone())?;
    let corpus_digest = corpus_digest(&fixture.corpus)?;
    let quality_harness_digest = harness_digest(&fixture.quality_harness)?;
    let mut outcome_counts = BTreeMap::new();
    let mut stratum_counts = BTreeMap::new();
    let mut miss_stage_counts = BTreeMap::new();
    let mut miss_evidence = Vec::new();
    let mut ablations = BTreeMap::<String, Vec<&SealedCorpusQualityCase>>::new();
    let mut discovery_case_count = 0usize;
    let mut exact_replay_case_count = 0usize;
    let mut correct_discovery_case_count = 0usize;

    for case in &fixture.quality_harness.cases {
        increment_count(&mut outcome_counts, case.outcome.as_str());
        increment_count(&mut stratum_counts, case.stratum.as_str());
        if case.stratum == DiscoveryQualityStratum::ExactKnownReplay {
            exact_replay_case_count += 1;
        } else {
            discovery_case_count += 1;
            if case.outcome == SealedCaseOutcome::Correct {
                correct_discovery_case_count += 1;
            }
        }
        if let Some(miss_stage) = case.miss_stage {
            increment_count(&mut miss_stage_counts, miss_stage.as_str());
        }
        if case.outcome == SealedCaseOutcome::MeasuredMiss {
            miss_evidence.push(SealedCorpusMissEvidence {
                case_id: case.case_id.clone(),
                left_observation_id: case.left_observation_id.clone(),
                right_observation_id: case.right_observation_id.clone(),
                stratum: case.stratum,
                miss_stage: case
                    .miss_stage
                    .expect("measured misses validated to include miss stage"),
                ablation_id: case.ablation_id.clone(),
                evidence_locator: case.evidence_locator.clone(),
            });
        }
        if let Some(ablation_id) = &case.ablation_id {
            ablations.entry(ablation_id.clone()).or_default().push(case);
        }
    }

    let mut ablation_evidence = ablations
        .into_iter()
        .map(|(ablation_id, mut cases)| {
            cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
            SealedCorpusAblationEvidence {
                ablation_id,
                affected_case_count: cases.len(),
                measured_miss_count: cases
                    .iter()
                    .filter(|case| case.outcome == SealedCaseOutcome::MeasuredMiss)
                    .count(),
                case_ids: cases.into_iter().map(|case| case.case_id.clone()).collect(),
            }
        })
        .collect::<Vec<_>>();
    ablation_evidence.sort_by(|left, right| left.ablation_id.cmp(&right.ablation_id));
    miss_evidence.sort_by(|left, right| left.case_id.cmp(&right.case_id));

    Ok(SealedCorpusQualityReport {
        version: "canon.evaluation.corpus.quality_report".to_string(),
        corpus_id: fixture.corpus.corpus_id.clone(),
        corpus_version: fixture.corpus.corpus_version.clone(),
        corpus_digest,
        quality_harness_digest,
        labels_sealed: fixture.quality_harness.labels_sealed,
        discovery_case_count,
        exact_replay_case_count,
        correct_discovery_case_count,
        discovery_success_basis_points: basis_points(
            correct_discovery_case_count,
            discovery_case_count,
        ),
        outcome_counts,
        stratum_counts,
        miss_stage_counts,
        miss_evidence,
        ablation_evidence,
    })
}

#[cfg_attr(test, allow(dead_code))]
pub fn canonical_sealed_corpus_quality_report_bytes(
    fixture: &SealedCorpusQualityFixture,
) -> EvaluationResult<Vec<u8>> {
    let report = sealed_corpus_quality_report(fixture)?;
    serde_json::to_vec(&report).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize sealed corpus quality report: {error}"
        ))
    })
}

fn normalize_license(mut license: CorpusLicenseGrant) -> EvaluationResult<CorpusLicenseGrant> {
    license.license_id = normalized_component_id(&license.license_id, "license.license_id")?;
    license.license_expression =
        normalized_non_empty(&license.license_expression, "license.license_expression")?;
    license.usage_notice = normalized_non_empty(&license.usage_notice, "license.usage_notice")?;
    Ok(license)
}

fn normalize_redaction_class(
    mut redaction: CorpusRedactionClass,
) -> EvaluationResult<CorpusRedactionClass> {
    redaction.redaction_id =
        normalized_component_id(&redaction.redaction_id, "redaction.redaction_id")?;
    if redaction.raw_content_retained && redaction.export_surface_fingerprints_only {
        return Err(redaction_gap_error(
            "redaction classes cannot retain raw content while claiming fingerprint-only export",
        ));
    }
    Ok(redaction)
}

fn normalize_execution_policy(
    mut policy: CorpusExecutionPolicy,
) -> EvaluationResult<CorpusExecutionPolicy> {
    policy.tuning_partitions = normalize_partition_list(policy.tuning_partitions);
    policy.scoring_partitions = normalize_partition_list(policy.scoring_partitions);

    let expected_tuning = [CorpusPartition::Train, CorpusPartition::Tune]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let expected_scoring = [CorpusPartition::Holdout, CorpusPartition::ExactReplay]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual_tuning = policy
        .tuning_partitions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_scoring = policy
        .scoring_partitions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    if actual_tuning != expected_tuning {
        return Err(compatibility_policy_error(
            "execution_policy.tuning_partitions must be exactly [train, tune]",
        ));
    }
    if actual_scoring != expected_scoring {
        return Err(compatibility_policy_error(
            "execution_policy.scoring_partitions must be exactly [holdout, exact_replay]",
        ));
    }
    if !policy.holdout_labels_sealed_from_tuning {
        return Err(compatibility_policy_error(
            "execution_policy.holdout_labels_sealed_from_tuning must remain true",
        ));
    }
    if !policy.exact_replay_partition_separate {
        return Err(compatibility_policy_error(
            "execution_policy.exact_replay_partition_separate must remain true",
        ));
    }

    Ok(policy)
}

fn normalize_dataset(
    mut dataset: CorpusDataset,
    license_map: &BTreeMap<String, &CorpusLicenseGrant>,
    redaction_map: &BTreeMap<String, &CorpusRedactionClass>,
) -> EvaluationResult<CorpusDataset> {
    dataset.dataset_id = normalized_component_id(&dataset.dataset_id, "dataset.dataset_id")?;
    dataset.source_locator =
        normalized_non_empty(&dataset.source_locator, "dataset.source_locator")?;
    dataset.content_digest = normalized_hash(&dataset.content_digest, "dataset.content_digest")?;
    dataset.license_id = normalized_component_id(&dataset.license_id, "dataset.license_id")?;
    dataset.redaction_id = normalized_component_id(&dataset.redaction_id, "dataset.redaction_id")?;

    let Some(license) = license_map.get(&dataset.license_id) else {
        return Err(license_gap_error(format!(
            "dataset {} references unknown license {}",
            dataset.dataset_id, dataset.license_id
        )));
    };
    let Some(redaction) = redaction_map.get(&dataset.redaction_id) else {
        return Err(redaction_gap_error(format!(
            "dataset {} references unknown redaction class {}",
            dataset.dataset_id, dataset.redaction_id
        )));
    };

    if dataset.storage_kind == DatasetStorageKind::PublicFixture && !license.redistributable {
        return Err(license_gap_error(format!(
            "public fixture dataset {} must use a redistributable license",
            dataset.dataset_id
        )));
    }
    if dataset.storage_kind == DatasetStorageKind::PrivatePathRef && !redaction.private_path_allowed
    {
        return Err(redaction_gap_error(format!(
            "private-path dataset {} requires a redaction class that explicitly allows private locators",
            dataset.dataset_id
        )));
    }

    Ok(dataset)
}

fn normalize_observation(
    mut observation: CorpusObservation,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
) -> EvaluationResult<CorpusObservation> {
    observation.observation_id =
        normalized_component_id(&observation.observation_id, "observation.observation_id")?;
    observation.dataset_id =
        normalized_component_id(&observation.dataset_id, "observation.dataset_id")?;
    if !dataset_map.contains_key(&observation.dataset_id) {
        return Err(missing_reference_error(format!(
            "observation {} references unknown dataset {}",
            observation.observation_id, observation.dataset_id
        )));
    }
    observation.subject_key =
        normalized_non_empty(&observation.subject_key, "observation.subject_key")?;
    observation.split_group_id =
        normalized_component_id(&observation.split_group_id, "observation.split_group_id")?;
    observation.surface_fingerprint = normalized_hash(
        &observation.surface_fingerprint,
        "observation.surface_fingerprint",
    )?;
    observation.locator = normalize_locator(observation.locator)?;
    observation.observed_at = observation
        .observed_at
        .map(|timestamp| normalized_non_empty(&timestamp, "observation.observed_at"))
        .transpose()?;

    Ok(observation)
}

fn normalize_locator(mut locator: ObservationLocator) -> EvaluationResult<ObservationLocator> {
    locator.locator = normalized_non_empty(&locator.locator, "observation.locator.locator")?;
    locator.content_digest = normalized_hash(
        &locator.content_digest,
        "observation.locator.content_digest",
    )?;
    Ok(locator)
}

fn normalize_adjudication(
    mut adjudication: AdjudicationRecord,
) -> EvaluationResult<AdjudicationRecord> {
    adjudication.adjudication_id = normalized_component_id(
        &adjudication.adjudication_id,
        "adjudication.adjudication_id",
    )?;
    if adjudication.confidence_basis_points > 10_000 {
        return Err(artifact_contract_error(
            "adjudication confidence_basis_points must be between 0 and 10000",
        ));
    }
    adjudication.reviewer_set_digest = normalized_hash(
        &adjudication.reviewer_set_digest,
        "adjudication.reviewer_set_digest",
    )?;
    adjudication.note_digest =
        normalized_hash(&adjudication.note_digest, "adjudication.note_digest")?;
    Ok(adjudication)
}

fn normalize_cluster_label(
    mut cluster: ClusterLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<ClusterLabel> {
    cluster.cluster_id = normalized_component_id(&cluster.cluster_id, "cluster.cluster_id")?;
    cluster.adjudication_ref =
        normalized_component_id(&cluster.adjudication_ref, "cluster.adjudication_ref")?;
    require_adjudication(
        &cluster.adjudication_ref,
        adjudication_ids,
        "cluster.adjudication_ref",
    )?;
    cluster.observation_ids =
        normalize_reference_list(cluster.observation_ids, "cluster.observation_ids")?;
    if cluster.observation_ids.is_empty() {
        return Err(artifact_contract_error(
            "cluster labels must include at least one observation_id",
        ));
    }
    for observation_id in &cluster.observation_ids {
        if !observation_map.contains_key(observation_id) {
            return Err(missing_reference_error(format!(
                "cluster {} references unknown observation {}",
                cluster.cluster_id, observation_id
            )));
        }
    }
    Ok(cluster)
}

fn normalize_cross_dataset_pair(
    mut pair: CrossDatasetPair,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<CrossDatasetPair> {
    pair.pair_id = normalized_component_id(&pair.pair_id, "pair.pair_id")?;
    pair.left_observation_id =
        normalized_component_id(&pair.left_observation_id, "pair.left_observation_id")?;
    pair.right_observation_id =
        normalized_component_id(&pair.right_observation_id, "pair.right_observation_id")?;
    pair.adjudication_ref =
        normalized_component_id(&pair.adjudication_ref, "pair.adjudication_ref")?;
    require_adjudication(
        &pair.adjudication_ref,
        adjudication_ids,
        "pair.adjudication_ref",
    )?;

    if pair.left_observation_id == pair.right_observation_id {
        return Err(artifact_contract_error(
            "cross_dataset_pairs must reference two different observations",
        ));
    }
    let left_observation = observation_map
        .get(&pair.left_observation_id)
        .ok_or_else(|| {
            missing_reference_error(format!(
                "cross dataset pair {} references unknown observation {}",
                pair.pair_id, pair.left_observation_id
            ))
        })?;
    let right_observation = observation_map
        .get(&pair.right_observation_id)
        .ok_or_else(|| {
            missing_reference_error(format!(
                "cross dataset pair {} references unknown observation {}",
                pair.pair_id, pair.right_observation_id
            ))
        })?;
    let left_dataset = dataset_map
        .get(&left_observation.dataset_id)
        .expect("observation dataset validated");
    let right_dataset = dataset_map
        .get(&right_observation.dataset_id)
        .expect("observation dataset validated");
    if left_dataset.dataset_id == right_dataset.dataset_id {
        return Err(artifact_contract_error(format!(
            "cross dataset pair {} must span distinct datasets",
            pair.pair_id
        )));
    }

    Ok(pair)
}

fn normalize_identifier_label(
    mut identifier: IdentifierEvaluationLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<IdentifierEvaluationLabel> {
    identifier.identifier_id =
        normalized_component_id(&identifier.identifier_id, "identifier.identifier_id")?;
    identifier.observation_id =
        normalized_component_id(&identifier.observation_id, "identifier.observation_id")?;
    if !observation_map.contains_key(&identifier.observation_id) {
        return Err(missing_reference_error(format!(
            "identifier {} references unknown observation {}",
            identifier.identifier_id, identifier.observation_id
        )));
    }
    identifier.namespace_id =
        normalized_component_id(&identifier.namespace_id, "identifier.namespace_id")?;
    identifier.value_fingerprint = normalized_hash(
        &identifier.value_fingerprint,
        "identifier.value_fingerprint",
    )?;
    identifier.adjudication_ref =
        normalized_component_id(&identifier.adjudication_ref, "identifier.adjudication_ref")?;
    require_adjudication(
        &identifier.adjudication_ref,
        adjudication_ids,
        "identifier.adjudication_ref",
    )?;
    Ok(identifier)
}

fn normalize_relationship_label(
    mut relationship: RelationshipLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<RelationshipLabel> {
    relationship.relationship_id = normalized_component_id(
        &relationship.relationship_id,
        "relationship.relationship_id",
    )?;
    relationship.left_observation_id = normalized_component_id(
        &relationship.left_observation_id,
        "relationship.left_observation_id",
    )?;
    relationship.right_observation_id = normalized_component_id(
        &relationship.right_observation_id,
        "relationship.right_observation_id",
    )?;
    if relationship.left_observation_id == relationship.right_observation_id {
        return Err(artifact_contract_error(
            "relationships must reference two different observations",
        ));
    }
    require_observation(
        &relationship.left_observation_id,
        observation_map,
        "relationship.left_observation_id",
    )?;
    require_observation(
        &relationship.right_observation_id,
        observation_map,
        "relationship.right_observation_id",
    )?;
    relationship.ontology_id =
        normalized_component_id(&relationship.ontology_id, "relationship.ontology_id")?;
    relationship.role_id = normalized_component_id(&relationship.role_id, "relationship.role_id")?;
    relationship.adjudication_ref = normalized_component_id(
        &relationship.adjudication_ref,
        "relationship.adjudication_ref",
    )?;
    require_adjudication(
        &relationship.adjudication_ref,
        adjudication_ids,
        "relationship.adjudication_ref",
    )?;
    Ok(relationship)
}

fn normalize_assignment_label(
    mut assignment: AssignmentLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<AssignmentLabel> {
    assignment.assignment_id =
        normalized_component_id(&assignment.assignment_id, "assignment.assignment_id")?;
    assignment.observation_id =
        normalized_component_id(&assignment.observation_id, "assignment.observation_id")?;
    require_observation(
        &assignment.observation_id,
        observation_map,
        "assignment.observation_id",
    )?;
    assignment.assignee_key =
        normalized_non_empty(&assignment.assignee_key, "assignment.assignee_key")?;
    assignment.ontology_id =
        normalized_component_id(&assignment.ontology_id, "assignment.ontology_id")?;
    assignment.role_id = normalized_component_id(&assignment.role_id, "assignment.role_id")?;
    assignment.adjudication_ref =
        normalized_component_id(&assignment.adjudication_ref, "assignment.adjudication_ref")?;
    require_adjudication(
        &assignment.adjudication_ref,
        adjudication_ids,
        "assignment.adjudication_ref",
    )?;
    Ok(assignment)
}

fn normalize_temporal_change(
    mut change: TemporalChangeLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<TemporalChangeLabel> {
    change.change_id = normalized_component_id(&change.change_id, "temporal.change_id")?;
    change.previous_observation_id = normalized_component_id(
        &change.previous_observation_id,
        "temporal.previous_observation_id",
    )?;
    change.next_observation_id =
        normalized_component_id(&change.next_observation_id, "temporal.next_observation_id")?;
    if change.previous_observation_id == change.next_observation_id {
        return Err(artifact_contract_error(
            "temporal changes must reference two different observations",
        ));
    }
    require_observation(
        &change.previous_observation_id,
        observation_map,
        "temporal.previous_observation_id",
    )?;
    require_observation(
        &change.next_observation_id,
        observation_map,
        "temporal.next_observation_id",
    )?;
    change.ontology_id = normalized_component_id(&change.ontology_id, "temporal.ontology_id")?;
    change.change_kind_id =
        normalized_component_id(&change.change_kind_id, "temporal.change_kind_id")?;
    change.adjudication_ref =
        normalized_component_id(&change.adjudication_ref, "temporal.adjudication_ref")?;
    require_adjudication(
        &change.adjudication_ref,
        adjudication_ids,
        "temporal.adjudication_ref",
    )?;
    Ok(change)
}

fn normalize_hard_negative(
    mut hard_negative: HardNegativeLabel,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    adjudication_ids: &BTreeSet<String>,
) -> EvaluationResult<HardNegativeLabel> {
    hard_negative.hard_negative_id = normalized_component_id(
        &hard_negative.hard_negative_id,
        "hard_negative.hard_negative_id",
    )?;
    hard_negative.left_observation_id = normalized_component_id(
        &hard_negative.left_observation_id,
        "hard_negative.left_observation_id",
    )?;
    hard_negative.right_observation_id = normalized_component_id(
        &hard_negative.right_observation_id,
        "hard_negative.right_observation_id",
    )?;
    if hard_negative.left_observation_id == hard_negative.right_observation_id {
        return Err(artifact_contract_error(
            "hard negatives must reference two different observations",
        ));
    }
    require_observation(
        &hard_negative.left_observation_id,
        observation_map,
        "hard_negative.left_observation_id",
    )?;
    require_observation(
        &hard_negative.right_observation_id,
        observation_map,
        "hard_negative.right_observation_id",
    )?;
    hard_negative.reason_code =
        normalized_component_id(&hard_negative.reason_code, "hard_negative.reason_code")?;
    hard_negative.adjudication_ref = normalized_component_id(
        &hard_negative.adjudication_ref,
        "hard_negative.adjudication_ref",
    )?;
    require_adjudication(
        &hard_negative.adjudication_ref,
        adjudication_ids,
        "hard_negative.adjudication_ref",
    )?;
    Ok(hard_negative)
}

fn validate_dataset_partitions(
    policy: &CorpusExecutionPolicy,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
) -> EvaluationResult<()> {
    let available = dataset_map
        .values()
        .map(|dataset| dataset.partition)
        .collect::<BTreeSet<_>>();
    for required in [
        CorpusPartition::Train,
        CorpusPartition::Tune,
        CorpusPartition::Holdout,
        CorpusPartition::ExactReplay,
    ] {
        if !available.contains(&required) {
            return Err(compatibility_policy_error(format!(
                "evaluation corpora must include at least one {:?} dataset",
                required
            )));
        }
    }

    let tuning = policy
        .tuning_partitions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let scoring = policy
        .scoring_partitions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if tuning.intersection(&scoring).next().is_some() {
        return Err(compatibility_policy_error(
            "tuning_partitions and scoring_partitions must remain disjoint",
        ));
    }

    Ok(())
}

fn validate_partition_leakage(
    observation_map: &BTreeMap<String, &CorpusObservation>,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
) -> EvaluationResult<()> {
    let mut groups = BTreeMap::<String, CorpusPartition>::new();
    for observation in observation_map.values() {
        let partition = dataset_map
            .get(&observation.dataset_id)
            .expect("observation dataset validated")
            .partition;
        match groups.insert(observation.split_group_id.clone(), partition) {
            Some(existing) if existing != partition => {
                return Err(partition_leakage_error(format!(
                    "split_group_id {} appears in both {} and {} partitions",
                    observation.split_group_id,
                    existing.as_str(),
                    partition.as_str()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_cluster_membership(clusters: &[ClusterLabel]) -> EvaluationResult<()> {
    let mut memberships = BTreeMap::<String, String>::new();
    for cluster in clusters {
        for observation_id in &cluster.observation_ids {
            if let Some(previous_cluster) =
                memberships.insert(observation_id.clone(), cluster.cluster_id.clone())
            {
                return Err(inconsistent_label_error(format!(
                    "observation {} appears in both cluster {} and {}",
                    observation_id, previous_cluster, cluster.cluster_id
                )));
            }
        }
    }
    Ok(())
}

fn positive_pairs_from_clusters(clusters: &[ClusterLabel]) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for cluster in clusters {
        for left_index in 0..cluster.observation_ids.len() {
            for right_index in (left_index + 1)..cluster.observation_ids.len() {
                pairs.insert(pair_key(
                    &cluster.observation_ids[left_index],
                    &cluster.observation_ids[right_index],
                ));
            }
        }
    }
    pairs
}

fn validate_hard_negatives(
    hard_negatives: &[HardNegativeLabel],
    positive_pairs: &BTreeSet<(String, String)>,
) -> EvaluationResult<()> {
    for hard_negative in hard_negatives {
        let key = pair_key(
            &hard_negative.left_observation_id,
            &hard_negative.right_observation_id,
        );
        if positive_pairs.contains(&key) {
            return Err(inconsistent_label_error(format!(
                "hard negative {} overlaps a positive same-entity label for pair {} <-> {}",
                hard_negative.hard_negative_id,
                hard_negative.left_observation_id,
                hard_negative.right_observation_id
            )));
        }
    }
    Ok(())
}

fn normalize_provenance(mut provenance: CorpusProvenance) -> EvaluationResult<CorpusProvenance> {
    provenance.manifest_locator =
        normalized_non_empty(&provenance.manifest_locator, "provenance.manifest_locator")?;
    provenance.source_snapshot_locator = normalized_non_empty(
        &provenance.source_snapshot_locator,
        "provenance.source_snapshot_locator",
    )?;
    provenance.source_snapshot_digest = normalized_hash(
        &provenance.source_snapshot_digest,
        "provenance.source_snapshot_digest",
    )?;
    provenance.generated_at =
        normalized_non_empty(&provenance.generated_at, "provenance.generated_at")?;
    provenance.exact_replay_runner_ref = normalized_non_empty(
        &provenance.exact_replay_runner_ref,
        "provenance.exact_replay_runner_ref",
    )?;
    Ok(provenance)
}

#[cfg_attr(test, allow(dead_code))]
fn normalize_quality_harness(
    mut harness: SealedCorpusQualityHarness,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
) -> EvaluationResult<SealedCorpusQualityHarness> {
    if !harness.labels_sealed {
        return Err(compatibility_policy_error(
            "quality_harness.labels_sealed must remain true",
        ));
    }
    harness.cases = dedupe_components(
        harness
            .cases
            .into_iter()
            .map(|case| normalize_quality_case(case, observation_map, dataset_map))
            .collect::<EvaluationResult<Vec<_>>>()?,
        |case| case.case_id.clone(),
        "quality harness case",
    )?;
    if harness.cases.is_empty() {
        return Err(artifact_contract_error(
            "quality_harness.cases must include at least one sealed scoring case",
        ));
    }
    Ok(harness)
}

#[cfg_attr(test, allow(dead_code))]
fn normalize_quality_case(
    mut case: SealedCorpusQualityCase,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    dataset_map: &BTreeMap<String, &CorpusDataset>,
) -> EvaluationResult<SealedCorpusQualityCase> {
    case.case_id = normalized_component_id(&case.case_id, "quality_harness.case_id")?;
    case.left_observation_id = normalized_component_id(
        &case.left_observation_id,
        "quality_harness.left_observation_id",
    )?;
    case.right_observation_id = normalized_component_id(
        &case.right_observation_id,
        "quality_harness.right_observation_id",
    )?;
    if case.left_observation_id == case.right_observation_id {
        return Err(artifact_contract_error(
            "quality_harness cases must reference two different observations",
        ));
    }
    require_observation(
        &case.left_observation_id,
        observation_map,
        "quality_harness.left_observation_id",
    )?;
    require_observation(
        &case.right_observation_id,
        observation_map,
        "quality_harness.right_observation_id",
    )?;
    if case.label_disposition != PairDisposition::SameEntity {
        return Err(compatibility_policy_error(
            "quality_harness currently supports only same_entity labels",
        ));
    }
    case.evidence_locator =
        normalized_non_empty(&case.evidence_locator, "quality_harness.evidence_locator")?;
    case.ablation_id = case
        .ablation_id
        .map(|ablation_id| normalized_component_id(&ablation_id, "quality_harness.ablation_id"))
        .transpose()?;

    match case.outcome {
        SealedCaseOutcome::MeasuredMiss if case.miss_stage.is_none() => {
            return Err(artifact_contract_error(
                "quality_harness measured_miss cases must declare miss_stage",
            ));
        }
        outcome if outcome != SealedCaseOutcome::MeasuredMiss && case.miss_stage.is_some() => {
            return Err(artifact_contract_error(
                "quality_harness miss_stage is only allowed on measured_miss cases",
            ));
        }
        _ => {}
    }

    let left_dataset = dataset_map
        .get(
            &observation_map
                .get(&case.left_observation_id)
                .expect("left observation validated")
                .dataset_id,
        )
        .expect("left dataset validated");
    let right_dataset = dataset_map
        .get(
            &observation_map
                .get(&case.right_observation_id)
                .expect("right observation validated")
                .dataset_id,
        )
        .expect("right dataset validated");
    if left_dataset.partition != right_dataset.partition {
        return Err(compatibility_policy_error(format!(
            "quality_harness case {} must stay within one scoring partition",
            case.case_id
        )));
    }
    match case.stratum {
        DiscoveryQualityStratum::ExactKnownReplay => {
            if left_dataset.partition != CorpusPartition::ExactReplay {
                return Err(compatibility_policy_error(format!(
                    "quality_harness exact replay case {} must use exact_replay observations",
                    case.case_id
                )));
            }
        }
        _ => {
            if left_dataset.partition != CorpusPartition::Holdout {
                return Err(compatibility_policy_error(format!(
                    "quality_harness discovery case {} must use holdout observations",
                    case.case_id
                )));
            }
        }
    }
    Ok(case)
}

fn require_observation(
    observation_id: &str,
    observation_map: &BTreeMap<String, &CorpusObservation>,
    field: &str,
) -> EvaluationResult<()> {
    if observation_map.contains_key(observation_id) {
        Ok(())
    } else {
        Err(missing_reference_error(format!(
            "{field} references unknown observation {observation_id}"
        )))
    }
}

fn require_adjudication(
    adjudication_ref: &str,
    adjudication_ids: &BTreeSet<String>,
    field: &str,
) -> EvaluationResult<()> {
    if adjudication_ids.contains(adjudication_ref) {
        Ok(())
    } else {
        Err(missing_reference_error(format!(
            "{field} references unknown adjudication {adjudication_ref}"
        )))
    }
}

fn normalize_reference_list(values: Vec<String>, field: &str) -> EvaluationResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_component_id(&value, field))
        .collect::<EvaluationResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_partition_list(mut partitions: Vec<CorpusPartition>) -> Vec<CorpusPartition> {
    partitions.sort();
    partitions.dedup();
    partitions
}

fn typed_metric_key(ontology_id: &str, role_or_kind: &str) -> String {
    format!("{ontology_id}::{role_or_kind}")
}

fn confidence_band(confidence_basis_points: u16) -> &'static str {
    match confidence_basis_points {
        0..=4_999 => "low",
        5_000..=8_999 => "medium",
        _ => "high",
    }
}

fn pair_key(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn increment_count(map: &mut BTreeMap<String, usize>, key: &str) {
    *map.entry(key.to_string()).or_insert(0) += 1;
}

fn dedupe_components<T, F>(mut values: Vec<T>, key: F, label: &str) -> EvaluationResult<Vec<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    values.sort_by_key(|value| key(value));
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.last()
            && key(previous) == key(&value)
        {
            if previous != &value {
                return Err(duplicate_record_error(format!(
                    "{label} {} was declared with conflicting content",
                    key(&value)
                )));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn normalized_package_id(value: &str, field: &str) -> EvaluationResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use lowercase [a-z0-9._-] characters"
    )))
}

fn normalized_component_id(value: &str, field: &str) -> EvaluationResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'.' | b'_' | b'-' | b':')
    }) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use lowercase [a-z0-9._:-] characters"
    )))
}

fn normalized_semver(value: &str, field: &str) -> EvaluationResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use MAJOR.MINOR.PATCH numeric semver"
    )))
}

fn normalized_hash(value: &str, field: &str) -> EvaluationResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64
        && hex
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^blake3:[0-9a-f]{{64}}$"
    )))
}

fn normalized_non_empty(value: &str, field: &str) -> EvaluationResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    }
    Ok(value)
}

fn artifact_contract_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::ArtifactContract, message)
}

fn missing_reference_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::MissingReference, message)
}

fn duplicate_record_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::DuplicateRecord, message)
}

fn partition_leakage_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::PartitionLeakage, message)
}

fn inconsistent_label_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::InconsistentLabel, message)
}

fn license_gap_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::LicenseGap, message)
}

fn redaction_gap_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::RedactionGap, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> EvaluationError {
    EvaluationError::new(EvaluationErrorCode::CompatibilityPolicy, message)
}

fn blake3_digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[cfg_attr(test, allow(dead_code))]
fn harness_digest(harness: &SealedCorpusQualityHarness) -> EvaluationResult<String> {
    let bytes = serde_json::to_vec(harness).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize sealed corpus quality harness: {error}"
        ))
    })?;
    Ok(blake3_digest(&bytes))
}

#[cfg_attr(test, allow(dead_code))]
fn basis_points(numerator: usize, denominator: usize) -> Option<u16> {
    if denominator == 0 {
        return None;
    }
    let scaled = (numerator * 10_000 + (denominator / 2)) / denominator;
    u16::try_from(scaled).ok()
}
