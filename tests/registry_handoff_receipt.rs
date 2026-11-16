#![forbid(unsafe_code)]
//! bd-vdcw.55: the session commit receipt carries what a driver needs to
//! publish the registry directory: base and new identity, the changed files,
//! the closures the commit wrote and the export command. Every flow goes
//! through the public CLI on temp directories; nothing is uploaded.

#[allow(dead_code)]
mod distribution {
    pub mod backend {
        include!("../src/distribution/backend.rs");
    }
}

mod registry {
    pub use canon::registry::{
        canonical_package_bytes, compile_registry_package, parse_registry_package,
    };
}

mod registry_transaction {
    include!("../src/registry/transaction.rs");
}

use assert_cmd::Command;
use distribution::backend::{
    FilesystemPublicationBackend, PublicationOutcome, PublishedPackageRef,
};
use registry_transaction::{RegistryPublicationTransaction, publish_registry_transaction};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
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

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn seed(dir: &Path) {
    write(
        &dir.join("registry.json"),
        &serde_json::to_string_pretty(&json!({
            "id": "firms",
            "version": "1.0.0",
            "description": "hand-off fixture",
            "updated": "2026-11-16",
            "entry_count": 1,
            "default_id_scheme": {"prefix": "ORG", "zero_pad": 3}
        }))
        .unwrap(),
    );
    write(
        &dir.join("aliases.json"),
        &serde_json::to_string_pretty(&json!([
            {
                "input": "Orion Ltd",
                "canonical_id": "ORG-001",
                "canonical_type": "firm",
                "rule_id": "SESSION_TIER_A"
            }
        ]))
        .unwrap(),
    );
    write(&dir.join("_escrow/cannot_link.jsonl"), "");
}

/// Every file under `dir`, relative path to blake3 hash. Independent of canon's
/// own file discovery: it walks the whole tree.
fn tree(dir: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    format!(
                        "blake3:{}",
                        blake3::hash(&fs::read(&path).unwrap()).to_hex()
                    ),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(dir, dir, &mut files);
    files
}

fn copy_tree(from: &Path, to: &Path) {
    for (relative, _) in tree(from) {
        let target = to.join(&relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(from.join(&relative), target).unwrap();
    }
}

fn identity(dir: &Path) -> Value {
    let output = canon()
        .args(["registry", "identity", "--registry", dir.to_str().unwrap()])
        .assert()
        .success();
    serde_json::from_slice(&output.get_output().stdout).expect("identity JSON")
}

fn stdout_json(assertion: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout).expect("JSON stdout")
}

fn run_session(root: &Path, name: &str, registry: &Path, observation: Value) -> std::path::PathBuf {
    let session = root.join(name);
    canon()
        .args([
            "entity",
            "session",
            "init",
            "--goal",
            "hand-off",
            "--registry",
            registry.to_str().unwrap(),
            "--work-dir",
            session.to_str().unwrap(),
            "--registry-id",
            "firms",
        ])
        .assert()
        .success();
    let observations = root.join(format!("{name}.jsonl"));
    write(
        &observations,
        &format!("{}\n", serde_json::to_string(&observation).unwrap()),
    );
    let rules = root.join("rules.yaml");
    write(&rules, RULES);
    canon()
        .args([
            "entity",
            "session",
            "ingest",
            session.to_str().unwrap(),
            observations.to_str().unwrap(),
        ])
        .assert()
        .success();
    canon()
        .args([
            "entity",
            "session",
            "solve",
            session.to_str().unwrap(),
            "--rules",
            rules.to_str().unwrap(),
        ])
        .assert()
        .success();
    session
}

fn commit(session: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut args = vec!["entity", "session", "commit", session.to_str().unwrap()];
    args.extend_from_slice(extra);
    canon().args(args).assert()
}

fn stored_commit_receipt(session: &Path) -> Value {
    let snapshot = canon::entity::publication::open_current_stream_generation(
        session,
        canon::session::SESSION_COMMIT_STREAM_ID,
    )
    .expect("commit stream");
    serde_json::from_slice(
        snapshot
            .read_logical_file("commit/commit.json")
            .expect("stored commit/commit.json"),
    )
    .expect("stored receipt JSON")
}

fn changed_by_path(entry: &Value) -> BTreeMap<String, (String, Option<String>, Option<String>)> {
    entry["changed_files"]
        .as_array()
        .expect("changed_files array")
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap().to_string(),
                (
                    file["change"].as_str().unwrap().to_string(),
                    file["blake3_before"].as_str().map(str::to_string),
                    file["blake3_after"].as_str().map(str::to_string),
                ),
            )
        })
        .collect()
}

#[test]
fn commit_receipt_hands_off_base_new_changed_files_closures_and_export() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry_dir = root.join("registry");
    seed(&registry_dir);
    let session = run_session(
        root,
        "session",
        &registry_dir,
        json!({
            "obs_id": "g1",
            "entity_type": "organisation",
            "surface": "Gamma Corp",
            "keys": {"cik": "321"},
            "source": {"channel": "file", "id": "handoff", "lineage": "a"}
        }),
    );

    // Independent record of the tree and the identity before the commit.
    let before_copy = root.join("before-copy");
    copy_tree(&registry_dir, &before_copy);
    let before_tree = tree(&registry_dir);
    let identity_before = identity(&registry_dir);

    let committed = commit(&session, &["--next-version", "1.0.1"]).success();
    let receipt = stdout_json(&committed);
    eprintln!("commit receipt: {receipt}");
    assert_eq!(receipt["outcome"], "committed");
    let entry = &receipt["registries"][0];
    assert_eq!(entry["id"], "firms");

    // base / new are the identities before and after.
    let identity_after = identity(&registry_dir);
    assert_eq!(
        entry["base"]["content_hash"],
        identity_before["content_hash"]
    );
    assert_eq!(entry["base"]["version"], "1.0.0");
    assert_eq!(
        entry["base"]["package_digest"],
        identity_before["package_digest"]
    );
    assert_eq!(entry["new"]["content_hash"], identity_after["content_hash"]);
    assert_eq!(entry["new"]["version"], "1.0.1");
    assert_eq!(
        entry["new"]["package_digest"],
        identity_after["package_digest"]
    );
    assert_ne!(entry["base"]["content_hash"], entry["new"]["content_hash"]);
    // And the copy proves the "before" side is what the directory held.
    assert_eq!(
        identity(&before_copy)["content_hash"],
        entry["base"]["content_hash"]
    );

    // changed_files is exactly the independent tree diff.
    let after_tree = tree(&registry_dir);
    let mut expected = BTreeMap::new();
    for path in before_tree.keys().chain(after_tree.keys()) {
        let old = before_tree.get(path);
        let new = after_tree.get(path);
        let change = match (old, new) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(old), Some(new)) if old != new => "modified",
            _ => continue,
        };
        expected.insert(
            path.clone(),
            (change.to_string(), old.cloned(), new.cloned()),
        );
    }
    assert!(!expected.is_empty(), "the commit changed something");
    assert_eq!(changed_by_path(entry), expected);
    assert!(
        expected.contains_key("registry.json"),
        "registry.json carries the version bump"
    );

    // Closures: the entries this commit wrote, each a resolvable pinned ref.
    let closures = entry["closures"].as_array().expect("closures");
    assert!(!closures.is_empty(), "the minted entry has a closure");
    assert!(entry["commit_closure_hash"].is_string());
    for closure in closures {
        let reference = closure["ref"].as_str().unwrap();
        assert!(
            reference.starts_with("firms:ORG-002@1.0.1#blake3:"),
            "{reference}"
        );
        assert!(
            closure["outcome_ref"].as_str().unwrap().len() > 3
                && closure["closure_hash"]
                    .as_str()
                    .unwrap()
                    .starts_with("blake3:")
        );
        let check = canon()
            .args([
                "registry",
                "identity",
                "--registry",
                registry_dir.to_str().unwrap(),
                "--ref",
                reference,
            ])
            .assert()
            .success();
        let check = stdout_json(&check);
        assert_eq!(check["closure_found"], true, "{reference}: {check}");
        assert_eq!(check["version_match"], true);
    }

    // The stored commit.v1 receipt says the same thing as stdout.
    let stored = stored_commit_receipt(&session);
    assert_eq!(stored["registries"][0], *entry, "stored == stdout");

    // The printed export command runs and produces the served search index.
    let export = entry["export_command"].as_array().expect("export_command");
    let argv = export
        .iter()
        .map(|part| part.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(argv[0], "canon");
    assert_eq!(argv[1..3], ["registry", "export"]);
    let out_dir = TempDir::new().unwrap();
    canon()
        .current_dir(out_dir.path())
        .args(&argv[1..])
        .assert()
        .success();
    assert!(
        out_dir.path().join("1.0.1/search.sqlite").is_file(),
        "export_command wrote <version>/search.sqlite"
    );

    // Publishing the committed directory through the filesystem backend
    // yields the package the receipt named.
    let backend_dir = TempDir::new().unwrap();
    let backend = FilesystemPublicationBackend::new(backend_dir.path());
    let published = publish_registry_transaction(
        &backend,
        RegistryPublicationTransaction {
            registry_dir: registry_dir.clone(),
            channel: "stable".to_string(),
            expected_base: PublishedPackageRef {
                package_id: "firms".to_string(),
                package_version: "0.0.0".to_string(),
                content_digest: format!("blake3:{}", blake3::hash(b"genesis").to_hex()),
            },
            expected_channel_digest: None,
        },
    )
    .expect("publish the committed directory");
    assert_eq!(published.receipt.outcome, PublicationOutcome::Published);
    assert_eq!(published.package.package_id, "firms");
    assert_eq!(published.package.package_version, entry["new"]["version"]);
    assert_eq!(
        published.package.content_digest,
        entry["new"]["package_digest"]
    );
    assert!(
        backend_dir
            .path()
            .join(&published.receipt.object_path)
            .exists()
    );

    // An exact repeat: nothing written, base equals new, nothing changed.
    let before_repeat = tree(&registry_dir);
    let repeat = commit(&session, &["--next-version", "1.0.1"]).success();
    let repeat = stdout_json(&repeat);
    assert_eq!(repeat["outcome"], "already_committed");
    let repeated = &repeat["registries"][0];
    assert_eq!(repeated["base"], repeated["new"]);
    assert_eq!(
        repeated["new"]["content_hash"],
        identity_after["content_hash"]
    );
    assert_eq!(repeated["changed_files"], json!([]));
    assert_eq!(repeated["closures"], json!([]));
    assert_eq!(tree(&registry_dir), before_repeat);
}

#[test]
fn nothing_to_commit_reports_equal_base_and_new_and_no_changes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let registry_dir = root.join("registry");
    seed(&registry_dir);
    // No authoritative key and no match: standalone rows never mint.
    let session = run_session(
        root,
        "idle",
        &registry_dir,
        json!({
            "obs_id": "d1",
            "entity_type": "organisation",
            "surface": "Delta Holdings",
            "source": {"channel": "file", "id": "handoff", "lineage": "a"}
        }),
    );
    let before = tree(&registry_dir);
    let identity_before = identity(&registry_dir);

    let nothing = commit(&session, &["--next-version", "1.0.1"]).code(1);
    let receipt = stdout_json(&nothing);
    eprintln!("nothing receipt: {receipt}");
    assert_eq!(receipt["outcome"], "nothing_to_commit");
    let entry = &receipt["registries"][0];
    assert_eq!(entry["id"], "firms");
    assert_eq!(entry["base"], entry["new"]);
    assert_eq!(
        entry["base"]["content_hash"],
        identity_before["content_hash"]
    );
    assert_eq!(entry["changed_files"], json!([]));
    assert_eq!(entry["closures"], json!([]));
    assert!(entry["export_command"].is_array());
    assert_eq!(tree(&registry_dir), before, "no file written");
}
