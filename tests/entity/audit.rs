#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_AUDIT_VERSION, CANON_ENTITY_DECISION_LEDGER_VERSION,
        CANON_ENTITY_SOLVE_VERSION, EntityArtifactHeader, EntityArtifactMetadata,
        EntityArtifactReference, EntityDeterministicSummary, EntityInputReference,
        EntityPatchNamespaces, EntityProfileReference, EntityRegistrySnapshot,
        EntityStrategyReference,
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite, run_entity_audit},
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn entity_audit_emits_passed_artifact_with_hash_continuity() {
    let result = solve_header();
    let audit = run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: certified_artifacts(),
        result,
        suite: passing_suite(),
    })
    .expect("audit passes");

    assert_eq!(audit.version, CANON_ENTITY_AUDIT_VERSION);
    assert!(audit.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        audit.metadata.artifact_content_hash,
        audit.artifact_content_hash
    );
    assert_eq!(audit.audited_artifact.version, CANON_ENTITY_SOLVE_VERSION);
    assert_eq!(audit.audited_artifact.content_hash, "blake3:solve");
    assert_eq!(audit.metadata.profile.id, "cmbs_tenant_label");
    assert_eq!(audit.metadata.strategy.content_hash, "blake3:strategy");
    assert_eq!(
        audit.metadata.registry_snapshot.lookup_snapshot_hash,
        "blake3:registry"
    );
    assert_eq!(
        AuditProjection::from_audit(&audit),
        expected_audit_projection()
    );
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_AUDIT_GATE_failed_gate_refuses_before_artifact() {
    let result = solve_header();
    let mut suite = passing_suite();
    suite.gates.push(EntityAuditGateCheck {
        gate_id: "G14".to_string(),
        label: "promotion gate".to_string(),
        passed: false,
        expected: "audit_status=passed".to_string(),
        actual: "audit_status=failed".to_string(),
        evidence: BTreeMap::from([("failure".to_string(), "stale audit".to_string())]),
    });

    let refusal = run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: certified_artifacts(),
        result,
        suite,
    })
    .expect_err("failed gate refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["gate_id"], "G14");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn promotion_gate_registry_snapshot_mismatch_uses_registry_refusal() {
    let result = solve_header();
    let mut expected = EntityArtifactChainExpectation::from_link(
        EntityChainStage::Audit,
        &EntityArtifactChainLink::from_header(&result),
    );
    expected.registry_snapshot_hash = "blake3:other-registry".to_string();

    let refusal = run_entity_audit(EntityAuditRequest {
        expected,
        certified_artifacts: certified_artifacts(),
        result,
        suite: passing_suite(),
    })
    .expect_err("registry mismatch refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityRegistrySnapshot);
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn entity_audit_wrong_artifact_version_uses_artifact_contract_refusal() {
    let result = solve_header();
    let mut expected = EntityArtifactChainExpectation::from_link(
        EntityChainStage::Audit,
        &EntityArtifactChainLink::from_header(&result),
    );
    expected.expected_version = "canon_entity_run.v0".to_string();

    let refusal = run_entity_audit(EntityAuditRequest {
        expected,
        certified_artifacts: certified_artifacts(),
        result,
        suite: passing_suite(),
    })
    .expect_err("version mismatch refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["field"], "artifact_version");
    assert_eq!(refusal.detail["writes_performed"], false);
}

fn solve_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([
                ("entity_count".to_string(), 2),
                ("review_group_count".to_string(), 1),
            ]),
            labels: BTreeMap::from([(
                "decision_ledger".to_string(),
                "required_before_review_import_or_promotion".to_string(),
            )]),
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
            row_count: 153,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: "canon_entity_block.v0".to_string(),
                content_hash: "blake3:block".to_string(),
            },
            EntityArtifactReference {
                version: "canon_entity_edge.v0".to_string(),
                content_hash: "blake3:edge".to_string(),
            },
        ],
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
        id: "cmbs_tenant_smoke".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![
            EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "all_hashes_match".to_string(),
                actual: "all_hashes_match".to_string(),
                evidence: BTreeMap::from([("artifact".to_string(), "blake3:solve".to_string())]),
            },
            EntityAuditGateCheck {
                gate_id: "G08".to_string(),
                label: "review grouping".to_string(),
                passed: true,
                expected: "grouped_by_ambiguity".to_string(),
                actual: "grouped_by_ambiguity".to_string(),
                evidence: BTreeMap::from([("review_items".to_string(), "1".to_string())]),
            },
            EntityAuditGateCheck {
                gate_id: "G09".to_string(),
                label: "decision ledger continuity".to_string(),
                passed: true,
                expected: "continuous_jsonl".to_string(),
                actual: "continuous_jsonl".to_string(),
                evidence: BTreeMap::from([("events_replayed".to_string(), "2".to_string())]),
            },
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AuditProjection {
    version: String,
    suite_id: String,
    suite_version: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    audited_artifact: EntityArtifactReference,
    certified_artifacts: Vec<EntityArtifactReference>,
    gate_ids: Vec<String>,
}

impl AuditProjection {
    fn from_audit(audit: &canon::entity::audit::EntityAuditArtifact) -> Self {
        Self {
            version: audit.version.clone(),
            suite_id: audit.suite_id.clone(),
            suite_version: audit.suite_version.clone(),
            summary_counts: audit.summary.counts.clone(),
            summary_labels: audit.summary.labels.clone(),
            audited_artifact: audit.audited_artifact.clone(),
            certified_artifacts: audit.certified_artifacts.clone(),
            gate_ids: audit
                .gates
                .iter()
                .map(|gate| gate.gate_id.clone())
                .collect(),
        }
    }
}

fn expected_audit_projection() -> AuditProjection {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/audit/passed_expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}
