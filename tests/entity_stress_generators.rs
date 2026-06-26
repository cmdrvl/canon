#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const CMBS_GENERATOR: &str =
    include_str!("fixtures/entity/cmbs/generators/cmbs_500k_contract.json");
const CMBS_STRESS_GENERATOR: &str =
    include_str!("fixtures/entity/stress/generators/cmbs_500k_contract.json");
const UNIQUE_STRESS_GENERATOR: &str =
    include_str!("fixtures/entity/stress/generators/unique_500k_contract.json");
const PERF_TIERS: &str = include_str!("fixtures/entity/perf/ci_tiers.json");

#[test]
fn entity_stress_generators_deterministic() {
    let cmbs = fixture(CMBS_GENERATOR);
    let cmbs_stress = fixture(CMBS_STRESS_GENERATOR);
    let unique = fixture(UNIQUE_STRESS_GENERATOR);
    let tiers = fixture(PERF_TIERS);

    assert_cmbs_generator_contract(&cmbs, &cmbs_stress, &tiers);
    assert_unique_generator_contract(&unique, &tiers);

    let first = dry_run_manifest(
        &cmbs,
        "cmbs-500k",
        stress_command(&tiers, "cmbs_500k_stress"),
    );
    let second = dry_run_manifest(
        &cmbs,
        "cmbs-500k",
        stress_command(&tiers, "cmbs_500k_stress"),
    );
    assert_eq!(
        serde_json::to_vec(&first).expect("first manifest serializes"),
        serde_json::to_vec(&second).expect("second manifest serializes"),
        "same seed/config must produce byte-identical dry-run metadata"
    );

    let mut changed_seed = cmbs.clone();
    changed_seed["seed"] = json!(cmbs["seed"].as_u64().expect("seed") + 1);
    let changed = dry_run_manifest(
        &changed_seed,
        "cmbs-500k",
        stress_command(&tiers, "cmbs_500k_stress"),
    );
    assert_ne!(
        first, changed,
        "seed must participate in deterministic generator metadata"
    );

    let unique_manifest = dry_run_manifest(
        &unique,
        "unique-500k",
        stress_command(&tiers, "entity_500k_unique_stress"),
    );
    assert_eq!(unique_manifest["workload"], "unique-500k");
    assert_eq!(unique_manifest["row_count"], 500_000);
    assert_eq!(unique_manifest["unique_surface_count"], 500_000);
    assert_eq!(
        unique_manifest["candidate_cap_behavior"],
        "bounded_completion_or_deterministic_refusal"
    );
}

fn assert_cmbs_generator_contract(cmbs: &Value, stress: &Value, tiers: &Value) {
    assert_eq!(
        cmbs["schema_version"],
        "canon.entity.cmbs_500k_generator.v0"
    );
    assert_eq!(cmbs["id"], "cmbs_500k_fixture_shape");
    assert_eq!(cmbs["seed"], 424242);
    assert_eq!(cmbs["profile_id"], "cmbs_tenant_label");
    assert_eq!(cmbs["identity_semantics"], "canonical_display_label");
    assert_eq!(cmbs["row_count"], 500_000);
    assert_eq!(cmbs["deal_count"], 3_000);
    assert_eq!(cmbs["normalized_unique_surface_count"], 25_000);
    assert_eq!(
        cmbs["generator_policy"]["fixture_materialization"],
        "streamed_from_seed"
    );
    assert_eq!(
        cmbs["generator_policy"]["commit_static_generated_rows"],
        false
    );
    assert!(
        cmbs["generator_policy"]["operator_command"]
            .as_str()
            .expect("operator command")
            .contains("cmbs_500k_stress")
    );

    assert_family_totals(cmbs);
    assert_eq!(
        stress["row_count"], cmbs["row_count"],
        "operator stress contract and detailed generator disagree on row count"
    );
    assert_eq!(stress["seed"], cmbs["seed"]);
    assert_eq!(
        stress["prepared_surface_count"],
        cmbs["normalized_unique_surface_count"]
    );
    assert_eq!(stress["deal_count"], cmbs["deal_count"]);

    let candidate = &cmbs["candidate_model"];
    let expected = &cmbs["expected"];
    assert!(candidate["candidate_cap"].as_u64().expect("cap") <= 25);
    assert!(
        candidate["surface_p95_emitted"].as_u64().expect("p95")
            <= expected["candidate_pairs_per_surface_p95_max"]
                .as_u64()
                .expect("p95 max")
    );
    assert!(
        candidate["surface_p99_emitted"].as_u64().expect("p99")
            <= expected["candidate_pairs_per_surface_p99_max"]
                .as_u64()
                .expect("p99 max")
    );

    let exact = &cmbs["exact_bucket_model"];
    assert_eq!(exact["expected_pair_expansion_count"], 0);
    assert!(
        exact["expected_exact_bucket_count"]
            .as_u64()
            .expect("bucket count")
            > 0
    );
    assert!(
        exact["expected_membership_record_count"]
            .as_u64()
            .expect("membership")
            > 0
    );
    let placeholders = exact["placeholder_bucket_values"]
        .as_array()
        .expect("placeholder array")
        .iter()
        .map(|value| value.as_str().expect("placeholder"))
        .collect::<BTreeSet<_>>();
    for placeholder in ["0", "vacant", "n/a", "placeholder:0"] {
        assert!(
            placeholders.contains(placeholder),
            "missing placeholder {placeholder}"
        );
    }

    let command = stress_command(tiers, "cmbs_500k_stress");
    assert!(command.contains("--ignored"));
    assert!(command.contains("cmbs_500k_stress"));
}

fn assert_unique_generator_contract(unique: &Value, tiers: &Value) {
    assert_eq!(unique["schema_version"], "canon.entity.stress_generator.v0");
    assert_eq!(unique["id"], "entity_500k_unique_stress");
    assert_eq!(unique["seed"], 424243);
    assert_eq!(unique["profile_id"], "cmbs_tenant_label");
    assert_eq!(unique["surface_count"], 500_000);
    assert_eq!(unique["topk"]["candidate_cap"], 25);
    assert_eq!(
        unique["virtual_candidate_stream"]["emitted_candidates_per_surface"],
        25
    );
    assert_eq!(
        unique["expected"]["candidate_budget_refusal_code"],
        "E_ENTITY_CANDIDATE_BUDGET"
    );
    assert_eq!(unique["expected"]["exact_bucket_pair_expansion_count"], 0);
    assert!(
        unique["common_token"]
            .as_str()
            .expect("common token")
            .contains("tenant")
    );

    let command = stress_command(tiers, "entity_500k_unique_stress");
    assert!(command.contains("--ignored"));
    assert!(command.contains("entity_500k_unique_stress"));
}

fn assert_family_totals(cmbs: &Value) {
    let families = cmbs["families"].as_array().expect("families");
    let totals = families
        .iter()
        .fold(BTreeMap::<&str, u64>::new(), |mut totals, family| {
            for field in [
                "row_count",
                "raw_unique_surface_count",
                "normalized_unique_surface_count",
                "canonical_label_count",
                "exact_registry_hit_surface_count",
                "hard_negative_group_count",
                "review_group_count",
            ] {
                *totals.entry(field).or_default() += family[field].as_u64().expect(field);
            }
            totals
        });

    assert_eq!(
        totals["row_count"],
        cmbs["row_count"].as_u64().expect("rows")
    );
    assert_eq!(
        totals["raw_unique_surface_count"],
        cmbs["raw_unique_surface_count"]
            .as_u64()
            .expect("raw unique")
    );
    assert_eq!(
        totals["normalized_unique_surface_count"],
        cmbs["normalized_unique_surface_count"]
            .as_u64()
            .expect("normalized unique")
    );
    assert_eq!(
        totals["canonical_label_count"],
        cmbs["canonical_label_count"]
            .as_u64()
            .expect("canonical labels")
    );
    assert!(
        totals["hard_negative_group_count"] > 0,
        "CMBS generator must encode hard-negative families"
    );
    assert!(
        totals["review_group_count"]
            <= cmbs["expected"]["review_group_count_max"]
                .as_u64()
                .expect("review max"),
        "review grouping must stay within G08/G11 budget"
    );
}

fn dry_run_manifest(contract: &Value, workload: &str, command: &str) -> Value {
    let is_unique = workload == "unique-500k";
    let row_count = if is_unique {
        contract["surface_count"].as_u64().expect("surface count")
    } else {
        contract["row_count"].as_u64().expect("row count")
    };
    let unique_surface_count = if is_unique {
        contract["surface_count"].as_u64().expect("surface count")
    } else {
        contract["normalized_unique_surface_count"]
            .as_u64()
            .expect("normalized unique")
    };
    let deal_count = if is_unique {
        0
    } else {
        contract["deal_count"].as_u64().expect("deal count")
    };
    let expected_exact_buckets = if is_unique {
        0
    } else {
        contract["exact_bucket_model"]["expected_exact_bucket_count"]
            .as_u64()
            .expect("exact bucket count")
    };
    let expected_hard_negatives = if is_unique {
        0
    } else {
        contract["families"]
            .as_array()
            .expect("families")
            .iter()
            .map(|family| {
                family["hard_negative_group_count"]
                    .as_u64()
                    .expect("hard negatives")
            })
            .sum()
    };
    let expected_review_groups = if is_unique {
        0
    } else {
        contract["families"]
            .as_array()
            .expect("families")
            .iter()
            .map(|family| {
                family["review_group_count"]
                    .as_u64()
                    .expect("review groups")
            })
            .sum()
    };
    let candidate_cap_behavior = if is_unique {
        "bounded_completion_or_deterministic_refusal"
    } else {
        "bounded_completion"
    };

    json!({
        "schema_version": "canon.entity.stress_generator_dry_run.v0",
        "workload": workload,
        "seed": contract["seed"].as_u64().expect("seed"),
        "profile_id": contract["profile_id"].as_str().expect("profile"),
        "row_count": row_count,
        "unique_surface_count": unique_surface_count,
        "deal_count": deal_count,
        "expected_exact_bucket_count": expected_exact_buckets,
        "expected_hard_negative_group_count": expected_hard_negatives,
        "expected_review_group_count": expected_review_groups,
        "candidate_cap_behavior": candidate_cap_behavior,
        "materialization": "streamed_from_seed",
        "static_generated_rows_committed": false,
        "operator_command": command,
        "metadata_capture": [
            "hardware",
            "os",
            "rust_profile",
            "target_triple",
            "canon_git_sha",
            "cache_state",
            "artifact_hashes"
        ]
    })
}

fn stress_command<'a>(tiers: &'a Value, id: &str) -> &'a str {
    tiers["ignored_stress"]
        .as_array()
        .expect("ignored stress")
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing stress tier {id}"))["command"]
        .as_str()
        .expect("stress command")
}

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture parses")
}
