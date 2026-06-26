#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_INDEX_VERSION,
        EntityArtifactHeader, EntityArtifactMetadata, EntityDeterministicSummary,
        EntityInputReference, EntityNamekitReference, EntityPatchNamespaces,
        EntityPatchSetReference, EntityProfileReference, EntityRegistrySnapshot,
        EntityStrategyReference,
        block::{
            AliasPatchMatchBlockOperator, AliasPatchPair, BlockCandidateBudgetConfig,
            BlockCandidateGenerationDiagnostics, BlockCandidateGenerationRequest,
            BlockCandidateHit, BlockCandidateOperator, BlockCandidateRecord,
            BlockOperatorCandidateDiagnostics, BlockOperatorYield, RareTokenOverlapBlockOperator,
            generate_block_candidates,
        },
        block_artifact::{
            CannotLinkAction, CannotLinkValidationHook, CannotLinkValidationStatus,
            EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN, ExactBucketAssertion, ExactBucketDiagnostics,
            ExactBucketMembership, ExactBucketProfile, ExactBucketUpstream,
            build_block_candidate_artifact_contract, validate_block_candidate_artifact_contract,
        },
        edge::EdgeCandidateBudgetProof,
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn entity_block_artifact_contract_carries_hashes_summary_and_validates() {
    let candidates = vec![candidate_record("surf:001", "surf:002")];
    let buckets = vec![exact_bucket_assertion()];
    let request = artifact_request(candidates.clone(), buckets.clone(), known_surface_ids());

    let artifact = build_block_candidate_artifact_contract(request).expect("artifact builds");

    validate_block_candidate_artifact_contract(&artifact).expect("artifact validates");
    assert_eq!(artifact.version, CANON_ENTITY_BLOCK_VERSION);
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(
        artifact.metadata.strategy.content_hash,
        "blake3:block-strategy"
    );
    assert_eq!(
        artifact.metadata.registry_snapshot.lookup_snapshot_hash,
        "blake3:registry"
    );
    assert_eq!(artifact.upstream_artifacts.len(), 2);
    assert_eq!(
        artifact.upstream_artifacts[0].content_hash,
        "blake3:prepare"
    );
    assert_eq!(artifact.upstream_artifacts[1].content_hash, "blake3:index");
    assert!(artifact.candidate_records_hash.starts_with("blake3:"));
    assert!(artifact.bucket_assertions_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.summary.counts["candidate_pairs"],
        candidates.len() as u64
    );
    assert_eq!(artifact.summary.counts["block_hits"], 1);
    assert_eq!(artifact.summary.counts["exact_bucket_count"], 1);
    assert_eq!(
        artifact.summary.counts["exact_bucket_pair_expansion_count"],
        0
    );
    assert_eq!(artifact.summary.labels["blocking"], "bounded");

    let replayed = build_block_candidate_artifact_contract(artifact_request(
        candidates,
        buckets,
        known_surface_ids(),
    ))
    .expect("artifact replay builds");
    assert_eq!(
        serde_json::to_vec(&artifact).expect("artifact serializes"),
        serde_json::to_vec(&replayed).expect("replayed artifact serializes")
    );
}

#[test]
#[allow(non_snake_case)]
fn EN_B001_block_artifact_keeps_eight_thousand_row_exact_bucket_compact() {
    let mut bucket = exact_bucket_assertion();
    bucket.membership.surface_ids = vec!["surf:001".to_string()];
    bucket.row_count = 8_000;
    bucket.deal_count = 934;
    bucket.diagnostics.largest_bucket_size = 8_000;
    bucket.diagnostics.suppressed_pair_count = 8_000_u64 * 7_999 / 2;

    let artifact = build_block_candidate_artifact_contract(artifact_request(
        Vec::new(),
        vec![bucket],
        vec!["surf:001".to_string()],
    ))
    .expect("compact exact bucket artifact builds");

    assert_eq!(artifact.summary.counts["candidate_pairs"], 0);
    assert_eq!(artifact.summary.counts["exact_bucket_count"], 1);
    assert_eq!(
        artifact.summary.counts["exact_bucket_pair_expansion_count"],
        0
    );
    validate_block_candidate_artifact_contract(&artifact).expect("compact artifact validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B002_block_artifact_records_common_token_suppression_without_candidates() {
    let posting_index = common_token_posting_index(8);
    let result = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(4, 16, 32),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(0)
                .with_topk(8, 8)
                .with_max_posting_size(3),
        )],
    })
    .expect("common-token block generation stays bounded");

    assert!(result.candidates.is_empty());
    assert!(result.diagnostics.large_buckets_suppressed > 0);

    let artifact = build_block_candidate_artifact_contract(artifact_request_with_diagnostics(
        result.candidates,
        Vec::new(),
        common_token_surface_ids(8),
        result.diagnostics,
    ))
    .expect("bounded common-token artifact builds");

    assert_eq!(artifact.summary.counts["candidate_pairs"], 0);
    assert_eq!(artifact.summary.counts["large_buckets_suppressed"], 8);
    validate_block_candidate_artifact_contract(&artifact).expect("bounded artifact validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B005_candidate_budget_refusal_happens_before_block_artifact_build() {
    let posting_index = alias_budget_posting_index();
    let refusal = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(1, 16, 16),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![
                    AliasPatchPair::new("surf:001", "surf:002", "patch:one"),
                    AliasPatchPair::new("surf:001", "surf:003", "patch:two"),
                ],
            ),
        )],
    })
    .expect_err("candidate cap breach refuses before artifact emission");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["artifact"], "candidate_artifact");
    assert_eq!(refusal.detail["candidate_artifact_written"], json!(false));
    assert_eq!(
        refusal.detail["partial_candidate_artifact_written"],
        json!(false)
    );
}

#[test]
fn entity_block_artifact_refuses_unstable_candidate_order() {
    let refusal = build_block_candidate_artifact_contract(artifact_request(
        vec![
            candidate_record_with_score("surf:001", "surf:002", "alias_patch_match", 10),
            candidate_record_with_score("surf:001", "surf:003", "alias_patch_match", 100),
        ],
        Vec::new(),
        vec![
            "surf:001".to_string(),
            "surf:002".to_string(),
            "surf:003".to_string(),
        ],
    ))
    .expect_err("unstable candidate order refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "unstable_candidate_order");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

#[test]
fn entity_block_artifact_validator_refuses_self_hash_drift() {
    let mut artifact = build_block_candidate_artifact_contract(artifact_request(
        vec![candidate_record("surf:001", "surf:002")],
        vec![exact_bucket_assertion()],
        known_surface_ids(),
    ))
    .expect("artifact builds");
    artifact
        .summary
        .counts
        .insert("candidate_pairs".to_string(), 99);

    let refusal =
        validate_block_candidate_artifact_contract(&artifact).expect_err("hash drift refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["field"], "artifact_content_hash");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

#[test]
fn entity_block_artifact_refuses_unknown_candidate_surface() {
    let refusal = build_block_candidate_artifact_contract(artifact_request(
        vec![candidate_record("surf:001", "surf:missing")],
        vec![exact_bucket_assertion()],
        known_surface_ids(),
    ))
    .expect_err("unknown candidate surface refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "unknown_surface_id");
    assert_eq!(refusal.detail["surface_id"], "surf:missing");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

#[test]
fn entity_block_artifact_refuses_stale_exact_bucket_hashes() {
    let mut bucket = exact_bucket_assertion();
    bucket.upstream.strategy_hash = "blake3:old-block-strategy".to_string();
    let refusal = build_block_candidate_artifact_contract(artifact_request(
        vec![candidate_record("surf:001", "surf:002")],
        vec![bucket],
        known_surface_ids(),
    ))
    .expect_err("stale exact bucket refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "stale_exact_bucket_assertion");
    assert_eq!(refusal.detail["field"], "upstream.strategy_hash");
    assert_eq!(refusal.detail["expected"], "blake3:block-strategy");
    assert_eq!(refusal.detail["actual"], "blake3:old-block-strategy");
}

fn artifact_request(
    candidates: Vec<BlockCandidateRecord>,
    bucket_assertions: Vec<ExactBucketAssertion>,
    known_surface_ids: Vec<String>,
) -> canon::entity::block_artifact::BlockCandidateArtifactRequest {
    let candidate_artifact_bytes = serde_json::to_vec(&candidates)
        .expect("candidate records serialize")
        .len() as u64;
    canon::entity::block_artifact::BlockCandidateArtifactRequest {
        index: index_header(),
        strategy: block_strategy(),
        candidate_records_path: "entity/block/candidates.jsonl".to_string(),
        diagnostics: diagnostics(candidates.len() as u64, candidate_artifact_bytes),
        candidate_records: candidates,
        bucket_assertions,
        known_surface_ids,
    }
}

fn artifact_request_with_diagnostics(
    candidates: Vec<BlockCandidateRecord>,
    bucket_assertions: Vec<ExactBucketAssertion>,
    known_surface_ids: Vec<String>,
    diagnostics: BlockCandidateGenerationDiagnostics,
) -> canon::entity::block_artifact::BlockCandidateArtifactRequest {
    canon::entity::block_artifact::BlockCandidateArtifactRequest {
        index: index_header(),
        strategy: block_strategy(),
        candidate_records_path: "entity/block/candidates.jsonl".to_string(),
        diagnostics,
        candidate_records: candidates,
        bucket_assertions,
        known_surface_ids,
    }
}

fn candidate_record(left_surface_id: &str, right_surface_id: &str) -> BlockCandidateRecord {
    candidate_record_with_score(
        left_surface_id,
        right_surface_id,
        "alias_patch_match",
        1_000_000,
    )
}

fn candidate_record_with_score(
    left_surface_id: &str,
    right_surface_id: &str,
    operator_id: &str,
    score_units: u32,
) -> BlockCandidateRecord {
    BlockCandidateRecord {
        version: CANON_ENTITY_BLOCK_VERSION.to_string(),
        left_surface_id: left_surface_id.to_string(),
        right_surface_id: right_surface_id.to_string(),
        block_hits: vec![BlockCandidateHit {
            operator_id: operator_id.to_string(),
            rank: None,
            score_units,
        }],
        candidate_score_hint: score_units,
    }
}

fn exact_bucket_assertion() -> ExactBucketAssertion {
    ExactBucketAssertion {
        version: CANON_ENTITY_BLOCK_BUCKET_VERSION.to_string(),
        bucket_id: "bucket:tenant_core:sears".to_string(),
        operator_id: "exact_bucket:tenant_core".to_string(),
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
            surface_ids: known_surface_ids(),
            surface_ranges: vec![],
        },
        row_count: 2,
        deal_count: 1,
        pair_expansion: EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN.to_string(),
        diagnostics: ExactBucketDiagnostics {
            largest_bucket_size: 2,
            suppressed_pair_count: 1,
            labels: BTreeMap::from([("identity_view".to_string(), "tenant_core".to_string())]),
        },
        cannot_link_validation: CannotLinkValidationHook {
            status: CannotLinkValidationStatus::CheckedNoConflicts,
            checked_fact_count: 0,
            hard_cannot_link_count: 0,
            action: CannotLinkAction::AllowMerge,
        },
    }
}

fn diagnostics(
    candidate_count: u64,
    candidate_artifact_bytes: u64,
) -> BlockCandidateGenerationDiagnostics {
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
        candidate_budget: EdgeCandidateBudgetProof::within_run_budget(candidate_count, 128),
        candidate_artifact_bytes,
        partial_candidate_artifact_written: false,
        operator_yield: vec![BlockOperatorYield {
            operator_id: "alias_patch_match".to_string(),
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
        operator_diagnostics: vec![BlockOperatorCandidateDiagnostics {
            operator_id: "alias_patch_match".to_string(),
            input_candidate_count: candidate_count,
            eligible_candidate_count: candidate_count,
            emitted_candidate_count: candidate_count,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
    }
}

fn index_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_INDEX_VERSION.to_string(),
        metadata: EntityArtifactMetadata {
            profile: profile_reference(),
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
                sidecar_snapshot_hash: None,
            },
            patch_namespace: "cmbs_tenant_label.aliases".to_string(),
            input: Some(EntityInputReference {
                row_count: 4,
                content_hash: "blake3:input".to_string(),
            }),
            upstream_artifacts: vec![canon::entity::EntityArtifactReference {
                version: "canon_entity_prepare.v0".to_string(),
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
        summary: EntityDeterministicSummary::default(),
    }
}

fn profile_reference() -> EntityProfileReference {
    EntityProfileReference {
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
    }
}

fn block_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.block".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:block-strategy".to_string(),
    }
}

fn common_token_posting_index(surface_count: usize) -> EntityPostingIndex {
    let surfaces = (1..=surface_count)
        .map(|index| {
            EntityPostingSurface::new(format!("surf:{index:03}"))
                .with_exact_view("tenant_core", format!("bank branch {index:03}"))
                .with_tokens(["bank".to_string(), format!("unique-{index:03}")])
        })
        .collect::<Vec<_>>();
    EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 16,
        },
    )
    .expect("common-token posting index builds")
}

fn alias_budget_posting_index() -> EntityPostingIndex {
    EntityPostingIndex::build(
        &[
            EntityPostingSurface::new("surf:001")
                .with_exact_view("tenant_core", "tenant one")
                .with_tokens(["tenant".to_string(), "one".to_string()]),
            EntityPostingSurface::new("surf:002")
                .with_exact_view("tenant_core", "tenant two")
                .with_tokens(["tenant".to_string(), "two".to_string()]),
            EntityPostingSurface::new("surf:003")
                .with_exact_view("tenant_core", "tenant three")
                .with_tokens(["tenant".to_string(), "three".to_string()]),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("alias-budget posting index builds")
}

fn common_token_surface_ids(surface_count: usize) -> Vec<String> {
    (1..=surface_count)
        .map(|index| format!("surf:{index:03}"))
        .collect()
}

fn known_surface_ids() -> Vec<String> {
    vec!["surf:001".to_string(), "surf:002".to_string()]
}
