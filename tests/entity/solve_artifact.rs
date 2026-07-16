#![forbid(unsafe_code)]

use canon::RefusalCode;
use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1,
    EntityArtifactMetadata, EntityArtifactReference, EntityArtifactReferenceV1,
    EntityArtifactStageV1, EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
    EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
    review::{ReviewExportInclude, ReviewV1ExportRequest, build_review_v1_artifact},
    schema::{
        entity_v1_contract_for_stage, entity_v1_schema_reference, entity_v1_workdir_layout,
        finalize_entity_v1_self_hash,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveAliasProposalSurface, SolveAliasProposalSurfaceStatus, SolveArtifact,
        SolveArtifactRequest, SolveReconciliationConfig, SolveReconciliationState,
        SolveSurfaceProvenance, build_solve_artifact_contract,
        build_solve_artifact_contract_with_alias_proposals, validate_solve_artifact_contract,
    },
};

#[test]
fn entity_solve_artifact_records_hashes_summary_and_upstream_contracts() {
    let request = solve_artifact_request(metadata_with_upstreams());
    let artifact =
        build_solve_artifact_contract_with_alias_proposals(request.clone(), alias_surfaces())
            .expect("solve artifact builds");
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
    assert_eq!(artifact.summary.counts["promotable_alias_count"], 2);
    assert_eq!(artifact.decision_ledger_path, "solve/decisions.jsonl");
    let resolved = artifact
        .entities
        .iter()
        .find(|entity| {
            entity.state == SolveReconciliationState::ResolvedExisting
                && entity.canonical_id.as_deref() == Some("TNT-SEARS")
        })
        .expect("resolved existing entity is present");
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::ResolvedExisting
        && entity.canonical_id.as_deref() == Some("TNT-SEARS")));
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::PromotableNew
        && entity.candidate_id.is_some()));
    assert!(artifact.entities.iter().any(|entity| entity.state
        == SolveReconciliationState::Contradiction
        && entity.hard_cannot_link_count == 1));
    assert_eq!(
        artifact
            .promotable_aliases
            .iter()
            .map(|alias| alias.input.as_str())
            .collect::<Vec<_>>(),
        ["Sears LLC", "Sears, LLC"]
    );
    assert!(artifact.promotable_aliases.iter().all(|alias| {
        alias.version == "canon_entity_alias_proposal.v0"
            && alias.proposal_id.starts_with("alias_proposal:blake3:")
            && alias.content_hash.starts_with("blake3:")
            && alias.proposal_id.ends_with(&alias.content_hash)
            && alias.allowed_actions == vec!["accept_alias".to_string(), "reject_alias".to_string()]
            && alias.canonical_id == "TNT-SEARS"
            && alias.canonical_type == "tenant_label"
            && alias.rule_id == "entity_solve_alias_proposal"
            && alias.component_id == resolved.component_id
            && alias.source_surface_ids == vec!["surf:sears_llc".to_string()]
    }));
    assert!(
        !artifact
            .promotable_aliases
            .iter()
            .any(|alias| alias.input == "Alpha LLC" || alias.input == "Sears Auto")
    );

    let first = serde_json::to_vec(&artifact).expect("artifact serializes");
    let second = serde_json::to_vec(
        &build_solve_artifact_contract_with_alias_proposals(request.clone(), alias_surfaces())
            .expect("second build"),
    )
    .expect("second artifact serializes");
    let third = serde_json::to_vec(
        &build_solve_artifact_contract_with_alias_proposals(request, reordered_alias_surfaces())
            .expect("third build"),
    )
    .expect("third artifact serializes");
    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
fn entity_solve_artifact_alias_proposal_tamper_refuses() {
    assert_alias_proposal_tamper_refuses("canonical_id", |proposal| {
        proposal.canonical_id = "TNT-OTHER".to_string();
    });
    assert_alias_proposal_tamper_refuses("version", |proposal| {
        proposal.version = "canon_entity_alias_proposal.v999".to_string();
    });
    assert_alias_proposal_tamper_refuses("content_hash", |proposal| {
        proposal.content_hash = "blake3:wrong".to_string();
    });
    assert_alias_proposal_tamper_refuses("proposal_id", |proposal| {
        proposal.proposal_id = "alias_proposal:blake3:wrong".to_string();
    });
    assert_alias_proposal_tamper_refuses("allowed_actions", |proposal| {
        proposal.allowed_actions = vec!["accept_alias".to_string()];
    });
    assert_alias_proposal_tamper_refuses("rule_id", |proposal| {
        proposal.rule_id = "reviewer_supplied_alias".to_string();
    });
    assert_alias_proposal_tamper_refuses("source_surface_ids", |proposal| {
        proposal
            .source_surface_ids
            .push("surf:sears_llc".to_string());
    });
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

#[test]
fn entity_solve_alias_proposals_export_as_typed_review_items() {
    let artifact = build_solve_artifact_contract_with_alias_proposals(
        solve_artifact_request(metadata_with_upstreams()),
        alias_surfaces(),
    )
    .expect("solve artifact builds");
    let persisted_artifact = persisted_solve_artifact_value(&artifact);
    let review = build_review_v1_artifact(ReviewV1ExportRequest {
        result_artifact: persisted_artifact,
        include: ReviewExportInclude::Resolved,
    })
    .expect("review artifact builds");

    let items = review["review_items"]
        .as_array()
        .expect("review items array");
    let alias_items = items
        .iter()
        .filter(|item| item["reason_code"] == "solve_alias_proposal")
        .collect::<Vec<_>>();
    assert_eq!(alias_items.len(), artifact.promotable_aliases.len());
    for proposal in &artifact.promotable_aliases {
        let item = alias_items
            .iter()
            .find(|item| item["review_id"].as_str() == Some(proposal.proposal_id.as_str()))
            .expect("review item for proposal");
        assert_eq!(
            item["review_id"].as_str(),
            Some(proposal.proposal_id.as_str())
        );
        assert_eq!(item["decision"].as_str(), Some(""));
        assert_eq!(
            item["proposed_action"].as_str(),
            Some("accept_or_reject_alias_proposal")
        );
        assert_eq!(
            item["alias_proposal"],
            serde_json::to_value(proposal).expect("proposal JSON")
        );
        for forbidden in [
            "proposal_id",
            "content_hash",
            "proposal_content_hash",
            "allowed_actions",
            "alias_input",
            "alias_inputs",
            "input",
            "target_canonical_id",
            "canonical_id",
            "canonical_type",
            "rule_id",
        ] {
            assert!(
                item.get(forbidden).is_none(),
                "review item must not duplicate alias authority field {forbidden}"
            );
        }
    }
}

#[test]
fn entity_review_export_refuses_tampered_nested_alias_proposal() {
    let mut artifact = build_solve_artifact_contract_with_alias_proposals(
        solve_artifact_request(metadata_with_upstreams()),
        alias_surfaces(),
    )
    .expect("solve artifact builds");
    artifact.promotable_aliases[0].content_hash = "blake3:wrong".to_string();

    let refusal = build_review_v1_artifact(ReviewV1ExportRequest {
        result_artifact: persisted_solve_artifact_value(&artifact),
        include: ReviewExportInclude::Resolved,
    })
    .expect_err("tampered proposal refuses before review export");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
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
            id: "tenant_label".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "tenant".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            canonical_type: "tenant_label".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "tenant_label.aliases".to_string(),
                distinct: "tenant_label.distinct".to_string(),
                relations: "tenant_label.relations".to_string(),
            },
            content_hash: Some("blake3:profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "tenant_label.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "tenant-labels".to_string(),
            version: "2026.06.25".to_string(),
            source: "registries/tenant-labels".to_string(),
            lookup_snapshot_hash: "blake3:registry".to_string(),
            sidecar_snapshot_hash: None,
        },
        patch_namespace: "tenant_label.aliases".to_string(),
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

fn persisted_solve_artifact_value(artifact: &SolveArtifact) -> serde_json::Value {
    let contract =
        entity_v1_contract_for_stage(EntityArtifactStageV1::Solve).expect("solve v1 contract");
    let mut value = serde_json::to_value(artifact).expect("solve artifact JSON");
    let metadata = value["metadata"]
        .as_object_mut()
        .expect("solve metadata object");
    metadata.insert(
        "schema".to_string(),
        serde_json::to_value(entity_v1_schema_reference(contract).expect("solve schema ref"))
            .expect("schema ref JSON"),
    );
    metadata.insert(
        "workdir".to_string(),
        serde_json::to_value(entity_v1_workdir_layout(
            contract,
            "target/entity-solve-artifact-test",
        ))
        .expect("workdir layout JSON"),
    );
    let upstream_refs = v1_upstream_references();
    metadata.insert(
        "upstream_artifacts".to_string(),
        serde_json::to_value(&upstream_refs).expect("upstream refs JSON"),
    );
    metadata.insert(
        "artifact_content_hash".to_string(),
        serde_json::Value::String(String::new()),
    );
    value["artifact_content_hash"] = serde_json::Value::String(String::new());
    value["upstream_artifacts"] = serde_json::to_value(upstream_refs).expect("upstream refs JSON");
    finalize_entity_v1_self_hash(&mut value).expect("solve v1 self hash");
    value
}

fn v1_upstream_references() -> Vec<EntityArtifactReferenceV1> {
    vec![
        v1_upstream_reference(EntityArtifactStageV1::Block, "blake3:block"),
        v1_upstream_reference(EntityArtifactStageV1::Evidence, "blake3:evidence"),
    ]
}

fn v1_upstream_reference(
    stage: EntityArtifactStageV1,
    content_hash: &str,
) -> EntityArtifactReferenceV1 {
    let contract = entity_v1_contract_for_stage(stage).expect("v1 upstream contract");
    let schema = entity_v1_schema_reference(contract).expect("v1 upstream schema ref");
    EntityArtifactReferenceV1 {
        version: contract.artifact_version.to_string(),
        schema_key: schema.key,
        schema_hash: schema.content_hash,
        content_hash: content_hash.to_string(),
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

fn alias_surfaces() -> Vec<SolveAliasProposalSurface> {
    vec![
        alias_surface(
            "surf:sears",
            SolveAliasProposalSurfaceStatus::Resolved,
            ["Sears Holdings"],
        ),
        alias_surface(
            "surf:sears_llc",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Sears LLC", "Sears, LLC", "Sears LLC"],
        ),
        alias_surface(
            "surf:alpha_llc",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Alpha LLC"],
        ),
        alias_surface(
            "surf:sears_auto",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Sears Auto"],
        ),
    ]
}

fn reordered_alias_surfaces() -> Vec<SolveAliasProposalSurface> {
    vec![
        alias_surface(
            "surf:sears_auto",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Sears Auto"],
        ),
        alias_surface(
            "surf:sears_llc",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Sears, LLC", "Sears LLC"],
        ),
        alias_surface(
            "surf:alpha_llc",
            SolveAliasProposalSurfaceStatus::Unresolved,
            ["Alpha LLC"],
        ),
        alias_surface(
            "surf:sears",
            SolveAliasProposalSurfaceStatus::Resolved,
            ["Sears Holdings"],
        ),
    ]
}

fn alias_surface(
    surface_id: &str,
    exact_lookup_status: SolveAliasProposalSurfaceStatus,
    raw_variants: impl IntoIterator<Item = &'static str>,
) -> SolveAliasProposalSurface {
    SolveAliasProposalSurface {
        surface_id: surface_id.to_string(),
        exact_lookup_status,
        raw_variants: raw_variants.into_iter().map(str::to_string).collect(),
    }
}

fn assert_alias_proposal_tamper_refuses(
    _field: &str,
    mutate: impl FnOnce(&mut canon::entity::solve::SolveAliasProposal),
) {
    let mut artifact = build_solve_artifact_contract_with_alias_proposals(
        solve_artifact_request(metadata_with_upstreams()),
        alias_surfaces(),
    )
    .expect("solve artifact builds");
    mutate(&mut artifact.promotable_aliases[0]);

    let refusal =
        validate_solve_artifact_contract(&artifact).expect_err("alias proposal tamper refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "solve");
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
