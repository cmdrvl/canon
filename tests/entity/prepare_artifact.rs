use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_PREPARE_VERSION,
        prepare::{PrepareRunRequest, run_prepare},
        schema::validate_artifact_core_contract,
    },
};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[test]
fn entity_prepare_artifact_contract_is_deterministic_and_chain_valid() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    write_rows(&rows);
    write_registry(&registry);

    let mut previous_bytes = None;
    let mut previous_surfaces = None;
    for index in 0..3 {
        let work_dir = temp.path().join(format!("work-{index}"));
        let artifact = run_prepare(PrepareRunRequest {
            rows: &rows,
            profile: "cmbs_tenant_label",
            registry: &registry,
            work_dir: &work_dir,
        })
        .expect("prepare run");

        assert_eq!(artifact.version, CANON_ENTITY_PREPARE_VERSION);
        assert_eq!(
            artifact.metadata.artifact_content_hash,
            artifact.artifact_content_hash
        );
        assert_eq!(artifact.metadata.profile.id, "cmbs_tenant_label");
        assert_eq!(
            artifact
                .metadata
                .profile
                .content_hash
                .as_deref()
                .map(hash_prefix),
            Some("blake3:")
        );
        assert_eq!(artifact.metadata.strategy.id, "cmbs_tenant_label.prepare");
        assert!(
            artifact
                .metadata
                .strategy
                .content_hash
                .starts_with("blake3:")
        );
        assert_eq!(
            artifact.metadata.registry_snapshot.lookup_snapshot_hash,
            artifact.registry_snapshot.lookup_snapshot_hash
        );
        assert_eq!(
            artifact
                .metadata
                .input
                .as_ref()
                .expect("input")
                .content_hash,
            artifact.input.content_hash
        );
        assert!(
            artifact
                .metadata
                .patch_set
                .as_ref()
                .expect("patch set")
                .content_hash
                .starts_with("blake3:")
        );
        assert!(
            artifact
                .metadata
                .namekit
                .as_ref()
                .expect("namekit")
                .content_hash
                .starts_with("blake3:")
        );
        assert_eq!(artifact.summary["row_count"], 3);
        assert_eq!(artifact.summary["prepared_surfaces"], 2);
        assert_eq!(artifact.surfaces_path, "prepare/surfaces.jsonl");

        let artifact_path = work_dir.join("prepare").join("prepare.json");
        let bytes = fs::read(&artifact_path).expect("prepare artifact bytes");
        let json: Value = serde_json::from_slice(&bytes).expect("prepare artifact json");
        let snapshot = validate_artifact_core_contract(&json).expect("core contract validates");
        assert_eq!(snapshot.artifact_version, CANON_ENTITY_PREPARE_VERSION);

        let surfaces = fs::read_to_string(work_dir.join(&artifact.surfaces_path))
            .expect("prepare surfaces jsonl");
        if let Some(previous) = &previous_bytes {
            assert_eq!(&bytes, previous);
        }
        if let Some(previous) = &previous_surfaces {
            assert_eq!(&surfaces, previous);
        }
        previous_bytes = Some(bytes);
        previous_surfaces = Some(surfaces);
    }
}

#[test]
fn entity_prepare_artifact_contract_refuses_malformed_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_rows(&rows);
    write_registry(&registry);

    run_prepare(PrepareRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("prepare run");

    let artifact_path = work_dir.join("prepare").join("prepare.json");
    let mut json: Value =
        serde_json::from_slice(&fs::read(&artifact_path).expect("artifact bytes"))
            .expect("artifact json");
    json["metadata"]["strategy"]
        .as_object_mut()
        .expect("strategy object")
        .remove("content_hash");

    let refusal = validate_artifact_core_contract(&json).expect_err("malformed artifact refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
}

fn hash_prefix(hash: &str) -> &str {
    &hash[..7]
}

fn write_rows(path: &Path) {
    fs::write(
        path,
        concat!(
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\n",
            "row-1,deal-1,loan-1,prop-1,Sears LLC,\"[\"\"Sears Roebuck\"\"]\",[]\n",
            "row-2,deal-1,loan-2,prop-1,Sears,[],[]\n",
            "row-3,deal-2,loan-3,prop-2,Unknown Shop,[],[]\n",
        ),
    )
    .expect("rows");
}

fn write_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"prepare artifact test registry","updated":"2026-06-25","entry_count":1}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        r#"[{"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"TENANT_ALIAS"}]"#,
    )
    .expect("registry aliases");
}
