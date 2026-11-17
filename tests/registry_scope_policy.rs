//! bd-vdcw.54: the `role` scope dimension and the per-registry `scope_policy`
//! that makes it optional, through the public CLI.

use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::Path;
use tempfile::tempdir;

mod common;
use common::{write_registry_metadata, write_seed_csv};

fn role_scope(role: &str) -> Value {
    json!({"dimensions": [{
        "dimension": {"kind": "core", "dimension": "role"},
        "binding": {"binding": "exact", "value": role}
    }]})
}

fn write_entries(dir: &Path, entries: Value) {
    std::fs::write(
        dir.join("00-orgs.json"),
        serde_json::to_string_pretty(&entries).unwrap(),
    )
    .unwrap();
}

/// A miniature organisations registry: an invented firm and its invented unit,
/// plus the bare firm name scoped to the unit under role=servicer.
fn staged_registry(root: &Path, with_scoped: bool) -> std::path::PathBuf {
    let registry = root.join("orgs");
    std::fs::create_dir(&registry).unwrap();
    write_registry_metadata(&registry, "orgs", "1.0.0", if with_scoped { 3 } else { 2 });
    let mut entries = vec![
        json!({"input": "Harbor Trust Bank, National Association", "canonical_id": "ORG-001",
               "canonical_type": "organization", "rule_id": "FIRM"}),
        json!({"input": "Harbor Servicing, a division of Harbor Trust Bank, National Association",
               "canonical_id": "ORG-002", "canonical_type": "organization", "rule_id": "UNIT"}),
    ];
    if with_scoped {
        entries.push(json!({
            "input": "Harbor Trust Bank, National Association", "canonical_id": "ORG-002",
            "canonical_type": "organization", "rule_id": "UNIT_ROLE",
            "scope": role_scope("servicer")}));
    }
    write_entries(&registry, Value::Array(entries));
    registry
}

fn lookup(registry: &Path, root: &Path, name: &str, scope: Option<&str>) -> (i32, Value) {
    let input = root.join("rows.csv");
    write_seed_csv(&input, &format!("name\n\"{name}\"\n"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command
        .arg(&input)
        .arg("--registry")
        .arg(registry)
        .arg("--column")
        .arg("name")
        .arg("--emit")
        .arg("json")
        .arg("--explicit")
        .arg("--plain-json-values");
    if let Some(scope) = scope {
        command.arg("--scope").arg(scope);
    }
    let output = command.output().unwrap();
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json output");
    (output.status.code().unwrap(), payload)
}

fn set_policy(registry: &Path, extra: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["registry", "scope-policy", "--registry"])
        .arg(registry)
        .args(extra)
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).expect("scope-policy json")
}

const FIRM_NAME: &str = "Harbor Trust Bank, National Association";

fn mapped_id(payload: &Value) -> Option<String> {
    payload["mappings"][0]["canonical_id"]
        .as_str()
        .map(str::to_string)
}

#[test]
fn without_a_policy_a_scoped_entry_still_refuses_a_bare_lookup() {
    let temp = tempdir().unwrap();
    let registry = staged_registry(temp.path(), true);
    let (code, payload) = lookup(&registry, temp.path(), FIRM_NAME, None);
    assert_eq!(code, 2);
    assert_eq!(payload["refusal"]["code"], "E_SCOPE_REQUIRED");
    // And a scoped query does not fall back to an unscoped entry.
    let (_, payload) = lookup(
        &registry,
        temp.path(),
        "Harbor Servicing, a division of Harbor Trust Bank, National Association",
        Some("role=servicer"),
    );
    assert_eq!(mapped_id(&payload), None);
}

#[test]
fn an_opted_in_role_dimension_gives_the_unit_scoped_and_the_firm_bare() {
    let temp = tempdir().unwrap();
    let registry = staged_registry(temp.path(), true);
    let policy = set_policy(
        &registry,
        &["--optional-dimension", "role", "--next-version", "1.0.1"],
    );
    assert_eq!(policy["new_policy"]["optional_dimensions"], json!(["role"]));
    assert_eq!(policy["version_after"], "1.0.1");
    let registry_json: Value =
        serde_json::from_slice(&std::fs::read(registry.join("registry.json")).unwrap()).unwrap();
    assert_eq!(
        registry_json["scope_policy"]["optional_dimensions"],
        json!(["role"])
    );

    let (code, bare) = lookup(&registry, temp.path(), FIRM_NAME, None);
    assert_eq!(code, 0, "{bare}");
    assert_eq!(mapped_id(&bare).as_deref(), Some("ORG-001"));
    let (_, scoped) = lookup(&registry, temp.path(), FIRM_NAME, Some("role=servicer"));
    assert_eq!(mapped_id(&scoped).as_deref(), Some("ORG-002"));
    // A role with no entry scoped exactly so falls back to the unscoped firm.
    let (_, other) = lookup(&registry, temp.path(), FIRM_NAME, Some("role=trustee"));
    assert_eq!(mapped_id(&other).as_deref(), Some("ORG-001"));
    // A name that exists only unscoped resolves the same way either route.
    let unit_name = "Harbor Servicing, a division of Harbor Trust Bank, National Association";
    let (_, bare_unit) = lookup(&registry, temp.path(), unit_name, None);
    let (_, scoped_unit) = lookup(&registry, temp.path(), unit_name, Some("role=servicer"));
    assert_eq!(mapped_id(&bare_unit).as_deref(), Some("ORG-002"));
    assert_eq!(mapped_id(&scoped_unit).as_deref(), Some("ORG-002"));
}

#[test]
fn a_dimension_not_listed_keeps_the_old_rules() {
    let temp = tempdir().unwrap();
    let registry = temp.path().join("deals");
    std::fs::create_dir(&registry).unwrap();
    write_registry_metadata(&registry, "deals", "1.0.0", 1);
    write_entries(
        &registry,
        json!([{
            "input": "41-001", "canonical_id": "PROPERTY-A", "canonical_type": "property",
            "rule_id": "R",
            "scope": {"dimensions": [{
                "dimension": {"kind": "core", "dimension": "dataset"},
                "binding": {"binding": "exact", "value": "deal:first"}}]}
        }]),
    );
    set_policy(
        &registry,
        &["--optional-dimension", "role", "--next-version", "1.0.1"],
    );
    let (code, payload) = lookup(&registry, temp.path(), "41-001", None);
    assert_eq!(code, 2);
    assert_eq!(payload["refusal"]["code"], "E_SCOPE_REQUIRED");
}

#[test]
fn removing_the_dimension_restores_the_refusal_and_unknown_dimensions_refuse() {
    let temp = tempdir().unwrap();
    let registry = staged_registry(temp.path(), true);
    set_policy(
        &registry,
        &["--optional-dimension", "role", "--next-version", "1.0.1"],
    );
    set_policy(
        &registry,
        &["--remove-dimension", "role", "--next-version", "1.0.2"],
    );
    let (code, payload) = lookup(&registry, temp.path(), FIRM_NAME, None);
    assert_eq!(
        (code, payload["refusal"]["code"].as_str()),
        (2, Some("E_SCOPE_REQUIRED"))
    );
    let refused = set_policy(
        &registry,
        &["--optional-dimension", "dataset", "--next-version", "1.0.3"],
    );
    assert_eq!(refused["outcome"], "REFUSAL");
}

#[test]
fn lint_refuses_two_ids_for_one_input_under_the_same_scope() {
    let temp = tempdir().unwrap();
    let registry = staged_registry(temp.path(), true);
    std::fs::write(
        registry.join("01-extra.json"),
        serde_json::to_string(&json!([{
            "input": FIRM_NAME, "canonical_id": "ORG-009", "canonical_type": "organization",
            "rule_id": "CLASH", "scope": role_scope("servicer")}]))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["registry", "lint", "--emit", "json"])
        .arg(&registry)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("scope_collision"), "{text}");
    // The same input under different scopes is not a collision.
    std::fs::remove_file(registry.join("01-extra.json")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args(["registry", "lint", "--emit", "json"])
        .arg(&registry)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("scope_collision"));
}
