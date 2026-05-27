use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write_registry_metadata(registry: &Path, version: &str, entry_count: usize) {
    fs::write(
        registry.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "people",
            "version": version,
            "description": "registry default-id-scheme test fixture",
            "updated": "2026-05-27",
            "entry_count": entry_count,
            "owner": "test-suite"
        }))
        .unwrap(),
    )
    .unwrap();
}

fn write_mapping_file(registry: &Path, entries: Value) {
    fs::write(
        registry.join("aliases.json"),
        serde_json::to_vec_pretty(&entries).unwrap(),
    )
    .unwrap();
}

fn make_registry(version: &str, entries: Value) -> TempDir {
    let temp = TempDir::new().unwrap();
    let entry_count = entries.as_array().unwrap().len();
    write_registry_metadata(temp.path(), version, entry_count);
    write_mapping_file(temp.path(), entries);
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
fn default_id_scheme_writes_metadata_preserves_unknowns_and_records_warnings() {
    let registry = make_registry(
        "1.2.3",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-1", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
    );
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));

    let output = canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "PPL",
        ])
        .assert()
        .success();

    assert!(output.get_output().stderr.is_empty());
    let actual =
        scrub_source(serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap());
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/golden/registry_id_scheme.json")).unwrap();
    assert_eq!(actual, expected);

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.2.4");
    assert_eq!(registry_json["entry_count"], 1);
    assert_eq!(registry_json["owner"], "test-suite");
    assert_eq!(registry_json["default_id_scheme"]["prefix"], "PPL");
    assert_eq!(registry_json["default_id_scheme"]["zero_pad"], 3);
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );
    assert!(!registry.path().join("_index.sqlite").exists());
}

#[test]
fn default_id_scheme_strict_refuses_out_of_scheme_ids_without_writing() {
    let registry = make_registry(
        "1.2.3",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-1", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
    );
    let registry_before = file_bytes(&registry.path().join("registry.json"));

    let output = canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "PPL",
            "--strict",
        ])
        .assert()
        .code(2);

    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_BAD_REGISTRY");
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
}

#[test]
fn default_id_scheme_validates_prefix_padding_and_versions() {
    for args in [
        vec!["--prefix", "ppl"],
        vec!["--prefix", "PPL", "--zero-pad", "0"],
        vec!["--prefix", "PPL", "--zero-pad", "21"],
    ] {
        let registry = make_registry("1.2.3", json!([]));
        let before = file_bytes(&registry.path().join("registry.json"));
        let mut command_args = vec![
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
        ];
        command_args.extend(args);
        let output = canon_command().args(command_args).assert().code(2);
        let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
        assert_eq!(payload["refusal"]["code"], "E_PARSE");
        assert_eq!(file_bytes(&registry.path().join("registry.json")), before);
    }

    let registry = make_registry("2026-05-27", json!([]));
    let output = canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "PPL",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");

    let output = canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "PPL",
            "--next-version",
            "2026-05-28",
            "--emit",
            "plain",
        ])
        .assert()
        .success();
    assert_eq!(output.get_output().stdout, b"PPL-3\n");

    let unchanged = make_registry("1.2.3", json!([]));
    let output = canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            unchanged.path().to_str().unwrap(),
            "--prefix",
            "PPL",
            "--next-version",
            "1.2.3",
        ])
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
}

#[test]
fn default_id_scheme_feeds_next_id_and_mint_defaults() {
    let registry = make_registry("1.2.3", json!([]));
    canon_command()
        .args([
            "registry",
            "default-id-scheme",
            "--registry",
            registry.path().to_str().unwrap(),
            "--prefix",
            "IC",
            "--zero-pad",
            "4",
        ])
        .assert()
        .success();

    let next_id = canon_command()
        .args([
            "registry",
            "next-id",
            "--registry",
            registry.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(next_id.get_output().stdout, b"IC-0001\n");

    write_mapping_file(registry.path(), json!([]));
    let mint = canon_command()
        .args([
            "registry",
            "mint",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "issuer",
            "--with-alias",
            "aliases.json=Issuer Co:MANUAL",
            "--emit",
            "plain",
            "--no-lint",
        ])
        .assert()
        .success();
    assert_eq!(mint.get_output().stdout, b"IC-0001\n");
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases[0]["canonical_id"], "IC-0001");
}
