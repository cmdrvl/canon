#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/project/lock.rs"]
mod lock;
#[allow(dead_code)]
#[path = "../src/project/manifest.rs"]
mod manifest;
#[allow(dead_code)]
#[path = "../src/project/plan.rs"]
mod plan;

use canon::registry::{RegistryExportFormat, RegistryExportRequest, export_registry};
use lock::{
    ProjectLock, ProjectLockInput, ProjectLockManifestProjection, ProjectLockRefKind,
    ProjectLockResolvedRef, digest_bytes, refresh_project_lock,
};
use manifest::{
    ProjectManifest, ProjectPackageKind, ProjectTemporalMode, ProjectTemporalPackageRole,
    load_project_manifest_toml, project_manifest_digest, project_temporal_contract,
};
use plan::{ProjectPlanRequest, compile_project_plan};
use serde_json::json;
use std::{fs, path::Path};
use tempfile::tempdir;

#[test]
fn project_temporal_contract_normalizes_once_and_every_plan_node_records_it() {
    let manifest = temporal_manifest(TEMPORAL_PROJECT_TOML);
    let contract = project_temporal_contract(&manifest).expect("temporal contract");
    assert_eq!(contract.mode, ProjectTemporalMode::AsOf);
    assert_eq!(contract.valid_at, "2026-07-10T16:00:00Z");
    assert_eq!(contract.known_as_of, "2026-07-11T01:30:00Z");
    assert_eq!(contract.calendar, "gregorian");
    assert_eq!(contract.timezone, "UTC");
    assert!(!contract.date_only_values_are_fabricated);
    assert_eq!(
        contract
            .fact_packages
            .iter()
            .map(|package| package.role)
            .collect::<Vec<_>>(),
        vec![
            ProjectTemporalPackageRole::ProviderFacts,
            ProjectTemporalPackageRole::ReviewedFacts
        ]
    );
    assert!(
        contract
            .policy_packages
            .iter()
            .any(|package| package.role == ProjectTemporalPackageRole::TrustPolicy)
    );
    assert!(
        contract
            .projection_packages
            .iter()
            .any(|package| package.role == ProjectTemporalPackageRole::RelationProjection)
    );

    let plan = compile_project_plan(plan_request(&manifest)).expect("temporal plan compiles");
    assert!(plan.nodes.len() > 8);
    for node in &plan.nodes {
        assert!(
            node.content_hash_inputs
                .iter()
                .any(|input| input.ref_id == "temporal.contract"),
            "node {} did not record temporal.contract",
            node.node_id
        );
    }

    let shifted = temporal_manifest(&TEMPORAL_PROJECT_TOML.replace(
        "known_as_of = \"2026-07-10T21:30:00-04:00\"",
        "known_as_of = \"2026-07-12T21:30:00-04:00\"",
    ));
    let shifted_plan =
        compile_project_plan(plan_request(&shifted)).expect("shifted temporal plan compiles");
    assert_ne!(plan.graph_hash, shifted_plan.graph_hash);
}

#[test]
fn timeless_project_is_explicit_and_date_only_values_refuse() {
    let timeless = temporal_manifest(
        &TEMPORAL_PROJECT_TOML
            .replace(
                "valid_at = \"2026-07-10T12:00:00-04:00\"",
                "valid_at = \"timeless\"",
            )
            .replace(
                "known_as_of = \"2026-07-10T21:30:00-04:00\"",
                "known_as_of = \"timeless\"",
            ),
    );
    let contract = project_temporal_contract(&timeless).expect("timeless contract");
    assert_eq!(contract.mode, ProjectTemporalMode::Timeless);
    assert_eq!(contract.valid_at, "timeless");
    assert_eq!(contract.known_as_of, "timeless");
    compile_project_plan(plan_request(&timeless)).expect("timeless plan remains valid");

    let date_only = TEMPORAL_PROJECT_TOML.replace(
        "valid_at = \"2026-07-10T12:00:00-04:00\"",
        "valid_at = \"2026-07-10\"",
    );
    let error = load_project_manifest_toml(&date_only).expect_err("date-only valid_at refuses");
    assert!(
        error
            .message
            .contains("date-only values must not be promoted")
    );
}

#[test]
fn registry_exports_can_emit_temporal_projection_contracts_without_changing_export_bytes() {
    let temp = tempdir().expect("tempdir");
    let registry_dir = temp.path().join("registry");
    write_registry_fixture(&registry_dir);
    let seed_path = temp.path().join("canon_seed.csv");

    let output = export_registry(RegistryExportRequest {
        registry: registry_dir,
        format: RegistryExportFormat::DbtSeed,
        out: seed_path.clone(),
        namespace: Some("warehouse_people".to_string()),
        source_files: Vec::new(),
        canonical_types: Vec::new(),
        rule_id_prefixes: Vec::new(),
        canonical_iri_prefix: "cmdrvl:".to_string(),
        schema_out: None,
        anti_collapse_test_out: None,
    })
    .expect("dbt export succeeds");
    let seed = fs::read_to_string(&seed_path).expect("seed written");
    assert!(!seed.contains("compiled_snapshot_digest"));

    let contract = output
        .temporal_projection_contract_json(
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "2026-07-10T12:00:00-04:00",
            "2026-07-10T21:30:00-04:00",
            Some("tenant:global"),
        )
        .expect("projection contract");
    assert_eq!(
        contract["compiled_snapshot_digest"],
        "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(contract["valid_at"], "2026-07-10T16:00:00Z");
    assert_eq!(contract["known_as_of"], "2026-07-11T01:30:00Z");
    assert_eq!(
        contract["current_format"]["preserves_compiled_snapshot_identity"],
        true
    );
    assert!(
        contract["portable_projection_formats"]
            .as_array()
            .expect("formats")
            .iter()
            .any(|format| format["format"] == "parquet"
                && format["relationship_validity"] == "typed_interval_columns")
    );
    assert!(
        contract["portable_projection_formats"]
            .as_array()
            .expect("formats")
            .iter()
            .any(|format| format["format"] == "rdf")
    );

    let refusal = output
        .temporal_projection_contract_json(
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "2026-07-10",
            "2026-07-10T21:30:00-04:00",
            None,
        )
        .expect_err("date-only projection refuses");
    assert!(refusal.message.contains("must not fabricate timestamps"));
}

fn temporal_manifest(toml: &str) -> ProjectManifest {
    load_project_manifest_toml(toml).expect("temporal manifest loads")
}

fn plan_request(manifest: &ProjectManifest) -> ProjectPlanRequest {
    ProjectPlanRequest::new(
        manifest.clone(),
        lock_for_manifest(manifest),
        "tests/fixtures/project/temporal.toml",
        "tests/fixtures/project/temporal.lock.json",
    )
}

fn lock_for_manifest(manifest: &ProjectManifest) -> ProjectLock {
    let digest = project_manifest_digest(manifest).expect("manifest digest");
    refresh_project_lock(&ProjectLockManifestProjection {
        project_id: manifest.project_id.clone(),
        project_digest: digest,
        inputs: manifest
            .sources
            .iter()
            .map(|source| ProjectLockInput {
                input_id: source.source_id.clone(),
                relative_path: source.path.clone(),
                content_digest: digest_bytes(source.path.as_bytes()),
            })
            .collect(),
        resolved_refs: manifest
            .packages
            .iter()
            .map(|package| ProjectLockResolvedRef {
                ref_id: package.alias.clone(),
                kind: match package.kind {
                    ProjectPackageKind::Strategy => ProjectLockRefKind::Strategy,
                    ProjectPackageKind::Registry
                    | ProjectPackageKind::EntityProfile
                    | ProjectPackageKind::SourceMapping
                    | ProjectPackageKind::Extension => ProjectLockRefKind::Package,
                },
                resolved_digest: package.content_hash.clone(),
            })
            .collect(),
    })
    .expect("lock builds")
}

fn write_json(path: &Path, value: serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn write_registry_fixture(registry_dir: &Path) {
    fs::create_dir_all(registry_dir).unwrap();
    write_json(
        &registry_dir.join("registry.json"),
        json!({
            "id": "people",
            "version": "1.0.0",
            "description": "temporal export fixture",
            "updated": "2026-07-10",
            "entry_count": 1
        }),
    );
    write_json(
        &registry_dir.join("aliases.json"),
        json!([
            {
                "input": "Alpha Person",
                "canonical_id": "person:alpha",
                "canonical_type": "person",
                "rule_id": "TEMPORAL_ALIAS_SNAPSHOT:policy.default"
            }
        ]),
    );
}

const TEMPORAL_PROJECT_TOML: &str = r#"schema_version = "canon.project.v1"
project_id = "project.temporal.integration"

[review]
cannot_link_max_score_basis_points = 2500
review_required_min_score_basis_points = 7200
auto_promote_min_score_basis_points = 9600

[temporal]
valid_at = "2026-07-10T12:00:00-04:00"
known_as_of = "2026-07-10T21:30:00-04:00"
scope_ref = "tenant:global"

[budgets]
max_input_bytes = 1048576
max_rows = 5000
max_candidates = 2000
max_review_items = 500
max_runtime_seconds = 600

[runtime]
offline_build_only = true
network_policy = "deny_all"
declared_hosts = []

[[packages]]
alias = "registry"
kind = "registry_package"
id = "pkg.temporal.registry"
version = "1.0.0"
content_hash = "blake3:1111111111111111111111111111111111111111111111111111111111111111"

[[packages]]
alias = "strategy"
kind = "strategy_package"
id = "pkg.temporal.strategy"
version = "1.0.0"
content_hash = "blake3:2222222222222222222222222222222222222222222222222222222222222222"

[[packages]]
alias = "profile"
kind = "entity_profile_package"
id = "pkg.temporal.profile"
version = "1.0.0"
content_hash = "blake3:3333333333333333333333333333333333333333333333333333333333333333"

[[packages]]
alias = "mapping"
kind = "source_mapping_package"
id = "pkg.temporal.mapping"
version = "1.0.0"
content_hash = "blake3:4444444444444444444444444444444444444444444444444444444444444444"

[[packages]]
alias = "provider_facts"
kind = "extension_package"
id = "pkg.temporal.provider_facts"
version = "1.0.0"
content_hash = "blake3:5555555555555555555555555555555555555555555555555555555555555555"

[[packages]]
alias = "reviewed_facts"
kind = "extension_package"
id = "pkg.temporal.reviewed_facts"
version = "1.0.0"
content_hash = "blake3:6666666666666666666666666666666666666666666666666666666666666666"

[[packages]]
alias = "trust_policy"
kind = "extension_package"
id = "pkg.temporal.trust_policy"
version = "1.0.0"
content_hash = "blake3:7777777777777777777777777777777777777777777777777777777777777777"

[[packages]]
alias = "scope_vocab"
kind = "extension_package"
id = "pkg.temporal.scope_vocab"
version = "1.0.0"
content_hash = "blake3:8888888888888888888888888888888888888888888888888888888888888888"

[[packages]]
alias = "relation_projection"
kind = "extension_package"
id = "pkg.temporal.relation_projection"
version = "1.0.0"
content_hash = "blake3:9999999999999999999999999999999999999999999999999999999999999999"

[[sources]]
source_id = "provider_feed"
path = "feeds/provider.csv"
format = "csv"
mapping_package = "mapping"
mapping_profile = "pkg.temporal:provider"
required = true

[[sources]]
source_id = "review_feed"
path = "feeds/review.jsonl"
format = "jsonl"
mapping_package = "mapping"
mapping_profile = "pkg.temporal:review"
required = true

[[outputs]]
output_id = "summary"
kind = "summary_json"
path = "out/summary.json"
redact_identity = true

[[outputs]]
output_id = "review_queue"
kind = "review_queue_csv"
path = "out/review.csv"
redact_identity = true

[[modes]]
mode_id = "cluster_temporal"
kind = "cluster"
source_ids = ["provider_feed", "review_feed"]
registry_package = "registry"
strategy_package = "strategy"
profile_package = "profile"
output_ids = ["summary", "review_queue"]
"#;
