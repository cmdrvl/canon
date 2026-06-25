mod support;

use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};
use support::entity_harness::{
    EntityCommandTranscript, EntityFixtureTier, TreeSnapshot, assert_deterministic_runs,
    assert_refusal_envelope, blake3_bytes, blake3_file, copy_fixture_file, stable_seed,
};
use tempfile::TempDir;

#[test]
fn entity_harness_contract_requires_no_theatre_transcript_fields() {
    let valid = EntityCommandTranscript {
        command: vec![
            "canon".to_string(),
            "entity".to_string(),
            "prepare".to_string(),
        ],
        input_hash: blake3_bytes(b"rows"),
        output_path: "target/entity-evals/run/prepare.json".into(),
        artifact_version: "canon_entity_prepare.v0".to_string(),
        summary_counters: BTreeMap::from([("prepared_surfaces".to_string(), 3)]),
        exit_code: 0,
        stderr_refusal: None,
    };
    valid.validate_contract().expect("valid transcript");

    let mut missing_summary = valid.clone();
    missing_summary.summary_counters.clear();
    assert!(
        missing_summary
            .validate_contract()
            .unwrap_err()
            .contains("summary counters")
    );

    let mut refusal_without_envelope = valid;
    refusal_without_envelope.exit_code = 2;
    assert!(
        refusal_without_envelope
            .validate_contract()
            .unwrap_err()
            .contains("stderr refusal envelope")
    );
}

#[test]
fn entity_harness_contract_asserts_refusal_envelopes() {
    let value = json!({
        "version": "canon.v0",
        "outcome": "REFUSAL",
        "registry": null,
        "summary": null,
        "mappings": [],
        "unresolved": [],
        "refusal": {
            "code": "E_ENTITY_ARTIFACT_CONTRACT",
            "message": "Artifact hash mismatch",
            "detail": {
                "expected": "blake3:a",
                "actual": "blake3:b"
            },
            "next_command": "canon entity prepare <ROWS> --profile <PROFILE>"
        }
    });

    assert_refusal_envelope(&value, "E_ENTITY_ARTIFACT_CONTRACT");
}

#[test]
fn entity_refusal_no_mutation_helper_compares_registry_trees() {
    let temp_dir = TempDir::new().expect("temp dir");
    let registry = temp_dir.path().join("registry");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::write(registry.join("registry.json"), "{}\n").expect("registry metadata");
    fs::write(registry.join("aliases.json"), "[]\n").expect("aliases");

    let before = TreeSnapshot::capture(&registry);
    let after_refusal = TreeSnapshot::capture(&registry);
    before.assert_unchanged(&after_refusal);

    fs::write(registry.join("aliases.json"), "[{}]\n").expect("mutated aliases");
    let after_mutation = TreeSnapshot::capture(&registry);
    let diff = before.diff(&after_mutation);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].path, PathBuf::from("aliases.json"));
}

#[test]
fn entity_harness_contract_runs_deterministic_checks_twice() {
    assert_deterministic_runs(|| {
        let seed = stable_seed("fixture:row-shuffle");
        let mut counters = BTreeMap::new();
        counters.insert("seed_low_bits".to_string(), seed & 0xffff);
        counters
    });
}

#[test]
fn entity_harness_contract_copies_fixtures_with_hashes() {
    let temp_dir = TempDir::new().expect("temp dir");
    let source = temp_dir.path().join("source.jsonl");
    let dest = temp_dir.path().join("nested/copied.jsonl");
    fs::write(&source, "{\"row\":1}\n").expect("source fixture");

    let copied_hash = copy_fixture_file(&source, &dest);

    assert_eq!(copied_hash, blake3_file(&source));
    assert_eq!(copied_hash, blake3_file(&dest));
}

#[test]
fn entity_harness_contract_distinguishes_ci_and_operator_stress_fixtures() {
    assert_eq!(EntityFixtureTier::NormalCi.as_str(), "normal_ci");
    assert!(EntityFixtureTier::NormalCi.runs_in_default_ci());

    assert_eq!(
        EntityFixtureTier::OperatorStress.as_str(),
        "operator_stress"
    );
    assert!(!EntityFixtureTier::OperatorStress.runs_in_default_ci());
}
