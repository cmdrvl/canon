#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_APPLY_VERSION_V1, CANON_ENTITY_RUN_VERSION, CANON_ENTITY_RUN_VERSION_V1,
        EntityArtifactStageV1,
        apply::{ApplyV1ArtifactRequest, DEFAULT_APPLY_ROWS_PER_CHUNK, run_apply_v1_from_registry},
        schema::{
            entity_v1_contract_for_stage, entity_v1_schema_content_hash,
            validate_entity_v1_self_hash,
        },
    },
};
use serde_json::{Value, json};
use std::{fs, path::Path};

#[test]
fn explicit_column_and_out_replay_custom_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let registry_hash = registry_snapshot_hash(&registry);

    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label,source\nAcme,feed-a\n").expect("rows");
    let result = temp.path().join("result.json");
    write_json(
        &result,
        &minimal_v1_source_artifact(
            EntityArtifactStageV1::Run,
            &registry_hash,
            &file_content_hash(&rows),
            1,
            path_str(temp.path()),
        ),
    );
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
    assert_eq!(artifact["version"], CANON_ENTITY_APPLY_VERSION_V1);
    assert_eq!(artifact["summary"]["counts"]["rows"], 1);
    assert_eq!(artifact["summary"]["counts"]["resolved"], 1);
    assert_eq!(artifact["summary"]["counts"]["unresolved"], 0);
    assert_eq!(
        artifact["summary"]["labels"]["lookup_column"],
        "tenant_label"
    );
    assert_eq!(
        validate_entity_v1_self_hash(&artifact).expect("apply cli self hash"),
        artifact["artifact_content_hash"].as_str().expect("hash")
    );
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
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let registry_hash = registry_snapshot_hash(&registry);

    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label\nAcme\nUnknown\n").expect("rows");
    let result = temp.path().join("result.json");
    write_json(
        &result,
        &minimal_v1_source_artifact(
            EntityArtifactStageV1::Run,
            &registry_hash,
            &file_content_hash(&rows),
            2,
            path_str(temp.path()),
        ),
    );

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
    assert_eq!(artifact["version"], CANON_ENTITY_APPLY_VERSION_V1);
    assert_eq!(artifact["summary"]["counts"]["rows"], 2);
    assert_eq!(artifact["summary"]["counts"]["resolved"], 1);
    assert_eq!(artifact["summary"]["counts"]["unresolved"], 1);

    let csv = fs::read_to_string(&partial_out).expect("partial output");
    assert!(csv.contains("Acme,TNT-ACME,tenant_label,resolved"));
    assert!(csv.contains("Unknown,,,unresolved,apply-cli-registry,2026.07.11,"));
}

#[test]
fn v1_apply_emits_self_hashed_artifact_and_replays_csv_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme, Inc",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let registry_hash = registry_snapshot_hash(&registry);
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label,raw\n\"Acme, Inc\",keep-me\n").expect("rows");
    let source = minimal_v1_source_artifact(
        EntityArtifactStageV1::Run,
        &registry_hash,
        &file_content_hash(&rows),
        1,
        path_str(temp.path()),
    );
    let out = temp.path().join("rows.canon.csv");

    let artifact = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
        source_artifact: &source,
        rows: &rows,
        output: &out,
        lookup_column: "tenant_label",
        registry_dir: &registry,
        require_full_resolution: true,
        target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
    })
    .expect("v1 apply succeeds");

    assert_eq!(artifact["version"], CANON_ENTITY_APPLY_VERSION_V1);
    assert_eq!(
        artifact["metadata"]["schema"]["key"],
        CANON_ENTITY_APPLY_VERSION_V1
    );
    assert_eq!(
        artifact["metadata"]["upstream_artifacts"][0]["version"],
        CANON_ENTITY_RUN_VERSION_V1
    );
    assert_eq!(artifact["summary"]["counts"]["rows"], 1);
    assert_eq!(artifact["summary"]["counts"]["resolved"], 1);
    assert_eq!(artifact["summary"]["counts"]["unresolved"], 0);
    assert_eq!(
        artifact["summary"]["labels"]["lookup_column"],
        "tenant_label"
    );
    assert!(validate_entity_v1_self_hash(&artifact).is_ok());
    assert_eq!(
        fs::read_to_string(&out).expect("apply output"),
        "tenant_label,raw,canonical_id,canonical_type,canonical_status,canonical_registry_id,canonical_registry_version,canonical_rule_id\n\"Acme, Inc\",keep-me,TNT-ACME,tenant_label,resolved,apply-cli-registry,2026.07.11,MANUAL\n"
    );
}

#[test]
fn v1_apply_refuses_legacy_and_tampered_sources_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label\nAcme\n").expect("rows");
    let legacy_out = temp.path().join("legacy.csv");
    fs::write(&legacy_out, "sentinel\n").expect("sentinel");
    let legacy = json!({
        "version": CANON_ENTITY_RUN_VERSION,
        "summary": {},
        "metadata": {}
    });

    let refusal = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
        source_artifact: &legacy,
        rows: &rows,
        output: &legacy_out,
        lookup_column: "tenant_label",
        registry_dir: &registry,
        require_full_resolution: true,
        target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
    })
    .expect_err("legacy source refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["reason"], "legacy_entity_result_version");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        fs::read_to_string(&legacy_out).expect("legacy output"),
        "sentinel\n"
    );

    let registry_hash = registry_snapshot_hash(&registry);
    let mut tampered = minimal_v1_source_artifact(
        EntityArtifactStageV1::Run,
        &registry_hash,
        &file_content_hash(&rows),
        1,
        path_str(temp.path()),
    );
    tampered["summary"]["counts"]["rows"] = json!(99);
    let tamper_out = temp.path().join("tampered.csv");
    fs::write(&tamper_out, "sentinel\n").expect("sentinel");

    let refusal = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
        source_artifact: &tampered,
        rows: &rows,
        output: &tamper_out,
        lookup_column: "tenant_label",
        registry_dir: &registry,
        require_full_resolution: true,
        target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
    })
    .expect_err("tampered source refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
    assert_eq!(
        fs::read_to_string(&tamper_out).expect("tamper output"),
        "sentinel\n"
    );
}

#[test]
fn entity_apply_cli_refuses_legacy_and_tampered_sources_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label\nAcme\n").expect("rows");

    let legacy_result = temp.path().join("legacy-result.json");
    write_json(&legacy_result, &legacy_run_artifact());
    let legacy_out = temp.path().join("legacy.csv");
    let refusal = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&legacy_result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&legacy_out),
            "--emit",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("legacy refusal");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "legacy_entity_result_version"
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert!(!legacy_out.exists());

    let registry_hash = registry_snapshot_hash(&registry);
    let mut tampered = minimal_v1_source_artifact(
        EntityArtifactStageV1::Run,
        &registry_hash,
        &file_content_hash(&rows),
        1,
        path_str(temp.path()),
    );
    tampered["summary"]["counts"]["rows"] = json!(99);
    let tampered_result = temp.path().join("tampered-result.json");
    write_json(&tampered_result, &tampered);
    let tampered_out = temp.path().join("tampered.csv");
    let refusal = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&tampered_result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&tampered_out),
            "--emit",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("tamper refusal");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["field"],
        "artifact_content_hash"
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert!(!tampered_out.exists());
}

#[test]
fn entity_apply_cli_refuses_input_binding_mismatches_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let registry_hash = registry_snapshot_hash(&registry);
    let rows = temp.path().join("rows.csv");
    fs::write(&rows, "tenant_label\nAcme\n").expect("rows");

    let bad_hash_result = temp.path().join("bad-hash-result.json");
    write_json(
        &bad_hash_result,
        &minimal_v1_source_artifact(
            EntityArtifactStageV1::Run,
            &registry_hash,
            "blake3:not-the-actual-rows",
            1,
            path_str(temp.path()),
        ),
    );
    let bad_hash_out = temp.path().join("bad-hash.csv");
    let refusal = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&bad_hash_result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&bad_hash_out),
            "--emit",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("bad hash refusal");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["field"],
        "metadata.input.content_hash"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["expected"],
        "blake3:not-the-actual-rows"
    );
    assert!(
        refusal["refusal"]["detail"]["actual"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("blake3:"))
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert!(!bad_hash_out.exists());

    let bad_count_result = temp.path().join("bad-count-result.json");
    write_json(
        &bad_count_result,
        &minimal_v1_source_artifact(
            EntityArtifactStageV1::Run,
            &registry_hash,
            &file_content_hash(&rows),
            2,
            path_str(temp.path()),
        ),
    );
    let bad_count_out = temp.path().join("bad-count.csv");
    fs::write(&bad_count_out, "sentinel\n").expect("sentinel");
    let refusal = canon_cmd()
        .args([
            "entity",
            "apply",
            path_str(&bad_count_result),
            "--rows",
            path_str(&rows),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant_label",
            "--out",
            path_str(&bad_count_out),
            "--emit",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let refusal: Value = serde_json::from_slice(&refusal).expect("bad count refusal");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["field"],
        "metadata.input.row_count"
    );
    assert_eq!(refusal["refusal"]["detail"]["expected"], 2);
    assert_eq!(refusal["refusal"]["detail"]["actual"], 1);
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(
        fs::read_to_string(&bad_count_out).expect("bad count output"),
        "sentinel\n"
    );
}

#[test]
fn v1_apply_replays_jsonl_order_and_partial_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    write_profile_registry(
        &registry,
        &[json!({
            "input": "Acme",
            "canonical_id": "TNT-ACME",
            "canonical_type": "tenant_label",
            "rule_id": "MANUAL"
        })],
    );
    let registry_hash = registry_snapshot_hash(&registry);
    let rows = temp.path().join("rows.jsonl");
    fs::write(
        &rows,
        "{\"tenant_label\":\"Unknown\",\"amount\":20}\n{\"amount\":10,\"tenant_label\":\"Acme\"}\n",
    )
    .expect("jsonl rows");
    let source = minimal_v1_source_artifact(
        EntityArtifactStageV1::Run,
        &registry_hash,
        &file_content_hash(&rows),
        2,
        path_str(temp.path()),
    );
    let out = temp.path().join("rows.canon.jsonl");

    let artifact = run_apply_v1_from_registry(ApplyV1ArtifactRequest {
        source_artifact: &source,
        rows: &rows,
        output: &out,
        lookup_column: "tenant_label",
        registry_dir: &registry,
        require_full_resolution: false,
        target_rows_per_chunk: DEFAULT_APPLY_ROWS_PER_CHUNK,
    })
    .expect("partial v1 apply succeeds");

    assert_eq!(artifact["version"], CANON_ENTITY_APPLY_VERSION_V1);
    assert_eq!(artifact["summary"]["counts"]["rows"], 2);
    assert_eq!(artifact["summary"]["counts"]["resolved"], 1);
    assert_eq!(artifact["summary"]["counts"]["unresolved"], 1);
    assert_eq!(
        fs::read_to_string(&out).expect("jsonl output"),
        "{\"tenant_label\":\"Unknown\",\"amount\":20,\"canonical_id\":null,\"canonical_type\":null,\"canonical_status\":\"unresolved\",\"canonical_registry_id\":\"apply-cli-registry\",\"canonical_registry_version\":\"2026.07.11\",\"canonical_rule_id\":null}\n{\"amount\":10,\"tenant_label\":\"Acme\",\"canonical_id\":\"TNT-ACME\",\"canonical_type\":\"tenant_label\",\"canonical_status\":\"resolved\",\"canonical_registry_id\":\"apply-cli-registry\",\"canonical_registry_version\":\"2026.07.11\",\"canonical_rule_id\":\"MANUAL\"}\n"
    );
}

fn canon_cmd() -> assert_cmd::Command {
    assert_cmd::cargo::cargo_bin_cmd!("canon")
}

fn legacy_run_artifact() -> Value {
    json!({
        "version": CANON_ENTITY_RUN_VERSION,
        "summary": {},
        "metadata": {}
    })
}

fn write_profile_registry(registry: &Path, aliases: &[Value]) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "apply-cli-registry",
            "version": "2026.07.11",
            "description": "apply CLI test registry",
            "updated": "2026-07-11",
            "entry_count": aliases.len(),
            "entity_profile": {
                "id": "apply_cli_profile",
                "identity_semantics": "canonical_display_label"
            }
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

fn minimal_v1_source_artifact(
    stage: EntityArtifactStageV1,
    registry_hash: &str,
    input_hash: &str,
    row_count: u64,
    root_dir: &str,
) -> Value {
    let contract = entity_v1_contract_for_stage(stage).expect("stage contract");
    let (payload_field, payload_path) = match stage {
        EntityArtifactStageV1::Solve => ("entities_path", "solve/entities.jsonl"),
        EntityArtifactStageV1::Run => ("run_manifest_path", "run/manifest.json"),
        _ => panic!("apply source must be solve or run"),
    };
    let mut artifact = json!({
        "version": contract.artifact_version,
        "artifact_content_hash": "",
        "metadata": {
            "profile": {
                "id": "apply_cli_profile",
                "version": "2026.07.11",
                "entity_type": "organization",
                "identity_semantics": "canonical_display_label",
                "canonical_type": "tenant_label",
                "patch_namespaces": {
                    "aliases": "apply_cli_profile.aliases",
                    "distinct": "apply_cli_profile.distinct",
                    "relations": "apply_cli_profile.relations"
                },
                "content_hash": "blake3:profile"
            },
            "strategy": {
                "id": "apply-cli-strategy",
                "version": "2026.07.11",
                "content_hash": "blake3:strategy"
            },
            "registry_snapshot": {
                "id": "apply-cli-registry",
                "version": "2026.07.11",
                "source": "registry",
                "lookup_snapshot_hash": registry_hash
            },
            "input": {
                "row_count": row_count,
                "content_hash": input_hash
            },
            "patch_namespace": "apply_cli_profile.aliases",
            "schema": {
                "key": contract.schema_key,
                "content_hash": entity_v1_schema_content_hash(contract).expect("schema hash")
            },
            "workdir": {
                "root_dir": root_dir,
                "stage_dir": contract.stage_dir,
                "artifact_relpath": contract.artifact_relpath,
                "payload_relpath": contract.payload_relpath
            },
            "upstream_artifacts": [],
            "artifact_content_hash": ""
        },
        "summary": {
            "counts": {
                "rows": row_count
            },
            "labels": {
                "stage": stage.as_str()
            }
        }
    });
    artifact
        .as_object_mut()
        .expect("source artifact object")
        .insert(payload_field.to_string(), json!(payload_path));
    canon::entity::schema::finalize_entity_v1_self_hash(&mut artifact).expect("self hash");
    artifact
}

fn registry_snapshot_hash(registry: &Path) -> String {
    let mut files = fs::read_dir(registry)
        .expect("registry dir")
        .map(|entry| entry.expect("registry entry").path())
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("utf-8 filename");
        let bytes = fs::read(&path).expect("registry file");
        hasher.update(file_name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn file_content_hash(path: &Path) -> String {
    let bytes = fs::read(path).expect("hash input file");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
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
