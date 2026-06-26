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
        schema::CANON_ENTITY_REVIEW_QUEUE_VERSION,
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs};

const AUDIT_REFUSAL_MATRIX: &str =
    include_str!("../fixtures/entity/audit/refusals/audit_refusal_matrix.json");

#[derive(Debug, Deserialize)]
struct AuditRefusalMatrix {
    version: String,
    cases: Vec<AuditRefusalCase>,
}

#[derive(Debug, Deserialize)]
struct AuditRefusalCase {
    id: String,
    refusal_code: String,
    required_detail_fields: Vec<String>,
    writes_performed: bool,
}

#[test]
fn audit_refusal_matrix_fixture_locks_audit_stage_cases() {
    let matrix: AuditRefusalMatrix =
        serde_json::from_str(AUDIT_REFUSAL_MATRIX).expect("matrix fixture");
    assert_eq!(matrix.version, "canon_entity_audit_refusal_matrix.v0");
    let by_id = matrix
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for (id, code) in [
        (
            "audit_stale_registry_snapshot",
            "E_ENTITY_REGISTRY_SNAPSHOT",
        ),
        ("audit_missing_certified_result", "E_ENTITY_AUDIT_GATE"),
        ("audit_failed_required_gate", "E_ENTITY_AUDIT_GATE"),
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
            "{id} must refuse before writing downstream artifacts"
        );
    }
}

#[test]
fn audit_registry_snapshot_refusal_preserves_registry_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry_file = temp.path().join("registry.json");
    fs::write(&registry_file, "{\"version\":\"2026.06.25\"}\n").expect("registry");
    let registry_before = fs::read_to_string(&registry_file).expect("registry before");

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
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["field"], "registry_snapshot_hash");
    assert_eq!(refusal.detail["expected"], "blake3:other-registry");
    assert_eq!(refusal.detail["actual"], "blake3:registry");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert_eq!(
        fs::read_to_string(&registry_file).expect("registry after"),
        registry_before
    );
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_AUDIT_GATE_missing_certified_result_refuses_before_artifact() {
    let result = solve_header();
    let refusal = run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: vec![EntityArtifactReference {
            version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
            content_hash: "blake3:review-queue".to_string(),
        }],
        result,
        suite: passing_suite(),
    })
    .expect_err("missing certified result refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["field"], "certified_artifacts");
    assert_eq!(
        refusal.detail["expected"],
        "canon_entity_solve.v0@blake3:solve"
    );
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
#[allow(non_snake_case)]
fn E_ENTITY_AUDIT_GATE_failed_required_gate_refuses_before_artifact() {
    let result = solve_header();
    let mut suite = passing_suite();
    suite.gates.push(EntityAuditGateCheck {
        gate_id: "G14".to_string(),
        label: "promotion gate".to_string(),
        passed: false,
        expected: "audit_status=passed".to_string(),
        actual: "audit_status=failed".to_string(),
        evidence: BTreeMap::from([("reason".to_string(), "registry snapshot stale".to_string())]),
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
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(refusal.detail["stage"], "audit");
    assert_eq!(refusal.detail["gate_id"], "G14");
    assert_eq!(refusal.detail["writes_performed"], false);
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
        id: "audit_refusal_matrix".to_string(),
        version: "2026.06.26".to_string(),
        gates: vec![EntityAuditGateCheck {
            gate_id: "G01".to_string(),
            label: "artifact continuity".to_string(),
            passed: true,
            expected: "all_hashes_match".to_string(),
            actual: "all_hashes_match".to_string(),
            evidence: BTreeMap::new(),
        }],
    }
}
