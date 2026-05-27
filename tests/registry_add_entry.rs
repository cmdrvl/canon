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
        "description": "registry add-entry test fixture",
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

fn make_registry(
    version: &str,
    entries: Value,
    default_id_scheme: Option<(&str, usize)>,
) -> TempDir {
    let temp = TempDir::new().unwrap();
    let entry_count = entries.as_array().unwrap().len();
    write_registry_metadata(temp.path(), version, entry_count, default_id_scheme);
    write_mapping_file(temp.path(), "aliases.json", entries);
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

fn add_entry_args<'a>(
    registry: &'a Path,
    canonical_id: &'a str,
    input: &'a str,
    canonical_type: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args = vec![
        "registry",
        "add-entry",
        "--registry",
        registry.to_str().unwrap(),
        "--alias-file",
        "aliases.json",
        "--canonical-id",
        canonical_id,
        "--input",
        input,
        "--rule-id",
        "MANUAL",
    ];
    if let Some(canonical_type) = canonical_type {
        args.extend(["--canonical-type", canonical_type]);
    }
    args
}

#[test]
fn add_entry_json_appends_metadata_preserves_unknowns_and_round_trips() {
    let registry = make_registry("1.2.3", json!([]), Some(("PPL", 3)));

    let output = canon_command()
        .args(add_entry_args(
            registry.path(),
            "PPL-001",
            "Jane Doe",
            Some("person"),
        ))
        .assert()
        .success();

    assert!(output.get_output().stderr.is_empty());
    let actual =
        scrub_source(serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap());
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/golden/registry_add_entry.json")).unwrap();
    assert_eq!(actual, expected);
    assert!(!registry.path().join("_index.sqlite").exists());

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.2.4");
    assert_eq!(registry_json["entry_count"], 1);
    assert_eq!(registry_json["owner"], "test-suite");
    assert_eq!(registry_json["default_id_scheme"]["prefix"], "PPL");

    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases.as_array().unwrap().len(), 1);
    assert_eq!(aliases[0]["input"], "Jane Doe");
    assert_eq!(aliases[0]["canonical_id"], "PPL-001");

    let input_path = registry.path().join("input.csv");
    fs::write(&input_path, "name\nJane Doe\n").unwrap();
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
    assert_eq!(payload["mappings"][0]["canonical_id"], "u8:PPL-001");
}

#[test]
fn add_entry_duplicate_input_refuses_before_writing() {
    let registry = make_registry(
        "1.2.3",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
        None,
    );
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));

    let output = canon_command()
        .args(add_entry_args(
            registry.path(),
            "PPL-002",
            "Jane Doe",
            Some("person"),
        ))
        .assert()
        .code(2);

    assert!(output.get_output().stderr.is_empty());
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );
}

#[test]
fn add_entry_inferrs_type_and_honors_version_bumps() {
    let registry = make_registry(
        "1.2.3",
        json!([
            {"input": "Jane Doe", "canonical_id": "PPL-001", "canonical_type": "person", "rule_id": "MANUAL"}
        ]),
        None,
    );

    let mut args = add_entry_args(registry.path(), "PPL-001", "J. Doe", None);
    args.extend(["--bump", "minor", "--emit", "plain", "--no-lint"]);
    let output = canon_command().args(args).assert().success();
    assert_eq!(
        output.get_output().stdout,
        b"added J. Doe -> PPL-001 in aliases.json (1.3.0)\n"
    );
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases[1]["canonical_type"], "person");
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["version"],
        "1.3.0"
    );

    let major = make_registry("1.2.3", json!([]), None);
    let mut args = add_entry_args(major.path(), "PPL-001", "Jane Doe", Some("person"));
    args.extend(["--bump", "major", "--no-lint"]);
    canon_command().args(args).assert().success();
    assert_eq!(
        read_json(&major.path().join("registry.json"))["version"],
        "2.0.0"
    );
}

#[test]
fn add_entry_non_bumpable_versions_require_next_version() {
    let registry = make_registry("2026-05-27", json!([]), None);
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));

    let output = canon_command()
        .args(add_entry_args(
            registry.path(),
            "PPL-001",
            "Jane Doe",
            Some("person"),
        ))
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

    let mut args = add_entry_args(registry.path(), "PPL-001", "Jane Doe", Some("person"));
    args.extend(["--next-version", "2026-05-28", "--no-lint"]);
    canon_command().args(args).assert().success();
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["version"],
        "2026-05-28"
    );
}

#[test]
fn add_entry_rejects_bad_alias_files_and_out_of_scheme_ids() {
    for alias_file in [
        "../aliases.json",
        "registry.json",
        "_build.json",
        "missing.json",
    ] {
        let registry = make_registry("1.2.3", json!([]), None);
        let before = file_bytes(&registry.path().join("registry.json"));
        let output = canon_command()
            .args([
                "registry",
                "add-entry",
                "--registry",
                registry.path().to_str().unwrap(),
                "--alias-file",
                alias_file,
                "--canonical-id",
                "PPL-001",
                "--input",
                "Jane Doe",
                "--rule-id",
                "MANUAL",
                "--canonical-type",
                "person",
            ])
            .assert()
            .code(2);
        let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
        assert_eq!(payload["refusal"]["code"], "E_PARSE");
        assert_eq!(file_bytes(&registry.path().join("registry.json")), before);
    }

    let registry = make_registry("1.2.3", json!([]), Some(("PPL", 3)));
    let output = canon_command()
        .args(add_entry_args(
            registry.path(),
            "ORG-001",
            "Jane Doe",
            Some("person"),
        ))
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(
        payload["refusal"]["detail"]["prefix"],
        Value::String("PPL".to_string())
    );
}

#[test]
fn add_entry_rejects_untrimmed_input_and_missing_new_type() {
    let registry = make_registry("1.2.3", json!([]), None);
    let output = canon_command()
        .args(add_entry_args(
            registry.path(),
            "PPL-001",
            " Jane Doe",
            Some("person"),
        ))
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert_eq!(payload["refusal"]["detail"]["trimmed"], "Jane Doe");

    let output = canon_command()
        .args(add_entry_args(registry.path(), "PPL-001", "Jane Doe", None))
        .assert()
        .code(2);
    let payload: Value = serde_json::from_slice(output.get_output().stdout.as_slice()).unwrap();
    assert_eq!(payload["refusal"]["code"], "E_PARSE");
    assert!(
        payload["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("--canonical-type")
    );
}
