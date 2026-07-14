#![forbid(unsafe_code)]

use canon::entity::{
    CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_EVIDENCE_VERSION_V1, EntityArtifactMetadata,
    EntityArtifactReference, EntityInputReference, EntityPatchNamespaces, EntityProfileReference,
    EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    ledger_replay::parse_decision_ledger_jsonl,
    review::{
        ReviewExportInclude, ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueRequest,
        ReviewRelationHint, build_review_queue_artifact, render_review_queue_csv,
    },
    review_import::{
        ReviewImportAction, ReviewImportContext, ReviewImportDecision, ReviewImportRequest,
        import_review_decisions,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveSurfaceProvenance,
        build_solve_artifact_contract,
    },
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MANIFEST_PATH: &str = "tests/fixtures/entity/cmbs/review_loop/manifest.json";

#[derive(Debug, Deserialize)]
struct ReviewLoopManifest {
    schema_version: String,
    expected_review_ids: Vec<String>,
    expected_actions: BTreeMap<String, ReviewImportAction>,
    expected_ledger_event_types: Vec<String>,
    forbidden_assertion_scopes: Vec<String>,
}

#[test]
fn cmbs_review_loop_exports_ambiguity_groups_once() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, "canon.entity.cmbs_review_loop.v0");

    let review = review_queue();
    let review_ids = review
        .review_items
        .iter()
        .map(|item| item.review_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(review_ids, manifest.expected_review_ids);
    assert_eq!(
        review_ids.iter().collect::<BTreeSet<_>>().len(),
        review_ids.len()
    );
    assert_eq!(review.summary.counts["review_items"], 2);
    assert_eq!(review.summary.counts["review_rows_covered"], 153);

    let csv = render_review_queue_csv(&review).expect("review csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().expect("review headers").clone();
    for forbidden in ["canonical_id", "registry_version", "apply_output"] {
        assert!(
            !headers.iter().any(|header| header.contains(forbidden)),
            "review export must not hide promotion/apply assertions in {forbidden}"
        );
    }
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("review csv records parse");
    assert_eq!(rows.len(), 2);

    for scope in &manifest.forbidden_assertion_scopes {
        assert!(
            !csv.to_ascii_lowercase().contains(scope),
            "review fixture must not assert {scope}"
        );
    }
}

#[test]
fn cmbs_review_loop_imports_expected_actions_into_ledger() {
    let manifest = manifest();
    let review = review_queue();
    let temp = tempfile::tempdir().expect("tempdir");
    let ledger_path = temp.path().join("decision-ledger.jsonl");
    let receipt = import_review_decisions(ReviewImportRequest {
        context: review_import_context(&review),
        decisions: review_import_decisions_from_manifest(&manifest, &review),
        ledger_path: ledger_path.clone(),
        timestamp: "2026-06-26T19:52:00Z".to_string(),
        previous_event_hash: "blake3:cmbs-review-start".to_string(),
    })
    .expect("review decisions import");

    assert_eq!(receipt.accepted_decisions, 2);
    assert_eq!(receipt.ledger_path, ledger_path);
    assert!(receipt.last_event_hash.starts_with("blake3:"));
    let ledger_jsonl = fs::read_to_string(&receipt.ledger_path).expect("ledger reads");
    let events = parse_decision_ledger_jsonl(&ledger_jsonl).expect("ledger parses");
    assert_eq!(events.len(), 2);
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type.as_str().to_string())
            .collect::<Vec<_>>(),
        manifest.expected_ledger_event_types
    );
    for event in events {
        assert_eq!(event.source_artifact_hash, review.artifact_content_hash);
        assert_eq!(event.metadata.profile.id, "cmbs_tenant_label");
        assert_eq!(
            event.metadata.upstream_artifacts,
            vec![EntityArtifactReference {
                version: "canon_entity_review_queue.v0".to_string(),
                content_hash: review.artifact_content_hash.clone()
            }]
        );
    }
}

#[test]
#[allow(non_snake_case)]
fn ER_REVIEW_GOLDEN_001_cmbs_review_loop_manifest_is_isolated() {
    let manifest = manifest();

    assert_eq!(manifest.schema_version, "canon.entity.cmbs_review_loop.v0");
    assert_eq!(
        manifest.expected_review_ids,
        ["review:surf_sears", "review:surf_pnc_bank"]
    );
    assert_eq!(
        manifest.expected_actions["review:surf_sears"],
        ReviewImportAction::DistinctConfirmed
    );
    assert_eq!(
        manifest.expected_actions["review:surf_pnc_bank"],
        ReviewImportAction::RelationConfirmed
    );
    assert_eq!(
        manifest.expected_ledger_event_types,
        ["distinct_confirmed", "relation_confirmed"]
    );
    assert_eq!(manifest.forbidden_assertion_scopes, ["promotion", "apply"]);
}

fn review_queue() -> ReviewQueueArtifact {
    build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact_with_review_groups(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("review queue builds")
}

fn review_import_context(review: &ReviewQueueArtifact) -> ReviewImportContext {
    ReviewImportContext {
        metadata: review.metadata.clone(),
        source_review_queue_hash: review.artifact_content_hash.clone(),
        known_review_ids: review
            .review_items
            .iter()
            .map(|item| item.review_id.clone())
            .collect(),
        cannot_link_review_ids: review
            .review_items
            .iter()
            .filter(|item| item.strongest_negative_cut.is_some())
            .map(|item| item.review_id.clone())
            .collect(),
    }
}

fn review_import_decisions_from_manifest(
    manifest: &ReviewLoopManifest,
    review: &ReviewQueueArtifact,
) -> Vec<ReviewImportDecision> {
    let items = review
        .review_items
        .iter()
        .map(|item| (item.review_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    manifest
        .expected_review_ids
        .iter()
        .map(|review_id| {
            let item = items
                .get(review_id.as_str())
                .unwrap_or_else(|| panic!("missing review item {review_id}"));
            ReviewImportDecision {
                review_id: review_id.clone(),
                action: manifest.expected_actions[review_id],
                operator_id: "operator:cmbs-review-fixture".to_string(),
                source_review_queue_hash: review.artifact_content_hash.clone(),
                profile_id: review.metadata.profile.id.clone(),
                profile_version: review.metadata.profile.version.clone(),
                entity_type: Some(review.metadata.profile.entity_type.clone()),
                identity_semantics: Some(review.metadata.profile.identity_semantics.clone()),
                strategy_hash: review.metadata.strategy.content_hash.clone(),
                registry_snapshot_hash: review
                    .metadata
                    .registry_snapshot
                    .lookup_snapshot_hash
                    .clone(),
                surface_ids: item.surface_ids.clone(),
                reason_code: format!("cmbs_{}", item.proposed_action),
                note: "CMBS review loop fixture decision; no promotion or apply assertion"
                    .to_string(),
                override_approved_by: None,
                override_reason_code: None,
            }
        })
        .collect()
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
            evidence_hit(
                ScoreLane::Support,
                "name",
                "string_similarity",
                "positive_identity_evidence",
                support_units,
                false,
            ),
            evidence_hit(
                ScoreLane::AntiMerge,
                "cmbs_tenant_label.distinct",
                "operator_patch",
                "distinct_identity_evidence",
                anti_merge_units,
                true,
            ),
        ],
    )
    .expect("support plus anti-merge edge builds")
}

fn evidence_hit(
    lane: ScoreLane,
    namespace: &str,
    operator_id: &str,
    reason_code: &str,
    units: u32,
    hard_veto: bool,
) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        lane,
        namespace,
        operator_id,
        reason_code,
        score(units),
        hard_veto,
        reason_code,
    )
}

fn provenance(surface_id: &str, row_count: u64, deal_count: u64) -> SolveSurfaceProvenance {
    SolveSurfaceProvenance {
        surface_id: surface_id.to_string(),
        row_count,
        deal_count,
    }
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

fn provenance_samples() -> Vec<ReviewProvenanceSample> {
    vec![
        sample("surf:sears", "row-001", "deal-a", "Sears"),
        sample("surf:sears_auto", "row-057", "deal-q", "Sears Auto Center"),
        sample("surf:pnc_bank", "row-104", "deal-pnc-a", "PNC Bank"),
        sample(
            "surf:pnc_midland_loan_services",
            "row-147",
            "deal-pnc-b",
            "PNC Midland Loan Services",
        ),
    ]
}

fn sample(surface_id: &str, row_id: &str, source: &str, raw_value: &str) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row_id.to_string(),
        source: source.to_string(),
        raw_value: raw_value.to_string(),
    }
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
            relation: "division_of".to_string(),
            reason_code: "bank_vs_servicing_division_review_only".to_string(),
        },
    ]
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("score units fit")
}

fn manifest() -> ReviewLoopManifest {
    serde_json::from_slice(&fs::read(repo_path(MANIFEST_PATH)).expect("manifest bytes"))
        .expect("manifest parses")
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
