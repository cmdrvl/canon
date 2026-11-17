//! `canon entity session export --format evidence-closure` (bd-vdcw.44): the
//! evidence behind each committed outcome as deterministic local files. Every
//! flow runs the real binary on temp sessions; nothing is sealed, stored or
//! uploaded here. The one `pack seal` case runs only when `pack` is on PATH and
//! reports SKIPPED by name otherwise.

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

fn path_str(path: &Path) -> &str {
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

fn observation(obs_id: &str, lineage: &str, surface: &str, cik: Option<&str>) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": cik.map(|value| json!({"cik": value})).unwrap_or(json!({})),
        "source": {"channel": "file", "id": "closure-fixture", "lineage": lineage}
    })
}

fn evidence(kind: &str, left: &str, right: &str, lineage: &str, basis: Value) -> Value {
    json!({
        "type": kind,
        "left": left,
        "right": right,
        "basis": basis,
        "source": {"channel": "fixture", "id": lineage, "lineage": lineage},
        "asserted_by": "agent:fixture"
    })
}

/// A session over a fresh registry with: a keyed firm (mint plus key alias), a
/// second keyed firm, a `related` record between the two, and a reviewed
/// `distinct` record between two un-merged names.
struct Fixture {
    _temp: TempDir,
    registry: PathBuf,
    session: PathBuf,
    commit: Value,
}

fn build(label: &str) -> Fixture {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join(format!("{label}-registry"));
    let session = root.join(format!("{label}-session"));
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "closure export",
            "--registry",
            path_str(&registry),
            "--work-dir",
            path_str(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    let rows = [
        observation("o1", "fundA", "Alpha Holdings", Some("100")),
        observation("o2", "fundB", "Beta Partners", Some("200")),
        observation("o3", "fundA", "Gamma Credit", None),
        observation("o4", "fundB", "Gamma Credit LLC", None),
    ];
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            path_str(&session),
            path_str(&write_jsonl(root, "obs.jsonl", &rows)),
        ],
        0,
    );
    let proof = [
        evidence(
            "related",
            "o1",
            "o2",
            "rel",
            json!({"relation": "part_of", "subject": "o1"}),
        ),
        evidence(
            "distinct",
            "o3",
            "o4",
            "review",
            json!({"authority": "reviewed"}),
        ),
    ];
    run_json(
        &[
            "entity",
            "session",
            "assert",
            path_str(&session),
            path_str(&write_jsonl(root, "ev.jsonl", &proof)),
        ],
        0,
    );
    run_json(
        &[
            "entity",
            "session",
            "solve",
            path_str(&session),
            "--rules",
            path_str(&rules),
        ],
        0,
    );
    let commit = run_json(
        &[
            "entity",
            "session",
            "commit",
            path_str(&session),
            "--next-version",
            "0.2.0",
        ],
        0,
    );
    Fixture {
        _temp: temp,
        registry,
        session,
        commit,
    }
}

fn read_tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let child = entry.unwrap().path();
            if child.is_dir() {
                visit(root, &child, out);
            } else {
                out.insert(
                    child
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&child).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(dir, dir, &mut out);
    out
}

fn blake3_of(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn export(session: &Path, out: &Path, extra: &[&str]) -> Value {
    let mut args = vec![
        "entity",
        "session",
        "export",
        path_str(session),
        "--format",
        "evidence-closure",
        "--out",
        path_str(out),
    ];
    args.extend_from_slice(extra);
    run_json(&args, 0)
}

#[test]
fn one_closure_per_outcome_hashes_match_the_registry_and_files_are_complete() {
    let fixture = build("first");
    eprintln!("commit receipt: {}", fixture.commit);
    let out = TempDir::new().unwrap();
    let result = export(&fixture.session, out.path(), &[]);
    eprintln!("export: {result}");
    let closures = result["closures"].as_array().unwrap();

    // Outcome kinds written: minted_id, key_alias, relation, cannot_link.
    let kinds = closures
        .iter()
        .map(|closure| {
            let text = fs::read_to_string(
                out.path()
                    .join(closure["directory"].as_str().unwrap())
                    .join("closure.json"),
            )
            .unwrap();
            let manifest: Value = serde_json::from_str(&text).unwrap();
            manifest["kind"].as_str().unwrap().to_string()
        })
        .collect::<Vec<_>>();
    for kind in ["minted_id", "relation", "cannot_link"] {
        assert!(kinds.contains(&kind.to_string()), "{kind} in {kinds:?}");
    }

    // The blake3 of every closure.json equals its closure_hash, the directory
    // is its 64 hex characters, and the hash is the one on the registry record.
    let aliases: Value =
        serde_json::from_slice(&fs::read(fixture.registry.join("aliases.json")).unwrap()).unwrap();
    let relations = fs::read_to_string(fixture.registry.join("relations.jsonl")).unwrap();
    let cannot_link =
        fs::read_to_string(fixture.registry.join("_escrow/cannot_link.jsonl")).unwrap();
    let registry_hashes = aliases
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["session_authority"]["closure_hash"].as_str())
        .map(str::to_string)
        .chain(relations.lines().filter_map(|line| {
            serde_json::from_str::<Value>(line).ok().and_then(|fact| {
                fact["provenance"]["closure_hash"]
                    .as_str()
                    .map(str::to_string)
            })
        }))
        .chain(cannot_link.lines().filter_map(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|fact| fact["closure_hash"].as_str().map(str::to_string))
        }))
        .collect::<std::collections::BTreeSet<_>>();
    for closure in closures {
        let hash = closure["closure_hash"].as_str().unwrap();
        let directory = closure["directory"].as_str().unwrap();
        assert_eq!(directory, hash.strip_prefix("blake3:").unwrap());
        assert_eq!(directory.len(), 64);
        let closure_json = fs::read(out.path().join(directory).join("closure.json")).unwrap();
        assert_eq!(
            blake3_of(&closure_json),
            hash,
            "closure.json hashes to closure_hash"
        );
        assert!(
            registry_hashes.contains(hash),
            "{hash} is on a registry entry, relation line or cannot-link record: {registry_hashes:?}"
        );

        // Every obs_id and evidence id in the manifest is in its files.
        let manifest: Value = serde_json::from_slice(&closure_json).unwrap();
        let observation_lines =
            fs::read_to_string(out.path().join(directory).join("observations.jsonl"))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>();
        for entry in manifest["observations"].as_array().unwrap() {
            if let Some(id) = entry["obs_id"].as_str() {
                assert!(
                    observation_lines.iter().any(|line| line["obs_id"] == id),
                    "{id}"
                );
            }
        }
        let evidence_text =
            fs::read_to_string(out.path().join(directory).join("evidence.jsonl")).unwrap();
        for id in manifest["evidence_ids"].as_array().unwrap() {
            assert!(evidence_text.contains(id.as_str().unwrap()), "{id}");
        }
        assert!(out.path().join(directory).join("rules.yaml").is_file());
        // A plain-file observation carries none of the warehouse handles.
        for line in &observation_lines {
            for handle in [
                "catalog_key",
                "columns",
                "row_hash",
                "artifact_id",
                "receipt_digest",
            ] {
                assert!(line["source_handles"][handle].is_null(), "{handle}: {line}");
            }
        }
        // And its sensitivity is restricted with the unstated input counted.
        assert_eq!(closure["sensitivity"]["level"], "restricted");
        assert_eq!(
            closure["sensitivity"]["unstated_inputs"].as_u64().unwrap() as usize,
            observation_lines.len()
        );
    }

    // commit.json is the stored closure set: its blake3 is the closure_set_hash
    // of the commit receipt, and it lists exactly the closure directories.
    let commit_json = fs::read(out.path().join("commit.json")).unwrap();
    assert_eq!(
        blake3_of(&commit_json),
        fixture.commit["closure_set_hash"].as_str().unwrap()
    );
    let listed = serde_json::from_slice::<Value>(&commit_json).unwrap()["closures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            entry["closure_hash"]
                .as_str()
                .unwrap()
                .strip_prefix("blake3:")
                .unwrap()
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    let written = closures
        .iter()
        .map(|closure| closure["directory"].as_str().unwrap().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(listed, written);
    // The hand-off receipt names the same hashes.
    let receipt_hashes = fixture.commit["registries"][0]["closures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|closure| closure["closure_hash"].as_str().unwrap().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    for closure in closures {
        assert!(receipt_hashes.contains(closure["closure_hash"].as_str().unwrap()));
    }

    // Exporting twice gives byte-identical directories.
    let again = TempDir::new().unwrap();
    export(&fixture.session, again.path(), &[]);
    assert_eq!(read_tree(out.path()), read_tree(again.path()));

    // `--outcome` writes exactly that one closure and no commit.json.
    let one = &closures[0];
    let single = TempDir::new().unwrap();
    let only = export(
        &fixture.session,
        single.path(),
        &["--outcome", one["outcome_ref"].as_str().unwrap()],
    );
    assert_eq!(only["closures"].as_array().unwrap().len(), 1);
    let tree = read_tree(single.path());
    assert!(
        tree.keys()
            .all(|path| path.starts_with(one["directory"].as_str().unwrap()))
    );
    assert!(!tree.contains_key("commit.json"));
    assert_eq!(
        tree[&format!("{}/closure.json", one["directory"].as_str().unwrap())],
        read_tree(out.path())[&format!("{}/closure.json", one["directory"].as_str().unwrap())]
    );
}

#[test]
fn a_second_session_built_from_the_same_inputs_exports_the_same_bytes() {
    let first = build("one");
    let second = build("two");
    let first_out = TempDir::new().unwrap();
    let second_out = TempDir::new().unwrap();
    export(&first.session, first_out.path(), &[]);
    export(&second.session, second_out.path(), &[]);
    let strip = |mut tree: BTreeMap<String, Vec<u8>>| {
        // commit.json carries the session id (a function of the directory); the
        // closure directories do not.
        tree.remove("commit.json");
        tree
    };
    assert_eq!(
        strip(read_tree(first_out.path())),
        strip(read_tree(second_out.path()))
    );
}

#[test]
fn one_public_and_one_restricted_input_make_the_closure_restricted() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    let session = root.join("session");
    let rules = root.join("rules.yaml");
    fs::write(&rules, RULES).unwrap();
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "sensitivity",
            "--registry",
            path_str(&registry),
            "--work-dir",
            path_str(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    let mut public = observation("p1", "fundA", "Delta Corp", Some("300"));
    public["source"]["sensitivity"] = json!("public");
    let mut restricted = observation("p2", "fundB", "Delta Corporation", Some("300"));
    restricted["source"]["sensitivity"] = json!("test_restricted");
    run_json(
        &[
            "entity",
            "session",
            "ingest",
            path_str(&session),
            path_str(&write_jsonl(root, "obs.jsonl", &[public, restricted])),
        ],
        0,
    );
    run_json(
        &[
            "entity",
            "session",
            "solve",
            path_str(&session),
            "--rules",
            path_str(&rules),
        ],
        0,
    );
    run_json(
        &[
            "entity",
            "session",
            "commit",
            path_str(&session),
            "--next-version",
            "0.2.0",
        ],
        0,
    );
    let out = TempDir::new().unwrap();
    let result = export(&session, out.path(), &[]);
    let minted = result["closures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|closure| {
            closure["outcome_ref"]
                .as_str()
                .unwrap()
                .contains("/minted_id/")
        })
        .expect("minted closure");
    assert_eq!(minted["sensitivity"]["level"], "restricted");
    assert_eq!(minted["sensitivity"]["unstated_inputs"], 0);
    assert_eq!(
        minted["sensitivity"]["values"],
        json!(["public", "test_restricted"])
    );
}

#[test]
fn export_without_a_commit_refuses_and_names_commit() {
    let temp = TempDir::new().unwrap();
    let session = temp.path().join("session");
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "no commit",
            "--registry",
            path_str(&temp.path().join("registry")),
            "--work-dir",
            path_str(&session),
            "--registry-id",
            "organisations",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ],
        0,
    );
    let out = TempDir::new().unwrap();
    let refusal = run_json(
        &[
            "entity",
            "session",
            "export",
            path_str(&session),
            "--format",
            "evidence-closure",
            "--out",
            path_str(out.path()),
        ],
        2,
    );
    assert!(
        refusal["refusal"]["next_command"]
            .as_str()
            .unwrap()
            .contains("canon entity session commit"),
        "{refusal}"
    );
    assert!(read_tree(out.path()).is_empty(), "nothing written");
}

#[test]
fn pack_seal_accepts_the_commit_directory_and_one_closure_directory() {
    if std::process::Command::new("pack")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!(
            "SKIPPED: pack_seal_accepts_the_commit_directory_and_one_closure_directory (pack is not on PATH)"
        );
        return;
    }
    let fixture = build("seal");
    let out = TempDir::new().unwrap();
    let result = export(&fixture.session, out.path(), &[]);
    let one = result["closures"][0]["directory"].as_str().unwrap();
    for target in [out.path().to_path_buf(), out.path().join(one)] {
        let sealed = TempDir::new().unwrap();
        let status = std::process::Command::new("pack")
            .args(["seal", "--no-witness", "--created", "2026-01-01T00:00:00Z"])
            .arg(&target)
            .arg("--output")
            .arg(sealed.path().join("pack"))
            .status()
            .expect("pack runs");
        assert!(status.success(), "pack seal {target:?}");
    }
}
