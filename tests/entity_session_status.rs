//! bd-vdcw.5: `canon entity session status` answers "where does this session
//! stand" in one call. Every assertion reads the JSON; the summary text is
//! checked only after the JSON fields are.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
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

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn log_step(label: &str, output: &std::process::Output) {
    eprintln!(
        "[{label}] exit={} stdout={} stderr={}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn run_ok(label: &str, args: &[&str]) -> Value {
    let output = canon().args(args).output().expect("canon runs");
    log_step(label, &output);
    assert_eq!(output.status.code(), Some(0), "{label} must exit 0");
    serde_json::from_slice(&output.stdout).expect("JSON stdout")
}

fn init_session(temp: &TempDir, label: &str) -> (PathBuf, PathBuf) {
    let registry = temp.path().join(format!("{label}-registry"));
    let session = temp.path().join(format!("{label}-session"));
    run_ok(
        "session init",
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "session status acceptance",
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
        ],
    );
    (session, registry)
}

fn observation(obs_id: &str, lineage: &str, surface: &str, cik: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "file", "id": "status-fixture", "lineage": lineage}
    }))
    .unwrap()
}

fn lei_observation(obs_id: &str, lineage: &str, surface: &str, lei: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"lei": lei},
        "source": {"channel": "file", "id": "status-fixture", "lineage": lineage}
    }))
    .unwrap()
}

/// Thirteen rows: Medline pair (A), Acme pair via LEI (A), Zeta trio (A),
/// Solo (A, single keyed), Apex with two CIKs (key_conflict), a Falcon pair
/// that gets one evidence type (review) and a keyless Lone Corp (standalone).
fn fixture_rows() -> Vec<String> {
    vec![
        observation("r1", "fundA", "Medline Borrower, LP", Some("111")),
        observation(
            "r2",
            "fundB",
            "Mozart Borrower LP (dba Medline)",
            Some("111"),
        ),
        lei_observation("r3", "fundA", "Acme Holdings Inc", "CANONFIXT0000000AA49"),
        lei_observation("r4", "fundB", "Acme Hldgs", "CANONFIXT0000000AA49"),
        observation("r5", "fundA", "Zeta Corp", Some("222")),
        lei_observation("r6", "fundB", "Zeta Corporation", "CANONFIXT0000000BB37"),
        serde_json::to_string(&json!({
            "obs_id": "r7",
            "entity_type": "organisation",
            "surface": "ZC Inc",
            "keys": {"cik": "222", "lei": "CANONFIXT0000000BB37"},
            "source": {"channel": "file", "id": "status-fixture", "lineage": "fundC"}
        }))
        .unwrap(),
        observation("r8", "fundA", "Project Falcon Buyer, Inc.", None),
        observation("r9", "fundB", "Falcon Parent Holdings (dba SkyWorks)", None),
        observation("solo", "fundD", "Solo Corp", Some("555")),
        observation("lone", "fundE", "Lone No Key", None),
        observation("r10", "fundA", "Apex LLC", Some("333")),
        observation("r11", "fundB", "Apex LLC", Some("444")),
    ]
}

fn write_lines(temp: &TempDir, name: &str, rows: &[String]) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, format!("{}\n", rows.join("\n"))).expect("fixture file");
    path
}

fn same_slot(left: &str, right: &str) -> String {
    serde_json::to_string(&json!({
        "type": "same_slot",
        "left": left,
        "right": right,
        "basis": {
            "rule": "same loan and role",
            "left_value": "loan-7:sponsor",
            "right_value": "loan-7:sponsor",
            "steps": ["join:loan_id", "join:role"]
        },
        "source": {"channel": "agent", "id": "fixture-agent"},
        "asserted_by": "agent:fixture"
    }))
    .unwrap()
}

fn ingest(session: &Path, file: &Path) -> Value {
    run_ok(
        "session ingest",
        &[
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            file.to_str().unwrap(),
        ],
    )
}

fn solve(session: &Path, rules: &Path) -> Value {
    run_ok(
        "session solve",
        &[
            "entity",
            "session",
            "solve",
            session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
        ],
    )
}

type TreeSnapshot = BTreeMap<String, (Vec<u8>, Option<SystemTime>)>;

fn snapshot_tree(root: &Path) -> TreeSnapshot {
    fn walk(root: &Path, path: &Path, out: &mut TreeSnapshot) {
        let mut entries = fs::read_dir(path)
            .expect("read_dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("dir entries");
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let child = entry.path();
            let relative = child
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if child.is_dir() {
                out.insert(format!("{relative}/"), (Vec::new(), None));
                walk(root, &child, out);
            } else {
                let modified = fs::metadata(&child).and_then(|meta| meta.modified()).ok();
                out.insert(relative, (fs::read(&child).expect("read file"), modified));
            }
        }
    }
    let mut out = TreeSnapshot::new();
    walk(root, root, &mut out);
    out
}

/// Run status and prove it wrote nothing anywhere under the test root: the
/// listing, every file's bytes and every mtime are identical afterwards.
fn status(temp: &TempDir, label: &str, session: &Path, extra: &[&str]) -> Value {
    let before = snapshot_tree(temp.path());
    let mut args = vec!["entity", "session", "status", session.to_str().unwrap()];
    args.extend_from_slice(extra);
    let value = run_ok(label, &args);
    assert_eq!(
        snapshot_tree(temp.path()),
        before,
        "{label}: status must not create, change or touch any file"
    );
    value
}

fn reasons(status: &Value) -> Vec<String> {
    status["solve"]["stale_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reason| reason.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn status_tracks_the_loop_from_init_to_commit() {
    let temp = TempDir::new().unwrap();
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    let (session, registry) = init_session(&temp, "loop");
    let session_arg = session.to_str().unwrap();

    // init: nothing yet. A session with no solve is a status, not a refusal.
    let fresh = status(&temp, "status after init", &session, &[]);
    assert_eq!(fresh["version"], "canon.entity.session.status.v1");
    assert_eq!(fresh["session"]["goal"], "session status acceptance");
    assert_eq!(fresh["session"]["registry"]["id"], "organisations");
    assert_eq!(fresh["log"]["observations"], 0);
    assert_eq!(fresh["solve"]["present"], false);
    assert_eq!(fresh["solve"]["stale"], false);
    assert_eq!(fresh["commit"]["present"], false);
    assert_eq!(fresh["top_unresolved"].as_array().unwrap().len(), 0);
    assert!(
        fresh["next_command"]
            .as_str()
            .unwrap()
            .starts_with(&format!("canon entity session ingest {session_arg}"))
    );

    // ingest + assert: still no solve, and the next step is to solve.
    ingest(&session, &write_lines(&temp, "rows.jsonl", &fixture_rows()));
    let evidence = write_lines(&temp, "evidence.jsonl", &[same_slot("r8", "r9")]);
    run_ok(
        "session assert",
        &[
            "entity",
            "session",
            "assert",
            session_arg,
            evidence.to_str().unwrap(),
        ],
    );
    let logged = status(&temp, "status before solve", &session, &[]);
    assert_eq!(logged["log"]["observations"], 13);
    assert_eq!(logged["log"]["observation_shards"], 1);
    assert_eq!(logged["log"]["evidence"]["total"], 1);
    assert_eq!(logged["log"]["evidence"]["by_type"]["same_slot"], 1);
    assert_eq!(logged["solve"]["present"], false);
    assert_eq!(logged["solve"]["stale"], false);
    assert!(logged["solve"].get("counts").is_none());
    assert_eq!(
        logged["next_command"],
        format!("canon entity session solve {session_arg} --rules <RULES.yaml>")
    );

    // solve: counts restate solve.json exactly.
    let solved = solve(&session, &rules);
    let after_solve = status(&temp, "status after solve", &session, &[]);
    let counts = &after_solve["solve"]["counts"];
    assert_eq!(after_solve["solve"]["present"], true);
    assert_eq!(after_solve["solve"]["stale"], false);
    assert_eq!(reasons(&after_solve), Vec::<String>::new());
    assert_eq!(
        counts["accepted_tier_a"],
        solved["summary"]["accepted_tier_a"]
    );
    assert_eq!(
        counts["accepted_tier_b"],
        solved["summary"]["accepted_tier_b"]
    );
    assert_eq!(counts["review"], solved["summary"]["review"]);
    assert_eq!(counts["standalone"], solved["summary"]["standalone"]);
    assert_eq!(
        counts["resolved_existing"],
        solved["summary"]["resolved_existing"]
    );
    assert_eq!(counts["contradictions"], solved["summary"]["contradiction"]);
    assert_eq!(
        counts["dangling_evidence"],
        solved["summary"]["dangling_evidence"]
    );
    assert_eq!(counts["accepted_tier_a"], 4);
    assert_eq!(counts["contradictions"], 1);
    assert_eq!(counts["contradictions_by_reason"]["key_conflict"], 1);
    assert_eq!(counts["review"], 1);
    assert_eq!(counts["standalone"], 3);
    let accepted_new = after_solve["commit"]["uncommitted_accepted_clusters"]
        .as_u64()
        .unwrap();
    assert_eq!(accepted_new, 4, "four accepted clusters wait to be minted");
    assert_eq!(
        after_solve["next_command"],
        format!("canon entity session commit {session_arg} --bump patch")
    );
    let review = after_solve["top_unresolved"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "review")
        .expect("the Falcon pair is a review item");
    assert_eq!(review["missing"], "second_evidence_type");
    assert_eq!(review["evidence_types"], json!(["same_slot"]));
    assert_eq!(review["observation_count"], 2);

    // One more ingest: the solve is stale for exactly that reason and the
    // next command is the solve, not the commit.
    ingest(
        &session,
        &write_lines(
            &temp,
            "more.jsonl",
            &[observation("r12", "fundF", "Beta Unrelated Ltd", None)],
        ),
    );
    let stale = status(&temp, "status after another ingest", &session, &[]);
    assert_eq!(stale["log"]["observations"], 14);
    assert_eq!(stale["solve"]["stale"], true);
    assert_eq!(reasons(&stale), vec!["new_observations".to_string()]);
    assert_eq!(
        stale["solve"]["counts"]["standalone"], 3,
        "counts restate the recorded solve, not a recomputation"
    );
    assert!(
        stale["next_command"]
            .as_str()
            .unwrap()
            .starts_with(&format!("canon entity session solve {session_arg} --rules"))
    );
    let with_rules = status(
        &temp,
        "status naming the rules file",
        &session,
        &["--rules", rules.to_str().unwrap()],
    );
    assert_eq!(
        with_rules["next_command"],
        format!(
            "canon entity session solve {session_arg} --rules {}",
            rules.display()
        )
    );
    assert_eq!(reasons(&with_rules), vec!["new_observations".to_string()]);

    // Re-solve: fresh again, now with the extra standalone surface.
    solve(&session, &rules);
    let refreshed = status(&temp, "status after re-solve", &session, &[]);
    assert_eq!(refreshed["solve"]["stale"], false);
    assert_eq!(refreshed["solve"]["counts"]["standalone"], 4);

    // A different rules file is rule drift, reported only when named.
    let changed_rules = temp.path().join("rules-changed.yaml");
    fs::write(
        &changed_rules,
        RULES.replace("min_distinct_lineages: 2", "min_distinct_lineages: 3"),
    )
    .unwrap();
    let drifted = status(
        &temp,
        "status with changed rules",
        &session,
        &["--rules", changed_rules.to_str().unwrap()],
    );
    assert_eq!(reasons(&drifted), vec!["rules_changed".to_string()]);
    let unchanged = status(
        &temp,
        "status with the solve's rules",
        &session,
        &["--rules", rules.to_str().unwrap()],
    );
    assert_eq!(reasons(&unchanged), Vec::<String>::new());

    // Commit: nothing is left to commit, and the registry change the commit
    // itself made does not make the solve look stale.
    let committed = run_ok(
        "session commit",
        &[
            "entity",
            "session",
            "commit",
            session_arg,
            "--next-version",
            "0.1.0",
        ],
    );
    let minted = committed["minted"].as_array().unwrap().len();
    assert_eq!(minted as u64, accepted_new);
    let done = status(&temp, "status after commit", &session, &[]);
    assert_eq!(done["commit"]["present"], true);
    assert_eq!(done["commit"]["last_version"], "0.1.0");
    assert_eq!(done["commit"]["minted"], minted);
    assert_eq!(done["commit"]["uncommitted_accepted_clusters"], 0);
    assert!(done["commit"]["aliases_attached"].as_u64().unwrap() >= minted as u64);
    assert_eq!(done["session"]["registry"]["version"], "0.1.0");
    assert_eq!(done["solve"]["stale"], false);
    assert!(
        done["next_command"].is_null(),
        "nothing pending means no next command: {}",
        done["next_command"]
    );

    // A registry that moved on its own after the commit does make it stale.
    let registry_json = registry.join("registry.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&registry_json).unwrap()).unwrap();
    metadata["version"] = json!("9.9.9");
    fs::write(
        &registry_json,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    let moved = status(&temp, "status after registry moved", &session, &[]);
    assert_eq!(reasons(&moved), vec!["registry_changed".to_string()]);
    assert!(moved["next_command"].as_str().unwrap().contains(" solve "));
}

#[test]
fn frequency_not_name_ranks_unresolved_surfaces() {
    let temp = TempDir::new().unwrap();
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    let (session, _registry) = init_session(&temp, "ranking");

    // "Zulu Holdings" sorts after "Alpha Partners" but is seen 40 times
    // against 2. A ranking by name would put Alpha first.
    let mut rows = Vec::new();
    for index in 0..40 {
        let lineage = if index % 2 == 0 { "fundA" } else { "fundB" };
        rows.push(observation(
            &format!("zulu-{index:02}"),
            lineage,
            "Zulu Holdings",
            None,
        ));
    }
    for index in 0..2 {
        rows.push(observation(
            &format!("alpha-{index}"),
            "fundA",
            "Alpha Partners",
            None,
        ));
    }
    rows.push(observation("mid-0", "fundA", "Mid Corp", None));
    ingest(&session, &write_lines(&temp, "rank.jsonl", &rows));
    solve(&session, &rules);

    let ranked = status(&temp, "status ranking", &session, &[]);
    let top = ranked["top_unresolved"].as_array().unwrap();
    let surfaces = top
        .iter()
        .map(|item| item["surface"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        surfaces,
        vec!["Zulu Holdings", "Alpha Partners", "Mid Corp"]
    );
    assert_eq!(top[0]["observation_count"], 40);
    assert_eq!(top[0]["lineage_count"], 2);
    assert_eq!(top[1]["observation_count"], 2);
    assert_eq!(top[2]["observation_count"], 1);
    assert!(top.iter().all(|item| item["kind"] == "standalone"));
}

#[test]
fn only_twenty_unresolved_items_are_listed() {
    let temp = TempDir::new().unwrap();
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    let (session, _registry) = init_session(&temp, "cap");
    let rows = (0..25)
        .map(|index| {
            observation(
                &format!("c{index:02}"),
                "fundA",
                &format!("Corp {index:02}"),
                None,
            )
        })
        .collect::<Vec<_>>();
    ingest(&session, &write_lines(&temp, "cap.jsonl", &rows));
    solve(&session, &rules);
    let capped = status(&temp, "status cap", &session, &[]);
    assert_eq!(capped["solve"]["counts"]["standalone"], 25);
    assert_eq!(capped["top_unresolved"].as_array().unwrap().len(), 20);
}

#[test]
fn status_refuses_a_directory_that_is_not_a_session() {
    let temp = TempDir::new().unwrap();
    let not_a_session = temp.path().join("plain-dir");
    fs::create_dir(&not_a_session).unwrap();
    let before = snapshot_tree(temp.path());
    let output = canon()
        .args([
            "entity",
            "session",
            "status",
            not_a_session.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    log_step("status on a plain directory", &output);
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refusal["refusal"]["code"], "E_PARSE");
    assert_eq!(refusal["refusal"]["detail"]["writes_performed"], false);
    assert_eq!(
        snapshot_tree(temp.path()),
        before,
        "a refusal writes nothing"
    );

    let missing = temp.path().join("does-not-exist");
    let output = canon()
        .args(["entity", "session", "status", missing.to_str().unwrap()])
        .output()
        .unwrap();
    log_step("status on a missing path", &output);
    assert_eq!(output.status.code(), Some(2));
    assert!(!missing.exists(), "status must not create the directory");
}

#[test]
fn summary_mode_prints_the_same_facts_as_a_table() {
    let temp = TempDir::new().unwrap();
    let rules = temp.path().join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    let (session, _registry) = init_session(&temp, "summary");
    ingest(&session, &write_lines(&temp, "rows.jsonl", &fixture_rows()));
    solve(&session, &rules);

    let json_status = status(&temp, "status json", &session, &[]);
    let output = canon()
        .args([
            "entity",
            "session",
            "status",
            session.to_str().unwrap(),
            "--emit",
            "summary",
        ])
        .output()
        .unwrap();
    log_step("status summary", &output);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&format!(
        "tier_a={}",
        json_status["solve"]["counts"]["accepted_tier_a"]
    )));
    assert!(text.contains("key_conflict:1"));
    assert!(text.contains(&format!(
        "next: {}",
        json_status["next_command"].as_str().unwrap_or("none")
    )));
}
