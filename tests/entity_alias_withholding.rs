#![forbid(unsafe_code)]

use canon::{
    entity::{
        CANON_ENTITY_BLOCK_VERSION_V1, CANON_ENTITY_SOLVE_VERSION_V1, EntityArtifactHeader,
        EntityArtifactReference,
        artifact_chain::{
            EntityArtifactChainExpectation, EntityArtifactChainLink, EntityChainStage,
        },
        audit::{
            EntityAuditArtifact, EntityAuditGateCheck, EntityAuditRequest, EntityAuditSuite,
            run_entity_audit,
        },
        block::{
            BlockCandidateBudgetConfig, BlockCandidateGenerationDiagnostics, BlockCandidateHit,
            BlockCandidateRecord, BlockOperatorCandidateDiagnostics, BlockOperatorYield,
        },
        edge::{EdgeCandidateBudgetProof, EdgeEvidenceHit, build_edge_evidence_record},
        graph::{SignedEvidenceGraphInput, SurfaceIncumbentId, build_signed_evidence_graph},
        publication::{
            CANON_ENTITY_STAGE_PUBLICATION_VERSION, EntityPublicationFileInput,
            EntityPublicationRequest, EntityPublicationUpstreamRef, open_current_stream_generation,
            publish_stream_patch,
        },
        record_link::ASSIGNMENT_ALIGNMENT_VERSION,
        review::{
            LinkReviewQueueRequest, ReviewExportInclude, ReviewQueueArtifact, ReviewQueueRequest,
            build_link_review_queue_artifact, build_review_queue_artifact,
        },
        review_export::{NativeReviewExportRequest, build_native_review_artifact},
        review_import::{
            NativeReviewDecision, NativeReviewDecisionAction, NativeReviewDecisionMode,
            import_native_review_decisions, native_review_import_context_from_artifact,
        },
        run::{
            ENTITY_RUN_PUBLICATION_STREAM_ID, EntityRunArtifact,
            link::{
                ENTITY_LINK_DECISIONS_VERSION, ENTITY_LINK_MATERIALIZED_ROWS_VERSION,
                ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION, ENTITY_LINK_VERSION,
                EntityLinkArtifact, EntityLinkDecisionArtifact,
                EntityLinkObservationSurfaceBinding, EntityLinkRole, LINK_ARTIFACT_PATH,
                LINK_ASSIGNMENT_ALIGNMENT_PATH, LINK_MATERIALIZED_ROWS_PATH,
                LINK_OBSERVATION_SURFACE_BINDINGS_PATH,
                read_validated_entity_link_observation_surface_bindings_at_path,
            },
            publish_entity_run_link_publication_patch,
        },
        score::{ScoreLane, ScoreUnits},
        solve::{
            SolveArtifact, SolveArtifactRequest, SolveReconciliationConfig,
            SolveReconciliationState, SolveSurfaceProvenance, build_solve_artifact_contract,
        },
    },
    evaluation::alias_withholding::{
        ALIAS_WITHHOLDING_SEALED_REVIEW_LABEL_SET_VERSION, AliasClass, AliasRecord,
        AliasWithholdingBenchmark, AliasWithholdingError, AliasWithholdingErrorCode,
        AliasWithholdingExecutionAssertions, AliasWithholdingExecutionEnvelope,
        AliasWithholdingExecutionManifest, CANON_ALIAS_WITHHOLDING_ASSIGNMENT_FIREWALL_VERSION,
        CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION,
        CANON_ALIAS_WITHHOLDING_LEAKAGE_SCAN_VERSION, CANON_ALIAS_WITHHOLDING_VERSION,
        CandidateEvaluation, CandidateRecallExecutionPaths, EntityEngineDecision,
        EvidenceLaneReport, ExactReplayExecutionPaths, IncumbentEntitySnapshot, LeakChannel,
        LeakageExecutionPath, LeakageProbe, NativeCandidateRecallDisposition, NativePromotionRoute,
        PermissibleContext, PromotionExecutionPaths, PromotionReplay, RegistryIdentity,
        RelationPolicy, ReviewAction, SealedReviewDenominators, SealedReviewLabelBinding,
        SealedReviewLabelDisposition, SealedReviewLabelSet, TrialOutcome, TrustedIdentifier,
        WithheldAlias, alias_withholding_schema_version, build_clean_base_registry_snapshot,
        canonical_benchmark_bytes, canonical_report_bytes, compile_alias_withholding_benchmark,
        compile_alias_withholding_benchmark_from_execution_manifest, exact_lookup,
    },
    resolve::{AssertionResult, MatchRecord, ResolveSummary, UnmatchedRecord},
    witness,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Output,
};

const MINIMAL_TRIALS: &str =
    include_str!("fixtures/canon_v1/quality/alias_withholding/minimal_trials.json");
const LEAK_CHANNELS: &str =
    include_str!("fixtures/canon_v1/quality/alias_withholding/leak_channels.json");

#[derive(Debug, Deserialize)]
struct LeakFixture {
    cases: Vec<LeakCase>,
}

#[derive(Debug, Deserialize)]
struct LeakCase {
    case_id: String,
    channel: LeakChannel,
    locator: String,
    leak_value: String,
}

#[test]
fn minimal_alias_withholding_fixture_compiles_clean_base_trials() {
    let benchmark = minimal_benchmark();
    let report = compile_alias_withholding_benchmark(benchmark).expect("benchmark compiles");

    assert_eq!(
        alias_withholding_schema_version(),
        CANON_ALIAS_WITHHOLDING_VERSION
    );
    assert_eq!(report.version, CANON_ALIAS_WITHHOLDING_VERSION);
    assert_eq!(report.aggregate.trial_count, 2);
    assert_eq!(report.aggregate.clean_base_snapshot_count, 2);
    assert_eq!(report.aggregate.credited_attachment_count, 1);
    assert_eq!(report.aggregate.unsupported_guess_count, 1);

    let attach = report
        .trials
        .iter()
        .find(|trial| trial.trial_id == "trial.punctuation.attach")
        .expect("attach trial");
    assert_eq!(attach.candidate_rank, Some(1));
    assert_eq!(attach.decision, EntityEngineDecision::Attach);
    assert_eq!(attach.review_action, ReviewAction::PromoteAlias);
    assert_eq!(attach.outcome, TrialOutcome::CorrectAttachment);
    assert!(attach.credited_attachment);
    assert!(!attach.exact_absence_proof.lookup_found);
    assert_eq!(
        attach
            .promotion_replay
            .as_ref()
            .and_then(|replay| replay.exact_replay_canonical_id.as_deref()),
        Some("ENT-001")
    );
    assert!(
        attach
            .evidence_lanes
            .iter()
            .any(|lane| lane.lane_id == "surface_tokens")
    );

    let unsupported = report
        .trials
        .iter()
        .find(|trial| trial.trial_id == "trial.division.unsupported_guess")
        .expect("unsupported relation trial");
    assert_eq!(unsupported.relation_policy, RelationPolicy::DivisionLabel);
    assert_eq!(unsupported.outcome, TrialOutcome::UnsupportedGuess);
    assert!(!unsupported.credited_attachment);
}

#[test]
fn clean_base_snapshot_proves_exact_lookup_absence() {
    let benchmark = minimal_benchmark();
    let trial = benchmark
        .trials
        .iter()
        .find(|trial| trial.trial_id == "trial.punctuation.attach")
        .expect("attach trial");
    let snapshot = build_clean_base_registry_snapshot(&benchmark.registry, trial)
        .expect("base snapshot builds");

    assert!(exact_lookup(&snapshot, "ACME, Inc.").is_none());
    assert_eq!(
        exact_lookup(&snapshot, "Acme Inc").map(|mapping| mapping.canonical_id.as_str()),
        Some("ENT-001")
    );
    assert!(
        snapshot
            .exact_mappings
            .iter()
            .all(|mapping| mapping.input_value != "ACME, Inc.")
    );
}

#[test]
fn canonical_bytes_are_stable_across_trial_and_alias_order() {
    let left = minimal_benchmark();
    let mut right = minimal_benchmark();
    right.trials.reverse();
    for trial in &mut right.trials {
        trial.entity.aliases.reverse();
        trial.entity.trusted_identifiers.reverse();
        trial.entity.permissible_context.reverse();
        trial.retained_alias_ids.reverse();
        trial.evaluation.evidence_lanes.reverse();
        trial.leakage_probes.reverse();
    }

    assert_eq!(
        canonical_benchmark_bytes(&left).expect("left bytes"),
        canonical_benchmark_bytes(&right).expect("right bytes")
    );

    let left_report = compile_alias_withholding_benchmark(left).expect("left report");
    let right_report = compile_alias_withholding_benchmark(right).expect("right report");
    assert_eq!(
        canonical_report_bytes(&left_report).expect("left report bytes"),
        canonical_report_bytes(&right_report).expect("right report bytes")
    );
}

#[test]
fn prohibited_leak_channels_refuse_before_scoring() {
    let leak_fixture: LeakFixture = serde_json::from_str(LEAK_CHANNELS).expect("leaks parse");
    for case in leak_fixture.cases {
        let mut benchmark = minimal_benchmark();
        let trial = benchmark
            .trials
            .iter_mut()
            .find(|trial| trial.trial_id == "trial.punctuation.attach")
            .expect("attach trial");

        if case.channel == LeakChannel::DisplayNameCopy {
            trial.entity.display_name = case.leak_value.clone();
        } else {
            trial.leakage_probes.push(LeakageProbe {
                channel: case.channel,
                locator: case.locator.clone(),
                value: case.leak_value.clone(),
            });
        }

        let error = match compile_alias_withholding_benchmark(benchmark) {
            Ok(_) => panic!("{} should refuse", case.case_id),
            Err(error) => error,
        };
        assert_eq!(error.code, AliasWithholdingErrorCode::SideChannelLeak);
        assert!(
            error.message.contains(case.channel.as_str()),
            "error should identify leak channel for {}: {}",
            case.case_id,
            error.message
        );
    }
}

#[test]
fn retained_withheld_alias_is_exact_lookup_leak() {
    let mut benchmark = minimal_benchmark();
    let trial = benchmark
        .trials
        .iter_mut()
        .find(|trial| trial.trial_id == "trial.punctuation.attach")
        .expect("attach trial");
    trial.retained_alias_ids.push("alias.withheld".to_string());

    let error = compile_alias_withholding_benchmark(benchmark)
        .expect_err("retained withheld alias must refuse");
    assert_eq!(error.code, AliasWithholdingErrorCode::ExactLookupLeak);
}

#[test]
fn replay_mismatch_is_reported_without_granting_credit() {
    let mut benchmark = minimal_benchmark();
    let trial = benchmark
        .trials
        .iter_mut()
        .find(|trial| trial.trial_id == "trial.punctuation.attach")
        .expect("attach trial");
    trial
        .evaluation
        .promotion_replay
        .as_mut()
        .expect("promotion replay")
        .exact_replay_canonical_id = Some("ENT-WRONG".to_string());

    let report = compile_alias_withholding_benchmark(benchmark).expect("report compiles");
    let trial = report
        .trials
        .iter()
        .find(|trial| trial.trial_id == "trial.punctuation.attach")
        .expect("attach trial");
    assert_eq!(trial.outcome, TrialOutcome::ReplayMismatch);
    assert!(!trial.credited_attachment);
}

fn minimal_benchmark() -> AliasWithholdingBenchmark {
    serde_json::from_str(MINIMAL_TRIALS).expect("minimal alias-withholding fixture parses")
}

#[test]
fn native_execution_manifest_derives_outcomes_from_artifacts_not_declared_evaluation() {
    let fixture = NativeAliasFixture::new();
    let report = fixture.compile().expect("native manifest compiles");

    assert_eq!(report.aggregate.trial_count, 2);
    assert_eq!(report.aggregate.credited_attachment_count, 1);
    assert_eq!(report.aggregate.abstain_count, 1);

    let attach = report_for(&report, NATIVE_ATTACH_TRIAL);
    assert_eq!(attach.decision, EntityEngineDecision::Attach);
    assert_eq!(attach.candidate_rank, None);
    assert_eq!(
        attach.candidate_recall_disposition,
        Some(NativeCandidateRecallDisposition::PreparedSurfaceCollapse)
    );
    assert_eq!(attach.candidate_canonical_id.as_deref(), Some("ORG-001"));
    assert_eq!(attach.outcome, TrialOutcome::CorrectAttachment);
    assert!(attach.credited_attachment);
    assert!(!attach.exact_absence_proof.lookup_found);
    assert_eq!(attach.exact_absence_proof.checked_mapping_count, 1);
    assert_eq!(
        attach
            .promotion_replay
            .as_ref()
            .and_then(|replay| replay.exact_replay_canonical_id.as_deref()),
        Some("ORG-001")
    );
    let attach_receipt = attach
        .native_engine_evidence
        .as_ref()
        .expect("attach receipt");
    assert_eq!(
        attach_receipt.candidate_recall_disposition,
        NativeCandidateRecallDisposition::PreparedSurfaceCollapse
    );
    assert!(
        !attach_receipt
            .link_observation_surface_bindings_hash
            .is_empty()
    );
    assert_eq!(attach_receipt.assignment_fact_count, 1);
    assert_eq!(attach_receipt.issuer_identity_alias_count, 1);
    assert_eq!(attach_receipt.assignment_derived_alias_count, 0);
    assert_eq!(attach_receipt.identity_key_count, 0);
    assert_eq!(attach_receipt.external_crosswalk_identity_key_count, 0);
    assert!(!attach_receipt.assignment_facts_used_as_aliases);
    assert!(attach_receipt.promotion_artifact_hash.is_some());
    assert!(attach_receipt.promotion_lock_hash.is_some());
    assert!(attach_receipt.promotion_pack_id.is_some());
    assert!(attach_receipt.apply_artifact_hash.is_some());
    assert_eq!(
        attach_receipt.sealed_review_label.denominators.total_labels,
        2
    );
    assert_eq!(
        attach_receipt.sealed_review_label.disposition,
        SealedReviewLabelDisposition::ReviewedPositive
    );
    assert_eq!(
        attach_receipt
            .leak_channels_checked
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        LeakChannel::all().into_iter().collect::<BTreeSet<_>>()
    );

    let abstain = report_for(&report, NATIVE_ABSTAIN_TRIAL);
    assert_eq!(abstain.decision, EntityEngineDecision::Abstain);
    assert_eq!(abstain.outcome, TrialOutcome::CorrectAbstention);
    assert!(!abstain.credited_attachment);
    assert!(abstain.promotion_replay.is_none());
    assert!(abstain.native_engine_evidence.is_some());
    assert_eq!(abstain.exact_absence_proof.checked_mapping_count, 1);
    let abstain_receipt = abstain
        .native_engine_evidence
        .as_ref()
        .expect("abstain receipt");
    assert_eq!(
        abstain_receipt.sealed_review_label.disposition,
        SealedReviewLabelDisposition::HardNegative
    );
    assert_eq!(
        abstain_receipt
            .sealed_review_label
            .corroborating_attribute_lanes,
        vec![
            "amount".to_string(),
            "category".to_string(),
            "date".to_string()
        ]
    );

    let declared_attach = fixture
        .benchmark
        .trials
        .iter()
        .find(|trial| trial.trial_id == NATIVE_ATTACH_TRIAL)
        .expect("declared attach trial");
    assert_eq!(
        declared_attach.evaluation.decision,
        EntityEngineDecision::Abstain
    );
    let declared_abstain = fixture
        .benchmark
        .trials
        .iter()
        .find(|trial| trial.trial_id == NATIVE_ABSTAIN_TRIAL)
        .expect("declared abstain trial");
    assert_eq!(
        declared_abstain.evaluation.decision,
        EntityEngineDecision::Attach
    );
}

#[test]
fn native_relation_policy_controls_are_excluded_from_recall_and_expose_false_merges() {
    let mut fixture = NativeAliasFixture::new();
    for trial in &mut fixture.benchmark.trials {
        trial.withheld_alias.relation_policy = RelationPolicy::RelatedDistinct;
    }
    rewrite_sealed_label_disposition(
        &mut fixture,
        NATIVE_ATTACH_TRIAL,
        SealedReviewLabelDisposition::HardNegative,
    );
    fixture.manifests[0].promotion = None;
    fixture.manifests[0].exact_replay = None;
    rewrite_candidate_case_disposition(&fixture, 0, "relation_policy_control");
    rewrite_candidate_case_disposition(&fixture, 1, "relation_policy_control");

    let report = fixture
        .compile()
        .expect("relation-policy controls compile to visible outcomes");
    let false_merge = report_for(&report, NATIVE_ATTACH_TRIAL);
    assert_eq!(false_merge.decision, EntityEngineDecision::Attach);
    assert_eq!(false_merge.outcome, TrialOutcome::UnsupportedGuess);
    assert_eq!(false_merge.candidate_rank, None);
    assert_eq!(
        false_merge.candidate_recall_disposition,
        Some(NativeCandidateRecallDisposition::RelationPolicyControl)
    );
    assert_eq!(false_merge.review_action, ReviewAction::RejectCandidate);
    assert!(false_merge.promotion_replay.is_none());
    assert!(!false_merge.credited_attachment);
    assert!(false_merge.evidence_lanes.iter().any(|lane| {
        lane.lane_id == "relation_policy_control" && lane.contradiction_basis_points == 10_000
    }));

    let abstain = report_for(&report, NATIVE_ABSTAIN_TRIAL);
    assert_eq!(abstain.decision, EntityEngineDecision::Abstain);
    assert_eq!(abstain.outcome, TrialOutcome::CorrectAbstention);
    assert_eq!(abstain.candidate_rank, None);
    assert_eq!(
        abstain.candidate_recall_disposition,
        Some(NativeCandidateRecallDisposition::RelationPolicyControl)
    );
    assert!(abstain.promotion_replay.is_none());
    assert_eq!(report.aggregate.unsupported_guess_count, 1);
    assert_eq!(report.aggregate.abstain_count, 1);
}

#[test]
fn native_nonattach_target_may_be_absent_from_solve_when_link_and_review_agree() {
    let mut fixture = NativeAliasFixture::new();
    let block: canon::entity::block_artifact::BlockCandidateArtifact =
        read_json(&fixture.block_path);
    let solve = build_native_solve_without_abstain_target(
        &fixture.solve_path,
        &block,
        &fixture.surface_ids,
    );
    write_json(&fixture.solve_path, &solve);

    let mut run: EntityRunArtifact = read_json(&fixture.run_path);
    replace_stage_hash(
        &mut run,
        CANON_ENTITY_SOLVE_VERSION_V1,
        &solve.artifact_content_hash,
    );
    sync_run_metadata_upstreams(&mut run);
    reseal_run(&mut run);
    write_json(&fixture.run_path, &run);

    let link = republish_mutated_native_link_fixture(
        &fixture.link_path,
        &run,
        &solve,
        NativeLinkDecisionRefreshMode::DeriveFromSolve,
    );

    let review = build_link_review_queue_artifact(LinkReviewQueueRequest {
        link_artifact: link,
        include: ReviewExportInclude::All,
    })
    .expect("link review queue rebuilds");
    write_json(&fixture.review_queue_path, &review);
    write_json(
        &fixture.audit_path,
        &passing_native_audit(&solve, &run, &review),
    );

    let clean_tree_hash = registry_tree_hash(&fixture.clean_abstain_dir);
    write_assignment_firewall(
        &fixture.base_dir,
        &fixture.abstain_assignment_firewall_path,
        NATIVE_ABSTAIN_TRIAL,
        &run.artifact_content_hash,
        &clean_tree_hash,
        false,
    );
    let sources = leakage_sources(
        &fixture.clean_abstain_dir,
        &fixture.abstain_quality_manifest_path,
        &fixture.block_path,
        &fixture.candidates_path,
        &fixture.run_path,
        &clean_tree_hash,
        &block.artifact_content_hash,
        &block.candidate_records_hash,
        &run.artifact_content_hash,
    );
    fixture.abstain_leakage_paths =
        write_leakage_artifacts(&fixture.base_dir, "abstain", NATIVE_ABSTAIN_TRIAL, &sources);

    fixture
        .benchmark
        .trials
        .retain(|trial| trial.trial_id == NATIVE_ABSTAIN_TRIAL);
    fixture
        .manifests
        .retain(|manifest| manifest.trial_id == NATIVE_ABSTAIN_TRIAL);
    fixture.manifests[0].assertions.review_id = Some(review_id_for(&review, ABSTAIN_OBSERVATION));

    let report = fixture
        .compile()
        .expect("non-attach solve absence compiles");
    let trial = report_for(&report, NATIVE_ABSTAIN_TRIAL);
    assert_eq!(trial.outcome, TrialOutcome::CorrectAbstention);
    assert_eq!(trial.decision, EntityEngineDecision::Abstain);
    assert!(!trial.credited_attachment);
    assert!(trial.promotion_replay.is_none());
    let native = trial
        .native_engine_evidence
        .as_ref()
        .expect("native evidence");
    assert!(native.promotion_artifact_hash.is_none());
    assert!(native.apply_artifact_hash.is_none());
}

#[test]
fn native_execution_manifest_refuses_tampered_artifact_chain() {
    let cases: &[TamperCase] = &[
        (
            "candidate payload",
            tamper_candidate_payload,
            AliasWithholdingErrorCode::ArtifactContract,
            "candidate JSONL",
        ),
        (
            "candidate report",
            tamper_candidate_report,
            AliasWithholdingErrorCode::ArtifactContract,
            "candidate-recall report does not match",
        ),
        (
            "review drift",
            tamper_review_queue,
            AliasWithholdingErrorCode::ArtifactContract,
            "review queue artifact does not match",
        ),
        (
            "review item target drift",
            tamper_review_item_target,
            AliasWithholdingErrorCode::ArtifactContract,
            "does not bind the withheld link target",
        ),
        (
            "wrong audited solve",
            tamper_audit_target,
            AliasWithholdingErrorCode::ArtifactContract,
            "audit artifact does not certify",
        ),
        (
            "solve target membership",
            tamper_solve_target_membership,
            AliasWithholdingErrorCode::ArtifactContract,
            "audited solve target component",
        ),
        (
            "missing alias patch",
            tamper_missing_alias_patch,
            AliasWithholdingErrorCode::ArtifactContract,
            "alias patch",
        ),
        (
            "stale sealed label set hash",
            tamper_stale_sealed_label_set_hash,
            AliasWithholdingErrorCode::ArtifactContract,
            "sealed review label set hash is stale",
        ),
        (
            "hard negative without corroboration",
            tamper_hard_negative_without_corroboration,
            AliasWithholdingErrorCode::ArtifactContract,
            "hard-negative label",
        ),
        (
            "promote-v1 missing review receipt",
            tamper_promote_v1_missing_review_receipt,
            AliasWithholdingErrorCode::ArtifactContract,
            "matched promotion requires review import receipt",
        ),
        (
            "missing promotion lock hash",
            tamper_missing_promotion_lock_hash,
            AliasWithholdingErrorCode::ArtifactContract,
            "lock_hash",
        ),
        (
            "missing promotion pack id",
            tamper_missing_promotion_pack_id,
            AliasWithholdingErrorCode::ArtifactContract,
            "pack_id",
        ),
        (
            "non-positive label with promotion",
            tamper_non_positive_label_with_promotion,
            AliasWithholdingErrorCode::ArtifactContract,
            "non-positive sealed reviewed labels",
        ),
        (
            "wrong replay",
            tamper_wrong_replay,
            AliasWithholdingErrorCode::ReplayMismatch,
            "exact replay output",
        ),
        (
            "replay registry binding",
            tamper_replay_registry_binding,
            AliasWithholdingErrorCode::ArtifactContract,
            "promoted registry snapshot",
        ),
        (
            "replay output binding",
            tamper_replay_output_binding,
            AliasWithholdingErrorCode::ArtifactContract,
            "output bytes",
        ),
        (
            "replay contradictory extra row",
            tamper_replay_contradictory_extra_row,
            AliasWithholdingErrorCode::ArtifactContract,
            "exactly one data row",
        ),
        (
            "path traversal",
            tamper_path_traversal,
            AliasWithholdingErrorCode::ArtifactContract,
            "path traversal segments are not allowed",
        ),
        (
            "assignment as alias",
            tamper_assignment_as_alias,
            AliasWithholdingErrorCode::ArtifactContract,
            "assignment",
        ),
        (
            "direct leakage",
            tamper_direct_leakage,
            AliasWithholdingErrorCode::SideChannelLeak,
            "mapping_file",
        ),
    ];

    for (name, mutate, code, message) in cases {
        let mut fixture = NativeAliasFixture::new();
        mutate(&mut fixture);
        let error = match fixture.compile() {
            Ok(_) => panic!("{name} should refuse"),
            Err(error) => error,
        };
        assert_eq!(error.code, *code, "{name}");
        assert!(
            error.message.contains(message),
            "{name} error should mention {message:?}: {}",
            error.message
        );
    }
}

#[test]
fn native_execution_manifest_refuses_adversarial_public_artifacts() {
    let cases: &[TamperCase] = &[
        (
            "empty retained clean registry",
            tamper_empty_retained_clean_registry,
            AliasWithholdingErrorCode::ArtifactContract,
            "clean registry",
        ),
        (
            "wrong retained clean registry",
            tamper_wrong_retained_clean_registry,
            AliasWithholdingErrorCode::ArtifactContract,
            "clean registry",
        ),
        (
            "forged add-entry receipt",
            tamper_forged_add_entry_receipt,
            AliasWithholdingErrorCode::ArtifactContract,
            "add-entry receipt",
        ),
        (
            "all leakage clearances unrelated",
            tamper_unrelated_leakage_clearances,
            AliasWithholdingErrorCode::SideChannelLeak,
            "leakage",
        ),
        (
            "zero-source assignment firewall",
            tamper_zero_source_assignment_firewall,
            AliasWithholdingErrorCode::ArtifactContract,
            "assignment",
        ),
    ];

    for (name, mutate, code, message) in cases {
        let mut fixture = NativeAliasFixture::new();
        mutate(&mut fixture);
        let error = match fixture.compile() {
            Ok(_) => panic!("{name} should refuse"),
            Err(error) => error,
        };
        assert_eq!(error.code, *code, "{name}");
        assert!(
            error.message.contains(message),
            "{name} error should mention {message:?}: {}",
            error.message
        );
    }
}

#[test]
fn alias_withholding_cli_routes_refusals_and_success_without_writes_or_secret_surfaces() {
    let fixture = NativeAliasFixture::new();
    let manifest = fixture.write_envelope("envelope.json");
    let before = workspace_file_fingerprints(&fixture.base_dir);

    let json_output = run_alias_withholding_cli(&manifest, "json");
    assert!(
        json_output.status.success(),
        "json stdout={} stderr={}",
        String::from_utf8_lossy(&json_output.stdout),
        String::from_utf8_lossy(&json_output.stderr)
    );
    assert!(
        json_output.stderr.is_empty(),
        "json success should not write stderr: {}",
        String::from_utf8_lossy(&json_output.stderr)
    );
    let json_report = parse_json_stdout(&json_output);
    assert_eq!(json_report["aggregate"]["trial_count"], json!(2));
    assert_eq!(
        trial_json(&json_report, NATIVE_ATTACH_TRIAL)["decision"],
        json!("attach")
    );
    assert_eq!(
        trial_json(&json_report, NATIVE_ABSTAIN_TRIAL)["decision"],
        json!("abstain")
    );
    assert_public_output_does_not_leak_surfaces(&json_output);
    assert_eq!(before, workspace_file_fingerprints(&fixture.base_dir));

    let summary_output = run_alias_withholding_cli(&manifest, "summary");
    assert!(
        summary_output.status.success(),
        "summary stdout={} stderr={}",
        String::from_utf8_lossy(&summary_output.stdout),
        String::from_utf8_lossy(&summary_output.stderr)
    );
    assert!(summary_output.stderr.is_empty());
    assert!(
        String::from_utf8_lossy(&summary_output.stdout).contains(&format!(
            "{} trials=2",
            witness::hash_bytes(b"neutral-native-alias-withholding.v1")
        ))
    );
    assert_public_output_does_not_leak_surfaces(&summary_output);
    assert_eq!(before, workspace_file_fingerprints(&fixture.base_dir));

    let missing = fixture.base_dir.join("missing-envelope.json");
    let missing_json = run_alias_withholding_cli(&missing, "json");
    assert_eq!(missing_json.status.code(), Some(2));
    assert!(missing_json.stderr.is_empty());
    let missing_refusal = parse_json_stdout(&missing_json);
    assert_eq!(
        missing_refusal["refusal"]["detail"]["reason"],
        json!("manifest_read_failed")
    );

    let missing_summary = run_alias_withholding_cli(&missing, "summary");
    assert_eq!(missing_summary.status.code(), Some(2));
    assert!(missing_summary.stdout.is_empty());
    let missing_summary_refusal = parse_json_stderr(&missing_summary);
    assert_eq!(
        missing_summary_refusal["refusal"]["detail"]["reason"],
        json!("manifest_read_failed")
    );

    let malformed = fixture.base_dir.join("malformed-envelope.json");
    fs::write(&malformed, "{").expect("malformed envelope");
    let malformed_json = run_alias_withholding_cli(&malformed, "json");
    assert_eq!(malformed_json.status.code(), Some(2));
    assert_eq!(
        parse_json_stdout(&malformed_json)["refusal"]["detail"]["reason"],
        json!("manifest_parse_failed")
    );
}

#[test]
fn alias_withholding_cli_refuses_path_escapes_and_tampered_references() {
    let cases: &[CliTamperCase] = &[
        (
            "path traversal",
            cli_tamper_path_traversal,
            "path traversal segments are not allowed",
        ),
        (
            "absolute path",
            cli_tamper_absolute_path,
            "absolute paths are not allowed",
        ),
        (
            "tampered referenced artifact",
            cli_tamper_candidate_report,
            "candidate-recall report does not match",
        ),
    ];

    for (name, mutate, message) in cases {
        let mut fixture = NativeAliasFixture::new();
        mutate(&mut fixture);
        let internal_error = match fixture.compile() {
            Ok(_) => panic!("{name} should refuse before CLI routing"),
            Err(error) => error,
        };
        assert!(
            internal_error.message.contains(message),
            "{name} internal refusal should mention {message:?}: {}",
            internal_error.message
        );
        let manifest = fixture.write_envelope(format!("{name}.json"));
        let output = run_alias_withholding_cli(&manifest, "json");
        assert_eq!(output.status.code(), Some(2), "{name}");
        let refusal = parse_json_stdout(&output);
        assert_eq!(
            refusal["refusal"]["detail"]["stage"],
            json!("alias_withholding"),
            "{name}"
        );
        assert_eq!(
            refusal["refusal"]["detail"]["writes_performed"],
            json!(false),
            "{name}"
        );
        assert_eq!(
            refusal["refusal"]["detail"]["message_fingerprint"],
            json!(witness::hash_bytes(internal_error.message.as_bytes())),
            "{name}"
        );
        assert!(refusal["refusal"]["detail"].get("message").is_none());
        assert_public_output_does_not_leak_surfaces(&output);
    }
}

#[cfg(unix)]
#[test]
fn alias_withholding_cli_refuses_symlink_leaf_reference() {
    let mut fixture = NativeAliasFixture::new();
    let target = fixture
        .base_dir
        .join(&fixture.manifests[0].link_artifact_path);
    let symlink = fixture.base_dir.join("link-symlink.json");
    std::os::unix::fs::symlink(&target, &symlink).expect("symlink link artifact");
    fixture.manifests[0].link_artifact_path = rel(&fixture.base_dir, &symlink);
    let internal_error = fixture
        .compile()
        .expect_err("symlink leaf should refuse before CLI routing");
    assert!(internal_error.message.contains("must not be a symlink"));
    let manifest = fixture.write_envelope("symlink-envelope.json");

    let output = run_alias_withholding_cli(&manifest, "json");
    assert_eq!(output.status.code(), Some(2));
    let refusal = parse_json_stdout(&output);
    assert_eq!(
        refusal["refusal"]["detail"]["message_fingerprint"],
        json!(witness::hash_bytes(internal_error.message.as_bytes()))
    );
    assert!(refusal["refusal"]["detail"].get("message").is_none());
    assert_public_output_does_not_leak_surfaces(&output);
}

const NATIVE_ATTACH_TRIAL: &str = "trial.native.attach";
const NATIVE_ABSTAIN_TRIAL: &str = "trial.native.abstain";
const ATTACH_OBSERVATION: &str = "obs-attach";
const ABSTAIN_OBSERVATION: &str = "obs-abstain";
const ATTACH_WITHHELD: &str = "Arcadia Lab LLC";
const ABSTAIN_WITHHELD: &str = "Northstar Research Group";

#[derive(Clone)]
struct NativeFixtureSurfaceIds {
    attach_reference: String,
    abstain_reference: String,
    attach_target: String,
    abstain_target: String,
}

struct NativeAliasFixture {
    _temp: tempfile::TempDir,
    base_dir: PathBuf,
    benchmark: AliasWithholdingBenchmark,
    manifests: Vec<AliasWithholdingExecutionManifest>,
    block_path: PathBuf,
    candidates_path: PathBuf,
    attach_report_path: PathBuf,
    abstain_quality_manifest_path: PathBuf,
    run_path: PathBuf,
    solve_path: PathBuf,
    link_path: PathBuf,
    review_queue_path: PathBuf,
    audit_path: PathBuf,
    clean_attach_dir: PathBuf,
    clean_abstain_dir: PathBuf,
    review_import_receipt_path: PathBuf,
    add_entry_receipt_path: PathBuf,
    apply_artifact_path: PathBuf,
    replay_output_path: PathBuf,
    assignment_firewall_path: PathBuf,
    abstain_assignment_firewall_path: PathBuf,
    link_artifact_hash: String,
    leakage_paths: BTreeMap<LeakChannel, PathBuf>,
    abstain_leakage_paths: BTreeMap<LeakChannel, PathBuf>,
    surface_ids: NativeFixtureSurfaceIds,
}

type TamperCase = (
    &'static str,
    fn(&mut NativeAliasFixture),
    AliasWithholdingErrorCode,
    &'static str,
);

type CliTamperCase = (&'static str, fn(&mut NativeAliasFixture), &'static str);

impl NativeAliasFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let base_dir = temp.path().to_path_buf();
        let work_dir = base_dir.join("work");
        let registry = base_dir.join("input_registry");
        let reference = base_dir.join("reference.csv");
        let target = base_dir.join("target.csv");
        let strategy = base_dir.join("strategy.yaml");

        write_input_registry(&registry);
        fs::write(
            &reference,
            "source_row_id,entity_id,org_name,field_name,dataset,doc_id,accession,filing_cik,role_context,capacity,subject_role,alias_surfaces_json,mention_surfaces_json\nORG-001,ORG-001,Arcadia Lab,issuer,neutral,doc-a,acc-a,cik-a,identity,issuer,subject,[],[]\nORG-002,ORG-002,Northstar Supply,issuer,neutral,doc-b,acc-b,cik-b,identity,issuer,subject,[],[]\n",
        )
        .expect("reference rows");
        fs::write(
            &target,
            format!(
                "source_row_id,entity_id,org_name,field_name,dataset,doc_id,accession,filing_cik,role_context,capacity,subject_role,alias_surfaces_json,mention_surfaces_json\n{ATTACH_OBSERVATION},{ATTACH_OBSERVATION},{ATTACH_WITHHELD},issuer,neutral,doc-c,acc-c,cik-c,identity,issuer,subject,[],[]\n{ABSTAIN_OBSERVATION},{ABSTAIN_OBSERVATION},{ABSTAIN_WITHHELD},issuer,neutral,doc-d,acc-d,cik-d,identity,issuer,subject,[],[]\n"
            ),
        )
        .expect("target rows");
        fs::write(
            &strategy,
            r#"strategy_id: neutral-alias-withholding-link.v1
strategy_version: "1.0.0"
entity_type: organization
identity:
  reference:
    id_columns: [entity_id]
  target:
    id_columns: [entity_id]
candidate_filter: []
assertions:
  - field_ref: org_name
    field_tgt: org_name
    op: prefix
    weight: 1.0
    required: true
match_threshold: 0.75
ambiguity_gap: 0.10
max_candidates: 10
"#,
        )
        .expect("strategy");

        assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
            .args([
                "entity",
                "link",
                path_str(&reference),
                path_str(&target),
                "--profile",
                "regab_firm_identity",
                "--strategy",
                path_str(&strategy),
                "--registry",
                path_str(&registry),
                "--work-dir",
                path_str(&work_dir),
                "--no-witness",
                "--emit",
                "json",
            ])
            .assert()
            .code(1);

        let block_path = work_dir.join("block/block.json");
        let candidates_path = work_dir.join("block/candidates.jsonl");
        let diagnostics_path = work_dir.join("block/diagnostics.json");
        let exact_buckets_path = work_dir.join("block/exact_buckets.jsonl");
        let solve_path = work_dir.join("solve/solve.json");
        let run_path = work_dir.join("run/run.json");
        let link_path = work_dir.join("link/link.json");
        let review_queue_path = work_dir.join("review/all.json");
        let audit_path = work_dir.join("audit/audit.json");
        fs::create_dir_all(review_queue_path.parent().expect("review parent")).expect("review dir");
        fs::create_dir_all(audit_path.parent().expect("audit parent")).expect("audit dir");

        let generated_link: EntityLinkArtifact = read_json(&link_path);
        let generated_bindings = read_validated_entity_link_observation_surface_bindings_at_path(
            &generated_link,
            &link_path,
        )
        .expect("generated link bindings validate");
        let surface_ids = native_fixture_surface_ids(&generated_bindings);

        let mut candidate_records = Vec::new();
        if surface_ids.attach_reference != surface_ids.attach_target {
            candidate_records.push(BlockCandidateRecord {
                version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
                left_surface_id: surface_ids.attach_reference.clone(),
                right_surface_id: surface_ids.attach_target.clone(),
                block_hits: vec![BlockCandidateHit {
                    operator_id: "native_prefix_candidate".to_string(),
                    rank: Some(1),
                    score_units: 10_000,
                }],
                candidate_score_hint: 10_000,
            });
        }
        if surface_ids.abstain_reference != surface_ids.abstain_target {
            candidate_records.push(BlockCandidateRecord {
                version: CANON_ENTITY_BLOCK_VERSION_V1.to_string(),
                left_surface_id: surface_ids.abstain_reference.clone(),
                right_surface_id: surface_ids.abstain_target.clone(),
                block_hits: vec![BlockCandidateHit {
                    operator_id: "native_abstain_candidate".to_string(),
                    rank: Some(1),
                    score_units: 5_000,
                }],
                candidate_score_hint: 5_000,
            });
        }
        candidate_records.sort_by(|left, right| {
            left.left_surface_id
                .cmp(&right.left_surface_id)
                .then_with(|| left.right_surface_id.cmp(&right.right_surface_id))
        });
        write_jsonl(&candidates_path, &candidate_records);
        let diagnostics = candidate_diagnostics(&candidate_records);
        write_json(&diagnostics_path, &diagnostics);

        let mut block: canon::entity::block_artifact::BlockCandidateArtifact =
            read_json(&block_path);
        block.candidate_records_hash = hash_jsonl_records(&candidate_records);
        block.candidate_diagnostics_hash = hash_compact_json(&diagnostics);
        block.summary.counts.insert(
            "candidate_pairs".to_string(),
            candidate_records.len() as u64,
        );
        block.summary.counts.insert(
            "candidate_pair_count".to_string(),
            candidate_records.len() as u64,
        );
        block
            .summary
            .counts
            .insert("block_hits".to_string(), candidate_records.len() as u64);
        block.summary.counts.insert(
            "operator_hit_count".to_string(),
            candidate_records.len() as u64,
        );
        reseal_block(&mut block);
        write_json(&block_path, &block);

        let mut solve = build_native_solve(&solve_path, &block, &surface_ids);
        write_json(&solve_path, &solve);

        let mut run: EntityRunArtifact = read_json(&run_path);
        replace_stage_hash(
            &mut run,
            CANON_ENTITY_BLOCK_VERSION_V1,
            &block.artifact_content_hash,
        );
        replace_stage_hash(
            &mut run,
            CANON_ENTITY_SOLVE_VERSION_V1,
            &solve.artifact_content_hash,
        );
        sync_run_metadata_upstreams(&mut run);
        reseal_run(&mut run);
        write_json(&run_path, &run);

        let link = republish_mutated_native_link_fixture(
            &link_path,
            &run,
            &solve,
            NativeLinkDecisionRefreshMode::DeriveFromSolve,
        );
        let link_artifact_hash = link.artifact_content_hash.clone();

        let review = build_link_review_queue_artifact(LinkReviewQueueRequest {
            link_artifact: link.clone(),
            include: ReviewExportInclude::All,
        })
        .expect("link review queue builds");
        write_json(&review_queue_path, &review);

        solve = read_json(&solve_path);
        let audit = passing_native_audit(&solve, &run, &review);
        write_json(&audit_path, &audit);

        let clean_attach = base_dir.join("clean_attach_registry");
        let promoted_registry_dir = base_dir.join("promoted_attach_registry");
        let clean_abstain = base_dir.join("clean_abstain_registry");
        write_registry_with_entries(
            &clean_attach,
            "1.0.0",
            &[("Arcadia Lab", "ORG-001", "RET_Attach")],
        );
        write_registry_with_entries(
            &promoted_registry_dir,
            "1.0.1",
            &[
                ("Arcadia Lab", "ORG-001", "RET_Attach"),
                (ATTACH_WITHHELD, "ORG-001", "ADD_Attach"),
            ],
        );
        write_registry_with_entries(
            &clean_abstain,
            "1.0.0",
            &[("Northstar Supply", "ORG-002", "RET_Abstain")],
        );

        let benchmark = native_benchmark();
        let promotion_review_queue = build_review_queue_artifact(ReviewQueueRequest {
            solve_artifact: solve.clone(),
            include: ReviewExportInclude::All,
            provenance_samples: Vec::new(),
            relation_hints: Vec::new(),
        })
        .expect("promotion solve review queue builds");
        let promotion_review_queue_path = base_dir.join("review/promotion-solve.json");
        fs::create_dir_all(
            promotion_review_queue_path
                .parent()
                .expect("promotion review parent"),
        )
        .expect("promotion review dir");
        write_json(&promotion_review_queue_path, &promotion_review_queue);
        let promotion_review_id =
            review_id_for(&promotion_review_queue, &surface_ids.attach_target);
        let native_review = build_native_review_artifact(NativeReviewExportRequest {
            review_queue: promotion_review_queue,
            run_content_hash: run.artifact_content_hash.clone(),
            policy_content_hash: benchmark.policy_digest.clone(),
        })
        .expect("native promotion review builds");
        let native_review_item = native_review
            .review_items
            .iter()
            .find(|item| item.review_id == promotion_review_id)
            .expect("promotion review item");
        let native_review_value =
            serde_json::to_value(&native_review).expect("native review value");
        let native_review_context =
            native_review_import_context_from_artifact(&native_review_value)
                .expect("native review import context");
        let native_review_item_value = native_review_value["review_items"]
            .as_array()
            .expect("native review items")
            .iter()
            .find(|item| item["review_id"] == json!(promotion_review_id))
            .expect("native review item value");
        let review_import_receipt = import_native_review_decisions(
            native_review_context,
            vec![NativeReviewDecision {
                review_id: promotion_review_id.clone(),
                mode: NativeReviewDecisionMode::Cluster,
                action: NativeReviewDecisionAction::Alias,
                operator_id: "fixture-reviewer".to_string(),
                reason_code: "confirmed_same_entity".to_string(),
                note: "derivation-proven prepared surface collapse".to_string(),
                source_review_artifact_hash: native_review.artifact_content_hash.clone(),
                decision_binding_hash: native_review_item.decision_binding_hash.clone(),
                run_content_hash: native_review.binding.run_content_hash.clone(),
                policy_content_hash: native_review.binding.policy_content_hash.clone(),
                registry_snapshot_hash: native_review.binding.registry_snapshot_hash.clone(),
                mode_context: serde_json::from_value(
                    native_review_item_value["mode_context"].clone(),
                )
                .expect("native review cluster context"),
                surface_ids: Vec::new(),
                target_canonical_id: Some("ORG-001".to_string()),
                relation: None,
            }],
        )
        .expect("singleton collapse review decision imports");
        let review_import_receipt_path = base_dir.join("review/import-receipt.json");
        fs::create_dir_all(
            review_import_receipt_path
                .parent()
                .expect("review import parent"),
        )
        .expect("review import dir");
        write_json(&review_import_receipt_path, &review_import_receipt);

        let add_entry_receipt_path = base_dir.join("promotion/add-entry-receipt.json");
        fs::create_dir_all(add_entry_receipt_path.parent().expect("promotion parent"))
            .expect("promotion dir");
        write_json_value(
            &add_entry_receipt_path,
            json!({
                "version": "canon_registry_add_entry.v0",
                "registry": {
                    "id": "neutral-registry",
                    "entry_count_before": 1,
                    "entry_count_after": 2,
                    "version_before": "1.0.0",
                    "version_after": "1.0.1"
                },
                "alias_entry": {
                    "input": ATTACH_WITHHELD,
                    "canonical_id": "ORG-001",
                    "canonical_type": "org",
                    "rule_id": "ADD_Attach"
                },
                "lint": { "errors": 0 },
                "touched_files": ["registry.json", "aliases.json"]
            }),
        );

        let replay_input_path = base_dir.join("replay/input.csv");
        let replay_output_path = base_dir.join("replay/output.csv");
        let apply_artifact_path = base_dir.join("replay/apply.json");
        fs::create_dir_all(replay_input_path.parent().expect("replay parent")).expect("replay dir");
        fs::write(&replay_input_path, format!("org_name\n{ATTACH_WITHHELD}\n"))
            .expect("replay input");
        let apply = canon::entity::apply::run_apply_streaming_from_registry(
            canon::entity::apply::ApplyRegistryStreamRequest {
                rows: &replay_input_path,
                output: &replay_output_path,
                lookup_column: "org_name",
                registry_dir: &promoted_registry_dir,
                safety: canon::entity::apply::ApplySafetyCheck::default(),
                require_full_resolution: true,
                target_rows_per_chunk: canon::entity::apply::DEFAULT_APPLY_ROWS_PER_CHUNK,
            },
        )
        .expect("exact replay apply succeeds");
        write_json(&apply_artifact_path, &apply);

        let exact_bucket_count = line_count(&exact_buckets_path);
        let attach_manifest_path = base_dir.join("candidate/attach_manifest.json");
        let abstain_manifest_path = base_dir.join("candidate/abstain_manifest.json");
        let attach_report_path = base_dir.join("candidate/attach_report.json");
        let abstain_report_path = base_dir.join("candidate/abstain_report.json");
        fs::create_dir_all(attach_manifest_path.parent().expect("candidate parent"))
            .expect("candidate dir");
        write_quality_manifest(
            &attach_manifest_path,
            "gold.attach",
            &surface_ids.attach_reference,
            &surface_ids.attach_target,
            "withheld_alias",
            if surface_ids.attach_reference == surface_ids.attach_target {
                "prepared_surface_collapse"
            } else {
                "same_entity"
            },
        );
        write_quality_manifest(
            &abstain_manifest_path,
            "gold.abstain",
            &surface_ids.abstain_reference,
            &surface_ids.abstain_target,
            "withheld_alias",
            "relation_policy_control",
        );
        run_candidate_recall_command(
            &attach_manifest_path,
            &candidates_path,
            &diagnostics_path,
            exact_bucket_count,
            &attach_report_path,
        );
        run_candidate_recall_command(
            &abstain_manifest_path,
            &candidates_path,
            &diagnostics_path,
            exact_bucket_count,
            &abstain_report_path,
        );

        let attach_clean_tree_hash = registry_tree_hash(&clean_attach);
        let abstain_clean_tree_hash = registry_tree_hash(&clean_abstain);
        let assignment_firewall_path = base_dir.join("assignment/attach/firewall.json");
        let abstain_assignment_firewall_path = base_dir.join("assignment/abstain/firewall.json");
        write_assignment_firewall(
            &base_dir,
            &assignment_firewall_path,
            NATIVE_ATTACH_TRIAL,
            &run.artifact_content_hash,
            &attach_clean_tree_hash,
            false,
        );
        write_assignment_firewall(
            &base_dir,
            &abstain_assignment_firewall_path,
            NATIVE_ABSTAIN_TRIAL,
            &run.artifact_content_hash,
            &abstain_clean_tree_hash,
            false,
        );
        let attach_leakage_sources = leakage_sources(
            &clean_attach,
            &attach_manifest_path,
            &block_path,
            &candidates_path,
            &run_path,
            &attach_clean_tree_hash,
            &block.artifact_content_hash,
            &hash_jsonl_records(&candidate_records),
            &run.artifact_content_hash,
        );
        let abstain_leakage_sources = leakage_sources(
            &clean_abstain,
            &abstain_manifest_path,
            &block_path,
            &candidates_path,
            &run_path,
            &abstain_clean_tree_hash,
            &block.artifact_content_hash,
            &hash_jsonl_records(&candidate_records),
            &run.artifact_content_hash,
        );
        let leakage_paths = write_leakage_artifacts(
            &base_dir,
            "attach",
            NATIVE_ATTACH_TRIAL,
            &attach_leakage_sources,
        );
        let abstain_leakage_paths = write_leakage_artifacts(
            &base_dir,
            "abstain",
            NATIVE_ABSTAIN_TRIAL,
            &abstain_leakage_sources,
        );

        let attach_manifest = execution_manifest(
            &base_dir,
            NATIVE_ATTACH_TRIAL,
            ATTACH_OBSERVATION,
            "gold.attach",
            "ORG-001",
            ATTACH_OBSERVATION,
            Some(review_id_for(&review, ATTACH_OBSERVATION)),
            &attach_manifest_path,
            &block_path,
            &candidates_path,
            &diagnostics_path,
            &exact_buckets_path,
            &attach_report_path,
            exact_bucket_count,
            &link_path,
            &run_path,
            &solve_path,
            &review_queue_path,
            &audit_path,
            &clean_attach,
            &assignment_firewall_path,
            &leakage_paths,
            Some(PromotionExecutionPaths {
                route: NativePromotionRoute::RegistryAddEntry,
                lock_hash: Some(witness::hash_bytes(b"bd-2hav neutral project lock")),
                pack_id: Some(witness::hash_bytes(b"bd-2hav neutral promotion package")),
                promotion_artifact_path: Some(rel(&base_dir, &add_entry_receipt_path)),
                review_import_receipt_path: Some(rel(&base_dir, &review_import_receipt_path)),
                review_queue_artifact_path: Some(rel(&base_dir, &promotion_review_queue_path)),
                review_id: Some(promotion_review_id),
                promoted_registry_dir: rel(&base_dir, &promoted_registry_dir),
            }),
            Some(ExactReplayExecutionPaths {
                input_path: rel(&base_dir, &replay_input_path),
                lookup_column: "org_name".to_string(),
                apply_artifact_path: rel(&base_dir, &apply_artifact_path),
                output_path: rel(&base_dir, &replay_output_path),
            }),
        );
        let abstain_manifest = execution_manifest(
            &base_dir,
            NATIVE_ABSTAIN_TRIAL,
            ABSTAIN_OBSERVATION,
            "gold.abstain",
            "ORG-002",
            ABSTAIN_OBSERVATION,
            Some(review_id_for(&review, ABSTAIN_OBSERVATION)),
            &abstain_manifest_path,
            &block_path,
            &candidates_path,
            &diagnostics_path,
            &exact_buckets_path,
            &abstain_report_path,
            exact_bucket_count,
            &link_path,
            &run_path,
            &solve_path,
            &review_queue_path,
            &audit_path,
            &clean_abstain,
            &abstain_assignment_firewall_path,
            &abstain_leakage_paths,
            None,
            None,
        );

        Self {
            _temp: temp,
            base_dir,
            benchmark,
            manifests: vec![attach_manifest, abstain_manifest],
            block_path,
            candidates_path,
            attach_report_path,
            abstain_quality_manifest_path: abstain_manifest_path,
            run_path,
            solve_path,
            link_path,
            review_queue_path,
            audit_path,
            clean_attach_dir: clean_attach,
            clean_abstain_dir: clean_abstain,
            review_import_receipt_path,
            add_entry_receipt_path,
            apply_artifact_path,
            replay_output_path,
            assignment_firewall_path,
            abstain_assignment_firewall_path,
            link_artifact_hash,
            leakage_paths,
            abstain_leakage_paths,
            surface_ids,
        }
    }

    fn compile(
        &self,
    ) -> Result<canon::evaluation::alias_withholding::AliasWithholdingReport, AliasWithholdingError>
    {
        compile_alias_withholding_benchmark_from_execution_manifest(
            self.benchmark.clone(),
            &self.base_dir,
            self.manifests.clone(),
        )
    }

    fn envelope(&self) -> AliasWithholdingExecutionEnvelope {
        AliasWithholdingExecutionEnvelope {
            version: CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION.to_string(),
            benchmark: self.benchmark.clone(),
            manifests: self.manifests.clone(),
        }
    }

    fn write_envelope(&self, file_name: impl AsRef<Path>) -> PathBuf {
        let path = self.base_dir.join(file_name);
        write_json(&path, &self.envelope());
        path
    }
}

fn native_benchmark() -> AliasWithholdingBenchmark {
    AliasWithholdingBenchmark {
        version: CANON_ALIAS_WITHHOLDING_VERSION.to_string(),
        benchmark_id: "neutral-native-alias-withholding.v1".to_string(),
        registry: RegistryIdentity {
            registry_id: "neutral-registry".to_string(),
            registry_version: "1.0.0".to_string(),
        },
        policy_digest: witness::hash_bytes(b"neutral alias withholding policy"),
        trials: vec![
            native_trial(
                NATIVE_ATTACH_TRIAL,
                "ORG-001",
                "Arcadia issuer",
                "alias.arcadia.retained",
                "Arcadia Lab",
                "alias.arcadia.withheld",
                ATTACH_OBSERVATION,
                ATTACH_WITHHELD,
                EntityEngineDecision::Abstain,
            ),
            native_trial(
                NATIVE_ABSTAIN_TRIAL,
                "ORG-002",
                "Northstar issuer",
                "alias.northstar.retained",
                "Northstar Supply",
                "alias.northstar.withheld",
                ABSTAIN_OBSERVATION,
                ABSTAIN_WITHHELD,
                EntityEngineDecision::Attach,
            ),
        ],
    }
}

fn native_sealed_review_label_set() -> SealedReviewLabelSet {
    let mut label_set = SealedReviewLabelSet {
        version: ALIAS_WITHHOLDING_SEALED_REVIEW_LABEL_SET_VERSION.to_string(),
        label_set_hash: String::new(),
        source_manifest_hash: witness::hash_bytes(b"bd-2hav neutral owner-only input manifest"),
        selection_seed: "bd-2hav-neutral-selection-seed-0001".to_string(),
        denominators: SealedReviewDenominators {
            total_labels: 2,
            reviewed_positive_count: 1,
            hard_negative_count: 1,
            ambiguity_count: 0,
            unmatched_count: 0,
            censored_attempt_count: 0,
        },
        labels: vec![
            sealed_label(
                "sealed.attach",
                NATIVE_ATTACH_TRIAL,
                ATTACH_OBSERVATION,
                SealedReviewLabelDisposition::ReviewedPositive,
            ),
            sealed_label(
                "sealed.hard_negative",
                NATIVE_ABSTAIN_TRIAL,
                ABSTAIN_OBSERVATION,
                SealedReviewLabelDisposition::HardNegative,
            ),
        ],
    };
    reseal_label_set(&mut label_set);
    label_set
}

fn sealed_label(
    label_id: &str,
    trial_id: &str,
    canonical_record_id: &str,
    disposition: SealedReviewLabelDisposition,
) -> SealedReviewLabelBinding {
    let hard_negative = disposition == SealedReviewLabelDisposition::HardNegative;
    SealedReviewLabelBinding {
        label_id: label_id.to_string(),
        trial_id: trial_id.to_string(),
        lane: "issuer_identity_record_link".to_string(),
        canonical_record_id: canonical_record_id.to_string(),
        material_hash: witness::hash_bytes(
            format!("material:{trial_id}:{canonical_record_id}").as_bytes(),
        ),
        disposition,
        lookalike_signal_hashes: if hard_negative {
            vec![witness::hash_bytes(b"name-surface-lookalike")]
        } else {
            Vec::new()
        },
        corroborating_attribute_lanes: if hard_negative {
            vec![
                "amount".to_string(),
                "category".to_string(),
                "date".to_string(),
            ]
        } else {
            Vec::new()
        },
        corroborating_attribute_hashes: if hard_negative {
            vec![
                witness::hash_bytes(b"amount-refutes-identity"),
                witness::hash_bytes(b"category-refutes-identity"),
                witness::hash_bytes(b"date-refutes-identity"),
            ]
        } else {
            Vec::new()
        },
        hard_negative_basis: hard_negative.then(|| {
            "candidate-like name surface refuted by amount/date/category corroboration".to_string()
        }),
    }
}

fn reseal_label_set(label_set: &mut SealedReviewLabelSet) {
    for label in &mut label_set.labels {
        label.lookalike_signal_hashes.sort();
        label.lookalike_signal_hashes.dedup();
        label.corroborating_attribute_lanes.sort();
        label.corroborating_attribute_lanes.dedup();
        label.corroborating_attribute_hashes.sort();
        label.corroborating_attribute_hashes.dedup();
    }
    label_set.labels.sort();
    label_set.label_set_hash.clear();
    label_set.label_set_hash = hash_compact_json(label_set);
}

fn refresh_label_denominators(label_set: &mut SealedReviewLabelSet) {
    label_set.denominators = SealedReviewDenominators {
        total_labels: label_set.labels.len() as u64,
        reviewed_positive_count: label_set
            .labels
            .iter()
            .filter(|label| label.disposition == SealedReviewLabelDisposition::ReviewedPositive)
            .count() as u64,
        hard_negative_count: label_set
            .labels
            .iter()
            .filter(|label| label.disposition == SealedReviewLabelDisposition::HardNegative)
            .count() as u64,
        ambiguity_count: label_set
            .labels
            .iter()
            .filter(|label| label.disposition == SealedReviewLabelDisposition::Ambiguity)
            .count() as u64,
        unmatched_count: label_set
            .labels
            .iter()
            .filter(|label| label.disposition == SealedReviewLabelDisposition::Unmatched)
            .count() as u64,
        censored_attempt_count: label_set
            .labels
            .iter()
            .filter(|label| label.disposition == SealedReviewLabelDisposition::CensoredAttempt)
            .count() as u64,
    };
}

fn rewrite_sealed_label_disposition(
    fixture: &mut NativeAliasFixture,
    trial_id: &str,
    disposition: SealedReviewLabelDisposition,
) {
    for manifest in &mut fixture.manifests {
        let label_set = &mut manifest.sealed_review_label_set;
        let label = label_set
            .labels
            .iter_mut()
            .find(|label| label.trial_id == trial_id)
            .unwrap_or_else(|| panic!("missing sealed label for {trial_id}"));
        let label_id = label.label_id.clone();
        let canonical_record_id = label.canonical_record_id.clone();
        *label = sealed_label(&label_id, trial_id, &canonical_record_id, disposition);
        refresh_label_denominators(label_set);
        reseal_label_set(label_set);
    }
}

#[allow(clippy::too_many_arguments)]
fn native_trial(
    trial_id: &str,
    canonical_id: &str,
    display_name: &str,
    retained_alias_id: &str,
    retained_alias: &str,
    withheld_alias_id: &str,
    observation_id: &str,
    withheld_surface: &str,
    declared_decision: EntityEngineDecision,
) -> canon::evaluation::alias_withholding::AliasWithholdingTrialSpec {
    canon::evaluation::alias_withholding::AliasWithholdingTrialSpec {
        trial_id: trial_id.to_string(),
        entity: IncumbentEntitySnapshot {
            canonical_id: canonical_id.to_string(),
            display_name: display_name.to_string(),
            aliases: vec![
                AliasRecord {
                    alias_id: retained_alias_id.to_string(),
                    value: retained_alias.to_string(),
                    alias_class: AliasClass::ReviewedRename,
                    reviewed: true,
                    eligible: true,
                },
                AliasRecord {
                    alias_id: withheld_alias_id.to_string(),
                    value: withheld_surface.to_string(),
                    alias_class: AliasClass::LegalSuffix,
                    reviewed: true,
                    eligible: true,
                },
            ],
            trusted_identifiers: Vec::<TrustedIdentifier>::new(),
            permissible_context: Vec::<PermissibleContext>::new(),
        },
        withheld_alias: WithheldAlias {
            alias_id: withheld_alias_id.to_string(),
            observation_id: observation_id.to_string(),
            surface: withheld_surface.to_string(),
            alias_class: AliasClass::LegalSuffix,
            relation_policy: if trial_id == NATIVE_ABSTAIN_TRIAL {
                RelationPolicy::RelatedDistinct
            } else {
                RelationPolicy::SameEntityAllowed
            },
        },
        retained_alias_ids: vec![retained_alias_id.to_string()],
        evaluation: declared_candidate_evaluation(declared_decision, canonical_id),
        leakage_probes: vec![],
    }
}

fn declared_candidate_evaluation(
    decision: EntityEngineDecision,
    canonical_id: &str,
) -> CandidateEvaluation {
    CandidateEvaluation {
        candidate_rank: (decision == EntityEngineDecision::Attach).then_some(1),
        decision,
        candidate_canonical_id: (decision == EntityEngineDecision::Attach)
            .then(|| canonical_id.to_string()),
        evidence_lanes: vec![EvidenceLaneReport {
            lane_id: "declared_fixture_not_authority".to_string(),
            support_basis_points: 1,
            contradiction_basis_points: 0,
            public_evidence_ref: witness::hash_bytes(b"declared fixture"),
        }],
        abstention_action: ReviewAction::DeferReview,
        review_action: if decision == EntityEngineDecision::Attach {
            ReviewAction::PromoteAlias
        } else {
            ReviewAction::DeferReview
        },
        promotion_replay: (decision == EntityEngineDecision::Attach).then(|| PromotionReplay {
            approved: true,
            promoted_registry_digest: witness::hash_bytes(b"declared promotion digest"),
            exact_replay_canonical_id: Some(canonical_id.to_string()),
        }),
    }
}

fn native_fixture_surface_ids(
    bindings: &[EntityLinkObservationSurfaceBinding],
) -> NativeFixtureSurfaceIds {
    NativeFixtureSurfaceIds {
        attach_reference: native_fixture_surface(bindings, EntityLinkRole::Reference, "ORG-001"),
        abstain_reference: native_fixture_surface(bindings, EntityLinkRole::Reference, "ORG-002"),
        attach_target: native_fixture_surface(bindings, EntityLinkRole::Target, ATTACH_OBSERVATION),
        abstain_target: native_fixture_surface(
            bindings,
            EntityLinkRole::Target,
            ABSTAIN_OBSERVATION,
        ),
    }
}

fn native_fixture_surface(
    bindings: &[EntityLinkObservationSurfaceBinding],
    side: EntityLinkRole,
    link_id: &str,
) -> String {
    bindings
        .iter()
        .find(|binding| binding.side == side && binding.link_id == link_id)
        .unwrap_or_else(|| panic!("missing {side:?} fixture binding for {link_id}"))
        .surface_id
        .clone()
}

fn build_native_solve(
    solve_path: &Path,
    block: &canon::entity::block_artifact::BlockCandidateArtifact,
    surface_ids: &NativeFixtureSurfaceIds,
) -> SolveArtifact {
    build_native_solve_variant(solve_path, block, surface_ids, true, false)
}

fn build_native_solve_variant(
    solve_path: &Path,
    block: &canon::entity::block_artifact::BlockCandidateArtifact,
    surface_ids: &NativeFixtureSurfaceIds,
    attach_to_incumbent: bool,
    abstain_to_incumbent: bool,
) -> SolveArtifact {
    let mut original: SolveArtifact = read_json(solve_path);
    replace_ref_hash(
        &mut original.metadata.upstream_artifacts,
        CANON_ENTITY_BLOCK_VERSION_V1,
        &block.artifact_content_hash,
    );
    original.metadata.artifact_content_hash.clear();
    let attach_reference = if attach_to_incumbent {
        surface_ids.attach_reference.clone()
    } else {
        "NOVEL-ATTACH-PEER".to_string()
    };
    let abstain_reference = if abstain_to_incumbent {
        surface_ids.abstain_reference.clone()
    } else {
        "NOVEL-ABSTAIN-PEER".to_string()
    };
    let mut edge_records = Vec::new();
    if attach_reference != surface_ids.attach_target {
        let (left_surface_id, right_surface_id) =
            ordered_fixture_surface_pair(&attach_reference, &surface_ids.attach_target);
        edge_records.push(
            build_edge_evidence_record(
                left_surface_id,
                right_surface_id,
                vec![EdgeEvidenceHit::new(
                    ScoreLane::Support,
                    "name",
                    "native_prefix_candidate",
                    "prefix_identity_evidence",
                    score(10_000),
                    false,
                    "native prefix identity evidence",
                )],
            )
            .expect("support edge"),
        );
    }
    if abstain_reference != surface_ids.abstain_target {
        let (left_surface_id, right_surface_id) =
            ordered_fixture_surface_pair(&abstain_reference, &surface_ids.abstain_target);
        edge_records.push(
            build_edge_evidence_record(
                left_surface_id,
                right_surface_id,
                vec![EdgeEvidenceHit::new(
                    ScoreLane::Support,
                    "name",
                    "native_novel_cluster",
                    "novel_cluster_support",
                    score(10_000),
                    false,
                    "native novel cluster support",
                )],
            )
            .expect("abstain novel cluster edge"),
        );
    }
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![
            SurfaceIncumbentId {
                surface_id: surface_ids.attach_reference.clone(),
                canonical_id: "ORG-001".to_string(),
            },
            SurfaceIncumbentId {
                surface_id: surface_ids.abstain_reference.clone(),
                canonical_id: "ORG-002".to_string(),
            },
        ],
    })
    .expect("signed graph");
    let provenance = native_solve_provenance([
        surface_ids.attach_reference.clone(),
        surface_ids.abstain_reference.clone(),
        surface_ids.attach_target.clone(),
        surface_ids.abstain_target.clone(),
        attach_reference,
        abstain_reference,
    ]);
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: original.metadata,
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance,
        decision_ledger_path: "solve/decision_ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds")
}

fn native_solve_provenance(
    surface_ids: impl IntoIterator<Item = String>,
) -> Vec<SolveSurfaceProvenance> {
    surface_ids
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|surface_id| SolveSurfaceProvenance {
            surface_id,
            row_count: 1,
            deal_count: 0,
        })
        .collect()
}

fn ordered_fixture_surface_pair<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn build_native_solve_without_abstain_target(
    solve_path: &Path,
    block: &canon::entity::block_artifact::BlockCandidateArtifact,
    surface_ids: &NativeFixtureSurfaceIds,
) -> SolveArtifact {
    let mut original: SolveArtifact = read_json(solve_path);
    replace_ref_hash(
        &mut original.metadata.upstream_artifacts,
        CANON_ENTITY_BLOCK_VERSION_V1,
        &block.artifact_content_hash,
    );
    original.metadata.artifact_content_hash.clear();
    let edge_records = if surface_ids.attach_reference == surface_ids.attach_target {
        Vec::new()
    } else {
        let (left_surface_id, right_surface_id) =
            ordered_fixture_surface_pair(&surface_ids.attach_reference, &surface_ids.attach_target);
        vec![
            build_edge_evidence_record(
                left_surface_id,
                right_surface_id,
                vec![EdgeEvidenceHit::new(
                    ScoreLane::Support,
                    "name",
                    "native_prefix_candidate",
                    "prefix_identity_evidence",
                    score(10_000),
                    false,
                    "native prefix identity evidence",
                )],
            )
            .expect("support edge"),
        ]
    };
    let graph = build_signed_evidence_graph(SignedEvidenceGraphInput {
        edge_records,
        exact_bucket_assertions: vec![],
        incumbent_ids: vec![
            SurfaceIncumbentId {
                surface_id: surface_ids.attach_reference.clone(),
                canonical_id: "ORG-001".to_string(),
            },
            SurfaceIncumbentId {
                surface_id: surface_ids.abstain_reference.clone(),
                canonical_id: "ORG-002".to_string(),
            },
        ],
    })
    .expect("signed graph");
    let provenance = native_solve_provenance([
        surface_ids.attach_reference.clone(),
        surface_ids.attach_target.clone(),
        surface_ids.abstain_reference.clone(),
    ]);
    build_solve_artifact_contract(SolveArtifactRequest {
        metadata: original.metadata,
        graph,
        config: SolveReconciliationConfig::delegate_new_ids(score(5_000)),
        provenance,
        decision_ledger_path: "solve/decision_ledger.jsonl".to_string(),
    })
    .expect("solve artifact builds")
}

fn candidate_diagnostics(
    candidate_records: &[BlockCandidateRecord],
) -> BlockCandidateGenerationDiagnostics {
    BlockCandidateGenerationDiagnostics {
        candidate_record_count: candidate_records.len() as u64,
        candidate_pairs_emitted: candidate_records.len() as u64,
        candidate_pairs_suppressed_by_cap: 0,
        suppressed_candidate_count: 0,
        large_buckets_suppressed: 0,
        candidate_pairs_per_surface_p50: 1,
        candidate_pairs_per_surface_p95: 1,
        candidate_pairs_per_surface_p99: 1,
        max_candidates_for_surface: 1,
        max_candidates_for_operator: 1,
        configured_budget: BlockCandidateBudgetConfig::new(10, 10, 10),
        candidate_budget: EdgeCandidateBudgetProof::within_run_budget(1, 10),
        candidate_artifact_bytes: jsonl_bytes(candidate_records).len() as u64,
        partial_candidate_artifact_written: false,
        operator_yield: vec![BlockOperatorYield {
            operator_id: "native_prefix_candidate".to_string(),
            emitted_candidate_count: candidate_records.len() as u64,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
        operator_diagnostics: vec![BlockOperatorCandidateDiagnostics {
            operator_id: "native_prefix_candidate".to_string(),
            input_candidate_count: candidate_records.len() as u64,
            eligible_candidate_count: candidate_records.len() as u64,
            emitted_candidate_count: candidate_records.len() as u64,
            suppressed_candidate_count: 0,
            large_posting_suppressed_count: 0,
        }],
    }
}

#[allow(clippy::too_many_arguments)]
fn execution_manifest(
    base_dir: &Path,
    trial_id: &str,
    observation_id: &str,
    gold_pair_id: &str,
    reference_observation_id: &str,
    target_observation_id: &str,
    review_id: Option<String>,
    quality_manifest_path: &Path,
    block_artifact_path: &Path,
    candidates_path: &Path,
    diagnostics_path: &Path,
    exact_buckets_path: &Path,
    report_path: &Path,
    exact_bucket_count: u64,
    link_artifact_path: &Path,
    run_artifact_path: &Path,
    solve_artifact_path: &Path,
    review_queue_artifact_path: &Path,
    audit_artifact_path: &Path,
    clean_registry_dir: &Path,
    assignment_firewall_path: &Path,
    leakage_paths: &BTreeMap<LeakChannel, PathBuf>,
    promotion: Option<PromotionExecutionPaths>,
    exact_replay: Option<ExactReplayExecutionPaths>,
) -> AliasWithholdingExecutionManifest {
    AliasWithholdingExecutionManifest {
        version: CANON_ALIAS_WITHHOLDING_EXECUTION_MANIFEST_VERSION.to_string(),
        trial_id: trial_id.to_string(),
        observation_id: observation_id.to_string(),
        assertions: AliasWithholdingExecutionAssertions {
            gold_pair_id: gold_pair_id.to_string(),
            reference_observation_id: reference_observation_id.to_string(),
            target_observation_id: target_observation_id.to_string(),
            incumbent_canonical_id: if trial_id == NATIVE_ATTACH_TRIAL {
                "ORG-001".to_string()
            } else {
                "ORG-002".to_string()
            },
            review_id,
        },
        sealed_review_label_set: native_sealed_review_label_set(),
        candidate_recall: CandidateRecallExecutionPaths {
            quality_manifest_path: rel(base_dir, quality_manifest_path),
            block_artifact_path: rel(base_dir, block_artifact_path),
            candidates_path: rel(base_dir, candidates_path),
            diagnostics_path: rel(base_dir, diagnostics_path),
            exact_bucket_assertions_path: rel(base_dir, exact_buckets_path),
            report_path: rel(base_dir, report_path),
            exact_bucket_count,
        },
        link_artifact_path: rel(base_dir, link_artifact_path),
        run_artifact_path: rel(base_dir, run_artifact_path),
        solve_artifact_path: rel(base_dir, solve_artifact_path),
        review_queue_artifact_path: rel(base_dir, review_queue_artifact_path),
        audit_artifact_path: rel(base_dir, audit_artifact_path),
        clean_registry_dir: rel(base_dir, clean_registry_dir),
        promotion,
        exact_replay,
        assignment_firewall_path: rel(base_dir, assignment_firewall_path),
        leakage: LeakChannel::all()
            .into_iter()
            .map(|channel| LeakageExecutionPath {
                channel,
                artifact_path: rel(
                    base_dir,
                    leakage_paths
                        .get(&channel)
                        .expect("leakage path for every channel"),
                ),
            })
            .collect(),
    }
}

fn passing_native_audit(
    solve: &SolveArtifact,
    _run: &EntityRunArtifact,
    _review: &ReviewQueueArtifact,
) -> EntityAuditArtifact {
    let result = EntityArtifactHeader {
        version: solve.version.clone(),
        metadata: solve.metadata.clone(),
        summary: solve.summary.clone(),
    };
    let mut certified = solve.metadata.upstream_artifacts.clone();
    certified.push(EntityArtifactReference {
        version: solve.version.clone(),
        content_hash: solve.artifact_content_hash.clone(),
    });
    run_entity_audit(EntityAuditRequest {
        expected: EntityArtifactChainExpectation::from_link(
            EntityChainStage::Audit,
            &EntityArtifactChainLink::from_header(&result),
        ),
        certified_artifacts: certified,
        result,
        suite: EntityAuditSuite {
            id: "neutral_alias_withholding_native".to_string(),
            version: "1.0.0".to_string(),
            gates: vec![EntityAuditGateCheck {
                gate_id: "G01".to_string(),
                label: "artifact continuity".to_string(),
                passed: true,
                expected: "native_chain_hash_bound".to_string(),
                actual: "native_chain_hash_bound".to_string(),
                evidence: BTreeMap::new(),
            }],
        },
    })
    .expect("audit passes")
}

fn write_quality_manifest(
    path: &Path,
    case_id: &str,
    left_observation_id: &str,
    right_observation_id: &str,
    stratum: &str,
    label_disposition: &str,
) {
    write_json_value(
        path,
        json!({
            "observations": [
                { "observation_id": left_observation_id },
                { "observation_id": right_observation_id }
            ],
            "quality_harness": {
                "cases": [
                    {
                        "case_id": case_id,
                        "left_observation_id": left_observation_id,
                        "right_observation_id": right_observation_id,
                        "stratum": stratum,
                        "label_disposition": label_disposition
                    }
                ]
            }
        }),
    );
}

fn rewrite_candidate_case_disposition(
    fixture: &NativeAliasFixture,
    manifest_index: usize,
    disposition: &str,
) {
    let manifest = &fixture.manifests[manifest_index];
    let quality_path = fixture
        .base_dir
        .join(&manifest.candidate_recall.quality_manifest_path);
    let mut quality: Value = read_json(&quality_path);
    quality["quality_harness"]["cases"][0]["label_disposition"] = json!(disposition);
    write_json_value(&quality_path, quality);

    let candidates_path = fixture
        .base_dir
        .join(&manifest.candidate_recall.candidates_path);
    let diagnostics_path = fixture
        .base_dir
        .join(&manifest.candidate_recall.diagnostics_path);
    let report_path = fixture
        .base_dir
        .join(&manifest.candidate_recall.report_path);
    run_candidate_recall_command(
        &quality_path,
        &candidates_path,
        &diagnostics_path,
        manifest.candidate_recall.exact_bucket_count,
        &report_path,
    );

    let leakage = manifest
        .leakage
        .iter()
        .find(|leakage| leakage.channel == LeakChannel::NormalizationPatch)
        .expect("normalization leakage path");
    let leakage_path = fixture.base_dir.join(&leakage.artifact_path);
    let mut artifact: Value = read_json(&leakage_path);
    let binding_hash =
        witness::hash_bytes(&fs::read(&quality_path).expect("rewritten quality manifest bytes"));
    artifact["checked_sources"][0] =
        checked_source_descriptor(&fixture.base_dir, &quality_path, &binding_hash);
    write_json_value(&leakage_path, seal_value_artifact_hash(artifact));
}

fn run_candidate_recall_command(
    manifest: &Path,
    candidates: &Path,
    diagnostics: &Path,
    exact_bucket_count: u64,
    report_path: &Path,
) {
    let exact_bucket_count_arg = exact_bucket_count.to_string();
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "candidate-recall",
            "--manifest",
            path_str(manifest),
            "--candidates",
            path_str(candidates),
            "--diagnostics",
            path_str(diagnostics),
            "--exact-bucket-count",
            exact_bucket_count_arg.as_str(),
            "--emit",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    fs::write(report_path, output).expect("write candidate recall report");
}

fn write_input_registry(registry: &Path) {
    write_registry_with_entries(registry, "1.0.0", &[]);
}

fn write_registry_with_entries(registry: &Path, version: &str, entries: &[(&str, &str, &str)]) {
    fs::create_dir_all(registry).expect("registry dir");
    write_json_value(
        &registry.join("registry.json"),
        json!({
            "id": "neutral-registry",
            "version": version,
            "description": "Neutral alias withholding registry",
            "updated": "2026-07-12",
            "entry_count": entries.len()
        }),
    );
    let aliases = entries
        .iter()
        .map(|(input, canonical_id, rule_id)| {
            json!({
                "input": input,
                "canonical_id": canonical_id,
                "canonical_type": "org",
                "rule_id": rule_id
            })
        })
        .collect::<Vec<_>>();
    write_json_value(&registry.join("aliases.json"), Value::Array(aliases));
}

fn write_assignment_firewall(
    base_dir: &Path,
    path: &Path,
    trial_id: &str,
    assignment_binding_hash: &str,
    identity_binding_hash: &str,
    assignment_as_alias: bool,
) {
    let parent = path.parent().expect("assignment firewall parent");
    fs::create_dir_all(parent).expect("assignment firewall dir");
    let assignment_path = parent.join("assignment_facts.json");
    let identity_path = parent.join("issuer_identity_aliases.json");
    let assignment_fact = json!({
        "fact_type": "typed_assignment",
        "fact_id": format!("fact:{trial_id}"),
        "subject_ref": "holder:proxy",
        "object_ref": "issuer:proxy"
    });
    let identity_alias = json!({
        "record_type": "issuer_identity_alias",
        "alias_id": format!("retained:{trial_id}"),
        "canonical_ref": "issuer:proxy"
    });
    write_json_value(&assignment_path, assignment_fact.clone());
    write_json_value(&identity_path, identity_alias.clone());
    let assignment_hashes = vec![hash_compact_json(&assignment_fact)];
    let artifact = json!({
        "version": CANON_ALIAS_WITHHOLDING_ASSIGNMENT_FIREWALL_VERSION,
        "artifact_content_hash": "",
        "trial_id": trial_id,
        "assignment_facts_used_as_aliases": assignment_as_alias,
        "assignment_fact_hashes": assignment_hashes,
        "issuer_identity_alias_count": 1,
        "assignment_fact_count": 1,
        "assignment_derived_alias_count": if assignment_as_alias { 1 } else { 0 },
        "identity_key_count": 0,
        "external_crosswalk_identity_key_count": 0,
        "checked_sources": [
            {
                "kind": "assignment_facts",
                "source": checked_source_descriptor(base_dir, &assignment_path, assignment_binding_hash)
            },
            {
                "kind": "issuer_identity_aliases",
                "source": checked_source_descriptor(base_dir, &identity_path, identity_binding_hash)
            }
        ]
    });
    write_json_value(path, seal_value_artifact_hash(artifact));
}

#[allow(clippy::too_many_arguments)]
fn leakage_sources(
    clean_registry: &Path,
    quality_manifest_path: &Path,
    block_artifact_path: &Path,
    candidates_path: &Path,
    run_artifact_path: &Path,
    clean_registry_tree_hash: &str,
    block_artifact_hash: &str,
    candidate_records_hash: &str,
    run_artifact_hash: &str,
) -> BTreeMap<LeakChannel, (Vec<PathBuf>, String)> {
    let mut registry_files = vec![clean_registry.join("registry.json")];
    for entry in fs::read_dir(clean_registry).expect("clean registry dir") {
        let path = entry.expect("clean registry entry").path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
            && path
                .file_name()
                .is_some_and(|name| name != "registry.json" && name != "_build.json")
        {
            registry_files.push(path);
        }
    }
    registry_files.sort();
    let quality_manifest_hash =
        witness::hash_bytes(&fs::read(quality_manifest_path).expect("quality manifest bytes"));
    BTreeMap::from([
        (
            LeakChannel::MappingFile,
            (registry_files, clean_registry_tree_hash.to_string()),
        ),
        (
            LeakChannel::SearchIndex,
            (
                vec![block_artifact_path.to_path_buf()],
                block_artifact_hash.to_string(),
            ),
        ),
        (
            LeakChannel::Cache,
            (
                vec![run_artifact_path.to_path_buf()],
                run_artifact_hash.to_string(),
            ),
        ),
        (
            LeakChannel::NormalizationPatch,
            (
                vec![quality_manifest_path.to_path_buf()],
                quality_manifest_hash,
            ),
        ),
        (
            LeakChannel::GeneratedCorpus,
            (
                vec![candidates_path.to_path_buf()],
                candidate_records_hash.to_string(),
            ),
        ),
        (
            LeakChannel::DisplayNameCopy,
            (
                vec![clean_registry.join("aliases.json")],
                clean_registry_tree_hash.to_string(),
            ),
        ),
    ])
}

fn write_leakage_artifacts(
    base_dir: &Path,
    scope: &str,
    trial_id: &str,
    sources: &BTreeMap<LeakChannel, (Vec<PathBuf>, String)>,
) -> BTreeMap<LeakChannel, PathBuf> {
    let dir = base_dir.join("leakage").join(scope);
    fs::create_dir_all(&dir).expect("leakage dir");
    LeakChannel::all()
        .into_iter()
        .map(|channel| {
            let path = dir.join(format!("{}.json", channel.as_str()));
            let (paths, binding_hash) = sources.get(&channel).expect("source for every channel");
            let checked_sources = paths
                .iter()
                .map(|source| checked_source_descriptor(base_dir, source, binding_hash))
                .collect::<Vec<_>>();
            write_json_value(
                &path,
                seal_value_artifact_hash(json!({
                    "version": CANON_ALIAS_WITHHOLDING_LEAKAGE_SCAN_VERSION,
                    "artifact_content_hash": "",
                    "trial_id": trial_id,
                    "channel": channel.as_str(),
                    "checked_sources": checked_sources
                })),
            );
            (channel, path)
        })
        .collect()
}

fn checked_source_descriptor(base_dir: &Path, path: &Path, binding_hash: &str) -> Value {
    let bytes = fs::read(path).expect("checked source bytes");
    json!({
        "path": rel(base_dir, path),
        "content_hash": witness::hash_bytes(&bytes),
        "binding_hash": binding_hash,
        "byte_count": bytes.len(),
        "record_count": source_record_count(&bytes)
    })
}

fn source_record_count(bytes: &[u8]) -> u64 {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        return match value {
            Value::Array(records) => records.len() as u64,
            Value::Object(ref object) => ["records", "assignment_facts", "aliases"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_array))
                .map_or(1, |records| records.len() as u64),
            Value::Null => 0,
            _ => 1,
        };
    }
    std::str::from_utf8(bytes).map_or(1, |text| {
        text.lines().filter(|line| !line.trim().is_empty()).count() as u64
    })
}

fn registry_tree_hash(registry: &Path) -> String {
    let mut files = vec![registry.join("registry.json")];
    for entry in fs::read_dir(registry).expect("registry dir") {
        let path = entry.expect("registry entry").path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
            && path
                .file_name()
                .is_some_and(|name| name != "registry.json" && name != "_build.json")
        {
            files.push(path);
        }
    }
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("registry file name");
        let bytes = fs::read(&path).expect("registry file bytes");
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0xff]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn tamper_candidate_payload(fixture: &mut NativeAliasFixture) {
    fs::write(
        &fixture.candidates_path,
        "{\"version\":\"canon_entity_block.v1\",\"left_surface_id\":\"ORG-001\",\"right_surface_id\":\"obs-attach\",\"block_hits\":[],\"candidate_score_hint\":0}\n",
    )
    .expect("tamper candidates");
}

fn tamper_candidate_report(fixture: &mut NativeAliasFixture) {
    let mut report: Value = read_json(&fixture.attach_report_path);
    report["total_gold_pairs"] = json!(999);
    write_json_value(&fixture.attach_report_path, report);
}

fn tamper_review_queue(fixture: &mut NativeAliasFixture) {
    let mut review: Value = read_json(&fixture.review_queue_path);
    review["review_items"][0]["proposed_action"] = json!("tampered_action");
    write_json_value(&fixture.review_queue_path, review);
}

fn tamper_review_item_target(fixture: &mut NativeAliasFixture) {
    let review: ReviewQueueArtifact = read_json(&fixture.review_queue_path);
    fixture.manifests[0].assertions.review_id = Some(review_id_for(&review, ABSTAIN_OBSERVATION));
}

fn tamper_audit_target(fixture: &mut NativeAliasFixture) {
    let mut audit: EntityAuditArtifact = read_json(&fixture.audit_path);
    audit.audited_artifact.content_hash = fixture.link_artifact_hash.clone();
    reseal_audit(&mut audit);
    write_json(&fixture.audit_path, &audit);
}

fn tamper_solve_target_membership(fixture: &mut NativeAliasFixture) {
    let block: canon::entity::block_artifact::BlockCandidateArtifact =
        read_json(&fixture.block_path);
    let solve = build_native_solve_variant(
        &fixture.solve_path,
        &block,
        &fixture.surface_ids,
        true,
        true,
    );
    write_json(&fixture.solve_path, &solve);

    let mut run: EntityRunArtifact = read_json(&fixture.run_path);
    replace_stage_hash(
        &mut run,
        CANON_ENTITY_SOLVE_VERSION_V1,
        &solve.artifact_content_hash,
    );
    sync_run_metadata_upstreams(&mut run);
    reseal_run(&mut run);
    write_json(&fixture.run_path, &run);

    let link = republish_mutated_native_link_fixture(
        &fixture.link_path,
        &run,
        &solve,
        NativeLinkDecisionRefreshMode::PreserveExistingDecisions,
    );
    let review = build_link_review_queue_artifact(LinkReviewQueueRequest {
        link_artifact: link,
        include: ReviewExportInclude::All,
    })
    .expect("tampered link review queue builds");
    write_json(&fixture.review_queue_path, &review);
    let audit = passing_native_audit(&solve, &run, &review);
    write_json(&fixture.audit_path, &audit);
}

fn tamper_missing_alias_patch(fixture: &mut NativeAliasFixture) {
    let mut receipt: Value = read_json(&fixture.review_import_receipt_path);
    receipt["accepted_decisions"] = json!(0);
    receipt["patches"]["alias_patches"] = json!([]);
    write_json_value(&fixture.review_import_receipt_path, receipt);
}

fn tamper_stale_sealed_label_set_hash(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0]
        .sealed_review_label_set
        .selection_seed
        .push_str("-tampered");
}

fn tamper_hard_negative_without_corroboration(fixture: &mut NativeAliasFixture) {
    let label_set = &mut fixture.manifests[1].sealed_review_label_set;
    let label = label_set
        .labels
        .iter_mut()
        .find(|label| label.trial_id == NATIVE_ABSTAIN_TRIAL)
        .expect("abstain hard-negative label");
    label.lookalike_signal_hashes.clear();
    label.corroborating_attribute_lanes.clear();
    label.corroborating_attribute_hashes.clear();
    reseal_label_set(label_set);
}

fn tamper_promote_v1_missing_review_receipt(fixture: &mut NativeAliasFixture) {
    let promotion = fixture.manifests[0]
        .promotion
        .as_mut()
        .expect("matched trial promotion");
    promotion.route = NativePromotionRoute::PromoteV1;
    promotion.review_import_receipt_path = None;
}

fn tamper_missing_promotion_lock_hash(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0]
        .promotion
        .as_mut()
        .expect("matched trial promotion")
        .lock_hash = None;
}

fn tamper_missing_promotion_pack_id(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0]
        .promotion
        .as_mut()
        .expect("matched trial promotion")
        .pack_id = None;
}

fn tamper_non_positive_label_with_promotion(fixture: &mut NativeAliasFixture) {
    let label_set = &mut fixture.manifests[0].sealed_review_label_set;
    let label = label_set
        .labels
        .iter_mut()
        .find(|label| label.trial_id == NATIVE_ATTACH_TRIAL)
        .expect("attach label");
    label.disposition = SealedReviewLabelDisposition::Ambiguity;
    refresh_label_denominators(label_set);
    reseal_label_set(label_set);
}

fn tamper_wrong_replay(fixture: &mut NativeAliasFixture) {
    let output = format!("org_name,canonical_id\n{ATTACH_WITHHELD},ORG-WRONG\n");
    fs::write(&fixture.replay_output_path, &output).expect("tamper replay output");
    let mut apply: canon::entity::apply::ApplyRunArtifact = read_json(&fixture.apply_artifact_path);
    apply.output_content_hash = witness::hash_bytes(output.as_bytes());
    reseal_apply(&mut apply);
    write_json(&fixture.apply_artifact_path, &apply);
}

fn tamper_replay_registry_binding(fixture: &mut NativeAliasFixture) {
    let mut apply: canon::entity::apply::ApplyRunArtifact = read_json(&fixture.apply_artifact_path);
    apply.registry_snapshot_hash = Some(witness::hash_bytes(b"unrelated promoted registry"));
    reseal_apply(&mut apply);
    write_json(&fixture.apply_artifact_path, &apply);
}

fn tamper_replay_output_binding(fixture: &mut NativeAliasFixture) {
    let mut output = fs::read(&fixture.replay_output_path).expect("replay output");
    output.push(b'\n');
    fs::write(&fixture.replay_output_path, output).expect("tamper replay output binding");
}

fn tamper_replay_contradictory_extra_row(fixture: &mut NativeAliasFixture) {
    let output =
        format!("org_name,canonical_id\n{ATTACH_WITHHELD},ORG-001\n{ATTACH_WITHHELD},ORG-WRONG\n");
    fs::write(&fixture.replay_output_path, &output).expect("tamper replay output cardinality");
    let mut apply: canon::entity::apply::ApplyRunArtifact = read_json(&fixture.apply_artifact_path);
    apply.output_content_hash = witness::hash_bytes(output.as_bytes());
    reseal_apply(&mut apply);
    write_json(&fixture.apply_artifact_path, &apply);
}

fn tamper_path_traversal(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0].link_artifact_path = "../outside-link.json".to_string();
}

fn tamper_assignment_as_alias(fixture: &mut NativeAliasFixture) {
    let mut artifact: Value = read_json(&fixture.assignment_firewall_path);
    artifact["assignment_facts_used_as_aliases"] = json!(true);
    artifact["assignment_derived_alias_count"] = json!(1);
    write_json_value(
        &fixture.assignment_firewall_path,
        seal_value_artifact_hash(artifact),
    );
}

fn tamper_direct_leakage(fixture: &mut NativeAliasFixture) {
    let path = fixture
        .leakage_paths
        .get(&LeakChannel::MappingFile)
        .expect("mapping leakage path");
    let mut artifact: Value = read_json(path);
    let binding_hash = artifact["checked_sources"][0]["binding_hash"]
        .as_str()
        .expect("mapping source binding")
        .to_string();
    let leaked_source = fixture.base_dir.join("leakage/direct-leak-source.txt");
    fs::write(&leaked_source, ATTACH_WITHHELD).expect("tamper leakage source");
    artifact["checked_sources"][0] =
        checked_source_descriptor(&fixture.base_dir, &leaked_source, &binding_hash);
    write_json_value(path, seal_value_artifact_hash(artifact));
}

fn tamper_empty_retained_clean_registry(fixture: &mut NativeAliasFixture) {
    write_registry_with_entries(&fixture.clean_attach_dir, "1.0.0", &[]);
}

fn tamper_wrong_retained_clean_registry(fixture: &mut NativeAliasFixture) {
    write_registry_with_entries(
        &fixture.clean_attach_dir,
        "1.0.0",
        &[("Arcadia Lab", "ORG-WRONG", "RET_Attach")],
    );
}

fn tamper_forged_add_entry_receipt(fixture: &mut NativeAliasFixture) {
    let mut receipt: Value = read_json(&fixture.add_entry_receipt_path);
    receipt["registry"]["id"] = json!("forged-registry");
    write_json_value(&fixture.add_entry_receipt_path, receipt);
}

fn tamper_unrelated_leakage_clearances(fixture: &mut NativeAliasFixture) {
    for path in fixture.leakage_paths.values() {
        let mut artifact: Value = read_json(path);
        artifact["trial_id"] = json!("trial.unrelated");
        write_json_value(path, seal_value_artifact_hash(artifact));
    }
}

fn tamper_zero_source_assignment_firewall(fixture: &mut NativeAliasFixture) {
    let mut artifact: Value = read_json(&fixture.assignment_firewall_path);
    artifact["checked_sources"] = json!([]);
    write_json_value(
        &fixture.assignment_firewall_path,
        seal_value_artifact_hash(artifact),
    );
}

fn cli_tamper_path_traversal(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0].link_artifact_path = "../outside-link.json".to_string();
}

fn cli_tamper_absolute_path(fixture: &mut NativeAliasFixture) {
    fixture.manifests[0].link_artifact_path = fixture
        .base_dir
        .join(&fixture.manifests[0].link_artifact_path)
        .to_str()
        .expect("absolute link path utf-8")
        .to_string();
}

fn cli_tamper_candidate_report(fixture: &mut NativeAliasFixture) {
    tamper_candidate_report(fixture);
}

fn report_for<'a>(
    report: &'a canon::evaluation::alias_withholding::AliasWithholdingReport,
    trial_id: &str,
) -> &'a canon::evaluation::alias_withholding::AliasWithholdingTrialReport {
    report
        .trials
        .iter()
        .find(|trial| trial.trial_id == trial_id)
        .unwrap_or_else(|| panic!("missing trial {trial_id}"))
}

fn review_id_for(review: &ReviewQueueArtifact, surface_id: &str) -> String {
    review
        .review_items
        .iter()
        .find(|item| {
            item.surface_ids
                .iter()
                .any(|candidate| candidate == surface_id)
        })
        .unwrap_or_else(|| panic!("missing review item for {surface_id}"))
        .review_id
        .clone()
}

fn replace_stage_hash(run: &mut EntityRunArtifact, version: &str, hash: &str) {
    let stage = run
        .stage_artifacts
        .iter_mut()
        .find(|stage| stage.version == version)
        .unwrap_or_else(|| panic!("missing run stage {version}"));
    stage.artifact_content_hash = hash.to_string();
}

fn replace_ref_hash(refs: &mut [EntityArtifactReference], version: &str, hash: &str) {
    let reference = refs
        .iter_mut()
        .find(|reference| reference.version == version)
        .unwrap_or_else(|| panic!("missing artifact reference {version}"));
    reference.content_hash = hash.to_string();
}

fn sync_run_metadata_upstreams(run: &mut EntityRunArtifact) {
    run.metadata.upstream_artifacts = run
        .stage_artifacts
        .iter()
        .map(|stage| EntityArtifactReference {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect();
}

#[derive(Clone, Copy)]
enum NativeLinkDecisionRefreshMode {
    DeriveFromSolve,
    PreserveExistingDecisions,
}

fn republish_mutated_native_link_fixture(
    link_path: &Path,
    run: &EntityRunArtifact,
    solve: &SolveArtifact,
    decision_refresh: NativeLinkDecisionRefreshMode,
) -> EntityLinkArtifact {
    let work_dir = link_path
        .parent()
        .and_then(|path| path.parent())
        .unwrap_or_else(|| panic!("link path must live under <work-dir>/link"));
    write_run_manifest_from_artifact(work_dir, run);
    let mut link: EntityLinkArtifact = read_json(link_path);
    let bindings =
        read_validated_entity_link_observation_surface_bindings_at_path(&link, link_path)
            .expect("link observation/surface bindings validate before republish");

    let current = open_current_stream_generation(work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
        .expect("current entity run publication stream opens");
    let mut link_records = current
        .manifest
        .files
        .iter()
        .filter(|record| record.stage == "link")
        .map(|record| {
            (
                record.logical_path.clone(),
                record.stage.clone(),
                record.version.clone(),
            )
        })
        .collect::<Vec<_>>();
    link_records.sort();
    assert_link_publication_records(&link_records);

    let mut run_files = current
        .manifest
        .files
        .iter()
        .filter(|record| record.stage != "link")
        .map(|record| {
            publication_file_from_stable(
                work_dir,
                &record.logical_path,
                &record.stage,
                &record.version,
            )
        })
        .collect::<Vec<_>>();
    run_files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));

    let mut omitted_link_paths = link_records
        .iter()
        .map(|(logical_path, _, _)| logical_path.clone())
        .collect::<Vec<_>>();
    omitted_link_paths.sort();

    let run_upstreams = run
        .stage_artifacts
        .iter()
        .map(|stage| EntityPublicationUpstreamRef {
            version: stage.version.clone(),
            content_hash: stage.artifact_content_hash.clone(),
        })
        .collect::<Vec<_>>();
    let cache_mode = current.manifest.cache_mode.clone();
    let cache_status = current.manifest.cache_status.clone();
    let cache_receipt_hash = current.manifest.cache_receipt_hash.clone();
    let request_fingerprint = test_publication_request_fingerprint(
        &current.generation_id,
        &cache_mode,
        &cache_status,
        &cache_receipt_hash,
        &run_upstreams,
        &omitted_link_paths,
        &run_files,
    );
    let run_receipt = publish_stream_patch(
        work_dir,
        EntityPublicationRequest {
            stream_id: ENTITY_RUN_PUBLICATION_STREAM_ID.to_string(),
            supersedes_generation_id: Some(current.generation_id.clone()),
            request_fingerprint,
            cache_mode,
            cache_status,
            cache_receipt_hash,
            stage_order: entity_run_publication_stage_order(),
            upstream_artifacts: run_upstreams,
            files: run_files,
            omit_logical_paths: omitted_link_paths,
        },
    )
    .expect("mutated run-stage publication patch commits");
    let committed_run_generation =
        open_current_stream_generation(work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
            .expect("mutated run-stage publication stream opens");
    assert_eq!(
        committed_run_generation.generation_id,
        run_receipt.generation_id
    );
    let committed_solve: SolveArtifact = serde_json::from_slice(
        committed_run_generation
            .read_logical_file("solve/solve.json")
            .expect("committed mutated solve is present"),
    )
    .expect("committed mutated solve parses");
    assert_eq!(
        committed_solve.artifact_content_hash, solve.artifact_content_hash,
        "committed solve bytes must match the mutated solve before link decision refresh"
    );

    let (run_ref, solve_ref) =
        refresh_link_shared_artifacts(&mut link, run, solve, &run_receipt.generation_id);
    match decision_refresh {
        NativeLinkDecisionRefreshMode::DeriveFromSolve => {
            refresh_native_link_decisions_from_solve(&mut link, &committed_solve, &bindings);
        }
        NativeLinkDecisionRefreshMode::PreserveExistingDecisions => {}
    }
    reseal_link(&mut link);
    write_json(link_path, &link);

    let link_files = link_records
        .iter()
        .map(|(logical_path, stage, version)| {
            publication_file_from_stable(work_dir, logical_path, stage, version)
        })
        .collect::<Vec<_>>();
    publish_entity_run_link_publication_patch(
        work_dir,
        &run_receipt.generation_id,
        vec![run_ref, solve_ref],
        link_files,
    )
    .expect("mutated link-stage publication patch commits");

    assert_committed_logical_matches_stable(work_dir, "run/run.json");
    assert_committed_logical_matches_stable(work_dir, "solve/solve.json");
    assert_committed_logical_matches_stable(work_dir, LINK_ARTIFACT_PATH);
    link
}

fn refresh_native_link_decisions_from_solve(
    link: &mut EntityLinkArtifact,
    solve: &SolveArtifact,
    bindings: &[EntityLinkObservationSurfaceBinding],
) {
    let target_ids = bindings
        .iter()
        .filter(|binding| binding.side == EntityLinkRole::Target)
        .map(|binding| binding.link_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        target_ids,
        BTreeSet::from([ATTACH_OBSERVATION, ABSTAIN_OBSERVATION]),
        "native fixture must classify exactly the attach and abstain targets"
    );

    let attach_reference = native_link_binding(bindings, EntityLinkRole::Reference, "ORG-001");
    let attach_target = native_link_binding(bindings, EntityLinkRole::Target, ATTACH_OBSERVATION);
    assert_eq!(
        attach_reference.surface_id, attach_target.surface_id,
        "attach fixture must prove prepared-surface collapse from bindings"
    );
    let attach_entity = solve_entity_for_surface(solve, &attach_target.surface_id);
    assert_eq!(
        attach_entity.state,
        SolveReconciliationState::ResolvedExisting
    );
    assert_eq!(attach_entity.canonical_id.as_deref(), Some("ORG-001"));
    assert_eq!(
        attach_entity.adjusted_support_score_units,
        ScoreUnits::ZERO,
        "prepared-surface collapse carries no candidate support credit"
    );
    assert!(
        attach_entity
            .surface_ids
            .iter()
            .any(|surface_id| surface_id == &attach_reference.surface_id),
        "prepared-surface collapse match must share the asserted reference surface"
    );

    let abstain_reference = native_link_binding(bindings, EntityLinkRole::Reference, "ORG-002");
    let abstain_target = native_link_binding(bindings, EntityLinkRole::Target, ABSTAIN_OBSERVATION);
    assert_ne!(
        abstain_reference.surface_id, abstain_target.surface_id,
        "abstain target must remain a distinct prepared surface"
    );
    let abstain_reference_entity = solve_entity_for_surface(solve, &abstain_reference.surface_id);
    assert_eq!(
        abstain_reference_entity.state,
        SolveReconciliationState::ResolvedExisting
    );
    assert_eq!(
        abstain_reference_entity.canonical_id.as_deref(),
        Some("ORG-002")
    );
    let unmatched_reason = if let Some(abstain_entity) =
        maybe_solve_entity_for_surface(solve, &abstain_target.surface_id)
    {
        assert_eq!(
            abstain_entity.state,
            SolveReconciliationState::PromotableNew
        );
        assert!(abstain_entity.canonical_id.is_none());
        assert!(
            !abstain_entity
                .surface_ids
                .iter()
                .any(|surface_id| surface_id == &abstain_reference.surface_id),
            "honest unmatched target must not contain the asserted incumbent reference"
        );
        assert_ne!(
            abstain_entity.component_id, abstain_reference_entity.component_id,
            "unmatched target must live outside the incumbent component"
        );
        let abstain_component_references =
            reference_link_ids_in_component(bindings, abstain_entity);
        assert!(
            abstain_component_references.is_empty(),
            "promotable-new unmatched component must not carry reference bindings"
        );
        "no_resolved_reference_surface_in_solve_component"
    } else {
        "missing_solve_component"
    };

    let matches = vec![MatchRecord {
        reference_id: "ORG-001".to_string(),
        target_id: ATTACH_OBSERVATION.to_string(),
        canonical_id: "ORG-001".to_string(),
        score: 0.0,
        assertions: vec![prepared_surface_collapse_assertion()],
        runner_up: None,
    }];
    let unmatched = vec![UnmatchedRecord {
        target_id: ABSTAIN_OBSERVATION.to_string(),
        reason: unmatched_reason.to_string(),
        best_candidate: None,
    }];
    let summary = ResolveSummary {
        target_records: link.target.row_count as usize,
        matched: matches.len(),
        unmatched: unmatched.len(),
        ambiguous: 0,
        match_rate: matches.len() as f64 / link.target.row_count as f64,
    };
    assert_eq!(summary.target_records, 2);
    assert!(summary.partition_holds());
    assert!(link.decision_artifact.gold_score.is_none());
    assert!(link.decision_artifact.write_back.is_none());

    let mut decision_artifact = EntityLinkDecisionArtifact {
        version: ENTITY_LINK_DECISIONS_VERSION.to_string(),
        artifact_content_hash: String::new(),
        strategy: link.decision_artifact.strategy.clone(),
        registry: link.decision_artifact.registry.clone(),
        reference_tape: link.decision_artifact.reference_tape.clone(),
        target_tape: link.decision_artifact.target_tape.clone(),
        summary,
        matches,
        unmatched,
        ambiguous: Vec::new(),
        conflict_warnings: Vec::new(),
        gold_score: None,
        write_back: None,
    };
    reseal_link_decision_artifact(&mut decision_artifact);
    link.summary = decision_artifact.summary.clone();
    link.decision_artifact = decision_artifact;
}

fn native_link_binding<'a>(
    bindings: &'a [EntityLinkObservationSurfaceBinding],
    side: EntityLinkRole,
    link_id: &str,
) -> &'a EntityLinkObservationSurfaceBinding {
    bindings
        .iter()
        .find(|binding| binding.side == side && binding.link_id == link_id)
        .unwrap_or_else(|| panic!("missing {side:?} link binding for {link_id}"))
}

fn solve_entity_for_surface<'a>(
    solve: &'a SolveArtifact,
    surface_id: &str,
) -> &'a canon::entity::solve::SolveEntityRecord {
    maybe_solve_entity_for_surface(solve, surface_id)
        .unwrap_or_else(|| panic!("missing solve entity for surface {surface_id}"))
}

fn maybe_solve_entity_for_surface<'a>(
    solve: &'a SolveArtifact,
    surface_id: &str,
) -> Option<&'a canon::entity::solve::SolveEntityRecord> {
    solve.entities.iter().find(|entity| {
        entity
            .surface_ids
            .iter()
            .any(|surface| surface == surface_id)
    })
}

fn reference_link_ids_in_component(
    bindings: &[EntityLinkObservationSurfaceBinding],
    entity: &canon::entity::solve::SolveEntityRecord,
) -> Vec<String> {
    let surface_ids = entity.surface_ids.iter().collect::<BTreeSet<_>>();
    let mut reference_ids = bindings
        .iter()
        .filter(|binding| binding.side == EntityLinkRole::Reference)
        .filter(|binding| surface_ids.contains(&binding.surface_id))
        .map(|binding| binding.link_id.clone())
        .collect::<Vec<_>>();
    reference_ids.sort();
    reference_ids
}

fn prepared_surface_collapse_assertion() -> AssertionResult {
    let mut detail = BTreeMap::new();
    detail.insert("candidate_credit".to_string(), Value::Bool(false));
    detail.insert(
        "surface_equality".to_string(),
        Value::String("exact_prepared_surface".to_string()),
    );
    AssertionResult {
        field_ref: "prepared_surface_id".to_string(),
        field_tgt: "prepared_surface_id".to_string(),
        op: "prepared_surface_collapse".to_string(),
        passed: true,
        score: 0.0,
        weight: 0.0,
        required: true,
        detail,
    }
}

fn reseal_link_decision_artifact(artifact: &mut EntityLinkDecisionArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.artifact_content_hash = hash_compact_json(artifact);
    let mut round_tripped: EntityLinkDecisionArtifact = serde_json::from_slice(
        &serde_json::to_vec(artifact).expect("decision artifact serializes"),
    )
    .expect("decision artifact roundtrip parses");
    round_tripped.artifact_content_hash.clear();
    round_tripped.artifact_content_hash = hash_compact_json(&round_tripped);
    *artifact = round_tripped;
}

fn write_run_manifest_from_artifact(work_dir: &Path, run: &EntityRunArtifact) {
    write_json_value(
        &work_dir.join("run/manifest.json"),
        json!({
            "version": "canon_entity_run_manifest.v0",
            "summary": &run.summary,
            "stage_artifacts": &run.stage_artifacts,
            "orchestration": &run.orchestration,
            "next_commands": &run.next_commands
        }),
    );
}

fn publication_file_from_stable(
    work_dir: &Path,
    logical_path: &str,
    stage: &str,
    version: &str,
) -> EntityPublicationFileInput {
    let stable_path = work_dir.join(logical_path);
    let bytes = fs::read(&stable_path).unwrap_or_else(|error| {
        panic!(
            "read stable publication file {}: {error}",
            stable_path.display()
        )
    });
    EntityPublicationFileInput::new(logical_path, stage, version, bytes)
}

fn assert_link_publication_records(records: &[(String, String, String)]) {
    assert!(
        !records.is_empty(),
        "fixture publication stream must carry link-stage files before republish"
    );
    let versions = records
        .iter()
        .map(|(logical_path, _, version)| (logical_path.as_str(), version.as_str()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        versions.get(LINK_ARTIFACT_PATH).copied(),
        Some(ENTITY_LINK_VERSION)
    );
    let materialized_rows = format!("link/{LINK_MATERIALIZED_ROWS_PATH}");
    assert_eq!(
        versions.get(materialized_rows.as_str()).copied(),
        Some(ENTITY_LINK_MATERIALIZED_ROWS_VERSION)
    );
    let bindings = format!("link/{LINK_OBSERVATION_SURFACE_BINDINGS_PATH}");
    assert_eq!(
        versions.get(bindings.as_str()).copied(),
        Some(ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION)
    );
    let assignment_alignment = format!("link/{LINK_ASSIGNMENT_ALIGNMENT_PATH}");
    if let Some(version) = versions.get(assignment_alignment.as_str()) {
        assert_eq!(*version, ASSIGNMENT_ALIGNMENT_VERSION);
    }
}

fn entity_run_publication_stage_order() -> Vec<String> {
    ["block", "evidence", "solve", "run", "link"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn refresh_link_shared_artifacts(
    link: &mut EntityLinkArtifact,
    run: &EntityRunArtifact,
    solve: &SolveArtifact,
    publication_generation_id: &str,
) -> (EntityArtifactReference, EntityArtifactReference) {
    let run_ref = EntityArtifactReference {
        version: run.version.clone(),
        content_hash: run.artifact_content_hash.clone(),
    };
    let solve_ref = EntityArtifactReference {
        version: solve.version.clone(),
        content_hash: solve.artifact_content_hash.clone(),
    };
    let publication_ref = EntityArtifactReference {
        version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
        content_hash: publication_generation_id.to_string(),
    };
    link.shared_run_artifact = run_ref.clone();
    link.shared_solve_artifact = solve_ref.clone();
    link.metadata.upstream_artifacts = vec![run_ref.clone(), solve_ref.clone(), publication_ref];
    link.metadata.upstream_artifacts.sort_by(|left, right| {
        left.version
            .cmp(&right.version)
            .then_with(|| left.content_hash.cmp(&right.content_hash))
    });
    (run_ref, solve_ref)
}

fn test_publication_request_fingerprint(
    parent_generation_id: &str,
    cache_mode: &str,
    cache_status: &str,
    cache_receipt_hash: &str,
    upstream_artifacts: &[EntityPublicationUpstreamRef],
    omitted_logical_paths: &[String],
    files: &[EntityPublicationFileInput],
) -> String {
    let mut file_refs = files
        .iter()
        .map(|file| {
            json!({
                "logical_path": file.logical_path.as_str(),
                "stage": file.stage.as_str(),
                "version": file.version.as_str(),
                "byte_len": file.bytes.len(),
                "content_hash": witness::hash_bytes(&file.bytes)
            })
        })
        .collect::<Vec<_>>();
    file_refs.sort_by(|left, right| {
        left["logical_path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["logical_path"].as_str().unwrap_or_default())
            .then_with(|| {
                left["stage"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["stage"].as_str().unwrap_or_default())
            })
    });

    let mut upstream_refs = upstream_artifacts
        .iter()
        .map(|reference| {
            json!({
                "version": reference.version.as_str(),
                "artifact_content_hash": reference.content_hash.as_str()
            })
        })
        .collect::<Vec<_>>();
    upstream_refs.sort_by(|left, right| {
        left["version"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["version"].as_str().unwrap_or_default())
            .then_with(|| {
                left["artifact_content_hash"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["artifact_content_hash"].as_str().unwrap_or_default())
            })
    });

    hash_compact_json(&json!({
        "version": "canon_entity_run_publication_request.v1",
        "stream_id": ENTITY_RUN_PUBLICATION_STREAM_ID,
        "supersedes_generation_id": parent_generation_id,
        "cache_mode": cache_mode,
        "cache_status": cache_status,
        "cache_receipt_hash": cache_receipt_hash,
        "upstream_artifacts": upstream_refs,
        "omit_logical_paths": omitted_logical_paths,
        "files": file_refs
    }))
}

fn assert_committed_logical_matches_stable(work_dir: &Path, logical_path: &str) {
    let current = open_current_stream_generation(work_dir, ENTITY_RUN_PUBLICATION_STREAM_ID)
        .expect("current entity run publication stream opens");
    let committed = current
        .read_logical_file(logical_path)
        .unwrap_or_else(|| panic!("committed stream missing {logical_path}"));
    let stable_path = work_dir.join(logical_path);
    let stable = fs::read(&stable_path).unwrap_or_else(|error| {
        panic!(
            "read stable publication file {}: {error}",
            stable_path.display()
        )
    });
    assert_eq!(
        committed,
        stable.as_slice(),
        "committed {logical_path} bytes must match mutated stable artifact"
    );
}

fn reseal_block(block: &mut canon::entity::block_artifact::BlockCandidateArtifact) {
    block.artifact_content_hash.clear();
    block.metadata.artifact_content_hash.clear();
    let hash = hash_compact_json(block);
    block.artifact_content_hash = hash.clone();
    block.metadata.artifact_content_hash = hash;
}

fn reseal_run(run: &mut EntityRunArtifact) {
    run.artifact_content_hash.clear();
    run.metadata.artifact_content_hash.clear();
    let hash = hash_compact_json(run);
    run.artifact_content_hash = hash.clone();
    run.metadata.artifact_content_hash = hash;
}

fn reseal_link(link: &mut EntityLinkArtifact) {
    link.artifact_content_hash.clear();
    link.metadata.artifact_content_hash.clear();
    let hash = hash_compact_json(link);
    link.artifact_content_hash = hash.clone();
    link.metadata.artifact_content_hash = hash;
}

fn reseal_audit(audit: &mut EntityAuditArtifact) {
    audit.artifact_content_hash.clear();
    audit.metadata.artifact_content_hash.clear();
    let hash = hash_compact_json(audit);
    audit.artifact_content_hash = hash.clone();
    audit.metadata.artifact_content_hash = hash;
}

fn reseal_apply(apply: &mut canon::entity::apply::ApplyRunArtifact) {
    apply.artifact_content_hash.clear();
    apply.artifact_content_hash = hash_compact_json(apply);
}

fn hash_jsonl_records<T: Serialize>(records: &[T]) -> String {
    witness::hash_bytes(&jsonl_bytes(records))
}

fn jsonl_bytes<T: Serialize>(records: &[T]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).expect("record serializes");
        bytes.push(b'\n');
    }
    bytes
}

fn hash_compact_json<T: Serialize>(value: &T) -> String {
    witness::hash_bytes(&serde_json::to_vec(value).expect("json serializes"))
}

fn seal_value_artifact_hash(artifact: Value) -> Value {
    let mut sealed = roundtrip_json_value(&artifact);
    let mut hashable = sealed.clone();
    clear_value_hash(&mut hashable, &["artifact_content_hash"]);
    clear_value_hash(&mut hashable, &["receipt_content_hash"]);
    clear_value_hash(&mut hashable, &["content_hash"]);
    clear_value_hash(&mut hashable, &["metadata", "artifact_content_hash"]);
    *sealed
        .get_mut("artifact_content_hash")
        .expect("artifact hash field") = Value::String(hash_compact_json(&hashable));
    sealed
}

fn roundtrip_json_value(value: &Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(value).expect("json serializes"))
        .expect("json roundtrip parses")
}

fn clear_value_hash(value: &mut Value, path: &[&str]) {
    if path.is_empty() {
        return;
    }
    let mut current = value;
    for key in &path[..path.len() - 1] {
        let Some(next) = current.get_mut(*key) else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut()
        && let Some(value) = object.get_mut(path[path.len() - 1])
    {
        *value = Value::String(String::new());
    }
}

fn run_alias_withholding_cli(manifest: &Path, emit: &str) -> Output {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "alias-withholding",
            "--manifest",
            path_str(manifest),
            "--emit",
            emit,
        ])
        .output()
        .expect("run alias-withholding cli")
}

fn parse_json_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout JSON parse failed: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn parse_json_stderr(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "stderr JSON parse failed: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn trial_json<'a>(report: &'a Value, trial_id: &str) -> &'a Value {
    let trial_fingerprint = witness::hash_bytes(trial_id.as_bytes());
    report["trials"]
        .as_array()
        .expect("trials array")
        .iter()
        .find(|trial| trial["trial_id"] == json!(trial_fingerprint))
        .unwrap_or_else(|| panic!("missing trial {trial_id}"))
}

fn assert_public_output_does_not_leak_surfaces(output: &Output) {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for value in [
        ATTACH_WITHHELD,
        ABSTAIN_WITHHELD,
        "Arcadia Lab",
        "Northstar Supply",
        "Arcadia issuer",
        "Northstar issuer",
        NATIVE_ATTACH_TRIAL,
        NATIVE_ABSTAIN_TRIAL,
        ATTACH_OBSERVATION,
        ABSTAIN_OBSERVATION,
        "ORG-001",
        "ORG-002",
        "neutral-native-alias-withholding.v1",
        "neutral-registry",
        "gold.attach",
        "gold.abstain",
        "sealed.attach",
        "sealed.hard_negative",
    ] {
        assert!(
            !combined.contains(value),
            "public CLI output leaked {value:?}: {combined}"
        );
    }
}

fn workspace_file_fingerprints(root: &Path) -> BTreeMap<String, String> {
    let mut fingerprints = BTreeMap::new();
    collect_workspace_file_fingerprints(root, root, &mut fingerprints);
    fingerprints
}

fn collect_workspace_file_fingerprints(
    root: &Path,
    dir: &Path,
    fingerprints: &mut BTreeMap<String, String>,
) {
    let mut entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read dir {}: {error}", dir.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("read dir entry {}: {error}", dir.display()));
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()));
        if metadata.is_dir() {
            collect_workspace_file_fingerprints(root, &path, fingerprints);
        } else if metadata.is_file() {
            let rel_path = rel(root, &path);
            let bytes =
                fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            fingerprints.insert(rel_path, witness::hash_bytes(&bytes));
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    fs::write(path, serde_json::to_vec(value).expect("json serializes")).expect("write json");
}

fn write_json_value(path: &Path, value: Value) {
    write_json(path, &value);
}

fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) {
    fs::write(path, jsonl_bytes(records)).expect("write jsonl");
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn rel(base_dir: &Path, path: &Path) -> String {
    path.strip_prefix(base_dir)
        .unwrap_or_else(|error| {
            panic!(
                "{} not below {}: {error}",
                path.display(),
                base_dir.display()
            )
        })
        .to_str()
        .expect("relative path utf-8")
        .to_string()
}

fn line_count(path: &Path) -> u64 {
    fs::read_to_string(path)
        .expect("line-count file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u64
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path utf-8")
}

fn score(units: u32) -> ScoreUnits {
    ScoreUnits::from_scaled(units).expect("score units in range")
}
