//! bd-rgjo stage 2: `registry correct` op `supersede` records succession, not
//! sameness. Both ids stay; the old id answers as itself flagged `superseded_by`
//! with the chain to the active end; a named spelling moves to the successor so
//! a shared name resolves to the active holder only. Invented names throughout.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const SHARED: &str = "Delta Holdings Corp";

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

/// Three ids with their own CIK and name; the shared name belongs to none.
fn registry(root: &Path) -> PathBuf {
    let registry = root.join("organisations");
    run(
        &[
            "registry",
            "init",
            "--registry",
            p(&registry),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    for (id, name, cik) in [
        ("ORG-001", "Delta Holdings Legacy Corp", "3845"),
        ("ORG-002", "Delta Holdings Lending Corp", "3906"),
        ("ORG-003", "Orion Lending Corp", "1287750"),
    ] {
        run(
            &[
                "registry",
                "mint",
                "--registry",
                p(&registry),
                "--canonical-id",
                id,
                "--canonical-type",
                "firm",
                "--with-alias",
                &format!("00-firms.json={name}:NAME"),
                "--with-alias",
                &format!("40-keys.json={cik}:SESSION_KEY_CIK"),
                "--create-alias-file",
                "--bump",
                "patch",
                "--no-lint",
            ],
            0,
        );
    }
    registry
}

fn line(superseded: &str, successor: &str, date: &str, inputs: &[&str]) -> Value {
    json!({
        "op": "supersede",
        "superseded": superseded,
        "successor": successor,
        "succession": "merged_into",
        "effective_date": date,
        "inputs": inputs,
        "reviewer": "human:reviewer",
        "evidence_ids": ["evidence:blake3:aaaa"],
        "reason": "the filing states the merger and its effective date",
        "source": {"channel": "filing", "id": "supersede-fixture"}
    })
}

fn write_line(root: &Path, name: &str, value: &Value) -> PathBuf {
    let path = root.join(name);
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(value).unwrap()),
    )
    .unwrap();
    path
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

fn lookup(registry: &Path, root: &Path, name: &str) -> Option<Value> {
    let input = root.join("lookup.csv");
    fs::write(&input, format!("name\n\"{name}\"\n")).unwrap();
    let output = canon()
        .arg(&input)
        .arg("--registry")
        .arg(registry)
        .arg("--column")
        .arg("name")
        .args(["--emit", "json", "--explicit", "--plain-json-values"])
        .output()
        .unwrap();
    let payload: Value = serde_json::from_slice(&output.stdout).expect("lookup json");
    payload["mappings"].get(0).cloned()
}

fn id_of(registry: &Path, root: &Path, name: &str) -> Option<String> {
    lookup(registry, root, name).and_then(|m| m["canonical_id"].as_str().map(str::to_string))
}

fn refused_with(registry: &Path, file: &Path, needle: &str) {
    let output = run(
        &[
            "registry",
            "correct",
            "--registry",
            p(registry),
            "--from",
            p(file),
            "--next-version",
            "9.0.0",
        ],
        2,
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(text.contains(needle), "expected '{needle}' in: {text}");
}

#[test]
fn a_succession_keeps_both_ids_flags_the_old_one_and_moves_the_shared_name() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = registry(root);
    assert_eq!(id_of(&registry, root, SHARED), None, "an alias of neither");

    // Dry run first: nothing is written.
    let first = write_line(
        root,
        "first.jsonl",
        &line("ORG-001", "ORG-002", "1997-12-31", &[SHARED]),
    );
    let dry = correct(&registry, &first, "1.0.0", &["--dry-run"], 0);
    assert_eq!(dry["outcome"], "dry_run", "{dry}");
    assert_eq!(id_of(&registry, root, SHARED), None);

    let applied = correct(&registry, &first, "1.0.0", &[], 0);
    assert_eq!(applied["outcome"], "applied", "{applied}");
    assert!(applied["corrections"][0]["closure_hash"].is_string());

    // The shared name resolves to the active holder; the old id keeps its own
    // key and name and answers as itself, flagged.
    assert_eq!(id_of(&registry, root, SHARED).as_deref(), Some("ORG-002"));
    let old = lookup(&registry, root, "3845").expect("old cik resolves");
    assert_eq!(old["canonical_id"], "ORG-001");
    assert_eq!(old["succession"]["superseded_by"], "ORG-002", "{old}");
    assert_eq!(old["succession"]["chain"], json!(["ORG-002"]));
    assert_eq!(old["succession"]["effective_date"], "1997-12-31");
    let by_name = lookup(&registry, root, "Delta Holdings Legacy Corp").unwrap();
    assert_eq!(by_name["canonical_id"], "ORG-001");
    // The active id carries no flag.
    assert!(
        lookup(&registry, root, "3906")
            .unwrap()
            .get("succession")
            .is_none()
    );

    // Re-running the same correction changes nothing.
    let again = correct(&registry, &first, "1.0.1", &[], 0);
    assert_eq!(again["outcome"], "already_applied", "{again}");

    // The second succession extends the chain and moves the name again.
    let second = write_line(
        root,
        "second.jsonl",
        &line("ORG-002", "ORG-003", "2010-04-01", &[SHARED]),
    );
    let applied = correct(&registry, &second, "1.0.2", &[], 0);
    assert_eq!(applied["outcome"], "applied", "{applied}");
    assert_eq!(id_of(&registry, root, SHARED).as_deref(), Some("ORG-003"));
    let old = lookup(&registry, root, "3845").unwrap();
    assert_eq!(old["canonical_id"], "ORG-001");
    assert_eq!(
        old["succession"]["chain"],
        json!(["ORG-002", "ORG-003"]),
        "{old}"
    );
    let middle = lookup(&registry, root, "3906").unwrap();
    assert_eq!(middle["canonical_id"], "ORG-002");
    assert_eq!(middle["succession"]["superseded_by"], "ORG-003");
    assert!(
        lookup(&registry, root, "1287750")
            .unwrap()
            .get("succession")
            .is_none()
    );

    // The registry still lints clean with the chain recorded.
    let lint = run_json(&["registry", "lint", p(&registry), "--emit", "json"], 0);
    assert_eq!(lint["summary"]["errors"], 0, "{lint}");
}

#[test]
fn a_succession_without_a_human_reviewer_or_evidence_is_refused() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = registry(root);
    let mut no_reviewer = line("ORG-001", "ORG-002", "1997-12-31", &[]);
    no_reviewer.as_object_mut().unwrap().remove("reviewer");
    refused_with(
        &registry,
        &write_line(root, "a.jsonl", &no_reviewer),
        "reviewer",
    );
    let mut agent = line("ORG-001", "ORG-002", "1997-12-31", &[]);
    agent["reviewer"] = json!("agent:script");
    refused_with(
        &registry,
        &write_line(root, "b.jsonl", &agent),
        "human reviewer",
    );
    let mut no_evidence = line("ORG-001", "ORG-002", "1997-12-31", &[]);
    no_evidence["evidence_ids"] = json!([]);
    refused_with(
        &registry,
        &write_line(root, "c.jsonl", &no_evidence),
        "evidence id",
    );
    // Nothing was written by any refusal.
    assert!(!registry.join("corrections.jsonl").exists());
}

#[test]
fn a_succession_needs_a_stated_valid_date_kind_and_two_distinct_existing_ids() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = registry(root);
    let mut no_date = line("ORG-001", "ORG-002", "1997-12-31", &[]);
    no_date.as_object_mut().unwrap().remove("effective_date");
    refused_with(
        &registry,
        &write_line(root, "a.jsonl", &no_date),
        "effective_date",
    );
    refused_with(
        &registry,
        &write_line(
            root,
            "b.jsonl",
            &line("ORG-001", "ORG-002", "1997-13-40", &[]),
        ),
        "calendar date",
    );
    let mut kind = line("ORG-001", "ORG-002", "1997-12-31", &[]);
    kind["succession"] = json!("same_entity");
    refused_with(
        &registry,
        &write_line(root, "c.jsonl", &kind),
        "succession must be",
    );
    refused_with(
        &registry,
        &write_line(
            root,
            "d.jsonl",
            &line("ORG-001", "ORG-001", "1997-12-31", &[]),
        ),
        "supersede itself",
    );
    refused_with(
        &registry,
        &write_line(
            root,
            "e.jsonl",
            &line("ORG-001", "ORG-999", "1997-12-31", &[]),
        ),
        "not a canonical id",
    );
    assert!(!registry.join("corrections.jsonl").exists());
}

#[test]
fn an_already_superseded_id_a_cycle_and_a_name_held_by_a_third_id_are_refused() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = registry(root);
    let first = write_line(
        root,
        "first.jsonl",
        &line("ORG-001", "ORG-002", "1997-12-31", &[]),
    );
    correct(&registry, &first, "1.0.0", &[], 0);

    // ORG-001 already has a successor.
    refused_with(
        &registry,
        &write_line(
            root,
            "twice.jsonl",
            &line("ORG-001", "ORG-003", "2001-01-01", &[]),
        ),
        "already superseded",
    );
    // ORG-001 -> ORG-002, then ORG-002 -> ORG-001 would loop.
    refused_with(
        &registry,
        &write_line(
            root,
            "loop.jsonl",
            &line("ORG-002", "ORG-001", "2001-01-01", &[]),
        ),
        "cycle",
    );
    // A name another id holds cannot be moved by this succession.
    refused_with(
        &registry,
        &write_line(
            root,
            "third.jsonl",
            &line(
                "ORG-002",
                "ORG-003",
                "2001-01-01",
                &["Orion Lending Corp", "Delta Holdings Legacy Corp"],
            ),
        ),
        "already names",
    );
    // The refusals left the registry as the first correction made it.
    assert_eq!(
        id_of(&registry, root, "Orion Lending Corp").as_deref(),
        Some("ORG-003")
    );
}

#[test]
fn a_moved_base_refuses_the_succession() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = registry(root);
    let file = write_line(
        root,
        "first.jsonl",
        &line("ORG-001", "ORG-002", "1997-12-31", &[]),
    );
    let refused = correct(
        &registry,
        &file,
        "1.0.0",
        &["--expect-base", "blake3:00"],
        2,
    );
    assert_eq!(
        refused["refusal"]["detail"]["reason"], "base_moved",
        "{refused}"
    );
}
