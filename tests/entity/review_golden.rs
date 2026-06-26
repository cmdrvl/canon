#![forbid(unsafe_code)]

use canon::entity::{
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    review::{
        ReviewExportInclude, ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem,
        ReviewQueueRequest, ReviewRelationHint, build_review_queue_artifact,
        render_review_queue_csv,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveReconciliationState,
        SolveSurfaceProvenance, build_solve_artifact_contract,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn entity_review_golden_exports_grouped_json_and_csv_for_real_ambiguities() {
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("review queue builds");
    let csv = render_review_queue_csv(&artifact).expect("review csv renders");

    let actual = ReviewQueueGolden::from_artifact_and_csv(&artifact, &csv);
    let expected = review_queue_golden();
    assert_eq!(actual, expected);

    assert_eq!(artifact.review_items.len(), 2);
    assert_eq!(artifact.review_items[0].review_id, "review:surf_sears");
    assert_eq!(artifact.review_items[1].review_id, "review:surf_pnc_bank");
    assert_eq!(artifact.summary.counts["review_rows_covered"], 153);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
}

fn solve_artifact_with_review_groups() -> SolveArtifact {
    solve_artifact(
        vec![
            support_and_anti_merge_record("surf:sears", "surf:sears_auto", 9_500, 9_000),
            support_and_anti_merge_record(
                "surf:pnc_bank",
                "surf:pnc_midland_loan_services",
                8_750,
                8_200,
            ),
        ],
        vec![
            provenance("surf:sears", 60, 25),
            provenance("surf:sears_auto", 40, 15),
            provenance("surf:pnc_bank", 32, 12),
            provenance("surf:pnc_midland_loan_services", 21, 8),
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

fn anti_merge_hit(namespace: &str, operator_id: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::AntiMerge,
        namespace,
        operator_id,
        "distinct_identity_evidence",
        score(units),
        true,
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
            row_count: 153,
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
        ReviewProvenanceSample {
            surface_id: "surf:pnc_bank".to_string(),
            row_id: "row-104".to_string(),
            source: "deal-pnc-a".to_string(),
            raw_value: "PNC Bank".to_string(),
        },
        ReviewProvenanceSample {
            surface_id: "surf:pnc_midland_loan_services".to_string(),
            row_id: "row-147".to_string(),
            source: "deal-pnc-b".to_string(),
            raw_value: "PNC Midland Loan Services".to_string(),
        },
    ]
}

fn relation_hints() -> Vec<ReviewRelationHint> {
    vec![
        ReviewRelationHint {
            left_surface_id: "surf:sears".to_string(),
            right_surface_id: "surf:sears_auto".to_string(),
            relation: "related_brand_family".to_string(),
            reason_code: "same_brand_family_review_only".to_string(),
        },
        ReviewRelationHint {
            left_surface_id: "surf:pnc_bank".to_string(),
            right_surface_id: "surf:pnc_midland_loan_services".to_string(),
            relation: "master_special_servicer_platform".to_string(),
            reason_code: "servicer_relation_review_only".to_string(),
        },
    ]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReviewQueueGolden {
    version: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    json_items: Vec<ReviewItemGolden>,
    csv_headers: Vec<String>,
    csv_rows: Vec<ReviewCsvRowGolden>,
}

impl ReviewQueueGolden {
    fn from_artifact_and_csv(artifact: &ReviewQueueArtifact, csv: &str) -> Self {
        Self {
            version: artifact.version.clone(),
            summary_counts: artifact.summary.counts.clone(),
            summary_labels: artifact.summary.labels.clone(),
            json_items: artifact
                .review_items
                .iter()
                .map(ReviewItemGolden::from_item)
                .collect(),
            csv_headers: csv_projection(csv).headers,
            csv_rows: csv_projection(csv).rows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReviewItemGolden {
    review_id: String,
    ambiguity_key: String,
    component_id: String,
    state: String,
    proposed_action: String,
    review_priority_units: u32,
    priority_reasons: Vec<String>,
    affected_rows: u64,
    affected_deals: u64,
    surface_ids: Vec<String>,
    positive_evidence_reason_codes: Vec<String>,
    negative_evidence_reason_codes: Vec<String>,
    relation_hints: Vec<ReviewHintGolden>,
    provenance_raw_values: Vec<String>,
}

impl ReviewItemGolden {
    fn from_item(item: &ReviewQueueItem) -> Self {
        Self {
            review_id: item.review_id.clone(),
            ambiguity_key: item.ambiguity_key.clone(),
            component_id: item.component_id.clone(),
            state: state_name(item.state),
            proposed_action: item.proposed_action.clone(),
            review_priority_units: item.review_priority_units,
            priority_reasons: item.priority_reasons.clone(),
            affected_rows: item.affected_rows,
            affected_deals: item.affected_deals,
            surface_ids: item.surface_ids.clone(),
            positive_evidence_reason_codes: item
                .strongest_positive_cut
                .as_ref()
                .map(|cut| cut.evidence_reason_codes.clone())
                .unwrap_or_default(),
            negative_evidence_reason_codes: item
                .strongest_negative_cut
                .as_ref()
                .map(|cut| cut.evidence_reason_codes.clone())
                .unwrap_or_default(),
            relation_hints: item
                .relation_hints
                .iter()
                .map(ReviewHintGolden::from_hint)
                .collect(),
            provenance_raw_values: item
                .provenance_samples
                .iter()
                .map(|sample| sample.raw_value.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReviewCsvProjection {
    headers: Vec<String>,
    rows: Vec<ReviewCsvRowGolden>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReviewCsvRowGolden {
    review_id: String,
    review_priority_units: u32,
    priority_reasons: Vec<String>,
    affected_rows: u64,
    affected_deals: u64,
    component_id: String,
    state: String,
    proposed_action: String,
    surface_ids: Vec<String>,
    relation_hints: Vec<ReviewHintGolden>,
    provenance_raw_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReviewHintGolden {
    left_surface_id: String,
    right_surface_id: String,
    relation: String,
    reason_code: String,
}

impl ReviewHintGolden {
    fn from_hint(hint: &ReviewRelationHint) -> Self {
        Self {
            left_surface_id: hint.left_surface_id.clone(),
            right_surface_id: hint.right_surface_id.clone(),
            relation: hint.relation.clone(),
            reason_code: hint.reason_code.clone(),
        }
    }
}

fn csv_projection(csv: &str) -> ReviewCsvProjection {
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader
        .headers()
        .expect("review csv headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let rows = reader
        .records()
        .map(|record| {
            let record = record.expect("review csv record");
            let get = |name: &str| {
                let index = headers
                    .iter()
                    .position(|header| header == name)
                    .unwrap_or_else(|| panic!("missing csv header {name}"));
                record.get(index).unwrap_or("")
            };
            let relation_hints =
                serde_json::from_str::<Vec<ReviewRelationHint>>(get("relation_hints_json"))
                    .expect("relation hints json parses")
                    .iter()
                    .map(ReviewHintGolden::from_hint)
                    .collect();
            let provenance_samples =
                serde_json::from_str::<Vec<ReviewProvenanceSample>>(get("provenance_samples_json"))
                    .expect("provenance samples json parses");
            ReviewCsvRowGolden {
                review_id: get("review_id").to_string(),
                review_priority_units: get("review_priority_units")
                    .parse()
                    .expect("priority parses"),
                priority_reasons: serde_json::from_str(get("priority_reasons_json"))
                    .expect("priority reasons parse"),
                affected_rows: get("affected_rows").parse().expect("rows parse"),
                affected_deals: get("affected_deals").parse().expect("deals parse"),
                component_id: get("component_id").to_string(),
                state: get("state").to_string(),
                proposed_action: get("proposed_action").to_string(),
                surface_ids: serde_json::from_str(get("surface_ids_json"))
                    .expect("surface ids parse"),
                relation_hints,
                provenance_raw_values: provenance_samples
                    .into_iter()
                    .map(|sample| sample.raw_value)
                    .collect(),
            }
        })
        .collect();
    ReviewCsvProjection { headers, rows }
}

fn state_name(state: SolveReconciliationState) -> String {
    format!("{state:?}").to_ascii_lowercase()
}

fn review_queue_golden() -> ReviewQueueGolden {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/review/golden/review_queue_projection.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}
