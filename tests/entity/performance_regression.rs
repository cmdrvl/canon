#![forbid(unsafe_code)]

use canon::entity::telemetry::{required_telemetry_fields, required_telemetry_stage_ids};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

#[test]
fn entity_performance_regression_contract() {
    let cmbs = fixture("stress/expected_metrics/cmbs_500k_metrics.json");
    let unique = fixture("stress/expected_metrics/unique_500k_metrics.json");
    let cmbs_generator = fixture("stress/generators/cmbs_500k_contract.json");
    let unique_generator = fixture("stress/generators/unique_500k_contract.json");
    let tiers = fixture("perf/ci_tiers.json");
    let evals = fixture("evals/entity_eval_performance_targets.json");

    assert_expected_metrics_match_generator(&cmbs, &cmbs_generator, &tiers, &evals);
    assert_expected_metrics_match_generator(&unique, &unique_generator, &tiers, &evals);
    assert_cache_and_telemetry_contract(&cmbs);
    assert_cache_and_telemetry_contract(&unique);
}

fn assert_expected_metrics_match_generator(
    metrics: &Value,
    generator: &Value,
    tiers: &Value,
    evals: &Value,
) {
    assert_eq!(
        metrics["schema_version"],
        "canon.entity.stress_expected_metrics.v0"
    );
    assert_eq!(metrics["generator_id"], generator["id"]);
    assert_eq!(metrics["gate_id"], generator["gate_id"]);
    assert_eq!(metrics["seed"], generator["seed"]);
    assert_eq!(metrics["profile_id"], generator["profile_id"]);
    assert_eq!(
        metrics["static_generated_rows_committed"], false,
        "large generated rows must stay streamed, not committed"
    );

    let workload = metrics["workload"].as_str().expect("workload");
    let input = &metrics["input_size"];
    let candidates = &metrics["candidate_metrics"];
    let exact = &metrics["exact_bucket_metrics"];
    let outcome = &metrics["expected_outcome"];

    let unique_surface_count = input["unique_surface_count"]
        .as_u64()
        .expect("unique surface count");
    let emitted_per_surface = candidates["emitted_candidates_per_surface_p50"]
        .as_u64()
        .expect("p50 emitted");
    let suppressed_per_surface = candidates["suppressed_candidates_per_surface"]
        .as_u64()
        .expect("suppressed per surface");

    assert_eq!(
        candidates["emitted_candidate_pair_count"],
        unique_surface_count.saturating_mul(emitted_per_surface)
    );
    assert_eq!(
        candidates["suppressed_candidate_count"],
        unique_surface_count.saturating_mul(suppressed_per_surface)
    );
    assert_eq!(exact["exact_bucket_pair_expansion_count"], 0);
    assert_eq!(
        exact["exact_bucket_pair_expansion_count"],
        generator["expected"]["exact_bucket_pair_expansion_count"]
    );

    let gates = &evals["structural_performance_gates"];
    assert!(
        candidates["candidate_pairs_per_surface_p95_max"]
            .as_u64()
            .expect("p95 max")
            <= gates["candidate_pairs_per_surface_p95_max"]
                .as_u64()
                .expect("gate p95")
    );
    assert!(
        candidates["candidate_pairs_per_surface_p99_max"]
            .as_u64()
            .expect("p99 max")
            <= gates["candidate_pairs_per_surface_p99_max"]
                .as_u64()
                .expect("gate p99")
    );

    match workload {
        "cmbs-500k" => assert_cmbs_metrics(metrics, generator, gates),
        "unique-500k" => assert_unique_metrics(metrics, generator, gates),
        other => panic!("unknown stress workload {other}"),
    }

    let tier = stress_tier(
        tiers,
        metrics["stress_tier_id"].as_str().expect("stress tier id"),
    );
    assert_eq!(metrics["operator_log_contract"]["command"], tier["command"]);
    assert_eq!(
        metrics["operator_log_contract"]["telemetry_out"],
        tier["telemetry_out"]
    );
    assert!(
        metrics["operator_log_contract"]["command"]
            .as_str()
            .expect("operator command")
            .contains("--ignored")
    );
    assert_eq!(
        outcome["wall_clock_enforcement"],
        "not_enforced_until_measured_baseline"
    );
    assert_eq!(metrics["baseline_policy"]["calibrated_by"], "bd-1pz.9");
}

fn assert_cmbs_metrics(metrics: &Value, generator: &Value, gates: &Value) {
    assert_eq!(metrics["input_size"]["row_count"], generator["row_count"]);
    assert_eq!(
        metrics["input_size"]["unique_surface_count"],
        generator["prepared_surface_count"]
    );
    assert_eq!(metrics["input_size"]["deal_count"], generator["deal_count"]);
    assert_eq!(
        metrics["candidate_metrics"]["candidate_cap"],
        generator["topk"]["candidate_cap"]
    );
    assert_eq!(
        metrics["candidate_metrics"]["emitted_candidates_per_surface_p50"],
        generator["virtual_candidate_stream"]["emitted_candidates_per_surface"]
    );
    assert_eq!(
        metrics["candidate_metrics"]["suppressed_candidates_per_surface"],
        generator["virtual_candidate_stream"]["suppressed_candidates_per_surface"]
    );
    assert_eq!(
        metrics["exact_bucket_metrics"]["exact_bucket_count"],
        generator["exact_bucket_count"]
    );
    assert_eq!(
        metrics["exact_bucket_metrics"]["largest_exact_bucket_size"],
        generator["largest_exact_bucket_size"]
    );
    assert_eq!(
        metrics["review_metrics"]["review_group_count"],
        generator["review_group_count"]
    );
    assert!(
        metrics["review_metrics"]["review_group_count"]
            .as_u64()
            .expect("review groups")
            <= gates["review_groups_per_500k_cmbs_backfill_max"]
                .as_u64()
                .expect("review gate")
    );
    assert_eq!(metrics["expected_outcome"]["kind"], "success");
    assert_eq!(metrics["expected_outcome"]["refusal_code"], "none");
}

fn assert_unique_metrics(metrics: &Value, generator: &Value, gates: &Value) {
    assert_eq!(
        metrics["input_size"]["row_count"], generator["surface_count"],
        "500k-unique stress treats each surface as one synthetic row"
    );
    assert_eq!(
        metrics["input_size"]["unique_surface_count"],
        generator["surface_count"]
    );
    assert_eq!(metrics["input_size"]["deal_count"], 0);
    assert_eq!(
        metrics["candidate_metrics"]["candidate_cap"],
        generator["topk"]["candidate_cap"]
    );
    assert_eq!(
        metrics["candidate_metrics"]["emitted_candidates_per_surface_p50"],
        generator["virtual_candidate_stream"]["emitted_candidates_per_surface"]
    );
    assert_eq!(
        metrics["candidate_metrics"]["suppressed_candidates_per_surface"],
        generator["virtual_candidate_stream"]["suppressed_candidates_per_surface"]
    );
    assert_eq!(metrics["exact_bucket_metrics"]["exact_bucket_count"], 0);
    assert_eq!(
        metrics["candidate_metrics"]["large_buckets_suppressed_behavior"],
        "common_token_bucket_suppressed_before_pair_expansion"
    );
    assert!(
        metrics["candidate_metrics"]["large_buckets_suppressed_min"]
            .as_u64()
            .expect("suppressed min")
            > 0
    );
    assert_eq!(
        metrics["expected_outcome"]["kind"],
        gates["unique_500k_required_behavior"]
    );
    assert_eq!(
        metrics["expected_outcome"]["refusal_code"],
        generator["expected"]["candidate_budget_refusal_code"]
    );
}

fn assert_cache_and_telemetry_contract(metrics: &Value) {
    let cache = &metrics["cache_hit_behavior"];
    let cache_states = strings(&cache["expected_cache_state_sequence"]);
    assert_eq!(cache_states, BTreeSet::from(["cold", "warm"]));

    let skip_steps = strings(&cache["warm_rerun_must_skip"]);
    for required in ["prepare_normalization", "posting_index_rebuild"] {
        assert!(
            skip_steps.contains(required),
            "warm cache rerun must skip {required}"
        );
    }

    let dimensions = strings(&cache["cache_key_dimensions"]);
    for required in [
        "input_hash",
        "profile_hash",
        "strategy_hash",
        "registry_snapshot_hash",
        "patch_hash",
        "namekit_hash",
    ] {
        assert!(
            dimensions.contains(required),
            "cache key must include {required}"
        );
    }

    let telemetry = &metrics["telemetry_requirements"];
    assert_eq!(
        strings(&telemetry["stage_ids"]),
        required_telemetry_stage_ids()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    );
    let fields = strings(&telemetry["required_fields"]);
    for required in required_telemetry_fields() {
        assert!(
            fields.contains(required),
            "expected metrics omit telemetry field {required}"
        );
    }

    let metadata = strings(&metrics["operator_log_contract"]["required_metadata"]);
    for required in [
        "hardware",
        "os",
        "rust_profile",
        "target_triple",
        "canon_git_sha",
        "cache_state",
        "artifact_hashes",
        "fixture_seed",
        "input_size",
        "unique_surface_count",
    ] {
        assert!(
            metadata.contains(required),
            "operator log must capture {required}"
        );
    }

    assert!(
        metrics["memory"]["peak_memory_bytes_max"]
            .as_u64()
            .expect("peak memory max")
            > 0
    );
    assert_eq!(metrics["memory"]["high_water_mark_required"], true);
}

fn stress_tier<'a>(tiers: &'a Value, id: &str) -> &'a Value {
    tiers["ignored_stress"]
        .as_array()
        .expect("ignored stress tiers")
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing stress tier {id}"))
}

fn strings(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item"))
        .collect()
}

fn fixture(path: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity")
        .join(path);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture opens")).expect("fixture parses")
}
