//! Public-command acceptance for `canon entity session explain` and
//! read-only `session solve --what-if`.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
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

fn run_json(args: &[&str], expected_exit: i32) -> Value {
    let output = canon().args(args).output().expect("canon runs");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "stderr={:?}",
        output.stderr
    );
    serde_json::from_slice(&output.stdout).expect("JSON stdout")
}

fn init(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "explain acceptance",
            "--registry",
            registry.to_str().expect("registry path"),
            "--work-dir",
            session.to_str().expect("session path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    (session, registry, rules)
}

fn write_jsonl(temp: &TempDir, name: &str, rows: &[Value]) -> PathBuf {
    let path = temp.path().join(name);
    let text = rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("row JSON"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{text}\n")).expect("JSONL");
    path
}

fn observation(obs_id: &str, lineage: &str, surface: &str) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {},
        "source": {"channel": "fixture", "id": "explain", "lineage": lineage}
    })
}

fn same_slot(left: &str, right: &str) -> Value {
    json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {
            "rule": "same loan and role",
            "left_value": "loan-7:sponsor",
            "right_value": "loan-7:sponsor",
            "steps": ["join:loan_id", "join:role"]
        },
        "source": {"channel": "agent", "id": "same-slot", "lineage": "fund-a"},
        "asserted_by": "agent:fixture"
    })
}

fn name_equivalent(left: &str, right: &str) -> Value {
    json!({
        "type": "name_equivalent",
        "left": left,
        "right": right,
        "basis": {
            "rule": "casefold",
            "left_value": "Alpha Borrower",
            "right_value": "Alpha Borrower LP",
            "steps": ["casefold"]
        },
        "source": {"channel": "script", "id": "name-equivalence", "lineage": "fund-b"},
        "asserted_by": "script:fixture"
    })
}

fn ingest(session: &Path, path: &Path) {
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            path.to_str().expect("input path"),
        ],
        0,
    );
}

fn assert_evidence(session: &Path, path: &Path) {
    run_json(
        &[
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            path.to_str().expect("input path"),
        ],
        0,
    );
}

fn solve(session: &Path, rules: &Path) {
    run_json(
        &[
            "entity",
            "session",
            "solve",
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
        ],
        0,
    );
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries = fs::read_dir(path)
            .expect("read directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("directory entries");
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let child = entry.path();
            let relative = child
                .strip_prefix(root)
                .expect("relative path")
                .to_string_lossy()
                .into_owned();
            if child.is_dir() {
                visit(root, &child, out);
            } else {
                out.insert(relative, fs::read(child).expect("read file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

#[test]
fn explain_projects_recorded_edges_and_next_evidence() {
    let temp = TempDir::new().expect("tempdir");
    let (session, _registry, rules) = init(&temp);
    let observations = write_jsonl(
        &temp,
        "observations.jsonl",
        &[
            observation("r8", "fund-a", "Alpha Borrower"),
            observation("r9", "fund-b", "Alpha Borrower LP"),
        ],
    );
    ingest(&session, &observations);
    let first_evidence = write_jsonl(&temp, "same-slot.jsonl", &[same_slot("r8", "r9")]);
    assert_evidence(&session, &first_evidence);
    solve(&session, &rules);

    let first = run_json(
        &[
            "entity",
            "session",
            "explain",
            session.to_str().expect("session path"),
            "r8",
        ],
        0,
    );
    assert_eq!(first["version"], "canon.entity.session.explain.v1");
    assert_eq!(first["target"]["kind"], "obs_id");
    assert_eq!(first["node"]["surface"], "Alpha Borrower");
    // Below-threshold edges are kept as review evidence, while the solver
    // retains each node as its own standalone cluster until the edge is
    // promotable.
    assert_eq!(first["cluster"]["outcome"], "standalone");
    assert!(first["cluster"]["tier"].is_null());
    assert_eq!(first["edges"][0]["evidence"][0]["type"], "same_slot");
    assert_eq!(first["edges"][0]["tier"], "C");
    assert_eq!(first["to_move_up"][0]["needs"], "second_evidence_type");
    assert_eq!(
        first["to_move_up"][0]["edge"],
        "Alpha Borrower <> Alpha Borrower LP"
    );

    let second_evidence = write_jsonl(
        &temp,
        "name-equivalent.jsonl",
        &[name_equivalent("r8", "r9")],
    );
    assert_evidence(&session, &second_evidence);
    solve(&session, &rules);
    let second = run_json(
        &[
            "entity",
            "session",
            "explain",
            session.to_str().expect("session path"),
            "surface:Alpha Borrower",
        ],
        0,
    );
    assert_eq!(second["cluster"]["tier"], "B");
    assert_eq!(second["edges"][0]["tier"], "B");
    let evidence = second["edges"][0]["evidence"]
        .as_array()
        .expect("edge evidence");
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().any(|row| row["type"] == "same_slot"));
    assert!(evidence.iter().any(|row| row["type"] == "name_equivalent"));
    assert!(evidence.iter().all(|row| row["lineages"].is_array()));
}

#[test]
fn what_if_reports_a_tier_change_without_touching_session_files() {
    let temp = TempDir::new().expect("tempdir");
    let (session, _registry, rules) = init(&temp);
    let observations = write_jsonl(
        &temp,
        "observations.jsonl",
        &[
            observation("r8", "fund-a", "Alpha Borrower"),
            observation("r9", "fund-b", "Alpha Borrower LP"),
        ],
    );
    ingest(&session, &observations);
    let first_evidence = write_jsonl(&temp, "same-slot.jsonl", &[same_slot("r8", "r9")]);
    assert_evidence(&session, &first_evidence);
    solve(&session, &rules);
    let extra = write_jsonl(&temp, "what-if.jsonl", &[name_equivalent("r8", "r9")]);
    let before = tree(&session);
    let result = run_json(
        &[
            "entity",
            "session",
            "solve",
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
            "--what-if",
            extra.to_str().expect("what-if path"),
        ],
        0,
    );
    assert!(result["rejected"].as_array().expect("rejected").is_empty());
    assert_eq!(result["changes"].as_array().expect("changes").len(), 1);
    assert_eq!(result["changes"][0]["before"]["tier"], "C");
    assert_eq!(result["changes"][0]["after"]["tier"], "B");
    assert_eq!(result["changes"][0]["before"]["outcome"], "review");
    assert_eq!(result["changes"][0]["after"]["outcome"], "accepted");
    assert_eq!(
        tree(&session),
        before,
        "what-if must not write the source session"
    );
}

#[test]
fn explain_refuses_unknown_target_and_marks_stale_solve() {
    let temp = TempDir::new().expect("tempdir");
    let (session, _registry, rules) = init(&temp);
    let observations = write_jsonl(
        &temp,
        "observations.jsonl",
        &[observation("r8", "fund-a", "Alpha Borrower")],
    );
    ingest(&session, &observations);
    solve(&session, &rules);
    let refusal = run_json(
        &[
            "entity",
            "session",
            "explain",
            session.to_str().expect("session path"),
            "missing-obs",
        ],
        2,
    );
    assert_eq!(refusal["refusal"]["code"], "E_ENTITY_ARTIFACT_CONTRACT");
    assert!(
        refusal["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("status")
    );

    let extra = write_jsonl(
        &temp,
        "new-observation.jsonl",
        &[observation("r9", "fund-b", "New Surface")],
    );
    ingest(&session, &extra);
    let stale = run_json(
        &[
            "entity",
            "session",
            "explain",
            session.to_str().expect("session path"),
            "r8",
        ],
        0,
    );
    assert_eq!(stale["stale"], true);
    assert!(
        stale["stale_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "observations_generation")
    );
    assert!(stale["cluster"].is_null());
}
