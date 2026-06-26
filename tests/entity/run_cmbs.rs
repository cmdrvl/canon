use assert_cmd::Command;
use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_INDEX_VERSION,
        CANON_ENTITY_PREPARE_VERSION, CANON_ENTITY_RUN_VERSION, CANON_ENTITY_SOLVE_VERSION,
        EntityArtifactReference,
        run::{EntityRunArtifact, EntityRunRequest, EntityRunStageArtifact, run_entity_workbench},
    },
};
use predicates::prelude::*;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn entity_run_cmbs_emits_chained_stage_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let result = run_entity_workbench(EntityRunRequest {
        rows: &fixture("tests/fixtures/entity/cmbs/small_book/observations.csv"),
        profile: "cmbs_tenant_label",
        strategy: &fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("entity run succeeds");

    let artifact = result.artifact;
    assert_eq!(artifact.version, CANON_ENTITY_RUN_VERSION);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.summary.counts["row_count"], 15);
    assert!(artifact.summary.counts["prepared_surfaces"] >= 10);
    assert!(artifact.summary.counts["exact_resolved_surfaces"] >= 3);
    assert_eq!(artifact.summary.labels["profile_id"], "cmbs_tenant_label");
    assert_eq!(artifact.summary.labels["registry_id"], "cmbs-tenants");

    assert_stage(&artifact, "prepare", CANON_ENTITY_PREPARE_VERSION, &[]);
    let prepare = stage(&artifact, "prepare");
    assert_stage(
        &artifact,
        "index",
        CANON_ENTITY_INDEX_VERSION,
        &[stage_ref(prepare)],
    );
    let index = stage(&artifact, "index");
    assert_stage(
        &artifact,
        "block",
        CANON_ENTITY_BLOCK_VERSION,
        &[stage_ref(prepare), stage_ref(index)],
    );
    let block = stage(&artifact, "block");
    assert_stage(
        &artifact,
        "edge",
        CANON_ENTITY_EDGE_VERSION,
        &[stage_ref(prepare), stage_ref(index), stage_ref(block)],
    );
    let edge = stage(&artifact, "edge");
    assert_stage(
        &artifact,
        "solve",
        CANON_ENTITY_SOLVE_VERSION,
        &[stage_ref(block), stage_ref(edge)],
    );

    for stage in &artifact.stage_artifacts {
        let path = Path::new(&stage.path);
        assert!(path.is_relative(), "stage path is relative: {}", stage.path);
        assert!(
            !path.components().any(|component| matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )),
            "stage path stays inside work dir: {}",
            stage.path
        );
        assert!(work_dir.join(&stage.path).exists(), "artifact exists");
    }
    assert!(work_dir.join("prepare/surfaces.jsonl").exists());
    assert!(work_dir.join("block/candidates.jsonl").exists());
    assert!(work_dir.join("block/exact_buckets.jsonl").exists());
    assert!(work_dir.join("edge/edges.jsonl").exists());
    assert!(work_dir.join("solve/decision_ledger.jsonl").exists());

    let persisted: EntityRunArtifact = read_json(&work_dir.join("run.json"));
    assert_eq!(persisted, artifact);
}

#[test]
fn entity_run_cmbs_cli_summary_uses_artifact_backed_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "run",
            fixture("tests/fixtures/entity/cmbs/small_book/observations.csv")
                .to_str()
                .expect("fixture path"),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml")
                .to_str()
                .expect("strategy path"),
            "--registry",
            registry.to_str().expect("registry path"),
            "--work-dir",
            work_dir.to_str().expect("work dir path"),
            "--emit",
            "summary",
            "--no-witness",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("canon_entity_run.v0"))
        .stdout(predicate::str::contains("profile=cmbs_tenant_label"))
        .stdout(predicate::str::contains("candidate_pairs="));

    let run_json: Value = read_json(&work_dir.join("run.json"));
    assert_eq!(run_json["version"], CANON_ENTITY_RUN_VERSION);
    assert_eq!(
        run_json["summary"]["labels"]["profile_id"],
        "cmbs_tenant_label"
    );
}

#[test]
fn entity_run_cmbs_refusal_keeps_work_dir_next_command() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let refusal = run_entity_workbench(EntityRunRequest {
        rows: &fixture("tests/fixtures/entity/cmbs/small_book/observations.csv"),
        profile: "cmbs_tenant_label",
        strategy: &temp.path().join("missing.yaml"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect_err("missing strategy refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityStrategy);
    assert_eq!(refusal.detail["stage"], "strategy");
    assert_eq!(refusal.detail["work_dir"], work_dir.display().to_string());
    assert!(
        refusal
            .next_command
            .as_deref()
            .expect("next command")
            .contains("--work-dir")
    );
}

fn assert_stage(
    artifact: &EntityRunArtifact,
    stage_name: &str,
    version: &str,
    required_upstreams: &[EntityArtifactReference],
) {
    let stage = stage(artifact, stage_name);
    assert_eq!(stage.version, version);
    assert!(stage.artifact_content_hash.starts_with("blake3:"));
    for upstream in required_upstreams {
        assert!(
            stage.upstream_artifacts.contains(upstream),
            "{stage_name} stage records upstream {upstream:?}"
        );
    }
}

fn stage<'a>(artifact: &'a EntityRunArtifact, stage_name: &str) -> &'a EntityRunStageArtifact {
    artifact
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == stage_name)
        .expect("stage exists")
}

fn stage_ref(stage: &EntityRunStageArtifact) -> EntityArtifactReference {
    EntityArtifactReference {
        version: stage.version.clone(),
        content_hash: stage.artifact_content_hash.clone(),
    }
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"CMBS run test registry","updated":"2026-06-25","entry_count":8}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&serde_json::json!([
            {"input":"Sears","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"SEARS LLC","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"Sears Roebuck & Co.","canonical_id":"TNT-SEARS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 Hour Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HOUR FITNESS USA, INC.","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"24 HR Fitness","canonical_id":"TNT-24-HOUR-FITNESS","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 Sand Island Prop","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"},
            {"input":"238 SAND ISLAND PROPERTY LLC","canonical_id":"TNT-238-SAND-ISLAND-PROPERTY","canonical_type":"tenant_label","rule_id":"CMBS_ALIAS"}
        ]))
        .expect("aliases json"),
    )
    .expect("aliases");
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}
