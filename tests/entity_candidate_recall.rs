#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION,
    block::{
        BlockCandidateBudgetConfig, BlockCandidateGenerationDiagnostics, BlockCandidateHit,
        BlockCandidateRecord, BlockOperatorCandidateDiagnostics, BlockOperatorYield,
        CandidateRecallEvaluationRequest, evaluate_candidate_recall,
    },
    edge::EdgeCandidateBudgetProof,
    telemetry::{
        CandidateRecallGoldPair, CandidateRecallMissForensic, CandidateRecallMissReason,
        CandidateRecallStratum,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, fs};

#[test]
fn candidate_recall_reports_cutoffs_strata_union_and_marginal_operator_contribution() {
    let candidates = [
        candidate("surf:raw-a", "surf:raw-b", vec![hit("raw", 1, 1000)]),
        candidate(
            "surf:legal-a",
            "surf:legal-b",
            vec![hit("legal_name", 4, 900), hit("token_ngram", 8, 850)],
        ),
        candidate("surf:dba-a", "surf:dba-b", vec![hit("dba", 12, 800)]),
        candidate(
            "surf:temporal-a",
            "surf:temporal-b",
            vec![hit("temporal_alias", 50, 750)],
        ),
        candidate("surf:cap-a", "surf:cap-b", vec![hit("raw", 51, 700)]),
    ];
    let surface_ids = surface_ids([
        "surf:raw-a",
        "surf:raw-b",
        "surf:legal-a",
        "surf:legal-b",
        "surf:dba-a",
        "surf:dba-b",
        "surf:temporal-a",
        "surf:temporal-b",
        "surf:cap-a",
        "surf:cap-b",
        "surf:novel-a",
        "surf:novel-b",
    ]);
    let gold_pairs = vec![
        gold(
            "gold:raw",
            "surf:raw-a",
            "surf:raw-b",
            CandidateRecallStratum::ExactKnown,
        ),
        gold(
            "gold:legal",
            "surf:legal-a",
            "surf:legal-b",
            CandidateRecallStratum::ExactKnown,
        ),
        gold(
            "gold:cap",
            "surf:cap-a",
            "surf:cap-b",
            CandidateRecallStratum::ExactKnown,
        ),
        gold(
            "gold:dba",
            "surf:dba-a",
            "surf:dba-b",
            CandidateRecallStratum::WithheldAlias,
        ),
        gold(
            "gold:novel",
            "surf:novel-a",
            "surf:novel-b",
            CandidateRecallStratum::NovelCluster,
        ),
        gold(
            "gold:directional",
            "surf:temporal-a",
            "surf:temporal-b",
            CandidateRecallStratum::DirectionalLink,
        ),
    ];
    let diagnostics = diagnostics(
        [
            ("raw", 2, 4, 0),
            ("legal_name", 1, 0, 0),
            ("token_ngram", 1, 0, 0),
            ("dba", 1, 0, 0),
            ("temporal_alias", 1, 0, 0),
        ],
        4,
        4,
        0,
    );

    let report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &candidates,
        diagnostics: &diagnostics,
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: 2,
    });
    report
        .validate()
        .expect("candidate recall report validates");

    assert_eq!(report.cutoffs, [1, 5, 10, 25, 50]);
    assert_metric(&report.union_recall_at_k, 1, 1, 6);
    assert_metric(&report.union_recall_at_k, 5, 2, 6);
    assert_metric(&report.union_recall_at_k, 10, 2, 6);
    assert_metric(&report.union_recall_at_k, 25, 3, 6);
    assert_metric(&report.union_recall_at_k, 50, 4, 6);
    assert_eq!(report.cap_effects.candidate_pairs_suppressed_by_cap, 4);
    assert_eq!(report.cap_effects.suppressed_candidate_count, 4);
    assert!(report.cap_effects.candidate_budget_validated);
    assert_eq!(report.exact_buckets.exact_bucket_count, 2);
    assert_eq!(
        report.exact_buckets.pair_expansion_policy,
        "compact_no_pair_expansion"
    );

    let exact_known = report
        .strata
        .iter()
        .find(|stratum| stratum.stratum == CandidateRecallStratum::ExactKnown)
        .expect("exact-known stratum");
    assert_metric(&exact_known.recall_at_k, 50, 2, 3);
    let withheld_alias = report
        .strata
        .iter()
        .find(|stratum| stratum.stratum == CandidateRecallStratum::WithheldAlias)
        .expect("withheld-alias stratum");
    assert_metric(&withheld_alias.recall_at_k, 50, 1, 1);
    let novel_cluster = report
        .strata
        .iter()
        .find(|stratum| stratum.stratum == CandidateRecallStratum::NovelCluster)
        .expect("novel-cluster stratum");
    assert_metric(&novel_cluster.recall_at_k, 50, 0, 1);
    let directional_link = report
        .strata
        .iter()
        .find(|stratum| stratum.stratum == CandidateRecallStratum::DirectionalLink)
        .expect("directional-link stratum");
    assert_metric(&directional_link.recall_at_k, 50, 1, 1);

    let operators = report
        .operators
        .iter()
        .map(|operator| (operator.operator_id.as_str(), operator.marginal_hits_at_50))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(operators["raw"], 1);
    assert_eq!(operators["legal_name"], 0);
    assert_eq!(operators["token_ngram"], 0);
    assert_eq!(operators["dba"], 1);
    assert_eq!(operators["temporal_alias"], 1);

    let rank_records = report
        .true_pair_ranks
        .iter()
        .map(|record| {
            (
                record.gold_pair_id.as_str(),
                record.operator_id.as_str(),
                record.rank,
            )
        })
        .collect::<Vec<_>>();
    assert!(rank_records.contains(&("gold:directional", "temporal_alias", 50)));
    assert!(!rank_records.iter().any(|record| record.0 == "gold:cap"));

    let misses = misses_by_id(&report.misses_at_50);
    assert_eq!(
        misses["gold:cap"].reason,
        CandidateRecallMissReason::CandidateCap
    );
    assert_eq!(misses["gold:cap"].best_rank, Some(51));
    assert!(misses["gold:cap"].candidate_cap_effective);
    assert_eq!(
        misses["gold:novel"].reason,
        CandidateRecallMissReason::CandidateCap
    );
}

#[test]
fn candidate_recall_miss_forensics_are_deterministic_and_redaction_safe() {
    let surface_ids = surface_ids(["surf:known-a", "surf:known-b"]);
    let gold_pairs = vec![
        gold(
            "gold:malformed",
            "surf:known-a",
            "surf:known-a",
            CandidateRecallStratum::ExactKnown,
        ),
        gold(
            "gold:suppressed",
            "surf:known-a",
            "surf:known-b",
            CandidateRecallStratum::WithheldAlias,
        ),
        gold(
            "gold:unknown",
            "surf:known-a",
            "surf:missing",
            CandidateRecallStratum::NovelCluster,
        ),
    ];
    let diagnostics = diagnostics([("common_bucket", 0, 0, 3)], 0, 0, 3);

    let first = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &[],
        diagnostics: &diagnostics,
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: 1,
    });
    let second = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &[],
        diagnostics: &diagnostics,
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: 1,
    });
    assert_eq!(first, second);
    first.validate().expect("suppression report validates");

    let misses = misses_by_id(&first.misses_at_50);
    assert_eq!(
        misses["gold:malformed"].reason,
        CandidateRecallMissReason::MalformedGold
    );
    assert_eq!(
        misses["gold:suppressed"].reason,
        CandidateRecallMissReason::PostingSuppression
    );
    assert_eq!(
        misses["gold:unknown"].reason,
        CandidateRecallMissReason::ProfileMapping
    );
    assert!(misses["gold:suppressed"].large_bucket_suppression);
    assert_eq!(first.large_bucket_suppression.large_buckets_suppressed, 3);

    let json = serde_json::to_string(&first).expect("report serializes");
    assert!(!json.contains("raw_rows"));
    assert!(!json.contains("source_rows"));
    assert!(!json.contains("Sears"));
    assert!(!json.contains("Acme"));
}

#[test]
fn candidate_recall_distinguishes_operator_coverage_from_absent_evidence() {
    let surface_ids = surface_ids(["surf:left", "surf:right"]);
    let gold_pairs = vec![gold(
        "gold:absent",
        "surf:left",
        "surf:right",
        CandidateRecallStratum::NovelCluster,
    )];

    let coverage_report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &[],
        diagnostics: &diagnostics([("token_ngram", 1, 0, 0)], 0, 0, 0),
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: 0,
    });
    assert_eq!(
        coverage_report.misses_at_50[0].reason,
        CandidateRecallMissReason::OperatorCoverage
    );

    let absent_report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
        candidate_records: &[],
        diagnostics: &diagnostics([("token_ngram", 0, 0, 0)], 0, 0, 0),
        gold_pairs: &gold_pairs,
        surface_ids: &surface_ids,
        exact_bucket_count: 0,
    });
    assert_eq!(
        absent_report.misses_at_50[0].reason,
        CandidateRecallMissReason::AbsentNormalizedEvidence
    );
}

#[test]
fn candidate_recall_cli_runs_public_neutral_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let candidates_path = temp.path().join("candidates.json");
    let diagnostics_path = temp.path().join("diagnostics.json");
    let candidates = [
        candidate(
            "obs.replay.left",
            "obs.replay.right",
            vec![hit("exact_view:stable_id", 1, 1000)],
        ),
        candidate(
            "obs.hold.alpha.left",
            "obs.hold.alpha.right",
            vec![hit("withheld_alias:legal_name", 5, 900)],
        ),
        candidate(
            "obs.hold.beta.left",
            "obs.hold.beta.right",
            vec![hit("token_ngram:neutral", 51, 800)],
        ),
    ];
    let diagnostics = diagnostics(
        [
            ("exact_view:stable_id", 1, 0, 0),
            ("withheld_alias:legal_name", 1, 0, 0),
            ("token_ngram:neutral", 1, 2, 0),
        ],
        2,
        2,
        0,
    );
    fs::write(
        &candidates_path,
        serde_json::to_vec(&candidates).expect("candidates serialize"),
    )
    .expect("write candidates");
    fs::write(
        &diagnostics_path,
        serde_json::to_vec(&diagnostics).expect("diagnostics serialize"),
    )
    .expect("write diagnostics");

    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "candidate-recall",
            "--manifest",
            "tests/fixtures/canon_v1/quality/corpus/neutral_manifest.json",
            "--candidates",
            candidates_path.to_str().expect("candidate path utf-8"),
            "--diagnostics",
            diagnostics_path.to_str().expect("diagnostics path utf-8"),
            "--exact-bucket-count",
            "1",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("report json");

    assert_eq!(report["version"], "canon_entity_candidate_recall.v0");
    assert_eq!(report["total_gold_pairs"], 3);
    assert_eq!(report["exact_buckets"]["exact_bucket_count"], 1);
    assert_eq!(report["union_recall_at_k"][4]["k"], 50);
    assert_eq!(report["union_recall_at_k"][4]["hits"], 2);
    assert_eq!(report["union_recall_at_k"][4]["total"], 3);
    assert_eq!(report["misses_at_50"].as_array().expect("misses").len(), 1);
    assert_eq!(
        report["misses_at_50"][0]["gold_pair_id"],
        "case.hold.novel.beta"
    );
    assert_eq!(report["misses_at_50"][0]["reason"], "candidate_cap");
    assert_eq!(report["misses_at_50"][0]["best_rank"], 51);

    let report_text = String::from_utf8(output).expect("utf8 report");
    assert!(!report_text.contains("North Harbor Labs"));
    assert!(!report_text.contains("South Ridge Studio"));
}

#[test]
fn candidate_recall_cli_accepts_native_candidate_jsonl() {
    let temp = tempfile::tempdir().expect("tempdir");
    let candidates_path = temp.path().join("candidates.jsonl");
    let diagnostics_path = temp.path().join("diagnostics.json");
    let candidates = [
        candidate(
            "obs.replay.left",
            "obs.replay.right",
            vec![hit("exact_view:stable_id", 1, 1000)],
        ),
        candidate(
            "obs.hold.alpha.left",
            "obs.hold.alpha.right",
            vec![hit("withheld_alias:legal_name", 5, 900)],
        ),
    ];
    let diagnostics = diagnostics(
        [
            ("exact_view:stable_id", 1, 0, 0),
            ("withheld_alias:legal_name", 1, 0, 0),
        ],
        0,
        0,
        0,
    );
    let mut jsonl = candidates
        .iter()
        .map(|candidate| serde_json::to_string(candidate).expect("candidate serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    jsonl.push('\n');
    fs::write(&candidates_path, jsonl).expect("write native candidates jsonl");
    fs::write(
        &diagnostics_path,
        serde_json::to_vec(&diagnostics).expect("diagnostics serialize"),
    )
    .expect("write diagnostics");

    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "candidate-recall",
            "--manifest",
            "tests/fixtures/canon_v1/quality/corpus/neutral_manifest.json",
            "--candidates",
            candidates_path.to_str().expect("candidate path utf-8"),
            "--diagnostics",
            diagnostics_path.to_str().expect("diagnostics path utf-8"),
            "--exact-bucket-count",
            "0",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("report json");

    assert_eq!(report["version"], "canon_entity_candidate_recall.v0");
    assert_eq!(report["total_gold_pairs"], 3);
    assert_eq!(report["union_recall_at_k"][4]["k"], 50);
    assert_eq!(report["union_recall_at_k"][4]["hits"], 2);
    assert_eq!(report["exact_buckets"]["exact_bucket_count"], 0);
}

fn candidate(
    left_surface_id: &str,
    right_surface_id: &str,
    block_hits: Vec<BlockCandidateHit>,
) -> BlockCandidateRecord {
    let candidate_score_hint = block_hits
        .iter()
        .map(|hit| hit.score_units)
        .max()
        .unwrap_or_default();
    BlockCandidateRecord {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        block_hits,
        candidate_score_hint,
    }
}

fn hit(operator_id: &str, rank: usize, score_units: u32) -> BlockCandidateHit {
    BlockCandidateHit {
        operator_id: operator_id.to_string(),
        rank: Some(rank),
        score_units,
    }
}

fn gold(
    gold_pair_id: &str,
    left_surface_id: &str,
    right_surface_id: &str,
    stratum: CandidateRecallStratum,
) -> CandidateRecallGoldPair {
    CandidateRecallGoldPair::new(gold_pair_id, left_surface_id, right_surface_id, stratum)
}

fn surface_ids<const N: usize>(ids: [&str; N]) -> Vec<String> {
    ids.into_iter().map(str::to_string).collect()
}

fn diagnostics<const N: usize>(
    operator_counts: [(&str, u64, u64, u64); N],
    suppressed_by_cap: u64,
    suppressed_candidate_count: u64,
    large_buckets_suppressed: u64,
) -> BlockCandidateGenerationDiagnostics {
    let candidate_count = operator_counts
        .iter()
        .map(|(_, emitted, _, _)| *emitted)
        .sum::<u64>();
    let operator_yield = operator_counts
        .iter()
        .map(
            |(operator_id, emitted, suppressed, large)| BlockOperatorYield {
                operator_id: (*operator_id).to_string(),
                emitted_candidate_count: *emitted,
                suppressed_candidate_count: *suppressed,
                large_posting_suppressed_count: *large,
            },
        )
        .collect::<Vec<_>>();
    let operator_diagnostics = operator_counts
        .iter()
        .map(
            |(operator_id, emitted, suppressed, large)| BlockOperatorCandidateDiagnostics {
                operator_id: (*operator_id).to_string(),
                input_candidate_count: emitted.saturating_add(*suppressed),
                eligible_candidate_count: *emitted,
                emitted_candidate_count: *emitted,
                suppressed_candidate_count: *suppressed,
                large_posting_suppressed_count: *large,
            },
        )
        .collect::<Vec<_>>();

    BlockCandidateGenerationDiagnostics {
        candidate_record_count: candidate_count,
        candidate_pairs_emitted: candidate_count,
        candidate_pairs_suppressed_by_cap: suppressed_by_cap,
        suppressed_candidate_count,
        large_buckets_suppressed,
        candidate_pairs_per_surface_p50: candidate_count.min(50),
        candidate_pairs_per_surface_p95: candidate_count.min(50),
        candidate_pairs_per_surface_p99: candidate_count.min(50),
        max_candidates_for_surface: candidate_count.min(50),
        max_candidates_for_operator: candidate_count,
        configured_budget: BlockCandidateBudgetConfig::new(50, 500, 5_000),
        candidate_budget: EdgeCandidateBudgetProof::within_run_budget(candidate_count, 5_000),
        candidate_artifact_bytes: candidate_count.saturating_mul(128),
        partial_candidate_artifact_written: false,
        operator_yield,
        operator_diagnostics,
    }
}

fn assert_metric(
    metrics: &[canon::entity::telemetry::CandidateRecallAtK],
    k: usize,
    hits: u64,
    total: u64,
) {
    let metric = metrics
        .iter()
        .find(|metric| metric.k == k)
        .expect("metric k present");
    assert_eq!(metric.hits, hits);
    assert_eq!(metric.total, total);
}

fn misses_by_id(
    misses: &[CandidateRecallMissForensic],
) -> BTreeMap<&str, &CandidateRecallMissForensic> {
    misses
        .iter()
        .map(|miss| (miss.gold_pair_id.as_str(), miss))
        .collect()
}
