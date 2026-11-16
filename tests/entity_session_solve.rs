#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    entity::publication::open_current_stream_generation, entity::session::SESSION_SOLVE_STREAM_ID,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
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
    serde_json::from_slice(&output.get_output().stdout).expect("JSON stdout")
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
            "session solve acceptance",
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

fn observation(
    obs_id: &str,
    lineage: &str,
    surface: &str,
    cik: Option<&str>,
    lei: Option<&str>,
) -> String {
    observation_with_type("organisation", obs_id, lineage, surface, cik, lei)
}

fn observation_with_type(
    entity_type: &str,
    obs_id: &str,
    lineage: &str,
    surface: &str,
    cik: Option<&str>,
    lei: Option<&str>,
) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": entity_type,
        "surface": surface,
        "keys": {
            "cik": cik,
            "lei": lei
        },
        "source": {
            "channel": "file",
            "id": "fixture-observations",
            "lineage": lineage
        }
    }))
    .unwrap()
}

fn fixture_observations(include_conflict_rows: bool) -> Vec<String> {
    let mut rows = vec![
        observation("r1", "fundA", "Medline Borrower, LP", Some("111"), None),
        observation(
            "r2",
            "fundB",
            "Mozart Borrower LP (dba Medline)",
            Some("111"),
            None,
        ),
        observation(
            "r3",
            "fundA",
            "Acme Holdings Inc",
            None,
            Some("LEI0000000000000001"),
        ),
        observation(
            "r4",
            "fundB",
            "Acme Hldgs",
            None,
            Some("LEI0000000000000001"),
        ),
        observation("r5", "fundA", "Zeta Corp", Some("222"), None),
        observation(
            "r6",
            "fundB",
            "Zeta Corporation",
            None,
            Some("LEI0000000000000002"),
        ),
        observation(
            "r7",
            "fundC",
            "ZC Inc",
            Some("222"),
            Some("LEI0000000000000002"),
        ),
        observation("r8", "fundA", "Project Falcon Buyer, Inc.", None, None),
        observation(
            "r9",
            "fundB",
            "Falcon Parent Holdings (dba SkyWorks)",
            None,
            None,
        ),
        observation("solo", "fundD", "Solo Corp", Some("555"), None),
        observation("lone", "fundE", "Lone No Key", None, None),
    ];
    if include_conflict_rows {
        rows.extend([
            observation("r10", "fundA", "Apex LLC", Some("333"), None),
            observation("r11", "fundB", "Apex LLC", Some("444"), None),
        ]);
    }
    rows
}

fn write_observations(temp: &TempDir, label: &str, rows: &[String]) -> std::path::PathBuf {
    let path = temp.path().join(format!("{label}-observations.jsonl"));
    fs::write(&path, format!("{}\n", rows.join("\n"))).expect("observations");
    path
}

fn same_slot(left: &str, right: &str, lineage: Option<&str>) -> String {
    let mut value = json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {
            "rule": "same loan and role",
            "left_value": "loan-7:sponsor",
            "right_value": "loan-7:sponsor",
            "steps": ["join:loan_id", "join:role"]
        },
        "source": {
            "channel": "agent",
            "id": "fixture-agent"
        },
        "asserted_by": "agent:fixture"
    });
    if let Some(lineage) = lineage {
        value["source"]["lineage"] = Value::String(lineage.to_string());
    }
    serde_json::to_string(&value).unwrap()
}

fn name_equivalent(left: &str, right: &str, lineage: Option<&str>) -> String {
    let mut value = json!({
        "type": "name_equivalent",
        "left": left,
        "right": right,
        "basis": {
            "rule": "casefold+strip_punct+strip_legal_suffix",
            "left_value": "Project Falcon Buyer, Inc.",
            "right_value": "Falcon Parent Holdings (dba SkyWorks)",
            "steps": ["casefold", "strip_punctuation"]
        },
        "source": {
            "channel": "script",
            "id": "fixture-script"
        },
        "asserted_by": "script:fixture"
    });
    if let Some(lineage) = lineage {
        value["source"]["lineage"] = Value::String(lineage.to_string());
    }
    serde_json::to_string(&value).unwrap()
}

fn distinct(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "distinct",
        "left": left,
        "right": right,
        "basis": {"authority": "reviewed"},
        "source": {"channel": "human", "id": "review:fixture"},
        "asserted_by": "human:fixture"
    }))
    .unwrap()
}

fn related(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "related",
        "left": left,
        "right": right,
        "basis": {
            "relation": "affiliate_of",
            "subject": left
        },
        "source": {"channel": "agent", "id": "relation:fixture"},
        "asserted_by": "agent:fixture"
    }))
    .unwrap()
}

fn write_evidence(temp: &TempDir, label: &str, rows: &[String]) -> std::path::PathBuf {
    let path = temp.path().join(format!("{label}-evidence.jsonl"));
    fs::write(&path, format!("{}\n", rows.join("\n"))).expect("evidence");
    path
}

fn ingest(session: &Path, observations: &Path) {
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
    assert_eq!(
        json_stdout(&output)["rejected"].as_array().unwrap().len(),
        0
    );
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
    assert_eq!(
        json_stdout(&output)["rejected"].as_array().unwrap().len(),
        0
    );
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

fn published_solve_bytes(session: &Path) -> Vec<u8> {
    open_current_stream_generation(session, SESSION_SOLVE_STREAM_ID)
        .expect("solve generation")
        .read_logical_file("solve/solve.json")
        .expect("published solve artifact")
        .to_vec()
}

fn clusters_by_surfaces(payload: &Value) -> BTreeMap<String, Value> {
    payload["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cluster| {
            let mut surfaces = cluster["surfaces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|surface| surface.as_str().unwrap())
                .collect::<Vec<_>>();
            surfaces.sort();
            (surfaces.join("|"), cluster.clone())
        })
        .collect()
}

#[test]
fn session_solve_classifies_fixture_and_planted_negatives() {
    let temp = TempDir::new().expect("temp root");
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");
    let session = init_session(&temp, "positive");
    let observations = write_observations(&temp, "positive", &fixture_observations(true));
    ingest(&session, &observations);
    let evidence = write_evidence(
        &temp,
        "positive",
        &[
            same_slot("r8", "r9", None),
            name_equivalent("r8", "r9", None),
        ],
    );
    assert_evidence(&session, &evidence);
    let payload = solve(&session, &rules);
    let clusters = clusters_by_surfaces(&payload);
    assert_eq!(payload["version"], "canon.entity.session.solve.v1");
    assert_eq!(payload["summary"]["accepted_tier_a"], 4);
    assert_eq!(payload["summary"]["accepted_tier_b"], 1);
    assert_eq!(payload["summary"]["contradiction"], 1);
    assert_eq!(payload["summary"]["standalone"], 1);
    assert!(clusters["Medline Borrower, LP|Mozart Borrower LP (dba Medline)"]["tier"] == "A");
    assert_eq!(
        clusters["Falcon Parent Holdings (dba SkyWorks)|Project Falcon Buyer, Inc."]["tier"],
        "B"
    );
    assert_eq!(clusters["Apex LLC"]["outcome"], "contradiction");
    assert_eq!(clusters["Apex LLC"]["reason"], "key_conflict");
    assert_eq!(clusters["Solo Corp"]["reason"], "single_keyed");
    assert_eq!(clusters["Lone No Key"]["outcome"], "standalone");
    assert!(
        payload["derived_evidence"]
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["operator"]["operator_id"] == "same_key")
    );

    let review_session = init_session(&temp, "review");
    let review_observations = write_observations(
        &temp,
        "review",
        &[
            observation("r8", "fundA", "Project Falcon Buyer, Inc.", None, None),
            observation("r9", "fundB", "Falcon Parent Holdings", None, None),
        ],
    );
    ingest(&review_session, &review_observations);
    let review_evidence = write_evidence(&temp, "review", &[same_slot("r8", "r9", None)]);
    assert_evidence(&review_session, &review_evidence);
    let review = solve(&review_session, &rules);
    assert_eq!(review["summary"]["review"], 1);
    assert_eq!(review["summary"]["standalone"], 2);
    assert_eq!(review["review"][0]["missing"], "second_evidence_type");

    let same_origin_session = init_session(&temp, "same-origin");
    let same_origin_observations = write_observations(
        &temp,
        "same-origin",
        &[
            observation("r8", "fundA", "Falcon Buyer", None, None),
            observation("r9", "fundA", "Falcon Parent", None, None),
        ],
    );
    ingest(&same_origin_session, &same_origin_observations);
    let same_origin_evidence = write_evidence(
        &temp,
        "same-origin",
        &[
            same_slot("r8", "r9", None),
            name_equivalent("r8", "r9", None),
        ],
    );
    assert_evidence(&same_origin_session, &same_origin_evidence);
    let same_origin = solve(&same_origin_session, &rules);
    assert_eq!(same_origin["summary"]["review"], 1);
    assert_eq!(same_origin["summary"]["standalone"], 2);
    assert_eq!(same_origin["review"][0]["missing"], "second_lineage");

    let distinct_session = init_session(&temp, "distinct");
    let distinct_observations = write_observations(
        &temp,
        "distinct",
        &[
            observation("r1", "fundA", "Medline Borrower", Some("111"), None),
            observation("r2", "fundB", "Medline Alias", Some("111"), None),
        ],
    );
    ingest(&distinct_session, &distinct_observations);
    let distinct_evidence = write_evidence(&temp, "distinct", &[distinct("r1", "r2")]);
    assert_evidence(&distinct_session, &distinct_evidence);
    let distinct_payload = solve(&distinct_session, &rules);
    assert_eq!(distinct_payload["summary"]["contradiction"], 1);
    assert_eq!(distinct_payload["contradictions"][0]["reason"], "distinct");
    assert_eq!(distinct_payload["clusters"][0]["outcome"], "contradiction");

    let key_conflict_session = init_session(&temp, "key-conflict");
    let key_conflict_observations = write_observations(
        &temp,
        "key-conflict",
        &[
            observation("r1", "fundA", "Key One", Some("777"), None),
            observation("r2", "fundB", "Key Two", Some("888"), None),
        ],
    );
    ingest(&key_conflict_session, &key_conflict_observations);
    let key_conflict_evidence = write_evidence(
        &temp,
        "key-conflict",
        &[
            same_slot("r1", "r2", None),
            name_equivalent("r1", "r2", None),
        ],
    );
    assert_evidence(&key_conflict_session, &key_conflict_evidence);
    let key_conflict_payload = solve(&key_conflict_session, &rules);
    assert_eq!(key_conflict_payload["summary"]["contradiction"], 1);
    assert_eq!(
        key_conflict_payload["contradictions"][0]["reason"],
        "key_conflict"
    );

    let relation_session = init_session(&temp, "related");
    let relation_observations = write_observations(
        &temp,
        "related",
        &[
            observation("r1", "fundA", "First", None, None),
            observation("r2", "fundB", "Second", None, None),
        ],
    );
    ingest(&relation_session, &relation_observations);
    let relation_evidence = write_evidence(&temp, "related", &[related("r1", "r2")]);
    assert_evidence(&relation_session, &relation_evidence);
    let relation_payload = solve(&relation_session, &rules);
    assert_eq!(relation_payload["summary"]["relations"], 1);
    assert_eq!(relation_payload["summary"]["standalone"], 2);

    let mixed_key_session = init_session(&temp, "mixed-key-kinds");
    let mixed_key_observations = write_observations(
        &temp,
        "mixed-key-kinds",
        &[
            observation("r1", "fundA", "Organisation Key", Some("999"), None),
            observation_with_type("person", "r2", "fundB", "Person Key", Some("999"), None),
        ],
    );
    ingest(&mixed_key_session, &mixed_key_observations);
    let mixed_key_payload = solve(&mixed_key_session, &rules);
    assert_eq!(mixed_key_payload["summary"]["cluster_count"], 2);
    assert_eq!(mixed_key_payload["summary"]["accepted_tier_a"], 2);

    let cross_kind_session = init_session(&temp, "cross-kind-evidence");
    let cross_kind_observations = write_observations(
        &temp,
        "cross-kind-evidence",
        &[
            observation("r1", "fundA", "Organisation Surface", None, None),
            observation_with_type("person", "r2", "fundB", "Person Surface", None, None),
        ],
    );
    ingest(&cross_kind_session, &cross_kind_observations);
    let cross_kind_evidence =
        write_evidence(&temp, "cross-kind-evidence", &[same_slot("r1", "r2", None)]);
    assert_evidence(&cross_kind_session, &cross_kind_evidence);
    let output = canon_command()
        .args([
            "entity",
            "session",
            "solve",
            cross_kind_session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
            "--emit",
            "json",
        ])
        .assert()
        .code(2);
    run_and_log("cross-kind evidence", &output);
    let refusal = json_stdout(&output);
    assert_eq!(
        refusal["refusal"]["detail"]["reason"],
        "cross_entity_type_evidence"
    );
    assert!(open_current_stream_generation(&cross_kind_session, SESSION_SOLVE_STREAM_ID).is_err());
}

#[test]
fn session_solve_is_byte_stable_when_inputs_are_shuffled() {
    let temp = TempDir::new().expect("temp root");
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");
    let rows = fixture_observations(false);
    let first = init_session(&temp, "ordered");
    let first_observations = write_observations(&temp, "ordered", &rows);
    ingest(&first, &first_observations);
    let first_evidence = write_evidence(
        &temp,
        "ordered",
        &[
            same_slot("r8", "r9", None),
            name_equivalent("r8", "r9", None),
        ],
    );
    assert_evidence(&first, &first_evidence);
    let first_payload = solve(&first, &rules);

    let second = init_session(&temp, "shuffled");
    let mut shuffled_rows = rows.clone();
    shuffled_rows.reverse();
    let second_observations = write_observations(&temp, "shuffled", &shuffled_rows);
    ingest(&second, &second_observations);
    let second_evidence = write_evidence(
        &temp,
        "shuffled",
        &[
            name_equivalent("r8", "r9", None),
            same_slot("r8", "r9", None),
        ],
    );
    assert_evidence(&second, &second_evidence);
    let second_payload = solve(&second, &rules);
    assert_eq!(
        first_payload["inputs"], second_payload["inputs"],
        "canonical input identities must be order independent"
    );
    assert_eq!(
        first_payload["clusters"], second_payload["clusters"],
        "cluster decisions must be order independent"
    );
    assert_eq!(
        first_payload["derived_evidence"], second_payload["derived_evidence"],
        "derived same_key evidence must be order independent"
    );
    assert_eq!(
        published_solve_bytes(&first),
        published_solve_bytes(&second),
        "published solve bytes must be deterministic when inputs are shuffled"
    );
}

#[test]
fn session_solve_rejects_unknown_rule_path_without_writing() {
    let temp = TempDir::new().expect("temp root");
    let session = init_session(&temp, "bad-rules");
    let observations = write_observations(
        &temp,
        "bad-rules",
        &[observation("r1", "fundA", "Only Row", None, None)],
    );
    ingest(&session, &observations);
    let rules = temp.path().join("bad-rules.yaml");
    fs::write(
        &rules,
        "version: canon.entity.session.rules.v1\nkeys:\n  authoritative: [cik, lei]\ntier_a:\n  evidence_types: [same_key, asserted_alias]\ntier_b:\n  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]\n  min_distinct_evidence_types: 2\n  min_distinct_lineages: 2\nstandalone:\n  mint_when_authoritative_key: true\nunknown:\n  value: true\n",
    )
    .expect("bad rules");
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
        .code(2);
    run_and_log("bad rules", &output);
    let payload = json_stdout(&output);
    assert!(
        payload["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown")
    );
    assert!(open_current_stream_generation(&session, SESSION_SOLVE_STREAM_ID).is_err());
}
