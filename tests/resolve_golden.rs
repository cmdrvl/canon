use assert_cmd::prelude::*;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const UNCHANGED_REFERENCE_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/reference_loans.link.csv";
const UNCHANGED_TARGET_TAPE: &str =
    "tests/fixtures/resolve/parity/unchanged-link/target_loans.link.csv";
const LOAN_MATCH_GOLD: &str = "tests/fixtures/resolve/gold/loan_matches.jsonl";

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn entity_link_stdout(
    work_root: &Path,
    strategy: &Path,
    extra_args: &[&str],
    exit_code: i32,
) -> Vec<u8> {
    let reference = Path::new(UNCHANGED_REFERENCE_TAPE);
    let target = Path::new(UNCHANGED_TARGET_TAPE);
    let work_dir = work_root.join("entity-link-work");
    let mut args = vec![
        "entity",
        "link",
        reference.to_str().unwrap(),
        target.to_str().unwrap(),
        "--profile",
        "cmbs_tenant_label",
        "--strategy",
        strategy.to_str().unwrap(),
        "--registry",
        "tests/fixtures/registries/resolve-servicers",
        "--work-dir",
        work_dir.to_str().unwrap(),
        "--no-witness",
    ];
    args.extend_from_slice(extra_args);
    let output = canon_command()
        .args(args)
        .assert()
        .code(exit_code)
        .get_output()
        .clone();
    if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    }
}

#[test]
fn minimal_legacy_loan_strategy_refuses_native_profile_cutover() {
    let temp_dir = tempfile::tempdir().unwrap();
    let stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/minimal.valid.yaml"),
        &[],
        2,
    );
    let refusal: Value = serde_json::from_slice(&stdout).unwrap();

    assert_legacy_strategy_input_contract_refusal(&refusal, "minimal-loan-match.v1");
}

#[test]
fn legacy_cmbs_cutover_refusal_is_byte_stable_for_same_inputs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extra = ["--gold", LOAN_MATCH_GOLD, "--cache-mode", "disabled"];

    let first = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &extra,
        2,
    );
    let second = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &extra,
        2,
    );

    assert_eq!(first, second);
    let refusal: Value = serde_json::from_slice(&first).unwrap();
    assert_legacy_strategy_input_contract_refusal(&refusal, "cmbs-loan-match.v1");
}

#[test]
fn unchanged_input_legacy_golden_is_refused_not_reinterpreted_as_v1_decisions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &["--gold", LOAN_MATCH_GOLD],
        2,
    );
    let refusal: Value = serde_json::from_slice(&stdout).unwrap();

    assert_legacy_strategy_input_contract_refusal(&refusal, "cmbs-loan-match.v1");
}

#[test]
fn json_and_summary_modes_agree_on_legacy_cutover_refusal_code() {
    let temp_dir = tempfile::tempdir().unwrap();
    let json_stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &[],
        2,
    );
    let summary_stdout = entity_link_stdout(
        temp_dir.path(),
        Path::new("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml"),
        &["--emit", "summary"],
        2,
    );
    let json_refusal: Value = serde_json::from_slice(&json_stdout).unwrap();
    let summary_refusal: Value = serde_json::from_slice(&summary_stdout).unwrap();

    assert_eq!(json_refusal["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    assert_eq!(
        summary_refusal["refusal"]["code"],
        "E_ENTITY_INPUT_CONTRACT"
    );
    assert_eq!(json_refusal["refusal"], summary_refusal["refusal"]);
}

fn assert_legacy_strategy_input_contract_refusal(public: &Value, strategy_id: &str) {
    assert_eq!(public["outcome"], "REFUSAL");
    assert_eq!(public["refusal"]["code"], "E_ENTITY_INPUT_CONTRACT");
    let next_command = public["refusal"]["next_command"]
        .as_str()
        .expect("actionable next command");
    assert!(
        next_command.contains("entity_type 'loan'"),
        "{next_command}"
    );
    assert!(next_command.contains("cmbs_tenant_label"), "{next_command}");
    let detail = &public["refusal"]["detail"];
    assert_eq!(detail["stage"], "link");
    assert_eq!(detail["field"], "profile.entity_type");
    assert_eq!(detail["profile_source"], "cmbs_tenant_label");
    assert_eq!(detail["expected"]["strategy_entity_type"], "loan");
    assert_eq!(detail["expected"]["strategy_id"], strategy_id);
    assert_eq!(detail["expected"]["strategy_version"], "0.1.0");
    assert!(
        detail["expected"]["strategy_content_hash"].is_string(),
        "strategy hash must be present"
    );
    assert_eq!(detail["actual"]["profile_entity_type"], "tenant_label");
    assert_eq!(detail["actual"]["profile_id"], "cmbs_tenant_label");
    assert_eq!(detail["actual"]["profile_version"], "0.1.0");
    assert!(
        detail["actual"]["profile_content_hash"].is_string(),
        "profile hash must be present"
    );
    assert_eq!(detail["writes_performed"], false);
}
