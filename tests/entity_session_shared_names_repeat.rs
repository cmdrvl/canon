//! bd-rgjo stage 1 defect: with `standalone.split_shared_names` a repeat or a
//! second session over data already committed must report `nothing_to_commit`
//! with nothing attached and the registry bytes unchanged. A commit that only
//! clears stale pending records is a real write and says so once; after that it
//! is quiet. Attribute-bearing rows, the `authoritative: [cik]` rules form and a
//! registry produced by earlier commits. Invented names.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const SHARED: &str = "Delta Holdings Corp";
const OTHER: &str = "Orion Lending Corp";

fn rules(split: bool) -> String {
    format!(
        "version: canon.entity.session.rules.v1\nkeys:\n  authoritative: [cik]\ntier_a:\n  evidence_types: [same_key, asserted_alias]\ntier_b:\n  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]\n  min_distinct_evidence_types: 2\n  min_distinct_lineages: 2\nstandalone:\n  mint_when_authoritative_key: true\n{}incumbent_attach:\n  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]\n",
        if split {
            "  split_shared_names: true\n"
        } else {
            ""
        }
    )
}

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

fn row(obs_id: &str, lineage: &str, surface: &str, cik: &str) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "fund",
        "surface": surface,
        "keys": {"cik": cik},
        "attrs": {"filing_types": "N-54C", "first_filed": "1998-01-05", "last_filed": "2005-08-23"},
        "source": {"channel": "file", "id": "repeat-fixture", "lineage": lineage}
    })
}

fn rows() -> Vec<Value> {
    vec![
        row("a1", "cik:0000003845", SHARED, "0000003845"),
        row("b1", "cik:0000003906", SHARED, "0000003906"),
        row("c1", "cik:0001287750", OTHER, "0001287750"),
    ]
}

/// Every file of the registry directory, in order, as one comparable value.
fn registry_bytes(registry: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(registry, registry, &mut out);
    out
}

struct Batch {
    session: PathBuf,
    rules: PathBuf,
}

fn batch(root: &Path, registry: &Path, label: &str, split: bool) -> Batch {
    let dir = root.join(label);
    fs::create_dir_all(&dir).unwrap();
    let rules_path = dir.join("rules.yaml");
    fs::write(&rules_path, rules(split)).unwrap();
    let map = dir.join("registries.yaml");
    fs::write(
        &map,
        format!(
            "version: canon.session.registries.v1\nkinds:\n  fund:\n    registry: {}\n    registry_id: funds\n    id_scheme: agent\n    kind_prefix: fund\n    canonical_type: fund\n    alias_file: aliases.json\n",
            registry.display()
        ),
    )
    .unwrap();
    let session = dir.join("session");
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            label,
            "--work-dir",
            p(&session),
            "--registries",
            p(&map),
        ],
        0,
    );
    let obs = dir.join("obs.jsonl");
    let text = rows()
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&obs, format!("{text}\n")).unwrap();
    run_json(&["entity", "session", "ingest", p(&session), p(&obs)], 0);
    Batch {
        session,
        rules: rules_path,
    }
}

impl Batch {
    fn solve(&self) -> Value {
        run_json(
            &[
                "entity",
                "session",
                "solve",
                p(&self.session),
                "--rules",
                p(&self.rules),
            ],
            0,
        )
    }

    fn commit(&self, version: &str) -> Value {
        let output = canon()
            .args([
                "entity",
                "session",
                "commit",
                p(&self.session),
                "--next-version",
                version,
                "--emit",
                "json",
            ])
            .output()
            .unwrap();
        serde_json::from_slice(&output.stdout).expect("commit json")
    }
}

#[test]
fn a_repeat_and_a_second_session_after_a_flag_on_commit_write_and_attach_nothing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("funds");

    // Day one without the rule: the shared name blocks both CIKs, is parked as
    // pending memory, and only the unrelated entity mints.
    let day_one = batch(root, &registry, "day-one", false);
    day_one.solve();
    let first = day_one.commit("1.0.0");
    assert_eq!(first["outcome"], "committed", "{first}");
    assert_eq!(first["minted"].as_array().unwrap().len(), 1, "{first}");
    let pending = fs::read_to_string(registry.join("_escrow/pending.jsonl")).unwrap_or_default();
    assert!(
        pending.contains(SHARED),
        "stale pending expected: {pending}"
    );

    // The rule turned on: both CIKs mint, the stale pending record is cleared.
    let on = batch(root, &registry, "flag-on", true);
    on.solve();
    let second = on.commit("1.1.0");
    assert_eq!(second["outcome"], "committed", "{second}");
    assert_eq!(second["minted"].as_array().unwrap().len(), 2, "{second}");
    assert_eq!(second["pending_cleared"], 1, "{second}");
    let pending = fs::read_to_string(registry.join("_escrow/pending.jsonl")).unwrap_or_default();
    assert!(
        !pending.contains(SHARED),
        "the stale record must be gone: {pending}"
    );
    // A second session over the same data: nothing to commit, nothing attached,
    // registry bytes unchanged.
    let before = registry_bytes(&registry);
    let again = batch(root, &registry, "flag-on-again", true);
    again.solve();
    let third = again.commit("1.2.0");
    assert_eq!(third["outcome"], "nothing_to_commit", "{third}");
    assert_eq!(third["attached"].as_array().unwrap().len(), 0, "{third}");
    assert_eq!(registry_bytes(&registry), before);

    // The same session again: still nothing, still unchanged.
    again.solve();
    let repeat = again.commit("1.2.0");
    assert_ne!(repeat["outcome"], "committed", "{repeat}");
    assert_eq!(repeat["attached"].as_array().unwrap().len(), 0, "{repeat}");
    assert_eq!(registry_bytes(&registry), before);
}
