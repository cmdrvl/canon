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

fn tiny_address_evidence_request() -> Value {
    let member = json!({
        "member_id": "pad:first:199",
        "lot_id": "1004540041",
        "house": { "kind": "discrete", "value": 199 },
        "street": {
            "name": [{ "kind": "ordinal", "value": 1 }],
            "suffix": "avenue"
        }
    });
    let typed_member: canon::geo::GeoPadAddressMember =
        serde_json::from_value(member.clone()).expect("address member fixture parses");
    let normalized_member_blake3 =
        canon::geo::geo_pad_member_blake3(&typed_member).expect("address member fixture hashes");
    json!({
        "version": "canon_geo_address_parcel_evidence_request.v0",
        "parse_request": {
            "version": "canon_geo_address_parse_request.v0",
            "input": "199 First Avenue",
            "jurisdiction": { "kind": "nyc", "borough": "manhattan" }
        },
        "address_set": {
            "version": "canon_geo_pad_address_set.v0",
            "jurisdiction": { "kind": "nyc", "borough": "manhattan" },
            "members": [member]
        },
        "bridge_request": {
            "version": "canon_geo_address_parcel_bridge_request.v0",
            "observation_id": "obs.address.pad.membership",
            "contract_id": "rho.address.pad.membership",
            "query_as_of": {
                "utc_day": "2026-08-31",
                "semantic_id": "demo:query_as_of",
                "unit": "utc_day",
                "origin": "caller_declared"
            },
            "member_source_records": [{
                "member_id": "pad:first:199",
                "normalized_member_blake3": normalized_member_blake3,
                "source_record": {
                    "source_record_id": "pad:26B:first:199",
                    "source_vintage": "26B/2026-05-01",
                    "record_blake3": blake3::hash(b"pad:26B:first:199").to_hex().to_string()
                }
            }]
        }
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

fn synthetic_building_center_cell() -> &'static str {
    "892a100d62bffff"
}

fn synthetic_building_home_cell_rows() -> Value {
    let center = synthetic_building_center_cell();
    json!({
        "version": "canon_geo_home_cell_rows.v0",
        "coordinate_crs": "EPSG:4326",
        "coordinate_decimal_places": 9,
        "h3_resolution": 9,
        "stability_radius_fixed": 1000,
        "rows": [
            synthetic_building_home_cell_row("building-a", "synthetic-not-live/building-a", center),
            synthetic_building_home_cell_row("building-b", "synthetic-not-live/building-b", center)
        ],
        "max_rows": 16
    })
}

fn synthetic_building_home_cell_row(
    feature_id: &str,
    source_record_id: &str,
    center: &str,
) -> Value {
    json!({
        "source_name": "synthetic_building_fixture_not_live",
        "feature_id": feature_id,
        "source_snapshot": "synthetic-fixture-not-live/2026-08-31",
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

fn synthetic_building_tile_work_request() -> Value {
    let center = synthetic_building_center_cell();
    json!({
        "version": "canon_geo_tile_work_request.v0",
        "center_cell": center,
        "halo_k": 1,
        "features": [
            {
                "source_name": "synthetic_building_fixture_not_live",
                "feature_id": "building-a",
                "home_cell": center
            },
            {
                "source_name": "synthetic_building_fixture_not_live",
                "feature_id": "building-b",
                "home_cell": center
            }
        ],
        "max_features": 16,
        "max_work_cells": 7
    })
}

fn synthetic_building_warehouse_rows() -> Value {
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

fn geo_run_building_question() -> Value {
    json!({
        "version": "canon_geo_question.v0",
        "question_id": "question.synthetic-not-live.geo-run.building",
        "subject_bindings": [
            {
                "role": "target",
                "binding_class": "operator_label",
                "value": "synthetic-not-live building fixture"
            }
        ],
        "bounded_geography": geo_plan_region(),
        "requested_grains": [{
            "entity_level": "building",
            "required_evidence_classes": ["building_footprint"],
            "optional_evidence_classes": ["address_set"]
        }],
        "query_as_of": geo_plan_as_of("2026-08-31", "question.synthetic-not-live.query_as_of.utc_day"),
        "requested_claim_classes": ["candidate_reach", "stable_identity"],
        "presentation_limits": [
            geo_plan_bound("presentation.synthetic-not-live.max_models", "models", 16, "model"),
            geo_plan_bound("presentation.synthetic-not-live.max_candidates", "candidates", 32, "candidate")
        ],
        "abstention_policy": {
            "unsupported_grain": "report_unsupported",
            "unresolved_residual": "report_residual",
            "budget_fallback": "report_residual"
        },
        "decision_policy": null,
        "resource_budget_ref": "budget.fixture.geo-plan"
    })
}

fn write_geo_run_building_plan(dir: &std::path::Path) -> PathBuf {
    let paths = GeoPlanInputPaths {
        question: write_json(
            dir,
            "synthetic-not-live-question.json",
            &geo_run_building_question(),
        ),
        capabilities: write_geo_plan_capabilities(dir),
        inventory: write_json(
            dir,
            "synthetic-not-live-inventory.json",
            &geo_plan_inventory(false),
        ),
        profile: write_json(
            dir,
            "synthetic-not-live-profile.json",
            &geo_plan_profile("canon_geo_composition_profile.v0"),
        ),
        budget: write_json(
            dir,
            "synthetic-not-live-budget.json",
            &geo_plan_budget(false),
        ),
    };
    let assert = geo_plan_command(&paths).assert().success();
    assert!(assert.get_output().stderr.is_empty());
    let plan: Value = serde_json::from_slice(&assert.get_output().stdout).expect("plan parses");
    assert_eq!(plan["status"], "planned");
    assert_eq!(
        plan["project_plan"]["nodes"].as_array().unwrap().len(),
        5,
        "synthetic building demo must exercise the five Geo leaf commands"
    );
    let path = dir.join("synthetic-not-live-building-plan.json");
    fs::write(&path, &assert.get_output().stdout).expect("write synthetic Geo plan");
    path
}

fn write_geo_run_synthetic_input_set(dir: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        write_json(
            dir,
            "synthetic-not-live-home-cell-rows.json",
            &synthetic_building_home_cell_rows(),
        ),
        write_json(
            dir,
            "synthetic-not-live-tile-work-request.json",
            &synthetic_building_tile_work_request(),
        ),
        write_json(
            dir,
            "synthetic-not-live-warehouse-rows.json",
            &synthetic_building_warehouse_rows(),
        ),
    )
}

fn geo_run_acquisition_inventory() -> Value {
    let mut inventory = geo_plan_inventory(false);
    let source = inventory["sources"]
        .as_array_mut()
        .expect("inventory sources are an array")
        .iter_mut()
        .find(|source| source["source_instance_id"] == "source.fixture.building-footprints")
        .expect("building footprint source exists");
    source["local_state"] = json!({ "state": "missing" });
    inventory
}

fn write_geo_run_acquisition_plan(dir: &std::path::Path) -> (PathBuf, Value) {
    let paths = GeoPlanInputPaths {
        question: write_json(
            dir,
            "synthetic-not-live-acquisition-question.json",
            &geo_run_building_question(),
        ),
        capabilities: write_geo_plan_capabilities(dir),
        inventory: write_json(
            dir,
            "synthetic-not-live-acquisition-inventory.json",
            &geo_run_acquisition_inventory(),
        ),
        profile: write_json(
            dir,
            "synthetic-not-live-acquisition-profile.json",
            &geo_plan_profile("canon_geo_composition_profile.v0"),
        ),
        budget: write_json(
            dir,
            "synthetic-not-live-acquisition-budget.json",
            &geo_plan_budget(false),
        ),
    };
    let assert = geo_plan_command(&paths).assert().success();
    assert!(assert.get_output().stderr.is_empty());
    let plan_bytes = assert.get_output().stdout.clone();
    let plan: Value = serde_json::from_slice(&plan_bytes).expect("acquisition plan parses");
    assert_eq!(plan["status"], "partial");
    let acquisition_requests = plan["external_requests"]
        .as_array()
        .expect("external requests")
        .iter()
        .filter(|request| request["kind"] == "acquisition")
        .collect::<Vec<_>>();
    assert_eq!(
        acquisition_requests.len(),
        1,
        "synthetic missing-local plan must emit one acquisition request"
    );
    let acquisition = acquisition_requests[0];
    assert_eq!(
        acquisition["handoff"]["expected_receipt_contract"],
        "canon_geo_acquisition_receipt.v0"
    );
    assert_eq!(
        acquisition["handoff"]["required_result_digest_algorithm"],
        "blake3"
    );
    let path = dir.join("synthetic-not-live-acquisition-plan.json");
    fs::write(&path, plan_bytes).expect("write synthetic acquisition Geo plan");
    (path, acquisition["request"].clone())
}

fn synthetic_not_live_empty_acquisition_artifact() -> Value {
    json!({
        "version": "canon_geo_warehouse_rows.v0",
        "profile": {
            "version": "canon_geo_composition_profile.v0",
            "selection_level": "building"
        },
        "parcel_rows": [],
        "building_parcel_rows": [],
        "contracts": [],
        "evidence_rows": [],
        "max_assignments": 16,
        "max_materialized_models": 16
    })
}

fn geo_cli_blake3_digest(digest_id: &str, bytes: &[u8]) -> Value {
    json!({
        "digest_id": digest_id,
        "algorithm": "blake3",
        "hex_digest": blake3::hash(bytes).to_hex().to_string()
    })
}

fn write_geo_run_zero_rows_acquisition_receipt(
    dir: &std::path::Path,
    request: &Value,
    artifact_bytes: &[u8],
) -> PathBuf {
    let typed_request: canon::geo::GeoAcquisitionRequest =
        serde_json::from_value(request.clone()).expect("acquisition request parses");
    let request_semantic_hash = canon::geo::geo_acquisition_request_semantic_hash(&typed_request)
        .expect("acquisition request semantic hash computes");
    let artifact_digest =
        geo_cli_blake3_digest("artifact.synthetic-not-live.zero_rows", artifact_bytes);
    let receipt = json!({
        "version": "canon_geo_acquisition_receipt.v0",
        "request_id": request["request_id"].clone(),
        "request_semantic_hash": request_semantic_hash,
        "terminal_state": "ZERO_ROWS",
        "proof_class": "fixture",
        "fixture_id": "synthetic-not-live.geo-cli.zero-rows",
        "bounded_geography": request["bounded_geography"].clone(),
        "subset": request["subset"].clone(),
        "releases": request["releases"].clone(),
        "fields": request["fields"].clone(),
        "projection": request["projection"].clone(),
        "normalized_executed_request_digest": geo_cli_blake3_digest(
            "normalized.synthetic-not-live.zero_rows",
            b"synthetic-not-live-zero-rows-executed-request"
        ),
        "pagination": {
            "requested_page": request["pagination"].clone(),
            "rows_truncated": false,
            "bytes_truncated": false
        },
        "counts": {
            "rows": 0,
            "bytes": artifact_bytes.len() as u64
        },
        "denominators": [{
            "denominator_id": "requested_subset.synthetic-not-live",
            "source": "requested_subset",
            "count": 0,
            "unit": "row",
            "description": "synthetic-not-live zero-row acquisition denominator"
        }],
        "source_digests": [
            geo_cli_blake3_digest("source.synthetic-not-live.zero_rows", b"synthetic-not-live-source")
        ],
        "result_digests": [
            geo_cli_blake3_digest("result.synthetic-not-live.zero_rows", artifact_bytes)
        ],
        "local_artifacts": [{
            "artifact_id": "artifact.synthetic-not-live.zero_rows",
            "media_type": "application/json",
            "byte_count": artifact_bytes.len() as u64,
            "digest": artifact_digest
        }],
        "unreadable_columns": [],
        "resumability": {
            "resumable": false,
            "retry_guidance": "retry the same synthetic-not-live request with a positive receipt"
        }
    });
    write_json(
        dir,
        "synthetic-not-live-zero-rows-acquisition-receipt.json",
        &receipt,
    )
}

fn geo_plan_digest(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn geo_plan_region() -> Value {
    json!({
        "geography_id": "region.fixture.geo-plan",
        "geography_kind": "bounded_fixture",
        "description": "Geo plan CLI fixture region"
    })
}

fn geo_plan_as_of(day: &str, semantic_id: &str) -> Value {
    json!({
        "utc_day": day,
        "semantic_id": semantic_id,
        "unit": "utc_day",
        "origin": "caller_declared"
    })
}

fn geo_plan_question(reordered: bool) -> Value {
    let mut subject_bindings = vec![
        json!({
            "role": "target",
            "binding_class": "operator_label",
            "value": "fixture building"
        }),
        json!({
            "role": "input_address",
            "binding_class": "address_text",
            "value": "10 Fixture Street"
        }),
    ];
    let mut requested_grains = vec![
        json!({
            "entity_level": "building",
            "required_evidence_classes": ["building_footprint"],
            "optional_evidence_classes": ["address_set"]
        }),
        json!({
            "entity_level": "unit",
            "required_evidence_classes": ["asserted_attribute"],
            "optional_evidence_classes": []
        }),
    ];
    let mut requested_claim_classes = vec![json!("candidate_reach"), json!("stable_identity")];
    let mut presentation_limits = vec![
        geo_plan_bound("presentation.max_models", "models", 16, "model"),
        geo_plan_bound("presentation.max_candidates", "candidates", 32, "candidate"),
    ];
    if reordered {
        subject_bindings.reverse();
        requested_grains.reverse();
        requested_claim_classes.reverse();
        presentation_limits.reverse();
    }
    json!({
        "version": "canon_geo_question.v0",
        "question_id": "question.fixture.geo-plan",
        "subject_bindings": subject_bindings,
        "bounded_geography": geo_plan_region(),
        "requested_grains": requested_grains,
        "query_as_of": geo_plan_as_of("2026-08-31", "question.query_as_of.utc_day"),
        "requested_claim_classes": requested_claim_classes,
        "presentation_limits": presentation_limits,
        "abstention_policy": {
            "unsupported_grain": "report_unsupported",
            "unresolved_residual": "report_residual",
            "budget_fallback": "report_residual"
        },
        "decision_policy": null,
        "resource_budget_ref": "budget.fixture.geo-plan"
    })
}

fn geo_plan_bound(id: &str, counter: &str, value: u64, unit: &str) -> Value {
    json!({
        "semantic_id": id,
        "counter": counter,
        "value": value,
        "unit": unit,
        "origin": "caller_declared",
        "action": "report_budget_fallback"
    })
}

fn geo_plan_budget(reordered: bool) -> Value {
    let mut deterministic_bounds = vec![
        geo_plan_bound("budget.max_bytes", "bytes", 1_000_000, "byte"),
        geo_plan_bound("budget.max_rows", "rows", 10_000, "row"),
        geo_plan_bound("budget.max_cells", "cells", 64, "cell"),
        geo_plan_bound("budget.max_candidates", "candidates", 500, "candidate"),
        geo_plan_bound("budget.max_variables", "variables", 128, "variable"),
        geo_plan_bound("budget.max_states", "states", 100_000, "state"),
        geo_plan_bound("budget.max_models", "models", 10_000, "model"),
        geo_plan_bound(
            "budget.max_operations",
            "operations",
            1_000_000,
            "operation",
        ),
    ];
    if reordered {
        deterministic_bounds.reverse();
    }
    json!({
        "version": "canon_geo_resource_budget.v0",
        "budget_id": "budget.fixture.geo-plan",
        "deterministic_bounds": deterministic_bounds,
        "telemetry": [{
            "metric": "wall_time",
            "unit": "millisecond",
            "origin": "operator_policy",
            "semantic_effect": "none"
        }]
    })
}

fn geo_plan_source(
    source_instance_id: &str,
    evidence_classes: Vec<&str>,
    temporal_scope: Value,
    reordered: bool,
) -> Value {
    let mut lineage_ids = vec![json!("lineage.fixture.one"), json!("lineage.fixture.two")];
    if reordered {
        lineage_ids.reverse();
    }
    json!({
        "source_instance_id": source_instance_id,
        "release": {
            "release_id": "release.fixture.geo-plan",
            "release_digest": geo_plan_digest("release.fixture.geo-plan")
        },
        "temporal_scope": temporal_scope,
        "lineage_ids": lineage_ids,
        "native_scope": {
            "kind": "native_entity",
            "entity_level": "building"
        },
        "evidence_classes": evidence_classes,
        "coverage": {
            "coverage_id": format!("coverage.{source_instance_id}"),
            "region": geo_plan_region(),
            "predicate": "all declared fixture records"
        },
        "local_state": {
            "state": "available",
            "local_ref": {
                "artifact_id": format!("artifact.{source_instance_id}"),
                "content_hash": geo_plan_digest(&format!("local.{source_instance_id}")),
                "media_type": "application/json"
            }
        },
        "geometry": {
            "geometry_contract_version": "geometry.fixture.v1",
            "coordinate_reference_system": "EPSG:4326",
            "transform_id": "identity.fixture",
            "transform_digest": geo_plan_digest("identity.fixture"),
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

fn geo_plan_inventory(reordered: bool) -> Value {
    let building_evidence = if reordered {
        vec!["address_set", "building_footprint"]
    } else {
        vec!["building_footprint", "address_set"]
    };
    let address_evidence = if reordered {
        vec!["asserted_attribute", "address_set"]
    } else {
        vec!["address_set", "asserted_attribute"]
    };
    let timed_scope = json!({
        "valid_time": {
            "start_utc_day": "2026-01-01",
            "end_utc_day": "2026-12-31"
        },
        "release_time": geo_plan_as_of("2026-05-01", "source.release.utc_day")
    });
    let mut sources = vec![
        geo_plan_source(
            "source.fixture.building-footprints",
            building_evidence,
            timed_scope,
            reordered,
        ),
        geo_plan_source(
            "source.fixture.address-attributes",
            address_evidence,
            json!({}),
            reordered,
        ),
    ];
    if reordered {
        sources.reverse();
    }
    json!({
        "version": "canon_geo_regional_inventory.v0",
        "inventory_id": "inventory.fixture.geo-plan",
        "region": geo_plan_region(),
        "sources": sources,
        "discovery_gaps": []
    })
}

fn geo_plan_profile(version: &str) -> Value {
    json!({
        "version": version,
        "selection_level": "building"
    })
}

struct GeoPlanInputPaths {
    question: PathBuf,
    capabilities: PathBuf,
    inventory: PathBuf,
    profile: PathBuf,
    budget: PathBuf,
}

fn write_geo_plan_capabilities(dir: &std::path::Path) -> PathBuf {
    let capabilities = canon_command()
        .args(["geo", "capabilities", "--emit", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let path = dir.join("capabilities.json");
    fs::write(&path, capabilities).expect("write capabilities file");
    path
}

fn write_geo_plan_inputs(
    dir: &std::path::Path,
    reordered: bool,
    profile_version: &str,
) -> GeoPlanInputPaths {
    GeoPlanInputPaths {
        question: write_json(dir, "question.json", &geo_plan_question(reordered)),
        capabilities: write_geo_plan_capabilities(dir),
        inventory: write_json(dir, "inventory.json", &geo_plan_inventory(reordered)),
        profile: write_json(dir, "profile.json", &geo_plan_profile(profile_version)),
        budget: write_json(dir, "budget.json", &geo_plan_budget(reordered)),
    }
}

fn geo_plan_command(paths: &GeoPlanInputPaths) -> Command {
    let mut command = canon_command();
    command
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
        .arg(&paths.budget);
    command
}

#[test]
fn geo_plan_emits_canonical_partial_plan_and_binds_capabilities() {
    let temp = tempdir().expect("tempdir");
    let paths = write_geo_plan_inputs(temp.path(), false, "canon_geo_composition_profile.v0");

    let first = geo_plan_command(&paths).assert().success();
    let second = geo_plan_command(&paths).assert().success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    assert!(first.get_output().stderr.is_empty());

    let stdout = String::from_utf8(first.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(stdout.ends_with('\n'));
    let plan: Value = serde_json::from_str(stdout.trim_end()).expect("plan JSON parses");
    let capabilities: Value =
        serde_json::from_slice(&fs::read(&paths.capabilities).expect("read capabilities"))
            .expect("capabilities JSON parses");

    assert_eq!(plan["version"], "canon_geo_plan.v0");
    assert_eq!(plan["status"], "partial");
    assert_eq!(
        plan["capabilities_ref"]["semantic_hash"], capabilities["semantic_hash"],
        "--capabilities is a semantic input to the plan"
    );
    assert_eq!(
        plan["profile_ref"]["version"],
        "canon_geo_composition_profile.v0"
    );
    assert_eq!(plan["profile_ref"]["selection_level"], "building");
    assert_eq!(
        plan["project_plan"]["schema_version"],
        "canon.project.plan.v1"
    );
    assert!(plan["external_requests"].as_array().unwrap().is_empty());

    let outcomes = plan["grain_outcomes"].as_array().unwrap();
    let building = outcomes
        .iter()
        .find(|outcome| outcome["entity_level"] == "building")
        .expect("building grain outcome");
    assert_eq!(building["status"], "planned_relative_to_declared_universe");
    assert!(
        building["claim_limitation"]
            .as_str()
            .unwrap()
            .contains("truth reach is unverified")
    );
    let unit = outcomes
        .iter()
        .find(|outcome| outcome["entity_level"] == "unit")
        .expect("unit grain outcome");
    assert_eq!(unit["status"], "unsupported_by_profile");
    assert!(
        unit.get("project_node_ids")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    );

    let node_ids = plan["project_plan"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["node_id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    for expected in [
        "geo.building.home_cells",
        "geo.building.section",
        "geo.building.materialize_evidence",
        "geo.building.compile_evidence",
        "geo.building.solve",
    ] {
        assert!(
            node_ids.contains(expected),
            "supported building grain should schedule {expected}"
        );
    }
    assert!(!node_ids.contains("geo.unit.solve"));
}

#[test]
fn geo_plan_is_byte_identical_for_reordered_inputs() {
    let temp = tempdir().expect("tempdir");
    let paths_a = write_geo_plan_inputs(temp.path(), false, "canon_geo_composition_profile.v0");
    let nested = temp.path().join("reordered");
    fs::create_dir(&nested).expect("create reordered fixture dir");
    let paths_b = write_geo_plan_inputs(&nested, true, "canon_geo_composition_profile.v0");

    let first = geo_plan_command(&paths_a).assert().success();
    let second = geo_plan_command(&paths_b).assert().success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
}

#[test]
fn geo_run_cli_executes_synthetic_not_live_five_leaf_building_chain() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("synthetic-not-live-inputs");
    fs::create_dir(&input_dir).expect("create synthetic input dir");
    let plan = write_geo_run_building_plan(&input_dir);
    let (home_cells, tile_work, warehouse_rows) = write_geo_run_synthetic_input_set(&input_dir);
    let work_dir = temp.path().join("synthetic-not-live-work");
    fs::create_dir(&work_dir).expect("create synthetic run work dir");
    let home_binding = format!("geo.building.home_cells:rows={}", home_cells.display());
    let tile_binding = format!("geo.building.section:request={}", tile_work.display());
    let warehouse_binding = format!(
        "geo.building.materialize_evidence:rows={}",
        warehouse_rows.display()
    );

    let assert = canon_command()
        .arg("geo")
        .arg("run")
        .arg("--plan")
        .arg(&plan)
        .arg("--work-dir")
        .arg(&work_dir)
        .arg("--input")
        .arg(&home_binding)
        .arg("--input")
        .arg(&tile_binding)
        .arg("--input")
        .arg(&warehouse_binding)
        .assert()
        .success();
    assert!(assert.get_output().stderr.is_empty());

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout");
    assert!(stdout.ends_with('\n'));
    let run: Value = serde_json::from_str(stdout.trim_end()).expect("run JSON parses");
    assert_eq!(run["version"], "canon_geo_run.v0");
    assert_eq!(run["status"], "COMPLETED", "run JSON: {run}");
    assert_eq!(run["phase"], "SOLVED");
    let semantic_hash = run["semantic_hash"].as_str().expect("semantic hash");
    assert!(semantic_hash.starts_with("blake3:"));
    assert_eq!(
        run["run_id"],
        format!(
            "canon_geo_run.v0:{}",
            semantic_hash.trim_start_matches("blake3:")
        )
    );
    assert_eq!(
        run["project_run_report"]["executed_nodes"],
        json!([
            "geo.building.compile_evidence",
            "geo.building.home_cells",
            "geo.building.materialize_evidence",
            "geo.building.section",
            "geo.building.solve"
        ])
    );
    assert_eq!(run["artifact_inputs"].as_array().unwrap().len(), 3);
    assert_eq!(run["output_refs"].as_array().unwrap().len(), 5);
    assert!(
        run["output_refs"].as_array().unwrap().iter().any(|output| {
            output["artifact_id"] == "geo.building.solve/solve"
                && output["contract_version"] == "canon_geo_composition.v0"
        }),
        "run JSON must expose the typed solve output ref"
    );

    let solve_path = work_dir.join("geo/building/solve.json");
    let solve_bytes = fs::read(&solve_path).expect("solve artifact is published");
    let solve: Value = serde_json::from_slice(&solve_bytes).expect("solve artifact parses");
    assert_eq!(solve["version"], "canon_geo_composition.v0");
    assert_eq!(solve["status"], "resolved");
    assert_eq!(solve["summary"]["residual_model_count"], 1);
    assert_eq!(solve["summary"]["component_count"], 1);
    assert_eq!(solve["factorization"][0]["key"], "building:building-a");
    assert_eq!(
        solve["evidence_compilation"]["version"],
        "canon_geo_evidence_compilation.v0"
    );
}

#[test]
fn geo_run_cli_refuses_synthetic_not_live_wrong_explicit_binding_contract() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("synthetic-not-live-inputs");
    fs::create_dir(&input_dir).expect("create synthetic input dir");
    let plan = write_geo_run_building_plan(&input_dir);
    let (home_cells, _, warehouse_rows) = write_geo_run_synthetic_input_set(&input_dir);
    let work_dir = temp.path().join("synthetic-not-live-work");
    fs::create_dir(&work_dir).expect("create synthetic run work dir");
    let home_binding = format!("geo.building.home_cells:rows={}", home_cells.display());
    let wrong_tile_binding = format!("geo.building.section:request={}", home_cells.display());
    let warehouse_binding = format!(
        "geo.building.materialize_evidence:rows={}",
        warehouse_rows.display()
    );

    let refusal = canon_command()
        .arg("geo")
        .arg("run")
        .arg("--plan")
        .arg(&plan)
        .arg("--work-dir")
        .arg(&work_dir)
        .arg("--input")
        .arg(&home_binding)
        .arg("--input")
        .arg(&wrong_tile_binding)
        .arg("--input")
        .arg(&warehouse_binding)
        .assert()
        .code(2);

    let refusal: Value =
        serde_json::from_slice(&refusal.get_output().stdout).expect("wrong binding refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_run_error_code"],
        "ARTIFACT_CONTRACT"
    );
    assert!(
        refusal["refusal"]["detail"]["message"]
            .as_str()
            .unwrap()
            .contains("input binding contract")
    );
    assert!(
        !work_dir.join("geo/building/solve.json").exists(),
        "wrong explicit binding must refuse before publishing a solve artifact"
    );
}

#[test]
fn geo_run_cli_refuses_synthetic_not_live_non_positive_acquisition_satisfaction() {
    let temp = tempdir().expect("tempdir");
    let input_dir = temp.path().join("synthetic-not-live-acquisition-inputs");
    fs::create_dir(&input_dir).expect("create synthetic acquisition input dir");
    let (plan, request) = write_geo_run_acquisition_plan(&input_dir);
    let artifact = write_json(
        &input_dir,
        "synthetic-not-live-zero-rows-artifact.json",
        &synthetic_not_live_empty_acquisition_artifact(),
    );
    let artifact_bytes = fs::read(&artifact).expect("read synthetic acquisition artifact");
    let receipt = write_geo_run_zero_rows_acquisition_receipt(
        &input_dir,
        &request,
        artifact_bytes.as_slice(),
    );
    let work_dir = temp.path().join("synthetic-not-live-acquisition-work");
    fs::create_dir(&work_dir).expect("create synthetic acquisition work dir");
    let request_id = request["request_id"]
        .as_str()
        .expect("acquisition request id");
    let input_binding = format!("geo.synthetic.acquisition:artifact={}", artifact.display());
    let satisfaction = format!("{request_id}={}", receipt.display());

    let assert = canon_command()
        .arg("geo")
        .arg("run")
        .arg("--plan")
        .arg(&plan)
        .arg("--work-dir")
        .arg(&work_dir)
        .arg("--input")
        .arg(&input_binding)
        .arg("--satisfy")
        .arg(&satisfaction)
        .assert()
        .code(2);
    assert!(
        assert.get_output().stderr.is_empty(),
        "non-positive satisfaction must be a refusal, not a stderr warning"
    );

    let refusal: Value = serde_json::from_slice(&assert.get_output().stdout)
        .expect("non-positive satisfaction refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["message"],
        "Geo run --satisfy receipt did not meet its positive acquisition gate"
    );
    assert_eq!(refusal["refusal"]["detail"]["request_id"], request_id);
    assert_eq!(refusal["refusal"]["detail"]["status"], "not_satisfied");
    let findings = refusal["refusal"]["detail"]["findings"]
        .as_array()
        .expect("satisfaction findings");
    assert!(
        findings.iter().any(|finding| {
            finding["code"] == "zero_rows"
                && finding["detail"]["rows"] == "0"
                && finding["detail"]["positive_path_min_rows"] == "1"
        }),
        "refusal must expose the zero-row acquisition finding"
    );
    assert_ne!(refusal["version"], "canon_geo_run.v0");
    assert!(
        !work_dir.join("geo/building/solve.json").exists(),
        "non-positive satisfaction must refuse before an ordinary run publishes solve output"
    );
}

#[test]
fn geo_plan_refuses_missing_capabilities_file() {
    let temp = tempdir().expect("tempdir");
    let mut paths = write_geo_plan_inputs(temp.path(), false, "canon_geo_composition_profile.v0");
    paths.capabilities = temp.path().join("missing-capabilities.json");

    let refusal = geo_plan_command(&paths).assert().code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout)
        .expect("missing capabilities refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_IO");
    assert_eq!(
        refusal["refusal"]["detail"]["capabilities"],
        paths.capabilities.to_string_lossy().as_ref()
    );
    assert_eq!(
        refusal["refusal"]["next_command"],
        "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>"
    );
}

#[test]
fn geo_plan_surfaces_profile_contract_mismatch_as_typed_refusal() {
    let temp = tempdir().expect("tempdir");
    let paths = write_geo_plan_inputs(temp.path(), false, "canon_geo_composition_profile.v9");

    let refusal = geo_plan_command(&paths).assert().code(2);
    let refusal: Value = serde_json::from_slice(&refusal.get_output().stdout)
        .expect("profile contract refusal parses");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_plan_error_code"],
        "unsupported_version"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["detail"]["expected"],
        "canon_geo_composition_profile.v0"
    );
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
            "profile": {
                "version": "canon_geo_composition_profile.v0",
                "selection_level": "parcel"
            },
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
fn geo_materialize_address_evidence_runs_the_parse_pad_and_bridge_stages() {
    let temp = tempdir().expect("tempdir");
    let request = write_json(
        temp.path(),
        "address-evidence.json",
        &tiny_address_evidence_request(),
    );

    let first = canon_command()
        .args([
            "geo",
            "materialize-address-evidence",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    let second = canon_command()
        .args([
            "geo",
            "materialize-address-evidence",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);

    let bundle: Value =
        serde_json::from_slice(&first.get_output().stdout).expect("address evidence bundle parses");
    assert_eq!(
        bundle["version"],
        "canon_geo_address_parcel_evidence_bundle.v0"
    );
    assert_eq!(bundle["bridge"]["status"], "evidence_observation");
    assert_eq!(
        bundle["bridge"]["parcel_candidates"],
        json!([{ "level": "parcel", "id": "1004540041" }])
    );
    assert_eq!(
        bundle["bridge"]["observation"]["observation"]["kind"],
        "existential_membership"
    );
    assert!(bundle.get("universe").is_none());
    assert!(bundle.get("contracts").is_none());
    assert!(bundle.get("observations").is_none());
}

#[test]
fn geo_materialize_address_evidence_bundle_is_not_a_direct_compile_request() {
    let temp = tempdir().expect("tempdir");
    let request = write_json(
        temp.path(),
        "address-evidence.json",
        &tiny_address_evidence_request(),
    );

    let materialized = canon_command()
        .args([
            "geo",
            "materialize-address-evidence",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let bundle_path = temp.path().join("address-bundle.json");
    fs::write(&bundle_path, &materialized).expect("write address bundle");

    let refusal = canon_command()
        .args([
            "geo",
            "compile-evidence",
            "--request",
            bundle_path.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("refusal parses");
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert_eq!(
        refusal["refusal"]["detail"]["expected_version"],
        "canon_geo_evidence_request.v0"
    );
}

#[test]
fn geo_materialize_address_evidence_refuses_an_unknown_pad_source_binding() {
    let temp = tempdir().expect("tempdir");
    let mut request_value = tiny_address_evidence_request();
    request_value["bridge_request"]["member_source_records"][0]["member_id"] = json!("pad:unknown");
    let request = write_json(temp.path(), "bad-address-evidence.json", &request_value);

    let refusal = canon_command()
        .args([
            "geo",
            "materialize-address-evidence",
            "--request",
            request.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("refusal parses");
    assert_eq!(
        refusal["refusal"]["detail"]["geo_address_error_code"],
        "invalid_input"
    );
    assert_eq!(
        refusal["refusal"]["next_command"],
        "repair the request against canon_geo_address_parcel_evidence_request.v0, then rerun canon geo materialize-address-evidence"
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
