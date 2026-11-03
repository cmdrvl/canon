//! `canon registry mint-batch` (bd-vula): N IDs in one registry version.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn make_registry() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "vehicles",
            "version": "1.2.3",
            "description": "mint-batch fixture",
            "updated": "2026-05-27",
            "entry_count": 1,
            "owner": "test-suite",
            "default_id_scheme": {"prefix": "VEH", "zero_pad": 3}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        temp.path().join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {"input": "Existing Vehicle LLC", "canonical_id": "VEH-004", "canonical_type": "vehicle", "rule_id": "MANUAL"}
        ]))
        .unwrap(),
    )
    .unwrap();
    fs::write(temp.path().join("other.json"), "[]").unwrap();
    temp
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn batch_file(lines: &[&str]) -> (TempDir, String) {
    // Own directory: the batch file must never be part of the registry.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("batch.jsonl");
    fs::write(&path, lines.join("\n")).unwrap();
    let path = path.to_string_lossy().into_owned();
    (dir, path)
}

fn run_batch(registry: &TempDir, batch: &str, extra: &[&str]) -> (i32, Value) {
    let output = canon()
        .args([
            "registry",
            "mint-batch",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "vehicle",
            "--from",
            batch,
        ])
        .args(extra)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null),
    )
}

#[test]
fn batch_mints_sequential_ids_in_one_version_across_alias_files() {
    let registry = make_registry();
    let (_batch_dir, batch) = batch_file(&[
        r#"{"with_alias": ["aliases.json=Alpha Fund II, LLC:MANUAL", "aliases.json=Alpha Fund II:MANUAL"]}"#,
        "",
        r#"{"with_alias": ["other.json=Beta Fund III, LLC:MANUAL"], "canonical_type": "fund"}"#,
        r#"{"with_alias": ["aliases.json=Gamma Holdings:MANUAL"]}"#,
    ]);
    let (code, out) = run_batch(&registry, &batch, &[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["version"], "canon_registry_mint_batch.v0");

    // Sequential after the existing maximum (VEH-004), not from 001.
    let ids = out["minted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["canonical_id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["VEH-005", "VEH-006", "VEH-007"]);
    assert_eq!(out["minted"][1]["canonical_type"], "fund");
    assert_eq!(out["minted"][0]["aliases"].as_array().unwrap().len(), 2);

    // ONE version bump and one entry_count update for the whole batch.
    assert_eq!(out["registry"]["version_before"], "1.2.3");
    assert_eq!(out["registry"]["version_after"], "1.2.4");
    assert_eq!(out["registry"]["entry_count_after"], 5);
    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.2.4");
    assert_eq!(registry_json["entry_count"], 5);
    assert_eq!(registry_json["owner"], "test-suite");
    let updated = registry_json["updated"].as_str().unwrap();
    assert_ne!(updated, "2026-05-27", "updated must move with the mutation");

    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases.as_array().unwrap().len(), 4);
    assert_eq!(
        read_json(&registry.path().join("other.json"))
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // The minted IDs resolve through the normal exact lookup.
    let rows = registry.path().join("names.csv");
    fs::write(
        &rows,
        "name\n\"Alpha Fund II, LLC\"\n\"Beta Fund III, LLC\"\nGamma Holdings\n",
    )
    .unwrap();
    let resolved = canon()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.path().to_str().unwrap(),
            "--column",
            "name",
            "--no-witness",
        ])
        .assert()
        .success();
    let payload: Value = serde_json::from_slice(&resolved.get_output().stdout).unwrap();
    assert_eq!(payload["summary"]["resolved"], 3);
}

#[test]
fn plain_emit_prints_one_id_per_line() {
    let registry = make_registry();
    let (_batch_dir, batch) = batch_file(&[
        r#"{"with_alias": ["aliases.json=One:MANUAL"]}"#,
        r#"{"with_alias": ["aliases.json=Two:MANUAL"]}"#,
    ]);
    let output = canon()
        .args([
            "registry",
            "mint-batch",
            "--registry",
            registry.path().to_str().unwrap(),
            "--canonical-type",
            "vehicle",
            "--from",
            &batch,
            "--emit",
            "plain",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "VEH-005\nVEH-006\n"
    );
}

#[test]
fn every_refusal_leaves_the_registry_byte_identical() {
    let cases: [(&str, Vec<&str>); 6] = [
        (
            "duplicate input across two items",
            vec![
                r#"{"with_alias": ["aliases.json=Same Name:MANUAL"]}"#,
                r#"{"with_alias": ["other.json=Same Name:MANUAL"]}"#,
            ],
        ),
        (
            "input already in the registry",
            vec![
                r#"{"with_alias": ["aliases.json=Fresh One:MANUAL"]}"#,
                r#"{"with_alias": ["aliases.json=Existing Vehicle LLC:MANUAL"]}"#,
            ],
        ),
        ("item without aliases", vec![r#"{"with_alias": []}"#]),
        (
            "malformed json line",
            vec![
                r#"{"with_alias": ["aliases.json=Fine:MANUAL"]}"#,
                "not json",
            ],
        ),
        (
            "unknown field",
            vec![r#"{"with_alias": ["aliases.json=Fine:MANUAL"], "canonical_id": "VEH-999"}"#],
        ),
        ("empty batch", vec![""]),
    ];
    for (label, lines) in cases {
        let registry = make_registry();
        let (_batch_dir, batch) = batch_file(&lines);
        let before = snapshot(registry.path());
        let (code, out) = run_batch(&registry, &batch, &[]);
        assert_eq!(code, 2, "{label}: {out}");
        assert_eq!(out["outcome"], "REFUSAL", "{label}");
        assert_eq!(
            snapshot(registry.path()),
            before,
            "{label}: registry must not change on refusal"
        );
    }
}

#[test]
fn the_first_valid_item_is_not_committed_when_a_later_item_is_refused() {
    // Atomicity: this is the property that distinguishes mint-batch from a
    // loop of `registry mint` calls, where item 1 would already be on disk.
    let registry = make_registry();
    let (_batch_dir, batch) = batch_file(&[
        r#"{"with_alias": ["aliases.json=Brand New Vehicle:MANUAL"]}"#,
        r#"{"with_alias": ["nope/escape.json=Bad Alias File:MANUAL"]}"#,
    ]);
    let before = snapshot(registry.path());
    let (code, _) = run_batch(&registry, &batch, &[]);
    assert_eq!(code, 2);
    assert_eq!(snapshot(registry.path()), before);
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert!(
        aliases
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["input"] != "Brand New Vehicle")
    );
}

#[test]
fn explicit_next_version_and_prefix_override_are_honored() {
    let registry = make_registry();
    let (_batch_dir, batch) = batch_file(&[r#"{"with_alias": ["aliases.json=Prefixed:MANUAL"]}"#]);
    let (code, out) = run_batch(
        &registry,
        &batch,
        &["--prefix", "ZZZ", "--next-version", "2026.11.03"],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["minted"][0]["canonical_id"], "ZZZ-001");
    assert_eq!(out["scheme"]["source"], "prefix");
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["version"],
        "2026.11.03"
    );
}
