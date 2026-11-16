//! `canon registry add-entries` (bd-vdcw.10): one atomic incumbent-attach batch.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn make_registry() -> TempDir {
    let temp = TempDir::new().expect("registry tempdir");
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "organisations",
            "version": "1.0.0",
            "description": "add-entries fixture",
            "updated": "2026-11-16",
            "entry_count": 2,
            "default_id_scheme": {"prefix": "ORG", "zero_pad": 3}
        }))
        .expect("registry metadata"),
    )
    .expect("write registry metadata");
    fs::write(
        temp.path().join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {"input": "Medline Borrower, LP", "canonical_id": "ORG-001", "canonical_type": "organisation", "rule_id": "SESSION_TIER_A"},
            {"input": "Zeta Corporation", "canonical_id": "ORG-002", "canonical_type": "organisation", "rule_id": "SESSION_TIER_A"}
        ]))
        .expect("aliases"),
    )
    .expect("write aliases");
    temp
}

fn write_batch(temp: &TempDir, name: &str, lines: &[&str]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("batch");
    path
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read JSON")).expect("JSON")
}

fn snapshot(registry: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(registry)
        .expect("registry directory")
        .map(|entry| {
            let entry = entry.expect("directory entry");
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("registry file"),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[test]
fn attaches_new_aliases_once_and_skips_same_incumbent_aliases() {
    let registry = make_registry();
    let batch = write_batch(
        &registry,
        "attach.jsonl",
        &[
            r#"{"canonical_id":"ORG-001","alias_file":"aliases.json","input":"Medline Borrower L.P.","rule_id":"SESSION_TIER_A"}"#,
            r#"{"canonical_id":"ORG-001","alias_file":"aliases.json","input":"Medline Borrower, LP","rule_id":"SESSION_TIER_A"}"#,
        ],
    );
    let output = canon()
        .args([
            "registry",
            "add-entries",
            "--registry",
            registry.path().to_str().expect("registry path"),
            "--from",
            batch.to_str().expect("batch path"),
            "--next-version",
            "1.0.1",
        ])
        .output()
        .expect("run add-entries");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(payload["version"], "canon_registry_add_entries.v0");
    assert_eq!(payload["attached"].as_array().expect("attached").len(), 1);
    assert_eq!(payload["attached"][0]["canonical_id"], "ORG-001");
    assert_eq!(payload["attached"][0]["input"], "Medline Borrower L.P.");
    assert_eq!(payload["skipped"].as_array().expect("skipped").len(), 1);
    assert_eq!(payload["registry"]["version_after"], "1.0.1");
    assert_eq!(
        read_json(&registry.path().join("registry.json"))["entry_count"],
        3
    );
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert!(aliases.as_array().expect("aliases").iter().any(|entry| {
        entry["input"] == "Medline Borrower L.P." && entry["canonical_id"] == "ORG-001"
    }));
}

#[test]
fn unknown_id_and_cross_incumbent_alias_refuse_atomically() {
    for lines in [
        vec![
            r#"{"canonical_id":"ORG-001","alias_file":"aliases.json","input":"Fresh Alias","rule_id":"SESSION_TIER_A"}"#,
            r#"{"canonical_id":"ORG-999","alias_file":"aliases.json","input":"Unknown ID Alias","rule_id":"SESSION_TIER_A"}"#,
        ],
        vec![
            r#"{"canonical_id":"ORG-001","alias_file":"aliases.json","input":"Fresh Alias","rule_id":"SESSION_TIER_A"}"#,
            r#"{"canonical_id":"ORG-002","alias_file":"aliases.json","input":"Medline Borrower, LP","rule_id":"SESSION_TIER_A"}"#,
        ],
    ] {
        let registry = make_registry();
        let batch = write_batch(&registry, "bad.jsonl", &lines);
        let before = snapshot(registry.path());
        let output = canon()
            .args([
                "registry",
                "add-entries",
                "--registry",
                registry.path().to_str().expect("registry path"),
                "--from",
                batch.to_str().expect("batch path"),
            ])
            .output()
            .expect("run add-entries");
        assert_eq!(output.status.code(), Some(2));
        let payload: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
        assert_eq!(payload["outcome"], "REFUSAL");
        assert_eq!(snapshot(registry.path()), before);
    }
}
