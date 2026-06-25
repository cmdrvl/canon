use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

fn fixture(path: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity")
        .join(path);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture opens")).expect("fixture parses")
}

fn scorecard_metric<'a>(contract: &'a Value, id: &str) -> &'a Value {
    contract["scorecard_metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|metric| metric["id"] == id)
        .unwrap_or_else(|| panic!("missing scorecard metric {id}"))
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
    assert!(commands.contains_key("final_guardrails_future"));
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
        "ER-REGISTRY-001",
        "ER-EXPLAIN-001",
        "ER-REVIEW-GOLDEN-001",
        "ER-META-001",
        "ER-HOLDOUT-001",
        "ER-RUNTIME-001",
        "ER-MEM-001",
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
        "peak_memory_method",
        "registry_pre_mutation_hash",
        "registry_post_mutation_hash",
        "runtime_guard_status",
        "next_command",
    ] {
        assert!(
            fields.contains(required),
            "missing telemetry field {required}"
        );
    }
}

#[test]
fn entity_eval_metamorphic_relations_are_strong_and_complete() {
    let contract = fixture("evals/entity_eval_performance_targets.json");
    let metric = scorecard_metric(&contract, "ER-META-001");
    let required_relation_ids = metric["required_relation_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect::<BTreeSet<_>>();

    let relations = contract["metamorphic_relations"].as_array().unwrap();
    let mut seen = BTreeSet::new();
    for relation in relations {
        let id = relation["id"].as_str().unwrap();
        assert!(seen.insert(id.to_string()), "duplicate relation id {id}");
        assert!(
            relation["score_strength"].as_f64().unwrap() >= 2.0,
            "{id} has weak metamorphic score"
        );
        assert!(
            relation["expected_invariant"]
                .as_str()
                .is_some_and(|invariant| !invariant.is_empty()),
            "{id} names an invariant"
        );
        assert!(
            relation["allowed_differences"].as_array().is_some(),
            "{id} names allowed differences"
        );
    }

    for required in required_relation_ids {
        assert!(seen.contains(required), "missing relation {required}");
    }
}

#[test]
fn entity_eval_registry_runtime_and_memory_contracts_are_stop_ship_safe() {
    let contract = fixture("evals/entity_eval_performance_targets.json");

    let registry = &contract["registry_mutation_safety"];
    assert_eq!(registry["gates"]["refusal_paths_write_registry"], false);
    assert_eq!(registry["gates"]["stale_registry_snapshot_writes"], false);
    assert_eq!(registry["gates"]["failed_audit_writes"], false);
    assert_eq!(registry["gates"]["atomic_temp_write_required"], true);

    let runtime = &contract["runtime_guards"];
    for forbidden in [
        "network_access_allowed",
        "frontier_model_calls_allowed",
        "runtime_model_downloads_allowed",
        "python_ml_runtime_allowed",
        "general_ml_framework_runtime_allowed",
        "dense_embedding_service_allowed_for_large_corpora",
    ] {
        assert_eq!(runtime[forbidden], false, "{forbidden} must be false");
    }
    assert_eq!(runtime["runtime_guard_verdict_required"], true);

    let targets = contract["peak_memory_targets"].as_array().unwrap();
    let by_id = targets
        .iter()
        .map(|target| (target["id"].as_str().unwrap(), target))
        .collect::<BTreeMap<_, _>>();
    for id in [
        "MEM-SMALL-CI",
        "MEM-CMBS-PUBLIC",
        "MEM-REGAB-FULL",
        "MEM-CMBS-500K",
        "MEM-CMBS-500K-UNIQUE",
    ] {
        assert!(by_id.contains_key(id), "missing {id}");
        assert_eq!(by_id[id]["measurement_method_required"], true);
    }
    assert_eq!(by_id["MEM-SMALL-CI"]["max_bytes"], 268435456);
    assert_eq!(by_id["MEM-CMBS-500K"]["max_bytes"], 2147483648_i64);
    assert_eq!(
        by_id["MEM-CMBS-500K-UNIQUE"]["enforcement"],
        "bounded_completion_or_deterministic_refusal_before_limit"
    );
}

#[test]
fn entity_eval_review_explain_and_holdout_contracts_are_actionable() {
    let contract = fixture("evals/entity_eval_performance_targets.json");

    let explain_sections = contract["explainability_contract"]["required_sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for required in [
        "normalized_views",
        "blocking_candidates",
        "support_evidence",
        "anti_merge_evidence",
        "solver_decision",
        "registry_snapshot",
        "promotion_provenance",
        "next_action",
    ] {
        assert!(explain_sections.contains(required), "missing {required}");
    }

    let review = &contract["review_golden_contract"];
    let artifacts = review["required_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for required in [
        "review.csv",
        "review.jsonl",
        "review.summary.md",
        "review.expected_actions.json",
    ] {
        assert!(artifacts.contains(required), "missing {required}");
    }
    assert_eq!(review["stable_csv_headers_required"], true);
    assert_eq!(review["summary_generated_from_jsonl_required"], true);

    let holdout = &contract["holdout_protocol"];
    assert_eq!(holdout["gates"]["holdout_id_must_be_monotonic"], true);
    assert_eq!(holdout["gates"]["older_holdouts_are_append_only"], true);
    assert_eq!(holdout["gates"]["threshold_lowering_requires_waiver"], true);
    let series = holdout["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["current_holdout_id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert!(series.contains("cmbs-public-v1"));
    assert!(series.contains("regab-baseline-v1"));
}
