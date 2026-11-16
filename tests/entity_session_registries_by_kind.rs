#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik]
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

fn json_stdout(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn write_lines(temp: &Path, name: &str, lines: &[String]) -> PathBuf {
    let path = temp.join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture");
    path
}

fn observation(obs_id: &str, entity_type: &str, surface: &str, cik: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": entity_type,
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "file", "id": "per-kind", "lineage": obs_id}
    }))
    .expect("observation JSON")
}

fn snapshot_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .expect("read registry")
            .map(|entry| entry.expect("read registry entry"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_path_buf(),
                    fs::read(path).expect("read registry file"),
                ));
            }
        }
    }

    let mut files = Vec::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn session_registries_keep_kinds_separate_and_reject_unmapped_rows() {
    let temp = TempDir::new().expect("temp root");
    let organisations = temp.path().join("organisations");
    let people = temp.path().join("people");
    let session = temp.path().join("session");
    let rules = write_lines(temp.path(), "rules.yaml", &[RULES.trim_end().to_string()]);

    canon()
        .args([
            "registry",
            "init",
            "--registry",
            organisations.to_str().expect("organisations path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    let original_organisation_files = snapshot_files(&organisations);
    let map = write_lines(
        temp.path(),
        "registries.yaml",
        &[format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n  person:\n    registry: {}\n    registry_id: people\n    id_scheme: agent\n    kind_prefix: per\n    canonical_type: person\n    alias_file: aliases.json",
            organisations.display(),
            people.display()
        )],
    );

    let init = canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "separate kinds",
            "--work-dir",
            session.to_str().expect("session path"),
            "--registries",
            map.to_str().expect("map path"),
        ])
        .assert()
        .success();
    let init_json = json_stdout(&init);
    assert_eq!(
        init_json["registries"]
            .as_array()
            .expect("registries")
            .len(),
        2
    );
    assert!(people.join("registry.json").is_file());
    assert!(organisations.join("30-session-aliases.json").is_file());

    let observations = write_lines(
        temp.path(),
        "observations.jsonl",
        &[
            observation("org-1", "organisation", "Shared Surface", Some("111")),
            observation("person-1", "person", "Shared Surface", None),
            observation("fund-1", "fund", "Unmapped Fund", None),
        ],
    );
    let ingest = canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
        ])
        .assert()
        .code(1);
    let ingest_json = json_stdout(&ingest);
    assert_eq!(ingest_json["accepted"], 2);
    assert_eq!(ingest_json["rejected"][0]["code"], "unmapped_entity_kind");

    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
        ])
        .assert()
        .success();
    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            "organisation=1.0.1,person=0.1.0",
        ])
        .assert()
        .success();
    let commit_json = json_stdout(&commit);
    assert_eq!(
        commit_json["registries"]
            .as_array()
            .expect("receipt registries")
            .len(),
        2
    );

    let after = snapshot_files(&organisations);
    for (relative, before) in original_organisation_files {
        if relative == Path::new("registry.json") {
            continue;
        }
        let current = after
            .iter()
            .find(|(path, _)| path == &relative)
            .map(|(_, bytes)| bytes)
            .expect("original registry file remains");
        assert_eq!(
            current,
            &before,
            "original file changed: {}",
            relative.display()
        );
    }
    assert!(organisations.join("30-session-aliases.json").is_file());
    assert!(people.join("aliases.json").is_file());

    for registry in [&organisations, &people] {
        canon()
            .args([
                "registry",
                "lint",
                registry.to_str().expect("registry path"),
                "--emit",
                "json",
            ])
            .assert()
            .success();
    }
}
