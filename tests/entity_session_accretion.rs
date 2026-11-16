#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik, lei]
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
"#;

const RULES_WITHOUT_INCUMBENT_ATTACH: &str = r#"version: canon.entity.session.rules.v1
keys:
  authoritative: [cik, lei]
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

fn json_output(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn observation(obs_id: &str, lineage: &str, surface: &str, cik: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "file", "id": "accretion", "lineage": lineage}
    }))
    .expect("observation JSON")
}

fn observation_without_key(obs_id: &str, lineage: &str, surface: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {},
        "source": {"channel": "file", "id": "accretion", "lineage": lineage}
    }))
    .expect("observation JSON")
}

fn write_lines(temp: &TempDir, name: &str, lines: &[String]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("fixture");
    path
}

fn init(registry: &Path, session: &Path, prefix: Option<&str>) {
    let mut args = vec![
        "entity".to_string(),
        "session".to_string(),
        "init".to_string(),
        "--goal".to_string(),
        "session accretion".to_string(),
        "--registry".to_string(),
        registry.to_str().expect("registry path").to_string(),
        "--work-dir".to_string(),
        session.to_str().expect("session path").to_string(),
        "--registry-id".to_string(),
        "organisations".to_string(),
        "--entity-kind".to_string(),
        "organisation".to_string(),
    ];
    if let Some(prefix) = prefix {
        args.extend([
            "--prefix".to_string(),
            prefix.to_string(),
            "--zero-pad".to_string(),
            "3".to_string(),
        ]);
    }
    canon().args(args).assert().success();
}

fn ingest(session: &Path, observations: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
        ])
        .assert()
        .success();
}

fn assert_evidence(session: &Path, evidence: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            evidence.to_str().expect("evidence path"),
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
            session.to_str().expect("session path"),
            "--rules",
            rules.to_str().expect("rules path"),
        ])
        .assert()
        .success();
    json_output(&output)
}

fn commit(session: &Path, version: &str) -> assert_cmd::assert::Assert {
    canon()
        .args([
            "entity",
            "session",
            "commit",
            session.to_str().expect("session path"),
            "--next-version",
            version,
        ])
        .assert()
}

fn same_key(left: &str, right: &str, value: &str) -> String {
    serde_json::to_string(&json!({
        "type": "same_key",
        "left": left,
        "right": right,
        "basis": {"namespace": "cik", "value": value},
        "source": {"channel": "agent", "id": "accretion", "lineage": "agent-key"},
        "asserted_by": "agent:accretion"
    }))
    .expect("same_key evidence")
}

fn name_equivalent(left: &str, right: &str, left_value: &str, right_value: &str) -> String {
    serde_json::to_string(&json!({
        "type": "name_equivalent",
        "left": left,
        "right": right,
        "basis": {
            "left_value": left_value,
            "right_value": right_value,
            "steps": ["casefold", "strip_punctuation", "collapse_whitespace"]
        },
        "source": {"channel": "agent", "id": "accretion", "lineage": "agent-name"},
        "asserted_by": "agent:accretion"
    }))
    .expect("name_equivalent evidence")
}

fn asserted_alias(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "asserted_alias",
        "left": left,
        "right": right,
        "basis": {"reason": "reviewed incumbent alias"},
        "source": {"channel": "human", "id": "review:accretion"},
        "asserted_by": "human:accretion"
    }))
    .expect("asserted_alias evidence")
}

#[test]
fn session_accretion_attaches_to_incumbents_and_rejects_planted_conflicts() {
    let temp = TempDir::new().expect("tempdir");
    let registry = temp.path().join("registry");
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");

    let first = temp.path().join("first-session");
    init(&registry, &first, Some("ORG"));
    let first_observations = write_lines(
        &temp,
        "first-observations.jsonl",
        &[
            observation("first-a", "fund-a", "Medline Borrower, LP", "111"),
            observation("first-b", "fund-b", "Medline Borrower Partners", "111"),
            observation("second-a", "fund-c", "Other Holdings, Inc", "222"),
            observation("second-b", "fund-d", "Other Holdings Incorporated", "222"),
        ],
    );
    ingest(&first, &first_observations);
    solve(&first, &rules);
    let first_commit = commit(&first, "1.0.1").success();
    let first_json = json_output(&first_commit);
    assert_eq!(first_json["outcome"], "committed");
    assert_eq!(first_json["registry"]["version_after"], "1.0.1");
    assert_eq!(first_json["registry"]["entry_count_after"], 6);
    assert_eq!(first_json["minted"].as_array().expect("minted").len(), 2);
    assert_eq!(
        first_json["attached"].as_array().expect("attached").len(),
        0
    );
    let first_aliases: Value = serde_json::from_slice(
        &fs::read(registry.join("aliases.json")).expect("aliases after first commit"),
    )
    .expect("aliases JSON");
    let incumbent_id = first_aliases
        .as_array()
        .expect("aliases array")
        .iter()
        .find(|alias| alias["input"] == "Medline Borrower, LP")
        .and_then(|alias| alias["canonical_id"].as_str())
        .expect("Medline incumbent")
        .to_string();

    let second = temp.path().join("second-session");
    init(&registry, &second, None);
    let second_observations = write_lines(
        &temp,
        "second-observations.jsonl",
        &[observation(
            "day-two",
            "fund-e",
            "Medline Borrower LP",
            "111",
        )],
    );
    ingest(&second, &second_observations);
    let second_evidence = write_lines(
        &temp,
        "second-evidence.jsonl",
        &[
            same_key("day-two", "registry:key:cik:111", "111"),
            name_equivalent(
                "day-two",
                "registry:alias:Medline Borrower, LP",
                "Medline Borrower LP",
                "Medline Borrower, LP",
            ),
        ],
    );
    assert_evidence(&second, &second_evidence);
    solve(&second, &rules);
    let second_commit = commit(&second, "1.0.2").success();
    let second_json = json_output(&second_commit);
    assert_eq!(second_json["outcome"], "committed");
    assert_eq!(second_json["registry"]["version_after"], "1.0.2");
    assert_eq!(second_json["registry"]["entry_count_before"], 6);
    assert_eq!(second_json["registry"]["entry_count_after"], 7);
    assert_eq!(second_json["minted"].as_array().expect("minted").len(), 0);
    let attached = second_json["attached"].as_array().expect("attached");
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0]["canonical_id"], incumbent_id);
    assert!(attached[0]["aliases"].as_array().is_some_and(|aliases| {
        aliases
            .iter()
            .any(|alias| alias["input"] == "Medline Borrower LP")
    }));
    let repeat = commit(&second, "1.0.2").success();
    assert_eq!(json_output(&repeat)["outcome"], "already_committed");
    assert_eq!(json_output(&repeat)["attached"], second_json["attached"]);

    let second_more = write_lines(
        &temp,
        "second-more-observations.jsonl",
        &[observation(
            "day-two-more",
            "fund-f",
            "Medline Borrower Limited Partnership",
            "111",
        )],
    );
    ingest(&second, &second_more);
    solve(&second, &rules);
    let second_more_commit = commit(&second, "1.0.3").success();
    let second_more_json = json_output(&second_more_commit);
    assert_eq!(second_more_json["outcome"], "committed");
    assert_eq!(second_more_json["registry"]["version_after"], "1.0.3");
    assert_eq!(second_more_json["registry"]["entry_count_before"], 7);
    assert_eq!(second_more_json["registry"]["entry_count_after"], 8);
    assert_eq!(
        second_more_json["minted"].as_array().expect("minted").len(),
        0
    );
    assert_eq!(
        second_more_json["attached"]
            .as_array()
            .expect("attached")
            .len(),
        1
    );
    assert!(
        second_more_json["attached"][0]["aliases"]
            .as_array()
            .is_some_and(|aliases| {
                aliases
                    .iter()
                    .any(|alias| alias["input"] == "Medline Borrower Limited Partnership")
            })
    );
    let second_more_repeat = commit(&second, "1.0.3").success();
    assert_eq!(
        json_output(&second_more_repeat)["outcome"],
        "already_committed"
    );

    let by_key = temp.path().join("by-key");
    init(&registry, &by_key, None);
    let by_key_observations = write_lines(
        &temp,
        "by-key-observations.jsonl",
        &[observation(
            "key-only",
            "fund-f",
            "Medline Industries LP",
            "111",
        )],
    );
    ingest(&by_key, &by_key_observations);
    solve(&by_key, &rules);
    let by_key_commit = commit(&by_key, "1.0.4").success();
    let by_key_json = json_output(&by_key_commit);
    assert_eq!(by_key_json["outcome"], "committed");
    assert_eq!(by_key_json["registry"]["version_after"], "1.0.4");
    assert_eq!(by_key_json["registry"]["entry_count_before"], 8);
    assert_eq!(by_key_json["registry"]["entry_count_after"], 9);
    assert_eq!(by_key_json["minted"].as_array().expect("minted").len(), 0);
    assert_eq!(
        by_key_json["attached"].as_array().expect("attached").len(),
        1
    );
    assert!(
        by_key_json["attached"][0]["aliases"]
            .as_array()
            .is_some_and(|aliases| aliases
                .iter()
                .any(|alias| { alias["input"] == "Medline Industries LP" }))
    );

    let key_conflict = temp.path().join("key-conflict");
    init(&registry, &key_conflict, None);
    let key_conflict_observations = write_lines(
        &temp,
        "key-conflict-observations.jsonl",
        &[
            observation(
                "wrong-incumbent-key",
                "fund-f",
                "Medline Borrower, LP",
                "222",
            ),
            observation("wrong-new-key", "fund-g", "Medline Wrong", "999"),
        ],
    );
    ingest(&key_conflict, &key_conflict_observations);
    let key_conflict_evidence = write_lines(
        &temp,
        "key-conflict-evidence.jsonl",
        &[name_equivalent(
            "wrong-new-key",
            "registry:alias:Medline Borrower, LP",
            "Medline Wrong",
            "Medline Borrower, LP",
        )],
    );
    assert_evidence(&key_conflict, &key_conflict_evidence);
    solve(&key_conflict, &rules);
    let key_conflict_commit = commit(&key_conflict, "1.0.5").code(1);
    let key_conflict_json = json_output(&key_conflict_commit);
    assert_eq!(key_conflict_json["outcome"], "nothing_to_commit");
    let key_conflict_reasons = key_conflict_json["skipped"]
        .as_array()
        .expect("key conflict skipped")
        .iter()
        .filter_map(|row| row["reason"].as_str())
        .collect::<Vec<_>>();
    assert!(key_conflict_reasons.contains(&"incumbent_conflict"));
    assert!(key_conflict_reasons.contains(&"key_conflict"));

    let cross_incumbent = temp.path().join("cross-incumbent");
    init(&registry, &cross_incumbent, None);
    let cross_observations = write_lines(
        &temp,
        "cross-observations.jsonl",
        &[observation_without_key("cross", "fund-g", "Shared Alias")],
    );
    ingest(&cross_incumbent, &cross_observations);
    let cross_evidence = write_lines(
        &temp,
        "cross-evidence.jsonl",
        &[
            asserted_alias("cross", "registry:alias:Medline Borrower, LP"),
            asserted_alias("cross", "registry:alias:Other Holdings, Inc"),
        ],
    );
    assert_evidence(&cross_incumbent, &cross_evidence);
    solve(&cross_incumbent, &rules);
    let cross_commit = commit(&cross_incumbent, "1.0.5").code(1);
    let cross_json = json_output(&cross_commit);
    assert_eq!(cross_json["outcome"], "nothing_to_commit");
    assert!(
        cross_json["skipped"]
            .as_array()
            .is_some_and(|rows| { rows.iter().any(|row| row["reason"] == "contradiction") })
    );

    let no_attach = temp.path().join("no-attach");
    init(&registry, &no_attach, None);
    let no_attach_observations = write_lines(
        &temp,
        "no-attach-observations.jsonl",
        &[observation_without_key(
            "no-attach-row",
            "fund-h",
            "Medline LP",
        )],
    );
    ingest(&no_attach, &no_attach_observations);
    let no_attach_evidence = write_lines(
        &temp,
        "no-attach-evidence.jsonl",
        &[name_equivalent(
            "no-attach-row",
            "registry:alias:Medline Borrower, LP",
            "Medline LP",
            "Medline Borrower, LP",
        )],
    );
    assert_evidence(&no_attach, &no_attach_evidence);
    let no_attach_rules = temp.path().join("rules-without-incumbent-attach.yaml");
    fs::write(&no_attach_rules, RULES_WITHOUT_INCUMBENT_ATTACH).expect("no-attach rules");
    solve(&no_attach, &no_attach_rules);
    let no_attach_commit = commit(&no_attach, "1.0.5").code(1);
    let no_attach_json = json_output(&no_attach_commit);
    assert_eq!(no_attach_json["outcome"], "nothing_to_commit");
    assert_eq!(no_attach_json["attached"], json!([]));

    let dangling = temp.path().join("dangling");
    init(&registry, &dangling, None);
    let dangling_observations = write_lines(
        &temp,
        "dangling-observations.jsonl",
        &[observation_without_key(
            "dangling-row",
            "fund-i",
            "Dangling Surface",
        )],
    );
    ingest(&dangling, &dangling_observations);
    let dangling_evidence = write_lines(
        &temp,
        "dangling-evidence.jsonl",
        &[name_equivalent(
            "dangling-row",
            "registry:alias:Not In Registry",
            "Dangling Surface",
            "Not In Registry",
        )],
    );
    assert_evidence(&dangling, &dangling_evidence);
    let dangling_payload = solve(&dangling, &rules);
    assert_eq!(dangling_payload["summary"]["dangling_evidence"], 1);
    let dangling_commit = commit(&dangling, "1.0.5").code(1);
    assert_eq!(
        json_output(&dangling_commit)["outcome"],
        "nothing_to_commit"
    );

    let no_op = temp.path().join("no-op");
    init(&registry, &no_op, None);
    let no_op_observations = write_lines(
        &temp,
        "no-op-observations.jsonl",
        &[observation(
            "already-known",
            "fund-h",
            "Medline Borrower, LP",
            "111",
        )],
    );
    ingest(&no_op, &no_op_observations);
    solve(&no_op, &rules);
    let no_op_commit = commit(&no_op, "1.0.3").code(1);
    let no_op_json = json_output(&no_op_commit);
    assert_eq!(no_op_json["outcome"], "nothing_to_commit");
    assert_eq!(no_op_json["attached"], json!([]));
}
