#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1,
        CANON_ENTITY_RUN_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1,
        block::EntityBlockStageRequest,
        block_artifact::BlockCandidateArtifact,
        edge::{EdgeEvidenceRecord, EntityEvidenceStageRequest},
        edge_artifact::EdgeEvidenceArtifact,
        run::{
            EntityRunRequest, run_entity_block_stage, run_entity_evidence_stage,
            run_entity_solve_stage, run_entity_workbench,
        },
        score::ScoreLane,
        solve::{EntitySolveStageRequest, SolveArtifact, SolveReconciliationState},
    },
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[test]
fn manual_artifact_backed_stages_match_run_stage_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("work");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");

    let block = run_entity_block_stage(EntityBlockStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual block stage");
    let evidence = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual evidence stage");
    let solve = run_entity_solve_stage(EntitySolveStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        evidence: &work_dir.join("evidence/evidence.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual solve stage");
    let manual_block_bytes = fs::read(work_dir.join("block/block.json")).unwrap();
    let manual_candidate_bytes = fs::read(work_dir.join("block/candidates.jsonl")).unwrap();
    let manual_diagnostics_bytes = fs::read(work_dir.join("block/diagnostics.json")).unwrap();
    let manual_bucket_bytes = fs::read(work_dir.join("block/exact_buckets.jsonl")).unwrap();
    let manual_evidence_bytes = fs::read(work_dir.join("evidence/evidence.json")).unwrap();
    let manual_evidence_record_bytes = fs::read(work_dir.join("evidence/evidence.jsonl")).unwrap();
    let manual_solve_bytes = fs::read(work_dir.join("solve/solve.json")).unwrap();

    let run = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("full run");

    let run_block_bytes = fs::read(work_dir.join("block/block.json")).unwrap();
    let run_candidate_bytes = fs::read(work_dir.join("block/candidates.jsonl")).unwrap();
    let run_diagnostics_bytes = fs::read(work_dir.join("block/diagnostics.json")).unwrap();
    let run_bucket_bytes = fs::read(work_dir.join("block/exact_buckets.jsonl")).unwrap();
    let run_evidence_bytes = fs::read(work_dir.join("evidence/evidence.json")).unwrap();
    let run_evidence_record_bytes = fs::read(work_dir.join("evidence/evidence.jsonl")).unwrap();
    let run_solve_bytes = fs::read(work_dir.join("solve/solve.json")).unwrap();
    assert_eq!(manual_block_bytes, run_block_bytes);
    assert_eq!(manual_candidate_bytes, run_candidate_bytes);
    assert_eq!(manual_diagnostics_bytes, run_diagnostics_bytes);
    assert_eq!(manual_bucket_bytes, run_bucket_bytes);
    assert_eq!(manual_evidence_bytes, run_evidence_bytes);
    assert_eq!(manual_evidence_record_bytes, run_evidence_record_bytes);
    assert_eq!(manual_solve_bytes, run_solve_bytes);

    let run_block: BlockCandidateArtifact = read_json(&work_dir.join("block/block.json"));
    let run_edge: EdgeEvidenceArtifact = read_json(&work_dir.join("evidence/evidence.json"));
    let run_solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    let evidence_record_count = run_edge.summary.counts["evidence_records"];

    assert_eq!(block.artifact, run_block);
    assert_eq!(
        block.artifact.candidate_diagnostics_path,
        "block/diagnostics.json"
    );
    assert!(
        block
            .artifact
            .candidate_diagnostics_hash
            .starts_with("blake3:")
    );
    assert!(work_dir.join("block/diagnostics.json").exists());
    let run_evidence_value: Value = read_json(&work_dir.join("evidence/evidence.json"));
    assert_eq!(
        run_evidence_value["evidence_records_path"],
        "evidence/evidence.jsonl"
    );
    assert_eq!(
        run_evidence_value["summary"]["counts"]["evidence_records"],
        Value::from(evidence_record_count)
    );
    assert!(run_evidence_value.get("edge_records_path").is_none());
    assert!(run_evidence_value.get("edge_records_hash").is_none());
    assert!(
        run_evidence_value["summary"]["counts"]
            .as_object()
            .expect("evidence summary counts")
            .get("edge_records")
            .is_none()
    );

    let run_artifact_value = serde_json::to_value(&run.artifact).expect("run artifact value");
    assert_eq!(
        run_artifact_value["work_dir"]["evidence_artifact_path"],
        "evidence/evidence.json"
    );
    assert_eq!(
        run_artifact_value["work_dir"]["evidence_records_path"],
        "evidence/evidence.jsonl"
    );
    assert_eq!(
        run_artifact_value["summary"]["counts"]["evidence_records"],
        Value::from(evidence_record_count)
    );
    assert!(
        run_artifact_value["work_dir"]
            .as_object()
            .expect("work_dir object")
            .get("edge_artifact_path")
            .is_none()
    );
    assert!(
        run_artifact_value["work_dir"]
            .as_object()
            .expect("work_dir object")
            .get("edge_records_path")
            .is_none()
    );
    assert!(
        run_artifact_value["summary"]["counts"]
            .as_object()
            .expect("run summary counts")
            .get("edge_records")
            .is_none()
    );
    let stage_artifacts = run_artifact_value["stage_artifacts"]
        .as_array()
        .expect("stage_artifacts array");
    assert!(
        stage_artifacts
            .iter()
            .any(|stage| stage["stage"] == "evidence")
    );
    assert!(!stage_artifacts.iter().any(|stage| stage["stage"] == "edge"));

    assert_eq!(evidence.artifact, run_edge);
    assert_eq!(solve.artifact, run_solve);

    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_BLOCK_VERSION_V1
            && stage.artifact_content_hash == run_block.artifact_content_hash
    }));
    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_EVIDENCE_VERSION_V1
            && stage.artifact_content_hash == run_edge.artifact_content_hash
    }));
    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_SOLVE_VERSION_V1
            && stage.artifact_content_hash == run_solve.artifact_content_hash
    }));
    assert_eq!(run.artifact.version, CANON_ENTITY_RUN_VERSION_V1);
    assert!(work_dir.join("run/run.json").exists());
    assert!(!work_dir.join("run.json").exists());
}

#[test]
fn evidence_stage_uses_committed_block_payload_when_candidate_mirror_is_stale() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    let block = run_entity_block_stage(EntityBlockStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual block stage");

    fs::write(work_dir.join("block/candidates.jsonl"), b"").expect("stale payload write");
    let evidence = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("committed block payload remains authoritative");

    assert_eq!(
        fs::read(work_dir.join("block/candidates.jsonl")).unwrap(),
        b""
    );
    assert_eq!(evidence.candidate_records, block.candidates);
    assert_eq!(
        evidence.artifact.candidate_records_hash,
        block.artifact.candidate_records_hash
    );
    assert!(work_dir.join("evidence/evidence.json").exists());
}

#[test]
fn evidence_stage_uses_committed_block_diagnostics_when_diagnostics_mirror_is_stale() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    let block = run_entity_block_stage(EntityBlockStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual block stage");

    let diagnostics_path = work_dir.join("block/diagnostics.json");
    let mut diagnostics: Value = read_json(&diagnostics_path);
    let stale_count = diagnostics["candidate_record_count"]
        .as_u64()
        .expect("diagnostics candidate count")
        + 1;
    diagnostics["candidate_record_count"] = Value::from(stale_count);
    fs::write(
        &diagnostics_path,
        serde_json::to_vec(&diagnostics).expect("diagnostics serialize"),
    )
    .expect("stale diagnostics write");

    let evidence = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("committed block diagnostics remain authoritative");

    let retained_diagnostics: Value = read_json(&diagnostics_path);
    assert_eq!(retained_diagnostics["candidate_record_count"], stale_count);
    assert_eq!(evidence.candidate_records, block.candidates);
    assert_eq!(
        evidence.artifact.candidate_records_hash,
        block.artifact.candidate_records_hash
    );
    assert!(work_dir.join("evidence/evidence.json").exists());
}

#[test]
fn public_manual_cli_executes_v1_block_evidence_solve_chain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual-cli");
    let block_artifact_path = work_dir.join("block/block.json");
    let evidence_artifact_path = work_dir.join("evidence/evidence.json");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");

    let block = canon_entity_command([
        "entity",
        "block",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
        "--emit",
        "jsonl",
    ]);
    assert!(block.status.success(), "block stderr: {}", stderr(&block));
    assert!(!stdout(&block).contains("entity_v1_executor_pending"));
    assert_eq!(
        read_json::<Value>(&work_dir.join("block/block.json"))["version"],
        CANON_ENTITY_BLOCK_VERSION_V1
    );
    assert!(
        stdout(&block)
            .lines()
            .any(|line| line.contains(CANON_ENTITY_BLOCK_VERSION_V1))
    );

    let evidence = canon_entity_command([
        "entity",
        "evidence",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--candidates",
        block_artifact_path.to_str().expect("block artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
        "--emit",
        "jsonl",
    ]);
    assert!(
        evidence.status.success(),
        "evidence stderr: {}",
        stderr(&evidence)
    );
    assert!(!stdout(&evidence).contains("entity_v1_executor_pending"));
    assert_eq!(
        read_json::<Value>(&evidence_artifact_path)["version"],
        CANON_ENTITY_EVIDENCE_VERSION_V1
    );
    assert!(
        stdout(&evidence)
            .lines()
            .any(|line| line.contains(CANON_ENTITY_EVIDENCE_VERSION_V1))
    );

    let evidence_summary = canon_entity_command([
        "entity",
        "evidence",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--candidates",
        block_artifact_path.to_str().expect("block artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
        "--emit",
        "summary",
    ]);
    assert!(
        evidence_summary.status.success(),
        "evidence summary stderr: {}",
        stderr(&evidence_summary)
    );
    assert!(stdout(&evidence_summary).contains("evidence_records="));
    assert!(!stdout(&evidence_summary).contains("edge_records="));
    assert!(!stdout(&evidence_summary).contains("canon entity edge"));

    let solve = canon_entity_command([
        "entity",
        "solve",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--evidence",
        evidence_artifact_path
            .to_str()
            .expect("evidence artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
        "--emit",
        "json",
    ]);
    assert!(solve.status.success(), "solve stderr: {}", stderr(&solve));
    assert!(!stdout(&solve).contains("entity_v1_executor_pending"));
    let solve_stdout: Value = serde_json::from_slice(&solve.stdout).expect("solve json");
    assert_eq!(solve_stdout["version"], CANON_ENTITY_SOLVE_VERSION_V1);
    assert_eq!(
        read_json::<Value>(&work_dir.join("solve/solve.json"))["version"],
        CANON_ENTITY_SOLVE_VERSION_V1
    );
}

#[test]
fn public_evidence_cli_refuses_legacy_block_before_output_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual-cli");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    let block = canon_entity_command([
        "entity",
        "block",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
    ]);
    assert!(block.status.success(), "block stderr: {}", stderr(&block));

    let block_artifact_path = work_dir.join("block/block.json");
    let mut block_artifact: Value = read_json(&block_artifact_path);
    block_artifact["version"] = Value::String("canon_entity_block.v0".to_string());
    fs::write(
        &block_artifact_path,
        serde_json::to_vec(&block_artifact).expect("legacy block serialize"),
    )
    .expect("legacy block write");

    let evidence = canon_entity_command([
        "entity",
        "evidence",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--candidates",
        block_artifact_path.to_str().expect("block artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        work_dir.to_str().expect("work dir"),
    ]);
    assert_eq!(evidence.status.code(), Some(2));
    assert!(!work_dir.join("evidence/evidence.json").exists());
    let refusal: Value = serde_json::from_slice(&evidence.stdout).expect("refusal json");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert!(
        refusal["refusal"]["detail"]
            .to_string()
            .contains("canon_entity_block.v0"),
        "refusal detail: {refusal}"
    );
}

#[test]
fn public_evidence_cli_refuses_tampered_v1_block_before_workdir_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let producer_work_dir = temp.path().join("producer");
    let refusal_work_dir = temp.path().join("refusal");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    let block = canon_entity_command([
        "entity",
        "block",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        producer_work_dir.to_str().expect("producer work dir"),
    ]);
    assert!(block.status.success(), "block stderr: {}", stderr(&block));

    let tampered_block_artifact_path = temp.path().join("tampered-block.json");
    let mut block_artifact: Value = read_json(&producer_work_dir.join("block/block.json"));
    block_artifact["summary"]["counts"]["candidate_pairs"] = Value::from(999_u64);
    fs::write(
        &tampered_block_artifact_path,
        serde_json::to_vec(&block_artifact).expect("tampered block serialize"),
    )
    .expect("tampered block write");

    let evidence = canon_entity_command([
        "entity",
        "evidence",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--candidates",
        tampered_block_artifact_path
            .to_str()
            .expect("tampered block artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        refusal_work_dir.to_str().expect("refusal work dir"),
    ]);
    assert_eq!(evidence.status.code(), Some(2));
    assert!(!refusal_work_dir.exists());
    let refusal: Value = serde_json::from_slice(&evidence.stdout).expect("refusal json");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "invalid_v1_self_hash"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["field"],
        "artifact_content_hash"
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
}

#[test]
fn public_solve_cli_refuses_tampered_v1_evidence_before_workdir_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let producer_work_dir = temp.path().join("producer");
    let refusal_work_dir = temp.path().join("solve-refusal");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    let block = canon_entity_command([
        "entity",
        "block",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        producer_work_dir.to_str().expect("producer work dir"),
    ]);
    assert!(block.status.success(), "block stderr: {}", stderr(&block));

    let evidence = canon_entity_command([
        "entity",
        "evidence",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--candidates",
        producer_work_dir
            .join("block/block.json")
            .to_str()
            .expect("block artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        producer_work_dir.to_str().expect("producer work dir"),
    ]);
    assert!(
        evidence.status.success(),
        "evidence stderr: {}",
        stderr(&evidence)
    );

    let tampered_evidence_artifact_path = temp.path().join("tampered-evidence.json");
    let mut evidence_artifact: Value = read_json(&producer_work_dir.join("evidence/evidence.json"));
    evidence_artifact["summary"]["counts"]["evidence_records"] = Value::from(999_u64);
    fs::write(
        &tampered_evidence_artifact_path,
        serde_json::to_vec(&evidence_artifact).expect("tampered evidence serialize"),
    )
    .expect("tampered evidence write");

    let solve = canon_entity_command([
        "entity",
        "solve",
        rows.to_str().expect("rows path"),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().expect("strategy path"),
        "--evidence",
        tampered_evidence_artifact_path
            .to_str()
            .expect("tampered evidence artifact path"),
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        refusal_work_dir.to_str().expect("refusal work dir"),
    ]);
    assert_eq!(solve.status.code(), Some(2));
    assert!(!refusal_work_dir.exists());
    let refusal: Value = serde_json::from_slice(&solve.stdout).expect("refusal json");
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "invalid_v1_self_hash"
    );
    assert_eq!(
        refusal["refusal"]["detail"]["field"],
        "artifact_content_hash"
    );
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
}

#[test]
fn full_run_wires_profile_declared_support_to_resolved_existing_incumbent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("run");
    let rows = write_support_rows(temp.path());
    let profile = write_support_profile(temp.path(), "9000", "1");
    write_support_registry(&registry);

    run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: &profile,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("full run");

    let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("evidence/evidence.json"));
    let edge_records: Vec<EdgeEvidenceRecord> =
        read_jsonl(&work_dir.join("evidence/evidence.jsonl"));
    assert!(edge.summary.counts["support_hit_count"] >= 2);
    assert!(
        edge_records
            .iter()
            .flat_map(|record| &record.hits)
            .any(|hit| {
                hit.lane == ScoreLane::Support && hit.operator_id == "string_similarity:tenant_core"
            })
    );
    assert!(
        edge_records
            .iter()
            .flat_map(|record| &record.hits)
            .any(|hit| {
                hit.lane == ScoreLane::Support && hit.operator_id == "tfidf_cosine:tenant_tokens"
            })
    );

    let solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    let resolved = solve
        .entities
        .iter()
        .find(|entity| entity.canonical_id.as_deref() == Some("TNT-ACME-COFFEE"))
        .expect("resolved incumbent component");
    assert_eq!(resolved.state, SolveReconciliationState::ResolvedExisting);
    assert_eq!(resolved.surface_ids.len(), 2);
    assert_eq!(
        resolved.incumbent_canonical_ids,
        vec!["TNT-ACME-COFFEE".to_string()]
    );
}

#[test]
fn nonpositive_support_threshold_preserves_relation_hint_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("run");
    let rows = write_support_rows(temp.path());
    let profile = write_support_profile(temp.path(), "0", "0");
    write_support_registry(&registry);

    run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: &profile,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("full run");

    let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("evidence/evidence.json"));
    let edge_records: Vec<EdgeEvidenceRecord> =
        read_jsonl(&work_dir.join("evidence/evidence.jsonl"));
    assert_eq!(edge.summary.counts["support_hit_count"], 0);
    assert!(edge.summary.counts["relation_hint_count"] > 0);
    assert!(
        !edge_records
            .iter()
            .flat_map(|record| &record.hits)
            .any(|hit| hit.lane == ScoreLane::Support)
    );
    assert!(
        edge_records
            .iter()
            .flat_map(|record| &record.hits)
            .any(|hit| hit.lane == ScoreLane::RelationHint)
    );

    let solve: SolveArtifact = read_json(&work_dir.join("solve/solve.json"));
    let incumbent = solve
        .entities
        .iter()
        .find(|entity| entity.canonical_id.as_deref() == Some("TNT-ACME-COFFEE"))
        .expect("incumbent singleton");
    assert_eq!(incumbent.surface_ids.len(), 1);
}

#[test]
fn malformed_support_threshold_refuses_before_edge_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("run");
    let rows = write_support_rows(temp.path());
    let profile = write_support_profile(temp.path(), "not-a-score", "1");
    write_support_registry(&registry);

    let refusal = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: profile.to_str().expect("profile path utf8"),
        strategy: &profile,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect_err("malformed threshold refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["operator"], "string_similarity");
    assert_eq!(refusal.detail["field"], "min_score_units");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!work_dir.join("evidence/evidence.json").exists());
}

fn write_cmbs_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"cmbs-tenants","version":"2026.06.25","description":"CMBS pipeline test registry","updated":"2026-06-25","entry_count":8}"#,
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

fn write_support_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"support-tenants","version":"2026.07.12","description":"Support edge test registry","updated":"2026-07-12","entry_count":1}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&serde_json::json!([
            {"input":"Acme Coffee","canonical_id":"TNT-ACME-COFFEE","canonical_type":"tenant_label","rule_id":"TEST_ALIAS"}
        ]))
        .expect("aliases json"),
    )
    .expect("aliases");
}

fn write_support_rows(base: &Path) -> PathBuf {
    let rows = base.join("support_rows.csv");
    fs::write(
        &rows,
        "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n\
support:001,D001,L001,P001,Acme Coffee\n\
support:002,D002,L002,P002,Acme Coffee Shop\n",
    )
    .expect("support rows");
    rows
}

fn write_support_profile(base: &Path, string_threshold: &str, tfidf_threshold: &str) -> PathBuf {
    let profile = base.join("support_profile.yaml");
    fs::write(
        &profile,
        format!(
            r#"profile: cmbs_tenant_label
version: 0.1.0
entity_type: tenant_label
identity_semantics: canonical_display_label
canonical_type: tenant_label
required_fields:
  - source_row_id
  - deal_id
  - loan_id
  - property_id
  - raw_tenant_name
normalized_views:
  tenant_core:
    operators:
      - unicode_fold
      - lowercase
      - strip_tenant_noise
      - strip_legal_suffixes
      - normalize_whitespace
  tenant_tokens:
    operators:
      - unicode_fold
      - lowercase
      - tokenize
      - drop_tenant_stopwords
  tenant_brand:
    operators:
      - unicode_fold
      - lowercase
      - tenant_brand_fingerprint
      - normalize_whitespace
evidence:
  support:
    - op: exact_view
      view: tenant_core
    - op: string_similarity
      view: tenant_core
      params:
        metric: jaro_winkler
        min_score_units: "{string_threshold}"
    - op: tfidf_cosine
      view: tenant_tokens
      params:
        min_score_units: "{tfidf_threshold}"
        top_k: "10"
        candidate_cap: "10"
  cannot_link:
    - op: protected_token_conflict
      view: tenant_tokens
  relation_hints:
    - op: related_brand_family
      view: tenant_brand
      params:
        merge_authorized: "false"
        review_policy: relation_hint_only
patch_namespaces:
  aliases: cmbs_tenant_label.aliases
  distinct: cmbs_tenant_label.distinct
  relations: cmbs_tenant_label.relations
"#,
        ),
    )
    .expect("support profile");
    profile
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .expect("jsonl bytes")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl record parses"))
        .collect()
}

fn canon_entity_command<const N: usize>(args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(args)
        .output()
        .expect("canon command runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
