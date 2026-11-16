#![forbid(unsafe_code)]
//! bd-vdcw.28: keys reach the registry. Every flow goes through the public CLI.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path, path::PathBuf};
use tempfile::TempDir;

const RULES: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  lei: {authoritative: true, single_valued: true}
  ticker: {authoritative: false, single_valued: false}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
"#;

const RULES_NO_MINT: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: false
"#;

const RULES_WITH_TICKER_COMMIT: &str = r#"version: canon.entity.session.rules.v1
keys:
  cik: {authoritative: true, single_valued: true}
  ticker: {authoritative: false, single_valued: false}
tier_a:
  evidence_types: [same_key, asserted_alias]
tier_b:
  eligible_evidence_types: [same_slot, attribute_agreement, name_equivalent]
  min_distinct_evidence_types: 2
  min_distinct_lineages: 2
standalone:
  mint_when_authoritative_key: true
commit:
  key_namespaces: [ticker]
"#;

const LEI_ZETA: &str = "CANONFIXT0000000BB37";

fn canon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

fn stdout_json(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|_| panic!("read {}", path.display())))
        .expect("JSON file")
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).expect("write fixture");
}

fn observation(obs_id: &str, surface: &str, keys: Value, lineage: &str) -> String {
    serde_json::to_string(&json!({
        "obs_id": obs_id,
        "entity_type": "organisation",
        "surface": surface,
        "keys": keys,
        "source": {"channel": "file", "id": "key-registry", "lineage": lineage}
    }))
    .unwrap()
}

/// A registry written as files, so a case can start from known incumbents.
fn seed_registry(dir: &Path, files: &[(&str, Value)], entry_count: usize) {
    fs::create_dir_all(dir).unwrap();
    write(
        &dir.join("registry.json"),
        &serde_json::to_string_pretty(&json!({
            "id": "firms",
            "version": "1.0.0",
            "description": "key registry fixture",
            "updated": "2026-11-16",
            "entry_count": entry_count,
            "default_id_scheme": {"prefix": "ORG", "zero_pad": 3}
        }))
        .unwrap(),
    );
    for (name, entries) in files {
        write(
            &dir.join(name),
            &serde_json::to_string_pretty(entries).unwrap(),
        );
    }
}

fn alias(input: &str, canonical_id: &str, rule_id: &str) -> Value {
    json!({
        "input": input,
        "canonical_id": canonical_id,
        "canonical_type": "firm",
        "rule_id": rule_id
    })
}

fn registry_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

struct Session {
    dir: PathBuf,
}

fn init_legacy(root: &Path, name: &str, registry: &Path) -> Session {
    let dir = root.join(name);
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "keys in the registry",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            dir.to_str().unwrap(),
            "--registry-id",
            "firms",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();
    Session { dir }
}

fn ingest_solve(session: &Session, root: &Path, rows: &[String], rules: &str) -> Value {
    let observations = root.join(format!(
        "{}-observations.jsonl",
        session.dir.file_name().unwrap().to_string_lossy()
    ));
    write(&observations, &format!("{}\n", rows.join("\n")));
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.dir.to_str().unwrap(),
            observations.to_str().unwrap(),
        ])
        .assert()
        .success();
    let rules_path = root.join("rules.yaml");
    write(&rules_path, rules);
    let solve = canon()
        .args([
            "entity",
            "session",
            "solve",
            session.dir.to_str().unwrap(),
            "--rules",
            rules_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    stdout_json(&solve)
}

fn commit(session: &Session, version: &str) -> assert_cmd::assert::Assert {
    canon()
        .args([
            "entity",
            "session",
            "commit",
            session.dir.to_str().unwrap(),
            "--next-version",
            version,
        ])
        .assert()
}

/// Plain exact lookup of one value; returns the canonical id or None.
fn lookup(root: &Path, registry: &Path, value: &str) -> Option<String> {
    let rows = root.join("lookup-rows.csv");
    write(&rows, &format!("value\n{value}\n"));
    let output = canon()
        .args([
            rows.to_str().unwrap(),
            "--registry",
            registry.to_str().unwrap(),
            "--column",
            "value",
            "--explicit",
            "--plain-json-values",
            "--no-witness",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).expect("lookup JSON");
    json["mappings"]
        .get(0)
        .and_then(|mapping| mapping["canonical_id"].as_str())
        .map(str::to_string)
}

fn export_crosswalk(registry: &Path, out: &Path) -> Vec<Vec<String>> {
    canon()
        .args([
            "registry",
            "export",
            "--registry",
            registry.to_str().unwrap(),
            "--format",
            "key-crosswalk",
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    fs::read_to_string(out)
        .unwrap()
        .lines()
        .map(|line| line.split(',').map(str::to_string).collect())
        .collect()
}

fn id_for_surface(commit: &Value, surface: &str) -> String {
    commit["minted"]
        .as_array()
        .expect("minted")
        .iter()
        .find(|minted| {
            minted["aliases"]
                .as_array()
                .expect("aliases")
                .iter()
                .any(|alias| alias["input"] == surface)
        })
        .unwrap_or_else(|| panic!("no minted entry for {surface}: {commit}"))["canonical_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn committed_keys_resolve_by_plain_lookup_export_as_a_crosswalk_and_attach_later_spellings() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    canon()
        .args([
            "registry",
            "init",
            "--registry",
            registry.to_str().unwrap(),
            "--registry-id",
            "firms",
            "--prefix",
            "ORG",
            "--zero-pad",
            "3",
        ])
        .assert()
        .success();

    let first = init_legacy(root, "first", &registry);
    ingest_solve(
        &first,
        root,
        &[
            observation("z5", "Zeta Corp", json!({"cik": "222"}), "lineage-a"),
            observation(
                "z6",
                "Zeta Corporation",
                json!({"lei": LEI_ZETA}),
                "lineage-b",
            ),
            observation(
                "z7",
                "ZC Inc",
                json!({"cik": "222", "lei": LEI_ZETA}),
                "lineage-c",
            ),
            observation(
                "a1",
                "Acme Holdings Inc",
                json!({"cik": "111"}),
                "lineage-d",
            ),
        ],
        RULES,
    );
    let committed = stdout_json(&commit(&first, "0.1.0").success());
    assert_eq!(committed["outcome"], "committed", "{committed}");
    let zeta = id_for_surface(&committed, "Zeta Corp");
    let acme = id_for_surface(&committed, "Acme Holdings Inc");
    assert_ne!(zeta, acme);

    // Keys got their own files, with the namespace on the entry.
    for (file, rule) in [
        ("keys.cik.json", "SESSION_KEY_CIK"),
        ("keys.lei.json", "SESSION_KEY_LEI"),
    ] {
        let entries = read_json(&registry.join(file));
        assert!(!entries.as_array().unwrap().is_empty(), "{file}");
        for entry in entries.as_array().unwrap() {
            assert_eq!(entry["rule_id"], rule);
            assert_eq!(entry["namespace"]["class"], "external_global_id");
        }
    }
    let names = read_json(&registry.join("aliases.json"));
    assert!(
        names
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| !entry["rule_id"]
                .as_str()
                .unwrap()
                .starts_with("SESSION_KEY_")),
        "no key value is filed with the spellings"
    );

    // A CSV of CIKs resolves to the canonical id.
    assert_eq!(
        lookup(root, &registry, "222").as_deref(),
        Some(zeta.as_str())
    );
    assert_eq!(
        lookup(root, &registry, "111").as_deref(),
        Some(acme.as_str())
    );
    assert_eq!(
        lookup(root, &registry, LEI_ZETA).as_deref(),
        Some(zeta.as_str())
    );

    // The crosswalk row for the Zeta id shows the CIK and the LEI together.
    let table = export_crosswalk(&registry, &root.join("crosswalk.csv"));
    assert_eq!(
        table[0],
        [
            "canonical_id",
            "canonical_type",
            "primary_name",
            "cik",
            "lei"
        ]
    );
    let zeta_row = table.iter().find(|row| row[0] == zeta).expect("zeta row");
    assert_eq!(
        (zeta_row[3].as_str(), zeta_row[4].as_str()),
        ("222", LEI_ZETA)
    );
    let acme_row = table.iter().find(|row| row[0] == acme).expect("acme row");
    assert_eq!((acme_row[3].as_str(), acme_row[4].as_str()), ("111", ""));
    // Deterministic bytes.
    export_crosswalk(&registry, &root.join("crosswalk-again.csv"));
    assert_eq!(
        fs::read(root.join("crosswalk.csv")).unwrap(),
        fs::read(root.join("crosswalk-again.csv")).unwrap()
    );

    // A later session with a brand-new spelling and only the CIK attaches it
    // to the existing id: the incumbent is recognised by its key.
    let entries_before = read_json(&registry.join("registry.json"))["entry_count"]
        .as_u64()
        .unwrap();
    let second = init_legacy(root, "second", &registry);
    ingest_solve(
        &second,
        root,
        &[observation(
            "z9",
            "Zeta Corp Holdings",
            json!({"cik": "222"}),
            "lineage-e",
        )],
        RULES,
    );
    let attached = stdout_json(&commit(&second, "0.2.0").success());
    assert_eq!(
        attached["minted"].as_array().unwrap().len(),
        0,
        "{attached}"
    );
    assert_eq!(attached["attached"][0]["canonical_id"], zeta.as_str());
    assert_eq!(
        lookup(root, &registry, "Zeta Corp Holdings").as_deref(),
        Some(zeta.as_str())
    );
    assert_eq!(
        read_json(&registry.join("registry.json"))["entry_count"]
            .as_u64()
            .unwrap(),
        entries_before + 1,
        "only the new spelling was added; the key was already there"
    );
}

#[test]
fn an_incumbent_named_with_a_key_gains_the_key_without_minting_or_a_new_spelling() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    seed_registry(
        &registry,
        &[
            (
                "aliases.json",
                json!([alias("Orion Ltd", "ORG-001", "MANUAL")]),
            ),
            ("20-firm-keys.json", json!([])),
        ],
        1,
    );
    let map = root.join("registries.yaml");
    write(
        &map,
        &format!(
            "version: canon.session.registries.v1\nkinds:\n  organisation:\n    registry: {}\n    registry_id: firms\n    prefix: ORG\n    zero_pad: 3\n    canonical_type: firm\n    alias_file: aliases.json\n    key_file: 20-firm-keys.json\n",
            registry.display()
        ),
    );
    let session_dir = root.join("session");
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "incumbent keys",
            "--registries",
            map.to_str().unwrap(),
            "--work-dir",
            session_dir.to_str().unwrap(),
        ])
        .assert()
        .success();
    let session = Session { dir: session_dir };
    let solved = ingest_solve(
        &session,
        root,
        &[observation(
            "o1",
            "Orion Ltd",
            json!({"cik": "444"}),
            "lineage-a",
        )],
        RULES_NO_MINT,
    );
    assert_eq!(
        solved["clusters"][0]["reason"], "incumbent_keyed",
        "{solved}"
    );

    let committed = stdout_json(&commit(&session, "1.0.1").success());
    assert_eq!(
        committed["minted"].as_array().unwrap().len(),
        0,
        "{committed}"
    );
    assert_eq!(committed["attached"][0]["canonical_id"], "ORG-001");
    let keys = read_json(&registry.join("20-firm-keys.json"));
    assert_eq!(keys.as_array().unwrap().len(), 1, "{keys}");
    assert_eq!(keys[0]["input"], "444");
    assert_eq!(keys[0]["canonical_id"], "ORG-001");
    assert!(
        !registry.join("keys.cik.json").exists(),
        "the map's key_file is used, not the default name"
    );
    assert_eq!(lookup(root, &registry, "444").as_deref(), Some("ORG-001"));
    assert_eq!(
        read_json(&registry.join("aliases.json"))
            .as_array()
            .unwrap()
            .len(),
        1,
        "no new spelling was needed"
    );
}

#[test]
fn a_key_equal_to_another_ids_name_is_refused_alone_and_never_shadows() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    // A company literally named "111".
    seed_registry(
        &registry,
        &[("aliases.json", json!([alias("111", "ORG-002", "MANUAL")]))],
        1,
    );
    let session = init_legacy(root, "session", &registry);
    ingest_solve(
        &session,
        root,
        &[observation(
            "g1",
            "Gamma Corp",
            json!({"cik": "111"}),
            "lineage-a",
        )],
        RULES,
    );
    let committed = stdout_json(&commit(&session, "1.0.1").success());
    assert_eq!(committed["outcome"], "committed", "{committed}");
    let collision = committed["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["reason"] == "key_alias_collision")
        .unwrap_or_else(|| panic!("no key_alias_collision: {committed}"));
    let detail = collision["detail"]
        .as_str()
        .expect("detail names both entries");
    assert!(
        detail.contains("ORG-002") && detail.contains("'111'"),
        "{detail}"
    );
    let new_id = id_for_surface(&committed, "Gamma Corp");
    assert!(detail.contains(&new_id), "{detail}");
    assert_eq!(lookup(root, &registry, "111").as_deref(), Some("ORG-002"));
    assert_eq!(
        lookup(root, &registry, "Gamma Corp").as_deref(),
        Some(new_id.as_str()),
        "the rest of the cluster was written"
    );
}

#[test]
fn a_name_and_a_key_that_belong_to_two_ids_write_nothing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    let mut key_entry = alias("555", "ORG-002", "SESSION_KEY_CIK");
    key_entry["namespace"] = json!({"kind": "core", "class": "external_global_id"});
    seed_registry(
        &registry,
        &[
            (
                "aliases.json",
                json!([alias("Orion Ltd", "ORG-001", "MANUAL")]),
            ),
            ("keys.cik.json", json!([key_entry])),
        ],
        2,
    );
    let session = init_legacy(root, "session", &registry);
    let solved = ingest_solve(
        &session,
        root,
        &[observation(
            "o1",
            "Orion Ltd",
            json!({"cik": "555"}),
            "lineage-a",
        )],
        RULES,
    );
    assert_eq!(solved["summary"]["contradiction"], 1, "{solved}");
    assert_eq!(solved["contradictions"][0]["reason"], "incumbent_conflict");
    let before = registry_files(&registry);
    let nothing = commit(&session, "1.0.1").code(1);
    assert_eq!(stdout_json(&nothing)["outcome"], "nothing_to_commit");
    assert_eq!(registry_files(&registry), before, "no write happened");
}

#[test]
fn a_non_authoritative_key_is_written_only_when_the_rule_table_lists_it() {
    for (rules, written) in [(RULES, false), (RULES_WITH_TICKER_COMMIT, true)] {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let registry = root.join("registry");
        seed_registry(&registry, &[("aliases.json", json!([]))], 0);
        let session = init_legacy(root, "session", &registry);
        ingest_solve(
            &session,
            root,
            &[observation(
                "d1",
                "Delta Corp",
                json!({"cik": "777", "ticker": "dlt"}),
                "lineage-a",
            )],
            rules,
        );
        let committed = stdout_json(&commit(&session, "1.0.1").success());
        assert_eq!(committed["outcome"], "committed", "{committed}");
        let id = id_for_surface(&committed, "Delta Corp");
        assert_eq!(lookup(root, &registry, "777").as_deref(), Some(id.as_str()));
        let ticker_file = registry.join("keys.ticker.json");
        assert_eq!(ticker_file.exists(), written, "listed={written}");
        if written {
            let entries = read_json(&ticker_file);
            assert_eq!(entries[0]["input"], "DLT", "canonical ticker form");
            assert_eq!(entries[0]["rule_id"], "SESSION_KEY_TICKER");
            assert_eq!(lookup(root, &registry, "DLT").as_deref(), Some(id.as_str()));
        } else {
            assert_eq!(lookup(root, &registry, "DLT"), None);
        }
    }
}

#[test]
fn registry_lint_and_diff_accept_a_registry_with_key_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry = root.join("registry");
    seed_registry(&registry, &[("aliases.json", json!([]))], 0);
    let session = init_legacy(root, "session", &registry);
    ingest_solve(
        &session,
        root,
        &[observation(
            "a1",
            "Acme Holdings Inc",
            json!({"cik": "111"}),
            "lineage-a",
        )],
        RULES,
    );
    let old = root.join("old");
    fs::create_dir_all(&old).unwrap();
    for (name, bytes) in registry_files(&registry) {
        fs::write(old.join(name), bytes).unwrap();
    }
    commit(&session, "1.0.1").success();
    let lint = canon()
        .args(["registry", "lint", registry.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(stdout_json(&lint)["summary"]["errors"], 0);
    let diff = canon()
        .args([
            "registry",
            "diff",
            "--old",
            old.to_str().unwrap(),
            "--new",
            registry.to_str().unwrap(),
        ])
        .assert()
        .success();
    let diff = stdout_json(&diff);
    assert!(
        diff["summary"]["added"].as_u64().unwrap_or(0) >= 2,
        "name and key aliases both show as added: {diff}"
    );
}
