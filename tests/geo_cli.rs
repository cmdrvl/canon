//! CLI-level contract for the `canon geo` family.
//!
//! The library kernel is exercised by `tests/geo_composition.rs` and friends;
//! this file only pins the operator surface: typed requests in, canonical
//! artifact bytes out, typed refusals with exit code 2 on bad input.

use assert_cmd::Command;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use canon::geo::{GeoTileWorkRequest, materialize_tile_work_unit};
use h3o::CellIndex;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf, str::FromStr};
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

fn tiny_geometry_request(max_geometry_bytes_per_tile: u64) -> Value {
    json!({
        "version": "canon_geo_geometry_request.v0",
        "frame": {
            "version": "canon_geo_local_frame.v0",
            "frame_id": "tile:892a100d26bffff:local-mm:v1",
            "tile_id": "892a100d26bffff",
            "source_crs": "LOCAL:TEST-METRES",
            "source_axis_domain": "planar",
            "source_decimal_places": 3,
            "source_origin": { "x": 0, "y": 0 },
            "affine": {
                "x_from_source_x_numerator": 1,
                "x_from_source_y_numerator": 0,
                "y_from_source_x_numerator": 0,
                "y_from_source_y_numerator": 1,
                "denominator": 1
            },
            "projection": {
                "method_id": "fixture-fixed-affine",
                "method_version": "1.0.0",
                "parameters_blake3": blake3::hash(b"fixture-fixed-affine-v1").to_hex().to_string(),
                "max_projection_error_micrometres": 200
            },
            "max_abs_coordinate_mm": 2_000_000
        },
        "features": [{
            "feature_id": "parcel-1",
            "source_crs": "LOCAL:TEST-METRES",
            "geometry": {
                "kind": "polygon",
                "exterior": [
                    { "x": "0", "y": "0" },
                    { "x": "5", "y": "0" },
                    { "x": "5", "y": "5" },
                    { "x": "0", "y": "5" },
                    { "x": "0", "y": "0" }
                ],
                "holes": []
            }
        }],
        "max_vertices_per_geometry": 100,
        "max_geometry_bytes_per_tile": max_geometry_bytes_per_tile
    })
}

fn tiny_warehouse_geometry_request(declared_sha256: Option<&str>) -> Value {
    let points = [
        (980_252.301_632_881_2_f64, 191_655.610_172_272_3_f64),
        (980_352.301_632_881_2, 191_655.610_172_272_3),
        (980_352.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_755.610_172_272_3),
        (980_252.301_632_881_2, 191_655.610_172_272_3),
    ];
    let mut wkb = vec![1];
    wkb.extend_from_slice(&3_u32.to_le_bytes());
    wkb.extend_from_slice(&1_u32.to_le_bytes());
    wkb.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for (x, y) in points {
        wkb.extend_from_slice(&x.to_le_bytes());
        wkb.extend_from_slice(&y.to_le_bytes());
    }
    let sha256: String = Sha256::digest(&wkb)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    json!({
        "version": "canon_geo_warehouse_geometry_rows.v0",
        "tile_id": "892a100d26bffff",
        "frame_id": "tile:892a100d26bffff:epsg2263-mm:v0",
        "source_crs": "EPSG:2263",
        "source_srid": 2263,
        "source_decimal_places": 9,
        "source_origin": { "x": "980000", "y": "191000" },
        "source_unit_to_millimetres": {
            "unit_id": "us-survey-foot",
            "numerator": 1200000,
            "denominator": 3937
        },
        "rows": [{
            "feature_id": "parcel-1",
            "source_record_id": "mn/000000/1",
            "source_dataset": "nyc_dcp_mappluto",
            "source_release": "26v2",
            "source_release_date": "2026-08-01",
            "source_geometry_contract_version": "nyc_dcp_mappluto_geometry_evidence.v3",
            "source_archive_sha256": "e06eca9034731bc23f058bf532090e3c1ea6aed44a8128c6928f33872da34ab5",
            "source_crs": "EPSG:2263",
            "source_srid": 2263,
            "source_geom_wkb_base64": BASE64_STANDARD.encode(&wkb),
            "source_geom_wkb_sha256": declared_sha256.unwrap_or(&sha256),
            "transform_execution_id": "sha256-execution-26v2",
            "transform_definition_id": "sha256-definition-hpgn"
        }],
        "max_abs_coordinate_mm": 1000000,
        "max_vertices_per_geometry": 10000,
        "max_geometry_bytes_per_tile": 1000000
    })
}

fn tiny_home_cell_rows(coordinate_crs: &str) -> Value {
    json!({
        "version": "canon_geo_home_cell_rows.v0",
        "coordinate_crs": coordinate_crs,
        "coordinate_decimal_places": 9,
        "h3_resolution": 9,
        "stability_radius_fixed": 1000,
        "rows": [{
            "source_name": "mappluto",
            "feature_id": "parcel-a",
            "source_snapshot": "26v2/2026-08-01/geom-v3",
            "source_record_id": "mn/000000/1",
            "geometry_sha256": "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed",
            "representative_point_method": "centroid_of_derived_wgs84_geometry",
            "longitude": "-73.977264000",
            "latitude": "40.753429000",
            "transform_execution_id": "sha256-execution-26v2",
            "transform_definition_id": "sha256-definition-hpgn",
            "claimed_home_cell": "892a100d26bffff"
        }],
        "max_rows": 8
    })
}

fn tile_cells() -> (CellIndex, CellIndex, CellIndex) {
    let center = CellIndex::from_str("892a100d26bffff").expect("valid fixture cell");
    let neighbor = center
        .grid_disk_safe(1)
        .find(|cell| *cell != center)
        .expect("fixture cell has a neighbor");
    let k1 = center.grid_disk_safe(1).collect::<BTreeSet<_>>();
    let outside = center
        .grid_disk_safe(2)
        .find(|cell| !k1.contains(cell))
        .expect("k2 contains a cell outside k1");
    (center, neighbor, outside)
}

fn tiny_tile_work_request(home_cell: CellIndex) -> Value {
    let (center, _, _) = tile_cells();
    json!({
        "version": "canon_geo_tile_work_request.v0",
        "center_cell": center.to_string(),
        "halo_k": 1,
        "features": [{
            "source_name": "parcel",
            "feature_id": "parcel-a",
            "home_cell": home_cell.to_string()
        }],
        "max_features": 8,
        "max_work_cells": 7
    })
}

fn tiny_tile_reconciliation_request(second_payload: &str) -> Value {
    let (first, second, _) = tile_cells();
    let members = json!([
        {
            "source_name": "parcel",
            "feature_id": "parcel-a",
            "home_cell": first.to_string()
        },
        {
            "source_name": "building",
            "feature_id": "building-a",
            "home_cell": second.to_string()
        }
    ]);
    let work_unit = |center: CellIndex| {
        let request: GeoTileWorkRequest = serde_json::from_value(json!({
            "version": "canon_geo_tile_work_request.v0",
            "center_cell": center.to_string(),
            "halo_k": 1,
            "features": [
                {
                    "source_name": "parcel",
                    "feature_id": "parcel-a",
                    "home_cell": first.to_string()
                },
                {
                    "source_name": "building",
                    "feature_id": "building-a",
                    "home_cell": second.to_string()
                }
            ],
            "max_features": 8,
            "max_work_cells": 7
        }))
        .expect("typed tile work request");
        serde_json::to_value(
            materialize_tile_work_unit(&request).expect("tile work unit materializes"),
        )
        .expect("tile work unit serializes")
    };
    let first_payload = format!("blake3:{}", blake3::hash(b"tile payload").to_hex());
    json!({
        "version": "canon_geo_tile_reconciliation_request.v0",
        "halo_k": 1,
        "batches": [
            {
                "work_unit": work_unit(first),
                "proposals": [{
                    "payload_blake3": first_payload,
                    "members": members.clone()
                }]
            },
            {
                "work_unit": work_unit(second),
                "proposals": [{
                    "payload_blake3": second_payload,
                    "members": members
                }]
            }
        ],
        "max_batches": 4,
        "max_proposals": 8,
        "max_members_per_decision": 8,
        "max_features_per_batch": 8,
        "max_work_cells_per_batch": 7
    })
}

#[test]
fn geo_materialize_geometry_emits_canonical_values_and_typed_budget_refusals() {
    let temp = tempdir().expect("tempdir");
    let request = write_json(
        temp.path(),
        "geometry.json",
        &tiny_geometry_request(100_000),
    );

    let first = canon_command()
        .args([
            "geo",
            "materialize-geometry",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    let second = canon_command()
        .args([
            "geo",
            "materialize-geometry",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    let artifact: Value =
        serde_json::from_slice(&first.get_output().stdout).expect("geometry tile artifact parses");
    assert_eq!(artifact["version"], "canon_geo_geometry_tile.v0");
    assert_eq!(artifact["features"][0]["value"]["kind"], "geometry");
    assert_eq!(
        artifact["features"][0]["value"]["value"]["coordinate_unit"],
        "millimetre"
    );
    assert_eq!(artifact["features"][0]["value"]["value"]["vertex_count"], 4);

    let too_small = write_json(
        temp.path(),
        "geometry-too-small.json",
        &tiny_geometry_request(1),
    );
    let refusal = canon_command()
        .args([
            "geo",
            "materialize-geometry",
            "--request",
            too_small.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout)
        .expect("geometry budget refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_geometry_error_code"],
        "tile_byte_budget_exceeded"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["budget"]["policy_id"],
        "geometry.max_bytes_per_tile"
    );
}

#[test]
fn geo_materialize_warehouse_geometry_verifies_source_bytes_and_emits_receipts() {
    let temp = tempdir().expect("tempdir");
    let rows = write_json(
        temp.path(),
        "warehouse-geometry.json",
        &tiny_warehouse_geometry_request(None),
    );
    let first = canon_command()
        .args([
            "geo",
            "materialize-warehouse-geometry",
            "--rows",
            rows.to_str().unwrap(),
        ])
        .assert()
        .success();
    let second = canon_command()
        .args([
            "geo",
            "materialize-warehouse-geometry",
            "--rows",
            rows.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    let artifact: Value = serde_json::from_slice(&first.get_output().stdout)
        .expect("warehouse geometry artifact parses");
    assert_eq!(artifact["version"], "canon_geo_warehouse_geometry.v0");
    assert_eq!(artifact["source_receipt"]["source_crs"], "EPSG:2263");
    assert_eq!(
        artifact["geometry_tile"]["frame"]["projection"]["max_projection_error_micrometres"],
        0
    );
    assert_eq!(
        artifact["source_receipt"]["max_abs_source_quantization_error_micrometres_ceiling"],
        1
    );

    let bad = write_json(
        temp.path(),
        "warehouse-geometry-bad.json",
        &tiny_warehouse_geometry_request(Some(&"0".repeat(64))),
    );
    let refusal = canon_command()
        .args([
            "geo",
            "materialize-warehouse-geometry",
            "--rows",
            bad.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value =
        serde_json::from_slice(&refusal.get_output().stdout).expect("digest refusal parses");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_geometry_error_code"],
        "invalid_source_digest"
    );
}

#[test]
fn geo_tile_work_emits_a_bounded_work_unit_and_refuses_outside_reach() {
    let temp = tempdir().expect("tempdir");
    let (center, _, outside) = tile_cells();
    let request = write_json(
        temp.path(),
        "tile-work.json",
        &tiny_tile_work_request(center),
    );
    let first = canon_command()
        .args(["geo", "tile-work", "--request", request.to_str().unwrap()])
        .assert()
        .success();
    let second = canon_command()
        .args(["geo", "tile-work", "--request", request.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    let artifact: Value = serde_json::from_slice(&first.get_output().stdout).unwrap();
    assert_eq!(artifact["version"], "canon_geo_tile_work_unit.v0");
    assert_eq!(artifact["work_cells"].as_array().unwrap().len(), 7);
    assert_eq!(artifact["center_feature_count"], 1);
    assert_eq!(artifact["halo_feature_count"], 0);

    let outside_request = write_json(
        temp.path(),
        "tile-work-outside.json",
        &tiny_tile_work_request(outside),
    );
    let refusal = canon_command()
        .args([
            "geo",
            "tile-work",
            "--request",
            outside_request.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_tile_error_code"],
        "feature_outside_halo"
    );
}

#[test]
fn geo_materialize_home_cells_emits_h3o_assignment_and_reports_claimed_mismatch() {
    let temp = tempdir().expect("tempdir");
    let rows = write_json(
        temp.path(),
        "home-cells.json",
        &tiny_home_cell_rows("EPSG:4326"),
    );
    let first = canon_command()
        .args([
            "geo",
            "materialize-home-cells",
            "--rows",
            rows.to_str().unwrap(),
        ])
        .assert()
        .success();
    let second = canon_command()
        .args([
            "geo",
            "materialize-home-cells",
            "--rows",
            rows.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    let artifact: Value = serde_json::from_slice(&first.get_output().stdout).unwrap();
    assert_eq!(artifact["version"], "canon_geo_home_cell_assignment.v0");
    assert_eq!(artifact["features"][0]["home_cell"], "892a100d62bffff");
    assert_eq!(artifact["features"][0]["parity"], "mismatch");
    assert_eq!(artifact["summary"]["mismatches"], 1);
    assert_eq!(artifact["tile_work_features"][0]["feature_id"], "parcel-a");

    let bad = write_json(
        temp.path(),
        "home-cells-bad-crs.json",
        &tiny_home_cell_rows("EPSG:2263"),
    );
    let refusal = canon_command()
        .args([
            "geo",
            "materialize-home-cells",
            "--rows",
            bad.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout).unwrap();
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_tile_error_code"],
        "invalid_input"
    );
}

#[test]
fn geo_reconcile_tiles_emits_one_owner_and_refuses_nonconfluence() {
    let temp = tempdir().expect("tempdir");
    let payload = format!("blake3:{}", blake3::hash(b"tile payload").to_hex());
    let request = write_json(
        temp.path(),
        "reconcile.json",
        &tiny_tile_reconciliation_request(&payload),
    );
    let success = canon_command()
        .args([
            "geo",
            "reconcile-tiles",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    let artifact: Value = serde_json::from_slice(&success.get_output().stdout).unwrap();
    assert_eq!(artifact["version"], "canon_geo_tile_reconciliation.v0");
    assert_eq!(artifact["input_proposals"], 2);
    assert_eq!(artifact["owned_decisions"], 1);
    assert_eq!(artifact["discarded_halo_proposals"], 1);

    let conflicting = format!("blake3:{}", blake3::hash(b"conflicting payload").to_hex());
    let bad_request = write_json(
        temp.path(),
        "reconcile-conflict.json",
        &tiny_tile_reconciliation_request(&conflicting),
    );
    let refusal = canon_command()
        .args([
            "geo",
            "reconcile-tiles",
            "--request",
            bad_request.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout).unwrap();
    assert_eq!(
        refusal["refusal"]["detail"]["geo_tile_error_code"],
        "non_confluent_decision"
    );
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
        "canon_geo_composition_request.v0 or canon_geo_evidence_compilation.v0"
    );
    assert!(refusal["refusal"]["next_command"].is_string());
}

#[test]
fn geo_materialize_evidence_emits_a_compiler_accepted_request() {
    let temp = tempdir().expect("tempdir");
    let rows = write_json(
        temp.path(),
        "warehouse-rows.json",
        &json!({
            "version": "canon_geo_warehouse_rows.v0",
            "parcel_rows": [
                { "parcel_id": "parcel-b" },
                { "parcel_id": "parcel-a" }
            ],
            "building_parcel_rows": [],
            "contracts": [{
                "id": "rho.exported-candidates",
                "version": "1.0.0",
                "source_dataset": "SOURCE.EXPORTED_PARCEL_FACTS",
                "source_release": "26v1",
                "source_lineage_ids": ["SOURCE.NYC_DCP_MAPPLUTO_HOT:26v1"],
                "method_id": "predicate-c-positive-area",
                "method_version": "1.0.0",
                "claim_role": "stable_identity_anchor",
                "basis": {
                    "kind": "logical_relaxation",
                    "invariant_id": "candidate-set-is-a-superset"
                }
            }],
            "evidence_rows": [
                {
                    "observation_id": "obs.candidates",
                    "contract_id": "rho.exported-candidates",
                    "source_record": {
                        "source_record_id": "export-row-b",
                        "source_vintage": "26v1",
                        "record_blake3": "6ee7136102b255723487ec7a5d9f0a8ac0efc6fdf1972830c25eda91072ee151"
                    },
                    "observation": {
                        "kind": "exact_sets",
                        "level": "parcel",
                        "sets": [["parcel-a", "parcel-b"]]
                    }
                },
                {
                    "observation_id": "obs.candidates",
                    "contract_id": "rho.exported-candidates",
                    "source_record": {
                        "source_record_id": "export-row-a",
                        "source_vintage": "26v1",
                        "record_blake3": "c54e12755a1240376324a828921506c5090b4d653d67dad129713a1856f766cc"
                    },
                    "observation": {
                        "kind": "exact_sets",
                        "level": "parcel",
                        "sets": [["parcel-a", "parcel-b"]]
                    }
                }
            ],
            "max_assignments": 64,
            "max_materialized_models": 64
        }),
    );

    let materialized = canon_command()
        .args([
            "geo",
            "materialize-evidence",
            "--rows",
            rows.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(materialized.get_output().stderr.is_empty());
    let request: Value = serde_json::from_slice(&materialized.get_output().stdout)
        .expect("materialized request parses");
    assert_eq!(request["version"], "canon_geo_evidence_request.v0");
    assert_eq!(
        request["universe"]["parcels"],
        json!(["parcel-a", "parcel-b"])
    );
    assert_eq!(request["observations"].as_array().unwrap().len(), 1);
    assert_eq!(
        request["observations"][0]["source_records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let request_path = temp.path().join("evidence-request.json");
    fs::write(&request_path, &materialized.get_output().stdout)
        .expect("write materialized request");
    let compilation = canon_command()
        .args([
            "geo",
            "compile-evidence",
            "--request",
            request_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let compilation: Value =
        serde_json::from_slice(&compilation.get_output().stdout).expect("compilation parses");
    assert_eq!(compilation["admissions"].as_array().unwrap().len(), 1);
    assert_eq!(
        compilation["composition_request"]["hard_constraints"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
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
                    "source_dataset": "fixture:parcel-addresses",
                    "source_release": "fixture-v1",
                    "source_lineage_ids": ["fixture:parcel-addresses:upstream"],
                    "method_id": "fixture:address-existential",
                    "method_version": "1.0.0",
                    "claim_role": "stable_identity_anchor",
                    "basis": {
                        "kind": "logical_relaxation",
                        "invariant_id": "fixture:address-membership"
                    }
                }
            ],
            "observations": [
                {
                    "id": "obs.one",
                    "contract_id": "rho.existential",
                    "source_records": [{
                        "source_record_id": "parcel-address-row-1",
                        "source_vintage": "fixture-v1",
                        "record_blake3": "8c7db293f7195e1a3c4d397c2bcf2f59a4fd289f9a302b295669bcccb938d333"
                    }],
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

    // The compilation artifact itself is a first-class solve input. The solve
    // output keeps a content-addressed link to the exact admitted evidence
    // artifact instead of forcing operators to extract and orphan its nested
    // composition request.
    let compilation = temp.path().join("compiled-evidence.json");
    fs::write(&compilation, &stdout).expect("write compilation artifact");
    let solved = canon_command()
        .args(["geo", "solve", "--request", compilation.to_str().unwrap()])
        .assert()
        .success();
    let solved: Value =
        serde_json::from_slice(&solved.get_output().stdout).expect("solve artifact parses");
    assert_eq!(solved["summary"]["residual_model_count"], 2);
    assert_eq!(
        solved["evidence_compilation"]["version"],
        "canon_geo_evidence_compilation.v0"
    );
    assert_eq!(
        solved["evidence_compilation"]["blake3"],
        blake3::hash(stdout.trim_end().as_bytes())
            .to_hex()
            .to_string()
    );

    let mut tampered = artifact.clone();
    tampered["composition_request"]["hard_constraints"][0]["id"] =
        json!("rho:injected@v1:not-admitted");
    let tampered_path = write_json(temp.path(), "tampered-compilation.json", &tampered);
    let refusal = canon_command()
        .args(["geo", "solve", "--request", tampered_path.to_str().unwrap()])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout)
        .expect("tampered compilation refusal parses");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_evidence_error_code"],
        "invalid_input"
    );

    let mut semantic_tamper = artifact;
    semantic_tamper["admissions"][0]["observation"]["members"][0]["id"] = json!("parcel-b");
    let semantic_tamper_path = write_json(
        temp.path(),
        "semantic-tamper-compilation.json",
        &semantic_tamper,
    );
    let refusal = canon_command()
        .args([
            "geo",
            "solve",
            "--request",
            semantic_tamper_path.to_str().unwrap(),
        ])
        .assert()
        .code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout)
        .expect("semantic tamper refusal parses");
    assert_eq!(
        refusal["refusal"]["detail"]["message"],
        "Geo evidence compilation does not replay from its admitted observations"
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
                                "source_records": [{
                                    "source_record_id": "parcel-set-row-1",
                                    "source_vintage": "fixture-v1",
                                    "record_blake3": "97e7e532ba98fb5ce35769f30b61b738d906c6686f17c7d8bbbf61bf3f8b910c"
                                }],
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
