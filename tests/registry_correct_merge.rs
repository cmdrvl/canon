//! `canon registry correct` (bd-vdcw.52): merge two canonical IDs as a reviewed
//! registry version. The registries here are built by real session commits (two
//! sessions on two days minting two IDs for one entity); nothing edits a
//! mapping file by hand.

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
  lei: {authoritative: true, single_valued: true}
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

fn run(args: &[&str], expected_exit: i32) -> std::process::Output {
    let output = canon().args(args).output().expect("canon runs");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "args={args:?} stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_json(args: &[&str], expected_exit: i32) -> Value {
    serde_json::from_slice(&run(args, expected_exit).stdout).expect("JSON stdout")
}

fn p(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

fn write_jsonl(dir: &Path, name: &str, rows: &[Value]) -> PathBuf {
    let path = dir.join(name);
    let text = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{text}\n")).unwrap();
    path
}

fn observation(obs_id: &str, lineage: &str, surface: &str, keys: Value) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "file", "id": "correct-fixture", "lineage": lineage}
    })
}

fn commit_session(
    root: &Path,
    label: &str,
    registry: &Path,
    rows: &[Value],
    proof: &[Value],
    version: &str,
) -> Value {
    let session = root.join(format!("{label}-session"));
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "correction fixture",
            "--registry",
            p(registry),
            "--work-dir",
            p(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            p(&session),
            p(&write_jsonl(root, &format!("{label}-obs.jsonl"), rows)),
        ],
        0,
    );
    if !proof.is_empty() {
        run_json(
            &[
                "entity",
                "session",
                "assert",
                p(&session),
                p(&write_jsonl(root, &format!("{label}-ev.jsonl"), proof)),
            ],
            0,
        );
    }
    run_json(
        &[
            "entity",
            "session",
            "solve",
            p(&session),
            "--rules",
            p(&rules),
        ],
        0,
    );
    run_json(
        &[
            "entity",
            "session",
            "commit",
            p(&session),
            "--next-version",
            version,
        ],
        0,
    )
}

/// ORG-001 "Acme Corp" (cik 100, day 2) and ORG-002 "Acme Corporation" (lei
/// L1, day 9): two IDs for one entity, committed by two sessions.
fn two_ids_for_one_entity(root: &Path, second_keys: Value) -> PathBuf {
    let registry = root.join("registry");
    commit_session(
        root,
        "day2",
        &registry,
        &[observation(
            "a1",
            "fundA",
            "Acme Corp",
            json!({"cik": "100"}),
        )],
        &[],
        "0.2.0",
    );
    commit_session(
        root,
        "day9",
        &registry,
        &[observation("b1", "fundB", "Acme Corporation", second_keys)],
        &[],
        "0.3.0",
    );
    registry
}

fn merge_line(extra: Value) -> Value {
    let mut line = json!({
        "op": "merge",
        "survivor": "ORG-001",
        "retired": "ORG-002",
        "reviewer": "human:reviewer",
        "evidence_ids": ["evidence:blake3:aaaa"],
        "reason": "same legal entity",
        "source": {"solve_hash": "blake3:solve", "cluster_id": "cluster:x"},
    });
    if let (Some(base), Some(more)) = (line.as_object_mut(), extra.as_object()) {
        for (key, value) in more {
            if value.is_null() {
                base.remove(key);
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    line
}

fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(dir, dir, &mut out);
    out
}

fn copy_tree(from: &Path, to: &Path) {
    for (relative, bytes) in tree(from) {
        let target = to.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
}

fn correct(registry: &Path, corrections: &Path, extra: &[&str], exit: i32) -> Value {
    let mut args = vec![
        "registry",
        "correct",
        "--registry",
        p(registry),
        "--from",
        p(corrections),
        "--next-version",
        "1.0.0",
    ];
    args.extend_from_slice(extra);
    run_json(&args, exit)
}

fn aliases_of(registry: &Path) -> Vec<Value> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(registry).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".json")
            && name != "registry.json"
            && name != "_build.json"
            && let Ok(Value::Array(found)) =
                serde_json::from_slice::<Value>(&fs::read(&path).unwrap())
        {
            rows.extend(found);
        }
    }
    rows
}

#[test]
fn a_reviewed_merge_resolves_the_retired_id_to_the_survivor_and_keeps_the_evidence() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_ids_for_one_entity(root, json!({"lei": "5493001KJTIIGC8Y1R12"}));
    let before = root.join("before");
    copy_tree(&registry, &before);
    let corrections = write_jsonl(root, "corrections.jsonl", &[merge_line(json!({}))]);

    // --dry-run prints the plan and leaves every file byte-identical.
    let plan = correct(&registry, &corrections, &["--dry-run"], 0);
    eprintln!("dry run: {plan}");
    assert_eq!(plan["outcome"], "dry_run");
    assert_eq!(plan["corrections"][0]["retired"], "ORG-002");
    assert_eq!(tree(&registry), tree(&before), "dry run wrote nothing");

    let applied = correct(
        &registry,
        &corrections,
        &["--rules", p(&root.join("rules.yaml"))],
        0,
    );
    eprintln!("applied: {applied}");
    assert_eq!(applied["outcome"], "applied");
    assert_eq!(applied["version_before"], "0.3.0");
    assert_eq!(applied["version_after"], "1.0.0");
    let correction = &applied["corrections"][0];
    assert_eq!(correction["key_check"], "passed");
    assert_eq!(correction["survivor"], "ORG-001");

    // Hand-off blocks (bd-vdcw.55): base and new are the identities before and
    // after, and changed_files is the independent tree diff.
    let after_identity = run_json(&["registry", "identity", "--registry", p(&registry)], 0);
    assert_eq!(
        applied["new"]["content_hash"],
        after_identity["content_hash"]
    );
    let before_identity = run_json(&["registry", "identity", "--registry", p(&before)], 0);
    assert_eq!(
        applied["base"]["content_hash"],
        before_identity["content_hash"]
    );
    let mut independent = Vec::new();
    let (old_tree, new_tree) = (tree(&before), tree(&registry));
    for path in old_tree.keys().chain(new_tree.keys()) {
        if old_tree.get(path) != new_tree.get(path) && !independent.contains(path) {
            independent.push(path.clone());
        }
    }
    independent.retain(|path| !path.starts_with('.'));
    independent.sort();
    let reported = applied["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(reported, independent);

    // Every spelling of both resolves to the survivor.
    let rows = aliases_of(&registry);
    assert!(
        rows.iter().all(|row| row["canonical_id"] == "ORG-001"),
        "{rows:?}"
    );
    let superseded = rows
        .iter()
        .filter(|row| row["rule_id"] == "SUPERSEDED_ID_ALIAS")
        .collect::<Vec<_>>();
    assert_eq!(superseded.len(), 1);
    assert_eq!(superseded[0]["input"], "ORG-002");

    // The correction is an outcome whose closure is its own line; the alias
    // entry carries the same one, and re-pointed entries kept their evidence.
    let stored: Vec<Value> = fs::read_to_string(registry.join("corrections.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["closure_hash"], correction["closure_hash"]);
    assert_eq!(
        superseded[0]["session_authority"]["closure_hash"],
        correction["closure_hash"]
    );
    let moved = rows
        .iter()
        .find(|row| row["input"] == "Acme Corporation")
        .expect("the retired id's own spelling");
    assert_eq!(moved["correction"]["from"], "ORG-002");
    assert!(
        moved["session_authority"]["closure_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:"),
        "the original closure survives: {moved}"
    );

    // Plain exact lookup of the retired ID resolves to the survivor and shows
    // the SUPERSEDED_ID_ALIAS rule.
    let csv = root.join("rows.csv");
    fs::write(&csv, "name\nORG-002\nAcme Corporation\n").unwrap();
    let lookup = run(
        &[
            p(&csv),
            "--registry",
            p(&registry),
            "--column",
            "name",
            "--no-witness",
            "--explicit",
            "--plain-json-values",
        ],
        0,
    );
    let text = String::from_utf8(lookup.stdout).unwrap();
    eprintln!("lookup output: {text}");
    assert!(text.contains("SUPERSEDED_ID_ALIAS"), "{text}");
    assert!(text.matches("ORG-001").count() >= 2, "{text}");

    // Lint passes and the diff names the re-pointed entries and the added alias.
    let lint = run_json(&["registry", "lint", p(&registry)], 0);
    assert_eq!(lint["summary"]["errors"], 0, "{lint}");
    let diff = run_json(
        &[
            "registry",
            "diff",
            "--old",
            p(&before),
            "--new",
            p(&registry),
        ],
        0,
    );
    eprintln!("diff: {diff}");
    assert!(diff.to_string().contains("ORG-002"));

    // The search index keeps the retired id out of the ranked material.
    let index = root.join("search.sqlite");
    run(
        &[
            "registry",
            "export",
            "--registry",
            p(&registry),
            "--format",
            "search-index",
            "--out",
            p(&index),
        ],
        0,
    );
    let conn = rusqlite::Connection::open(&index).unwrap();
    let kind: String = conn
        .query_row(
            "SELECT alias_kind FROM aliases WHERE alias = 'ORG-002'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kind, "superseded-id");
    let found: i64 = conn
        .query_row(
            "SELECT count(*) FROM aliases_fts WHERE aliases_fts MATCH '\"002\"'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        found, 0,
        "a query for a substring of the retired id finds nothing"
    );

    // link-evidence from the retired ID to a survivor spelling runs through the
    // surviving id and includes the correction's closure.
    let answer = run_json(
        &[
            "registry",
            "export",
            "--registry",
            p(&registry),
            "--format",
            "link-evidence",
            "--from",
            "ORG-002",
            "--to",
            "Acme Corp",
        ],
        0,
    );
    let chain = answer["chain"].as_array().expect("a chain");
    assert!(
        chain
            .iter()
            .any(|hop| hop["closure_hash"] == correction["closure_hash"]),
        "{answer}"
    );

    // A retired id is never minted again.
    let next = run(
        &["registry", "next-id", "ORG", "--registry", p(&registry)],
        0,
    );
    assert_eq!(String::from_utf8(next.stdout).unwrap().trim(), "ORG-003");

    // Re-running the same corrections file is already_applied with no writes.
    let settled = tree(&registry);
    let again = correct(&registry, &corrections, &[], 0);
    assert_eq!(again["outcome"], "already_applied");
    assert_eq!(again["base"], again["new"]);
    assert_eq!(again["changed_files"], json!([]));
    assert_eq!(tree(&registry), settled);
}

#[test]
fn refusals_write_nothing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_ids_for_one_entity(root, json!({"lei": "5493001KJTIIGC8Y1R12"}));
    let snapshot = tree(&registry);

    // No human reviewer, no evidence ids.
    for (name, extra) in [
        ("agent", json!({"reviewer": "agent:claude"})),
        ("no-evidence", json!({"evidence_ids": []})),
        ("no-reviewer", json!({"reviewer": null})),
    ] {
        let corrections = write_jsonl(root, &format!("{name}.jsonl"), &[merge_line(extra)]);
        let refused = correct(&registry, &corrections, &[], 2);
        assert!(
            refused["refusal"]["message"].is_string(),
            "{name}: {refused}"
        );
        assert_eq!(tree(&registry), snapshot, "{name} wrote nothing");
    }

    // A stale base.
    let corrections = write_jsonl(root, "ok.jsonl", &[merge_line(json!({}))]);
    let stale = "blake3:0000000000000000000000000000000000000000000000000000000000000000";
    let refused = correct(&registry, &corrections, &["--expect-base", stale], 2);
    assert_eq!(refused["refusal"]["detail"]["reason"], "base_moved");
    assert_eq!(tree(&registry), snapshot);

    // An id that does not exist.
    let missing = write_jsonl(
        root,
        "missing.jsonl",
        &[merge_line(json!({"retired": "ORG-404"}))],
    );
    correct(&registry, &missing, &[], 2);
    assert_eq!(tree(&registry), snapshot);
}

#[test]
fn different_ciks_are_refused_naming_both_with_rules_and_a_lint_failure_restores_everything() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_ids_for_one_entity(root, json!({"cik": "200"}));
    let snapshot = tree(&registry);
    let corrections = write_jsonl(root, "merge.jsonl", &[merge_line(json!({}))]);

    let refused = correct(
        &registry,
        &corrections,
        &["--rules", p(&root.join("rules.yaml"))],
        2,
    );
    eprintln!("refused: {refused}");
    let detail = &refused["refusal"]["detail"];
    assert_eq!(detail["namespace"], "cik");
    assert_eq!(detail["survivor_values"], json!(["100"]));
    assert_eq!(detail["retired_values"], json!(["200"]));
    assert_eq!(tree(&registry), snapshot);

    // Without --rules the check does not run and says so.
    let plan = correct(&registry, &corrections, &["--dry-run"], 0);
    assert_eq!(plan["corrections"][0]["key_check"], "not_run");

    // A planted lint failure: a third entry already claims the retired id's
    // string as its own alias, so the superseded alias would be a duplicate
    // input for a different id. Everything is restored.
    let planted = root.join("planted");
    copy_tree(&registry, &planted);
    run_json(
        &[
            "registry",
            "mint",
            "--registry",
            p(&planted),
            "--canonical-id",
            "ORG-900",
            "--canonical-type",
            "organisation",
            "--with-alias",
            "aliases.json=ORG-002:MANUAL",
            "--next-version",
            "0.3.1",
            "--no-lint",
        ],
        0,
    );
    let planted_snapshot = tree(&planted);
    let refused = correct(&planted, &corrections, &[], 2);
    eprintln!("planted: {refused}");
    assert!(
        refused["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("lint")
    );
    assert_eq!(tree(&planted), planted_snapshot, "every file restored");
}

#[test]
fn a_cannot_link_between_the_two_ids_refuses_the_merge() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_ids_for_one_entity(root, json!({"lei": "5493001KJTIIGC8Y1R12"}));
    // A reviewed `distinct` decision between the two committed IDs, recorded by
    // a real session as registry memory.
    commit_session(
        root,
        "review",
        &registry,
        &[
            observation("c1", "fundA", "Acme Corp", json!({"cik": "100"})),
            observation(
                "c2",
                "fundB",
                "Acme Corporation",
                json!({"lei": "5493001KJTIIGC8Y1R12"}),
            ),
        ],
        &[json!({
            "type": "distinct",
            "left": "c1",
            "right": "c2",
            "basis": {"authority": "reviewed"},
            "source": {"channel": "agent", "id": "review", "lineage": "review"},
            "asserted_by": "human:reviewer"
        })],
        "0.4.0",
    );
    let cannot = fs::read_to_string(registry.join("_escrow/cannot_link.jsonl")).unwrap_or_default();
    assert!(
        !cannot.trim().is_empty(),
        "the session recorded a cannot-link"
    );
    let snapshot = tree(&registry);
    let corrections = write_jsonl(root, "merge.jsonl", &[merge_line(json!({}))]);
    let refused = correct(&registry, &corrections, &[], 2);
    assert!(
        refused["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("cannot-link"),
        "{refused}"
    );
    assert_eq!(tree(&registry), snapshot);
}
