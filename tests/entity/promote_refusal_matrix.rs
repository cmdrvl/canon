#![forbid(unsafe_code)]

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
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const PROMOTE_REFUSAL_MATRIX: &str =
    include_str!("../fixtures/entity/promote/refusals/promote_refusal_matrix.json");

#[derive(Debug, Deserialize)]
struct PromoteRefusalMatrix {
    version: String,
    cases: Vec<PromoteRefusalCase>,
}

#[derive(Debug, Deserialize)]
struct PromoteRefusalCase {
    id: String,
    refusal_code: String,
    required_detail_fields: Vec<String>,
    writes_performed: bool,
}

#[test]
fn promote_refusal_matrix_fixture_locks_promote_stage_cases() {
    let matrix: PromoteRefusalMatrix =
        serde_json::from_str(PROMOTE_REFUSAL_MATRIX).expect("matrix fixture");
    assert_eq!(matrix.version, "canon_entity_promote_refusal_matrix.v0");
    let by_id = matrix
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for (id, code) in [
        ("promote_stale_audit", "E_ENTITY_AUDIT_GATE"),
        (
            "promote_registry_snapshot_mismatch",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        (
            "promote_atomic_temp_collision",
            "E_ENTITY_ARTIFACT_CONTRACT",
        ),
        ("promote_idempotent_replay", "NONE"),
    ] {
        let case = by_id.get(id).unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(case.refusal_code, code);
        assert!(
            case.required_detail_fields
                .iter()
                .any(|field| field == "writes_performed"),
            "{id} must require writes_performed"
        );
        assert!(
            !case.writes_performed,
            "{id} must not mutate protected artifacts during refusal or replay"
        );
    }
}

#[test]
fn promote_refusal_no_mutation_on_stale_audit() {
    let registry = make_registry("1.0.0", json!([]));
    let before = registry_tree_hash(registry.path());
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
    assert_eq!(registry_tree_hash(registry.path()), before);
}

#[test]
fn promote_refusal_no_mutation_on_atomic_registry_temp_collision() {
    let registry = make_registry("1.0.0", json!([]));
    let registry_temp = registry.path().join("registry.json.canon-promote.tmp");
    fs::write(&registry_temp, b"preexisting temp").expect("registry temp");
    let before = registry_tree_hash(registry.path());
    let audit = passing_audit();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect_err("registry temp collision refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(registry_tree_hash(registry.path()), before);
    assert_eq!(
        fs::read_to_string(&registry_temp).expect("registry temp after"),
        "preexisting temp"
    );
}

#[test]
fn promote_idempotent_registry_write_leaves_tree_unchanged() {
    let registry = make_registry("1.0.0", json!([]));
    let audit = passing_audit();
    let first = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect("initial promotion succeeds");
    let after_first = registry_tree_hash(registry.path());

    let replay = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&audit),
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect("idempotent replay succeeds");

    assert_eq!(first.registry.version_after, "1.0.1");
    assert_eq!(replay.registry.version_before, "1.0.1");
    assert_eq!(replay.registry.version_after, "1.0.1");
    assert_eq!(
        replay.registry.entry_count_before,
        replay.registry.entry_count_after
    );
    assert!(replay.touched_files.is_empty());
    assert_eq!(replay.aliases, vec![sears_alias()]);
    assert_eq!(registry_tree_hash(registry.path()), after_first);
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
        id: "promotion_refusal_matrix".to_string(),
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
