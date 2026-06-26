#![forbid(unsafe_code)]

use assert_cmd::Command;
use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_DECISION_LEDGER_VERSION, CANON_ENTITY_SOLVE_VERSION, EntityArtifactHeader,
        EntityArtifactMetadata, EntityArtifactReference, EntityDeterministicSummary,
        EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
        EntityRegistrySnapshot, EntityStrategyReference,
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
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

fn canon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_canon"))
}

#[test]
#[allow(non_snake_case)]
fn EN_PR002_promoted_alias_resolves_through_ordinary_exact_lookup() {
    let registry = make_registry("1.0.0", json!([]));
    let audit = passing_audit();
    let output = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: vec![sears_alias()],
        no_lint: false,
    })
    .expect("promotion succeeds");

    assert_eq!(output.registry.version_before, "1.0.0");
    assert_eq!(output.registry.version_after, "1.0.1");
    assert_eq!(output.registry.entry_count_after, 1);
    assert_eq!(output.aliases, vec![sears_alias()]);
    assert!(output.lint.enabled);
    assert_eq!(output.lint.errors, 0);

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(registry_json["version"], "1.0.1");
    assert_eq!(registry_json["entry_count"], 1);
    let aliases = read_json(&registry.path().join("aliases.json"));
    assert_eq!(aliases[0]["input"], "Sears");
    assert_eq!(aliases[0]["canonical_id"], "TNT-SEARS");
    assert_eq!(aliases[0]["canonical_type"], "tenant_label");
    assert_eq!(aliases[0]["rule_id"], "ENTITY_REVIEW_PROMOTE");

    let input_path = registry.path().join("input.csv");
    fs::write(&input_path, "tenant\nSears\n").expect("input csv");
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
    assert_eq!(payload["mappings"][0]["canonical_id"], "u8:TNT-SEARS");
}

#[test]
fn promotion_refuses_without_matching_passing_audit_before_registry_write() {
    let registry = make_registry("1.0.0", json!([]));
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));
    let audit = passing_audit();
    let mut expectation = audit_expectation(&audit);
    expectation.audit_artifact_hash = "blake3:stale-audit".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit,
        audit_expectation: expectation,
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect_err("stale audit refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "audit_artifact_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );
}

#[test]
fn promotion_registry_snapshot_mismatch_refuses_before_registry_write() {
    let registry = make_registry("1.0.0", json!([]));
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));
    let audit = passing_audit();
    let mut expectation = audit_expectation(&audit);
    expectation.registry_snapshot_hash = "blake3:other-registry".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit,
        audit_expectation: expectation,
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect_err("registry snapshot mismatch refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        file_bytes(&registry.path().join("registry.json")),
        registry_before
    );
    assert_eq!(
        file_bytes(&registry.path().join("aliases.json")),
        aliases_before
    );
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

fn sears_alias() -> EntityPromotedAlias {
    EntityPromotedAlias {
        input: "Sears".to_string(),
        canonical_id: "TNT-SEARS".to_string(),
        canonical_type: "tenant_label".to_string(),
        rule_id: "ENTITY_REVIEW_PROMOTE".to_string(),
    }
}

fn solve_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 1)]),
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
            row_count: 1,
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
        id: "promotion_smoke".to_string(),
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

fn file_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read file")
}
