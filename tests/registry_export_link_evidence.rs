//! `canon registry export --format link-evidence` (bd-vdcw.44): how are these
//! two things linked, answered from the registry directories alone. Every
//! registry here is built by real session commits; the session directories are
//! deleted before the question is asked, so nothing but the registry is read.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
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

fn observation(obs_id: &str, lineage: &str, surface: &str, cik: &str, context: Value) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik},
        "context": context,
        "source": {"channel": "file", "id": "link-fixture", "lineage": lineage}
    })
}

fn related(left: &str, right: &str, relation: &str) -> Value {
    json!({
        "type": "related",
        "left": left,
        "right": right,
        "basis": {"relation": relation, "subject": left},
        "source": {"channel": "fixture", "id": "rel", "lineage": "rel"},
        "asserted_by": "agent:fixture"
    })
}

/// One session directory: init, ingest, optional evidence, solve, commit.
/// Returns the session path so the caller can delete it.
fn commit_session(
    root: &Path,
    label: &str,
    registry: &Path,
    rows: &[Value],
    proof: &[Value],
    version: &str,
) -> PathBuf {
    let session = root.join(format!("{label}-session"));
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "link evidence",
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
    );
    session
}

fn link(registry_args: &[&str], from: &str, to: &str) -> Value {
    let mut args = vec!["registry", "export"];
    args.extend_from_slice(registry_args);
    args.extend_from_slice(&["--format", "link-evidence", "--from", from, "--to", to]);
    run_json(&args, 0)
}

fn closure_hashes(session_export: &Path) -> Vec<String> {
    fs::read_dir(session_export)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| format!("blake3:{}", entry.file_name().to_string_lossy()))
        .collect()
}

#[test]
fn two_funds_linked_through_a_unit_across_two_commits_with_both_sessions_deleted() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");

    // Commit 1 (session one): Fund Alpha's name is an alias of the unit.
    let first = commit_session(
        root,
        "one",
        &registry,
        &[
            observation("u1", "fundA", "Unit One", "10", json!({})),
            observation("u2", "fundB", "Fund Alpha Name", "10", json!({})),
        ],
        &[],
        "0.2.0",
    );

    // Commit 2 (session two, a different directory, a later day): the firm and
    // fund B's adviser spelling (same key), and the unit `part_of` the firm.
    let second = commit_session(
        root,
        "two",
        &registry,
        &[
            observation("f1", "fundA", "Firm Corp", "20", json!({})),
            observation("f2", "fundB", "Adviser Spelling Inc", "20", json!({})),
            observation("u3", "fundA", "Unit One", "10", json!({})),
        ],
        &[related("u3", "f1", "part_of")],
        "0.3.0",
    );

    // Closure hashes the sessions wrote, for reading the answer back against.
    let exported = root.join("closures");
    run_json(
        &[
            "entity",
            "session",
            "export",
            p(&second),
            "--format",
            "evidence-closure",
            "--out",
            p(&exported),
        ],
        0,
    );
    let second_hashes = closure_hashes(&exported);

    // Delete both session directories: the answer comes from the registry alone.
    fs::remove_dir_all(&first).unwrap();
    fs::remove_dir_all(&second).unwrap();

    let answer = link(
        &["--registry", p(&registry)],
        "Fund Alpha Name",
        "Adviser Spelling Inc",
    );
    eprintln!("link-evidence: {answer}");
    let chain = answer["chain"].as_array().expect("a chain");
    assert_eq!(chain.len(), 3, "{chain:?}");
    assert_eq!(chain[0]["hop"], "identity");
    assert_eq!(chain[0]["from"], "Fund Alpha Name");
    assert_eq!(chain[1]["hop"], "relation");
    assert_eq!(chain[1]["relation"], "part_of");
    assert_eq!(chain[2]["hop"], "identity");
    assert_eq!(chain[2]["to"], "Adviser Spelling Inc");
    // The middle hop is never labelled identity, and every hop cites its outcome.
    assert!(chain.iter().filter(|hop| hop["hop"] == "identity").count() == 2);
    for hop in chain {
        assert!(
            hop["outcome_ref"]
                .as_str()
                .unwrap()
                .starts_with("cmdrvl://canon/outcome/")
        );
        assert!(hop["closure_hash"].as_str().unwrap().starts_with("blake3:"));
    }
    // The relation hop's closure is the one the second commit wrote.
    assert!(
        second_hashes.contains(&chain[1]["closure_hash"].as_str().unwrap().to_string()),
        "{:?} in {second_hashes:?}",
        chain[1]["closure_hash"]
    );
    assert_eq!(chain[0]["to"], chain[1]["from"], "hops connect");
    assert_eq!(chain[1]["to"], chain[2]["from"]);

    // Same answer twice.
    assert_eq!(
        answer,
        link(
            &["--registry", p(&registry)],
            "Fund Alpha Name",
            "Adviser Spelling Inc"
        )
    );

    // A key and a canonical id are endpoints too.
    let by_key = link(&["--registry", p(&registry)], "key:cik:10", "key:cik:20");
    assert_eq!(by_key["chain"].as_array().unwrap().len(), 3);
}

#[test]
fn look_alike_names_with_no_committed_chain_report_none() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    commit_session(
        root,
        "alike",
        &registry,
        &[
            observation("a1", "fundA", "BlackRock, Inc.", "300", json!({})),
            observation("a2", "fundB", "Black Rock Coffee Bar", "400", json!({})),
        ],
        &[],
        "0.2.0",
    );
    let answer = link(
        &["--registry", p(&registry)],
        "Black Rock Coffee Bar",
        "BlackRock, Inc.",
    );
    assert!(answer["chain"].is_null(), "{answer}");
    assert_eq!(answer["reason"], "no_committed_chain");
    // A name nobody committed is unresolved, not guessed.
    let unknown = link(
        &["--registry", p(&registry)],
        "Black Rock",
        "BlackRock, Inc.",
    );
    assert!(unknown["chain"].is_null());
    assert_eq!(unknown["unresolved"], json!(["from"]));
}

#[test]
fn across_registries_only_a_relation_crosses_and_one_registry_names_registries() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let organisations = root.join("organisations");
    let deals = root.join("deals");

    // A deals registry holds the canonical id the organisations registry's
    // sponsor relation names, with an alias spelling.
    run(
        &[
            "registry",
            "init",
            "--registry",
            p(&deals),
            "--registry-id",
            "deals",
            "--prefix",
            "D",
            "--zero-pad",
            "1",
        ],
        0,
    );
    run(
        &[
            "registry",
            "mint",
            "--registry",
            p(&deals),
            "--canonical-id",
            "D-1",
            "--canonical-type",
            "deal",
            "--with-alias",
            "aliases.json=Deal One:MANUAL",
            "--create-alias-file",
            "--next-version",
            "0.2.0",
        ],
        0,
    );

    // The organisations registry: a sponsor of deal D-1 (a role relation).
    commit_session(
        root,
        "orgs",
        &organisations,
        &[observation(
            "s1",
            "fundA",
            "Sponsor Capital",
            "500",
            json!({"role": "sponsor", "deal_id": "D-1", "as_of_date": "2025-01-01"}),
        )],
        &[],
        "0.2.0",
    );

    let map = root.join("registries.yaml");
    fs::write(
        &map,
        format!(
            "version: canon.session.registries.v1\nkinds:\n  deal:\n    registry: {}\n    registry_id: deals\n    prefix: D\n    zero_pad: 1\n    canonical_type: deal\n    alias_file: aliases.json\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: aliases.json\n",
            deals.display(),
            organisations.display()
        ),
    )
    .unwrap();

    let across = link(&["--registries", p(&map)], "Sponsor Capital", "Deal One");
    eprintln!("across: {across}");
    let chain = across["chain"]
        .as_array()
        .expect("a chain across registries");
    assert_eq!(chain.len(), 3, "{chain:?}");
    assert_eq!(chain[0]["registry"], "organisations");
    assert_eq!(chain[1]["hop"], "relation");
    assert_eq!(chain[1]["relation"], "sponsor_of");
    assert_eq!(chain[1]["registry"], "organisations");
    assert_eq!(chain[1]["to"], "deals:D-1");
    assert_eq!(chain[2]["hop"], "identity");
    assert_eq!(chain[2]["registry"], "deals");
    assert!(
        chain[1]["closure_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:")
    );
    // The deals entry predates sessions: no closure, said so, never invented.
    assert!(chain[2]["closure_hash"].is_null());
    assert_eq!(chain[2]["reason"], "no_session_authority");

    // The organisations registry alone cannot answer, and says what to use.
    let alone = link(
        &["--registry", p(&organisations)],
        "Sponsor Capital",
        "Deal One",
    );
    assert!(alone["chain"].is_null());
    assert_eq!(alone["reason"], "no_committed_chain");
    assert!(
        alone["note"].as_str().unwrap().contains("--registries"),
        "{alone}"
    );
}

#[test]
fn the_summary_names_each_hop_and_nothing_else() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    commit_session(
        root,
        "sum",
        &registry,
        &[
            observation("a1", "fundA", "Omega One", "600", json!({})),
            observation("a2", "fundB", "Omega Uno", "600", json!({})),
        ],
        &[],
        "0.2.0",
    );
    let output = run(
        &[
            "registry",
            "export",
            "--registry",
            p(&registry),
            "--format",
            "link-evidence",
            "--from",
            "Omega One",
            "--to",
            "Omega Uno",
            "--emit",
            "summary",
        ],
        0,
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("2 hop(s)"), "{text}");
    assert_eq!(text.lines().count(), 3);
}
