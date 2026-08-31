#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

const MANIFEST: &str = include_str!("../scripts/geo_measurements/manifest.json");
const E5_ID: &str = "e5_franklin_county_thin_tier_readiness_v0";
const E5_QUERY_ID: &str = "01c6c151-0821-a0dc-006c-c703088daaba";
const FIXTURE_DIR: &str =
    "scripts/geo_measurements/fixtures/e5_franklin_county_thin_tier_readiness";
const RESULT_SET_VERSION: &str = "canon_geo_measurement_result_set.v0";
const LIVENESS_NOT_ATTESTED: &str = "receipt is internally consistent, but this offline runner does not attest liveness, authenticity, or query-history provenance";

#[derive(Serialize)]
struct CanonicalResultSet<'a> {
    version: &'static str,
    measurement_id: &'a str,
    source_sql_sha256: &'a str,
    executed_query_text_sha256: &'a str,
    rows: Vec<BTreeMap<String, Value>>,
}

struct E5Fixture {
    _dir: TempDir,
    manifest_path: PathBuf,
    receipts_path: PathBuf,
    artifact_path: PathBuf,
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon_geo_measurements"))
}

fn manifest_value() -> Value {
    serde_json::from_str(MANIFEST).expect("manifest parses")
}

fn e5_index(manifest: &Value) -> usize {
    manifest["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .position(|measurement| measurement["id"] == E5_ID)
        .expect("e5 measurement")
}

fn copy_e5_fixture() -> E5Fixture {
    let dir = tempdir().expect("tempdir");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = repo.join(FIXTURE_DIR);
    let manifest_path = dir.path().join("manifest.json");
    let receipts_path = dir.path().join("receipts.json");
    let artifact_path = dir.path().join("result_artifact.json");
    fs::write(&manifest_path, MANIFEST).expect("write manifest");
    fs::copy(fixture_dir.join("receipts.json"), &receipts_path).expect("copy receipts");
    fs::copy(fixture_dir.join("result_artifact.json"), &artifact_path).expect("copy artifact");
    fs::copy(
        fixture_dir.join("executed_query_text.sql"),
        dir.path().join("executed_query_text.sql"),
    )
    .expect("copy executed query text");
    E5Fixture {
        _dir: dir,
        manifest_path,
        receipts_path,
        artifact_path,
    }
}

fn run_report(fixture: &E5Fixture) -> (bool, Value) {
    let output = bin()
        .arg("--manifest")
        .arg(&fixture.manifest_path)
        .arg("--receipts")
        .arg(&fixture.receipts_path)
        .arg("--emit")
        .arg("report")
        .output()
        .expect("run report");
    let stdout = serde_json::from_slice(&output.stdout).expect("report json");
    (output.status.success(), stdout)
}

fn run_plan(manifest: &Value) -> std::process::Output {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join("manifest.json");
    write_json(&manifest_path, manifest);
    bin()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--emit")
        .arg("plan")
        .output()
        .expect("run plan")
}

fn status_for<'a>(report: &'a Value, measurement_id: &str) -> &'a Value {
    report["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .find(|row| row["measurement_id"] == measurement_id)
        .expect("measurement row")
}

fn mutate_receipt(fixture: &E5Fixture, mutate: impl FnOnce(&mut Value)) {
    let mut receipts = read_json(&fixture.receipts_path);
    mutate(&mut receipts["receipts"][0]);
    write_json(&fixture.receipts_path, &receipts);
}

fn mutate_artifact(
    fixture: &E5Fixture,
    mutate: impl FnOnce(&mut Vec<Value>),
    refresh_counts: bool,
) {
    let mut artifact = read_json(&fixture.artifact_path);
    mutate(artifact["rows"].as_array_mut().expect("rows"));
    write_json(&fixture.artifact_path, &artifact);

    let mut receipts = read_json(&fixture.receipts_path);
    let receipt = &mut receipts["receipts"][0];
    let artifact_bytes = fs::read(&fixture.artifact_path).expect("artifact bytes");
    receipt["result_artifact_sha256"] = json!(sha256_hex(&artifact_bytes));
    receipt["result_set_sha256"] = json!(result_set_sha256(
        artifact["measurement_id"].as_str().expect("measurement id"),
        artifact["source_sql_sha256"].as_str().expect("source sha"),
        artifact["executed_query_text_sha256"]
            .as_str()
            .expect("executed sha"),
        artifact["rows"].as_array().expect("rows")
    ));
    receipt["row_count"] = json!(artifact["rows"].as_array().expect("rows").len());
    if refresh_counts {
        receipt["denominators"] = e5_denominators(artifact["rows"].as_array().expect("rows"));
        receipt["sanity"] = e5_sanity(artifact["rows"].as_array().expect("rows"));
    }
    write_json(&fixture.receipts_path, &receipts);
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
            .expect("e5 source row");
        for field in ["feature_rows", "distinct_features", "occupied_work_cells"] {
            denominators.insert(
                format!("{evidence_class}.{field}"),
                json!(row[field].as_u64().expect("u64 field")),
            );
        }
    }
    Value::Object(denominators)
}

fn e5_sanity(rows: &[Value]) -> Value {
    json!({
        "artifact_row_count_matches_expected": rows.len() == 4,
        "county_fips": if all_rows_equal(rows, "county_fips", &json!("39049")) { "39049" } else { "FAIL" },
        "guard_status": if all_rows_equal(rows, "guard_status", &json!("ok")) { "ok" } else { "FAIL" },
        "measurement_scope": if all_rows_equal(rows, "measurement_scope", &json!("r8_center_plus_k1_source_availability")) { "r8_center_plus_k1_source_availability" } else { "FAIL" },
        "row_contract": if all_rows_equal(rows, "row_contract", &json!("canon_geo_e5_thin_tier_readiness.v0")) { "canon_geo_e5_thin_tier_readiness.v0" } else { "FAIL" }
    })
}

fn shared_u64(rows: &[Value], field: &str) -> u64 {
    let first = rows[0][field].as_u64().expect("shared u64 field");
    assert!(
        rows.iter()
            .all(|row| row[field].as_u64().expect("shared u64 field") == first)
    );
    first
}

fn all_rows_equal(rows: &[Value], field: &str, expected: &Value) -> bool {
    rows.iter().all(|row| row.get(field) == Some(expected))
}

fn result_set_sha256(
    measurement_id: &str,
    source_sql_sha256: &str,
    executed_query_text_sha256: &str,
    rows: &[Value],
) -> String {
    let view = CanonicalResultSet {
        version: RESULT_SET_VERSION,
        measurement_id,
        source_sql_sha256,
        executed_query_text_sha256,
        rows: canonical_rows(rows),
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

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json")
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("json serializes"),
    )
    .expect("write json");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn e5_fixture_is_receipt_consistent_but_not_live_attested() {
    let fixture = copy_e5_fixture();
    let (ok, report) = run_report(&fixture);
    assert!(!ok, "E5-only fixture should leave core receipts missing");
    assert_eq!(report["summary"]["receipt_consistent"], 1);
    assert_eq!(report["summary"]["missing"], 7);
    assert!(report["summary"].get("verified").is_none());
    let row = status_for(&report, E5_ID);
    assert_eq!(row["status"], "receipt_consistent");
    assert_eq!(row["query_id"], E5_QUERY_ID);
    assert_eq!(row["declared_proof_class"], "contract_fixture");
    assert_eq!(row["row_count"], 4);
    assert!(
        row["details"]
            .as_array()
            .expect("details")
            .iter()
            .any(|detail| detail == LIVENESS_NOT_ATTESTED)
    );
    assert!(!report.to_string().contains("live_attested"));
    assert!(!report.to_string().contains("verified"));
}

#[test]
fn e5_county_and_release_drift_are_snapshot_moved() {
    let fixture = copy_e5_fixture();
    mutate_receipt(&fixture, |receipt| {
        receipt["release_pins"]["county_fips"] = json!("39051");
    });
    let (_, report) = run_report(&fixture);
    assert_eq!(status_for(&report, E5_ID)["status"], "snapshot_moved");

    let fixture = copy_e5_fixture();
    mutate_receipt(&fixture, |receipt| {
        receipt["release_pins"]["fema_structures.release_dt"] = json!("2025-06-06");
    });
    let (_, report) = run_report(&fixture);
    assert_eq!(status_for(&report, E5_ID)["status"], "snapshot_moved");
}

#[test]
fn e5_row_mutation_and_missing_source_are_rejected() {
    let fixture = copy_e5_fixture();
    mutate_artifact(
        &fixture,
        |rows| {
            rows.iter_mut()
                .find(|row| row["evidence_class"] == "fema_structures")
                .expect("fema row")["feature_rows"] = json!(160772);
        },
        true,
    );
    let (_, report) = run_report(&fixture);
    assert_eq!(status_for(&report, E5_ID)["status"], "result_mismatch");

    let fixture = copy_e5_fixture();
    mutate_artifact(
        &fixture,
        |rows| rows.retain(|row| row["evidence_class"] != "overture_buildings"),
        false,
    );
    let (_, report) = run_report(&fixture);
    assert_eq!(status_for(&report, E5_ID)["status"], "malformed");
}

#[test]
fn e5_precision_or_completion_claims_are_rejected_by_manifest_contract() {
    let mut completion_claim = manifest_value();
    let index = e5_index(&completion_claim);
    completion_claim["measurements"][index]["description"] =
        json!("Franklin County E5 complete precision measurement");
    let output = run_plan(&completion_claim);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E5 completion"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut precision_field = manifest_value();
    let index = e5_index(&precision_field);
    precision_field["measurements"][index]["result_fields"]
        .as_array_mut()
        .expect("result fields")
        .push(json!("precision"));
    let output = run_plan(&precision_field);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("result field precision"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
