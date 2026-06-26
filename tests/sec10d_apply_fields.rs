#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::apply::{
        ApplyRegistryReference, ApplySafetyCheck, SEC10D_ORG_FIELD_SUFFIXES,
        Sec10dOrgApplyResolution, Sec10dOrgApplyStreamRequest, run_sec10d_org_apply_streaming,
    },
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[test]
fn sec10d_apply_fields_match_snowflake_jsonl_contract_byte_for_byte() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input = temp.path().join("selected-org-mentions.jsonl");
    let output = temp.path().join("selected-org-mentions.enriched.jsonl");
    fs::write(&input, selected_source_rows()).expect("selected source rows");

    let artifact = run_sec10d_org_apply_streaming(Sec10dOrgApplyStreamRequest {
        rows: &input,
        output: &output,
        lookup_column: "org_name",
        field_name_column: "field_name",
        registry: registry(),
        resolutions: &resolution_table(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: false,
        target_rows_per_chunk: 2,
    })
    .expect("sec10d apply writes Snowflake-facing fields");

    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert_eq!(artifact.registry, registry());
    assert_eq!(artifact.summary["rows"], 2);
    assert_eq!(artifact.summary["resolved"], 1);
    assert_eq!(artifact.summary["unresolved"], 1);
    assert_eq!(artifact.output_path, output.display().to_string());
    assert_eq!(
        fs::read_to_string(&output).expect("output opens"),
        fs::read_to_string(fixture_root().join("applied_org_enrichment.jsonl"))
            .expect("expected output opens")
    );

    let source_rows = source_rows_by_id();
    for object in read_jsonl_objects(&output) {
        let source_row_id = object["source_row_id"].as_str().expect("source_row_id");
        let source = source_rows
            .get(source_row_id)
            .unwrap_or_else(|| panic!("unknown output source row {source_row_id}"));
        for (field, value) in source {
            assert_eq!(
                object.get(field),
                Some(value),
                "raw sec10d parser field {field} changed"
            );
        }

        let org_fields = object
            .keys()
            .filter(|field| field.contains("_org_"))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            org_fields,
            expected_org_fields(object["field_name"].as_str().expect("field_name"))
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
    }
}

#[test]
fn sec10d_apply_fields_refuses_existing_snowflake_org_fields() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("already-enriched.jsonl");

    let refusal = run_sec10d_org_apply_streaming(Sec10dOrgApplyStreamRequest {
        rows: &fixture_root().join("applied_org_enrichment.jsonl"),
        output: &output,
        lookup_column: "org_name",
        field_name_column: "field_name",
        registry: registry(),
        resolutions: &resolution_table(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: false,
        target_rows_per_chunk: 2,
    })
    .expect_err("existing *_org_* fields refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityInputContract);
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["row_number"], 1);
    assert_eq!(refusal.detail["field"], "servicer_org_canon_id");
    assert!(!output.exists(), "refusal must not write output");
}

#[test]
fn sec10d_apply_fields_full_resolution_guard_treats_review_required_as_unresolved() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input = temp.path().join("selected-org-mentions.jsonl");
    let output = temp.path().join("selected-org-mentions.enriched.jsonl");
    fs::write(&input, selected_source_rows()).expect("selected source rows");

    let refusal = run_sec10d_org_apply_streaming(Sec10dOrgApplyStreamRequest {
        rows: &input,
        output: &output,
        lookup_column: "org_name",
        field_name_column: "field_name",
        registry: registry(),
        resolutions: &resolution_table(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: true,
        target_rows_per_chunk: 2,
    })
    .expect_err("review-required row refuses full-resolution apply");

    assert_eq!(refusal.code, RefusalCode::EEntityApplyUnresolved);
    assert_eq!(refusal.detail["rows"], 2);
    assert_eq!(refusal.detail["resolved"], 1);
    assert_eq!(refusal.detail["unresolved"], 1);
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!output.exists(), "refusal must not write output");
}

fn selected_source_rows() -> String {
    fs::read_to_string(org_mentions_jsonl())
        .expect("org mentions opens")
        .lines()
        .filter(|line| {
            line.contains("\"source_row_id\":\"regab-fixture-001\"")
                || line.contains("\"source_row_id\":\"regab-fixture-005\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn expected_org_fields(field_name: &str) -> Vec<String> {
    let prefix = field_name
        .strip_suffix("_name")
        .expect("fixture field names end in _name");
    SEC10D_ORG_FIELD_SUFFIXES
        .iter()
        .map(|suffix| format!("{prefix}{suffix}"))
        .collect()
}

fn resolution_table() -> BTreeMap<String, Sec10dOrgApplyResolution> {
    BTreeMap::from([(
        "PNC Bank, National Association".to_string(),
        Sec10dOrgApplyResolution {
            canonical_id: "ORG-034".to_string(),
            canonical_name: "PNC Bank, National Association".to_string(),
            resolution_status: "resolved_exact".to_string(),
            rule_id: "REGAB_EXACT_ALIAS".to_string(),
        },
    )])
}

fn registry() -> ApplyRegistryReference {
    ApplyRegistryReference {
        id: "firms".to_string(),
        version: "1.0.12".to_string(),
    }
}

fn source_rows_by_id() -> BTreeMap<String, Map<String, Value>> {
    read_jsonl_objects(org_mentions_jsonl())
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

fn org_mentions_jsonl() -> PathBuf {
    fixture_root().join("org_mentions.jsonl")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/org_mentions")
}
