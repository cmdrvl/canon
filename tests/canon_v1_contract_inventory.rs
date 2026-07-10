#![forbid(unsafe_code)]

use canon::cli::Cli;
use clap::CommandFactory;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const INVENTORY_JSON: &str = include_str!("fixtures/canon_v1/contract_inventory.json");
const OPERATOR_JSON: &str = include_str!("../operator.json");
const REQUIRED_SPECIAL_SURFACES: &[&str] = &[
    "canon <INPUT>",
    "canon",
    "canon --version",
    "canon --describe",
    "canon --schema",
    "canon doctor",
    "canon doctor --robot-triage",
];
const EXPECTED_INCOMPATIBILITIES: &[&str] = &[
    "entity-clap-leaves-missing-from-operator-json",
    "entity-run-dual-engine-shares-one-command-and-one-version",
    "legacy-org-runtime-still-backs-shipped-entity-surfaces",
    "shared-canon-entity-version-ids-span-legacy-and-workbench-contracts",
    "docs-and-plan-omit-prepare-profile-and-run-workdir-options",
    "operator-json-status-null-on-implemented-rows",
];

#[derive(Debug, Deserialize)]
struct Inventory {
    schema_version: String,
    bead: String,
    command_rows: Vec<CommandRow>,
    contract_rows: Vec<ContractRow>,
    incompatibility_rows: Vec<IncompatibilityRow>,
    unresolved_decisions: Vec<UnresolvedDecision>,
}

#[derive(Debug, Deserialize)]
struct CommandRow {
    id: String,
    clap_path: Option<String>,
    operator_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContractRow {
    id: String,
}

#[derive(Debug, Deserialize)]
struct IncompatibilityRow {
    id: String,
    subjects: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UnresolvedDecision {
    id: String,
    subject: String,
    notes: String,
}

#[test]
fn canon_v1_contract_inventory_covers_shipped_commands_and_contract_ids() {
    let inventory = inventory();
    assert_eq!(inventory.schema_version, "canon_v1_contract_inventory.v0");
    assert_eq!(inventory.bead, "bd-glo7");

    assert_unique(
        inventory.command_rows.iter().map(|row| row.id.as_str()),
        "command row ids",
    );
    assert_unique(
        inventory
            .command_rows
            .iter()
            .filter_map(|row| row.clap_path.as_deref()),
        "clap paths",
    );
    assert_unique(
        inventory
            .command_rows
            .iter()
            .filter_map(|row| row.operator_name.as_deref()),
        "operator names",
    );
    assert_unique(
        inventory.contract_rows.iter().map(|row| row.id.as_str()),
        "contract row ids",
    );
    assert_unique(
        inventory
            .incompatibility_rows
            .iter()
            .map(|row| row.id.as_str()),
        "incompatibility row ids",
    );
    assert_unique(
        inventory
            .unresolved_decisions
            .iter()
            .map(|row| row.id.as_str()),
        "unresolved decision ids",
    );

    let inventory_command_ids = inventory
        .command_rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    let inventory_clap_paths = inventory
        .command_rows
        .iter()
        .filter_map(|row| row.clap_path.clone())
        .collect::<BTreeSet<_>>();
    let inventory_operator_names = inventory
        .command_rows
        .iter()
        .filter_map(|row| row.operator_name.clone())
        .collect::<BTreeSet<_>>();
    let inventory_contract_ids = inventory
        .contract_rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();

    let actual_clap_paths = clap_leaf_commands();
    let actual_operator_names = operator_subcommands();
    let actual_contract_ids = shipped_contract_ids();

    println!(
        "canon_v1 contract inventory: command_rows={}, clap_leafs={}, operator_rows={}, contract_rows={}, incompatibilities={}, unresolved={}",
        inventory.command_rows.len(),
        actual_clap_paths.len(),
        actual_operator_names.len(),
        actual_contract_ids.len(),
        inventory.incompatibility_rows.len(),
        inventory.unresolved_decisions.len(),
    );
    for decision in &inventory.unresolved_decisions {
        println!(
            "unresolved {}: {} :: {}",
            decision.id, decision.subject, decision.notes
        );
    }

    assert_set_eq(
        "Clap leaf commands",
        &inventory_clap_paths,
        &actual_clap_paths,
    );
    assert_set_eq(
        "operator.json subcommands",
        &inventory_operator_names,
        &actual_operator_names,
    );
    assert_set_eq(
        "versioned contracts from production source",
        &inventory_contract_ids,
        &actual_contract_ids,
    );

    let required_special = REQUIRED_SPECIAL_SURFACES
        .iter()
        .map(|surface| (*surface).to_string())
        .collect::<BTreeSet<_>>();
    let missing_special = required_special
        .difference(&inventory_command_ids)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing_special.is_empty(),
        "inventory is missing special executable surfaces: {missing_special:?}"
    );
}

#[test]
fn canon_v1_contract_inventory_freezes_current_incompatibility_rows() {
    let inventory = inventory();
    let incompatibility_ids = inventory
        .incompatibility_rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<BTreeSet<_>>();

    for expected in EXPECTED_INCOMPATIBILITIES {
        assert!(
            incompatibility_ids.contains(expected),
            "missing incompatibility row {expected}"
        );
    }

    let missing_operator = inventory
        .incompatibility_rows
        .iter()
        .find(|row| row.id == "entity-clap-leaves-missing-from-operator-json")
        .expect("missing operator coverage row");
    let expected_subjects = [
        "canon entity prepare",
        "canon entity profile list",
        "canon entity profile init",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<BTreeSet<_>>();
    let actual_subjects = missing_operator
        .subjects
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_subjects, expected_subjects);

    assert!(
        !inventory.unresolved_decisions.is_empty(),
        "inventory should keep unresolved decisions visible"
    );
}

fn inventory() -> Inventory {
    serde_json::from_str(INVENTORY_JSON).expect("contract inventory fixture parses")
}

fn clap_leaf_commands() -> BTreeSet<String> {
    fn walk(prefix: &str, command: &clap::Command, out: &mut BTreeSet<String>) {
        let subcommands = command
            .get_subcommands()
            .filter(|subcommand| subcommand.get_name() != "help")
            .collect::<Vec<_>>();
        if subcommands.is_empty() {
            out.insert(prefix.to_string());
            return;
        }
        for subcommand in subcommands {
            let next = if prefix.is_empty() {
                subcommand.get_name().to_string()
            } else {
                format!("{prefix} {}", subcommand.get_name())
            };
            walk(&next, subcommand, out);
        }
    }

    let mut out = BTreeSet::new();
    let root = Cli::command();
    for subcommand in root
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help")
    {
        walk(subcommand.get_name(), subcommand, &mut out);
    }
    out
}

fn operator_subcommands() -> BTreeSet<String> {
    let manifest = serde_json::from_str::<Value>(OPERATOR_JSON).expect("operator.json parses");
    manifest["subcommands"]
        .as_array()
        .expect("operator subcommands array")
        .iter()
        .map(|row| {
            row["name"]
                .as_str()
                .expect("operator subcommand name")
                .to_string()
        })
        .collect()
}

fn shipped_contract_ids() -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_files(&repo_root().join("src"), &mut files);
    files.push(repo_root().join("operator.json"));

    let mut ids = BTreeSet::new();
    for path in files {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        ids.extend(scan_contract_ids(&text));
    }
    ids
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read_dir {}: {error}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("dir entry {}: {error}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn scan_contract_ids(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-'))
        .filter(|token| is_contract_id(token))
        .map(ToOwned::to_owned)
        .collect()
}

fn is_contract_id(token: &str) -> bool {
    let has_prefix = token.starts_with("canon") || token.starts_with("operator.");
    let Some((_, suffix)) = token.rsplit_once(".v") else {
        return false;
    };
    has_prefix && !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn assert_unique<'a>(items: impl Iterator<Item = &'a str>, label: &str) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for item in items {
        if !seen.insert(item.to_string()) {
            duplicates.insert(item.to_string());
        }
    }
    assert!(
        duplicates.is_empty(),
        "{label} contains duplicates: {duplicates:?}"
    );
}

fn assert_set_eq(label: &str, inventory: &BTreeSet<String>, actual: &BTreeSet<String>) {
    let missing = actual.difference(inventory).cloned().collect::<Vec<_>>();
    let extra = inventory.difference(actual).cloned().collect::<Vec<_>>();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{label} drifted\nmissing from inventory: {missing:?}\nextra in inventory: {extra:?}"
    );
}
