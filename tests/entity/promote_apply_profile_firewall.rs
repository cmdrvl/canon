#![forbid(unsafe_code)]

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
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use tempfile::TempDir;

#[test]
fn promote_apply_profile_firewall_refuses_cross_profile_promotion_before_write() {
    let registry = make_registry("1.0.0", json!([]));
    let registry_before =
        fs::read_to_string(registry.path().join("registry.json")).expect("registry");
    let aliases_before = fs::read_to_string(registry.path().join("aliases.json")).expect("aliases");
    let audit = passing_audit();
    let mut expectation = audit_expectation(&audit);
    expectation.profile_id = "regab_firm_identity".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: "1.0.1".to_string(),
        audit,
        audit_expectation: expectation,
        aliases: vec![sears_alias()],
        no_lint: true,
    })
    .expect_err("cross-profile promotion refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "profile_id");
    assert_eq!(refusal.detail["expected"], "regab_firm_identity");
    assert_eq!(refusal.detail["actual"], "cmbs_tenant_label");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(
        fs::read_to_string(registry.path().join("registry.json")).expect("registry after"),
        registry_before
    );
    assert_eq!(
        fs::read_to_string(registry.path().join("aliases.json")).expect("aliases after"),
        aliases_before
    );
}

#[test]
fn apply_profile_mismatch_refusal_preserves_raw_input_and_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rows = temp.path().join("rows.csv");
    let output = temp.path().join("rows.canon.csv");
    let raw_rows = "loan_id,tenant_name\nL-001,Sears\n";
    fs::write(&rows, raw_rows).expect("rows");
    fs::write(&output, "sentinel output\n").expect("output sentinel");
    let output_before = fs::read_to_string(&output).expect("output before");

    let refusal = run_apply_streaming(ApplyStreamRequest {
        rows: &rows,
        output: &output,
        lookup_column: "tenant_name",
        registry: ApplyRegistryReference {
            id: "cmbs-tenants".to_string(),
            version: "1.0.1".to_string(),
        },
        resolutions: &BTreeMap::from([(
            "Sears".to_string(),
            ApplyCanonicalResolution {
                canonical_id: "TNT-SEARS".to_string(),
                canonical_type: "tenant_label".to_string(),
                rule_id: "REGISTRY_EXACT".to_string(),
            },
        )]),
        safety: ApplySafetyCheck {
            expected_profile_id: Some("cmbs_tenant_label".to_string()),
            actual_profile_id: Some("regab_firm_identity".to_string()),
            expected_identity_semantics: Some("canonical_display_label".to_string()),
            actual_identity_semantics: Some("same_firm_or_reviewed_alias".to_string()),
            ..ApplySafetyCheck::default()
        },
        require_full_resolution: true,
        target_rows_per_chunk: 1024,
    })
    .expect_err("cross-profile apply refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "apply");
    assert_eq!(refusal.detail["field"], "profile_id");
    assert_eq!(refusal.detail["expected"], "cmbs_tenant_label");
    assert_eq!(refusal.detail["actual"], "regab_firm_identity");
    assert_eq!(refusal.detail["writes_performed"], false);
    assert!(
        refusal
            .next_command
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(fs::read_to_string(&rows).expect("rows after"), raw_rows);
    assert_eq!(
        fs::read_to_string(&output).expect("output after"),
        output_before
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
        id: "profile_firewall".to_string(),
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
