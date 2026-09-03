//! D1 residual artifact regressions that must exercise the public evaluate
//! surface, including the internal geo-run propagation stage.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("serialize JSON"),
    )
    .expect("write JSON file");
    path
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|error| {
        panic!("{} should be readable: {error}", path.display());
    }))
    .unwrap_or_else(|error| panic!("{} should parse as JSON: {error}", path.display()))
}

fn propagation_population_request() -> Value {
    json!({
        "version": "canon_geo_population_request.v0",
        "max_cases": 1,
        "cases": [
            {
                "id": "case-propagation-artifact",
                "evidence": {
                    "version": "canon_geo_evidence_request.v0",
                    "profile": {
                        "version": "canon_geo_composition_profile.v0",
                        "selection_level": "parcel"
                    },
                    "universe": {
                        "parcels": ["parcel-a", "parcel-b"],
                        "buildings": []
                    },
                    "contracts": [
                        {
                            "id": "rho.exact",
                            "version": "1.0.0",
                            "source_dataset": "fixture:parcel-sets",
                            "source_release": "fixture-v1",
                            "source_lineage_ids": ["fixture:parcel-sets:upstream"],
                            "method_id": "fixture:exact-set",
                            "method_version": "1.0.0",
                            "claim_role": "stable_identity_anchor",
                            "basis": {
                                "kind": "logical_relaxation",
                                "invariant_id": "fixture:exact-set-invariant"
                            }
                        }
                    ],
                    "observations": [
                        {
                            "id": "obs.exact",
                            "contract_id": "rho.exact",
                            "source_records": [
                                {
                                    "source_record_id": "parcel-set-row-1",
                                    "source_vintage": "fixture-v1",
                                    "record_blake3": "97e7e532ba98fb5ce35769f30b61b738d906c6686f17c7d8bbbf61bf3f8b910c"
                                }
                            ],
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
                "truth_plane": "gate_v2_historical",
                "truth": { "parcels": ["parcel-a"], "buildings": [] }
            }
        ]
    })
}

#[test]
fn evaluate_artifact_dir_writes_propagation_artifacts_and_refuses_stale_bytes() {
    let temp = tempdir().expect("tempdir");
    let population_path = write_json(
        temp.path(),
        "population.json",
        &propagation_population_request(),
    );
    let artifact_dir = temp.path().join("artifacts");

    let stdout = canon_command()
        .args(["geo", "evaluate", "--population"])
        .arg(&population_path)
        .arg("--artifact-dir")
        .arg(&artifact_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let evaluation: Value = serde_json::from_slice(&stdout).expect("evaluation JSON parses");
    assert_eq!(evaluation["summary"]["cases"], 1);
    assert_eq!(evaluation["summary"]["resolved_cases"], 1);

    let index = read_json(&artifact_dir.join("index.json"));
    let entry = index["cases"]
        .as_array()
        .expect("index cases")
        .first()
        .expect("first index entry");
    assert_eq!(entry["case_id"], "case-propagation-artifact");
    let propagation_file = entry["propagation_file"]
        .as_str()
        .expect("propagation file path");
    let propagation_path = artifact_dir.join(propagation_file);
    let propagation_bytes = fs::read(&propagation_path).expect("propagation artifact bytes");
    assert_eq!(
        blake3::hash(&propagation_bytes).to_hex().to_string(),
        entry["propagation_digest"]
            .as_str()
            .expect("propagation digest")
    );

    let propagation: Value =
        serde_json::from_slice(&propagation_bytes).expect("propagation JSON parses");
    assert_eq!(propagation["version"], "canon_geo_propagation.v0");
    assert_eq!(propagation["fixpoint_reached"], json!(true));
    let prunings = propagation["prunings"]
        .as_array()
        .expect("propagation prunings");
    assert!(
        !prunings.is_empty(),
        "fixture must exercise nonempty propagation"
    );
    assert!(
        prunings.iter().any(|pruning| {
            pruning["propagator"] == "source_exclusivity"
                && pruning["constraint_ids"] == json!(["rho:rho.exact@1.0.0:obs.exact"])
                && pruning["evidence_ids"] == json!(["obs.exact"])
        }),
        "typed propagation reason must name the constraint id and evidence id"
    );

    let solve_file = entry["solve_file"].as_str().expect("solve file path");
    let solve_bytes = fs::read(artifact_dir.join(solve_file)).expect("solve artifact bytes");
    assert_eq!(
        blake3::hash(&solve_bytes).to_hex().to_string(),
        entry["solver_digest"].as_str().expect("solver digest")
    );

    fs::write(&propagation_path, br#"{"tampered":true}"#).expect("tamper propagation artifact");
    let refusal_stdout = canon_command()
        .args(["geo", "evaluate", "--population"])
        .arg(&population_path)
        .arg("--artifact-dir")
        .arg(&artifact_dir)
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal_stdout).expect("refusal JSON parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_population_error_code"],
        "invalid_input"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["detail"]["case_id"],
        "case-propagation-artifact"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["detail"]["artifact_kind"],
        "propagation"
    );
}
