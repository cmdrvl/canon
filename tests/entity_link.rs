#![forbid(unsafe_code)]

use canon::{
    entity::{
        CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
        CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactReference,
        publication::{
            CANON_ENTITY_STAGE_PUBLICATION_VERSION, EntityPublicationErrorKind,
            open_current_stream_generation, publication_object_path,
        },
        run::{
            ENTITY_RUN_PUBLICATION_STREAM_ID, EntityRunArtifact,
            link::{
                ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION, ENTITY_LINK_VERSION,
                EntityLinkArtifact, EntityLinkRequest, EntityLinkRole,
                LINK_OBSERVATION_SURFACE_BINDINGS_PATH, LINK_SIDE_COLUMN, materialized_rows_path,
                observation_surface_bindings_path,
                read_derivation_validated_entity_link_observation_surface_bindings_at_path,
                read_validated_entity_link_observation_surface_bindings_at_path, run_entity_link,
                validate_entity_link_artifact_at_path,
                validate_entity_link_observation_surface_bindings,
            },
        },
    },
    resolve::{ResolveNativeEntityLinkRequest, run_native_entity_link},
    witness,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const ENTITY_LINK_PRODUCTION_SOURCES: &[&str] = &[
    "src/entity/link.rs",
    "src/entity/multisource.rs",
    "src/resolve/assertions.rs",
    "src/resolve/gold.rs",
    "src/resolve/graph.rs",
    "src/resolve/mod.rs",
    "src/resolve/output.rs",
    "src/resolve/scoring.rs",
    "src/resolve/strategy.rs",
    "src/resolve/tape.rs",
    "src/resolve/types.rs",
    "src/resolve/writeback.rs",
];

const FORBIDDEN_DOMAIN_TOKENS: &[&str] = &[
    "cmbs", "bdc", "servicer", "loan", "deal", "tranche", "cusip", "isin", "figi", "borrower",
    "mortgage",
];

#[test]
fn entity_link_production_sources_remain_domain_neutral() {
    let forbidden = FORBIDDEN_DOMAIN_TOKENS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut violations = Vec::new();

    for relative_path in ENTITY_LINK_PRODUCTION_SOURCES {
        let source = fs::read_to_string(fixture_path(relative_path)).expect("source file readable");
        collect_domain_token_violations(relative_path, &source, &forbidden, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "generic entity-link production sources contain forbidden domain tokens:\n{}",
        violations.join("\n")
    );
}

fn collect_domain_token_violations(
    relative_path: &str,
    source: &str,
    forbidden: &BTreeSet<&str>,
    violations: &mut Vec<String>,
) {
    for (line_index, line) in source.lines().enumerate() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        collect_line_domain_token_violations(
            relative_path,
            line_index + 1,
            line,
            forbidden,
            violations,
        );
    }
}

fn collect_line_domain_token_violations(
    relative_path: &str,
    line_number: usize,
    line: &str,
    forbidden: &BTreeSet<&str>,
    violations: &mut Vec<String>,
) {
    let mut token = String::new();
    let mut token_start_column = 1;

    for (index, character) in line
        .char_indices()
        .chain(std::iter::once((line.len(), ' ')))
    {
        if character.is_ascii_alphanumeric() || character == '_' {
            if token.is_empty() {
                token_start_column = index + 1;
            }
            token.push(character.to_ascii_lowercase());
        } else if !token.is_empty() {
            if forbidden.contains(token.as_str()) {
                violations.push(format!(
                    "{relative_path}:{line_number}:{token_start_column}: token `{token}`"
                ));
            }
            token.clear();
        }
    }
}

#[test]
fn entity_link_reuses_native_run_stages_and_preserves_direction() {
    let fixture = LinkFixture::new();
    let result = run_entity_link(EntityLinkRequest {
        reference_rows: &fixture.reference,
        target_rows: &fixture.target,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        registry: &fixture.registry,
        work_dir: &fixture.work_dir,
    })
    .expect("entity link runs shared native stages");

    assert_eq!(result.artifact.version, ENTITY_LINK_VERSION);
    assert_eq!(result.artifact.reference.role, EntityLinkRole::Reference);
    assert_eq!(result.artifact.target.role, EntityLinkRole::Target);
    assert_eq!(result.artifact.reference.row_count, 1);
    assert_eq!(result.artifact.target.row_count, 1);
    assert_eq!(result.run.artifact.version, CANON_ENTITY_RUN_VERSION_V1);
    assert_eq!(
        result.artifact.shared_run_artifact.content_hash,
        result.run.artifact.artifact_content_hash
    );
    assert!(result.run.artifact.stage_artifacts.iter().any(|stage| {
        stage.stage == "evidence"
            && stage.version == CANON_ENTITY_EVIDENCE_VERSION_V1
            && stage.path == "evidence/evidence.json"
    }));
    assert!(result.run.artifact.stage_artifacts.iter().any(|stage| {
        stage.stage == "solve"
            && stage.version == CANON_ENTITY_SOLVE_VERSION_V1
            && stage.path == "solve/solve.json"
    }));

    let combined = fs::read_to_string(materialized_rows_path(&fixture.work_dir))
        .expect("materialized rows exist");
    assert!(combined.lines().next().unwrap().contains(LINK_SIDE_COLUMN));
    assert!(combined.contains("reference"));
    assert!(combined.contains("target"));
}

#[test]
fn entity_link_native_artifacts_feed_candidate_recall_without_sidecar() {
    let fixture = LinkFixture::with_candidate_handoff_rows();
    let result = run_entity_link(EntityLinkRequest {
        reference_rows: &fixture.reference,
        target_rows: &fixture.target,
        profile: "cmbs_tenant_label",
        strategy: &fixture.strategy,
        registry: &fixture.registry,
        work_dir: &fixture.work_dir,
    })
    .expect("entity link runs shared native stages");
    let run = &result.run.artifact;

    assert_eq!(
        run.work_dir.candidate_records_path,
        "block/candidates.jsonl"
    );
    assert_eq!(
        run.work_dir.candidate_diagnostics_path,
        "block/diagnostics.json"
    );
    let candidates_path = fixture.work_dir.join(&run.work_dir.candidate_records_path);
    let diagnostics_path = fixture
        .work_dir
        .join(&run.work_dir.candidate_diagnostics_path);
    assert!(candidates_path.exists());
    assert!(diagnostics_path.exists());
    let candidate_jsonl = fs::read_to_string(&candidates_path).expect("native candidate jsonl");
    assert!(!candidate_jsonl.trim_start().starts_with('['));
    let first_candidate: Value = serde_json::from_str(
        candidate_jsonl
            .lines()
            .find(|line| !line.trim().is_empty())
            .expect("at least one native candidate"),
    )
    .expect("candidate record json");
    let left_surface_id = first_candidate["left_surface_id"]
        .as_str()
        .expect("candidate left surface");
    let right_surface_id = first_candidate["right_surface_id"]
        .as_str()
        .expect("candidate right surface");
    let block: Value =
        serde_json::from_slice(&fs::read(fixture.work_dir.join("block/block.json")).unwrap())
            .expect("block artifact json");
    let exact_bucket_count = block["summary"]["counts"]["exact_bucket_count"]
        .as_u64()
        .expect("exact bucket count");
    let exact_bucket_count_arg = exact_bucket_count.to_string();
    let manifest_path = fixture.work_dir.join("native-recall-manifest.json");
    let manifest = json!({
        "observations": [
            { "observation_id": left_surface_id },
            { "observation_id": right_surface_id }
        ],
        "quality_harness": {
            "cases": [
                {
                    "case_id": "case.native.link",
                    "left_observation_id": left_surface_id,
                    "right_observation_id": right_surface_id,
                    "stratum": "withheld_alias",
                    "label_disposition": "same_entity"
                }
            ]
        }
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec(&manifest).expect("manifest serializes"),
    )
    .expect("write native recall manifest");

    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "candidate-recall",
            "--manifest",
            path_str(&manifest_path),
            "--candidates",
            path_str(&candidates_path),
            "--diagnostics",
            path_str(&diagnostics_path),
            "--exact-bucket-count",
            exact_bucket_count_arg.as_str(),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("candidate recall report");

    assert_eq!(report["version"], "canon_entity_candidate_recall.v0");
    assert_eq!(report["total_gold_pairs"], 1);
    assert_eq!(
        report["exact_buckets"]["exact_bucket_count"],
        exact_bucket_count
    );
    assert_eq!(report["union_recall_at_k"][4]["hits"], 1);
    assert_eq!(report["union_recall_at_k"][4]["total"], 1);
}

#[test]
fn entity_link_cli_emits_hash_bound_observation_surface_sidecar() {
    let fixture = LinkFixture::new();
    let artifact_value = run_entity_link_cli_json(&fixture);
    let artifact: EntityLinkArtifact =
        serde_json::from_value(artifact_value.clone()).expect("typed link artifact");
    let run_artifact = read_run_artifact(&fixture);
    assert_eq!(
        artifact.shared_run_artifact.content_hash,
        run_artifact.artifact_content_hash
    );
    let derivation_run_artifact = resealed_typed_run_artifact(run_artifact);
    let derivation_artifact =
        link_artifact_for_typed_run_derivation(artifact.clone(), &derivation_run_artifact);
    let link_path = fixture.work_dir.join("link/link.json");
    let bindings_path = observation_surface_bindings_path(&fixture.work_dir);
    let binding_bytes = fs::read(&bindings_path).expect("observation/surface sidecar");
    let binding_text = String::from_utf8(binding_bytes.clone()).expect("sidecar utf8");

    assert_eq!(
        artifact_value["observation_surface_bindings_path"],
        LINK_OBSERVATION_SURFACE_BINDINGS_PATH
    );
    assert_eq!(
        artifact_value["observation_surface_bindings_content_hash"],
        witness::hash_bytes(&binding_bytes)
    );
    validate_entity_link_artifact_at_path(&artifact, &link_path)
        .expect("link artifact validates sidecar path, hash, and coverage");
    assert!(!binding_text.contains("North Harbor"));
    assert!(!binding_text.contains("Labs"));

    let bindings =
        read_validated_entity_link_observation_surface_bindings_at_path(&artifact, &link_path)
            .expect("typed sidecar helper validates and returns bindings");
    let derived_bindings =
        read_derivation_validated_entity_link_observation_surface_bindings_at_path(
            &derivation_artifact,
            &link_path,
            &derivation_run_artifact,
        )
        .expect("derivation helper validates and returns bindings");
    assert_eq!(derived_bindings, bindings);
    assert_eq!(bindings.len(), 2);
    validate_entity_link_observation_surface_bindings(&artifact, &bindings)
        .expect("bindings cover decisions");

    let reference = bindings
        .iter()
        .find(|binding| binding.side == EntityLinkRole::Reference)
        .expect("reference binding");
    let target = bindings
        .iter()
        .find(|binding| binding.side == EntityLinkRole::Target)
        .expect("target binding");

    assert_eq!(
        reference.version,
        ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION
    );
    assert_eq!(reference.link_id, "L1");
    assert_eq!(reference.source_row_id.as_deref(), Some("ref-1"));
    assert_eq!(reference.source_ordinal, 1);
    assert!(reference.surface_id.starts_with("surf:"));
    assert_ne!(reference.surface_id, reference.link_id);
    assert_ne!(
        reference.surface_id,
        reference.source_row_id.as_deref().unwrap()
    );

    assert_eq!(
        target.version,
        ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION
    );
    assert_eq!(target.link_id, "L2");
    assert_eq!(target.source_row_id.as_deref(), Some("tgt-1"));
    assert_eq!(target.source_ordinal, 1);
    assert!(target.surface_id.starts_with("surf:"));
    assert_ne!(target.surface_id, target.link_id);
    assert_ne!(target.surface_id, target.source_row_id.as_deref().unwrap());
}

#[test]
fn entity_link_derivation_reader_rejects_resealed_forged_sidecar() {
    let fixture = LinkFixture::new();
    let artifact_value = run_entity_link_cli_json(&fixture);
    let mut artifact: EntityLinkArtifact =
        serde_json::from_value(artifact_value).expect("typed link artifact");
    let run_artifact = read_run_artifact(&fixture);
    let derivation_run_artifact = resealed_typed_run_artifact(run_artifact);
    artifact = link_artifact_for_typed_run_derivation(artifact, &derivation_run_artifact);
    let detached = tempfile::tempdir().expect("detached tempdir");
    let detached_work_dir = detached.path().join("work");
    copy_detached_link_validation_bundle(
        &fixture.work_dir,
        &detached_work_dir,
        &derivation_run_artifact,
    );
    let link_path = detached_work_dir.join("link/link.json");
    let bindings_path = observation_surface_bindings_path(&detached_work_dir);
    fs::write(
        &link_path,
        serde_json::to_vec(&artifact).expect("artifact serializes"),
    )
    .expect("write detached link artifact");
    let mut bindings =
        read_validated_entity_link_observation_surface_bindings_at_path(&artifact, &link_path)
            .expect("baseline sidecar validates");
    let target_index = bindings
        .iter()
        .position(|binding| binding.side == EntityLinkRole::Target)
        .expect("target binding");
    bindings[target_index].surface_id.push_str(":forged");
    bindings[target_index].surface_binding_hash =
        witness::hash_bytes(bindings[target_index].surface_id.as_bytes());
    let mut forged_sidecar = bindings
        .iter()
        .map(|binding| serde_json::to_string(binding).expect("binding serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    forged_sidecar.push('\n');
    fs::write(&bindings_path, forged_sidecar).expect("write forged sidecar");
    let forged_sidecar_bytes = fs::read(&bindings_path).expect("read forged sidecar");
    artifact.observation_surface_bindings_content_hash = witness::hash_bytes(&forged_sidecar_bytes);
    reseal_link_artifact(&mut artifact);
    fs::write(
        &link_path,
        serde_json::to_vec(&artifact).expect("artifact serializes"),
    )
    .expect("write resealed artifact");

    validate_entity_link_artifact_at_path(&artifact, &link_path)
        .expect("ordinary self/hash validation accepts resealed sidecar");
    let refusal = read_derivation_validated_entity_link_observation_surface_bindings_at_path(
        &artifact,
        &link_path,
        &derivation_run_artifact,
    )
    .expect_err("derivation validation rejects forged sidecar");
    assert_eq!(refusal.detail["field"], "observation_surface_bindings");
    assert_eq!(refusal.detail["reason"], "derivation_mismatch");
}

#[test]
fn entity_link_sidecar_tamper_refuses_at_artifact_validation() {
    let fixture = LinkFixture::new();
    let artifact_value = run_entity_link_cli_json(&fixture);
    let mut false_claim_artifact: EntityLinkArtifact =
        serde_json::from_value(artifact_value).expect("typed link artifact");
    false_claim_artifact.observation_surface_bindings_content_hash =
        witness::hash_bytes(b"false observation/surface binding claim");
    reseal_link_artifact(&mut false_claim_artifact);

    let refusal = validate_entity_link_artifact_at_path(
        &false_claim_artifact,
        &fixture.work_dir.join("link/link.json"),
    )
    .expect_err("resealed false sidecar hash claim refuses");
    assert_eq!(
        refusal.detail["field"],
        "observation_surface_bindings_content_hash"
    );

    let committed_bindings_path = format!("link/{LINK_OBSERVATION_SURFACE_BINDINGS_PATH}");
    let current =
        open_current_stream_generation(&fixture.work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
            .expect("current entity-run publication opens");
    let bindings_record = current
        .manifest
        .files
        .iter()
        .find(|record| record.logical_path == committed_bindings_path)
        .expect("committed observation/surface bindings record");
    let object_path = publication_object_path(&fixture.work_dir, &bindings_record.content_hash)
        .expect("publication object path");
    let mut object_bytes = fs::read(&object_path).expect("committed sidecar object");
    object_bytes.push(b'\n');
    fs::write(&object_path, object_bytes).expect("corrupt committed sidecar object");

    let error = open_current_stream_generation(&fixture.work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
        .expect_err("committed publication object hash mismatch refuses");
    assert_eq!(error.kind, EntityPublicationErrorKind::HashMismatch);
    assert!(!error.writes_performed);
}

#[test]
fn entity_link_sidecar_validation_rejects_missing_decision_target_and_duplicates() {
    let fixture = LinkFixture::new();
    let artifact_value = run_entity_link_cli_json(&fixture);
    let artifact: EntityLinkArtifact =
        serde_json::from_value(artifact_value).expect("typed link artifact");
    let link_path = fixture.work_dir.join("link/link.json");
    let mut bindings =
        read_validated_entity_link_observation_surface_bindings_at_path(&artifact, &link_path)
            .expect("typed sidecar helper returns bindings for mutation tests");

    let target_index = bindings
        .iter()
        .position(|binding| binding.side == EntityLinkRole::Target)
        .expect("target binding");
    bindings[target_index].link_id = "not-the-decision-target".to_string();
    let refusal = validate_entity_link_observation_surface_bindings(&artifact, &bindings)
        .expect_err("target decision coverage refuses");
    assert_eq!(refusal.detail["field"], "observation_surface_bindings");

    let mut duplicate_bindings =
        read_validated_entity_link_observation_surface_bindings_at_path(&artifact, &link_path)
            .expect("typed sidecar helper returns bindings for duplicate test");
    duplicate_bindings.push(duplicate_bindings[0].clone());
    let refusal = validate_entity_link_observation_surface_bindings(&artifact, &duplicate_bindings)
        .expect_err("duplicate binding refuses");
    assert_eq!(refusal.detail["field"], "observation_surface_bindings");
}

#[test]
fn resolve_native_entity_link_bridge_uses_entity_link_adapter() {
    let fixture = LinkFixture::new();
    let result = run_native_entity_link(ResolveNativeEntityLinkRequest {
        reference_tape: fixture.reference.clone(),
        target_tape: fixture.target.clone(),
        profile: "cmbs_tenant_label".to_string(),
        strategy: fixture.strategy.clone(),
        registry: fixture.registry.clone(),
        work_dir: fixture.work_dir.clone(),
    })
    .expect("resolve bridge delegates to native entity link");

    assert_eq!(result.artifact.version, ENTITY_LINK_VERSION);
    assert_eq!(result.artifact.reference.row_count, 1);
    assert_eq!(result.artifact.target.row_count, 1);
    assert_eq!(result.run.artifact.version, CANON_ENTITY_RUN_VERSION_V1);
}

#[test]
fn entity_link_cli_runs_native_adapter_instead_of_scaffold_refusal() {
    let fixture = LinkFixture::new();
    let artifact = run_entity_link_cli_json(&fixture);

    assert_eq!(artifact["version"], ENTITY_LINK_VERSION);
    assert_eq!(artifact["mode"], "directional_two_tape");
    assert_eq!(artifact["reference"]["role"], "reference");
    assert_eq!(artifact["target"]["role"], "target");
    assert_eq!(artifact["reference"]["row_count"], 1);
    assert_eq!(artifact["target"]["row_count"], 1);
    assert_ne!(
        artifact["refusal"]["detail"]["reason"],
        "entity_v1_executor_pending"
    );
}

fn run_entity_link_cli_json(fixture: &LinkFixture) -> Value {
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            path_str(&fixture.reference),
            path_str(&fixture.target),
            "--profile",
            "cmbs_tenant_label",
            "--strategy",
            path_str(&fixture.strategy),
            "--registry",
            path_str(&fixture.registry),
            "--work-dir",
            path_str(&fixture.work_dir),
            "--no-witness",
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    serde_json::from_slice(&output).expect("entity link artifact json")
}

fn read_run_artifact(fixture: &LinkFixture) -> EntityRunArtifact {
    serde_json::from_slice(&fs::read(fixture.work_dir.join("run/run.json")).expect("run artifact"))
        .expect("typed run artifact")
}

fn resealed_typed_run_artifact(mut artifact: EntityRunArtifact) -> EntityRunArtifact {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let content_hash = witness::hash_bytes(
        &serde_json::to_vec(&artifact).expect("hashable run artifact serializes"),
    );
    artifact.artifact_content_hash = content_hash.clone();
    artifact.metadata.artifact_content_hash = content_hash;
    artifact
}

fn link_artifact_for_typed_run_derivation(
    mut artifact: EntityLinkArtifact,
    run_artifact: &EntityRunArtifact,
) -> EntityLinkArtifact {
    let publication_parent = existing_publication_parent(&artifact);
    artifact.shared_run_artifact = EntityArtifactReference {
        version: run_artifact.version.clone(),
        content_hash: run_artifact.artifact_content_hash.clone(),
    };
    artifact.shared_solve_artifact = solve_stage_reference(run_artifact);
    artifact.metadata.upstream_artifacts = vec![
        artifact.shared_run_artifact.clone(),
        artifact.shared_solve_artifact.clone(),
        publication_parent,
    ];
    artifact
        .metadata
        .upstream_artifacts
        .sort_by(artifact_ref_cmp);
    reseal_link_artifact(&mut artifact);
    artifact
}

fn copy_detached_link_validation_bundle(
    source_work_dir: &Path,
    detached_work_dir: &Path,
    run_artifact: &EntityRunArtifact,
) {
    copy_file_to_path(
        &materialized_rows_path(source_work_dir),
        &materialized_rows_path(detached_work_dir),
    );
    copy_file_to_path(
        &observation_surface_bindings_path(source_work_dir),
        &observation_surface_bindings_path(detached_work_dir),
    );
    copy_file_to_path(
        &source_work_dir.join(&run_artifact.work_dir.surfaces_path),
        &detached_work_dir.join(&run_artifact.work_dir.surfaces_path),
    );
}

fn copy_file_to_path(source: &Path, target: &Path) {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).expect("create detached validation directory");
    }
    fs::copy(source, target).unwrap_or_else(|error| {
        panic!(
            "copy {} to {} for detached validation bundle: {error}",
            source.display(),
            target.display()
        )
    });
}

fn existing_publication_parent(artifact: &EntityLinkArtifact) -> EntityArtifactReference {
    let parents = artifact
        .metadata
        .upstream_artifacts
        .iter()
        .filter(|reference| reference.version == CANON_ENTITY_STAGE_PUBLICATION_VERSION)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        parents.len(),
        1,
        "link artifact should carry exactly one committed publication parent"
    );
    parents.into_iter().next().unwrap()
}

fn solve_stage_reference(run_artifact: &EntityRunArtifact) -> EntityArtifactReference {
    let stage = run_artifact
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == "solve")
        .expect("run artifact carries solve stage");
    EntityArtifactReference {
        version: stage.version.clone(),
        content_hash: stage.artifact_content_hash.clone(),
    }
}

fn artifact_ref_cmp(
    left: &EntityArtifactReference,
    right: &EntityArtifactReference,
) -> std::cmp::Ordering {
    left.version
        .cmp(&right.version)
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn reseal_link_artifact(artifact: &mut EntityLinkArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let content_hash = witness::hash_bytes(
        &serde_json::to_vec(artifact).expect("hashable link artifact serializes"),
    );
    artifact.artifact_content_hash = content_hash.clone();
    artifact.metadata.artifact_content_hash = content_hash;
}

struct LinkFixture {
    _temp: tempfile::TempDir,
    reference: PathBuf,
    target: PathBuf,
    registry: PathBuf,
    strategy: PathBuf,
    work_dir: PathBuf,
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

impl LinkFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let reference = temp.path().join("reference.csv");
        let target = temp.path().join("target.csv");
        let registry = temp.path().join("registry");
        let strategy = temp.path().join("strategy.yaml");
        let work_dir = temp.path().join("work");

        fs::write(
            &reference,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\nref-1,D1,L1,P1,North Harbor Labs,,[]\n",
        )
        .expect("reference rows");
        fs::write(
            &target,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\ntgt-1,D2,L2,P2,North Harbor Labs LLC,,[]\n",
        )
        .expect("target rows");
        write_registry(&registry);
        fs::write(
            &strategy,
            r#"strategy_id: entity-link-fixture.v1
strategy_version: "1.0.0"
entity_type: tenant_label
description: "Domain-neutral entity link integration fixture"
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [loan_id]
candidate_filter: []
assertions:
  - field_ref: mention_surfaces_json
    field_tgt: mention_surfaces_json
    op: exact
    weight: 1.0
    required: true
match_threshold: 0.75
ambiguity_gap: 0.10
max_candidates: 10
"#,
        )
        .expect("strategy");

        Self {
            _temp: temp,
            reference,
            target,
            registry,
            strategy,
            work_dir,
        }
    }

    fn with_candidate_handoff_rows() -> Self {
        let fixture = Self::new();
        fs::write(
            &fixture.reference,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\nref-1,D1,L1,P1,North Harbor Labs,,[]\nref-2,D2,L2,P2,Blue River Studio,,[]\nref-3,D3,L3,P3,Cedar Field Works,,[]\n",
        )
        .expect("reference rows");
        fs::write(
            &fixture.target,
            "source_row_id,deal_id,loan_id,property_id,raw_tenant_name,alias_surfaces_json,mention_surfaces_json\ntgt-1,D4,L4,P4,North Harbor Labs LLC,,[]\ntgt-2,D5,L5,P5,Silver Ridge Supply,,[]\ntgt-3,D6,L6,P6,Willow Point Studio,,[]\n",
        )
        .expect("target rows");
        fixture
    }
}

fn write_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"entity-link-registry","version":"2026.07.11","description":"entity link test registry","updated":"2026-07-11","entry_count":1}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        r#"[
  {"input":"North Harbor Labs","canonical_id":"TNT-NORTH-HARBOR-LABS","canonical_type":"tenant_label","rule_id":"ENTITY_LINK_FIXTURE_INCUMBENT"}
]
"#,
    )
    .expect("aliases");
}
