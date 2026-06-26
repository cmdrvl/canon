#![forbid(unsafe_code)]

use canon::entity::apply::{
    APPLY_CANONICAL_FIELDS, ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck,
    ApplyStreamRequest, run_apply_streaming,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/apply_loop/manifest.json";
const ROWS_PATH: &str = "tests/fixtures/entity/cmbs/apply_loop/rows.csv";
const EXPECTED_PATH: &str = "tests/fixtures/entity/cmbs/apply_loop/expected.csv";

#[derive(Debug, Deserialize)]
struct ApplyLoopManifest {
    schema_version: String,
    profile_id: String,
    identity_semantics: String,
    registry: ApplyLoopRegistry,
    lookup_column: String,
    row_count: u64,
    canonical_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ApplyLoopRegistry {
    id: String,
    version: String,
    snapshot_hash: String,
    sidecar_version: String,
    sidecar_snapshot_hash: String,
}

#[test]
fn cmbs_apply_loop_appends_canonical_fields_and_preserves_raw_columns() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, "canon.entity.cmbs_apply_loop.v0");
    assert_eq!(
        manifest.canonical_fields,
        APPLY_CANONICAL_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("tenants.canon.csv");
    let artifact = run_apply_streaming(apply_request(&manifest, &output))
        .expect("CMBS apply exact replay succeeds");

    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert_eq!(artifact.registry.id, manifest.registry.id);
    assert_eq!(artifact.registry.version, manifest.registry.version);
    assert_eq!(artifact.summary["rows"], manifest.row_count);
    assert_eq!(artifact.summary["resolved"], manifest.row_count);
    assert_eq!(artifact.summary["unresolved"], 0);
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        fs::read_to_string(repo_path(EXPECTED_PATH)).expect("expected output")
    );

    assert_raw_columns_preserved(repo_path(ROWS_PATH), &output, APPLY_CANONICAL_FIELDS);
}

#[test]
fn apply_streaming_exact_replay_cmbs_apply_loop_is_byte_stable() {
    let manifest = manifest();
    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("tenants.canon.csv");

    let first =
        run_apply_streaming(apply_request(&manifest, &output)).expect("first CMBS apply succeeds");
    let first_bytes = fs::read(&output).expect("first output bytes");
    let second =
        run_apply_streaming(apply_request(&manifest, &output)).expect("second CMBS apply succeeds");
    let second_bytes = fs::read(&output).expect("second output bytes");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first.artifact_content_hash, second.artifact_content_hash);
    assert_eq!(first.streaming.chunks, second.streaming.chunks);
}

fn apply_request<'a>(manifest: &'a ApplyLoopManifest, output: &'a Path) -> ApplyStreamRequest<'a> {
    ApplyStreamRequest {
        rows: &repo_path(ROWS_PATH),
        output,
        lookup_column: &manifest.lookup_column,
        registry: ApplyRegistryReference {
            id: manifest.registry.id.clone(),
            version: manifest.registry.version.clone(),
        },
        resolutions: &resolutions(),
        safety: ApplySafetyCheck {
            expected_profile_id: Some(manifest.profile_id.clone()),
            actual_profile_id: Some(manifest.profile_id.clone()),
            expected_identity_semantics: Some(manifest.identity_semantics.clone()),
            actual_identity_semantics: Some(manifest.identity_semantics.clone()),
            expected_registry_snapshot_hash: Some(manifest.registry.snapshot_hash.clone()),
            actual_registry_snapshot_hash: Some(manifest.registry.snapshot_hash.clone()),
            expected_sidecar_artifact_version: Some(manifest.registry.sidecar_version.clone()),
            actual_sidecar_artifact_version: Some(manifest.registry.sidecar_version.clone()),
            expected_sidecar_snapshot_hash: Some(manifest.registry.sidecar_snapshot_hash.clone()),
            actual_sidecar_snapshot_hash: Some(manifest.registry.sidecar_snapshot_hash.clone()),
        },
        require_full_resolution: true,
        target_rows_per_chunk: 2,
    }
}

fn resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    BTreeMap::from([
        ("Sears".to_string(), resolution("TNT-SEARS")),
        (
            "24 Hour Fitness".to_string(),
            resolution("TNT-24-HOUR-FITNESS"),
        ),
        (
            "238 Sand Island Prop".to_string(),
            resolution("TNT-238-SAND-ISLAND-PROPERTY"),
        ),
    ])
}

fn resolution(canonical_id: &str) -> ApplyCanonicalResolution {
    ApplyCanonicalResolution {
        canonical_id: canonical_id.to_string(),
        canonical_type: "tenant_label".to_string(),
        rule_id: "REGISTRY_EXACT".to_string(),
    }
}

fn assert_raw_columns_preserved(input: PathBuf, output: &Path, appended_fields: &[&str]) {
    let mut input_reader = csv::Reader::from_path(&input).expect("input csv opens");
    let input_headers = input_reader
        .headers()
        .expect("input headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let input_rows = input_reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("input rows parse");

    let mut output_reader = csv::Reader::from_path(output).expect("output csv opens");
    let output_headers = output_reader
        .headers()
        .expect("output headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        &output_headers[..input_headers.len()],
        input_headers.as_slice()
    );
    assert_eq!(
        &output_headers[input_headers.len()..],
        appended_fields
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
            .as_slice()
    );

    let output_rows = output_reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("output rows parse");
    assert_eq!(output_rows.len(), input_rows.len());
    for (input, output) in input_rows.iter().zip(output_rows.iter()) {
        for header in &input_headers {
            assert_eq!(
                output.get(header),
                input.get(header),
                "raw field {header} changed"
            );
        }
    }
}

fn manifest() -> ApplyLoopManifest {
    serde_json::from_slice(&fs::read(repo_path(MANIFEST_PATH)).expect("manifest bytes"))
        .expect("manifest parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
