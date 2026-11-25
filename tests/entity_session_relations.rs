#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::entity::{
    publication::open_current_stream_generation, session::SESSION_SOLVE_STREAM_ID,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
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

fn init(temp: &TempDir, label: &str, registry: Option<&Path>) -> (PathBuf, PathBuf) {
    let registry = registry
        .map(Path::to_path_buf)
        .unwrap_or_else(|| temp.path().join(format!("{label}-registry")));
    let session = temp.path().join(format!("{label}-session"));
    let mut args = vec![
        "entity".to_string(),
        "session".to_string(),
        "init".to_string(),
        "--goal".to_string(),
        format!("relation acceptance {label}"),
        "--registry".to_string(),
        registry.to_string_lossy().into_owned(),
        "--work-dir".to_string(),
        session.to_string_lossy().into_owned(),
    ];
    if registry.join("registry.json").is_file() {
        args.push("--registry-id".to_string());
        args.push("organisations".to_string());
    } else {
        args.extend([
            "--registry-id".to_string(),
            "organisations".to_string(),
            "--prefix".to_string(),
            "ORG".to_string(),
            "--zero-pad".to_string(),
            "3".to_string(),
        ]);
    }
    canon().args(args).assert().success();
    (registry, session)
}

fn observation(
    id: &str,
    lineage: &str,
    surface: &str,
    cik: &str,
    role: Option<&str>,
    deal: Option<&str>,
    as_of: Option<&str>,
) -> String {
    let mut value = json!({
        "obs_id": id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage}
    });
    if let Some(role) = role {
        value["context"] = json!({"role": role});
    }
    if let Some(deal) = deal {
        value["context"]["deal_id"] = json!(deal);
        value["context"]["as_of_date"] = json!(as_of.unwrap_or("2025-01-01"));
    }
    serde_json::to_string(&value).expect("observation JSON")
}

fn write_lines(temp: &TempDir, name: &str, lines: &[String]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture");
    path
}

fn ingest(session: &Path, input: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .assert()
        .success();
}

fn assert_evidence(session: &Path, input: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .assert()
        .success();
}

fn solve(session: &Path, rules: &Path) -> Value {
    let output = canon()
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
    json_stdout(&output)
}

fn commit(session: &Path, version: Option<&str>) -> Value {
    let mut args = vec![
        "entity".to_string(),
        "session".to_string(),
        "commit".to_string(),
        session.to_string_lossy().into_owned(),
    ];
    if let Some(version) = version {
        args.extend(["--next-version".to_string(), version.to_string()]);
    } else {
        args.extend(["--bump".to_string(), "patch".to_string()]);
    }
    let output = canon().args(args).assert().success();
    json_stdout(&output)
}

fn relation_evidence(left: &str, right: &str, relation: &str, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "type": "related",
        "left": left,
        "right": right,
        "basis": {"relation": relation, "subject": left},
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
        "asserted_by": "agent:fixture"
    }))
    .expect("relation JSON")
}

fn same_slot(left: &str, right: &str, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {"rule": "same loan and role", "left_value": "loan-7:sponsor", "right_value": "loan-7:sponsor"},
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
        "asserted_by": "agent:fixture"
    }))
    .expect("same-slot JSON")
}

fn name_equivalent(left: &str, right: &str, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "type": "name_equivalent",
        "left": left,
        "right": right,
        "basis": {"rule": "casefold", "left_value": "Alpha", "right_value": "Beta"},
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
        "asserted_by": "agent:fixture"
    }))
    .expect("name-equivalent JSON")
}

fn copy_registry_without_relations(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("copy registry directory");
    for entry in fs::read_dir(source).expect("read registry directory") {
        let entry = entry.expect("registry entry");
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("relations.jsonl")
            || !path.is_file()
        {
            continue;
        }
        fs::copy(&path, target.join(entry.file_name())).expect("copy registry file");
    }
}

fn solve_artifact_hash(session: &Path) -> String {
    let snapshot = open_current_stream_generation(session, SESSION_SOLVE_STREAM_ID)
        .expect("current solve generation");
    let bytes = snapshot
        .read_logical_file("solve/solve.json")
        .expect("solve artifact");
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn relation_facts(registry: &Path) -> Vec<Value> {
    fs::read_to_string(registry.join("relations.jsonl"))
        .expect("relations sidecar")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("relation line JSON"))
        .collect()
}

#[test]
fn resolving_again_then_recommitting_keeps_relation_fact_and_id_stable() {
    let temp = TempDir::new().expect("tempdir");
    let rules = write_lines(&temp, "stable-rules.yaml", &[RULES.trim_end().to_string()]);
    let (registry, session) = init(&temp, "stable-relation", None);
    let observations = write_lines(
        &temp,
        "stable-observations.jsonl",
        &[
            observation("stable-a", "filing-a", "Alpha", "100", None, None, None),
            observation("stable-b", "filing-b", "Beta", "200", None, None, None),
        ],
    );
    ingest(&session, &observations);
    let evidence = write_lines(
        &temp,
        "stable-evidence.jsonl",
        &[relation_evidence(
            "stable-a",
            "stable-b",
            "part_of",
            "stable-relation-lineage",
        )],
    );
    assert_evidence(&session, &evidence);

    let first_solve = solve(&session, &rules);
    assert_eq!(first_solve["summary"]["relations"], 1);
    let first_solve_hash = solve_artifact_hash(&session);
    let first_commit = commit(&session, Some("0.1.0"));
    assert_eq!(first_commit["relations_written"], 1);
    let first_facts = relation_facts(&registry);
    assert_eq!(first_facts.len(), 1);
    assert_eq!(first_facts[0]["relation"]["value"], "part_of");
    let relation_id = first_facts[0]["relation_id"]
        .as_str()
        .expect("first relation id");

    // A new unrelated observation changes the solve artifact while leaving
    // the existing relation evidence and edge unchanged.
    let extra_observation = write_lines(
        &temp,
        "stable-extra-observation.jsonl",
        &[observation(
            "stable-c", "filing-c", "Gamma", "300", None, None, None,
        )],
    );
    ingest(&session, &extra_observation);
    let second_solve = solve(&session, &rules);
    assert_eq!(second_solve["summary"]["relations"], 1);
    let second_solve_hash = solve_artifact_hash(&session);
    assert_ne!(
        second_solve_hash, first_solve_hash,
        "the regression must exercise a distinct solve artifact"
    );

    let second_commit = commit(&session, None);
    assert_eq!(second_commit["relations_written"], 0);
    let second_facts = relation_facts(&registry);
    assert_eq!(
        second_facts.len(),
        1,
        "re-commit must not append a duplicate edge"
    );
    assert_eq!(second_facts[0]["relation_id"], relation_id);
}

#[test]
fn relations_persist_suppress_without_merging_and_reject_uncommitted_endpoints() {
    let temp = TempDir::new().expect("tempdir");
    let rules = write_lines(&temp, "rules.yaml", &[RULES.trim_end().to_string()]);
    let (registry, session) = init(&temp, "first", None);
    let observations = write_lines(
        &temp,
        "observations.jsonl",
        &[
            observation(
                "a",
                "filing-a",
                "Alpha",
                "100",
                Some("sponsor"),
                Some("deal-a"),
                Some("2024-01-01"),
            ),
            observation(
                "b",
                "filing-b",
                "Beta",
                "200",
                Some("guarantor"),
                Some("deal-b"),
                Some("2024-02-01"),
            ),
            observation(
                "a2",
                "filing-a2",
                "Alpha",
                "100",
                Some("sponsor"),
                Some("deal-a"),
                Some("2025-01-01"),
            ),
            observation(
                "a3",
                "filing-a3",
                "Alpha",
                "100",
                Some("guarantor"),
                Some("deal-b"),
                Some("2025-02-01"),
            ),
        ],
    );
    ingest(&session, &observations);
    let evidence = write_lines(
        &temp,
        "relations.jsonl",
        &[relation_evidence("a", "b", "part_of", "relation-lineage")],
    );
    assert_evidence(&session, &evidence);
    let first_solve = solve(&session, &rules);
    assert_eq!(first_solve["summary"]["relations"], 4);
    let first_commit = commit(&session, Some("0.1.0"));
    assert_eq!(first_commit["outcome"], "committed");
    assert_eq!(first_commit["relations_written"], 4);

    let relation_lines = fs::read_to_string(registry.join("relations.jsonl"))
        .expect("relations sidecar")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("relation line JSON"))
        .collect::<Vec<_>>();
    assert_eq!(relation_lines.len(), 4);
    assert!(
        relation_lines
            .iter()
            .any(|line| line["relation"]["value"] == "part_of")
    );
    assert!(
        relation_lines
            .iter()
            .any(|line| line["relation"]["value"] == "sponsor_of")
    );
    assert!(
        relation_lines
            .iter()
            .any(|line| line["relation"]["value"] == "guarantor_of")
    );
    let sponsor = relation_lines
        .iter()
        .find(|line| {
            line["relation"]["value"] == "sponsor_of" && line["object"]["identity_id"] == "deal-a"
        })
        .expect("sponsor relation");
    assert_eq!(sponsor["valid_time"]["start_at"], "2024-01-01T00:00:00Z");
    assert_eq!(sponsor["valid_time"]["end_at"], "2025-01-01T00:00:00Z");
    let same_entity_guarantor = relation_lines
        .iter()
        .find(|line| {
            line["relation"]["value"] == "guarantor_of"
                && line["object"]["identity_id"] == "deal-b"
                && line["subject"]["identity_id"] == sponsor["subject"]["identity_id"]
        })
        .expect("same entity guarantor relation");
    assert_eq!(
        same_entity_guarantor["subject"]["identity_id"],
        sponsor["subject"]["identity_id"]
    );

    let lint = canon()
        .args(["registry", "lint", registry.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(json_stdout(&lint)["summary"]["errors"], 0);

    let (same_registry, same_session) = init(&temp, "same", Some(&registry));
    let same_observations = write_lines(
        &temp,
        "same-observations.jsonl",
        &[
            observation("same-a", "same-a", "Alpha", "100", None, None, None),
            observation("same-b", "same-b", "Beta", "200", None, None, None),
        ],
    );
    ingest(&same_session, &same_observations);
    let same_evidence = write_lines(
        &temp,
        "same-evidence.jsonl",
        &[same_slot("same-a", "same-b", "same-lineage")],
    );
    assert_evidence(&same_session, &same_evidence);
    let same_solve = solve(&same_session, &rules);
    assert_eq!(same_solve["summary"]["review"], 0);
    assert_eq!(same_solve["summary"]["suppressed_by_relation"], 1);
    assert_eq!(same_solve["summary"]["cluster_count"], 2);
    assert_eq!(same_solve["summary"]["accepted_tier_a"], 0);
    assert_eq!(same_solve["summary"]["accepted_tier_b"], 0);
    assert_eq!(same_registry, registry);

    let no_relation_registry = temp.path().join("no-relation-registry");
    copy_registry_without_relations(&registry, &no_relation_registry);
    let (_no_relation_registry, no_relation_session) =
        init(&temp, "no-relation", Some(&no_relation_registry));
    let no_relation_observations = write_lines(
        &temp,
        "no-relation-observations.jsonl",
        &[
            observation("nr-a", "nr-a", "Alpha", "100", None, None, None),
            observation("nr-b", "nr-b", "Beta", "200", None, None, None),
        ],
    );
    ingest(&no_relation_session, &no_relation_observations);
    let no_relation_evidence = write_lines(
        &temp,
        "no-relation-evidence.jsonl",
        &[same_slot("nr-a", "nr-b", "no-relation-lineage")],
    );
    assert_evidence(&no_relation_session, &no_relation_evidence);
    let no_relation_solve = solve(&no_relation_session, &rules);
    assert_eq!(no_relation_solve["summary"]["review"], 1);
    assert_eq!(no_relation_solve["summary"]["suppressed_by_relation"], 0);

    let role_registry = temp.path().join("role-registry");
    let role_session = init(&temp, "role-first", Some(&role_registry)).1;
    let role_observations = write_lines(
        &temp,
        "role-observations.jsonl",
        &[
            observation(
                "role-a",
                "role-a",
                "Alpha",
                "100",
                Some("sponsor"),
                Some("deal-a"),
                Some("2024-01-01"),
            ),
            observation(
                "role-b",
                "role-b",
                "Beta",
                "200",
                Some("guarantor"),
                Some("deal-b"),
                Some("2024-02-01"),
            ),
        ],
    );
    ingest(&role_session, &role_observations);
    let role_solve = solve(&role_session, &rules);
    assert_eq!(role_solve["summary"]["relations"], 2);
    let role_commit = commit(&role_session, Some("0.1.0"));
    assert_eq!(role_commit["relations_written"], 2);

    let role_later_session = init(&temp, "role-later", Some(&role_registry)).1;
    let role_later_observations = write_lines(
        &temp,
        "role-later-observations.jsonl",
        &[
            observation(
                "role-later-a",
                "role-later-a",
                "Alpha",
                "100",
                None,
                None,
                None,
            ),
            observation(
                "role-later-b",
                "role-later-b",
                "Beta",
                "200",
                None,
                None,
                None,
            ),
        ],
    );
    ingest(&role_later_session, &role_later_observations);
    let role_later_evidence = write_lines(
        &temp,
        "role-later-evidence.jsonl",
        &[same_slot(
            "role-later-a",
            "role-later-b",
            "role-later-lineage",
        )],
    );
    assert_evidence(&role_later_session, &role_later_evidence);
    let role_later_solve = solve(&role_later_session, &rules);
    assert_eq!(role_later_solve["summary"]["review"], 1);
    assert_eq!(role_later_solve["summary"]["suppressed_by_relation"], 0);
    assert_eq!(role_later_solve["summary"]["contradiction"], 0);
    assert_eq!(role_later_solve["summary"]["cluster_count"], 2);

    let conflict_session = init(&temp, "conflict", Some(&registry)).1;
    let conflict_observations = write_lines(
        &temp,
        "conflict-observations.jsonl",
        &[
            observation("c-a", "c-a", "Alpha", "100", None, None, None),
            observation("c-b", "c-b", "Beta", "200", None, None, None),
        ],
    );
    ingest(&conflict_session, &conflict_observations);
    let conflict_evidence = write_lines(
        &temp,
        "conflict-evidence.jsonl",
        &[
            same_slot("c-a", "c-b", "lineage-a"),
            name_equivalent("c-a", "c-b", "lineage-b"),
        ],
    );
    assert_evidence(&conflict_session, &conflict_evidence);
    let conflict_solve = solve(&conflict_session, &rules);
    assert_eq!(conflict_solve["summary"]["contradiction"], 1);
    assert_eq!(
        conflict_solve["contradictions"][0]["reason"],
        "related_not_same"
    );

    let uncommitted = write_lines(
        &temp,
        "uncommitted-observations.jsonl",
        &[observation("u", "u", "Uncommitted", "", None, None, None)],
    );
    ingest(&same_session, &uncommitted);
    let uncommitted_evidence = write_lines(
        &temp,
        "uncommitted-evidence.jsonl",
        &[relation_evidence(
            "same-a",
            "u",
            "affiliate_of",
            "uncommitted-lineage",
        )],
    );
    assert_evidence(&same_session, &uncommitted_evidence);
    let uncommitted_solve = solve(&same_session, &rules);
    assert_eq!(uncommitted_solve["summary"]["relations"], 1);
    let unchanged = fs::read_to_string(registry.join("relations.jsonl")).expect("sidecar");
    // Nothing in this session is writable (the one relation touches an
    // uncommitted surface), so commit reports nothing_to_commit with exit 1.
    let nothing = canon()
        .args([
            "entity",
            "session",
            "commit",
            same_session.to_str().unwrap(),
            "--bump",
            "patch",
        ])
        .assert()
        .code(1);
    assert_eq!(json_stdout(&nothing)["outcome"], "nothing_to_commit");
    assert_eq!(
        fs::read_to_string(registry.join("relations.jsonl")).expect("sidecar"),
        unchanged,
        "relations touching an uncommitted surface stay session-only"
    );
}
