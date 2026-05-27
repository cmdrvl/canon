use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_registry_metadata(
    registry: &Path,
    version: &str,
    entry_count: usize,
    default_id_scheme: Option<(&str, usize)>,
) {
    let mut value = json!({
        "id": "people",
        "version": version,
        "description": "registry mint test fixture",
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
    write_registry_metadata(temp.path(), "1.2.3", 1, default_id_scheme);
    write_mapping_file(
        temp.path(),
        "aliases.json",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
    );
    write_mapping_file(temp.path(), "nicknames.json", json!([]));
    temp
}

fn scrub_source(mut payload: Value) -> Value {
    payload["registry"]["source"] = Value::String("<REGISTRY>".to_string());
    payload
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn file_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn mint_allocates_next_id_writes_multiple_aliases_and_round_trips() {
    let registry = make_registry(Some(("PPL", 3)));

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "person",
            "--with-alias",
            "aliases.json=John Doe:MANUAL",
            "--with-alias",
            "nicknames.json=J:Doe:ALIAS",
        ])
        .assert()
        .success();

    assert!(output.get_output().stderr.is_empty());
    let actual =
        scrub_source(serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap());
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/golden/registry_mint.json")).unwrap();
    assert_eq!(actual, expected);
    assert!(!registry.path().join("_index.sqlite").exists());

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.2.4");
    assert_eq!(registry_json["entry_count"], 3);
    assert_eq!(registry_json["owner"], "test-suite");

    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases.as_array().unwrap().len(), 2);
    assert_eq!(aliases[1]["canonical_id"], "PPL-002");
    let nicknames = read_json(&registry.path().join("nicknames.json"));
    assert_eq!(nicknames.as_array().unwrap().len(), 1);
    assert_eq!(nicknames[0]["input"], "J:Doe");

    let input_path = registry.path().join("input.csv");
    fs::write(&input_path, "name\nJohn Doe\nJ:Doe\n").unwrap();
    let resolve = canon_command()
        .args([
            input_path.to_str().unwrap(),
            "--registry",
            registry.path().to_str().unwrap(),
            "--column",
            "name",
            "--no-witness",
            "--explicit",
        ])
        .assert()
        .success();
    let payload: Value = serde_json::from_slice(resolve.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["outcome"], "RESOLVED");
    assert_eq!(payload["mappings"][0]["canonical_id"], "u8:PPL-002");
    assert_eq!(payload["summary"]["resolved"], 2);
}

#[test]
fn mint_explicit_canonical_id_skips_allocation_and_plain_prints_id() {
    let registry = make_registry(None);

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-id",
            "IC-009",
            "--canonical-type",
            "issuer",
            "--with-alias",
            "nicknames.json=Issuer Co:MANUAL",
            "--emit",
            "plain",
            "--no-lint",
        ])
        .assert()
        .success();

    assert_eq!(output.get_output().stdout, b"IC-009\n");
    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.2.4");
    assert_eq!(registry_json["entry_count"], 2);
    let nicknames = read_json(&registry.path().join("nicknames.json"));
    assert_eq!(nicknames[0]["canonical_id"], "IC-009");
}

#[test]
fn mint_prefix_override_beats_default_scheme_for_allocation() {
    let registry = make_registry(Some(("PPL", 3)));

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "ORG",
            "--canonical-type",
            "org",
            "--with-alias",
            "nicknames.json=Acme Inc:MANUAL",
            "--emit",
            "plain",
            "--no-lint",
        ])
        .assert()
        .success();

    assert_eq!(output.get_output().stdout, b"ORG-001\n");
    let nicknames = read_json(&registry.path().join("nicknames.json"));
    assert_eq!(nicknames[0]["canonical_id"], "ORG-001");
}

#[test]
fn mint_refuses_zero_aliases_and_duplicate_request_inputs_without_writes() {
    let registry = make_registry(Some(("PPL", 3)));
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));
    let nicknames_before = file_bytes(&registry.path().join("nicknames.json"));

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "person",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "person",
            "--with-alias",
            "aliases.json=John Doe:MANUAL",
            "--with-alias",
            "nicknames.json=John Doe:ALIAS",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );
    assert_eq!(
        file_bytes(&registry.path().join("nicknames.json")),
        nicknames_before
    );
}

#[test]
fn mint_refuses_existing_duplicate_and_bad_alias_spec_without_writes() {
    let registry = make_registry(Some(("PPL", 3)));
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "person",
            "--with-alias",
            "aliases.json=Jane Doe:MANUAL",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );

    let output = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "person",
            "--with-alias",
            "aliases.json=MissingRule",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
}
