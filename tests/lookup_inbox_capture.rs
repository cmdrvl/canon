//! bd-vdcw.56: plain exact lookup records its misses as unresolved-inbox items
//! with `--inbox-out`. Everything here goes through the public CLI; the inbox
//! file is only ever produced by `canon`, never written by hand.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

const SEEN_AT: &str = "2026-11-16T00:00:00Z";

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn make_registry(dir: &Path) {
    fs::write(
        dir.join("registry.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "orgs",
            "version": "1.0.0",
            "description": "lookup inbox fixture",
            "updated": "2026-11-15",
            "entry_count": 2
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("aliases.json"),
        serde_json::to_vec_pretty(&json!([
            {"input": "Alpha Corp", "canonical_id": "ORG-001", "canonical_type": "org", "rule_id": "MANUAL"},
            {"input": "Beta LLC", "canonical_id": "ORG-002", "canonical_type": "org", "rule_id": "MANUAL"}
        ]))
        .unwrap(),
    )
    .unwrap();
}

fn write_rows(path: &Path, names: &[&str]) {
    let mut text = String::from("name\n");
    for name in names {
        text.push_str(name);
        text.push('\n');
    }
    fs::write(path, text).unwrap();
}

struct Run {
    code: i32,
    stdout: Vec<u8>,
}

fn lookup(rows: &Path, registry: &Path, extra: &[&str]) -> Run {
    let output = canon()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "name",
            "--no-witness",
        ])
        .args(extra)
        .output()
        .unwrap();
    Run {
        code: output.status.code().unwrap(),
        stdout: output.stdout,
    }
}

fn inbox_flags<'a>(inbox: &'a str, run: &'a str) -> Vec<&'a str> {
    vec![
        "--inbox-out",
        inbox,
        "--inbox-project",
        "proj-1",
        "--inbox-run",
        run,
        "--inbox-seen-at",
        SEEN_AT,
    ]
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn json_of(run: &Run) -> Value {
    serde_json::from_slice(&run.stdout).unwrap()
}

#[test]
fn misses_become_inbox_items_and_repeats_add_occurrences() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");
    let inbox = temp.path().join("misses.inbox.json");
    let inbox_arg = inbox.to_str().unwrap();

    // 5 rows, 2 distinct misses (one of them twice).
    write_rows(
        &rows,
        &[
            "Alpha Corp",
            "Beta LLC",
            "Gamma Holdings",
            "Delta Partners",
            "Gamma Holdings",
        ],
    );
    let plain = lookup(&rows, &registry, &[]);
    let with_flag = lookup(&rows, &registry, &inbox_flags(inbox_arg, "run-1"));
    assert_eq!(plain.code, 1);
    assert_eq!(
        with_flag.code, plain.code,
        "exit code is unchanged by --inbox-out"
    );

    let mut stdout = json_of(&with_flag);
    let block = stdout.as_object_mut().unwrap().remove("inbox").unwrap();
    assert_eq!(stdout, json_of(&plain), "only the inbox block differs");
    assert_eq!(block["items_added"], 2);
    assert_eq!(block["items_total"], 2);
    let first_hash = block["artifact_content_hash"].as_str().unwrap().to_string();
    assert!(first_hash.starts_with("blake3:"));
    assert_eq!(
        read_json(&inbox)["artifact_content_hash"],
        first_hash.as_str()
    );

    // Second lookup, another run: one repeated miss and one new one.
    write_rows(&rows, &["Alpha Corp", "Gamma Holdings", "Epsilon Trust"]);
    let second = lookup(&rows, &registry, &inbox_flags(inbox_arg, "run-2"));
    assert_eq!(second.code, 1);
    let block = json_of(&second)["inbox"].clone();
    assert_eq!(block["items_added"], 1);
    assert_eq!(block["items_total"], 3);

    let file = read_json(&inbox);
    let items = file["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    let mut occurrence_counts: Vec<usize> = items
        .iter()
        .map(|item| item["occurrences"].as_array().unwrap().len())
        .collect();
    occurrence_counts.sort();
    assert_eq!(
        occurrence_counts,
        vec![1, 1, 2],
        "the repeated miss has 2 occurrences"
    );

    let listed = canon()
        .args(["inbox", "list", "--inbox", inbox_arg])
        .assert()
        .success();
    let listed: Value = serde_json::from_slice(&listed.get_output().stdout).unwrap();
    assert_eq!(listed["page"]["returned"], 3);
}

#[test]
fn missed_strings_never_appear_in_the_inbox_with_or_without_raw_ref() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");
    write_rows(&rows, &["Alpha Corp", "Gamma Holdings", "Delta Partners"]);

    for (name, raw_ref) in [("plain.inbox.json", false), ("rawref.inbox.json", true)] {
        let inbox = temp.path().join(name);
        let mut flags = inbox_flags(inbox.to_str().unwrap(), "run-1");
        if raw_ref {
            flags.push("--inbox-raw-ref");
        }
        let run = lookup(&rows, &registry, &flags);
        assert_eq!(run.code, 1);
        let text = fs::read_to_string(&inbox).unwrap();
        for value in ["Gamma Holdings", "Delta Partners", "Gamma", "Delta"] {
            assert!(!text.contains(value), "{name} must not contain {value:?}");
        }
        let file: Value = serde_json::from_str(&text).unwrap();
        for item in file["items"].as_array().unwrap() {
            let refs = item["raw_values"].as_array().cloned().unwrap_or_default();
            if raw_ref {
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0]["store"], "input_file");
                assert!(
                    refs[0]["locator"]
                        .as_str()
                        .unwrap()
                        .ends_with("#column=name"),
                    "reference names the input and column"
                );
            } else {
                assert!(refs.is_empty());
            }
        }
    }
}

#[test]
fn fully_resolved_lookup_adds_no_item() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");
    write_rows(&rows, &["Alpha Corp", "Beta LLC"]);
    let inbox = temp.path().join("none.inbox.json");

    let run = lookup(
        &rows,
        &registry,
        &inbox_flags(inbox.to_str().unwrap(), "run-1"),
    );
    assert_eq!(run.code, 0);
    let block = json_of(&run)["inbox"].clone();
    assert_eq!(block["items_added"], 0);
    assert_eq!(block["items_total"], 0);
    assert!(read_json(&inbox)["items"].as_array().unwrap().is_empty());
}

#[test]
fn refusal_leaves_existing_inbox_byte_identical_and_missing_seen_at_is_refused() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");
    write_rows(&rows, &["Alpha Corp", "Gamma Holdings"]);
    let inbox = temp.path().join("misses.inbox.json");
    let inbox_arg = inbox.to_str().unwrap();
    assert_eq!(
        lookup(&rows, &registry, &inbox_flags(inbox_arg, "run-1")).code,
        1
    );
    let before = fs::read(&inbox).unwrap();

    // Missing column: a refusal that must not touch the inbox.
    let output = canon()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "no_such_column",
            "--no-witness",
        ])
        .args(inbox_flags(inbox_arg, "run-2"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(&inbox).unwrap(), before);

    // --inbox-out without --inbox-seen-at is refused naming the flag.
    let fresh = temp.path().join("fresh.inbox.json");
    let output = canon()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "name",
            "--no-witness",
            "--inbox-out",
            fresh.to_str().unwrap(),
            "--inbox-project",
            "proj-1",
            "--inbox-run",
            "run-1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).contains("--inbox-seen-at"));
    assert!(!fresh.exists());
}

#[test]
fn shuffled_row_order_gives_a_byte_identical_inbox() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");

    write_rows(
        &rows,
        &[
            "Gamma Holdings",
            "Alpha Corp",
            "Delta Partners",
            "Epsilon Trust",
        ],
    );
    let first = temp.path().join("a.inbox.json");
    assert_eq!(
        lookup(
            &rows,
            &registry,
            &inbox_flags(first.to_str().unwrap(), "run-1")
        )
        .code,
        1
    );

    write_rows(
        &rows,
        &[
            "Epsilon Trust",
            "Delta Partners",
            "Alpha Corp",
            "Gamma Holdings",
        ],
    );
    let second = temp.path().join("b.inbox.json");
    assert_eq!(
        lookup(
            &rows,
            &registry,
            &inbox_flags(second.to_str().unwrap(), "run-1")
        )
        .code,
        1
    );
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
}

#[test]
fn mixed_resolved_and_unresolved_rows_in_another_order_give_a_byte_identical_inbox() {
    let temp = TempDir::new().unwrap();
    let registry = temp.path().join("reg");
    fs::create_dir(&registry).unwrap();
    make_registry(&registry);
    let rows = temp.path().join("rows.csv");

    write_rows(
        &rows,
        &[
            "Gamma Holdings",
            "Alpha Corp",
            "Delta Partners",
            "Beta LLC",
            "Epsilon Trust",
        ],
    );
    let first = temp.path().join("a.inbox.json");
    let first_run = lookup(
        &rows,
        &registry,
        &inbox_flags(first.to_str().unwrap(), "run-1"),
    );
    assert_eq!(first_run.code, 1, "partial lookup exits 1");

    write_rows(
        &rows,
        &[
            "Beta LLC",
            "Epsilon Trust",
            "Alpha Corp",
            "Gamma Holdings",
            "Delta Partners",
        ],
    );
    let second = temp.path().join("b.inbox.json");
    let second_run = lookup(
        &rows,
        &registry,
        &inbox_flags(second.to_str().unwrap(), "run-1"),
    );
    assert_eq!(second_run.code, 1, "partial lookup exits 1");

    let first_bytes = fs::read(&first).unwrap();
    let second_bytes = fs::read(&second).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&first_bytes),
        String::from_utf8_lossy(&second_bytes),
        "the inbox must not depend on where resolved rows sit among the misses"
    );
}
