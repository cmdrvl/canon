//! bd-rgjo stage 1: one name under two different CIKs. With
//! `standalone.split_shared_names` each CIK mints its own id, the shared name is
//! an alias of neither, and the run reports it; without the rule, and for one
//! entity carrying two CIKs, the key conflict still blocks. Invented names only.

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
        r#"version: canon.entity.session.rules.v1
keys:
  cik: {{authoritative: true, single_valued: true}}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
{}incumbent_attach:
  name_equivalent_steps: [casefold, strip_punctuation, collapse_whitespace]
"#,
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

fn observation(obs_id: &str, lineage: &str, surface: &str, cik: &str) -> Value {
    json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": {"cik": cik},
        "source": {"channel": "file", "id": "shared-fixture", "lineage": lineage}
    })
}

struct Case {
    _temp: TempDir,
    root: PathBuf,
    registry: PathBuf,
    session: PathBuf,
    rules: PathBuf,
}

fn case(split: bool) -> Case {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
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
    run(
        &[
            "registry",
            "mint",
            "--registry",
            p(&registry),
            "--canonical-id",
            "ORG-001",
            "--canonical-type",
            "firm",
            "--with-alias",
            "00-firms.json=Seed Holdings Ltd:SEED",
            "--create-alias-file",
            "--bump",
            "patch",
            "--no-lint",
        ],
        0,
    );
    let rules_path = root.join("rules.yaml");
    fs::write(&rules_path, rules(split)).unwrap();
    let map = root.join("registries.yaml");
    fs::write(
        &map,
        format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: organisations\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: 30-session-aliases.json\n    key_file: 40-keys.json\n",
            registry.display()
        ),
    )
    .unwrap();
    let session = root.join("session");
    run_json(
        &[
            "entity",
            "session",
            "init",
            "--goal",
            "shared-names",
            "--work-dir",
            p(&session),
            "--registries",
            p(&map),
        ],
        0,
    );
    Case {
        _temp: temp,
        root,
        registry,
        session,
        rules: rules_path,
    }
}

impl Case {
    fn ingest(&self, rows: &[Value]) {
        let path = self.root.join("obs.jsonl");
        let text = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{text}\n")).unwrap();
        run_json(
            &["entity", "session", "ingest", p(&self.session), p(&path)],
            0,
        );
    }

    fn assert_lines(&self, lines: &[Value]) {
        let path = self.root.join("assert.jsonl");
        let text = lines
            .iter()
            .map(|line| serde_json::to_string(line).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{text}\n")).unwrap();
        run_json(
            &["entity", "session", "assert", p(&self.session), p(&path)],
            0,
        );
    }

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

    fn commit(&self, expected_exit: i32) -> Value {
        run_json(
            &[
                "entity",
                "session",
                "commit",
                p(&self.session),
                "--next-version",
                "0.9.0",
            ],
            expected_exit,
        )
    }

    fn lookup(&self, name: &str) -> Option<String> {
        let input = self.root.join("lookup.csv");
        fs::write(&input, format!("name\n\"{name}\"\n")).unwrap();
        let output = canon()
            .arg(&input)
            .arg("--registry")
            .arg(&self.registry)
            .arg("--column")
            .arg("name")
            .args(["--emit", "json", "--explicit", "--plain-json-values"])
            .output()
            .unwrap();
        let payload: Value = serde_json::from_slice(&output.stdout).expect("lookup json");
        payload["mappings"][0]["canonical_id"]
            .as_str()
            .map(str::to_string)
    }
}

fn three_rows() -> Vec<Value> {
    vec![
        observation("a1", "fundA", SHARED, "0000003845"),
        observation("b1", "fundB", SHARED, "0000003906"),
        observation("c1", "fundC", OTHER, "0001287750"),
    ]
}

#[test]
fn a_name_under_two_ciks_mints_each_cik_and_is_an_alias_of_neither() {
    let case = case(true);
    case.ingest(&three_rows());
    let solved = case.solve();
    assert_eq!(solved["summary"]["contradiction"], 0, "{solved}");
    assert_eq!(solved["summary"]["accepted_tier_a"], 3, "{solved}");
    let shared = solved["shared_names"].as_array().expect("shared_names");
    assert_eq!(shared.len(), 1, "{solved}");
    assert_eq!(shared[0]["surface"], SHARED);
    let values = shared[0]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["value"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(values, vec!["3845", "3906"], "{solved}");

    let committed = case.commit(0);
    assert_eq!(
        committed["minted"].as_array().unwrap().len(),
        3,
        "{committed}"
    );
    let members = committed["shared_names"][0]["members"].as_array().unwrap();
    let ids = members
        .iter()
        .map(|member| member["canonical_id"].as_str().expect("id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "each CIK has its own id: {committed}");

    // Each CIK resolves to its own id; the shared name resolves to neither.
    assert_eq!(case.lookup("3845").as_deref(), Some(ids[0].as_str()));
    assert_eq!(case.lookup("3906").as_deref(), Some(ids[1].as_str()));
    assert_eq!(case.lookup(SHARED), None);
    // The unrelated name is unaffected: it is an ordinary alias of its own id.
    assert!(case.lookup(OTHER).is_some());
}

#[test]
fn without_the_rule_the_same_rows_still_block_both() {
    let case = case(false);
    case.ingest(&three_rows());
    let solved = case.solve();
    assert!(solved.get("shared_names").is_none(), "{solved}");
    assert_eq!(solved["summary"]["contradiction"], 1, "{solved}");
    assert_eq!(solved["contradictions"][0]["reason"], "key_conflict");
    let committed = case.commit(0);
    assert_eq!(
        committed["minted"].as_array().unwrap().len(),
        1,
        "only the unrelated entity mints: {committed}"
    );
    assert_eq!(case.lookup(SHARED), None);
}

#[test]
fn one_entity_asserted_to_carry_two_ciks_is_still_a_key_conflict() {
    let case = case(true);
    case.ingest(&three_rows());
    // A cited assertion that the two rows are the same entity.
    case.assert_lines(&[json!({
        "type": "asserted_alias",
        "left": "a1",
        "right": "b1",
        "basis": {"explanation": "a reviewer states the two rows are one entity"},
        "source": {
            "channel": "filing",
            "id": "shared-fixture-filing",
            "locator": "filing:shared-fixture-filing#cover",
            "content_hash": format!("blake3:{}", "d".repeat(64))
        },
        "asserted_by": "human:analyst"
    })]);
    let solved = case.solve();
    let reasons = solved["contradictions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["reason"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(
        reasons.contains(&"key_conflict".to_string()),
        "the joined pair must still conflict: {solved}"
    );
}

#[test]
fn rows_without_the_key_do_not_split_a_name() {
    // One keyed row and one keyless row of the same name: no second key, no split.
    let case = case(true);
    case.ingest(&[
        observation("a1", "fundA", SHARED, "0000003845"),
        json!({
            "obs_id": "k1",
            "entity_type": "organisation",
            "surface": SHARED,
            "keys": {},
            "source": {"channel": "file", "id": "shared-fixture", "lineage": "fundD"}
        }),
    ]);
    let solved = case.solve();
    assert!(solved.get("shared_names").is_none(), "{solved}");
}

#[test]
fn the_rule_key_must_be_boolean() {
    let case = case(true);
    fs::write(
        &case.rules,
        rules(true).replace("split_shared_names: true", "split_shared_names: sometimes"),
    )
    .unwrap();
    let output = canon()
        .args(["entity", "session", "rules", "validate", p(&case.rules)])
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0));
}
