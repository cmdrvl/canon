#![forbid(unsafe_code)]

use canon::evaluation::generalization::{
    BenchmarkPartition, CANON_GENERALIZATION_CACHE_EXECUTION_VERSION,
    CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION,
    CANON_GENERALIZATION_EXECUTION_ENVELOPE_VERSION,
    CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION, CANON_GENERALIZATION_SOLVE_POLICY_VERSION,
    CANON_GENERALIZATION_VERSION, CorpusVisibility, DatasetRole, DifficultyBand, DiscoveryDecision,
    GeneralizationBenchmark, GeneralizationErrorCode, GeneralizationLeakSourceBundleRef,
    GeneralizationLeakSourcePhase, GeneralizationNativeStageRebindRequest,
    GeneralizationQualityGateReport, GeneralizationQualityGateStatus,
    GeneralizationReleaseClaimStatus, GeneralizationReport, GeneralizationTrialFamily, LeakChannel,
    LeakageProbe, ProtectedSet, RelationClass, SourceFamily, bind_generalization_run_provenance,
    canonical_benchmark_bytes, canonical_report_bytes, compile_generalization_benchmark,
    generalization_schema_version, rebind_generalization_native_stages,
};
use canon::{
    entity::{
        CANON_ENTITY_BLOCK_BUCKET_VERSION, CANON_ENTITY_BLOCK_VERSION, CANON_ENTITY_EDGE_VERSION,
        CANON_ENTITY_PREPARE_VERSION, CANON_ENTITY_RUN_VERSION, CANON_ENTITY_SOLVE_VERSION,
        block::{
            BlockCandidateGenerationDiagnostics, BlockCandidateRecord,
            CandidateRecallEvaluationRequest, evaluate_candidate_recall,
        },
        block_artifact::{BlockCandidateArtifact, ExactBucketAssertion},
        edge::EdgeEvidenceRecord,
        edge_artifact::EdgeEvidenceArtifact,
        index_io::{CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION, INDEX_CACHE_RECEIPT_FILE},
        prepare::PreparedSurfaceRecord,
        run::{
            EntityRunArtifact, EntityRunStageArtifact,
            link::{
                ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION, ENTITY_LINK_VERSION,
                EntityLinkArtifact, EntityLinkFinalizeRequest, EntityLinkObservationSurfaceBinding,
                finalize_entity_link_artifact,
                read_validated_entity_link_observation_surface_bindings_at_path,
            },
        },
        score::ScoreUnits,
        solve::{SolveArtifact, SolveReconciliationConfig, SolveReconciliationState},
        telemetry::{
            CANON_ENTITY_CANDIDATE_RECALL_VERSION, CandidateRecallGoldPair, CandidateRecallStratum,
        },
    },
    resolve::ResolveArtifact,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const BENCHMARK_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/time_forward/generalization_benchmark.json");
const LEAKAGE_JSON: &str =
    include_str!("fixtures/extensions/neutral-domain/time_forward/leakage_controls.json");
const BENCHMARK_PATH: &str =
    "tests/fixtures/extensions/neutral-domain/time_forward/generalization_benchmark.json";
const AD_HOC_ENVELOPE_PATH: &str =
    "tests/fixtures/extensions/neutral-domain/time_forward/generalization_bad_ad_hoc_envelope.json";
const TRIAL_SOURCE_ROOT: &str = "tests/fixtures/extensions/neutral-domain/time_forward/trials";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrictCacheExecutionMode {
    DisabledBypass,
    EnabledWarmHit,
}

impl StrictCacheExecutionMode {
    fn cli_arg(self) -> &'static str {
        match self {
            Self::DisabledBypass => "disabled",
            Self::EnabledWarmHit => "enabled",
        }
    }

    fn manifest_mode(self) -> &'static str {
        match self {
            Self::DisabledBypass => "disabled_bypass",
            Self::EnabledWarmHit => "enabled_warm_hit",
        }
    }

    fn native_stage(self) -> &'static str {
        match self {
            Self::DisabledBypass => "cache_disabled",
            Self::EnabledWarmHit => "cache_enabled",
        }
    }

    fn native_mode(self) -> &'static str {
        match self {
            Self::DisabledBypass => "disabled",
            Self::EnabledWarmHit => "enabled",
        }
    }

    fn native_status(self) -> &'static str {
        match self {
            Self::DisabledBypass => "bypassed",
            Self::EnabledWarmHit => "hit",
        }
    }
}

#[derive(Debug, Deserialize)]
struct LeakageFixture {
    cases: Vec<LeakageCase>,
}

#[derive(Debug, Deserialize)]
struct LeakageCase {
    case_id: String,
    family: String,
    trial_id: String,
    channel: LeakChannel,
    protected_set: ProtectedSet,
    locator: String,
    value: String,
    expected_error: String,
}

#[test]
fn clean_public_generalization_fixture_reports_required_slices() {
    let benchmark = benchmark();
    let report = compile_generalization_benchmark(benchmark).expect("benchmark compiles");

    assert_eq!(
        generalization_schema_version(),
        CANON_GENERALIZATION_VERSION
    );
    assert_eq!(report.version, CANON_GENERALIZATION_VERSION);
    assert_eq!(report.corpus_visibility, CorpusVisibility::PublicFixture);
    assert_eq!(report.entity_disjoint.len(), 1);
    assert_eq!(report.time_forward.len(), 1);
    assert_eq!(report.aggregate.entity_disjoint_trial_count, 1);
    assert_eq!(report.aggregate.time_forward_trial_count, 1);
    assert_eq!(report.aggregate.critical_false_merge_count, 0);
    assert_eq!(report.aggregate.directional_cross_source_count, 2);
    assert!(report.aggregate.head_result_count > 0);
    assert!(report.aggregate.tail_result_count > 0);
    assert!(report.aggregate.easy_result_count > 0);
    assert!(report.aggregate.hard_result_count > 0);

    let entity = &report.entity_disjoint[0];
    assert_eq!(entity.novel_cluster_result_count, 1);
    assert_eq!(entity.correct_novel_cluster_count, 1);
    assert_eq!(entity.related_distinct_hard_negative_count, 1);
    assert_eq!(entity.critical_false_merge_count, 0);
    assert_eq!(entity.directional_cross_source_count, 1);

    let time = &report.time_forward[0];
    assert_eq!(time.cutoff, "2026-01-01");
    assert_eq!(time.evaluation_result_count, 3);
    assert_eq!(time.correct_evaluation_count, 3);
    assert_eq!(time.renamed_surface_count, 1);
    assert_eq!(time.new_entity_count, 1);
    assert_eq!(time.changed_relationship_count, 1);
    assert_eq!(time.critical_false_merge_count, 0);
}

#[test]
fn shipped_public_binary_compiles_generalization_manifest() {
    let first = TempGeneralizationScaffold::new();
    first.build_strict_manifest();
    let first_output = run_generalization_cli_path(&first.manifest_path, "json");
    let first_stdout = sanitized_strict_stdout_bytes(&first_output);
    let first_report = assert_strict_generalization_report(&first_output);

    let first_repeat_output = run_generalization_cli_path(&first.manifest_path, "json");
    let first_repeat_stdout = sanitized_strict_stdout_bytes(&first_repeat_output);
    let first_repeat_report = assert_strict_generalization_report(&first_repeat_output);
    assert_eq!(
        first_stdout, first_repeat_stdout,
        "same generated strict manifest should produce byte-identical stdout"
    );
    assert_eq!(
        first_report, first_repeat_report,
        "same generated strict manifest should produce identical report JSON"
    );

    let second = TempGeneralizationScaffold::new();
    second.build_strict_manifest();
    let second_output = run_generalization_cli_path(&second.manifest_path, "json");
    let second_report = assert_strict_generalization_report(&second_output);
    let second_stdout = sanitized_strict_stdout_bytes(&second_output);

    let first_sanitized_report = sanitize_root_specific_report(&first_report, &first.root);
    let second_sanitized_report = sanitize_root_specific_report(&second_report, &second.root);
    assert_json_eq_with_path(
        &first_sanitized_report,
        &second_sanitized_report,
        "strict_report",
    );
    assert_eq!(
        sanitize_root_specific_stdout(&first_stdout, &first.root),
        sanitize_root_specific_stdout(&second_stdout, &second.root),
        "independent temp strict chains should produce byte-identical stdout after normalizing only root-specific absolute paths"
    );
}

#[test]
fn shipped_public_binary_rejects_raw_self_attested_benchmark_shape() {
    let output = run_generalization_cli(BENCHMARK_PATH, "json");
    assert_eq!(
        output.status.code(),
        Some(2),
        "raw benchmark shape should refuse through the shipped CLI\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "refusal must not emit a successful report on stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"stage\":\"generalization\"")
            && stderr.contains("\"writes_performed\":false"),
        "refusal should be sanitized and scoped to generalization: {stderr}"
    );
}

#[test]
fn source_fixtures_are_trial_specific_and_match_public_gold() {
    let benchmark = benchmark();
    let entity = &benchmark.entity_disjoint_trials[0];
    let time = &benchmark.time_forward_trials[0];

    assert_eq!(
        entity
            .observations
            .iter()
            .find(|observation| observation.observation_id == "obs.holdout.beta.target")
            .expect("beta target observation")
            .surface,
        "Beta Workshop North"
    );
    assert_distinct_trial_sources(
        "entity_disjoint",
        entity
            .observations
            .iter()
            .map(|observation| observation.observation_id.as_str()),
    );

    let new_result = time
        .event_results
        .iter()
        .find(|result| result.result_id == "result.new.cluster")
        .expect("new entity result");
    assert_eq!(
        new_result.observation_ids,
        vec![
            "obs.eval.new".to_string(),
            "obs.eval.new.reference".to_string()
        ]
    );
    let new_roles = time
        .observations
        .iter()
        .filter(|observation| {
            new_result
                .observation_ids
                .contains(&observation.observation_id)
        })
        .map(|observation| observation.dataset_role)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        new_roles,
        BTreeSet::from([DatasetRole::Reference, DatasetRole::Target])
    );
    assert_eq!(
        time.observations
            .iter()
            .find(|observation| observation.observation_id == "obs.eval.relationship")
            .expect("relationship control")
            .dataset_role,
        DatasetRole::Target
    );
    assert_eq!(
        time.observations
            .iter()
            .find(|observation| observation.observation_id == "obs.eval.rename")
            .expect("rename observation")
            .surface,
        "Harbor Signals"
    );
    assert_eq!(
        time.observations
            .iter()
            .find(|observation| observation.observation_id == "obs.eval.relationship")
            .expect("relationship control")
            .surface,
        "Harbor Partner Holdings"
    );
    assert_distinct_trial_sources(
        "time_forward",
        time.observations
            .iter()
            .map(|observation| observation.observation_id.as_str()),
    );

    let entity_patch = fs::read(
        "tests/fixtures/extensions/neutral-domain/time_forward/trials/entity_disjoint/source/patches/regab_firm_identity.aliases",
    )
    .expect("entity patch namespace");
    let time_patch = fs::read(
        "tests/fixtures/extensions/neutral-domain/time_forward/trials/time_forward/source/patches/regab_firm_identity.aliases",
    )
    .expect("time patch namespace");
    assert_eq!(entity_patch, b"regab_firm_identity.entity_disjoint.aliases");
    assert_eq!(time_patch, b"regab_firm_identity.time_forward.aliases");
    assert_ne!(entity_patch, time_patch);
}

#[test]
fn native_trial_sources_materialize_through_public_entity_link() {
    let temp = tempfile::tempdir().expect("native link tempdir");
    for expectation in native_link_expectations() {
        let work_dir = temp.path().join(expectation.trial_slug);
        let output = run_entity_link_cli(expectation.trial_slug, &work_dir);
        assert_native_link_output(expectation, &output);
        assert_native_trial_artifacts(expectation.trial_slug, &work_dir);
    }
}

#[test]
fn strict_manifest_tempdir_scaffold_uses_only_public_source_inputs() {
    let scaffold = TempGeneralizationScaffold::new();
    assert_eq!(
        fs::read_to_string(&scaffold.benchmark_path).expect("temp benchmark"),
        BENCHMARK_JSON
    );
    assert_eq!(
        blake3_file(&scaffold.benchmark_path),
        blake3_bytes(BENCHMARK_JSON.as_bytes())
    );
    assert!(
        !scaffold.root.join("artifacts").exists(),
        "strict scaffold must not copy stale static artifact JSON"
    );
    assert!(
        !scaffold.manifest_path.exists(),
        "strict scaffold should leave manifest generation to the native chain builder"
    );

    for trial_slug in ["entity_disjoint", "time_forward"] {
        let source = scaffold.trial_source(trial_slug);
        for relative in [
            "reference_rows.csv",
            "target_rows.csv",
            "profile/regab_firm_identity.yaml",
            "patches/regab_firm_identity.aliases",
            "link_strategy.yaml",
            "registry/registry.json",
            "registry/aliases.json",
        ] {
            assert!(
                source.join(relative).is_file(),
                "temp scaffold should copy {relative} for {trial_slug}"
            );
        }
    }
    assert_ne!(
        blake3_file(
            &scaffold
                .trial_source("entity_disjoint")
                .join("patches/regab_firm_identity.aliases")
        ),
        blake3_file(
            &scaffold
                .trial_source("time_forward")
                .join("patches/regab_firm_identity.aliases")
        ),
        "trial-specific patch namespace bytes should stay isolated"
    );
}

#[test]
fn strict_manifest_tempdir_scaffold_materializes_copied_sources() {
    let scaffold = TempGeneralizationScaffold::new();
    for expectation in native_link_expectations() {
        let work_dir = scaffold.trial_work_dir(expectation.trial_slug);
        let output = run_entity_link_cli_from_source(
            scaffold.trial_source(expectation.trial_slug),
            &work_dir,
        );
        assert_native_link_output(expectation, &output);
        assert_native_trial_artifacts(expectation.trial_slug, &work_dir);
    }
}

#[test]
fn shipped_public_binary_rejects_ad_hoc_derived_result_artifacts() {
    let output = run_generalization_cli(AD_HOC_ENVELOPE_PATH, "json");
    assert_eq!(
        output.status.code(),
        Some(2),
        "ad hoc derived_results artifact should refuse through the shipped CLI\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "refusal must not emit a successful report on stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"stage\":\"generalization\"")
            && stderr.contains("\"writes_performed\":false"),
        "refusal should be sanitized and scoped to generalization: {stderr}"
    );
}

#[test]
fn strict_generated_manifest_refuses_stale_final_artifact_bindings() {
    let stale_run = TempGeneralizationScaffold::new();
    stale_run.build_strict_manifest();
    stale_run.mutate_link_artifact("entity_disjoint", |link| {
        link.shared_run_artifact.content_hash = fake_blake3("stale-run-binding");
    });
    let output = run_generalization_cli_path(&stale_run.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let stale_solve = TempGeneralizationScaffold::new();
    stale_solve.build_strict_manifest();
    let run_hash = stale_solve.mutate_run_artifact("entity_disjoint", |run| {
        let solve_stage = run
            .stage_artifacts
            .iter_mut()
            .find(|stage| stage.stage == "solve")
            .expect("solve stage");
        solve_stage.artifact_content_hash = fake_blake3("stale-run-solve-stage");
    });
    stale_solve.mutate_link_artifact("entity_disjoint", |link| {
        link.shared_run_artifact.content_hash = run_hash;
    });
    let output = run_generalization_cli_path(&stale_solve.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_candidate_report_and_gold_endpoint_mutations() {
    let swapped_report = TempGeneralizationScaffold::new();
    swapped_report.build_strict_manifest();
    fs::copy(
        swapped_report
            .trial_work_dir("time_forward")
            .join("candidate_recall/report.json"),
        swapped_report
            .trial_work_dir("entity_disjoint")
            .join("candidate_recall/report.json"),
    )
    .expect("swap candidate report");
    swapped_report.refresh_manifest_refs(&[swapped_report
        .trial_work_dir("entity_disjoint")
        .join("candidate_recall/report.json")]);
    let output = run_generalization_cli_path(&swapped_report.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let bad_endpoint = TempGeneralizationScaffold::new();
    bad_endpoint.build_strict_manifest();
    bad_endpoint.mutate_candidate_quality_manifest("entity_disjoint", |quality| {
        let alternate_surface = quality["observations"][2]["observation_id"]
            .as_str()
            .expect("alternate quality surface")
            .to_string();
        quality["quality_harness"]["cases"][0]["left_observation_id"] =
            Value::String(alternate_surface);
    });
    let output = run_generalization_cli_path(&bad_endpoint.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_fabricated_solve_even_when_run_and_link_resealed() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.build_strict_manifest();
    scaffold.mutate_solve_artifact_consistently("entity_disjoint", |solve| {
        let entity = solve
            .entities
            .iter_mut()
            .find(|entity| entity.state != SolveReconciliationState::ResolvedExisting)
            .expect("solve entity to fabricate");
        entity.state = SolveReconciliationState::ResolvedExisting;
        entity.canonical_id = Some("ORG-FABRICATED-001".to_string());
        entity.reason = "bd_2w13_fabricated_solve_state".to_string();
    });

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_stale_edge_records() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.build_strict_manifest();
    let edge_records_path = scaffold
        .trial_work_dir("entity_disjoint")
        .join("edge/edges.jsonl");
    let mut records: Vec<Value> = read_jsonl(&edge_records_path);
    let first_hit = records[0]["hits"]
        .as_array_mut()
        .expect("edge hits")
        .first_mut()
        .expect("edge hit");
    first_hit["reason_code"] = Value::String("bd_2w13_stale_edge_record".to_string());
    write_jsonl(&edge_records_path, &records);
    scaffold.refresh_manifest_refs(&[edge_records_path]);

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_wrong_solve_policy_hash_and_config() {
    let wrong_hash = TempGeneralizationScaffold::new();
    wrong_hash.build_strict_manifest();
    wrong_hash.mutate_manifest_trial("entity_disjoint", |trial| {
        trial["solve_derivation"]["solve_policy"]["content_hash"] =
            Value::String(fake_blake3("wrong-solve-policy-hash"));
    });
    let output = run_generalization_cli_path(&wrong_hash.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let wrong_config = TempGeneralizationScaffold::new();
    wrong_config.build_strict_manifest();
    wrong_config.rewrite_all_solve_policies(SolveReconciliationConfig::escrow_only(
        ScoreUnits::from_scaled(9000).expect("valid solve threshold"),
    ));
    let output = run_generalization_cli_path(&wrong_config.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_ambient_registry_dir_sources() {
    let absolute = TempGeneralizationScaffold::new();
    absolute.build_strict_manifest();
    let absolute_registry_dir = absolute
        .trial_source("entity_disjoint")
        .join("registry")
        .canonicalize()
        .expect("absolute registry dir");
    absolute.mutate_manifest_trial("entity_disjoint", |trial| {
        trial["registry_dir"] = Value::String(absolute_registry_dir.to_string_lossy().into_owned());
    });
    let output = run_generalization_cli_path(&absolute.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let traversal = TempGeneralizationScaffold::new();
    traversal.build_strict_manifest();
    traversal.mutate_manifest_trial("entity_disjoint", |trial| {
        trial["registry_dir"] =
            Value::String("trials/entity_disjoint/source/registry/../registry".to_string());
    });
    let output = run_generalization_cli_path(&traversal.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_emits_blocked_quality_report_for_valid_false_merge() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.plant_time_forward_native_false_merge_source();
    scaffold.build_strict_manifest();

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    let report = assert_successful_generalization_report(&output);
    assert_eq!(report["aggregate"]["critical_false_merge_count"], 1);
    assert_eq!(
        report["quality"]["release_claim_status"], "blocked",
        "structurally valid critical false merges should block release claims without refusing"
    );
    let gate = json_quality_gate(&report, "critical_false_merges_max");
    assert_eq!(gate["status"], "fail");
    assert_eq!(gate["observed_value"], 1.0);
}

#[test]
fn strict_generated_manifest_refuses_planted_checked_source_leaks_for_each_channel() {
    for channel in strict_representable_leak_channels() {
        let scaffold = TempGeneralizationScaffold::new();
        if is_source_leak_channel(channel) {
            scaffold.plant_source_checked_source_leak_before_build(
                "entity_disjoint",
                channel,
                "Beta Workshop North",
            );
            scaffold.build_strict_manifest();
        } else {
            scaffold.build_strict_manifest();
            scaffold.plant_receipt_checked_source_leak(
                "entity_disjoint",
                channel,
                "Beta Workshop North",
            );
        }
        let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
        assert_strict_generalization_refusal(&output, "entity_disjoint_leak");
    }
}

#[test]
fn strict_generated_manifest_refuses_time_forward_planted_leaks_for_each_representable_channel() {
    for channel in strict_representable_leak_channels() {
        let scaffold = TempGeneralizationScaffold::new();
        if is_source_leak_channel(channel) {
            scaffold.plant_source_checked_source_leak_before_build(
                "time_forward",
                channel,
                "Harbor Signals",
            );
            scaffold.build_strict_manifest();
        } else {
            scaffold.build_strict_manifest();
            scaffold.plant_receipt_checked_source_leak("time_forward", channel, "Harbor Signals");
        }
        let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
        assert_strict_generalization_refusal(&output, "future_leakage");
    }
}

#[test]
fn strict_generated_manifest_refuses_fake_registry_completeness() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.build_strict_manifest();
    let bundle_path = scaffold
        .trial_work_dir("entity_disjoint")
        .join("leakage/leak_scan_sources.json");
    let mut bundle: Value = read_json(&bundle_path);
    let alias = leak_source_by_channel_mut(&mut bundle, LeakChannel::Alias);
    let completeness_path = scaffold.root.join(
        alias["completeness_manifest"]["path"]
            .as_str()
            .expect("completeness path"),
    );
    let mut completeness: Value = read_json(&completeness_path);
    completeness["entries"]
        .as_array_mut()
        .expect("completeness entries")
        .pop()
        .expect("entry to remove");
    write_json(&completeness_path, &completeness);
    alias["completeness_manifest"]["content_hash"] = Value::String(blake3_file(&completeness_path));
    write_json(&bundle_path, &bundle);
    scaffold.rebind_leak_bundle("entity_disjoint");

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_unsupported_loaded_solve_state() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.build_strict_manifest();
    let solve_path = scaffold
        .trial_work_dir("entity_disjoint")
        .join("solve/solve.json");
    let mut solve: Value = read_json(&solve_path);
    solve["entities"][0]["state"] = Value::String("unsupported_state".to_string());
    write_json(&solve_path, &solve);
    scaffold.refresh_manifest_refs(&[solve_path]);

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_refuses_hard_negative_and_absent_conflicts() {
    let hard_negative = TempGeneralizationScaffold::new();
    hard_negative.build_strict_manifest();
    hard_negative.mutate_trial_bindings("entity_disjoint", |bindings| {
        bindings["hard_negative_bindings"][0]["expected_false_merge"] = Value::Bool(true);
    });
    let output = run_generalization_cli_path(&hard_negative.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let absent_match = TempGeneralizationScaffold::new();
    absent_match.build_strict_manifest();
    absent_match.mutate_trial_bindings("entity_disjoint", |bindings| {
        bindings["result_bindings"][0]["solve_disposition"] = json!({ "kind": "absent" });
    });
    let output = run_generalization_cli_path(&absent_match.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_generated_manifest_semantics_are_stable_under_ref_order_perturbation() {
    let baseline = TempGeneralizationScaffold::new();
    baseline.build_strict_manifest();
    let baseline_output = run_generalization_cli_path(&baseline.manifest_path, "json");
    let baseline_report = assert_strict_generalization_report(&baseline_output);

    let shuffled = TempGeneralizationScaffold::new();
    shuffled.build_strict_manifest();
    shuffled.reverse_manifest_reference_order();
    let shuffled_output = run_generalization_cli_path(&shuffled.manifest_path, "json");
    let shuffled_report = assert_strict_generalization_report(&shuffled_output);

    assert_eq!(
        strict_domain_report_semantics(&baseline_report),
        strict_domain_report_semantics(&shuffled_report),
        "strict report semantics should not depend on manifest trial/artifact order"
    );
}

#[test]
fn strict_generated_manifest_semantics_are_stable_under_native_source_row_shuffle() {
    let baseline = TempGeneralizationScaffold::new();
    baseline.build_strict_manifest();
    let baseline_output = run_generalization_cli_path(&baseline.manifest_path, "json");
    let baseline_report = assert_strict_generalization_report(&baseline_output);

    let shuffled = TempGeneralizationScaffold::new();
    shuffled.reverse_native_source_rows();
    shuffled.build_strict_manifest();
    let shuffled_output = run_generalization_cli_path(&shuffled.manifest_path, "json");
    let shuffled_report = assert_strict_generalization_report(&shuffled_output);

    assert_eq!(
        strict_domain_report_semantics(&baseline_report),
        strict_domain_report_semantics(&shuffled_report),
        "strict report semantics should not depend on native CSV physical row order"
    );
}

#[test]
fn strict_generated_manifest_cache_disabled_and_warm_hit_have_equal_domain_semantics() {
    let disabled = TempGeneralizationScaffold::new();
    disabled.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    assert_trial_cache_execution(
        &disabled,
        "entity_disjoint",
        StrictCacheExecutionMode::DisabledBypass,
    );
    assert_trial_cache_execution(
        &disabled,
        "time_forward",
        StrictCacheExecutionMode::DisabledBypass,
    );
    let disabled_output = run_generalization_cli_path(&disabled.manifest_path, "json");
    let disabled_report = assert_strict_generalization_report(&disabled_output);

    let enabled = TempGeneralizationScaffold::new();
    enabled.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::EnabledWarmHit);
    assert_trial_cache_execution(
        &enabled,
        "entity_disjoint",
        StrictCacheExecutionMode::EnabledWarmHit,
    );
    assert_trial_cache_execution(
        &enabled,
        "time_forward",
        StrictCacheExecutionMode::EnabledWarmHit,
    );
    let enabled_output = run_generalization_cli_path(&enabled.manifest_path, "json");
    let enabled_report = assert_strict_generalization_report(&enabled_output);
    assert_cache_mode_semantic_artifacts_equal(&disabled, &enabled, "entity_disjoint");
    assert_cache_mode_semantic_artifacts_equal(&disabled, &enabled, "time_forward");

    assert_eq!(
        strict_domain_result_slices(&disabled_report),
        strict_domain_result_slices(&enabled_report),
        "disabled bypass and enabled warm-hit executions should produce identical domain report slices"
    );
}

#[test]
fn strict_generated_manifest_refuses_cache_execution_mismatches() {
    let bad_hash = TempGeneralizationScaffold::new();
    bad_hash.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    bad_hash.mutate_cache_execution("entity_disjoint", |cache| {
        cache["receipt"]["content_hash"] = Value::String(fake_blake3("wrong-cache-receipt"));
    });
    let output = run_generalization_cli_path(&bad_hash.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let bad_path = TempGeneralizationScaffold::new();
    bad_path.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    bad_path.mutate_cache_execution("entity_disjoint", |cache| {
        cache["receipt"]["path"] =
            Value::String("trials/entity_disjoint/index/cache_key.json".to_string());
    });
    let output = run_generalization_cli_path(&bad_path.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let bad_mode = TempGeneralizationScaffold::new();
    bad_mode.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    bad_mode.mutate_cache_execution("entity_disjoint", |cache| {
        cache["mode"] = Value::String("enabled_warm_hit".to_string());
    });
    let output = run_generalization_cli_path(&bad_mode.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let enabled_rebuilt = TempGeneralizationScaffold::new();
    enabled_rebuilt.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::EnabledWarmHit);
    enabled_rebuilt.mutate_cache_receipt_status("entity_disjoint", "rebuilt");
    let output = run_generalization_cli_path(&enabled_rebuilt.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    for status in ["hit", "miss", "rebuilt"] {
        let disabled_claim = TempGeneralizationScaffold::new();
        disabled_claim
            .build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
        disabled_claim.mutate_cache_receipt_status("entity_disjoint", status);
        let output = run_generalization_cli_path(&disabled_claim.manifest_path, "json");
        assert_strict_generalization_refusal(&output, "artifact_contract");
    }
}

#[test]
fn strict_generated_manifest_refuses_ad_hoc_raw_data_in_native_cache_receipt() {
    let scaffold = TempGeneralizationScaffold::new();
    scaffold.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    scaffold.mutate_cache_receipt("entity_disjoint", |receipt| {
        receipt["ad_hoc_raw_protected_value"] = Value::String("Beta Workshop North".to_string());
    });

    let output = run_generalization_cli_path(&scaffold.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");
}

#[test]
fn strict_public_and_private_corpus_refs_use_same_shipped_command_with_redaction() {
    let public = TempGeneralizationScaffold::new();
    public.build_strict_manifest();
    let public_output = run_generalization_cli_path(&public.manifest_path, "json");
    let public_report = assert_strict_generalization_report(&public_output);

    let private = TempGeneralizationScaffold::new();
    private.build_strict_manifest();
    private.mark_benchmark_private_corpus_ref();
    let private_output = run_generalization_cli_path(&private.manifest_path, "json");
    let private_report = assert_successful_generalization_report(&private_output);

    assert_eq!(private_report["corpus_visibility"], "private_corpus_ref");
    assert_redacted_b3(&private_report["corpus_ref"]);
    assert!(
        !String::from_utf8_lossy(&private_output.stdout).contains("private://"),
        "private corpus reference must not leak in shipped CLI stdout"
    );
    assert_eq!(
        private_report["quality"]["release_claim_status"],
        "eligible"
    );
    assert_eq!(private_report["aggregate"], public_report["aggregate"]);
    assert_eq!(
        strict_domain_report_semantics(&private_report)["aggregate"],
        strict_domain_report_semantics(&public_report)["aggregate"]
    );
}

#[test]
fn strict_generated_manifest_refuses_missing_timestamp_temporal_reversal_and_conflict() {
    let missing_timestamp = TempGeneralizationScaffold::new();
    missing_timestamp.build_strict_manifest();
    missing_timestamp.mutate_benchmark(|benchmark| {
        time_forward_observation_mut(benchmark, "obs.eval.rename")["observed_at"] =
            Value::String(" ".to_string());
    });
    let output = run_generalization_cli_path(&missing_timestamp.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "artifact_contract");

    let future_build_id = TempGeneralizationScaffold::new();
    future_build_id.build_strict_manifest();
    future_build_id.mutate_benchmark(|benchmark| {
        benchmark["time_forward_trials"][0]["build_observation_ids"]
            .as_array_mut()
            .expect("build observation ids")
            .push(Value::String("obs.eval.rename".to_string()));
    });
    let output = run_generalization_cli_path(&future_build_id.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "temporal_reversal");

    let build_at_cutoff = TempGeneralizationScaffold::new();
    build_at_cutoff.build_strict_manifest();
    build_at_cutoff.mutate_benchmark(|benchmark| {
        time_forward_observation_mut(benchmark, "obs.build.anchor")["observed_at"] =
            Value::String("2026-01-01".to_string());
    });
    let output = run_generalization_cli_path(&build_at_cutoff.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "temporal_reversal");

    let evaluation_at_cutoff = TempGeneralizationScaffold::new();
    evaluation_at_cutoff.build_strict_manifest();
    evaluation_at_cutoff.mutate_benchmark(|benchmark| {
        time_forward_observation_mut(benchmark, "obs.eval.rename")["observed_at"] =
            Value::String("2026-01-01".to_string());
    });
    let output = run_generalization_cli_path(&evaluation_at_cutoff.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "temporal_reversal");

    let conflicting_timestamp = TempGeneralizationScaffold::new();
    conflicting_timestamp.build_strict_manifest();
    conflicting_timestamp.mutate_benchmark(|benchmark| {
        let observations = benchmark["time_forward_trials"][0]["observations"]
            .as_array_mut()
            .expect("time-forward observations");
        let mut duplicate = observations
            .iter()
            .find(|observation| observation["observation_id"] == "obs.eval.rename")
            .expect("rename observation")
            .clone();
        duplicate["observed_at"] = Value::String("2026-04-01".to_string());
        observations.push(duplicate);
    });
    let output = run_generalization_cli_path(&conflicting_timestamp.manifest_path, "json");
    assert_strict_generalization_refusal(&output, "duplicate_record");
}

#[test]
fn public_and_private_corpus_use_same_contract() {
    let public = benchmark();
    let mut private = benchmark();
    private.corpus_visibility = CorpusVisibility::PrivateCorpusRef;
    private.corpus_ref = "private://operator-owned/time-forward".to_string();

    let public_report = compile_generalization_benchmark(public).expect("public compiles");
    let private_report = compile_generalization_benchmark(private).expect("private compiles");

    assert_eq!(
        public_report.aggregate.result_count,
        private_report.aggregate.result_count
    );
    assert_eq!(
        public_report.aggregate.directional_cross_source_count,
        private_report.aggregate.directional_cross_source_count
    );
    assert_eq!(
        private_report.corpus_visibility,
        CorpusVisibility::PrivateCorpusRef
    );
}

#[test]
fn report_slices_cover_required_axes() {
    let report = compile_generalization_benchmark(benchmark()).expect("benchmark compiles");
    let mut slice_keys = report
        .aggregate
        .strata
        .iter()
        .map(|slice| SliceKey {
            source_family: slice.key.source_family,
            relation_class: slice.key.relation_class,
            difficulty_band: slice.key.difficulty_band,
        })
        .collect::<Vec<_>>();
    slice_keys.sort();
    slice_keys.dedup();

    assert!(
        slice_keys
            .iter()
            .any(|slice| slice.source_family == SourceFamily::Reference
                && slice.relation_class == RelationClass::Hierarchy)
    );
    assert!(
        slice_keys
            .iter()
            .any(|slice| slice.source_family == SourceFamily::Target
                && slice.relation_class == RelationClass::RenameContinuity)
    );
    assert!(
        slice_keys
            .iter()
            .any(|slice| slice.relation_class == RelationClass::NewEntity
                && slice.difficulty_band == DifficultyBand::Hard)
    );
    assert!(
        slice_keys
            .iter()
            .any(|slice| slice.relation_class == RelationClass::ChangedRelationship)
    );

    let slice_result_count = report
        .aggregate
        .strata
        .iter()
        .map(|slice| slice.result_count)
        .sum::<usize>();
    assert_eq!(slice_result_count, report.aggregate.result_count);
}

#[test]
fn canonical_bytes_are_stable_across_physical_ordering() {
    let left = benchmark();
    let mut right = benchmark();
    right.entity_disjoint_trials.reverse();
    right.time_forward_trials.reverse();
    for trial in &mut right.entity_disjoint_trials {
        trial.observations.reverse();
        trial.discovery_results.reverse();
        trial.hard_negatives.reverse();
        trial.directional_links.reverse();
        trial.leakage_probes.reverse();
    }
    for trial in &mut right.time_forward_trials {
        trial.observations.reverse();
        trial.build_observation_ids.reverse();
        trial.evaluation_observation_ids.reverse();
        trial.event_results.reverse();
        trial.hard_negatives.reverse();
        trial.directional_links.reverse();
        trial.leakage_probes.reverse();
    }

    assert_eq!(
        canonical_benchmark_bytes(&left).expect("left bytes"),
        canonical_benchmark_bytes(&right).expect("right bytes")
    );

    let left_report = compile_generalization_benchmark(left).expect("left report");
    let right_report = compile_generalization_benchmark(right).expect("right report");
    assert_eq!(
        canonical_report_bytes(&left_report).expect("left report bytes"),
        canonical_report_bytes(&right_report).expect("right report bytes")
    );
}

#[test]
fn non_leaking_cache_probe_keeps_metrics_stable() {
    let clean = compile_generalization_benchmark(benchmark()).expect("clean benchmark");
    let mut cached = benchmark();
    cached.time_forward_trials[0]
        .leakage_probes
        .push(LeakageProbe {
            channel: LeakChannel::Cache,
            protected_set: ProtectedSet::FutureObservation,
            locator: "cache/cold-run-index".to_string(),
            value: "pre_cutoff_cache_key".to_string(),
        });
    let cached = compile_generalization_benchmark(cached).expect("cache probe is non-leaking");

    assert_eq!(cached.aggregate.result_count, clean.aggregate.result_count);
    assert_eq!(
        cached.aggregate.correct_count,
        clean.aggregate.correct_count
    );
    assert_eq!(
        cached.aggregate.critical_false_merge_count,
        clean.aggregate.critical_false_merge_count
    );
    assert_eq!(
        cached.aggregate.directional_cross_source_count,
        clean.aggregate.directional_cross_source_count
    );
}

#[test]
fn planted_leakage_controls_refuse_by_family() {
    let leakage: LeakageFixture = serde_json::from_str(LEAKAGE_JSON).expect("leakage parses");
    for case in leakage.cases {
        let mut benchmark = benchmark();
        let probe = LeakageProbe {
            channel: case.channel,
            protected_set: case.protected_set,
            locator: case.locator,
            value: case.value,
        };

        match case.family.as_str() {
            "entity_disjoint" => benchmark
                .entity_disjoint_trials
                .iter_mut()
                .find(|trial| trial.trial_id == case.trial_id)
                .expect("entity trial")
                .leakage_probes
                .push(probe),
            "time_forward" => benchmark
                .time_forward_trials
                .iter_mut()
                .find(|trial| trial.trial_id == case.trial_id)
                .expect("time trial")
                .leakage_probes
                .push(probe),
            family => panic!("unexpected fixture family {family}"),
        }

        let error =
            compile_generalization_benchmark(benchmark).expect_err("planted leakage should refuse");
        let expected = match case.expected_error.as_str() {
            "entity_disjoint_leak" => GeneralizationErrorCode::EntityDisjointLeak,
            "future_leakage" => GeneralizationErrorCode::FutureLeakage,
            other => panic!("unexpected expected error {other}"),
        };
        assert_eq!(error.code, expected, "{}", case.case_id);
        assert!(
            error.message.contains(case.channel.as_str()),
            "error should cite leak channel for {}: {}",
            case.case_id,
            error.message
        );
    }
}

#[test]
fn declared_result_changes_without_artifact_derivation_refuse() {
    let mut benchmark = benchmark();
    benchmark.time_forward_trials[0].event_results[0].actual_decision =
        DiscoveryDecision::FalseMerge;

    let error = compile_generalization_benchmark(benchmark)
        .expect_err("self-declared result changes need artifact derivation");
    assert_eq!(error.code, GeneralizationErrorCode::ArtifactContract);
}

#[test]
fn entity_disjoint_split_rejects_entity_overlap() {
    let mut benchmark = benchmark();
    let trial = &mut benchmark.entity_disjoint_trials[0];
    let mut leaked = trial.observations[0].clone();
    leaked.observation_id = "obs.holdout.leaked-anchor".to_string();
    leaked.partition = BenchmarkPartition::Holdout;
    trial.observations.push(leaked);

    let error = compile_generalization_benchmark(benchmark).expect_err("overlap refuses");
    assert_eq!(error.code, GeneralizationErrorCode::EntityDisjointLeak);
}

#[test]
fn temporal_reversal_controls_reject_future_build_inputs() {
    let mut benchmark = benchmark();
    let trial = &mut benchmark.time_forward_trials[0];
    trial
        .build_observation_ids
        .push("obs.eval.rename".to_string());

    let error = compile_generalization_benchmark(benchmark).expect_err("future build refuses");
    assert_eq!(error.code, GeneralizationErrorCode::TemporalReversal);
}

#[test]
fn time_forward_timestamps_are_required_and_strictly_ordered() {
    let mut missing = benchmark();
    missing.time_forward_trials[0].observations[0].observed_at = " ".to_string();
    let error = compile_generalization_benchmark(missing).expect_err("missing timestamp refuses");
    assert_eq!(error.code, GeneralizationErrorCode::ArtifactContract);

    let mut build_at_cutoff = benchmark();
    build_at_cutoff.time_forward_trials[0].observations[0].observed_at = "2026-01-01".to_string();
    let error = compile_generalization_benchmark(build_at_cutoff)
        .expect_err("build timestamp at cutoff refuses");
    assert_eq!(error.code, GeneralizationErrorCode::TemporalReversal);

    let mut evaluation_at_cutoff = benchmark();
    evaluation_at_cutoff.time_forward_trials[0].observations[1].observed_at =
        "2026-01-01".to_string();
    let error = compile_generalization_benchmark(evaluation_at_cutoff)
        .expect_err("evaluation timestamp at cutoff refuses");
    assert_eq!(error.code, GeneralizationErrorCode::TemporalReversal);
}

#[test]
fn severity_critical_false_merge_blocks_release_claims() {
    let mut benchmark = benchmark();
    benchmark.entity_disjoint_trials[0].hard_negatives[0].false_merge = true;

    let report =
        compile_generalization_benchmark(benchmark).expect("critical false merge emits report");

    assert_eq!(report.aggregate.critical_false_merge_count, 1);
    assert_eq!(
        report.quality.release_claim_status,
        GeneralizationReleaseClaimStatus::Blocked
    );
    let gate = quality_gate(&report, "critical_false_merges_max");
    assert_eq!(gate.status, GeneralizationQualityGateStatus::Fail);
    assert_eq!(gate.observed_value, Some(1.0));
}

#[test]
fn directional_cross_source_links_require_different_dataset_roles() {
    let mut benchmark = benchmark();
    let link = &mut benchmark.entity_disjoint_trials[0].directional_links[0];
    link.target_dataset_id = link.reference_dataset_id.clone();

    let error = compile_generalization_benchmark(benchmark).expect_err("bad link refuses");
    assert_eq!(error.code, GeneralizationErrorCode::DirectionalLinkContract);
}

#[test]
fn directional_cross_source_links_bind_reference_and_target_observations() {
    let mut benchmark = benchmark();
    let link = &mut benchmark.time_forward_trials[0].directional_links[0];
    link.reference_observation_id = "obs.eval.rename".to_string();

    let error = compile_generalization_benchmark(benchmark).expect_err("bad link refuses");
    assert_eq!(error.code, GeneralizationErrorCode::DirectionalLinkContract);
}

fn benchmark() -> GeneralizationBenchmark {
    serde_json::from_str(BENCHMARK_JSON).expect("generalization fixture parses")
}

fn assert_distinct_trial_sources<'a>(
    trial_slug: &str,
    expected_observation_ids: impl IntoIterator<Item = &'a str>,
) {
    let base = trial_source_dir(trial_slug);
    let mut rows = read_source_rows(&base.join("reference_rows.csv"));
    rows.extend(read_source_rows(&base.join("target_rows.csv")));

    let actual = rows
        .iter()
        .map(|row| row["source_row_id"].as_str())
        .collect::<BTreeSet<_>>();
    let expected = expected_observation_ids
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "{trial_slug} source rows match public gold"
    );

    for row in rows {
        for field in [
            "source_row_id",
            "field_name",
            "org_name",
            "dataset",
            "role_context",
            "capacity",
            "subject_role",
            "alias_surfaces_json",
            "mention_surfaces_json",
            "filing_cik",
            "accession",
        ] {
            assert!(
                row.contains_key(field),
                "{trial_slug} source row should include {field}: {row:?}"
            );
        }
        assert_eq!(row["alias_surfaces_json"], "[]");
        assert_eq!(row["mention_surfaces_json"], "[]");
    }

    let profile =
        fs::read_to_string(base.join("profile/regab_firm_identity.yaml")).expect("trial profile");
    let patch = fs::read_to_string(base.join("patches/regab_firm_identity.aliases"))
        .expect("trial patch namespace");
    assert!(
        profile.contains(&format!("aliases: {patch}")),
        "{trial_slug} profile aliases namespace should match exact patch bytes"
    );
    assert!(
        profile.contains("op: string_similarity")
            && profile.contains("view: firm_core")
            && profile.contains("metric: jaro_winkler")
            && profile.contains("min_score: \"0.9000\""),
        "{trial_slug} profile should declare calibrated native string similarity support"
    );
}

fn run_entity_link_cli(trial_slug: &str, work_dir: &Path) -> Output {
    let base = trial_source_dir(trial_slug);
    run_entity_link_cli_from_source(&base, work_dir)
}

fn run_entity_link_cli_from_source(source_dir: &Path, work_dir: &Path) -> Output {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            "entity",
            "link",
            source_dir
                .join("reference_rows.csv")
                .to_str()
                .expect("reference rows path"),
            source_dir
                .join("target_rows.csv")
                .to_str()
                .expect("target rows path"),
            "--profile",
            source_dir
                .join("profile/regab_firm_identity.yaml")
                .to_str()
                .expect("profile path"),
            "--strategy",
            source_dir
                .join("link_strategy.yaml")
                .to_str()
                .expect("strategy path"),
            "--registry",
            source_dir.join("registry").to_str().expect("registry path"),
            "--work-dir",
            work_dir.to_str().expect("work dir path"),
            "--no-witness",
            "--emit",
            "json",
        ])
        .output()
        .expect("run entity link cli")
}

fn assert_native_link_output(expectation: NativeLinkExpectation, output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(1),
        "native link fixture should be partial for {}\nstdout={}\nstderr={}",
        expectation.trial_slug,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let artifact: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("entity link emits JSON artifact");
    assert_eq!(artifact["version"], "canon_entity_link.v0");
    assert_eq!(artifact["summary"]["matched"], expectation.matched);
    assert_eq!(artifact["summary"]["unmatched"], expectation.unmatched);
    assert_eq!(
        artifact["summary"]["target_records"],
        expectation.target_records
    );
    assert_redacted_b3(&artifact["artifact_content_hash"]);
    assert_redacted_b3(&artifact["shared_run_artifact"]["content_hash"]);
    assert_redacted_b3(&artifact["shared_solve_artifact"]["content_hash"]);
}

fn assert_native_trial_artifacts(trial_slug: &str, work_dir: &Path) {
    for relative in [
        "link/combined_rows.csv",
        "link/link.json",
        "link/observation_surface_bindings.jsonl",
        "run.json",
        "block/block.json",
        "edge/edge.json",
        "solve/solve.json",
        "index/cache_key.json",
        "index/cache_receipt.json",
    ] {
        assert!(
            work_dir.join(relative).is_file(),
            "native link should write {relative} for {trial_slug}"
        );
    }
}

fn trial_source_dir(trial_slug: &str) -> PathBuf {
    Path::new(TRIAL_SOURCE_ROOT).join(trial_slug).join("source")
}

struct TempGeneralizationScaffold {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifest_path: PathBuf,
    benchmark_path: PathBuf,
    trial_sources: BTreeMap<&'static str, PathBuf>,
}

struct StrictTrialExecutionSpec {
    trial_slug: &'static str,
    trial_id: String,
    family: GeneralizationTrialFamily,
    observation_ids: Vec<String>,
}

impl TempGeneralizationScaffold {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("generalization tempdir");
        let root = temp.path().join("strict-generalization");
        fs::create_dir_all(&root).expect("strict root");
        let benchmark_path = root.join("generalization_benchmark.json");
        fs::copy(BENCHMARK_PATH, &benchmark_path).expect("copy benchmark");

        let mut trial_sources = BTreeMap::new();
        for trial_slug in ["entity_disjoint", "time_forward"] {
            let source = root.join("trials").join(trial_slug).join("source");
            copy_dir_recursive(&trial_source_dir(trial_slug), &source);
            trial_sources.insert(trial_slug, source);
        }

        Self {
            _temp: temp,
            root: root.clone(),
            manifest_path: root.join("generalization_execution_envelope.json"),
            benchmark_path,
            trial_sources,
        }
    }

    fn trial_source(&self, trial_slug: &str) -> &Path {
        self.trial_sources
            .get(trial_slug)
            .unwrap_or_else(|| panic!("missing temp source for {trial_slug}"))
    }

    fn trial_work_dir(&self, trial_slug: &str) -> PathBuf {
        self.root.join("trials").join(trial_slug)
    }

    fn mutate_benchmark(&self, mutate: impl FnOnce(&mut Value)) {
        let mut benchmark: Value = read_json(&self.benchmark_path);
        mutate(&mut benchmark);
        write_json(&self.benchmark_path, &benchmark);
        if self.manifest_path.exists() {
            self.refresh_manifest_refs(std::slice::from_ref(&self.benchmark_path));
        }
    }

    fn mark_benchmark_private_corpus_ref(&self) {
        self.mutate_benchmark(|benchmark| {
            benchmark["corpus_visibility"] = Value::String("private_corpus_ref".to_string());
            benchmark["corpus_ref"] =
                Value::String("private://operator-owned/bd-2w13/time-forward".to_string());
        });
    }

    fn reverse_native_source_rows(&self) {
        for trial_slug in ["entity_disjoint", "time_forward"] {
            for filename in ["reference_rows.csv", "target_rows.csv"] {
                rewrite_source_rows(&self.trial_source(trial_slug).join(filename), |rows| {
                    rows.reverse();
                });
            }
        }
    }

    fn plant_time_forward_native_false_merge_source(&self) {
        rewrite_source_rows(
            &self.trial_source("time_forward").join("target_rows.csv"),
            |rows| {
                let relationship = rows
                    .iter_mut()
                    .find(|row| {
                        row.get("source_row_id").map(String::as_str)
                            == Some("obs.eval.relationship")
                    })
                    .expect("time-forward relationship control row");
                relationship.insert(
                    "org_name".to_string(),
                    "Future Nova Relationship".to_string(),
                );
                relationship.insert("filing_cik".to_string(), "200101".to_string());
                relationship.insert("accession".to_string(), "tf-new-001".to_string());

                let mut unmatched = relationship.clone();
                unmatched.insert(
                    "source_row_id".to_string(),
                    "obs.bd_2w13.unmatched.control".to_string(),
                );
                unmatched.insert(
                    "org_name".to_string(),
                    "Isolated Quality Control".to_string(),
                );
                unmatched.insert("role_context".to_string(), "quality_control".to_string());
                unmatched.insert("subject_role".to_string(), "issuer".to_string());
                unmatched.insert("filing_cik".to_string(), "299999".to_string());
                unmatched.insert(
                    "accession".to_string(),
                    "tf-unmatched-quality-control".to_string(),
                );
                rows.push(unmatched);
            },
        );
    }

    fn write_benchmark_policy_digest(&self, policy_hash: &str) {
        let mut benchmark: Value = read_json(&self.benchmark_path);
        benchmark["policy_digest"] = Value::String(policy_hash.to_string());
        write_json(&self.benchmark_path, &benchmark);
    }

    fn write_trial_solve_policies(&self, solve_config: SolveReconciliationConfig) -> String {
        let entity_policy_path = self.write_solve_policy("entity_disjoint", solve_config);
        let time_policy_path = self.write_solve_policy("time_forward", solve_config);
        let entity_policy = fs::read(&entity_policy_path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", entity_policy_path.display())
        });
        let time_policy = fs::read(&time_policy_path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", time_policy_path.display())
        });
        assert_eq!(
            entity_policy, time_policy,
            "both trials must bind byte-identical solve policy files"
        );
        let policy_hash = blake3_file(&entity_policy_path);
        assert_eq!(
            blake3_file(&time_policy_path),
            policy_hash,
            "both trial policy paths should hash to the same exact bytes"
        );
        policy_hash
    }

    fn write_solve_policy(
        &self,
        trial_slug: &str,
        solve_config: SolveReconciliationConfig,
    ) -> PathBuf {
        let path = self.trial_work_dir(trial_slug).join("solve/policy.json");
        write_json(&path, &solve_policy_value(solve_config));
        path
    }

    fn build_strict_manifest(&self) {
        self.build_strict_manifest_with_cache_mode(StrictCacheExecutionMode::DisabledBypass);
    }

    fn build_strict_manifest_with_cache_mode(&self, cache_mode: StrictCacheExecutionMode) {
        let solve_config = solve_policy_config();
        let policy_hash = self.write_trial_solve_policies(solve_config);
        self.write_benchmark_policy_digest(&policy_hash);
        let benchmark = benchmark();
        let entity_trial = benchmark
            .entity_disjoint_trials
            .first()
            .expect("entity-disjoint fixture trial");
        let time_trial = benchmark
            .time_forward_trials
            .first()
            .expect("time-forward fixture trial");

        let trials = vec![
            self.build_trial_execution(
                StrictTrialExecutionSpec {
                    trial_slug: "entity_disjoint",
                    trial_id: entity_trial.trial_id.clone(),
                    family: GeneralizationTrialFamily::EntityDisjoint,
                    observation_ids: entity_trial
                        .observations
                        .iter()
                        .map(|observation| observation.observation_id.clone())
                        .collect(),
                },
                &policy_hash,
                solve_config,
                cache_mode,
            ),
            self.build_trial_execution(
                StrictTrialExecutionSpec {
                    trial_slug: "time_forward",
                    trial_id: time_trial.trial_id.clone(),
                    family: GeneralizationTrialFamily::TimeForward,
                    observation_ids: time_trial
                        .observations
                        .iter()
                        .map(|observation| observation.observation_id.clone())
                        .collect(),
                },
                &policy_hash,
                solve_config,
                cache_mode,
            ),
        ];

        let envelope = json!({
            "version": CANON_GENERALIZATION_EXECUTION_ENVELOPE_VERSION,
            "benchmark": {
                "path": self.rel_path(&self.benchmark_path),
                "content_hash": blake3_file(&self.benchmark_path),
                "role": "gold_only"
            },
            "execution": {
                "path_resolver": "crate::fs_safety::resolve_workspace_path",
                "required_refusals": [
                    "traversal",
                    "symlink",
                    "missing",
                    "stale_hash",
                    "version_mismatch",
                    "noncanonical_artifact"
                ],
                "derive_observations": true,
                "derive_candidate_ranks": true,
                "derive_evidence_lanes": true,
                "derive_hard_negative_outcomes": true,
                "recompute_leakage": true,
                "self_attested_outcomes_used": false,
                "canonical_time_parsing": true,
                "max_artifact_bytes": 10485760,
                "max_artifact_count": 64
            },
            "trials": trials
        });
        write_json(&self.manifest_path, &envelope);
    }

    fn build_trial_execution(
        &self,
        spec: StrictTrialExecutionSpec,
        policy_hash: &str,
        solve_config: SolveReconciliationConfig,
        cache_mode: StrictCacheExecutionMode,
    ) -> Value {
        let trial_slug = spec.trial_slug;
        let trial_id = spec.trial_id;
        let family = spec.family;
        let observation_ids = spec.observation_ids;
        let work_dir = self.trial_work_dir(trial_slug);
        let registry_dir = self.trial_source(trial_slug).join("registry");
        let (link_artifact, decisions) = self.run_native_link_chain(trial_slug, cache_mode);
        let solve_policy_path = work_dir.join("solve/policy.json");
        assert_eq!(
            blake3_file(&solve_policy_path),
            policy_hash,
            "trial solve policy must share the exact benchmark policy digest"
        );

        let run: EntityRunArtifact = read_json(&work_dir.join("run.json"));
        let block: BlockCandidateArtifact = read_json(&work_dir.join("block/block.json"));
        let candidates: Vec<BlockCandidateRecord> =
            read_jsonl(&work_dir.join("block/candidates.jsonl"));
        let diagnostics: BlockCandidateGenerationDiagnostics =
            read_json(&work_dir.join("block/diagnostics.json"));
        let exact_buckets: Vec<ExactBucketAssertion> =
            read_jsonl(&work_dir.join("block/exact_buckets.jsonl"));
        let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("edge/edge.json"));
        let edge_records: Vec<EdgeEvidenceRecord> = read_jsonl(&work_dir.join("edge/edges.jsonl"));
        let surfaces: Vec<PreparedSurfaceRecord> =
            read_jsonl(&work_dir.join("prepare/surfaces.jsonl"));
        let rebound = rebind_generalization_native_stages(GeneralizationNativeStageRebindRequest {
            run: &run,
            registry_dir: &registry_dir,
            block: &block,
            block_candidate_records: &candidates,
            block_diagnostics: &diagnostics,
            exact_buckets: &exact_buckets,
            edge: &edge,
            edge_records: &edge_records,
            prepared_surfaces: &surfaces,
            solve_config,
        })
        .expect("native stages rebind through V5 helper");
        write_json(&work_dir.join("solve/solve.json"), &rebound.solve);

        let (leak_scan_sources, generated_corpus_receipt) =
            self.write_leak_source_bundle(trial_slug, &work_dir, &rebound.run, cache_mode);
        let run_id = format!("bd-2w13.{trial_slug}.strict-run");
        let final_run = bind_generalization_run_provenance(
            &rebound.run,
            "neutral.generalization.public",
            &run_id,
            &trial_id,
            family,
            &leak_scan_sources,
            generated_corpus_receipt,
        )
        .expect("strict generalization provenance binds to native run");
        let final_rebound =
            rebind_generalization_native_stages(GeneralizationNativeStageRebindRequest {
                run: &final_run,
                registry_dir: &registry_dir,
                block: &block,
                block_candidate_records: &candidates,
                block_diagnostics: &diagnostics,
                exact_buckets: &exact_buckets,
                edge: &edge,
                edge_records: &edge_records,
                prepared_surfaces: &surfaces,
                solve_config,
            })
            .expect("final run remains idempotent under V6 strict rebuild");
        assert_eq!(
            final_rebound.solve, rebound.solve,
            "final run normalization must not change the rebuilt solve"
        );
        let final_run = final_rebound.run;
        write_json(&work_dir.join("run.json"), &final_run);
        assert!(
            self.trial_source(trial_slug)
                .join("link_strategy.yaml")
                .is_file(),
            "native link work directory must not remove the copied trial strategy source"
        );

        let final_link =
            self.finalize_link_artifact(trial_slug, link_artifact, &final_run, &decisions);
        let sidecar = read_validated_entity_link_observation_surface_bindings_at_path(
            &final_link,
            &work_dir.join("link/link.json"),
        )
        .expect("link observation/surface sidecar validates against final run");

        let candidate_recall = self.write_candidate_recall(
            trial_slug,
            &work_dir,
            &observation_ids,
            &sidecar,
            &candidates,
            &diagnostics,
            &exact_buckets,
        );
        let bindings = self.trial_bindings(
            trial_slug,
            &trial_id,
            &observation_ids,
            &sidecar,
            &rebound.solve,
        );
        let artifacts = vec![
            self.artifact_ref(
                &work_dir.join("candidate_recall/report.json"),
                CANON_ENTITY_CANDIDATE_RECALL_VERSION,
                "candidate_recall",
                "candidate_recall.report",
            ),
            self.artifact_ref(
                &work_dir.join("link/link.json"),
                ENTITY_LINK_VERSION,
                "link",
                "entity_link",
            ),
            self.artifact_ref(
                &work_dir.join("link/observation_surface_bindings.jsonl"),
                ENTITY_LINK_OBSERVATION_SURFACE_BINDINGS_VERSION,
                "link_observation_surface_bindings",
                "entity_link.observation_surface_bindings",
            ),
            self.artifact_ref(
                &work_dir.join("run.json"),
                CANON_ENTITY_RUN_VERSION,
                "run",
                "entity_run",
            ),
            self.artifact_ref(
                &work_dir.join("solve/solve.json"),
                CANON_ENTITY_SOLVE_VERSION,
                "solve",
                "entity_solve",
            ),
        ];

        json!({
            "trial_id": trial_id,
            "family": family,
            "registry_dir": self.rel_path(&registry_dir),
            "candidate_recall": candidate_recall,
            "solve_derivation": {
                "edge_artifact": self.typed_ref(
                    &work_dir.join("edge/edge.json"),
                    CANON_ENTITY_EDGE_VERSION
                ),
                "edge_records": self.typed_ref(
                    &work_dir.join("edge/edges.jsonl"),
                    CANON_ENTITY_EDGE_VERSION
                ),
                "prepared_surfaces": self.typed_ref(
                    &work_dir.join("prepare/surfaces.jsonl"),
                    CANON_ENTITY_PREPARE_VERSION
                ),
                "solve_policy": self.typed_ref(
                    &solve_policy_path,
                    CANON_GENERALIZATION_SOLVE_POLICY_VERSION
                )
            },
            "cache_execution": self.cache_execution_ref(&work_dir, &final_run, cache_mode),
            "artifacts": artifacts,
            "cross_bindings": {
                "benchmark_id": "neutral.generalization.public",
                "run_id": run_id,
                "policy_digest": policy_hash,
                "registry_id": final_run.metadata.registry_snapshot.id,
                "registry_version": final_run.metadata.registry_snapshot.version,
                "registry_snapshot_hash": final_run.metadata.registry_snapshot.lookup_snapshot_hash,
                "observation_namespace": format!("neutral-domain/{trial_slug}"),
                "required_identity_links": [
                    "trial_id",
                    "observation_id",
                    "surface_id",
                    "surface_binding_hash",
                    "result_id",
                    "directional_link_id",
                    "gold_pair_id",
                    "solve_disposition",
                    "component_id",
                    "run_id",
                    "policy_digest",
                    "registry_snapshot_hash"
                ]
            },
            "bindings": bindings,
            "leak_scan_sources": leak_scan_sources
        })
    }

    fn run_native_link_chain(
        &self,
        trial_slug: &str,
        cache_mode: StrictCacheExecutionMode,
    ) -> (EntityLinkArtifact, ResolveArtifact) {
        if cache_mode == StrictCacheExecutionMode::EnabledWarmHit {
            let output = self.run_native_link_command(trial_slug, cache_mode);
            assert_eq!(
                output.status.code(),
                Some(1),
                "native link cache primer should be partial for {trial_slug}\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let primed_run: EntityRunArtifact =
                read_json(&self.trial_work_dir(trial_slug).join("run.json"));
            assert_eq!(
                primed_run.summary.labels["cache_status"], "rebuilt",
                "first enabled run should materialize a cold cache receipt"
            );
        }
        let output = self.run_native_link_command(trial_slug, cache_mode);
        assert_eq!(
            output.status.code(),
            Some(1),
            "native link fixture should be partial for {trial_slug}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let artifact: EntityLinkArtifact =
            serde_json::from_slice(&output.stdout).expect("native link stdout artifact");
        let run: EntityRunArtifact = read_json(&self.trial_work_dir(trial_slug).join("run.json"));
        assert_eq!(
            run.summary.labels["cache_mode"],
            cache_mode.native_mode(),
            "native run should record requested cache mode"
        );
        assert_eq!(
            run.summary.labels["cache_status"],
            cache_mode.native_status(),
            "native run should record expected strict cache status"
        );
        let decisions: ResolveArtifact = serde_json::from_value(
            serde_json::to_value(&artifact.decision_artifact).expect("decision artifact value"),
        )
        .expect("decision artifact maps to resolve artifact");
        (artifact, decisions)
    }

    fn run_native_link_command(
        &self,
        trial_slug: &str,
        cache_mode: StrictCacheExecutionMode,
    ) -> Output {
        let source = self.trial_source(trial_slug);
        let work_dir = self.trial_work_dir(trial_slug);
        let mut command = assert_cmd::Command::new(env!("CARGO_BIN_EXE_canon"));
        command
            .current_dir(&self.root)
            .args(["entity", "link"])
            .arg(self.rel_path(&source.join("reference_rows.csv")))
            .arg(self.rel_path(&source.join("target_rows.csv")))
            .args(["--profile"])
            .arg(self.rel_path(&source.join("profile/regab_firm_identity.yaml")))
            .args(["--strategy"])
            .arg(self.rel_path(&source.join("link_strategy.yaml")))
            .args(["--registry"])
            .arg(self.rel_path(&source.join("registry")))
            .args(["--work-dir"])
            .arg(self.rel_path(&work_dir))
            .args(["--cache-mode", cache_mode.cli_arg()])
            .args(["--no-witness", "--emit", "json"]);
        command.output().expect("run native entity link CLI")
    }

    #[allow(clippy::too_many_arguments)]
    fn write_candidate_recall(
        &self,
        trial_slug: &str,
        work_dir: &Path,
        observation_ids: &[String],
        sidecar: &[EntityLinkObservationSurfaceBinding],
        candidates: &[BlockCandidateRecord],
        diagnostics: &BlockCandidateGenerationDiagnostics,
        exact_buckets: &[ExactBucketAssertion],
    ) -> Value {
        let recall_dir = work_dir.join("candidate_recall");
        let surface_by_observation = surface_by_observation(sidecar, observation_ids);
        let mut surface_ids = surface_by_observation.values().cloned().collect::<Vec<_>>();
        surface_ids.sort();
        surface_ids.dedup();

        let gold_cases = trial_gold_cases(trial_slug);
        let gold_pairs = gold_cases
            .iter()
            .map(|case| {
                CandidateRecallGoldPair::new(
                    case.case_id,
                    surface_by_observation[case.left_observation_id].clone(),
                    surface_by_observation[case.right_observation_id].clone(),
                    case.stratum,
                )
            })
            .collect::<Vec<_>>();
        let quality_manifest = json!({
            "version": CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION,
            "observations": surface_ids
                .iter()
                .map(|surface_id| json!({ "observation_id": surface_id }))
                .collect::<Vec<_>>(),
            "quality_harness": {
                "cases": gold_cases
                    .iter()
                    .map(|case| {
                        json!({
                            "case_id": case.case_id,
                            "left_observation_id": surface_by_observation[case.left_observation_id],
                            "right_observation_id": surface_by_observation[case.right_observation_id],
                            "stratum": candidate_recall_stratum_name(case.stratum),
                            "label_disposition": "same_entity"
                        })
                    })
                    .collect::<Vec<_>>()
            }
        });
        let quality_path = recall_dir.join("quality_manifest.json");
        write_json(&quality_path, &quality_manifest);

        let report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
            candidate_records: candidates,
            diagnostics,
            gold_pairs: &gold_pairs,
            surface_ids: &surface_ids,
            exact_bucket_count: exact_buckets.len() as u64,
        });
        let report_path = recall_dir.join("report.json");
        write_json(&report_path, &report);

        json!({
            "quality_manifest": self.typed_ref(
                &quality_path,
                CANON_GENERALIZATION_CANDIDATE_RECALL_QUALITY_MANIFEST_VERSION
            ),
            "block_artifact": self.typed_ref(
                &work_dir.join("block/block.json"),
                CANON_ENTITY_BLOCK_VERSION
            ),
            "candidates": self.typed_ref(
                &work_dir.join("block/candidates.jsonl"),
                CANON_ENTITY_BLOCK_VERSION
            ),
            "diagnostics": self.typed_ref(
                &work_dir.join("block/diagnostics.json"),
                CANON_ENTITY_BLOCK_VERSION
            ),
            "exact_bucket_assertions": self.typed_ref(
                &work_dir.join("block/exact_buckets.jsonl"),
                CANON_ENTITY_BLOCK_BUCKET_VERSION
            ),
            "report": self.typed_ref(&report_path, CANON_ENTITY_CANDIDATE_RECALL_VERSION),
            "exact_bucket_count": exact_buckets.len() as u64
        })
    }

    fn write_leak_source_bundle(
        &self,
        trial_slug: &str,
        work_dir: &Path,
        run: &EntityRunArtifact,
        cache_mode: StrictCacheExecutionMode,
    ) -> (GeneralizationLeakSourceBundleRef, EntityRunStageArtifact) {
        let generated_path = work_dir.join("generated/corpus_receipt.json");
        let generated_receipt_payload = json!({
            "version": "canon.evaluation.generalization.generated_corpus_receipt.v0",
            "records": [
                {
                    "receipt_id": format!("bd-2w13.{trial_slug}.generated_corpus"),
                    "phase": "pre_evaluation",
                    "source": "neutral_public_fixture_builder",
                    "status": "safe_influence_receipt_only"
                }
            ]
        });
        write_json(&generated_path, &generated_receipt_payload);

        let cache_stage = self.native_cache_stage(run, cache_mode);
        assert_eq!(
            cache_stage.version,
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
        );
        assert_eq!(cache_stage.path, INDEX_CACHE_RECEIPT_FILE);
        let cache_path = work_dir.join(&cache_stage.path);
        let cache_hash = blake3_file(&cache_path);
        assert_eq!(
            cache_hash, cache_stage.artifact_content_hash,
            "native cache receipt bytes must match the run cache stage hash"
        );
        let generated_hash = blake3_file(&generated_path);
        let registry_paths = [
            self.trial_source(trial_slug).join("registry/aliases.json"),
            self.trial_source(trial_slug).join("registry/registry.json"),
        ];
        let registry_binding_hash = registry_tree_binding_hash(&registry_paths);
        assert_eq!(
            registry_binding_hash, run.metadata.registry_snapshot.lookup_snapshot_hash,
            "registry leak binding should match native registry snapshot"
        );

        let registry_checked_sources = registry_paths
            .iter()
            .map(|path| self.checked_source(path, "json"))
            .collect::<Vec<_>>();
        let registry_completeness = json!({
            "version": CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION,
            "coverage": "complete_registry_tree",
            "root": self.rel_path(&self.trial_source(trial_slug).join("registry")),
            "entries": registry_checked_sources
        });
        let registry_completeness_path = work_dir.join("leakage/registry_completeness.json");
        write_json(&registry_completeness_path, &registry_completeness);
        let registry_completeness_ref = json!({
            "path": self.rel_path(&registry_completeness_path),
            "content_hash": blake3_file(&registry_completeness_path)
        });

        let profile_hash = run
            .metadata
            .profile
            .content_hash
            .clone()
            .expect("native profile hash");
        let patch_hash = run
            .metadata
            .patch_set
            .as_ref()
            .expect("native patch set")
            .content_hash
            .clone();
        let sources = vec![
            self.leak_source(
                "alias.complete_registry_tree",
                LeakChannel::Alias,
                "registry_tree",
                "registry_snapshot",
                &registry_binding_hash,
                "complete_registry_tree",
                &registry_paths,
                "json",
                Some(registry_completeness_ref.clone()),
            ),
            self.leak_source(
                "anchor.complete_registry_tree",
                LeakChannel::Anchor,
                "registry_tree",
                "registry_snapshot",
                &registry_binding_hash,
                "complete_registry_tree",
                &registry_paths,
                "json",
                Some(registry_completeness_ref),
            ),
            self.leak_source(
                "threshold.profile",
                LeakChannel::Threshold,
                "threshold",
                "profile",
                &profile_hash,
                "complete_source",
                &[self
                    .trial_source(trial_slug)
                    .join("profile/regab_firm_identity.yaml")],
                "text",
                None,
            ),
            self.leak_source(
                "dictionary.strategy",
                LeakChannel::Dictionary,
                "dictionary",
                "strategy",
                &run.metadata.strategy.content_hash,
                "complete_source",
                &[self.trial_source(trial_slug).join("link_strategy.yaml")],
                "text",
                None,
            ),
            self.leak_source(
                "patch.alias_namespace",
                LeakChannel::Patch,
                "patch",
                "patch_set",
                &patch_hash,
                "complete_source",
                &[self
                    .trial_source(trial_slug)
                    .join("patches/regab_firm_identity.aliases")],
                "text",
                None,
            ),
            self.leak_source(
                "cache.native_index_receipt",
                LeakChannel::Cache,
                "cache",
                "run_stage_artifact",
                &cache_hash,
                "complete_source",
                std::slice::from_ref(&cache_path),
                "json",
                None,
            ),
            self.leak_source(
                "generated_corpus.safe_receipt",
                LeakChannel::GeneratedCorpus,
                "generated_corpus",
                "run_stage_artifact",
                &generated_hash,
                "complete_source",
                std::slice::from_ref(&generated_path),
                "json",
                None,
            ),
        ];
        let channels = leak_channels();
        let bundle = json!({
            "version": CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION,
            "scope": "pre_evaluation_influence_only",
            "channels": channels,
            "sources": sources
        });
        let bundle_path = work_dir.join("leakage/leak_scan_sources.json");
        write_json(&bundle_path, &bundle);
        let leak_ref = GeneralizationLeakSourceBundleRef {
            version: CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION.to_string(),
            phase: GeneralizationLeakSourcePhase::PreEvaluationInfluence,
            channels,
            path: self.rel_path(&bundle_path),
            content_hash: blake3_file(&bundle_path),
        };
        let leak_upstream = vec![canon::entity::EntityArtifactReference {
            version: leak_ref.version.clone(),
            content_hash: leak_ref.content_hash.clone(),
        }];
        let generated_corpus_receipt = EntityRunStageArtifact {
            stage: "generated_corpus_receipt".to_string(),
            version: "canon.evaluation.generalization.generated_corpus_receipt.v0".to_string(),
            path: "generated/corpus_receipt.json".to_string(),
            artifact_content_hash: generated_hash,
            upstream_artifacts: leak_upstream,
        };
        (leak_ref, generated_corpus_receipt)
    }

    fn cache_execution_ref(
        &self,
        work_dir: &Path,
        run: &EntityRunArtifact,
        cache_mode: StrictCacheExecutionMode,
    ) -> Value {
        let cache_stage = self.native_cache_stage(run, cache_mode);
        assert_eq!(
            cache_stage.version,
            CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
        );
        assert_eq!(cache_stage.path, INDEX_CACHE_RECEIPT_FILE);
        assert_eq!(
            run.summary.labels["cache_mode"],
            cache_mode.native_mode(),
            "final run should preserve native cache mode"
        );
        assert_eq!(
            run.summary.labels["cache_status"],
            cache_mode.native_status(),
            "final run should preserve strict cache status"
        );
        assert_eq!(
            run.summary.labels["cache_receipt_path"],
            INDEX_CACHE_RECEIPT_FILE
        );
        assert_eq!(
            run.summary.labels["cache_receipt_hash"],
            cache_stage.artifact_content_hash
        );
        let receipt_path = work_dir.join(&cache_stage.path);
        assert_eq!(
            blake3_file(&receipt_path),
            cache_stage.artifact_content_hash,
            "cache execution receipt ref must bind exact native receipt bytes"
        );
        json!({
            "version": CANON_GENERALIZATION_CACHE_EXECUTION_VERSION,
            "mode": cache_mode.manifest_mode(),
            "receipt": self.typed_ref(&receipt_path, CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION)
        })
    }

    fn native_cache_stage<'a>(
        &self,
        run: &'a EntityRunArtifact,
        cache_mode: StrictCacheExecutionMode,
    ) -> &'a EntityRunStageArtifact {
        let stage = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == cache_mode.native_stage())
            .unwrap_or_else(|| panic!("missing native {} stage", cache_mode.native_stage()));
        assert_eq!(stage.path, INDEX_CACHE_RECEIPT_FILE);
        stage
    }

    #[allow(clippy::too_many_arguments)]
    fn leak_source(
        &self,
        source_id: &str,
        channel: LeakChannel,
        source_kind: &str,
        binding_kind: &str,
        binding_hash: &str,
        coverage: &str,
        paths: &[PathBuf],
        format: &str,
        completeness_manifest: Option<Value>,
    ) -> Value {
        let mut checked_sources = Vec::new();
        let mut records = Vec::new();
        for path in paths {
            let (checked_source, mut checked_records) =
                self.checked_source_with_records(path, format);
            checked_sources.push(checked_source);
            records.append(&mut checked_records);
        }
        let mut source = json!({
            "source_id": source_id,
            "phase": GeneralizationLeakSourcePhase::PreEvaluationInfluence,
            "channel": channel,
            "source_kind": source_kind,
            "binding_kind": binding_kind,
            "binding_hash": binding_hash,
            "coverage": coverage,
            "content_hash": blake3_serialized(&records),
            "content_hash_basis": "canonical_inline_records",
            "protected_match_derivation": "derive_from_checked_sources",
            "checked_sources": checked_sources,
            "records": records
        });
        if let Some(completeness_manifest) = completeness_manifest {
            source["completeness_manifest"] = completeness_manifest;
        }
        source
    }

    fn checked_source(&self, path: &Path, format: &str) -> Value {
        self.checked_source_with_records(path, format).0
    }

    fn checked_source_with_records(&self, path: &Path, format: &str) -> (Value, Vec<Value>) {
        let bytes = fs::read(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let records = leak_projection_records(format, &bytes);
        (
            json!({
                "path": self.rel_path(path),
                "format": format,
                "content_hash": blake3_bytes(&bytes),
                "byte_count": bytes.len() as u64,
                "record_count": records.len() as u64
            }),
            records,
        )
    }

    fn trial_bindings(
        &self,
        trial_slug: &str,
        trial_id: &str,
        observation_ids: &[String],
        sidecar: &[EntityLinkObservationSurfaceBinding],
        solve: &SolveArtifact,
    ) -> Value {
        let surface_by_observation = surface_by_observation(sidecar, observation_ids);
        let observation_bindings = observation_ids
            .iter()
            .map(|observation_id| {
                let binding = binding_for_observation(sidecar, observation_id);
                json!({
                    "trial_id": trial_id,
                    "observation_id": observation_id,
                    "surface_id": binding.surface_id,
                    "surface_binding_hash": binding.surface_binding_hash,
                    "profile_id": binding.profile_id,
                    "side": binding.side,
                    "source_row_id": binding.source_row_id.as_ref().unwrap_or(&binding.link_id)
                })
            })
            .collect::<Vec<_>>();

        let surfaces = |ids: &[&str]| {
            ids.iter()
                .map(|id| surface_by_observation[*id].clone())
                .collect::<Vec<_>>()
        };
        let absent = || json!({ "kind": "absent" });

        match trial_slug {
            "entity_disjoint" => {
                let beta = surfaces(&["obs.holdout.beta.ref", "obs.holdout.beta.target"]);
                let hierarchy =
                    surfaces(&["obs.holdout.hierarchy.left", "obs.holdout.hierarchy.right"]);
                json!({
                    "observation_bindings": observation_bindings,
                    "result_bindings": [
                        {
                            "trial_id": trial_id,
                            "result_id": "result.beta.cluster",
                            "observation_ids": ["obs.holdout.beta.ref", "obs.holdout.beta.target"],
                            "surface_ids": beta,
                            "candidate_gold_pair_id": "gold.entity.beta",
                            "candidate_pair_observation_ids": [
                                "obs.holdout.beta.ref",
                                "obs.holdout.beta.target"
                            ],
                            "solve_disposition": present_disposition(solve, &beta),
                            "expected_decision": DiscoveryDecision::ClusterNew
                        },
                        {
                            "trial_id": trial_id,
                            "result_id": "result.hierarchy.safety",
                            "observation_ids": [
                                "obs.holdout.hierarchy.left",
                                "obs.holdout.hierarchy.right"
                            ],
                            "surface_ids": hierarchy,
                            "solve_disposition": absent(),
                            "expected_decision": DiscoveryDecision::Abstain
                        }
                    ],
                    "directional_link_bindings": [
                        {
                            "trial_id": trial_id,
                            "directional_link_id": "link.beta.reference-to-target",
                            "gold_pair_id": "gold.entity.beta",
                            "reference_observation_id": "obs.holdout.beta.ref",
                            "target_observation_id": "obs.holdout.beta.target",
                            "reference_surface_id": surface_by_observation["obs.holdout.beta.ref"],
                            "target_surface_id": surface_by_observation["obs.holdout.beta.target"],
                            "solve_disposition": present_disposition(solve, &beta),
                            "expected_decision": DiscoveryDecision::ClusterNew,
                            "link_disposition": "matched"
                        }
                    ],
                    "hard_negative_bindings": [
                        {
                            "trial_id": trial_id,
                            "control_id": "hard.hierarchy.no-critical-merge",
                            "left_observation_id": "obs.holdout.hierarchy.left",
                            "right_observation_id": "obs.holdout.hierarchy.right",
                            "left_surface_id": surface_by_observation["obs.holdout.hierarchy.left"],
                            "right_surface_id": surface_by_observation["obs.holdout.hierarchy.right"],
                            "left_solve_disposition": absent(),
                            "right_solve_disposition": absent(),
                            "expected_false_merge": false,
                            "link_disposition": "unmatched"
                        }
                    ]
                })
            }
            "time_forward" => {
                let rename = surfaces(&["obs.eval.rename"]);
                let rename_pair = surfaces(&["obs.build.anchor", "obs.eval.rename"]);
                let new_pair = surfaces(&["obs.eval.new", "obs.eval.new.reference"]);
                let relationship = surfaces(&["obs.eval.relationship"]);
                let relationship_disposition = solve_disposition(solve, &relationship);
                let relationship_false_merge =
                    hard_negative_false_merge(solve, &new_pair, &relationship);
                json!({
                    "observation_bindings": observation_bindings,
                    "result_bindings": [
                        {
                            "trial_id": trial_id,
                            "result_id": "result.rename.attach",
                            "observation_ids": ["obs.eval.rename"],
                            "surface_ids": rename,
                            "candidate_gold_pair_id": "gold.time.rename",
                            "candidate_pair_observation_ids": [
                                "obs.build.anchor",
                                "obs.eval.rename"
                            ],
                            "solve_disposition": present_disposition(solve, &rename_pair),
                            "expected_decision": DiscoveryDecision::AttachExisting
                        },
                        {
                            "trial_id": trial_id,
                            "result_id": "result.new.cluster",
                            "observation_ids": ["obs.eval.new", "obs.eval.new.reference"],
                            "surface_ids": new_pair,
                            "candidate_gold_pair_id": "gold.time.new",
                            "candidate_pair_observation_ids": [
                                "obs.eval.new.reference",
                                "obs.eval.new"
                            ],
                            "solve_disposition": present_disposition(solve, &new_pair),
                            "expected_decision": DiscoveryDecision::ClusterNew
                        },
                        {
                            "trial_id": trial_id,
                            "result_id": "result.relationship.abstain",
                            "observation_ids": ["obs.eval.relationship"],
                            "surface_ids": relationship,
                            "solve_disposition": relationship_disposition.clone(),
                            "expected_decision": DiscoveryDecision::Abstain
                        }
                    ],
                    "directional_link_bindings": [
                        {
                            "trial_id": trial_id,
                            "directional_link_id": "link.rename.target-to-reference",
                            "gold_pair_id": "gold.time.rename",
                            "reference_observation_id": "obs.build.anchor",
                            "target_observation_id": "obs.eval.rename",
                            "reference_surface_id": surface_by_observation["obs.build.anchor"],
                            "target_surface_id": surface_by_observation["obs.eval.rename"],
                            "solve_disposition": present_disposition(solve, &rename_pair),
                            "expected_decision": DiscoveryDecision::AttachExisting,
                            "link_disposition": "matched"
                        }
                    ],
                    "hard_negative_bindings": [
                        {
                            "trial_id": trial_id,
                            "control_id": "hard.lookalike.time-forward",
                            "left_observation_id": "obs.eval.new",
                            "right_observation_id": "obs.eval.relationship",
                            "left_surface_id": surface_by_observation["obs.eval.new"],
                            "right_surface_id": surface_by_observation["obs.eval.relationship"],
                            "left_solve_disposition": present_disposition(solve, &new_pair),
                            "right_solve_disposition": solve_disposition(solve, &relationship),
                            "expected_false_merge": relationship_false_merge
                        }
                    ]
                })
            }
            other => panic!("unexpected trial slug {other}"),
        }
    }

    fn mutate_link_artifact(
        &self,
        trial_slug: &str,
        mutate: impl FnOnce(&mut EntityLinkArtifact),
    ) -> String {
        let path = self.trial_work_dir(trial_slug).join("link/link.json");
        let mut artifact: EntityLinkArtifact = read_json(&path);
        mutate(&mut artifact);
        reseal_link_artifact(&mut artifact);
        let content_hash = artifact.artifact_content_hash.clone();
        write_json(&path, &artifact);
        self.refresh_manifest_refs(&[path]);
        content_hash
    }

    fn mutate_run_artifact(
        &self,
        trial_slug: &str,
        mutate: impl FnOnce(&mut EntityRunArtifact),
    ) -> String {
        let path = self.trial_work_dir(trial_slug).join("run.json");
        let mut artifact: EntityRunArtifact = read_json(&path);
        mutate(&mut artifact);
        reseal_run_artifact(&mut artifact);
        let content_hash = artifact.artifact_content_hash.clone();
        write_json(&path, &artifact);
        self.refresh_manifest_refs(&[path]);
        content_hash
    }

    fn mutate_candidate_quality_manifest(&self, trial_slug: &str, mutate: impl FnOnce(&mut Value)) {
        let work_dir = self.trial_work_dir(trial_slug);
        let quality_path = work_dir.join("candidate_recall/quality_manifest.json");
        let report_path = work_dir.join("candidate_recall/report.json");
        let mut quality: Value = read_json(&quality_path);
        mutate(&mut quality);
        write_json(&quality_path, &quality);

        let surface_ids = quality["observations"]
            .as_array()
            .expect("quality observations")
            .iter()
            .map(|observation| {
                observation["observation_id"]
                    .as_str()
                    .expect("quality observation id")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let gold_pairs = quality["quality_harness"]["cases"]
            .as_array()
            .expect("quality cases")
            .iter()
            .map(|case| {
                CandidateRecallGoldPair::new(
                    case["case_id"].as_str().expect("case id"),
                    case["left_observation_id"]
                        .as_str()
                        .expect("left surface id"),
                    case["right_observation_id"]
                        .as_str()
                        .expect("right surface id"),
                    candidate_recall_stratum_from_name(
                        case["stratum"].as_str().expect("case stratum"),
                    ),
                )
            })
            .collect::<Vec<_>>();
        let candidates: Vec<BlockCandidateRecord> =
            read_jsonl(&work_dir.join("block/candidates.jsonl"));
        let diagnostics: BlockCandidateGenerationDiagnostics =
            read_json(&work_dir.join("block/diagnostics.json"));
        let exact_buckets: Vec<ExactBucketAssertion> =
            read_jsonl(&work_dir.join("block/exact_buckets.jsonl"));
        let report = evaluate_candidate_recall(CandidateRecallEvaluationRequest {
            candidate_records: &candidates,
            diagnostics: &diagnostics,
            gold_pairs: &gold_pairs,
            surface_ids: &surface_ids,
            exact_bucket_count: exact_buckets.len() as u64,
        });
        write_json(&report_path, &report);
        self.refresh_manifest_refs(&[quality_path, report_path]);
    }

    fn mutate_trial_bindings(&self, trial_slug: &str, mutate: impl FnOnce(&mut Value)) {
        self.mutate_manifest_trial(trial_slug, |trial| mutate(&mut trial["bindings"]));
    }

    fn mutate_manifest_trial(&self, trial_slug: &str, mutate: impl FnOnce(&mut Value)) {
        let mut manifest: Value = read_json(&self.manifest_path);
        let trial = manifest_trial_mut(&mut manifest, trial_slug);
        mutate(trial);
        write_json(&self.manifest_path, &manifest);
    }

    fn mutate_cache_execution(&self, trial_slug: &str, mutate: impl FnOnce(&mut Value)) {
        self.mutate_manifest_trial(trial_slug, |trial| mutate(&mut trial["cache_execution"]));
    }

    fn mutate_cache_receipt_status(&self, trial_slug: &str, status: &str) {
        self.mutate_cache_receipt(trial_slug, |receipt| {
            receipt["status"] = Value::String(status.to_string());
        });
    }

    fn mutate_cache_receipt(&self, trial_slug: &str, mutate: impl FnOnce(&mut Value)) {
        let mut manifest: Value = read_json(&self.manifest_path);
        let trial = manifest_trial_mut(&mut manifest, trial_slug);
        let receipt_rel = trial["cache_execution"]["receipt"]["path"]
            .as_str()
            .expect("cache receipt path")
            .to_string();
        let receipt_path = self.root.join(&receipt_rel);
        let mut receipt: Value = read_json(&receipt_path);
        mutate(&mut receipt);
        write_json(&receipt_path, &receipt);
        trial["cache_execution"]["receipt"]["content_hash"] =
            Value::String(blake3_file(&receipt_path));
        write_json(&self.manifest_path, &manifest);
    }

    fn mutate_solve_artifact_consistently(
        &self,
        trial_slug: &str,
        mutate: impl FnOnce(&mut SolveArtifact),
    ) {
        let work_dir = self.trial_work_dir(trial_slug);
        let solve_path = work_dir.join("solve/solve.json");
        let run_path = work_dir.join("run.json");
        let link_path = work_dir.join("link/link.json");
        let mut solve: SolveArtifact = read_json(&solve_path);
        mutate(&mut solve);
        reseal_solve_artifact(&mut solve);
        write_json(&solve_path, &solve);

        let mut run: EntityRunArtifact = read_json(&run_path);
        let solve_stage = run
            .stage_artifacts
            .iter_mut()
            .find(|stage| stage.stage == "solve")
            .expect("solve stage");
        solve_stage.artifact_content_hash = solve.artifact_content_hash.clone();
        refresh_run_metadata_stage_refs(&mut run);
        reseal_run_artifact(&mut run);
        write_json(&run_path, &run);

        let link: EntityLinkArtifact = read_json(&link_path);
        let decisions: ResolveArtifact = serde_json::from_value(
            serde_json::to_value(&link.decision_artifact).expect("link decision artifact value"),
        )
        .expect("link decision artifact maps to resolve artifact");
        self.finalize_link_artifact(trial_slug, link, &run, &decisions);

        self.refresh_manifest_refs(&[solve_path, run_path, link_path]);
    }

    fn rewrite_all_solve_policies(&self, solve_config: SolveReconciliationConfig) {
        let entity_policy = self.write_solve_policy("entity_disjoint", solve_config);
        let time_policy = self.write_solve_policy("time_forward", solve_config);
        let policy_hash = blake3_file(&entity_policy);
        assert_eq!(
            blake3_file(&time_policy),
            policy_hash,
            "rewritten policy files should remain byte-identical"
        );
        self.write_benchmark_policy_digest(&policy_hash);

        let mut manifest: Value = read_json(&self.manifest_path);
        for trial in manifest["trials"].as_array_mut().expect("manifest trials") {
            trial["cross_bindings"]["policy_digest"] = Value::String(policy_hash.clone());
        }
        write_json(&self.manifest_path, &manifest);
        self.refresh_manifest_refs(&[self.benchmark_path.clone(), entity_policy, time_policy]);
    }

    fn reverse_manifest_reference_order(&self) {
        let mut manifest: Value = read_json(&self.manifest_path);
        let trials = manifest["trials"].as_array_mut().expect("manifest trials");
        trials.reverse();
        for trial in trials {
            trial["artifacts"]
                .as_array_mut()
                .expect("trial artifacts")
                .reverse();
        }
        write_json(&self.manifest_path, &manifest);
    }

    fn plant_source_checked_source_leak_before_build(
        &self,
        trial_slug: &str,
        channel: LeakChannel,
        leak: &str,
    ) {
        let source = self.trial_source(trial_slug);
        match channel {
            LeakChannel::Alias | LeakChannel::Anchor => {
                write_checked_source_with_leak(&source.join("registry/aliases.json"), "json", leak);
            }
            LeakChannel::Threshold => {
                write_checked_source_with_leak(
                    &source.join("profile/regab_firm_identity.yaml"),
                    "text",
                    leak,
                );
            }
            LeakChannel::Dictionary => {
                write_checked_source_with_leak(&source.join("link_strategy.yaml"), "text", leak);
            }
            LeakChannel::Patch => {
                self.write_patch_namespace_source_leak(trial_slug, leak);
            }
            LeakChannel::Cache | LeakChannel::GeneratedCorpus => {
                panic!("{channel:?} leaks are receipt-local post-build mutations")
            }
        }
    }

    fn write_patch_namespace_source_leak(&self, trial_slug: &str, leak: &str) {
        let source = self.trial_source(trial_slug);
        let patch_path = source.join("patches/regab_firm_identity.aliases");
        let original = fs::read_to_string(&patch_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", patch_path.display()));
        let planted = format!("{original}.{leak}");
        fs::write(&patch_path, &planted)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", patch_path.display()));

        let profile_path = source.join("profile/regab_firm_identity.yaml");
        let profile = fs::read_to_string(&profile_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", profile_path.display()));
        let updated = profile.replace(
            &format!("aliases: {original}"),
            &format!("aliases: {planted}"),
        );
        assert_ne!(
            profile, updated,
            "patch leak should update the copied profile namespace"
        );
        fs::write(&profile_path, updated)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", profile_path.display()));
    }

    fn plant_receipt_checked_source_leak(
        &self,
        trial_slug: &str,
        channel: LeakChannel,
        leak: &str,
    ) {
        let (stage_name, receipt_path) = match channel {
            LeakChannel::Cache => {
                let run: EntityRunArtifact =
                    read_json(&self.trial_work_dir(trial_slug).join("run.json"));
                let stage = run
                    .stage_artifacts
                    .iter()
                    .find(|stage| {
                        matches!(stage.stage.as_str(), "cache_enabled" | "cache_disabled")
                    })
                    .expect("native cache receipt stage");
                (stage.stage.clone(), stage.path.clone())
            }
            LeakChannel::GeneratedCorpus => (
                "generated_corpus_receipt".to_string(),
                "generated/corpus_receipt.json".to_string(),
            ),
            _ => panic!("{channel:?} leaks should be planted before build_strict_manifest"),
        };
        let bundle_path = self
            .trial_work_dir(trial_slug)
            .join("leakage/leak_scan_sources.json");
        let mut bundle: Value = read_json(&bundle_path);
        let source = leak_source_by_channel_mut(&mut bundle, channel);
        let checked_sources = source["checked_sources"]
            .as_array_mut()
            .expect("checked sources");
        assert_eq!(
            checked_sources.len(),
            1,
            "receipt leak source should bind exactly one checked receipt"
        );
        let checked = checked_sources.first_mut().expect("receipt checked source");
        assert!(
            checked["path"]
                .as_str()
                .expect("checked receipt path")
                .ends_with(&receipt_path),
            "receipt checked source should bind {receipt_path}"
        );
        let leak_path = self
            .root
            .join(checked["path"].as_str().expect("checked source path"));
        let format = checked["format"]
            .as_str()
            .expect("checked source format")
            .to_string();
        write_checked_source_with_leak(&leak_path, &format, leak);
        let bytes = fs::read(&leak_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", leak_path.display()));
        let records = leak_projection_records(&format, &bytes);
        let receipt_hash = blake3_bytes(&bytes);
        checked["content_hash"] = Value::String(receipt_hash.clone());
        checked["byte_count"] = Value::Number((bytes.len() as u64).into());
        checked["record_count"] = Value::Number((records.len() as u64).into());
        source["records"] = Value::Array(records);
        source["content_hash"] = Value::String(blake3_serialized(
            source["records"].as_array().expect("source records"),
        ));
        source["binding_hash"] = Value::String(receipt_hash.clone());
        write_json(&bundle_path, &bundle);
        self.rebind_receipt_leak_bundle(trial_slug, &stage_name, &receipt_hash);
    }

    fn rebind_leak_bundle(&self, trial_slug: &str) {
        let work_dir = self.trial_work_dir(trial_slug);
        let bundle_path = work_dir.join("leakage/leak_scan_sources.json");
        let leak_bundle_hash = blake3_file(&bundle_path);

        let mut manifest: Value = read_json(&self.manifest_path);
        let trial = manifest_trial_mut(&mut manifest, trial_slug);
        trial["leak_scan_sources"]["content_hash"] = Value::String(leak_bundle_hash.clone());
        write_json(&self.manifest_path, &manifest);

        let run_path = work_dir.join("run.json");
        let link_path = work_dir.join("link/link.json");
        let mut run: EntityRunArtifact = read_json(&run_path);
        update_run_leak_bundle_refs(&mut run, &leak_bundle_hash);
        reseal_run_artifact(&mut run);
        write_json(&run_path, &run);

        let link: EntityLinkArtifact = read_json(&link_path);
        let decisions: ResolveArtifact = serde_json::from_value(
            serde_json::to_value(&link.decision_artifact).expect("link decision artifact value"),
        )
        .expect("link decision artifact maps to resolve artifact");
        self.finalize_link_artifact(trial_slug, link, &run, &decisions);

        self.refresh_manifest_refs(&[run_path, link_path, bundle_path]);
    }

    fn rebind_receipt_leak_bundle(&self, trial_slug: &str, stage_name: &str, receipt_hash: &str) {
        let work_dir = self.trial_work_dir(trial_slug);
        let bundle_path = work_dir.join("leakage/leak_scan_sources.json");
        let leak_bundle_hash = blake3_file(&bundle_path);

        let mut manifest: Value = read_json(&self.manifest_path);
        let trial = manifest_trial_mut(&mut manifest, trial_slug);
        trial["leak_scan_sources"]["content_hash"] = Value::String(leak_bundle_hash.clone());
        write_json(&self.manifest_path, &manifest);

        let solve_path = work_dir.join("solve/solve.json");
        let run_path = work_dir.join("run.json");
        let link_path = work_dir.join("link/link.json");
        let block: BlockCandidateArtifact = read_json(&work_dir.join("block/block.json"));
        let edge: EdgeEvidenceArtifact = read_json(&work_dir.join("edge/edge.json"));
        let mut run: EntityRunArtifact = read_json(&run_path);
        let link: EntityLinkArtifact = read_json(&link_path);
        let candidates: Vec<BlockCandidateRecord> =
            read_jsonl(&work_dir.join("block/candidates.jsonl"));
        let diagnostics: BlockCandidateGenerationDiagnostics =
            read_json(&work_dir.join("block/diagnostics.json"));
        let exact_buckets: Vec<ExactBucketAssertion> =
            read_jsonl(&work_dir.join("block/exact_buckets.jsonl"));
        let edge_records: Vec<EdgeEvidenceRecord> = read_jsonl(&work_dir.join("edge/edges.jsonl"));
        let surfaces: Vec<PreparedSurfaceRecord> =
            read_jsonl(&work_dir.join("prepare/surfaces.jsonl"));

        let stage = run
            .stage_artifacts
            .iter_mut()
            .find(|stage| stage.stage == stage_name)
            .unwrap_or_else(|| panic!("missing {stage_name} stage"));
        stage.artifact_content_hash = receipt_hash.to_string();
        update_run_leak_bundle_refs(&mut run, &leak_bundle_hash);
        reseal_run_artifact(&mut run);
        let registry_dir = self.trial_source(trial_slug).join("registry");
        let rebound = rebind_generalization_native_stages(GeneralizationNativeStageRebindRequest {
            run: &run,
            registry_dir: &registry_dir,
            block: &block,
            block_candidate_records: &candidates,
            block_diagnostics: &diagnostics,
            exact_buckets: &exact_buckets,
            edge: &edge,
            edge_records: &edge_records,
            prepared_surfaces: &surfaces,
            solve_config: solve_policy_config(),
        })
        .expect("leak-source mutation keeps native stages rebuildable");

        write_json(&solve_path, &rebound.solve);
        let run = rebound.run;
        write_json(&run_path, &run);

        let decisions: ResolveArtifact = serde_json::from_value(
            serde_json::to_value(&link.decision_artifact).expect("link decision artifact value"),
        )
        .expect("link decision artifact maps to resolve artifact");
        self.finalize_link_artifact(trial_slug, link, &run, &decisions);

        let receipt_path = run
            .stage_artifacts
            .iter()
            .find(|stage| stage.stage == stage_name)
            .map(|stage| work_dir.join(&stage.path))
            .unwrap_or_else(|| work_dir.join("generated/corpus_receipt.json"));
        self.refresh_manifest_refs(&[solve_path, run_path, link_path, bundle_path, receipt_path]);
    }

    fn refresh_manifest_refs(&self, paths: &[PathBuf]) {
        let mut manifest: Value = read_json(&self.manifest_path);
        for path in paths {
            let rel = self.rel_path(path);
            let hash = blake3_file(path);
            replace_content_hash_for_path(&mut manifest, &rel, &hash);
        }
        write_json(&self.manifest_path, &manifest);
    }

    fn typed_ref(&self, path: &Path, version: &str) -> Value {
        json!({
            "path": self.rel_path(path),
            "content_hash": blake3_file(path),
            "version": version
        })
    }

    fn artifact_ref(&self, path: &Path, version: &str, kind: &str, artifact_id: &str) -> Value {
        json!({
            "path": self.rel_path(path),
            "content_hash": blake3_file(path),
            "version": version,
            "kind": kind,
            "artifact_id": artifact_id
        })
    }

    fn rel_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or_else(|error| {
                panic!(
                    "{} should be under {}: {error}",
                    path.display(),
                    self.root.display()
                )
            })
            .to_str()
            .expect("fixture path is UTF-8")
            .replace('\\', "/")
    }

    fn run_with_absolute_strategy_source(
        &self,
        trial_slug: &str,
        run: &EntityRunArtifact,
    ) -> EntityRunArtifact {
        let mut run = run.clone();
        run.summary.labels.insert(
            "strategy_source".to_string(),
            self.trial_source(trial_slug)
                .join("link_strategy.yaml")
                .to_string_lossy()
                .into_owned(),
        );
        run
    }

    fn finalize_link_artifact(
        &self,
        trial_slug: &str,
        artifact: EntityLinkArtifact,
        run: &EntityRunArtifact,
        decisions: &ResolveArtifact,
    ) -> EntityLinkArtifact {
        let work_dir = self.trial_work_dir(trial_slug);
        let finalization_run = self.run_with_absolute_strategy_source(trial_slug, run);
        let mut link = finalize_entity_link_artifact(EntityLinkFinalizeRequest {
            artifact,
            run_artifact: &finalization_run,
            decisions,
            work_dir: &work_dir,
        })
        .expect("link artifact binds final run");
        link.next_commands.review_export = format!(
            "canon entity review export {} --include escrow --emit csv",
            self.rel_path(&work_dir.join("link/link.json"))
        );
        reseal_link_artifact(&mut link);
        write_json(&work_dir.join("link/link.json"), &link);
        link
    }
}

fn fake_blake3(label: &str) -> String {
    blake3_bytes(label.as_bytes())
}

fn solve_policy_config() -> SolveReconciliationConfig {
    SolveReconciliationConfig::delegate_new_ids(
        ScoreUnits::from_scaled(9000).expect("valid solve threshold"),
    )
}

fn solve_policy_value(config: SolveReconciliationConfig) -> Value {
    json!({
        "version": CANON_GENERALIZATION_SOLVE_POLICY_VERSION,
        "config": config
    })
}

fn manifest_trial_mut<'a>(manifest: &'a mut Value, trial_slug: &str) -> &'a mut Value {
    let trial_id = match trial_slug {
        "entity_disjoint" => "entity_disjoint.neutral.planted",
        "time_forward" => "time_forward.neutral.cutoff",
        other => panic!("unexpected trial slug {other}"),
    };
    manifest["trials"]
        .as_array_mut()
        .expect("manifest trials")
        .iter_mut()
        .find(|trial| trial["trial_id"].as_str() == Some(trial_id))
        .unwrap_or_else(|| panic!("missing manifest trial {trial_id}"))
}

fn manifest_trial<'a>(manifest: &'a Value, trial_slug: &str) -> &'a Value {
    let trial_id = match trial_slug {
        "entity_disjoint" => "entity_disjoint.neutral.planted",
        "time_forward" => "time_forward.neutral.cutoff",
        other => panic!("unexpected trial slug {other}"),
    };
    manifest["trials"]
        .as_array()
        .expect("manifest trials")
        .iter()
        .find(|trial| trial["trial_id"].as_str() == Some(trial_id))
        .unwrap_or_else(|| panic!("missing manifest trial {trial_id}"))
}

fn time_forward_observation_mut<'a>(
    benchmark: &'a mut Value,
    observation_id: &str,
) -> &'a mut Value {
    benchmark["time_forward_trials"][0]["observations"]
        .as_array_mut()
        .expect("time-forward observations")
        .iter_mut()
        .find(|observation| observation["observation_id"].as_str() == Some(observation_id))
        .unwrap_or_else(|| panic!("missing time-forward observation {observation_id}"))
}

fn leak_source_by_channel_mut(bundle: &mut Value, channel: LeakChannel) -> &mut Value {
    let expected = serde_json::to_value(channel).expect("serialize leak channel");
    bundle["sources"]
        .as_array_mut()
        .expect("leak sources")
        .iter_mut()
        .find(|source| source["channel"] == expected)
        .unwrap_or_else(|| panic!("missing leak source for {channel:?}"))
}

fn write_checked_source_with_leak(path: &Path, format: &str, leak: &str) {
    match format {
        "json" => {
            let mut value: Value = read_json(path);
            insert_json_leak_marker(&mut value, leak);
            write_json(path, &value);
        }
        "text" => {
            let mut text = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str("# bd-2w13 strict leak marker: ");
            text.push_str(leak);
            text.push('\n');
            fs::write(path, text)
                .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
        }
        other => panic!("unsupported leak checked source format {other}"),
    }
}

fn insert_json_leak_marker(value: &mut Value, leak: &str) {
    let marker = || {
        json!({
            "input": format!("bd-2w13 strict leak marker {leak}"),
            "canonical_id": "ORG-TUNE-001",
            "canonical_type": "org",
            "rule_id": "BD_2W13_STRICT_LEAK_MARKER"
        })
    };
    match value {
        Value::Object(object) => {
            if let Some(records) = object.get_mut("records").and_then(Value::as_array_mut) {
                records.push(marker());
            } else {
                object.insert(
                    "bd_2w13_strict_leak_marker".to_string(),
                    Value::String(leak.to_string()),
                );
            }
        }
        Value::Array(records) => records.push(marker()),
        _ => {
            *value = json!({ "bd_2w13_strict_leak_marker": leak });
        }
    }
}

fn update_run_leak_bundle_refs(run: &mut EntityRunArtifact, leak_bundle_hash: &str) {
    fn update_refs(refs: &mut Vec<canon::entity::EntityArtifactReference>, hash: &str) {
        for reference in refs {
            if reference.version == CANON_GENERALIZATION_LEAK_SCAN_SOURCES_VERSION {
                reference.content_hash = hash.to_string();
            }
        }
    }
    update_refs(&mut run.metadata.upstream_artifacts, leak_bundle_hash);
    for stage in &mut run.stage_artifacts {
        update_refs(&mut stage.upstream_artifacts, leak_bundle_hash);
    }
}

fn entity_stage_artifact_ref(
    stage: &EntityRunStageArtifact,
) -> canon::entity::EntityArtifactReference {
    canon::entity::EntityArtifactReference {
        version: stage.version.clone(),
        content_hash: stage.artifact_content_hash.clone(),
    }
}

fn refresh_run_metadata_stage_refs(run: &mut EntityRunArtifact) {
    run.metadata.upstream_artifacts = run
        .stage_artifacts
        .iter()
        .map(entity_stage_artifact_ref)
        .collect();
    run.metadata.upstream_artifacts.sort_by(|left, right| {
        left.version
            .cmp(&right.version)
            .then_with(|| left.content_hash.cmp(&right.content_hash))
    });
}

fn reseal_solve_artifact(artifact: &mut SolveArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let hash = blake3_serialized(artifact);
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
}

fn reseal_run_artifact(artifact: &mut EntityRunArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let hash = blake3_serialized(artifact);
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
}

fn reseal_link_artifact(artifact: &mut EntityLinkArtifact) {
    artifact.artifact_content_hash.clear();
    artifact.metadata.artifact_content_hash.clear();
    let hash = blake3_serialized(artifact);
    artifact.artifact_content_hash = hash.clone();
    artifact.metadata.artifact_content_hash = hash;
}

fn replace_content_hash_for_path(value: &mut Value, rel_path: &str, content_hash: &str) {
    match value {
        Value::Object(object) => {
            if object
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| path == rel_path)
                && object.contains_key("content_hash")
            {
                object.insert(
                    "content_hash".to_string(),
                    Value::String(content_hash.to_string()),
                );
            }
            for child in object.values_mut() {
                replace_content_hash_for_path(child, rel_path, content_hash);
            }
        }
        Value::Array(values) => {
            for child in values {
                replace_content_hash_for_path(child, rel_path, content_hash);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy)]
struct CandidateGoldCase {
    case_id: &'static str,
    left_observation_id: &'static str,
    right_observation_id: &'static str,
    stratum: CandidateRecallStratum,
}

fn trial_gold_cases(trial_slug: &str) -> Vec<CandidateGoldCase> {
    match trial_slug {
        "entity_disjoint" => vec![CandidateGoldCase {
            case_id: "gold.entity.beta",
            left_observation_id: "obs.holdout.beta.ref",
            right_observation_id: "obs.holdout.beta.target",
            stratum: CandidateRecallStratum::NovelCluster,
        }],
        "time_forward" => vec![
            CandidateGoldCase {
                case_id: "gold.time.rename",
                left_observation_id: "obs.build.anchor",
                right_observation_id: "obs.eval.rename",
                stratum: CandidateRecallStratum::WithheldAlias,
            },
            CandidateGoldCase {
                case_id: "gold.time.new",
                left_observation_id: "obs.eval.new.reference",
                right_observation_id: "obs.eval.new",
                stratum: CandidateRecallStratum::NovelCluster,
            },
        ],
        other => panic!("unexpected trial slug {other}"),
    }
}

fn candidate_recall_stratum_name(stratum: CandidateRecallStratum) -> &'static str {
    match stratum {
        CandidateRecallStratum::ExactKnown => "exact_known",
        CandidateRecallStratum::WithheldAlias => "withheld_alias",
        CandidateRecallStratum::NovelCluster => "novel_cluster",
        CandidateRecallStratum::DirectionalLink => "directional_link",
    }
}

fn candidate_recall_stratum_from_name(value: &str) -> CandidateRecallStratum {
    match value {
        "exact_known" => CandidateRecallStratum::ExactKnown,
        "withheld_alias" => CandidateRecallStratum::WithheldAlias,
        "novel_cluster" => CandidateRecallStratum::NovelCluster,
        "directional_link" => CandidateRecallStratum::DirectionalLink,
        other => panic!("unsupported candidate recall stratum {other}"),
    }
}

fn present_disposition(solve: &SolveArtifact, surface_ids: &[String]) -> Value {
    let component = solve
        .entities
        .iter()
        .find(|entity| {
            let entity_surfaces = entity.surface_ids.iter().collect::<BTreeSet<_>>();
            surface_ids
                .iter()
                .all(|surface_id| entity_surfaces.contains(surface_id))
        })
        .unwrap_or_else(|| panic!("missing solve component for surfaces {surface_ids:?}"));
    json!({
        "kind": "present",
        "component_id": component.component_id,
        "state": component.state
    })
}

fn absent() -> Value {
    json!({ "kind": "absent" })
}

fn solve_disposition(solve: &SolveArtifact, surface_ids: &[String]) -> Value {
    solve
        .entities
        .iter()
        .find(|entity| {
            let entity_surfaces = entity.surface_ids.iter().collect::<BTreeSet<_>>();
            surface_ids
                .iter()
                .all(|surface_id| entity_surfaces.contains(surface_id))
        })
        .map(|component| {
            json!({
                "kind": "present",
                "component_id": component.component_id,
                "state": component.state
            })
        })
        .unwrap_or_else(absent)
}

fn hard_negative_false_merge(
    solve: &SolveArtifact,
    left_surface_ids: &[String],
    right_surface_ids: &[String],
) -> bool {
    let component_for = |surface_ids: &[String]| {
        solve.entities.iter().find(|entity| {
            let entity_surfaces = entity.surface_ids.iter().collect::<BTreeSet<_>>();
            surface_ids
                .iter()
                .all(|surface_id| entity_surfaces.contains(surface_id))
        })
    };
    match (
        component_for(left_surface_ids),
        component_for(right_surface_ids),
    ) {
        (Some(left), Some(right)) if left.component_id == right.component_id => matches!(
            left.state,
            SolveReconciliationState::ResolvedExisting | SolveReconciliationState::PromotableNew
        ),
        _ => false,
    }
}

fn surface_by_observation(
    sidecar: &[EntityLinkObservationSurfaceBinding],
    observation_ids: &[String],
) -> BTreeMap<String, String> {
    observation_ids
        .iter()
        .map(|observation_id| {
            (
                observation_id.clone(),
                binding_for_observation(sidecar, observation_id)
                    .surface_id
                    .clone(),
            )
        })
        .collect()
}

fn binding_for_observation<'a>(
    sidecar: &'a [EntityLinkObservationSurfaceBinding],
    observation_id: &str,
) -> &'a EntityLinkObservationSurfaceBinding {
    sidecar
        .iter()
        .find(|binding| {
            binding.source_row_id.as_deref() == Some(observation_id)
                || binding.link_id == observation_id
        })
        .unwrap_or_else(|| panic!("missing sidecar binding for {observation_id}"))
}

fn leak_channels() -> Vec<LeakChannel> {
    vec![
        LeakChannel::Alias,
        LeakChannel::Anchor,
        LeakChannel::Threshold,
        LeakChannel::Dictionary,
        LeakChannel::Patch,
        LeakChannel::Cache,
        LeakChannel::GeneratedCorpus,
    ]
}

fn strict_representable_leak_channels() -> Vec<LeakChannel> {
    leak_channels()
        .into_iter()
        .filter(|channel| *channel != LeakChannel::Cache)
        .collect()
}

fn is_source_leak_channel(channel: LeakChannel) -> bool {
    matches!(
        channel,
        LeakChannel::Alias
            | LeakChannel::Anchor
            | LeakChannel::Threshold
            | LeakChannel::Dictionary
            | LeakChannel::Patch
    )
}

fn leak_projection_records(format: &str, bytes: &[u8]) -> Vec<Value> {
    match format {
        "json" => match serde_json::from_slice(bytes).expect("checked JSON leak source") {
            Value::Array(records) => records,
            Value::Object(mut object) => {
                if let Some(Value::Array(records)) = object.remove("canonical_inline_records") {
                    records
                } else if let Some(Value::Array(records)) = object.remove("records") {
                    records
                } else {
                    vec![Value::Object(object)]
                }
            }
            Value::Null => panic!("leak source JSON must not be null"),
            scalar => vec![scalar],
        },
        "jsonl" => std::str::from_utf8(bytes)
            .expect("checked JSONL leak source UTF-8")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("checked JSONL record"))
            .collect(),
        "text" => std::str::from_utf8(bytes)
            .expect("checked text leak source UTF-8")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| Value::String(line.to_string()))
            .collect(),
        other => panic!("unsupported leak source format {other}"),
    }
}

fn registry_tree_binding_hash(paths: &[PathBuf; 2]) -> String {
    let mut entries = paths
        .iter()
        .map(|path| {
            (
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("registry path filename")
                    .to_string(),
                fs::read(path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = blake3::Hasher::new();
    for (name, bytes) in entries {
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(
        &fs::read(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("failed to parse {} JSONL: {error}", path.display()))
        })
        .collect()
}

fn write_json(path: &Path, value: &impl Serialize) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("serialize JSON fixture"),
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn write_jsonl(path: &Path, values: &[Value]) {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend(serde_json::to_vec(value).expect("serialize JSONL fixture value"));
        bytes.push(b'\n');
    }
    fs::write(path, bytes)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}

fn copy_dir_recursive(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap_or_else(|error| {
        panic!(
            "failed to create destination {}: {error}",
            destination.display()
        )
    });
    let mut entries = fs::read_dir(source)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()))
        .map(|entry| entry.expect("directory entry"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", source_path.display()));
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path);
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
                panic!(
                    "failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            });
        }
    }
}

fn read_source_rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
    let headers = reader
        .headers()
        .unwrap_or_else(|error| panic!("failed to read {} headers: {error}", path.display()))
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    reader
        .records()
        .map(|record| {
            let record = record
                .unwrap_or_else(|error| panic!("failed to read {} row: {error}", path.display()));
            headers
                .iter()
                .enumerate()
                .map(|(index, header)| {
                    (
                        header.clone(),
                        record.get(index).unwrap_or_default().to_string(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .collect()
}

fn rewrite_source_rows(path: &Path, mutate: impl FnOnce(&mut Vec<BTreeMap<String, String>>)) {
    let mut reader = csv::Reader::from_path(path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
    let headers = reader
        .headers()
        .unwrap_or_else(|error| panic!("failed to read {} headers: {error}", path.display()))
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut rows = reader
        .records()
        .map(|record| {
            let record = record
                .unwrap_or_else(|error| panic!("failed to read {} row: {error}", path.display()));
            headers
                .iter()
                .enumerate()
                .map(|(index, header)| {
                    (
                        header.clone(),
                        record.get(index).unwrap_or_default().to_string(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .collect::<Vec<_>>();
    mutate(&mut rows);

    let mut writer = csv::Writer::from_path(path)
        .unwrap_or_else(|error| panic!("failed to rewrite {}: {error}", path.display()));
    writer
        .write_record(&headers)
        .unwrap_or_else(|error| panic!("failed to write {} headers: {error}", path.display()));
    for row in rows {
        writer
            .write_record(
                headers
                    .iter()
                    .map(|header| row.get(header).map(String::as_str).unwrap_or_default()),
            )
            .unwrap_or_else(|error| panic!("failed to write {} row: {error}", path.display()));
    }
    writer
        .flush()
        .unwrap_or_else(|error| panic!("failed to flush {}: {error}", path.display()));
}

fn run_generalization_cli(manifest: &str, emit: &str) -> Output {
    run_generalization_cli_path(Path::new(manifest), emit)
}

fn run_generalization_cli_path(manifest: &Path, emit: &str) -> Output {
    let manifest_dir = manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let manifest_name = manifest.file_name().unwrap_or(manifest.as_os_str());
    let manifest_arg = Path::new(".").join(manifest_name);
    Command::new(env!("CARGO_BIN_EXE_canon"))
        .current_dir(manifest_dir)
        .env("PWD", manifest_dir)
        .args(["entity", "generalization", "--manifest"])
        .arg(manifest_arg)
        .args(["--emit", emit])
        .output()
        .expect("run generalization cli")
}

fn assert_strict_generalization_report(output: &Output) -> serde_json::Value {
    let report = assert_successful_generalization_report(output);

    assert_eq!(report["corpus_visibility"], "public_fixture");
    assert_eq!(
        report["entity_disjoint"]
            .as_array()
            .expect("entity reports")
            .len(),
        1
    );
    assert_eq!(
        report["time_forward"]
            .as_array()
            .expect("time reports")
            .len(),
        1
    );
    assert_eq!(report["aggregate"]["entity_disjoint_trial_count"], 1);
    assert_eq!(report["aggregate"]["time_forward_trial_count"], 1);
    assert_eq!(report["aggregate"]["result_count"], 8);
    assert_eq!(report["aggregate"]["correct_count"], 8);
    assert_eq!(report["aggregate"]["abstain_count"], 3);
    assert_eq!(report["aggregate"]["critical_false_merge_count"], 0);
    assert_eq!(report["aggregate"]["directional_cross_source_count"], 2);
    assert_eq!(report["aggregate"]["head_result_count"], 2);
    assert_eq!(report["aggregate"]["tail_result_count"], 6);
    assert_eq!(report["aggregate"]["easy_result_count"], 1);
    assert_eq!(report["aggregate"]["hard_result_count"], 7);
    let entity = &report["entity_disjoint"][0];
    assert_eq!(entity["novel_cluster_result_count"], 1);
    assert_eq!(entity["correct_novel_cluster_count"], 1);
    assert_eq!(entity["related_distinct_hard_negative_count"], 1);
    assert_eq!(entity["critical_false_merge_count"], 0);
    assert_eq!(entity["directional_cross_source_count"], 1);
    let time = &report["time_forward"][0];
    assert_redacted_b3(&time["cutoff"]);
    assert_eq!(time["evaluation_result_count"], 3);
    assert_eq!(time["correct_evaluation_count"], 3);
    assert_eq!(time["renamed_surface_count"], 1);
    assert_eq!(time["new_entity_count"], 1);
    assert_eq!(time["changed_relationship_count"], 1);
    assert_eq!(time["critical_false_merge_count"], 0);
    assert_eq!(time["directional_cross_source_count"], 1);
    assert_eq!(report["quality"]["release_claim_status"], "eligible");
    for gate in report["quality"]["gates"]
        .as_array()
        .expect("quality gates")
    {
        assert_eq!(
            gate["status"], "pass",
            "clean strict report gate should pass"
        );
    }
    report
}

fn assert_trial_cache_execution(
    scaffold: &TempGeneralizationScaffold,
    trial_slug: &str,
    cache_mode: StrictCacheExecutionMode,
) {
    let manifest: Value = read_json(&scaffold.manifest_path);
    let trial = manifest_trial(&manifest, trial_slug);
    let cache_execution = &trial["cache_execution"];
    assert_eq!(
        cache_execution["version"],
        CANON_GENERALIZATION_CACHE_EXECUTION_VERSION
    );
    assert_eq!(cache_execution["mode"], cache_mode.manifest_mode());
    assert_eq!(
        cache_execution["receipt"]["version"],
        CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
    );

    let run: EntityRunArtifact = read_json(&scaffold.trial_work_dir(trial_slug).join("run.json"));
    assert_eq!(run.summary.labels["cache_mode"], cache_mode.native_mode());
    assert_eq!(
        run.summary.labels["cache_status"],
        cache_mode.native_status()
    );
    assert_eq!(
        run.summary.labels["cache_receipt_path"],
        INDEX_CACHE_RECEIPT_FILE
    );
    let cache_stage = run
        .stage_artifacts
        .iter()
        .find(|stage| stage.stage == cache_mode.native_stage())
        .expect("mode-specific native cache stage");
    assert_eq!(
        cache_stage.version,
        CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION
    );
    assert_eq!(cache_stage.path, INDEX_CACHE_RECEIPT_FILE);
    assert_eq!(
        run.summary.labels["cache_receipt_hash"],
        cache_stage.artifact_content_hash
    );
    assert_eq!(
        cache_execution["receipt"]["content_hash"],
        cache_stage.artifact_content_hash
    );

    let receipt_path = scaffold.root.join(
        cache_execution["receipt"]["path"]
            .as_str()
            .expect("cache receipt path"),
    );
    let receipt: Value = read_json(&receipt_path);
    assert_eq!(receipt["version"], CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION);
    assert_eq!(receipt["mode"], cache_mode.native_mode());
    assert_eq!(receipt["status"], cache_mode.native_status());
    assert_eq!(
        receipt["reusable"],
        Value::Bool(cache_mode == StrictCacheExecutionMode::EnabledWarmHit)
    );
    assert_eq!(
        blake3_file(&receipt_path),
        cache_stage.artifact_content_hash
    );
}

fn assert_cache_mode_semantic_artifacts_equal(
    disabled: &TempGeneralizationScaffold,
    enabled: &TempGeneralizationScaffold,
    trial_slug: &str,
) {
    let disabled_run: EntityRunArtifact =
        read_json(&disabled.trial_work_dir(trial_slug).join("run.json"));
    let enabled_run: EntityRunArtifact =
        read_json(&enabled.trial_work_dir(trial_slug).join("run.json"));

    for stage_name in ["prepare", "index", "block", "edge", "solve"] {
        let disabled_stage = semantic_stage(&disabled_run, stage_name);
        let enabled_stage = semantic_stage(&enabled_run, stage_name);
        assert_eq!(
            disabled_stage.version, enabled_stage.version,
            "{trial_slug} {stage_name} semantic stage versions differ"
        );
        assert_eq!(
            disabled_stage.path, enabled_stage.path,
            "{trial_slug} {stage_name} semantic stage paths differ"
        );
        assert_eq!(
            disabled_stage.artifact_content_hash, enabled_stage.artifact_content_hash,
            "{trial_slug} {stage_name} semantic stage hashes differ"
        );
        let disabled_bytes = stage_artifact_bytes(disabled, trial_slug, disabled_stage, stage_name);
        let enabled_bytes = stage_artifact_bytes(enabled, trial_slug, enabled_stage, stage_name);
        assert_eq!(
            disabled_bytes, enabled_bytes,
            "{trial_slug} {stage_name} semantic stage bytes differ between disabled bypass and enabled warm-hit"
        );
    }

    let disabled_link: EntityLinkArtifact =
        read_json(&disabled.trial_work_dir(trial_slug).join("link/link.json"));
    let enabled_link: EntityLinkArtifact =
        read_json(&enabled.trial_work_dir(trial_slug).join("link/link.json"));
    assert_eq!(
        disabled_link.decision_artifact, enabled_link.decision_artifact,
        "{trial_slug} link decision artifact differs between disabled bypass and enabled warm-hit"
    );
}

fn semantic_stage<'a>(run: &'a EntityRunArtifact, stage_name: &str) -> &'a EntityRunStageArtifact {
    run.stage_artifacts
        .iter()
        .find(|stage| stage.stage == stage_name)
        .unwrap_or_else(|| panic!("missing semantic stage {stage_name}"))
}

fn stage_artifact_bytes(
    scaffold: &TempGeneralizationScaffold,
    trial_slug: &str,
    stage: &EntityRunStageArtifact,
    stage_name: &str,
) -> Vec<u8> {
    let path = scaffold.trial_work_dir(trial_slug).join(&stage.path);
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let artifact: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    assert_eq!(
        artifact["artifact_content_hash"].as_str(),
        Some(stage.artifact_content_hash.as_str()),
        "{trial_slug} {stage_name} artifact self-hash does not match run stage hash"
    );
    bytes
}

fn assert_successful_generalization_report(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "generalization CLI should compile the strict execution envelope\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("generalization CLI emits JSON");

    assert_eq!(report["version"], CANON_GENERALIZATION_VERSION);
    assert_eq!(report["identifier_redaction"], "blake3");
    assert_redacted_b3(&report["benchmark_id"]);
    assert_eq!(report["derivation"]["source"], "strict_execution_envelope");
    assert_eq!(report["derivation"]["self_attested_outcomes_used"], false);
    assert!(
        report["benchmark_digest"]
            .as_str()
            .expect("benchmark digest")
            .starts_with("blake3:")
    );
    assert!(
        report["report_digest"]
            .as_str()
            .expect("report digest")
            .starts_with("blake3:")
    );
    report
}

fn json_quality_gate<'a>(report: &'a serde_json::Value, gate_id: &str) -> &'a serde_json::Value {
    report["quality"]["gates"]
        .as_array()
        .expect("quality gates")
        .iter()
        .find(|gate| gate["gate_id"].as_str() == Some(gate_id))
        .unwrap_or_else(|| panic!("missing JSON quality gate {gate_id}"))
}

fn assert_strict_generalization_refusal(output: &Output, expected_generalization_code: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "strict generalization mutation should refuse\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "strict refusal must not emit a report on stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"stage\":\"generalization\"")
            && stderr.contains("\"writes_performed\":false"),
        "refusal should be sanitized and scoped to generalization: {stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "\"generalization_code\":\"{expected_generalization_code}\""
        )),
        "refusal should expose generalization_code={expected_generalization_code:?}\nstderr={stderr}"
    );
}

fn sanitized_strict_stdout_bytes(output: &Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "generalization CLI should emit successful stdout before byte comparison\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout.clone()
}

fn sanitize_root_specific_stdout(bytes: &[u8], root: &Path) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("strict stdout is UTF-8 JSON");
    text.replace(&root.to_string_lossy().into_owned(), "<temp-root>")
        .into_bytes()
}

fn sanitize_root_specific_report(report: &serde_json::Value, root: &Path) -> serde_json::Value {
    let bytes = serde_json::to_vec(report).expect("serialize strict report");
    serde_json::from_slice(&sanitize_root_specific_stdout(&bytes, root))
        .expect("root-sanitized report remains JSON")
}

fn assert_json_eq_with_path(left: &serde_json::Value, right: &serde_json::Value, path: &str) {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            assert_eq!(
                left.keys().collect::<Vec<_>>(),
                right.keys().collect::<Vec<_>>(),
                "{path} object keys differ"
            );
            for key in left.keys() {
                assert_json_eq_with_path(&left[key], &right[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            assert_eq!(left.len(), right.len(), "{path} array lengths differ");
            for (index, (left, right)) in left.iter().zip(right.iter()).enumerate() {
                assert_json_eq_with_path(left, right, &format!("{path}[{index}]"));
            }
        }
        _ => assert_eq!(left, right, "{path} differs"),
    }
}

fn strict_domain_report_semantics(report: &serde_json::Value) -> serde_json::Value {
    let mut report = report.clone();
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "report_digest".to_string(),
            json!("<path-derived-report-receipt>"),
        );
    }
    if let Some(derivation) = report
        .get_mut("derivation")
        .and_then(serde_json::Value::as_object_mut)
    {
        derivation.insert(
            "manifest_hash".to_string(),
            json!("<path-derived-manifest-receipt>"),
        );
        if let Some(artifact_hashes) = derivation
            .get_mut("artifact_hashes")
            .and_then(serde_json::Value::as_array_mut)
        {
            for artifact in artifact_hashes {
                if let Some(artifact) = artifact.as_object_mut() {
                    artifact.insert(
                        "content_hash".to_string(),
                        json!("<path-derived-artifact-receipt>"),
                    );
                }
            }
        }
        if let Some(leak_hashes) = derivation
            .get_mut("leak_source_hashes")
            .and_then(serde_json::Value::as_array_mut)
        {
            for source in leak_hashes {
                if let Some(source) = source.as_object_mut() {
                    for key in [
                        "content_hash",
                        "bundle_content_hash",
                        "binding_hash",
                        "checked_source_hashes",
                    ] {
                        source.insert(key.to_string(), json!("<path-derived-leak-receipt>"));
                    }
                }
            }
        }
    }
    report
}

fn strict_domain_result_slices(report: &serde_json::Value) -> serde_json::Value {
    json!({
        "entity_disjoint": report["entity_disjoint"],
        "time_forward": report["time_forward"],
        "aggregate": report["aggregate"],
        "quality": report["quality"]
    })
}

fn quality_gate<'a>(
    report: &'a GeneralizationReport,
    gate_id: &str,
) -> &'a GeneralizationQualityGateReport {
    report
        .quality
        .gates
        .iter()
        .find(|gate| gate.gate_id == gate_id)
        .unwrap_or_else(|| panic!("missing quality gate {gate_id}"))
}

fn assert_redacted_b3(value: &serde_json::Value) {
    let value = value.as_str().expect("redacted string");
    assert!(
        value.starts_with("blake3:") && value.len() == "blake3:".len() + 64,
        "expected blake3 redaction, got {value}"
    );
}

fn blake3_file(path: &Path) -> String {
    blake3_bytes(
        &fs::read(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
    )
}

fn blake3_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn blake3_serialized(value: &impl Serialize) -> String {
    blake3_bytes(&serde_json::to_vec(value).expect("serialize canonical JSON value"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct SliceKey {
    source_family: SourceFamily,
    relation_class: RelationClass,
    difficulty_band: DifficultyBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeLinkExpectation {
    trial_slug: &'static str,
    matched: i64,
    unmatched: i64,
    target_records: i64,
}

fn native_link_expectations() -> [NativeLinkExpectation; 2] {
    [
        NativeLinkExpectation {
            trial_slug: "entity_disjoint",
            matched: 1,
            unmatched: 1,
            target_records: 2,
        },
        NativeLinkExpectation {
            trial_slug: "time_forward",
            matched: 2,
            unmatched: 1,
            target_records: 3,
        },
    ]
}
