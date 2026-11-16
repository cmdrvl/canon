#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::entity::session::rules::{
    DEFAULT_MAX_CLUSTER_SURFACES, SESSION_RULES_VERSION, init_rules, parse_rules, validate_rules,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn rules_file(temp: &TempDir, contents: &str) -> PathBuf {
    let path = temp.path().join("rules.yaml");
    fs::write(&path, contents).expect("rules fixture");
    path
}

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn init_session(temp: &TempDir, label: &str) -> PathBuf {
    let registry = temp.path().join(format!("{label}-registry"));
    let session = temp.path().join(format!("{label}-session"));
    canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "rule table acceptance",
            "--registry",
            registry.to_str().expect("registry path"),
            "--work-dir",
            session.to_str().expect("session path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
        ])
        .assert()
        .success();
    session
}

fn write_jsonl(temp: &TempDir, name: &str, rows: &[Value]) -> PathBuf {
    let path = temp.path().join(name);
    let contents = rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("JSON row"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{contents}\n")).expect("JSONL fixture");
    path
}

fn ingest(session: &Path, observations: &Path) {
    canon_command()
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

fn solve(session: &Path, rules: &Path) -> Value {
    let output = canon_command()
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
    serde_json::from_slice(&output.get_output().stdout).expect("solve JSON")
}

fn observation(id: &str, surface: &str, lineage: &str, keys: Value) -> Value {
    json!({
        "obs_id": id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {
            "channel": "fixture",
            "id": "rules-fixture",
            "lineage": lineage
        }
    })
}

fn observation_with_ancestry(
    id: &str,
    surface: &str,
    status: &str,
    origin_source_ids: &[&str],
) -> Value {
    json!({
        "obs_id": id,
        "entity_type": "organisation",
        "surface": surface,
        "source": {
            "channel": "fixture",
            "id": format!("observation-{id}"),
            "ancestry": {
                "status": status,
                "origin_source_ids": origin_source_ids
            }
        }
    })
}

fn pair_evidence_with_ancestry(
    kind: &str,
    left: &str,
    right: &str,
    origin_source_ids: &[&str],
) -> Value {
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": {"rule": format!("fixture_{kind}")},
        "source": {
            "channel": "fixture",
            "id": format!("evidence-{kind}-{left}-{right}"),
            "ancestry": {
                "status": "known",
                "origin_source_ids": origin_source_ids
            }
        },
        "asserted_by": "script:rules-fixture"
    })
}

fn pair_evidence(kind: &str, left: &str, right: &str, lineage: &str) -> Value {
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": {"rule": format!("fixture_{kind}")},
        "source": {"channel": "fixture", "id": format!("evidence-{lineage}"), "lineage": lineage},
        "asserted_by": "script:rules-fixture"
    })
}

const V1: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  lei: {authoritative: true, single_valued: true}
  ticker: {authoritative: false, single_valued: false}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
  required_evidence_types: []
standalone:
  mint_when_authoritative_key: true
guards:
  max_cluster_surfaces: 50
"#;

#[test]
fn rules_init_template_round_trips_through_validator() {
    let temp = TempDir::new().expect("temp root");
    let path = temp.path().join("generated-rules.yaml");
    canon_command()
        .args([
            "entity",
            "session",
            "rules",
            "init",
            "--out",
            path.to_str().expect("rules path"),
        ])
        .assert()
        .success();
    let output = init_rules(Some(&path)).expect("rules init");
    assert_eq!(output.version, SESSION_RULES_VERSION);
    assert_eq!(output.path.as_deref(), path.to_str());
    let validated = validate_rules(&path).expect("template validates");
    assert_eq!(validated.version, SESSION_RULES_VERSION);
    assert_eq!(validated.max_cluster_surfaces, DEFAULT_MAX_CLUSTER_SURFACES);
    assert_eq!(validated.key_namespaces, vec!["cik", "lei", "ticker"]);
    canon_command()
        .args([
            "entity",
            "session",
            "rules",
            "validate",
            path.to_str().expect("rules path"),
        ])
        .assert()
        .success();
}

#[test]
fn rules_parse_namespace_policies_and_alternatives() {
    let temp = TempDir::new().expect("temp root");
    let path = rules_file(
        &temp,
        &V1.replace(
            "required_evidence_types: []",
            "required_evidence_types: [name_equivalent]\n  any_of:\n    - {required_evidence_types: [same_slot, name_equivalent], min_independent_origins: 1}",
        ),
    );
    let (rules, hash) = parse_rules(&path).expect("rules parse");
    assert!(!hash.is_empty());
    assert!(rules.key_rules["cik"].authoritative);
    assert!(rules.key_rules["cik"].single_valued);
    assert!(!rules.key_rules["ticker"].authoritative);
    assert!(!rules.key_rules["ticker"].single_valued);
    assert_eq!(rules.tier_b_alternatives.len(), 1);
    // Evidence-kind lists are BTreeSet-canonicalized so the rules hash does not
    // depend on the order in which equivalent YAML declarations are written.
    assert_eq!(
        rules.tier_b_alternatives[0].required_evidence_types,
        vec!["name_equivalent", "same_slot"]
    );
    assert_eq!(rules.tier_b_alternatives[0].min_independent_origins, 1);
}

#[test]
fn rules_refusals_name_each_planted_yaml_path_and_values() {
    let cases = [
        ("min_distinct_lineages: 0", "tier_b.min_distinct_lineages"),
        (
            "eligible_evidence_types: [unknown_kind]",
            "tier_b.eligible_evidence_types[0]",
        ),
        ("min_distinct_lineage: 2", "tier_b.min_distinct_lineage"),
    ];
    for (fragment, expected_path) in cases {
        let temp = TempDir::new().expect("temp root");
        let contents = V1.replace("min_distinct_lineages: 2", fragment);
        let path = rules_file(&temp, &contents);
        let refusal = parse_rules(&path).expect_err("invalid rules must refuse");
        assert_eq!(refusal.detail["yaml_path"], expected_path);
        assert!(refusal.detail.get("offending_value").is_some());
        assert!(
            refusal.detail.get("allowed_values").is_some() || expected_path.ends_with("lineage")
        );
    }
}

#[test]
fn legacy_authoritative_key_list_remains_accepted() {
    let temp = TempDir::new().expect("temp root");
    let path = rules_file(
        &temp,
        &V1.replace(
            "cik: {authoritative: true, single_valued: true}\n  lei: {authoritative: true, single_valued: true}\n  ticker: {authoritative: false, single_valued: false}",
            "authoritative: [cik, lei]",
        ),
    );
    let (rules, _) = parse_rules(&path).expect("legacy rule table");
    assert_eq!(rules.authoritative_keys, vec!["cik", "lei"]);
    assert!(rules.key_rules["cik"].single_valued);
}

#[test]
fn known_empty_ancestry_is_rejected_during_observation_ingest() {
    let temp = TempDir::new().expect("temp root");
    let registry = temp.path().join("registry");
    let session = temp.path().join("session");
    canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "ancestry validation",
            "--registry",
            registry.to_str().expect("registry path"),
            "--work-dir",
            session.to_str().expect("session path"),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
        ])
        .assert()
        .success();
    let observations = temp.path().join("observations.jsonl");
    fs::write(
        &observations,
        r#"{"obs_id":"bad-ancestry","entity_type":"organisation","surface":"Solo Corp","source":{"channel":"fixture","ancestry":{"status":"known","origin_source_ids":[]}}}
"#,
    )
    .expect("observations");
    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
        ])
        .assert()
        .code(1);
    let payload: Value = serde_json::from_slice(&output.get_output().stdout).expect("JSON output");
    assert_eq!(
        payload["rejected"][0]["code"],
        "missing_field:source.ancestry.origin_source_ids"
    );
}

#[test]
fn v1_rules_accept_a_lone_authoritative_key_but_review_a_ticker_only_pair() {
    let temp = TempDir::new().expect("temp root");
    let rules = rules_file(&temp, V1);
    let session = init_session(&temp, "key-and-ticker");
    let observations = write_jsonl(
        &temp,
        "key-and-ticker.jsonl",
        &[
            observation("solo", "Solo Corp", "origin-solo", json!({"cik": "555"})),
            observation("ticker-a", "Ticker A", "origin-a", json!({"ticker": "ABC"})),
            observation("ticker-b", "Ticker B", "origin-b", json!({"ticker": "ABC"})),
        ],
    );
    ingest(&session, &observations);
    let payload = solve(&session, &rules);
    let clusters = payload["clusters"].as_array().expect("clusters");
    let solo = clusters
        .iter()
        .find(|cluster| cluster["surfaces"] == json!(["Solo Corp"]))
        .expect("solo cluster");
    assert_eq!(solo["outcome"], "accepted");
    assert_eq!(solo["reason"], "single_keyed");
    assert_eq!(solo["tier"], "A");
    assert_eq!(payload["summary"]["review"], 1);
    assert_eq!(payload["summary"]["accepted_tier_a"], 1);
}

#[test]
fn required_evidence_type_keeps_otherwise_diverse_pair_in_review() {
    let temp = TempDir::new().expect("temp root");
    let rules = rules_file(
        &temp,
        &V1.replace(
            "required_evidence_types: []",
            "required_evidence_types: [name_equivalent]",
        ),
    );
    let session = init_session(&temp, "required-kind");
    let observations = write_jsonl(
        &temp,
        "required-kind-observations.jsonl",
        &[
            observation("left", "Left Corp", "origin-left", json!({})),
            observation("right", "Right Corp", "origin-right", json!({})),
        ],
    );
    ingest(&session, &observations);
    let evidence = write_jsonl(
        &temp,
        "required-kind-evidence.jsonl",
        &[
            pair_evidence("same_slot", "left", "right", "origin-left"),
            pair_evidence("attribute_agreement", "left", "right", "origin-right"),
        ],
    );
    canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            evidence.to_str().expect("evidence path"),
        ])
        .assert()
        .success();
    let payload = solve(&session, &rules);
    assert_eq!(payload["summary"]["accepted_tier_b"], 0);
    assert_eq!(payload["summary"]["review"], 1);
    assert_eq!(payload["review"][0]["missing"], "required_evidence_type");
}

#[test]
fn ancestry_origin_components_do_not_overcount_one_deal() {
    let temp = TempDir::new().expect("temp root");
    let rules = rules_file(&temp, V1);
    let session = init_session(&temp, "origin-components");
    let observations = write_jsonl(
        &temp,
        "origin-components-observations.jsonl",
        &[
            observation_with_ancestry("same-left", "Same Left", "known", &["deal:D1", "filing:F1"]),
            observation_with_ancestry(
                "same-right",
                "Same Right",
                "known",
                &["deal:D1", "filing:F2"],
            ),
            observation_with_ancestry("diff-left", "Diff Left", "known", &["deal:D2", "filing:F3"]),
            observation_with_ancestry(
                "diff-right",
                "Diff Right",
                "known",
                &["deal:D3", "filing:F4"],
            ),
        ],
    );
    ingest(&session, &observations);
    let evidence = write_jsonl(
        &temp,
        "origin-components-evidence.jsonl",
        &[
            pair_evidence_with_ancestry(
                "same_slot",
                "same-left",
                "same-right",
                &["deal:D1", "filing:F1"],
            ),
            pair_evidence_with_ancestry(
                "name_equivalent",
                "same-left",
                "same-right",
                &["deal:D1", "filing:F2"],
            ),
            pair_evidence_with_ancestry(
                "same_slot",
                "diff-left",
                "diff-right",
                &["deal:D2", "filing:F3"],
            ),
            pair_evidence_with_ancestry(
                "name_equivalent",
                "diff-left",
                "diff-right",
                &["deal:D3", "filing:F4"],
            ),
        ],
    );
    canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            evidence.to_str().expect("evidence path"),
        ])
        .assert()
        .success();
    let payload = solve(&session, &rules);
    assert_eq!(payload["summary"]["accepted_tier_b"], 1);
    assert_eq!(payload["summary"]["review"], 1);
}

#[test]
fn v1_size_guard_reports_an_oversized_chain_without_accepting_it() {
    let temp = TempDir::new().expect("temp root");
    let rules = rules_file(&temp, V1);
    let session = init_session(&temp, "oversized");
    let observations = (0..60)
        .map(|index| {
            observation(
                &format!("obs-{index:02}"),
                &format!("Surface {index:02}"),
                if index % 2 == 0 {
                    "origin-a"
                } else {
                    "origin-b"
                },
                json!({}),
            )
        })
        .collect::<Vec<_>>();
    let observation_path = write_jsonl(&temp, "oversized-observations.jsonl", &observations);
    ingest(&session, &observation_path);
    let mut evidence = Vec::new();
    for index in 0..59 {
        evidence.push(pair_evidence(
            "same_slot",
            &format!("obs-{index:02}"),
            &format!("obs-{:02}", index + 1),
            "edge-slot",
        ));
        evidence.push(pair_evidence(
            "name_equivalent",
            &format!("obs-{index:02}"),
            &format!("obs-{:02}", index + 1),
            "edge-name",
        ));
    }
    let evidence_path = write_jsonl(&temp, "oversized-evidence.jsonl", &evidence);
    canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().expect("session path"),
            evidence_path.to_str().expect("evidence path"),
        ])
        .assert()
        .success();
    let payload = solve(&session, &rules);
    assert_eq!(payload["summary"]["oversized_cluster"], 1);
    assert_eq!(payload["summary"]["accepted_tier_b"], 0);
    assert_eq!(payload["clusters"][0]["outcome"], "oversized_cluster");
    assert_eq!(
        payload["clusters"][0]["surfaces"]
            .as_array()
            .expect("oversized cluster surfaces")
            .len(),
        60
    );
}
