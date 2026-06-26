#![forbid(unsafe_code)]

use canon::entity::apply::SEC10D_ORG_FIELD_SUFFIXES;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[test]
fn sec10d_org_mentions_contract() {
    let manifest = manifest();
    assert_eq!(
        manifest["schema_version"],
        "canon.entity.sec10d_contract_manifest.v0"
    );
    assert_eq!(manifest["profile_id"], "regab_firm_identity");
    assert_eq!(
        manifest["identity_semantics"],
        "same_firm_or_reviewed_alias"
    );
    assert_eq!(
        manifest["source_baseline"]["sha256"],
        "5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b"
    );
    assert_snapshot_hashes(&manifest);

    let expected_columns = string_array(&manifest["required_org_mentions_columns"]);
    let mut reader = csv::Reader::from_path(fixture_root().join("org_mentions.csv"))
        .expect("org_mentions csv opens");
    let csv_headers = reader
        .headers()
        .expect("headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(csv_headers, expected_columns);
    assert_eq!(reader.records().count(), 8);

    for object in read_jsonl_objects(fixture_root().join("org_mentions.jsonl")) {
        for column in &expected_columns {
            assert!(
                object.contains_key(column),
                "jsonl source row has required field {column}"
            );
        }
    }
}

#[test]
fn sec10d_raw_parser_field_preservation() {
    let source_rows = read_jsonl_objects(fixture_root().join("org_mentions.jsonl"))
        .into_iter()
        .map(|object| {
            (
                object["source_row_id"]
                    .as_str()
                    .expect("source_row_id")
                    .to_string(),
                object,
            )
        })
        .collect::<BTreeMap<_, _>>();

    for enriched in read_jsonl_objects(fixture_root().join("applied_org_enrichment.jsonl")) {
        let source_row_id = enriched["source_row_id"].as_str().expect("source_row_id");
        let source = source_rows
            .get(source_row_id)
            .unwrap_or_else(|| panic!("unknown enriched source row {source_row_id}"));
        for (field, value) in source {
            assert_eq!(
                enriched.get(field),
                Some(value),
                "raw parser field {field} changed for {source_row_id}"
            );
        }
    }
}

#[test]
fn sec10d_snowflake_append_only_fields() {
    let manifest = manifest();
    let approved_suffixes = string_array(&manifest["approved_snowflake_org_suffixes"]);
    assert_eq!(
        approved_suffixes,
        SEC10D_ORG_FIELD_SUFFIXES
            .iter()
            .map(|suffix| (*suffix).to_string())
            .collect::<Vec<_>>()
    );

    for enriched in read_jsonl_objects(fixture_root().join("applied_org_enrichment.jsonl")) {
        for key in enriched.keys().filter(|key| key.starts_with("canonical_")) {
            panic!("sec10d enriched JSONL must not use generic apply field {key}");
        }

        let org_fields = enriched
            .keys()
            .filter(|key| key.contains("_org_"))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(!org_fields.is_empty(), "row should append *_org_* fields");
        for field in &org_fields {
            assert!(
                approved_suffixes
                    .iter()
                    .any(|suffix| field.ends_with(suffix)),
                "unexpected Snowflake org field {field}"
            );
        }

        let statuses = enriched
            .iter()
            .filter(|(key, _)| key.ends_with("_org_resolution_status"))
            .map(|(_, value)| value.as_str().expect("status is string"))
            .collect::<BTreeSet<_>>();
        assert!(
            statuses
                .iter()
                .all(|status| matches!(*status, "resolved_exact" | "review_required")),
            "unexpected resolution statuses: {statuses:?}"
        );
    }
}

#[test]
fn sec10d_regab_boundary_cases_remain_review_or_distinct() {
    let manifest = manifest();
    let expected_summary = read_json_value(fixture_root().join("expected_summary.json"));
    let hard_negative_cases = hard_negative_cases(&manifest);
    let org_mentions_names = read_jsonl_objects(fixture_root().join("org_mentions.jsonl"))
        .into_iter()
        .map(|object| object["org_name"].as_str().expect("org_name").to_string())
        .collect::<BTreeSet<_>>();

    let boundary_cases = manifest["required_boundary_cases"]
        .as_array()
        .expect("required boundary cases array");
    assert_eq!(boundary_cases.len(), 5);

    for boundary in boundary_cases {
        let fixture_case_id = boundary["fixture_case_id"]
            .as_str()
            .expect("fixture case id");
        let hard_negative = hard_negative_cases
            .get(fixture_case_id)
            .unwrap_or_else(|| panic!("missing hard-negative fixture case {fixture_case_id}"));
        assert_eq!(hard_negative["guard"], boundary["required_guard"]);
        assert_eq!(
            hard_negative["expected_review_priority"],
            boundary["review_priority"]
        );
        assert_eq!(hard_negative["expected_auto_merge"], false);
        assert!(
            matches!(
                boundary["required_outcome"]
                    .as_str()
                    .expect("required outcome"),
                "distinct_or_review" | "review_or_escrow"
            ),
            "unsupported boundary outcome {}",
            boundary["required_outcome"]
        );

        if boundary["org_mentions_required"].as_bool().unwrap_or(false) {
            for surface in string_array(&boundary["surface_values"]) {
                assert!(
                    org_mentions_names.contains(&surface),
                    "org_mentions fixture must cover boundary surface {surface}"
                );
            }
        }
    }

    let resolved = resolved_ids_by_name(&expected_summary);
    assert_ne!(
        resolved["PNC Bank, National Association"],
        resolved["Midland Loan Services, a division of PNC Bank, National Association"],
        "PNC and Midland must not collapse to the same reviewed firm"
    );
    assert_ne!(
        resolved["Wells Fargo Bank, National Association"],
        resolved["Wells Fargo Commercial Mortgage Servicing, a division of Wells Fargo Bank, National Association"],
        "Wells Fargo bank and servicing division must not collapse"
    );

    let unresolved = string_array(&expected_summary["unresolved_surfaces"])
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert!(unresolved.contains("Wells Fargo Commercial Mortgage Securities Platform"));
    assert!(unresolved.contains("KPMG Securitization Trust 2024-C1"));
}

fn assert_snapshot_hashes(manifest: &Value) {
    for snapshot in manifest["snapshots"].as_array().expect("snapshots array") {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(snapshot["path"].as_str().expect("snapshot path"));
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("failed to read snapshot {}: {error}", path.display()));
        assert_eq!(
            bytes.len() as u64,
            snapshot["byte_count"].as_u64().expect("byte count"),
            "{} byte count",
            path.display()
        );
        assert_eq!(
            blake3::hash(&bytes).to_hex().to_string(),
            snapshot["blake3"].as_str().expect("blake3"),
            "{} blake3",
            path.display()
        );
    }
}

fn hard_negative_cases(manifest: &Value) -> BTreeMap<String, Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        manifest["hard_negative_fixture"]
            .as_str()
            .expect("fixture path"),
    );
    read_json_value(path)["cases"]
        .as_array()
        .expect("hard-negative cases array")
        .iter()
        .map(|case| {
            (
                case["id"].as_str().expect("case id").to_string(),
                case.clone(),
            )
        })
        .collect()
}

fn resolved_ids_by_name(expected_summary: &Value) -> BTreeMap<String, String> {
    expected_summary["exact_resolved_surfaces"]
        .as_array()
        .expect("exact resolved surfaces array")
        .iter()
        .map(|surface| {
            (
                surface["org_name"].as_str().expect("org name").to_string(),
                surface["canonical_id"]
                    .as_str()
                    .expect("canonical id")
                    .to_string(),
            )
        })
        .collect()
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn read_json_value(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json opens")).expect("json parses")
}

fn read_jsonl_objects(path: impl AsRef<Path>) -> Vec<Map<String, Value>> {
    fs::read_to_string(path)
        .expect("jsonl opens")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .expect("jsonl line parses")
                .as_object()
                .expect("jsonl line object")
                .clone()
        })
        .collect()
}

fn manifest() -> Value {
    serde_json::from_str(
        &fs::read_to_string(contract_root().join("manifest.json")).expect("manifest opens"),
    )
    .expect("manifest parses")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/org_mentions")
}

fn contract_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/sec10d_contract")
}
