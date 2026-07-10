#![forbid(unsafe_code)]

#[path = "../src/evaluation/corpus.rs"]
mod corpus;

use corpus::{
    CorpusPartition, EvaluationErrorCode, HoldoutMode, SealedCorpusQualityFixture,
    SplitConformanceFixture, canonical_sealed_corpus_quality_report_bytes,
    canonical_split_conformance_report_bytes, deterministic_metrics,
    finalize_sealed_corpus_quality_fixture, sealed_corpus_quality_report, split_conformance_report,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const SCHEMA_JSON: &str = include_str!("../schemas/canon.evaluation.corpus.v1.schema.json");
const MANIFEST_JSON: &str = include_str!("fixtures/canon_v1/quality/corpus/neutral_manifest.json");
const OBSERVATIONS_JSONL: &str =
    include_str!("fixtures/canon_v1/quality/corpus/neutral_observations.jsonl");
const EXPECTED_REPORT_JSON: &str =
    include_str!("fixtures/canon_v1/quality/corpus/expected_quality_report.json");
const VALID_ALIAS_DISJOINT_JSON: &str = include_str!(
    "fixtures/extensions/neutral-domain/evaluation/valid_alias_disjoint_manifest.json"
);
const VALID_ENTITY_DISJOINT_JSON: &str = include_str!(
    "fixtures/extensions/neutral-domain/evaluation/valid_entity_disjoint_manifest.json"
);
const VALID_TIME_FORWARD_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/valid_time_forward_manifest.json");
const LEAK_SHARED_ENTITY_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/leak_shared_entity_manifest.json");
const LEAK_SHARED_ALIAS_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/leak_shared_alias_manifest.json");
const LEAK_SHARED_SOURCE_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/leak_shared_source_manifest.json");
const LEAK_MUTATION_SEED_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/leak_mutation_seed_manifest.json");
const LEAK_DUPLICATE_ROW_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/leak_duplicate_row_manifest.json");
const SPLIT_OBSERVATIONS_JSONL: &str =
    include_str!("fixtures/extensions/neutral-domain/evaluation/observations.jsonl");

#[derive(Debug, Deserialize)]
struct NeutralObservationRow {
    observation_id: String,
}

#[test]
fn schema_exposes_minimal_sealed_quality_harness_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(
        schema["x-canon-contract"]["minimal_same_entity_quality_harness_supported"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["sealed_quality_labels_supported"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["quality_miss_and_ablation_evidence_supported"],
        true
    );
    assert_eq!(
        schema["$defs"]["quality_outcome"]["enum"],
        json!(["correct", "review", "explicit_refusal", "measured_miss"])
    );
    assert_eq!(
        schema["$defs"]["quality_miss_stage"]["enum"],
        json!(["candidate_generation", "evidence_scoring", "solver"])
    );
}

#[test]
fn schema_exposes_split_conformance_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("schema parses");
    assert_eq!(
        schema["x-canon-contract"]["split_boundary_conformance_supported"],
        true
    );
    assert_eq!(
        schema["x-canon-contract"]["holdout_modes_supported"],
        json!(["alias_disjoint", "entity_disjoint", "time_forward"])
    );
    assert_eq!(
        schema["x-canon-contract"]["split_label_commitments_required"],
        true
    );
    assert_eq!(
        schema["$defs"]["holdout_mode"]["enum"],
        json!(["alias_disjoint", "entity_disjoint", "time_forward"])
    );
}

#[test]
fn neutral_fixture_emits_canonical_quality_report() {
    let fixture = fixture();
    let finalized =
        finalize_sealed_corpus_quality_fixture(fixture.clone()).expect("fixture finalizes");
    let scoring_ids = finalized
        .corpus
        .observations
        .iter()
        .filter_map(|observation| {
            let dataset = finalized
                .corpus
                .datasets
                .iter()
                .find(|dataset| dataset.dataset_id == observation.dataset_id)
                .expect("dataset exists");
            matches!(
                dataset.partition,
                CorpusPartition::Holdout | CorpusPartition::ExactReplay
            )
            .then_some(observation.observation_id.as_str())
        })
        .collect::<BTreeSet<_>>();
    let jsonl_ids = observation_ids(OBSERVATIONS_JSONL);
    for case in &finalized.quality_harness.cases {
        assert!(jsonl_ids.contains(case.left_observation_id.as_str()));
        assert!(jsonl_ids.contains(case.right_observation_id.as_str()));
        assert!(scoring_ids.contains(case.left_observation_id.as_str()));
        assert!(scoring_ids.contains(case.right_observation_id.as_str()));
    }

    let report = sealed_corpus_quality_report(&fixture).expect("report builds");
    let metrics = deterministic_metrics(&fixture.corpus).expect("metrics compute");
    assert_eq!(report.discovery_case_count, 2);
    assert_eq!(report.exact_replay_case_count, 1);
    assert_eq!(report.correct_discovery_case_count, 1);
    assert_eq!(report.discovery_success_basis_points, Some(5000));
    assert_eq!(report.miss_stage_counts["solver"], 1);
    assert_eq!(report.ablation_evidence.len(), 1);
    assert_eq!(metrics.exact_replay_coverage.observation_count, 2);

    let actual_bytes =
        canonical_sealed_corpus_quality_report_bytes(&fixture).expect("canonical bytes");
    let expected_bytes = EXPECTED_REPORT_JSON.trim().as_bytes();
    assert_eq!(
        actual_bytes,
        expected_bytes,
        "canonical quality report drifted\nactual={}\nexpected={}",
        String::from_utf8_lossy(&actual_bytes),
        EXPECTED_REPORT_JSON.trim()
    );
}

#[test]
fn reordered_quality_cases_preserve_canonical_report_bytes() {
    let left = fixture();
    let mut right = fixture();
    right.quality_harness.cases.reverse();
    right.corpus.datasets.reverse();
    right.corpus.observations.reverse();
    right.corpus.cluster_labels.reverse();
    let left_bytes =
        canonical_sealed_corpus_quality_report_bytes(&left).expect("left bytes canonical");
    let right_bytes =
        canonical_sealed_corpus_quality_report_bytes(&right).expect("right bytes canonical");
    assert_eq!(left_bytes, right_bytes);
}

#[test]
fn quality_harness_refuses_unsealed_labels_and_tuning_pairs() {
    let mut unsealed = fixture();
    unsealed.quality_harness.labels_sealed = false;
    let error =
        finalize_sealed_corpus_quality_fixture(unsealed).expect_err("unsealed labels must fail");
    assert_eq!(error.code, EvaluationErrorCode::CompatibilityPolicy);

    let mut tuning_case = fixture();
    tuning_case.quality_harness.cases[1].left_observation_id = "obs.train.seed".to_string();
    tuning_case.quality_harness.cases[1].right_observation_id = "obs.train.peer".to_string();
    let error =
        finalize_sealed_corpus_quality_fixture(tuning_case).expect_err("tuning case must fail");
    assert_eq!(error.code, EvaluationErrorCode::CompatibilityPolicy);
}

#[test]
fn valid_split_manifests_finalize_and_report_exact_replay_separately() {
    let common_ids = split_observation_ids();
    let cases = [
        (
            VALID_ALIAS_DISJOINT_JSON,
            HoldoutMode::AliasDisjoint,
            2usize,
            1usize,
            5usize,
            4usize,
        ),
        (
            VALID_ENTITY_DISJOINT_JSON,
            HoldoutMode::EntityDisjoint,
            2usize,
            2usize,
            6usize,
            5usize,
        ),
        (
            VALID_TIME_FORWARD_JSON,
            HoldoutMode::TimeForward,
            2usize,
            1usize,
            5usize,
            4usize,
        ),
    ];

    for (
        manifest_json,
        expected_mode,
        expected_exact_replay,
        expected_holdout,
        expected_rows,
        expected_unique_rows,
    ) in cases
    {
        let fixture = split_fixture(manifest_json);
        let finalized = corpus::finalize_split_conformance_fixture(fixture.clone())
            .expect("split fixture finalizes");
        for observation in &finalized.corpus.observations {
            assert!(
                common_ids.contains(observation.observation_id.as_str()),
                "missing neutral-domain observation {}",
                observation.observation_id
            );
        }

        let report = split_conformance_report(&fixture).expect("split report builds");
        assert_eq!(report.holdout_mode, expected_mode);
        assert!(!report.labels_accessible_to_strategy);
        assert_eq!(report.exact_replay_observation_count, expected_exact_replay);
        assert_eq!(report.protected_holdout_observation_count, expected_holdout);
        assert_eq!(report.physical_row_count, expected_rows);
        assert_eq!(report.unique_row_fingerprint_count, expected_unique_rows);
        assert!(
            report
                .forbidden_boundary_counts
                .values()
                .all(|count| *count == 0),
            "valid split report should not retain forbidden overlaps: {:?}",
            report.forbidden_boundary_counts
        );
    }
}

#[test]
fn split_reports_are_deterministic_under_shuffled_manifest_order() {
    let left = split_fixture(VALID_ENTITY_DISJOINT_JSON);
    let mut right = split_fixture(VALID_ENTITY_DISJOINT_JSON);
    right.corpus.datasets.reverse();
    right.corpus.observations.reverse();
    right.corpus.adjudications.reverse();
    right.split_conformance.records.reverse();

    let left_bytes =
        canonical_split_conformance_report_bytes(&left).expect("left split bytes canonical");
    let right_bytes =
        canonical_split_conformance_report_bytes(&right).expect("right split bytes canonical");
    assert_eq!(left_bytes, right_bytes);
}

#[test]
fn planted_split_leakage_manifests_refuse_before_reporting() {
    for manifest_json in [
        LEAK_SHARED_ENTITY_JSON,
        LEAK_SHARED_ALIAS_JSON,
        LEAK_SHARED_SOURCE_JSON,
        LEAK_MUTATION_SEED_JSON,
        LEAK_DUPLICATE_ROW_JSON,
    ] {
        let error = corpus::finalize_split_conformance_fixture(split_fixture(manifest_json))
            .expect_err("leaking manifest must fail");
        assert_eq!(error.code, EvaluationErrorCode::PartitionLeakage);
    }
}

#[test]
fn split_conformance_refuses_accessible_labels_bad_hash_and_time_regression() {
    let mut accessible = split_fixture(VALID_ALIAS_DISJOINT_JSON);
    accessible.split_conformance.labels_accessible_to_strategy = true;
    let error = corpus::finalize_split_conformance_fixture(accessible)
        .expect_err("accessible labels must fail");
    assert_eq!(error.code, EvaluationErrorCode::CompatibilityPolicy);

    let mut bad_hash = split_fixture(VALID_ALIAS_DISJOINT_JSON);
    bad_hash.split_conformance.holdout_label_commitment_digest = "bad-hash".to_string();
    let error = corpus::finalize_split_conformance_fixture(bad_hash)
        .expect_err("bad commitment hash must fail");
    assert_eq!(error.code, EvaluationErrorCode::ArtifactContract);

    let mut time_regression = split_fixture(VALID_TIME_FORWARD_JSON);
    let holdout_observation = time_regression
        .corpus
        .observations
        .iter_mut()
        .find(|observation| observation.observation_id == "obs.hold.future.alpha")
        .expect("holdout observation");
    holdout_observation.observed_at = Some("2026-01-05".to_string());
    let error = corpus::finalize_split_conformance_fixture(time_regression)
        .expect_err("time regression must fail");
    assert_eq!(error.code, EvaluationErrorCode::PartitionLeakage);
}

fn fixture() -> SealedCorpusQualityFixture {
    serde_json::from_str(MANIFEST_JSON).expect("fixture parses")
}

fn observation_ids(lines: &str) -> BTreeSet<String> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<NeutralObservationRow>(line)
                .expect("observation row parses")
                .observation_id
        })
        .collect()
}

fn split_fixture(manifest_json: &str) -> SplitConformanceFixture {
    serde_json::from_str(manifest_json).expect("split fixture parses")
}

fn split_observation_ids() -> BTreeSet<String> {
    observation_ids(SPLIT_OBSERVATIONS_JSONL)
}
