#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        apply::{
            ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
            run_apply_streaming,
        },
        schema::CANON_ENTITY_PROMOTION_SIDECAR_VERSION,
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

const APPLY_REFUSAL_MATRIX: &str =
    include_str!("../fixtures/entity/apply/refusals/apply_refusal_matrix.json");

#[derive(Debug, Deserialize)]
struct ApplyRefusalMatrix {
    version: String,
    cases: Vec<ApplyRefusalCase>,
}

#[derive(Debug, Deserialize)]
struct ApplyRefusalCase {
    id: String,
    refusal_code: String,
    required_detail_fields: Vec<String>,
    writes_performed: bool,
}

#[test]
fn apply_refusal_matrix_fixture_locks_apply_stage_cases() {
    let matrix: ApplyRefusalMatrix =
        serde_json::from_str(APPLY_REFUSAL_MATRIX).expect("matrix fixture");
    assert_eq!(matrix.version, "canon_entity_apply_refusal_matrix.v0");
    let by_id = matrix
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for (id, code) in [
        (
            "apply_stale_registry_snapshot",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        (
            "apply_unresolved_full_resolution",
            "E_ENTITY_APPLY_UNRESOLVED",
        ),
        ("apply_malformed_sidecar", "E_ENTITY_ARTIFACT_CONTRACT"),
    ] {
        let case = by_id.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(case.refusal_code, code);
        assert!(
            case.required_detail_fields
                .iter()
                .any(|field| field == "writes_performed"),
            "{id} must require writes_performed"
        );
        assert!(
            !case.writes_performed,
            "{id} must refuse before writing output"
        );
    }
}

#[test]
fn apply_refusal_no_output_mutation_on_stale_registry_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = write_rows(temp.path());
    let output = temp.path().join("existing.canon.csv");
    fs::write(&output, "sentinel output\n").expect("sentinel output");
    let rows_before = fs::read_to_string(&rows).expect("rows before");
    let output_before = fs::read_to_string(&output).expect("output before");

    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: registry(),
        resolutions: &resolutions(),
        safety: ApplySafetyCheck {
            expected_registry_snapshot_hash: Some("blake3:registry-before".to_string()),
            actual_registry_snapshot_hash: Some("blake3:registry-after".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: false,
        target_rows_per_chunk: 1024,
    })
    .expect_err("stale registry refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(
        refusal.detail["expected_registry_snapshot_hash"],
        "blake3:registry-before"
    );
    assert_eq!(
        refusal.detail["actual_registry_snapshot_hash"],
        "blake3:registry-after"
    );
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(fs::read_to_string(&rows).expect("rows after"), rows_before);
    assert_eq!(
        fs::read_to_string(&output).expect("output after"),
        output_before
    );
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_APPLY_UNRESOLVED_refuses_before_creating_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "loan_id,tenant_name\nL-001,Sears\nL-404,Unknown\n").expect("rows");
    let rows_before = fs::read_to_string(&rows).expect("rows before");
    let output = temp.path().join("new").join("rows.canon.csv");

    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: registry(),
        resolutions: &resolutions(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect_err("full apply refuses unresolved rows");

    assert_eq!(refusal.code, RefusalCode::EEntityApplyUnresolved);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["unresolved"], 1);
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(fs::read_to_string(&rows).expect("rows after"), rows_before);
    assert!(!output.exists());
    assert!(!output.parent().expect("output parent").exists());
}

#[test]
fn apply_malformed_sidecar_refuses_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = write_rows(temp.path());
    let output = temp.path().join("existing.canon.csv");
    fs::write(&output, "sentinel output\n").expect("sentinel output");
    let output_before = fs::read_to_string(&output).expect("output before");

    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: registry(),
        resolutions: &resolutions(),
        safety: ApplySafetyCheck {
            expected_sidecar_artifact_version: Some(
                CANON_ENTITY_PROMOTION_SIDECAR_VERSION.to_string(),
            ),
            actual_sidecar_artifact_version: Some("canon_entity_promotion_sidecar.v99".to_string()),
            expected_sidecar_snapshot_hash: Some("blake3:sidecar".to_string()),
            actual_sidecar_snapshot_hash: Some("blake3:sidecar".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: false,
        target_rows_per_chunk: 1024,
    })
    .expect_err("malformed sidecar refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["field"], "sidecar_artifact_version");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        fs::read_to_string(&output).expect("output after"),
        output_before
    );
}

fn write_rows(dir: &Path) -> std::path::PathBuf {
    let rows = dir.join("rows.csv");
    fs::write(&rows, "loan_id,tenant_name\nL-001,Sears\nL-002,Kmart\n").expect("rows");
    rows
}

fn registry() -> ApplyRegistryReference {
    ApplyRegistryReference {
        id: "cmbs-tenants".to_string(),
        version: "2026.06.25".to_string(),
    }
}

fn resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    BTreeMap::from([
        (
            "Sears".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-SEARS".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
        (
            "Kmart".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-KMART".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
    ])
}
