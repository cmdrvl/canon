#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    geo::{
        CANON_GEO_EXPLANATION_VERSION, CANON_GEO_INSPECTION_VERSION, CANON_GEO_RUN_VERSION,
        GeoInspectErrorCode, GeoInspection, GeoInspectionQuestion, GeoRun, canonical_geo_run_bytes,
        compare, geo_run_manifest_head_path, geo_run_manifest_revision_path, geo_run_semantic_hash,
        inspect, validate_inspection_artifact,
    },
    project::ProjectRunPolicy,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("serialize fixture JSON"),
    )
    .expect("write fixture JSON");
    path
}

fn digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn tile_source_json(source_instance_id: &str, entity_level: &str, participation: &str) -> Value {
    json!({
        "source_instance_id": source_instance_id,
        "release": {
            "release_id": format!("{source_instance_id}.release"),
            "release_digest": digest(source_instance_id)
        },
        "native_scope": {
            "kind": "native_entity",
            "entity_level": entity_level,
            "identity_participation": participation
        },
        "inventory_ref": {
            "inventory_id": "inventory.fixture.inspect",
            "semantic_hash": digest("inventory.fixture.inspect.semantic"),
            "planning_hash": digest("inventory.fixture.inspect.planning")
        }
    })
}

fn region() -> Value {
    json!({
        "geography_id": "region.fixture.geo-inspect",
        "geography_kind": "bounded_fixture",
        "description": "Geo inspect CLI fixture region"
    })
}

fn as_of(day: &str, semantic_id: &str) -> Value {
    json!({
        "utc_day": day,
        "semantic_id": semantic_id,
        "unit": "utc_day",
        "origin": "caller_declared"
    })
}

fn bound(id: &str, counter: &str, value: u64, unit: &str) -> Value {
    json!({
        "semantic_id": id,
        "counter": counter,
        "value": value,
        "unit": unit,
        "origin": "caller_declared",
        "action": "report_budget_fallback"
    })
}

fn budget() -> Value {
    json!({
        "version": "canon_geo_resource_budget.v0",
        "budget_id": "budget.fixture.geo-inspect",
        "deterministic_bounds": [
            bound("budget.max_bytes", "bytes", 1_000_000, "byte"),
            bound("budget.max_rows", "rows", 10_000, "row"),
            bound("budget.max_cells", "cells", 64, "cell"),
            bound("budget.max_candidates", "candidates", 500, "candidate"),
            bound("budget.max_variables", "variables", 128, "variable"),
            bound("budget.max_states", "states", 100_000, "state"),
            bound("budget.max_models", "models", 10_000, "model"),
            bound("budget.max_operations", "operations", 1_000_000, "operation")
        ],
        "telemetry": [{
            "metric": "wall_time",
            "unit": "millisecond",
            "origin": "operator_policy",
            "semantic_effect": "none"
        }]
    })
}

fn source_instance(source_instance_id: &str, evidence_classes: Vec<&str>) -> Value {
    json!({
        "source_instance_id": source_instance_id,
        "release": {
            "release_id": "release.fixture.geo-inspect",
            "release_digest": digest("release.fixture.geo-inspect")
        },
        "temporal_scope": {
            "valid_time": {
                "start_utc_day": "2026-01-01",
                "end_utc_day": "2026-12-31"
            },
            "release_time": as_of("2026-05-01", "source.release.utc_day")
        },
        "lineage_ids": ["lineage.fixture.inspect.one", "lineage.fixture.inspect.two"],
        "native_scope": {
            "kind": "native_entity",
            "entity_level": "building",
            "identity_participation": "stable_alias"
        },
        "evidence_classes": evidence_classes,
        "coverage": {
            "coverage_id": format!("coverage.{source_instance_id}"),
            "region": region(),
            "predicate": "all declared inspect fixture records"
        },
        "local_state": {
            "state": "available",
            "local_ref": {
                "artifact_id": format!("artifact.{source_instance_id}"),
                "contract_version": "canon_geo_warehouse_rows.v0",
                "content_hash": digest(&format!("local.{source_instance_id}")),
                "media_type": "application/json"
            }
        },
        "geometry": {
            "geometry_contract_version": "geometry.fixture.v1",
            "coordinate_reference_system": "EPSG:4326",
            "transform_id": "identity.fixture",
            "transform_digest": digest("identity.fixture"),
            "numeric_error_bounds": [{
                "semantic_id": "transform.error",
                "value": 0,
                "unit": "millimetre",
                "origin": "adapter_contract"
            }]
        },
        "license_class": "public_redistributable",
        "egress_class": "shareable",
        "estimates": [{
            "semantic_id": "source.rows",
            "value": 5,
            "unit": "row",
            "origin": "source_release"
        }]
    })
}

fn inventory() -> Value {
    json!({
        "version": "canon_geo_regional_inventory.v1",
        "inventory_id": "inventory.fixture.geo-inspect",
        "region": region(),
        "sources": [
            source_instance("source.fixture.inspect-building-footprints", vec!["building_footprint", "address_set"]),
            source_instance("source.fixture.inspect-address-attributes", vec!["address_set", "asserted_attribute"])
        ],
        "discovery_gaps": []
    })
}

fn profile() -> Value {
    json!({
        "version": "canon_geo_composition_profile.v0",
        "selection_level": "building"
    })
}

fn question() -> Value {
    json!({
        "version": "canon_geo_question.v0",
        "question_id": "question.synthetic-not-live.geo-inspect.building",
        "subject_bindings": [{
            "role": "target",
            "binding_class": "operator_label",
            "value": "synthetic-not-live inspect building fixture"
        }],
        "bounded_geography": region(),
        "requested_grains": [{
            "entity_level": "building",
            "required_evidence_classes": ["building_footprint"],
            "optional_evidence_classes": ["address_set"]
        }],
        "query_as_of": as_of("2026-08-31", "question.synthetic-not-live.inspect.query_as_of.utc_day"),
        "requested_claim_classes": ["candidate_reach", "stable_identity"],
        "presentation_limits": [
            bound("presentation.synthetic-not-live.max_models", "models", 16, "model"),
            bound("presentation.synthetic-not-live.max_candidates", "candidates", 32, "candidate")
        ],
        "abstention_policy": {
            "unsupported_grain": "report_unsupported",
            "unresolved_residual": "report_residual",
            "budget_fallback": "report_residual"
        },
        "decision_policy": null,
        "resource_budget_ref": "budget.fixture.geo-inspect"
    })
}

fn center_cell() -> &'static str {
    "892a100d62bffff"
}

fn home_cell_rows() -> Value {
    let center = center_cell();
    json!({
        "version": "canon_geo_home_cell_rows.v1",
        "coordinate_crs": "EPSG:4326",
        "coordinate_decimal_places": 9,
        "h3_resolution": 9,
        "stability_radius_fixed": 1000,
        "rows": [
            home_cell_row("building-a", "synthetic-not-live/building-a", center),
            home_cell_row("building-b", "synthetic-not-live/building-b", center)
        ],
        "max_rows": 16
    })
}

fn home_cell_row(feature_id: &str, source_record_id: &str, center: &str) -> Value {
    json!({
        "source": tile_source_json(
            "synthetic_building_fixture_not_live",
            "building",
            "stable_alias"
        ),
        "feature_id": feature_id,
        "source_record_id": source_record_id,
        "geometry_sha256": "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
        "representative_point_method": "synthetic_centroid_of_fixture_geometry",
        "longitude": "-73.977264000",
        "latitude": "40.753429000",
        "transform_execution_id": "synthetic-not-live-transform-execution",
        "transform_definition_id": "synthetic-not-live-transform-definition",
        "claimed_home_cell": center
    })
}

fn tile_work_request() -> Value {
    let center = center_cell();
    json!({
        "version": "canon_geo_tile_work_request.v1",
        "center_cell": center,
        "halo_k": 1,
        "features": [
            {
                "source": tile_source_json(
                    "synthetic_building_fixture_not_live",
                    "building",
                    "stable_alias"
                ),
                "feature_id": "building-a",
                "home_cell": center
            },
            {
                "source": tile_source_json(
                    "synthetic_building_fixture_not_live",
                    "building",
                    "stable_alias"
                ),
                "feature_id": "building-b",
                "home_cell": center
            }
        ],
        "max_features": 16,
        "max_work_cells": 7
    })
}

fn warehouse_rows() -> Value {
    json!({
        "version": "canon_geo_warehouse_rows.v0",
        "profile": {
            "version": "canon_geo_composition_profile.v0",
            "selection_level": "building"
        },
        "parcel_rows": [],
        "building_parcel_rows": [
            {
                "building_id": "building-b",
                "parcel_id": null
            },
            {
                "building_id": "building-a",
                "parcel_id": null
            }
        ],
        "contracts": [{
            "id": "rho.synthetic-not-live.building-set",
            "version": "1.0.0",
            "source_dataset": "synthetic.not_live.building_fixture",
            "source_release": "synthetic-not-live/2026-08-31",
            "source_lineage_ids": ["synthetic.not_live.building_fixture.release"],
            "method_id": "synthetic-not-live-building-candidate-set",
            "method_version": "1.0.0",
            "claim_role": "stable_identity_anchor",
            "basis": {
                "kind": "logical_relaxation",
                "invariant_id": "synthetic-not-live-candidate-set-is-a-superset"
            }
        }],
        "evidence_rows": [
            {
                "observation_id": "obs.synthetic-not-live.building-set",
                "contract_id": "rho.synthetic-not-live.building-set",
                "source_record": {
                    "source_record_id": "synthetic-not-live-row-b",
                    "source_vintage": "synthetic-not-live/2026-08-31",
                    "record_blake3": blake3::hash(b"synthetic-not-live-row-b").to_hex().to_string()
                },
                "observation": {
                    "kind": "exact_sets",
                    "level": "building",
                    "sets": [["building-a", "building-b"]]
                }
            },
            {
                "observation_id": "obs.synthetic-not-live.building-set",
                "contract_id": "rho.synthetic-not-live.building-set",
                "source_record": {
                    "source_record_id": "synthetic-not-live-row-a",
                    "source_vintage": "synthetic-not-live/2026-08-31",
                    "record_blake3": blake3::hash(b"synthetic-not-live-row-a").to_hex().to_string()
                },
                "observation": {
                    "kind": "exact_sets",
                    "level": "building",
                    "sets": [["building-a", "building-b"]]
                }
            }
        ],
        "max_assignments": 128,
        "max_materialized_models": 64
    })
}

fn separation_inputs() -> Value {
    json!({
        "version": "canon_geo_separation_inputs.v0",
        "prospective": [
            {
                "id": "obs.synthetic-not-live.prospective.binary-a",
                "contract_id": "rho.synthetic-not-live.prospective.binary-a",
                "cost_units": 1,
                "outcomes": [
                    {
                        "outcome_id": "outcome.forbid-a",
                        "induced": [{
                            "kind": "forbid",
                            "member": { "level": "building", "id": "building-a" }
                        }]
                    },
                    {
                        "outcome_id": "outcome.require-a",
                        "induced": [{
                            "kind": "require",
                            "member": { "level": "building", "id": "building-a" }
                        }]
                    }
                ]
            },
            {
                "id": "obs.synthetic-not-live.prospective.full-choice",
                "contract_id": "rho.synthetic-not-live.prospective.full-choice",
                "cost_units": 3,
                "outcomes": [
                    {
                        "outcome_id": "outcome.only-a",
                        "induced": [{
                            "kind": "allowed_sets",
                            "level": "building",
                            "sets": [["building-a"]]
                        }]
                    },
                    {
                        "outcome_id": "outcome.only-ab",
                        "induced": [{
                            "kind": "allowed_sets",
                            "level": "building",
                            "sets": [["building-a", "building-b"]]
                        }]
                    },
                    {
                        "outcome_id": "outcome.only-b",
                        "induced": [{
                            "kind": "allowed_sets",
                            "level": "building",
                            "sets": [["building-b"]]
                        }]
                    }
                ]
            }
        ]
    })
}

fn next_evidence_inputs() -> Value {
    json!({
        "version": "canon_geo_next_evidence_inputs.v0",
        "candidates": [
            {
                "action_id": "action.synthetic-not-live.binary-a",
                "class": "separate_residual",
                "kind": {
                    "kind": "observe",
                    "payload": "obs.synthetic-not-live.prospective.binary-a"
                },
                "observation_id": "obs.synthetic-not-live.prospective.binary-a",
                "cost_units": 1
            },
            {
                "action_id": "action.synthetic-not-live.full-choice",
                "class": "separate_residual",
                "kind": {
                    "kind": "observe",
                    "payload": "obs.synthetic-not-live.prospective.full-choice"
                },
                "observation_id": "obs.synthetic-not-live.prospective.full-choice",
                "cost_units": 3
            }
        ],
        "budget": budget()
    })
}

struct PlanInputPaths {
    question: PathBuf,
    capabilities: PathBuf,
    inventory: PathBuf,
    profile: PathBuf,
    budget: PathBuf,
}

fn write_capabilities(dir: &Path) -> PathBuf {
    let output = canon_command()
        .args(["geo", "capabilities", "--emit", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let path = dir.join("capabilities.json");
    fs::write(&path, output).expect("write capabilities");
    path
}

fn write_plan_inputs(dir: &Path) -> PlanInputPaths {
    PlanInputPaths {
        question: write_json(dir, "question.json", &question()),
        capabilities: write_capabilities(dir),
        inventory: write_json(dir, "inventory.json", &inventory()),
        profile: write_json(dir, "profile.json", &profile()),
        budget: write_json(dir, "budget.json", &budget()),
    }
}

fn write_public_plan(dir: &Path) -> PathBuf {
    let paths = write_plan_inputs(dir);
    let assert = canon_command()
        .arg("geo")
        .arg("plan")
        .arg("--question")
        .arg(&paths.question)
        .arg("--capabilities")
        .arg(&paths.capabilities)
        .arg("--inventory")
        .arg(&paths.inventory)
        .arg("--profile")
        .arg(&paths.profile)
        .arg("--budget")
        .arg(&paths.budget)
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let plan: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("Geo plan stdout parses");
    assert_eq!(plan["status"], "planned");
    assert_eq!(
        plan["project_plan"]["nodes"].as_array().unwrap().len(),
        9,
        "inspect public journey must start from the generated run plan"
    );
    let path = dir.join("plan.json");
    fs::write(&path, &assert.get_output().stdout).expect("write plan stdout");
    path
}

fn write_run_inputs(dir: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    (
        write_json(dir, "home-cell-rows.json", &home_cell_rows()),
        write_json(dir, "tile-work-request.json", &tile_work_request()),
        write_json(dir, "warehouse-rows.json", &warehouse_rows()),
        write_json(dir, "separation-inputs.json", &separation_inputs()),
        write_json(dir, "next-evidence-inputs.json", &next_evidence_inputs()),
    )
}

struct SyntheticRun {
    _temp: TempDir,
    work_dir: PathBuf,
    run: Value,
}

fn run_public_synthetic_chain() -> SyntheticRun {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("synthetic-not-live-inputs");
    fs::create_dir(&input_dir).expect("create input dir");
    let plan = write_public_plan(&input_dir);
    let (home_cells, tile_work, warehouse, separation, next_evidence) =
        write_run_inputs(&input_dir);
    let work_dir = temp.path().join("synthetic-not-live-work");
    fs::create_dir(&work_dir).expect("create work dir");
    let bindings = [
        format!("geo.building.home_cells:rows={}", home_cells.display()),
        format!("geo.building.section:request={}", tile_work.display()),
        format!(
            "geo.building.materialize_evidence:rows={}",
            warehouse.display()
        ),
        format!("geo.building.separation:request={}", separation.display()),
        format!(
            "geo.building.next_evidence:request={}",
            next_evidence.display()
        ),
    ];
    let assert = canon_command()
        .arg("geo")
        .arg("run")
        .arg("--plan")
        .arg(&plan)
        .arg("--work-dir")
        .arg(&work_dir)
        .arg("--input")
        .arg(&bindings[0])
        .arg("--input")
        .arg(&bindings[1])
        .arg("--input")
        .arg(&bindings[2])
        .arg("--input")
        .arg(&bindings[3])
        .arg("--input")
        .arg(&bindings[4])
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("run stdout utf8");
    assert!(stdout.ends_with('\n'));
    let run: Value = serde_json::from_str(stdout.trim_end()).expect("Geo run stdout parses");
    assert_eq!(run["version"], CANON_GEO_RUN_VERSION);
    assert_eq!(run["status"], "COMPLETED");
    let input_contracts = run["artifact_inputs"]
        .as_array()
        .expect("artifact inputs")
        .iter()
        .filter_map(|input| input["contract_version"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(input_contracts.contains("canon_geo_home_cell_rows.v1"));
    assert!(input_contracts.contains("canon_geo_tile_work_request.v1"));
    assert!(input_contracts.contains("canon_geo_warehouse_rows.v0"));
    assert!(input_contracts.contains("canon_geo_separation_inputs.v0"));
    assert!(input_contracts.contains("canon_geo_next_evidence_inputs.v0"));
    assert!(!input_contracts.contains("canon_geo_separation.v0"));
    assert!(!input_contracts.contains("canon_geo_next_evidence.v0"));
    SyntheticRun {
        _temp: temp,
        work_dir,
        run,
    }
}

fn run_output_ref_ids(run: &Value) -> BTreeSet<String> {
    run["output_refs"]
        .as_array()
        .expect("run output refs")
        .iter()
        .map(|reference| {
            reference["artifact_id"]
                .as_str()
                .expect("artifact_id")
                .to_string()
        })
        .collect()
}

fn output_ref<'a>(run: &'a Value, artifact_id: &str) -> &'a Value {
    run["output_refs"]
        .as_array()
        .expect("run output refs")
        .iter()
        .find(|reference| reference["artifact_id"] == artifact_id)
        .unwrap_or_else(|| panic!("run output_refs must include {artifact_id}"))
}

fn cas_path(work_dir: &Path, content_digest: &str) -> PathBuf {
    let digest_hex = content_digest
        .strip_prefix("blake3:")
        .expect("content digest prefix");
    work_dir
        .join(".canon")
        .join("geo-run")
        .join("artifacts")
        .join("cas")
        .join(format!("{digest_hex}.bin"))
}

fn parse_inspection(bytes: &[u8]) -> GeoInspection {
    let inspection: GeoInspection =
        serde_json::from_slice(bytes).expect("inspection artifact parses");
    validate_inspection_artifact(&inspection).expect("inspection validates");
    inspection
}

#[test]
fn t12_geo_inspect_cli_plans_runs_and_reads_synthetic_not_live_stored_artifacts() {
    let fixture = run_public_synthetic_chain();
    let assert = canon_command()
        .arg("geo")
        .arg("inspect")
        .arg("--run")
        .arg(&fixture.work_dir)
        .arg("--component")
        .arg("building:building-a")
        .arg("--recommend-next")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("inspect stdout");
    assert!(stdout.ends_with('\n'));
    let inspection = parse_inspection(stdout.trim_end().as_bytes());
    let inspection_json: Value = serde_json::to_value(&inspection).expect("inspection to JSON");
    assert_eq!(inspection.version, CANON_GEO_INSPECTION_VERSION);
    assert_eq!(
        inspection.run_id,
        fixture.run["run_id"].as_str().expect("run_id is string")
    );
    assert_eq!(
        inspection.component_id.as_deref(),
        Some("building:building-a")
    );
    assert!(inspection.recommend_next);
    assert_eq!(inspection.answers.len(), 8);
    assert_eq!(inspection.bounds.max_component_refs, 8);
    assert!(!inspection.bounds.component_refs_truncated);
    assert_eq!(inspection.metrics.proof_class, "fixture");
    assert!(
        inspection
            .planes
            .candidate_reach
            .contains("truth_reach=unverified")
    );
    assert!(inspection.planes.truth_quality.contains("unverified"));
    assert!(
        !inspection
            .planes
            .truth_quality
            .contains("accurate resolution"),
        "unverified reach must not be summarized as accurate resolution"
    );
    assert!(
        inspection
            .planes
            .reconciliation
            .contains("artifact_digest_match_is_not_semantic_reconciliation")
    );

    let mut allowed_artifact_ids = run_output_ref_ids(&fixture.run);
    allowed_artifact_ids.insert("geo-run-manifest/head.json".to_string());
    let mut abstained_questions = Vec::new();
    for (index, answer) in inspection_json["answers"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            answer["question"],
            format!("Q{}", index + 1),
            "inspection answers must stay in architecture order"
        );
        let refs = answer["artifact_refs"].as_array().expect("artifact refs");
        assert!(!refs.is_empty());
        let mut has_non_manifest_ref = false;
        for reference in refs {
            let artifact_id = reference["artifact_id"].as_str().expect("artifact id");
            assert!(
                allowed_artifact_ids.contains(artifact_id),
                "inspect must reference only the run manifest or output_refs reported by geo run, got {artifact_id}"
            );
            has_non_manifest_ref |= artifact_id != "geo-run-manifest/head.json";
        }
        if answer.get("abstention").is_some() {
            abstained_questions.push(answer["question"].as_str().unwrap().to_string());
        } else {
            assert!(
                has_non_manifest_ref,
                "non-abstaining answers must cite at least one stored output artifact"
            );
        }
    }
    assert!(
        abstained_questions.is_empty(),
        "full synthetic run should answer without abstentions: {abstained_questions:?}"
    );
    assert!(
        inspection_json["answers"][1]["answer"]
            .as_str()
            .unwrap()
            .contains("proof_class=fixture")
    );
    assert!(
        inspection_json["answers"][3]["answer"]
            .as_str()
            .unwrap()
            .contains("truth_reach=unverified")
    );
    assert!(
        !inspection_json["answers"][3]["answer"]
            .as_str()
            .unwrap()
            .contains("accurate resolution")
    );
    assert!(
        inspection_json["answers"][5]["answer"]
            .as_str()
            .unwrap()
            .contains("semantic reconciliation requires its own artifact/verdict")
    );
    assert!(
        inspection_json["answers"][7]["answer"]
            .as_str()
            .unwrap()
            .contains("recommend_next=true reads this artifact only")
    );

    let compare = canon_command()
        .arg("geo")
        .arg("inspect")
        .arg("--run")
        .arg(&fixture.work_dir)
        .arg("--compare")
        .arg(&fixture.work_dir)
        .assert()
        .success();
    let compared = parse_inspection(&compare.get_output().stdout);
    let delta = compared.compare.expect("compare delta");
    assert!(delta.evidence_added.is_empty());
    assert!(delta.evidence_removed.is_empty());
    assert_eq!(delta.model_count_before, delta.model_count_after);
}

#[test]
fn t12_geo_inspect_refuses_first_missing_referenced_cas_artifact() {
    let fixture = run_public_synthetic_chain();
    let solve = output_ref(&fixture.run, "geo.building.solve/solve");
    let digest = solve["content_digest"].as_str().expect("solve digest");
    let path = cas_path(&fixture.work_dir, digest);
    fs::rename(&path, fixture.work_dir.join("removed-solve-cas.bin"))
        .expect("move solve CAS artifact out of place");

    let assert = canon_command()
        .arg("geo")
        .arg("inspect")
        .arg("--run")
        .arg(&fixture.work_dir)
        .assert()
        .code(2);
    assert!(assert.get_output().stderr.is_empty());
    let refusal: Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_inspect_error_code"],
        "inspect_artifact_missing"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["detail"]["artifact"],
        "geo.building.solve/solve"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["detail"]["expected_digest"],
        json!(digest)
    );
    assert_eq!(
        refusal["refusal"]["next_command"],
        "canon geo inspect --run <DIR> [--component <ID>] [--compare <OTHER_RUN>] [--recommend-next]"
    );
}

#[test]
fn t25_geo_inspect_types_missing_explanation_as_unanswerable_not_refusal() {
    let fixture = run_public_synthetic_chain();
    let mut run: GeoRun = serde_json::from_value(fixture.run.clone()).expect("Geo run parses");
    run.output_refs
        .retain(|reference| reference.contract_version != CANON_GEO_EXPLANATION_VERSION);
    restamp_and_write_run_manifest(&fixture.work_dir, &mut run);

    let inspection = inspect(&fixture.work_dir).expect("inspect succeeds without explanation");
    validate_inspection_artifact(&inspection).expect("inspection validates");
    let q6 = inspection
        .answers
        .iter()
        .find(|answer| answer.question == GeoInspectionQuestion::Q6)
        .expect("Q6 answer exists");
    assert!(q6.answer.contains(CANON_GEO_EXPLANATION_VERSION));
    assert_eq!(q6.artifact_refs.len(), 1);
    assert_eq!(
        q6.artifact_refs[0].artifact_id,
        "geo-run-manifest/head.json"
    );
    let abstention = q6.abstention.as_ref().expect("Q6 abstains");
    assert_eq!(
        abstention.code,
        GeoInspectErrorCode::InspectQuestionUnanswerable
    );
    assert_eq!(abstention.missing_contract, CANON_GEO_EXPLANATION_VERSION);
}

#[test]
fn t25_geo_inspection_validation_rejects_empty_answer_refs() {
    let fixture = run_public_synthetic_chain();
    let mut inspection = inspect(&fixture.work_dir).expect("inspection builds");
    inspection.answers[0].artifact_refs.clear();
    let error =
        validate_inspection_artifact(&inspection).expect_err("empty answer refs must reject");
    assert_eq!(error.code, GeoInspectErrorCode::InvalidInput);
    assert_eq!(error.detail.get("question").map(String::as_str), Some("Q1"));
}

#[test]
fn geo_inspect_compare_refuses_mismatched_question_hash() {
    let fixture = run_public_synthetic_chain();
    let base = inspect(&fixture.work_dir).expect("base inspection");
    let mut other = base.clone();
    other.plan_ref.question_hash = digest("different.question.hash");
    let error = compare(&base, &other).expect_err("mismatched question hash must refuse");
    assert_eq!(error.code, GeoInspectErrorCode::InvalidInput);
    assert_eq!(
        error.detail.get("plan_ref").map(String::as_str),
        Some("question_hash")
    );
}

#[test]
fn t27_geo_inspect_has_no_solve_composition_call_path() {
    let source = fs::read_to_string("src/geo/inspect.rs").expect("read inspect module");
    assert!(
        !source.contains("solve_composition"),
        "inspect must answer from stored artifacts without a solve call path"
    );
}

fn restamp_and_write_run_manifest(work_dir: &Path, run: &mut GeoRun) {
    run.semantic_hash.clear();
    run.run_id.clear();
    run.semantic_hash = geo_run_semantic_hash(run).expect("Geo run semantic hash");
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
    let bytes = canonical_geo_run_bytes(run).expect("Geo run canonical bytes");
    let policy = ProjectRunPolicy::new(work_dir, ".canon/geo-run");
    let content_hash = format!("blake3:{}", blake3::hash(&bytes).to_hex());
    let revision =
        geo_run_manifest_revision_path(&policy, &content_hash).expect("Geo run revision path");
    fs::create_dir_all(revision.parent().expect("revision parent"))
        .expect("create revision parent");
    fs::write(&revision, &bytes).expect("write manifest revision");
    let head = geo_run_manifest_head_path(&policy).expect("Geo run head path");
    fs::write(&head, &bytes).expect("write manifest head");
}

#[test]
fn geo_inspection_answers_are_architecture_questions_in_order() {
    let expected = [
        GeoInspectionQuestion::Q1,
        GeoInspectionQuestion::Q2,
        GeoInspectionQuestion::Q3,
        GeoInspectionQuestion::Q4,
        GeoInspectionQuestion::Q5,
        GeoInspectionQuestion::Q6,
        GeoInspectionQuestion::Q7,
        GeoInspectionQuestion::Q8,
    ];
    let serialized = serde_json::to_value(expected).expect("questions serialize");
    assert_eq!(
        serialized,
        json!(["Q1", "Q2", "Q3", "Q4", "Q5", "Q6", "Q7", "Q8"])
    );
}
