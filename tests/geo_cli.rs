//! CLI-level contract for the `canon geo` family.
//!
//! The library kernel is exercised by `tests/geo_composition.rs` and friends;
//! this file only pins the operator surface: typed requests in, canonical
//! artifact bytes out, typed refusals with exit code 2 on bad input.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_json(dir: &std::path::Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("serialize request"),
    )
    .expect("write request file");
    path
}

/// Three parcels, no buildings, one AnyOf over two of the parcels.
///
/// The hard residual is every nonempty parcel subset that selects P1 or P2:
/// 7 nonempty subsets minus the single `{P3}` model = 6.
fn tiny_composition_request() -> Value {
    json!({
        "version": "canon_geo_composition_request.v0",
        "universe": {
            "parcels": ["parcel-a", "parcel-b", "parcel-c"],
            "buildings": []
        },
        "hard_constraints": [
            {
                "id": "anyof-ab",
                "constraint": {
                    "kind": "any_of",
                    "members": [
                        { "level": "parcel", "id": "parcel-a" },
                        { "level": "parcel", "id": "parcel-b" }
                    ]
                }
            }
        ],
        "soft_preferences": [],
        "max_assignments": 64,
        "max_materialized_models": 64
    })
}

#[test]
fn geo_solve_emits_canonical_composition_artifact_on_stdout() {
    let temp = tempdir().expect("tempdir");
    let request = write_json(temp.path(), "composition.json", &tiny_composition_request());

    let assert = canon_command()
        .args(["geo", "solve", "--request", request.to_str().unwrap()])
        .assert()
        .success();
    let output = assert.get_output();
    assert!(output.stderr.is_empty(), "solve must not write to stderr");

    let stdout = String::from_utf8(output.stdout.clone()).expect("utf-8 stdout");
    assert!(
        stdout.ends_with('\n'),
        "canonical bytes must carry exactly one trailing newline"
    );
    let artifact: Value = serde_json::from_str(stdout.trim_end()).expect("stdout parses as JSON");

    assert_eq!(artifact["version"], "canon_geo_composition.v0");
    assert_eq!(
        artifact["request_version"],
        "canon_geo_composition_request.v0"
    );
    assert_eq!(artifact["summary"]["residual_model_count"], 6);
    assert_eq!(artifact["summary"]["parcel_candidates"], 3);
    assert_eq!(artifact["summary"]["building_candidates"], 0);
    assert_eq!(artifact["status"], "ambiguous");
    assert_eq!(artifact["summary"]["summary_counts_saturated"], false);
    // The AnyOf closed form reports the exact count and backbone without
    // materializing models, so `residual_models` stays empty by contract.
    assert_eq!(artifact["summary"]["residual_models_materialized"], false);
    assert!(artifact["residual_models"].as_array().unwrap().is_empty());

    // Same request twice must produce byte-identical output.
    let second = canon_command()
        .args(["geo", "solve", "--request", request.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(output.stdout, second.get_output().stdout);
}

#[test]
fn geo_solve_refuses_a_malformed_request_file() {
    let temp = tempdir().expect("tempdir");
    let request = temp.path().join("malformed.json");
    fs::write(
        &request,
        b"{ \"version\": \"canon_geo_composition_request.v0\", ",
    )
    .expect("write malformed request");

    let assert = canon_command()
        .args(["geo", "solve", "--request", request.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal envelope parses");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert!(
        refusal["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("parse"),
        "refusal message must name the failure: {refusal}"
    );
    assert!(
        !refusal["refusal"]["detail"]["error"]
            .as_str()
            .unwrap()
            .is_empty(),
        "refusal detail must carry the underlying parse error"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["expected_version"],
        "canon_geo_composition_request.v0"
    );
    assert!(refusal["refusal"]["next_command"].is_string());
}

#[test]
fn geo_solve_refuses_a_missing_request_file() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("absent.json");

    let assert = canon_command()
        .args(["geo", "solve", "--request", missing.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal envelope parses");

    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_IO");
    assert_eq!(
        refusal["refusal"]["detail"]["request"],
        missing.to_string_lossy().as_ref()
    );
}

#[test]
fn geo_solve_surfaces_a_version_mismatch_as_a_typed_refusal() {
    let temp = tempdir().expect("tempdir");
    let mut request_value = tiny_composition_request();
    request_value["version"] = json!("canon_geo_composition_request.v9");
    let request = write_json(temp.path(), "wrong_version.json", &request_value);

    let assert = canon_command()
        .args(["geo", "solve", "--request", request.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal envelope parses");

    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_composition_error_code"],
        "unsupported_version"
    );
}

#[test]
fn geo_compile_evidence_emits_a_bounded_composition_request() {
    let temp = tempdir().expect("tempdir");
    let request = write_json(
        temp.path(),
        "evidence.json",
        &json!({
            "version": "canon_geo_evidence_request.v0",
            "universe": {
                "parcels": ["parcel-a", "parcel-b"],
                "buildings": []
            },
            "contracts": [
                {
                    "id": "rho.existential",
                    "version": "1.0.0",
                    "soundness": "logically_sound"
                }
            ],
            "observations": [
                {
                    "id": "obs.one",
                    "contract_id": "rho.existential",
                    "observation": {
                        "kind": "existential_membership",
                        "members": [{ "level": "parcel", "id": "parcel-a" }]
                    }
                }
            ],
            "max_assignments": 64,
            "max_materialized_models": 64
        }),
    );

    let assert = canon_command()
        .args([
            "geo",
            "compile-evidence",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(stdout.ends_with('\n'));
    let artifact: Value = serde_json::from_str(stdout.trim_end()).expect("stdout parses as JSON");

    assert_eq!(artifact["version"], "canon_geo_evidence_compilation.v0");
    assert_eq!(artifact["request_version"], "canon_geo_evidence_request.v0");
    assert_eq!(
        artifact["composition_request"]["version"],
        "canon_geo_composition_request.v0"
    );
    assert_eq!(artifact["admissions"][0]["disposition"], "hard_constraint");
    assert_eq!(
        artifact["composition_request"]["hard_constraints"][0]["constraint"]["kind"],
        "any_of"
    );
}

#[test]
fn geo_evaluate_scores_a_minimal_labeled_population() {
    let temp = tempdir().expect("tempdir");
    let population = write_json(
        temp.path(),
        "population.json",
        &json!({
            "version": "canon_geo_population_request.v0",
            "max_cases": 4,
            "cases": [
                {
                    "id": "case.alpha",
                    "evidence": {
                        "version": "canon_geo_evidence_request.v0",
                        "universe": {
                            "parcels": ["parcel-a", "parcel-b"],
                            "buildings": []
                        },
                        "contracts": [
                            {
                                "id": "rho.exact",
                                "version": "1.0.0",
                                "soundness": "logically_sound"
                            }
                        ],
                        "observations": [
                            {
                                "id": "obs.exact",
                                "contract_id": "rho.exact",
                                "observation": {
                                    "kind": "exact_sets",
                                    "level": "parcel",
                                    "sets": [["parcel-a"]]
                                }
                            }
                        ],
                        "max_assignments": 64,
                        "max_materialized_models": 64
                    },
                    "truth": { "parcels": ["parcel-a"], "buildings": [] }
                }
            ]
        }),
    );

    let assert = canon_command()
        .args([
            "geo",
            "evaluate",
            "--population",
            population.to_str().unwrap(),
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(stdout.ends_with('\n'));
    let artifact: Value = serde_json::from_str(stdout.trim_end()).expect("stdout parses as JSON");

    assert_eq!(artifact["version"], "canon_geo_population_evaluation.v0");
    assert_eq!(
        artifact["request_version"],
        "canon_geo_population_request.v0"
    );
    assert_eq!(artifact["summary"]["cases"], 1);
    assert_eq!(artifact["summary"]["resolved_cases"], 1);
    assert_eq!(artifact["summary"]["false_merge_cases"], 0);
    assert_eq!(artifact["cases"][0]["case_id"], "case.alpha");
    assert_eq!(artifact["cases"][0]["status"], "resolved");
    assert_eq!(artifact["cases"][0]["truth_model_in_residual"], true);
}

#[test]
fn geo_evaluate_refuses_a_missing_population_file() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("absent.json");

    let assert = canon_command()
        .args(["geo", "evaluate", "--population", missing.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal envelope parses");

    assert_eq!(refusal["refusal"]["code"], "E_IO");
    assert_eq!(
        refusal["refusal"]["detail"]["population"],
        missing.to_string_lossy().as_ref()
    );
}
