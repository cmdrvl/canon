#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Output},
};
use tempfile::{TempDir, tempdir};

const MANIFEST: &str = include_str!("../scripts/geo_measurements/manifest.json");
const RECEIPTS_VERSION: &str = "canon_geo_measurement_receipts.v0";
const RESULT_ARTIFACT_VERSION: &str = "canon_geo_measurement_result_artifact.v0";
const RESULT_SET_VERSION: &str = "canon_geo_measurement_result_set.v0";
const EXECUTION_CHANNEL: &str = "cmdrvl_data_mcp";
const EXECUTION_TRANSFORM: &str = "cmdrvl_data_sqlglot_normalized_plus_tool_row_limit";
const LIVENESS_NOT_ATTESTED: &str = "receipt is internally consistent, but this offline runner does not attest liveness, authenticity, or query-history provenance";

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon_geo_measurements"))
}

fn manifest_value() -> Value {
    serde_json::from_str(MANIFEST).expect("manifest parses")
}

fn manifest_measurement_count() -> usize {
    manifest_value()["measurements"]
        .as_array()
        .expect("measurements")
        .len()
}

struct MeasurementFixture {
    _dir: TempDir,
    receipts_path: PathBuf,
    receipts: Value,
    executed_query_text_paths: Vec<PathBuf>,
    artifact_paths: Vec<PathBuf>,
}

type FixtureMutationCase = (
    &'static str,
    fn(&mut MeasurementFixture),
    &'static str,
    &'static str,
);
type ManifestMutationCase = (&'static str, fn(&mut Value));

#[derive(Serialize)]
struct CanonicalResultSet<'a> {
    version: &'static str,
    measurement_id: &'a str,
    source_sql_sha256: &'a str,
    executed_query_text_sha256: &'a str,
    rows: Vec<BTreeMap<String, Value>>,
}

fn valid_fixture() -> MeasurementFixture {
    let manifest = manifest_value();
    let dir = tempdir().expect("tempdir");
    let artifact_dir = dir.path().join("artifacts");
    let query_dir = dir.path().join("queries");
    fs::create_dir(&artifact_dir).expect("artifact dir");
    fs::create_dir(&query_dir).expect("query dir");
    let mut executed_query_text_paths = Vec::new();
    let mut artifact_paths = Vec::new();
    let receipts = manifest["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .enumerate()
        .map(|(index, measurement)| {
            let measurement_id = measurement["id"].as_str().expect("measurement id");
            let source_sql_sha256 = measurement["source_sql_sha256"]
                .as_str()
                .expect("source sql sha");
            let query_id = format!("01c6bd3a-0821-4a0c-9000-{:012x}", index + 1);
            let executed_query_text_path = query_dir.join(format!("{measurement_id}.sql"));
            fs::write(
                &executed_query_text_path,
                executed_query_text_bytes(index, measurement_id),
            )
            .expect("write executed query text");
            let executed_query_text_sha256 =
                sha256_hex(&fs::read(&executed_query_text_path).expect("query text bytes"));
            executed_query_text_paths.push(executed_query_text_path);
            let rows = artifact_rows(measurement);
            let artifact = json!({
                "version": RESULT_ARTIFACT_VERSION,
                "measurement_id": measurement_id,
                "execution_channel": EXECUTION_CHANNEL,
                "execution_transform": EXECUTION_TRANSFORM,
                "executed_query_text_path": format!("queries/{measurement_id}.sql"),
                "query_id": query_id.as_str(),
                "source_sql_sha256": source_sql_sha256,
                "executed_query_text_sha256": executed_query_text_sha256.as_str(),
                "rows": rows.clone()
            });
            let artifact_path = artifact_dir.join(format!("{measurement_id}.json"));
            write_json(&artifact_path, &artifact);
            let artifact_bytes = fs::read(&artifact_path).expect("artifact bytes");
            let written_artifact: Value =
                serde_json::from_slice(&artifact_bytes).expect("written artifact json");
            let written_rows = written_artifact["rows"].as_array().expect("written rows");
            let result_artifact_sha256 = sha256_hex(&artifact_bytes);
            let result_set_sha256 = result_set_sha256(
                measurement_id,
                source_sql_sha256,
                &executed_query_text_sha256,
                written_rows,
            );
            artifact_paths.push(artifact_path);
            json!({
                "measurement_id": measurement_id,
                "source_sql_sha256": source_sql_sha256,
                "executed_query_text_sha256": executed_query_text_sha256,
                "release_pins": measurement["release_pins"],
                "execution_channel": EXECUTION_CHANNEL,
                "execution_transform": EXECUTION_TRANSFORM,
                "executed_query_text_path": format!("queries/{measurement_id}.sql"),
                "query_id": query_id,
                "executed_at": "2026-08-30T12:00:00Z",
                "as_of": measurement["as_of"],
                "row_count": written_rows.len(),
                "proof_class": "contract_fixture",
                "result_artifact_path": format!("artifacts/{measurement_id}.json"),
                "result_artifact_sha256": result_artifact_sha256,
                "result_set_sha256": result_set_sha256,
                "denominators": derive_denominators(measurement_id, written_rows),
                "sanity": derive_sanity(measurement, written_rows),
                "gate_values": {}
            })
        })
        .collect::<Vec<_>>();
    let receipts = json!({
        "version": RECEIPTS_VERSION,
        "receipts": receipts
    });
    let receipts_path = dir.path().join("receipts.json");
    write_json(&receipts_path, &receipts);
    MeasurementFixture {
        _dir: dir,
        receipts_path,
        receipts,
        executed_query_text_paths,
        artifact_paths,
    }
}

fn artifact_rows(measurement: &Value) -> Value {
    match measurement["id"].as_str().expect("measurement id") {
        "appendix_b_centroid_percolation" => Value::Array(
            (0..193)
                .map(|index| {
                    json!({
                        "source": if index < 100 { "parcel" } else { "footprint" },
                        "native_id": format!("fixture-{index:03}"),
                        "lon": -73.958055_f64 + (index as f64 * 0.000001_f64),
                        "lat": 40.768843_f64 + (index as f64 * 0.000001_f64)
                    })
                })
                .collect(),
        ),
        "appendix_c_r8_density" => json!([
            {
                "parcel_containing_cell_count": 1192,
                "parcel_denominator": 856614,
                "parcel_min": 1,
                "parcel_median": 637.5,
                "parcel_mean": 718.64,
                "parcel_p90": 1586.8,
                "parcel_p99": 2103.27,
                "parcel_max": 2422,
                "footprint_denominator_in_parcel_cells": 1081175,
                "active_footprint_denominator": 1081999,
                "footprints_outside_parcel_home_cells": 824,
                "null_h3_footprints": 0,
                "footprint_median": 758.0,
                "footprint_max": 3589,
                "total_feature_median": 1395.5,
                "total_feature_p99": 4824.17,
                "total_feature_max": 6011,
                "invalid_count_cells": 0
            }
        ]),
        "appendix_d_same_cell_predicates" => json!([
            {
                "cell_name": "BK_DENSE",
                "footprint_count": 2354,
                "intersects_zero": 1,
                "intersects_one": 2353,
                "intersects_multi": 0,
                "contains_zero": 22,
                "contains_one": 2332,
                "contains_multi": 0,
                "majority_zero": 22,
                "majority_one": 2332,
                "majority_multi": 0,
                "parcel_count": 2343,
                "positive_area_parcel_overlap_pairs": 0,
                "denominator_sanity": "PASS"
            },
            {
                "cell_name": "BX_LOWER",
                "footprint_count": 291,
                "intersects_zero": 1,
                "intersects_one": 290,
                "intersects_multi": 0,
                "contains_zero": 4,
                "contains_one": 287,
                "contains_multi": 0,
                "majority_zero": 4,
                "majority_one": 287,
                "majority_multi": 0,
                "parcel_count": 300,
                "positive_area_parcel_overlap_pairs": 0,
                "denominator_sanity": "PASS"
            }
        ]),
        "appendix_d_candidate_reach" | "appendix_d_stratified_halo_centers" => {
            measurement["expected_result_rows"].clone()
        }
        "appendix_d_stratified_halo" => stratified_halo_rows(),
        "appendix_f_overture_three_source" => overture_rows(),
        "e5_franklin_county_thin_tier_readiness_v0" => measurement["expected_result_rows"].clone(),
        other => panic!("unexpected measurement {other}"),
    }
}

fn stratified_halo_rows() -> Value {
    let cells = [
        ("BK_DENSE", "882a100d8bfffff", "892a100d8a3ffff"),
        ("BX_LOWER", "882a100f4dfffff", "892a100f4c3ffff"),
        ("MN_SMALL", "882a1008c7fffff", "892a1008c67ffff"),
        ("QN_DENSE", "882a103b6bfffff", "892a103b6b7ffff"),
        ("QN_MEDIUM", "882a100e25fffff", "892a100e24fffff"),
        ("SI_LOW", "882a106019fffff", "892a1060197ffff"),
    ];
    let metrics = [
        (8, "BK_DENSE", 25786, 2354, 2333, 21, 2353, 1, 5),
        (8, "BX_LOWER", 2688, 291, 287, 4, 290, 1, 9),
        (8, "MN_SMALL", 2403, 45, 42, 3, 45, 0, 4),
        (8, "QN_DENSE", 15062, 2007, 1993, 14, 2007, 0, 4),
        (8, "QN_MEDIUM", 11856, 1049, 1036, 13, 1044, 5, 5),
        (8, "SI_LOW", 2260, 256, 204, 52, 256, 0, 71),
        (9, "BK_DENSE", 4670, 375, 366, 9, 374, 1, 5),
        (9, "BX_LOWER", 617, 69, 67, 2, 69, 0, 3),
        (9, "MN_SMALL", 378, 34, 34, 0, 34, 0, 3),
        (9, "QN_DENSE", 2451, 362, 352, 10, 362, 0, 4),
        (9, "QN_MEDIUM", 3489, 386, 372, 14, 386, 0, 4),
        (9, "SI_LOW", 662, 193, 153, 40, 193, 0, 65),
    ];
    Value::Array(
        metrics
            .into_iter()
            .map(
                |(
                    resolution,
                    stratum,
                    work_unit_nodes,
                    target_footprints,
                    same_one,
                    same_zero,
                    k1_one,
                    k1_zero,
                    max_component_size,
                )| {
                    let center_cell = cells
                        .iter()
                        .find(|(name, _, _)| *name == stratum)
                        .map(|(_, r8, r9)| if resolution == 8 { *r8 } else { *r9 })
                        .expect("center cell");
                    let work_footprints = target_footprints;
                    let work_parcels = work_unit_nodes - work_footprints;
                    json!({
                        "stratum": stratum,
                        "resolution": resolution,
                        "center_cell": center_cell,
                        "work_parcels": work_parcels,
                        "work_footprints": work_footprints,
                        "work_unit_nodes": work_unit_nodes,
                        "target_footprints": target_footprints,
                        "same_one": same_one,
                        "same_zero": same_zero,
                        "same_multi": 0,
                        "k1_one": k1_one,
                        "k1_zero": k1_zero,
                        "k1_multi": 0,
                        "global_one": k1_one,
                        "global_zero": k1_zero,
                        "global_multi": 0,
                        "truth_outside_k1": 0,
                        "repaired_by_k1": k1_one - same_one,
                        "component_count": work_parcels,
                        "component_nodes": work_parcels + target_footprints,
                        "mean_component_size": 1.0,
                        "median_component_size": 1.0,
                        "p90_component_size": 2.0,
                        "max_component_size": max_component_size,
                        "component_size_histogram": format!("1:{work_parcels}"),
                        "work_unit_sanity": "PASS",
                        "same_denominator_sanity": "PASS",
                        "k1_denominator_sanity": "PASS",
                        "global_denominator_sanity": "PASS",
                        "reach_sanity": "PASS",
                        "forest_sanity": "PASS",
                        "component_accounting_sanity": "PASS"
                    })
                },
            )
            .collect(),
    )
}

fn overture_rows() -> Value {
    let cells = [
        ("BK_DENSE", "882a100d8bfffff", "892a100d8a3ffff"),
        ("BX_LOWER", "882a100f4dfffff", "892a100f4c3ffff"),
        ("MN_SMALL", "882a1008c7fffff", "892a1008c67ffff"),
        ("QN_DENSE", "882a103b6bfffff", "892a103b6b7ffff"),
        ("QN_MEDIUM", "882a100e25fffff", "892a100e24fffff"),
        ("SI_LOW", "882a106019fffff", "892a1060197ffff"),
    ];
    let groups = [
        (8, "nyc_footprint", 6002, 0),
        (8, "overture_building", 6018, 5967),
        (9, "nyc_footprint", 1419, 0),
        (9, "overture_building", 1401, 1393),
    ];
    let mut rows = Vec::new();
    for (resolution, source_name, target_total, osm_total) in groups {
        let targets = split_counts(target_total, cells.len());
        let osm_counts = split_counts(osm_total, cells.len());
        for (index, (stratum, r8_cell, r9_cell)) in cells.iter().enumerate() {
            let target_observations = targets[index];
            let osm_lineage_observations = osm_counts[index];
            let same_zero = if target_observations > 1 { 1 } else { 0 };
            let same_one = target_observations - same_zero;
            let center_cell = if resolution == 8 { *r8_cell } else { *r9_cell };
            let work_parcels = 400 + index as u64;
            let work_nyc_footprints = 500 + index as u64;
            let work_overture_buildings = 600 + index as u64;
            rows.push(json!({
                "stratum": stratum,
                "resolution": resolution,
                "center_cell": center_cell,
                "source_name": source_name,
                "work_parcels": work_parcels,
                "work_nyc_footprints": work_nyc_footprints,
                "work_overture_buildings": work_overture_buildings,
                "work_unit_nodes": work_parcels + work_nyc_footprints + work_overture_buildings,
                "target_observations": target_observations,
                "osm_lineage_observations": osm_lineage_observations,
                "same_one": same_one,
                "same_zero": same_zero,
                "same_multi": 0,
                "k1_one": target_observations,
                "k1_zero": 0,
                "k1_multi": 0,
                "global_one": target_observations,
                "global_zero": 0,
                "global_multi": 0,
                "truth_outside_k1": 0,
                "repaired_by_k1": same_zero,
                "component_count": work_parcels,
                "component_nodes": work_parcels + target_observations,
                "mean_component_size": 1.0,
                "median_component_size": 1.0,
                "p90_component_size": 2.0,
                "max_component_size": 7 + index as u64,
                "work_unit_sanity": "PASS",
                "same_denominator_sanity": "PASS",
                "k1_denominator_sanity": "PASS",
                "global_denominator_sanity": "PASS",
                "reach_sanity": "PASS",
                "source_forest_sanity": "PASS",
                "source_count_sanity": "PASS",
                "component_accounting_sanity": "PASS"
            }));
        }
    }
    Value::Array(rows)
}

fn split_counts(total: u64, parts: usize) -> Vec<u64> {
    let parts_u64 = u64::try_from(parts).expect("parts fit u64");
    let base = total / parts_u64;
    let remainder = total % parts_u64;
    (0..parts)
        .map(|index| {
            let extra = if u64::try_from(index).expect("index") < remainder {
                1
            } else {
                0
            };
            base + extra
        })
        .collect()
}

fn derive_denominators(measurement_id: &str, rows: &[Value]) -> Value {
    match measurement_id {
        "appendix_b_centroid_percolation" => json!({
            "observation_count": rows.len()
        }),
        "appendix_c_r8_density" => {
            let row = rows.first().expect("density row");
            json!({
                "parcel_containing_cell_count": row["parcel_containing_cell_count"],
                "parcel_denominator": row["parcel_denominator"],
                "active_footprint_denominator": row["active_footprint_denominator"]
            })
        }
        "appendix_d_same_cell_predicates" | "appendix_d_candidate_reach" => json!({
            "total_footprints": sum_rows(rows, "footprint_count")
        }),
        "appendix_d_stratified_halo_centers" => json!({
            "selected_center_count": rows.len()
        }),
        "appendix_d_stratified_halo" => json!({
            "r8_target_footprints": sum_rows_where(rows, "target_footprints", "resolution", &json!(8)),
            "r9_target_footprints": sum_rows_where(rows, "target_footprints", "resolution", &json!(9))
        }),
        "appendix_f_overture_three_source" => json!({
            "total_center_observations": sum_rows(rows, "target_observations"),
            "overture_osm_lineage_observations": sum_rows_where(rows, "osm_lineage_observations", "source_name", &json!("overture_building"))
        }),
        "e5_franklin_county_thin_tier_readiness_v0" => e5_denominators(rows),
        other => panic!("unexpected measurement {other}"),
    }
}

fn e5_denominators(rows: &[Value]) -> Value {
    let mut denominators = serde_json::Map::new();
    for field in [
        "subject_properties",
        "subject_loans",
        "multi_property_loans",
        "subject_center_cells",
        "work_cells",
    ] {
        denominators.insert(field.to_string(), json!(shared_u64(rows, field)));
    }
    for evidence_class in [
        "fema_structures",
        "microsoft_footprints",
        "overture_addresses",
        "overture_buildings",
    ] {
        let row = rows
            .iter()
            .find(|row| row["evidence_class"] == evidence_class)
            .expect("e5 evidence row");
        for field in ["feature_rows", "distinct_features", "occupied_work_cells"] {
            denominators.insert(
                format!("{evidence_class}.{field}"),
                json!(row[field].as_u64().expect("u64 field")),
            );
        }
    }
    Value::Object(denominators)
}

fn shared_u64(rows: &[Value], field: &str) -> u64 {
    let first = rows[0][field].as_u64().expect("shared u64 field");
    assert!(
        rows.iter()
            .all(|row| row[field].as_u64().expect("shared u64 field") == first)
    );
    first
}

fn derive_sanity(measurement: &Value, rows: &[Value]) -> Value {
    let mut sanity = serde_json::Map::new();
    for (field, expected) in measurement["expected_sanity"]
        .as_object()
        .expect("expected sanity")
    {
        let actual = if field == "artifact_row_count_matches_expected" {
            json!(
                rows.len()
                    == measurement["expected_row_count"]
                        .as_u64()
                        .expect("row count") as usize
            )
        } else if expected.is_string() {
            if rows.iter().all(|row| row.get(field) == Some(expected)) {
                expected.clone()
            } else {
                json!("FAIL")
            }
        } else if expected.is_u64() || expected.is_i64() {
            json!(sum_rows(rows, field))
        } else if expected.is_boolean() {
            json!(rows.iter().all(|row| row.get(field) == Some(expected)))
        } else {
            panic!("unsupported sanity fixture field {field}");
        };
        sanity.insert(field.clone(), actual);
    }
    Value::Object(sanity)
}

fn sum_rows(rows: &[Value], field: &str) -> u64 {
    rows.iter()
        .map(|row| row[field].as_u64().expect("u64 field"))
        .sum()
}

fn sum_rows_where(rows: &[Value], field: &str, selector: &str, expected_selector: &Value) -> u64 {
    rows.iter()
        .filter(|row| row.get(selector) == Some(expected_selector))
        .map(|row| row[field].as_u64().expect("u64 field"))
        .sum()
}

fn executed_query_text_bytes(index: usize, measurement_id: &str) -> Vec<u8> {
    format!(
        "-- contract fixture normalized query text; not live cmdrvl-data proof\nSELECT '{measurement_id}' AS measurement_id, {index} AS fixture_order\nLIMIT 100000;\n"
    )
    .into_bytes()
}

fn result_set_sha256(
    measurement_id: &str,
    source_sql_sha256: &str,
    executed_query_text_sha256: &str,
    rows: &[Value],
) -> String {
    let rows = canonical_rows(rows);
    let view = CanonicalResultSet {
        version: RESULT_SET_VERSION,
        measurement_id,
        source_sql_sha256,
        executed_query_text_sha256,
        rows,
    };
    sha256_hex(&serde_json::to_vec(&view).expect("canonical result set"))
}

fn canonical_rows(rows: &[Value]) -> Vec<BTreeMap<String, Value>> {
    let mut rows = rows
        .iter()
        .map(|row| {
            row.as_object()
                .expect("row object")
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| serde_json::to_string(row).expect("row json"));
    rows
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("json serializes"),
    )
    .expect("write json");
}

fn write_fixture_receipts(fixture: &MeasurementFixture) {
    write_json(&fixture.receipts_path, &fixture.receipts);
}

fn run_report_raw_path(receipts_path: &Path) -> std::process::Output {
    bin()
        .arg("--manifest")
        .arg("scripts/geo_measurements/manifest.json")
        .arg("--receipts")
        .arg(receipts_path)
        .arg("--emit")
        .arg("report")
        .output()
        .expect("run measurement validator")
}

fn run_report_raw_value(receipts: &Value) -> std::process::Output {
    let dir = tempdir().expect("tempdir");
    let receipts_path = dir.path().join("receipts.json");
    write_json(&receipts_path, receipts);
    run_report_raw_path(&receipts_path)
}

fn run_measurement_script_with_paths(args: Vec<String>) -> Output {
    let mut command = StdCommand::new("bash");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/geo_measurements/run.sh"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env(
            "CANON_GEO_MEASUREMENTS_BIN",
            env!("CARGO_BIN_EXE_canon_geo_measurements"),
        );
    for arg in args {
        command.arg(arg);
    }
    command.output().expect("run measurement shell runner")
}

fn run_report(fixture: &MeasurementFixture) -> (bool, Value) {
    let output = run_report_raw_path(&fixture.receipts_path);
    let stdout: Value = serde_json::from_slice(&output.stdout).expect("stdout json");
    (output.status.success(), stdout)
}

fn rewrite_artifact_rows(fixture: &mut MeasurementFixture, index: usize, rows: Value) {
    let receipt = &mut fixture.receipts["receipts"][index];
    let measurement_id = receipt["measurement_id"]
        .as_str()
        .expect("measurement id")
        .to_string();
    let source_sql_sha256 = receipt["source_sql_sha256"]
        .as_str()
        .expect("source sha")
        .to_string();
    let executed_query_text_sha256 = receipt["executed_query_text_sha256"]
        .as_str()
        .expect("executed sha")
        .to_string();
    let artifact = json!({
        "version": RESULT_ARTIFACT_VERSION,
        "measurement_id": measurement_id.as_str(),
        "execution_channel": EXECUTION_CHANNEL,
        "execution_transform": EXECUTION_TRANSFORM,
        "executed_query_text_path": receipt["executed_query_text_path"],
        "query_id": receipt["query_id"],
        "source_sql_sha256": source_sql_sha256.as_str(),
        "executed_query_text_sha256": executed_query_text_sha256.as_str(),
        "rows": rows
    });
    write_json(&fixture.artifact_paths[index], &artifact);
    receipt["result_artifact_sha256"] = json!(sha256_hex(
        &fs::read(&fixture.artifact_paths[index]).expect("artifact bytes")
    ));
    receipt["result_set_sha256"] = json!(result_set_sha256(
        &measurement_id,
        &source_sql_sha256,
        &executed_query_text_sha256,
        artifact["rows"].as_array().expect("rows"),
    ));
    receipt["row_count"] = json!(artifact["rows"].as_array().expect("rows").len());
    write_fixture_receipts(fixture);
}

fn rewrite_artifact_query_id(fixture: &mut MeasurementFixture, index: usize, query_id: Value) {
    let artifact_path = &fixture.artifact_paths[index];
    let mut artifact: Value =
        serde_json::from_slice(&fs::read(artifact_path).expect("artifact bytes"))
            .expect("artifact json");
    artifact["query_id"] = query_id.clone();
    write_json(artifact_path, &artifact);
    let receipt = &mut fixture.receipts["receipts"][index];
    receipt["query_id"] = query_id;
    receipt["result_artifact_sha256"] = json!(sha256_hex(
        &fs::read(artifact_path).expect("artifact bytes")
    ));
}

fn status_for<'a>(report: &'a Value, measurement_id: &str) -> &'a str {
    report["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|row| row["measurement_id"] == measurement_id)
        .expect("measurement row")["status"]
        .as_str()
        .expect("status")
}

fn details_for<'a>(report: &'a Value, measurement_id: &str) -> &'a Vec<Value> {
    report["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|row| row["measurement_id"] == measurement_id)
        .expect("measurement row")["details"]
        .as_array()
        .expect("details")
}

#[test]
fn plan_is_ordered_offline_and_excludes_h7() {
    let output = bin().arg("--emit").arg("plan").output().expect("run plan");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("plan json");
    assert_eq!(plan["version"], "canon_geo_measurement_plan.v0");
    assert_eq!(
        plan["execution"],
        "operator_fed_cmdrvl_data_receipts_only_no_snowflake_execution"
    );
    assert!(
        plan["claim_boundary"]
            .as_str()
            .expect("claim boundary")
            .contains("contract fixture")
    );
    assert!(
        plan["claim_boundary"]
            .as_str()
            .expect("claim boundary")
            .contains("unordered canonical result set")
    );
    let ids = plan["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "appendix_b_centroid_percolation",
            "appendix_c_r8_density",
            "appendix_d_same_cell_predicates",
            "appendix_d_candidate_reach",
            "appendix_d_stratified_halo_centers",
            "appendix_d_stratified_halo",
            "appendix_f_overture_three_source",
            "e5_franklin_county_thin_tier_readiness_v0"
        ]
    );
    assert!(!ids.iter().any(|id| id.to_ascii_lowercase().contains("h7")));

    let dir = tempdir().expect("tempdir");
    let output = bin()
        .current_dir(dir.path())
        .arg("--repo-root")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("--emit")
        .arg("plan")
        .output()
        .expect("run plan from another cwd");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn valid_receipts_verify_and_permutation_is_deterministic() {
    let fixture = valid_fixture();
    let (ok, report) = run_report(&fixture);
    assert!(ok, "{report}");
    assert_eq!(
        report["summary"]["receipt_consistent"],
        json!(manifest_measurement_count())
    );
    assert!(report["summary"].get("verified").is_none());
    assert_eq!(report["summary"]["malformed"], 0);
    assert_eq!(report["measurements"][0]["status"], "receipt_consistent");
    assert_eq!(
        report["measurements"][0]["executed_at"],
        "2026-08-30T12:00:00Z"
    );
    assert_eq!(report["measurements"][0]["as_of"], "2026-08-28");
    assert_eq!(
        report["measurements"][0]["declared_proof_class"],
        "contract_fixture"
    );
    assert!(report["measurements"][0]["proof_class"].is_null());
    assert_eq!(
        report["measurements"][0]["executed_query_text_path"],
        "queries/appendix_b_centroid_percolation.sql"
    );
    assert!(
        report["measurements"][0]["details"]
            .as_array()
            .expect("details")
            .iter()
            .any(|detail| detail == LIVENESS_NOT_ATTESTED)
    );
    assert_eq!(
        report["measurements"][0]["execution_transform"],
        EXECUTION_TRANSFORM
    );
    assert_ne!(
        report["measurements"][0]["source_sql_sha256"],
        report["measurements"][0]["executed_query_text_sha256"]
    );
    assert_eq!(
        report["measurements"][0]["release_pins"],
        manifest_value()["measurements"][0]["release_pins"]
    );

    let mut permuted = valid_fixture();
    permuted.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
        .reverse();
    write_fixture_receipts(&permuted);
    let (permuted_ok, permuted_report) = run_report(&permuted);
    assert!(permuted_ok, "{permuted_report}");
    assert_eq!(report, permuted_report);
}

#[test]
fn missing_and_duplicate_receipts_are_rejected() {
    let mut missing = valid_fixture();
    let missing_index = missing.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
        .iter()
        .position(|receipt| receipt["measurement_id"] == "appendix_f_overture_three_source")
        .expect("appendix F receipt");
    missing.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
        .remove(missing_index);
    write_fixture_receipts(&missing);
    let (ok, report) = run_report(&missing);
    assert!(!ok);
    assert_eq!(report["summary"]["missing"], 1);
    assert_eq!(
        status_for(&report, "appendix_f_overture_three_source"),
        "missing"
    );

    let mut duplicate = valid_fixture();
    let first = duplicate.receipts["receipts"][0].clone();
    duplicate.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
        .push(first);
    write_fixture_receipts(&duplicate);
    let (ok, report) = run_report(&duplicate);
    assert!(!ok);
    assert_eq!(
        status_for(&report, "appendix_b_centroid_percolation"),
        "malformed"
    );
}

#[test]
fn duplicate_query_ids_and_bad_receipt_claims_are_rejected() {
    let cases: [FixtureMutationCase; 14] = [
        (
            "duplicate_query_id",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][1]["query_id"] =
                    fixture.receipts["receipts"][0]["query_id"].clone();
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "noncanonical_query_id",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["query_id"] =
                    json!("01C6BD3A-0821-4a0c-9000-000000000001");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "wrong_execution_channel",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["execution_channel"] = json!("snowflake_console");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "wrong_execution_transform",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["execution_transform"] =
                    json!("direct_file_bytes_no_transform");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "invalid_calendar_date",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["as_of"] = json!("2026-02-30");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "zero_row_green",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["row_count"] = json!(0);
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "zero_denominator_green",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["denominators"]["observation_count"] = json!(0);
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "failed_sanity",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["sanity"]["artifact_row_count_matches_expected"] =
                    json!(false);
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "receipt_sql_drift",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["source_sql_sha256"] =
                    json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "executed_query_digest_bad_hex",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["executed_query_text_sha256"] =
                    json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "executed_query_text_corruption",
            |fixture: &mut MeasurementFixture| {
                fs::write(
                    &fixture.executed_query_text_paths[0],
                    b"corrupted query text\n",
                )
                .expect("corrupt query text");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "undeclared_gate_field",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["gate_values"]["warehouse_magic"] = json!(1);
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "fixture_labeled_live",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["proof_class"] = json!("fixture_live");
            },
            "appendix_b_centroid_percolation",
            "malformed",
        ),
        (
            "snapshot_moved",
            |fixture: &mut MeasurementFixture| {
                fixture.receipts["receipts"][0]["release_pins"]["mappluto.release"] = json!("26v9");
            },
            "appendix_b_centroid_percolation",
            "snapshot_moved",
        ),
    ];

    for (name, mutate, id, expected_status) in cases {
        let mut fixture = valid_fixture();
        mutate(&mut fixture);
        write_fixture_receipts(&fixture);
        let (ok, report) = run_report(&fixture);
        assert!(!ok, "{name} unexpectedly passed");
        assert_eq!(status_for(&report, id), expected_status, "{name}");
    }
}

#[test]
fn artifact_corruption_and_result_mismatch_are_rejected() {
    let mut incomplete = valid_fixture();
    let mut rows = artifact_rows(&manifest_value()["measurements"][0]);
    rows[0].as_object_mut().expect("row").remove("lat");
    rewrite_artifact_rows(&mut incomplete, 0, rows);
    let (ok, report) = run_report(&incomplete);
    assert!(!ok);
    assert_eq!(
        status_for(&report, "appendix_b_centroid_percolation"),
        "malformed"
    );

    let mut mismatch = valid_fixture();
    let mut rows = manifest_value()["measurements"][3]["expected_result_rows"].clone();
    rows[0]["global_one"] = json!(2352);
    rewrite_artifact_rows(&mut mismatch, 3, rows);
    let (ok, report) = run_report(&mismatch);
    assert!(!ok);
    assert_eq!(
        status_for(&report, "appendix_d_candidate_reach"),
        "result_mismatch"
    );
}

#[test]
fn manifest_derived_synthetic_fresh_live_is_not_verified() {
    let manifest = manifest_value();
    let receipts = manifest["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .enumerate()
        .map(|(index, measurement)| {
            let executed_query_text_sha256 = sha256_hex(&executed_query_text_bytes(
                index,
                measurement["id"].as_str().expect("id"),
            ));
            json!({
                "measurement_id": measurement["id"],
                "source_sql_sha256": measurement["source_sql_sha256"],
                "executed_query_text_sha256": executed_query_text_sha256,
                "release_pins": measurement["release_pins"],
                "execution_channel": EXECUTION_CHANNEL,
                "execution_transform": EXECUTION_TRANSFORM,
                "executed_query_text_path": format!(
                    "queries/{}.sql",
                    measurement["id"].as_str().expect("id")
                ),
                "query_id": format!("01c6bd3a-0821-4a0c-9000-{:012x}", index + 1),
                "executed_at": "2026-08-30T12:00:00Z",
                "as_of": measurement["as_of"],
                "row_count": measurement["expected_row_count"],
                "proof_class": "fresh_live",
                "denominators": measurement["expected_denominators"],
                "sanity": measurement["expected_sanity"],
                "gate_values": {}
            })
        })
        .collect::<Vec<_>>();
    let synthetic = json!({
        "version": RECEIPTS_VERSION,
        "receipts": receipts
    });
    let output = run_report_raw_value(&synthetic);
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("report json");
    assert_eq!(report["summary"]["receipt_consistent"], 0);
    assert!(report["summary"].get("verified").is_none());
    assert_eq!(
        report["summary"]["malformed"],
        json!(manifest_measurement_count())
    );
}

#[test]
fn self_authored_fresh_live_bundle_is_not_live_attested() {
    let mut fixture = valid_fixture();
    for receipt in fixture.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
    {
        receipt["proof_class"] = json!("fresh_live");
    }
    write_fixture_receipts(&fixture);
    let (ok, report) = run_report(&fixture);
    assert!(ok, "{report}");
    assert_eq!(
        report["summary"]["receipt_consistent"],
        json!(manifest_measurement_count())
    );
    assert!(report["summary"].get("verified").is_none());
    assert_eq!(report["measurements"][0]["status"], "receipt_consistent");
    assert_eq!(
        report["measurements"][0]["declared_proof_class"],
        "fresh_live"
    );
    assert_eq!(
        report["measurements"][0]["proof_attestation"],
        "receipt_consistent"
    );
    assert!(
        report["measurements"][0]["details"]
            .as_array()
            .expect("details")
            .iter()
            .any(|detail| detail == LIVENESS_NOT_ATTESTED)
    );
}

#[test]
fn t58_manifest_integrity_checks_sha256_gate_fields_and_stale_digest_exit_4() {
    let manifest = manifest_value();
    let allowed_gates = [
        "G3",
        "G4",
        "G6",
        "G7",
        "appendix_b",
        "appendix_c",
        "appendix_d",
        "appendix_f",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    for measurement in manifest["measurements"].as_array().expect("measurements") {
        let id = measurement["id"].as_str().expect("id");
        let gate = measurement["gate"].as_str().expect("gate");
        assert!(allowed_gates.contains(gate), "{id} gate {gate}");
        assert!(
            measurement["geography"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "{id} geography"
        );
        assert!(
            measurement["tier"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "{id} tier"
        );
        assert!(
            measurement["declared_grain"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "{id} declared_grain"
        );
    }

    let out = tempdir().expect("out dir");
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--dry-run".to_string(),
        "--out".to_string(),
        out.path().display().to_string(),
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("dry-run ok: 7 entries"),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        fs::read_to_string(out.path().join("run.log"))
            .expect("run log")
            .contains("entry=appendix_b_centroid_percolation")
    );

    let scratch = tempdir().expect("scratch dir");
    let manifest_path = scratch.path().join("manifest.json");
    let mut stale = manifest;
    stale["measurements"][0]["source_sql_sha256"] =
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    write_json(&manifest_path, &stale);
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--dry-run".to_string(),
        "--manifest".to_string(),
        manifest_path.display().to_string(),
        "--out".to_string(),
        scratch.path().join("stale-out").display().to_string(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("measurement diverged: sql drift"),
        "{stderr}"
    );
    assert!(
        stderr.contains("appendix_b_centroid_percolation"),
        "{stderr}"
    );
}

#[test]
fn t59_runner_classifies_snapshot_moved_and_measurement_diverged_exit_codes() {
    let valid = valid_fixture();
    let out = tempdir().expect("valid out");
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--receipts".to_string(),
        valid.receipts_path.display().to_string(),
        "--out".to_string(),
        out.path().display().to_string(),
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.path().join("report.json").is_file());
    assert!(
        out.path()
            .join("appendix_b_centroid_percolation.result.json")
            .is_file()
    );

    let mut moved = valid_fixture();
    moved.receipts["receipts"][0]["release_pins"]["mappluto.release"] = json!("26v9");
    write_fixture_receipts(&moved);
    let moved_out = tempdir().expect("moved out");
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--receipts".to_string(),
        moved.receipts_path.display().to_string(),
        "--out".to_string(),
        moved_out.path().display().to_string(),
    ]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("snapshot moved"), "{stderr}");
    assert!(
        stderr.contains("appendix_b_centroid_percolation"),
        "{stderr}"
    );

    let mut diverged = valid_fixture();
    let mut rows = manifest_value()["measurements"][3]["expected_result_rows"].clone();
    rows[0]["global_one"] = json!(2352);
    rewrite_artifact_rows(&mut diverged, 3, rows);
    let diverged_out = tempdir().expect("diverged out");
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--receipts".to_string(),
        diverged.receipts_path.display().to_string(),
        "--out".to_string(),
        diverged_out.path().display().to_string(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("measurement diverged"), "{stderr}");
    assert!(stderr.contains("appendix_d_candidate_reach"), "{stderr}");

    let mut collapsed = valid_fixture();
    let scratch = tempdir().expect("scratch dir");
    let manifest_path = scratch.path().join("manifest.json");
    let mut manifest = manifest_value();
    let index = 4;
    manifest["measurements"][index]["declared_grain"] = json!("loan_id");
    manifest["measurements"][index]["expected_row_count"] = json!(3);
    manifest["measurements"][index]["denominator_fields"] = json!(["selected_center_count"]);
    manifest["measurements"][index]["expected_denominators"] = json!({"selected_center_count": 3});
    manifest["measurements"][index]["expected_sanity"] =
        json!({"artifact_row_count_matches_expected": true});
    manifest["measurements"][index]["result_fields"] = json!(["loan_id", "literal_count"]);
    manifest["measurements"][index]["expected_result_rows"] = json!([
        {"loan_id": "L1", "literal_count": 10},
        {"loan_id": "L2", "literal_count": 10},
        {"loan_id": "L3", "literal_count": 10}
    ]);
    write_json(&manifest_path, &manifest);
    rewrite_artifact_rows(
        &mut collapsed,
        index,
        json!([{"loan_id": "L1", "literal_count": 30}]),
    );
    collapsed.receipts["receipts"][index]["denominators"] = json!({"selected_center_count": 1});
    collapsed.receipts["receipts"][index]["sanity"] =
        json!({"artifact_row_count_matches_expected": false});
    write_fixture_receipts(&collapsed);
    let output = run_measurement_script_with_paths(vec![
        "--geography".to_string(),
        "nyc".to_string(),
        "--tier".to_string(),
        "nyc_full".to_string(),
        "--manifest".to_string(),
        manifest_path.display().to_string(),
        "--receipts".to_string(),
        collapsed.receipts_path.display().to_string(),
        "--out".to_string(),
        scratch.path().join("collapsed-out").display().to_string(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("grain loan_id expected 3 rows, actual 1"),
        "{stderr}"
    );
    assert!(stderr.contains("missing ids L2, L3"), "{stderr}");
}

#[test]
fn t60_receipt_attestation_requires_query_ids_for_live_complete() {
    let mut live = valid_fixture();
    for receipt in live.receipts["receipts"].as_array_mut().expect("receipts") {
        receipt["proof_class"] = json!("cmdrvl_data_live");
    }
    write_fixture_receipts(&live);
    let (ok, report) = run_report(&live);
    assert!(ok, "{report}");
    assert_eq!(
        report["measurements"][0]["proof_attestation"],
        "LiveComplete"
    );

    let mut observed = valid_fixture();
    let len = observed.receipts["receipts"]
        .as_array()
        .expect("receipts")
        .len();
    for index in 0..len {
        observed.receipts["receipts"][index]["proof_class"] = json!("observed");
        rewrite_artifact_query_id(&mut observed, index, Value::Null);
    }
    write_fixture_receipts(&observed);
    let (ok, report) = run_report(&observed);
    assert!(ok, "{report}");
    assert_eq!(report["measurements"][0]["proof_attestation"], "observed");
    assert!(report["measurements"][0]["query_id"].is_null());

    let mut relabeled = observed;
    for receipt in relabeled.receipts["receipts"]
        .as_array_mut()
        .expect("receipts")
    {
        receipt["proof_class"] = json!("cmdrvl_data_live");
    }
    write_fixture_receipts(&relabeled);
    let (ok, report) = run_report(&relabeled);
    assert!(!ok, "{report}");
    assert_eq!(
        status_for(&report, "appendix_b_centroid_percolation"),
        "malformed"
    );
    let details = details_for(&report, "appendix_b_centroid_percolation");
    assert!(
        details.iter().any(|detail| detail.as_str().is_some_and(
            |value| value.contains("query_id missing for proof_class cmdrvl_data_live")
        )),
        "{details:?}"
    );
}

#[test]
fn malformed_manifest_contract_is_rejected() {
    let cases: [ManifestMutationCase; 16] = [
        ("manifest_measurement_order", |manifest: &mut Value| {
            manifest["measurements"]
                .as_array_mut()
                .expect("measurements")
                .swap(0, 1);
        }),
        ("missing_core_prefix_id", |manifest: &mut Value| {
            manifest["required_measurement_ids"]
                .as_array_mut()
                .expect("required ids")
                .remove(0);
            manifest["measurements"]
                .as_array_mut()
                .expect("measurements")
                .remove(0);
        }),
        ("mutated_core_prefix_id", |manifest: &mut Value| {
            manifest["required_measurement_ids"][0] = json!("appendix_b_centroid_percolation_v9");
            manifest["measurements"][0]["id"] = json!("appendix_b_centroid_percolation_v9");
        }),
        ("duplicate_manifest_id", |manifest: &mut Value| {
            manifest["measurements"][1]["id"] = manifest["measurements"][0]["id"].clone();
        }),
        ("missing_manifest_id", |manifest: &mut Value| {
            manifest["measurements"]
                .as_array_mut()
                .expect("measurements")
                .pop();
        }),
        ("absolute_sql_path", |manifest: &mut Value| {
            manifest["measurements"][0]["sql_path"] = json!("/tmp/query.sql");
        }),
        ("traversal_sql_path", |manifest: &mut Value| {
            manifest["measurements"][0]["sql_path"] = json!("../query.sql");
        }),
        ("manifest_sql_drift", |manifest: &mut Value| {
            manifest["measurements"][0]["source_sql_sha256"] =
                json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        }),
        ("invalid_execution_transform", |manifest: &mut Value| {
            manifest["measurements"][0]["execution_transform"] =
                json!("direct_file_bytes_no_transform");
        }),
        ("unknown_manifest_field", |manifest: &mut Value| {
            manifest["surprise"] = json!(true);
        }),
        ("invalid_manifest_calendar", |manifest: &mut Value| {
            manifest["measurements"][0]["as_of"] = json!("2026-02-30");
        }),
        ("extra_expected_denominator", |manifest: &mut Value| {
            manifest["measurements"][0]["expected_denominators"]["extra_count"] = json!(1);
        }),
        ("incomplete_expected_result_row", |manifest: &mut Value| {
            manifest["measurements"][3]["expected_result_rows"][0]
                .as_object_mut()
                .expect("row object")
                .remove("global_one");
        }),
        ("h7_excluded", |manifest: &mut Value| {
            manifest["measurements"][0]["id"] = json!("appendix_h7_forbidden");
            manifest["required_measurement_ids"][0] = json!("appendix_h7_forbidden");
        }),
        ("duplicate_extension_id", |manifest: &mut Value| {
            let extension = manifest["measurements"]
                .as_array()
                .expect("measurements")
                .last()
                .expect("extension")
                .clone();
            manifest["required_measurement_ids"]
                .as_array_mut()
                .expect("required ids")
                .push(json!("e5_franklin_county_thin_tier_readiness_v0"));
            manifest["measurements"]
                .as_array_mut()
                .expect("measurements")
                .push(extension);
        }),
        ("unsorted_extension_ids", |manifest: &mut Value| {
            let mut extension = manifest["measurements"]
                .as_array()
                .expect("measurements")
                .last()
                .expect("extension")
                .clone();
            extension["id"] = json!("e4_unsorted_extension_v0");
            manifest["required_measurement_ids"]
                .as_array_mut()
                .expect("required ids")
                .push(json!("e4_unsorted_extension_v0"));
            manifest["measurements"]
                .as_array_mut()
                .expect("measurements")
                .push(extension);
        }),
    ];

    for (name, mutate) in cases {
        let dir = tempdir().expect("tempdir");
        let manifest_path = dir.path().join("manifest.json");
        let mut manifest = manifest_value();
        mutate(&mut manifest);
        write_json(&manifest_path, &manifest);

        let output = bin()
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("--emit")
            .arg("plan")
            .output()
            .expect("run plan");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            !String::from_utf8_lossy(&output.stderr).is_empty(),
            "{name} should explain the rejection"
        );
    }
}

#[test]
fn receipt_bundle_shape_and_unknown_fields_are_rejected() {
    let bare_array = valid_fixture().receipts["receipts"].clone();
    let output = run_report_raw_value(&bare_array);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("receipt bundle"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut fixture = valid_fixture();
    fixture.receipts["receipts"][0]["unexpected"] = json!(true);
    write_fixture_receipts(&fixture);
    let output = run_report_raw_path(&fixture.receipts_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown field"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
