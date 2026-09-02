#![forbid(unsafe_code)]

use canon::entity::evidence_ir as evidence;
use canon::entity::record_link::{
    self, ASSIGNMENT_ALIGNMENT_PATH, ASSIGNMENT_ALIGNMENT_VERSION, AssignmentAlignmentDecisionKind,
    AssignmentAlignmentPolicy, AssignmentCardinality, RecordLinkBlockingComponent,
    RecordLinkBlockingKey, RecordLinkBlockingPolicy, RecordLinkCandidateAbstentionReason,
    RecordLinkCandidateConfig, RecordLinkCandidateRequest, RecordLinkCandidateSet,
    RecordLinkEvidenceRequest, RecordLinkFeatureKind, RecordLinkFeaturePolicy,
    RecordLinkInputSource, RecordLinkLoadRequest, RecordLinkPairAccounting,
    RecordLinkSupportPolicy, RecordLinkSurfaceBindingInput, build_record_link_evidence,
    build_record_link_input_set, canonical_assignment_alignment_bytes,
    canonical_record_link_candidate_set_bytes, generate_record_link_candidates,
    load_record_link_inputs, validate_assignment_alignment_sidecar,
    validate_record_link_candidate_set, validate_record_link_candidate_set_for_inputs,
};
use canon::entity::source_mapping::{
    self, AssignmentArtifact, CapturePolicy, CellDispositionReason, MappedCell,
    MappedSourceArtifacts, MappingProvenance, ObservationArtifact, RecordLinkComparisonKind,
    RecordLinkComparisonMapping, RecordLinkComparisonPolicies, RecordLinkComparisonSource,
    RecordLinkComparisonView, RecordLinkInputBuildRequest, SourceLocator, TemporalContext,
    build_record_link_input_sidecar,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const HASH_A: &str = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn record_link_core_builds_stable_candidates_evidence_and_alignment() {
    let input_set = neutral_input_set(false);
    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir_all(temp_dir.path().join("left")).expect("left dir");
    std::fs::create_dir_all(temp_dir.path().join("right")).expect("right dir");
    for input in &input_set.inputs {
        std::fs::write(
            temp_dir.path().join(&input.path),
            serde_json::to_vec_pretty(&input.sidecar).expect("sidecar bytes"),
        )
        .expect("write sidecar");
    }
    let loaded = load_record_link_inputs(RecordLinkLoadRequest {
        workspace_root: temp_dir.path(),
        sidecar_paths: input_set
            .inputs
            .iter()
            .map(|input| input.path.clone().into())
            .collect(),
        expected_profile_id: Some("neutral:profile".to_string()),
        expected_profile_digest: Some(HASH_A.to_string()),
        expected_scope_id: Some("scope.synthetic".to_string()),
    })
    .expect("load sidecars");
    assert_eq!(loaded.content_hash, input_set.content_hash);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");

    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("candidate set");
    assert_eq!(
        candidate_set.version,
        record_link::RECORD_LINK_CANDIDATE_SET_VERSION
    );
    assert!(
        !canonical_record_link_candidate_set_bytes(&candidate_set)
            .expect("candidate bytes")
            .is_empty()
    );
    assert_eq!(candidate_set.candidates.len(), 2);
    assert!(
        candidate_set
            .candidates
            .iter()
            .all(|candidate| !candidate.hard_cannot_link),
        "ordinary mismatch must not become a hard veto without a policy"
    );
    validate_record_link_candidate_set(&candidate_set).expect("candidate set validates");

    let evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &candidate_set,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy {
            cardinality: AssignmentCardinality::ManyToMany,
            ..AssignmentAlignmentPolicy::default()
        },
    })
    .expect("evidence output");
    assert_eq!(evidence.bundle.version, evidence::CANON_EVIDENCE_VERSION);
    assert!(
        evidence
            .bundle
            .records
            .iter()
            .any(|record| record.kind == evidence::EvidenceKind::RecordLinkSupport)
    );
    assert!(
        evidence
            .bundle
            .records
            .iter()
            .all(|record| record.kind != evidence::EvidenceKind::AntiMergeVeto)
    );
    assert_eq!(evidence.alignment.version, ASSIGNMENT_ALIGNMENT_VERSION);
    assert_eq!(
        evidence.alignment.feature_policy_digest,
        candidate_set.feature_policy_digest
    );
    assert_eq!(
        evidence.alignment.record_link_evidence_path,
        record_link::RECORD_LINK_EVIDENCE_PATH
    );
    assert_eq!(
        evidence.alignment.summary["cannot_link_veto_count"], 0,
        "benign mismatch must not create a veto"
    );
    assert!(
        evidence
            .alignment
            .alignments
            .iter()
            .any(|record| record.decision == AssignmentAlignmentDecisionKind::Aligned)
    );
    assert!(
        evidence
            .alignment
            .alignments
            .iter()
            .all(|record| record.decision != AssignmentAlignmentDecisionKind::CannotLinkVeto)
    );

    let shuffled = neutral_input_set(true);
    let shuffled_surfaces = neutral_surfaces(&shuffled);
    let shuffled_surface_index =
        record_link::bind_record_link_surfaces(&shuffled, &shuffled_surfaces, "block")
            .expect("shuffled surface bindings");
    let shuffled_candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &shuffled,
        surface_index: &shuffled_surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("shuffled candidate set");
    assert_eq!(
        candidate_set.content_hash,
        shuffled_candidate_set.content_hash
    );
}

#[test]
fn record_link_core_requires_declared_policy_for_hard_conflict() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:amount",
            RecordLinkFeatureKind::Numeric,
            RecordLinkSupportPolicy::NumericTolerance {
                tolerance_scaled_units: 0,
            },
            1_000,
            true,
        ),
    );
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("candidate set");
    assert!(
        candidate_set
            .candidates
            .iter()
            .any(|candidate| candidate.hard_cannot_link),
        "declared hard conflict policy must produce a hard veto"
    );
    let evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &candidate_set,
        feature_policies: &amount_hard_conflict_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect("evidence output");
    assert!(
        evidence
            .bundle
            .records
            .iter()
            .any(|record| record.kind == evidence::EvidenceKind::AntiMergeVeto)
    );
    assert_eq!(evidence.alignment.summary["cannot_link_veto_count"], 1);
}

#[test]
fn record_link_core_supports_numeric_tolerance_and_near_dates() {
    let input_set = near_input_set();
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:amount",
            RecordLinkFeatureKind::Numeric,
            RecordLinkSupportPolicy::NumericTolerance {
                tolerance_scaled_units: 2,
            },
            700,
            false,
        ),
    );
    feature_policies.insert(
        "cmp:as_of".to_string(),
        feature_policy(
            "cmp:as_of",
            RecordLinkFeatureKind::Date,
            RecordLinkSupportPolicy::DateNear { max_days: 2 },
            300,
            false,
        ),
    );
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("candidate set");
    let candidate = candidate_set
        .candidates
        .first()
        .expect("candidate from tolerant policies");
    let feature_ids = candidate
        .support_features
        .iter()
        .map(|feature| feature.feature_id.as_str())
        .collect::<Vec<_>>();
    assert!(feature_ids.contains(&"cmp:amount"));
    assert!(feature_ids.contains(&"cmp:as_of"));
    assert!(!feature_ids.contains(&"cmp:category"));
    assert_eq!(candidate.score_hint_units, 1_000);
}

#[test]
fn record_link_core_evidence_hashes_actual_policy_for_same_outcome() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let standard_policies = standard_feature_policies();
    let alternate_policies = alternate_same_outcome_feature_policies();
    let standard_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            feature_policies: standard_policies.clone(),
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("standard candidates");
    let alternate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            feature_policies: alternate_policies.clone(),
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("alternate candidates");
    assert_ne!(
        standard_set.feature_policy_digest,
        alternate_set.feature_policy_digest
    );
    assert_eq!(standard_set.candidates, alternate_set.candidates);

    let standard_evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &standard_set,
        feature_policies: &standard_policies,
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy {
            cardinality: AssignmentCardinality::ManyToMany,
            ..AssignmentAlignmentPolicy::default()
        },
    })
    .expect("standard evidence");
    let alternate_evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &alternate_set,
        feature_policies: &alternate_policies,
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy {
            cardinality: AssignmentCardinality::ManyToMany,
            ..AssignmentAlignmentPolicy::default()
        },
    })
    .expect("alternate evidence");
    let standard_policy_hashes = standard_evidence
        .bundle
        .records
        .iter()
        .map(|record| record.policy.content_hash.as_str())
        .collect::<Vec<_>>();
    let alternate_policy_hashes = alternate_evidence
        .bundle
        .records
        .iter()
        .map(|record| record.policy.content_hash.as_str())
        .collect::<Vec<_>>();
    assert_ne!(standard_policy_hashes, alternate_policy_hashes);
    assert_ne!(
        standard_evidence.alignment.feature_policy_digest,
        alternate_evidence.alignment.feature_policy_digest
    );
}

#[test]
fn record_link_core_binds_profile_digest_across_inputs_and_load() {
    let input_set = neutral_input_set(false);
    assert_eq!(input_set.profile_digest, HASH_A);
    assert!(
        input_set
            .refs
            .iter()
            .all(|input_ref| input_ref.profile_digest == HASH_A)
    );

    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir_all(temp_dir.path().join("left")).expect("left dir");
    std::fs::create_dir_all(temp_dir.path().join("right")).expect("right dir");
    for input in &input_set.inputs {
        std::fs::write(
            temp_dir.path().join(&input.path),
            serde_json::to_vec_pretty(&input.sidecar).expect("sidecar bytes"),
        )
        .expect("write sidecar");
    }
    let error = load_record_link_inputs(RecordLinkLoadRequest {
        workspace_root: temp_dir.path(),
        sidecar_paths: input_set
            .inputs
            .iter()
            .map(|input| input.path.clone().into())
            .collect(),
        expected_profile_id: Some("neutral:profile".to_string()),
        expected_profile_digest: Some(HASH_C.to_string()),
        expected_scope_id: Some("scope.synthetic".to_string()),
    })
    .expect_err("stale loaded profile digest must refuse");
    assert_eq!(error.reason, "profile_digest_mismatch");

    let left = sidecar(
        "left",
        HASH_A,
        vec![mapped_row(
            "left",
            "profile-left",
            "a1",
            100,
            2,
            "2026-01-01",
            "gold",
        )],
    );
    let right = sidecar_with_profile_digest(
        "right",
        HASH_B,
        HASH_C,
        vec![mapped_row(
            "right",
            "profile-right",
            "b1",
            100,
            2,
            "2026-01-01",
            "gold",
        )],
    );
    let error = build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: left,
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: right,
        },
    ])
    .expect_err("mixed profile digests must refuse");
    assert_eq!(error.reason, "mixed_profile_digest");
}

#[test]
fn record_link_core_does_not_score_undeclared_features() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("candidate set");
    assert!(
        candidate_set.candidates.is_empty(),
        "undeclared matching feature IDs must not mint support"
    );
    assert!(
        candidate_set
            .abstentions
            .iter()
            .any(|abstention| abstention.reason
                == RecordLinkCandidateAbstentionReason::UnconfiguredFeature)
    );
}

#[test]
fn record_link_core_rejects_self_rehashed_cross_bound_candidates() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("candidate set");
    validate_record_link_candidate_set_for_inputs(
        &input_set,
        &candidate_set,
        &standard_feature_policies(),
        None,
    )
    .expect("candidate set binds inputs");

    let mut tampered_refs = candidate_set.clone();
    tampered_refs.input_refs[0].content_hash = HASH_C.to_string();
    reseal_candidate_set(&mut tampered_refs);
    validate_record_link_candidate_set(&tampered_refs).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_refs,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("input ref mismatch must refuse before evidence");
    assert_eq!(error.reason, "candidate_input_ref_mismatch");

    let mut tampered_endpoint = candidate_set.clone();
    tampered_endpoint.candidates[0].left.sidecar_hash = HASH_C.to_string();
    reseal_candidate_set(&mut tampered_endpoint);
    validate_record_link_candidate_set(&tampered_endpoint).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_endpoint,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("endpoint hash mismatch must refuse before evidence");
    assert_eq!(error.reason, "candidate_endpoint_hash_mismatch");
}

#[test]
fn record_link_core_rejects_self_rehashed_policy_and_decision_drift() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("candidate set");

    let mut tampered_policy = candidate_set.clone();
    tampered_policy.feature_policy_digest = HASH_C.to_string();
    reseal_candidate_set(&mut tampered_policy);
    validate_record_link_candidate_set(&tampered_policy).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_policy,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("policy digest drift must refuse");
    assert_eq!(error.reason, "candidate_feature_policy_digest_mismatch");

    let mut tampered_score = candidate_set.clone();
    tampered_score.candidates[0].score_hint_units += 1;
    reseal_candidate_set(&mut tampered_score);
    validate_record_link_candidate_set(&tampered_score).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_score,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("score drift must refuse");
    assert_eq!(error.reason, "candidate_score_mismatch");

    let mut tampered_support = candidate_set.clone();
    let removed = tampered_support.candidates[0]
        .support_features
        .pop()
        .expect("support feature");
    tampered_support.candidates[0]
        .missing_feature_ids
        .push(removed.feature_id);
    tampered_support.candidates[0].missing_feature_ids.sort();
    reseal_candidate_set(&mut tampered_support);
    validate_record_link_candidate_set(&tampered_support).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_support,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("support-to-abstention drift must refuse");
    assert_eq!(error.reason, "candidate_support_feature_mismatch");

    let hard_candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 4,
            require_unique_best_per_record: false,
            feature_policies: amount_hard_conflict_policies(),
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("hard candidate set");
    let mut tampered_hard = hard_candidate_set.clone();
    let hard = tampered_hard
        .candidates
        .iter_mut()
        .find(|candidate| candidate.hard_cannot_link)
        .expect("hard candidate");
    hard.hard_cannot_link = false;
    tampered_hard
        .summary
        .insert("hard_cannot_link_count".to_string(), 0);
    reseal_candidate_set(&mut tampered_hard);
    validate_record_link_candidate_set(&tampered_hard).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_hard,
        feature_policies: &amount_hard_conflict_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("hard-conflict drift must refuse");
    assert_eq!(error.reason, "candidate_hard_veto_mismatch");
}

#[test]
fn record_link_core_preserves_hard_veto_under_pruning() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_candidates_per_record: 1,
            require_unique_best_per_record: true,
            feature_policies: amount_hard_conflict_policies(),
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect("candidate set");
    assert!(
        candidate_set
            .candidates
            .iter()
            .any(|candidate| candidate.hard_cannot_link),
        "hard cannot-link must survive pruning"
    );
    let evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &candidate_set,
        feature_policies: &amount_hard_conflict_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect("evidence output");
    assert!(
        evidence
            .bundle
            .records
            .iter()
            .any(|record| record.kind == evidence::EvidenceKind::AntiMergeVeto)
    );
}

#[test]
fn record_link_core_refuses_policy_date_and_score_invariants() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");

    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:other",
            RecordLinkFeatureKind::Numeric,
            RecordLinkSupportPolicy::Exact,
            1_000,
            false,
        ),
    );
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect_err("map key must match feature_id");
    assert_eq!(error.reason, "feature_policy_key_mismatch");

    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:as_of".to_string(),
        feature_policy(
            "cmp:as_of",
            RecordLinkFeatureKind::Date,
            RecordLinkSupportPolicy::NumericTolerance {
                tolerance_scaled_units: 1,
            },
            1_000,
            false,
        ),
    );
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect_err("support policy must match feature kind");
    assert_eq!(error.reason, "feature_policy_support_mismatch");

    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:amount",
            RecordLinkFeatureKind::Date,
            RecordLinkSupportPolicy::Exact,
            1_000,
            false,
        ),
    );
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect_err("policy kind must match observed feature kind");
    assert_eq!(error.reason, "feature_policy_kind_mismatch");

    let near = near_input_set();
    let near_surfaces = neutral_surfaces(&near);
    let near_surface_index = record_link::bind_record_link_surfaces(&near, &near_surfaces, "block")
        .expect("near surface bindings");
    let mut feature_policies = BTreeMap::new();
    feature_policies.insert(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:amount",
            RecordLinkFeatureKind::Numeric,
            RecordLinkSupportPolicy::NumericTolerance {
                tolerance_scaled_units: 2,
            },
            u64::MAX,
            false,
        ),
    );
    feature_policies.insert(
        "cmp:as_of".to_string(),
        feature_policy(
            "cmp:as_of",
            RecordLinkFeatureKind::Date,
            RecordLinkSupportPolicy::DateNear { max_days: 2 },
            1,
            false,
        ),
    );
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &near,
        surface_index: &near_surface_index,
        config: RecordLinkCandidateConfig {
            feature_policies,
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect_err("support score overflow must fail closed");
    assert_eq!(error.reason, "candidate_score_overflow");

    let mut invalid_date = sidecar(
        "right",
        HASH_C,
        vec![mapped_row(
            "right",
            "invalid-date",
            "b1",
            100,
            2,
            "2026-01-01",
            "gold",
        )],
    );
    for record in &mut invalid_date.records {
        for view in &mut record.comparison_views {
            if let RecordLinkComparisonView::Date { value, .. } = view {
                *value = "2026-02-31".to_string();
            }
        }
    }
    reseal_record_link_input(&mut invalid_date);
    let error = build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: input_set.inputs[0].sidecar.clone(),
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: invalid_date,
        },
    ])
    .expect_err("impossible calendar dates must refuse");
    assert_eq!(error.reason, "invalid_sidecar");
}

#[test]
fn record_link_core_abstains_on_tie_duplicate_missing_and_scale_mismatch() {
    let input_set = tie_input_set();
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(1, true),
    })
    .expect("candidate set");
    let reasons = candidate_set
        .abstentions
        .iter()
        .map(|abstention| abstention.reason)
        .collect::<Vec<_>>();
    assert!(
        reasons.contains(&RecordLinkCandidateAbstentionReason::DuplicateBest)
            || reasons.contains(&RecordLinkCandidateAbstentionReason::Tie),
        "equal best candidates must not silently align"
    );
    assert!(
        reasons.contains(&RecordLinkCandidateAbstentionReason::ScaleMismatch),
        "unit/scale mismatch must be explicit"
    );
    assert!(
        reasons.contains(&RecordLinkCandidateAbstentionReason::MissingComparison),
        "missing comparison must be explicit"
    );
}

#[test]
fn record_link_core_refuses_assignment_or_record_id_surface_authority() {
    let input_set = neutral_input_set(false);
    let left_record = input_set.inputs[0]
        .sidecar
        .records
        .first()
        .expect("left record");
    let assignment_id = left_record
        .assignment_ref
        .as_ref()
        .expect("assignment")
        .assignment_id
        .clone();
    let surfaces = vec![
        RecordLinkSurfaceBindingInput {
            source_id: input_set.inputs[0].sidecar.source_id.clone(),
            surface_id: "surface.assignment-collision".to_string(),
            source_row_ids: vec![assignment_id],
        },
        RecordLinkSurfaceBindingInput {
            source_id: input_set.inputs[0].sidecar.source_id.clone(),
            surface_id: "surface.record-id-collision".to_string(),
            source_row_ids: vec![left_record.record_id.clone()],
        },
    ];
    let error = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect_err("assignment/record ids must not bind issuer surfaces");
    assert_eq!(error.reason, "missing_surface_binding");
}

#[test]
fn record_link_core_enforces_pair_comparison_budget_before_emission() {
    let input_set = incomparable_input_set();
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: RecordLinkCandidateConfig {
            max_pair_comparisons: 1,
            require_unique_best_per_record: false,
            feature_policies: standard_feature_policies(),
            ..RecordLinkCandidateConfig::default()
        },
    })
    .expect_err("pair-comparison budget must fail before unbounded cartesian work");
    assert_eq!(error.reason, "pair_comparison_budget_exceeded");
}

#[test]
fn record_link_core_structured_blocking_admits_before_pair_scoring_budget() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");

    let mut unblocked_config = standard_candidate_config(4, false);
    unblocked_config.max_pair_comparisons = 1;
    let unblocked = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: unblocked_config,
    })
    .expect_err("unblocked all-pairs traversal must hit the tight comparison budget");
    assert_eq!(unblocked.reason, "pair_comparison_budget_exceeded");

    let blocking = composite_blocking_policy();
    let mut blocked_config = standard_candidate_config(4, false);
    blocked_config.max_pair_comparisons = 1;
    blocked_config.blocking_policy = Some(blocking.clone());
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: blocked_config,
    })
    .expect("structured blocking should avoid the nonmatching pair before scoring");
    assert_eq!(candidate_set.candidates.len(), 1);
    assert_eq!(candidate_set.blocking_policy, Some(blocking.clone()));
    assert!(!candidate_set.blocking_policy_digest.is_empty());
    assert_eq!(
        candidate_set.pair_accounting,
        RecordLinkPairAccounting {
            cross_source_pair_count: 2,
            admitted_pair_count: 1,
            suppressed_pair_count: 1,
            scored_pair_count: 1,
            blocking_policy_miss_count: 1,
            comparison_abstention_count: 0,
            ranking_abstention_count: 0,
        }
    );
    assert_eq!(candidate_set.summary["suppressed_pair_count"], 1);
    assert_eq!(candidate_set.summary["blocking_policy_miss_count"], 1);
    validate_record_link_candidate_set_for_inputs(
        &input_set,
        &candidate_set,
        &standard_feature_policies(),
        Some(&blocking),
    )
    .expect("blocking policy binds to candidate set");

    let shuffled = neutral_input_set(true);
    let shuffled_surfaces = neutral_surfaces(&shuffled);
    let shuffled_surface_index =
        record_link::bind_record_link_surfaces(&shuffled, &shuffled_surfaces, "block")
            .expect("shuffled surface bindings");
    let mut shuffled_config = standard_candidate_config(4, false);
    shuffled_config.max_pair_comparisons = 1;
    shuffled_config.blocking_policy = Some(blocking);
    let shuffled_candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &shuffled,
        surface_index: &shuffled_surface_index,
        config: shuffled_config,
    })
    .expect("shuffled candidate set");
    assert_eq!(
        canonical_record_link_candidate_set_bytes(&candidate_set).expect("candidate bytes"),
        canonical_record_link_candidate_set_bytes(&shuffled_candidate_set)
            .expect("shuffled candidate bytes")
    );
}

#[test]
fn record_link_core_rejects_self_rehashed_blocking_policy_and_accounting_drift() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let blocking = composite_blocking_policy();
    let mut config = standard_candidate_config(4, false);
    config.blocking_policy = Some(blocking.clone());
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config,
    })
    .expect("candidate set");

    let alternate_blocking = categorical_blocking_policy();
    let mut alternate_config = standard_candidate_config(4, false);
    alternate_config.blocking_policy = Some(alternate_blocking.clone());
    let alternate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: alternate_config,
    })
    .expect("alternate candidate set");

    let mut tampered_policy = candidate_set.clone();
    tampered_policy.blocking_policy = Some(alternate_blocking);
    tampered_policy.blocking_policy_digest = alternate_set.blocking_policy_digest;
    reseal_candidate_set(&mut tampered_policy);
    validate_record_link_candidate_set(&tampered_policy).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_policy,
        feature_policies: &standard_feature_policies(),
        blocking_policy: Some(blocking.clone()),
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("strategy blocking policy drift must refuse before evidence");
    assert_eq!(error.reason, "candidate_blocking_policy_digest_mismatch");

    let mut tampered_accounting = candidate_set.clone();
    tampered_accounting.pair_accounting = RecordLinkPairAccounting {
        cross_source_pair_count: 2,
        admitted_pair_count: 2,
        suppressed_pair_count: 0,
        scored_pair_count: 2,
        blocking_policy_miss_count: 0,
        comparison_abstention_count: 0,
        ranking_abstention_count: 0,
    };
    tampered_accounting
        .summary
        .insert("admitted_pair_count".to_string(), 2);
    tampered_accounting
        .summary
        .insert("suppressed_pair_count".to_string(), 0);
    tampered_accounting
        .summary
        .insert("scored_pair_count".to_string(), 2);
    tampered_accounting
        .summary
        .insert("blocking_policy_miss_count".to_string(), 0);
    reseal_candidate_set(&mut tampered_accounting);
    validate_record_link_candidate_set(&tampered_accounting).expect("self hash still validates");
    let error = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &tampered_accounting,
        feature_policies: &standard_feature_policies(),
        blocking_policy: Some(blocking),
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect_err("self-consistent false pair accounting must refuse");
    assert_eq!(error.reason, "candidate_pair_accounting_mismatch");
}

#[test]
fn record_link_core_refuses_malformed_structured_blocking_policy() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");

    let mut missing_units = standard_candidate_config(4, false);
    let mut policy = composite_blocking_policy();
    if let RecordLinkBlockingComponent::FixedDecimalBucket { units, .. } =
        &mut policy.keys[0].components[0]
    {
        units.clear();
    }
    missing_units.blocking_policy = Some(policy);
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: missing_units,
    })
    .expect_err("numeric buckets must declare units explicitly");
    assert_eq!(error.reason, "blocking_component_empty_units");

    let mut kind_mismatch = standard_candidate_config(4, false);
    kind_mismatch.blocking_policy = Some(RecordLinkBlockingPolicy {
        policy_id: "neutral:blocking".to_string(),
        policy_version: "1".to_string(),
        keys: vec![RecordLinkBlockingKey {
            key_id: "wrong-kind".to_string(),
            components: vec![RecordLinkBlockingComponent::FixedDecimalBucket {
                feature_id: "cmp:category".to_string(),
                units: "basis_points".to_string(),
                scale: 2,
                bucket_width_scaled_units: 1,
            }],
        }],
    });
    let error = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: kind_mismatch,
    })
    .expect_err("blocking components must match observed typed feature values");
    assert_eq!(error.reason, "blocking_component_kind_mismatch");
}

#[test]
fn record_link_core_scopes_duplicate_local_ids_by_source() {
    let input_set = neutral_input_set(false);
    let left = &input_set.inputs[0].sidecar.records[0];
    let right = &input_set.inputs[1].sidecar.records[0];
    assert_eq!(
        left.source_ref.source_object_id,
        right.source_ref.source_object_id
    );

    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("source-scoped local ids must bind independently");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("candidate set");
    assert!(
        candidate_set.candidates.iter().any(|candidate| {
            candidate.left.source_id != candidate.right.source_id
                && candidate.left.observation_id != candidate.right.observation_id
        }),
        "duplicate local object ids across sources must not collide"
    );
}

#[test]
fn assignment_alignment_self_hash_refuses_tampering() {
    let input_set = neutral_input_set(false);
    let surfaces = neutral_surfaces(&input_set);
    let surface_index = record_link::bind_record_link_surfaces(&input_set, &surfaces, "block")
        .expect("surface bindings");
    let candidate_set = generate_record_link_candidates(RecordLinkCandidateRequest {
        input_set: &input_set,
        surface_index: &surface_index,
        config: standard_candidate_config(4, false),
    })
    .expect("candidate set");
    let mut evidence = build_record_link_evidence(RecordLinkEvidenceRequest {
        input_set: &input_set,
        candidate_set: &candidate_set,
        feature_policies: &standard_feature_policies(),
        blocking_policy: None,
        policy: AssignmentAlignmentPolicy::default(),
    })
    .expect("evidence output");
    canonical_assignment_alignment_bytes(&evidence.alignment).expect("canonical alignment bytes");
    evidence.alignment.record_link_evidence_path = ASSIGNMENT_ALIGNMENT_PATH.to_string();
    let error = validate_assignment_alignment_sidecar(&evidence.alignment)
        .expect_err("tampered path must refuse");
    assert_eq!(error.reason, "wrong_evidence_path");
}

fn neutral_input_set(shuffle_bundles: bool) -> record_link::RecordLinkInputSet {
    let mut left_bundles = vec![mapped_row(
        "left",
        "alpha",
        "a1",
        100,
        2,
        "2026-01-01",
        "gold",
    )];
    let mut right_bundles = vec![
        mapped_row("right", "alpha", "b1", 100, 2, "2026-01-01", "gold"),
        mapped_row("right", "blocked", "b2", 999, 2, "2026-01-01", "gold"),
    ];
    if shuffle_bundles {
        left_bundles.reverse();
        right_bundles.reverse();
    }
    let left = sidecar("left", HASH_A, left_bundles);
    let right = sidecar("right", HASH_B, right_bundles);
    build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: left,
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: right,
        },
    ])
    .expect("input set")
}

fn near_input_set() -> record_link::RecordLinkInputSet {
    let left = sidecar(
        "left",
        HASH_A,
        vec![mapped_row(
            "left",
            "near",
            "a1",
            100,
            2,
            "2026-01-01",
            "gold",
        )],
    );
    let right = sidecar(
        "right",
        HASH_B,
        vec![mapped_row(
            "right",
            "near",
            "b1",
            101,
            2,
            "2026-01-03",
            "gold",
        )],
    );
    build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: left,
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: right,
        },
    ])
    .expect("input set")
}

fn tie_input_set() -> record_link::RecordLinkInputSet {
    let left = sidecar(
        "left",
        HASH_A,
        vec![
            mapped_row("left", "anchor", "a1", 100, 2, "2026-01-01", "gold"),
            mapped_row("left", "scale", "a2", 100, 2, "2026-01-01", "gold"),
            mapped_row("left", "missing", "a3", 100, 2, "2026-01-01", "gold"),
        ],
    );
    let right_scale = mapped_row("right", "scale", "b3", 100, 2, "2026-01-01", "gold");
    let mut right_missing = mapped_row("right", "missing", "b4", 100, 2, "2026-01-01", "gold");
    if let Some(observation) = right_missing.observations.first_mut() {
        observation.context.remove("category");
    }
    let mut right = sidecar(
        "right",
        HASH_C,
        vec![
            mapped_row("right", "match-1", "b1", 100, 2, "2026-01-01", "gold"),
            mapped_row("right", "match-2", "b2", 100, 2, "2026-01-01", "gold"),
            right_scale,
            right_missing,
        ],
    );
    for record in &mut right.records {
        if record.source_ref.source_object_id == "scale" {
            for view in &mut record.comparison_views {
                if let RecordLinkComparisonView::Numeric {
                    feature_id, scale, ..
                } = view
                    && feature_id == "cmp:amount"
                {
                    *scale = 3;
                }
            }
        }
    }
    reseal_record_link_input(&mut right);
    build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: left,
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: right,
        },
    ])
    .expect("input set")
}

fn incomparable_input_set() -> record_link::RecordLinkInputSet {
    let left = sidecar(
        "left",
        HASH_A,
        vec![mapped_row(
            "left",
            "incomparable-left",
            "a1",
            100,
            2,
            "2026-01-01",
            "gold",
        )],
    );
    let mut right = sidecar(
        "right",
        HASH_C,
        vec![
            mapped_row(
                "right",
                "incomparable-right-1",
                "b1",
                100,
                2,
                "2026-01-01",
                "gold",
            ),
            mapped_row(
                "right",
                "incomparable-right-2",
                "b2",
                100,
                2,
                "2026-01-01",
                "gold",
            ),
        ],
    );
    for record in &mut right.records {
        record.comparison_views = vec![
            RecordLinkComparisonView::Numeric {
                feature_id: "cmp:as_of".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                source_path: "as_of".to_string(),
                units: "days".to_string(),
                scaled_value: 1,
                scale: 0,
            },
            RecordLinkComparisonView::Numeric {
                feature_id: "cmp:category".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                source_path: "category".to_string(),
                units: "category_code".to_string(),
                scaled_value: 1,
                scale: 0,
            },
            RecordLinkComparisonView::Date {
                feature_id: "cmp:amount".to_string(),
                source: RecordLinkComparisonSource::ObservationContext,
                source_path: "amount".to_string(),
                value: "2026-02-01".to_string(),
            },
        ];
    }
    reseal_record_link_input(&mut right);
    build_record_link_input_set(vec![
        RecordLinkInputSource {
            path: "left/record_link_input.json".to_string(),
            sidecar: left,
        },
        RecordLinkInputSource {
            path: "right/record_link_input.json".to_string(),
            sidecar: right,
        },
    ])
    .expect("input set")
}

fn sidecar(
    source_id: &str,
    input_digest: &str,
    bundles: Vec<MappedSourceArtifacts>,
) -> source_mapping::RecordLinkInputSidecar {
    sidecar_with_profile_digest(source_id, input_digest, HASH_A, bundles)
}

fn sidecar_with_profile_digest(
    source_id: &str,
    input_digest: &str,
    profile_digest: &str,
    bundles: Vec<MappedSourceArtifacts>,
) -> source_mapping::RecordLinkInputSidecar {
    build_record_link_input_sidecar(
        &RecordLinkInputBuildRequest {
            source_id: source_id.to_string(),
            scope_id: "scope.synthetic".to_string(),
            profile_id: "neutral:profile".to_string(),
            profile_digest: profile_digest.to_string(),
            input_digest: input_digest.to_string(),
            source_mapping_digest: HASH_B.to_string(),
            subject_observation_mapping_id: "obs:subject".to_string(),
            assignment_mapping_ids: vec!["assign:primary".to_string()],
            missing_assignment_policy: CapturePolicy::Quarantine,
            comparison_mappings: vec![
                RecordLinkComparisonMapping {
                    feature_id: "cmp:amount".to_string(),
                    source: RecordLinkComparisonSource::ObservationContext,
                    path: "amount".to_string(),
                    value_kind: RecordLinkComparisonKind::Numeric,
                    units: Some("usd".to_string()),
                    scale: Some(2),
                    policies: RecordLinkComparisonPolicies::default(),
                },
                RecordLinkComparisonMapping {
                    feature_id: "cmp:as_of".to_string(),
                    source: RecordLinkComparisonSource::ObservationContext,
                    path: "as_of".to_string(),
                    value_kind: RecordLinkComparisonKind::Date,
                    units: None,
                    scale: None,
                    policies: RecordLinkComparisonPolicies::default(),
                },
                RecordLinkComparisonMapping {
                    feature_id: "cmp:category".to_string(),
                    source: RecordLinkComparisonSource::ObservationContext,
                    path: "category".to_string(),
                    value_kind: RecordLinkComparisonKind::Categorical,
                    units: None,
                    scale: None,
                    policies: RecordLinkComparisonPolicies {
                        missing: CapturePolicy::Quarantine,
                        ..RecordLinkComparisonPolicies::default()
                    },
                },
            ],
            duplicate_record_policy: CapturePolicy::Reject,
        },
        &bundles,
    )
    .expect("record-link input sidecar")
}

fn reseal_record_link_input(sidecar: &mut source_mapping::RecordLinkInputSidecar) {
    sidecar.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(sidecar).expect("sidecar hash bytes");
    sidecar.artifact_content_hash = format!("blake3:{}", blake3::hash(&bytes).to_hex());
}

fn mapped_row(
    source_id: &str,
    object_id: &str,
    assignment_id: &str,
    amount: i64,
    scale: u32,
    as_of: &str,
    category: &str,
) -> MappedSourceArtifacts {
    let locator = SourceLocator {
        source_system: source_id.to_string(),
        locator: format!("{source_id}:{object_id}"),
        fragment: None,
    };
    let raw_fields = BTreeMap::from([
        ("amount".to_string(), json!(amount)),
        ("as_of".to_string(), json!(as_of)),
        ("category".to_string(), json!(category)),
    ]);
    let provenance = MappingProvenance {
        profile_id: "neutral:profile".to_string(),
        mapping_digest: HASH_B.to_string(),
        source_locator: locator.clone(),
        raw_fields,
    };
    MappedSourceArtifacts {
        mapping_digest: HASH_B.to_string(),
        profile_id: "neutral:profile".to_string(),
        object_id: Some(object_id.to_string()),
        source_locator: Some(locator),
        temporal: TemporalContext::default(),
        observations: vec![ObservationArtifact {
            observation_id: format!("obs.{source_id}.{object_id}"),
            mapping_id: "obs:subject".to_string(),
            object_id: object_id.to_string(),
            subject_type_id: "type:subject".to_string(),
            surface: MappedCell {
                path: "name".to_string(),
                value: object_id.to_string(),
            },
            anchors: Vec::new(),
            context: BTreeMap::from([
                ("amount".to_string(), json!(format_scaled(amount, scale))),
                ("as_of".to_string(), json!(as_of)),
                ("category".to_string(), json!(category)),
            ]),
            temporal: TemporalContext::default(),
            provenance: provenance.clone(),
        }],
        assignments: vec![AssignmentArtifact {
            assignment_id: assignment_id.to_string(),
            mapping_id: "assign:primary".to_string(),
            subject_object_id: object_id.to_string(),
            subject_type_id: "type:subject".to_string(),
            role_id: "role:owner".to_string(),
            assignee_type_id: "type:assignee".to_string(),
            assignee_surface: MappedCell {
                path: "assignee".to_string(),
                value: assignment_id.to_string(),
            },
            assignee_anchors: Vec::new(),
            context: BTreeMap::new(),
            temporal: TemporalContext::default(),
            provenance,
        }],
        relationships: Vec::new(),
        preserved_cells: vec![source_mapping::PreservedCell {
            reason: CellDispositionReason::UnknownField,
            path: "unused".to_string(),
            value: Value::String("preserved".to_string()),
        }],
        quarantined_cells: Vec::new(),
    }
}

fn neutral_surfaces(
    input_set: &record_link::RecordLinkInputSet,
) -> Vec<RecordLinkSurfaceBindingInput> {
    input_set
        .inputs
        .iter()
        .flat_map(|input| {
            input
                .sidecar
                .records
                .iter()
                .map(|record| RecordLinkSurfaceBindingInput {
                    source_id: input.sidecar.source_id.clone(),
                    surface_id: format!("surface.{}", record.record_id),
                    source_row_ids: vec![
                        record.source_ref.source_object_id.clone(),
                        record.source_ref.source_locator.locator.clone(),
                        record.subject_observation_ref.observation_id.clone(),
                    ],
                })
        })
        .collect()
}

fn standard_candidate_config(
    max_candidates_per_record: usize,
    require_unique_best_per_record: bool,
) -> RecordLinkCandidateConfig {
    RecordLinkCandidateConfig {
        max_candidates_per_record,
        require_unique_best_per_record,
        feature_policies: standard_feature_policies(),
        ..RecordLinkCandidateConfig::default()
    }
}

fn composite_blocking_policy() -> RecordLinkBlockingPolicy {
    RecordLinkBlockingPolicy {
        policy_id: "neutral:blocking".to_string(),
        policy_version: "1".to_string(),
        keys: vec![RecordLinkBlockingKey {
            key_id: "amount-date-category".to_string(),
            components: vec![
                RecordLinkBlockingComponent::FixedDecimalBucket {
                    feature_id: "cmp:amount".to_string(),
                    units: "basis_points".to_string(),
                    scale: 2,
                    bucket_width_scaled_units: 1,
                },
                RecordLinkBlockingComponent::DateBucket {
                    feature_id: "cmp:as_of".to_string(),
                    bucket_days: 1,
                },
                RecordLinkBlockingComponent::CategoricalExact {
                    feature_id: "cmp:category".to_string(),
                },
            ],
        }],
    }
}

fn categorical_blocking_policy() -> RecordLinkBlockingPolicy {
    RecordLinkBlockingPolicy {
        policy_id: "neutral:blocking".to_string(),
        policy_version: "1".to_string(),
        keys: vec![RecordLinkBlockingKey {
            key_id: "category-only".to_string(),
            components: vec![RecordLinkBlockingComponent::CategoricalExact {
                feature_id: "cmp:category".to_string(),
            }],
        }],
    }
}

fn standard_feature_policies() -> BTreeMap<String, RecordLinkFeaturePolicy> {
    BTreeMap::from([
        (
            "cmp:amount".to_string(),
            feature_policy(
                "cmp:amount",
                RecordLinkFeatureKind::Numeric,
                RecordLinkSupportPolicy::NumericTolerance {
                    tolerance_scaled_units: 0,
                },
                1_000,
                false,
            ),
        ),
        (
            "cmp:as_of".to_string(),
            feature_policy(
                "cmp:as_of",
                RecordLinkFeatureKind::Date,
                RecordLinkSupportPolicy::DateNear { max_days: 0 },
                1_000,
                false,
            ),
        ),
        (
            "cmp:category".to_string(),
            feature_policy(
                "cmp:category",
                RecordLinkFeatureKind::Categorical,
                RecordLinkSupportPolicy::CategoricalExact,
                1_000,
                false,
            ),
        ),
    ])
}

fn alternate_same_outcome_feature_policies() -> BTreeMap<String, RecordLinkFeaturePolicy> {
    BTreeMap::from([
        (
            "cmp:amount".to_string(),
            feature_policy(
                "cmp:amount",
                RecordLinkFeatureKind::Numeric,
                RecordLinkSupportPolicy::NumericTolerance {
                    tolerance_scaled_units: 1,
                },
                1_000,
                false,
            ),
        ),
        (
            "cmp:as_of".to_string(),
            feature_policy(
                "cmp:as_of",
                RecordLinkFeatureKind::Date,
                RecordLinkSupportPolicy::DateNear { max_days: 1 },
                1_000,
                false,
            ),
        ),
        (
            "cmp:category".to_string(),
            feature_policy(
                "cmp:category",
                RecordLinkFeatureKind::Categorical,
                RecordLinkSupportPolicy::CategoricalExact,
                1_000,
                false,
            ),
        ),
    ])
}

fn amount_hard_conflict_policies() -> BTreeMap<String, RecordLinkFeaturePolicy> {
    BTreeMap::from([(
        "cmp:amount".to_string(),
        feature_policy(
            "cmp:amount",
            RecordLinkFeatureKind::Numeric,
            RecordLinkSupportPolicy::NumericTolerance {
                tolerance_scaled_units: 0,
            },
            1_000,
            true,
        ),
    )])
}

fn feature_policy(
    feature_id: &str,
    kind: RecordLinkFeatureKind,
    support: RecordLinkSupportPolicy,
    score_units: u64,
    hard_conflict_on_mismatch: bool,
) -> RecordLinkFeaturePolicy {
    RecordLinkFeaturePolicy {
        feature_id: feature_id.to_string(),
        kind,
        support,
        score_units,
        hard_conflict_on_mismatch,
    }
}

fn reseal_candidate_set(candidate_set: &mut RecordLinkCandidateSet) {
    candidate_set.content_hash.clear();
    let bytes = serde_json::to_vec(candidate_set).expect("candidate-set hash bytes");
    candidate_set.content_hash = format!("blake3:{}", blake3::hash(&bytes).to_hex());
}

fn format_scaled(value: i64, scale: u32) -> String {
    if scale == 0 {
        return value.to_string();
    }
    let divisor = 10_i64.pow(scale);
    format!(
        "{}.{:0width$}",
        value / divisor,
        value % divisor,
        width = scale as usize
    )
}
