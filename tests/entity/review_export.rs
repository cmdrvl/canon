#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_INDEX_VERSION_V1,
    CANON_ENTITY_PREPARE_VERSION_V1, EntityArtifactHeader, EntityArtifactMetadata,
    EntityArtifactReference, EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
    EntityRegistrySnapshot, EntityStrategyReference,
    block::{
        BlockCandidateBudgetConfig, BlockCandidateGenerationDiagnostics, BlockCandidateHit,
        BlockCandidateRecord, BlockOperatorCandidateDiagnostics, BlockOperatorYield,
    },
    block_artifact::{BlockCandidateArtifactRequest, build_block_candidate_artifact_contract},
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    edge_artifact::{EdgeEvidenceArtifactRequest, build_edge_evidence_artifact_contract},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    review::{
        ReviewExportInclude, ReviewProvenanceSample, ReviewQueueRequest, ReviewRelationHint,
        build_review_queue_artifact, render_review_queue_csv,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveReconciliationState,
        SolveSurfaceProvenance, build_solve_artifact_contract,
    },
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
#[allow(non_snake_case)]
fn EN_R001_repeated_ambiguity_exports_one_grouped_review_item() {
    let expected = review_fixture();
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("review queue builds");

    assert_eq!(artifact.version, expected.version);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert!(
        artifact
            .source_solve_hash
            .starts_with(&expected.source_solve_hash_prefix)
    );
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.summary.counts, expected.summary_counts);
    assert_eq!(artifact.summary.labels, expected.summary_labels);
    assert_eq!(artifact.review_items.len(), 1);

    let item = &artifact.review_items[0];
    assert_eq!(item.review_id, expected.first_item.review_id);
    assert_eq!(item.state, expected.first_item.state);
    assert_eq!(item.proposed_action, expected.first_item.proposed_action);
    assert_eq!(item.affected_rows, expected.first_item.affected_rows);
    assert_eq!(item.affected_deals, expected.first_item.affected_deals);
    assert_eq!(item.priority_reasons, expected.first_item.priority_reasons);
    assert_eq!(item.surface_ids, expected.first_item.surface_ids);
    assert_eq!(
        item.relation_hints.len(),
        expected.first_item.relation_hints
    );
    assert_eq!(
        item.provenance_samples.len(),
        expected.first_item.provenance_samples
    );
    assert!(item.review_priority_units > 0);
    assert!(item.strongest_positive_cut.is_some());
    assert!(item.strongest_negative_cut.is_some());

    let rebuilt = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("rebuilt review queue builds");
    assert_eq!(
        serde_json::to_vec(&artifact).expect("artifact serializes"),
        serde_json::to_vec(&rebuilt).expect("rebuilt artifact serializes")
    );
}

#[test]
fn entity_review_export_csv_contains_decision_context() {
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("review queue builds");
    let csv = render_review_queue_csv(&artifact).expect("csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().expect("headers").clone();
    assert!(headers.iter().any(|header| header == "review_id"));
    assert!(
        headers
            .iter()
            .any(|header| header == "priority_reasons_json")
    );
    assert!(headers.iter().any(|header| header == "relation_hints_json"));

    let records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv records");
    assert_eq!(records.len(), 1);
    let record = &records[0];
    let review_id_index = headers
        .iter()
        .position(|header| header == "review_id")
        .expect("review_id column");
    let reasons_index = headers
        .iter()
        .position(|header| header == "priority_reasons_json")
        .expect("reasons column");
    assert_eq!(&record[review_id_index], "review:surf_sears");
    assert!(record[reasons_index].contains("support_and_cannot_link"));
}

#[test]
fn active_review_priority_orders_high_impact_groups_first() {
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_two_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: vec![],
        relation_hints: vec![],
    })
    .expect("review queue builds");

    assert_eq!(artifact.review_items.len(), 2);
    assert_eq!(artifact.review_items[0].review_id, "review:surf_sears");
    assert!(
        artifact.review_items[0].review_priority_units
            > artifact.review_items[1].review_priority_units
    );
    assert!(
        artifact.review_items[0]
            .priority_reasons
            .iter()
            .any(|reason| reason == "high_row_count")
    );
}

fn solve_artifact_with_review_groups() -> SolveArtifact {
    solve_artifact(
        vec![support_and_anti_merge_record(
            "surf:sears",
            "surf:sears_auto",
            9_500,
            9_000,
            true,
        )],
        vec![
            provenance("surf:sears", 60, 25),
            provenance("surf:sears_auto", 40, 15),
        ],
    )
}

fn solve_artifact_with_two_review_groups() -> SolveArtifact {
    solve_artifact(
        vec![
            support_and_anti_merge_record("surf:sears", "surf:sears_auto", 9_500, 9_000, true),
            support_and_anti_merge_record("surf:alpha", "surf:alpha_llc", 7_000, 2_500, false),
        ],
        vec![
            provenance("surf:sears", 60, 25),
            provenance("surf:sears_auto", 40, 15),
            provenance("surf:alpha", 3, 1),
            provenance("surf:alpha_llc", 2, 1),
        ],
    )
}

fn solve_artifact(
    edge_records: Vec<EdgeEvidenceRecord>,
    provenance: Vec<SolveSurfaceProvenance>,
) -> SolveArtifact {
    let evidence = evidence_artifact_for_review_edges(&edge_records);
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed graph builds");
    let mut metadata = evidence.metadata.clone();
    metadata.strategy = solve_strategy();
    metadata.upstream_artifacts.push(EntityArtifactReference {
        version: evidence.version,
        content_hash: evidence.artifact_content_hash,
    });
    metadata.artifact_content_hash.clear();
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata,
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance,
        decision_ledger_path: "solve/decision_ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds")
}

fn evidence_artifact_for_review_edges(
    edge_records: &[EdgeEvidenceRecord],
) -> canon::entity::edge_artifact::EdgeEvidenceArtifact {
    let mut evidence_records = edge_records.to_vec();
    for record in &mut evidence_records {
        record.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    }
    evidence_records.sort_by(|left, right| {
        left.left_surface_id
            .cmp(&right.left_surface_id)
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });
    let candidate_records = candidate_records_for_edges(&evidence_records);
    let block = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: index_header(),
        strategy: block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_diagnostics_path: "block/diagnostics.json".to_string(),
        candidate_records: candidate_records.clone(),
        bucket_assertions: vec![],
        known_surface_ids: known_surface_ids(&candidate_records),
        diagnostics: diagnostics(candidate_records.len() as u64),
    })
    .expect("block artifact builds");
    build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
        block,
        strategy: evidence_strategy(),
        edge_records_path: "evidence/evidence.jsonl".to_string(),
        edge_records: evidence_records,
        candidate_records,
        bucket_assertions: vec![],
    })
    .expect("evidence artifact builds")
}

fn candidate_records_for_edges(edge_records: &[EdgeEvidenceRecord]) -> Vec<BlockCandidateRecord> {
    let mut candidates = edge_records
        .iter()
        .map(|record| BlockCandidateRecord {
            version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
            left_surface_id: record.left_surface_id.clone(),
            right_surface_id: record.right_surface_id.clone(),
            block_hits: vec![BlockCandidateHit {
                operator_id: "review_fixture:block_candidate".to_string(),
                rank: Some(1),
                score_units: 10_000,
            }],
            candidate_score_hint: 10_000,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.left_surface_id
            .cmp(&right.left_surface_id)
            .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
    });
    candidates
}

fn known_surface_ids(candidate_records: &[BlockCandidateRecord]) -> Vec<String> {
    let mut surface_ids = candidate_records
        .iter()
        .flat_map(|candidate| {
            [
                candidate.left_surface_id.clone(),
                candidate.right_surface_id.clone(),
            ]
        })
        .collect::<Vec<_>>();
    surface_ids.sort();
    surface_ids.dedup();
    surface_ids
}

fn diagnostics(candidate_count: u64) -> BlockCandidateGenerationDiagnostics {
    BlockCandidateGenerationDiagnostics {
        candidate_record_count: candidate_count,
        candidate_pairs_emitted: candidate_count,
        candidate_pairs_suppressed_by_cap: 0,
        suppressed_candidate_count: 0,
        large_buckets_suppressed: 0,
        candidate_pairs_per_surface_p50: candidate_count,
        candidate_pairs_per_surface_p95: candidate_count,
        candidate_pairs_per_surface_p99: candidate_count,
        max_candidates_for_surface: candidate_count,
        max_candidates_for_operator: candidate_count,
        configured_budget: BlockCandidateBudgetConfig::new(8, 64, 128),
        candidate_budget: canon::entity::edge::EdgeCandidateBudgetProof::within_run_budget(
            candidate_count,
            64,
        ),
        candidate_artifact_bytes: 512,
        partial_candidate_artifact_written: false,
        operator_yield: vec![BlockOperatorYield {
            operator_id: "review_fixture:block_candidate".to_string(),
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
        operator_diagnostics: vec![BlockOperatorCandidateDiagnostics {
            operator_id: "review_fixture:block_candidate".to_string(),
            input_candidate_count: candidate_count,
            eligible_candidate_count: candidate_count,
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
    }
}

fn support_and_anti_merge_record(
    left_surface_id: &str,
    right_surface_id: &str,
    support_units: u32,
    anti_merge_units: u32,
    hard_cannot_link: bool,
) -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![
            support_hit("name", "string_similarity", support_units),
            anti_merge_hit(
                "cmbs_tenant_label.distinct",
                "operator_patch",
                anti_merge_units,
                hard_cannot_link,
            ),
        ],
    )
    .expect("support plus anti-merge edge builds")
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
        "distinct_identity_evidence",
        score(units),
        hard_cannot_link,
        "distinct identity evidence",
    )
}

fn metadata_with_upstreams() -> EntityArtifactMetadata {
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
            row_count: 100,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn index_header() -> EntityArtifactHeader {
    let mut metadata = metadata_with_upstreams();
    metadata.strategy = EntityStrategyReference {
        id: "cmbs_tenant_label.index".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:index-strategy".to_string(),
    };
    metadata.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
        content_hash: "blake3:prepare".to_string(),
    }];
    metadata.artifact_content_hash = "blake3:index".to_string();
    EntityArtifactHeader {
        version: CANON_ENTITY_INDEX_VERSION_V1.to_string(),
        metadata,
        summary: Default::default(),
    }
}

fn block_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.block".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:block-strategy".to_string(),
    }
}

fn evidence_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.evidence".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:evidence-strategy".to_string(),
    }
}

fn solve_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.solve".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:solve-strategy".to_string(),
    }
}

fn provenance_samples() -> Vec<ReviewProvenanceSample> {
    vec![
        ReviewProvenanceSample {
            surface_id: "surf:sears".to_string(),
            row_id: "row-001".to_string(),
            source: "deal-a".to_string(),
            raw_value: "Sears".to_string(),
        },
        ReviewProvenanceSample {
            surface_id: "surf:sears_auto".to_string(),
            row_id: "row-057".to_string(),
            source: "deal-q".to_string(),
            raw_value: "Sears Auto Center".to_string(),
        },
    ]
}

fn relation_hints() -> Vec<ReviewRelationHint> {
    vec![ReviewRelationHint {
        left_surface_id: "surf:sears".to_string(),
        right_surface_id: "surf:sears_auto".to_string(),
        relation: "related_brand_family".to_string(),
        reason_code: "same_brand_family_review_only".to_string(),
    }]
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

fn review_fixture() -> ReviewFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/export/en_r001_expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[derive(Debug, Deserialize)]
struct ReviewFixture {
    version: String,
    source_solve_hash_prefix: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    first_item: ReviewItemFixture,
}

#[derive(Debug, Deserialize)]
struct ReviewItemFixture {
    review_id: String,
    state: SolveReconciliationState,
    proposed_action: String,
    affected_rows: u64,
    affected_deals: u64,
    priority_reasons: Vec<String>,
    surface_ids: Vec<String>,
    relation_hints: usize,
    provenance_samples: usize,
}
