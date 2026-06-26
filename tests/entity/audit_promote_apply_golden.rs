#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_SOLVE_VERSION, EntityArtifactHeader,
        EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
        EntityRegistrySnapshot, EntityStrategyReference,
        apply::{
            ApplyCanonicalResolution, ApplyRegistryReference, ApplySafetyCheck, ApplyStreamRequest,
            run_apply_streaming,
        },
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite, run_entity_audit},
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const SCENARIOS: &str =
    include_str!("../fixtures/entity/promote/audit_promote_apply/scenarios.json");
const EXPECTED_ALIASES: &str =
    include_str!("../fixtures/entity/promote/audit_promote_apply/en_pr002_aliases_expected.json");
const EXPECTED_APPLY_CSV: &str =
    include_str!("../fixtures/entity/apply/en_a001_after_promotion_expected.csv");
const PROTECTED_SIDECAR: &str =
    include_str!("../fixtures/entity/promote/audit_promote_apply/en_pr001_protected_sidecar.json");
const PROTECTED_LEDGER: &str =
    include_str!("../fixtures/entity/promote/audit_promote_apply/en_pr001_decision_ledger.jsonl");

#[derive(Debug, Deserialize)]
struct ScenarioManifest {
    version: String,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    assertion: String,
}

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
fn audit_promote_apply_manifest_names_real_assertions() {
    let manifest: ScenarioManifest = serde_json::from_str(SCENARIOS).expect("manifest parses");
    assert_eq!(
        manifest.version,
        "canon_entity_audit_promote_apply_golden.v0"
    );
    let assertions = manifest
        .scenarios
        .iter()
        .map(|scenario| (scenario.id.as_str(), scenario.assertion.as_str()))
        .collect::<BTreeMap<_, _>>();

    for id in ["EN-PR001", "EN-PR002", "EN-A001"] {
        let assertion = assertions.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert!(
            !assertion.trim().is_empty() && !assertion.contains("exists"),
            "{id} must describe behavior, not file existence"
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn EN_PR001_stale_audit_refuses_and_preserves_registry_sidecar_and_ledger() {
    let registry = make_registry("1.0.0", json!([]));
    let sidecar_path = registry.path().join("promotion-sidecars.json");
    let ledger_path = registry.path().join("decision-ledger.jsonl");
    fs::write(&sidecar_path, PROTECTED_SIDECAR).expect("sidecar");
    fs::write(&ledger_path, PROTECTED_LEDGER).expect("ledger");
    let registry_before = registry_tree_hash(registry.path());
    let sidecar_before = fs::read_to_string(&sidecar_path).expect("sidecar before");
    let ledger_before = fs::read_to_string(&ledger_path).expect("ledger before");
    let audit = passing_audit();
    let mut expectation = audit_expectation(&audit);
    expectation.audit_artifact_hash = "blake3:stale-audit".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit,
        audit_expectation: expectation,
        aliases: promoted_aliases(),
        no_lint: true,
    })
    .expect_err("stale audit refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["field"], "audit_artifact_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(registry_tree_hash(registry.path()), registry_before);
    assert_eq!(
        fs::read_to_string(&sidecar_path).expect("sidecar after"),
        sidecar_before
    );
    assert_eq!(
        fs::read_to_string(&ledger_path).expect("ledger after"),
        ledger_before
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_PR002_promotes_aliases_with_version_entry_count_lint_and_exact_lookup() {
    let registry = make_registry("1.0.0", json!([]));
    let audit = passing_audit();

    let output = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: promoted_aliases(),
        no_lint: false,
    })
    .expect("promotion succeeds");

    assert_eq!(output.registry.version_before, "1.0.0");
    assert_eq!(output.registry.version_after, "1.0.1");
    assert_eq!(output.registry.entry_count_after, 2);
    assert!(output.lint.enabled);
    assert_eq!(output.lint.errors, 0);

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.0.1");
    assert_eq!(registry_json["entry_count"], 2);
    let aliases = read_json(&registry.path().join("aliases.json"));
    let expected_aliases: Value =
        serde_json::from_str(EXPECTED_ALIASES).expect("expected aliases fixture");
    assert_eq!(aliases, expected_aliases);

    let input_path = registry.path().join("lookup.csv");
    fs::write(&input_path, "tenant\n\"SEARS, LLC\"\nKmart\n").expect("lookup input");
    let resolve = canon_command()
        .args([
            input_path.to_str().expect("input path"),
            "--registry",
            registry.path().to_str().expect("registry path"),
            "--column",
            "tenant",
            "--no-witness",
            "--explicit",
        ])
        .assert()
        .success();
    let payload: Value = serde_json::from_slice(resolve.get_output().stdout.as_slice())
        .expect("resolve stdout json");
    assert_eq!(payload["outcome"], "RESOLVED");
    assert_eq!(payload["summary"]["resolved"], 2);
}

#[test]
#[allow(non_snake_case)]
fn EN_A001_apply_after_promotion_preserves_raw_fields_and_appends_canonical_metadata() {
    let registry = make_registry("1.0.0", json!([]));
    let audit = passing_audit();
    promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: promoted_aliases(),
        no_lint: true,
    })
    .expect("promotion succeeds");

    let rows = registry.path().join("tenants.csv");
    let output = registry.path().join("tenants.canon.csv");
    let raw_rows = concat!(
        "loan_id,tenant_name,as_reported_amount\n",
        "L-001,\"SEARS, LLC\",10\n",
        "L-002,Kmart,20\n",
    );
    fs::write(&rows, raw_rows).expect("rows");

    let artifact = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "1.0.1".to_string(),
        },
        resolutions: &apply_resolutions(),
        safety: ApplySafetyCheck {
            expected_registry_snapshot_hash: Some("blake3:registry".to_string()),
            actual_registry_snapshot_hash: Some("blake3:registry".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect("apply succeeds");

    assert_eq!(fs::read_to_string(&rows).expect("raw rows"), raw_rows);
    assert_eq!(
        fs::read_to_string(&output).expect("apply output"),
        EXPECTED_APPLY_CSV
    );
    assert_eq!(artifact.version, "canon_entity_apply.v0");
    assert_eq!(artifact.summary["rows"], 2);
    assert_eq!(artifact.summary["resolved"], 2);
    assert_eq!(artifact.summary["unresolved"], 0);
}

fn promoted_aliases() -> Vec<EntityPromotedAlias> {
    vec![
        EntityPromotedAlias {
            input: "SEARS, LLC".to_string(),
            canonical_id: "TNT-SEARS".to_string(),
            canonical_type: "tenant_label".to_string(),
            rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
        },
        EntityPromotedAlias {
            input: "Kmart".to_string(),
            canonical_id: "TNT-KMART".to_string(),
            canonical_type: "tenant_label".to_string(),
            rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
        },
    ]
}

fn apply_resolutions() -> BTreeMap<String, ApplyCanonicalResolution> {
    promoted_aliases()
        .into_iter()
        .map(|alias| {
            (
                alias.input,
                ApplyCanonicalResolution {
                    canonical_id: alias.canonical_id,
                    canonical_type: alias.canonical_type,
                    rule_id: "REGISTRY_EXACT".to_string(),
                },
            )
        })
        .collect()
}

fn passing_audit() -> canon::entity::audit::EntityAuditArtifact {
    let result = solve_header();
    run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: certified_artifacts(),
        result,
        suite: passing_suite(),
    })
    .expect("audit passes")
}

fn audit_expectation(
    audit: &canon::entity::audit::EntityAuditArtifact,
) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: "blake3:solve".to_string(),
        profile_id: "cmbs_tenant_label".to_string(),
        profile_version: "0.1.0".to_string(),
        strategy_hash: "blake3:strategy".to_string(),
        registry_snapshot_hash: "blake3:registry".to_string(),
        required_gate_ids: vec!["G14".to_string()],
    }
}

fn solve_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 2)]),
            labels: BTreeMap::new(),
        },
    }
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 2,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: "blake3:solve".to_string(),
    }
}

fn certified_artifacts() -> Vec<EntityArtifactReference> {
    vec![
        EntityArtifactReference {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            content_hash: "blake3:solve".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
            content_hash: "blake3:review-queue".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
            content_hash: "blake3:decision-ledger".to_string(),
        },
    ]
}

fn passing_suite() -> EntityAuditSuite {
    EntityAuditSuite {
        id: "audit_promote_apply_golden".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "all_hashes_match".to_string(),
                actual: "all_hashes_match".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G09".to_string(),
                label: "decision ledger continuity".to_string(),
                passed: true,
                expected: "continuous_jsonl".to_string(),
                actual: "continuous_jsonl".to_string(),
                evidence: BTreeMap::new(),
            },
            EntityAuditGateCheck {
                gate_id: "G14".to_string(),
                label: "promotion gate".to_string(),
                passed: true,
                expected: "audit_status=passed".to_string(),
                actual: "audit_status=passed".to_string(),
                evidence: BTreeMap::new(),
            },
        ],
    }
}

fn make_registry(version: &str, entries: Value) -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    let entry_count = entries.as_array().expect("entries array").len();
    let registry = json!({
        "id": "cmbs-tenants",
        "version": version,
        "description": "entity promote registry fixture",
        "updated": "2026-06-26",
        "entry_count": entry_count,
        "owner": "test-suite"
    });
    fs::write(
        temp.path().join("registry.json"),
        serde_json::to_vec_pretty(&registry).expect("registry json"),
    )
    .expect("write registry");
    fs::write(
        temp.path().join("aliases.json"),
        serde_json::to_vec_pretty(&entries).expect("aliases json"),
    )
    .expect("write aliases");
    temp
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}

fn registry_tree_hash(path: &Path) -> String {
    let mut files = Vec::<PathBuf>::new();
    for entry in fs::read_dir(path).expect("read registry dir") {
        let path = entry.expect("dir entry").path();
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();

    let mut hasher = blake3::Hasher::new();
    for file in files {
        let relative = file
            .file_name()
            .and_then(|name| name.to_str())
            .expect("file name");
        hasher.update(relative.as_bytes());
        hasher.update(b"\0");
        hasher.update(&fs::read(&file).expect("file bytes"));
        hasher.update(b"\0");
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}
