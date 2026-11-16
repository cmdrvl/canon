#![forbid(unsafe_code)]

//! bd-vdcw.27: key namespaces are validated at session ingest. A key that fails
//! its namespace check is dropped from the observation and reported as a key
//! warning; the observation itself is still ingested.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
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
"#;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn run_and_log(label: &str, output: &assert_cmd::assert::Assert) {
    eprintln!(
        "[{label}] exit={} stdout={} stderr={}",
        output.get_output().status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.get_output().stdout),
        String::from_utf8_lossy(&output.get_output().stderr),
    );
}

fn json_stdout(output: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&output.get_output().stdout).expect("JSON stdout envelope")
}

/// ISO 17442 check digits computed independently of canon: MOD 97-10 over the
/// 18-character body followed by "00".
fn lei_with_check(body: &str) -> String {
    assert_eq!(body.len(), 18);
    let mut remainder = 0u32;
    for ch in body.chars().chain("00".chars()) {
        let value = ch.to_digit(36).expect("alphanumeric");
        remainder = if value >= 10 {
            (remainder * 100 + value) % 97
        } else {
            (remainder * 10 + value) % 97
        };
    }
    format!("{body}{:02}", 98 - remainder)
}

/// The same LEI with its last two characters (the check digits) transposed.
fn transposed(lei: &str) -> String {
    let mut chars: Vec<char> = lei.chars().collect();
    let last = chars.len() - 1;
    chars.swap(last - 1, last);
    let swapped: String = chars.into_iter().collect();
    assert_ne!(
        swapped, lei,
        "check digits must differ for a real transposition"
    );
    swapped
}

fn init_session(temp: &TempDir, label: &str) -> std::path::PathBuf {
    let registry = temp.path().join(format!("{label}-registry"));
    let session = temp.path().join(format!("{label}-session"));
    let output = canon_command()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "key validation",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            session.to_str().unwrap(),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "4",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session init", &output);
    session
}

fn observation(obs_id: &str, lineage: &str, surface: &str, keys: Value) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "file", "id": "keys-fixture", "lineage": lineage}
    }))
    .unwrap()
}

fn write_lines(temp: &TempDir, name: &str, rows: &[String]) -> std::path::PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", rows.join("\n"))).expect("jsonl");
    path
}

fn ingest(session: &Path, observations: &Path) -> Value {
    let output = canon_command()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session ingest", &output);
    let value = json_stdout(&output);
    assert_eq!(value["rejected"].as_array().unwrap().len(), 0);
    value
}

fn assert_evidence(session: &Path, evidence: &Path) {
    let output = canon_command()
        .args([
            "entity",
            "session",
            "assert",
            session.to_str().unwrap(),
            evidence.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session assert", &output);
}

fn solve(session: &Path, rules: &Path) -> Value {
    let output = canon_command()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    run_and_log("session solve", &output);
    json_stdout(&output)
}

fn evidence_row(kind: &str, left: &str, right: &str, channel: &str, basis: Value) -> String {
    serde_json::to_string(&json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": basis,
        "source": {"channel": channel, "id": format!("{kind}-fixture")},
        "asserted_by": format!("{channel}:fixture")
    }))
    .unwrap()
}

fn same_slot(left: &str, right: &str) -> String {
    evidence_row(
        "same_slot",
        left,
        right,
        "agent",
        json!({
            "rule": "same loan and role",
            "left_value": "loan-7:sponsor",
            "right_value": "loan-7:sponsor",
            "steps": ["join:loan_id", "join:role"]
        }),
    )
}

fn name_equivalent(left: &str, right: &str) -> String {
    evidence_row(
        "name_equivalent",
        left,
        right,
        "script",
        json!({
            "rule": "casefold+strip_punct",
            "left_value": "Orion Capital LP",
            "right_value": "Orion Capital, L.P.",
            "steps": ["casefold", "strip_punctuation"]
        }),
    )
}

fn rules_file(temp: &TempDir) -> std::path::PathBuf {
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");
    rules
}

fn cluster_with_surface<'a>(payload: &'a Value, surface: &str) -> &'a Value {
    payload["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cluster| {
            cluster["surfaces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|candidate| candidate == surface)
        })
        .unwrap_or_else(|| panic!("no cluster holds surface {surface}"))
}

#[test]
fn padded_and_unpadded_cik_form_one_tier_a_cluster() {
    let temp = TempDir::new().unwrap();
    let rules = rules_file(&temp);
    let session = init_session(&temp, "cik");
    let rows = write_lines(
        &temp,
        "cik.jsonl",
        &[
            observation(
                "a",
                "fundA",
                "Harbor Lending LP",
                json!({"cik": "0001287750"}),
            ),
            observation(
                "b",
                "fundB",
                "Harbor Lend. Partners",
                json!({"cik": "1287750"}),
            ),
        ],
    );
    let ingested = ingest(&session, &rows);
    assert_eq!(ingested["accepted"], 2);
    assert_eq!(ingested["key_warnings"].as_array().unwrap().len(), 0);

    let payload = solve(&session, &rules);
    assert_eq!(payload["summary"]["accepted_tier_a"], 1);
    assert_eq!(
        cluster_with_surface(&payload, "Harbor Lending LP")["tier"],
        "A"
    );
    assert_eq!(
        cluster_with_surface(&payload, "Harbor Lend. Partners")["tier"],
        "A"
    );
}

#[test]
fn valid_computed_lei_links_and_transposed_lei_is_dropped_and_reported() {
    let temp = TempDir::new().unwrap();
    let rules = rules_file(&temp);
    let lei = lei_with_check("CANONFIXT0000000AA");
    let bad = transposed(&lei);

    // Two rows carry the valid LEI: accepted, tier A.
    let good_session = init_session(&temp, "lei-good");
    let good_rows = write_lines(
        &temp,
        "lei-good.jsonl",
        &[
            observation("a", "fundA", "Orion Capital LP", json!({"lei": lei})),
            observation("b", "fundB", "Orion Cap Partners", json!({"lei": lei})),
        ],
    );
    let ingested = ingest(&good_session, &good_rows);
    assert_eq!(ingested["key_warnings"].as_array().unwrap().len(), 0);
    let payload = solve(&good_session, &rules);
    assert_eq!(payload["summary"]["accepted_tier_a"], 1);

    // One row carries the transposed LEI: dropped with check_digit, so the pair
    // neither links nor vetoes.
    let bad_session = init_session(&temp, "lei-bad");
    let bad_rows = write_lines(
        &temp,
        "lei-bad.jsonl",
        &[
            observation("a", "fundA", "Orion Capital LP", json!({"lei": lei})),
            observation("b", "fundB", "Orion Cap Partners", json!({"lei": bad})),
        ],
    );
    let ingested = ingest(&bad_session, &bad_rows);
    assert_eq!(ingested["accepted"], 2, "the observation is still ingested");
    let warnings = ingested["key_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["obs_id"], "b");
    assert_eq!(warnings[0]["namespace"], "lei");
    assert_eq!(warnings[0]["value"], Value::String(bad.clone()));
    assert_eq!(warnings[0]["reason"], "check_digit");
    assert_eq!(warnings[0]["line"], 2);
    assert!(warnings[0]["fix"].as_str().unwrap().contains("ISO 17442"));

    let payload = solve(&bad_session, &rules);
    // Row a still carries the valid LEI, so it mints alone (single_keyed counts
    // in accepted_tier_a); row b lost its key and stands alone. They are not
    // joined and there is no contradiction.
    assert_eq!(payload["summary"]["accepted_tier_a"], 1);
    assert_eq!(payload["summary"]["contradiction"], 0);
    let keyed = cluster_with_surface(&payload, "Orion Capital LP");
    assert_eq!(keyed["reason"], "single_keyed");
    assert_eq!(keyed["surfaces"].as_array().unwrap().len(), 1);
    let stripped = cluster_with_surface(&payload, "Orion Cap Partners");
    assert_eq!(stripped["outcome"], "standalone");
    assert_eq!(stripped["surfaces"].as_array().unwrap().len(), 1);

    // Re-ingesting the same file is idempotent: the stored row has no bad key.
    let again = ingest(&bad_session, &bad_rows);
    assert_eq!(again["accepted"], 0);
    assert_eq!(again["already_present"], 2);
}

#[test]
fn mistyped_lei_on_one_spelling_is_not_a_key_conflict() {
    let temp = TempDir::new().unwrap();
    let rules = rules_file(&temp);
    let lei = lei_with_check("CANONFIXT0000000AA");
    let bad = transposed(&lei);
    let session = init_session(&temp, "spelling");
    let rows = write_lines(
        &temp,
        "spelling.jsonl",
        &[
            observation("a", "fundA", "Orion Capital LP", json!({"lei": lei})),
            observation("b", "fundB", "Orion Capital, L.P.", json!({"lei": bad})),
        ],
    );
    let ingested = ingest(&session, &rows);
    assert_eq!(ingested["key_warnings"].as_array().unwrap().len(), 1);
    let evidence = write_lines(
        &temp,
        "spelling-evidence.jsonl",
        &[same_slot("a", "b"), name_equivalent("a", "b")],
    );
    assert_evidence(&session, &evidence);

    let payload = solve(&session, &rules);
    // A naive same-string key rule would see two different LEIs and report a
    // key_conflict contradiction; the bad key is dropped, so tier B decides.
    assert_eq!(payload["summary"]["contradiction"], 0);
    assert_eq!(payload["summary"]["accepted_tier_b"], 1);
    assert_eq!(
        cluster_with_surface(&payload, "Orion Capital LP")["tier"],
        "B"
    );
}

#[test]
fn placeholder_keys_do_not_merge_different_borrowers() {
    let temp = TempDir::new().unwrap();
    let rules = rules_file(&temp);
    let session = init_session(&temp, "placeholder");
    let rows = write_lines(
        &temp,
        "placeholder.jsonl",
        &[
            observation("a", "fundA", "Borrower One LP", json!({"cik": "0"})),
            observation("b", "fundB", "Borrower Two LLC", json!({"cik": "0"})),
            observation("c", "fundA", "Borrower Three Inc", json!({"cik": "N/A"})),
            observation(
                "d",
                "fundB",
                "Borrower Four Corp",
                json!({"cik": "0000000000"}),
            ),
        ],
    );
    let ingested = ingest(&session, &rows);
    assert_eq!(ingested["accepted"], 4);
    let warnings = ingested["key_warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 4);
    assert!(warnings.iter().all(|w| w["reason"] == "placeholder"));
    assert!(warnings.iter().all(|w| w["namespace"] == "cik"));

    let payload = solve(&session, &rules);
    assert_eq!(payload["summary"]["accepted_tier_a"], 0);
    assert_eq!(payload["summary"]["contradiction"], 0);
    for surface in [
        "Borrower One LP",
        "Borrower Two LLC",
        "Borrower Three Inc",
        "Borrower Four Corp",
    ] {
        let cluster = cluster_with_surface(&payload, surface);
        assert_eq!(
            cluster["surfaces"].as_array().unwrap().len(),
            1,
            "{surface} must stand alone"
        );
    }
}

#[test]
fn malformed_ciks_and_unknown_namespaces_are_handled_per_row() {
    let temp = TempDir::new().unwrap();
    let session = init_session(&temp, "mixed");
    let rows = write_lines(
        &temp,
        "mixed.jsonl",
        &[observation(
            "a",
            "fundA",
            "Mixed Co",
            json!({"cik": "12AB", "ticker": "brk.b", "duns": "  123-45  ", "isin": "US0378331006"}),
        )],
    );
    let ingested = ingest(&session, &rows);
    assert_eq!(ingested["accepted"], 1);
    let warnings = ingested["key_warnings"].as_array().unwrap();
    let reasons: Vec<(String, String)> = warnings
        .iter()
        .map(|w| {
            (
                w["namespace"].as_str().unwrap().to_string(),
                w["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("cik".to_string(), "format".to_string()),
            ("isin".to_string(), "check_digit".to_string()),
        ]
    );
}
