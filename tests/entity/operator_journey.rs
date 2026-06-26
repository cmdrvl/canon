#![forbid(unsafe_code)]

use canon::entity::{
    lint::{
        EntityLintRequest, EntityProfilePresenceCheck, EntityRuntimeGuardCheck,
        lint_entity_workbench,
    },
    runtime::{
        explain::explain_from_artifact_value,
        types::{EntityState, EvidenceKind, ExplainQuery, PromotionDecision},
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
        assert!(
            command.starts_with("canon entity ") || command.starts_with("cargo test "),
            "{command_name} command is not operator-runnable: {command}"
        );
        assert!(
            !command.contains("python") && !command.contains("curl "),
            "{command_name} command hides a forbidden runtime path: {command}"
        );
    }
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
            "edge",
            "solve",
            "review_export",
            "audit",
            "review_import",
            "promote",
            "apply",
            "run_wrapper"
        ]
    );
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
    assert_eq!(stage_names, ["prepare", "block", "solve", "apply"]);
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
    assert!(required.contains(&"candidates".to_string()) && !artifact.result.candidates.is_empty());
    assert!(
        required.contains(&"positive_evidence".to_string())
            && artifact
                .result
                .positive_evidence
                .iter()
                .any(|evidence| evidence.kind == EvidenceKind::Support)
    );
    assert!(
        required.contains(&"anti_merge_evidence".to_string())
            && !artifact.result.anti_merge_evidence.is_empty()
    );
    assert!(
        required.contains(&"review_decisions".to_string())
            && !artifact.result.review_decisions.is_empty()
    );
    assert!(
        required.contains(&"promotion_provenance".to_string())
            && artifact
                .result
                .promotion_provenance
                .iter()
                .any(|record| record.decision == PromotionDecision::Promote)
    );
}

fn assert_doctor_lint_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let report = lint_entity_workbench(EntityLintRequest {
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
    assert!(report.robot.retryable_after_fix);
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
