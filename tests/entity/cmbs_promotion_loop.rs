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
        audit::{
            EntityAuditArtifact, EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite,
            run_entity_audit,
        },
        patches::{CannotLinkSidecarRecord, RelationReviewPatch, ReviewPatchBundle},
        profiles::cmbs::{
            CMBS_TENANT_CANONICAL_TYPE, CMBS_TENANT_IDENTITY_SEMANTICS, CMBS_TENANT_PROFILE_ID,
            CMBS_TENANT_PROFILE_VERSION, CmbsTenantIdAllocation, CmbsTenantIdAllocationRequest,
            CmbsTenantIdAllocator,
        },
        promote::{
            EntityPromoteRegistryRequest, EntityPromotedAlias, EntityPromotionAuditExpectation,
            promote_registry_aliases,
        },
        schema::{
            CANON_ENTITY_PROMOTION_PROOF_VERSION, CANON_ENTITY_PROMOTION_SIDECAR_VERSION,
            CANON_ENTITY_REVIEW_QUEUE_VERSION,
        },
        sidecar::{
            EntityPromotionSidecarArtifacts, EntityPromotionSidecarRequest,
            build_promotion_sidecar_artifacts, write_promotion_sidecar_artifacts,
        },
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

const MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/promotion_loop/manifest.json";

#[derive(Debug, Deserialize)]
struct PromotionLoopManifest {
    schema_version: String,
    registry_version_before: String,
    registry_version_after: String,
    registry_snapshot_hash: String,
    decision_ledger_hash: String,
    expected_aliases: Vec<EntityPromotedAlias>,
    expected_sidecar_counts: PromotionSidecarCounts,
    stale_audit_refusal_code: String,
    forbidden_assertion_scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PromotionSidecarCounts {
    cannot_link_count: u64,
    relation_hint_count: u64,
    promoted_alias_count: u64,
}

#[test]
fn cmbs_promotion_loop_promotes_aliases_and_review_sidecars_without_apply() {
    let manifest = manifest();
    let registry = make_registry(&manifest.registry_version_before, json!([]));
    let audit = passing_audit(&manifest);
    let aliases = promoted_aliases(&manifest);

    let promotion = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: manifest.registry_version_after.clone(),
        audit: audit.clone(),
        audit_expectation: audit_expectation(&manifest, &audit),
        aliases: aliases.clone(),
        no_lint: false,
    })
    .expect("CMBS promotion succeeds");

    assert_eq!(promotion.version, "canon_entity_promote.v0");
    assert_eq!(
        promotion.registry.version_before,
        manifest.registry_version_before
    );
    assert_eq!(
        promotion.registry.version_after,
        manifest.registry_version_after
    );
    assert_eq!(promotion.registry.entry_count_before, 0);
    assert_eq!(promotion.registry.entry_count_after, aliases.len());
    assert_eq!(promotion.aliases, aliases);
    assert_eq!(promotion.touched_files, ["aliases.json", "registry.json"]);
    assert_eq!(promotion.lint.errors, 0);

    let registry_json = read_json(&registry.path().join("registry.json"));
    assert_eq!(
        registry_json["entity_profile"]["id"],
        CMBS_TENANT_PROFILE_ID
    );
    assert_eq!(
        registry_json["entity_profile"]["identity_semantics"],
        CMBS_TENANT_IDENTITY_SEMANTICS
    );
    assert_eq!(
        registry_json["entity_profile"]["canonical_type"],
        CMBS_TENANT_CANONICAL_TYPE
    );
    assert_eq!(
        read_json(&registry.path().join("aliases.json")),
        serde_json::to_value(&manifest.expected_aliases).expect("aliases json")
    );

    let artifacts = promotion_sidecars(&manifest, &audit, promotion.aliases.len() as u64);
    assert_eq!(
        artifacts.sidecar.version,
        CANON_ENTITY_PROMOTION_SIDECAR_VERSION
    );
    assert_eq!(
        artifacts.proof.version,
        CANON_ENTITY_PROMOTION_PROOF_VERSION
    );
    assert_eq!(
        artifacts.sidecar.summary.counts["cannot_link_count"],
        manifest.expected_sidecar_counts.cannot_link_count
    );
    assert_eq!(
        artifacts.sidecar.summary.counts["relation_hint_count"],
        manifest.expected_sidecar_counts.relation_hint_count
    );
    assert_eq!(
        artifacts.proof.summary.counts["promoted_alias_count"],
        manifest.expected_sidecar_counts.promoted_alias_count
    );
    assert_eq!(
        artifacts.sidecar.source_audit_hash,
        audit.artifact_content_hash
    );
    assert_eq!(
        artifacts.sidecar.source_decision_ledger_hash,
        manifest.decision_ledger_hash
    );
    assert_eq!(
        artifacts.proof.sidecar_snapshot_hash,
        artifacts.sidecar.artifact_content_hash
    );

    let sidecar_dir = registry.path().join("promotion");
    let receipt = write_promotion_sidecar_artifacts(&sidecar_dir, &artifacts)
        .expect("promotion sidecars write");
    assert_eq!(
        receipt.sidecar_snapshot_hash,
        artifacts.sidecar.artifact_content_hash
    );
    assert!(receipt.bytes_written > 0);
    assert!(!registry.path().join("apply.csv").exists());
}

#[test]
fn cmbs_promotion_loop_stale_audit_refuses_without_mutation() {
    let manifest = manifest();
    let registry = make_registry(&manifest.registry_version_before, json!([]));
    let registry_before = file_bytes(&registry.path().join("registry.json"));
    let aliases_before = file_bytes(&registry.path().join("aliases.json"));
    let audit = passing_audit(&manifest);
    let mut expectation = audit_expectation(&manifest, &audit);
    expectation.audit_artifact_hash = "blake3:stale-cmbs-audit".to_string();

    let refusal = promote_registry_aliases(EntityPromoteRegistryRequest {
        registry: registry.path().to_path_buf(),
        alias_file: "aliases.json".to_string(),
        next_version: manifest.registry_version_after.clone(),
        audit,
        audit_expectation: expectation,
        aliases: promoted_aliases(&manifest),
        no_lint: true,
    })
    .expect_err("stale audit refuses before writes");

    assert_eq!(refusal.code, RefusalCode::EEntityAuditGate);
    assert_eq!(manifest.stale_audit_refusal_code, "E_ENTITY_AUDIT_GATE");
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
    assert!(!registry.path().join("promotion").exists());
}

#[test]
fn cmbs_tenant_id_exact_replay_promotion_loop_uses_stable_tnt_id() {
    let manifest = manifest();
    let allocation = sears_allocation(&manifest.registry_snapshot_hash);

    assert_eq!(allocation.canonical_id, "TNT-SEARS");
    assert_eq!(
        allocation.version,
        "canon_entity_cmbs_tenant_id_allocator.v0"
    );
    assert_eq!(allocation.profile.id, CMBS_TENANT_PROFILE_ID);
    assert_eq!(allocation.side_effects.registry_writes, 0);
    assert_eq!(allocation.side_effects.output_rows_written, 0);
    assert_eq!(
        promoted_aliases(&manifest)
            .iter()
            .map(|alias| alias.canonical_id.as_str())
            .collect::<Vec<_>>(),
        ["TNT-SEARS", "TNT-SEARS"]
    );
}

#[test]
#[allow(non_snake_case)]
fn ER_PROMOTE_GOLDEN_001_cmbs_promotion_loop_manifest_is_stage_scoped() {
    let manifest = manifest();

    assert_eq!(
        manifest.schema_version,
        "canon.entity.cmbs_promotion_loop.v0"
    );
    assert_eq!(manifest.expected_aliases.len(), 2);
    assert_eq!(manifest.expected_aliases[0].canonical_id, "TNT-SEARS");
    assert_eq!(
        manifest.expected_sidecar_counts.promoted_alias_count,
        manifest.expected_aliases.len() as u64
    );
    assert_eq!(
        manifest.forbidden_assertion_scopes,
        ["apply", "canonical_status"]
    );
}

fn promoted_aliases(manifest: &PromotionLoopManifest) -> Vec<EntityPromotedAlias> {
    let allocation = sears_allocation(&manifest.registry_snapshot_hash);
    manifest
        .expected_aliases
        .iter()
        .map(|alias| EntityPromotedAlias {
            input: alias.input.clone(),
            canonical_id: allocation.canonical_id.clone(),
            canonical_type: allocation.profile.canonical_type.clone(),
            rule_id: alias.rule_id.clone(),
        })
        .collect()
}

fn sears_allocation(registry_snapshot_hash: &str) -> CmbsTenantIdAllocation {
    CmbsTenantIdAllocator::default()
        .allocate(&CmbsTenantIdAllocationRequest::new(
            "Sears",
            "sears",
            registry_snapshot_hash,
            "blake3:cmbs-sears-alias-patch",
            "review:surf_sears",
        ))
        .expect("Sears tenant allocation succeeds")
}

fn promotion_sidecars(
    manifest: &PromotionLoopManifest,
    audit: &EntityAuditArtifact,
    promoted_alias_count: u64,
) -> EntityPromotionSidecarArtifacts {
    build_promotion_sidecar_artifacts(EntityPromotionSidecarRequest {
        metadata: audit.metadata.clone(),
        source_audit_hash: audit.artifact_content_hash.clone(),
        source_decision_ledger_hash: manifest.decision_ledger_hash.clone(),
        patch_bundle: ReviewPatchBundle {
            alias_patches: vec![],
            distinct_patches: vec![],
            relation_patches: vec![RelationReviewPatch {
                patch_id: "relation:pnc_midland".to_string(),
                profile_id: CMBS_TENANT_PROFILE_ID.to_string(),
                identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
                namespace: "cmbs_tenant_label.relations".to_string(),
                left: "surf:pnc_bank".to_string(),
                right: "surf:pnc_midland_loan_services".to_string(),
                relation: "master_special_servicer_platform".to_string(),
                review_decision_id: "decision:relation:pnc_midland".to_string(),
                source_event_hash: "blake3:pnc-relation-event".to_string(),
            }],
            cannot_link_sidecars: vec![CannotLinkSidecarRecord {
                sidecar_id: "cannot_link:sears_auto".to_string(),
                profile_id: CMBS_TENANT_PROFILE_ID.to_string(),
                identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
                left: "surf:sears".to_string(),
                right: "surf:sears_auto".to_string(),
                hard_cannot_link: true,
                reason: "review_distinct_confirmed".to_string(),
                review_decision_id: "decision:distinct:sears_auto".to_string(),
                source_event_hash: "blake3:sears-distinct-event".to_string(),
            }],
            override_records: vec![],
        },
        escrow_entities: vec![],
        contradiction_entities: vec![],
        promoted_alias_count,
    })
    .expect("CMBS promotion sidecars build")
}

fn passing_audit(manifest: &PromotionLoopManifest) -> EntityAuditArtifact {
    let result = solve_header(&manifest.registry_snapshot_hash);
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
    manifest: &PromotionLoopManifest,
    audit: &EntityAuditArtifact,
) -> EntityPromotionAuditExpectation {
    EntityPromotionAuditExpectation {
        audit_artifact_hash: audit.artifact_content_hash.clone(),
        audited_artifact_hash: "blake3:cmbs-solve".to_string(),
        profile_id: CMBS_TENANT_PROFILE_ID.to_string(),
        profile_version: CMBS_TENANT_PROFILE_VERSION.to_string(),
        strategy_hash: "blake3:cmbs-strategy".to_string(),
        registry_snapshot_hash: manifest.registry_snapshot_hash.clone(),
        required_gate_ids: vec!["G14".to_string()],
    }
}

fn solve_header(registry_snapshot_hash: &str) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION.to_string(),
        metadata: metadata(registry_snapshot_hash),
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 1)]),
            labels: BTreeMap::new(),
        },
    }
}

fn metadata(registry_snapshot_hash: &str) -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: CMBS_TENANT_PROFILE_ID.to_string(),
            version: CMBS_TENANT_PROFILE_VERSION.to_string(),
            entity_type: "tenant_label".to_string(),
            identity_semantics: CMBS_TENANT_IDENTITY_SEMANTICS.to_string(),
            canonical_type: CMBS_TENANT_CANONICAL_TYPE.to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "cmbs_tenant_label.aliases".to_string(),
                distinct: "cmbs_tenant_label.distinct".to_string(),
                relations: "cmbs_tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:cmbs-profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "cmbs_tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:cmbs-strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "cmbs-tenants".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/cmbs-tenants".to_string(),
            lookup_snapshot_hash: registry_snapshot_hash.to_string(),
            sidecar_snapshot_hash: Some("blake3:cmbs-sidecars".to_string()),
        },
        patch_namespace: "cmbs_tenant_label.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 153,
            content_hash: "blake3:cmbs-input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: "blake3:cmbs-solve".to_string(),
    }
}

fn certified_artifacts() -> Vec<EntityArtifactReference> {
    vec![
        EntityArtifactReference {
            version: CANON_ENTITY_SOLVE_VERSION.to_string(),
            content_hash: "blake3:cmbs-solve".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_REVIEW_QUEUE_VERSION.to_string(),
            content_hash: "blake3:cmbs-review-queue".to_string(),
        },
        EntityArtifactReference {
            version: CANON_ENTITY_DECISION_LEDGER_VERSION.to_string(),
            content_hash: "blake3:cmbs-review-ledger".to_string(),
        },
    ]
}

fn passing_suite() -> EntityAuditSuite {
    EntityAuditSuite {
        id: "cmbs_promotion_loop".to_string(),
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
        "description": "CMBS promotion loop fixture registry",
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

fn manifest() -> PromotionLoopManifest {
    serde_json::from_slice(&fs::read(repo_path(MANIFEST_PATH)).expect("manifest bytes"))
        .expect("manifest parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}

fn file_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read file")
}
