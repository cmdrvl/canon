#![forbid(unsafe_code)]

#[path = "../src/evaluation/generalization.rs"]
mod generalization;

use generalization::{
    CANON_GENERALIZATION_VERSION, CorpusVisibility, GeneralizationBenchmark,
    GeneralizationErrorCode, LeakChannel, LeakageProbe, ProtectedSet, canonical_benchmark_bytes,
    canonical_report_bytes, compile_generalization_benchmark, generalization_schema_version,
};
use serde::Deserialize;

const BENCHMARK_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/time_forward/generalization_benchmark.json");
const LEAKAGE_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/time_forward/leakage_controls.json");

#[derive(Debug, Deserialize)]
struct LeakageFixture {
    cases: Vec<LeakageCase>,
}

#[derive(Debug, Deserialize)]
struct LeakageCase {
    case_id: String,
    family: String,
    trial_id: String,
    channel: LeakChannel,
    protected_set: ProtectedSet,
    locator: String,
    value: String,
    expected_error: String,
}

#[test]
fn clean_public_generalization_fixture_reports_required_slices() {
    let benchmark = benchmark();
    let report = compile_generalization_benchmark(benchmark).expect("benchmark compiles");

    assert_eq!(
        generalization_schema_version(),
        CANON_GENERALIZATION_VERSION
    );
    assert_eq!(report.version, CANON_GENERALIZATION_VERSION);
    assert_eq!(report.corpus_visibility, CorpusVisibility::PublicFixture);
    assert_eq!(report.entity_disjoint.len(), 1);
    assert_eq!(report.time_forward.len(), 1);
    assert_eq!(report.aggregate.entity_disjoint_trial_count, 1);
    assert_eq!(report.aggregate.time_forward_trial_count, 1);
    assert_eq!(report.aggregate.critical_false_merge_count, 0);
    assert_eq!(report.aggregate.directional_cross_source_count, 2);
    assert!(report.aggregate.head_result_count > 0);
    assert!(report.aggregate.tail_result_count > 0);
    assert!(report.aggregate.easy_result_count > 0);
    assert!(report.aggregate.hard_result_count > 0);

    let entity = &report.entity_disjoint[0];
    assert_eq!(entity.novel_cluster_result_count, 1);
    assert_eq!(entity.correct_novel_cluster_count, 1);
    assert_eq!(entity.related_distinct_hard_negative_count, 1);
    assert_eq!(entity.critical_false_merge_count, 0);
    assert_eq!(entity.directional_cross_source_count, 1);

    let time = &report.time_forward[0];
    assert_eq!(time.cutoff, "2026-01-01");
    assert_eq!(time.evaluation_result_count, 3);
    assert_eq!(time.correct_evaluation_count, 3);
    assert_eq!(time.renamed_surface_count, 1);
    assert_eq!(time.new_entity_count, 1);
    assert_eq!(time.changed_relationship_count, 1);
    assert_eq!(time.critical_false_merge_count, 0);
}

#[test]
fn public_and_private_corpus_use_same_contract() {
    let public = benchmark();
    let mut private = benchmark();
    private.corpus_visibility = CorpusVisibility::PrivateCorpusRef;
    private.corpus_ref = "private://operator-owned/time-forward".to_string();

    let public_report = compile_generalization_benchmark(public).expect("public compiles");
    let private_report = compile_generalization_benchmark(private).expect("private compiles");

    assert_eq!(
        public_report.aggregate.result_count,
        private_report.aggregate.result_count
    );
    assert_eq!(
        public_report.aggregate.directional_cross_source_count,
        private_report.aggregate.directional_cross_source_count
    );
    assert_eq!(
        private_report.corpus_visibility,
        CorpusVisibility::PrivateCorpusRef
    );
}

#[test]
fn canonical_bytes_are_stable_across_physical_ordering() {
    let left = benchmark();
    let mut right = benchmark();
    right.entity_disjoint_trials.reverse();
    right.time_forward_trials.reverse();
    for trial in &mut right.entity_disjoint_trials {
        trial.observations.reverse();
        trial.discovery_results.reverse();
        trial.hard_negatives.reverse();
        trial.directional_links.reverse();
        trial.leakage_probes.reverse();
    }
    for trial in &mut right.time_forward_trials {
        trial.observations.reverse();
        trial.build_observation_ids.reverse();
        trial.evaluation_observation_ids.reverse();
        trial.event_results.reverse();
        trial.hard_negatives.reverse();
        trial.directional_links.reverse();
        trial.leakage_probes.reverse();
    }

    assert_eq!(
        canonical_benchmark_bytes(&left).expect("left bytes"),
        canonical_benchmark_bytes(&right).expect("right bytes")
    );

    let left_report = compile_generalization_benchmark(left).expect("left report");
    let right_report = compile_generalization_benchmark(right).expect("right report");
    assert_eq!(
        canonical_report_bytes(&left_report).expect("left report bytes"),
        canonical_report_bytes(&right_report).expect("right report bytes")
    );
}

#[test]
fn planted_leakage_controls_refuse_by_family() {
    let leakage: LeakageFixture = serde_json::from_str(LEAKAGE_JSON).expect("leakage parses");
    for case in leakage.cases {
        let mut benchmark = benchmark();
        let probe = LeakageProbe {
            channel: case.channel,
            protected_set: case.protected_set,
            locator: case.locator,
            value: case.value,
        };

        match case.family.as_str() {
            "entity_disjoint" => benchmark
                .entity_disjoint_trials
                .iter_mut()
                .find(|trial| trial.trial_id == case.trial_id)
                .expect("entity trial")
                .leakage_probes
                .push(probe),
            "time_forward" => benchmark
                .time_forward_trials
                .iter_mut()
                .find(|trial| trial.trial_id == case.trial_id)
                .expect("time trial")
                .leakage_probes
                .push(probe),
            family => panic!("unexpected fixture family {family}"),
        }

        let error =
            compile_generalization_benchmark(benchmark).expect_err("planted leakage should refuse");
        let expected = match case.expected_error.as_str() {
            "entity_disjoint_leak" => GeneralizationErrorCode::EntityDisjointLeak,
            "future_leakage" => GeneralizationErrorCode::FutureLeakage,
            other => panic!("unexpected expected error {other}"),
        };
        assert_eq!(error.code, expected, "{}", case.case_id);
        assert!(
            error.message.contains(case.channel.as_str()),
            "error should cite leak channel for {}: {}",
            case.case_id,
            error.message
        );
    }
}

#[test]
fn entity_disjoint_split_rejects_entity_overlap() {
    let mut benchmark = benchmark();
    let trial = &mut benchmark.entity_disjoint_trials[0];
    let mut leaked = trial.observations[0].clone();
    leaked.observation_id = "obs.holdout.leaked-anchor".to_string();
    leaked.partition = generalization::BenchmarkPartition::Holdout;
    trial.observations.push(leaked);

    let error = compile_generalization_benchmark(benchmark).expect_err("overlap refuses");
    assert_eq!(error.code, GeneralizationErrorCode::EntityDisjointLeak);
}

#[test]
fn temporal_reversal_controls_reject_future_build_inputs() {
    let mut benchmark = benchmark();
    let trial = &mut benchmark.time_forward_trials[0];
    trial
        .build_observation_ids
        .push("obs.eval.rename".to_string());

    let error = compile_generalization_benchmark(benchmark).expect_err("future build refuses");
    assert_eq!(error.code, GeneralizationErrorCode::TemporalReversal);
}

#[test]
fn severity_critical_false_merge_blocks_release_claims() {
    let mut benchmark = benchmark();
    benchmark.entity_disjoint_trials[0].hard_negatives[0].false_merge = true;

    let error = compile_generalization_benchmark(benchmark).expect_err("critical merge refuses");
    assert_eq!(error.code, GeneralizationErrorCode::CriticalFalseMerge);
}

#[test]
fn directional_cross_source_links_require_different_dataset_roles() {
    let mut benchmark = benchmark();
    let link = &mut benchmark.entity_disjoint_trials[0].directional_links[0];
    link.target_dataset_id = link.reference_dataset_id.clone();

    let error = compile_generalization_benchmark(benchmark).expect_err("bad link refuses");
    assert_eq!(error.code, GeneralizationErrorCode::DirectionalLinkContract);
}

fn benchmark() -> GeneralizationBenchmark {
    serde_json::from_str(BENCHMARK_JSON).expect("generalization fixture parses")
}
