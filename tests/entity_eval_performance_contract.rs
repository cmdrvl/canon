use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

fn fixture(path: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity")
        .join(path);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture opens")).expect("fixture parses")
}

#[test]
fn entity_eval_performance_contract_is_parseable_and_linked_to_docs() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    assert_eq!(
        contract["schema_version"],
        "canon.entity.eval_performance_targets.v0"
    );

    let doc = contract["doc"].as_str().unwrap();
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(doc).exists(),
        "{doc} exists"
    );

    let commands = contract["run_commands"].as_object().unwrap();
    assert!(commands.contains_key("contract_ci"));
    assert!(commands.contains_key("small_ci_future"));
    assert!(commands.contains_key("operator_future"));
}

#[test]
fn entity_eval_scorecard_ids_are_unique_and_cover_required_eval_types() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    let metrics = contract["scorecard_metrics"].as_array().unwrap();
    let mut ids = BTreeSet::new();
    for metric in metrics {
        let id = metric["id"].as_str().unwrap();
        assert!(ids.insert(id.to_string()), "duplicate metric id {id}");
    }

    for required in [
        "ER-SCORE-001",
        "ER-SCORE-002",
        "ER-ADV-001",
        "ER-REVIEW-001",
        "ER-PERTURB-001",
        "ER-DET-001",
        "ER-DIFF-001",
    ] {
        assert!(ids.contains(required), "missing {required}");
    }
}

#[test]
fn entity_eval_structural_performance_gates_are_stop_ship_safe() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    let gates = &contract["structural_performance_gates"];

    assert_eq!(gates["row_level_all_pairs"], "forbidden");
    assert_eq!(
        gates["surface_level_all_pairs"],
        "forbidden_except_tiny_explicit_fixtures"
    );
    assert_eq!(gates["exact_bucket_pair_expansion_count"], 0);
    assert_eq!(gates["exact_bucket_representation"], "compact_hyperedge");
    assert_eq!(gates["candidate_pairs_per_surface_p95_max"], 25);
    assert_eq!(gates["candidate_pairs_per_surface_p99_max"], 100);
    assert_eq!(gates["review_groups_per_500k_cmbs_backfill_max"], 2000);
    assert_eq!(
        gates["unique_500k_required_behavior"],
        "bounded_completion_or_deterministic_refusal"
    );
}

#[test]
fn entity_eval_wall_clock_targets_cover_cmbs_and_regab_workloads() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    let targets = contract["wall_clock_targets"].as_array().unwrap();
    let by_id = targets
        .iter()
        .map(|target| (target["id"].as_str().unwrap(), target))
        .collect::<std::collections::BTreeMap<_, _>>();

    let required_ids = [
        "PERF-SMALL-CI",
        "PERF-REGAB-COMMITTED",
        "PERF-CMBS-PUBLIC",
        "PERF-REGAB-FULL",
        "PERF-REGAB-APPLY",
        "PERF-REGAB-PREPARE",
        "PERF-CMBS-500K-WARM",
        "PERF-CMBS-500K-COLD",
        "PERF-CMBS-500K-APPLY",
        "PERF-CMBS-500K-UNIQUE",
    ];
    for id in required_ids {
        assert!(by_id.contains_key(id), "missing {id}");
    }

    assert_eq!(by_id["PERF-CMBS-PUBLIC"]["target_seconds"], 2);
    assert_eq!(by_id["PERF-REGAB-FULL"]["target_seconds"], 10);
    assert_eq!(by_id["PERF-CMBS-500K-WARM"]["target_seconds"], 120);
    assert_eq!(by_id["PERF-CMBS-500K-COLD"]["target_seconds"], 300);
    assert_eq!(
        by_id["PERF-CMBS-500K-UNIQUE"]["target_seconds"],
        Value::Null
    );
    assert_eq!(
        by_id["PERF-CMBS-500K-UNIQUE"]["enforcement"],
        "bounded_completion_or_deterministic_refusal"
    );
}

#[test]
fn entity_eval_telemetry_contract_contains_performance_explainers() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    let fields = contract["telemetry_required_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field.as_str().unwrap())
        .collect::<BTreeSet<_>>();

    for required in [
        "cache_state",
        "raw_row_count",
        "prepared_surface_count",
        "candidate_pair_count",
        "candidate_pairs_per_surface_p95",
        "candidate_pairs_per_surface_p99",
        "suppressed_candidate_count",
        "exact_bucket_pair_expansion_count",
        "largest_component_size",
        "artifact_bytes_by_stage",
        "timings_ms_by_stage",
        "peak_memory_bytes",
        "next_command",
    ] {
        assert!(
            fields.contains(required),
            "missing telemetry field {required}"
        );
    }
}
