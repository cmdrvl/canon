#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    RefusalCode,
    cli::Cli,
    operator::{
        public_leaf_commands_from, public_leaf_long_flags_from, stable_manifest_digest,
        validate_operator_manifest_json,
    },
    refusal,
};
use clap::CommandFactory;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};

const OPERATOR_JSON: &str = include_str!("../operator.json");

#[derive(Debug, Deserialize)]
struct OperatorManifest {
    subcommands: Vec<OperatorRow>,
}

#[derive(Debug, Deserialize)]
struct OperatorRow {
    name: String,
    status: String,
    output_schema: Option<String>,
    read_only: Option<bool>,
    side_effects: Option<SideEffects>,
    safety: Option<Safety>,
}

#[derive(Debug, Deserialize)]
struct SideEffects {
    #[serde(default)]
    writes_registry_files: bool,
    #[serde(default)]
    writes_work_dir: bool,
    #[serde(default)]
    writes_project_files: bool,
    #[serde(default)]
    writes_output_files: bool,
    #[serde(default)]
    uses_network: bool,
}

#[derive(Debug, Deserialize)]
struct Safety {
    network_class: String,
    mutation_class: String,
}

#[derive(Debug, Clone)]
struct CompiledCommandContract {
    name: String,
    long_flags: Vec<String>,
}

#[derive(Debug, Clone)]
struct HelpCase {
    id: String,
    command: String,
    args: Vec<String>,
    expected_flags: Vec<String>,
}

struct RuntimeCase {
    id: &'static str,
    command_name: &'static str,
    args: Vec<String>,
    expected: RuntimeExpectation,
}

struct RuntimeExpectation {
    exit_code: i32,
    stdout: StdoutExpectation,
    stderr: StderrExpectation,
    mutations: Vec<MutationExpectation>,
}

enum StdoutExpectation {
    Empty,
    TextContains(&'static str),
    Json(JsonExpectation),
    ExactOperatorDescribe,
}

struct JsonExpectation {
    schema: &'static str,
    schema_field: SchemaField,
    assertions: Vec<JsonAssertion>,
}

#[derive(Clone, Copy)]
enum SchemaField {
    None,
    Schema,
    Title,
    Version,
    SchemaVersion,
}

enum JsonAssertion {
    Eq(&'static str, Value),
    ArrayLen(&'static str, usize),
    ArrayNonEmpty(&'static str),
    HashPrefix(&'static str),
    NotEq(&'static str, Value),
}

enum StderrExpectation {
    Empty,
    Contains(&'static str),
}

enum MutationExpectation {
    Exists(PathBuf),
    Missing(PathBuf),
    Unchanged(PathBuf, Vec<u8>),
}

#[test]
fn generated_help_corpus_covers_every_public_command_contract() {
    let manifest = operator_manifest();
    let compiled = compiled_command_contracts();
    let cases = help_contract_corpus(&manifest, &compiled);

    assert_corpus_ids_are_deterministic(&cases);
    assert_help_corpus_coverage(&manifest, &compiled, &cases).expect("help corpus covers contract");

    for case in cases {
        let output = canon_command()
            .args(&case.args)
            .output()
            .unwrap_or_else(|error| panic!("failed to run {}: {error}", case.id));
        assert_eq!(
            output.status.code(),
            Some(0),
            "{} did not exit successfully\nstdout={}\nstderr={}",
            case.id,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{} wrote unexpected stderr: {}",
            case.id,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout)
            .unwrap_or_else(|error| panic!("{} help stdout is not UTF-8: {error}", case.id));
        assert!(
            stdout.contains("Usage:"),
            "{} help output did not include a Usage block:\n{}",
            case.id,
            stdout
        );
        assert!(
            stdout.contains(&format!("canon {}", case.command)),
            "{} help output did not name the command path '{}':\n{}",
            case.id,
            case.command,
            stdout
        );
        for flag in &case.expected_flags {
            assert!(
                stdout.contains(&format!("--{flag}")),
                "{} help output omitted --{}:\n{}",
                case.id,
                flag,
                stdout
            );
        }
    }
}

#[test]
fn generated_command_corpus_rejects_missing_cases() {
    let manifest = operator_manifest();
    let compiled = compiled_command_contracts();
    let mut cases = help_contract_corpus(&manifest, &compiled);
    let removed = cases
        .iter()
        .position(|case| case.command == "registry providers")
        .expect("registry providers help case exists");
    cases.remove(removed);

    let error = assert_help_corpus_coverage(&manifest, &compiled, &cases)
        .expect_err("coverage validator rejects an omitted command");
    assert!(
        error.contains("missing help case for operator row registry providers"),
        "unexpected coverage error: {error}"
    );
}

#[test]
fn refusal_taxonomy_keeps_entity_artifact_contract_distinct_from_geo_unavailable() {
    let entity_artifact_contract = refusal::create_refusal(
        RefusalCode::EEntityArtifactContract,
        "Entity artifact has the wrong version".to_string(),
        json!({
            "stage": "solve",
            "field": "version",
            "expected": "canon_entity_solve.v1",
            "actual": "canon_entity_solve.v0"
        }),
        None,
    );
    let planned_geo_unavailable = refusal::create_refusal(
        RefusalCode::EGeoCommandUnavailable,
        "Geo primary command is planned but not implemented in this build".to_string(),
        json!({
            "command": "canon geo inspect",
            "status": "planned_not_implemented"
        }),
        Some("canon geo capabilities --emit json".to_string()),
    );
    let entity_code = &entity_artifact_contract.refusal.as_ref().unwrap().code;
    let planned_geo_code = &planned_geo_unavailable.refusal.as_ref().unwrap().code;

    assert_eq!(entity_code, &RefusalCode::EEntityArtifactContract);
    assert_eq!(
        serde_json::to_value(entity_code).unwrap(),
        json!("E_ENTITY_ARTIFACT_CONTRACT")
    );
    assert_eq!(
        serde_json::to_value(planned_geo_code).unwrap(),
        json!("E_GEO_COMMAND_UNAVAILABLE")
    );
    assert_ne!(entity_code, &RefusalCode::EGeoCommandUnavailable);
}

#[test]
fn generated_runtime_corpus_executes_expected_outputs_and_mutations() {
    assert!(
        validate_operator_manifest_json(&Cli::command(), OPERATOR_JSON).ok,
        "operator manifest must be valid before deriving runtime cases"
    );

    let harness = RuntimeHarness::new();
    let cases = harness.runtime_cases();
    assert_runtime_corpus_coverage(&operator_manifest(), &cases);

    for case in cases {
        let output = harness
            .command()
            .args(&case.args)
            .output()
            .unwrap_or_else(|error| panic!("failed to run {}: {error}", case.id));
        assert_eq!(
            output.status.code(),
            Some(case.expected.exit_code),
            "{} exited with the wrong code\nargs={:?}\nstdout={}\nstderr={}",
            case.id,
            case.args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_stderr(case.id, &output.stderr, &case.expected.stderr);
        assert_stdout(case.id, &output.stdout, &case.expected.stdout);
        for mutation in &case.expected.mutations {
            assert_mutation(case.id, mutation);
        }
    }
}

fn operator_manifest() -> OperatorManifest {
    serde_json::from_str(OPERATOR_JSON).expect("operator.json parses")
}

fn compiled_command_contracts() -> Vec<CompiledCommandContract> {
    let mut root = Cli::command();
    root.build();
    let mut out = Vec::new();
    for subcommand in root
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
    {
        collect_compiled_contract("", subcommand, &mut out);
    }
    out.sort_by(|left, right| left.name.cmp(&right.name));
    out
}

fn collect_compiled_contract(
    prefix: &str,
    command: &clap::Command,
    out: &mut Vec<CompiledCommandContract>,
) {
    let name = if prefix.is_empty() {
        command.get_name().to_string()
    } else {
        format!("{prefix} {}", command.get_name())
    };
    let mut long_flags = command
        .get_arguments()
        .filter_map(|argument| argument.get_long())
        .filter(|flag| *flag != "help")
        .map(str::to_string)
        .collect::<Vec<_>>();
    long_flags.sort();
    out.push(CompiledCommandContract {
        name: name.clone(),
        long_flags,
    });
    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
    {
        collect_compiled_contract(&name, subcommand, out);
    }
}

fn help_contract_corpus(
    manifest: &OperatorManifest,
    compiled: &[CompiledCommandContract],
) -> Vec<HelpCase> {
    let compiled_by_name = compiled
        .iter()
        .map(|command| (command.name.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let mut cases = manifest
        .subcommands
        .iter()
        .filter(|row| row_has_public_help(row))
        .filter_map(|row| {
            compiled_by_name
                .get(row.name.as_str())
                .map(|compiled| HelpCase {
                    id: format!("help::{}", row.name),
                    command: row.name.clone(),
                    args: row
                        .name
                        .split_whitespace()
                        .chain(["--help"])
                        .map(str::to_string)
                        .collect(),
                    expected_flags: compiled.long_flags.clone(),
                })
        })
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    cases
}

fn assert_help_corpus_coverage(
    manifest: &OperatorManifest,
    compiled: &[CompiledCommandContract],
    cases: &[HelpCase],
) -> Result<(), String> {
    let compiled_names = compiled
        .iter()
        .map(|command| command.name.as_str())
        .collect::<BTreeSet<_>>();
    let case_commands = cases
        .iter()
        .map(|case| case.command.as_str())
        .collect::<BTreeSet<_>>();
    let public_leafs = public_leaf_commands_from(&Cli::command())
        .into_iter()
        .collect::<BTreeSet<_>>();
    let public_leaf_flag_rows = public_leaf_long_flags_from(&Cli::command())
        .into_iter()
        .map(|row| row.command)
        .collect::<BTreeSet<_>>();
    if public_leafs != public_leaf_flag_rows {
        return Err("public leaf helpers disagree on command rows".to_string());
    }

    let mut errors = Vec::new();
    for row in manifest
        .subcommands
        .iter()
        .filter(|row| row_has_public_help(row))
    {
        if !compiled_names.contains(row.name.as_str()) {
            errors.push(format!(
                "operator row {} has no compiled Clap command",
                row.name
            ));
        }
        if !case_commands.contains(row.name.as_str()) {
            errors.push(format!("missing help case for operator row {}", row.name));
        }
    }
    for leaf in public_leafs {
        if !case_commands.contains(leaf.as_str()) {
            errors.push(format!("missing help case for compiled leaf {leaf}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn row_has_public_help(row: &OperatorRow) -> bool {
    matches!(row.status.as_str(), "implemented" | "unavailable")
}

fn assert_corpus_ids_are_deterministic(cases: &[HelpCase]) {
    let ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<Vec<_>>();
    let sorted = ids.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        sorted.len(),
        "command corpus case IDs must be unique"
    );
    assert_eq!(
        ids,
        sorted.into_iter().collect::<Vec<_>>(),
        "command corpus must be emitted in deterministic lexical order"
    );
}

fn assert_runtime_corpus_coverage(manifest: &OperatorManifest, cases: &[RuntimeCase]) {
    let manifest_by_name = manifest
        .subcommands
        .iter()
        .map(|row| (row.name.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let case_ids = cases.iter().map(|case| case.id).collect::<BTreeSet<_>>();
    assert_eq!(
        case_ids.len(),
        cases.len(),
        "runtime command corpus case IDs must be unique"
    );
    let case_command_names = cases
        .iter()
        .map(|case| case.command_name)
        .collect::<BTreeSet<_>>();

    for required in [
        "root lookup",
        "root orientation",
        "root --version",
        "root --describe",
        "root --schema",
        "doctor --robot-triage",
        "doctor health",
        "doctor capabilities",
        "doctor robot-docs",
        "geo capabilities",
        "geo inspect",
        "geo ledger",
        "registry providers",
        "registry provider-schema",
        "registry export",
        "registry build",
        "project init",
        "project validate",
        "package pack",
        "package inspect",
        "package verify",
        "package unpack",
        "inbox list",
        "inbox export-review",
        "entity calibrate sweep",
        "entity block preflight",
        "entity profile list",
        "entity profile init",
    ] {
        assert!(
            case_command_names.contains(required),
            "runtime corpus missing required executable surface {required}"
        );
    }

    for case in cases {
        if let Some(row) = manifest_by_name.get(case.command_name) {
            assert_runtime_case_matches_operator_status(case, row);
            if case.expected.exit_code == 0
                && let Some(expected_schema) = expected_json_schema(&case.expected.stdout)
            {
                assert_eq!(
                    row.output_schema.as_deref(),
                    Some(expected_schema),
                    "{} expected schema drifted from operator row",
                    case.id
                );
            }
            assert_side_effect_contract(case, row);
        }
    }
}

fn assert_runtime_case_matches_operator_status(case: &RuntimeCase, row: &OperatorRow) {
    match row.status.as_str() {
        "implemented" => {}
        "unavailable" => {
            assert_eq!(
                case.expected.exit_code, 2,
                "{} targets unavailable operator row {} without refusing",
                case.id, row.name
            );
            assert!(
                expected_json_asserts_eq(&case.expected.stdout, "outcome", &json!("REFUSAL")),
                "{} targets unavailable operator row {} without asserting REFUSAL outcome",
                case.id,
                row.name
            );
            assert!(
                expected_json_asserts_eq(
                    &case.expected.stdout,
                    "refusal.detail.status",
                    &json!("planned_not_implemented")
                ),
                "{} targets unavailable operator row {} without asserting planned_not_implemented",
                case.id,
                row.name
            );
            assert!(
                expected_json_asserts_path(&case.expected.stdout, "refusal.next_command"),
                "{} targets unavailable operator row {} without asserting next_command",
                case.id,
                row.name
            );
        }
        status => panic!(
            "{} targets operator row {} with unsupported status {status}",
            case.id, row.name
        ),
    }
}

fn expected_json_asserts_eq(stdout: &StdoutExpectation, path: &str, expected: &Value) -> bool {
    match stdout {
        StdoutExpectation::Json(expectation) => expectation.assertions.iter().any(|assertion| {
            matches!(assertion, JsonAssertion::Eq(actual_path, actual) if *actual_path == path && actual == expected)
        }),
        _ => false,
    }
}

fn expected_json_asserts_path(stdout: &StdoutExpectation, path: &str) -> bool {
    match stdout {
        StdoutExpectation::Json(expectation) => expectation.assertions.iter().any(|assertion| {
            matches!(
                assertion,
                JsonAssertion::Eq(actual_path, _)
                    | JsonAssertion::ArrayLen(actual_path, _)
                    | JsonAssertion::ArrayNonEmpty(actual_path)
                    | JsonAssertion::HashPrefix(actual_path)
                    | JsonAssertion::NotEq(actual_path, _) if *actual_path == path
            )
        }),
        _ => false,
    }
}

fn expected_json_schema(stdout: &StdoutExpectation) -> Option<&'static str> {
    match stdout {
        StdoutExpectation::Json(expectation) => Some(expectation.schema),
        _ => None,
    }
}

fn assert_side_effect_contract(case: &RuntimeCase, row: &OperatorRow) {
    let Some(side_effects) = &row.side_effects else {
        return;
    };
    let writes_anything = side_effects.writes_registry_files
        || side_effects.writes_work_dir
        || side_effects.writes_project_files
        || side_effects.writes_output_files;
    let observed_writes = case
        .expected
        .mutations
        .iter()
        .any(|mutation| matches!(mutation, MutationExpectation::Exists(_)));
    assert_eq!(
        observed_writes, writes_anything,
        "{} mutation expectation disagrees with operator side_effects",
        case.id
    );
    assert_eq!(
        side_effects.uses_network,
        row.safety
            .as_ref()
            .is_some_and(|safety| safety.network_class == "ExplicitExternalProvider"),
        "{} network side effect and safety class must agree",
        case.id
    );
    if row.read_only == Some(true) {
        assert!(
            !writes_anything && !side_effects.uses_network,
            "{} read_only command declares mutation or network side effects",
            case.id
        );
    }
    if let Some(safety) = &row.safety
        && observed_writes
    {
        assert_ne!(
            safety.mutation_class, "ReadOnly",
            "{} writes despite ReadOnly safety declaration",
            case.id
        );
    }
}

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

struct RuntimeHarness {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    work: PathBuf,
}

impl RuntimeHarness {
    fn new() -> Self {
        let temp = tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let work = temp.path().join("work");
        fs::create_dir(&home).expect("home dir");
        fs::create_dir(&work).expect("work dir");
        Self {
            _temp: temp,
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            home,
            work,
        }
    }

    fn command(&self) -> Command {
        let mut command = canon_command();
        command.env("HOME", &self.home);
        command.env("USERPROFILE", &self.home);
        command.env_remove("CANON_REGISTRY_INDEX_MODE");
        command
    }

    fn runtime_cases(&self) -> Vec<RuntimeCase> {
        let lookup_input = self.root.join("tests/fixtures/csv/all_resolved.csv");
        let lookup_registry = self.root.join("tests/fixtures/registries/cusip-isin");
        self.prewarm_lookup_registry(&lookup_input, &lookup_registry);
        let wrong_column_input = self.root.join("tests/fixtures/csv/wrong_column.csv");
        let export_seed = self.work.join("registry-export.csv");
        let dbt_schema = self.work.join("registry-export.schema.yml");
        let dbt_test = self.work.join("registry-export.tests.sql");
        let seed = self.work.join("seed.csv");
        fs::write(&seed, "cusip\nAAPL\n").expect("seed csv");
        let registry_build_dir = self.work.join("built-registry");
        let project_dir = self.work.join("project");
        let project_manifest = project_dir.join("canon.project.toml");
        let package_root = self.package_root();
        let package_archive = self.work.join("pkg.canonpkg");
        let package_unpack = self.work.join("pkg-unpack");
        fs::create_dir(&package_unpack).expect("package unpack target");
        let inbox = self.inbox_fixture();
        let review = self.work.join("review.json");
        let profile_output = self.work.join("regab_firm_identity.yaml");
        let schema_path = self.work.join("mapping.schema.json");
        let calibrate_result = self.work.join("calibrate-result.jsonl");
        let calibrate_gold = self.work.join("calibrate-gold.jsonl");
        let calibrate_strategy = self.work.join("calibrate-strategy.yaml");
        let block_preflight_rows = self.work.join("block-preflight-rows.csv");
        let block_preflight_strategy = self.work.join("block-preflight-strategy.yaml");
        fs::write(
            &calibrate_result,
            concat!(
                "{\"left_id\":\"A\",\"right_id\":\"B\",\"match_score\":90,\"backbone_score\":90,\"attach_score\":90,\"abstain_margin\":90,\"ambiguity_gap\":90}\n",
                "{\"left_id\":\"C\",\"right_id\":\"D\",\"match_score\":80,\"backbone_score\":80,\"attach_score\":80,\"abstain_margin\":80,\"ambiguity_gap\":80}\n",
                "{\"left_id\":\"E\",\"right_id\":\"F\",\"match_score\":70,\"backbone_score\":70,\"attach_score\":70,\"abstain_margin\":70,\"ambiguity_gap\":70}\n"
            ),
        )
        .expect("calibrate result fixture");
        fs::write(
            &calibrate_gold,
            concat!(
                "{\"left_id\":\"A\",\"right_id\":\"B\",\"label\":\"same\"}\n",
                "{\"left_id\":\"C\",\"right_id\":\"D\",\"label\":\"same\"}\n",
                "{\"left_id\":\"E\",\"right_id\":\"F\",\"label\":\"distinct\",\"severity\":\"critical\"}\n"
            ),
        )
        .expect("calibrate gold fixture");
        fs::write(
            &calibrate_strategy,
            "strategy_id: contract-corpus\nsolver:\n  backbone_score_min: 32\n  attach_score_min: 28\n",
        )
        .expect("calibrate strategy fixture");
        fs::write(
            &block_preflight_rows,
            concat!(
                "source_row_id,deal_id,loan_id,property_id,raw_tenant_name\n",
                "r001,D1,L1,P1,John Smith LLC\n",
                "r002,D2,L2,P2,John Smith LLC\n",
                "r003,D3,L3,P3,Sears Roebuck\n",
                "r004,D4,L4,P4,Kmart\n",
            ),
        )
        .expect("block preflight rows fixture");
        fs::write(
            &block_preflight_strategy,
            "strategy_id: contract-block-preflight\nstrategy_version: 1\n",
        )
        .expect("block preflight strategy fixture");

        vec![
            RuntimeCase {
                id: "root_orientation_refuses_without_pipeline_stdout",
                command_name: "root orientation",
                args: Vec::new(),
                expected: RuntimeExpectation {
                    exit_code: 2,
                    stdout: StdoutExpectation::Empty,
                    stderr: StderrExpectation::Contains("No input given"),
                    mutations: Vec::new(),
                },
            },
            RuntimeCase {
                id: "root_version_reports_package_version",
                command_name: "root --version",
                args: vec!["--version".to_string()],
                expected: RuntimeExpectation {
                    exit_code: 0,
                    stdout: StdoutExpectation::TextContains("canon "),
                    stderr: StderrExpectation::Empty,
                    mutations: Vec::new(),
                },
            },
            RuntimeCase {
                id: "root_lookup_resolved_json",
                command_name: "root lookup",
                args: vec![
                    path_arg(&lookup_input),
                    "--registry".to_string(),
                    path_arg(&lookup_registry),
                    "--column".to_string(),
                    "cusip".to_string(),
                    "--no-witness".to_string(),
                ],
                expected: RuntimeExpectation::json(0, "canon.v0", SchemaField::Version)
                    .assert_eq("outcome", json!("RESOLVED"))
                    .assert_eq("summary.total", json!(3))
                    .assert_eq("summary.resolved", json!(3))
                    .assert_eq("summary.unresolved", json!(0))
                    .with_stderr(StderrExpectation::Empty)
                    .with_mutation(MutationExpectation::Unchanged(
                        lookup_input.clone(),
                        fs::read(&lookup_input).expect("lookup fixture bytes"),
                    )),
            },
            RuntimeCase {
                id: "root_lookup_missing_column_refusal",
                command_name: "root lookup",
                args: vec![
                    path_arg(&wrong_column_input),
                    "--registry".to_string(),
                    path_arg(&lookup_registry),
                    "--column".to_string(),
                    "cusip".to_string(),
                    "--no-witness".to_string(),
                ],
                expected: RuntimeExpectation::json(2, "canon.v0", SchemaField::Version)
                    .assert_eq("outcome", json!("REFUSAL"))
                    .assert_eq("refusal.code", json!("E_COLUMN_NOT_FOUND"))
                    .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "root_describe_exact_manifest",
                command_name: "root --describe",
                args: vec!["--describe".to_string()],
                expected: RuntimeExpectation {
                    exit_code: 0,
                    stdout: StdoutExpectation::ExactOperatorDescribe,
                    stderr: StderrExpectation::Empty,
                    mutations: Vec::new(),
                },
            },
            RuntimeCase {
                id: "root_schema_maps_mapping_contract",
                command_name: "root --schema",
                args: vec!["--schema".to_string()],
                expected: RuntimeExpectation::json(0, "Canon Output Schema", SchemaField::Title)
                    .assert_eq("type", json!("object"))
                    .assert_eq("properties.version.const", json!("canon.v0"))
                    .with_stderr(StderrExpectation::Empty)
                    .with_mutation(MutationExpectation::Missing(schema_path)),
            },
            RuntimeCase {
                id: "doctor_robot_triage_json",
                command_name: "doctor --robot-triage",
                args: args(["doctor", "--robot-triage"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon.doctor.triage.v1",
                    SchemaField::Schema,
                )
                .assert_eq("contract", json!("cmdrvl.read_only_doctor.v1"))
                .assert_eq("ok", json!(true))
                .assert_eq("read_only", json!(true))
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "doctor_health_json",
                command_name: "doctor health",
                args: args(["doctor", "health", "--json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon.doctor.health.v1",
                    SchemaField::Schema,
                )
                .assert_eq("tool", json!("canon"))
                .assert_eq("ok", json!(true))
                .assert_eq("operator_manifest.blake3", json!(stable_operator_digest()))
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "doctor_capabilities_json",
                command_name: "doctor capabilities",
                args: args(["doctor", "capabilities", "--json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon.doctor.capabilities.v1",
                    SchemaField::Schema,
                )
                .assert_array_non_empty("commands")
                .assert_eq("fixers", json!([]))
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "doctor_robot_docs_text",
                command_name: "doctor robot-docs",
                args: args(["doctor", "robot-docs"]),
                expected: RuntimeExpectation {
                    exit_code: 0,
                    stdout: StdoutExpectation::TextContains("canon doctor health --json"),
                    stderr: StderrExpectation::Empty,
                    mutations: Vec::new(),
                },
            },
            RuntimeCase {
                id: "geo_capabilities_json",
                command_name: "geo capabilities",
                args: args(["geo", "capabilities", "--emit", "json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon_geo_capabilities.v0",
                    SchemaField::Version,
                )
                .assert_array_non_empty("commands.implemented")
                .assert_array_non_empty("contracts.implemented")
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "geo_inspect_planned_refusal",
                command_name: "geo inspect",
                args: args(["geo", "inspect"]),
                expected: RuntimeExpectation::json(2, "canon.v0", SchemaField::Version)
                    .assert_eq("outcome", json!("REFUSAL"))
                    .assert_eq("refusal.code", json!("E_GEO_COMMAND_UNAVAILABLE"))
                    .assert_ne("refusal.code", json!("E_ENTITY_ARTIFACT_CONTRACT"))
                    .assert_eq("refusal.detail.command", json!("canon geo inspect"))
                    .assert_eq("refusal.detail.status", json!("planned_not_implemented"))
                    .assert_eq(
                        "refusal.next_command",
                        json!("canon geo capabilities --emit json"),
                    )
                    .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "geo_ledger_planned_refusal",
                command_name: "geo ledger",
                args: args(["geo", "ledger"]),
                expected: RuntimeExpectation::json(2, "canon.v0", SchemaField::Version)
                    .assert_eq("outcome", json!("REFUSAL"))
                    .assert_eq("refusal.code", json!("E_GEO_COMMAND_UNAVAILABLE"))
                    .assert_ne("refusal.code", json!("E_ENTITY_ARTIFACT_CONTRACT"))
                    .assert_eq("refusal.detail.command", json!("canon geo ledger"))
                    .assert_eq("refusal.detail.status", json!("planned_not_implemented"))
                    .assert_eq(
                        "refusal.next_command",
                        json!("canon geo capabilities --emit json"),
                    )
                    .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "registry_providers_json",
                command_name: "registry providers",
                args: args(["registry", "providers", "--emit", "json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon_registry_providers.v0",
                    SchemaField::Version,
                )
                .assert_eq("providers.0.id", json!("mock"))
                .assert_eq("providers.1.id", json!("openfigi"))
                .assert_array_non_empty("providers.0.seed_columns")
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "registry_provider_schema_openfigi_json",
                command_name: "registry provider-schema",
                args: args(["registry", "provider-schema", "openfigi", "--emit", "json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon_registry_provider_schema.v0",
                    SchemaField::Version,
                )
                .assert_eq("id", json!("openfigi"))
                .assert_array_non_empty("options")
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "registry_provider_schema_unknown_refusal",
                command_name: "registry provider-schema",
                args: args(["registry", "provider-schema", "bogus", "--emit", "json"]),
                expected: RuntimeExpectation::json(2, "canon.v0", SchemaField::Version)
                    .assert_eq("outcome", json!("REFUSAL"))
                    .assert_eq("refusal.code", json!("E_PARSE"))
                    .assert_eq(
                        "refusal.next_command",
                        json!("canon registry providers --emit json"),
                    )
                    .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "registry_export_dbt_seed_writes_declared_files",
                command_name: "registry export",
                args: vec![
                    "registry".to_string(),
                    "export".to_string(),
                    "--format".to_string(),
                    "dbt-seed".to_string(),
                    "--registry".to_string(),
                    path_arg(&lookup_registry),
                    "--namespace".to_string(),
                    "contract_corpus".to_string(),
                    "--out".to_string(),
                    path_arg(&export_seed),
                    "--schema-out".to_string(),
                    path_arg(&dbt_schema),
                    "--anti-collapse-test-out".to_string(),
                    path_arg(&dbt_test),
                    "--emit".to_string(),
                    "json".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon_registry_export.v0",
                    SchemaField::Version,
                )
                .assert_eq("format", json!("dbt-seed"))
                .assert_eq("registry.id", json!("cusip-isin"))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(export_seed))
                .with_mutation(MutationExpectation::Exists(dbt_schema))
                .with_mutation(MutationExpectation::Exists(dbt_test)),
            },
            RuntimeCase {
                id: "registry_build_mock_materializes_registry",
                command_name: "registry build",
                args: vec![
                    "registry".to_string(),
                    "build".to_string(),
                    "--source".to_string(),
                    "mock".to_string(),
                    "--seed".to_string(),
                    path_arg(&seed),
                    "--seed-column".to_string(),
                    "cusip".to_string(),
                    "--output".to_string(),
                    path_arg(&registry_build_dir),
                    "--version".to_string(),
                    "1.2.3".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon_registry_build.v0",
                    SchemaField::Version,
                )
                .assert_eq("registry.version", json!("1.2.3"))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(
                    registry_build_dir.join("registry.json"),
                ))
                .with_mutation(MutationExpectation::Exists(
                    registry_build_dir.join("_build.json"),
                )),
            },
            RuntimeCase {
                id: "project_init_writes_manifest",
                command_name: "project init",
                args: vec![
                    "project".to_string(),
                    "init".to_string(),
                    path_arg(&project_dir),
                    "--project-id".to_string(),
                    "project.contract.corpus".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.project.cli.v1",
                    SchemaField::SchemaVersion,
                )
                .assert_eq("command", json!("project.init"))
                .assert_eq("project_id", json!("project.contract.corpus"))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(project_manifest.clone())),
            },
            RuntimeCase {
                id: "project_validate_reads_initialized_manifest",
                command_name: "project validate",
                args: vec![
                    "project".to_string(),
                    "validate".to_string(),
                    path_arg(&project_dir),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.project.cli.v1",
                    SchemaField::SchemaVersion,
                )
                .assert_eq("valid", json!(true))
                .assert_eq("manifest.project_id", json!("project.contract.corpus"))
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "package_pack_writes_archive",
                command_name: "package pack",
                args: vec![
                    "package".to_string(),
                    "pack".to_string(),
                    "--root".to_string(),
                    path_arg(&package_root),
                    "--package".to_string(),
                    path_arg(&package_root.join("package.json")),
                    "--out".to_string(),
                    path_arg(&package_archive),
                ],
                expected: RuntimeExpectation {
                    exit_code: 0,
                    stdout: StdoutExpectation::Empty,
                    stderr: StderrExpectation::Empty,
                    mutations: vec![MutationExpectation::Exists(package_archive.clone())],
                },
            },
            RuntimeCase {
                id: "package_inspect_archive_json",
                command_name: "package inspect",
                args: vec![
                    "package".to_string(),
                    "inspect".to_string(),
                    path_arg(&package_archive),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.local.package.archive.v1",
                    SchemaField::None,
                )
                .assert_eq("package.package_id", json!("pkg.contract.corpus"))
                .assert_eq("package.schema_version", json!("canon.strategy.package.v1"))
                .assert_array_len("inventory", 3)
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "package_verify_archive_json",
                command_name: "package verify",
                args: vec![
                    "package".to_string(),
                    "verify".to_string(),
                    path_arg(&package_archive),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.local.package.archive.v1",
                    SchemaField::None,
                )
                .assert_eq("verified_files", json!(3))
                .assert_hash_prefix("package_content_digest")
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "package_unpack_writes_target_files",
                command_name: "package unpack",
                args: vec![
                    "package".to_string(),
                    "unpack".to_string(),
                    path_arg(&package_archive),
                    "--target".to_string(),
                    path_arg(&package_unpack),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.local.package.archive.v1",
                    SchemaField::None,
                )
                .assert_eq("verified_files", json!(3))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(
                    package_unpack.join("package.json"),
                ))
                .with_mutation(MutationExpectation::Exists(
                    package_unpack.join("bin/run.sh"),
                )),
            },
            RuntimeCase {
                id: "inbox_list_json_is_read_only",
                command_name: "inbox list",
                args: vec![
                    "inbox".to_string(),
                    "list".to_string(),
                    "--inbox".to_string(),
                    path_arg(&inbox),
                    "--limit".to_string(),
                    "1".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.inbox.list.v1",
                    SchemaField::SchemaVersion,
                )
                .assert_eq("page.returned", json!(1))
                .assert_eq("identity_status", json!("no_identity_decision"))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Unchanged(
                    inbox.clone(),
                    fs::read(&inbox).expect("inbox fixture bytes"),
                )),
            },
            RuntimeCase {
                id: "inbox_export_review_writes_review_artifact",
                command_name: "inbox export-review",
                args: vec![
                    "inbox".to_string(),
                    "export-review".to_string(),
                    "--inbox".to_string(),
                    path_arg(&inbox),
                    "--out".to_string(),
                    path_arg(&review),
                    "--limit".to_string(),
                    "1".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.inbox.review_export.v1",
                    SchemaField::SchemaVersion,
                )
                .assert_array_len("decisions", 1)
                .assert_eq("identity_status", json!("no_identity_decision"))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(review)),
            },
            RuntimeCase {
                id: "entity_profile_list_json",
                command_name: "entity profile list",
                args: args(["entity", "profile", "list", "--emit", "json"]),
                expected: RuntimeExpectation::json(
                    0,
                    "canon_entity_profile_templates.v0",
                    SchemaField::Version,
                )
                .assert_array_non_empty("profiles")
                .assert_eq("profiles.0.profile", json!("cmbs_tenant_label"))
                .with_stderr(StderrExpectation::Empty),
            },
            RuntimeCase {
                id: "entity_profile_init_writes_template",
                command_name: "entity profile init",
                args: vec![
                    "entity".to_string(),
                    "profile".to_string(),
                    "init".to_string(),
                    "regab_firm_identity".to_string(),
                    "--output".to_string(),
                    path_arg(&profile_output),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon_entity_profile_templates.v0",
                    SchemaField::Version,
                )
                .assert_eq("profile", json!("regab_firm_identity"))
                .assert_eq("template_valid", json!(true))
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Exists(profile_output)),
            },
            RuntimeCase {
                id: "entity_calibrate_sweep_json_is_read_only",
                command_name: "entity calibrate sweep",
                args: vec![
                    "entity".to_string(),
                    "calibrate".to_string(),
                    "sweep".to_string(),
                    path_arg(&calibrate_result),
                    "--gold".to_string(),
                    path_arg(&calibrate_gold),
                    "--strategy".to_string(),
                    path_arg(&calibrate_strategy),
                    "--emit".to_string(),
                    "json".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon.entity.calibrate_sweep.v0",
                    SchemaField::Version,
                )
                .assert_eq("read_only", json!(true))
                .assert_eq("writes_performed", json!(false))
                .assert_eq("metric_units", json!("integer_basis_points"))
                .assert_eq("quality_contract", json!("canon.entity.quality.v1"))
                .assert_eq("inputs.labeled_pair_count", json!(3))
                .assert_eq("recommendation.status", json!("recommended"))
                .assert_eq(
                    "recommendation.selected_thresholds.match_threshold",
                    json!(80),
                )
                .assert_eq(
                    "recommendation.selected_metrics.auto_accept_rate_basis_points",
                    json!(6667),
                )
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Unchanged(
                    calibrate_strategy.clone(),
                    fs::read(&calibrate_strategy).expect("calibrate strategy bytes"),
                )),
            },
            RuntimeCase {
                id: "entity_block_preflight_json_is_read_only",
                command_name: "entity block preflight",
                args: vec![
                    "entity".to_string(),
                    "block".to_string(),
                    "preflight".to_string(),
                    path_arg(&block_preflight_rows),
                    "--profile".to_string(),
                    "cmbs_tenant_label".to_string(),
                    "--strategy".to_string(),
                    path_arg(&block_preflight_strategy),
                    "--sample-pct".to_string(),
                    "100".to_string(),
                    "--emit".to_string(),
                    "json".to_string(),
                ],
                expected: RuntimeExpectation::json(
                    0,
                    "canon_entity_block_preflight.v1",
                    SchemaField::Version,
                )
                .assert_eq("sample.exact", json!(true))
                .assert_eq("sample.requested_pct", json!(100))
                .assert_eq("budget_verdict.status", json!("pass"))
                .assert_array_non_empty("operators")
                .assert_array_non_empty("top_blocks")
                .with_stderr(StderrExpectation::Empty)
                .with_mutation(MutationExpectation::Unchanged(
                    block_preflight_rows.clone(),
                    fs::read(&block_preflight_rows).expect("block preflight rows bytes"),
                ))
                .with_mutation(MutationExpectation::Unchanged(
                    block_preflight_strategy.clone(),
                    fs::read(&block_preflight_strategy).expect("block preflight strategy bytes"),
                )),
            },
            RuntimeCase {
                id: "package_push_missing_remote_args_refuses_before_network",
                command_name: "package push",
                args: args([
                    "package",
                    "push",
                    "--archive",
                    "/definitely/missing/archive",
                ]),
                expected: RuntimeExpectation {
                    exit_code: 2,
                    stdout: StdoutExpectation::Empty,
                    stderr: StderrExpectation::Contains("required"),
                    mutations: Vec::new(),
                },
            },
        ]
    }

    fn prewarm_lookup_registry(&self, input: &Path, registry: &Path) {
        let output = self
            .command()
            .args([
                path_arg(input),
                "--registry".to_string(),
                path_arg(registry),
                "--column".to_string(),
                "cusip".to_string(),
                "--no-witness".to_string(),
            ])
            .output()
            .expect("prewarm lookup registry cache");
        assert_eq!(
            output.status.code(),
            Some(0),
            "prewarm lookup cache failed\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout: Value =
            serde_json::from_slice(&output.stdout).expect("prewarm lookup stdout is JSON");
        assert_eq!(stdout["outcome"], "RESOLVED");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("entry_count"),
            "prewarm lookup exposed inconsistent registry fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn package_root(&self) -> PathBuf {
        let root = self.work.join("package-root");
        fs::create_dir_all(root.join("bin")).expect("package bin dir");
        fs::write(root.join("README.md"), b"contract corpus\n").expect("package readme");
        fs::write(root.join("bin/run.sh"), b"#!/bin/sh\n").expect("package script");
        let package_json = json!({
            "schema_version": "canon.strategy.package.v1",
            "package_id": "pkg.contract.corpus",
            "package_version": "1.0.0",
            "content_digest": "",
            "license_expression": "MIT",
            "capabilities": ["read_registry"],
            "dependency_references": [],
            "provenance": {
                "source": "tests/command_contract.rs",
                "revision": "bd-ndfh"
            }
        });
        fs::write(
            root.join("package.json"),
            canonical_package_bytes(package_json),
        )
        .expect("package json");
        root
    }

    fn inbox_fixture(&self) -> PathBuf {
        let inbox = self.work.join("inbox.json");
        let artifact = json!({
            "version": "canon.unresolved.inbox.v1",
            "view": "redacted",
            "artifact_content_hash": "",
            "policy": {
                "policy_id": "policy.contract.corpus",
                "raw_value_retention": "omit",
                "default_export_mode": "redacted",
                "merge_mode": "strict"
            },
            "summary": {},
            "items": [
                sample_inbox_item("alpha", "exact_lookup", "no_matching_rule", 2),
                sample_inbox_item("beta", "cluster_abstention", "ambiguous_candidates", 1)
            ]
        });
        fs::write(
            &inbox,
            serde_json::to_vec(&artifact).expect("inbox serializes"),
        )
        .expect("inbox fixture");
        inbox
    }
}

impl RuntimeExpectation {
    fn json(exit_code: i32, schema: &'static str, schema_field: SchemaField) -> Self {
        Self {
            exit_code,
            stdout: StdoutExpectation::Json(JsonExpectation {
                schema,
                schema_field,
                assertions: Vec::new(),
            }),
            stderr: StderrExpectation::Empty,
            mutations: Vec::new(),
        }
    }

    fn assert_eq(mut self, path: &'static str, expected: Value) -> Self {
        if let StdoutExpectation::Json(expectation) = &mut self.stdout {
            expectation
                .assertions
                .push(JsonAssertion::Eq(path, expected));
        } else {
            panic!("JSON assertion added to non-JSON runtime expectation");
        }
        self
    }

    fn assert_array_len(mut self, path: &'static str, len: usize) -> Self {
        if let StdoutExpectation::Json(expectation) = &mut self.stdout {
            expectation
                .assertions
                .push(JsonAssertion::ArrayLen(path, len));
        } else {
            panic!("JSON assertion added to non-JSON runtime expectation");
        }
        self
    }

    fn assert_array_non_empty(mut self, path: &'static str) -> Self {
        if let StdoutExpectation::Json(expectation) = &mut self.stdout {
            expectation
                .assertions
                .push(JsonAssertion::ArrayNonEmpty(path));
        } else {
            panic!("JSON assertion added to non-JSON runtime expectation");
        }
        self
    }

    fn assert_hash_prefix(mut self, path: &'static str) -> Self {
        if let StdoutExpectation::Json(expectation) = &mut self.stdout {
            expectation.assertions.push(JsonAssertion::HashPrefix(path));
        } else {
            panic!("JSON assertion added to non-JSON runtime expectation");
        }
        self
    }

    fn assert_ne(mut self, path: &'static str, unexpected: Value) -> Self {
        if let StdoutExpectation::Json(expectation) = &mut self.stdout {
            expectation
                .assertions
                .push(JsonAssertion::NotEq(path, unexpected));
        } else {
            panic!("JSON assertion added to non-JSON runtime expectation");
        }
        self
    }

    fn with_stderr(mut self, stderr: StderrExpectation) -> Self {
        self.stderr = stderr;
        self
    }

    fn with_mutation(mut self, mutation: MutationExpectation) -> Self {
        self.mutations.push(mutation);
        self
    }
}

impl SchemaField {
    fn assert(self, value: &Value, expected: &str, case_id: &str) {
        let (field, actual) = match self {
            Self::None => return,
            Self::Schema => ("schema", &value["schema"]),
            Self::Title => ("title", &value["title"]),
            Self::Version => ("version", &value["version"]),
            Self::SchemaVersion => ("schema_version", &value["schema_version"]),
        };
        assert_eq!(
            actual, expected,
            "{} stdout JSON {} did not match contract schema",
            case_id, field
        );
    }
}

fn assert_stdout(case_id: &str, stdout: &[u8], expectation: &StdoutExpectation) {
    match expectation {
        StdoutExpectation::Empty => assert!(
            stdout.is_empty(),
            "{case_id} expected empty stdout, got {}",
            String::from_utf8_lossy(stdout)
        ),
        StdoutExpectation::TextContains(needle) => {
            let text = String::from_utf8(stdout.to_vec())
                .unwrap_or_else(|error| panic!("{case_id} stdout is not UTF-8: {error}"));
            assert!(
                text.contains(needle),
                "{case_id} stdout did not contain {needle:?}:\n{text}"
            );
        }
        StdoutExpectation::ExactOperatorDescribe => {
            assert_eq!(
                stdout,
                format!("{OPERATOR_JSON}\n").as_bytes(),
                "{case_id} --describe output drifted from operator.json bytes"
            );
        }
        StdoutExpectation::Json(expectation) => {
            let value: Value = serde_json::from_slice(stdout)
                .unwrap_or_else(|error| panic!("{case_id} stdout is not JSON: {error}"));
            expectation
                .schema_field
                .assert(&value, expectation.schema, case_id);
            for assertion in &expectation.assertions {
                assert_json(case_id, &value, assertion);
            }
        }
    }
}

fn assert_json(case_id: &str, value: &Value, assertion: &JsonAssertion) {
    match assertion {
        JsonAssertion::Eq(path, expected) => assert_eq!(
            value_at(value, path),
            Some(expected),
            "{} JSON path {} mismatch\nstdout={}",
            case_id,
            path,
            serde_json::to_string_pretty(value).expect("stdout JSON renders")
        ),
        JsonAssertion::ArrayLen(path, expected_len) => {
            let Some(array) = value_at(value, path).and_then(Value::as_array) else {
                panic!("{case_id} JSON path {path} is not an array");
            };
            assert_eq!(
                array.len(),
                *expected_len,
                "{case_id} JSON path {path} length mismatch"
            );
        }
        JsonAssertion::ArrayNonEmpty(path) => {
            let Some(array) = value_at(value, path).and_then(Value::as_array) else {
                panic!("{case_id} JSON path {path} is not an array");
            };
            assert!(
                !array.is_empty(),
                "{case_id} JSON path {path} should not be empty"
            );
        }
        JsonAssertion::HashPrefix(path) => {
            let Some(actual) = value_at(value, path).and_then(Value::as_str) else {
                panic!("{case_id} JSON path {path} is not a string");
            };
            assert!(
                actual.starts_with("blake3:") || actual.starts_with("sha256:"),
                "{case_id} JSON path {path} is not a supported digest: {actual}"
            );
        }
        JsonAssertion::NotEq(path, unexpected) => assert_ne!(
            value_at(value, path),
            Some(unexpected),
            "{} JSON path {} unexpectedly matched {}\nstdout={}",
            case_id,
            path,
            unexpected,
            serde_json::to_string_pretty(value).expect("stdout JSON renders")
        ),
    }
}

fn value_at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        if let Ok(index) = segment.parse::<usize>() {
            current = current.as_array()?.get(index)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

fn assert_stderr(case_id: &str, stderr: &[u8], expectation: &StderrExpectation) {
    match expectation {
        StderrExpectation::Empty => assert!(
            stderr.is_empty(),
            "{case_id} expected empty stderr, got {}",
            String::from_utf8_lossy(stderr)
        ),
        StderrExpectation::Contains(needle) => {
            let text = String::from_utf8(stderr.to_vec())
                .unwrap_or_else(|error| panic!("{case_id} stderr is not UTF-8: {error}"));
            assert!(
                text.contains(needle),
                "{case_id} stderr did not contain {needle:?}:\n{text}"
            );
        }
    }
}

fn assert_mutation(case_id: &str, mutation: &MutationExpectation) {
    match mutation {
        MutationExpectation::Exists(path) => {
            assert!(path.exists(), "{case_id} did not create {}", path.display());
        }
        MutationExpectation::Missing(path) => {
            assert!(
                !path.exists(),
                "{case_id} unexpectedly created {}",
                path.display()
            );
        }
        MutationExpectation::Unchanged(path, before) => {
            let after = fs::read(path).unwrap_or_else(|error| {
                panic!("{case_id} failed reading {}: {error}", path.display())
            });
            assert_eq!(
                &after,
                before,
                "{} mutated read-only input {}",
                case_id,
                path.display()
            );
        }
    }
}

fn stable_operator_digest() -> String {
    let manifest: Value = serde_json::from_str(OPERATOR_JSON).expect("operator JSON parses");
    stable_manifest_digest(&manifest)
}

fn canonical_package_bytes(mut value: Value) -> Vec<u8> {
    value["content_digest"] = Value::String(String::new());
    let digest = format!(
        "blake3:{}",
        blake3::hash(&serde_json::to_vec(&value).expect("package digest view serializes")).to_hex()
    );
    value["content_digest"] = Value::String(digest);
    serde_json::to_vec(&value).expect("canonical package JSON serializes")
}

fn sample_inbox_item(
    label: &str,
    event_kind: &str,
    reason_code: &str,
    occurrence_count: usize,
) -> Value {
    let occurrences = (0..occurrence_count)
        .map(|index| {
            json!({
                "project_ref": format!("project.{}", index),
                "run_ref": format!("run.{}", index),
                "source_ref": format!("source.{label}"),
                "record_ref": format!("row-{label}-{index}"),
                "seen_at": format!("2026-07-10T{:02}:00:00Z", index + 1)
            })
        })
        .collect::<Vec<_>>();
    json!({
        "event_key": "",
        "event_kind": event_kind,
        "reason_code": reason_code,
        "field_name": "counterparty",
        "field_role": "name_field",
        "profile_ref": {
            "profile_id": "profile.contract",
            "profile_version": "1.0.0"
        },
        "surface_fingerprints": [
            {
                "normalizer_id": "fixture.trim.v1",
                "surface_role": "name_field",
                "fingerprint": format!("blake3:{}", blake3::hash(label.as_bytes()).to_hex())
            }
        ],
        "namespace_hints": [
            {
                "namespace": "registry.counterparty",
                "source": "contract-corpus"
            }
        ],
        "candidate_summary": {
            "status": "ambiguous",
            "candidate_count": 2,
            "best_score_band": "medium",
            "rejection_reasons": ["fixture"]
        },
        "first_seen_at": "",
        "last_seen_at": "",
        "occurrence_summary": {},
        "occurrences": occurrences,
        "privacy_class": "internal"
    })
}

fn args<const N: usize>(items: [&str; N]) -> Vec<String> {
    items.into_iter().map(str::to_string).collect()
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
