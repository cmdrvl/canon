#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/telemetry.rs"]
mod telemetry;

use serde_json::Value;
use std::collections::BTreeSet;
use telemetry::{
    BaselineArtifact, BaselineBuild, BaselineCacheMetrics, BaselineCandidateMetrics,
    BaselineComparisonBand, BaselineComparisonReasonKind, BaselineComparisonStatus, BaselineCorpus,
    BaselineEvidenceMetrics, BaselineHardware, BaselineIo, BaselinePeakMemory, BaselinePrivacy,
    BaselineQualityMetrics, BaselineReviewMetrics, BaselineStageMetric, BaselineTool,
    ComparisonDirection, MetricComparisonStatus, PrivacyClassification, baseline_schema_version,
    canonical_baseline_bytes, compare_baselines, finalized_baseline,
    forbidden_baseline_payload_keys, required_baseline_fields,
};

const SCHEMA_JSON: &str = include_str!("../schemas/canon.baseline.v1.schema.json");

#[test]
fn schema_declares_privacy_safe_local_baseline_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], baseline_schema_version());
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        baseline_schema_version()
    );
    assert_eq!(schema["x-canon-contract"]["local_artifact_only"], true);
    assert_eq!(schema["x-canon-contract"]["phones_home"], false);
    assert_eq!(
        schema["x-canon-contract"]["raw_identity_values_recorded_by_default"],
        false
    );
    assert_eq!(
        schema["x-canon-contract"]["separates_context_changes_from_regressions"],
        true
    );

    let required = schema["required"]
        .as_array()
        .expect("required array")
        .iter()
        .map(|field| field.as_str().expect("required string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        required,
        required_baseline_fields()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    );
    let schema_text = SCHEMA_JSON.to_ascii_lowercase();
    for forbidden in ["cmbs", "regab", "servicer", "tranche", "loan"] {
        assert!(
            !schema_text.contains(forbidden),
            "baseline schema must stay domain-neutral: {forbidden}"
        );
    }
}

#[test]
fn baseline_artifact_is_canonical_redacted_and_digest_bound() {
    let mut left = sample_baseline("baseline-left");
    left.stages.reverse();
    left.comparison_bands.reverse();
    left.privacy.allowed_payload_keys.reverse();

    let right = sample_baseline("baseline-left");
    assert_eq!(
        canonical_baseline_bytes(&left).expect("left canonical bytes"),
        canonical_baseline_bytes(&right).expect("right canonical bytes")
    );
    assert_eq!(left.baseline_digest, right.baseline_digest);
    assert!(!left.privacy.raw_identity_values_recorded);
    assert!(left.privacy.raw_values_redacted);

    let mut raw = serde_json::to_value(&left).expect("baseline to json");
    raw.as_object_mut()
        .expect("object")
        .insert("raw_rows".to_string(), Value::Array(Vec::new()));
    let error = serde_json::from_value::<BaselineArtifact>(raw)
        .expect_err("unknown raw payload key refuses");
    assert!(error.to_string().contains("raw_rows"), "{error}");

    let mut unsafe_baseline = left;
    unsafe_baseline.privacy.raw_identity_values_recorded = true;
    let error =
        telemetry::validate_baseline(&unsafe_baseline).expect_err("raw identity flag must refuse");
    assert_eq!(error.field, "privacy.raw_identity_values_recorded");
}

#[test]
fn comparison_reports_noise_improvement_warning_and_regression_deterministically() {
    let baseline = sample_baseline("baseline");

    let mut noise = sample_baseline("noise");
    noise.peak_memory.bytes += 16;
    noise = finalized_baseline(noise).expect("noise finalizes");
    let comparison = compare_baselines(&baseline, &noise).expect("noise compares");
    assert_eq!(comparison.status, BaselineComparisonStatus::NoChange);
    assert!(
        comparison
            .metrics
            .iter()
            .any(|metric| metric.status == MetricComparisonStatus::Noise)
    );

    let mut improvement = sample_baseline("improvement");
    improvement
        .stages
        .iter_mut()
        .find(|stage| stage.stage_id == "block")
        .expect("block stage")
        .duration_ms = 80;
    improvement.quality.candidate_recall_at_50_basis_points = 9900;
    improvement = finalized_baseline(improvement).expect("improvement finalizes");
    let comparison = compare_baselines(&baseline, &improvement).expect("improvement compares");
    assert_eq!(comparison.status, BaselineComparisonStatus::Improvement);

    let mut warning = sample_baseline("warning");
    warning
        .stages
        .iter_mut()
        .find(|stage| stage.stage_id == "block")
        .expect("block stage")
        .duration_ms = 112;
    warning = finalized_baseline(warning).expect("warning finalizes");
    let comparison = compare_baselines(&baseline, &warning).expect("warning compares");
    assert_eq!(comparison.status, BaselineComparisonStatus::Warning);

    let mut regression = sample_baseline("regression");
    regression
        .stages
        .iter_mut()
        .find(|stage| stage.stage_id == "block")
        .expect("block stage")
        .duration_ms = 130;
    regression.quality.severe_false_merge_count = 1;
    regression = finalized_baseline(regression).expect("regression finalizes");
    let comparison = compare_baselines(&baseline, &regression).expect("regression compares");
    assert_eq!(comparison.status, BaselineComparisonStatus::Regression);
    assert!(comparison.metrics.iter().any(|metric| {
        metric.metric_id == "quality.severe_false_merge_count"
            && metric.status == MetricComparisonStatus::Regression
    }));
}

#[test]
fn changed_tool_hardware_or_corpus_is_incomparable_not_a_regression() {
    let baseline = sample_baseline("baseline");

    let mut changed = sample_baseline("changed");
    changed.corpus.corpus_digest = digest("different-corpus");
    changed.hardware.hardware_fingerprint_digest = digest("different-hardware");
    changed.tool.command_contract_digest = digest("different-command-contract");
    changed = finalized_baseline(changed).expect("changed finalizes");

    let comparison = compare_baselines(&baseline, &changed).expect("incomparable report");
    assert!(!comparison.comparable);
    assert_eq!(comparison.status, BaselineComparisonStatus::Incomparable);
    let kinds = comparison
        .reasons
        .iter()
        .map(|reason| reason.kind)
        .collect::<BTreeSet<_>>();
    assert!(kinds.contains(&BaselineComparisonReasonKind::ToolChanged));
    assert!(kinds.contains(&BaselineComparisonReasonKind::HardwareChanged));
    assert!(kinds.contains(&BaselineComparisonReasonKind::CorpusChanged));
    assert!(comparison.metrics.is_empty());
}

#[test]
fn forbidden_payload_key_inventory_stays_in_schema_and_module() {
    let schema_text = SCHEMA_JSON;
    for forbidden in forbidden_baseline_payload_keys() {
        assert!(
            schema_text.contains(forbidden),
            "schema must explicitly reject {forbidden}"
        );
    }
}

fn sample_baseline(id: &str) -> BaselineArtifact {
    finalized_baseline(BaselineArtifact {
        schema_version: baseline_schema_version().to_string(),
        baseline_id: id.to_string(),
        tool: BaselineTool {
            canon_version: "0.10.0".to_string(),
            command_contract_digest: digest("command-contract"),
            operator_contract_digest: digest("operator-contract"),
        },
        build: BaselineBuild {
            source_revision_digest: digest("source-revision"),
            cargo_lock_digest: digest("cargo-lock"),
            rustc_version_digest: digest("rustc-version"),
            target_triple: "synthetic-target".to_string(),
        },
        hardware: BaselineHardware {
            hardware_fingerprint_digest: digest("hardware"),
            os: "synthetic-os".to_string(),
            arch: "synthetic-arch".to_string(),
            logical_cores: 8,
            memory_bytes: 16 * 1024 * 1024 * 1024,
        },
        corpus: BaselineCorpus {
            corpus_id: "neutral-public-holdout".to_string(),
            corpus_digest: digest("corpus"),
            split_digest: digest("split"),
            label_digest: digest("labels"),
            row_count: 100,
            unique_surface_count: 70,
            license_class: "public_fixture".to_string(),
        },
        privacy: BaselinePrivacy {
            classification: PrivacyClassification::RedactedPrivate,
            raw_identity_values_recorded: false,
            raw_values_redacted: true,
            redaction_policy_digest: digest("redaction-policy"),
            redacted_field_count: 12,
            allowed_payload_keys: vec![
                "candidate_pair_count".to_string(),
                "duration_ms".to_string(),
                "peak_memory_bytes".to_string(),
            ],
        },
        stages: vec![
            BaselineStageMetric {
                stage_id: "prepare".to_string(),
                duration_ms: 50,
                artifact_bytes: 2_000,
                row_count: 100,
            },
            BaselineStageMetric {
                stage_id: "block".to_string(),
                duration_ms: 100,
                artifact_bytes: 3_000,
                row_count: 70,
            },
        ],
        peak_memory: BaselinePeakMemory {
            bytes: 100_000_000,
            method: "harness_rss_sample".to_string(),
        },
        io: BaselineIo {
            read_bytes: 10_000,
            write_bytes: 20_000,
        },
        candidates: BaselineCandidateMetrics {
            input_surface_count: 70,
            candidate_pair_count: 500,
            suppressed_candidate_count: 15,
            exact_bucket_count: 4,
        },
        evidence: BaselineEvidenceMetrics {
            evidence_edge_count: 300,
            support_edge_count: 280,
            cannot_link_count: 20,
        },
        review: BaselineReviewMetrics {
            review_group_count: 8,
            accepted_decision_count: 3,
            rejected_decision_count: 1,
            pending_decision_count: 4,
        },
        quality: BaselineQualityMetrics {
            candidate_recall_at_50_basis_points: 9500,
            auto_link_precision_basis_points: 9800,
            abstention_rate_basis_points: 1200,
            severe_false_merge_count: 0,
        },
        cache: BaselineCacheMetrics {
            cache_hit_count: 2,
            cache_miss_count: 4,
            cache_reused_bytes: 12_000,
            cache_written_bytes: 30_000,
        },
        comparison_bands: vec![
            band(
                "stage.block.duration_ms",
                ComparisonDirection::LowerIsBetter,
                1,
                1_000,
                2_000,
            ),
            band(
                "memory.peak_bytes",
                ComparisonDirection::LowerIsBetter,
                1024,
                1_000,
                2_000,
            ),
            band(
                "quality.candidate_recall_at_50_basis_points",
                ComparisonDirection::HigherIsBetter,
                0,
                100,
                200,
            ),
            band(
                "quality.severe_false_merge_count",
                ComparisonDirection::Exact,
                0,
                0,
                0,
            ),
        ],
        baseline_digest: String::new(),
    })
    .expect("sample baseline finalizes")
}

fn band(
    metric_id: &str,
    direction: ComparisonDirection,
    noise_absolute: u64,
    warn_relative_basis_points: u64,
    fail_relative_basis_points: u64,
) -> BaselineComparisonBand {
    BaselineComparisonBand {
        metric_id: metric_id.to_string(),
        direction,
        noise_absolute,
        warn_relative_basis_points,
        fail_relative_basis_points,
    }
}

fn digest(input: &str) -> String {
    format!("blake3:{}", blake3::hash(input.as_bytes()).to_hex())
}
