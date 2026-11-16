#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
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

fn goldset(company: &str, cik: &str, accession: &str, filed_date: &str) -> String {
    serde_json::to_string(&json!({
        "schema_id": "cmdrvl.bdc_schedule_gold_set",
        "schema_version": "1.0",
        "source": {
            "company": company,
            "ticker": company.to_ascii_lowercase().replace(' ', "-"),
            "cik": cik,
            "form": "10-Q",
            "filed_date": filed_date,
            "as_of_date": filed_date,
            "accession_number": accession
        },
        "rows": []
    }))
    .expect("goldset JSON")
}

fn write_goldsets(temp: &TempDir) -> PathBuf {
    let gold_dir = temp.path().join("gold_sets");
    fs::create_dir_all(&gold_dir).expect("gold-set directory");
    fs::write(
        gold_dir.join("alpha_2024-12-31.goldset.v0.json"),
        goldset("Alpha Lending", "1001", "0001001-24-000001", "2024-12-31"),
    )
    .expect("alpha earliest");
    fs::write(
        gold_dir.join("alpha_2025-06-30.goldset.v0.json"),
        goldset("Alpha Lending", "1001", "0001001-25-000001", "2025-06-30"),
    )
    .expect("alpha later");
    fs::write(
        gold_dir.join("bravo_2025-06-30.goldset.v0.json"),
        goldset("Bravo Capital", "1002", "0001002-25-000001", "2025-06-30"),
    )
    .expect("bravo");
    fs::write(
        gold_dir.join("charlie_2025-06-30.goldset.v0.json"),
        goldset("Charlie Finance", "1003", "0001003-25-000001", "2025-06-30"),
    )
    .expect("charlie");
    gold_dir
}

fn run_converter(gold_dir: &Path, ciks: &Path, pick: &str, out: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/entity_sessions/bdc_goldsets_to_session.py");
    let output = ProcessCommand::new("python3")
        .args([
            script.to_str().expect("converter path"),
            "--mode",
            "filers",
            "--gold-dir",
            gold_dir.to_str().expect("gold path"),
            "--ciks-file",
            ciks.to_str().expect("CIK path"),
            "--pick",
            pick,
            "--out",
            out.to_str().expect("output path"),
        ])
        .output()
        .expect("run converter");
    assert!(
        output.status.success(),
        "converter failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_ciks(temp: &TempDir, name: &str, ciks: &[&str]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", ciks.join("\n"))).expect("CIK selection");
    path
}

fn init_session(temp: &TempDir, name: &str, registry: &Path) -> PathBuf {
    let session = temp.path().join(name);
    let mut command = canon();
    command.args([
        "entity",
        "session",
        "init",
        "--goal",
        "BDC skeleton fixture",
        "--registry",
        registry.to_str().expect("registry path"),
        "--work-dir",
        session.to_str().expect("session path"),
        "--entity-kind",
        "organisation",
        "--emit",
        "json",
    ]);
    if !registry.join("registry.json").exists() {
        command.args([
            "--registry-id",
            "bdc-fixture",
            "--prefix",
            "STG",
            "--zero-pad",
            "3",
        ]);
    }
    command.assert().success();
    session
}

fn ingest(session: &Path, observations: &Path) {
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().expect("session path"),
            observations.to_str().expect("observations path"),
            "--emit",
            "json",
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
            "--emit",
            "json",
        ])
        .assert()
        .success();
    json_stdout(&output)
}

fn lookup(registry: &Path, input: &Path, column: &str) -> Value {
    let output = canon()
        .args([
            input.to_str().expect("input path"),
            "--registry",
            registry.to_str().expect("registry path"),
            "--column",
            column,
            "--explicit",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    json_stdout(&output)
}

fn copy_dir(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("copy target");
    for entry in fs::read_dir(source).expect("registry directory") {
        let entry = entry.expect("registry entry");
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir(&source_path, &target_path);
        } else {
            fs::copy(source_path, target_path).expect("copy registry file");
        }
    }
}

#[test]
fn session_skeleton_runs_public_commands_with_fixture_and_planted_negatives() {
    let temp = TempDir::new().expect("temp root");
    let gold_dir = write_goldsets(&temp);
    let day_one_ciks = write_ciks(&temp, "day-one-ciks.txt", &["1001", "1002"]);
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).expect("rules");

    let day_one_obs = temp.path().join("day-one.jsonl");
    run_converter(&gold_dir, &day_one_ciks, "earliest", &day_one_obs);
    let day_one_lines = fs::read_to_string(&day_one_obs).expect("day-one observations");
    assert_eq!(day_one_lines.lines().count(), 2);
    assert!(day_one_lines.contains("Alpha Lending"));
    assert!(day_one_lines.contains("Bravo Capital"));
    assert!(day_one_lines.contains("edgar:cik:1001"));
    assert!(day_one_lines.contains("\"context\""));

    let registry = temp.path().join("registry");
    let day_one = init_session(&temp, "day-one-session", &registry);
    ingest(&day_one, &day_one_obs);
    let day_one_solve = solve(&day_one, &rules);
    assert_eq!(day_one_solve["summary"]["accepted_tier_a"], 2);
    let commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            day_one.to_str().expect("session path"),
            "--next-version",
            "0.1.0",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let commit_json = json_stdout(&commit);
    assert_eq!(commit_json["outcome"], "committed");
    assert_eq!(commit_json["minted"].as_array().expect("minted").len(), 2);
    assert_eq!(commit_json["registry"]["entry_count_after"], 4);
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

    let lookup_input = temp.path().join("day-one-lookup.jsonl");
    fs::write(
        &lookup_input,
        "{\"company\":\"Alpha Lending\",\"cik\":\"1001\"}\n{\"company\":\"Bravo Capital\",\"cik\":\"1002\"}\n",
    )
    .expect("lookup input");
    let names = lookup(&registry, &lookup_input, "company");
    let ciks = lookup(&registry, &lookup_input, "cik");
    assert_eq!(names["summary"]["resolved"], 2);
    assert_eq!(ciks["summary"]["resolved"], 2);
    assert_eq!(
        names["mappings"][0]["canonical_id"],
        ciks["mappings"][0]["canonical_id"]
    );

    let before_day_two = temp.path().join("registry-before-day-two");
    copy_dir(&registry, &before_day_two);
    let day_two_ciks = write_ciks(&temp, "day-two-ciks.txt", &["1001"]);
    let day_two_later = temp.path().join("day-two-later.jsonl");
    run_converter(&gold_dir, &day_two_ciks, "later", &day_two_later);
    let other_ciks = write_ciks(&temp, "other-ciks.txt", &["1003"]);
    let day_two_other = temp.path().join("day-two-other.jsonl");
    run_converter(&gold_dir, &other_ciks, "all", &day_two_other);
    let day_two_obs = temp.path().join("day-two.jsonl");
    fs::write(
        &day_two_obs,
        format!(
            "{}{}",
            fs::read_to_string(&day_two_later).expect("later observations"),
            fs::read_to_string(&day_two_other).expect("other observations")
        ),
    )
    .expect("day-two observations");

    let day_two = init_session(&temp, "day-two-session", &registry);
    ingest(&day_two, &day_two_obs);
    let day_two_solve = solve(&day_two, &rules);
    assert_eq!(day_two_solve["summary"]["resolved_existing"], 1);
    assert_eq!(day_two_solve["summary"]["accepted_tier_a"], 2);
    let day_two_commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            day_two.to_str().expect("session path"),
            "--next-version",
            "0.2.0",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let day_two_commit_json = json_stdout(&day_two_commit);
    assert_eq!(day_two_commit_json["outcome"], "committed");
    assert_eq!(
        day_two_commit_json["minted"]
            .as_array()
            .expect("minted")
            .len(),
        1
    );
    let attached = day_two_commit_json["attached"]
        .as_array()
        .expect("attached");
    assert_eq!(attached.len(), 1);
    assert!(attached.iter().all(|row| row["aliases"] == json!([])));

    let diff = canon()
        .args([
            "registry",
            "diff",
            "--old",
            before_day_two.to_str().expect("old registry"),
            "--new",
            registry.to_str().expect("new registry"),
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let diff_json = json_stdout(&diff);
    assert_eq!(diff_json["summary"]["removed"], 0);
    assert_eq!(diff_json["summary"]["changed"], 0);
    assert!(diff_json["summary"]["added"].as_u64().expect("added") > 0);

    let all_lookup_input = temp.path().join("all-lookup.jsonl");
    fs::write(
        &all_lookup_input,
        "{\"company\":\"Alpha Lending\",\"cik\":\"1001\"}\n{\"company\":\"Bravo Capital\",\"cik\":\"1002\"}\n{\"company\":\"Charlie Finance\",\"cik\":\"1003\"}\n",
    )
    .expect("all lookup input");
    let all_names = lookup(&registry, &all_lookup_input, "company");
    let all_cik_values = lookup(&registry, &all_lookup_input, "cik");
    assert_eq!(all_names["summary"]["resolved"], 3);
    assert_eq!(all_cik_values["summary"]["resolved"], 3);

    let second_source_session = init_session(&temp, "second-source-session", &registry);
    let second_source_obs = temp.path().join("second-source.jsonl");
    fs::write(
        &second_source_obs,
        "{\"obs_id\":\"registrant-1\",\"entity_type\":\"organisation\",\"surface\":\"Alpha Lending Holdings\",\"keys\":{\"cik\":\"1001\"},\"source\":{\"channel\":\"file\",\"file\":\"second-source.jsonl\",\"lineage\":\"edgar:registrant\"}}\n",
    )
    .expect("second-source observation");
    ingest(&second_source_session, &second_source_obs);
    let second_source_solve = solve(&second_source_session, &rules);
    assert_eq!(second_source_solve["summary"]["resolved_existing"], 1);
    let second_source_commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            second_source_session.to_str().expect("session path"),
            "--next-version",
            "0.3.0",
            "--emit",
            "json",
        ])
        .assert()
        .success();
    let second_source_json = json_stdout(&second_source_commit);
    assert_eq!(second_source_json["minted"], json!([]));
    assert_eq!(
        second_source_json["attached"]
            .as_array()
            .expect("attached")
            .len(),
        1
    );
    let second_source_lookup = temp.path().join("second-source-lookup.jsonl");
    fs::write(
        &second_source_lookup,
        "{\"name\":\"Alpha Lending Holdings\"}\n",
    )
    .expect("second-source lookup");
    let attached_lookup = lookup(&registry, &second_source_lookup, "name");
    assert_eq!(attached_lookup["summary"]["resolved"], 1);

    let conflict_registry = temp.path().join("conflict-registry");
    copy_dir(&registry, &conflict_registry);
    let conflict_session = init_session(&temp, "conflict-session", &conflict_registry);
    let conflict_obs = temp.path().join("conflict.jsonl");
    fs::write(
        &conflict_obs,
        "{\"obs_id\":\"wrong-key\",\"entity_type\":\"organisation\",\"surface\":\"Alpha Lending\",\"keys\":{\"cik\":\"9999\"},\"source\":{\"channel\":\"file\",\"file\":\"conflict.jsonl\"}}\n",
    )
    .expect("conflict observation");
    ingest(&conflict_session, &conflict_obs);
    solve(&conflict_session, &rules);
    let conflict_commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            conflict_session.to_str().expect("session path"),
            "--next-version",
            "0.3.0",
            "--emit",
            "json",
        ])
        .assert()
        .code(1);
    let conflict_json = json_stdout(&conflict_commit);
    assert_eq!(conflict_json["outcome"], "nothing_to_commit");
    assert!(
        conflict_json["skipped"]
            .as_array()
            .expect("skipped")
            .iter()
            .any(|row| row["reason"] == "key_conflict")
    );

    let standalone_registry = temp.path().join("standalone-registry");
    copy_dir(&registry, &standalone_registry);
    let standalone_session = init_session(&temp, "standalone-session", &standalone_registry);
    let standalone_obs = temp.path().join("standalone.jsonl");
    fs::write(
        &standalone_obs,
        "{\"obs_id\":\"unseen\",\"entity_type\":\"organisation\",\"surface\":\"Unseen Invented BDC\",\"keys\":{\"cik\":null},\"source\":{\"channel\":\"file\",\"file\":\"standalone.jsonl\"}}\n",
    )
    .expect("standalone observation");
    ingest(&standalone_session, &standalone_obs);
    solve(&standalone_session, &rules);
    let standalone_commit = canon()
        .args([
            "entity",
            "session",
            "commit",
            standalone_session.to_str().expect("session path"),
            "--next-version",
            "0.3.0",
            "--emit",
            "json",
        ])
        .assert()
        .code(1);
    let standalone_json = json_stdout(&standalone_commit);
    assert_eq!(standalone_json["outcome"], "nothing_to_commit");
    assert!(
        standalone_json["skipped"]
            .as_array()
            .expect("skipped")
            .iter()
            .any(|row| row["reason"] == "standalone")
    );

    let standalone_lookup = temp.path().join("standalone-lookup.jsonl");
    fs::write(&standalone_lookup, "{\"name\":\"Unseen Invented BDC\"}\n")
        .expect("standalone lookup input");
    let unresolved = canon()
        .args([
            standalone_lookup.to_str().expect("lookup path"),
            "--registry",
            standalone_registry.to_str().expect("registry path"),
            "--column",
            "name",
            "--emit",
            "json",
        ])
        .assert()
        .code(1);
    let unresolved_json = json_stdout(&unresolved);
    assert_eq!(unresolved_json["summary"]["unresolved"], 1);
}
