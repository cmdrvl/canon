#![forbid(unsafe_code)]

use canon::entity::{
    EntityArtifactMetadata, EntityArtifactReference, EntityInputReference, EntityPatchNamespaces,
    EntityProfileReference, EntityRegistrySnapshot, EntityStrategyReference,
    edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
    graph::{SignedEvidenceGraphInput, build_signed_evidence_graph},
    profiles::regab::{RegabFirmGuardKind, RegabFirmGuardRequest, regab_firm_guard_hit},
    relation::{RelationHintRequest, relation_hint_hit},
    review::{
        ReviewExportInclude, ReviewProvenanceSample, ReviewQueueItem, ReviewQueueRequest,
        ReviewRelationHint, build_review_queue_artifact, render_review_queue_csv,
    },
    score::{ScoreLane, ScoreUnits},
    solve::{
        SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig, SolveReconciliationState,
        SolveSurfaceProvenance, build_solve_artifact_contract,
    },
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[test]
fn sec10d_review_queue_groups_repeated_regab_firm_ambiguity() {
    let expected = expected_fixture();
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("sec10d review queue builds");

    assert_eq!(artifact.version, expected.version);
    assert!(artifact.source_solve_hash.starts_with("blake3:"));
    assert_eq!(artifact.summary.counts, expected.summary_counts);
    assert_eq!(artifact.summary.labels, expected.summary_labels);
    assert_eq!(artifact.review_items.len(), expected.groups.len());
    assert_expected_groups(&artifact.review_items, &expected.groups);

    let pnc = item_for_group(
        &artifact.review_items,
        expected_group(&expected, "REGAB-I002-PNC-MIDLAND-DIVISION"),
    );
    assert_eq!(pnc.affected_rows, 84);
    assert!(
        pnc.priority_reasons
            .iter()
            .any(|reason| reason == "high_row_count")
    );
    assert_eq!(
        pnc.provenance_samples
            .iter()
            .map(|sample| sample.row_id.as_str())
            .collect::<Vec<_>>(),
        ["regab-fixture-002", "regab-fixture-001"]
    );
}

#[test]
#[allow(non_snake_case)]
fn REGAB_I002_I003_sec10d_review_queue_keeps_hard_negatives_reviewable() {
    let artifact = build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: solve_artifact(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: relation_hints(),
    })
    .expect("sec10d review queue builds");

    let expected = expected_fixture();
    let platform = item_for_group(
        &artifact.review_items,
        expected_group(&expected, "REGAB-I003-PLATFORM-LABEL"),
    );
    assert_eq!(platform.state, SolveReconciliationState::Contradiction);
    assert!(
        platform
            .priority_reasons
            .iter()
            .any(|reason| reason == "regab_platform_label_guard")
    );
    assert!(
        platform
            .provenance_samples
            .iter()
            .any(|sample| sample.raw_value == "Wells Fargo Commercial Mortgage Securities Platform")
    );

    let auditor = item_for_group(
        &artifact.review_items,
        expected_group(&expected, "REGAB-I003-AUDITOR-SUBJECT-CONFLICT"),
    );
    assert!(
        auditor
            .priority_reasons
            .iter()
            .any(|reason| reason == "regab_auditor_subject_role_conflict")
    );

    let unresolved = item_for_group(
        &artifact.review_items,
        expected_group(&expected, "REGAB-UNRESOLVED-EXACT-LOOKUP-MISS"),
    );
    assert_eq!(unresolved.state, SolveReconciliationState::Escrow);
    assert!(
        unresolved
            .priority_reasons
            .iter()
            .any(|reason| reason == "below_support_threshold")
    );
    assert!(
        unresolved
            .provenance_samples
            .iter()
            .any(|sample| sample.raw_value == "Acme Review Analytics LLC")
    );

    assert!(
        artifact.review_items.iter().all(|item| {
            item.state != SolveReconciliationState::PromotableNew
                && item.state != SolveReconciliationState::ResolvedExisting
        }),
        "Reg AB review queue must not auto-promote platform/category or role-conflict surfaces"
    );

    let csv = render_review_queue_csv(&artifact).expect("review queue csv renders");
    let mut reader = csv::Reader::from_reader(csv.as_bytes());
    let headers = reader.headers().expect("csv headers").clone();
    let reasons_index = headers
        .iter()
        .position(|header| header == "priority_reasons_json")
        .expect("priority reasons column");
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("review csv records");
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .any(|row| row[reasons_index].contains("regab_auditor_subject_role_conflict"))
    );
}

fn assert_expected_groups(items: &[ReviewQueueItem], expected_groups: &[ExpectedReviewGroup]) {
    for expected in expected_groups {
        let item = item_for_group(items, expected);
        assert_eq!(item.state, expected.state, "{}", expected.id);
        assert_eq!(
            item.proposed_action, expected.proposed_action,
            "{}",
            expected.id
        );
        assert_eq!(
            item.affected_rows, expected.affected_rows,
            "{}",
            expected.id
        );
        assert_eq!(
            item.affected_deals, expected.affected_deals,
            "{}",
            expected.id
        );
        for reason in &expected.required_priority_reasons {
            assert!(
                item.priority_reasons.iter().any(|actual| actual == reason),
                "{} missing priority reason {reason}: {:?}",
                expected.id,
                item.priority_reasons
            );
        }

        let negative_codes = item
            .strongest_negative_cut
            .as_ref()
            .map(|cut| cut.evidence_reason_codes.clone())
            .unwrap_or_default();
        assert_eq!(
            negative_codes, expected.negative_reason_codes,
            "{}",
            expected.id
        );

        let relation_codes = item
            .relation_hints
            .iter()
            .map(|hint| hint.reason_code.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            relation_codes, expected.relation_reason_codes,
            "{}",
            expected.id
        );
        assert!(
            item.provenance_samples.len() >= expected.min_provenance_samples,
            "{} provenance sample count",
            expected.id
        );
    }
}

fn item_for_group<'a>(
    items: &'a [ReviewQueueItem],
    expected: &ExpectedReviewGroup,
) -> &'a ReviewQueueItem {
    items
        .iter()
        .find(|item| item.surface_ids == expected.surface_ids)
        .unwrap_or_else(|| panic!("missing review group {}", expected.id))
}

fn expected_group<'a>(expected: &'a ExpectedReviewQueue, id: &str) -> &'a ExpectedReviewGroup {
    expected
        .groups
        .iter()
        .find(|group| group.id == id)
        .unwrap_or_else(|| panic!("missing expected group {id}"))
}

fn solve_artifact() -> SolveArtifact {
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records: edge_records(),
        exact_bucket_assertions: Vec::new(),
        incumbent_ids: Vec::new(),
    })
    .expect("Reg AB review graph builds");

    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: metadata(),
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance: surface_provenance(),
        decision_ledger_path: "review/sec10d-regab-decision-ledger.jsonl".to_string(),
    })
    .expect("Reg AB solve artifact builds")
}

fn edge_records() -> Vec<EdgeEvidenceRecord> {
    vec![
        guarded_edge_record(GuardedEdge {
            left_surface_id: "surf:regab:midland_loan_services_division_pnc_bank_na",
            right_surface_id: "surf:regab:pnc_bank_na",
            left_name: "Midland Loan Services, a division of PNC Bank, National Association",
            right_name: "PNC Bank, National Association",
            left_role: "master_servicer",
            right_role: "servicer",
            support_reason: "regab_same_family_high_recall_candidate",
            support_units: 7_000,
            guard: RegabFirmGuardKind::BankLoanServicesDivision,
            relation: "division_of",
        }),
        guarded_edge_record(GuardedEdge {
            left_surface_id: "surf:regab:wells_fargo_bank_na",
            right_surface_id: "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            left_name: "Wells Fargo Bank, National Association",
            right_name: "Wells Fargo Commercial Mortgage Securities Platform",
            left_role: "regulated_firm",
            right_role: "platform",
            support_reason: "regab_shared_platform_family_candidate",
            support_units: 6_500,
            guard: RegabFirmGuardKind::PlatformCategoryLabel,
            relation: "platform_to_firm_context",
        }),
        guarded_edge_record(GuardedEdge {
            left_surface_id: "surf:regab:kpmg_llp",
            right_surface_id: "surf:regab:kpmg_securitization_trust_2024_c1",
            left_name: "KPMG LLP",
            right_name: "KPMG Securitization Trust 2024-C1",
            left_role: "auditor",
            right_role: "subject_party",
            support_reason: "regab_shared_auditor_token_candidate",
            support_units: 6_200,
            guard: RegabFirmGuardKind::AuditorSubjectPartyRoleConflict,
            relation: "role_context_conflict",
        }),
        soft_parent_subsidiary_record(),
        unresolved_exact_lookup_record(),
    ]
}

fn guarded_edge_record(edge: GuardedEdge<'_>) -> EdgeEvidenceRecord {
    let support = EdgeEvidenceHit::new(
        ScoreLane::Support,
        "regab_firm_identity.fixture_support",
        "sec10d_review_queue_candidate",
        edge.support_reason,
        score(edge.support_units),
        false,
        format!(
            "sec10d review fixture candidate reason={} left_role={} right_role={}",
            edge.support_reason, edge.left_role, edge.right_role
        ),
    );
    let anti_merge = regab_firm_guard_hit(RegabFirmGuardRequest {
        namespace: "regab_firm_identity.guards",
        guard: edge.guard,
        left_name: edge.left_name,
        right_name: edge.right_name,
        left_role: Some(edge.left_role),
        right_role: Some(edge.right_role),
        score_units: score(10_000),
    })
    .expect("Reg AB guard emits anti-merge evidence");
    let relation = relation_hint_hit(RelationHintRequest {
        namespace: "regab_firm_identity.relations",
        operator_id: "relation_hint:sec10d_review_queue",
        reason_code: "regab_relation_context",
        relation: edge.relation,
        left_value: edge.left_name,
        right_value: edge.right_name,
        score_units: score(1),
    })
    .expect("Reg AB relation hint emits");

    build_edge_evidence_record(
        edge.left_surface_id.to_string(),
        edge.right_surface_id.to_string(),
        vec![support, anti_merge, relation],
    )
    .expect("Reg AB guarded edge builds")
}

fn soft_parent_subsidiary_record() -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        "surf:regab:acme_depositor_llc",
        "surf:regab:acme_mortgage_trust_2024_c1",
        vec![
            support_hit("regab_parent_subsidiary_name_overlap", 7_000),
            EdgeEvidenceHit::new(
                ScoreLane::AntiMerge,
                "regab_firm_identity.guards",
                "soft_parent_subsidiary_review",
                "regab_parent_subsidiary_boundary",
                score(1_000),
                false,
                "parent/subsidiary name overlap requires review, not auto-merge",
            ),
            relation_hint_hit(RelationHintRequest {
                namespace: "regab_firm_identity.relations",
                operator_id: "relation_hint:sec10d_review_queue",
                reason_code: "regab_relation_context",
                relation: "parent_subsidiary_context",
                left_value: "Acme Depositor LLC",
                right_value: "Acme Mortgage Trust 2024-C1",
                score_units: score(1),
            })
            .expect("Reg AB parent/subsidiary relation hint emits"),
        ],
    )
    .expect("Reg AB parent/subsidiary edge builds")
}

fn unresolved_exact_lookup_record() -> EdgeEvidenceRecord {
    build_edge_evidence_record(
        "surf:regab:acme_review_analytics",
        "surf:regab:acme_review_analytics_llc",
        vec![support_hit("regab_unresolved_exact_lookup_miss", 4_000)],
    )
    .expect("Reg AB unresolved exact lookup edge builds")
}

fn support_hit(reason_code: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::Support,
        "regab_firm_identity.fixture_support",
        "sec10d_review_queue_candidate",
        reason_code,
        score(units),
        false,
        format!("sec10d review fixture candidate reason={reason_code}"),
    )
}

fn surface_provenance() -> Vec<SolveSurfaceProvenance> {
    vec![
        provenance(
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            32,
            5,
        ),
        provenance("surf:regab:pnc_bank_na", 52, 9),
        provenance("surf:regab:wells_fargo_bank_na", 17, 3),
        provenance(
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            22,
            4,
        ),
        provenance("surf:regab:kpmg_llp", 15, 2),
        provenance("surf:regab:kpmg_securitization_trust_2024_c1", 12, 2),
        provenance("surf:regab:acme_depositor_llc", 31, 5),
        provenance("surf:regab:acme_mortgage_trust_2024_c1", 25, 5),
        provenance("surf:regab:acme_review_analytics", 8, 1),
        provenance("surf:regab:acme_review_analytics_llc", 17, 4),
    ]
}

fn provenance_samples() -> Vec<ReviewProvenanceSample> {
    vec![
        sample(
            "surf:regab:pnc_bank_na",
            "regab-fixture-001",
            "regab_servicer_schedules:DEAL-PNC-2025",
            "PNC Bank, National Association",
        ),
        sample(
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            "regab-fixture-002",
            "regab_servicer_schedules:DEAL-PNC-2025",
            "Midland Loan Services, a division of PNC Bank, National Association",
        ),
        sample(
            "surf:regab:wells_fargo_bank_na",
            "regab-fixture-003",
            "regab_servicer_schedules:DEAL-WF-2025",
            "Wells Fargo Bank, National Association",
        ),
        sample(
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            "regab-fixture-005",
            "regab_platform_rosters:DEAL-WF-2025",
            "Wells Fargo Commercial Mortgage Securities Platform",
        ),
        sample(
            "surf:regab:kpmg_llp",
            "regab-fixture-006",
            "regab_attestations:DEAL-KPMG-2025",
            "KPMG LLP",
        ),
        sample(
            "surf:regab:kpmg_securitization_trust_2024_c1",
            "regab-fixture-007",
            "regab_attestations:DEAL-KPMG-2025",
            "KPMG Securitization Trust 2024-C1",
        ),
        sample(
            "surf:regab:acme_depositor_llc",
            "regab-fixture-008",
            "regab_servicer_schedules:DEAL-ACME-2025",
            "Acme Depositor LLC",
        ),
        sample(
            "surf:regab:acme_mortgage_trust_2024_c1",
            "regab-fixture-009",
            "regab_servicer_schedules:DEAL-ACME-2025",
            "Acme Mortgage Trust 2024-C1",
        ),
        sample(
            "surf:regab:acme_review_analytics_llc",
            "regab-fixture-010",
            "regab_servicer_schedules:DEAL-ACME-2025",
            "Acme Review Analytics LLC",
        ),
        sample(
            "surf:regab:acme_review_analytics",
            "regab-fixture-011",
            "regab_servicer_schedules:DEAL-ACME-2025",
            "Acme Review Analytics",
        ),
    ]
}

fn relation_hints() -> Vec<ReviewRelationHint> {
    vec![
        review_relation(
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            "surf:regab:pnc_bank_na",
            "division_of",
            "regab_division_relation_review",
        ),
        review_relation(
            "surf:regab:wells_fargo_bank_na",
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            "platform_to_firm_context",
            "regab_platform_relation_review",
        ),
        review_relation(
            "surf:regab:kpmg_llp",
            "surf:regab:kpmg_securitization_trust_2024_c1",
            "role_context_conflict",
            "regab_auditor_relation_review",
        ),
        review_relation(
            "surf:regab:acme_depositor_llc",
            "surf:regab:acme_mortgage_trust_2024_c1",
            "parent_subsidiary_context",
            "regab_parent_subsidiary_relation_review",
        ),
    ]
}

fn metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "regab_firm_identity".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "organization".to_string(),
            identity_semantics: "same_firm_or_reviewed_alias".to_string(),
            canonical_type: "firm".to_string(),
            patch_namespaces: EntityPatchNamespaces {
                aliases: "regab_firm_identity.aliases".to_string(),
                distinct: "regab_firm_identity.distinct".to_string(),
                relations: "regab_firm_identity.relations".to_string(),
            },
            content_hash: Some("blake3:regab-profile".to_string()),
        },
        strategy: EntityStrategyReference {
            id: "regab_firm_identity.v1".to_string(),
            version: "0.1.0".to_string(),
            content_hash: "blake3:regab-strategy".to_string(),
        },
        registry_snapshot: EntityRegistrySnapshot {
            id: "firms".to_string(),
            version: "1.0.12".to_string(),
            source: "tests/fixtures/entity/regab/sec10d_baseline_public/registry_snapshot/firms"
                .to_string(),
            lookup_snapshot_hash: "blake3:regab-registry".to_string(),
            sidecar_snapshot_hash: Some("blake3:regab-sidecars".to_string()),
        },
        patch_namespace: "regab_firm_identity.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 231,
            content_hash: "blake3:sec10d-review-input".to_string(),
        }),
        upstream_artifacts: vec![
            EntityArtifactReference {
                version: "canon_entity_block.v0".to_string(),
                content_hash: "blake3:sec10d-block".to_string(),
            },
            EntityArtifactReference {
                version: "canon_entity_edge.v0".to_string(),
                content_hash: "blake3:sec10d-edge".to_string(),
            },
        ],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn provenance(surface_id: &str, row_count: u64, deal_count: u64) -> SolveSurfaceProvenance {
    SolveSurfaceProvenance {
        surface_id: surface_id.to_string(),
        row_count,
        deal_count,
    }
}

fn sample(surface_id: &str, row_id: &str, source: &str, raw_value: &str) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row_id.to_string(),
        source: source.to_string(),
        raw_value: raw_value.to_string(),
    }
}

fn review_relation(
    left_surface_id: &str,
    right_surface_id: &str,
    relation: &str,
    reason_code: &str,
) -> ReviewRelationHint {
    ReviewRelationHint {
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        relation: relation.to_string(),
        reason_code: reason_code.to_string(),
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("fixture score is in range")
}

fn expected_fixture() -> ExpectedReviewQueue {
    serde_json::from_str(
        &fs::read_to_string(fixture_root().join("sec10d_review_queue_groups_expected.json"))
            .expect("expected review fixture opens"),
    )
    .expect("expected review fixture parses")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab/review")
}

#[derive(Debug, Clone, Copy)]
struct GuardedEdge<'a> {
    left_surface_id: &'a str,
    right_surface_id: &'a str,
    left_name: &'a str,
    right_name: &'a str,
    left_role: &'a str,
    right_role: &'a str,
    support_reason: &'a str,
    support_units: u32,
    guard: RegabFirmGuardKind,
    relation: &'a str,
}

#[derive(Debug, Deserialize)]
struct ExpectedReviewQueue {
    version: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    groups: Vec<ExpectedReviewGroup>,
}

#[derive(Debug, Deserialize)]
struct ExpectedReviewGroup {
    id: String,
    surface_ids: Vec<String>,
    state: SolveReconciliationState,
    proposed_action: String,
    affected_rows: u64,
    affected_deals: u64,
    required_priority_reasons: Vec<String>,
    negative_reason_codes: Vec<String>,
    relation_reason_codes: Vec<String>,
    min_provenance_samples: usize,
}
