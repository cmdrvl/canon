#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_AUDIT_VERSION_V1, CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1,
    CANON_ENTITY_EXPLAIN_VERSION_V1, CANON_ENTITY_REVIEW_VERSION_V1, CANON_ENTITY_RUN_VERSION_V1,
    CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactStageV1,
    publication::{EntityPublicationFileInput, EntityPublicationRequest, publish_stream_patch},
    schema::{
        CANON_ENTITY_REVIEW_IMPORT_VERSION, entity_v1_contract_for_stage,
        entity_v1_schema_content_hash, entity_v1_schema_reference, entity_v1_workdir_layout,
        finalize_entity_v1_self_hash,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn entity_v1_lifecycle_cli_review_audit_promote_refusal_explain_and_unchanged_lookup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = solve_v1_artifact(&work);
    let result_path = temp.path().join("solve.v1.json");
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
    assert_eq!(review["summary"]["counts"]["review_items"], 2);
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
    assert_eq!(imported["version"], CANON_ENTITY_REVIEW_IMPORT_VERSION);
    assert_eq!(imported["summary"]["labels"]["stage"], "review_import");
    assert_eq!(
        imported["summary"]["labels"]["operation"],
        "default_queue_import"
    );

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

    let refusal = run_promote_expect_refusal(&result_path, &audit_path, &registry, "2026.06.26");
    assert_unreviewed_promote_refusal(&refusal, &result_path, &registry, "2026.06.26");
    assert_registry_unchanged(&registry, "2026.06.25");

    let explain_json = canon_cmd()
        .args([
            "entity",
            "explain",
            path_str(&result_path),
            "--canon-id",
            "TNT-SEARS",
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
    assert_eq!(explain["result"]["selector"]["value"], "TNT-SEARS");

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
        .code(1)
        .get_output()
        .stdout
        .clone();
    let resolved: Value = serde_json::from_slice(&resolved).expect("resolve json");
    assert_eq!(resolved["outcome"], "UNRESOLVED");
    assert_eq!(resolved["summary"]["resolved"], 0);
    assert_eq!(resolved["summary"]["unresolved"], 1);
}

#[test]
fn entity_v1_promote_refuses_tampered_result_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let result_path = temp.path().join("run.v1.json");
    write_json(&result_path, &result);
    let audit = audit_for_result(temp.path(), &result_path);
    let audit_path = temp.path().join("audit.v1.json");
    write_json(&audit_path, &audit);

    let mut tampered = result;
    tampered["summary"]["counts"]["entity_count"] = json!(2);
    let tampered_path = temp.path().join("run.tampered.v1.json");
    write_json(&tampered_path, &tampered);

    let refusal = run_promote_expect_refusal(&tampered_path, &audit_path, &registry, "2026.06.26");
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "artifact_content_hash");
    assert_eq!(refusal["detail"]["writes_performed"], false);
    assert_registry_unchanged(&registry, "2026.06.25");
}

#[test]
fn entity_v1_promote_refuses_tampered_audit_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let result_path = temp.path().join("run.v1.json");
    write_json(&result_path, &result);
    let mut audit = audit_for_result(temp.path(), &result_path);
    audit["summary"]["counts"]["gate_count"] = json!(99);
    let audit_path = temp.path().join("audit.tampered.v1.json");
    write_json(&audit_path, &audit);

    let refusal = run_promote_expect_refusal(&result_path, &audit_path, &registry, "2026.06.26");
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["artifact_role"], "audit");
    assert_eq!(refusal["detail"]["writes_performed"], false);
    assert_eq!(
        refusal["detail"]["source_detail"]["field"],
        "artifact_content_hash"
    );
    assert_registry_unchanged(&registry, "2026.06.25");
}

#[test]
fn entity_v1_promote_refuses_unreviewed_solve_artifact_alias_proposals() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = solve_v1_artifact(&work);
    let result_path = temp.path().join("solve.v1.json");
    write_json(&result_path, &result);
    let audit = audit_for_result(temp.path(), &result_path);
    let audit_path = temp.path().join("audit.v1.json");
    write_json(&audit_path, &audit);

    let refusal = run_promote_expect_refusal(&result_path, &audit_path, &registry, "2026.06.26");
    assert_unreviewed_promote_refusal(&refusal, &result_path, &registry, "2026.06.26");
    assert_registry_unchanged(&registry, "2026.06.25");
}

#[test]
fn entity_v1_promote_refuses_unreviewed_run_bound_solve_file_alias_proposals() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let result_path = temp.path().join("run.v1.json");
    write_json(&result_path, &result);
    let audit = audit_for_result(temp.path(), &result_path);
    let audit_path = temp.path().join("audit.v1.json");
    write_json(&audit_path, &audit);

    let refusal = run_promote_expect_refusal(&result_path, &audit_path, &registry, "2026.06.26");
    assert_unreviewed_promote_refusal(&refusal, &result_path, &registry, "2026.06.26");
    assert_registry_unchanged(&registry, "2026.06.25");
}

#[test]
fn entity_v1_promote_refuses_malformed_sibling_link_state_without_registry_writes() {
    for case in ["missing_link_json", "directory_link_json"] {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let work = temp.path().join("work");
        fs::create_dir_all(&registry).expect("registry dir");
        fs::create_dir_all(&work).expect("work dir");
        write_registry(&registry, "2026.06.25");

        let result = run_v1_artifact(&work);
        let expected_reason = match case {
            "missing_link_json" => {
                fs::create_dir_all(work.join("link")).expect("link dir");
                "incomplete_sibling_link_workdir"
            }
            "directory_link_json" => {
                fs::create_dir_all(work.join("link/link.json")).expect("link artifact dir");
                "malformed_sibling_link_artifact"
            }
            _ => unreachable!("covered cases"),
        };

        let refusal = promote_refusal_for_result(temp.path(), &registry, result);
        assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT", "{case}");
        assert_eq!(refusal["detail"]["field"], "link_artifact", "{case}");
        assert_eq!(refusal["detail"]["reason"], expected_reason, "{case}");
        assert_eq!(refusal["detail"]["writes_performed"], false, "{case}");
        assert_registry_unchanged(&registry, "2026.06.25");
    }
}

#[test]
fn entity_v1_promote_refuses_stable_link_signal_without_committed_link() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let solve = read_json(&work.join("solve/solve.json"));
    fs::create_dir_all(work.join("run")).expect("run dir");
    write_json(&work.join("run/run.json"), &result);
    publish_committed_run_solve(&work, &result, &solve);
    fs::create_dir_all(work.join("link")).expect("link dir");
    fs::write(work.join("link/link.json"), b"{}\n").expect("stable link signal");

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "link_artifact");
    assert_eq!(
        refusal["detail"]["reason"],
        "committed_link_artifact_missing"
    );
    assert_eq!(refusal["detail"]["stable_link_signal"], true);
    assert_eq!(refusal["detail"]["writes_performed"], false);
    assert_registry_unchanged(&registry, "2026.06.25");
}

#[test]
fn entity_v1_promote_refuses_binary_emitted_unreviewed_run_bound_solve_alias_proposals() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    let rows = write_support_rows(temp.path());
    let profile = write_support_profile(temp.path(), "9000", "1");
    write_support_registry(&registry);

    let emitted_run = canon_cmd()
        .args([
            "entity",
            "run",
            path_str(&rows),
            "--profile",
            path_str(&profile),
            "--strategy",
            path_str(&profile),
            "--registry",
            path_str(&registry),
            "--work-dir",
            path_str(&work),
            "--cache-mode",
            "disabled",
            "--emit",
            "json",
            "--no-witness",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let emitted_run: Value = serde_json::from_slice(&emitted_run).expect("emitted run json");
    let result_path = work.join("run/run.json");
    let run = read_json(&result_path);
    assert_eq!(
        run["artifact_content_hash"],
        emitted_run["artifact_content_hash"]
    );
    let solve_path = work.join(
        run["work_dir"]["solve_artifact_path"]
            .as_str()
            .expect("solve path string"),
    );
    let solve = read_json(&solve_path);
    assert_binary_run_binds_solve(&run, &solve);
    assert_eq!(solve["promotable_aliases"][0]["input"], "Acme Coffee Shop");
    assert_eq!(
        solve["promotable_aliases"][0]["canonical_id"],
        "TNT-ACME-COFFEE"
    );

    let audit = audit_for_result(temp.path(), &result_path);
    let audit_path = temp.path().join("audit.v1.json");
    write_json(&audit_path, &audit);
    let before = registry_snapshot(&registry);
    let mut tampered_stable_solve = solve.clone();
    tampered_stable_solve["summary"]["counts"]["entity_count"] = json!(2);
    write_json(&solve_path, &tampered_stable_solve);
    let refusal = run_promote_expect_refusal(&result_path, &audit_path, &registry, "2026.07.13");
    assert_unreviewed_promote_refusal(&refusal, &result_path, &registry, "2026.07.13");
    assert_eq!(before, registry_snapshot(&registry));

    let lookup = temp.path().join("lookup.csv");
    fs::write(&lookup, "tenant\nAcme Coffee Shop\n").expect("lookup rows");
    let resolved = canon_cmd()
        .args([
            path_str(&lookup),
            "--registry",
            path_str(&registry),
            "--column",
            "tenant",
            "--explicit",
            "--no-witness",
        ])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let resolved: Value = serde_json::from_slice(&resolved).expect("resolve json");
    assert_eq!(resolved["outcome"], "UNRESOLVED");
    assert_eq!(resolved["summary"]["resolved"], 0);
    assert_eq!(resolved["summary"]["unresolved"], 1);
}

#[test]
fn entity_v1_promote_refuses_untyped_alias_arrays_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    result["promotable_aliases"] = json!([{
        "input": "Sears",
        "canonical_id": "TNT-SEARS",
        "canonical_type": "tenant_label",
        "rule_id": "ENTITY_V1_PROMOTE"
    }]);
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "promotable_aliases");
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_fallback_entities_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    result
        .as_object_mut()
        .expect("run object")
        .remove("promotable_aliases");
    result["entities"] = json!([{
        "canonical_id": "TNT-SEARS",
        "canonical_type": "tenant_label",
        "alias_inputs": ["Sears"]
    }]);
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "entities.alias_inputs");
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_bad_alias_proposal_contract_without_registry_writes() {
    for mutation in [
        BadProposalMutation::ContentHash,
        BadProposalMutation::ProposalId,
        BadProposalMutation::AllowedActions,
        BadProposalMutation::CanonicalType,
        BadProposalMutation::SourceSurface,
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let registry = temp.path().join("registry");
        let work = temp.path().join("work");
        fs::create_dir_all(&registry).expect("registry dir");
        fs::create_dir_all(&work).expect("work dir");
        write_registry(&registry, "2026.06.25");

        let mut result = solve_v1_artifact(&work);
        mutate_solve_proposal(&mut result, mutation);

        let refusal = promote_refusal_for_result(temp.path(), &registry, result);
        assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
        assert_eq!(refusal["detail"]["field"], "solve_artifact");
        assert_eq!(refusal["detail"]["writes_performed"], false);
        assert_eq!(
            refusal["detail"]["source_detail"]["writes_performed"],
            false
        );
    }
}

#[test]
fn entity_v1_promote_refuses_missing_run_bound_solve_file_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    fs::remove_file(work.join("solve/solve.json")).expect("remove bound solve");

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "work_dir.solve_artifact_path");
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_tampered_run_bound_solve_file_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let result = run_v1_artifact(&work);
    let mut solve = read_json(&work.join("solve/solve.json"));
    solve["summary"]["counts"]["entity_count"] = json!(2);
    write_json(&work.join("solve/solve.json"), &solve);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["detail"]["field"],
        "solve_artifact.artifact_content_hash"
    );
    assert_eq!(refusal["detail"]["writes_performed"], false);
    assert_eq!(
        refusal["detail"]["source_detail"]["field"],
        "artifact_content_hash"
    );
}

#[test]
fn entity_v1_promote_refuses_run_solve_path_traversal_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    result["work_dir"]["solve_artifact_path"] = json!("../solve.json");
    result["stage_artifacts"][0]["path"] = json!("../solve.json");
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "work_dir.solve_artifact_path");
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_moved_run_solve_path_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    fs::copy(work.join("solve/solve.json"), work.join("solve/moved.json")).expect("move copy");
    result["work_dir"]["solve_artifact_path"] = json!("solve/moved.json");
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "stage_artifacts.solve.path");
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_run_solve_hash_mismatch_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    result["stage_artifacts"][0]["artifact_content_hash"] = json!("blake3:wrong");
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(
        refusal["detail"]["field"],
        "stage_artifacts.solve.artifact_content_hash"
    );
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[test]
fn entity_v1_promote_refuses_duplicate_run_solve_refs_without_registry_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = temp.path().join("registry");
    let work = temp.path().join("work");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::create_dir_all(&work).expect("work dir");
    write_registry(&registry, "2026.06.25");

    let mut result = run_v1_artifact(&work);
    let duplicate = result["stage_artifacts"][0].clone();
    result["stage_artifacts"]
        .as_array_mut()
        .expect("stage artifacts")
        .push(duplicate);
    set_v1_hash(&mut result);

    let refusal = promote_refusal_for_result(temp.path(), &registry, result);
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "stage_artifacts.solve");
    assert_eq!(refusal["detail"]["actual_count"], 2);
    assert_eq!(refusal["detail"]["writes_performed"], false);
}

#[derive(Debug, Clone, Copy)]
enum BadProposalMutation {
    ContentHash,
    ProposalId,
    AllowedActions,
    CanonicalType,
    SourceSurface,
}

fn canon_cmd() -> assert_cmd::Command {
    assert_cmd::cargo::cargo_bin_cmd!("canon")
}

fn audit_for_result(temp: &Path, result_path: &Path) -> Value {
    let suite = temp.join("suite");
    fs::create_dir_all(&suite).expect("suite dir");
    let audit_json = canon_cmd()
        .args([
            "entity",
            "audit",
            path_str(result_path),
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
    serde_json::from_slice(&audit_json).expect("audit json")
}

fn run_promote_expect_refusal(
    result_path: &Path,
    audit_path: &Path,
    registry: &Path,
    next_version: &str,
) -> Value {
    let output = canon_cmd()
        .args([
            "entity",
            "promote",
            path_str(result_path),
            "--audit",
            path_str(audit_path),
            "--registry",
            path_str(registry),
            "--next-version",
            next_version,
            "--emit",
            "json",
        ])
        .output()
        .expect("promote command runs");
    assert_eq!(output.status.code(), Some(2));
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("refusal json");
    assert_eq!(envelope["outcome"], "REFUSAL");
    envelope["refusal"].clone()
}

fn assert_unreviewed_promote_refusal(
    refusal: &Value,
    result_path: &Path,
    registry: &Path,
    next_version: &str,
) {
    assert_eq!(refusal["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert_eq!(refusal["detail"]["field"], "promotable_aliases");
    assert_eq!(refusal["detail"]["reason"], "reviewed_acceptance_required");
    assert_eq!(refusal["detail"]["proposal_count"], 1);
    assert_eq!(refusal["detail"]["writes_performed"], false);
    let review_export = refusal["detail"]["review_export_command"]
        .as_str()
        .expect("review export handoff");
    assert!(review_export.contains("canon entity review export"));
    assert!(review_export.contains(&result_path.display().to_string()));
    let review_import = refusal["detail"]["review_import_command"]
        .as_str()
        .expect("review import handoff");
    assert!(review_import.contains("canon entity review import"));
    assert!(review_import.contains(&registry.display().to_string()));
    assert!(review_import.contains(next_version));
    let next_command = refusal["next_command"]
        .as_str()
        .expect("unreviewed promote next command");
    assert!(next_command.contains(review_export));
    assert!(next_command.contains(review_import));
}

fn assert_registry_unchanged(registry: &Path, version: &str) {
    assert_eq!(
        read_json(&registry.join("registry.json"))["version"],
        version
    );
    assert_eq!(
        fs::read_to_string(registry.join("aliases.json")).expect("aliases reads"),
        "[]\n"
    );
}

fn assert_binary_run_binds_solve(run: &Value, solve: &Value) {
    assert_eq!(run["version"], CANON_ENTITY_RUN_VERSION_V1);
    assert_eq!(solve["version"], CANON_ENTITY_SOLVE_VERSION_V1);
    let solve_hash = solve["artifact_content_hash"]
        .as_str()
        .expect("solve hash string");
    assert_eq!(run["work_dir"]["solve_artifact_path"], "solve/solve.json");
    let solve_stages = run["stage_artifacts"]
        .as_array()
        .expect("stage artifacts array")
        .iter()
        .filter(|stage| stage["stage"] == "solve")
        .collect::<Vec<_>>();
    assert_eq!(solve_stages.len(), 1);
    let solve_stage = solve_stages[0];
    assert_eq!(solve_stage["version"], CANON_ENTITY_SOLVE_VERSION_V1);
    assert_eq!(solve_stage["path"], run["work_dir"]["solve_artifact_path"]);
    assert_eq!(solve_stage["artifact_content_hash"], solve_hash);

    let solve_metadata_refs = run["metadata"]["upstream_artifacts"]
        .as_array()
        .expect("metadata upstream refs array")
        .iter()
        .filter(|reference| {
            reference["version"] == CANON_ENTITY_SOLVE_VERSION_V1
                && reference["content_hash"] == solve_hash
        })
        .count();
    assert_eq!(solve_metadata_refs, 1);
}

fn write_support_registry(registry: &Path) {
    fs::create_dir_all(registry).expect("registry dir");
    fs::write(
        registry.join("registry.json"),
        r#"{"id":"support-tenants","version":"2026.07.12","description":"Support lifecycle test registry","updated":"2026-07-12","entry_count":1}"#,
    )
    .expect("registry metadata");
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_string_pretty(&json!([
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

fn promote_refusal_for_result(temp: &Path, registry: &Path, result: Value) -> Value {
    let before = registry_snapshot(registry);
    let result_path = temp.join("result.v1.json");
    write_json(&result_path, &result);
    let audit = audit_for_result(temp, &result_path);
    let audit_path = temp.join("audit.v1.json");
    write_json(&audit_path, &audit);

    let refusal = run_promote_expect_refusal(&result_path, &audit_path, registry, "2026.06.26");
    assert_eq!(before, registry_snapshot(registry));
    refusal
}

fn registry_snapshot(registry: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    for entry in fs::read_dir(registry).expect("registry reads") {
        let path = entry.expect("registry entry").path();
        if path.is_file() {
            snapshot.insert(path.clone(), fs::read(path).expect("registry file reads"));
        }
    }
    snapshot
}

fn run_v1_artifact(work: &Path) -> Value {
    let solve = solve_v1_artifact(work);
    write_bound_solve_artifact(work, &solve);
    run_v1_artifact_with_solve(work, &solve)
}

fn run_v1_artifact_with_solve(work: &Path, solve: &Value) -> Value {
    let solve_hash = artifact_hash(solve);
    let registry_snapshot_hash = registry_snapshot_hash_for_work(work);
    let solve_ref = v1_reference(
        EntityArtifactStageV1::Solve,
        CANON_ENTITY_SOLVE_VERSION_V1,
        &solve_hash,
    );
    let stage_artifact = json!({
        "stage": "solve",
        "version": CANON_ENTITY_SOLVE_VERSION_V1,
        "path": "solve/solve.json",
        "artifact_content_hash": solve_hash,
        "upstream_artifacts": solve["metadata"]["upstream_artifacts"].clone()
    });
    let mut artifact = json!({
        "version": CANON_ENTITY_RUN_VERSION_V1,
        "artifact_content_hash": "blake3:placeholder",
        "metadata": metadata_for_stage(work, EntityArtifactStageV1::Run, vec![solve_ref]),
        "summary": {
            "counts": {
                "review_groups": 1,
                "entity_count": 1,
                "promotable_aliases": 1
            },
            "labels": {
                "stage": "run",
                "status": "completed"
            }
        },
        "run_manifest_path": "run/manifest.json",
        "stage_artifacts": [stage_artifact],
        "work_dir": {
            "prepare_artifact_path": "prepare/prepare.json",
            "surfaces_path": "prepare/surfaces.jsonl",
            "index_artifact_path": "index/index.json",
            "block_artifact_path": "block/block.json",
            "candidate_records_path": "block/candidates.jsonl",
            "candidate_diagnostics_path": "block/diagnostics.jsonl",
            "exact_bucket_assertions_path": "block/exact_bucket_assertions.json",
            "evidence_artifact_path": "evidence/evidence.json",
            "evidence_records_path": "evidence/evidence.jsonl",
            "solve_artifact_path": "solve/solve.json",
            "decision_ledger_path": "solve/decision_ledger.jsonl",
            "run_artifact_path": "run/run.json"
        },
        "next_commands": {
            "resume": "canon entity run <ROWS> --registry <REGISTRY> --work-dir <WORK>",
            "review_export": format!("canon entity review export {}", work.join("solve/solve.json").display()),
            "audit": format!("canon entity audit {} --suite <SUITE_DIR>", work.join("solve/solve.json").display()),
            "promote": format!("canon entity promote {} --audit <AUDIT.json> --registry <REGISTRY> --next-version <VERSION>", work.join("solve/solve.json").display()),
            "apply": "canon entity apply <ROWS> --registry <REGISTRY> --column <COLUMN> --out <OUT>"
        },
        "orchestration": {
            "stage_order": ["prepare", "index", "block", "evidence", "solve", "run"],
            "profile_firewall": {
                "profile_id": "cmbs_tenant_label",
                "profile_version": "0.1.0",
                "identity_semantics": "canonical_display_label",
                "canonical_type": "tenant_label",
                "registry_id": "cmbs-tenants",
                "registry_version": "2026.06.25",
                "registry_snapshot_hash": registry_snapshot_hash,
                "strategy_hash": "blake3:strategy"
            },
            "handoff_steps": []
        }
    });
    set_v1_hash(&mut artifact);
    artifact
}

fn write_bound_solve_artifact(work: &Path, solve: &Value) {
    let solve_dir = work.join("solve");
    fs::create_dir_all(&solve_dir).expect("solve dir");
    write_json(&solve_dir.join("solve.json"), solve);
}

fn publish_committed_run_solve(work: &Path, run: &Value, solve: &Value) {
    let request = EntityPublicationRequest {
        stream_id: "entity-run-stage-set".to_string(),
        supersedes_generation_id: None,
        request_fingerprint: fixture_hash("lifecycle-run-solve-publication"),
        cache_mode: "disabled".to_string(),
        cache_status: "bypassed".to_string(),
        cache_receipt_hash: fixture_hash("lifecycle-cache-receipt"),
        stage_order: vec!["block", "evidence", "solve", "run", "link"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        upstream_artifacts: Vec::new(),
        files: vec![
            EntityPublicationFileInput::new(
                "solve/solve.json",
                "solve",
                CANON_ENTITY_SOLVE_VERSION_V1,
                serde_json::to_vec_pretty(solve).expect("solve serializes"),
            ),
            EntityPublicationFileInput::new(
                "run/run.json",
                "run",
                CANON_ENTITY_RUN_VERSION_V1,
                serde_json::to_vec_pretty(run).expect("run serializes"),
            ),
        ],
        omit_logical_paths: Vec::new(),
    };
    publish_stream_patch(work, request).expect("run/solve publication commits");
}

fn solve_v1_artifact(work: &Path) -> Value {
    let upstreams = vec![
        v1_reference(
            EntityArtifactStageV1::Block,
            CANON_ENTITY_BLOCK_VERSION_V1,
            "blake3:block",
        ),
        v1_reference(
            EntityArtifactStageV1::Evidence,
            CANON_ENTITY_EVIDENCE_VERSION_V1,
            "blake3:evidence",
        ),
    ];
    let proposal = alias_proposal(
        "Sears",
        "TNT-SEARS",
        "tenant_label",
        "component:sears",
        vec!["surf:sears"],
    );
    let mut artifact = json!({
        "version": CANON_ENTITY_SOLVE_VERSION_V1,
        "artifact_content_hash": "blake3:placeholder",
        "metadata": metadata_for_stage(work, EntityArtifactStageV1::Solve, upstreams.clone()),
        "summary": {
            "counts": {
                "entity_count": 1,
                "promotable_alias_count": 1,
                "resolved_existing": 1,
                "promotable_new": 0,
                "escrow": 0,
                "contradictions": 0,
                "conflicts": 0,
                "review_group_count": 0
            },
            "labels": {
                "decision_ledger": "required_before_review_import_or_promotion"
            }
        },
        "upstream_artifacts": upstreams,
        "promotable_aliases": [proposal],
        "entities": [
            {
                "component_id": "component:sears",
                "state": "resolved_existing",
                "reason": "exact_registry_support",
                "surface_ids": ["surf:seed", "surf:sears"],
                "incumbent_canonical_ids": ["TNT-SEARS"],
                "canonical_id": "TNT-SEARS",
                "support_score_units": 10000,
                "adjusted_support_score_units": 10000,
                "hard_cannot_link_count": 0,
                "soft_anti_merge_warning_count": 0,
                "review_priority_reasons": []
            }
        ],
        "review_groups": [],
        "diagnostics": {
            "summary": {
                "component_count": 1,
                "resolved_existing": 1
            },
            "components": [
                {
                    "component_id": "component:sears",
                    "state": "resolved_existing",
                    "reason": "exact_registry_support",
                    "surface_ids": ["surf:seed", "surf:sears"],
                    "support_score_units": 10000,
                    "adjusted_support_score_units": 10000,
                    "negative_score_units": 0,
                    "score_margin_units": 10000,
                    "strongest_positive_cut": null,
                    "strongest_negative_cut": null,
                    "affected_rows": 1,
                    "affected_deals": 1,
                    "review_priority_reasons": []
                }
            ],
            "review_group_seeds": []
        },
        "decision_ledger_path": "solve/decision_ledger.jsonl"
    });
    set_v1_hash(&mut artifact);
    artifact
}

fn metadata_for_stage(work: &Path, stage: EntityArtifactStageV1, upstreams: Vec<Value>) -> Value {
    let contract = entity_v1_contract_for_stage(stage).expect("v1 stage contract");
    let registry_snapshot_hash = registry_snapshot_hash_for_work(work);
    json!({
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
            "lookup_snapshot_hash": registry_snapshot_hash
        },
        "input": {
            "row_count": 1,
            "content_hash": "blake3:input"
        },
        "patch_namespace": "cmbs_tenant_label.aliases",
        "schema": entity_v1_schema_reference(contract).expect("schema ref"),
        "workdir": entity_v1_workdir_layout(contract, work.display().to_string()),
        "upstream_artifacts": upstreams,
        "patch_set": {
            "content_hash": "blake3:patch-set",
            "paths": []
        },
        "namekit": {
            "version": "canon_entity_namekit_fixture.v0",
            "content_hash": "blake3:namekit"
        },
        "artifact_content_hash": "blake3:placeholder"
    })
}

fn registry_snapshot_hash_for_work(work: &Path) -> String {
    let registry = work
        .parent()
        .expect("work dir has temp parent")
        .join("registry");
    registry_snapshot_hash(&registry)
}

fn registry_snapshot_hash(registry: &Path) -> String {
    let mut files = Vec::new();
    for entry in fs::read_dir(registry).expect("registry dir reads") {
        let path = entry.expect("registry entry reads").path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            files.push(path);
        }
    }
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("registry file name utf-8");
        let bytes = fs::read(&path).expect("registry file reads");
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn v1_reference(stage: EntityArtifactStageV1, version: &str, content_hash: &str) -> Value {
    let contract = entity_v1_contract_for_stage(stage).expect("v1 stage contract");
    assert_eq!(contract.artifact_version, version);
    json!({
        "version": version,
        "schema_key": contract.schema_key,
        "schema_hash": entity_v1_schema_content_hash(contract).expect("schema hash"),
        "content_hash": content_hash
    })
}

fn artifact_hash(artifact: &Value) -> String {
    artifact["artifact_content_hash"]
        .as_str()
        .expect("artifact hash")
        .to_string()
}

fn fixture_hash(label: &str) -> String {
    format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
}

fn alias_proposal(
    input: &str,
    canonical_id: &str,
    canonical_type: &str,
    component_id: &str,
    source_surface_ids: Vec<&str>,
) -> Value {
    let mut proposal = json!({
        "version": "canon_entity_alias_proposal.v0",
        "proposal_id": "",
        "content_hash": "",
        "input": input,
        "canonical_id": canonical_id,
        "canonical_type": canonical_type,
        "rule_id": "entity_solve_alias_proposal",
        "component_id": component_id,
        "source_surface_ids": source_surface_ids,
        "allowed_actions": ["accept_alias", "reject_alias"]
    });
    resign_alias_proposal(&mut proposal);
    proposal
}

fn resign_alias_proposal(proposal: &mut Value) {
    let hash = alias_proposal_hash(proposal);
    proposal["content_hash"] = Value::String(hash.clone());
    proposal["proposal_id"] = Value::String(format!("alias_proposal:{hash}"));
}

fn alias_proposal_hash(proposal: &Value) -> String {
    let material = json!({
        "version": proposal["version"],
        "input": proposal["input"],
        "canonical_id": proposal["canonical_id"],
        "canonical_type": proposal["canonical_type"],
        "rule_id": proposal["rule_id"],
        "component_id": proposal["component_id"],
        "source_surface_ids": proposal["source_surface_ids"],
        "allowed_actions": proposal["allowed_actions"]
    });
    let bytes = serde_json::to_vec(&material).expect("proposal hash material serializes");
    format!("blake3:{}", blake3::hash(&bytes).to_hex())
}

fn set_v1_hash(artifact: &mut Value) {
    finalize_entity_v1_self_hash(artifact).expect("v1 self hash");
}

fn mutate_solve_proposal(result: &mut Value, mutation: BadProposalMutation) {
    let proposal = &mut result["promotable_aliases"][0];
    match mutation {
        BadProposalMutation::ContentHash => {
            proposal["content_hash"] = Value::String("blake3:wrong".to_string());
        }
        BadProposalMutation::ProposalId => {
            proposal["proposal_id"] = Value::String("alias_proposal:blake3:wrong".to_string());
        }
        BadProposalMutation::AllowedActions => {
            proposal["allowed_actions"] = json!(["accept_alias"]);
            resign_alias_proposal(proposal);
        }
        BadProposalMutation::CanonicalType => {
            proposal["canonical_type"] = Value::String("legal_entity".to_string());
            resign_alias_proposal(proposal);
        }
        BadProposalMutation::SourceSurface => {
            proposal["source_surface_ids"] = json!(["surf:other"]);
            resign_alias_proposal(proposal);
        }
    }
    set_v1_hash(result);
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
