#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_BLOCK_VERSION_V1,
        CANON_ENTITY_EVIDENCE_VERSION_V1, CANON_ENTITY_INDEX_VERSION_V1,
        CANON_ENTITY_PREPARE_VERSION_V1,
        anti_merge::{ProtectedTokenConflictRequest, protected_token_conflict_hit},
        block::{
            BlockCandidateBudgetConfig, BlockCandidateGenerationDiagnostics, BlockCandidateHit,
            BlockCandidateRecord, BlockOperatorCandidateDiagnostics, BlockOperatorYield,
        },
        block_artifact::{
            BlockCandidateArtifact, BlockCandidateArtifactRequest, CannotLinkAction,
            CannotLinkValidationHook, CannotLinkValidationStatus,
            EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketDiagnostics,
            ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
            build_block_candidate_artifact_contract,
        },
        contracts::{
            EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
            EntityInputReference, EntityNamekitReference, EntityPatchNamespaces,
            EntityPatchSetReference, EntityProfileReference, EntityRegistrySnapshot,
            EntityStrategyReference,
        },
        edge::{EdgeEvidenceHit, EdgeEvidenceRecord, build_edge_evidence_record},
        edge_artifact::{
            EdgeEvidenceArtifactRequest, build_edge_evidence_artifact_contract,
            validate_edge_evidence_artifact_contract,
        },
        evidence::{ExactViewSupportRequest, exact_view_support_hit},
        relation::{RelationHintRequest, relation_hint_hit},
        score::ScoreUnits,
    },
};
use serde_json::json;

#[test]
fn entity_edge_artifact_records_hashes_lanes_and_stable_order() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let buckets = vec![exact_bucket_assertion()];
    let block = sample_block_artifact(candidates.clone(), buckets.clone());
    let records = vec![support_record("surf:cmbs:001", "surf:cmbs:002")];
    let request = edge_request(block, candidates, buckets, records);

    let artifact = build_edge_evidence_artifact_contract(request.clone()).expect("edge artifact");
    let repeated = build_edge_evidence_artifact_contract(request).expect("repeat edge artifact");

    assert_eq!(artifact, repeated);
    assert_eq!(artifact.version, CANON_ENTITY_EVIDENCE_VERSION_V1);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert!(artifact.edge_records_hash.starts_with("blake3:"));
    assert!(artifact.candidate_records_hash.starts_with("blake3:"));
    assert!(artifact.bucket_assertions_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert!(artifact.upstream_artifacts.iter().any(|reference| {
        reference.version == CANON_ENTITY_BLOCK_VERSION_V1
            && reference.content_hash.starts_with("blake3:")
    }));
    assert_eq!(artifact.summary.counts["evidence_record_count"], 1);
    assert_eq!(artifact.summary.counts["support_hit_count"], 1);
    assert_eq!(artifact.summary.counts["cannot_link_hit_count"], 0);
    assert_eq!(artifact.summary.counts["relation_hint_count"], 0);
    assert_eq!(artifact.summary.counts["exact_bucket_count"], 1);

    validate_edge_evidence_artifact_contract(&artifact).expect("edge artifact validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B003_edge_artifact_keeps_support_candidate_self_contained() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let records = vec![support_record("surf:cmbs:001", "surf:cmbs:002")];
    let artifact =
        build_edge_evidence_artifact_contract(edge_request(block, candidates, Vec::new(), records))
            .expect("EN-B003 edge artifact builds");

    assert_eq!(artifact.summary.counts["evidence_records"], 1);
    assert_eq!(artifact.summary.counts["evidence_hit_count"], 1);
    assert_eq!(artifact.summary.counts["support_hit_count"], 1);
    assert_eq!(
        artifact.summary.labels["upstream_version"],
        CANON_ENTITY_BLOCK_VERSION_V1
    );
    validate_edge_evidence_artifact_contract(&artifact).expect("artifact contract validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B004_edge_artifact_keeps_relation_and_cannot_link_lanes() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:004",
        "relation_hint:tenant_related_distinct",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let records = vec![sears_auto_record()];
    let artifact =
        build_edge_evidence_artifact_contract(edge_request(block, candidates, Vec::new(), records))
            .expect("EN-B004 edge artifact builds");

    assert_eq!(artifact.summary.counts["evidence_record_count"], 1);
    assert_eq!(artifact.summary.counts["support_hit_count"], 1);
    assert_eq!(artifact.summary.counts["cannot_link_hit_count"], 1);
    assert_eq!(artifact.summary.counts["relation_hint_count"], 1);
    assert_eq!(artifact.summary.counts["hard_cannot_link_count"], 1);
    validate_edge_evidence_artifact_contract(&artifact).expect("artifact contract validates");
}

#[test]
fn entity_edge_artifact_refuses_unknown_candidate_pair() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let records = vec![support_record("surf:cmbs:001", "surf:cmbs:004")];
    let refusal =
        build_edge_evidence_artifact_contract(edge_request(block, candidates, Vec::new(), records))
            .expect_err("unknown edge pair refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["reason"], "unknown_candidate_pair");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

#[test]
fn entity_edge_artifact_refuses_stale_candidate_jsonl() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let mut stale_candidates = candidates.clone();
    stale_candidates[0].candidate_score_hint = 8_000;
    let refusal = build_edge_evidence_artifact_contract(edge_request(
        block,
        stale_candidates,
        Vec::new(),
        vec![support_record("surf:cmbs:001", "surf:cmbs:002")],
    ))
    .expect_err("stale candidates refuse");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["reason"], "stale_candidate_records");
}

#[test]
fn entity_edge_artifact_validator_refuses_self_hash_drift() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let mut artifact = build_edge_evidence_artifact_contract(edge_request(
        block,
        candidates,
        Vec::new(),
        vec![support_record("surf:cmbs:001", "surf:cmbs:002")],
    ))
    .expect("edge artifact builds");
    artifact
        .summary
        .counts
        .insert("evidence_record_count".to_string(), 99);

    let refusal =
        validate_edge_evidence_artifact_contract(&artifact).expect_err("hash drift refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
}

#[test]
fn entity_edge_artifact_validator_refuses_upstream_reference_mismatch() {
    let candidates = vec![candidate_record(
        "surf:cmbs:001",
        "surf:cmbs:002",
        "rare_token_overlap:tenant_tokens",
        9_000,
    )];
    let block = sample_block_artifact(candidates.clone(), Vec::new());
    let mut artifact = build_edge_evidence_artifact_contract(edge_request(
        block,
        candidates,
        Vec::new(),
        vec![support_record("surf:cmbs:001", "surf:cmbs:002")],
    ))
    .expect("evidence artifact builds");
    artifact.upstream_artifacts[0].content_hash = "blake3:other-upstream".to_string();

    let refusal =
        validate_edge_evidence_artifact_contract(&artifact).expect_err("upstream mismatch refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "evidence");
    assert_eq!(refusal.detail["field"], "upstream_artifacts");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

fn edge_request(
    block: BlockCandidateArtifact,
    candidate_records: Vec<BlockCandidateRecord>,
    bucket_assertions: Vec<ExactBucketAssertion>,
    edge_records: Vec<EdgeEvidenceRecord>,
) -> EdgeEvidenceArtifactRequest {
    EdgeEvidenceArtifactRequest {
        block,
        strategy: sample_edge_strategy(),
        edge_records_path: "evidence/evidence.jsonl".to_string(),
        edge_records,
        candidate_records,
        bucket_assertions,
    }
}

fn sample_block_artifact(
    candidate_records: Vec<BlockCandidateRecord>,
    bucket_assertions: Vec<ExactBucketAssertion>,
) -> BlockCandidateArtifact {
    build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_diagnostics_path: "block/diagnostics.json".to_string(),
        candidate_records: candidate_records.clone(),
        bucket_assertions,
        known_surface_ids: vec![
            "surf:cmbs:001".to_string(),
            "surf:cmbs:002".to_string(),
            "surf:cmbs:004".to_string(),
        ],
        diagnostics: diagnostics(candidate_records.len() as u64),
    })
    .expect("block artifact builds")
}

fn support_record(left_surface_id: &str, right_surface_id: &str) -> EdgeEvidenceRecord {
    let mut record = build_edge_evidence_record(
        left_surface_id,
        right_surface_id,
        vec![support_hit("exact_tenant_core")],
    )
    .expect("support evidence record builds");
    record.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    record
}

fn sears_auto_record() -> EdgeEvidenceRecord {
    let mut record = build_edge_evidence_record(
        "surf:cmbs:001",
        "surf:cmbs:004",
        vec![
            support_hit("tenant_core_similarity"),
            protected_token_conflict_hit(ProtectedTokenConflictRequest {
                namespace: "tenant_role",
                operator_id: "protected_token_conflict:tenant_brand",
                reason_code: "protected_token_conflict",
                left_tokens: &["sears"],
                right_tokens: &["sears", "auto", "center"],
                score_units: score(10_000),
            })
            .expect("anti-merge hit"),
            relation_hint_hit(RelationHintRequest {
                namespace: "ontology",
                operator_id: "relation_hint:related_brand_family",
                reason_code: "related_brand_family",
                relation: "related_brand_family",
                left_value: "Sears",
                right_value: "Sears Auto Center",
                score_units: score(10_000),
            })
            .expect("relation hit"),
        ],
    )
    .expect("Sears Auto evidence record builds");
    record.version = CANON_ENTITY_EVIDENCE_VERSION_V1.to_string();
    record
}

fn support_hit(reason_code: &str) -> EdgeEvidenceHit {
    exact_view_support_hit(ExactViewSupportRequest {
        namespace: "name",
        operator_id: "exact_view:tenant_core",
        reason_code,
        view_name: "tenant_core",
        left_value: "sears",
        right_value: "sears",
        score_units: score(10_000),
    })
    .expect("support hit")
}

fn candidate_record(
    left_surface_id: &str,
    right_surface_id: &str,
    operator_id: &str,
    score_units: u32,
) -> BlockCandidateRecord {
    BlockCandidateRecord {
        version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        block_hits: vec![BlockCandidateHit {
            operator_id: operator_id.to_string(),
            rank: Some(1),
            score_units,
        }],
        candidate_score_hint: score_units,
    }
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
            operator_id: "test_operator".to_string(),
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
        operator_diagnostics: vec![BlockOperatorCandidateDiagnostics {
            operator_id: "test_operator".to_string(),
            input_candidate_count: candidate_count,
            eligible_candidate_count: candidate_count,
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
    }
}

fn exact_bucket_assertion() -> ExactBucketAssertion {
    ExactBucketAssertion {
        version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
        bucket_id: "bucket:tenant_core:sears".to_string(),
        operator_id: "exact_view:tenant_core".to_string(),
        profile: ExactBucketProfile {
            id: "cmbs_tenant_label".to_string(),
            version: "0.1.0".to_string(),
            identity_semantics: "canonical_display_label".to_string(),
            content_hash: "blake3:profile".to_string(),
        },
        upstream: ExactBucketUpstream {
            prepare_hash: "blake3:prepare".to_string(),
            index_hash: "blake3:index".to_string(),
            strategy_hash: "blake3:block-strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
        },
        membership: ExactBucketMembership {
            surface_ids: vec!["surf:cmbs:001".to_string(), "surf:cmbs:002".to_string()],
            surface_ranges: Vec::new(),
        },
        row_count: 2,
        deal_count: 1,
        pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
        diagnostics: ExactBucketDiagnostics {
            largest_bucket_size: 2,
            suppressed_pair_count: 1,
            labels: Default::default(),
        },
        cannot_link_validation: CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    }
}

fn sample_index_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_INDEX_VERSION_V1.to_string(),
        metadata: EntityArtifactMetadata {
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
                id: "cmbs_tenant_label.index".to_string(),
                version: "0.1.0".to_string(),
                content_hash: "blake3:index-strategy".to_string(),
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
            upstream_artifacts: vec![EntityArtifactReference {
                version: CANON_ENTITY_PREPARE_VERSION_V1.to_string(),
                content_hash: "blake3:prepare".to_string(),
            }],
            patch_set: Some(EntityPatchSetReference {
                content_hash: "blake3:patch".to_string(),
                paths: vec!["patches/cmbs-tenants.yaml".to_string()],
            }),
            namekit: Some(EntityNamekitReference {
                version: "namekit.v0".to_string(),
                content_hash: "blake3:namekit".to_string(),
            }),
            artifact_content_hash: "blake3:index".to_string(),
        },
        summary: Default::default(),
    }
}

fn sample_block_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.block".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:block-strategy".to_string(),
    }
}

fn sample_edge_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.edge".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:edge-strategy".to_string(),
    }
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("test score is inside score scale")
}
