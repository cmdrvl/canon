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

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
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
