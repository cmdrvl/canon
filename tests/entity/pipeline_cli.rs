#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION, CANON_ENTITY_SOLVE_VERSION,
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
};

#[test]
fn manual_artifact_backed_stages_match_run_stage_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let manual_work_dir = temp.path().join("manual");
    let run_work_dir = temp.path().join("run");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");

    let block = run_entity_block_stage(EntityBlockStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &manual_work_dir,
    })
    .expect("manual block stage");
    let evidence = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &manual_work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &manual_work_dir,
    })
    .expect("manual evidence stage");
    let solve = run_entity_solve_stage(EntitySolveStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        evidence: &manual_work_dir.join("edge/edge.json"),
        registry: &registry,
        work_dir: &manual_work_dir,
    })
    .expect("manual solve stage");

    let run = run_entity_workbench(EntityRunRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &run_work_dir,
    })
    .expect("full run");

    let run_block: BlockCandidateArtifact = read_json(&run_work_dir.join("block/block.json"));
    let run_edge: EdgeEvidenceArtifact = read_json(&run_work_dir.join("edge/edge.json"));
    let run_solve: SolveArtifact = read_json(&run_work_dir.join("solve/solve.json"));

    assert_eq!(
        serde_json::to_vec(&block.artifact).unwrap(),
        serde_json::to_vec(&run_block).unwrap()
    );
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
    assert_eq!(
        fs::read(manual_work_dir.join("block/diagnostics.json")).unwrap(),
        fs::read(run_work_dir.join("block/diagnostics.json")).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(&evidence.artifact).unwrap(),
        serde_json::to_vec(&run_edge).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(&solve.artifact).unwrap(),
        serde_json::to_vec(&run_solve).unwrap()
    );

    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_BLOCK_VERSION
            && stage.artifact_content_hash == block.artifact.artifact_content_hash
    }));
    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_EDGE_VERSION
            && stage.artifact_content_hash == evidence.artifact.artifact_content_hash
    }));
    assert!(run.artifact.stage_artifacts.iter().any(|stage| {
        stage.version == CANON_ENTITY_SOLVE_VERSION
            && stage.artifact_content_hash == solve.artifact.artifact_content_hash
    }));
}

#[test]
fn evidence_stage_refuses_stale_block_payload_before_edge_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    run_entity_block_stage(EntityBlockStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect("manual block stage");

    fs::write(work_dir.join("block/candidates.jsonl"), b"").expect("stale payload write");
    let refusal = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect_err("stale block payload refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "stale_candidate_records");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!work_dir.join("edge/edge.json").exists());
}

#[test]
fn evidence_stage_refuses_stale_block_diagnostics_before_edge_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work_dir = temp.path().join("manual");
    write_cmbs_registry(&registry);

    let rows = fixture("tests/fixtures/entity/cmbs/small_book/observations.csv");
    let strategy = fixture("tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
    run_entity_block_stage(EntityBlockStageRequest {
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

    let refusal = run_entity_evidence_stage(EntityEvidenceStageRequest {
        rows: &rows,
        profile: "cmbs_tenant_label",
        strategy: &strategy,
        candidates: &work_dir.join("block/block.json"),
        registry: &registry,
        work_dir: &work_dir,
    })
    .expect_err("stale diagnostics refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "stale_candidate_diagnostics");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!work_dir.join("edge/edge.json").exists());
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

    let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("edge/edge.json"));
    let edge_records: Vec<EdgeEvidenceRecord> = read_jsonl(&work_dir.join("edge/edges.jsonl"));
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

    let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("edge/edge.json"));
    let edge_records: Vec<EdgeEvidenceRecord> = read_jsonl(&work_dir.join("edge/edges.jsonl"));
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
    assert_eq!(refusal.detail["stage"], "edge");
    assert_eq!(refusal.detail["operator"], "string_similarity");
    assert_eq!(refusal.detail["field"], "min_score_units");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(!work_dir.join("edge/edge.json").exists());
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
