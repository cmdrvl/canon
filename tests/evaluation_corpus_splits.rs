#![forbid(unsafe_code)]

#[path = "../src/evaluation/corpus.rs"]
mod corpus;

use corpus::{
    CorpusPartition, EvaluationErrorCode, SealedCorpusQualityFixture,
    canonical_sealed_corpus_quality_report_bytes, deterministic_metrics,
    finalize_sealed_corpus_quality_fixture, sealed_corpus_quality_report,
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
    let jsonl_ids = observation_ids();
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

fn fixture() -> SealedCorpusQualityFixture {
    serde_json::from_str(MANIFEST_JSON).expect("fixture parses")
}

fn observation_ids() -> BTreeSet<String> {
    OBSERVATIONS_JSONL
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<NeutralObservationRow>(line)
                .expect("observation row parses")
                .observation_id
        })
        .collect()
}
