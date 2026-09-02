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
    "entity-run-dual-engine-shares-one-command-and-one-version",
    "legacy-org-runtime-still-backs-shipped-entity-surfaces",
    "shared-canon-entity-version-ids-span-legacy-and-workbench-contracts",
];
const RETIRED_INCOMPATIBILITIES: &[&str] = &[
    "entity-clap-leaves-missing-from-operator-json",
    "docs-and-plan-omit-prepare-profile-and-run-workdir-options",
    "operator-json-status-null-on-implemented-rows",
];
const HISTORICAL_GEO_SCHEMA_ONLY_CONTRACTS: &[(&str, &str)] = &[
    (
        "canon_geo_home_cell_rows.v0",
        "schemas/canon.geo.home_cell_rows.v0.schema.json",
    ),
    (
        "canon_geo_home_cell_assignment.v0",
        "schemas/canon.geo.home_cell_assignment.v0.schema.json",
    ),
    (
        "canon_geo_tile_work_request.v0",
        "schemas/canon.geo.tile_work_request.v0.schema.json",
    ),
    (
        "canon_geo_tile_work_unit.v0",
        "schemas/canon.geo.tile_work_unit.v0.schema.json",
    ),
    (
        "canon_geo_tile_reconciliation_request.v0",
        "schemas/canon.geo.tile_reconciliation_request.v0.schema.json",
    ),
    (
        "canon_geo_tile_reconciliation.v0",
        "schemas/canon.geo.tile_reconciliation.v0.schema.json",
    ),
];

#[derive(Debug, Deserialize)]
struct Inventory {
    schema_version: String,
    bead: String,
    command_rows: Vec<CommandRow>,
    contract_rows: Vec<ContractRow>,
    incompatibility_rows: Vec<IncompatibilityRow>,
    cutover_decisions: Vec<CutoverDecision>,
    bd_h9jn_scaffold: BdH9jnScaffold,
    unresolved_decisions: Vec<UnresolvedDecision>,
}

#[derive(Debug, Deserialize)]
struct CommandRow {
    id: String,
    clap_path: Option<String>,
    operator_name: Option<String>,
    primary_contracts: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ContractRow {
    id: String,
    access_boundary: Option<String>,
    crate_module: Option<String>,
    source_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IncompatibilityRow {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CutoverDecision {
    id: String,
    owner: String,
    status: String,
    subject: String,
    decision: String,
    selected_public_command: Option<String>,
    selected_artifact_version: Option<String>,
    selected_artifact_family: Option<String>,
    selected_public_commands: Option<Vec<String>>,
    legacy_versions: Option<Vec<String>>,
    acceptance_criteria: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BdH9jnScaffold {
    thread_id: String,
    reservation_paths: Vec<String>,
    acceptance_criteria: Vec<String>,
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
    assert_unique(
        inventory
            .cutover_decisions
            .iter()
            .map(|row| row.id.as_str()),
        "cutover decision ids",
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
        .filter(|row| {
            !HISTORICAL_GEO_SCHEMA_ONLY_CONTRACTS
                .iter()
                .any(|(id, _)| row.id == *id)
        })
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        !inventory
            .command_rows
            .iter()
            .any(|row| row.clap_path.as_deref() == Some("entity edge")),
        "stale public entity edge Clap leaf inventory row must not remain"
    );

    let actual_clap_paths = clap_leaf_commands();
    let actual_operator_names = operator_subcommands();
    let actual_contract_ids = shipped_contract_ids();

    println!(
        "canon_v1 contract inventory: command_rows={}, clap_leafs={}, operator_rows={}, contract_rows={}, incompatibilities={}, cutover_decisions={}, unresolved={}",
        inventory.command_rows.len(),
        actual_clap_paths.len(),
        actual_operator_names.len(),
        actual_contract_ids.len(),
        inventory.incompatibility_rows.len(),
        inventory.cutover_decisions.len(),
        inventory.unresolved_decisions.len(),
    );
    for decision in &inventory.cutover_decisions {
        println!(
            "decided {}: {} :: {}",
            decision.id, decision.subject, decision.decision
        );
    }
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

    for retired in RETIRED_INCOMPATIBILITIES {
        assert!(
            !incompatibility_ids.contains(retired),
            "retired incompatibility row must not remain: {retired}"
        );
    }

    assert!(
        inventory.unresolved_decisions.iter().all(|row| !matches!(
            row.id.as_str(),
            "entity-prepare-public-surface"
                | "version-bump-vs-in-place-cutover"
                | "operator-json-granularity"
        )),
        "bd-1cdf cutover decisions should move out of unresolved_decisions"
    );
}

#[test]
fn canon_v1_contract_inventory_freezes_bd_h9jn_cutover_decisions() {
    let inventory = inventory();
    let decisions = inventory
        .cutover_decisions
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect::<std::collections::BTreeMap<_, _>>();

    let public_name = decisions
        .get("bd-h9jn-public-scoring-name")
        .expect("public scoring name decision present");
    assert_eq!(public_name.owner, "bd-1cdf");
    assert_eq!(public_name.status, "decided");
    assert_eq!(
        public_name.selected_public_command.as_deref(),
        Some("canon entity evidence")
    );
    assert_eq!(
        public_name.selected_artifact_version.as_deref(),
        Some("canon_entity_evidence.v1")
    );
    assert_eq!(
        public_name.legacy_versions.as_ref(),
        Some(&vec!["canon_entity_edge.v0".to_string()])
    );
    assert!(
        public_name
            .decision
            .contains("no `edge` compatibility alias")
            || public_name
                .decision
                .contains("Do not keep `canon entity edge`")
    );

    let legacy_policy = decisions
        .get("bd-h9jn-v0-legacy-policy")
        .expect("legacy policy decision present");
    assert_eq!(
        legacy_policy.selected_artifact_family.as_deref(),
        Some("canon_entity_*.v1")
    );
    assert!(
        legacy_policy
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.contains("No public command emits `canon_entity_*.v0`")),
        "legacy v0 policy should forbid public v0 emission"
    );

    let prepare_profile = decisions
        .get("bd-h9jn-public-prepare-profile")
        .expect("prepare/profile decision present");
    let selected = prepare_profile
        .selected_public_commands
        .as_ref()
        .expect("prepare/profile selected commands");
    assert_eq!(
        selected,
        &[
            "canon entity prepare".to_string(),
            "canon entity profile list".to_string(),
            "canon entity profile init".to_string()
        ]
    );

    let contract_ids = inventory
        .contract_rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        contract_ids.contains("canon_entity_evidence.v1"),
        "v1 evidence contract row should be present"
    );
    assert!(
        !contract_ids.contains("canon_entity_edge.v1"),
        "edge.v1 must not remain as a final public v1 contract"
    );
}

#[test]
fn canon_v1_contract_inventory_freezes_bd_h9jn_scaffold_handoff() {
    let inventory = inventory();
    assert_eq!(inventory.bd_h9jn_scaffold.thread_id, "bd-h9jn");
    assert_eq!(
        inventory.bd_h9jn_scaffold.reservation_paths,
        vec![
            "src/cli.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/entity/mod.rs".to_string(),
            "src/entity/runtime.rs".to_string(),
            "tests/cli_smoke.rs".to_string(),
            "tests/fixtures/canon_v1/help/entity_help.txt".to_string(),
            "tests/fixtures/canon_v1/help/entity_link_help.txt".to_string()
        ]
    );
    assert!(
        inventory
            .bd_h9jn_scaffold
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.contains("no `edge` alias")),
        "bd-h9jn acceptance should forbid a public edge alias"
    );
    assert!(
        inventory
            .bd_h9jn_scaffold
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.contains("structured refusals")),
        "bd-h9jn acceptance should preserve refusal routing for legacy paths"
    );
}

#[test]
fn canon_v1_contract_inventory_marks_reviewed_public_contract_boundaries() {
    let inventory = inventory();
    let boundaries = inventory
        .contract_rows
        .iter()
        .filter_map(|row| {
            row.crate_module.as_ref().map(|module| {
                (
                    row.id.as_str(),
                    row.access_boundary.as_deref(),
                    module.as_str(),
                )
            })
        })
        .collect::<BTreeSet<_>>();

    assert!(
        boundaries.contains(&(
            "canon.identity.fact.v1",
            Some("public_crate_api"),
            "canon::temporal"
        )),
        "canon.identity.fact.v1 should be marked as a public crate contract"
    );
    assert!(
        boundaries.contains(&(
            "canon.unresolved.inbox.v1",
            Some("public_crate_api"),
            "canon::inbox"
        )),
        "canon.unresolved.inbox.v1 should be marked as a public crate contract"
    );
    assert!(
        boundaries.contains(&(
            "canon.review.policy.v1",
            Some("public_crate_api"),
            "canon::extensions::review_policy"
        )),
        "canon.review.policy.v1 should be marked as a public crate contract"
    );
    let review_policy = inventory
        .contract_rows
        .iter()
        .find(|row| row.id == "canon.review.policy.v1")
        .expect("canon.review.policy.v1 row present");
    assert_eq!(
        review_policy.source_path.as_deref(),
        Some("schemas/canon.review.policy.v1.schema.json"),
        "canon.review.policy.v1 should point at its public schema"
    );

    let extension_ontology = inventory
        .contract_rows
        .iter()
        .find(|row| row.id == "canon.extension.ontology.v1")
        .expect("canon.extension.ontology.v1 row present");
    assert_eq!(
        extension_ontology.access_boundary.as_deref(),
        Some("internal_source_only"),
        "canon.extension.ontology.v1 should be explicitly marked as non-public"
    );
    assert_eq!(
        extension_ontology.source_path.as_deref(),
        Some("src/extensions/ontology.rs"),
        "canon.extension.ontology.v1 should record its internal owner path"
    );
    assert!(
        extension_ontology.crate_module.is_none(),
        "internal-only contract rows should not claim a public canon:: path"
    );

    let legacy_resolve = inventory
        .contract_rows
        .iter()
        .find(|row| row.id == "canon_resolve.v0")
        .expect("canon_resolve.v0 row present");
    assert_eq!(
        legacy_resolve.access_boundary.as_deref(),
        Some("internal_source_only"),
        "canon_resolve.v0 should be explicitly marked as historical internal evidence"
    );
    assert_eq!(
        legacy_resolve.source_path.as_deref(),
        Some("src/resolve/types.rs"),
        "canon_resolve.v0 should record the source-only owner path"
    );
    assert!(
        legacy_resolve.crate_module.is_none(),
        "canon_resolve.v0 should not claim a public crate module"
    );
}

#[test]
fn canon_v1_contract_inventory_tracks_the_breaking_geo_tile_contracts() {
    let inventory = inventory();
    let expected = [
        (
            "canon geo materialize-home-cells",
            [
                "canon_geo_home_cell_rows.v1",
                "canon_geo_home_cell_assignment.v1",
            ],
        ),
        (
            "canon geo tile-work",
            [
                "canon_geo_tile_work_request.v1",
                "canon_geo_tile_work_unit.v1",
            ],
        ),
        (
            "canon geo reconcile-tiles",
            [
                "canon_geo_tile_reconciliation_request.v1",
                "canon_geo_tile_reconciliation.v1",
            ],
        ),
    ];
    for (command_id, contracts) in expected {
        let command = inventory
            .command_rows
            .iter()
            .find(|row| row.id == command_id)
            .unwrap_or_else(|| panic!("missing {command_id} inventory row"));
        assert_eq!(
            command.primary_contracts,
            contracts.map(str::to_string),
            "{command_id} must advertise only the source/release/entity-bound v1 contracts"
        );
    }

    let contract_ids = inventory
        .contract_rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<BTreeSet<_>>();
    for id in [
        "canon_geo_home_cell_rows.v1",
        "canon_geo_home_cell_assignment.v1",
        "canon_geo_tile_work_request.v1",
        "canon_geo_tile_work_unit.v1",
        "canon_geo_tile_reconciliation_request.v1",
        "canon_geo_tile_reconciliation.v1",
        "canon_geo_tile_decision.v1",
    ] {
        assert!(
            contract_ids.contains(id),
            "missing breaking Geo contract {id}"
        );
    }

    for (id, source_path) in HISTORICAL_GEO_SCHEMA_ONLY_CONTRACTS {
        let historical = inventory
            .contract_rows
            .iter()
            .find(|row| row.id == *id)
            .unwrap_or_else(|| panic!("missing historical Geo schema {id}"));
        assert_eq!(historical.access_boundary.as_deref(), Some("public_schema"));
        assert_eq!(historical.source_path.as_deref(), Some(*source_path));
        assert!(
            historical.crate_module.is_none(),
            "historical v0 tile schemas are published files, not the current Rust API"
        );
    }
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
