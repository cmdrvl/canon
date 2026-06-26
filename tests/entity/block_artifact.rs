#![forbid(unsafe_code)]

use canon::{
    RefusalCode,
    entity::{
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION,
        block::{
            AliasPatchMatchBlockOperator, AliasPatchPair, BlockCandidateBudgetConfig,
            BlockCandidateGenerationRequest, BlockCandidateOperator, ExactBucketBlockRequest,
            ExactBucketSurface, RareTokenOverlapBlockOperator, emit_exact_bucket_hyperedges,
            generate_block_candidates,
        },
        block_artifact::{
            BlockCandidateArtifactRequest, EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN,
            ExactBucketProfile, ExactBucketUpstream, build_block_candidate_artifact_contract,
            validate_block_candidate_artifact_contract,
        },
        contracts::{
            EntityArtifactHeader, EntityArtifactMetadata, EntityArtifactReference,
            EntityInputReference, EntityNamekitReference, EntityPatchNamespaces,
            EntityPatchSetReference, EntityProfileReference, EntityRegistrySnapshot,
            EntityStrategyReference,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[test]
#[allow(non_snake_case)]
fn EN_B001_block_artifact_records_hashes_payloads_and_stable_order() {
    let manifest = golden_manifest();
    let expected = &manifest["fixtures"]["EN-B001"];
    let posting_index = candidate_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![AliasPatchPair::new(
                    "surf:cmbs:002",
                    "surf:cmbs:001",
                    "patch:sears-alias",
                )],
            ),
        )],
    })
    .expect("candidate JSONL payload emits");
    let bucket = en_b001_bucket_assertions();

    let request = BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates.clone(),
        bucket_assertions: bucket.assertions.clone(),
        known_surface_ids: posting_index.surface_ids.clone(),
        diagnostics: candidates.diagnostics.clone(),
    };
    let artifact =
        build_block_candidate_artifact_contract(request.clone()).expect("block artifact");
    let repeated = build_block_candidate_artifact_contract(request).expect("repeat artifact");

    assert_eq!(artifact, repeated);
    assert_eq!(artifact.version, "canon_entity_block.v0");
    assert!(artifact.artifact_content_hash.starts_with("blake3:"));
    assert!(artifact.candidate_records_hash.starts_with("blake3:"));
    assert!(artifact.bucket_assertions_hash.starts_with("blake3:"));
    assert_eq!(
        artifact.metadata.artifact_content_hash,
        artifact.artifact_content_hash
    );
    assert_eq!(artifact.metadata.profile.id, "cmbs_tenant_label");
    assert_eq!(
        artifact.metadata.strategy.content_hash,
        "blake3:block-strategy"
    );
    assert_eq!(
        artifact.metadata.registry_snapshot.lookup_snapshot_hash,
        "blake3:registry"
    );
    assert!(artifact.upstream_artifacts.iter().any(|reference| {
        reference.version == CANON_ENTITY_PREPARE_VERSION
            && reference.content_hash == "blake3:prepare"
    }));
    assert!(artifact.upstream_artifacts.iter().any(|reference| {
        reference.version == CANON_ENTITY_INDEX_VERSION && reference.content_hash == "blake3:index"
    }));
    assert_eq!(artifact.summary.counts["candidate_pairs"], 1);
    assert_eq!(artifact.summary.counts["block_hits"], 1);
    assert_eq!(
        artifact.summary.counts["exact_bucket_count"],
        expected["expected_summary"]["exact_bucket_count"]
            .as_u64()
            .expect("fixture exact_bucket_count")
    );
    assert_eq!(
        artifact.summary.counts["exact_bucket_pair_expansion_count"],
        expected["expected_summary"]["exact_bucket_pair_expansion_count"]
            .as_u64()
            .expect("fixture exact_bucket_pair_expansion_count")
    );
    assert_eq!(
        artifact.summary.counts["bucket_assertion_records"],
        expected["expected_summary"]["bucket_assertion_records"]
            .as_u64()
            .expect("fixture bucket_assertion_records")
    );
    assert_eq!(
        artifact.summary.counts["candidate_artifact_bytes"],
        candidates.diagnostics.candidate_artifact_bytes
    );
    validate_block_candidate_artifact_contract(&artifact).expect("artifact contract validates");

    let candidate = &candidates.candidates[0];
    assert_eq!(candidate.left_surface_id, "surf:cmbs:001");
    assert_eq!(candidate.right_surface_id, "surf:cmbs:002");
    assert_eq!(candidate.block_hits[0].operator_id, "alias_patch_match");
    let assertion = &bucket.assertions[0];
    assert_eq!(
        assertion.row_count,
        expected["row_count"].as_u64().expect("fixture row_count")
    );
    assert_eq!(assertion.expanded_pair_count(), 0);
    assert_eq!(
        assertion.pair_expansion,
        EXACT_BUCKET_PAIR_EXPANSION_FORBIDDEN
    );
}

#[test]
fn entity_block_artifact_refuses_unknown_candidate_surface_refs() {
    let posting_index = candidate_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![AliasPatchPair::new(
                    "surf:cmbs:001",
                    "surf:cmbs:002",
                    "patch:sears-alias",
                )],
            ),
        )],
    })
    .expect("candidate payload emits");
    let mut candidate_records = candidates.candidates.clone();
    candidate_records[0].right_surface_id = "surf:cmbs:missing".to_string();

    let refusal = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids.clone(),
        diagnostics: candidates.diagnostics,
    })
    .expect_err("unknown candidate surface refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "unknown_surface_id");
    assert_eq!(refusal.detail["surface_id"], "surf:cmbs:missing");
}

#[test]
fn entity_block_artifact_refuses_stale_bucket_upstream_hashes() {
    let posting_index = candidate_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: Vec::new(),
    })
    .expect("empty candidate payload is valid");
    let mut bucket_assertions = en_b001_bucket_assertions().assertions;
    bucket_assertions[0].upstream.index_hash = "blake3:old-index".to_string();

    let refusal = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions,
        known_surface_ids: posting_index.surface_ids.clone(),
        diagnostics: candidates.diagnostics,
    })
    .expect_err("stale bucket upstream hash refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["reason"], "stale_exact_bucket_assertion");
    assert_eq!(refusal.detail["field"], "upstream.index_hash");
    assert_eq!(refusal.detail["expected"], "blake3:index");
    assert_eq!(refusal.detail["actual"], "blake3:old-index");
}

#[test]
fn entity_block_artifact_validator_refuses_self_hash_drift() {
    let posting_index = candidate_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![AliasPatchPair::new(
                    "surf:cmbs:001",
                    "surf:cmbs:002",
                    "patch:sears-alias",
                )],
            ),
        )],
    })
    .expect("candidate payload emits");
    let mut artifact = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids,
        diagnostics: candidates.diagnostics,
    })
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
fn entity_block_artifact_refuses_unstable_candidate_order() {
    let posting_index = candidate_posting_index();
    let mut candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![
                    AliasPatchPair::new("surf:cmbs:001", "surf:cmbs:002", "patch:one"),
                    AliasPatchPair::new("surf:cmbs:001", "surf:cmbs:003", "patch:two"),
                ],
            ),
        )],
    })
    .expect("candidate payload emits");
    candidates.candidates.reverse();

    let refusal = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids,
        diagnostics: candidates.diagnostics,
    })
    .expect_err("unstable candidate order refuses");

    assert_eq!(refusal.code, RefusalCode::EEntityArtifactContract);
    assert_eq!(refusal.detail["stage"], "block");
    assert_eq!(refusal.detail["reason"], "unstable_candidate_order");
    assert_eq!(refusal.detail["writes_performed"], json!(false));
}

#[test]
#[allow(non_snake_case)]
fn EN_B002_common_token_bucket_stays_bounded_in_artifact_summary() {
    let manifest = golden_manifest();
    let expected = &manifest["fixtures"]["EN-B002"];
    let posting_index = common_bank_posting_index(64);
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(1, 1, 1),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(0)
                .with_topk(25, 25)
                .with_max_posting_size(8),
        )],
    })
    .expect("common-token payload stays bounded");

    let artifact = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids,
        diagnostics: candidates.diagnostics,
    })
    .expect("bounded empty payload still emits summary");

    assert_eq!(
        artifact.summary.counts["candidate_pairs"],
        expected["expected_summary"]["candidate_pairs"]
            .as_u64()
            .expect("fixture candidate_pairs")
    );
    assert_eq!(
        artifact.summary.counts["large_buckets_suppressed"],
        expected["expected_summary"]["large_buckets_suppressed"]
            .as_u64()
            .expect("fixture large_buckets_suppressed")
    );
    assert_eq!(
        artifact.summary.counts["candidate_pairs_emitted"],
        expected["expected_summary"]["candidate_pairs_emitted"]
            .as_u64()
            .expect("fixture candidate_pairs_emitted")
    );
    validate_block_candidate_artifact_contract(&artifact).expect("artifact contract validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B003_sears_llc_support_candidate_is_emitted() {
    let manifest = golden_manifest();
    let expected = &manifest["fixtures"]["EN-B003"];
    let posting_index = candidate_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![BlockCandidateOperator::RareTokenOverlap(
            RareTokenOverlapBlockOperator::new("rare_token_overlap:tenant_tokens", "tenant_core")
                .with_min_idf_units(0)
                .with_topk(4, 4)
                .with_max_posting_size(10),
        )],
    })
    .expect("Sears alias support candidates emit");

    let support = candidate_pair(
        &candidates.candidates,
        expected["left_surface_id"]
            .as_str()
            .expect("fixture left surface"),
        expected["right_surface_id"]
            .as_str()
            .expect("fixture right surface"),
    );
    assert_eq!(support.version, "canon_entity_block.v0");
    assert!(support.block_hits.iter().any(|hit| {
        hit.operator_id
            == expected["expected_operator"]
                .as_str()
                .expect("fixture operator")
            && hit.score_units > 0
    }));

    let artifact = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids,
        diagnostics: candidates.diagnostics,
    })
    .expect("EN-B003 block artifact builds");

    assert!(artifact.summary.counts["candidate_pair_count"] >= 1);
    assert!(artifact.summary.counts["operator_hit_count"] >= 1);
    validate_block_candidate_artifact_contract(&artifact).expect("artifact contract validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B004_sears_auto_candidate_retains_relation_context() {
    let manifest = golden_manifest();
    let expected = &manifest["fixtures"]["EN-B004"];
    let posting_index = sears_auto_posting_index();
    let candidates = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(8, 64, 128),
        operators: vec![
            BlockCandidateOperator::RareTokenOverlap(
                RareTokenOverlapBlockOperator::new(
                    "rare_token_overlap:tenant_tokens",
                    "tenant_core",
                )
                .with_min_idf_units(0)
                .with_topk(4, 4)
                .with_max_posting_size(10),
            ),
            BlockCandidateOperator::AliasPatchMatch(AliasPatchMatchBlockOperator::new(
                "relation_hint:tenant_related_distinct",
                vec![AliasPatchPair::new(
                    "surf:cmbs:001",
                    "surf:cmbs:004",
                    "patch:sears-auto-related-distinct",
                )],
            )),
        ],
    })
    .expect("Sears Auto relation context candidates emit");

    let related = candidate_pair(
        &candidates.candidates,
        expected["left_surface_id"]
            .as_str()
            .expect("fixture left surface"),
        expected["right_surface_id"]
            .as_str()
            .expect("fixture right surface"),
    );
    let operators = related
        .block_hits
        .iter()
        .map(|hit| hit.operator_id.as_str())
        .collect::<Vec<_>>();
    assert!(
        operators.contains(
            &expected["expected_support_operator"]
                .as_str()
                .expect("fixture support operator")
        )
    );
    assert!(
        operators.contains(
            &expected["expected_relation_operator"]
                .as_str()
                .expect("fixture relation operator")
        )
    );

    let artifact = build_block_candidate_artifact_contract(BlockCandidateArtifactRequest {
        index: sample_index_header(),
        strategy: sample_block_strategy(),
        candidate_records_path: "block/candidates.jsonl".to_string(),
        candidate_records: candidates.candidates,
        bucket_assertions: Vec::new(),
        known_surface_ids: posting_index.surface_ids,
        diagnostics: candidates.diagnostics,
    })
    .expect("EN-B004 block artifact builds");

    assert_eq!(
        artifact.summary.counts["relation_hint_count"],
        expected["expected_summary"]["relation_hint_count"]
            .as_u64()
            .expect("fixture relation_hint_count")
    );
    validate_block_candidate_artifact_contract(&artifact).expect("artifact contract validates");
}

#[test]
#[allow(non_snake_case)]
fn EN_B005_candidate_budget_refuses_before_artifact_summary() {
    let manifest = golden_manifest();
    let expected = &manifest["fixtures"]["EN-B005"];
    let posting_index = candidate_posting_index();
    let refusal = generate_block_candidates(BlockCandidateGenerationRequest {
        profile_id: "cmbs_tenant_label".to_string(),
        posting_index: &posting_index,
        ngram_index: None,
        budget_config: BlockCandidateBudgetConfig::new(1, 16, 16),
        operators: vec![BlockCandidateOperator::AliasPatchMatch(
            AliasPatchMatchBlockOperator::new(
                "alias_patch_match",
                vec![
                    AliasPatchPair::new("surf:cmbs:001", "surf:cmbs:002", "patch:one"),
                    AliasPatchPair::new("surf:cmbs:001", "surf:cmbs:003", "patch:two"),
                ],
            ),
        )],
    })
    .expect_err("over-budget candidate generation refuses before artifact");

    assert_eq!(refusal.code, RefusalCode::EEntityCandidateBudget);
    assert_eq!(
        refusal.code.to_string(),
        expected["expected_refusal_code"]
            .as_str()
            .expect("fixture refusal code")
    );
    assert_eq!(
        refusal.detail["reason"],
        expected["expected_reason"]
            .as_str()
            .expect("fixture reason")
    );
    assert_eq!(
        refusal.detail["candidate_artifact_written"],
        json!(
            expected["candidate_artifact_written"]
                .as_bool()
                .expect("fixture artifact written")
        )
    );
}

fn golden_manifest() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/entity/block/blocking_golden_manifest.json"
    ))
    .expect("blocking golden manifest parses")
}

fn en_b001_bucket_assertions() -> canon::entity::block::ExactBucketBlockResult {
    emit_exact_bucket_hyperedges(ExactBucketBlockRequest {
        profile: sample_bucket_profile(),
        upstream: ExactBucketUpstream {
            prepare_hash: "blake3:prepare".to_string(),
            index_hash: "blake3:index".to_string(),
            strategy_hash: "blake3:block-strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
        },
        operator_id: "exact_view:tenant_core".to_string(),
        identity_view: "tenant_core".to_string(),
        placeholder_values: BTreeSet::from(["unknown".to_string(), "vacant".to_string()]),
        surfaces: vec![ExactBucketSurface::new(
            "surf:cmbs:003",
            "sears",
            8_000,
            934,
        )],
    })
    .expect("EN-B001 compact bucket assertion emits")
}

fn sample_bucket_profile() -> ExactBucketProfile {
    ExactBucketProfile {
        id: "cmbs_tenant_label".to_string(),
        version: "0.1.0".to_string(),
        identity_semantics: "canonical_display_label".to_string(),
        content_hash: "blake3:profile".to_string(),
    }
}

fn sample_index_header() -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: CANON_ENTITY_INDEX_VERSION.to_string(),
        metadata: sample_index_metadata(),
        summary: Default::default(),
    }
}

fn sample_index_metadata() -> EntityArtifactMetadata {
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
            row_count: 3,
            content_hash: "blake3:input".to_string(),
        }),
        upstream_artifacts: vec![EntityArtifactReference {
            version: CANON_ENTITY_PREPARE_VERSION.to_string(),
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
    }
}

fn sample_block_strategy() -> EntityStrategyReference {
    EntityStrategyReference {
        id: "cmbs_tenant_label.block".to_string(),
        version: "0.1.0".to_string(),
        content_hash: "blake3:block-strategy".to_string(),
    }
}

fn candidate_posting_index() -> EntityPostingIndex {
    EntityPostingIndex::build(
        &[
            posting_surface("surf:cmbs:001", "sears", ["sears"]),
            posting_surface("surf:cmbs:002", "sears llc", ["sears", "llc"]),
            posting_surface("surf:cmbs:003", "sears", ["sears"]),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("posting index builds")
}

fn common_bank_posting_index(surface_count: usize) -> EntityPostingIndex {
    let surfaces = (0..surface_count)
        .map(|index| {
            EntityPostingSurface::new(format!("surf:bank:{index:03}"))
                .with_exact_view("tenant_core", format!("bank branch {index:03}"))
                .with_tokens(["bank".to_string(), format!("unique-{index:03}")])
        })
        .collect::<Vec<_>>();
    EntityPostingIndex::build(
        &surfaces,
        EntityPostingBuildConfig {
            common_posting_limit: 8,
        },
    )
    .expect("common posting index builds")
}

fn sears_auto_posting_index() -> EntityPostingIndex {
    EntityPostingIndex::build(
        &[
            posting_surface("surf:cmbs:001", "sears", ["sears"]),
            posting_surface(
                "surf:cmbs:004",
                "sears auto center",
                ["sears", "auto", "center"],
            ),
            posting_surface("surf:cmbs:005", "kmart", ["kmart"]),
        ],
        EntityPostingBuildConfig {
            common_posting_limit: 10,
        },
    )
    .expect("Sears Auto posting index builds")
}

fn candidate_pair<'a>(
    candidates: &'a [canon::entity::block::BlockCandidateRecord],
    left_surface_id: &str,
    right_surface_id: &str,
) -> &'a canon::entity::block::BlockCandidateRecord {
    candidates
        .iter()
        .find(|candidate| {
            candidate.left_surface_id == left_surface_id
                && candidate.right_surface_id == right_surface_id
        })
        .expect("candidate pair exists")
}

fn posting_surface<const N: usize>(
    surface_id: &str,
    tenant_core: &str,
    tokens: [&str; N],
) -> EntityPostingSurface {
    EntityPostingSurface::new(surface_id)
        .with_exact_view("tenant_core", tenant_core)
        .with_tokens(tokens)
}
