#![forbid(unsafe_code)]

use canon::entity::{
    lint::{
        EntityArtifactFreshnessCheck, EntityLintRequest, EntityProfilePresenceCheck,
        EntityRuntimeGuardCheck, lint_entity_workbench, render_entity_lint_summary,
    },
    runtime::{
        explain::explain_from_artifact_value,
        types::{EntityState, EvidenceKind, ExplainQuery, InheritanceMode, PromotionDecision},
    },
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const JOURNEY_MANIFEST: &str =
    include_str!("../fixtures/entity/operator_journey/journey_manifest.json");

#[test]
fn entity_operator_journey_composes_final_acceptance_fixtures() {
    let manifest = manifest();
    assert_eq!(
        manifest["schema_version"],
        "canon.entity.operator_journey.v0"
    );
    assert_eq!(
        strings(&manifest["command_order"]),
        [
            "profile_list",
            "profile_init",
            "cmbs_backfill_run",
            "sec10d_regab_run",
            "summary_robot_json",
            "explain",
            "doctor_lint",
            "runtime_guard"
        ]
    );

    assert_commands_are_operator_runnable(&manifest);
    assert_refusal_handoffs_are_actionable(&manifest);
    assert_profile_cli_journey(&manifest);
    assert_fixture_paths_exist(&manifest);
    assert_cmbs_backfill_contract(&manifest);
    assert_sec10d_regab_contract(&manifest);
    assert_summary_robot_json_contract(&manifest);
    assert_explain_contract(&manifest);
    assert_doctor_lint_contract();
}

#[test]
fn entity_runtime_guard_contract_blocks_network_model_python_runtime() {
    let manifest = manifest();
    let eval_targets = json_file(&fixture_path(
        manifest["fixture_paths"]["eval_targets"]
            .as_str()
            .expect("eval targets path"),
    ));
    let guard = &manifest["runtime_guard"];
    let eval_guard = &eval_targets["runtime_guards"];

    for key in [
        "network_access_allowed",
        "frontier_model_calls_allowed",
        "runtime_model_downloads_allowed",
        "python_ml_runtime_allowed",
        "general_ml_framework_runtime_allowed",
        "dense_embedding_service_allowed_for_large_corpora",
    ] {
        assert_eq!(guard[key], false, "manifest runtime guard {key}");
        assert_eq!(eval_guard[key], false, "eval runtime guard {key}");
    }
    assert_eq!(guard["runtime_guard_verdict_required"], true);
    assert_eq!(eval_guard["runtime_guard_verdict_required"], true);
    assert!(
        guard["next_command"]
            .as_str()
            .expect("next command")
            .contains("entity_runtime_guard")
    );

    let forbidden = strings(&guard["forbidden_runtime_paths"])
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for term in [
        "frontier model call",
        "network call",
        "runtime model download",
        "python ml runtime",
    ] {
        assert!(forbidden.contains(term), "missing runtime guard {term}");
    }

    let eval = eval_targets["scorecard_metrics"]
        .as_array()
        .expect("eval target list")
        .iter()
        .find(|entry| entry["id"] == "ER-RUNTIME-001")
        .expect("runtime eval exists");
    assert_eq!(eval["name"], "no_network_no_model_runtime_eval");
    assert_eq!(eval["required_contract_ref"], "runtime_guards");
}

fn assert_commands_are_operator_runnable(manifest: &Value) {
    let commands = manifest["commands"].as_object().expect("commands object");
    for command_name in strings(&manifest["command_order"]) {
        let command = commands
            .get(command_name.as_str())
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing command {command_name}"));
        assert_actionable_command(command);
        assert_local_runtime_command(command);
    }
}

fn assert_refusal_handoffs_are_actionable(manifest: &Value) {
    let refusals = manifest["refusals"].as_array().expect("refusal handoffs");
    assert!(!refusals.is_empty(), "journey includes refusal handoffs");
    for refusal in refusals {
        assert!(
            refusal["code"]
                .as_str()
                .expect("refusal code")
                .starts_with("E_"),
            "refusal codes remain machine-readable"
        );
        let next_command = refusal["next_command"]
            .as_str()
            .expect("refusal next command");
        assert_actionable_command(next_command);
        assert_local_runtime_command(next_command);
    }
}

fn assert_profile_cli_journey(manifest: &Value) {
    let catalog = canon_json(["entity", "profile", "list", "--emit", "json"]);
    let listed = catalog["profiles"]
        .as_array()
        .expect("profile catalog")
        .iter()
        .map(|profile| profile["profile"].as_str().expect("profile id"))
        .collect::<BTreeSet<_>>();
    for profile in ["cmbs_tenant_label", "regab_firm_identity"] {
        assert!(listed.contains(profile), "profile {profile} is listed");
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let output = temp.path().join("cmbs_tenant_label.yaml");
    let report = canon_json([
        "entity",
        "profile",
        "init",
        "cmbs_tenant_label",
        "--output",
        output.to_str().expect("profile path"),
    ]);
    assert_eq!(report["template_valid"], true);
    assert!(
        report["next_command"]
            .as_str()
            .expect("profile init next command")
            .contains("canon entity prepare")
    );
    let yaml = fs::read_to_string(output).expect("profile template exists");
    assert!(yaml.contains("canonical_display_label"));

    let init_command = manifest["commands"]["profile_init"]
        .as_str()
        .expect("profile init command");
    assert!(init_command.contains("cmbs_tenant_label"));
}

fn assert_fixture_paths_exist(manifest: &Value) {
    for path in manifest["fixture_paths"]
        .as_object()
        .expect("fixture paths")
        .values()
        .filter_map(Value::as_str)
    {
        assert!(fixture_path(path).exists(), "fixture path exists: {path}");
    }
}

fn assert_cmbs_backfill_contract(manifest: &Value) {
    let runbook = json_file(&fixture_path(path(manifest, "cmbs_backfill_runbook")));
    assert_eq!(
        runbook["schema_version"],
        "canon.entity.cmbs_backfill_runbook.v0"
    );
    assert_eq!(runbook["profile_id"], "cmbs_tenant_label");
    assert_eq!(
        runbook["production_backfill"]["logical_surface_corpus"],
        "global"
    );
    assert_eq!(
        runbook["production_backfill"]["logical_index_scope"],
        "global"
    );
    assert_eq!(runbook["production_backfill"]["registry_memory"], "global");
    assert_eq!(runbook["duplicate_mint_guard"]["canonical_id"], "TNT-SEARS");
    let stages = runbook["stage_commands"]
        .as_array()
        .expect("stage commands")
        .iter()
        .map(|stage| stage["stage"].as_str().expect("stage").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "prepare",
            "index",
            "block",
            "evidence",
            "solve",
            "review_export",
            "audit",
            "review_import",
            "promote",
            "apply",
            "run_wrapper"
        ]
    );
    for stage in runbook["stage_commands"]
        .as_array()
        .expect("stage commands")
    {
        let command = stage["command"].as_str().expect("stage command");
        assert_actionable_command(command);
        assert_local_runtime_command(command);
    }
    let log_fields = strings(&runbook["required_log_fields"]);
    for field in [
        "row_count",
        "cache_status",
        "artifact_hashes",
        "next_commands",
    ] {
        assert!(
            log_fields.contains(&field.to_string()),
            "missing log field {field}"
        );
    }
}

fn assert_sec10d_regab_contract(manifest: &Value) {
    let regab = json_file(&fixture_path(path(manifest, "sec10d_regab_manifest")));
    assert_eq!(regab["schema_version"], "canon.entity.sec10d_regab_e2e.v0");
    assert_eq!(regab["profile_id"], "regab_firm_identity");
    for id in ["REGAB-I001", "REGAB-I002", "REGAB-I003", "REGAB-I004"] {
        assert!(
            regab["assertions"]
                .as_array()
                .expect("assertions")
                .iter()
                .any(|assertion| assertion["id"] == id),
            "Reg AB journey missing {id}"
        );
    }
    let forbidden = strings(&regab["runtime_forbidden"])
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for term in [
        "frontier model call",
        "network call",
        "runtime model download",
        "python ml runtime",
        "unsafe rust",
    ] {
        assert!(forbidden.contains(term), "Reg AB missing guard {term}");
    }

    let mentions = json_file(&fixture_path(path(
        manifest,
        "regab_public_mentions_summary",
    )));
    let resolution = json_file(&fixture_path(path(
        manifest,
        "regab_public_resolution_summary",
    )));
    assert_eq!(mentions["mention_count"], 127_991);
    assert_eq!(mentions["unique_surface_count"], 46);
    assert_eq!(resolution["canon_exit"], 0);
    assert_eq!(resolution["mention_count"], 127_991);
    assert_eq!(resolution["resolved_mentions"], 127_991);
    assert_eq!(resolution["unresolved_mentions"], 0);
    assert_eq!(resolution["registry"]["registry_id"], "firms");
}

fn assert_summary_robot_json_contract(manifest: &Value) {
    let robot = json_file(&fixture_path(path(manifest, "summary_robot_projection")));
    assert_eq!(
        robot["version"],
        "canon.entity.operator_journey.summary_robot.v0"
    );
    for field in strings(&manifest["required_robot_fields"]) {
        assert!(
            robot["run"].get(field.as_str()).is_some(),
            "robot run projection missing {field}"
        );
    }
    assert_eq!(robot["run"]["next_command_key"], "review_export");
    assert!(
        strings(&robot["run"]["next_command_keys"]).contains(&"apply".to_string()),
        "robot JSON must expose apply next command"
    );
    assert!(
        strings(&robot["run"]["telemetry_link_keys"]).contains(&"solve_artifact".to_string()),
        "robot JSON must expose solve telemetry"
    );
    let stage_names = robot["stages"]
        .as_array()
        .expect("stages")
        .iter()
        .map(|stage| stage["stage"].as_str().expect("stage").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        stage_names,
        ["prepare", "index", "block", "evidence", "solve", "apply"]
    );
}

fn assert_explain_contract(manifest: &Value) {
    let bundle = json_file(&fixture_path(path(manifest, "explain_bundle")));
    let artifact = explain_from_artifact_value(
        ExplainQuery {
            surface_id: Some("surf:cmbs:sears".to_string()),
            ..ExplainQuery::default()
        },
        bundle,
    )
    .expect("explain reconstructs operator journey fixture");
    assert_eq!(artifact.result.state, EntityState::ResolvedExisting);
    assert_eq!(artifact.result.canonical_id.as_deref(), Some("TNT-SEARS"));
    assert_eq!(
        artifact
            .result
            .registry_snapshot
            .as_ref()
            .map(|snapshot| (snapshot.id.as_str(), snapshot.version.as_str())),
        Some(("cmbs-tenants", "2026.06.25"))
    );
    assert_eq!(
        artifact.result.next_action.as_deref(),
        Some("replay exact apply against the promoted registry snapshot")
    );
    assert_eq!(
        artifact.result.inheritance.mode,
        InheritanceMode::SingleIncumbentOverlap
    );

    let required = strings(&manifest["required_explain_sections"]);
    assert!(required.contains(&"normalized_views".to_string()));
    assert!(
        artifact.result.surfaces.iter().any(|surface| {
            surface
                .normalized_views
                .get("tenant_label")
                .is_some_and(|view| view == "sears")
        }),
        "explain must reconstruct normalized views"
    );
    assert!(required.contains(&"candidates".to_string()) && artifact.result.candidates.len() == 3);
    assert!(
        required.contains(&"positive_evidence".to_string())
            && artifact
                .result
                .positive_evidence
                .iter()
                .any(|evidence| evidence.kind == EvidenceKind::Support)
    );
    assert!(
        required.contains(&"relation_hints".to_string())
            && artifact
                .result
                .positive_evidence
                .iter()
                .any(|evidence| evidence.namespace == "relation_hint"
                    && evidence.operator_id == "relation_hint:dba_alias")
    );
    assert!(
        required.contains(&"anti_merge_evidence".to_string())
            && artifact.result.anti_merge_evidence.iter().any(|evidence| {
                evidence.kind == EvidenceKind::CannotLink
                    && evidence.operator_id == "cannot_link:tenant_label_scope"
            })
    );
    assert!(
        required.contains(&"review_decisions".to_string())
            && !artifact.result.review_decisions.is_empty()
    );
    assert!(
        required.contains(&"solver_decision".to_string())
            && artifact.result.state == EntityState::ResolvedExisting
    );
    assert!(
        required.contains(&"promotion_provenance".to_string())
            && artifact
                .result
                .promotion_provenance
                .iter()
                .any(|record| record.decision == PromotionDecision::Promote)
    );
    assert_eq!(
        artifact.result.promotion_provenance[0]
            .registry_version_after
            .as_deref(),
        Some("2026.06.26")
    );

    let ledger = json_file(&fixture_path(path(manifest, "ledger_expected")));
    assert!(required.contains(&"ledger_events".to_string()));
    assert_eq!(ledger["version"], "canon_entity_decision_ledger.v0");
    assert_eq!(ledger["event_version"], "decision_event.v0");
    assert_eq!(ledger["summary_counts"]["events"], 1);
    assert_eq!(ledger["summary_labels"]["ledger"], "append_only");
}

fn assert_doctor_lint_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let report = lint_entity_workbench(EntityLintRequest {
        artifacts: vec![EntityArtifactFreshnessCheck {
            stage: "solve".to_string(),
            expected_hash: "blake3:expected".to_string(),
            actual_hash: "blake3:stale".to_string(),
        }],
        profile_presence: Some(EntityProfilePresenceCheck {
            profile_id: "cmbs_tenant_label".to_string(),
            profile_path: temp.path().join("missing-profile.yaml"),
        }),
        runtime_guard: Some(EntityRuntimeGuardCheck {
            guard_id: "no_network_or_model_runtime".to_string(),
            status: "failed".to_string(),
            next_command: "Disable network/model runtime path and rerun canon entity".to_string(),
        }),
        ..EntityLintRequest::default()
    });
    assert!(!report.ok);
    assert!(report.next_command.contains("Rerun canon entity"));
    assert!(report.robot.retryable_after_fix);
    assert!(render_entity_lint_summary(&report).contains("runtime_guard_failure"));
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.category == "missing_profile")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.category == "runtime_guard_failure")
    );
    assert!(
        report
            .robot
            .commands
            .iter()
            .all(|command| !command.trim().is_empty())
    );
}

fn assert_actionable_command(command: &str) {
    assert!(!command.trim().is_empty(), "command must be non-empty");
    assert!(
        command.starts_with("canon entity ") || command.starts_with("cargo test "),
        "unexpected operator command prefix: {command}"
    );
}

fn assert_local_runtime_command(command: &str) {
    let lower = command.to_ascii_lowercase();
    for forbidden in [
        "python",
        "pip ",
        "uv ",
        "curl ",
        "wget ",
        "http://",
        "https://",
        "openai",
        "anthropic",
        "frontier_model",
        "model_download",
    ] {
        assert!(
            !lower.contains(forbidden),
            "operator command has forbidden runtime dependency token {forbidden}: {command}"
        );
    }
}

fn canon_json<const N: usize>(args: [&str; N]) -> Value {
    let output = assert_cmd::cargo::cargo_bin_cmd!("canon")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("canon json")
}

fn path<'a>(manifest: &'a Value, key: &str) -> &'a str {
    manifest["fixture_paths"][key]
        .as_str()
        .unwrap_or_else(|| panic!("missing fixture path {key}"))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|entry| entry.as_str().expect("string").to_string())
        .collect()
}

fn manifest() -> Value {
    serde_json::from_str(JOURNEY_MANIFEST).expect("journey manifest parses")
}

fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json bytes")).expect("json parses")
}

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
