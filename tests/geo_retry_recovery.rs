#![forbid(unsafe_code)]

use canon::geo::{
    CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_REQUEST_VERSION,
    CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_PLAN_VERSION, CANON_GEO_RETRY_LOOP_VERSION,
    CANON_GEO_RETRY_RECOVERY_VERSION, CANON_GEO_RUN_VERSION, GEO_RUN_JSON_MEDIA_TYPE,
    GeoAcquisitionCounts, GeoAcquisitionDenominator, GeoAcquisitionProofClass,
    GeoAcquisitionReceipt, GeoAcquisitionRequest, GeoAcquisitionResumability,
    GeoAcquisitionTerminalState, GeoBoundedGeography, GeoBoundedSubset, GeoDenominatorSource,
    GeoDigest, GeoDigestAlgorithm, GeoExecutorKind, GeoFieldRole, GeoLocalArtifactDigest,
    GeoOrderDirection, GeoOrderingTerm, GeoPaginationReceipt, GeoPaginationRequest,
    GeoPlanGrainStatus, GeoPointPopulationArtifact, GeoPointPopulationPoint, GeoReleasePin,
    GeoRequestedField, GeoRetryErrorCode, GeoRetryLoopArtifact, GeoRetryPolicy, GeoRetryTerminal,
    GeoRowByteCeilings, GeoRun, GeoRunBlocker, GeoRunBlockerKind, GeoRunGrainState,
    GeoRunObservation, GeoRunOutputRef, GeoRunPhase, GeoRunPlanRef, GeoRunStatus,
    GeoSubsetPredicate, GeoSubsetPredicateKind, canonical_retry_recovery_bytes,
    geo_acquisition_request_id, geo_acquisition_request_semantic_hash,
    geo_run_declared_artifact_id, geo_run_semantic_hash, measure_recovery, record_pass,
    validate_geo_acquisition_receipt, validate_geo_acquisition_request, validate_geo_run,
    validate_retry_recovery_artifact,
};
use canon::project::{
    CANON_PROJECT_RUN_VERSION, ProjectRunHashRef, ProjectRunNextAction as ProjectNextAction,
    ProjectRunNodeOutcome, ProjectRunNodeReceipt, ProjectRunOutputReceipt, ProjectRunReceipt,
    ProjectRunReport,
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::tempdir;

const RESPONSE_BYTES_DIGEST_ID: &str = "provider_response_bytes";
const CANDIDATE_ROWS_ARTIFACT_ID: &str = "geocode_candidate_rows";
const PROVIDER_PROFILE_VERSION: &str = "canon_geo_acquisition_provider_profile.v0";
const CENSUS_PROVIDER_ID: &str = "census_geocoder_current";
const CENSUS_PROVIDER_VERSION: &str = "benchmark=Public_AR_Current;vintage=Current_Current";
const REGEOCODE_TOOL_ID: &str = "scripts/geo_acquisition/regeocode.py";
const REGEOCODE_TOOL_VERSION: &str = "bd-3p8e.v0";

#[test]
fn t46_measures_frozen_40_point_retry_recovery_denominator() {
    let fixture = recovery_fixture();
    let recovery = measure_recovery(
        &fixture.population,
        &fixture.loops,
        &fixture.runs,
        &fixture.receipts,
    )
    .unwrap_or_else(|error| {
        panic!(
            "T46 recovery measurement refused: {error:?}\nloops={} runs={} receipts={}",
            fixture.loops.len(),
            fixture.runs.len(),
            fixture.receipts.len()
        )
    });

    assert_eq!(recovery.version, CANON_GEO_RETRY_RECOVERY_VERSION);
    assert_eq!(recovery.denominator, 40);
    assert_eq!(recovery.recovered, 25);
    assert_eq!(recovery.abstained_at_ceiling, 10);
    assert_eq!(recovery.blocked, 5);
    assert_eq!(
        recovery.recovered + recovery.abstained_at_ceiling + recovery.blocked,
        recovery.denominator
    );
    assert!(!recovery.precision_claim);
    assert_eq!(recovery.per_point.len(), 40);
    assert!(
        recovery
            .per_point
            .iter()
            .take(25)
            .all(|point| point.recovered
                && point.terminal == GeoRetryTerminal::Resolved
                && point.first_recovering_pass == Some(1)
                && point.final_home_cell != point.landed_home_cell)
    );

    validate_retry_recovery_artifact(&recovery).expect("recovery validates");
    let canonical = canonical_retry_recovery_bytes(&recovery).expect("recovery serializes");
    let instance: Value = serde_json::from_slice(&canonical).expect("recovery JSON parses");
    assert_eq!(instance["precision_claim"], false);
}

#[test]
fn measurement_binary_measures_retry_recovery_from_fixture_sidecars() {
    let fixture = recovery_fixture();
    let temp = tempdir().expect("tempdir");
    let loops_dir = temp.path().join("loops");
    let runs_dir = temp.path().join("runs");
    let receipts_dir = temp.path().join("receipts");
    fs::create_dir_all(&loops_dir).expect("loops dir");
    fs::create_dir_all(&runs_dir).expect("runs dir");
    fs::create_dir_all(&receipts_dir).expect("receipts dir");
    let population_path = temp.path().join("population.json");
    write_json(&population_path, &fixture.population);
    for (index, loop_state) in fixture.loops.iter().enumerate() {
        write_json(&loops_dir.join(format!("loop-{index:02}.json")), loop_state);
    }
    for run in fixture.runs.values() {
        write_json(
            &runs_dir.join(format!("{}.json", file_safe_digest(&run.semantic_hash))),
            run,
        );
    }
    for receipt in fixture.receipts.values() {
        write_json(
            &receipts_dir.join(format!(
                "{}.receipt.json",
                file_safe_digest(&receipt.request_semantic_hash)
            )),
            receipt,
        );
        fs::write(
            receipts_dir.join(format!("{}.bytes", receipt.request_semantic_hash)),
            response_bytes(&receipt.request_id),
        )
        .expect("response bytes sidecar");
        fs::write(
            receipts_dir.join(format!("{}.rows.json", receipt.request_semantic_hash)),
            candidate_rows_bytes(&receipt.request_id),
        )
        .expect("candidate rows sidecar");
    }

    let output = assert_cmd::cargo::cargo_bin_cmd!("canon_geo_measurements")
        .arg("measure-retry-recovery")
        .arg("--population")
        .arg(&population_path)
        .arg("--loops")
        .arg(&loops_dir)
        .arg("--runs")
        .arg(&runs_dir)
        .arg("--receipts")
        .arg(&receipts_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recovery: Value = serde_json::from_slice(&output).expect("recovery JSON parses");
    assert_eq!(recovery["version"], CANON_GEO_RETRY_RECOVERY_VERSION);
    assert_eq!(recovery["denominator"], 40);
    assert_eq!(recovery["recovered"], 25);
    assert_eq!(recovery["abstained_at_ceiling"], 10);
    assert_eq!(recovery["blocked"], 5);
    assert_eq!(recovery["precision_claim"], false);
}

#[test]
fn measurement_binary_materializes_live_acquisition_receipt_sidecars() {
    let request = acquisition_request("materialize-live", "fixture.retry.materialize-live.subject");
    let temp = tempdir().expect("tempdir");
    let request_path = temp.path().join("request.json");
    let response_path = temp.path().join("provider-response.json");
    let rows_path = temp.path().join("candidate-rows.json");
    let out_dir = temp.path().join("receipts");
    let response_bytes =
        br#"{"provider":"fixture-geocoder","request_id":"materialize-live"}"#.to_vec();
    let rows = serde_json::json!([
        {
            "point_id": "e1.gross_class.0001",
            "candidate_rank": 1,
            "lon_e7": -739648020,
            "lat_e7": 405760240,
            "accuracy_type": "fixture_rooftop",
            "matched_address": "3111 BRIGHTON 2ND STREET, BROOKLYN, NY",
            "provider_id": "fixture-geocoder",
            "provider_version": "v1"
        }
    ]);
    let rows_bytes = serde_json::to_vec(&rows).expect("rows serialize");
    write_json(&request_path, &request);
    fs::write(&response_path, &response_bytes).expect("write response bytes");
    fs::write(&rows_path, &rows_bytes).expect("write candidate rows");

    let output = assert_cmd::cargo::cargo_bin_cmd!("canon_geo_measurements")
        .arg("materialize-acquisition-receipt")
        .arg("--request")
        .arg(&request_path)
        .arg("--provider-response-bytes")
        .arg(&response_path)
        .arg("--candidate-rows")
        .arg(&rows_path)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--proof-class")
        .arg("live")
        .arg("--executor-kind")
        .arg("http-service")
        .arg("--executor-id")
        .arg("fixture-geocoder")
        .arg("--executor-version")
        .arg("v1")
        .arg("--tool-id")
        .arg("scripts/geo_acquisition/regeocode.py")
        .arg("--tool-version")
        .arg("bd-3p8e.test")
        .arg("--executor-request-id")
        .arg("fixture-request-0001")
        .arg("--executor-query-id")
        .arg("fixture-query-0001")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let receipt: GeoAcquisitionReceipt =
        serde_json::from_slice(&output).expect("receipt JSON parses");
    assert_eq!(receipt.proof_class, GeoAcquisitionProofClass::Live);
    assert_eq!(
        receipt
            .executor
            .as_ref()
            .expect("live receipt has executor")
            .executor_kind,
        GeoExecutorKind::HttpService
    );
    assert_eq!(receipt.counts.rows, 1);
    validate_geo_acquisition_receipt(&request, &receipt).expect("materialized receipt validates");
    let request_hash = geo_acquisition_request_semantic_hash(&request).expect("request hash");
    assert_eq!(
        fs::read(out_dir.join(format!("{request_hash}.bytes"))).expect("response sidecar"),
        response_bytes
    );
    assert_eq!(
        fs::read(out_dir.join(format!("{request_hash}.rows.json"))).expect("rows sidecar"),
        rows_bytes
    );
    assert!(
        out_dir
            .join(format!("{}.receipt.json", request_hash.replace(':', "_")))
            .exists(),
        "receipt JSON should be written next to sidecars"
    );
    let response_digest = receipt
        .result_digests
        .iter()
        .find(|digest| digest.digest_id == RESPONSE_BYTES_DIGEST_ID)
        .expect("response digest present");
    assert_eq!(
        response_digest.hex_digest,
        blake3::hash(&response_bytes).to_hex().to_string()
    );
}

#[test]
fn measurement_binary_refuses_retained_acquisition_receipt_without_retained_id() {
    let request = acquisition_request("retained-missing", "fixture.retry.retained.subject");
    let temp = tempdir().expect("tempdir");
    let request_path = temp.path().join("request.json");
    let response_path = temp.path().join("provider-response.json");
    let rows_path = temp.path().join("candidate-rows.json");
    write_json(&request_path, &request);
    fs::write(&response_path, b"{\"provider\":\"fixture\"}").expect("write response bytes");
    fs::write(&rows_path, b"[]").expect("write candidate rows");

    assert_cmd::cargo::cargo_bin_cmd!("canon_geo_measurements")
        .arg("materialize-acquisition-receipt")
        .arg("--request")
        .arg(&request_path)
        .arg("--provider-response-bytes")
        .arg(&response_path)
        .arg("--candidate-rows")
        .arg(&rows_path)
        .arg("--out-dir")
        .arg(temp.path().join("receipts"))
        .arg("--proof-class")
        .arg("retained")
        .arg("--executor-id")
        .arg("fixture-geocoder")
        .arg("--executor-version")
        .arg("v1")
        .arg("--tool-id")
        .arg("scripts/geo_acquisition/regeocode.py")
        .arg("--tool-version")
        .arg("bd-3p8e.test")
        .arg("--executor-request-id")
        .arg("fixture-request-0002")
        .arg("--executor-query-id")
        .arg("fixture-query-0002")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "retained proof requires --retained-receipt-id",
        ));
}

#[test]
fn regeocode_script_import_mode_materializes_retained_receipt_without_network() {
    let request = acquisition_request("script-import", "fixture.retry.script-import.subject");
    let temp = tempdir().expect("tempdir");
    let request_path = temp.path().join("request.json");
    let response_path = temp.path().join("provider-response.json");
    let rows_path = temp.path().join("candidate-rows.json");
    let out_dir = temp.path().join("receipts");
    write_json(&request_path, &request);
    let response_bytes = br#"{"provider":"fixture-geocoder","mode":"import"}"#;
    let rows_bytes = br#"[{"candidate_rank":1,"lat_e7":405760240,"lon_e7":-739648020,"provider_id":"census_geocoder_current","provider_version":"benchmark=Public_AR_Current;vintage=Current_Current"}]"#;
    fs::write(&response_path, response_bytes).expect("write response bytes");
    fs::write(&rows_path, rows_bytes).expect("write candidate rows");

    let output = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/geo_acquisition/regeocode.py"))
        .arg("--request")
        .arg(&request_path)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--provider-response-bytes")
        .arg(&response_path)
        .arg("--candidate-rows")
        .arg(&rows_path)
        .arg("--provider-profile")
        .arg(provider_profile_path())
        .arg("--measurement-bin")
        .arg(env!("CARGO_BIN_EXE_canon_geo_measurements"))
        .output()
        .expect("run regeocode script");
    assert!(
        output.status.success(),
        "script failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: GeoAcquisitionReceipt =
        serde_json::from_slice(&output.stdout).expect("receipt JSON parses");
    assert_eq!(receipt.proof_class, GeoAcquisitionProofClass::Retained);
    let executor = receipt
        .executor
        .as_ref()
        .expect("script receipt has executor");
    assert_eq!(executor.executor_kind, GeoExecutorKind::LocalFile);
    assert_eq!(executor.executor_id, CENSUS_PROVIDER_ID);
    assert_eq!(executor.executor_version, CENSUS_PROVIDER_VERSION);
    assert_eq!(executor.tool_id, REGEOCODE_TOOL_ID);
    assert_eq!(executor.tool_version, REGEOCODE_TOOL_VERSION);
    let expected_retained_id = format!("retained-provider-response:{}", sha2_hex(response_bytes));
    assert_eq!(
        receipt.retained_receipt_id.as_deref(),
        Some(expected_retained_id.as_str())
    );
    validate_geo_acquisition_receipt(&request, &receipt).expect("script receipt validates");
    let request_hash = geo_acquisition_request_semantic_hash(&request).expect("request hash");
    assert_eq!(
        fs::read(out_dir.join(format!("{request_hash}.bytes"))).expect("response sidecar"),
        response_bytes
    );
    assert_eq!(
        fs::read(out_dir.join(format!("{request_hash}.rows.json"))).expect("rows sidecar"),
        rows_bytes
    );
}

#[test]
fn regeocode_provider_profile_declares_external_acquisition_boundary() {
    let profile: Value = serde_json::from_str(include_str!(
        "../scripts/geo_acquisition/providers/census_geocoder_current.json"
    ))
    .expect("provider profile parses");

    assert_eq!(profile["version"], PROVIDER_PROFILE_VERSION);
    assert_eq!(profile["provider_id"], CENSUS_PROVIDER_ID);
    assert_eq!(profile["provider_version"], CENSUS_PROVIDER_VERSION);
    assert_eq!(
        profile["endpoint"],
        "https://geocoding.geo.census.gov/geocoder/geographies/onelineaddress"
    );
    assert_eq!(profile["network_class"], "external_acquisition_only");
    assert_eq!(profile["tool_id"], REGEOCODE_TOOL_ID);
    assert_eq!(profile["tool_version"], REGEOCODE_TOOL_VERSION);
    assert!(
        profile["proof_boundary"]
            .as_str()
            .expect("proof boundary is string")
            .contains("retained import cannot claim live proof"),
        "profile must keep retained import outside live proof: {profile:?}"
    );
}

#[test]
fn regeocode_script_refuses_provider_profile_version_drift() {
    let request = acquisition_request("script-bad-profile", "fixture.retry.bad-profile.subject");
    let temp = tempdir().expect("tempdir");
    let request_path = temp.path().join("request.json");
    let response_path = temp.path().join("provider-response.json");
    let rows_path = temp.path().join("candidate-rows.json");
    let profile_path = temp.path().join("bad-provider-profile.json");
    write_json(&request_path, &request);
    fs::write(&response_path, b"{\"provider\":\"fixture\"}").expect("write response bytes");
    fs::write(&rows_path, b"[]").expect("write candidate rows");
    fs::write(
        &profile_path,
        br#"{"version":"wrong","provider_id":"fixture","provider_version":"v1","endpoint":"https://example.invalid","benchmark":"b","vintage":"v","source_attribution":"fixture","tool_id":"scripts/geo_acquisition/regeocode.py","tool_version":"fixture","network_class":"external_acquisition_only","proof_boundary":"retained import cannot claim live proof"}"#,
    )
    .expect("write bad provider profile");

    let output = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/geo_acquisition/regeocode.py"))
        .arg("--request")
        .arg(&request_path)
        .arg("--out-dir")
        .arg(temp.path().join("receipts"))
        .arg("--provider-response-bytes")
        .arg(&response_path)
        .arg("--candidate-rows")
        .arg(&rows_path)
        .arg("--provider-profile")
        .arg(&profile_path)
        .arg("--measurement-bin")
        .arg(env!("CARGO_BIN_EXE_canon_geo_measurements"))
        .output()
        .expect("run regeocode script");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(PROVIDER_PROFILE_VERSION),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn measurement_binary_prepares_retry_recovery_requests_from_bound_address_rows() {
    let fixture = recovery_fixture();
    let mut population = fixture.population.clone();
    let address_rows = rewrite_population_address_hashes(&mut population);
    let temp = tempdir().expect("tempdir");
    let population_path = temp.path().join("population.json");
    let address_rows_path = temp.path().join("address-rows.json");
    let out_dir = temp.path().join("prepared");
    write_json(&population_path, &population);
    write_json(&address_rows_path, &address_rows);

    let output = assert_cmd::cargo::cargo_bin_cmd!("canon_geo_measurements")
        .arg("prepare-retry-recovery")
        .arg("--population")
        .arg(&population_path)
        .arg("--address-rows")
        .arg(&address_rows_path)
        .arg("--provider-profile")
        .arg(provider_profile_path())
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("preparation report parses");
    assert_eq!(report["population_id"], population.population_id);
    assert_eq!(report["denominator"], 40);
    assert_eq!(report["prepared"], 40);
    assert_eq!(report["provider_id"], CENSUS_PROVIDER_ID);
    assert_eq!(report["provider_version"], CENSUS_PROVIDER_VERSION);
    let request_hashes = report["request_semantic_hashes"]
        .as_object()
        .expect("request hash map");
    assert_eq!(request_hashes.len(), 40);
    for point in &population.points {
        assert!(
            request_hashes
                .get(&point.point_id)
                .and_then(Value::as_str)
                .is_some_and(|hash| hash.starts_with("blake3:")),
            "prepared report must name the request hash for {}",
            point.point_id
        );
    }

    let first_point = &population.points[0];
    let request_path = out_dir
        .join("requests")
        .join(format!("{}.request.json", first_point.point_id));
    let loop_path = out_dir
        .join("loops")
        .join(format!("{}.loop.json", first_point.point_id));
    let address_path = out_dir
        .join("addresses")
        .join(format!("{}.address.txt", first_point.point_id));
    let request: GeoAcquisitionRequest =
        serde_json::from_slice(&fs::read(&request_path).expect("request file"))
            .expect("request parses");
    validate_geo_acquisition_request(&request).expect("prepared request validates");
    assert_eq!(request.version, CANON_GEO_ACQUISITION_REQUEST_VERSION);
    assert_eq!(request.releases[0].source_instance_id, CENSUS_PROVIDER_ID);
    assert_eq!(request.releases[0].release_id, CENSUS_PROVIDER_VERSION);
    assert_eq!(
        request.releases[0].release_digest.digest_id,
        "provider_profile"
    );
    assert_eq!(request.ceilings.max_rows, 10);
    assert_eq!(request.ceilings.max_bytes, 131_072);
    assert!(
        request
            .subset
            .predicates
            .iter()
            .any(
                |predicate| predicate.predicate_id == "asserted_address_blake3"
                    && predicate
                        .expression
                        .contains(first_point.asserted_address_blake3.as_str())
            ),
        "prepared request must bind the point's asserted address hash: {request:#?}"
    );
    let loop_state: GeoRetryLoopArtifact =
        serde_json::from_slice(&fs::read(&loop_path).expect("loop file")).expect("loop parses");
    assert_eq!(loop_state.subject_id, first_point.subject_id);
    assert_eq!(loop_state.policy.max_passes, 2);
    assert_eq!(loop_state.passes.len(), 0);
    assert_eq!(loop_state.terminal, None);
    assert_eq!(loop_state.policy.regeocode_request_template, request);
    validate_geo_acquisition_request(&loop_state.policy.regeocode_request_template)
        .expect("loop request validates");
    assert_eq!(
        fs::read_to_string(address_path).expect("address file"),
        format!("{}, New York, NY, 10000\n", first_point.point_id)
    );
}

#[test]
fn measurement_binary_refuses_retry_recovery_address_hash_mismatch() {
    let fixture = recovery_fixture();
    let mut population = fixture.population.clone();
    let mut address_rows = rewrite_population_address_hashes(&mut population);
    address_rows[0]["asserted_address"] = Value::String("wrong address".to_string());
    let temp = tempdir().expect("tempdir");
    let population_path = temp.path().join("population.json");
    let address_rows_path = temp.path().join("address-rows.json");
    write_json(&population_path, &population);
    write_json(&address_rows_path, &address_rows);

    assert_cmd::cargo::cargo_bin_cmd!("canon_geo_measurements")
        .arg("prepare-retry-recovery")
        .arg("--population")
        .arg(&population_path)
        .arg("--address-rows")
        .arg(&address_rows_path)
        .arg("--provider-profile")
        .arg(provider_profile_path())
        .arg("--out-dir")
        .arg(temp.path().join("prepared"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("asserted_address_blake3"));
}

#[test]
fn regeocode_script_import_mode_refuses_live_proof_label() {
    let request = acquisition_request("script-live-refusal", "fixture.retry.script-live.subject");
    let temp = tempdir().expect("tempdir");
    let request_path = temp.path().join("request.json");
    let response_path = temp.path().join("provider-response.json");
    let rows_path = temp.path().join("candidate-rows.json");
    write_json(&request_path, &request);
    fs::write(&response_path, b"{\"provider\":\"fixture\"}").expect("write response bytes");
    fs::write(&rows_path, b"[]").expect("write candidate rows");

    let output = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/geo_acquisition/regeocode.py"))
        .arg("--request")
        .arg(&request_path)
        .arg("--out-dir")
        .arg(temp.path().join("receipts"))
        .arg("--provider-response-bytes")
        .arg(&response_path)
        .arg("--candidate-rows")
        .arg(&rows_path)
        .arg("--proof-class")
        .arg("live")
        .arg("--measurement-bin")
        .arg(env!("CARGO_BIN_EXE_canon_geo_measurements"))
        .output()
        .expect("run regeocode script");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("retained response import cannot claim live proof"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn provider_profile_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/geo_acquisition/providers/census_geocoder_current.json")
}

fn rewrite_population_address_hashes(population: &mut GeoPointPopulationArtifact) -> Vec<Value> {
    population
        .points
        .iter_mut()
        .map(|point| {
            let asserted_address = point.point_id.to_ascii_uppercase();
            point.asserted_address_blake3 = blake3::hash(asserted_address.as_bytes())
                .to_hex()
                .to_string();
            serde_json::json!({
                "point_id": point.point_id.clone(),
                "subject_id": point.subject_id.clone(),
                "asserted_address": asserted_address,
                "one_line_address": format!("{}, New York, NY, 10000", point.point_id),
            })
        })
        .collect()
}

#[test]
fn t46_subject_outside_population_refuses_denominator_mismatch() {
    let mut fixture = recovery_fixture();
    let mut outside = fixture.loops[0].clone();
    outside.subject_id = "fixture.retry.outside-population".to_string();
    fixture.loops.push(outside);

    let error = measure_recovery(
        &fixture.population,
        &fixture.loops,
        &fixture.runs,
        &fixture.receipts,
    )
    .expect_err("outside subject must refuse the frozen denominator");

    assert_eq!(
        error.code,
        GeoRetryErrorCode::RetryRecoveryDenominatorMismatch
    );
    assert_eq!(
        error.detail.get("subject_id").map(String::as_str),
        Some("fixture.retry.outside-population"),
        "denominator refusal must name the offending subject: {error:?}"
    );
}

#[test]
fn t46_regeocode_pass_without_receipt_digest_refuses() {
    let mut fixture = recovery_fixture();
    fixture.loops[0].passes[0].receipt_blake3 = None;

    let error = measure_recovery(
        &fixture.population,
        &fixture.loops,
        &fixture.runs,
        &fixture.receipts,
    )
    .expect_err("regeocode pass without receipt digest must refuse");

    assert_eq!(error.code, GeoRetryErrorCode::RetryReceiptUnbound);
    assert_eq!(error.detail.get("pass").map(String::as_str), Some("1"));
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("passes[].receipt_blake3"),
        "receipt refusal must name the missing pass digest: {error:?}"
    );
}

#[test]
fn t47_recomputes_request_hash_and_rejects_edited_receipt_hash() {
    let request = acquisition_request("t47", "fixture.retry.t47.subject");
    let receipt = receipt_for_request(&request);
    validate_geo_acquisition_receipt(&request, &receipt).expect("receipt fixture validates");
    assert_eq!(
        receipt.request_semantic_hash,
        geo_acquisition_request_semantic_hash(&request).expect("request semantic hash")
    );

    let mut fixture = recovery_fixture();
    let request_hash = fixture.loops[0].passes[0]
        .regeocode
        .as_ref()
        .and_then(|request| geo_acquisition_request_semantic_hash(request).ok())
        .expect("first pass request hash");
    let receipt = fixture
        .receipts
        .get_mut(&request_hash)
        .expect("fixture receipt is keyed by request hash");
    receipt.request_semantic_hash = digest_label("edited-request-semantic-hash");

    let error = measure_recovery(
        &fixture.population,
        &fixture.loops,
        &fixture.runs,
        &fixture.receipts,
    )
    .expect_err("edited receipt request hash must be caught by recomputation");

    assert_eq!(error.code, GeoRetryErrorCode::RetryReceiptUnbound);
    assert_eq!(
        error.detail.get("field").map(String::as_str),
        Some("receipt.request_semantic_hash")
    );
    assert!(error.detail.contains_key("expected"));
    assert!(error.detail.contains_key("actual"));
}

#[test]
fn t47_stale_provider_response_bytes_digest_refuses() {
    let mut fixture = recovery_fixture();
    let request_hash = fixture.loops[0].passes[0]
        .regeocode
        .as_ref()
        .and_then(|request| geo_acquisition_request_semantic_hash(request).ok())
        .expect("first pass request hash");
    let receipt = fixture
        .receipts
        .get_mut(&request_hash)
        .expect("fixture receipt is keyed by request hash");
    let local_artifact = receipt
        .local_artifacts
        .iter_mut()
        .find(|artifact| artifact.artifact_id == RESPONSE_BYTES_DIGEST_ID)
        .expect("provider response bytes artifact");
    local_artifact.digest = digest_struct(RESPONSE_BYTES_DIGEST_ID, "changed-response-bytes");
    fixture.loops[0].passes[0].receipt_blake3 = Some(receipt_content_blake3(receipt));

    let error = measure_recovery(
        &fixture.population,
        &fixture.loops,
        &fixture.runs,
        &fixture.receipts,
    )
    .expect_err("stale provider response bytes digest must refuse");

    assert_eq!(error.code, GeoRetryErrorCode::RetryReceiptUnbound);
    assert_eq!(
        error.detail.get("digest_id").map(String::as_str),
        Some(RESPONSE_BYTES_DIGEST_ID),
        "stale response-byte refusal must name the digest id: {error:?}"
    );
}

struct RecoveryFixture {
    population: GeoPointPopulationArtifact,
    loops: Vec<GeoRetryLoopArtifact>,
    runs: BTreeMap<String, GeoRun>,
    receipts: BTreeMap<String, GeoAcquisitionReceipt>,
}

fn recovery_fixture() -> RecoveryFixture {
    let population: GeoPointPopulationArtifact =
        serde_json::from_str(include_str!("fixtures/geo/e1_gross_class_points.json"))
            .expect("40-point gross-class fixture parses");
    let mut loops = Vec::new();
    let mut runs = BTreeMap::new();
    let mut receipts = BTreeMap::new();
    let final_home_cells = population
        .points
        .iter()
        .map(|point| point.home_cell_r9.clone())
        .collect::<Vec<_>>();

    for (index, point) in population.points.iter().enumerate() {
        let policy = retry_policy(point, 2);
        let mut loop_state = GeoRetryLoopArtifact {
            version: CANON_GEO_RETRY_LOOP_VERSION.to_string(),
            subject_id: point.subject_id.clone(),
            policy,
            passes: Vec::new(),
            terminal: None,
        };
        let receipt = receipt_for_request(&loop_state.policy.regeocode_request_template);
        let request_hash = receipt.request_semantic_hash.clone();
        receipts.insert(request_hash.clone(), receipt);

        if index < 25 {
            let final_home_cell = different_home_cell(index, &final_home_cells);
            let run = completed_run(&format!("resolved-{index:02}"), final_home_cell);
            record_run(&mut runs, run.clone());
            record_pass(&mut loop_state, &run, receipts.get(&request_hash))
                .expect("resolved pass records");
        } else if index < 35 {
            for pass in 1..=2 {
                let run = abstaining_run(&format!("abstained-{index:02}-{pass}"));
                record_run(&mut runs, run.clone());
                record_pass(&mut loop_state, &run, receipts.get(&request_hash))
                    .expect("abstaining pass records");
            }
        } else {
            let run = blocked_run(&format!("blocked-{index:02}"));
            record_run(&mut runs, run.clone());
            record_pass(&mut loop_state, &run, receipts.get(&request_hash))
                .expect("blocked pass records");
        }
        loops.push(loop_state);
    }

    RecoveryFixture {
        population,
        loops,
        runs,
        receipts,
    }
}

fn retry_policy(point: &GeoPointPopulationPoint, max_passes: u8) -> GeoRetryPolicy {
    GeoRetryPolicy {
        max_passes,
        regeocode_request_template: acquisition_request(&point.point_id, &point.subject_id),
    }
}

fn acquisition_request(seed: &str, subject_id: &str) -> GeoAcquisitionRequest {
    let geography = GeoBoundedGeography {
        geography_id: format!("fixture.retry-recovery.{seed}.geography"),
        geography_kind: "bounded_fixture_region".to_string(),
        description: "bounded fixture region for retry-recovery tests".to_string(),
    };
    let subset = GeoBoundedSubset {
        subset_id: format!("fixture.retry-recovery.{seed}.subset"),
        geography: geography.clone(),
        h3_cells: Vec::new(),
        predicates: vec![GeoSubsetPredicate {
            predicate_id: "subject_ids".to_string(),
            kind: GeoSubsetPredicateKind::ExplicitIdentifiers,
            expression: format!("subject_id = '{subject_id}'"),
        }],
    };
    let mut request = GeoAcquisitionRequest {
        version: CANON_GEO_ACQUISITION_REQUEST_VERSION.to_string(),
        request_id: String::new(),
        discovery_request_id: None,
        bounded_geography: geography,
        subset,
        releases: vec![release_pin(seed)],
        fields: vec![GeoRequestedField {
            field_id: "address_text".to_string(),
            role: GeoFieldRole::Identifier,
            required: true,
        }],
        projection: None,
        ordering: vec![GeoOrderingTerm {
            position: 1,
            field_id: "address_text".to_string(),
            direction: GeoOrderDirection::Asc,
            nulls: canon::geo::GeoNullOrdering::Last,
        }],
        pagination: GeoPaginationRequest {
            page_size_rows: 10,
            page_token: None,
        },
        ceilings: GeoRowByteCeilings {
            max_rows: 10,
            max_bytes: 4096,
        },
        positive_path_min_rows: 1,
    };
    request.request_id = geo_acquisition_request_id(&request).expect("request id");
    validate_geo_acquisition_request(&request).expect("valid acquisition request");
    request
}

fn release_pin(seed: &str) -> GeoReleasePin {
    GeoReleasePin {
        source_instance_id: format!("fixture.retry-recovery.{seed}.source"),
        release_id: format!("fixture.retry-recovery.{seed}.release"),
        release_digest: digest_struct("release", &format!("release:{seed}")),
    }
}

fn receipt_for_request(request: &GeoAcquisitionRequest) -> GeoAcquisitionReceipt {
    let response_digest = digest_bytes(
        RESPONSE_BYTES_DIGEST_ID,
        &response_bytes(&request.request_id),
    );
    let candidate_rows_digest = digest_struct(
        CANDIDATE_ROWS_ARTIFACT_ID,
        &format!("candidate-rows:{}", request.request_id),
    );
    let receipt = GeoAcquisitionReceipt {
        version: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        request_id: request.request_id.clone(),
        request_semantic_hash: geo_acquisition_request_semantic_hash(request)
            .expect("request semantic hash"),
        terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Fixture,
        executor: None,
        fixture_id: Some("fixture.retry-recovery.receipt".to_string()),
        retained_receipt_id: None,
        bounded_geography: request.bounded_geography.clone(),
        subset: request.subset.clone(),
        releases: request.releases.clone(),
        fields: request.fields.clone(),
        projection: request.projection.clone(),
        normalized_executed_request_digest: digest_struct(
            "normalized_executed_request",
            &format!("normalized:{}", request.request_id),
        ),
        pagination: GeoPaginationReceipt {
            requested_page: request.pagination.clone(),
            next_page_token: None,
            rows_truncated: false,
            bytes_truncated: false,
        },
        counts: GeoAcquisitionCounts {
            rows: 1,
            bytes: 128,
        },
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "requested-subset".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: 1,
            unit: "row".to_string(),
            description: "one fixture subject requested".to_string(),
        }],
        source_digests: vec![digest_struct(
            "source",
            &format!("source:{}", request.request_id),
        )],
        result_digests: vec![response_digest.clone(), candidate_rows_digest.clone()],
        local_artifacts: vec![
            GeoLocalArtifactDigest {
                artifact_id: RESPONSE_BYTES_DIGEST_ID.to_string(),
                media_type: "application/octet-stream".to_string(),
                byte_count: 64,
                digest: response_digest,
            },
            GeoLocalArtifactDigest {
                artifact_id: CANDIDATE_ROWS_ARTIFACT_ID.to_string(),
                media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
                byte_count: 64,
                digest: candidate_rows_digest,
            },
        ],
        artifact_release_relations: Vec::new(),
        unreadable_columns: Vec::new(),
        resumability: GeoAcquisitionResumability {
            resumable: false,
            resume_token: None,
            resume_request_id: None,
            retry_guidance: "terminal fixture receipt requires no resume action".to_string(),
        },
        terminal_detail: None,
    };
    validate_geo_acquisition_receipt(request, &receipt).expect("valid acquisition receipt");
    receipt
}

fn completed_run(seed: &str, final_home_cell: &str) -> GeoRun {
    let mut run = base_run(seed, GeoRunStatus::Completed);
    run.phase = GeoRunPhase::Solved;
    run.output_refs = vec![GeoRunOutputRef {
        artifact_id: geo_run_declared_artifact_id("geo.building.home_cells", final_home_cell),
        project_node_id: "geo.building.home_cells".to_string(),
        output_id: final_home_cell.to_string(),
        content_digest: digest_label(&format!("{seed}:home-cell-assignment")),
        byte_count: 96,
        media_type: GEO_RUN_JSON_MEDIA_TYPE.to_string(),
        contract_version: CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION.to_string(),
        resolved_claim: None,
    }];
    run.grain_states = vec![GeoRunGrainState {
        entity_level: "building".to_string(),
        status: GeoPlanGrainStatus::PlannedRelativeToDeclaredUniverse,
        missing_evidence_classes: Vec::new(),
        project_node_ids: vec!["geo.building.home_cells".to_string()],
        claim_limitation: "retry recovery is reach-only; precision remains unclaimed".to_string(),
        next_action: "score recovery from the pinned run and acquisition receipt".to_string(),
    }];
    run.project_run_report = Some(project_run_report(seed, ProjectRunNodeOutcome::Completed));
    stamp_run_identity(&mut run);
    validate_geo_run(&run).expect("valid completed run");
    run
}

fn abstaining_run(seed: &str) -> GeoRun {
    let mut run = base_run(seed, GeoRunStatus::Abstained);
    run.phase = GeoRunPhase::Solved;
    run.blockers = vec![GeoRunBlocker {
        blocker_id: "geocode_ambiguous".to_string(),
        kind: GeoRunBlockerKind::WaitingForInput,
        project_node_id: Some("geo.building.solve".to_string()),
        entity_level: Some("building".to_string()),
        reason: "bounded retry fixture run abstained before a fresh acquisition pass".to_string(),
    }];
    stamp_run_identity(&mut run);
    validate_geo_run(&run).expect("valid abstaining run");
    run
}

fn blocked_run(seed: &str) -> GeoRun {
    let mut run = base_run(seed, GeoRunStatus::Failed);
    run.phase = GeoRunPhase::Solved;
    run.blockers = vec![GeoRunBlocker {
        blocker_id: "retry_receipt_blocked".to_string(),
        kind: GeoRunBlockerKind::ProjectFailure,
        project_node_id: Some("geo.building.solve".to_string()),
        entity_level: Some("building".to_string()),
        reason: "bounded retry fixture run remained blocked after acquisition".to_string(),
    }];
    run.project_run_report = Some(project_run_report(seed, ProjectRunNodeOutcome::Failed));
    stamp_run_identity(&mut run);
    validate_geo_run(&run).expect("valid blocked run");
    run
}

fn base_run(seed: &str, status: GeoRunStatus) -> GeoRun {
    let plan_hash = digest_label(&format!("{seed}:plan"));
    GeoRun {
        version: CANON_GEO_RUN_VERSION.to_string(),
        run_id: String::new(),
        semantic_hash: String::new(),
        status,
        phase: GeoRunPhase::Preflighted,
        plan_ref: GeoRunPlanRef {
            plan_id: format!(
                "{CANON_GEO_PLAN_VERSION}:{}",
                plan_hash.trim_start_matches("blake3:")
            ),
            semantic_hash: plan_hash,
            project_id: format!("geo.retry-recovery.{seed}.project"),
            project_graph_hash: digest_label(&format!("{seed}:project-graph")),
            question_hash: digest_label(&format!("{seed}:question")),
            capabilities_hash: digest_label(&format!("{seed}:capabilities")),
            inventory_planning_hash: digest_label(&format!("{seed}:inventory")),
            profile_hash: digest_label(&format!("{seed}:profile")),
            budget_planning_hash: digest_label(&format!("{seed}:budget")),
        },
        artifact_inputs: Vec::new(),
        acquisition_satisfactions: Vec::new(),
        output_refs: Vec::new(),
        grain_states: vec![GeoRunGrainState {
            entity_level: "building".to_string(),
            status: GeoPlanGrainStatus::WaitingForAcquisition,
            missing_evidence_classes: vec!["address_point".to_string()],
            project_node_ids: Vec::new(),
            claim_limitation: "local acquisition is required before deterministic execution"
                .to_string(),
            next_action: "record the emitted acquisition request and rerun".to_string(),
        }],
        blockers: Vec::new(),
        next_actions: Vec::new(),
        deterministic_usage: BTreeMap::new(),
        project_run_report: None,
        observation: GeoRunObservation::default(),
    }
}

fn project_run_report(seed: &str, outcome: ProjectRunNodeOutcome) -> ProjectRunReport {
    let node_receipt = ProjectRunNodeReceipt {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: format!("geo.retry-recovery.{seed}.project"),
        plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
        node_id: "geo.building.solve".to_string(),
        node_cache_key: digest_label(&format!("{seed}:node-cache")),
        content_hash_inputs: vec![ProjectRunHashRef {
            ref_id: "geo.run.input.geo.building.home_cells.rows".to_string(),
            content_hash: digest_label(&format!("{seed}:input")),
        }],
        dependency_semantic_hashes: BTreeMap::new(),
        dependency_receipt_hashes: BTreeMap::new(),
        outputs: vec![ProjectRunOutputReceipt {
            output_id: "solve".to_string(),
            path: "geo/building/solve.json".to_string(),
            content_digest: digest_label(&format!("{seed}:solve")),
            byte_count: 42,
        }],
        outcome,
        deterministic_usage: BTreeMap::new(),
        duration_millis: 0,
        resource_observations: BTreeMap::new(),
        next_action: if outcome == ProjectRunNodeOutcome::Completed {
            ProjectNextAction::ExecuteDependents
        } else {
            ProjectNextAction::InspectFailure
        },
        failure_code: (outcome != ProjectRunNodeOutcome::Completed)
            .then(|| "retry_blocked".to_string()),
        failure_message: (outcome != ProjectRunNodeOutcome::Completed)
            .then(|| "retry recovery fixture remained blocked".to_string()),
        semantic_hash: digest_label(&format!("{seed}:node-semantic")),
        telemetry_hash: digest_label(&format!("{seed}:node-telemetry")),
        receipt_hash: digest_label(&format!("{seed}:node-receipt")),
    };
    ProjectRunReport {
        schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
        project_id: format!("geo.retry-recovery.{seed}.project"),
        plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
        run_receipt_hash: digest_label(&format!("{seed}:run-receipt")),
        max_parallelism: 1,
        max_ready_width: 1,
        executed_nodes: (outcome == ProjectRunNodeOutcome::Completed)
            .then(|| "geo.building.solve".to_string())
            .into_iter()
            .collect(),
        resumed_nodes: Vec::new(),
        failed_nodes: (outcome == ProjectRunNodeOutcome::Failed)
            .then(|| "geo.building.solve".to_string())
            .into_iter()
            .collect(),
        cancelled_nodes: Vec::new(),
        invalidated_nodes: Vec::new(),
        blocked_nodes: Vec::new(),
        next_actions: BTreeMap::new(),
        receipt: ProjectRunReceipt {
            schema_version: CANON_PROJECT_RUN_VERSION.to_string(),
            project_id: format!("geo.retry-recovery.{seed}.project"),
            plan_graph_hash: digest_label(&format!("{seed}:project-graph")),
            receipt_hash: digest_label(&format!("{seed}:run-receipt")),
            completed_nodes: (outcome == ProjectRunNodeOutcome::Completed)
                .then(|| "geo.building.solve".to_string())
                .into_iter()
                .collect(),
            failed_nodes: (outcome == ProjectRunNodeOutcome::Failed)
                .then(|| "geo.building.solve".to_string())
                .into_iter()
                .collect(),
            cancelled_nodes: Vec::new(),
            invalidated_nodes: Vec::new(),
            blocked_nodes: Vec::new(),
            node_receipts: vec![node_receipt],
        },
        node_reports: Vec::new(),
        invalidation_reasons: Vec::new(),
        resource_reuse: Default::default(),
    }
}

fn different_home_cell(index: usize, home_cells: &[String]) -> &str {
    let landed = &home_cells[index];
    home_cells
        .iter()
        .enumerate()
        .find(|(candidate_index, cell)| *candidate_index != index && *cell != landed)
        .map(|(_, cell)| cell.as_str())
        .expect("fixture has at least two distinct home cells")
}

fn record_run(runs: &mut BTreeMap<String, GeoRun>, run: GeoRun) {
    assert!(
        runs.insert(run.semantic_hash.clone(), run).is_none(),
        "test fixture run hashes must be unique"
    );
}

fn write_json(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).expect("JSON serializes")).expect("write JSON");
}

fn file_safe_digest(value: &str) -> String {
    value.replace(':', "_")
}

fn response_bytes(request_id: &str) -> Vec<u8> {
    format!("response:{request_id}").into_bytes()
}

fn candidate_rows_bytes(request_id: &str) -> Vec<u8> {
    format!("candidate-rows:{request_id}").into_bytes()
}

fn stamp_run_identity(run: &mut GeoRun) {
    run.semantic_hash.clear();
    run.run_id.clear();
    run.semantic_hash = geo_run_semantic_hash(run).expect("run semantic hash");
    run.run_id = format!(
        "{CANON_GEO_RUN_VERSION}:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
}

fn digest_struct(id: &str, seed: &str) -> GeoDigest {
    digest_bytes(id, seed.as_bytes())
}

fn digest_bytes(id: &str, bytes: &[u8]) -> GeoDigest {
    GeoDigest {
        digest_id: id.to_string(),
        algorithm: GeoDigestAlgorithm::Blake3,
        hex_digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

fn digest_label(seed: &str) -> String {
    format!("blake3:{}", blake3::hash(seed.as_bytes()).to_hex())
}

fn sha2_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn receipt_content_blake3(receipt: &GeoAcquisitionReceipt) -> String {
    format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(receipt).expect("receipt serializes")).to_hex()
    )
}
