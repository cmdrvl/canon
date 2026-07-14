#![forbid(unsafe_code)]

use canon::RefusalCode;
use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1,
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifactRequest, SolveReconciliationConfig, SolveReconciliationState,
        SolveSurfaceProvenance, build_solve_artifact_contract, validate_solve_artifact_contract,
    },
};

#[test]
fn entity_solve_artifact_records_hashes_summary_and_upstream_contracts() {
    let request = solve_artifact_request(metadata_with_upstreams());
    let artifact = build_solve_artifact_contract(request.clone()).expect("solve artifact builds");
    validate_solve_artifact_contract(&artifact).expect("solve artifact validates");

    assert_eq!(artifact.version, CANON_ENTITY_SOLVE_VERSION_V1);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.upstream_artifacts.len(), 2);
    assert_eq!(
        artifact
            .upstream_artifacts
            .iter()
            .map(|reference| (reference.version.as_str(), reference.content_hash.as_str()))
            .collect::<Vec<_>>(),
        [
            (CANON_ENTITY_BLOCK_VERSION_V1, "blake3:block"),
            (CANON_ENTITY_EVIDENCE_VERSION_V1, "blake3:evidence"),
        ]
    );
    assert_eq!(artifact.summary.counts["resolved_existing"], 1);
    assert_eq!(artifact.summary.counts["promotable_new"], 1);
    assert_eq!(artifact.summary.counts["contradictions"], 1);
    assert_eq!(artifact.summary.counts["review_group_count"], 1);
    assert_eq!(artifact.decision_ledger_path, "solve/decisions.jsonl");
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::ResolvedExisting
        && entity.canonical_id.as_deref() == Some("TNT-SEARS")));
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::PromotableNew
        && entity.candidate_id.is_some()));
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::Contradiction
        && entity.hard_cannot_link_count == 1));

    let first = serde_json::to_vec(&artifact).expect("artifact serializes");
    let second =
        serde_json::to_vec(&build_solve_artifact_contract(request.clone()).expect("second build"))
            .expect("second artifact serializes");
    let third = serde_json::to_vec(&build_solve_artifact_contract(request).expect("third build"))
        .expect("third artifact serializes");
    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
fn entity_solve_artifact_refuses_missing_upstream_edge_hash() {
    let mut metadata = metadata_with_upstreams();
    metadata
        .upstream_artifacts
        .retain(|reference| reference.version != CANON_ENTITY_EVIDENCE_VERSION_V1);
    let refusal = build_solve_artifact_contract(solve_artifact_request(metadata))
        .expect_err("missing evidence upstream refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "solve");
    assert_eq!(
        refusal.detail["missing_version"],
        CANON_ENTITY_EVIDENCE_VERSION_V1
    );
}

#[test]
fn entity_solve_artifact_refuses_legacy_upstream_versions() {
    let mut metadata = metadata_with_upstreams();
    metadata.upstream_artifacts = vec![
        EntityArtifactReference {
            version: "canon_entity_block.v0".to_string(),
            content_hash: "blake3:block".to_string(),
        },
        EntityArtifactReference {
            version: "canon_entity_edge.v0".to_string(),
            content_hash: "blake3:edge".to_string(),
        },
    ];
    let refusal = build_solve_artifact_contract(solve_artifact_request(metadata))
        .expect_err("legacy upstream versions refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "solve");
    assert_eq!(
        refusal.detail["missing_version"],
        CANON_ENTITY_BLOCK_VERSION_V1
    );
}

#[test]
fn entity_solve_artifact_validator_refuses_self_hash_drift() {
    let mut artifact =
        build_solve_artifact_contract(solve_artifact_request(metadata_with_upstreams()))
            .expect("solve artifact builds");
    artifact
        .summary
        .counts
        .insert("resolved_existing".to_string(), 99);

    let refusal =
        validate_solve_artifact_contract(&artifact).expect_err("solve hash drift refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "solve");
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
}

fn solve_artifact_request(metadata: EntityArtifactMetadata) -> SolveArtifactRequest {
    SolveArtifactRequest {
        metadata,
        graph: graph_from_records(
            vec![
                support_record("surf:sears", "surf:sears_llc", 9_000),
                support_record("surf:alpha", "surf:alpha_llc", 8_000),
                build_edge_evidence_record(
                    "surf:kmart",
                    "surf:sears_auto",
                    vec![
                        support_hit("name", "string_similarity", 9_500),
                        anti_merge_hit("tenant_role", "protected_token", 9_000, true),
                    ],
                )
                .expect("hard conflict edge builds"),
            ],
            vec![SurfaceIncumbentId {
                surface_id: "surf:sears".to_string(),
                canonical_id: "TNT-SEARS".to_string(),
            }],
        ),
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance: vec![
            provenance("surf:sears", 10, 2),
            provenance("surf:sears_llc", 5, 1),
            provenance("surf:alpha", 4, 1),
            provenance("surf:alpha_llc", 3, 1),
            provenance("surf:kmart", 7, 2),
            provenance("surf:sears_auto", 6, 2),
        ],
        decision_ledger_path: "solve/decisions.jsonl".to_string(),
    }
}

fn metadata_with_upstreams() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant".to_string(),
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
            sidecar_snapshot_hash: None,
        },
        patch_namespace: "cmbs_tenant_label".to_string(),
        input: Some(EntityInputReference {
            row_count: 42,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: CANON_ENTITY_EVIDENCE_VERSION_V1.to_string(),
                content_hash: "blake3:evidence".to_string(),
            },
            EntityArtifactReference {
                version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
                content_hash: "blake3:block".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn graph_from_records(
    edge_records: Vec<EdgeEvidenceRecord>,
    incumbent_ids: Vec<SurfaceIncumbentId>,
) -> canon::entity::graph::EntityEvidenceGraph {
    build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids,
    })
    .expect("signed graph builds")
}

fn support_record(left_surface_id: &str, right_surface_id: &str, units: u32) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![support_hit("name", "string_similarity", units)],
    )
    .expect("support edge builds")
}

fn support_hit(namespace: &str, operator_id: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::Support,
        namespace,
        operator_id,
        "positive_identity_evidence",
        score(units),
        false,
        "positive identity evidence",
    )
}

fn anti_merge_hit(
    namespace: &str,
    operator_id: &str,
    units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        operator_id,
        "hard_cannot_link",
        score(units),
        hard_cannot_link,
        "distinct identity evidence",
    )
}

fn provenance(surface_id: &str, row_count: u64, deal_count: u64) -> SolveSurfaceProvenance {
    SolveSurfaceProvenance {
        surface_id: surface_id.to_string(),
        row_count,
        deal_count,
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
