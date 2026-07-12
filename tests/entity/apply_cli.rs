#![forbid(unsafe_code)]

use canon::entity::{CANON_ENTITY_APPLY_VERSION, CANON_ENTITY_RUN_VERSION_V1};
use serde_json::{Value, json};
use std::{fs, path::Path};

#[test]
fn explicit_column_and_out_replay_custom_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );

    let result = temp.path().join("result.json");
    write_json(&result, &minimal_run_artifact(Some("custom_profile")));
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label,source\nAcme,feed-a\n").expect("rows");
    let out = temp.path().join("rows.canon.csv");

    let output = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&out),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let artifact: Value = serde_json::from_slice(&output).expect("apply artifact");
    assert_eq!(artifact["version"], CANON_ENTITY_APPLY_VERSION);
    assert_eq!(artifact["summary"]["rows"], 1);
    assert_eq!(artifact["summary"]["resolved"], 1);
    assert_eq!(artifact["summary"]["unresolved"], 0);
    assert_eq!(artifact["output_path"], path_str(&out));

    let csv = fs::read_to_string(&out).expect("apply output");
    assert_eq!(
        csv,
        "tenant_label,source,canonical_id,canonical_type,canonical_status,canonical_registry_id,canonical_registry_version,canonical_rule_id\nAcme,feed-a,TNT-ACME,tenant_label,resolved,apply-cli-registry,2026.07.11,MANUAL\n"
    );
}

#[test]
fn full_resolution_refuses_before_output_and_partial_output_exits_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );

    let result = temp.path().join("result.json");
    write_json(&result, &minimal_run_artifact(None));
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label\nAcme\nUnknown\n").expect("rows");

    let full_out = temp.path().join("full.csv");
    let refusal = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&full_out),
            "--emit",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("refusal json");
    assert_eq!(refusal["outcome"], "REFUSAL");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_APPLY_UNRESOLVED");
    assert_eq!(refusal["refusal"]["detail"]["stage"], "apply");
    assert_eq!(refusal["refusal"]["detail"]["unresolved"], 1);
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert!(
        !full_out.exists(),
        "full-resolution refusal must not write output"
    );

    let partial_out = temp.path().join("partial.csv");
    let partial = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&partial_out),
            "--allow-partial-output",
            "--emit",
            "json",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let artifact: Value = serde_json::from_slice(&partial).expect("partial apply artifact");
    assert_eq!(artifact["summary"]["rows"], 2);
    assert_eq!(artifact["summary"]["resolved"], 1);
    assert_eq!(artifact["summary"]["unresolved"], 1);

    let csv = fs::read_to_string(&partial_out).expect("partial output");
    assert!(csv.contains("Acme,TNT-ACME,tenant_label,resolved"));
    assert!(csv.contains("Unknown,,,unresolved,apply-cli-registry,2026.07.11,"));
}

fn canon_cmd() -> assert_cmd::Command {
    assert_cmd::cargo::cargo_bin_cmd!("canon")
}

fn minimal_run_artifact(profile_id: Option<&str>) -> Value {
    let mut artifact = json!({
        "version": CANON_ENTITY_RUN_VERSION_V1,
        "metadata": {}
    });
    if let Some(profile_id) = profile_id {
        artifact["metadata"]["profile"] = json!({ "id": profile_id });
    }
    artifact
}

fn write_registry(registry: &Path, aliases: &[Value]) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "apply-cli-registry",
            "version": "2026.07.11",
            "description": "apply CLI test registry",
            "updated": "2026-07-11",
            "entry_count": aliases.len()
        }))
        .expect("registry json"),
    )
    .expect("write registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_vec_pretty(aliases).expect("aliases json"),
    )
    .expect("write aliases");
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("json serializes"),
    )
    .expect("json writes");
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}
