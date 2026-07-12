#![forbid(unsafe_code)]

//! Privacy-safe accretion metrics for unresolved inbox and review flywheel work.
//!
//! These metrics are count/digest artifacts. They report whether registry,
//! provider, review, and temporal changes improved coverage without recording
//! raw identity values or making new identity assertions.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const CANON_ACCRETION_METRICS_VERSION: &str = "canon.accretion.metrics.v1";
pub const ACCRETION_METRICS_IDENTITY_STATUS: &str = "metrics_only_no_identity_assertion";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionMetricsInput {
    pub report_id: String,
    pub generated_at: String,
    pub privacy: AccretionPrivacyPolicy,
    pub baseline: AccretionSnapshotInput,
    pub comparison: AccretionSnapshotInput,
    pub attribution: CoverageGainAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionPrivacyPolicy {
    pub classification: String,
    pub redaction: String,
    pub raw_identity_values_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionSnapshotInput {
    pub snapshot_id: String,
    pub corpus: FrozenCorpusRef,
    pub registry: RegistrySnapshotRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_receipt_digest: Option<String>,
    pub totals: AccretionSnapshotTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenCorpusRef {
    pub corpus_id: String,
    pub corpus_digest: String,
    pub policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySnapshotRef {
    pub registry_id: String,
    pub registry_version: String,
    pub registry_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AccretionSnapshotTotals {
    pub total_inputs: u64,
    pub resolved_inputs: u64,
    pub unresolved_groups: u64,
    pub ambiguous_groups: u64,
    pub contradiction_groups: u64,
    pub reviewed_groups: u64,
    pub promoted_groups: u64,
    pub hard_negative_regressions: u64,
    pub repeat_unresolved_after_promotion: u64,
    pub temporal_effect_groups: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CoverageGainAttribution {
    pub new_aliases: u64,
    pub new_entities: u64,
    pub remaps: u64,
    pub provider_additions: u64,
    pub temporal_effects: u64,
    pub policy_changes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionMetricsReport {
    pub version: String,
    pub report_content_hash: String,
    pub identity_status: String,
    pub report_id: String,
    pub generated_at: String,
    pub privacy: AccretionPrivacyPolicy,
    pub comparison_basis: ComparisonBasis,
    pub baseline: AccretionSnapshotMetrics,
    pub comparison: AccretionSnapshotMetrics,
    pub deltas: AccretionDeltas,
    pub attribution: CoverageGainAttribution,
    pub attribution_total: u64,
    pub unattributed_resolved_delta: i64,
    #[serde(default)]
    pub warning_codes: Vec<String>,
    pub summary: AccretionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonBasis {
    pub like_for_like: bool,
    pub same_corpus_digest: bool,
    pub same_policy_digest: bool,
    pub baseline_corpus_digest: String,
    pub comparison_corpus_digest: String,
    pub baseline_policy_digest: String,
    pub comparison_policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionSnapshotMetrics {
    pub snapshot_id: String,
    pub corpus: FrozenCorpusRef,
    pub registry: RegistrySnapshotRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_receipt_digest: Option<String>,
    pub totals: AccretionSnapshotTotals,
    pub exact_coverage: RatioMetric,
    pub unresolved_group_rate: RatioMetric,
    pub ambiguity_rate: RatioMetric,
    pub contradiction_rate: RatioMetric,
    pub review_yield: RatioMetric,
    pub repeat_unresolved_rate: RatioMetric,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatioMetric {
    pub numerator: u64,
    pub denominator: u64,
    pub basis_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionDeltas {
    pub resolved_inputs: i64,
    pub exact_coverage_basis_points: i64,
    pub unresolved_groups: i64,
    pub unresolved_group_rate_basis_points: i64,
    pub ambiguous_groups: i64,
    pub ambiguity_rate_basis_points: i64,
    pub contradiction_groups: i64,
    pub contradiction_rate_basis_points: i64,
    pub review_yield_basis_points: i64,
    pub hard_negative_regressions: i64,
    pub repeat_unresolved_after_promotion: i64,
    pub temporal_effect_groups: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccretionSummary {
    pub headline: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsError {
    message: String,
}

impl MetricsError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MetricsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MetricsError {}

pub type MetricsResult<T> = Result<T, MetricsError>;

pub fn compile_accretion_metrics(
    input: AccretionMetricsInput,
) -> MetricsResult<AccretionMetricsReport> {
    let report_id = non_empty(input.report_id, "report_id")?;
    let generated_at = canonical_timestamp(&input.generated_at, "generated_at")?;
    let privacy = normalize_privacy(input.privacy)?;
    let baseline = normalize_snapshot(input.baseline, "baseline")?;
    let comparison = normalize_snapshot(input.comparison, "comparison")?;
    let attribution = input.attribution;
    let comparison_basis = comparison_basis(&baseline, &comparison);

    let mut warning_codes = Vec::new();
    if !comparison_basis.like_for_like {
        warning_codes.push("corpus_or_policy_changed".to_string());
    }

    let deltas = AccretionDeltas {
        resolved_inputs: delta(
            comparison.totals.resolved_inputs,
            baseline.totals.resolved_inputs,
        ),
        exact_coverage_basis_points: delta_bps(
            comparison.exact_coverage.basis_points,
            baseline.exact_coverage.basis_points,
        ),
        unresolved_groups: delta(
            comparison.totals.unresolved_groups,
            baseline.totals.unresolved_groups,
        ),
        unresolved_group_rate_basis_points: delta_bps(
            comparison.unresolved_group_rate.basis_points,
            baseline.unresolved_group_rate.basis_points,
        ),
        ambiguous_groups: delta(
            comparison.totals.ambiguous_groups,
            baseline.totals.ambiguous_groups,
        ),
        ambiguity_rate_basis_points: delta_bps(
            comparison.ambiguity_rate.basis_points,
            baseline.ambiguity_rate.basis_points,
        ),
        contradiction_groups: delta(
            comparison.totals.contradiction_groups,
            baseline.totals.contradiction_groups,
        ),
        contradiction_rate_basis_points: delta_bps(
            comparison.contradiction_rate.basis_points,
            baseline.contradiction_rate.basis_points,
        ),
        review_yield_basis_points: delta_bps(
            comparison.review_yield.basis_points,
            baseline.review_yield.basis_points,
        ),
        hard_negative_regressions: delta(
            comparison.totals.hard_negative_regressions,
            baseline.totals.hard_negative_regressions,
        ),
        repeat_unresolved_after_promotion: delta(
            comparison.totals.repeat_unresolved_after_promotion,
            baseline.totals.repeat_unresolved_after_promotion,
        ),
        temporal_effect_groups: delta(
            comparison.totals.temporal_effect_groups,
            baseline.totals.temporal_effect_groups,
        ),
    };

    if deltas.hard_negative_regressions > 0 {
        warning_codes.push("hard_negative_regression_increased".to_string());
    }
    if deltas.repeat_unresolved_after_promotion > 0 {
        warning_codes.push("repeat_unresolved_after_promotion_increased".to_string());
    }
    if deltas.exact_coverage_basis_points < 0 {
        warning_codes.push("exact_coverage_decreased".to_string());
    }

    let attribution_total = attribution.total();
    let unattributed_resolved_delta = deltas.resolved_inputs - u64_to_i64(attribution_total);
    if unattributed_resolved_delta != 0 {
        warning_codes.push("resolved_delta_attribution_mismatch".to_string());
    }
    warning_codes.sort();
    warning_codes.dedup();

    let summary = build_summary(&baseline, &comparison, &deltas, &warning_codes);
    let mut report = AccretionMetricsReport {
        version: CANON_ACCRETION_METRICS_VERSION.to_string(),
        report_content_hash: String::new(),
        identity_status: ACCRETION_METRICS_IDENTITY_STATUS.to_string(),
        report_id,
        generated_at,
        privacy,
        comparison_basis,
        baseline,
        comparison,
        deltas,
        attribution,
        attribution_total,
        unattributed_resolved_delta,
        warning_codes,
        summary,
    };
    report.report_content_hash = hash_report(&report)?;
    Ok(report)
}

pub fn canonical_accretion_metrics_json_bytes(
    report: &AccretionMetricsReport,
) -> MetricsResult<Vec<u8>> {
    let mut canonical = report.clone();
    canonical.report_content_hash = hash_report(&canonical)?;
    serde_json::to_vec(&canonical)
        .map_err(|error| MetricsError::new(format!("failed to serialize metrics report: {error}")))
}

impl CoverageGainAttribution {
    pub fn total(&self) -> u64 {
        self.new_aliases
            .saturating_add(self.new_entities)
            .saturating_add(self.remaps)
            .saturating_add(self.provider_additions)
            .saturating_add(self.temporal_effects)
            .saturating_add(self.policy_changes)
    }
}

fn normalize_privacy(privacy: AccretionPrivacyPolicy) -> MetricsResult<AccretionPrivacyPolicy> {
    let classification = non_empty(privacy.classification, "privacy.classification")?;
    let redaction = non_empty(privacy.redaction, "privacy.redaction")?;
    if privacy.raw_identity_values_recorded {
        return Err(MetricsError::new(
            "accretion metrics must not record raw identity values; use counts and digests",
        ));
    }
    Ok(AccretionPrivacyPolicy {
        classification,
        redaction,
        raw_identity_values_recorded: false,
    })
}

fn normalize_snapshot(
    snapshot: AccretionSnapshotInput,
    field: &str,
) -> MetricsResult<AccretionSnapshotMetrics> {
    let snapshot_id = non_empty(snapshot.snapshot_id, &format!("{field}.snapshot_id"))?;
    let corpus = FrozenCorpusRef {
        corpus_id: non_empty(
            snapshot.corpus.corpus_id,
            &format!("{field}.corpus.corpus_id"),
        )?,
        corpus_digest: normalize_digest(
            snapshot.corpus.corpus_digest,
            &format!("{field}.corpus.corpus_digest"),
        )?,
        policy_digest: normalize_digest(
            snapshot.corpus.policy_digest,
            &format!("{field}.corpus.policy_digest"),
        )?,
    };
    let registry = RegistrySnapshotRef {
        registry_id: non_empty(
            snapshot.registry.registry_id,
            &format!("{field}.registry.registry_id"),
        )?,
        registry_version: non_empty(
            snapshot.registry.registry_version,
            &format!("{field}.registry.registry_version"),
        )?,
        registry_digest: normalize_digest(
            snapshot.registry.registry_digest,
            &format!("{field}.registry.registry_digest"),
        )?,
    };
    let strategy_digest = normalize_optional_digest(
        snapshot.strategy_digest,
        &format!("{field}.strategy_digest"),
    )?;
    let project_receipt_digest = normalize_optional_digest(
        snapshot.project_receipt_digest,
        &format!("{field}.project_receipt_digest"),
    )?;
    validate_totals(&snapshot.totals, field)?;

    let totals = snapshot.totals;
    let total_inputs = totals.total_inputs;
    let unresolved_like_groups = totals
        .unresolved_groups
        .saturating_add(totals.ambiguous_groups)
        .saturating_add(totals.contradiction_groups);
    let metrics = AccretionSnapshotMetrics {
        snapshot_id,
        corpus,
        registry,
        strategy_digest,
        project_receipt_digest,
        exact_coverage: RatioMetric::new(totals.resolved_inputs, total_inputs),
        unresolved_group_rate: RatioMetric::new(unresolved_like_groups, total_inputs),
        ambiguity_rate: RatioMetric::new(totals.ambiguous_groups, total_inputs),
        contradiction_rate: RatioMetric::new(totals.contradiction_groups, total_inputs),
        review_yield: RatioMetric::new(totals.promoted_groups, totals.reviewed_groups),
        repeat_unresolved_rate: RatioMetric::new(
            totals.repeat_unresolved_after_promotion,
            totals.promoted_groups,
        ),
        totals,
    };
    Ok(metrics)
}

fn validate_totals(totals: &AccretionSnapshotTotals, field: &str) -> MetricsResult<()> {
    if totals.resolved_inputs > totals.total_inputs {
        return Err(MetricsError::new(format!(
            "{field}.totals.resolved_inputs cannot exceed total_inputs"
        )));
    }
    if totals.promoted_groups > totals.reviewed_groups {
        return Err(MetricsError::new(format!(
            "{field}.totals.promoted_groups cannot exceed reviewed_groups"
        )));
    }
    Ok(())
}

fn comparison_basis(
    baseline: &AccretionSnapshotMetrics,
    comparison: &AccretionSnapshotMetrics,
) -> ComparisonBasis {
    let same_corpus_digest = baseline.corpus.corpus_digest == comparison.corpus.corpus_digest;
    let same_policy_digest = baseline.corpus.policy_digest == comparison.corpus.policy_digest;
    ComparisonBasis {
        like_for_like: same_corpus_digest && same_policy_digest,
        same_corpus_digest,
        same_policy_digest,
        baseline_corpus_digest: baseline.corpus.corpus_digest.clone(),
        comparison_corpus_digest: comparison.corpus.corpus_digest.clone(),
        baseline_policy_digest: baseline.corpus.policy_digest.clone(),
        comparison_policy_digest: comparison.corpus.policy_digest.clone(),
    }
}

impl RatioMetric {
    fn new(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
            basis_points: rate_basis_points(numerator, denominator),
        }
    }
}

fn build_summary(
    baseline: &AccretionSnapshotMetrics,
    comparison: &AccretionSnapshotMetrics,
    deltas: &AccretionDeltas,
    warning_codes: &[String],
) -> AccretionSummary {
    let headline = format!(
        "coverage {} -> {} bp ({:+} bp), unresolved groups {:+}, review yield {:+} bp",
        baseline.exact_coverage.basis_points,
        comparison.exact_coverage.basis_points,
        deltas.exact_coverage_basis_points,
        deltas.unresolved_groups,
        deltas.review_yield_basis_points
    );
    let mut lines = vec![
        format!(
            "resolved inputs {:+}; new total {} of {}",
            deltas.resolved_inputs,
            comparison.totals.resolved_inputs,
            comparison.totals.total_inputs
        ),
        format!(
            "ambiguity {:+} groups ({:+} bp); contradiction {:+} groups ({:+} bp)",
            deltas.ambiguous_groups,
            deltas.ambiguity_rate_basis_points,
            deltas.contradiction_groups,
            deltas.contradiction_rate_basis_points
        ),
        format!(
            "hard-negative regressions {:+}; repeat unresolved after promotion {:+}",
            deltas.hard_negative_regressions, deltas.repeat_unresolved_after_promotion
        ),
    ];
    if !warning_codes.is_empty() {
        lines.push(format!("warnings: {}", warning_codes.join(",")));
    }
    AccretionSummary { headline, lines }
}

fn non_empty(value: String, field: &str) -> MetricsResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(MetricsError::new(format!("{field} must be non-empty")))
    } else {
        Ok(trimmed.to_string())
    }
}

fn canonical_timestamp(value: &str, field: &str) -> MetricsResult<String> {
    let parsed = DateTime::parse_from_rfc3339(value.trim()).map_err(|error| {
        MetricsError::new(format!("{field} must be an RFC3339 timestamp: {error}"))
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn normalize_optional_digest(value: Option<String>, field: &str) -> MetricsResult<Option<String>> {
    value
        .map(|digest| normalize_digest(digest, field))
        .transpose()
}

fn normalize_digest(value: String, field: &str) -> MetricsResult<String> {
    let trimmed = value.trim();
    let Some(hex) = trimmed.strip_prefix("blake3:") else {
        return Err(MetricsError::new(format!(
            "{field} must be a blake3:<64 hex> digest"
        )));
    };
    if hex.len() != 64 || !hex.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(MetricsError::new(format!(
            "{field} must be a blake3:<64 hex> digest"
        )));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}

fn rate_basis_points(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    let rounded =
        (u128::from(numerator) * 10_000 + (u128::from(denominator) / 2)) / u128::from(denominator);
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

fn delta(new: u64, old: u64) -> i64 {
    if new >= old {
        u64_to_i64(new - old)
    } else {
        -u64_to_i64(old - new)
    }
}

fn delta_bps(new: u32, old: u32) -> i64 {
    i64::from(new) - i64::from(old)
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn hash_report(report: &AccretionMetricsReport) -> MetricsResult<String> {
    let mut hashable = report.clone();
    hashable.report_content_hash.clear();
    hash_serialized(&hashable)
}

fn hash_serialized<T: Serialize>(value: &T) -> MetricsResult<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| MetricsError::new(format!("failed to serialize for hashing: {error}")))?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}
