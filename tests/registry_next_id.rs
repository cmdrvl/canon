use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_registry_metadata(
    registry: &Path,
    id: &str,
    version: &str,
    entry_count: usize,
    default_id_scheme: Option<(&str, usize)>,
) {
    let mut value = json!({
        "id": id,
        "version": version,
        "description": "registry next-id test fixture",
        "updated": "2026-05-27",
        "entry_count": entry_count,
        "owner": "test-suite"
    });
    if let Some((prefix, zero_pad)) = default_id_scheme {
        value["default_id_scheme"] = json!({
            "prefix": prefix,
            "zero_pad": zero_pad
        });
    }
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}

fn write_mapping_file(registry: &Path, name: &str, entries: Value) {
    fs::write(
        registry.join(name),
        serde_json::to_vec_pretty(&entries).unwrap(),
    )
    .unwrap();
}

fn make_registry(default_id_scheme: Option<(&str, usize)>) -> TempDir {
    let temp = TempDir::new().unwrap();
    write_registry_metadata(temp.path(), "people", "1.2.3", 0, default_id_scheme);
    temp
}

fn scrub_source(mut payload: Value) -> Value {
    payload["registry"]["source"] = Value::String("<REGISTRY>".to_string());
    payload
}

#[test]
fn next_id_plain_for_empty_registry_is_shell_composable_and_read_only() {
    let registry = make_registry(None);

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "PPL",
            "--registry",
            registry.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(output.get_output().stdout, b"PPL-001\n");
    assert!(output.get_output().stderr.is_empty());
    assert!(!registry.path().join("_index.sqlite").exists());
}

#[test]
fn next_id_scans_distinct_canonical_ids_across_flat_mapping_files() {
    let registry = make_registry(None);
    write_mapping_file(
        registry.path(),
        "b-aliases.json",
        json!([
            {"input": "Jane", "canonical_id": "PPL-010", "canonical_type": "person", "rule_id": "ALIAS"},
            {"input": "Acme", "canonical_id": "CPTY-099", "canonical_type": "counterparty", "rule_id": "ALIAS"}
        ]),
    );
    write_mapping_file(
        registry.path(),
        "a-aliases.json",
        json!([
            {"input": "J. Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "ALIAS"},
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "ALIAS"},
            {"input": "John", "canonical_id": "PPL-009", "canonical_type": "person", "rule_id": "ALIAS"}
        ]),
    );

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "PPL",
            "--registry",
            registry.path().to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();

    let actual =
        scrub_source(serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap());
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/golden/registry_next_id.json")).unwrap();
    assert_eq!(actual, expected);
    assert!(!registry.path().join("_index.sqlite").exists());
}

#[test]
fn next_id_honors_zero_pad_widths_larger_than_default() {
    let registry = make_registry(None);
    write_mapping_file(
        registry.path(),
        "aliases.json",
        json!([
            {"input": "Jane", "canonical_id": "PPL-099", "canonical_type": "person", "rule_id": "ALIAS"}
        ]),
    );

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "PPL",
            "--registry",
            registry.path().to_str().unwrap(),
            "--zero-pad",
            "5",
        ])
        .assert()
        .success();

    assert_eq!(output.get_output().stdout, b"PPL-00100\n");
}

#[test]
fn next_id_uses_default_id_scheme_when_prefix_is_omitted() {
    let registry = make_registry(Some(("IC", 4)));

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "--registry",
            registry.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(output.get_output().stdout, b"IC-0001\n");
}

#[test]
fn next_id_refuses_omitted_prefix_without_default_id_scheme() {
    let registry = make_registry(None);

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "--registry",
            registry.path().to_str().unwrap(),
        ])
        .assert()
        .code(2);

    assert!(output.get_output().stdout.is_empty());
    let payload: Value = serde_json::from_slice(output.get_output().stderr.as_slice()).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert!(
        payload["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("canon registry next-id PPL")
    );
}

#[test]
fn next_id_refuses_malformed_in_namespace_id() {
    let registry = make_registry(None);
    write_mapping_file(
        registry.path(),
        "aliases.json",
        json!([
            {"input": "Jane", "canonical_id": "PPL-12A", "canonical_type": "person", "rule_id": "ALIAS"}
        ]),
    );

    let output = canon_command()
        .args([
            "registry",
            "next-id",
            "PPL",
            "--registry",
            registry.path().to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .code(2);

    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_BAD_REGISTRY");
    assert_eq!(payload["refusal"]["detail"]["canonical_id"], "PPL-12A");
}
