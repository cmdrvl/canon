use canon::{
    RefusalCode,
    distribution::package::pack_local_package,
    sdk::{
        ArtifactKind, ArtifactReadRequest, ExactBatchLookupRequest, ExactMappingRequest,
        PackageOpenRequest, PackageVerifyRequest, PageRequest, ProjectRunEventsRequest, ReadLimits,
        RegistryMetadataRequest, RowPreservingCsvMappingRequest, SdkApiVersion, exact_batch_lookup,
        exact_mapping_artifact, open_package, read_artifact, read_project_run_events,
        read_registry_metadata, row_preserving_csv_mapping, verify_package,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{NamedTempFile, TempDir};

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

fn registry_path() -> PathBuf {
    fixture_path("registries/cusip-isin")
}

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn run_canon(args: &[&str]) -> Output {
    canon_command()
        .args(args)
        .output()
        .expect("canon binary runs")
}

#[test]
fn cli_and_sdk_json_mapping_are_byte_identical() {
    let input = fixture_path("inputs/partial.csv");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--no-witness",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());

    let response = exact_mapping_artifact(ExactMappingRequest::v1(input, registry, "cusip"))
        .expect("sdk mapping succeeds");
    assert_eq!(response.exit_code, 1);
    assert_eq!(response.artifact_json, output.stdout);

    let cli_json: Value = serde_json::from_slice(&output.stdout).expect("cli json");
    let sdk_json: Value = serde_json::from_slice(&response.artifact_json).expect("sdk json");
    assert_eq!(sdk_json, cli_json);
    assert_eq!(sdk_json["registry"]["id"], "cusip-isin");
}

#[test]
fn cli_and_sdk_csv_mapping_preserve_rows_and_mapping_sidecar() {
    let input = fixture_path("inputs/blank_rows.csv");
    let registry = registry_path();
    let map_out = NamedTempFile::new().expect("map temp");
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "cusip",
        "--emit",
        "csv",
        "--map-out",
        map_out.path().to_str().unwrap(),
        "--no-witness",
    ]);
    assert_eq!(output.status.code(), Some(0));

    let response =
        row_preserving_csv_mapping(RowPreservingCsvMappingRequest::v1(input, registry, "cusip"))
            .expect("sdk csv mapping succeeds");
    assert_eq!(response.exit_code, 0);
    assert_eq!(response.csv_bytes, output.stdout);
    assert_eq!(
        response.mapping_artifact_json,
        fs::read(map_out.path()).expect("cli map sidecar")
    );
    assert_eq!(
        String::from_utf8(response.csv_bytes).expect("csv utf8"),
        "cusip,amount,cusip__canon\n037833100,100,US0378331005\n,,\n594918104,200,US5949181045\n  ,  ,\n17275R102,300,US17275R1023\n"
    );
}

#[test]
fn sdk_refusal_code_matches_cli_for_missing_column() {
    let input = fixture_path("inputs/partial.csv");
    let registry = registry_path();
    let output = run_canon(&[
        input.to_str().unwrap(),
        "--registry",
        registry.to_str().unwrap(),
        "--column",
        "missing",
        "--no-witness",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let cli_json: Value = serde_json::from_slice(&output.stdout).expect("cli refusal");

    let error = exact_mapping_artifact(ExactMappingRequest::v1(input, registry, "missing"))
        .expect_err("sdk refuses missing column");
    assert_eq!(
        serde_json::to_value(&error.code).expect("refusal code serializes"),
        cli_json["refusal"]["code"]
    );
    assert_eq!(error.code, RefusalCode::EColumnNotFound);
    assert_eq!(
        serde_json::from_slice::<Value>(&error.as_envelope_json().expect("envelope json"))
            .expect("sdk envelope"),
        cli_json
    );
}

#[test]
fn sdk_batch_bounds_refuse_with_existing_refusal_code() {
    let request = ExactBatchLookupRequest {
        api_version: SdkApiVersion::v1(),
        registry_path: registry_path(),
        values: vec!["037833100".to_string(), "594918104".to_string()],
        max_values: Some(1),
        explicit: false,
        plain_json_values: false,
    };

    let error = exact_batch_lookup(request).expect_err("batch limit refuses");
    assert_eq!(error.code, RefusalCode::ETooLarge);
    assert_eq!(error.detail["limit_type"], "max_rows");
}

#[test]
fn sdk_batch_lookup_artifact_is_stable_under_input_shuffle() {
    let mut first = ExactBatchLookupRequest::v1(
        registry_path(),
        vec![
            "UNKNOWN99".to_string(),
            "037833100".to_string(),
            "594918104".to_string(),
        ],
    );
    first.explicit = true;
    first.plain_json_values = true;
    let mut second = ExactBatchLookupRequest::v1(
        registry_path(),
        vec![
            "594918104".to_string(),
            "UNKNOWN99".to_string(),
            "037833100".to_string(),
        ],
    );
    second.explicit = true;
    second.plain_json_values = true;

    let left = exact_batch_lookup(first).expect("first batch");
    let right = exact_batch_lookup(second).expect("second batch");
    assert_eq!(left.artifact_json, right.artifact_json);
    assert_eq!(left.exit_code, 1);

    let json: Value = serde_json::from_slice(&left.artifact_json).expect("batch artifact");
    let inputs = json["mappings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mapping| mapping["input"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(inputs, vec!["037833100", "594918104"]);
}

#[test]
fn sdk_package_open_verify_and_json_artifact_reader_share_package_contracts() {
    let package_bytes = canonical_package_bytes(package_json());
    let root = package_root(&package_bytes);
    let archive_bytes = pack_local_package(root.path(), &package_bytes).expect("package packs");
    let archive_path = root.path().join("archive.canonpkg");
    fs::write(&archive_path, &archive_bytes).expect("archive writes");

    let open = open_package(PackageOpenRequest::v1(archive_path.clone())).expect("open package");
    let verify =
        verify_package(PackageVerifyRequest::v1(archive_path.clone())).expect("verify package");
    assert_eq!(open.inspection.package.package_id, "pkg.sdk.demo");
    assert_eq!(
        verify.verification.package_content_digest,
        open.inspection.package.content_digest
    );
    assert_eq!(
        verify.verification.verified_files,
        open.inspection.inventory.len()
    );

    let artifact = read_artifact(ArtifactReadRequest::v1(
        archive_path,
        ArtifactKind::Artifact,
    ))
    .expect("artifact reader parses archive");
    assert_eq!(
        artifact.declared_version.as_deref(),
        Some("canon.local.package.archive.v1")
    );
    assert_eq!(
        artifact.content_digest,
        canon::witness::hash_bytes(&archive_bytes)
    );
}

#[test]
fn sdk_registry_metadata_reader_is_read_only() {
    let registry = registry_path();
    let before = directory_entries(&registry);
    let metadata =
        read_registry_metadata(RegistryMetadataRequest::v1(registry.clone())).expect("metadata");
    let after = directory_entries(&registry);

    assert_eq!(metadata.id, "cusip-isin");
    assert_eq!(metadata.version, "1.0.0");
    assert_eq!(metadata.entry_count, 6);
    assert_eq!(before, after);
}

#[test]
fn sdk_artifact_reader_enforces_byte_bound() {
    let artifact_path = fixture_path("golden/partial.json");
    let mut request = ArtifactReadRequest::v1(artifact_path, ArtifactKind::Explanation);
    request.limits = ReadLimits { max_bytes: 8 };

    let error = read_artifact(request).expect_err("small byte bound refuses");
    assert_eq!(error.code, RefusalCode::ETooLarge);
}

#[test]
fn sdk_project_run_events_page_deterministically() {
    let temp = TempDir::new().expect("temp");
    let report_path = temp.path().join("run.json");
    fs::write(&report_path, project_run_report_json()).expect("report writes");

    let first = read_project_run_events(ProjectRunEventsRequest::v1(
        report_path.clone(),
        PageRequest::first(2),
    ))
    .expect("first page");
    assert_eq!(first.page.total, 3);
    assert_eq!(first.page.returned, 2);
    assert_eq!(first.page.next_cursor.as_deref(), Some("2"));
    assert_eq!(
        first
            .events
            .iter()
            .map(|event| event.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "bravo"]
    );

    let second = read_project_run_events(ProjectRunEventsRequest::v1(
        report_path,
        PageRequest {
            limit: 2,
            cursor: first.page.next_cursor,
        },
    ))
    .expect("second page");
    assert_eq!(second.page.next_cursor, None);
    assert_eq!(second.events[0].node_id, "charlie");
}

#[test]
fn sdk_public_request_and_response_types_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ExactMappingRequest>();
    assert_send_sync::<canon::sdk::ExactMappingResponse>();
    assert_send_sync::<RowPreservingCsvMappingRequest>();
    assert_send_sync::<canon::sdk::RowPreservingCsvMappingResponse>();
    assert_send_sync::<ExactBatchLookupRequest>();
    assert_send_sync::<canon::sdk::ExactBatchLookupResponse>();
    assert_send_sync::<PackageOpenRequest>();
    assert_send_sync::<canon::sdk::PackageOpenResponse>();
    assert_send_sync::<PackageVerifyRequest>();
    assert_send_sync::<canon::sdk::PackageVerifyResponse>();
    assert_send_sync::<ArtifactReadRequest>();
    assert_send_sync::<canon::sdk::ArtifactReadResponse>();
    assert_send_sync::<RegistryMetadataRequest>();
    assert_send_sync::<canon::sdk::RegistryMetadataResponse>();
    assert_send_sync::<ProjectRunEventsRequest>();
    assert_send_sync::<canon::sdk::ProjectRunEventsResponse>();
}

fn directory_entries(path: &Path) -> BTreeSet<String> {
    fs::read_dir(path)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn package_root(package_bytes: &[u8]) -> TempDir {
    let temp = TempDir::new().expect("package temp");
    fs::write(temp.path().join("README.md"), b"sdk demo\n").expect("readme");
    fs::write(temp.path().join("package.json"), package_bytes).expect("package json");
    temp
}

fn package_json() -> Value {
    json!({
        "schema_version": "canon.strategy.package.v1",
        "package_id": "pkg.sdk.demo",
        "package_version": "1.0.0",
        "content_digest": "",
        "license_expression": "MIT",
        "capabilities": ["read_registry"],
        "dependency_references": [],
        "provenance": {
            "source": "sdk-conformance"
        }
    })
}

fn canonical_package_bytes(mut value: Value) -> Vec<u8> {
    value["content_digest"] = Value::String(String::new());
    let digest = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&value).expect("digest view")).to_hex()
    );
    value["content_digest"] = Value::String(digest);
    serde_json::to_vec(&value).expect("canonical package")
}

fn project_run_report_json() -> String {
    serde_json::to_string(&json!({
        "schema_version": "canon.project.run.v2",
        "project_id": "project.sdk.demo",
        "plan_graph_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "run_receipt_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "max_parallelism": 1,
        "max_ready_width": 1,
        "executed_nodes": ["charlie", "alpha", "bravo"],
        "resumed_nodes": [],
        "failed_nodes": [],
        "cancelled_nodes": [],
        "invalidated_nodes": [],
        "blocked_nodes": [],
        "next_actions": {},
        "receipt": {
            "schema_version": "canon.project.run.v2",
            "project_id": "project.sdk.demo",
            "plan_graph_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "receipt_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "completed_nodes": ["alpha", "bravo", "charlie"],
            "failed_nodes": [],
            "cancelled_nodes": [],
            "invalidated_nodes": [],
            "blocked_nodes": [],
            "node_receipts": []
        },
        "node_reports": [
            {
                "node_id": "charlie",
                "outcome": "completed",
                "receipt_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            },
            {
                "node_id": "alpha",
                "outcome": "completed",
                "receipt_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            {
                "node_id": "bravo",
                "outcome": "failed",
                "reason": "fixture failure"
            }
        ]
    }))
    .expect("run report json")
}
