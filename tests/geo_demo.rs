#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::tempdir;

fn run_demo(work_dir: &Path) -> Vec<u8> {
    let mut command = Command::new("bash");
    command
        .arg(format!(
            "{}/scripts/geo_demo/demo0.sh",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("--work-dir")
        .arg(work_dir)
        .env("CANON_BIN", env!("CARGO_BIN_EXE_canon"));

    let assert = command.assert().success();
    assert
        .get_output()
        .stderr
        .is_empty()
        .then_some(())
        .expect("demo script must keep stdout as the only operator artifact");
    assert.get_output().stdout.clone()
}

fn geo_capability_surface_counts(capabilities: &Value) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    let commands = capabilities["commands"]
        .as_object()
        .expect("commands status sets");
    for bucket in ["implemented", "diagnostic_only", "unavailable"] {
        for command in commands[bucket].as_array().expect("command bucket array") {
            let surface = command["surface"]
                .as_str()
                .unwrap_or_else(|| panic!("{bucket} command missing surface: {command}"));
            *counts.entry(surface.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

#[test]
fn demo0_script_has_valid_bash_syntax() {
    Command::new("bash")
        .arg("-n")
        .arg(format!(
            "{}/scripts/geo_demo/demo0.sh",
            env!("CARGO_MANIFEST_DIR")
        ))
        .assert()
        .success();
}

#[test]
fn geo_capabilities_report_exact_surface_tier_counts() {
    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["geo", "capabilities", "--emit", "json"])
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let capabilities: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("capabilities parses");
    assert_eq!(
        geo_capability_surface_counts(&capabilities),
        BTreeMap::from([
            ("leaf".to_string(), 12),
            ("measurement".to_string(), 3),
            ("primary".to_string(), 7)
        ])
    );
    assert_eq!(
        capabilities["commands"]["unavailable"],
        json!([
            {
                "command": "canon geo inspect",
                "surface": "primary",
                "output_contract": "planned_not_implemented",
                "read_only": true,
                "uses_network": false
            },
            {
                "command": "canon geo ledger",
                "surface": "primary",
                "output_contract": "planned_not_implemented",
                "read_only": true,
                "uses_network": false
            }
        ])
    );
}

#[test]
fn demo0_fallback_cargo_path_is_cwd_independent() {
    let temp = tempdir().expect("tempdir");
    let foreign_dir = temp.path().join("foreign");
    let work_dir = temp.path().join("work");
    fs::create_dir_all(&foreign_dir).expect("foreign cwd");
    fs::create_dir_all(&work_dir).expect("demo workdir");

    let mut command = Command::new("bash");
    command
        .current_dir(&foreign_dir)
        .arg(format!(
            "{}/scripts/geo_demo/demo0.sh",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("--work-dir")
        .arg(&work_dir)
        .env_remove("CANON_BIN");

    let assert = command.assert().success();
    let summary: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("demo summary parses");
    assert_eq!(summary["proof"]["proof_class"], "fixture");
    assert_eq!(summary["fixture_address_probe"]["parser_exercised"], false);
    assert_eq!(
        summary["composition"]["bounded_universe"]["parcel_candidates"],
        7
    );
}

#[test]
fn demo0_case4_public_cli_journey_is_byte_deterministic_and_honest() {
    let temp = tempdir().expect("tempdir");
    let first_dir = temp.path().join("first");
    let second_dir = temp.path().join("second");
    fs::create_dir_all(&first_dir).expect("first demo workdir");
    fs::create_dir_all(&second_dir).expect("second demo workdir");

    let first = run_demo(&first_dir);
    let second = run_demo(&second_dir);
    assert_eq!(
        first, second,
        "Demo 0 stdout must be byte-identical across fresh work directories"
    );

    let summary: Value = serde_json::from_slice(&first).expect("demo summary parses");
    assert_eq!(summary["demo_id"], "canon_geo_demo0_case4_chimera_fixture");
    assert_eq!(summary["proof"]["proof_class"], "fixture");
    assert_eq!(summary["proof"]["fresh_live_receipt"], false);
    assert_eq!(
        summary["proof"]["bounded_scope"],
        "seven parcel candidates with a six-member solution plus one two-cell H3 ownership fixture"
    );
    assert!(
        summary["proof"]
            .as_object()
            .unwrap()
            .contains_key("retained_source_basis"),
        "fixture proof must keep retained-source basis separate from proof_class"
    );
    assert!(
        !summary["proof"]
            .as_object()
            .unwrap()
            .contains_key("retained_sources"),
        "proof_class must not be mixed into retained-source provenance labels"
    );
    assert!(
        summary["proof"]["not_claimed"]
            .as_array()
            .unwrap()
            .contains(&json!("national_solve")),
        "demo must explicitly avoid national-solve proof inflation"
    );

    assert_eq!(
        summary["artifact_versions"]["composition"],
        "canon_geo_composition.v0"
    );
    assert_eq!(
        summary["artifact_versions"]["evidence_compilation"],
        "canon_geo_evidence_compilation.v0"
    );
    assert_eq!(
        summary["artifact_versions"]["tile_reconciliation"],
        "canon_geo_tile_reconciliation.v1"
    );
    assert_eq!(
        summary["commands_exercised"].as_array().unwrap().len(),
        10,
        "summary should expose the exact public CLI journey"
    );
    assert!(
        summary["capabilities"]["implemented_geo_commands"]
            .as_u64()
            .unwrap()
            >= 9
    );
    assert_eq!(
        summary["capabilities"]["unavailable_control_plane"],
        json!(["canon geo inspect", "canon geo ledger"])
    );
    assert_eq!(
        summary["commands_exercised"],
        json!([
            "canon geo capabilities --emit json",
            "canon geo materialize-evidence --rows case4-warehouse-rows.json",
            "canon geo compile-evidence --request evidence-request.json",
            "canon geo solve --request evidence-compilation.json",
            "canon geo evaluate --population population.json",
            "canon geo solve --request negative-hard-chimera-composition.json",
            "canon geo tile-work --request tile-discovery-request.json",
            "canon geo tile-work --request tile-owner-request.json",
            "canon geo tile-work --request tile-observer-request.json",
            "canon geo reconcile-tiles --request tile-reconciliation-request.json"
        ])
    );
    assert!(
        !summary.as_object().unwrap().contains_key("shared_run"),
        "Demo 0 must not claim planned-run execution without fixture-byte BLAKE3 provenance"
    );

    assert_eq!(summary["composition"]["status"], "resolved");
    assert_eq!(summary["composition"]["residual_model_count"], 1);
    assert_eq!(
        summary["composition"]["residual_model_count_complete"],
        true
    );
    assert_eq!(summary["composition"]["backbone_complete"], true);
    assert_eq!(
        summary["tile_ownership"]["decision_semantics"],
        "composition"
    );
    assert_eq!(
        summary["composition"]["bounded_universe"],
        json!({
            "parcel_candidates": 7,
            "building_candidates": 7,
            "solution_parcels": 6,
            "solution_buildings": 6
        })
    );
    assert_eq!(
        summary["composition"]["hard_forced"]["parcels"],
        json!([
            "1004540041",
            "1004540042",
            "1004540043",
            "1004540044",
            "1004540045",
            "1004540046"
        ])
    );
    assert_eq!(
        summary["composition"]["hard_forced"]["buildings"],
        json!([
            "1006494", "1006495", "1006496", "1006497", "1006498", "1006499"
        ])
    );

    assert_eq!(summary["evidence"]["admissions_total"], 3);
    assert_eq!(summary["evidence"]["hard_constraint_admissions"], 2);
    assert_eq!(summary["evidence"]["diagnostic_admissions"], 1);
    assert_eq!(
        summary["evidence"]["source_record_hash_scope"],
        "fixture_row_blake3_values_not_original_warehouse_byte_receipts"
    );
    assert_eq!(
        summary["fixture_address_probe"]["probe_address"],
        "199 EAST 12 STREET"
    );
    assert_eq!(summary["fixture_address_probe"]["parser_exercised"], false);
    assert_eq!(
        summary["fixture_address_probe"]["diagnostic_disposition"],
        "diagnostic_only"
    );
    assert_eq!(
        summary["fixture_address_probe"]["admitted_as_hard_evidence"],
        false
    );
    assert_eq!(
        summary["fixture_address_probe"]["excluded_candidate_forced"],
        false
    );
    assert_eq!(
        summary["fixture_address_probe"]["retained_mappluto_match_count"],
        0
    );
    assert!(
        !summary
            .as_object()
            .unwrap()
            .contains_key("synthesized_address"),
        "summary must not label the fixture address probe as parser output"
    );

    assert_eq!(summary["evaluation"]["cases"], 1);
    assert_eq!(summary["evaluation"]["resolved_cases"], 1);
    assert_eq!(
        summary["evaluation"]["evaluation_role"],
        "contract_replay_not_accuracy"
    );
    assert_eq!(summary["evaluation"]["truth_independent"], false);
    assert_eq!(summary["evaluation"]["truth_model_in_residual"], true);
    assert_eq!(summary["evaluation"]["solver_truth_exclusion_cases"], 0);
    assert_eq!(summary["evaluation"]["false_merge_cases"], 0);

    assert_eq!(summary["tile_ownership"]["halo_k"], 1);
    assert_eq!(summary["tile_ownership"]["work_cells_per_tile"], 7);
    assert_eq!(summary["tile_ownership"]["input_proposals"], 2);
    assert_eq!(summary["tile_ownership"]["owned_decisions"], 1);
    assert_eq!(summary["tile_ownership"]["discarded_halo_proposals"], 1);
    assert_eq!(
        summary["tile_ownership"]["payload_blake3_source"],
        "solve.evidence_compilation.blake3"
    );

    assert_eq!(summary["negative"]["status"], "conflict");
    assert_eq!(summary["negative"]["residual_model_count"], 0);
    assert!(
        summary["negative"]["conflict_constraint_ids"]
            .as_array()
            .unwrap()
            .contains(&json!("chimera_wrongly_admitted")),
        "the negative must fail if the demo hard-codes the happy path"
    );
}
