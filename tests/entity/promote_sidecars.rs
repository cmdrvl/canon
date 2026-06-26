#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityInputReference, EntityPatchNamespaces,
        EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
        patches::{CannotLinkSidecarRecord, RelationReviewPatch, ReviewPatchBundle},
        schema::{CANON_ENTITY_PROMOTION_PROOF_VERSION, CANON_ENTITY_PROMOTION_SIDECAR_VERSION},
        sidecar::{
            EntityContradictionSidecarRecord, EntityEscrowSidecarRecord,
            EntityPromotionProofArtifact, EntityPromotionSidecarArtifact,
            EntityPromotionSidecarRequest, build_promotion_sidecar_artifacts,
            write_promotion_sidecar_artifacts,
        },
    },
};
use serde_json::{Value, json};
use std::fs;

const EXPECTED_PROJECTION: &str =
    include_str!("../fixtures/entity/promote/sidecars/expected_projection.json");

#[test]
fn entity_promote_sidecars_writes_profile_scoped_hash_linked_artifacts() {
    let request = sidecar_request();
    let artifacts = build_promotion_sidecar_artifacts(request).expect("sidecars build");

    assert_sidecar_contract(&artifacts.sidecar, &artifacts.proof);
    assert_eq!(
        projection(&artifacts.sidecar, &artifacts.proof),
        expected_projection()
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let source_rows_path = temp.path().join("source_rows.csv");
    let source_rows = "tenant,surface_id\nSears,surf:sears\n";
    fs::write(&source_rows_path, source_rows).expect("source rows");

    let output_dir = temp.path().join("sidecars");
    let receipt = write_promotion_sidecar_artifacts(&output_dir, &artifacts)
        .expect("write sidecar artifacts");

    assert_eq!(
        receipt.sidecar_snapshot_hash,
        artifacts.sidecar.artifact_content_hash
    );
    assert_eq!(receipt.proof_hash, artifacts.proof.artifact_content_hash);
    assert!(receipt.bytes_written > 0);
    assert_eq!(
        fs::read_to_string(&source_rows_path).expect("source rows read"),
        source_rows
    );

    let persisted_sidecar: EntityPromotionSidecarArtifact =
        serde_json::from_slice(&fs::read(&receipt.sidecar_path).expect("sidecar file"))
            .expect("sidecar json");
    let persisted_proof: EntityPromotionProofArtifact =
        serde_json::from_slice(&fs::read(&receipt.proof_path).expect("proof file"))
            .expect("proof json");
    assert_eq!(persisted_sidecar, artifacts.sidecar);
    assert_eq!(persisted_proof, artifacts.proof);
}

#[test]
fn profile_scoped_negative_knowledge_refuses_cross_profile_sidecar() {
    let mut request = sidecar_request();
    let cannot_link = request
        .patch_bundle
        .cannot_link_sidecars
        .first_mut()
        .expect("cannot-link sidecar");
    cannot_link.profile_id = "regab_firm_identity".to_string();
    cannot_link.identity_semantics = "same_firm_or_reviewed_alias".to_string();

    let refusal =
        build_promotion_sidecar_artifacts(request).expect_err("cross-profile sidecar refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "promote");
    assert_eq!(refusal.detail["field"], "profile_scope");
    assert_eq!(refusal.detail["record_kind"], "cannot_link");
    assert_eq!(refusal.detail["expected_profile_id"], "cmbs_tenant_label");
    assert_eq!(refusal.detail["actual_profile_id"], "regab_firm_identity");
    assert_eq!(refusal.detail["writes_performed"], false);
}

#[test]
fn sidecar_snapshot_hash_changes_when_sidecar_content_changes() {
    let base = build_promotion_sidecar_artifacts(sidecar_request()).expect("base sidecars");
    let mut changed_request = sidecar_request();
    changed_request
        .patch_bundle
        .cannot_link_sidecars
        .first_mut()
        .expect("cannot-link sidecar")
        .reason = "operator_reconfirmed_distinct".to_string();

    let changed = build_promotion_sidecar_artifacts(changed_request).expect("changed sidecars");

    assert_ne!(
        base.sidecar.artifact_content_hash,
        changed.sidecar.artifact_content_hash
    );
    assert_ne!(
        base.proof.sidecar_snapshot_hash,
        changed.proof.sidecar_snapshot_hash
    );
    assert_eq!(
        changed.proof.sidecar_snapshot_hash,
        changed.sidecar.artifact_content_hash
    );
}

fn assert_sidecar_contract(
    sidecar: &EntityPromotionSidecarArtifact,
    proof: &EntityPromotionProofArtifact,
) {
    assert_eq!(sidecar.version, CANON_ENTITY_PROMOTION_SIDECAR_VERSION);
    assert_eq!(proof.version, CANON_ENTITY_PROMOTION_PROOF_VERSION);
    assert!(sidecar.artifact_content_hash.starts_with("blake3:"));
    assert!(proof.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        sidecar.metadata.artifact_content_hash,
        sidecar.artifact_content_hash
    );
    assert_eq!(
        proof.metadata.artifact_content_hash,
        proof.artifact_content_hash
    );
    assert_eq!(proof.sidecar_snapshot_hash, sidecar.artifact_content_hash);
    assert_eq!(proof.registry_snapshot_hash, "blake3:registry");
    assert_eq!(sidecar.source_audit_hash, "blake3:audit");
    assert_eq!(proof.source_audit_hash, "blake3:audit");
    assert_eq!(
        sidecar.source_decision_ledger_hash,
        "blake3:decision-ledger"
    );
    assert_eq!(
        sidecar.metadata.upstream_artifacts[0].content_hash,
        "blake3:audit"
    );
    assert_eq!(
        sidecar.metadata.upstream_artifacts[1].content_hash,
        "blake3:decision-ledger"
    );

    assert_eq!(sidecar.summary.counts["escrow_count"], 1);
    assert_eq!(sidecar.summary.counts["contradiction_count"], 1);
    assert_eq!(sidecar.summary.counts["cannot_link_count"], 1);
    assert_eq!(sidecar.summary.counts["relation_hint_count"], 1);
    assert_eq!(proof.summary.counts["sidecar_record_count"], 4);
    assert_eq!(proof.summary.counts["promoted_alias_count"], 1);

    let cannot_link = &sidecar.cannot_link_facts[0];
    assert_eq!(cannot_link.profile_id, "cmbs_tenant_label");
    assert_eq!(cannot_link.identity_semantics, "canonical_display_label");
    assert!(cannot_link.hard_cannot_link);

    let relation = &sidecar.relation_hints[0];
    assert_eq!(relation.namespace, "cmbs_tenant_label.relations");
    assert_eq!(relation.profile_id, "cmbs_tenant_label");
    assert_eq!(relation.identity_semantics, "canonical_display_label");
}

fn sidecar_request() -> EntityPromotionSidecarRequest {
    EntityPromotionSidecarRequest {
        metadata: metadata(),
        source_audit_hash: "blake3:audit".to_string(),
        source_decision_ledger_hash: "blake3:decision-ledger".to_string(),
        patch_bundle: ReviewPatchBundle {
            alias_patches: vec![],
            distinct_patches: vec![],
            relation_patches: vec![RelationReviewPatch {
                patch_id: "relation:midland".to_string(),
                profile_id: "cmbs_tenant_label".to_string(),
                identity_semantics: "canonical_display_label".to_string(),
                namespace: "cmbs_tenant_label.relations".to_string(),
                left: "surf:pnc_bank".to_string(),
                right: "surf:pnc_midland_loan_services".to_string(),
                relation: "master_special_servicer_platform".to_string(),
                review_decision_id: "dec:relation:001".to_string(),
                source_event_hash: "blake3:relation-event".to_string(),
            }],
            cannot_link_sidecars: vec![CannotLinkSidecarRecord {
                sidecar_id: "cannot_link:sears".to_string(),
                profile_id: "cmbs_tenant_label".to_string(),
                identity_semantics: "canonical_display_label".to_string(),
                left: "surf:sears".to_string(),
                right: "surf:sears_auto".to_string(),
                hard_cannot_link: true,
                reason: "review_distinct_confirmed".to_string(),
                review_decision_id: "dec:distinct:001".to_string(),
                source_event_hash: "blake3:cannot-link-event".to_string(),
            }],
            override_records: vec![],
        },
        escrow_entities: vec![EntityEscrowSidecarRecord {
            escrow_id: "escrow:overlap:001".to_string(),
            profile_id: "cmbs_tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            surface_ids: vec!["surf:anchor_a".to_string(), "surf:anchor_b".to_string()],
            reason: "multiple_incumbent_overlap".to_string(),
            source_decision_id: "dec:escrow:001".to_string(),
        }],
        contradiction_entities: vec![EntityContradictionSidecarRecord {
            contradiction_id: "contradiction:cannot-link:001".to_string(),
            profile_id: "cmbs_tenant_label".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            surface_ids: vec!["surf:sears".to_string(), "surf:sears_auto".to_string()],
            reason: "support_conflicts_with_cannot_link".to_string(),
            source_decision_id: "dec:contradiction:001".to_string(),
        }],
        promoted_alias_count: 1,
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
            row_count: 4,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: "blake3:pre-sidecar".to_string(),
    }
}

fn projection(
    sidecar: &EntityPromotionSidecarArtifact,
    proof: &EntityPromotionProofArtifact,
) -> Value {
    json!({
        "sidecar_version": sidecar.version,
        "proof_version": proof.version,
        "profile_id": sidecar.metadata.profile.id,
        "identity_semantics": sidecar.metadata.profile.identity_semantics,
        "sidecar_summary": sidecar.summary.counts,
        "proof_summary": proof.summary.counts,
        "cannot_link": {
            "profile_id": sidecar.cannot_link_facts[0].profile_id,
            "identity_semantics": sidecar.cannot_link_facts[0].identity_semantics,
            "left": sidecar.cannot_link_facts[0].left,
            "right": sidecar.cannot_link_facts[0].right,
            "hard_cannot_link": sidecar.cannot_link_facts[0].hard_cannot_link,
            "reason": sidecar.cannot_link_facts[0].reason,
        },
        "relation": {
            "namespace": sidecar.relation_hints[0].namespace,
            "left": sidecar.relation_hints[0].left,
            "right": sidecar.relation_hints[0].right,
            "relation": sidecar.relation_hints[0].relation,
        },
        "hash_linked": proof.sidecar_snapshot_hash == sidecar.artifact_content_hash,
    })
}

fn expected_projection() -> Value {
    serde_json::from_str(EXPECTED_PROJECTION).expect("projection fixture")
}
