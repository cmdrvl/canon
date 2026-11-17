//! `canon registry correct` detach, split and remove_cannot_link
//! (bd-vdcw.53). The registry is built by a real session commit plus
//! `registry add-entry`; nothing edits a mapping file by hand.

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
        "source": {"channel": "file", "id": "split-fixture", "lineage": lineage}
    })
}

fn evidence(kind: &str, left: &str, right: &str, lineage: &str) -> Value {
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": {"rule": "fixture", "left_value": left, "right_value": right, "steps": ["fixture"]},
        "source": {"channel": "fixture", "id": format!("{kind}-{lineage}"), "lineage": lineage},
        "asserted_by": "agent:fixture"
    })
}

fn session_in(
    root: &Path,
    label: &str,
    registry: &Path,
    rows: &[Value],
    proof: &[Value],
) -> (PathBuf, Value) {
    let session = root.join(format!("{label}-session"));
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "split detach",
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
    let solved = run_json(
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
    (session, solved)
}

/// ORG-001 holds two entities: Apex Holdings (cik 111) and Apex Capital LLC
/// (cik 444), the latter attached later by a reviewer-authored add-entry.
fn two_entities_in_one_id(root: &Path) -> PathBuf {
    let registry = root.join("registry");
    let (session, _) = session_in(
        root,
        "mint",
        &registry,
        &[
            observation("a1", "fundA", "Apex Holdings", json!({"cik": "111"})),
            observation("a2", "fundB", "Apex Capital LLC", json!({"cik": "111"})),
        ],
        &[],
    );
    run_json(
        &[
            "entity",
            "session",
            "commit",
            p(&session),
            "--next-version",
            "0.2.0",
        ],
        0,
    );
    for (alias_file, input, rule) in [
        ("keys.cik.json", "444", "SESSION_KEY_CIK"),
        ("aliases.json", "Apex Capital, L.L.C.", "SESSION_TIER_A"),
    ] {
        run_json(
            &[
                "registry",
                "add-entry",
                "--registry",
                p(&registry),
                "--alias-file",
                alias_file,
                "--canonical-id",
                "ORG-001",
                "--input",
                input,
                "--rule-id",
                rule,
                "--canonical-type",
                "organisation",
                "--next-version",
                &format!("0.2.{}", if input == "444" { 1 } else { 2 }),
            ],
            0,
        );
    }
    registry
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

fn correction(extra: Value) -> Value {
    let mut base = json!({
        "reviewer": "human:reviewer",
        "evidence_ids": ["evidence:blake3:aaaa"],
        "reason": "two entities, two CIKs",
    });
    if let (Some(object), Some(more)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in more {
            if value.is_null() {
                object.remove(key);
            } else {
                object.insert(key.clone(), value.clone());
            }
        }
    }
    base
}

fn split_line() -> Value {
    correction(json!({
        "op": "split",
        "canonical_id": "ORG-001",
        "move": {
            "inputs": ["Apex Capital LLC", "Apex Capital, L.L.C."],
            "keys": [{"namespace": "cik", "value": "444"}],
        },
    }))
}

fn correct(registry: &Path, file: &Path, version: &str, extra: &[&str], exit: i32) -> Value {
    let mut args = vec![
        "registry",
        "correct",
        "--registry",
        p(registry),
        "--from",
        p(file),
        "--next-version",
        version,
    ];
    args.extend_from_slice(extra);
    run_json(&args, exit)
}

fn entries(registry: &Path) -> Vec<Value> {
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

fn id_of<'a>(rows: &'a [Value], input: &str) -> Option<&'a str> {
    rows.iter()
        .find(|row| row["input"] == input)
        .and_then(|row| row["canonical_id"].as_str())
}

#[test]
fn a_split_moves_the_named_entries_to_a_new_id_and_the_next_session_does_not_rejoin_them() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_entities_in_one_id(root);
    let before_rows = entries(&registry);
    let before_closures = before_rows
        .iter()
        .map(|row| {
            (
                row["input"].as_str().unwrap().to_string(),
                row["session_authority"]["closure_hash"].clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let file = write_jsonl(root, "split.jsonl", &[split_line()]);

    let plan = correct(&registry, &file, "1.0.0", &["--dry-run"], 0);
    assert_eq!(plan["outcome"], "dry_run");
    assert_eq!(plan["corrections"][0]["new_id"], "ORG-002");
    assert_eq!(
        tree(&registry).get("corrections.jsonl"),
        None,
        "dry run wrote nothing"
    );

    let snapshot = tree(&registry);
    let applied = correct(&registry, &file, "1.0.0", &[], 0);
    eprintln!("split: {applied}");
    assert_eq!(applied["outcome"], "applied");
    assert_eq!(applied["version_before"], "0.2.2");
    assert_eq!(applied["version_after"], "1.0.0");
    let summary = &applied["corrections"][0];
    assert_eq!(summary["op"], "split");
    assert_eq!(summary["moved"], 3);
    assert_eq!(summary["new_id"], "ORG-002");
    assert_ne!(tree(&registry), snapshot);

    // The moved spellings and CIK resolve to the new ID, the rest to the old.
    let rows = entries(&registry);
    assert_eq!(id_of(&rows, "Apex Capital LLC"), Some("ORG-002"));
    assert_eq!(id_of(&rows, "Apex Capital, L.L.C."), Some("ORG-002"));
    assert_eq!(id_of(&rows, "444"), Some("ORG-002"));
    assert_eq!(id_of(&rows, "Apex Holdings"), Some("ORG-001"));
    assert_eq!(id_of(&rows, "111"), Some("ORG-001"));
    // Kept and moved entries keep the closure they had; moved ones gain a mark.
    for row in &rows {
        let input = row["input"].as_str().unwrap();
        assert_eq!(
            row["session_authority"]["closure_hash"], before_closures[input],
            "{input} keeps its closure"
        );
    }
    let moved = rows
        .iter()
        .find(|row| row["input"] == "Apex Capital LLC")
        .unwrap();
    assert_eq!(moved["correction"]["op"], "split");
    assert_eq!(moved["correction"]["from"], "ORG-001");

    // Lint passes and memory holds the pair.
    let lint = run_json(&["registry", "lint", p(&registry)], 0);
    assert_eq!(lint["summary"]["errors"], 0, "{lint}");
    let cannot = fs::read_to_string(registry.join("_escrow/cannot_link.jsonl")).unwrap();
    assert!(
        cannot.contains("ORG-001") && cannot.contains("ORG-002"),
        "{cannot}"
    );

    // The line carries its own outcome and closure; versions and file hashes.
    let stored: Vec<Value> = fs::read_to_string(registry.join("corrections.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["op"], "split");
    assert_eq!(stored[0]["closure_hash"], summary["closure_hash"]);
    assert!(
        stored[0]["outcome_ref"]
            .as_str()
            .unwrap()
            .starts_with("cmdrvl://canon/outcome/")
    );
    assert_eq!(stored[0]["version_before"], "0.2.2");
    assert!(stored[0]["changed_files"].as_array().unwrap().len() >= 3);
    assert_eq!(stored[0]["moved"].as_array().unwrap().len(), 3);

    // Hand-off blocks (bd-vdcw.55).
    assert_ne!(
        applied["base"]["content_hash"],
        applied["new"]["content_hash"]
    );

    // A following session ingests both spellings with evidence that would join
    // them: the registry holds them apart and merges nothing.
    let (_, solved) = session_in(
        root,
        "after",
        &registry,
        &[
            observation("s1", "fundA", "Apex Holdings", json!({})),
            observation("s2", "fundB", "Apex Capital LLC", json!({})),
        ],
        &[
            evidence("same_slot", "s1", "s2", "fundA"),
            evidence("name_equivalent", "s1", "s2", "fundB"),
        ],
    );
    eprintln!("after split: {solved}");
    assert_eq!(
        solved["summary"]["contradiction"], 1,
        "nothing merges: {solved}"
    );
    let reasons = solved["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|cluster| {
            cluster["conflict_reasons"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert!(
        reasons
            .iter()
            .any(|reason| reason == "registry_cannot_link"),
        "the registry memory vetoes the join: {solved}"
    );

    // Re-running is already_applied with no writes; the corrections file only
    // grows (no line is rewritten).
    let settled = tree(&registry);
    let again = correct(&registry, &file, "1.0.0", &[], 0);
    assert_eq!(again["outcome"], "already_applied");
    assert_eq!(tree(&registry), settled);
}

#[test]
fn a_detach_takes_a_spelling_off_the_id_and_the_next_session_does_not_reattach_it() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_entities_in_one_id(root);
    let detach = write_jsonl(
        root,
        "detach.jsonl",
        &[correction(json!({
            "op": "detach",
            "canonical_id": "ORG-001",
            "inputs": ["Apex Capital LLC"],
        }))],
    );
    let applied = correct(&registry, &detach, "1.0.0", &[], 0);
    eprintln!("detach: {applied}");
    assert_eq!(applied["corrections"][0]["detached"], 1);
    assert_eq!(applied["corrections"][0]["memory_written"], 1);
    let rows = entries(&registry);
    assert!(id_of(&rows, "Apex Capital LLC").is_none(), "no entry left");
    assert_eq!(id_of(&rows, "Apex Holdings"), Some("ORG-001"));

    // The line records the outcome and closure of the entry it removed.
    let stored: Value = serde_json::from_str(
        fs::read_to_string(registry.join("corrections.jsonl"))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    let removed = &stored["removed"][0];
    assert_eq!(removed["input"], "Apex Capital LLC");
    assert!(
        removed["closure_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    assert!(
        removed["outcome_ref"]
            .as_str()
            .unwrap()
            .starts_with("cmdrvl://canon/outcome/")
    );

    // Plain exact lookup of the detached spelling is unresolved.
    let csv = root.join("rows.csv");
    fs::write(&csv, "name\nApex Capital LLC\nApex Holdings\n").unwrap();
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
        1, // an unresolved row exits 1
    );
    let text = String::from_utf8(lookup.stdout).unwrap();
    eprintln!("lookup: {text}");
    assert!(text.to_ascii_lowercase().contains("unresolved"), "{text}");

    // A following session with evidence from the detached spelling to the old
    // ID's spelling does not re-attach it.
    let before = tree(&registry);
    let (_, solved) = session_in(
        root,
        "after",
        &registry,
        &[
            observation("d1", "fundA", "Apex Holdings", json!({})),
            observation("d2", "fundB", "Apex Capital LLC", json!({})),
        ],
        &[
            evidence("same_slot", "d1", "d2", "fundA"),
            evidence("name_equivalent", "d1", "d2", "fundB"),
        ],
    );
    assert_eq!(
        solved["summary"]["contradiction"], 1,
        "not re-attached: {solved}"
    );
    assert!(
        solved.to_string().contains("registry_cannot_link"),
        "the detach memory vetoes the join: {solved}"
    );
    assert_eq!(
        tree(&registry),
        before,
        "a session does not write the registry on solve"
    );
}

#[test]
fn nothing_is_automatic_and_refusals_write_nothing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_entities_in_one_id(root);
    let snapshot = tree(&registry);

    // A session holding evidence against the ID changes no registry file
    // without a corrections line.
    session_in(
        root,
        "evidence",
        &registry,
        &[
            observation("e1", "fundA", "Apex Holdings", json!({})),
            observation("e2", "fundB", "Apex Capital LLC", json!({})),
        ],
        &[json!({
            "type": "distinct",
            "left": "e1",
            "right": "e2",
            "basis": {"authority": "reviewed"},
            "source": {"channel": "agent", "id": "review", "lineage": "review"},
            "asserted_by": "human:reviewer"
        })],
    );
    assert_eq!(tree(&registry), snapshot);

    for (name, line) in [
        (
            "split-agent",
            correction(json!({
                "op": "split", "canonical_id": "ORG-001",
                "move": {"inputs": ["Apex Capital LLC"]},
                "reviewer": "agent:claude",
            })),
        ),
        (
            "detach-no-evidence",
            correction(json!({
                "op": "detach", "canonical_id": "ORG-001",
                "inputs": ["Apex Capital LLC"], "evidence_ids": [],
            })),
        ),
        (
            "split-everything",
            correction(json!({
                "op": "split", "canonical_id": "ORG-001",
                "move": {
                    "inputs": ["Apex Holdings", "Apex Capital LLC", "Apex Capital, L.L.C."],
                    "keys": [
                        {"namespace": "cik", "value": "111"},
                        {"namespace": "cik", "value": "444"},
                    ],
                },
            })),
        ),
        (
            "split-missing-entry",
            correction(json!({
                "op": "split", "canonical_id": "ORG-001",
                "move": {"inputs": ["Not On This Id"]},
            })),
        ),
        (
            "detach-last",
            correction(json!({
                "op": "detach", "canonical_id": "ORG-001",
                "inputs": ["Apex Holdings", "Apex Capital LLC", "Apex Capital, L.L.C."],
                "keys": [
                    {"namespace": "cik", "value": "111"},
                    {"namespace": "cik", "value": "444"},
                ],
            })),
        ),
    ] {
        let file = write_jsonl(root, &format!("{name}.jsonl"), &[line]);
        let refused = correct(&registry, &file, "1.0.0", &[], 2);
        assert!(
            refused["refusal"]["message"].is_string(),
            "{name}: {refused}"
        );
        assert_eq!(tree(&registry), snapshot, "{name} wrote nothing");
    }
}

#[test]
fn remove_cannot_link_is_the_only_way_a_cannot_link_leaves_memory() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = two_entities_in_one_id(root);
    let split = write_jsonl(root, "split.jsonl", &[split_line()]);
    correct(&registry, &split, "1.0.0", &[], 0);
    let cannot_path = registry.join("_escrow/cannot_link.jsonl");
    assert!(!fs::read_to_string(&cannot_path).unwrap().trim().is_empty());

    // Removing a pair that is not in memory is refused.
    let unknown = write_jsonl(
        root,
        "unknown.jsonl",
        &[correction(
            json!({"op": "remove_cannot_link", "left": "ORG-001", "right": "ORG-404"}),
        )],
    );
    let before = tree(&registry);
    correct(&registry, &unknown, "1.1.0", &[], 2);
    assert_eq!(tree(&registry), before);

    let remove = write_jsonl(
        root,
        "remove.jsonl",
        &[correction(
            json!({"op": "remove_cannot_link", "left": "ORG-002", "right": "ORG-001"}),
        )],
    );
    let applied = correct(&registry, &remove, "1.1.0", &[], 0);
    assert_eq!(applied["corrections"][0]["cannot_links_removed"], 1);
    assert!(fs::read_to_string(&cannot_path).unwrap().trim().is_empty());

    // The corrections file only grew: the split line is byte-identical.
    let lines: Vec<String> = fs::read_to_string(registry.join("corrections.jsonl"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2);
    let first: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(first["op"], "split");
    let second: Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(second["op"], "remove_cannot_link");
    assert_eq!(
        second["removed_cannot_links"][0]["closure_hash"], first["closure_hash"],
        "the removed fact names the split that wrote it"
    );
}
