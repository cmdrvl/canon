#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    cli::Cli,
    operator::{
        OperatorManifestValidationReport, public_leaf_commands_from, public_leaf_long_flags_from,
        stable_manifest_digest, validate_operator_manifest_json,
    },
};
use clap::CommandFactory;
use serde_json::{Value, json};

const OPERATOR_JSON: &str = include_str!("../operator.json");
const README_MD: &str = include_str!("../README.md");
const PLAN_CANON_MD: &str = include_str!("../docs/PLAN_CANON.md");

#[test]
fn compiled_clap_describe_and_operator_contract_match_fail_closed() {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("--describe")
        .output()
        .expect("run canon --describe");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        format!("{OPERATOR_JSON}\n").into_bytes(),
        "canon --describe must emit checked operator.json bytes plus one trailing newline"
    );

    assert_report_ok(validate_operator_manifest_json(
        &Cli::command(),
        OPERATOR_JSON,
    ));
}

#[test]
fn operator_contract_mutations_are_rejected_by_core_validator() {
    assert_report_ok(validate_operator_manifest_json(
        &Cli::command(),
        OPERATOR_JSON,
    ));
    let valid = operator_manifest();

    assert!(
        report_for_value(remove_command(valid.clone(), "registry providers"))
            .missing_leaf_commands
            .contains(&"registry providers".to_string())
    );

    let omitted_flag =
        report_for_value(remove_option(valid.clone(), "registry providers", "--emit"));
    assert!(omitted_flag.flag_drifts.iter().any(|drift| {
        drift.command == "registry providers" && drift.missing_flags.contains(&"emit".to_string())
    }));

    for field in [
        "output_schema",
        "status",
        "exit_codes",
        "side_effects",
        "safety",
        "recovery",
    ] {
        assert_missing_field(
            remove_nested_command_field(valid.clone(), "registry providers", &[field]),
            "registry providers",
            field,
        );
    }

    let read_only_drift = report_for_value(set_nested_command_field(
        valid.clone(),
        "registry providers",
        &["read_only"],
        json!(false),
    ));
    assert_behavior_drift(read_only_drift, "registry providers", "read_only");

    let safety_drift = report_for_value(set_nested_command_field(
        valid.clone(),
        "registry providers",
        &["safety", "mutation_class"],
        json!("RegistryMutation"),
    ));
    assert_behavior_drift(safety_drift, "registry providers", "safety.mutation_class");

    let recovery_drift = report_for_value(remove_nested_command_field(
        valid,
        "registry provider-schema",
        &["recovery", "next_command"],
    ));
    assert_behavior_drift(recovery_drift, "registry provider-schema", "next_command");
}

#[test]
fn root_invocation_contract_mutations_are_rejected_by_core_validator() {
    let valid = operator_manifest();

    let positional_drift = report_for_value(set_nested_field(
        valid.clone(),
        &["arguments"],
        json!([{
            "name": "source",
            "type": "file_path",
            "required": true,
            "position": 0,
            "description": "Wrong root positional name"
        }]),
    ));
    assert_report_contains(
        positional_drift,
        &["root arguments", "compiled args", "source"],
    );

    let schema_drift = report_for_value(set_nested_field(
        valid,
        &["invocation", "output_schema"],
        json!("canon.bad.v0"),
    ));
    assert_report_contains(schema_drift, &["invocation.output_schema", "canon.v0"]);
}

#[test]
fn aggregate_doctor_contract_mutations_are_rejected_by_core_validator() {
    let valid = operator_manifest();

    let omitted_robot_triage =
        report_for_value(remove_option(valid.clone(), "doctor", "--robot-triage"));
    assert_flag_drift(omitted_robot_triage, "doctor", "robot-triage");

    let output_schema_drift = report_for_value(set_nested_command_field(
        valid.clone(),
        "doctor",
        &["output_schema"],
        json!("canon.doctor.unreviewed.v1"),
    ));
    let exit_code_drift = report_for_value(remove_nested_command_field(
        valid,
        "doctor",
        &["exit_codes", "2"],
    ));

    let mut failures = Vec::new();
    collect_report_contains_failure(
        &mut failures,
        "doctor output_schema mutation",
        output_schema_drift,
        &["doctor", "output_schema"],
    );
    collect_report_contains_failure(
        &mut failures,
        "doctor exit_codes mutation",
        exit_code_drift,
        &["doctor", "exit_codes"],
    );
    assert!(
        failures.is_empty(),
        "aggregate doctor contract mutations were not rejected:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn safety_declaration_command_mutations_are_rejected_by_core_validator() {
    let valid = operator_manifest();

    let missing = report_for_value(remove_nested_command_field(
        valid.clone(),
        "doctor",
        &["safety", "declaration_command"],
    ));
    assert_report_contains(missing, &["doctor", "declaration_command"]);

    let unknown = report_for_value(set_nested_command_field(
        valid.clone(),
        "doctor",
        &["safety", "declaration_command"],
        json!("not-a-registered-safety-declaration"),
    ));
    assert_report_contains(unknown, &["doctor", "declaration_command"]);

    let wrong = report_for_value(set_nested_command_field(
        valid,
        "doctor",
        &["safety", "declaration_command"],
        json!("package pack"),
    ));
    assert_report_contains(wrong, &["doctor", "package pack"]);
}

#[test]
fn core_contract_digest_and_leaf_order_are_deterministic() {
    let manifest = operator_manifest();
    let first_digest = stable_manifest_digest(&manifest);
    assert_eq!(first_digest, stable_manifest_digest(&manifest));
    assert_eq!(first_digest, stable_manifest_digest(&operator_manifest()));
    assert!(
        first_digest.starts_with("blake3:"),
        "digest must carry its hash algorithm domain"
    );

    let leaves = public_leaf_commands_from(&Cli::command());
    assert_eq!(leaves, public_leaf_commands_from(&Cli::command()));
    assert!(
        leaves.windows(2).all(|pair| pair[0] <= pair[1]),
        "public leaf commands must be emitted in deterministic sorted order"
    );

    for leaf in public_leaf_long_flags_from(&Cli::command()) {
        assert!(
            leaf.long_flags.windows(2).all(|pair| pair[0] <= pair[1]),
            "long flags for {} must be deterministic and sorted",
            leaf.command
        );
    }
}

#[test]
fn readme_and_plan_cli_reference_blocks_cover_compiled_leaves_and_flags() {
    let blocks = [
        (
            "README.md CLI Reference",
            cli_block(README_MD, "## CLI Reference"),
        ),
        (
            "PLAN_CANON.md CLI (v0)",
            cli_block(PLAN_CANON_MD, "## CLI (v0)"),
        ),
    ];
    let mut missing = Vec::new();
    for leaf in public_leaf_long_flags_from(&Cli::command()) {
        let needle = format!("canon {}", leaf.command);
        for (label, block) in &blocks {
            let Some(line) = block.lines().find(|line| line.contains(&needle)) else {
                missing.push(format!("{label}: missing command line {needle}"));
                continue;
            };
            for flag in &leaf.long_flags {
                let flag = format!("--{flag}");
                if !line.contains(&flag) {
                    missing.push(format!("{label}: {needle} line missing {flag}"));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "CLI reference blocks drifted from compiled Clap leaves/flags:\n{}",
        missing.join("\n")
    );
}

#[test]
fn readme_and_plan_reject_retired_org_terms_except_profile_value() {
    for (label, document) in [
        ("README.md", README_MD),
        ("docs/PLAN_CANON.md", PLAN_CANON_MD),
    ] {
        assert_no_retired_org_doc_terms(label, document);
    }
}

#[test]
fn readme_fixture_backed_lookup_example_executes_offline() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = root.join("tests/fixtures/csv/all_resolved.csv");
    let registry = root.join("tests/fixtures/registries/cusip-isin");
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(input)
        .arg("--registry")
        .arg(registry)
        .arg("--column")
        .arg("cusip")
        .arg("--no-witness")
        .output()
        .expect("run fixture-backed README lookup shape");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("lookup stdout is UTF-8 JSON");
    for raw in [
        "037833100",
        "594918104",
        "17275R102",
        "US0378331005",
        "US5949181045",
        "US17275R1023",
    ] {
        assert!(
            !stdout.contains(raw),
            "default README lookup output leaked raw value {raw}: {stdout}"
        );
    }
    let value: Value = serde_json::from_str(&stdout).expect("lookup stdout is JSON");
    assert_eq!(value["redacted"], true);
    assert_eq!(value["summary"]["total"], 3);
    assert_eq!(value["summary"]["resolved"], 3);
    assert_eq!(value["summary"]["unresolved"], 0);
    for mapping in value["mappings"].as_array().expect("mappings array") {
        assert_eq!(mapping["input"], "[REDACTED]");
        assert_eq!(mapping["canonical_id"], "[REDACTED]");
    }

    let audit_output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg("registry")
        .arg("audit")
        .arg(root.join("tests/fixtures/csv/all_resolved.csv"))
        .arg("--registry")
        .arg(root.join("tests/fixtures/registries/cusip-isin"))
        .arg("--column")
        .arg("cusip")
        .arg("--emit")
        .arg("json")
        .output()
        .expect("run fixture-backed README registry audit shape");

    assert_eq!(audit_output.status.code(), Some(0));
    let audit: Value =
        serde_json::from_slice(&audit_output.stdout).expect("registry audit stdout is JSON");
    assert_eq!(audit["version"], "canon_registry_audit.v0");
    assert_eq!(audit["summary"]["total"], 3);
    assert_eq!(audit["summary"]["resolved"], 3);
    assert_eq!(audit["summary"]["unresolved"], 0);
}

fn assert_report_ok(report: OperatorManifestValidationReport) {
    assert!(
        report.ok,
        "operator contract parity failed:\n{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}

fn assert_missing_field(value: Value, command: &str, field: &str) {
    let report = report_for_value(value);
    assert!(
        report
            .missing_required_fields
            .iter()
            .any(|missing| missing.command == command && missing.field == field),
        "missing field {command}.{field} was not reported:\n{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}

fn assert_behavior_drift(report: OperatorManifestValidationReport, command: &str, field: &str) {
    assert!(
        report
            .behavior_drifts
            .iter()
            .any(|drift| drift.command == command && drift.field == field),
        "behavior drift {command}.{field} was not reported:\n{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}

fn assert_flag_drift(report: OperatorManifestValidationReport, command: &str, missing_flag: &str) {
    assert!(
        report.flag_drifts.iter().any(|drift| {
            drift.command == command && drift.missing_flags.contains(&missing_flag.to_string())
        }),
        "flag drift {command}.--{missing_flag} was not reported:\n{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}

fn assert_report_contains(report: OperatorManifestValidationReport, snippets: &[&str]) {
    assert!(
        !report.ok,
        "mutation unexpectedly passed; wanted snippets {snippets:?} in failing report"
    );
    let rendered = serde_json::to_string_pretty(&report).expect("report serializes");
    for snippet in snippets {
        assert!(
            rendered.contains(snippet),
            "failing report did not contain {snippet:?}:\n{rendered}"
        );
    }
}

fn assert_no_retired_org_doc_terms(label: &str, document: &str) {
    let denied_tokens = [
        "canon org",
        "org review",
        "canon_org_",
        "E_ORG_",
        "registries/org",
        "org sidecars",
    ];
    let mut violations = Vec::new();
    for (line_index, line) in document.lines().enumerate() {
        if line.contains("--profile standard|org|strategy|auto") {
            continue;
        }
        let lower_line = line.to_ascii_lowercase();
        for token in denied_tokens {
            if lower_line.contains(&token.to_ascii_lowercase()) {
                violations.push(format!("{}:{}: {}", label, line_index + 1, line.trim()));
                break;
            }
        }
    }
    assert!(
        violations.is_empty(),
        "retired org terminology returned to docs:\n{}",
        violations.join("\n")
    );
}

fn collect_report_contains_failure(
    failures: &mut Vec<String>,
    label: &str,
    report: OperatorManifestValidationReport,
    snippets: &[&str],
) {
    let rendered = serde_json::to_string_pretty(&report).expect("report serializes");
    if report.ok {
        failures.push(format!(
            "{label}: mutation unexpectedly passed; wanted snippets {snippets:?}"
        ));
        return;
    }
    let missing = snippets
        .iter()
        .filter(|snippet| !rendered.contains(**snippet))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        failures.push(format!(
            "{label}: failing report missing snippets {missing:?}:\n{rendered}"
        ));
    }
}

fn report_for_value(value: Value) -> OperatorManifestValidationReport {
    validate_operator_manifest_json(
        &Cli::command(),
        &serde_json::to_string(&value).expect("manifest mutation serializes"),
    )
}

fn operator_manifest() -> Value {
    serde_json::from_str(OPERATOR_JSON).expect("operator.json parses")
}

fn remove_command(mut manifest: Value, name: &str) -> Value {
    let rows = manifest["subcommands"]
        .as_array_mut()
        .expect("subcommands array");
    rows.retain(|row| row.get("name").and_then(Value::as_str) != Some(name));
    manifest
}

fn remove_option(mut manifest: Value, command: &str, flag: &str) -> Value {
    let row = command_row_mut(&mut manifest, command);
    let options = row["options"].as_array_mut().expect("options array");
    options.retain(|option| option.get("flag").and_then(Value::as_str) != Some(flag));
    manifest
}

fn set_nested_field(mut value: Value, path: &[&str], new_value: Value) -> Value {
    let mut current = &mut value;
    for segment in &path[..path.len() - 1] {
        current = current
            .as_object_mut()
            .expect("nested object")
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    current
        .as_object_mut()
        .expect("leaf object")
        .insert(path[path.len() - 1].to_string(), new_value);
    value
}

fn set_nested_command_field(
    mut manifest: Value,
    command: &str,
    path: &[&str],
    value: Value,
) -> Value {
    let mut current = command_row_mut(&mut manifest, command);
    for segment in &path[..path.len() - 1] {
        current = current
            .as_object_mut()
            .expect("nested object")
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    current
        .as_object_mut()
        .expect("leaf object")
        .insert(path[path.len() - 1].to_string(), value);
    manifest
}

fn remove_nested_command_field(mut manifest: Value, command: &str, path: &[&str]) -> Value {
    let mut current = command_row_mut(&mut manifest, command);
    for segment in &path[..path.len() - 1] {
        current = current
            .get_mut(*segment)
            .unwrap_or_else(|| panic!("nested field {segment} present"));
    }
    current
        .as_object_mut()
        .expect("leaf object")
        .remove(path[path.len() - 1]);
    manifest
}

fn command_row_mut<'a>(manifest: &'a mut Value, command: &str) -> &'a mut Value {
    manifest["subcommands"]
        .as_array_mut()
        .expect("subcommands array")
        .iter_mut()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(command))
        .unwrap_or_else(|| panic!("command row {command} present"))
}

fn cli_block<'a>(document: &'a str, heading: &str) -> &'a str {
    let after_heading = document
        .split_once(heading)
        .unwrap_or_else(|| panic!("missing heading {heading}"))
        .1;
    let after_fence = after_heading
        .split_once("```bash")
        .unwrap_or_else(|| panic!("missing bash fence after {heading}"))
        .1;
    after_fence
        .split_once("```")
        .unwrap_or_else(|| panic!("missing closing fence after {heading}"))
        .0
}
