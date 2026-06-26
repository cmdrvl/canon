#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::apply::{
        ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
        run_apply_streaming,
    },
};
use std::{collections::BTreeMap, fs};

const EN_A001_EXPECTED_CSV: &str = include_str!("../fixtures/entity/apply/en_a001_expected.csv");

#[test]
#[allow(non_snake_case)]
fn EN_A001_apply_exact_replay_preserves_raw_fields_and_appends_canonical_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("tenants.csv");
    let output = temp.path().join("tenants.canon.csv");
    let raw_rows = concat!(
        "loan_id,tenant_name,as_reported_amount\n",
        "L-001,\"SEARS, LLC\",10\n",
        "L-002,Kmart,20\n",
    );
    fs::write(&rows, raw_rows).expect("rows");

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: registry(),
        resolutions: &resolutions(),
        safety: ApplySafetyCheck::default(),
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect("full apply resolves");

    assert_eq!(fs::read_to_string(&rows).expect("raw rows"), raw_rows);
    assert_eq!(
        fs::read_to_string(&output).expect("applied rows"),
        EN_A001_EXPECTED_CSV
    );
    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(artifact.registry, registry());
    assert_eq!(artifact.summary["rows"], 2);
    assert_eq!(artifact.summary["resolved"], 2);
    assert_eq!(artifact.summary["unresolved"], 0);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_APPLY_UNRESOLVED_refuses_full_resolution_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("tenants.csv");
    let output = temp.path().join("out").join("tenants.canon.csv");
    fs::write(
        &rows,
        concat!(
            "loan_id,tenant_name,as_reported_amount\n",
            "L-001,Sears,10\n",
            "L-999,Unknown,30\n",
        ),
    )
    .expect("rows");

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
    .expect_err("unresolved full apply refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityApplyUnresolved);
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["field"], "tenant_name");
    assert_eq!(refusal.detail["rows"], 2);
    assert_eq!(refusal.detail["resolved"], 1);
    assert_eq!(refusal.detail["unresolved"], 1);
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!output.exists());
    assert!(!output.parent().expect("output parent").exists());
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
            "SEARS, LLC".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-SEARS".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        ),
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
