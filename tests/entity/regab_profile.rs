use canon::entity::profile::EntityProfileDocument;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const REGAB_PROFILE: &str = include_str!("../fixtures/entity/profiles/regab_firm_identity.yaml");
const REGAB_CASES: &str =
    include_str!("../fixtures/entity/profiles/regab_firm_identity/integration_cases.json");

#[allow(non_snake_case)]
#[test]
fn REGAB_I001_profile_fixture_accepts_org_mentions_without_parser_rewrite() {
    let cases = regab_cases();
    let summary = expected_summary(&cases);
    let profile =
        EntityProfileDocument::from_yaml_str(REGAB_PROFILE).expect("regab profile validates");
    let case = case(&cases, "REGAB-I001");
    let headers = csv_headers(&fixture_path(&cases, "org_mentions_csv"));
    let required = string_array(&case["required_fields"]);

    assert_eq!(
        cases["schema_version"],
        "canon.entity.regab.profile_integration.v0"
    );
    assert_eq!(cases["profile_id"], profile.profile);
    assert_eq!(cases["identity_semantics"], profile.identity_semantics);
    assert_eq!(cases["canonical_type"], profile.canonical_type);
    assert!(required.iter().all(|field| headers.contains(field)));
    assert_eq!(
        profile.required_fields,
        ["source_row_id", "field_name", "org_name", "dataset"]
    );
    assert_eq!(
        summary["source"]["row_count"],
        case["expected_counts"]["row_count"]
    );
    assert_eq!(
        summary["source"]["prepared_surfaces"],
        case["expected_counts"]["prepared_surfaces"]
    );
    assert_eq!(
        summary["source"]["exact_resolved_surfaces"],
        case["expected_counts"]["exact_resolved_surfaces"]
    );
    assert_eq!(
        summary["source"]["unresolved_surfaces"],
        case["expected_counts"]["unresolved_surfaces"]
    );
}

#[allow(non_snake_case)]
#[test]
fn REGAB_I002_pnc_and_midland_remain_distinct_or_reviewable() {
    let cases = regab_cases();
    let summary = expected_summary(&cases);
    let case = case(&cases, "REGAB-I002");
    let expected_pair = &case["guarded_pair"];
    let guarded_pair = guarded_pair(
        &summary,
        expected_pair["left"].as_str().expect("left"),
        expected_pair["right"].as_str().expect("right"),
    );

    assert_eq!(guarded_pair["guard"], expected_pair["guard"]);
    assert_eq!(guarded_pair["relation"], expected_pair["relation"]);
    assert_eq!(
        guarded_pair["expected_review_priority"],
        expected_pair["expected_review_priority"]
    );
    assert_eq!(expected_pair["expected_auto_merge"], false);
    assert_eq!(summary["solve_summary"]["auto_merge_candidate_count"], 0);
    assert_eq!(summary["solve_summary"]["hard_cannot_link_count"], 3);
}

#[allow(non_snake_case)]
#[test]
fn REGAB_I003_platform_labels_and_role_conflicts_are_not_firms() {
    let cases = regab_cases();
    let summary = expected_summary(&cases);
    let case = case(&cases, "REGAB-I003");
    let unresolved_expected = string_array(&case["unresolved_surfaces"]);
    let unresolved_actual = string_array(&summary["unresolved_surfaces"]);

    for surface in &unresolved_expected {
        assert!(
            unresolved_actual.contains(surface),
            "{surface} remains unresolved/reviewable"
        );
    }

    for expected_pair in case["guarded_pairs"].as_array().expect("guarded pairs") {
        let guarded = guarded_pair(
            &summary,
            expected_pair["left"].as_str().expect("left"),
            expected_pair["right"].as_str().expect("right"),
        );
        assert_eq!(guarded["guard"], expected_pair["guard"]);
        assert_eq!(guarded["relation"], expected_pair["relation"]);
        assert_eq!(expected_pair["expected_auto_merge"], false);
        assert_eq!(
            guarded["expected_review_priority"],
            expected_pair["expected_review_priority"]
        );
    }
}

#[allow(non_snake_case)]
#[test]
fn REGAB_I004_apply_preserves_raw_parser_fields_and_appends_canonical_fields() {
    let cases = regab_cases();
    let summary = expected_summary(&cases);
    let case = case(&cases, "REGAB-I004");
    let apply_headers = csv_headers(&fixture_path(&cases, "expected_apply_csv"));
    let append_only = string_vec(&case["append_only_fields"]);
    let preserved_raw = string_vec(&case["preserved_raw_fields"]);
    let canonical_start = apply_headers
        .len()
        .checked_sub(append_only.len())
        .expect("canonical suffix fits");
    let raw_prefix = &apply_headers[..canonical_start];

    assert_eq!(
        &apply_headers[canonical_start..],
        append_only.as_slice(),
        "canonical fields are appended only"
    );
    assert!(
        preserved_raw.iter().all(|field| raw_prefix.contains(field)),
        "expected raw/review prefix fields are preserved before canonical suffix"
    );
    assert!(
        raw_prefix.iter().all(|field| !append_only.contains(field)),
        "canonical fields must not appear before the appended suffix"
    );
    assert_eq!(
        summary["apply"]["rows"],
        case["expected_apply_counts"]["rows"]
    );
    assert_eq!(
        summary["apply"]["resolved"],
        case["expected_apply_counts"]["resolved"]
    );
    assert_eq!(
        summary["apply"]["unresolved"],
        case["expected_apply_counts"]["unresolved"]
    );
    assert_eq!(
        string_vec(&summary["apply"]["canonical_fields"]),
        append_only
    );
    assert_eq!(
        string_vec(&summary["apply"]["downstream_org_fields"]),
        string_vec(&case["downstream_org_suffixes"])
    );
}

#[test]
fn regab_profile_fixture_keeps_profile_firewall_explicit() {
    let cases = regab_cases();
    assert_eq!(cases["profile_firewall"]["not_tenant_label"], true);
    assert_eq!(cases["profile_firewall"]["not_hierarchy_discovery"], true);
    assert_eq!(
        cases["profile_firewall"]["not_parent_subsidiary_auto_merge"],
        true
    );
}

fn regab_cases() -> Value {
    serde_json::from_str(REGAB_CASES).expect("REGAB profile cases parse")
}

fn expected_summary(cases: &Value) -> Value {
    read_json(&fixture_path(cases, "expected_summary"))
}

fn case<'a>(cases: &'a Value, id: &str) -> &'a Value {
    cases["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["id"] == id)
        .expect("case exists")
}

fn guarded_pair<'a>(summary: &'a Value, left: &str, right: &str) -> &'a Value {
    summary["guarded_pairs"]
        .as_array()
        .expect("guarded pairs")
        .iter()
        .find(|pair| pair["left"] == left && pair["right"] == right)
        .expect("guarded pair exists")
}

fn fixture_path(cases: &Value, key: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        cases["fixture_sources"][key]
            .as_str()
            .expect("fixture source"),
    )
}

fn csv_headers(path: &Path) -> Vec<String> {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    reader
        .headers()
        .expect("headers")
        .iter()
        .map(ToOwned::to_owned)
        .collect()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn string_array(value: &Value) -> Vec<String> {
    string_vec(value)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn string_vec(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect()
}
