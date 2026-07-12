#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_AUDIT_VERSION_V1, CANON_ENTITY_EXPLAIN_VERSION_V1,
    CANON_ENTITY_PROMOTE_VERSION_V1, CANON_ENTITY_REVIEW_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
    schema::compute_entity_v1_self_hash,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

#[test]
fn entity_v1_lifecycle_cli_review_audit_promote_explain_and_exact_lookup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let result_path = temp.path().join("run.v1.json");
    write_json(&result_path, &result);

    let review_json = canon_cmd()
        .args([
            "entity",
            "review",
            "export",
            path_str(&result_path),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let review: Value = serde_json::from_slice(&review_json).expect("review json");
    assert_eq!(review["version"], CANON_ENTITY_REVIEW_VERSION_V1);
    assert_eq!(review["summary"]["counts"]["review_items"], 1);
    let review_path = temp.path().join("review.v1.json");
    write_json(&review_path, &review);

    let review_csv = canon_cmd()
        .args([
            "entity",
            "review",
            "export",
            path_str(&result_path),
            "--emit",
            "csv",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let review_csv_path = temp.path().join("review.v1.csv");
    fs::write(&review_csv_path, review_csv).expect("review csv");

    let imported = canon_cmd()
        .args([
            "entity",
            "review",
            "import",
            path_str(&review_csv_path),
            "--registry",
            path_str(&registry),
            "--next-version",
            "2026.06.25-review",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let imported: Value = serde_json::from_slice(&imported).expect("review import json");
    assert_eq!(imported["version"], CANON_ENTITY_REVIEW_VERSION_V1);
    assert_eq!(imported["summary"]["labels"]["operation"], "import");

    let suite = temp.path().join("suite");
    fs::create_dir_all(&suite).expect("suite dir");
    let audit_json = canon_cmd()
        .args([
            "entity",
            "audit",
            path_str(&result_path),
            "--suite",
            path_str(&suite),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let audit: Value = serde_json::from_slice(&audit_json).expect("audit json");
    assert_eq!(audit["version"], CANON_ENTITY_AUDIT_VERSION_V1);
    assert_eq!(audit["summary"]["labels"]["status"], "passed");
    let audit_path = temp.path().join("audit.v1.json");
    write_json(&audit_path, &audit);

    let promote_json = canon_cmd()
        .args([
            "entity",
            "promote",
            path_str(&result_path),
            "--audit",
            path_str(&audit_path),
            "--registry",
            path_str(&registry),
            "--next-version",
            "2026.06.26",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let promote: Value = serde_json::from_slice(&promote_json).expect("promote json");
    assert_eq!(promote["version"], CANON_ENTITY_PROMOTE_VERSION_V1);
    assert_eq!(promote["summary"]["counts"]["promoted_aliases"], 1);
    assert_eq!(
        read_json(&registry.join("registry.json"))["version"],
        "2026.06.26"
    );

    let explain_json = canon_cmd()
        .args([
            "entity",
            "explain",
            path_str(&result_path),
            "--row",
            "row-1",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let explain: Value = serde_json::from_slice(&explain_json).expect("explain json");
    assert_eq!(explain["version"], CANON_ENTITY_EXPLAIN_VERSION_V1);
    assert_eq!(explain["result"]["selector"]["value"], "row-1");

    let input = temp.path().join("input.csv");
    fs::write(&input, "tenant\nSears\n").expect("input csv");
    let resolved = canon_cmd()
        .args([
            path_str(&input),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant",
            "--explicit",
            "--no-witness",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolved: Value = serde_json::from_slice(&resolved).expect("resolve json");
    assert_eq!(resolved["outcome"], "RESOLVED");
    assert_eq!(resolved["mappings"][0]["canonical_id"], "u8:TNT-SEARS");
}

fn canon_cmd() -> assert_cmd::Command {
    assert_cmd::cargo::cargo_bin_cmd!("canon")
}

fn run_v1_artifact(work: &Path) -> Value {
    let mut artifact = json!({
        "version": CANON_ENTITY_RUN_VERSION_V1,
        "artifact_content_hash": "blake3:placeholder",
        "metadata": {
            "profile": {
                "id": "cmbs_tenant_label",
                "version": "0.1.0",
                "entity_type": "tenant_label",
                "identity_semantics": "canonical_display_label",
                "canonical_type": "tenant_label",
                "patch_namespaces": {
                    "aliases": "cmbs_tenant_label.aliases",
                    "distinct": "cmbs_tenant_label.distinct",
                    "relations": "cmbs_tenant_label.relations"
                },
                "content_hash": "blake3:profile"
            },
            "strategy": {
                "id": "cmbs_tenant_label.v1",
                "version": "0.1.0",
                "content_hash": "blake3:strategy"
            },
            "registry_snapshot": {
                "id": "cmbs-tenants",
                "version": "2026.06.25",
                "source": "registry",
                "lookup_snapshot_hash": "blake3:registry"
            },
            "input": {
                "row_count": 1,
                "content_hash": "blake3:input"
            },
            "patch_namespace": "cmbs_tenant_label.aliases",
            "schema": {
                "key": CANON_ENTITY_RUN_VERSION_V1,
                "content_hash": "blake3:schema-run"
            },
            "workdir": {
                "root_dir": work.display().to_string(),
                "stage_dir": "run",
                "artifact_relpath": "run/run.json",
                "payload_relpath": "run/manifest.json"
            },
            "upstream_artifacts": [],
            "artifact_content_hash": "blake3:placeholder"
        },
        "summary": {
            "counts": {
                "review_groups": 1,
                "entity_count": 1
            },
            "labels": {
                "stage": "run"
            }
        },
        "run_manifest_path": "run/manifest.json",
        "rows": [
            {
                "row_id": "row-1",
                "surface_id": "surf:sears",
                "canonical_id": "TNT-SEARS"
            }
        ],
        "review_items": [
            {
                "review_id": "review:sears",
                "state": "promotable_new",
                "surface_ids": ["surf:sears"],
                "decision": "accept_aliases",
                "reason_code": "operator_confirmed_alias"
            }
        ],
        "promotable_aliases": [
            {
                "input": "Sears",
                "canonical_id": "TNT-SEARS",
                "canonical_type": "tenant_label",
                "rule_id": "ENTITY_V1_PROMOTE"
            }
        ]
    });
    let hash = compute_entity_v1_self_hash(&artifact).expect("run v1 hash");
    artifact["artifact_content_hash"] = Value::String(hash.clone());
    artifact["metadata"]["artifact_content_hash"] = Value::String(hash);
    artifact
}

fn write_registry(registry: &Path, version: &str) {
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "cmbs-tenants",
            "version": version,
            "description": "v1 lifecycle registry",
            "updated": "2026-06-26",
            "entry_count": 0
        }))
        .expect("registry json"),
    )
    .expect("write registry");
    fs::write(registry.join("aliases.json"), "[]\n").expect("write aliases");
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("json serializes"),
    )
    .expect("json writes");
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json reads")).expect("json parses")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}
