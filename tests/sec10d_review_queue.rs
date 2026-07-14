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
    profiles::regab::{RegabFirmGuardKind, RegabFirmGuardRequest, regab_firm_guard_hit},
    relation::{RelationHintRequest, relation_hint_hit},
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
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn sec10d_review_queue_groups_regab_ambiguities_and_unresolved_misses() {
    let expected = review_fixture();
    let artifact = regab_review_queue_artifact();

    assert_eq!(artifact.version, expected.version);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.summary.counts, expected.summary_counts);
    assert_eq!(artifact.summary.labels, expected.summary_labels);
    assert_eq!(artifact.review_items.len(), expected.groups.len());

    for group in &expected.groups {
        let item = item_for_surfaces(&artifact, &group.surface_ids);
        assert_eq!(item.state, group.state, "{}", group.id);
        assert_eq!(
            item.proposed_action, group.proposed_action,
            "{} proposed action",
            group.id
        );
        assert_eq!(item.affected_rows, group.affected_rows, "{} rows", group.id);
        assert_eq!(
            item.affected_deals, group.affected_deals,
            "{} deals",
            group.id
        );
        assert_contains_all(
            &item.priority_reasons,
            &group.required_priority_reasons,
            &group.id,
        );
        assert_contains_all(
            &negative_reason_codes(item),
            &group.negative_reason_codes,
            &group.id,
        );
        assert_contains_all(
            &relation_reason_codes(item),
            &group.relation_reason_codes,
            &group.id,
        );
        assert!(
            item.provenance_samples.len() >= group.min_provenance_samples,
            "{} provenance samples",
            group.id
        );
        assert!(
            item.review_priority_units > 0,
            "{} priority units",
            group.id
        );
    }

    let platform = item_for_fixture_group(&artifact, &expected, "REGAB-I003-PLATFORM-LABEL");
    assert!(
        !matches!(
            platform.state,
            SolveReconciliationState::ResolvedExisting | SolveReconciliationState::PromotableNew
        ),
        "platform/category labels must not auto-promote as firms"
    );

    let unresolved =
        item_for_fixture_group(&artifact, &expected, "REGAB-UNRESOLVED-EXACT-LOOKUP-MISS");
    assert_eq!(unresolved.state, SolveReconciliationState::Escrow);
    assert!(unresolved.strongest_negative_cut.is_none());

    let csv = render_review_queue_csv(&artifact).expect("review queue csv renders");
    let records = csv::Reader::from_reader(csv.as_bytes())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows parse");
    assert_eq!(records.len(), expected.groups.len());
}

fn regab_review_queue_artifact() -> ReviewQueueArtifact {
    build_review_queue_artifact(ReviewQueueRequest {
        solve_artifact: regab_solve_artifact(),
        include: ReviewExportInclude::All,
        provenance_samples: provenance_samples(),
        relation_hints: review_relation_hints(),
    })
    .expect("Reg AB review queue builds")
}

fn regab_solve_artifact() -> SolveArtifact {
    let edge_records = vec![
        guarded_edge_record(GuardedEdgeCase {
            left_surface_id: "surf:regab:pnc_bank_na",
            right_surface_id: "surf:regab:midland_loan_services_division_pnc_bank_na",
            left_name: "PNC Bank, National Association",
            right_name: "Midland Loan Services, a division of PNC Bank, National Association",
            left_role: "servicer",
            right_role: "master_servicer",
            guard: RegabFirmGuardKind::BankLoanServicesDivision,
            relation: "division_of",
            support_reason: "regab_same_family_high_recall_candidate",
            support_units: 7_000,
            guard_units: 10_000,
        }),
        guarded_edge_record(GuardedEdgeCase {
            left_surface_id: "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            right_surface_id: "surf:regab:wells_fargo_bank_na",
            left_name: "Wells Fargo Commercial Mortgage Securities Platform",
            right_name: "Wells Fargo Bank, National Association",
            left_role: "platform",
            right_role: "regulated_firm",
            guard: RegabFirmGuardKind::PlatformCategoryLabel,
            relation: "platform_to_firm_context",
            support_reason: "regab_shared_platform_family_candidate",
            support_units: 6_500,
            guard_units: 10_000,
        }),
        guarded_edge_record(GuardedEdgeCase {
            left_surface_id: "surf:regab:kpmg_llp",
            right_surface_id: "surf:regab:kpmg_securitization_trust_2024_c1",
            left_name: "KPMG LLP",
            right_name: "KPMG Securitization Trust 2024-C1",
            left_role: "auditor",
            right_role: "subject_party",
            guard: RegabFirmGuardKind::AuditorSubjectPartyRoleConflict,
            relation: "role_context_conflict",
            support_reason: "regab_shared_auditor_token_candidate",
            support_units: 6_200,
            guard_units: 10_000,
        }),
        soft_review_edge_record(
            "surf:regab:acme_depositor_llc",
            "surf:regab:acme_mortgage_trust_2024_c1",
            "regab_parent_subsidiary_name_overlap",
            "regab_parent_subsidiary_boundary",
            8_000,
            1_500,
        ),
        support_only_edge_record(
            "surf:regab:acme_review_analytics",
            "surf:regab:acme_review_analytics_llc",
            "regab_unresolved_exact_lookup_miss",
            3_000,
        ),
    ];
    let evidence = evidence_artifact_for_review_edges(&edge_records);
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![],
    })
    .expect("Reg AB signed graph builds");
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
        provenance: solve_provenance(),
        decision_ledger_path: "solve/sec10d-regab-decision-ledger.jsonl".to_string(),
    })
    .expect("Reg AB solve artifact builds")
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
    .expect("Reg AB block artifact builds");
    build_edge_evidence_artifact_contract(EdgeEvidenceArtifactRequest {
        block,
        strategy: evidence_strategy(),
        edge_records_path: "evidence/evidence.jsonl".to_string(),
        edge_records: evidence_records,
        candidate_records,
        bucket_assertions: vec![],
    })
    .expect("Reg AB evidence artifact builds")
}

fn candidate_records_for_edges(edge_records: &[EdgeEvidenceRecord]) -> Vec<BlockCandidateRecord> {
    let mut candidates = edge_records
        .iter()
        .map(|record| BlockCandidateRecord {
            version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
            left_surface_id: record.left_surface_id.clone(),
            right_surface_id: record.right_surface_id.clone(),
            block_hits: vec![BlockCandidateHit {
                operator_id: "sec10d_review_queue:block_candidate".to_string(),
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
            operator_id: "sec10d_review_queue:block_candidate".to_string(),
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
        operator_diagnostics: vec![BlockOperatorCandidateDiagnostics {
            operator_id: "sec10d_review_queue:block_candidate".to_string(),
            input_candidate_count: candidate_count,
            eligible_candidate_count: candidate_count,
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
    }
}

#[derive(Debug, Clone, Copy)]
struct GuardedEdgeCase<'a> {
    left_surface_id: &'a str,
    right_surface_id: &'a str,
    left_name: &'a str,
    right_name: &'a str,
    left_role: &'a str,
    right_role: &'a str,
    guard: RegabFirmGuardKind,
    relation: &'a str,
    support_reason: &'a str,
    support_units: u32,
    guard_units: u32,
}

fn guarded_edge_record(case: GuardedEdgeCase<'_>) -> EdgeEvidenceRecord {
    let (left_surface_id, right_surface_id) =
        ordered_pair(case.left_surface_id, case.right_surface_id);
    let anti_merge = regab_firm_guard_hit(RegabFirmGuardRequest {
        namespace: "regab_firm_identity.guards",
        guard: case.guard,
        left_name: case.left_name,
        right_name: case.right_name,
        left_role: Some(case.left_role),
        right_role: Some(case.right_role),
        score_units: score(case.guard_units),
    })
    .expect("Reg AB guard emits anti-merge evidence");
    let relation = relation_hint_hit(RelationHintRequest {
        namespace: "regab_firm_identity.relations",
        operator_id: "relation_hint:sec10d_review_queue",
        reason_code: "regab_relation_context",
        relation: case.relation,
        left_value: case.left_name,
        right_value: case.right_name,
        score_units: score(1),
    })
    .expect("Reg AB relation hint emits");

    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![
            support_hit(case.support_reason, case.support_units),
            anti_merge,
            relation,
        ],
    )
    .expect("guarded Reg AB edge builds")
}

fn soft_review_edge_record(
    left_surface_id: &str,
    right_surface_id: &str,
    support_reason: &str,
    anti_merge_reason: &str,
    support_units: u32,
    anti_merge_units: u32,
) -> EdgeEvidenceRecord {
    let (left_surface_id, right_surface_id) = ordered_pair(left_surface_id, right_surface_id);
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![
            support_hit(support_reason, support_units),
            EdgeEvidenceHit::new(
                ScoreLane::AntiMerge,
                "regab_firm_identity.guards",
                "parent_subsidiary_review:sec10d",
                anti_merge_reason,
                score(anti_merge_units),
                false,
                "parent/subsidiary context requires review before alias promotion",
            ),
        ],
    )
    .expect("soft Reg AB review edge builds")
}

fn support_only_edge_record(
    left_surface_id: &str,
    right_surface_id: &str,
    support_reason: &str,
    support_units: u32,
) -> EdgeEvidenceRecord {
    let (left_surface_id, right_surface_id) = ordered_pair(left_surface_id, right_surface_id);
    build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![support_hit(support_reason, support_units)],
    )
    .expect("support-only Reg AB edge builds")
}

fn support_hit(reason_code: &str, units: u32) -> EdgeEvidenceHit {
    EdgeEvidenceHit::new(
        ScoreLane::Support,
        "regab_firm_identity.fixture_support",
        "sec10d_review_queue_candidate",
        reason_code,
        score(units),
        false,
        format!("sec10d Reg AB review fixture support reason={reason_code}"),
    )
}

fn regab_metadata() -> EntityArtifactMetadata {
    EntityArtifactMetadata {
        profile: EntityProfileReference {
            id: "regab_firm_identity".to_string(),
            version: "0.1.0".to_string(),
            entity_type: "firm".to_string(),
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
            lookup_snapshot_hash: "blake3:regab-firms-lookup".to_string(),
            sidecar_snapshot_hash: Some("blake3:regab-firms-sidecars".to_string()),
        },
        patch_namespace: "regab_firm_identity.aliases".to_string(),
        input: Some(EntityInputReference {
            row_count: 231,
            content_hash: "blake3:regab-review-input".to_string(),
        }),
        upstream_artifacts: vec![],
        patch_set: None,
        namekit: None,
        artifact_content_hash: String::new(),
    }
}

fn index_header() -> EntityArtifactHeader {
    let mut metadata = regab_metadata();
    metadata.strategy = EntityStrategyReference {
        id: "regab_firm_identity.index".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:regab-index-strategy".to_string(),
    };
    metadata.upstream_artifacts = vec![EntityArtifactReference {
        version: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
        content_hash: "blake3:regab-prepare".to_string(),
    }];
    metadata.artifact_content_hash = "blake3:regab-index".to_string();
    EntityArtifactHeader {
        version: CANON_ENTITY_INDEX_VERSION_V1.to_string(),
        metadata,
        summary: Default::default(),
    }
}

fn block_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "regab_firm_identity.block".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:regab-block-strategy".to_string(),
    }
}

fn evidence_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "regab_firm_identity.evidence".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:regab-evidence-strategy".to_string(),
    }
}

fn solve_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "regab_firm_identity.solve".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:regab-solve-strategy".to_string(),
    }
}

fn solve_provenance() -> Vec<SolveSurfaceProvenance> {
    vec![
        provenance("surf:regab:pnc_bank_na", 46, 8),
        provenance(
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            38,
            6,
        ),
        provenance("surf:regab:wells_fargo_bank_na", 21, 4),
        provenance(
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            18,
            3,
        ),
        provenance("surf:regab:kpmg_llp", 14, 2),
        provenance("surf:regab:kpmg_securitization_trust_2024_c1", 13, 2),
        provenance("surf:regab:acme_depositor_llc", 31, 5),
        provenance("surf:regab:acme_mortgage_trust_2024_c1", 25, 5),
        provenance("surf:regab:acme_review_analytics", 16, 3),
        provenance("surf:regab:acme_review_analytics_llc", 9, 2),
    ]
}

fn provenance_samples() -> Vec<ReviewProvenanceSample> {
    vec![
        sample(
            "surf:regab:pnc_bank_na",
            "regab-fixture-001",
            "PNC Bank, National Association",
        ),
        sample(
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            "regab-fixture-002",
            "Midland Loan Services, a division of PNC Bank, National Association",
        ),
        sample(
            "surf:regab:wells_fargo_bank_na",
            "regab-fixture-003",
            "Wells Fargo Bank, National Association",
        ),
        sample(
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            "regab-fixture-004",
            "Wells Fargo Commercial Mortgage Securities Platform",
        ),
        sample("surf:regab:kpmg_llp", "regab-fixture-005", "KPMG LLP"),
        sample(
            "surf:regab:kpmg_securitization_trust_2024_c1",
            "regab-fixture-006",
            "KPMG Securitization Trust 2024-C1",
        ),
        sample(
            "surf:regab:acme_depositor_llc",
            "regab-fixture-007",
            "ACME Depositor LLC",
        ),
        sample(
            "surf:regab:acme_mortgage_trust_2024_c1",
            "regab-fixture-008",
            "ACME Mortgage Trust 2024-C1",
        ),
        sample(
            "surf:regab:acme_review_analytics",
            "regab-fixture-009",
            "Acme Review Analytics",
        ),
        sample(
            "surf:regab:acme_review_analytics_llc",
            "regab-fixture-010",
            "Acme Review Analytics LLC",
        ),
    ]
}

fn review_relation_hints() -> Vec<ReviewRelationHint> {
    vec![
        relation_hint(
            "surf:regab:pnc_bank_na",
            "surf:regab:midland_loan_services_division_pnc_bank_na",
            "division_of",
            "regab_division_relation_review",
        ),
        relation_hint(
            "surf:regab:wells_fargo_commercial_mortgage_securities_platform",
            "surf:regab:wells_fargo_bank_na",
            "platform_to_firm_context",
            "regab_platform_relation_review",
        ),
        relation_hint(
            "surf:regab:kpmg_llp",
            "surf:regab:kpmg_securitization_trust_2024_c1",
            "role_context_conflict",
            "regab_auditor_relation_review",
        ),
        relation_hint(
            "surf:regab:acme_depositor_llc",
            "surf:regab:acme_mortgage_trust_2024_c1",
            "parent_subsidiary_context",
            "regab_parent_subsidiary_relation_review",
        ),
    ]
}

fn relation_hint(
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

fn provenance(surface_id: &str, row_count: u64, deal_count: u64) -> SolveSurfaceProvenance {
    SolveSurfaceProvenance {
        surface_id: surface_id.to_string(),
        row_count,
        deal_count,
    }
}

fn sample(surface_id: &str, row_id: &str, raw_value: &str) -> ReviewProvenanceSample {
    ReviewProvenanceSample {
        surface_id: surface_id.to_string(),
        row_id: row_id.to_string(),
        source: "sec10d_regab_org_mentions".to_string(),
        raw_value: raw_value.to_string(),
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}

fn ordered_pair<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn item_for_fixture_group<'a>(
    artifact: &'a ReviewQueueArtifact,
    expected: &ReviewFixture,
    id: &str,
) -> &'a ReviewQueueItem {
    let group = expected
        .groups
        .iter()
        .find(|group| group.id == id)
        .unwrap_or_else(|| panic!("missing fixture group {id}"));
    item_for_surfaces(artifact, &group.surface_ids)
}

fn item_for_surfaces<'a>(
    artifact: &'a ReviewQueueArtifact,
    surface_ids: &[String],
) -> &'a ReviewQueueItem {
    let mut expected = surface_ids.to_vec();
    expected.sort();
    artifact
        .review_items
        .iter()
        .find(|item| item.surface_ids == expected)
        .unwrap_or_else(|| panic!("missing review item for surfaces {expected:?}"))
}

fn negative_reason_codes(item: &ReviewQueueItem) -> Vec<String> {
    item.strongest_negative_cut
        .as_ref()
        .map(|cut| cut.evidence_reason_codes.clone())
        .unwrap_or_default()
}

fn relation_reason_codes(item: &ReviewQueueItem) -> Vec<String> {
    item.relation_hints
        .iter()
        .map(|hint| hint.reason_code.clone())
        .collect()
}

fn assert_contains_all(actual: &[String], expected: &[String], label: &str) {
    for value in expected {
        assert!(
            actual.iter().any(|actual| actual == value),
            "{label} missing {value}; actual={actual:?}"
        );
    }
}

fn review_fixture() -> ReviewFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity/regab/review/sec10d_review_queue_groups_expected.json");
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[derive(Debug, Deserialize)]
struct ReviewFixture {
    version: String,
    summary_counts: BTreeMap<String, u64>,
    summary_labels: BTreeMap<String, String>,
    groups: Vec<ReviewGroupFixture>,
}

#[derive(Debug, Deserialize)]
struct ReviewGroupFixture {
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
