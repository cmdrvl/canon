use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    thread,
};
use tempfile::tempdir;
use tiny_http::{Header, Response, Server, StatusCode};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn write_registry_metadata(temp_dir: &Path, id: &str, version: &str, entry_count: usize) {
    let registry_json = serde_json::json!({
        "id": id,
        "version": version,
        "description": "Test registry",
        "updated": "2026-01-01",
        "entry_count": entry_count,
    });
    std::fs::write(
        temp_dir.join("registry.json"),
        serde_json::to_string_pretty(&registry_json).unwrap(),
    )
    .unwrap();
}

fn write_mapping_file(temp_dir: &Path, name: &str, entries: serde_json::Value) {
    std::fs::write(
        temp_dir.join(name),
        serde_json::to_string_pretty(&entries).unwrap(),
    )
    .unwrap();
}

fn write_seed_csv(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

#[cfg(unix)]
fn shell_quote(value: impl AsRef<Path>) -> String {
    let rendered = value.as_ref().to_string_lossy();
    format!("'{}'", rendered.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn twinning_bin() -> PathBuf {
    std::env::var_os("TWINNING_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| fixture_path("../twinning/target/debug/twinning"))
}

fn write_strategy_schema(path: &Path, vendor_cardinality: u64) {
    std::fs::write(
        path,
        serde_json::to_string_pretty(&serde_json::json!({
            "columns": [
                {"name": "vendor", "type": "string", "cardinality": vendor_cardinality},
                {"name": "amount", "type": "number", "cardinality": 10}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
}

fn doctor_cmd_in(dir: &Path, witness_path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command
        .current_dir(dir)
        .env("EPISTEMIC_WITNESS", witness_path);
    command
}

fn assert_doctor_side_effects_absent(dir: &Path, witness_path: &Path) {
    assert!(!witness_path.exists());
    if let Some(parent) = witness_path.parent() {
        assert!(!parent.exists());
    }
    assert!(!dir.join(".doctor").exists());
    assert!(!dir.join(".canon-witness.jsonl").exists());
    assert!(!dir.join(".cmdrvl").exists());
    assert!(!dir.join("_index.sqlite").exists());
}

fn assert_all_side_effects_false(side_effects: &Value) {
    let object = side_effects
        .as_object()
        .expect("side_effects should be a JSON object");
    assert!(!object.is_empty());
    for (name, value) in object {
        assert_eq!(value, false, "side effect {name} should be false");
    }
}

struct ResolveSmokeFixture {
    reference: PathBuf,
    target: PathBuf,
    strategy: PathBuf,
    registry: PathBuf,
    gold: PathBuf,
}

fn write_resolve_smoke_fixture(root: &Path, matched: bool) -> ResolveSmokeFixture {
    let reference = root.join("reference.csv");
    let target = root.join("target.csv");
    let strategy = root.join("strategy.yaml");
    let registry = root.join("registry");
    let gold = root.join("gold.jsonl");
    std::fs::create_dir_all(&registry).unwrap();

    write_registry_metadata(&registry, "resolve-smoke", "0.1.0", 0);
    write_seed_csv(
        &reference,
        "loan_id,deal,address,upb\nR-1,D1,100 Main St,100\n",
    );
    let target_row = if matched {
        "D1,1,100 Main St,101\n"
    } else {
        "D1,1,999 Other St,500\n"
    };
    write_seed_csv(
        &target,
        &format!("deal,loan_number,address,balance\n{target_row}"),
    );
    std::fs::write(
        &strategy,
        r#"strategy_id: resolve-smoke.v1
strategy_version: "0.1.0"
entity_type: loan
identity:
  reference:
    id_columns: [loan_id]
  target:
    id_columns: [deal, loan_number]
candidate_filter:
  - field_ref: deal
    field_tgt: deal
    op: exact
assertions:
  - field_ref: address
    field_tgt: address
    op: exact
    weight: 0.60
    required: true
  - field_ref: upb
    field_tgt: balance
    op: tolerance_pct
    tolerance: 0.05
    weight: 0.40
    required: false
match_threshold: 0.75
ambiguity_gap: 0.10
max_candidates: 10
"#,
    )
    .unwrap();
    std::fs::write(
        &gold,
        "{\"target_id\":\"D1|1\",\"expected_reference_id\":\"R-1\"}\n",
    )
    .unwrap();

    ResolveSmokeFixture {
        reference,
        target,
        strategy,
        registry,
        gold,
    }
}

type RecordedOpenFigiRequest = (String, BTreeMap<String, String>);
type OpenFigiServerHandle = thread::JoinHandle<RecordedOpenFigiRequest>;

fn spawn_openfigi_server(response_body: String) -> (String, OpenFigiServerHandle) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v3/mapping", server.server_addr());
    let handle = thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let headers = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_string().to_ascii_lowercase(),
                    header.value.as_str().to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        let response = Response::from_string(response_body)
            .with_status_code(StatusCode(200))
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response).unwrap();
        (body, headers)
    });

    (base_url, handle)
}

#[test]
fn test_version_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--version")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn test_bare_invocation_prints_orientation() {
    // Bare `canon` orients the caller toward the canonical command and the
    // machine-readable surfaces instead of emitting a raw clap error. Exit 2
    // (no task performed), guidance on stderr, stdout clean for pipelines.
    let assert = Command::new(env!("CARGO_BIN_EXE_canon"))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    let output = assert.get_output();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(stderr.contains("--registry"));
    assert!(stderr.contains("canon doctor --robot-triage"));
    assert!(stderr.contains("canon --describe"));
}

#[test]
fn entity_namespace_cli() {
    let help = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--help")
        .assert()
        .success();
    let help_stdout = String::from_utf8(help.get_output().stdout.clone()).unwrap();
    assert!(help_stdout.contains("entity"));
    assert!(!help_stdout.contains("\n  org"));

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["entity", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("review"));

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["org", "run", "--help"])
        .assert()
        .failure();
}

#[test]
fn exact_lookup_regression_after_entity_namespace() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("exact lookup emits valid JSON");
    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
    assert_eq!(json["summary"]["total"], 3);
    assert_eq!(json["summary"]["resolved"], 3);
    assert_eq!(json["summary"]["unresolved"], 0);
    assert_eq!(json["mappings"].as_array().unwrap().len(), 3);
}

#[test]
fn test_describe_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--describe should output valid JSON");

    assert_eq!(json["name"], "canon");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["schema_version"], "operator.v0");
    assert!(json["capabilities"].is_object());
    assert!(
        json["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "resolve"
                && entry["output_schema"] == "canon_resolve.v0"
                && entry["status"] == "implemented")
    );
    assert!(
        json["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "doctor"
                && entry["status"] == "implemented"
                && entry["read_only"] == true)
    );
}

#[test]
fn test_doctor_health_json_is_read_only() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "health", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.health.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["tool"], "canon");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["summary"]["checks_failed"], 0);
    assert_eq!(
        payload["config_footprint"]["managed_state_paths"][0],
        "~/.cmdrvl/state/witness/witness.jsonl"
    );
    assert_eq!(
        payload["config_footprint"]["legacy_migration_required"],
        true
    );
    assert_eq!(payload["config_footprint"]["self_contained"], true);
    assert_eq!(
        payload["observed_paths"]["witness_ledger"],
        witness_path.display().to_string()
    );
    assert_eq!(payload["side_effects"]["opens_witness_ledger"], false);
    assert_eq!(payload["side_effects"]["appends_witness_ledger"], false);
    assert_eq!(payload["side_effects"]["creates_witness_directory"], false);
    assert_all_side_effects_false(&payload["side_effects"]);
    assert!(payload["fixers"].as_array().unwrap().is_empty());
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_capabilities_json_has_no_fixers_or_side_effects() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "capabilities", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.capabilities.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["read_only"], true);
    assert_eq!(
        payload["config_footprint"]["deprecation_notices"],
        "~/.cmdrvl/notices/deprecated-paths.jsonl"
    );
    assert_all_side_effects_false(&payload["side_effects"]);
    assert!(payload["fixers"].as_array().unwrap().is_empty());
    assert!(
        payload["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["name"] == "robot-triage"
                && command["usage"] == "canon doctor --robot-triage")
    );
    assert_eq!(payload["composition"]["family"]["name"], "cmdrvl-spine");
    assert_eq!(
        payload["composition"]["role"],
        "canonical identifier normalization before structural checks and reconciliation"
    );
    assert!(
        payload["composition"]["canonical_chain"][0]
            .as_str()
            .is_some_and(|command| command.contains("canon old.csv --registry <REGISTRY>"))
    );
    assert!(
        payload["composition"]["canonical_chain"][2]
            .as_str()
            .is_some_and(|command| command.contains("shape old.canon.csv new.canon.csv"))
    );
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_robot_triage_json_is_machine_readable() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    let output = doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "--robot-triage"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["schema"], "canon.doctor.triage.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["score"], 100);
    assert_eq!(payload["read_only"], true);
    assert_eq!(
        payload["config_footprint"]["migration_policy"],
        "copy-only legacy witness migration; never delete or move legacy files; never record file contents or secret values"
    );
    assert_all_side_effects_false(&payload["side_effects"]);
    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_robot_docs_is_plain_text_and_read_only() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "robot-docs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cmdrvl.read_only_doctor.v1"))
        .stdout(predicate::str::contains("canon doctor health --json"))
        .stdout(predicate::str::contains("composition:"))
        .stdout(predicate::str::contains(
            "shape old.canon.csv new.canon.csv",
        ))
        .stdout(predicate::str::contains("rvl old.canon.csv new.canon.csv"))
        .stdout(predicate::str::contains("no --fix surface"));

    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_doctor_help_is_available() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("capabilities"))
        .stdout(predicate::str::contains("robot-docs"))
        .stdout(predicate::str::contains("--robot-triage"));
}

#[test]
fn test_doctor_fix_is_not_available() {
    let temp_dir = tempdir().unwrap();
    let witness_path = temp_dir.path().join("witness").join("canon-witness.jsonl");

    doctor_cmd_in(temp_dir.path(), &witness_path)
        .args(["doctor", "--fix"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("--fix"));

    assert_doctor_side_effects_absent(temp_dir.path(), &witness_path);
}

#[test]
fn test_resolve_cli_success_json() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["version"], "canon_resolve.v0");
    assert_eq!(payload["summary"]["target_records"], 1);
    assert_eq!(payload["summary"]["matched"], 1);
    assert_eq!(payload["summary"]["unmatched"], 0);
    assert_eq!(payload["summary"]["ambiguous"], 0);
    assert_eq!(payload["matches"][0]["reference_id"], "R-1");
    assert_eq!(payload["matches"][0]["target_id"], "D1|1");
}

#[test]
fn test_resolve_cli_summary_output() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--emit",
            "summary",
            "--no-witness",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("canon_resolve.v0"));
    assert!(stdout.contains("matched=1"));
    assert!(stdout.contains("match_rate=1.000"));
}

#[test]
fn test_resolve_cli_partial_exit_one() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), false);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(1);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["summary"]["matched"], 0);
    assert_eq!(payload["summary"]["unmatched"], 1);
    assert_eq!(
        payload["unmatched"][0]["reason"],
        "required_assertion_failed"
    );
}

#[test]
fn test_resolve_cli_malformed_strategy_refusal() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);
    std::fs::write(&fixture.strategy, "not: [valid").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_BAD_STRATEGY");
}

#[test]
fn test_resolve_cli_missing_column_refusal() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture_path("tests/fixtures/resolve/tapes/reference_loans.csv")
                .to_str()
                .unwrap(),
            fixture_path("tests/fixtures/resolve/tapes/missing_column_target.csv")
                .to_str()
                .unwrap(),
            "--strategy",
            fixture_path("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml")
                .to_str()
                .unwrap(),
            "--registry",
            fixture_path("tests/fixtures/registries/resolve-servicers")
                .to_str()
                .unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_COLUMN_NOT_FOUND");
}

#[test]
fn test_resolve_cli_empty_tape_refusal() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture_path("tests/fixtures/resolve/tapes/reference_loans.csv")
                .to_str()
                .unwrap(),
            fixture_path("tests/fixtures/resolve/tapes/empty_target.csv")
                .to_str()
                .unwrap(),
            "--strategy",
            fixture_path("tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml")
                .to_str()
                .unwrap(),
            "--registry",
            fixture_path("tests/fixtures/registries/resolve-servicers")
                .to_str()
                .unwrap(),
            "--no-witness",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_EMPTY_TAPE");
}

#[test]
fn test_resolve_cli_no_witness_suppresses_ledger() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);
    let ledger_path = temp_dir.path().join("resolve-witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--no-witness",
        ])
        .assert()
        .success();

    assert!(!ledger_path.exists());
}

#[test]
fn test_resolve_cli_witness_append_and_failure_nonfatal() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);
    let ledger_path = temp_dir.path().join("resolve-witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(record["outcome"], "RESOLVED");
    assert_eq!(record["exit_code"], 0);
    assert_eq!(record["params"]["command"], "resolve");
    assert_eq!(record["params"]["registry_id"], "resolve-smoke");
    assert_eq!(record["params"]["summary"]["matched"], 1);

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", temp_dir.path())
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn test_resolve_cli_writeback_invocation_shape() {
    let temp_dir = tempdir().unwrap();
    let fixture = write_resolve_smoke_fixture(temp_dir.path(), true);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "resolve",
            fixture.reference.to_str().unwrap(),
            fixture.target.to_str().unwrap(),
            "--strategy",
            fixture.strategy.to_str().unwrap(),
            "--registry",
            fixture.registry.to_str().unwrap(),
            "--gold",
            fixture.gold.to_str().unwrap(),
            "--write-back",
            "--no-witness",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["gold_score"]["accuracy"], 1.0);
    assert_eq!(payload["write_back"]["written"], true);
    assert_eq!(payload["write_back"]["entry_count"], 2);

    let mapping_file = payload["write_back"]["mapping_file"].as_str().unwrap();
    let mapping_path = fixture.registry.join(mapping_file);
    assert!(mapping_path.exists());
    let mapping_content = std::fs::read_to_string(mapping_path).unwrap();
    assert!(mapping_content.contains("STRUCTURAL_MATCH:resolve-smoke.v1"));
    assert!(!mapping_content.contains("100 Main St"));
}

#[test]
fn test_schema_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--schema")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("--schema should output valid JSON");

    assert_eq!(
        json["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(json["$id"], "https://canon.v0/schema.json");
    assert!(json["properties"].is_object());
}

#[test]
fn test_strategy_register_and_resolve_cli() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-test", "0.1.0", 0);

    let schema_path = registry_dir.path().join("profile.json");
    let compatible_schema_path = registry_dir.path().join("profile-compatible.json");
    let partial_schema_path = registry_dir.path().join("profile-partial.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");

    write_strategy_schema(&schema_path, 3);
    write_strategy_schema(&compatible_schema_path, 99);
    std::fs::write(
        &partial_schema_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "columns": [
                {"name": "vendor", "type": "string", "cardinality": 3},
                {"name": "category", "type": "string", "cardinality": 5}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "print('total')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let register = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "procurement-total.v1",
            "--language",
            "python",
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();
    let register_stdout = String::from_utf8(register.get_output().stdout.clone()).unwrap();
    let register_json: Value = serde_json::from_str(&register_stdout).unwrap();
    assert_eq!(register_json["version"], "canon_strategy_register.v0");
    assert_eq!(register_json["registry"]["version"], "0.2.0");
    assert_eq!(
        register_json["registered"]["script"]["id"],
        "procurement-total.v1"
    );

    let exact = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let exact_stdout = String::from_utf8(exact.get_output().stdout.clone()).unwrap();
    let exact_json: Value = serde_json::from_str(&exact_stdout).unwrap();
    assert_eq!(exact_json["outcome"], "EXACT");
    assert_eq!(exact_json["match"]["script"]["id"], "procurement-total.v1");

    let compatible = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            compatible_schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let compatible_stdout = String::from_utf8(compatible.get_output().stdout.clone()).unwrap();
    let compatible_json: Value = serde_json::from_str(&compatible_stdout).unwrap();
    assert_eq!(compatible_json["outcome"], "COMPATIBLE");

    let partial = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            partial_schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .code(1);
    let partial_stdout = String::from_utf8(partial.get_output().stdout.clone()).unwrap();
    let partial_json: Value = serde_json::from_str(&partial_stdout).unwrap();
    assert_eq!(partial_json["outcome"], "PARTIAL");
    assert_eq!(
        partial_json["escalation"]["reason"],
        "partial_schema_overlap"
    );
}

#[test]
fn test_strategy_task_operator_lifecycle_cli() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-task-test", "0.1.0", 0);

    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("sql_lineage.py");
    let updated_script_path = registry_dir.path().join("sql_lineage_v2.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");
    let witness_path = registry_dir.path().join("witness").join("strategy.jsonl");

    std::fs::write(&skill_path, "sql lineage skill").unwrap();
    std::fs::write(&script_path, "print('lineage v1')\n").unwrap();
    std::fs::write(&updated_script_path, "print('lineage v2')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let register = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v1",
            "--language",
            "python",
            "--grade",
            "operator-attested",
            "--operator",
            "Zac",
            "--reason",
            "worked on sample rows",
            "--attested-at",
            "2026-06-25T12:00:00Z",
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();
    let register_json: Value =
        serde_json::from_slice(&register.get_output().stdout).expect("register JSON");
    assert_eq!(register_json["registered"]["key"]["type"], "task");
    assert_eq!(register_json["registered"]["task"], "sql_lineage");
    assert_eq!(register_json["registered"]["grade"], "operator-attested");
    assert_eq!(register_json["receipt"]["operation"], "register");
    assert!(
        register_json["receipt"]["before_registry_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert!(
        register_json["receipt"]["after_registry_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    let witness_lines = std::fs::read_to_string(&witness_path).unwrap();
    assert_eq!(witness_lines.lines().count(), 1);
    assert!(witness_lines.contains("strategy_receipt"));

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let resolve_json: Value = serde_json::from_slice(&resolve.get_output().stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "EXACT");
    assert_eq!(resolve_json["match"]["script"]["id"], "sql-lineage.v1");
    assert_eq!(resolve_json["match"]["diagnostics"], Value::Null);

    let list = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "list",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--key-type",
            "task",
            "--status",
            "active",
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(list_json["version"], "canon_strategy_list.v0");
    assert_eq!(list_json["entries"].as_array().unwrap().len(), 1);

    let explain = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "explain",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let explain_json: Value = serde_json::from_slice(&explain.get_output().stdout).unwrap();
    assert_eq!(
        explain_json["active_resolution"]["script"]["id"],
        "sql-lineage.v1"
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "update",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            updated_script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v2",
            "--language",
            "python",
            "--operator",
            "Zac",
            "--reason",
            "tightened parser",
            "--attested-at",
            "2026-06-25T12:01:00Z",
            "--next-version",
            "0.3.0",
            "--no-witness",
        ])
        .assert()
        .success();
    let witness_lines_after_update = std::fs::read_to_string(&witness_path).unwrap();
    assert_eq!(witness_lines_after_update.lines().count(), 1);

    let updated_resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let updated_resolve_json: Value =
        serde_json::from_slice(&updated_resolve.get_output().stdout).unwrap();
    assert_eq!(
        updated_resolve_json["match"]["script"]["id"],
        "sql-lineage.v2"
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &witness_path)
        .args([
            "strategy",
            "deprecate",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--operator",
            "Zac",
            "--reason",
            "retired active champion",
            "--attested-at",
            "2026-06-25T12:02:00Z",
            "--next-version",
            "0.4.0",
        ])
        .assert()
        .success();

    let deprecated_resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .code(1);
    let deprecated_resolve_json: Value =
        serde_json::from_slice(&deprecated_resolve.get_output().stdout).unwrap();
    assert_eq!(deprecated_resolve_json["outcome"], "UNRESOLVED");

    let deprecated_explain = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "explain",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let deprecated_explain_json: Value =
        serde_json::from_slice(&deprecated_explain.get_output().stdout).unwrap();
    assert_eq!(deprecated_explain_json["active_resolution"], Value::Null);
    assert_eq!(
        deprecated_explain_json["ignored"].as_array().unwrap().len(),
        1
    );

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            updated_script_path.to_str().unwrap(),
            "--script-id",
            "sql-lineage.v3",
            "--language",
            "python",
            "--grade",
            "operator-attested",
            "--operator",
            "Zac",
            "--reason",
            "replacement champion",
            "--attested-at",
            "2026-06-25T12:03:00Z",
            "--next-version",
            "0.5.0",
            "--no-witness",
        ])
        .assert()
        .success();

    let promote = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "promote",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--task",
            "sql_lineage",
            "--skill",
            skill_path.to_str().unwrap(),
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.6.0",
            "--no-witness",
        ])
        .assert()
        .success();
    let promote_json: Value = serde_json::from_slice(&promote.get_output().stdout).unwrap();
    assert_eq!(promote_json["entry"]["grade"], "proof-attested");
    assert_eq!(promote_json["receipt"]["operation"], "promote");
}

#[test]
fn test_strategy_profile_cli_output_can_resolve_registered_strategy() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-profile-test", "0.1.0", 0);

    let rows_path = registry_dir.path().join("rows.csv");
    let profile_path = registry_dir.path().join("profile.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.py");
    let verify_path = registry_dir.path().join("verify.json");
    let assess_path = registry_dir.path().join("assess.json");
    let airlock_path = registry_dir.path().join("airlock.json");

    std::fs::write(
        &rows_path,
        "vendor,amount,active\nAcme,10,true\nBolt,20,false\nAcme,30,true\n",
    )
    .unwrap();
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "print('profiled total')\n").unwrap();
    std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
    std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
    std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

    let profile = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "profile",
            rows_path.to_str().unwrap(),
            "--max-rows",
            "10",
            "--max-bytes",
            "1024",
        ])
        .assert()
        .success();
    let profile_stdout = String::from_utf8(profile.get_output().stdout.clone()).unwrap();
    let profile_json: Value = serde_json::from_str(&profile_stdout).unwrap();
    assert_eq!(profile_json["version"], "canon_strategy_profile.v0");
    assert_eq!(profile_json["summary"]["rows"], 3);
    assert_eq!(profile_json["input"]["format"], "csv");
    assert_eq!(
        profile_json["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["active", "amount", "vendor"]
    );
    std::fs::write(&profile_path, &profile_stdout).unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            profile_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "procurement-profiled-total.v1",
            "--language",
            "python",
            "--verify",
            verify_path.to_str().unwrap(),
            "--assess",
            assess_path.to_str().unwrap(),
            "--airlock",
            airlock_path.to_str().unwrap(),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "resolve",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            profile_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "EXACT");
    assert_eq!(
        resolve_json["query"]["schema_fingerprint"],
        profile_json["schema_fingerprint"]
    );
    assert_eq!(
        resolve_json["match"]["script"]["id"],
        "procurement-profiled-total.v1"
    );
}

#[test]
fn test_strategy_diff_cli_reports_changed_entry() {
    let old_registry_dir = tempdir().unwrap();
    let new_registry_dir = tempdir().unwrap();
    write_registry_metadata(old_registry_dir.path(), "strategy-test", "0.1.0", 0);
    write_registry_metadata(new_registry_dir.path(), "strategy-test", "0.1.0", 0);

    for (registry_dir, script_body) in [
        (old_registry_dir.path(), "print('old')\n"),
        (new_registry_dir.path(), "print('new')\n"),
    ] {
        let schema_path = registry_dir.join("profile.json");
        let skill_path = registry_dir.join("SKILL.md");
        let script_path = registry_dir.join("script.py");
        let verify_path = registry_dir.join("verify.json");
        let assess_path = registry_dir.join("assess.json");
        let airlock_path = registry_dir.join("airlock.json");

        write_strategy_schema(&schema_path, 3);
        std::fs::write(&skill_path, "procurement skill").unwrap();
        std::fs::write(&script_path, script_body).unwrap();
        std::fs::write(&verify_path, r#"{"status":"PASS"}"#).unwrap();
        std::fs::write(&assess_path, r#"{"decision":"PROCEED"}"#).unwrap();
        std::fs::write(&airlock_path, r#"{"sealed":true}"#).unwrap();

        Command::new(env!("CARGO_BIN_EXE_canon"))
            .args([
                "strategy",
                "register",
                "--registry",
                registry_dir.to_str().unwrap(),
                "--schema",
                schema_path.to_str().unwrap(),
                "--skill",
                skill_path.to_str().unwrap(),
                "--script",
                script_path.to_str().unwrap(),
                "--script-id",
                "procurement-total.v1",
                "--language",
                "python",
                "--verify",
                verify_path.to_str().unwrap(),
                "--assess",
                assess_path.to_str().unwrap(),
                "--airlock",
                airlock_path.to_str().unwrap(),
                "--next-version",
                "0.2.0",
            ])
            .assert()
            .success();
    }

    let diff = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "diff",
            "--old",
            old_registry_dir.path().to_str().unwrap(),
            "--new",
            new_registry_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    let diff_stdout = String::from_utf8(diff.get_output().stdout.clone()).unwrap();
    let diff_json: Value = serde_json::from_str(&diff_stdout).unwrap();
    assert_eq!(diff_json["version"], "canon_strategy_diff.v0");
    assert_eq!(diff_json["summary"]["changed"], 1);
    assert_eq!(
        diff_json["changed"][0]["change_types"],
        serde_json::json!(["script_path_change", "script_content_hash_change"])
    );
}

#[cfg(unix)]
#[test]
fn test_strategy_audit_cli_produces_register_compatible_proof() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "strategy-audit-test", "0.1.0", 0);

    let schema_path = registry_dir.path().join("profile.json");
    let skill_path = registry_dir.path().join("SKILL.md");
    let script_path = registry_dir.path().join("script.sh");
    let suite_dir = registry_dir.path().join("suite");
    let audit_path = registry_dir.path().join("audit.json");

    write_strategy_schema(&schema_path, 1);
    std::fs::write(&skill_path, "procurement skill").unwrap();
    std::fs::write(&script_path, "#!/bin/sh\ncat\n").unwrap();
    let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions).unwrap();

    std::fs::create_dir(&suite_dir).unwrap();
    std::fs::create_dir(suite_dir.join("inputs")).unwrap();
    std::fs::create_dir(suite_dir.join("expected")).unwrap();
    std::fs::write(suite_dir.join("inputs/case1.txt"), "Acme,10\n").unwrap();
    std::fs::write(suite_dir.join("expected/case1.out"), "Acme,10\n").unwrap();
    std::fs::write(
        suite_dir.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "suite_id": "strategy_audit_suite.v1",
            "version": "1.0.0",
            "repeatability_runs": 2,
            "fixtures": [
                {
                    "id": "case1",
                    "input": "inputs/case1.txt",
                    "expected_stdout": "expected/case1.out",
                    "expected_exit_code": 0
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let audit = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "audit",
            "--schema",
            schema_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--suite",
            suite_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    let audit_stdout = String::from_utf8(audit.get_output().stdout.clone()).unwrap();
    let audit_json: Value = serde_json::from_str(&audit_stdout).unwrap();
    assert_eq!(audit_json["version"], "canon_strategy_audit.v0");
    assert_eq!(audit_json["passed"], true);
    assert_eq!(audit_json["decision"], "PROCEED");
    assert_eq!(audit_json["sealed"], true);
    std::fs::write(&audit_path, audit_stdout).unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "strategy",
            "register",
            "--registry",
            registry_dir.path().to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--skill",
            skill_path.to_str().unwrap(),
            "--script",
            script_path.to_str().unwrap(),
            "--script-id",
            "audited-script.v1",
            "--language",
            "sh",
            "--verify",
            audit_path.to_str().unwrap(),
            "--assess",
            audit_path.to_str().unwrap(),
            "--airlock",
            audit_path.to_str().unwrap(),
            "--next-version",
            "0.2.0",
        ])
        .assert()
        .success();
}

#[test]
fn info_flags_short_circuit_before_invalid_args_are_parsed() {
    let version = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--version", "--emit", "bogus"])
        .assert()
        .success();
    let version_stdout = String::from_utf8(version.get_output().stdout.clone()).unwrap();
    assert_eq!(
        version_stdout.trim(),
        format!("canon {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(version.get_output().stderr.is_empty());

    let describe = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--describe", "--column"])
        .assert()
        .success();
    let describe_stdout = String::from_utf8(describe.get_output().stdout.clone()).unwrap();
    let describe_json: Value =
        serde_json::from_str(&describe_stdout).expect("--describe should output valid JSON");
    assert_eq!(describe_json["name"], "canon");
    assert!(describe.get_output().stderr.is_empty());

    let schema = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["--schema", "--max-rows", "nope"])
        .assert()
        .success();
    let schema_stdout = String::from_utf8(schema.get_output().stdout.clone()).unwrap();
    let schema_json: Value =
        serde_json::from_str(&schema_stdout).expect("--schema should output valid JSON");
    assert_eq!(schema_json["$id"], "https://canon.v0/schema.json");
    assert!(schema.get_output().stderr.is_empty());
}

#[test]
fn test_registry_diff_json_output() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 3);
    write_mapping_file(
        old_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "MSFT",
                "canonical_id": "BBG000BPH459",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "TSLA",
                "canonical_id": "BBG000N9MNX3",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 3);
    write_mapping_file(
        new_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "MSFT",
                "canonical_id": "BBG000BPH45Z",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "NVDA",
                "canonical_id": "BBG000BBJQV0",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_diff.v0");
    assert_eq!(payload["old"]["id"], "openfigi-cusip");
    assert_eq!(payload["old"]["version"], "2026.02.28");
    assert_eq!(payload["new"]["version"], "2026.03.05");
    assert_eq!(payload["summary"]["added"], 1);
    assert_eq!(payload["summary"]["removed"], 1);
    assert_eq!(payload["summary"]["changed"], 1);
    assert_eq!(payload["summary"]["unchanged"], 1);
    assert_eq!(payload["added"][0]["input"], "NVDA");
    assert_eq!(payload["removed"][0]["input"], "TSLA");
    assert_eq!(payload["changed"][0]["input"], "MSFT");
    assert_eq!(payload["changed"][0]["change_type"], "canonical_id_change");
}

#[test]
fn test_registry_diff_summary_output() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 1);
    write_mapping_file(
        old_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 2);
    write_mapping_file(
        new_dir.path(),
        "a-primary.json",
        serde_json::json!([
            {
                "input": "AAPL",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            },
            {
                "input": "NVDA",
                "canonical_id": "BBG000BBJQV0",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI"
            }
        ]),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.trim(),
        "openfigi-cusip: 2026.02.28 -> 2026.03.05 | +1 added, -0 removed, ~0 changed, =1 unchanged"
    );
}

#[test]
fn test_registry_diff_mismatched_id_refusal_in_summary_mode() {
    let old_dir = tempdir().unwrap();
    write_registry_metadata(old_dir.path(), "old-registry", "1.0.0", 0);

    let new_dir = tempdir().unwrap();
    write_registry_metadata(new_dir.path(), "new-registry", "1.1.0", 0);

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "diff",
            "--old",
            old_dir.path().to_str().unwrap(),
            "--new",
            new_dir.path().to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    assert!(stdout.is_empty());
    assert!(stderr.contains("E_BAD_REGISTRY"));
    assert!(stderr.contains("old-registry"));
    assert!(stderr.contains("new-registry"));
}

#[test]
fn test_registry_audit_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "cusip",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_audit.v0");
    assert_eq!(payload["seed"]["column"], "cusip");
    assert_eq!(payload["registry"]["id"], "cusip-isin");
    assert_eq!(payload["summary"]["total"], 3);
    assert_eq!(payload["summary"]["resolved"], 2);
    assert_eq!(payload["summary"]["unresolved"], 1);
    assert_eq!(payload["summary"]["distinct_canonical_targets"], 2);
    assert_eq!(payload["summary"]["distinct_rule_ids"], 1);
    assert_eq!(payload["resolved"].as_array().unwrap().len(), 2);
    assert_eq!(payload["unresolved"].as_array().unwrap().len(), 1);
    assert_eq!(payload["canonical_targets"].as_array().unwrap().len(), 2);
    assert_eq!(payload["rule_hits"][0]["rule_id"], "CUSIP_TO_ISIN");
    assert_eq!(payload["rule_hits"][0]["count"], 2);
}

#[test]
fn test_registry_audit_summary_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "cusip",
            "--emit",
            "summary",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("cusip-isin@1.0.0 audit"));
    assert!(stdout.contains("3 total, 2 resolved, 1 unresolved"));
    assert!(stdout.contains("2 targets, 1 rules"));
}

#[test]
fn test_registry_audit_refusal_in_summary_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "audit",
            "tests/fixtures/inputs/partial.csv",
            "--registry",
            "tests/fixtures/registries/cusip-isin",
            "--column",
            "missing_column",
            "--emit",
            "summary",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    assert!(stdout.is_empty());
    assert!(stderr.contains("E_COLUMN_NOT_FOUND"));
}

#[test]
fn test_all_resolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(0); // RESOLVED
}

#[test]
fn test_partial_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/partial.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // PARTIAL
}

#[test]
fn test_all_unresolved_exit_code() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_unresolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(1); // UNRESOLVED
}

#[test]
fn test_missing_input_file_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("nonexistent.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("REFUSAL"));
}

#[test]
fn test_emit_csv_with_jsonl_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/basic.jsonl")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2) // REFUSAL
        .stderr(predicate::str::contains("E_EMIT_FORMAT"));
}

#[test]
fn test_column_not_found_refusal() {
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent_column")
        .assert()
        .code(2) // REFUSAL
        .stdout(predicate::str::contains("E_COLUMN_NOT_FOUND"));
}

#[test]
fn test_json_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&stdout).expect("JSON mode should output valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
    assert!(json["registry"].is_object());
    assert!(json["summary"].is_object());
}

#[test]
fn test_csv_mode_success_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Should be CSV format with canonical column
    assert!(stdout.contains("cusip__canon"));
    // Should not be JSON
    assert!(!stdout.starts_with('{'));
}

#[test]
fn test_csv_mode_refusal_to_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/wrong_column.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("nonexistent")
        .arg("--emit")
        .arg("csv")
        .assert()
        .code(2);

    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Refusal should go to stderr in CSV mode
    assert!(stderr.contains("E_COLUMN_NOT_FOUND"));
    // No CSV output on stdout
    assert!(stdout.is_empty());
}

#[test]
fn test_witness_flag_no_witness() {
    // This test just ensures --no-witness doesn't break execution
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--no-witness")
        .assert()
        .success();
}

#[test]
fn test_witness_uses_epistemic_witness_env_path() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("nested").join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(record["tool"], "canon");
    assert!(record["id"].as_str().unwrap().starts_with("blake3:"));
    assert!(
        record["binary_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(
        record["inputs"][0]["path"],
        fixture_path("tests/fixtures/inputs/all_resolved.csv")
            .display()
            .to_string()
    );
    assert!(
        record["inputs"][0]["hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert_eq!(record["params"]["registry_id"], "cusip-isin");
    assert_eq!(record["params"]["registry_version"], "1.0.0");
    assert_eq!(record["params"]["emit"], "json");
    assert_eq!(record["outcome"], "RESOLVED");
    assert_eq!(record["exit_code"], 0);
}

#[test]
fn test_witness_defaults_to_home_cmdrvl_path() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = home
        .path()
        .join(".cmdrvl")
        .join("state")
        .join("witness")
        .join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env_remove("EPISTEMIC_WITNESS")
        .env("HOME", home.path())
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(ledger_path.exists());
    assert!(
        !home
            .path()
            .join(".epistemic")
            .join("witness.jsonl")
            .exists()
    );
    assert!(!cwd.path().join(".canon-witness.jsonl").exists());
}

#[test]
fn test_witness_migrates_legacy_home_epistemic_path_copy_only() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let legacy_path = home.path().join(".epistemic").join("witness.jsonl");
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(&legacy_path, "{\"tool\":\"legacy-canon\"}\n").unwrap();

    let canonical_path = home
        .path()
        .join(".cmdrvl")
        .join("state")
        .join("witness")
        .join("witness.jsonl");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env_remove("EPISTEMIC_WITNESS")
        .env("HOME", home.path())
        .arg(fixture_path("tests/fixtures/inputs/all_resolved.csv"))
        .arg("--registry")
        .arg(fixture_path("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();

    assert!(legacy_path.exists());
    let content = std::fs::read_to_string(&canonical_path).unwrap();
    assert!(content.contains("\"tool\":\"legacy-canon\""));
    assert!(content.contains("\"tool\":\"canon\""));

    let migration_log =
        std::fs::read_to_string(home.path().join(".cmdrvl/migrations/applied.jsonl")).unwrap();
    assert!(migration_log.contains("\"path_class\":\"witness_ledger\""));
    assert!(migration_log.contains("\"secret_values_recorded\":false"));

    let notices =
        std::fs::read_to_string(home.path().join(".cmdrvl/notices/deprecated-paths.jsonl"))
            .unwrap();
    assert!(notices.contains("\"action\":\"legacy_path_migrated\""));
    assert!(notices.contains("\"secret_values_recorded\":false"));
}

#[test]
fn test_witness_hash_parity_and_chain_linkage() {
    let ledger_dir = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let input_path = fixture_path("tests/fixtures/inputs/all_resolved.csv");
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    let json_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .assert()
        .success();
    let json_stdout = json_output.get_output().stdout.clone();

    let csv_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg(&input_path)
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .assert()
        .success();
    let csv_stdout = csv_output.get_output().stdout.clone();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let second: Value = serde_json::from_str(lines[1]).unwrap();

    let expected_json_hash = format!("blake3:{}", blake3::hash(&json_stdout).to_hex());
    let expected_csv_hash = format!("blake3:{}", blake3::hash(&csv_stdout).to_hex());

    assert_eq!(first["output_hash"], expected_json_hash);
    assert_eq!(second["output_hash"], expected_csv_hash);
    assert_ne!(second["id"], first["id"]);
    assert_eq!(first["params"]["emit"], "json");
    assert_eq!(second["params"]["emit"], "csv");
}

#[test]
fn test_witness_hashes_stdin_bytes_without_dash_file() {
    let ledger_dir = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let ledger_path = ledger_dir.path().join("witness.jsonl");
    let stdin_data =
        std::fs::read_to_string(fixture_path("tests/fixtures/inputs/basic.jsonl")).unwrap();
    let registry_path = fixture_path("tests/fixtures/registries/cusip-isin");

    assert!(!cwd.path().join("-").exists());

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(cwd.path())
        .env("EPISTEMIC_WITNESS", &ledger_path)
        .arg("-")
        .arg("--registry")
        .arg(&registry_path)
        .arg("--column")
        .arg("cusip")
        .write_stdin(stdin_data.clone())
        .assert()
        .success();

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let record: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let expected_hash = format!("blake3:{}", blake3::hash(stdin_data.as_bytes()).to_hex());

    assert_eq!(record["inputs"][0]["path"], "-");
    assert_eq!(record["inputs"][0]["hash"], expected_hash);
    assert_eq!(record["inputs"][0]["bytes"], stdin_data.len() as u64);
    assert_eq!(record["outcome"], "RESOLVED");
}

#[test]
fn test_map_out_sidecar_in_csv_mode() {
    use tempfile::NamedTempFile;

    let temp_file = NamedTempFile::new().unwrap();
    let map_out_path = temp_file.path().to_str().unwrap();

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("tests/fixtures/inputs/all_resolved.csv")
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("csv")
        .arg("--map-out")
        .arg(map_out_path)
        .assert()
        .success();

    // Check that sidecar JSON was written
    assert!(Path::new(map_out_path).exists());
    let sidecar_content = std::fs::read_to_string(map_out_path).unwrap();
    let json: Value = serde_json::from_str(&sidecar_content).expect("Sidecar should be valid JSON");

    assert_eq!(json["version"], "canon.v0");
    assert_eq!(json["outcome"], "RESOLVED");
}

#[test]
fn test_registry_build_materializes_registry_and_resolves() {
    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");
    let resolve_path = temp_dir.path().join("resolve.csv");

    write_seed_csv(
        &seed_path,
        "cusip,note\nAAPL,ok\nMSFT,ok\nMISS_UNKNOWN,miss\nFAIL_BROKEN,fail\n,blank\nAAPL,dup\n",
    );
    write_seed_csv(&resolve_path, "cusip\nAAPL\nMSFT\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("provider failure(s)"));

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();

    assert_eq!(payload["version"], "canon_registry_build.v0");
    assert_eq!(payload["source"], "mock");
    assert_eq!(payload["registry"]["id"], "mock-cusip");
    assert_eq!(payload["registry"]["version"], "2026.03.13");
    assert_eq!(payload["summary"]["seed_count"], 4);
    assert_eq!(payload["summary"]["queried_count"], 4);
    assert_eq!(payload["summary"]["carried_forward_count"], 0);
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["unresolved_count"], 1);
    assert_eq!(payload["summary"]["failure_count"], 1);
    assert_eq!(payload["summary"]["skipped_special_reason_rows"], 1);
    assert_eq!(payload["special_reasons"][0]["reason"], "empty_value");
    assert_eq!(payload["special_reasons"][0]["count"], 1);
    assert_eq!(payload["files"], serde_json::json!(["cusip-to-mock.json"]));

    let registry_json: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("registry.json")).unwrap())
            .unwrap();
    assert_eq!(registry_json["id"], "mock-cusip");
    assert_eq!(registry_json["version"], "2026.03.13");
    assert_eq!(registry_json["entry_count"], 2);
    assert!(output_dir.join("_build.json").exists());

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&resolve_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();

    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(resolve_json["registry"]["id"], "mock-cusip");
    assert_eq!(resolve_json["summary"]["resolved"], 2);
    assert_eq!(resolve_json["mappings"][0]["canonical_id"], "u8:MOCK::AAPL");
    assert_eq!(resolve_json["mappings"][1]["canonical_id"], "u8:MOCK::MSFT");
}

#[test]
fn test_registry_build_incremental_carries_forward_existing_entries() {
    let temp_dir = tempdir().unwrap();
    let initial_seed_path = temp_dir.path().join("seed-initial.csv");
    let incremental_seed_path = temp_dir.path().join("seed-incremental.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");

    write_seed_csv(&initial_seed_path, "cusip\nAAPL\nMSFT\n");
    write_seed_csv(&incremental_seed_path, "cusip\nAAPL\nMSFT\nNVDA\n");

    Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            initial_seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .success();

    let incremental = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            incremental_seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.14",
            "--incremental",
        ])
        .assert()
        .success();

    let incremental_stdout = String::from_utf8(incremental.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&incremental_stdout).unwrap();

    assert_eq!(payload["summary"]["seed_count"], 3);
    assert_eq!(payload["summary"]["queried_count"], 1);
    assert_eq!(payload["summary"]["carried_forward_count"], 2);
    assert_eq!(payload["summary"]["resolved_count"], 3);
    assert_eq!(payload["summary"]["unresolved_count"], 0);
    assert_eq!(payload["summary"]["failure_count"], 0);

    let registry_json: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("registry.json")).unwrap())
            .unwrap();
    assert_eq!(registry_json["version"], "2026.03.14");
    assert_eq!(registry_json["entry_count"], 3);

    let mapping_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("cusip-to-mock.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(mapping_entries.as_array().unwrap().len(), 3);
    assert_eq!(mapping_entries[0]["input"], "AAPL");
    assert_eq!(mapping_entries[1]["input"], "MSFT");
    assert_eq!(mapping_entries[2]["input"], "NVDA");
}

#[test]
fn test_registry_lint_cli_json_and_summary_output() {
    let registry_dir = tempdir().unwrap();
    write_registry_metadata(registry_dir.path(), "lint-test", "1.0.0", 2);
    write_mapping_file(
        registry_dir.path(),
        "mappings.json",
        serde_json::json!([
            {"input":"A","canonical_id":"C1","canonical_type":"entity","rule_id":"r1"},
            {"input":"A","canonical_id":"C2","canonical_type":"entity","rule_id":"r2"}
        ]),
    );

    let json = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "lint",
            registry_dir.path().to_str().unwrap(),
            "--profile",
            "standard",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(json.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["version"], "canon_registry_lint.v0");
    assert_eq!(payload["profile"], "standard");
    assert_eq!(payload["summary"]["warnings"], 1);
    assert_eq!(payload["findings"][0]["code"], "index_missing");
    assert_eq!(payload["findings"][1]["code"], "shadowed_input");

    let summary = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "lint",
            registry_dir.path().to_str().unwrap(),
            "--profile",
            "standard",
            "--emit",
            "summary",
        ])
        .assert()
        .success();
    let summary_stdout = String::from_utf8(summary.get_output().stdout.clone()).unwrap();
    assert!(summary_stdout.contains("lint-test@1.0.0 lint standard"));
}

#[test]
fn test_registry_build_refuses_non_incremental_overwrite() {
    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/mock-cusip");

    write_seed_csv(&seed_path, "cusip\nAAPL\n");
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::write(output_dir.join("existing.txt"), "occupied").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "mock",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.13",
        ])
        .assert()
        .code(2);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_IO");
    assert!(
        payload["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("refuse to overwrite in place")
    );
}

#[test]
fn test_registry_build_openfigi_provider_materializes_registry() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000B9XRY4",
                "compositeFIGI": "BBG000B9XRY4",
                "ticker": "AAPL",
                "name": "APPLE INC",
                "securityType": "Common Stock"
            }]
        },
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "securityType": "Common Stock"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let resolve_path = temp_dir.path().join("resolve.csv");
    let base_url_arg = format!("base_url={base_url}");

    write_seed_csv(&seed_path, "cusip\n037833100\n594918104\n");
    write_seed_csv(&resolve_path, "cusip\n037833100\n594918104\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .env("OPENFIGI_API_KEY", "env-api-key")
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--provider-config",
            &base_url_arg,
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.03.14",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["registry"]["id"], "openfigi-cusip");
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["api_calls"], 1);
    assert_eq!(payload["summary"]["failure_count"], 0);
    assert_eq!(
        payload["files"],
        serde_json::json!([
            "cusip-to-figi.json",
            "cusip-to-name.json",
            "cusip-to-ticker.json"
        ])
    );

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["provider"]["options"]["base_url"], base_url);
    assert!(build_file["timing"]["elapsed_ms"].as_u64().is_some());

    let (request_body, headers) = server_handle.join().unwrap();
    assert!(request_body.contains("\"idType\":\"ID_CUSIP\""));
    assert!(request_body.contains("\"idValue\":\"037833100\""));
    assert_eq!(
        headers.get("x-openfigi-apikey").map(String::as_str),
        Some("env-api-key")
    );

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&resolve_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();

    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(
        resolve_json["mappings"][0]["canonical_id"],
        "u8:BBG000B9XRY4"
    );
    assert_eq!(
        resolve_json["mappings"][1]["canonical_id"],
        "u8:BBG000BPH459"
    );
}

#[test]
fn test_registry_build_openfigi_provider_passes_mapping_filters() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "shareClassFIGI": "BBG001S5TD05",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "exchCode": "US",
                "securityType": "Common Stock",
                "securityType2": "Common Stock",
                "marketSector": "Equity"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-isin");
    let base_url_arg = format!("base_url={base_url}");

    write_seed_csv(&seed_path, "isin\nUS5949181045\n");

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "isin",
            "--provider-config",
            "id_type=ID_ISIN",
            "--provider-config",
            &base_url_arg,
            "--provider-config",
            "exchCode=US",
            "--provider-config",
            "marketSecDes=Equity",
            "--provider-config",
            "securityType2=Common Stock",
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.06.09",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["registry"]["id"], "openfigi-isin");
    assert_eq!(payload["summary"]["resolved_count"], 1);
    assert_eq!(payload["summary"]["failure_count"], 0);

    let figi_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("isin-to-figi.json")).unwrap(),
    )
    .unwrap();
    let figi_entry = figi_entries
        .as_array()
        .and_then(|entries| entries.first())
        .unwrap();
    assert_eq!(figi_entry["input"], "US5949181045");
    assert_eq!(figi_entry["canonical_id"], "BBG000BPH459");
    assert_eq!(figi_entry["canonical_type"], "composite_figi");

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["provider"]["options"]["id_type"], "ID_ISIN");
    assert_eq!(build_file["provider"]["options"]["exchCode"], "US");
    assert_eq!(build_file["provider"]["options"]["marketSecDes"], "Equity");
    assert_eq!(
        build_file["provider"]["options"]["securityType2"],
        "Common Stock"
    );

    let (request_body, _) = server_handle.join().unwrap();
    let request_json: Value = serde_json::from_str(&request_body).unwrap();
    let request_job = request_json
        .as_array()
        .and_then(|jobs| jobs.first())
        .unwrap();
    assert_eq!(request_job["idType"], "ID_ISIN");
    assert_eq!(request_job["idValue"], "US5949181045");
    assert_eq!(request_job["exchCode"], "US");
    assert_eq!(request_job["marketSecDes"], "Equity");
    assert_eq!(request_job["securityType2"], "Common Stock");
}

#[test]
fn test_registry_build_openfigi_incremental_fetches_only_missing_identifiers() {
    let response_body = serde_json::json!([
        {
            "data": [{
                "figi": "BBG000BPH459",
                "compositeFIGI": "BBG000BPH459",
                "ticker": "MSFT",
                "name": "MICROSOFT CORP",
                "securityType": "Common Stock"
            }]
        }
    ])
    .to_string();
    let (base_url, server_handle) = spawn_openfigi_server(response_body);

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let base_url_arg = format!("base_url={base_url}");
    write_seed_csv(&seed_path, "cusip\n037833100\n594918104\n");
    std::fs::create_dir_all(&output_dir).unwrap();
    write_registry_metadata(&output_dir, "openfigi-cusip", "2026.06.01", 1);
    write_mapping_file(
        &output_dir,
        "cusip-to-figi.json",
        serde_json::json!([
            {
                "input": "037833100",
                "canonical_id": "BBG000B9XRY4",
                "canonical_type": "composite_figi",
                "rule_id": "OPENFIGI_CUSIP_TO_COMPOSITE_FIGI"
            }
        ]),
    );

    let build = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "registry",
            "build",
            "--source",
            "openfigi",
            "--seed",
            seed_path.to_str().unwrap(),
            "--seed-column",
            "cusip",
            "--provider-config",
            &base_url_arg,
            "--output",
            output_dir.to_str().unwrap(),
            "--version",
            "2026.06.09",
            "--incremental",
        ])
        .assert()
        .success();

    let build_stdout = String::from_utf8(build.get_output().stdout.clone()).unwrap();
    let payload: Value = serde_json::from_str(&build_stdout).unwrap();
    assert_eq!(payload["summary"]["seed_count"], 2);
    assert_eq!(payload["summary"]["carried_forward_count"], 1);
    assert_eq!(payload["summary"]["queried_count"], 1);
    assert_eq!(payload["summary"]["resolved_count"], 2);
    assert_eq!(payload["summary"]["api_calls"], 1);

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["summary"]["carried_forward_count"], 1);
    assert_eq!(build_file["summary"]["queried_count"], 1);
    assert_eq!(build_file["summary"]["resolved_count"], 2);

    let (request_body, _) = server_handle.join().unwrap();
    assert!(!request_body.contains("\"idValue\":\"037833100\""));
    assert!(request_body.contains("\"idValue\":\"594918104\""));
}

#[cfg(unix)]
#[test]
fn test_registry_build_openfigi_provider_materializes_registry_with_twinning_stub() {
    let twinning = twinning_bin();
    if !twinning.exists() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; expected twinning binary at {} or set TWINNING_BIN",
            twinning.display()
        );
        return;
    }

    let spec_path =
        fixture_path("../twinning/tests/fixtures/rest/openfigi_v2_v3/response-stub-schema.yaml");
    if !spec_path.exists() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; expected OpenFIGI response-stub fixture at {}",
            spec_path.display()
        );
        return;
    }

    let temp_dir = tempdir().unwrap();
    let seed_path = temp_dir.path().join("seed.csv");
    let output_dir = temp_dir.path().join("registries/openfigi-cusip");
    let report_path = temp_dir.path().join("twinning-rest-report.json");
    let preflight_report_path = temp_dir.path().join("twinning-rest-preflight.json");
    let preflight = std::process::Command::new(&twinning)
        .args([
            "rest",
            "--json",
            "--spec",
            spec_path.to_str().unwrap(),
            "--server-variable",
            "basePath=v3",
            "--auth-mode",
            "shape",
            "--report",
            preflight_report_path.to_str().unwrap(),
            "--run",
            "true",
        ])
        .output()
        .unwrap();
    if !preflight.status.success() {
        eprintln!(
            "skipping twinning-backed OpenFIGI smoke; twinning REST runtime is unavailable: {}",
            String::from_utf8_lossy(&preflight.stdout)
        );
        return;
    }
    write_seed_csv(&seed_path, "cusip\n037833100\n");

    let child_command = format!(
        "{} registry build --source openfigi --seed {} --seed-column cusip --provider-config id_type=ID_CUSIP --provider-config api_key=stub-key --provider-config exchCode=US --provider-config base_url=\"$TWIN_BASE_URL/v3/mapping\" --output {} --version 2026.06.09",
        shell_quote(env!("CARGO_BIN_EXE_canon")),
        shell_quote(&seed_path),
        shell_quote(&output_dir),
    );

    let twin_run = Command::new(&twinning)
        .args([
            "rest",
            "--json",
            "--spec",
            spec_path.to_str().unwrap(),
            "--server-variable",
            "basePath=v3",
            "--auth-mode",
            "shape",
            "--report",
            report_path.to_str().unwrap(),
            "--run",
            &child_command,
        ])
        .assert()
        .success();

    let twin_stdout = String::from_utf8(twin_run.get_output().stdout.clone()).unwrap();
    let twin_payload: Value = serde_json::from_str(&twin_stdout).unwrap();
    assert_eq!(twin_payload["version"], "twinning.rest-run.v0");
    assert_eq!(twin_payload["child"]["exit_code"], 0);
    assert!(
        twin_payload["child"]["command"]
            .as_str()
            .unwrap()
            .contains("registry build --source openfigi")
    );

    let build_file: Value =
        serde_json::from_str(&std::fs::read_to_string(output_dir.join("_build.json")).unwrap())
            .unwrap();
    assert_eq!(build_file["summary"]["queried_count"], 1);
    assert_eq!(build_file["summary"]["resolved_count"], 1);
    assert_eq!(build_file["summary"]["unresolved_count"], 0);
    assert_eq!(build_file["summary"]["failure_count"], 0);
    assert_eq!(build_file["summary"]["api_calls"], 1);
    assert_eq!(
        build_file["provider"]["options"]["api_key"],
        serde_json::json!("[REDACTED]")
    );
    let base_url = build_file["provider"]["options"]["base_url"]
        .as_str()
        .unwrap();
    assert!(base_url.starts_with("http://127.0.0.1:"));
    assert!(!base_url.contains("api.openfigi.com"));

    let figi_entries: Value = serde_json::from_str(
        &std::fs::read_to_string(output_dir.join("cusip-to-figi.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(figi_entries[0]["input"], "037833100");
    assert_eq!(figi_entries[0]["canonical_id"], "BBG000B9XRY4");
    assert_eq!(figi_entries[0]["canonical_type"], "composite_figi");

    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap()).unwrap();
    assert_eq!(report["version"], "twinning.rest-report.v0");
    assert_eq!(report["session"]["request_count"], 1);
    assert_eq!(
        report["session"]["response_stubs"]["openfigi_cusip_success_us"],
        1
    );

    let resolve = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&seed_path)
        .arg("--registry")
        .arg(&output_dir)
        .arg("--column")
        .arg("cusip")
        .arg("--explicit")
        .assert()
        .success();
    let resolve_stdout = String::from_utf8(resolve.get_output().stdout.clone()).unwrap();
    let resolve_json: Value = serde_json::from_str(&resolve_stdout).unwrap();
    assert_eq!(resolve_json["outcome"], "RESOLVED");
    assert_eq!(
        resolve_json["mappings"][0]["canonical_id"],
        "u8:BBG000B9XRY4"
    );
}

#[cfg(unix)]
#[test]
fn test_non_utf8_input_path_does_not_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let input_path = temp_dir
        .path()
        .join(OsString::from_vec(b"input-\xFF.csv".to_vec()));
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(&input_path)
        .arg("--registry")
        .arg("tests/fixtures/registries/cusip-isin")
        .arg("--column")
        .arg("cusip")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("E_IO"));
}
