#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_BASELINE_VERSION: &str = "canon.baseline.v1";
pub const CANON_BASELINE_COMPARE_VERSION: &str = "canon.baseline.compare.v1";

pub const REQUIRED_BASELINE_FIELDS: &[&str] = &[
    "schema_version",
    "baseline_id",
    "tool",
    "build",
    "hardware",
    "corpus",
    "privacy",
    "stages",
    "peak_memory",
    "io",
    "candidates",
    "evidence",
    "review",
    "quality",
    "cache",
    "comparison_bands",
    "baseline_digest",
];

pub const FORBIDDEN_BASELINE_PAYLOAD_KEYS: &[&str] = &[
    "raw_rows",
    "source_rows",
    "raw_identity_values",
    "identity_values",
    "candidate_values",
    "evidence_values",
    "operator_notes",
    "private_notes",
];

pub fn baseline_schema_version() -> &'static str {
    CANON_BASELINE_VERSION
}

pub fn required_baseline_fields() -> &'static [&'static str] {
    REQUIRED_BASELINE_FIELDS
}

pub fn forbidden_baseline_payload_keys() -> &'static [&'static str] {
    FORBIDDEN_BASELINE_PAYLOAD_KEYS
}

pub type BaselineResult<T> = Result<T, BaselineValidationError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineValidationError {
    pub field: String,
    pub message: String,
}

impl BaselineValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for BaselineValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl Error for BaselineValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClassification {
    PublicAggregate,
    RedactedPrivate,
    DigestOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonDirection {
    LowerIsBetter,
    HigherIsBetter,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricComparisonStatus {
    NoChange,
    Noise,
    Improvement,
    Warning,
    Regression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineComparisonStatus {
    NoChange,
    Improvement,
    Warning,
    Regression,
    Incomparable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineComparisonReasonKind {
    ToolChanged,
    BuildChanged,
    HardwareChanged,
    CorpusChanged,
    MetricMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineTool {
    pub canon_version: String,
    pub command_contract_digest: String,
    pub operator_contract_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineBuild {
    pub source_revision_digest: String,
    pub cargo_lock_digest: String,
    pub rustc_version_digest: String,
    pub target_triple: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineHardware {
    pub hardware_fingerprint_digest: String,
    pub os: String,
    pub arch: String,
    pub logical_cores: u64,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineCorpus {
    pub corpus_id: String,
    pub corpus_digest: String,
    pub split_digest: String,
    pub label_digest: String,
    pub row_count: u64,
    pub unique_surface_count: u64,
    pub license_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselinePrivacy {
    pub classification: PrivacyClassification,
    pub raw_identity_values_recorded: bool,
    pub raw_values_redacted: bool,
    pub redaction_policy_digest: String,
    pub redacted_field_count: u64,
    pub allowed_payload_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineStageMetric {
    pub stage_id: String,
    pub duration_ms: u64,
    pub artifact_bytes: u64,
    pub row_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselinePeakMemory {
    pub bytes: u64,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineIo {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineCandidateMetrics {
    pub input_surface_count: u64,
    pub candidate_pair_count: u64,
    pub suppressed_candidate_count: u64,
    pub exact_bucket_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineEvidenceMetrics {
    pub evidence_edge_count: u64,
    pub support_edge_count: u64,
    pub cannot_link_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineReviewMetrics {
    pub review_group_count: u64,
    pub accepted_decision_count: u64,
    pub rejected_decision_count: u64,
    pub pending_decision_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineQualityMetrics {
    pub candidate_recall_at_50_basis_points: u64,
    pub auto_link_precision_basis_points: u64,
    pub abstention_rate_basis_points: u64,
    pub severe_false_merge_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineCacheMetrics {
    pub cache_hit_count: u64,
    pub cache_miss_count: u64,
    pub cache_reused_bytes: u64,
    pub cache_written_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineComparisonBand {
    pub metric_id: String,
    pub direction: ComparisonDirection,
    pub noise_absolute: u64,
    pub warn_relative_basis_points: u64,
    pub fail_relative_basis_points: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineArtifact {
    pub schema_version: String,
    pub baseline_id: String,
    pub tool: BaselineTool,
    pub build: BaselineBuild,
    pub hardware: BaselineHardware,
    pub corpus: BaselineCorpus,
    pub privacy: BaselinePrivacy,
    pub stages: Vec<BaselineStageMetric>,
    pub peak_memory: BaselinePeakMemory,
    pub io: BaselineIo,
    pub candidates: BaselineCandidateMetrics,
    pub evidence: BaselineEvidenceMetrics,
    pub review: BaselineReviewMetrics,
    pub quality: BaselineQualityMetrics,
    pub cache: BaselineCacheMetrics,
    pub comparison_bands: Vec<BaselineComparisonBand>,
    pub baseline_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineComparisonReason {
    pub kind: BaselineComparisonReasonKind,
    pub field: String,
    pub baseline: String,
    pub candidate: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricComparison {
    pub metric_id: String,
    pub baseline_value: u64,
    pub candidate_value: u64,
    pub signed_delta: i128,
    pub relative_delta_basis_points: u64,
    pub direction: ComparisonDirection,
    pub status: MetricComparisonStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub schema_version: String,
    pub baseline_id: String,
    pub candidate_id: String,
    pub status: BaselineComparisonStatus,
    pub comparable: bool,
    pub reasons: Vec<BaselineComparisonReason>,
    pub metrics: Vec<MetricComparison>,
}

pub fn finalized_baseline(mut artifact: BaselineArtifact) -> BaselineResult<BaselineArtifact> {
    canonicalize_baseline(&mut artifact);
    artifact.baseline_digest.clear();
    artifact.baseline_digest = compute_baseline_digest(&artifact)?;
    validate_baseline(&artifact)?;
    Ok(artifact)
}

pub fn validate_baseline(artifact: &BaselineArtifact) -> BaselineResult<()> {
    let mut canonical = artifact.clone();
    canonicalize_baseline(&mut canonical);
    if canonical.schema_version != CANON_BASELINE_VERSION {
        return Err(BaselineValidationError::new(
            "schema_version",
            format!("must equal {CANON_BASELINE_VERSION}"),
        ));
    }
    require_non_empty("baseline_id", &canonical.baseline_id)?;
    validate_digest(
        "tool.command_contract_digest",
        &canonical.tool.command_contract_digest,
    )?;
    validate_digest(
        "tool.operator_contract_digest",
        &canonical.tool.operator_contract_digest,
    )?;
    validate_digest(
        "build.source_revision_digest",
        &canonical.build.source_revision_digest,
    )?;
    validate_digest(
        "build.cargo_lock_digest",
        &canonical.build.cargo_lock_digest,
    )?;
    validate_digest(
        "build.rustc_version_digest",
        &canonical.build.rustc_version_digest,
    )?;
    validate_digest(
        "hardware.hardware_fingerprint_digest",
        &canonical.hardware.hardware_fingerprint_digest,
    )?;
    validate_digest("corpus.corpus_digest", &canonical.corpus.corpus_digest)?;
    validate_digest("corpus.split_digest", &canonical.corpus.split_digest)?;
    validate_digest("corpus.label_digest", &canonical.corpus.label_digest)?;
    validate_digest(
        "privacy.redaction_policy_digest",
        &canonical.privacy.redaction_policy_digest,
    )?;
    if canonical.privacy.raw_identity_values_recorded {
        return Err(BaselineValidationError::new(
            "privacy.raw_identity_values_recorded",
            "privacy-safe baselines must not record raw identity values",
        ));
    }
    if !canonical.privacy.raw_values_redacted {
        return Err(BaselineValidationError::new(
            "privacy.raw_values_redacted",
            "baseline artifacts must be redacted by default",
        ));
    }
    if canonical.stages.is_empty() {
        return Err(BaselineValidationError::new(
            "stages",
            "at least one stage metric is required",
        ));
    }
    for stage in &canonical.stages {
        require_non_empty("stages.stage_id", &stage.stage_id)?;
    }
    for band in &canonical.comparison_bands {
        require_non_empty("comparison_bands.metric_id", &band.metric_id)?;
        if band.warn_relative_basis_points > band.fail_relative_basis_points {
            return Err(BaselineValidationError::new(
                format!("comparison_bands.{}", band.metric_id),
                "warn band must not exceed fail band",
            ));
        }
    }
    let expected_digest = compute_baseline_digest(&canonical)?;
    if canonical.baseline_digest != expected_digest {
        return Err(BaselineValidationError::new(
            "baseline_digest",
            format!(
                "digest mismatch: expected {expected_digest}, got {}",
                canonical.baseline_digest
            ),
        ));
    }
    Ok(())
}

pub fn canonical_baseline_bytes(artifact: &BaselineArtifact) -> BaselineResult<Vec<u8>> {
    validate_baseline(artifact)?;
    let mut canonical = artifact.clone();
    canonicalize_baseline(&mut canonical);
    serde_json::to_vec(&canonical).map_err(|error| {
        BaselineValidationError::new(
            "baseline",
            format!("failed to serialize canonical baseline: {error}"),
        )
    })
}

pub fn compare_baselines(
    baseline: &BaselineArtifact,
    candidate: &BaselineArtifact,
) -> BaselineResult<BaselineComparison> {
    validate_baseline(baseline)?;
    validate_baseline(candidate)?;

    let reasons = context_changes(baseline, candidate);
    if !reasons.is_empty() {
        return Ok(BaselineComparison {
            schema_version: CANON_BASELINE_COMPARE_VERSION.to_string(),
            baseline_id: baseline.baseline_id.clone(),
            candidate_id: candidate.baseline_id.clone(),
            status: BaselineComparisonStatus::Incomparable,
            comparable: false,
            reasons,
            metrics: Vec::new(),
        });
    }

    let baseline_values = metric_values(baseline);
    let candidate_values = metric_values(candidate);
    let mut metric_results = Vec::new();
    let mut missing_reasons = Vec::new();
    for band in &baseline.comparison_bands {
        let Some(&baseline_value) = baseline_values.get(&band.metric_id) else {
            missing_reasons.push(missing_metric_reason(
                &band.metric_id,
                baseline.baseline_id.as_str(),
                "baseline",
            ));
            continue;
        };
        let Some(&candidate_value) = candidate_values.get(&band.metric_id) else {
            missing_reasons.push(missing_metric_reason(
                &band.metric_id,
                candidate.baseline_id.as_str(),
                "candidate",
            ));
            continue;
        };
        metric_results.push(compare_metric(band, baseline_value, candidate_value));
    }
    if !missing_reasons.is_empty() {
        return Ok(BaselineComparison {
            schema_version: CANON_BASELINE_COMPARE_VERSION.to_string(),
            baseline_id: baseline.baseline_id.clone(),
            candidate_id: candidate.baseline_id.clone(),
            status: BaselineComparisonStatus::Incomparable,
            comparable: false,
            reasons: missing_reasons,
            metrics: metric_results,
        });
    }

    let status = aggregate_metric_status(&metric_results);
    Ok(BaselineComparison {
        schema_version: CANON_BASELINE_COMPARE_VERSION.to_string(),
        baseline_id: baseline.baseline_id.clone(),
        candidate_id: candidate.baseline_id.clone(),
        status,
        comparable: true,
        reasons: Vec::new(),
        metrics: metric_results,
    })
}

fn context_changes(
    baseline: &BaselineArtifact,
    candidate: &BaselineArtifact,
) -> Vec<BaselineComparisonReason> {
    let mut reasons = Vec::new();
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::ToolChanged,
        "tool.command_contract_digest",
        &baseline.tool.command_contract_digest,
        &candidate.tool.command_contract_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::ToolChanged,
        "tool.operator_contract_digest",
        &baseline.tool.operator_contract_digest,
        &candidate.tool.operator_contract_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::BuildChanged,
        "build.source_revision_digest",
        &baseline.build.source_revision_digest,
        &candidate.build.source_revision_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::BuildChanged,
        "build.cargo_lock_digest",
        &baseline.build.cargo_lock_digest,
        &candidate.build.cargo_lock_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::HardwareChanged,
        "hardware.hardware_fingerprint_digest",
        &baseline.hardware.hardware_fingerprint_digest,
        &candidate.hardware.hardware_fingerprint_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::CorpusChanged,
        "corpus.corpus_digest",
        &baseline.corpus.corpus_digest,
        &candidate.corpus.corpus_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::CorpusChanged,
        "corpus.split_digest",
        &baseline.corpus.split_digest,
        &candidate.corpus.split_digest,
    );
    push_change(
        &mut reasons,
        BaselineComparisonReasonKind::CorpusChanged,
        "corpus.label_digest",
        &baseline.corpus.label_digest,
        &candidate.corpus.label_digest,
    );
    reasons
}

fn push_change(
    reasons: &mut Vec<BaselineComparisonReason>,
    kind: BaselineComparisonReasonKind,
    field: &str,
    baseline: &str,
    candidate: &str,
) {
    if baseline != candidate {
        reasons.push(BaselineComparisonReason {
            kind,
            field: field.to_string(),
            baseline: baseline.to_string(),
            candidate: candidate.to_string(),
            message: format!("{field} changed; compare trends separately from regressions"),
        });
    }
}

fn metric_values(artifact: &BaselineArtifact) -> BTreeMap<String, u64> {
    let mut metrics = BTreeMap::new();
    for stage in &artifact.stages {
        metrics.insert(
            format!("stage.{}.duration_ms", stage.stage_id),
            stage.duration_ms,
        );
        metrics.insert(
            format!("stage.{}.artifact_bytes", stage.stage_id),
            stage.artifact_bytes,
        );
        metrics.insert(
            format!("stage.{}.row_count", stage.stage_id),
            stage.row_count,
        );
    }
    metrics.insert("memory.peak_bytes".to_string(), artifact.peak_memory.bytes);
    metrics.insert("io.read_bytes".to_string(), artifact.io.read_bytes);
    metrics.insert("io.write_bytes".to_string(), artifact.io.write_bytes);
    metrics.insert(
        "candidates.pair_count".to_string(),
        artifact.candidates.candidate_pair_count,
    );
    metrics.insert(
        "candidates.suppressed_count".to_string(),
        artifact.candidates.suppressed_candidate_count,
    );
    metrics.insert(
        "evidence.edge_count".to_string(),
        artifact.evidence.evidence_edge_count,
    );
    metrics.insert(
        "review.group_count".to_string(),
        artifact.review.review_group_count,
    );
    metrics.insert(
        "quality.candidate_recall_at_50_basis_points".to_string(),
        artifact.quality.candidate_recall_at_50_basis_points,
    );
    metrics.insert(
        "quality.auto_link_precision_basis_points".to_string(),
        artifact.quality.auto_link_precision_basis_points,
    );
    metrics.insert(
        "quality.abstention_rate_basis_points".to_string(),
        artifact.quality.abstention_rate_basis_points,
    );
    metrics.insert(
        "quality.severe_false_merge_count".to_string(),
        artifact.quality.severe_false_merge_count,
    );
    metrics.insert(
        "cache.hit_count".to_string(),
        artifact.cache.cache_hit_count,
    );
    metrics.insert(
        "cache.miss_count".to_string(),
        artifact.cache.cache_miss_count,
    );
    metrics
}

fn compare_metric(
    band: &BaselineComparisonBand,
    baseline_value: u64,
    candidate_value: u64,
) -> MetricComparison {
    let signed_delta = candidate_value as i128 - baseline_value as i128;
    let absolute_delta = signed_delta.unsigned_abs();
    let relative_delta_basis_points = relative_basis_points(absolute_delta, baseline_value);
    let status = if absolute_delta <= band.noise_absolute as u128 {
        MetricComparisonStatus::Noise
    } else {
        match band.direction {
            ComparisonDirection::Exact => {
                if signed_delta == 0 {
                    MetricComparisonStatus::NoChange
                } else {
                    MetricComparisonStatus::Regression
                }
            }
            ComparisonDirection::LowerIsBetter => directional_status(
                signed_delta > 0,
                signed_delta < 0,
                relative_delta_basis_points,
                band,
            ),
            ComparisonDirection::HigherIsBetter => directional_status(
                signed_delta < 0,
                signed_delta > 0,
                relative_delta_basis_points,
                band,
            ),
        }
    };
    MetricComparison {
        metric_id: band.metric_id.clone(),
        baseline_value,
        candidate_value,
        signed_delta,
        relative_delta_basis_points,
        direction: band.direction,
        status,
    }
}

fn directional_status(
    worse: bool,
    better: bool,
    relative_delta_basis_points: u64,
    band: &BaselineComparisonBand,
) -> MetricComparisonStatus {
    if worse {
        if relative_delta_basis_points >= band.fail_relative_basis_points {
            MetricComparisonStatus::Regression
        } else if relative_delta_basis_points >= band.warn_relative_basis_points {
            MetricComparisonStatus::Warning
        } else {
            MetricComparisonStatus::Noise
        }
    } else if better {
        MetricComparisonStatus::Improvement
    } else {
        MetricComparisonStatus::NoChange
    }
}

fn aggregate_metric_status(metrics: &[MetricComparison]) -> BaselineComparisonStatus {
    if metrics
        .iter()
        .any(|metric| metric.status == MetricComparisonStatus::Regression)
    {
        BaselineComparisonStatus::Regression
    } else if metrics
        .iter()
        .any(|metric| metric.status == MetricComparisonStatus::Warning)
    {
        BaselineComparisonStatus::Warning
    } else if metrics
        .iter()
        .any(|metric| metric.status == MetricComparisonStatus::Improvement)
    {
        BaselineComparisonStatus::Improvement
    } else {
        BaselineComparisonStatus::NoChange
    }
}

fn relative_basis_points(absolute_delta: u128, baseline_value: u64) -> u64 {
    if baseline_value == 0 {
        if absolute_delta == 0 { 0 } else { 10_000 }
    } else {
        ((absolute_delta * 10_000) / baseline_value as u128).min(u64::MAX as u128) as u64
    }
}

fn missing_metric_reason(
    metric_id: &str,
    artifact_id: &str,
    side: &str,
) -> BaselineComparisonReason {
    BaselineComparisonReason {
        kind: BaselineComparisonReasonKind::MetricMissing,
        field: metric_id.to_string(),
        baseline: artifact_id.to_string(),
        candidate: side.to_string(),
        message: format!("{side} artifact does not expose metric {metric_id}"),
    }
}

fn canonicalize_baseline(artifact: &mut BaselineArtifact) {
    artifact.stages.sort_by(|left, right| {
        left.stage_id
            .cmp(&right.stage_id)
            .then(left.duration_ms.cmp(&right.duration_ms))
    });
    artifact
        .comparison_bands
        .sort_by(|left, right| left.metric_id.cmp(&right.metric_id));
    artifact.privacy.allowed_payload_keys.sort();
    artifact.privacy.allowed_payload_keys.dedup();
}

fn compute_baseline_digest(artifact: &BaselineArtifact) -> BaselineResult<String> {
    let mut hashable = artifact.clone();
    canonicalize_baseline(&mut hashable);
    hashable.baseline_digest.clear();
    serde_json::to_vec(&hashable)
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .map_err(|error| {
            BaselineValidationError::new("baseline", format!("failed to hash baseline: {error}"))
        })
}

fn validate_digest(field: &str, value: &str) -> BaselineResult<()> {
    if value.len() == 71
        && value.starts_with("blake3:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(BaselineValidationError::new(
            field,
            "must be a blake3 digest",
        ))
    }
}

fn require_non_empty(field: &str, value: &str) -> BaselineResult<()> {
    if value.trim().is_empty() {
        Err(BaselineValidationError::new(field, "must not be empty"))
    } else {
        Ok(())
    }
}
