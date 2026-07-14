#![forbid(unsafe_code)]

use assert_cmd::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Deserialize)]
struct NamespaceMigrationFixture {
    schema_version: String,
    gates: Vec<String>,
    lookup_cases: Vec<LookupCase>,
    public_surface: PublicSurface,
}

#[derive(Debug, Deserialize)]
struct LookupCase {
    id: String,
    input: String,
    registry: String,
    column: String,
    pre_migration_golden: String,
    expected_exit_code: i32,
}

#[derive(Debug, Deserialize)]
struct PublicSurface {
    required_help_tokens: Vec<String>,
    forbidden_help_tokens: Vec<String>,
    required_usage_prefixes: Vec<String>,
    required_subcommands: Vec<RequiredSubcommand>,
    forbidden_public_tokens: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RequiredSubcommand {
    name: String,
    output_schema: String,
}

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixture_path(relative: &str) -> PathBuf {
    manifest_dir().join(relative)
}

fn migration_fixture() -> NamespaceMigrationFixture {
    let raw = fs::read_to_string(fixture_path(
        "tests/fixtures/entity/namespace/migration_contract.json",
    ))
    .expect("namespace migration fixture opens");
    serde_json::from_str(&raw).expect("namespace migration fixture parses")
}

fn read_json(relative: &str) -> Value {
    let raw = fs::read_to_string(fixture_path(relative)).expect("JSON fixture opens");
    serde_json::from_str(&raw).expect("JSON fixture parses")
}

fn canon_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.current_dir(manifest_dir());
    command
}

fn command_output(args: &[&str]) -> Output {
    canon_command().args(args).output().expect("canon runs")
}

fn command_text(args: &[&str], expected_code: i32) -> String {
    let output = command_output(args);
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "unexpected exit for {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn lookup_output(case: &LookupCase) -> Output {
    canon_command()
        .args([
            case.input.as_str(),
            "--registry",
            case.registry.as_str(),
            "--column",
            case.column.as_str(),
            "--explicit",
            "--no-witness",
        ])
        .output()
        .unwrap_or_else(|error| panic!("{} runs: {error}", case.id))
}

#[test]
fn entity_namespace_migration_fixture_declares_g01_g02_scope() {
    let fixture = migration_fixture();
    assert_eq!(
        fixture.schema_version,
        "canon.entity.namespace_migration_fixture.v0"
    );
    assert_eq!(fixture.gates, ["G01", "G02"]);
    assert_eq!(fixture.lookup_cases.len(), 2);
    assert!(!fixture.public_surface.required_usage_prefixes.is_empty());
    assert!(!fixture.public_surface.required_subcommands.is_empty());
}

#[test]
fn entity_namespace_migration_core_lookup_fixtures_remain_byte_stable() {
    let fixture = migration_fixture();

    for case in &fixture.lookup_cases {
        let first = lookup_output(case);
        assert_eq!(
            first.status.code(),
            Some(case.expected_exit_code),
            "{} first run failed: {}",
            case.id,
            String::from_utf8_lossy(&first.stderr)
        );

        let second = lookup_output(case);
        assert_eq!(
            second.status.code(),
            Some(case.expected_exit_code),
            "{} second run failed: {}",
            case.id,
            String::from_utf8_lossy(&second.stderr)
        );
        assert_eq!(second.stdout, first.stdout, "{} output changed", case.id);

        let actual: Value = serde_json::from_slice(&first.stdout)
            .unwrap_or_else(|error| panic!("{case_id} JSON: {error}", case_id = case.id));
        assert_eq!(actual, read_json(&case.pre_migration_golden), "{}", case.id);
    }
}

#[test]
fn entity_namespace_migration_public_surfaces_expose_entity_not_org() {
    let fixture = migration_fixture();
    let help = command_text(&["--help"], 0);

    for token in &fixture.public_surface.required_help_tokens {
        assert!(help.contains(token), "top-level help missing {token:?}");
    }
    for token in &fixture.public_surface.forbidden_help_tokens {
        assert!(
            !help.contains(token),
            "top-level help still exposes forbidden token {token:?}"
        );
    }

    let describe_text = command_text(&["--describe"], 0);
    let describe_json: Value = serde_json::from_str(&describe_text).expect("describe JSON");
    assert_operator_surface("canon --describe", &describe_text, &describe_json, &fixture);

    let operator_text = fs::read_to_string(fixture_path("operator.json")).expect("operator.json");
    let operator_json: Value = serde_json::from_str(&operator_text).expect("operator JSON");
    assert_operator_surface("operator.json", &operator_text, &operator_json, &fixture);

    let robot_docs = command_text(&["doctor", "robot-docs"], 0);
    assert_forbidden_tokens_absent(
        "canon doctor robot-docs",
        &robot_docs,
        &fixture.public_surface.forbidden_public_tokens,
    );
}

#[test]
fn entity_namespace_migration_removed_org_parser_is_not_accepted() {
    canon_command()
        .args(["org", "run", "--help"])
        .assert()
        .failure();

    canon_command()
        .args(["entity", "run", "--help"])
        .assert()
        .success();
}

fn assert_operator_surface(
    label: &str,
    text: &str,
    manifest: &Value,
    fixture: &NamespaceMigrationFixture,
) {
    assert_forbidden_tokens_absent(label, text, &fixture.public_surface.forbidden_public_tokens);

    let usage = manifest["invocation"]["usage"]
        .as_array()
        .unwrap_or_else(|| panic!("{label} invocation.usage"));
    for prefix in &fixture.public_surface.required_usage_prefixes {
        assert!(
            usage.iter().any(|entry| entry
                .as_str()
                .is_some_and(|usage| usage.starts_with(prefix))),
            "{label} missing usage prefix {prefix}"
        );
    }

    let subcommands = manifest["subcommands"]
        .as_array()
        .unwrap_or_else(|| panic!("{label} subcommands"));
    for required in public_required_subcommands(&fixture.public_surface.required_subcommands) {
        assert!(
            subcommands.iter().any(
                |entry| entry["name"].as_str() == Some(required.name.as_str())
                    && entry["output_schema"].as_str() == Some(required.output_schema.as_str())
            ),
            "{label} missing {} / {}",
            required.name,
            required.output_schema
        );
    }
    assert!(
        subcommands.iter().all(|entry| {
            let name_is_clean = entry["name"]
                .as_str()
                .is_none_or(|name| name != "org" && !name.starts_with("org "));
            let schema_is_clean = entry["output_schema"]
                .as_str()
                .is_none_or(|schema| !schema.starts_with("canon_org_"));
            name_is_clean && schema_is_clean
        }),
        "{label} exposes an org workbench subcommand or schema"
    );
}

fn public_required_subcommands(required: &[RequiredSubcommand]) -> Vec<RequiredSubcommand> {
    required
        .iter()
        .map(|required| RequiredSubcommand {
            name: required.name.clone(),
            output_schema: match required.name.as_str() {
                "entity block" => "canon_entity_block.v1".to_string(),
                "entity evidence" => "canon_entity_evidence.v1".to_string(),
                "entity solve" => "canon_entity_solve.v1".to_string(),
                _ => required.output_schema.clone(),
            },
        })
        .collect()
}

fn assert_forbidden_tokens_absent(label: &str, text: &str, forbidden_tokens: &[String]) {
    for token in forbidden_tokens {
        assert!(
            !text.contains(token),
            "{label} contains forbidden {token:?}"
        );
    }
}
