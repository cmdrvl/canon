use canon::{
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_INDEX_VERSION,
        CANON_ENTITY_PREPARE_VERSION, CANON_ENTITY_SOLVE_VERSION, EntityArtifactReference,
        run::{EntityRunArtifact, EntityRunHandoffStep, EntityRunRequest, run_entity_workbench},
    },
    registry::RegistryRef,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn cmbs_run_orchestration_orders_review_audit_promote_apply_handoffs() {
    let RunFixture {
        artifact,
        registry,
        rows,
        work_dir,
        _temp,
    } = run_cmbs_fixture();

    assert_eq!(
        artifact.orchestration.stage_order,
        [
            "prepare",
            "index",
            "block",
            "edge",
            "solve",
            "review_export",
            "audit",
            "review_import",
            "promote",
            "apply"
        ]
    );

    let handoff_order = artifact
        .orchestration
        .handoff_steps
        .iter()
        .map(|step| step.stage.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        handoff_order,
        ["review_export", "audit", "review_import", "promote", "apply"]
    );

    let solve_path = work_dir.join("solve/solve.json").display().to_string();
    let review_path = work_dir.join("review.csv").display().to_string();
    let audit_path = work_dir.join("audit.json").display().to_string();
    let decision_ledger_path = work_dir
        .join("solve/decision_ledger.jsonl")
        .display()
        .to_string();
    let promote_path = work_dir.join("promote.json").display().to_string();
    let sidecar_path = work_dir
        .join("promotion-sidecars.json")
        .display()
        .to_string();
    let apply_path = work_dir.join("apply.csv").display().to_string();
    let registry_path = registry.display().to_string();
    let rows_path = rows.display().to_string();

    let steps = handoff_steps_by_stage(&artifact);
    assert_handoff(
        steps["review_export"],
        &solve_path,
        &[solve_ref(&artifact)],
        &["solve"],
        &[],
        &[review_path.as_str()],
        false,
    );
    assert!(steps["review_export"].command.contains("review export"));
    assert!(steps["review_export"].command.contains("--include escrow"));

    let stage_refs = artifact
        .stage_artifacts
        .iter()
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    assert_handoff(
        steps["audit"],
        &solve_path,
        &stage_refs,
        &["solve"],
        &[],
        &[audit_path.as_str()],
        false,
    );
    assert!(steps["audit"].command.contains("entity audit"));

    assert_handoff(
        steps["review_import"],
        &review_path,
        &[solve_ref(&artifact)],
        &["review_export", "audit"],
        &[review_path.as_str(), audit_path.as_str()],
        &[decision_ledger_path.as_str()],
        true,
    );
    assert!(steps["review_import"].command.contains("review import"));
    assert!(steps["review_import"].command.contains("--audit"));

    assert_handoff(
        steps["promote"],
        &solve_path,
        &[solve_ref(&artifact)],
        &["audit", "review_import"],
        &[audit_path.as_str()],
        &[promote_path.as_str(), sidecar_path.as_str()],
        true,
    );
    assert!(steps["promote"].command.contains("entity promote"));
    assert!(steps["promote"].command.contains("--audit"));

    assert_handoff(
        steps["apply"],
        &rows_path,
        &[solve_ref(&artifact)],
        &["promote"],
        &[registry_path.as_str(), sidecar_path.as_str()],
        &[apply_path.as_str()],
        true,
    );
    assert!(steps["apply"].command.contains("entity apply"));

    for step in artifact.orchestration.handoff_steps {
        assert!(
            step.enforces_profile_firewall,
            "{} handoff must enforce the profile firewall",
            step.stage
        );
    }
}

#[test]
fn artifact_chain_continuity_cmbs_run_profiles_hashes_and_paths() {
    let RunFixture {
        artifact,
        registry,
        work_dir,
        _temp,
        ..
    } = run_cmbs_fixture();

    let firewall = artifact.orchestration.profile_firewall.clone();
    assert_eq!(firewall.profile_id, "cmbs_tenant_label");
    assert_eq!(firewall.profile_version, "0.1.0");
    assert_eq!(firewall.identity_semantics, "canonical_display_label");
    assert_eq!(firewall.canonical_type, "tenant_label");
    assert_eq!(firewall.registry_id, "cmbs-tenants");
    assert_eq!(firewall.registry_version, "2026.06.25");
    assert!(firewall.registry_snapshot_hash.starts_with("blake3:"));
    assert!(firewall.strategy_hash.starts_with("blake3:"));

    let solve = solve_ref(&artifact);
    assert_eq!(solve.version, CANON_ENTITY_SOLVE_VERSION);
    assert!(solve.content_hash.starts_with("blake3:"));

    let stage_versions = artifact
        .stage_artifacts
        .iter()
        .map(|stage| stage.version.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        stage_versions,
        [
            CANON_ENTITY_PREPARE_VERSION,
            CANON_ENTITY_INDEX_VERSION,
            CANON_ENTITY_BLOCK_VERSION,
            CANON_ENTITY_EDGE_VERSION,
            CANON_ENTITY_SOLVE_VERSION
        ]
    );

    for stage in &artifact.stage_artifacts {
        assert!(
            Path::new(&stage.path).is_relative(),
            "stage path must remain work-dir relative: {}",
            stage.path
        );
        assert!(
            work_dir.join(&stage.path).exists(),
            "stage artifact path exists: {}",
            stage.path
        );
        assert!(stage.artifact_content_hash.starts_with("blake3:"));
    }

    let persisted: EntityRunArtifact = read_json(&work_dir.join("run.json"));
    assert_eq!(persisted.orchestration, artifact.orchestration);

    let registry_ref = RegistryRef::load(&registry).expect("registry ref loads");
    assert_eq!(registry_ref.id, firewall.registry_id);
    assert_eq!(registry_ref.version, firewall.registry_version);
}

fn assert_handoff(
    step: &EntityRunHandoffStep,
    input_path: &str,
    input_artifacts: &[EntityArtifactReference],
    required_prior_stages: &[&str],
    required_paths: &[&str],
    output_paths: &[&str],
    requires_audit: bool,
) {
    assert_eq!(step.input_artifact_path, input_path);
    assert_eq!(step.input_artifacts, input_artifacts);
    assert_eq!(
        step.required_prior_stages,
        strings(required_prior_stages),
        "{} prior stages",
        step.stage
    );
    assert_eq!(
        step.required_paths,
        strings(required_paths),
        "{} required paths",
        step.stage
    );
    assert_eq!(
        step.output_paths,
        strings(output_paths),
        "{} output paths",
        step.stage
    );
    assert_eq!(step.requires_audit, requires_audit);
}

fn handoff_steps_by_stage<'a>(
    artifact: &'a EntityRunArtifact,
) -> BTreeMap<&'a str, &'a EntityRunHandoffStep> {
    artifact
        .orchestration
        .handoff_steps
        .iter()
        .map(|step| (step.stage.as_str(), step))
        .collect()
}

fn solve_ref(artifact: &EntityRunArtifact) -> EntityArtifactReference {
    artifact
        .stage_artifacts
        .iter()
        .find(|stage| stage.version == CANON_ENTITY_SOLVE_VERSION)
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .expect("solve stage reference")
}

struct RunFixture {
    _temp: tempfile::TempDir,
    artifact: EntityRunArtifact,
    registry: PathBuf,
    rows: PathBuf,
    work_dir: PathBuf,
}

fn run_cmbs_fixture() -> RunFixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let result = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("entity run succeeds");

    RunFixture {
        _temp: temp,
        artifact: result.artifact,
        registry,
        rows,
        work_dir,
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

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}
