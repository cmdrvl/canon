#![forbid(unsafe_code)]

use canon::namekit::tenant::cmbs_tenant_pair_evidence;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/cmbs/small_book")
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json fixture opens"))
        .expect("json fixture parses")
}

#[test]
#[allow(non_snake_case)]
fn CMBS_I001_small_book_global_surfaces_and_exact_hits() {
    let rows = observation_rows();
    let expected = expected_summary();

    assert_eq!(
        expected["schema_version"],
        "canon.entity.cmbs.small_book.v0"
    );
    assert_eq!(expected["profile_id"], "cmbs_tenant_label");
    assert_eq!(expected["identity_semantics"], "canonical_display_label");
    assert_eq!(
        rows.len(),
        expected["source"]["row_count"].as_u64().unwrap() as usize
    );

    let deal_count = unique_count_owned(&rows, "deal_id");
    let property_count = unique_count_owned(&rows, "property_id");
    assert_eq!(
        deal_count,
        expected["source"]["deal_count"].as_u64().unwrap()
    );
    assert_eq!(
        property_count,
        expected["source"]["property_count"].as_u64().unwrap()
    );

    let surface_groups = grouped_by_surface(&rows);
    assert_eq!(
        surface_groups.len() as u64,
        expected["prepare_summary"]["normalized_unique_surfaces"]
            .as_u64()
            .unwrap()
    );
    assert_surface_group(&surface_groups, "sears", 3, 3, 3);
    assert_surface_group(&surface_groups, "24 hour fitness", 3, 3, 3);
    assert_surface_group(&surface_groups, "238 sand island property", 2, 2, 2);

    for group in expected["must_link_groups"].as_array().unwrap() {
        let normalized_surface = group["normalized_surface"].as_str().unwrap();
        let rows = surface_groups.get(normalized_surface).unwrap_or_else(|| {
            panic!("missing surface group {normalized_surface}");
        });
        assert_eq!(rows.len() as u64, group["row_count"].as_u64().unwrap());
        assert_eq!(
            unique_count_borrowed(rows, "deal_id"),
            group["deal_count"].as_u64().unwrap()
        );
        assert_eq!(
            unique_count_borrowed(rows, "property_id"),
            group["property_count"].as_u64().unwrap()
        );
        assert!(
            group["expected_canonical_id"]
                .as_str()
                .unwrap()
                .starts_with("TNT-")
        );
    }

    let exact_rows = rows
        .iter()
        .filter(|row| row["expected_resolution_status"] == "exact_resolved")
        .collect::<Vec<_>>();
    assert_eq!(
        exact_rows.len() as u64,
        expected["prepare_summary"]["exact_resolved_row_count"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        exact_rows
            .iter()
            .map(|row| row["expected_canonical_id"].as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        expected["prepare_summary"]["exact_resolved_surface_count"]
            .as_u64()
            .unwrap()
    );
}

#[test]
fn cmbs_profile_integration_exact_hits_skip_fuzzy_blocking() {
    let expected = expected_summary();
    let skipped = string_set(
        &expected["stage_summaries"]["block"]["exact_resolved_surfaces_skipped_by_fuzzy_blocking"],
    );
    let exact_surfaces = expected["exact_resolved_surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|surface| surface["normalized_surface"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(skipped, exact_surfaces);
    assert_eq!(
        expected["stage_summaries"]["index"]["exact_bucket_pair_expansion_count"],
        0
    );

    let candidate_pairs = expected["stage_summaries"]["block"]["candidate_pairs"]
        .as_array()
        .unwrap();
    for pair in candidate_pairs {
        let pair = pair.as_array().unwrap();
        assert!(
            !(skipped.contains(pair[0].as_str().unwrap())
                && skipped.contains(pair[1].as_str().unwrap())),
            "exact-resolved surfaces must not be expanded into fuzzy candidate pairs"
        );
    }
    assert!(
        expected["stage_summaries"]["block"]["candidate_pairs_per_surface_p95"]
            .as_u64()
            .unwrap()
            <= 25
    );
    assert!(
        expected["stage_summaries"]["block"]["candidate_pairs_per_surface_p99"]
            .as_u64()
            .unwrap()
            <= 100
    );
}

#[test]
fn cmbs_profile_integration_hard_negatives_do_not_collapse() {
    let expected = expected_summary();
    for pair in expected["hard_negative_pairs"].as_array().unwrap() {
        let left = pair["left"].as_str().unwrap();
        let right = pair["right"].as_str().unwrap();
        let evidence = cmbs_tenant_pair_evidence(left, right);
        assert_eq!(pair["expected_auto_merge"], false);
        assert!(
            !evidence.same_tenant_label_support,
            "{left} vs {right} must not be same-label support"
        );
        if !evidence.shared_tokens.is_empty() {
            assert!(
                evidence.requires_review,
                "{left} vs {right} with shared tokens must require review"
            );
        }
        assert!(
            pair["reason_code"]
                .as_str()
                .is_some_and(|reason| reason.contains("not_same_tenant_label")
                    || reason.contains("not_display_label"))
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn EN_R001_small_book_review_groups_include_row_and_deal_counts() {
    let expected = expected_summary();
    let queue = review_queue_rows();

    assert_eq!(
        queue.len() as u64,
        expected["stage_summaries"]["review"]["group_count"]
            .as_u64()
            .unwrap()
    );
    let expected_by_id = expected["review_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| (group["id"].as_str().unwrap(), group))
        .collect::<BTreeMap<_, _>>();
    for row in queue {
        let group = expected_by_id
            .get(row["review_group_id"].as_str())
            .unwrap_or_else(|| panic!("unexpected review group {}", row["review_group_id"]));
        assert_eq!(row["benchmark_id"], "EN_R001");
        assert_eq!(row["reason_code"], group["reason_code"]);
        assert_eq!(
            row["row_count"].parse::<u64>().unwrap(),
            group["row_count"].as_u64().unwrap()
        );
        assert_eq!(
            row["deal_count"].parse::<u64>().unwrap(),
            group["deal_count"].as_u64().unwrap()
        );
        assert_eq!(
            row["property_count"].parse::<u64>().unwrap(),
            group["property_count"].as_u64().unwrap()
        );
        let representatives: Value =
            serde_json::from_str(&row["representative_surfaces_json"]).unwrap();
        assert_eq!(
            representatives.as_array().unwrap().len(),
            group["representative_surfaces"].as_array().unwrap().len()
        );
        assert_eq!(row["suggested_action"], group["suggested_action"]);
    }
}

#[test]
fn cmbs_profile_integration_stage_summaries_are_structural_not_row_pair_theatre() {
    let expected = expected_summary();
    assert_eq!(expected["stage_summaries"]["prepare"]["raw_rows"], 15);
    assert_eq!(
        expected["stage_summaries"]["prepare"]["prepared_unique_surfaces"],
        expected["prepare_summary"]["normalized_unique_surfaces"]
    );
    assert!(
        expected["stage_summaries"]["block"]["candidate_source_surfaces"]
            .as_u64()
            .unwrap()
            < expected["stage_summaries"]["prepare"]["raw_rows"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(
        expected["stage_summaries"]["solve"]["cannot_link_count"],
        expected["stage_summaries"]["edge"]["anti_merge_edges"]
    );
    assert_eq!(
        expected["stage_summaries"]["solve"]["review_group_count"],
        expected["stage_summaries"]["review"]["group_count"]
    );

    let non_goals = string_set(&expected["non_goals"]);
    for required in [
        "no legal-entity claim for tenant labels",
        "no row-level all-pairs candidate generation",
        "no exact-bucket pair expansion",
        "no frontier model call",
        "no network dependency",
    ] {
        assert!(non_goals.contains(required));
    }
}

fn expected_summary() -> Value {
    read_json(fixture_root().join("expected_summary.json"))
}

fn observation_rows() -> Vec<BTreeMap<String, String>> {
    csv::Reader::from_path(fixture_root().join("observations.csv"))
        .expect("observations csv opens")
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("observations csv parses")
}

fn review_queue_rows() -> Vec<BTreeMap<String, String>> {
    csv::Reader::from_path(fixture_root().join("review_queue.csv"))
        .expect("review queue csv opens")
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("review queue csv parses")
}

fn grouped_by_surface(
    rows: &[BTreeMap<String, String>],
) -> BTreeMap<&str, Vec<&BTreeMap<String, String>>> {
    let mut groups = BTreeMap::<&str, Vec<&BTreeMap<String, String>>>::new();
    for row in rows {
        groups
            .entry(row["expected_normalized_surface"].as_str())
            .or_default()
            .push(row);
    }
    groups
}

fn assert_surface_group(
    groups: &BTreeMap<&str, Vec<&BTreeMap<String, String>>>,
    surface: &str,
    row_count: usize,
    deal_count: u64,
    property_count: u64,
) {
    let rows = groups
        .get(surface)
        .unwrap_or_else(|| panic!("missing surface {surface}"));
    assert_eq!(rows.len(), row_count);
    assert_eq!(unique_count_borrowed(rows, "deal_id"), deal_count);
    assert_eq!(unique_count_borrowed(rows, "property_id"), property_count);
}

fn unique_count_owned(rows: &[BTreeMap<String, String>], field: &str) -> u64 {
    rows.iter()
        .map(|row| row[field].as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn unique_count_borrowed(rows: &[&BTreeMap<String, String>], field: &str) -> u64 {
    rows.iter()
        .map(|row| row[field].as_str())
        .collect::<BTreeSet<_>>()
        .len() as u64
}

fn string_set(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item"))
        .collect()
}
