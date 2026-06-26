use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

fn manifest() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/manifest.json");
    serde_json::from_str(&fs::read_to_string(path).expect("manifest opens"))
        .expect("manifest parses")
}

fn fixture_id_set(values: &[Value]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| value["id"].as_str().expect("fixture id").to_string())
        .collect()
}

#[test]
fn entity_fixture_manifest_schema_and_ids_are_stable() {
    let manifest = manifest();
    assert_eq!(
        manifest["schema_version"],
        "canon.entity.fixture_manifest.v0"
    );
    assert!(
        manifest["completion_policy"]
            .as_str()
            .is_some_and(|policy| policy.contains("fixture id exists here"))
    );

    let required = manifest["required_fixture_ids"].as_array().unwrap();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    assert_eq!(required.len(), 37);
    assert_eq!(fixtures.len(), required.len());

    let required_ids = required
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(fixture_id_set(fixtures), required_ids);
}

#[test]
fn entity_fixture_catalog_has_all_matrix_ids() {
    let manifest = manifest();
    let fixtures = fixture_id_set(manifest["fixtures"].as_array().unwrap());

    for id in [
        "NK-U001",
        "NK-U002",
        "NK-U003",
        "NK-U004",
        "NK-U005",
        "EN-P001",
        "EN-P002",
        "EN-P003",
        "EN-P004",
        "EN-P005",
        "EN-I001",
        "EN-I002",
        "EN-B001",
        "EN-B002",
        "EN-B003",
        "EN-B004",
        "EN-B005",
        "EN-S001",
        "EN-S002",
        "EN-S003",
        "EN-S004",
        "EN-S005",
        "EN-R001",
        "EN-R002",
        "EN-R003",
        "EN-PR001",
        "EN-PR002",
        "EN-A001",
        "CMBS-I001",
        "CMBS-I002",
        "CMBS-I003",
        "REGAB-I001",
        "REGAB-I002",
        "REGAB-I003",
        "REGAB-I004",
        "STRESS-CMBS-500K",
        "STRESS-UNIQUE-500K",
    ] {
        assert!(fixtures.contains(id), "missing fixture {id}");
    }
}

#[test]
fn entity_fixture_manifest_entries_have_owner_outcome_and_gate_metadata() {
    let manifest = manifest();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    let mut seen = BTreeSet::new();

    for fixture in fixtures {
        let id = fixture["id"].as_str().expect("id");
        assert!(seen.insert(id.to_string()), "duplicate fixture id {id}");
        assert_non_empty_string(fixture, "profile", id);
        assert_non_empty_string(fixture, "purpose", id);
        assert_allowed(
            fixture,
            "source_kind",
            id,
            &[
                "synthetic_hand_authored",
                "generated_synthetic",
                "production_derived_redacted",
                "production_derived_public_sample",
            ],
        );
        assert_allowed(fixture, "ci_tier", id, &["normal_ci", "operator_run_only"]);

        let record_size = fixture["record_size"].as_object().expect("record_size");
        assert!(
            record_size.get("rows").and_then(Value::as_u64).is_some(),
            "{id} record_size.rows"
        );
        assert!(
            record_size
                .get("surfaces")
                .and_then(Value::as_u64)
                .is_some(),
            "{id} record_size.surfaces"
        );

        assert_non_empty_array_of_strings(fixture, "input_files", id);
        assert_non_empty_array_of_strings(fixture, "gates", id);
        for gate in fixture["gates"].as_array().unwrap() {
            assert!(
                gate.as_str().unwrap().starts_with('G'),
                "{id} gate id is named"
            );
        }
        assert_non_empty_array_of_strings(fixture, "owning_tests", id);

        let outcome = fixture["expected_outcome"]
            .as_object()
            .expect("expected_outcome");
        let kind = outcome
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{id} expected_outcome.kind"));
        assert!(matches!(kind, "success" | "refusal"), "{id} outcome kind");
        match kind {
            "success" => assert!(outcome.get("refusal_code").is_some_and(Value::is_null)),
            "refusal" => assert!(
                outcome
                    .get("refusal_code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code.starts_with("E_ENTITY_")),
                "{id} refusal code"
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn entity_fixture_manifest_expected_artifacts_have_hash_policy_and_counters() {
    let manifest = manifest();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    let mut schema_counts = BTreeMap::<String, usize>::new();

    for fixture in fixtures {
        let id = fixture["id"].as_str().unwrap();
        let artifacts = fixture["expected_artifacts"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} expected_artifacts"));
        assert!(!artifacts.is_empty(), "{id} expected artifacts");

        for artifact in artifacts {
            assert_non_empty_value_string(artifact, "path", id);
            assert_non_empty_value_string(artifact, "schema_version", id);
            assert_non_empty_value_string(artifact, "hash_policy", id);
            assert!(
                artifact["summary_counters"]
                    .as_array()
                    .is_some_and(|counters| !counters.is_empty()),
                "{id} summary counters"
            );
            *schema_counts
                .entry(artifact["schema_version"].as_str().unwrap().to_string())
                .or_default() += 1;
        }
    }

    for required_schema in [
        "canon_entity_prepare.v0",
        "canon_entity_block.v0",
        "canon_entity_block_bucket.v0",
        "canon_entity_solve.v0",
        "canon_entity_decision_ledger.v0",
        "canon_entity_promote.v0",
        "canon_entity_apply.v0",
        "canon_refusal.v0",
    ] {
        assert!(
            schema_counts.contains_key(required_schema),
            "missing artifact schema {required_schema}"
        );
    }
}

fn assert_non_empty_string(value: &Value, field: &str, id: &str) {
    assert!(
        value[field]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        "{id} {field}"
    );
}

fn assert_non_empty_value_string(value: &Value, field: &str, id: &str) {
    assert!(
        value[field]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        "{id} {field}"
    );
}

fn assert_non_empty_array_of_strings(value: &Value, field: &str, id: &str) {
    let entries = value[field]
        .as_array()
        .unwrap_or_else(|| panic!("{id} {field} array"));
    assert!(!entries.is_empty(), "{id} {field} non-empty");
    for entry in entries {
        assert!(
            entry.as_str().is_some_and(|text| !text.trim().is_empty()),
            "{id} {field} string"
        );
    }
}

fn assert_allowed(value: &Value, field: &str, id: &str, allowed: &[&str]) {
    let actual = value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{id} {field}"));
    assert!(
        allowed.contains(&actual),
        "{id} {field} {actual} not in {allowed:?}"
    );
}
