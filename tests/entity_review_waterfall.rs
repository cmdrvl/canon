#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        EntityArtifactMetadata, EntityDeterministicSummary, EntityInputReference,
        EntityPatchNamespaces, EntityProfileReference, EntityRegistrySnapshot,
        EntityStrategyReference,
        review::render_review_queue_csv,
        review::{ReviewProvenanceSample, ReviewQueueArtifact, ReviewQueueItem},
        review_export::{
            NativeReviewArtifact, NativeReviewExportRequest, build_native_review_artifact,
            native_review_artifact_hash, render_native_review_csv, render_native_review_html,
        },
        review_import::native_review_import_context_from_artifact,
        score::{ScoreLane, ScoreUnits},
        solve::{SolveEvidenceCut, SolveEvidenceCutHit, SolveReconciliationState},
    },
};

#[test]
fn native_review_waterfall_saturates_orders_and_surfaces_frequency_metadata() {
    let artifact = native_artifact(false);
    let item = artifact
        .review_items
        .iter()
        .find(|item| item.review_id == "review:weighted")
        .expect("weighted item");
    let waterfall = &item.evidence_waterfall;

    assert_eq!(waterfall.score_total_units, 10_000);
    assert_eq!(waterfall.raw_support_score_units, 12_000);
    assert_eq!(
        waterfall
            .contributions
            .iter()
            .map(|contribution| contribution.score_units)
            .sum::<u32>(),
        waterfall.score_total_units
    );
    assert_eq!(waterfall.contributions.len(), 3);
    assert_eq!(waterfall.contributions[0].operator, "operator:exact_id");
    assert_eq!(waterfall.contributions[0].view_field, "view_field:tax_id");
    assert_eq!(waterfall.contributions[0].source_score_units, 7_000);
    assert_eq!(waterfall.contributions[0].score_units, 7_000);
    assert_eq!(waterfall.contributions[0].running_total_units, 7_000);
    assert_eq!(
        waterfall.contributions[1].operator,
        "operator:name_similarity"
    );
    assert_eq!(waterfall.contributions[1].source_score_units, 5_000);
    assert_eq!(waterfall.contributions[1].score_units, 3_000);
    assert_eq!(waterfall.contributions[1].running_total_units, 10_000);
    let frequency = waterfall.contributions[1]
        .value_frequency
        .as_ref()
        .expect("frequency metadata");
    assert_eq!(frequency.table_hash, "blake3:freq-table");
    assert_eq!(frequency.view_field, "name");
    assert_eq!(frequency.count, 42);
    assert_eq!(frequency.band, "common");
    assert_eq!(frequency.original_score_units, 7_143);
    assert_eq!(frequency.adjusted_score_units, 5_000);
    assert_eq!(waterfall.contributions[2].lane, "anti_merge");
    assert_eq!(waterfall.contributions[2].score_units, 0);
    assert_eq!(waterfall.contributions[2].running_total_units, 10_000);
    assert_eq!(
        waterfall
            .threshold_lines
            .iter()
            .map(|line| line.threshold_id.as_str())
            .collect::<Vec<_>>(),
        vec!["backbone_score_min", "attach_score_min", "abstain_margin"]
    );

    let json = serde_json::to_string(&artifact).expect("artifact json");
    assert!(json.contains("\"evidence_waterfall\""));
    assert!(json.contains("\"count\":42"));
    assert!(!json.contains("WellsFargo"));

    let csv = render_native_review_csv(&artifact).expect("csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().expect("headers").clone();
    assert!(
        headers
            .iter()
            .any(|header| header == "evidence_waterfall_json")
    );

    let html = render_native_review_html(&artifact).expect("html renders");
    assert!(html.contains("waterfall-table"));
    assert!(html.contains("count=${valueFrequency.count}"));
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
}

#[test]
fn review_queue_export_carries_waterfall_source_facts_for_native_projection() {
    let queue = review_queue(false);
    let json = serde_json::to_string(&queue).expect("queue json");
    assert!(json.contains("\"evidence_hits\""));
    assert!(json.contains("\"operator_id\":\"name_similarity\""));
    assert!(json.contains("value_frequency"));

    let csv = render_review_queue_csv(&queue).expect("queue csv");
    assert!(csv.contains("positive_evidence_json"));
    assert!(csv.contains("evidence_hits"));
    assert!(csv.contains("name_similarity"));
}

#[test]
fn native_review_waterfall_is_byte_stable_under_item_and_hit_shuffle() {
    let forward = native_artifact(false);
    let reversed = native_artifact(true);

    assert_eq!(
        serde_json::to_vec(&forward).expect("forward json"),
        serde_json::to_vec(&reversed).expect("reversed json")
    );
}

#[test]
fn native_review_waterfall_refuses_rehashed_sum_mismatch() {
    let mut artifact = native_artifact(false);
    artifact.review_items[1].evidence_waterfall.contributions[0].score_units = 2_999;
    artifact.review_items[1].evidence_waterfall.contributions[0].running_total_units = 2_999;
    reseal_native_artifact(&mut artifact);
    let artifact_value = serde_json::to_value(&artifact).expect("artifact value");

    let refusal = native_review_import_context_from_artifact(&artifact_value)
        .expect_err("tampered waterfall refuses");
    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(
        refusal.detail["field"],
        "review_items.evidence_waterfall.contributions"
    );
}

#[test]
fn native_review_waterfall_keeps_no_hit_item_empty_and_zero() {
    let artifact = native_artifact(false);
    let item = artifact
        .review_items
        .iter()
        .find(|item| item.review_id == "review:no_hit")
        .expect("no hit item");

    assert!(item.evidence_waterfall_refs.is_empty());
    assert!(item.evidence_waterfall.contributions.is_empty());
    assert_eq!(item.evidence_waterfall.score_total_units, 0);
    assert_eq!(item.evidence_waterfall.raw_support_score_units, 0);
}

fn native_artifact(reverse: bool) -> NativeReviewArtifact {
    build_native_review_artifact(NativeReviewExportRequest {
        review_queue: review_queue(reverse),
        run_content_hash: "blake3:run-waterfall".to_string(),
        policy_content_hash: "blake3:policy-waterfall".to_string(),
    })
    .expect("native review builds")
}

fn review_queue(reverse: bool) -> ReviewQueueArtifact {
    let mut review_items = vec![weighted_item(reverse), small_score_item(), no_hit_item()];
    if reverse {
        review_items.reverse();
    }
    ReviewQueueArtifact {
        version: "canon_entity_review_queue.v0".to_string(),
        artifact_content_hash: "blake3:review-queue-waterfall".to_string(),
        metadata: metadata(),
        summary: EntityDeterministicSummary::default(),
        source_solve_hash: "blake3:solve-waterfall".to_string(),
        source_link_hash: None,
        review_items,
    }
}

fn weighted_item(reverse_hits: bool) -> ReviewQueueItem {
    let mut support_hits = vec![
        cut_hit(
            ScoreLane::Support,
            "name",
            "name_similarity",
            "name.normalized",
            5_000,
            false,
            "string metric value_frequency version=canon_entity_value_frequency.v0 table_hash=blake3:freq-table view=name value=WellsFargo count=42 band=common floor_applied=false multiplier_basis_points=7000 original_score_units=7143 adjusted_score_units=5000",
        ),
        cut_hit(
            ScoreLane::Support,
            "tax_id",
            "exact_id",
            "tax_id.exact",
            7_000,
            false,
            "exact id matched score_units=7000",
        ),
    ];
    if reverse_hits {
        support_hits.reverse();
    }
    ReviewQueueItem {
        review_id: "review:weighted".to_string(),
        ambiguity_key: "weighted".to_string(),
        component_id: "component:weighted".to_string(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_merge_distinct_or_relation".to_string(),
        review_priority_units: 6_000,
        priority_reasons: vec!["support_and_cannot_link".to_string()],
        affected_rows: 3,
        affected_deals: 1,
        surface_ids: vec!["surf:weighted_a".to_string(), "surf:weighted_b".to_string()],
        strongest_positive_cut: Some(SolveEvidenceCut {
            left_surface_id: "surf:weighted_a".to_string(),
            right_surface_id: "surf:weighted_b".to_string(),
            score_units: ScoreUnits::saturating_from_units(12_000),
            evidence_count: 2,
            evidence_reason_codes: vec!["name.normalized".to_string(), "tax_id.exact".to_string()],
            evidence_hits: support_hits,
        }),
        strongest_negative_cut: Some(SolveEvidenceCut {
            left_surface_id: "surf:weighted_a".to_string(),
            right_surface_id: "surf:weighted_b".to_string(),
            score_units: ScoreUnits::saturating_from_units(9_000),
            evidence_count: 1,
            evidence_reason_codes: vec!["distinct.cannot_link".to_string()],
            evidence_hits: vec![cut_hit(
                ScoreLane::AntiMerge,
                "distinct",
                "cannot_link",
                "distinct.cannot_link",
                9_000,
                true,
                "hard cannot-link evidence score_units=9000",
            )],
        }),
        relation_hints: Vec::new(),
        provenance_samples: vec![
            provenance("surf:weighted_a", "row:1"),
            provenance("surf:weighted_b", "row:2"),
        ],
    }
}

fn small_score_item() -> ReviewQueueItem {
    ReviewQueueItem {
        review_id: "review:small".to_string(),
        ambiguity_key: "small".to_string(),
        component_id: "component:small".to_string(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "confirm_merge_distinct_or_relation".to_string(),
        review_priority_units: 2_000,
        priority_reasons: vec!["operator_review_required".to_string()],
        affected_rows: 1,
        affected_deals: 0,
        surface_ids: vec!["surf:small_a".to_string(), "surf:small_b".to_string()],
        strongest_positive_cut: Some(SolveEvidenceCut {
            left_surface_id: "surf:small_a".to_string(),
            right_surface_id: "surf:small_b".to_string(),
            score_units: ScoreUnits::saturating_from_units(3_000),
            evidence_count: 1,
            evidence_reason_codes: vec!["address.normalized".to_string()],
            evidence_hits: vec![cut_hit(
                ScoreLane::Support,
                "address",
                "address_match",
                "address.normalized",
                3_000,
                false,
                "address matched score_units=3000",
            )],
        }),
        strongest_negative_cut: None,
        relation_hints: Vec::new(),
        provenance_samples: vec![provenance("surf:small_a", "row:3")],
    }
}

fn no_hit_item() -> ReviewQueueItem {
    ReviewQueueItem {
        review_id: "review:no_hit".to_string(),
        ambiguity_key: "no_hit".to_string(),
        component_id: "component:no_hit".to_string(),
        state: SolveReconciliationState::Escrow,
        proposed_action: "review_directional_abstention".to_string(),
        review_priority_units: 2_000,
        priority_reasons: vec!["unmatched".to_string()],
        affected_rows: 1,
        affected_deals: 0,
        surface_ids: vec!["surf:no_hit".to_string()],
        strongest_positive_cut: None,
        strongest_negative_cut: None,
        relation_hints: Vec::new(),
        provenance_samples: vec![provenance("surf:no_hit", "row:4")],
    }
}

fn cut_hit(
    lane: ScoreLane,
    namespace: &str,
    operator_id: &str,
    reason_code: &str,
    score_units: u64,
    hard_cannot_link: bool,
    explanation: &str,
) -> SolveEvidenceCutHit {
    SolveEvidenceCutHit {
        lane,
        namespace: namespace.to_string(),
        operator_id: operator_id.to_string(),
        reason_code: reason_code.to_string(),
        score_units: ScoreUnits::saturating_from_units(score_units),
        hard_cannot_link,
        explanation: explanation.to_string(),
    }
}

fn provenance(surface_id: &str, row_id: &str) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row_id.to_string(),
        source: "waterfall-fixture.csv".to_string(),
        raw_value: "[redacted]".to_string(),
    }
}

fn reseal_native_artifact(artifact: &mut NativeReviewArtifact) {
    let hash = native_review_artifact_hash(artifact).expect("hash recomputes");
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
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
        upstream_artifacts: Vec::new(),
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}
