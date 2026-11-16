#![forbid(unsafe_code)]
//! bd-vdcw.55: the hand-off identity of a registry directory and --expect-base.
//! Every flow goes through the public CLI on temp directories; nothing is
//! uploaded and no network is used.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn seed(dir: &Path) {
    write(
        &dir.join("registry.json"),
        &serde_json::to_string_pretty(&json!({
            "id": "firms",
            "version": "1.0.0",
            "description": "identity fixture",
            "updated": "2026-11-16",
            "entry_count": 2,
            "default_id_scheme": {"prefix": "ORG", "zero_pad": 3}
        }))
        .unwrap(),
    );
    write(
        &dir.join("aliases.json"),
        &serde_json::to_string_pretty(&json!([
            {
                "input": "Orion Ltd",
                "canonical_id": "ORG-001",
                "canonical_type": "firm",
                "rule_id": "SESSION_TIER_A",
                "session_authority": {"closure_hash": "blake3:abcdef", "outcome_ref": "cmdrvl://x"}
            },
            {
                "input": "ORG-009",
                "canonical_id": "ORG-001",
                "canonical_type": "firm",
                "rule_id": "SUPERSEDED_ID_ALIAS"
            }
        ]))
        .unwrap(),
    );
    write(&dir.join("_escrow/cannot_link.jsonl"), "");
}

fn identity(dir: &Path) -> Value {
    let output = canon()
        .args(["registry", "identity", "--registry", dir.to_str().unwrap()])
        .assert()
        .success();
    serde_json::from_slice(&output.get_output().stdout).expect("identity JSON")
}

fn ref_check(dir: &Path, reference: &str) -> assert_cmd::assert::Assert {
    canon()
        .args([
            "registry",
            "identity",
            "--registry",
            dir.to_str().unwrap(),
            "--ref",
            reference,
        ])
        .assert()
}

fn json_of(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

#[test]
fn identity_ignores_the_directory_path_and_sees_sidecars() {
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("one");
    let second = temp.path().join("nested/two");
    seed(&first);
    seed(&second);
    let first_identity = identity(&first);
    let second_identity = identity(&second);
    eprintln!("identity one: {first_identity}");
    eprintln!("identity two: {second_identity}");
    assert_eq!(
        first_identity["content_hash"],
        second_identity["content_hash"]
    );
    assert_eq!(first_identity["registry_id"], "firms");
    assert_eq!(first_identity["registry_version"], "1.0.0");
    let paths = first_identity["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        ["_escrow/cannot_link.jsonl", "aliases.json", "registry.json"],
        "sorted, relative, sidecars included"
    );

    // One byte of a sidecar changes the hash: a hash over mapping files only
    // would not notice.
    write(&second.join("_escrow/cannot_link.jsonl"), "x");
    assert_ne!(
        identity(&second)["content_hash"],
        first_identity["content_hash"]
    );
    // A registry.json-only change is seen too.
    write(&second.join("_escrow/cannot_link.jsonl"), "");
    assert_eq!(
        identity(&second)["content_hash"],
        first_identity["content_hash"]
    );
}

#[test]
fn pinned_references_are_checked_against_the_directory() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("registry");
    seed(&dir);

    let current = json_of(&ref_check(&dir, "firms:ORG-001@1.0.0#blake3:abcdef").success());
    assert_eq!(current["id_status"], "current");
    assert_eq!(current["resolves_to"], "ORG-001");
    assert_eq!(current["version_match"], true);
    assert_eq!(current["closure_found"], true);

    // Without a registry id prefix the same reference checks the same way.
    let bare = json_of(&ref_check(&dir, "ORG-001@1.0.0#blake3:abcdef").success());
    assert_eq!(bare["closure_found"], true);

    // One hex digit changed: the closure is not found.
    let changed = json_of(&ref_check(&dir, "firms:ORG-001@1.0.0#blake3:abcdee").success());
    assert_eq!(changed["id_status"], "current");
    assert_eq!(changed["closure_found"], false);

    // A different version is a version mismatch, not a refusal.
    let other_version = json_of(&ref_check(&dir, "firms:ORG-001@2.0.0#blake3:abcdef").success());
    assert_eq!(other_version["version_match"], false);

    // A retired id that only resolves through SUPERSEDED_ID_ALIAS.
    let superseded = json_of(&ref_check(&dir, "ORG-009@1.0.0#blake3:abcdef").success());
    assert_eq!(superseded["id_status"], "superseded");
    assert_eq!(superseded["resolves_to"], "ORG-001");

    let unknown = json_of(&ref_check(&dir, "ORG-404@1.0.0#blake3:abcdef").success());
    assert_eq!(unknown["id_status"], "unknown");
    assert!(unknown["resolves_to"].is_null());

    // Malformed references are refused naming the form.
    for bad in ["ORG-001", "ORG-001@1.0.0", "ORG-001@1.0.0#"] {
        let refused = ref_check(&dir, bad).code(2);
        let output = json_of(&refused);
        assert_eq!(output["refusal"]["code"], "E_PARSE", "{bad}");
        assert!(
            output["refusal"]["message"]
                .as_str()
                .unwrap()
                .contains("<canonical id>@<registry version>#<closure hash>"),
            "{bad}: {output}"
        );
    }
}

fn registry_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut files = Vec::new();
    visit(dir, dir, &mut files);
    files.sort();
    files
}

#[test]
fn expect_base_refuses_a_moved_registry_before_any_write() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    seed(&registry);
    let base = identity(&registry)["content_hash"]
        .as_str()
        .unwrap()
        .to_string();

    let session = root.join("session");
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "hand-off",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            session.to_str().unwrap(),
            "--registry-id",
            "firms",
        ])
        .assert()
        .success();
    let observations = root.join("observations.jsonl");
    write(
        &observations,
        &format!(
            "{}\n",
            serde_json::to_string(&json!({
                "obs_id": "g1",
                "entity_type": "organisation",
                "surface": "Gamma Corp",
                "keys": {"cik": "321"},
                "source": {"channel": "file", "id": "handoff", "lineage": "a"}
            }))
            .unwrap()
        ),
    );
    let rules = root.join("rules.yaml");
    write(&rules, RULES);
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations.to_str().unwrap(),
        ])
        .assert()
        .success();
    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
        ])
        .assert()
        .success();

    // The driver synced a different copy: refuse, name both hashes, write nothing.
    let stale = "blake3:0000000000000000000000000000000000000000000000000000000000000000";
    let before = registry_files(&registry);
    let refused = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().unwrap(),
            "--next-version",
            "1.0.1",
            "--expect-base",
            stale,
        ])
        .assert()
        .code(2);
    let refusal = json_of(&refused);
    assert_eq!(refusal["refusal"]["detail"]["reason"], "base_moved");
    assert_eq!(refusal["refusal"]["detail"]["expected"], stale);
    assert_eq!(refusal["refusal"]["detail"]["actual"], base.as_str());
    assert_eq!(registry_files(&registry), before, "no file changed");

    // The right base commits.
    canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().unwrap(),
            "--next-version",
            "1.0.1",
            "--expect-base",
            &base,
        ])
        .assert()
        .success();
    let after = identity(&registry);
    assert_ne!(after["content_hash"], base.as_str());
    assert_eq!(after["registry_version"], "1.0.1");

    // Repeating the exact same commit with the old base is an exact repeat,
    // not a moved base.
    let repeat = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().unwrap(),
            "--next-version",
            "1.0.1",
            "--expect-base",
            &base,
        ])
        .assert()
        .success();
    assert_eq!(json_of(&repeat)["outcome"], "already_committed");
}
