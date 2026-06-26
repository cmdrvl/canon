#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/tenant_sample_benchmark_manifest.json";
const SELECTOR_PATH: &str =
    "tests/fixtures/entity/cmbs/sample_selectors/tier0_small_golden_selector.json";

#[test]
fn cmbs_tenant_sample_benchmark_manifest_pins_source_counts_and_fields() {
    let manifest = json_fixture(MANIFEST_PATH);

    assert_eq!(
        manifest["version"],
        "canon_entity_cmbs_tenant_benchmark_manifest.v0"
    );
    assert_eq!(manifest["source"]["data_rows"], 6000);
    assert_eq!(manifest["source"]["columns"], 74);
    assert_eq!(manifest["source"]["tenant_slots"], 18_000);
    assert_eq!(manifest["source"]["tenant_observations"], 10_143);
    assert_eq!(manifest["source"]["blank_tenant_slots"], 7_857);
    assert_eq!(manifest["source"]["unique_raw_tenant_names"], 431);
    assert_eq!(manifest["fixture_policy"]["raw_csv_committed"], false);
    assert_eq!(
        manifest["fixture_policy"]["selector_root"],
        "tests/fixtures/entity/cmbs/sample_selectors"
    );

    let ranks = manifest["tenant_fields"]
        .as_array()
        .expect("tenant fields")
        .iter()
        .map(|field| field["rank"].as_u64().expect("rank"))
        .collect::<BTreeSet<_>>();
    assert_eq!(ranks, BTreeSet::from([1, 2, 3]));

    for field in [
        "source_row_id",
        "source_file_sha256",
        "csv_line_number",
        "filing_id",
        "asset_number",
        "tenant_rank",
        "tenant_name_raw",
        "tenant_square_feet",
        "tenant_lease_expiration",
    ] {
        assert!(
            strings(&manifest["observation_shape"]).contains(field),
            "observation shape omits {field}"
        );
    }
}

#[test]
fn cmbs_tenant_sample_benchmark_manifest_covers_required_behavioral_benchmarks() {
    let manifest = json_fixture(MANIFEST_PATH);
    let doc = text_fixture("docs/CMBS_TENANT_BENCHMARKS.md");
    let required = strings(&manifest["required_benchmarks"]);
    let assertions = manifest["benchmark_assertions"]
        .as_object()
        .expect("benchmark assertions");
    let by_id = assertions
        .iter()
        .map(|(id, benchmark)| (id.as_str(), benchmark))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(required.len(), 13);
    assert_eq!(required, by_id.keys().copied().collect::<BTreeSet<_>>());

    for id in &required {
        let benchmark = by_id.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert!(
            doc.contains(id),
            "docs/CMBS_TENANT_BENCHMARKS.md omits {id}"
        );
        assert!(
            benchmark["tier"]
                .as_str()
                .is_some_and(|tier| !tier.trim().is_empty()),
            "{id} must name a tier"
        );
        assert!(
            benchmark["failure_meaning"]
                .as_str()
                .is_some_and(|meaning| !meaning.trim().is_empty()),
            "{id} must explain failure meaning"
        );
        for assertion in strings(&benchmark["assertions"]) {
            assert!(
                !assertion.contains("exists"),
                "{id} has artifact-exists assertion: {assertion}"
            );
        }
    }
}

#[test]
fn cmbs_tenant_sample_benchmark_manifest_pins_goldens_review_and_hard_negatives() {
    let manifest = json_fixture(MANIFEST_PATH);

    let clusters = manifest["must_link_clusters"]
        .as_array()
        .expect("must_link_clusters array");
    let cluster_ids = clusters
        .iter()
        .map(|cluster| cluster["id"].as_str().expect("cluster id"))
        .collect::<BTreeSet<_>>();
    for expected in [
        "TNT-238-SAND-ISLAND-PROPERTY",
        "TNT-2020-AUTO-BODY",
        "TNT-24-HOUR-FITNESS",
        "TNT-FOOT-LOCKER",
        "TNT-TJ-MAXX",
        "TNT-10X-GENOMICS",
        "TNT-23ANDME",
        "TNT-1-LIFE-HEALTHCARE",
        "TNT-TAVERN-BOWL",
        "TNT-PANGAEA-OUTPOST",
        "TNT-TWO-TAILS",
        "TNT-ETHOS-LENDING",
        "TNT-MGA-ENTERTAINMENT",
    ] {
        assert!(cluster_ids.contains(expected), "missing cluster {expected}");
    }

    let fitness = clusters
        .iter()
        .find(|cluster| cluster["id"] == "TNT-24-HOUR-FITNESS")
        .expect("24 Hour Fitness cluster");
    assert_eq!(fitness["observations"], 681);
    assert!(
        strings(&fitness["variants"]).contains("24 HR Fitness"),
        "24 Hour Fitness cluster must pin short-form variant"
    );

    let hard_negative_pairs = pair_set(&manifest["hard_negative_pairs"]);
    for expected in [
        ("2020 Auto Body, LLC", "2020 Broadway Ave"),
        ("100 Riverside Parking LLC", "220 Riverside Parking LLC"),
        ("1OAK", "1 Life Healthcare Inc"),
        ("24 Hour Fitness", "24 Hour Club"),
        ("Triangle Cinemas", "TIME NIGHT CLUB"),
        (
            "MGA Entertainment Inc., a California corporation",
            "San Fernando Valley Mental Health Center, Inc.",
        ),
    ] {
        assert!(
            hard_negative_pairs.contains(&expected),
            "missing hard-negative pair {expected:?}"
        );
    }

    let review_ids = manifest["review_cases"]
        .as_array()
        .expect("review cases")
        .iter()
        .map(|case| case["id"].as_str().expect("review id"))
        .collect::<BTreeSet<_>>();
    for expected in [
        "CMBS-REVIEW-CHINA-KING",
        "CMBS-REVIEW-RANDALLS-TOM-THUMB",
        "CMBS-REVIEW-WEWORK-SHELLS",
        "CMBS-REVIEW-TEPPER-SOUTHERN-STATES",
        "CMBS-REVIEW-FORSYTH-WAYLA",
    ] {
        assert!(
            review_ids.contains(expected),
            "missing review case {expected}"
        );
    }
}

#[test]
fn cmbs_tenant_sample_selector_contract_is_deterministic_without_committing_raw_csv() {
    let selector = json_fixture(SELECTOR_PATH);
    let manifest = json_fixture(MANIFEST_PATH);

    assert_eq!(
        selector["schema_version"],
        "canon.entity.cmbs_sample_selector.v0"
    );
    assert_eq!(selector["source_sha256"], manifest["source"]["sha256"]);
    assert_eq!(selector["raw_csv_committed"], false);
    assert_eq!(selector["deterministic"], true);
    assert_eq!(
        selector["source_required_when"],
        "operator_full_sample_or_selector_regeneration"
    );
    assert_eq!(
        strings(&selector["selected_benchmark_ids"]),
        strings(&manifest["required_benchmarks"])
    );

    let coverage = &selector["expected_coverage"];
    assert_eq!(coverage["placeholder_surface"], "0");
    assert_eq!(coverage["must_link_clusters_min"], 13);
    assert_eq!(coverage["hard_negative_pairs_min"], 13);
    assert_eq!(coverage["review_cases_min"], 5);
    assert_eq!(coverage["tenant_rank_fields"], serde_json::json!([1, 2, 3]));
    assert_eq!(
        selector["output_contract"]["raw_fields_byte_preserved"],
        true
    );
    assert_eq!(
        selector["output_contract"]["source_row_id_semantics"],
        "provenance_only_not_identity"
    );
}

#[test]
fn cmbs_tenant_sample_runtime_and_performance_guardrails_are_structural() {
    let manifest = json_fixture(MANIFEST_PATH);
    let performance = &manifest["performance_contract"];

    assert_eq!(performance["candidate_pairs_per_surface_p95_max"], 25);
    assert_eq!(performance["candidate_pairs_per_surface_p99_max"], 100);
    assert_eq!(performance["exact_bucket_pair_expansion_count"], 0);
    assert_eq!(
        performance["wall_clock_enforcement"],
        "operator_release_only_after_telemetry_baseline"
    );
    assert_eq!(performance["normal_ci_wall_clock_assertions"], false);

    let prohibited = strings(&manifest["prohibited_runtime_dependencies"]);
    for required in [
        "network",
        "frontier_model_call",
        "runtime_model_download",
        "python_ml_runtime",
        "general_ml_framework",
    ] {
        assert!(
            prohibited.contains(required),
            "missing prohibited {required}"
        );
    }

    let reasons = strings(&manifest["normalization_reason_codes_required"]);
    for required in [
        "legal_suffix_removed",
        "punctuation_folded",
        "repeated_spaces_folded",
        "all_caps_folded",
        "store_number_stripped",
        "leading_record_number_stripped",
    ] {
        assert!(reasons.contains(required), "missing reason {required}");
    }

    assert_eq!(
        manifest["candidate_recall_policy"]["must_link_pairs"],
        "candidate_or_compact_exact_bucket"
    );
    assert_eq!(
        manifest["hard_negative_policy"]["auto_merge_allowed"],
        false
    );
}

fn json_fixture(path: &str) -> Value {
    serde_json::from_str(&text_fixture(path))
        .unwrap_or_else(|error| panic!("{path} parses: {error}"))
}

fn text_fixture(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("{path} opens: {error}"))
}

fn strings(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|value| value.as_str().expect("string"))
        .collect()
}

fn pair_set(value: &Value) -> BTreeSet<(&str, &str)> {
    value
        .as_array()
        .expect("pairs")
        .iter()
        .map(|pair| {
            let pair = pair.as_array().expect("pair array");
            (
                pair[0].as_str().expect("left"),
                pair[1].as_str().expect("right"),
            )
        })
        .collect()
}
