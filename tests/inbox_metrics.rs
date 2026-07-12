#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/inbox/metrics.rs"]
mod inbox_metrics;

use inbox_metrics::{
    ACCRETION_METRICS_IDENTITY_STATUS, AccretionMetricsInput, AccretionPrivacyPolicy,
    AccretionSnapshotInput, AccretionSnapshotTotals, CANON_ACCRETION_METRICS_VERSION,
    CoverageGainAttribution, FrozenCorpusRef, RegistrySnapshotRef,
    canonical_accretion_metrics_json_bytes, compile_accretion_metrics,
};
use serde_json::Value;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.accretion.metrics.v1.schema.json");

#[test]
fn schema_declares_privacy_safe_metrics_and_attribution_categories() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");

    assert_eq!(schema["title"], CANON_ACCRETION_METRICS_VERSION);
    assert!(
        schema["description"]
            .as_str()
            .unwrap()
            .contains("must not contain raw identity values")
    );
    assert_eq!(
        schema["properties"]["identity_status"]["const"],
        ACCRETION_METRICS_IDENTITY_STATUS
    );
    assert_eq!(
        schema["properties"]["privacy"]["properties"]["raw_identity_values_recorded"]["const"],
        false
    );

    let attribution = &schema["$defs"]["attribution"]["required"];
    assert!(
        attribution
            .as_array()
            .unwrap()
            .contains(&"new_aliases".into())
    );
    assert!(
        attribution
            .as_array()
            .unwrap()
            .contains(&"provider_additions".into())
    );
    assert!(
        attribution
            .as_array()
            .unwrap()
            .contains(&"temporal_effects".into())
    );
}

#[test]
fn like_for_like_metrics_separate_coverage_gain_and_visible_regressions() {
    let report = compile_accretion_metrics(sample_input()).expect("report compiles");

    assert_eq!(report.version, CANON_ACCRETION_METRICS_VERSION);
    assert_eq!(
        report.identity_status, ACCRETION_METRICS_IDENTITY_STATUS,
        "metrics never assert identity"
    );
    assert_eq!(report.baseline.exact_coverage.basis_points, 6_000);
    assert_eq!(report.comparison.exact_coverage.basis_points, 7_500);
    assert_eq!(report.deltas.exact_coverage_basis_points, 1_500);
    assert_eq!(report.deltas.resolved_inputs, 15);
    assert_eq!(report.attribution_total, 15);
    assert_eq!(report.unattributed_resolved_delta, 0);
    assert_eq!(report.comparison.review_yield.basis_points, 7_500);
    assert!(report.comparison_basis.like_for_like);
    assert!(
        report
            .warning_codes
            .contains(&"hard_negative_regression_increased".to_string())
    );
    assert!(
        report
            .warning_codes
            .contains(&"repeat_unresolved_after_promotion_increased".to_string())
    );
    assert!(report.summary.headline.contains("coverage 6000 -> 7500 bp"));
}

#[test]
fn corpus_or_policy_drift_is_flagged_not_hidden() {
    let mut input = sample_input();
    input.comparison.corpus.corpus_digest = digest("different corpus");
    input.comparison.corpus.policy_digest = digest("different policy");

    let report = compile_accretion_metrics(input).expect("report compiles with warning");

    assert!(!report.comparison_basis.like_for_like);
    assert!(!report.comparison_basis.same_corpus_digest);
    assert!(!report.comparison_basis.same_policy_digest);
    assert!(
        report
            .warning_codes
            .contains(&"corpus_or_policy_changed".to_string())
    );
}

#[test]
fn raw_identity_values_are_refused_by_default() {
    let mut input = sample_input();
    input.privacy.raw_identity_values_recorded = true;

    let error = compile_accretion_metrics(input).expect_err("raw identity values refuse");

    assert!(
        error
            .to_string()
            .contains("must not record raw identity values")
    );
}

#[test]
fn canonical_report_bytes_and_hash_are_deterministic() {
    let first = compile_accretion_metrics(sample_input()).expect("first report");
    let second = compile_accretion_metrics(sample_input()).expect("second report");

    assert_eq!(first.report_content_hash, second.report_content_hash);
    assert_eq!(
        canonical_accretion_metrics_json_bytes(&first).unwrap(),
        canonical_accretion_metrics_json_bytes(&second).unwrap()
    );
    assert!(first.report_content_hash.starts_with("blake3:"));
}

#[test]
fn invalid_snapshot_totals_refuse_instead_of_masking_math() {
    let mut input = sample_input();
    input.comparison.totals.resolved_inputs = 101;

    let error = compile_accretion_metrics(input).expect_err("invalid totals refuse");

    assert!(
        error
            .to_string()
            .contains("resolved_inputs cannot exceed total_inputs")
    );
}

fn sample_input() -> AccretionMetricsInput {
    AccretionMetricsInput {
        report_id: "report.accretion.synthetic".to_string(),
        generated_at: "2026-07-11T04:20:00-07:00".to_string(),
        privacy: AccretionPrivacyPolicy {
            classification: "internal".to_string(),
            redaction: "counts_and_digests_only".to_string(),
            raw_identity_values_recorded: false,
        },
        baseline: snapshot(
            "baseline",
            "1.0.0",
            AccretionSnapshotTotals {
                total_inputs: 100,
                resolved_inputs: 60,
                unresolved_groups: 22,
                ambiguous_groups: 5,
                contradiction_groups: 1,
                reviewed_groups: 4,
                promoted_groups: 2,
                hard_negative_regressions: 0,
                repeat_unresolved_after_promotion: 1,
                temporal_effect_groups: 0,
            },
        ),
        comparison: snapshot(
            "comparison",
            "1.1.0",
            AccretionSnapshotTotals {
                total_inputs: 100,
                resolved_inputs: 75,
                unresolved_groups: 12,
                ambiguous_groups: 4,
                contradiction_groups: 2,
                reviewed_groups: 8,
                promoted_groups: 6,
                hard_negative_regressions: 1,
                repeat_unresolved_after_promotion: 2,
                temporal_effect_groups: 3,
            },
        ),
        attribution: CoverageGainAttribution {
            new_aliases: 6,
            new_entities: 2,
            remaps: 1,
            provider_additions: 3,
            temporal_effects: 3,
            policy_changes: 0,
        },
    }
}

fn snapshot(
    snapshot_id: &str,
    registry_version: &str,
    totals: AccretionSnapshotTotals,
) -> AccretionSnapshotInput {
    AccretionSnapshotInput {
        snapshot_id: snapshot_id.to_string(),
        corpus: FrozenCorpusRef {
            corpus_id: "corpus.synthetic".to_string(),
            corpus_digest: digest("corpus"),
            policy_digest: digest("policy"),
        },
        registry: RegistrySnapshotRef {
            registry_id: "registry.synthetic".to_string(),
            registry_version: registry_version.to_string(),
            registry_digest: digest(registry_version),
        },
        strategy_digest: Some(digest("strategy")),
        project_receipt_digest: Some(digest(snapshot_id)),
        totals,
    }
}

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}
