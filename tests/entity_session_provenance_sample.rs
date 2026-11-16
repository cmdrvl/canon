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

fn write(path: &Path, value: &str) -> PathBuf {
    fs::write(path, value).expect("fixture");
    path.to_path_buf()
}

fn init(temp: &TempDir) -> (PathBuf, PathBuf) {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "provenance sample acceptance",
            "--registry",
            registry.to_str().expect("registry"),
            "--work-dir",
            session.to_str().expect("session"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    (registry, session)
}

#[test]
fn session_commit_writes_authority_and_sampling_is_seeded_and_scored() {
    let temp = TempDir::new().expect("tempdir");
    let (registry, session) = init(&temp);
    let observation_text = (0..7)
        .map(|index| {
            json!({
                "obs_id": format!("obs-{index}"),
                "entity_type": "organisation",
                "surface": format!("Alpha Holdings {index}"),
                "keys": {"cik": "111"},
                "source": {
                    "channel": "warehouse",
                    "id": format!("row-{index}"),
                    "lineage": format!("filing-{index}")
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let observations = write(
        &temp.path().join("observations.jsonl"),
        &format!("{observation_text}\n"),
    );
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session"),
            observations.to_str().expect("observations"),
        ])
        .assert()
        .success();
    let rules = write(&temp.path().join("rules.yaml"), RULES);
    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().expect("session"),
            "--rules",
            rules.to_str().expect("rules"),
        ])
        .assert()
        .success();
    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session"),
            "--next-version",
            "0.1.0",
        ])
        .assert()
        .success();
    let commit_json: Value =
        serde_json::from_slice(&commit.get_output().stdout).expect("commit JSON");
    assert_eq!(commit_json["outcome"], "committed");
    assert!(commit_json["closure_set_hash"].as_str().is_some());
    let aliases: Value =
        serde_json::from_slice(&fs::read(registry.join("aliases.json")).expect("aliases"))
            .expect("aliases JSON");
    let alias = aliases
        .as_array()
        .expect("alias array")
        .first()
        .expect("committed alias");
    let authority = alias
        .get("session_authority")
        .expect("session authority on committed alias");
    assert_eq!(authority["tier"], "A");
    assert_eq!(authority["sensitivity"], "not_public");
    assert!(
        authority["outcome_ref"]
            .as_str()
            .is_some_and(|value| { value.starts_with("cmdrvl://canon/outcome/organisations/") })
    );
    assert!(authority["closure_hash"].as_str().is_some());

    let first = canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--stratum",
            "tier_a",
            "--count",
            "20",
            "--seed",
            "7",
        ])
        .assert()
        .success();
    let second = canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--stratum",
            "tier_a",
            "--count",
            "20",
            "--seed",
            "7",
        ])
        .assert()
        .success();
    assert_eq!(first.get_output().stdout, second.get_output().stdout);
    let different_seed = canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--stratum",
            "tier_a",
            "--count",
            "20",
            "--seed",
            "8",
        ])
        .assert()
        .success();
    assert_ne!(
        first.get_output().stdout,
        different_seed.get_output().stdout
    );
    let first_text = String::from_utf8(first.get_output().stdout.clone()).expect("sample UTF-8");
    let mut sample_lines = first_text.lines();
    let header: Value = serde_json::from_str(sample_lines.next().expect("sample header"))
        .expect("sample header JSON");
    let sample_rows = sample_lines
        .map(|line| serde_json::from_str::<Value>(line).expect("sample row JSON"))
        .collect::<Vec<_>>();
    assert_eq!(header["population_size"], 21);
    assert_eq!(header["returned_count"], 20);

    let blind = canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--stratum",
            "tier_a",
            "--count",
            "20",
            "--seed",
            "7",
            "--blind",
        ])
        .assert()
        .success();
    let blind_text = String::from_utf8_lossy(&blind.get_output().stdout);
    assert!(!blind_text.contains("\"stratum\""));
    assert!(!blind_text.contains("evidence_types"));

    let mut label_lines = vec![serde_json::to_string(&header).expect("header")];
    label_lines.extend(sample_rows.iter().enumerate().map(|(index, row)| {
        json!({
            "sample_id": row["sample_id"],
            "label": if index < 18 { "same" } else { "different" },
            "labeller": "operator-1"
        })
        .to_string()
    }));
    let labels = write(
        &temp.path().join("labels.jsonl"),
        &format!("{}\n", label_lines.join("\n")),
    );
    let score = canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--score",
            labels.to_str().expect("labels"),
        ])
        .assert()
        .success();
    let score_json: Value = serde_json::from_slice(&score.get_output().stdout).expect("score JSON");
    assert_eq!(score_json["strata"][0]["n"], 20);
    assert_eq!(score_json["strata"][0]["precision"], 0.9);
    assert_eq!(score_json["strata"][0]["population_size"], 21);

    let wrong_labels = write(
        &temp.path().join("wrong-labels.jsonl"),
        &format!(
            "{}\n{}\n",
            json!({
                "kind": "canon.entity.session.sample.header",
                "solve_hash": "blake3:wrong"
            }),
            json!({
                "sample_id": sample_rows[0]["sample_id"],
                "label": "same",
                "labeller": "operator-1"
            })
        ),
    );
    canon()
        .args([
            "entity",
            "session",
            "sample",
            session.to_str().expect("session"),
            "--score",
            wrong_labels.to_str().expect("wrong labels"),
        ])
        .assert()
        .code(2);
}
