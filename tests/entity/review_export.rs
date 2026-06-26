#![forbid(unsafe_code)]

use canon::entity::{
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
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
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("signed graph builds");
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: metadata_with_upstreams(),
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance,
        decision_ledger_path: "review/decision-ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds")
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
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: "canon_entity_edge.v0".to_string(),
                content_hash: "blake3:edge".to_string(),
            },
            EntityArtifactReference {
                version: "canon_entity_block.v0".to_string(),
                content_hash: "blake3:block".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
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
