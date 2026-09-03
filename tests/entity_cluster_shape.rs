#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1,
    EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
    EntityArtifactReferenceV1, EntityArtifactStageV1, EntityDeterministicSummary,
    EntityInputReference, EntityPatchNamespaces, EntityProfileReference, EntityRegistrySnapshot,
    EntityStrategyReference,
    artifact_chain::{EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage},
    audit::{
        EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite,
        run_entity_audit_with_cluster_shape,
    },
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{
        CANON_ENTITY_CLUSTER_SHAPE_VERSION, EntityClusterShapeClusterInput,
        EntityClusterShapeEdgeInput, EntityClusterShapeInput, EntityEvidenceGraph,
        build_entity_cluster_shape_report, build_signed_evidence_graph,
    },
    review::{
        ReviewExportInclude, ReviewV1ExportRequest, build_review_v1_artifact_with_cluster_shape,
    },
    schema::{
        entity_v1_contract_for_stage, entity_v1_schema_reference, entity_v1_workdir_layout,
        finalize_entity_v1_self_hash,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
        build_solve_artifact_contract,
    },
};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn cluster_shape_ranks_sparse_chain_above_same_size_dense_clique() {
    let report = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![
            cluster(
                "cluster:clique",
                ["clique:a", "clique:b", "clique:c", "clique:d"],
            ),
            cluster(
                "cluster:chain",
                ["chain:d", "chain:c", "chain:b", "chain:a"],
            ),
        ],
        scored_edges: chain_edges().into_iter().chain(clique_edges()).collect(),
    });

    assert_eq!(report.version, CANON_ENTITY_CLUSTER_SHAPE_VERSION);
    assert_eq!(report.summary.cluster_count, 2);
    assert_eq!(report.clusters[0].cluster_id, "cluster:chain");
    assert_eq!(report.clusters[0].size, 4);
    assert_eq!(report.clusters[0].possible_edge_count, 6);
    assert_eq!(report.clusters[0].scored_edge_count, 3);
    assert_eq!(report.clusters[0].edge_density_basis_points, 5_000);
    assert_eq!(report.clusters[0].bridge_edge_count, 3);
    assert_eq!(report.clusters[0].diameter, 3);
    assert_eq!(
        report.clusters[0].min_internal_edge_score_units,
        Some(score(8_000))
    );

    assert_eq!(report.clusters[1].cluster_id, "cluster:clique");
    assert_eq!(report.clusters[1].edge_density_basis_points, 10_000);
    assert_eq!(report.clusters[1].bridge_edge_count, 0);
    assert_eq!(report.clusters[1].diameter, 1);
}

#[test]
fn cluster_shape_is_stable_under_input_row_shuffle() {
    let forward = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![
            cluster(
                "cluster:chain",
                ["chain:a", "chain:b", "chain:c", "chain:d"],
            ),
            cluster(
                "cluster:clique",
                ["clique:a", "clique:b", "clique:c", "clique:d"],
            ),
        ],
        scored_edges: chain_edges().into_iter().chain(clique_edges()).collect(),
    });
    let shuffled = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![
            cluster(
                "cluster:clique",
                ["clique:d", "clique:b", "clique:c", "clique:a"],
            ),
            cluster(
                "cluster:chain",
                ["chain:d", "chain:b", "chain:c", "chain:a"],
            ),
        ],
        scored_edges: vec![
            edge("clique:d", "clique:a", 9_000),
            edge("chain:d", "chain:c", 8_000),
            edge("clique:c", "clique:a", 9_000),
            edge("chain:c", "chain:b", 8_500),
            edge("clique:b", "clique:a", 9_000),
            edge("clique:d", "clique:b", 9_000),
            edge("chain:b", "chain:a", 9_000),
            edge("clique:d", "clique:c", 9_000),
            edge("clique:c", "clique:b", 9_000),
        ],
    });

    assert_eq!(
        serde_json::to_vec(&forward).expect("forward report serializes"),
        serde_json::to_vec(&shuffled).expect("shuffled report serializes")
    );
}

#[test]
fn two_node_cluster_has_no_reported_bridge_and_diameter_one() {
    let report = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![cluster("cluster:pair", ["pair:a", "pair:b"])],
        scored_edges: vec![edge("pair:b", "pair:a", 7_500)],
    });
    let pair = &report.clusters[0];

    assert_eq!(pair.size, 2);
    assert_eq!(pair.possible_edge_count, 1);
    assert_eq!(pair.scored_edge_count, 1);
    assert_eq!(pair.edge_density_basis_points, 10_000);
    assert_eq!(pair.bridge_edge_count, 0);
    assert!(pair.bridge_edges.is_empty());
    assert_eq!(pair.diameter, 1);
}

#[test]
fn single_bridge_edge_is_reported_exactly() {
    let report = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![cluster(
            "cluster:barbell",
            ["bar:a", "bar:b", "bar:c", "bar:d", "bar:e", "bar:f"],
        )],
        scored_edges: vec![
            edge("bar:a", "bar:b", 9_100),
            edge("bar:a", "bar:c", 9_000),
            edge("bar:b", "bar:c", 8_900),
            edge("bar:c", "bar:d", 7_900),
            edge("bar:d", "bar:e", 9_200),
            edge("bar:d", "bar:f", 9_300),
            edge("bar:e", "bar:f", 9_400),
        ],
    });
    let barbell = &report.clusters[0];

    assert_eq!(barbell.edge_density_basis_points, 4_666);
    assert_eq!(barbell.bridge_edge_count, 1);
    assert_eq!(barbell.bridge_edges.len(), 1);
    assert_eq!(barbell.bridge_edges[0].left_surface_id, "bar:c");
    assert_eq!(barbell.bridge_edges[0].right_surface_id, "bar:d");
    assert_eq!(barbell.bridge_edges[0].score_units, score(7_900));
    assert_eq!(barbell.diameter, 3);
}

#[test]
fn equal_shape_ties_break_by_canonical_id_before_component_id() {
    let report = build_entity_cluster_shape_report(EntityClusterShapeInput {
        clusters: vec![
            cluster_with_canonical(
                "component:a",
                "ENT-002",
                ["tie:right-a", "tie:right-b", "tie:right-c"],
            ),
            cluster_with_canonical(
                "component:z",
                "ENT-001",
                ["tie:left-a", "tie:left-b", "tie:left-c"],
            ),
        ],
        scored_edges: vec![
            edge("tie:left-a", "tie:left-b", 8_000),
            edge("tie:left-b", "tie:left-c", 8_000),
            edge("tie:right-a", "tie:right-b", 8_000),
            edge("tie:right-b", "tie:right-c", 8_000),
        ],
    });

    assert_eq!(report.clusters[0].cluster_id, "component:z");
    assert_eq!(report.clusters[0].canonical_id.as_deref(), Some("ENT-001"));
    assert_eq!(report.clusters[1].cluster_id, "component:a");
    assert_eq!(report.clusters[1].canonical_id.as_deref(), Some("ENT-002"));
}

#[test]
fn audit_and_review_artifacts_embed_cluster_shape_without_mutating_solve_output() {
    let edge_records = chain_edges()
        .into_iter()
        .chain(clique_edges())
        .map(|edge| {
            support_record(
                &edge.left_surface_id,
                &edge.right_surface_id,
                edge.score_units,
            )
        })
        .collect::<Vec<_>>();
    let graph = graph_from_records(edge_records);
    let solve = solve_artifact(graph.clone());
    let solve_before = serde_json::to_vec(&solve).expect("solve artifact serializes");
    let shape = build_entity_cluster_shape_report(EntityClusterShapeInput::from_graph(
        clusters_from_solve(&solve),
        &graph,
    ));
    let solve_after = serde_json::to_vec(&solve).expect("solve artifact serializes after shape");

    assert_eq!(solve_before, solve_after);

    let review = build_review_v1_artifact_with_cluster_shape(
        ReviewV1ExportRequest {
            result_artifact: persisted_solve_artifact_value(&solve),
            include: ReviewExportInclude::All,
        },
        shape.clone(),
    )
    .expect("review artifact embeds shape");
    assert_eq!(
        review["cluster_shape"]["version"].as_str(),
        Some(CANON_ENTITY_CLUSTER_SHAPE_VERSION)
    );
    assert_eq!(
        review["summary"]["labels"]["order_by"].as_str(),
        Some("suspicion")
    );
    assert_eq!(
        review["review_items"][0]["component_id"].as_str(),
        shape
            .clusters
            .first()
            .map(|cluster| cluster.cluster_id.as_str())
    );

    let audit = run_entity_audit_with_cluster_shape(audit_request(), shape.clone())
        .expect("audit embeds shape");
    assert_eq!(
        audit
            .cluster_shape
            .as_ref()
            .map(|shape| shape.version.as_str()),
        Some(CANON_ENTITY_CLUSTER_SHAPE_VERSION)
    );
    assert_eq!(audit.cluster_shape, Some(shape));
}

fn chain_edges() -> Vec<EntityClusterShapeEdgeInput> {
    vec![
        edge("chain:a", "chain:b", 9_000),
        edge("chain:b", "chain:c", 8_500),
        edge("chain:c", "chain:d", 8_000),
    ]
}

fn clique_edges() -> Vec<EntityClusterShapeEdgeInput> {
    vec![
        edge("clique:a", "clique:b", 9_000),
        edge("clique:a", "clique:c", 9_000),
        edge("clique:a", "clique:d", 9_000),
        edge("clique:b", "clique:c", 9_000),
        edge("clique:b", "clique:d", 9_000),
        edge("clique:c", "clique:d", 9_000),
    ]
}

fn cluster<const N: usize>(
    cluster_id: &str,
    surface_ids: [&str; N],
) -> EntityClusterShapeClusterInput {
    EntityClusterShapeClusterInput {
        cluster_id: cluster_id.to_string(),
        canonical_id: None,
        surface_ids: surface_ids.into_iter().map(ToString::to_string).collect(),
    }
}

fn cluster_with_canonical<const N: usize>(
    cluster_id: &str,
    canonical_id: &str,
    surface_ids: [&str; N],
) -> EntityClusterShapeClusterInput {
    EntityClusterShapeClusterInput {
        cluster_id: cluster_id.to_string(),
        canonical_id: Some(canonical_id.to_string()),
        surface_ids: surface_ids.into_iter().map(ToString::to_string).collect(),
    }
}

fn edge(left_surface_id: &str, right_surface_id: &str, units: u32) -> EntityClusterShapeEdgeInput {
    EntityClusterShapeEdgeInput {
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        score_units: score(units),
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("score is within scale")
}

fn support_record(
    left_surface_id: &str,
    right_surface_id: &str,
    score_units: ScoreUnits,
) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![EdgeEvidenceHit::new(
            ScoreLane::Support,
            "shape_fixture",
            "string_similarity",
            "positive_identity_evidence",
            score_units,
            false,
            "positive identity evidence",
        )],
    )
    .expect("support edge record builds")
}

fn graph_from_records(edge_records: Vec<EdgeEvidenceRecord>) -> EntityEvidenceGraph {
    build_signed_evidence_graph(canon::entity::graph::SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed evidence graph builds")
}

fn solve_artifact(graph: EntityEvidenceGraph) -> SolveArtifact {
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: metadata_with_upstreams(),
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance: [
            "chain:a", "chain:b", "chain:c", "chain:d", "clique:a", "clique:b", "clique:c",
            "clique:d",
        ]
        .into_iter()
        .map(|surface_id| SolveSurfaceProvenance {
            surface_id: surface_id.to_string(),
            row_count: 1,
            deal_count: 1,
        })
        .collect(),
        decision_ledger_path: "solve/decisions.jsonl".to_string(),
    })
    .expect("solve artifact builds")
}

fn clusters_from_solve(artifact: &SolveArtifact) -> Vec<EntityClusterShapeClusterInput> {
    let canonical_by_component = artifact
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .canonical_id
                .as_ref()
                .map(|canonical_id| (entity.component_id.as_str(), canonical_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    artifact
        .diagnostics
        .components
        .iter()
        .map(|component| EntityClusterShapeClusterInput {
            cluster_id: component.component_id.clone(),
            canonical_id: canonical_by_component
                .get(component.component_id.as_str())
                .cloned(),
            surface_ids: component.surface_ids.clone(),
        })
        .collect()
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
            row_count: 8,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
                content_hash: "blake3:block".to_string(),
            },
            EntityArtifactReference {
                version: CANON_ENTITY_EVIDENCE_VERSION_V1.to_string(),
                content_hash: "blake3:evidence".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn persisted_solve_artifact_value(artifact: &SolveArtifact) -> Value {
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
            "target/entity-cluster-shape-test",
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
        Value::String(String::new()),
    );
    value["artifact_content_hash"] = Value::String(String::new());
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

fn audit_request() -> EntityAuditRequest {
    let mut metadata = metadata_with_upstreams();
    metadata.artifact_content_hash = "blake3:solve".to_string();
    let result = EntityArtifactHeader {
        version: CANON_ENTITY_SOLVE_VERSION_V1.to_string(),
        metadata,
        summary: EntityDeterministicSummary {
            counts: BTreeMap::from([("entity_count".to_string(), 2)]),
            labels: BTreeMap::new(),
        },
    };
    EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: vec![EntityArtifactReference {
            version: result.version.clone(),
            content_hash: result.metadata.artifact_content_hash.clone(),
        }],
        result,
        suite: EntityAuditSuite {
            id: "shape_suite".to_string(),
            version: "v1".to_string(),
            gates: vec![EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "valid".to_string(),
                actual: "valid".to_string(),
                evidence: BTreeMap::new(),
            }],
        },
    }
}
