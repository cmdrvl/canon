#![forbid(unsafe_code)]

use canon::strategy::tournament::{
    STRATEGY_TOURNAMENT_SCHEMA_VERSION, StrategyTournamentCandidate, StrategyTournamentErrorKind,
    StrategyTournamentEvaluation, StrategyTournamentInput, StrategyTournamentPartition,
    StrategyTournamentPartitions, StrategyTournamentRankingPolicy, TournamentAccessKind,
    TournamentEvaluationStatus, TournamentGenerationAccess, TournamentMetricGoal,
    TournamentMetricRule, TournamentPartitionRole, TournamentResourceCost, TournamentUncertainty,
    canonical_tournament_report_bytes, run_strategy_tournament, strategy_tournament_schema_version,
};
use serde_json::Value;
use std::collections::BTreeMap;

const TOURNAMENT_SCHEMA_JSON: &str =
    include_str!("../schemas/canon.strategy.tournament.v1.schema.json");

#[test]
fn tournament_schema_declares_holdout_and_promotion_boundaries() {
    let schema: Value = serde_json::from_str(TOURNAMENT_SCHEMA_JSON).expect("schema parses");
    assert_eq!(schema["title"], STRATEGY_TOURNAMENT_SCHEMA_VERSION);
    assert_eq!(
        schema["properties"]["version"]["const"],
        strategy_tournament_schema_version()
    );
    assert_eq!(
        schema["$defs"]["ranking_policy"]["properties"]["partition_role"]["enum"],
        serde_json::json!(["train", "tune"])
    );
    assert_eq!(
        schema["x-canon-contract"]["holdout_access_during_candidate_generation"],
        "rejected"
    );
    assert_eq!(
        schema["x-canon-contract"]["candidate_generation_sources"],
        "each generation input source_digest must match the declared train/tune partition corpus_digest or labels_digest for its access kind"
    );
    assert_eq!(
        schema["x-canon-contract"]["promotion"],
        "never automatic; tournament output is a recommendation for explicit operator review"
    );
}

#[test]
fn shuffled_candidates_produce_byte_identical_ranking_and_report() {
    let left = fixture_input(false);
    let right = fixture_input(true);

    let left_report = run_strategy_tournament(left).expect("left tournament");
    let right_report = run_strategy_tournament(right).expect("right tournament");

    assert_eq!(
        left_report
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_id.as_str())
            .collect::<Vec<_>>(),
        vec!["balanced", "cheap-tie", "overfit", "resource-hit"]
    );
    assert_eq!(left_report, right_report);
    assert_eq!(
        canonical_tournament_report_bytes(&left_report).unwrap(),
        canonical_tournament_report_bytes(&right_report).unwrap()
    );
    assert_eq!(
        left_report.summary.decision,
        "recommend_review_no_auto_promotion"
    );
    assert_eq!(
        left_report.candidates[0].recommendation,
        "candidate_for_operator_review"
    );
    assert!(
        left_report
            .candidates
            .iter()
            .all(|candidate| candidate.recommendation != "promoted")
    );
    assert_eq!(left_report.summary.holdout_evaluations, 4);
    assert_eq!(left_report.summary.failed_candidates, 1);

    let resource_hit = &left_report.candidates[3];
    assert_eq!(
        resource_hit.ranking_status,
        TournamentEvaluationStatus::ResourceFailure
    );
}

#[test]
fn holdout_metrics_are_reported_but_never_used_for_candidate_ordering() {
    let report = run_strategy_tournament(fixture_input(false)).expect("tournament");

    let balanced = &report.candidates[0];
    let overfit = &report.candidates[2];
    assert_eq!(balanced.candidate_id, "balanced");
    assert_eq!(overfit.candidate_id, "overfit");
    assert_eq!(balanced.ranking_metrics["quality_score"], 910);
    assert_eq!(overfit.ranking_metrics["quality_score"], 900);
    assert_eq!(balanced.holdout_metrics["quality_score"], 880);
    assert_eq!(overfit.holdout_metrics["quality_score"], 990);
    assert_eq!(overfit.hard_negative_failures, 1);
    assert_eq!(balanced.resource_cost.wall_ms, 50);
    assert_eq!(balanced.uncertainty[0].metric, "quality_score");
    assert_eq!(
        overfit.regressions,
        vec!["hard_negative_cluster_regression".to_string()]
    );
}

#[test]
fn rejects_holdout_access_before_tournament_evaluation() {
    let mut input = fixture_input(false);
    input.candidates[0]
        .generation_inputs
        .push(TournamentGenerationAccess {
            partition_role: TournamentPartitionRole::Holdout,
            access_kind: TournamentAccessKind::Labels,
            package_digest: digest("package"),
            source_digest: digest("holdout-labels"),
        });

    let error = run_strategy_tournament(input).expect_err("holdout generation access rejected");
    assert_eq!(
        error.kind,
        StrategyTournamentErrorKind::HoldoutGenerationAccess
    );
}

#[test]
fn rejects_undeclared_generation_sources_and_holdout_masquerade() {
    let mut undeclared = fixture_input(false);
    undeclared.candidates[0].generation_inputs[1].source_digest = digest("external-tune-corpus");
    let error = run_strategy_tournament(undeclared).expect_err("undeclared source digest rejected");
    assert_eq!(
        error.kind,
        StrategyTournamentErrorKind::UndeclaredPartitionSource
    );

    let mut input = fixture_input(false);
    input.candidates[0].generation_inputs[0].partition_role = TournamentPartitionRole::Train;
    input.candidates[0].generation_inputs[0].access_kind = TournamentAccessKind::Labels;
    input.candidates[0].generation_inputs[0].source_digest =
        input.partitions.holdout.labels_digest.clone();
    let error =
        run_strategy_tournament(input).expect_err("holdout digest cannot masquerade as train");
    assert_eq!(
        error.kind,
        StrategyTournamentErrorKind::UndeclaredPartitionSource
    );
}

#[test]
fn rejects_duplicate_partition_source_digests() {
    let mut input = fixture_input(false);
    input.partitions.tune.labels_digest = input.partitions.holdout.labels_digest.clone();

    let error = run_strategy_tournament(input).expect_err("duplicate partition source rejected");
    assert_eq!(
        error.kind,
        StrategyTournamentErrorKind::DuplicatePartitionSource
    );
}

#[test]
fn rejects_holdout_ranking_policy_and_missing_metrics() {
    let mut holdout_policy = fixture_input(false);
    holdout_policy.ranking_policy.partition_role = TournamentPartitionRole::Holdout;
    let error =
        run_strategy_tournament(holdout_policy).expect_err("holdout ranking policy rejected");
    assert_eq!(error.kind, StrategyTournamentErrorKind::HoldoutRanking);

    let mut missing_metric = fixture_input(false);
    missing_metric.evaluations[0].metrics.clear();
    let error = run_strategy_tournament(missing_metric).expect_err("missing metric rejected");
    assert_eq!(
        error.kind,
        StrategyTournamentErrorKind::MissingRankingMetric
    );
}

#[test]
fn rejects_unknown_candidates_and_package_boundary_escapes() {
    let mut unknown = fixture_input(false);
    unknown.evaluations.push(evaluation(
        "not-declared",
        TournamentPartitionRole::Tune,
        TournamentEvaluationStatus::Passed,
        [("quality_score", 1), ("false_merge_risk", 0)],
        0,
        1,
    ));
    let error = run_strategy_tournament(unknown).expect_err("unknown candidate rejected");
    assert_eq!(error.kind, StrategyTournamentErrorKind::UnknownCandidate);

    let mut package_escape = fixture_input(false);
    package_escape.candidates[0].generation_inputs[0].package_digest = digest("other-package");
    let error = run_strategy_tournament(package_escape).expect_err("package escape rejected");
    assert_eq!(error.kind, StrategyTournamentErrorKind::PackageBoundary);
}

fn fixture_input(reverse: bool) -> StrategyTournamentInput {
    let package_digest = digest("package");
    let mut candidates = vec![
        candidate("overfit", [("threshold", "0.99")], &package_digest),
        candidate("balanced", [("threshold", "0.82")], &package_digest),
        candidate("cheap-tie", [("threshold", "0.82")], &package_digest),
        candidate("resource-hit", [("threshold", "0.80")], &package_digest),
    ];
    let mut evaluations = vec![
        evaluation(
            "overfit",
            TournamentPartitionRole::Tune,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 900), ("false_merge_risk", 2)],
            1,
            40,
        ),
        evaluation(
            "balanced",
            TournamentPartitionRole::Tune,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 910), ("false_merge_risk", 0)],
            0,
            50,
        ),
        evaluation(
            "cheap-tie",
            TournamentPartitionRole::Tune,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 910), ("false_merge_risk", 0)],
            0,
            30,
        ),
        evaluation(
            "resource-hit",
            TournamentPartitionRole::Tune,
            TournamentEvaluationStatus::ResourceFailure,
            [("quality_score", 999), ("false_merge_risk", 0)],
            0,
            10,
        ),
        evaluation(
            "overfit",
            TournamentPartitionRole::Holdout,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 990), ("false_merge_risk", 8)],
            7,
            60,
        ),
        evaluation(
            "balanced",
            TournamentPartitionRole::Holdout,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 880), ("false_merge_risk", 0)],
            0,
            70,
        ),
        evaluation(
            "cheap-tie",
            TournamentPartitionRole::Holdout,
            TournamentEvaluationStatus::Passed,
            [("quality_score", 870), ("false_merge_risk", 0)],
            0,
            20,
        ),
        evaluation(
            "resource-hit",
            TournamentPartitionRole::Holdout,
            TournamentEvaluationStatus::ResourceFailure,
            [("quality_score", 1000), ("false_merge_risk", 0)],
            0,
            10,
        ),
    ];
    if reverse {
        candidates.reverse();
        evaluations.reverse();
    }

    StrategyTournamentInput {
        version: STRATEGY_TOURNAMENT_SCHEMA_VERSION.to_string(),
        tournament_id: "schema-transform-thresholds".to_string(),
        strategy_kind: "schema-transform".to_string(),
        package_digest,
        partitions: StrategyTournamentPartitions {
            train: partition("train", "train-corpus", "train-labels", 20),
            tune: partition("tune", "tune-corpus", "tune-labels", 10),
            holdout: partition("holdout", "holdout-corpus", "holdout-labels", 10),
        },
        candidates,
        evaluations,
        ranking_policy: StrategyTournamentRankingPolicy {
            partition_role: TournamentPartitionRole::Tune,
            metric_order: vec![
                TournamentMetricRule {
                    metric: "quality_score".to_string(),
                    goal: TournamentMetricGoal::Maximize,
                },
                TournamentMetricRule {
                    metric: "false_merge_risk".to_string(),
                    goal: TournamentMetricGoal::Minimize,
                },
            ],
        },
    }
}

fn candidate<const N: usize>(
    candidate_id: &str,
    parameters: [(&str, &str); N],
    package_digest: &str,
) -> StrategyTournamentCandidate {
    StrategyTournamentCandidate {
        candidate_id: candidate_id.to_string(),
        package_digest: package_digest.to_string(),
        parameters: parameters
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
        generation_inputs: vec![
            TournamentGenerationAccess {
                partition_role: TournamentPartitionRole::Train,
                access_kind: TournamentAccessKind::Labels,
                package_digest: package_digest.to_string(),
                source_digest: digest("train-labels"),
            },
            TournamentGenerationAccess {
                partition_role: TournamentPartitionRole::Tune,
                access_kind: TournamentAccessKind::Features,
                package_digest: package_digest.to_string(),
                source_digest: digest("tune-features"),
            },
        ],
    }
}

fn evaluation<const N: usize>(
    candidate_id: &str,
    partition_role: TournamentPartitionRole,
    status: TournamentEvaluationStatus,
    metrics: [(&str, i64); N],
    hard_negative_failures: u64,
    wall_ms: u64,
) -> StrategyTournamentEvaluation {
    let regressions =
        if candidate_id == "overfit" && partition_role == TournamentPartitionRole::Tune {
            vec!["hard_negative_cluster_regression".to_string()]
        } else {
            Vec::new()
        };

    StrategyTournamentEvaluation {
        candidate_id: candidate_id.to_string(),
        partition_role,
        run_digest: digest(&format!("{candidate_id}-{partition_role:?}")),
        status,
        metrics: metrics
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<BTreeMap<_, _>>(),
        hard_negative_failures,
        resource_cost: TournamentResourceCost {
            wall_ms,
            peak_memory_bytes: wall_ms * 100,
            output_bytes: 128,
        },
        regressions,
        uncertainty: vec![TournamentUncertainty {
            metric: "quality_score".to_string(),
            lower: metrics[0].1 - 5,
            upper: metrics[0].1 + 5,
        }],
    }
}

fn partition(
    partition_id: &str,
    corpus_seed: &str,
    labels_seed: &str,
    row_count: u64,
) -> StrategyTournamentPartition {
    StrategyTournamentPartition {
        partition_id: partition_id.to_string(),
        corpus_digest: digest(corpus_seed),
        labels_digest: digest(labels_seed),
        row_count,
    }
}

fn digest(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}
