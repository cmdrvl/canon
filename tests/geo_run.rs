#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/fs_safety.rs"]
mod fs_safety;

mod project {
    pub use canon::project::*;
}

mod geo {
    pub use canon::geo::*;

    pub mod executor {
        pub use crate::executor::*;
    }
}

#[allow(dead_code)]
#[path = "../src/geo/executor.rs"]
mod executor;

#[allow(dead_code)]
#[path = "../src/geo/run.rs"]
mod run;

use canon::{
    geo::{
        CANON_GEO_ACQUISITION_RECEIPT_VERSION, CANON_GEO_ACQUISITION_SATISFACTION_VERSION,
        CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION, CANON_GEO_COMPOSITION_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_GEOMETRY_TILE_VERSION,
        CANON_GEO_HOME_CELL_ROWS_VERSION, CANON_GEO_LOCAL_FRAME_VERSION,
        CANON_GEO_QUESTION_VERSION, CANON_GEO_REGIONAL_INVENTORY_VERSION,
        CANON_GEO_RESOURCE_BUDGET_VERSION, CANON_GEO_TILE_WORK_REQUEST_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS,
        GeoAbstentionDisposition, GeoAbstentionPolicy, GeoAcquisitionDenominator,
        GeoAcquisitionProofClass, GeoAcquisitionTerminalState, GeoAffineProjectionMm, GeoAsOf,
        GeoBoundedGeography, GeoBudgetAction, GeoClaimClass, GeoClientTileCoverageExtent,
        GeoClientTileCoverageExtentKind, GeoClientTileIngestRequest, GeoClientTileSourceFormat,
        GeoClientTileVendorIdentifier, GeoCompositionProfile, GeoControlEntityLevel,
        GeoCoveragePredicate, GeoDateInterval, GeoDenominatorSource, GeoDigest, GeoDigestAlgorithm,
        GeoEgressClass, GeoEvidenceClaimRole, GeoEvidenceClass, GeoEvidenceRecordRef,
        GeoGeometryTransformContract, GeoIdentityParticipation, GeoLicenseClass,
        GeoLocalAcquisitionState, GeoLocalArtifactRef, GeoLocalFrameContract, GeoNativeEntityScope,
        GeoNumericBound, GeoNumericMeasure, GeoPlan, GeoPlanInventoryRef, GeoPlanRequest,
        GeoPlanStage, GeoPlanStatus, GeoProjectionProvenance, GeoRegionalInventory,
        GeoRegionalSourceInstance, GeoRequestedGrain, GeoResourceBudget, GeoResourceCounter,
        GeoRhoBasis, GeoRhoContract, GeoRhoObservationKind, GeoSatisfactionExecutionRef,
        GeoSatisfactionFileAudit, GeoSatisfactionFinding, GeoSatisfactionFindingCode,
        GeoSatisfactionLocalInputBinding, GeoSatisfactionRunInputRef, GeoSatisfactionStatus,
        GeoSourceAvailability, GeoSourceAxisDomain, GeoSourcePointFixed, GeoSourceRelease,
        GeoSubjectBinding, GeoSubjectBindingClass, GeoTelemetryDeclaration, GeoTelemetryMetric,
        GeoTelemetrySemanticEffect, GeoTemporalScope, GeoTileFeatureRef, GeoTileSourceBinding,
        GeoTileWorkRequest, GeoValueOrigin, GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow,
        GeoWarehouseRowsRequest, compile_geo_plan, default_geo_capabilities,
        geo_plan_semantic_hash, materialize_warehouse_rows,
    },
    project::{
        ProjectExtensionDagNode, ProjectExtensionDagOutput, ProjectExtensionDagRequest,
        ProjectNodeExecutionContext, ProjectNodeExecutionResult, ProjectNodeExecutor, ProjectPlan,
        ProjectPlanErrorCode, ProjectPlanHashRef, ProjectPlanNode, ProjectPlanNodeClass,
        ProjectPlanNodeKind, ProjectPlanOutputMaterialization, ProjectPlanRefusalCondition,
        ProjectPlanSideEffect, ProjectPlanSideEffectKind, ProjectRunError, ProjectRunErrorCode,
        ProjectRunFailurePolicy, ProjectRunPolicy, ProjectRunResult,
        compile_extension_project_plan, digest_bytes, read_node_receipt,
    },
};
use executor::{
    CANON_GEO_CLIENT_TILE_SOURCE_VERSION, GEO_CLIENT_TILE_INGEST_STAGE_COMMAND,
    GEO_CLIENT_TILE_SOURCE_BINDING_ID, GEO_REQUEST_BINDING_ID, GEO_ROWS_BINDING_ID,
};
use h3o::{CellIndex, LatLng, Resolution};
use run::{
    CANON_GEO_RUN_PROGRESS_VERSION, GeoRun, GeoRunAcquisitionSatisfactionRef,
    GeoRunArtifactBinding, GeoRunErrorCode, GeoRunNextActionKind, GeoRunObservation,
    GeoRunProgressEvent, GeoRunProgressEventKind, GeoRunRequest, GeoRunStatus,
    canonical_geo_run_bytes, canonical_geo_run_semantic_bytes, geo_run_input_hash_ref_id,
    geo_run_semantic_hash, run_geo_plan, run_geo_plan_with_progress_writer,
    run_geo_plan_with_project_executor,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    str::FromStr,
};

const CLIENT_TILE_NODE_ID: &str = "geo.client.tile_ingest";
const CLIENT_TILE_OUTPUT_ID: &str = "client_tile";
const CLIENT_TILE_OUTPUT_PATH: &str = "geo/client/client_tile.json";

#[test]
fn geo_run_executes_real_kernels_and_folds_input_hashes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );

    let run = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("Geo run executes");

    assert_eq!(run.status, GeoRunStatus::Completed);
    assert_eq!(
        run.project_run_report
            .as_ref()
            .expect("project report")
            .failed_nodes,
        Vec::<String>::new()
    );
    assert_eq!(
        run.project_run_report
            .as_ref()
            .expect("project report")
            .executed_nodes
            .len(),
        5
    );
    assert_ne!(
        run.plan_ref.project_graph_hash,
        plan.project_plan.graph_hash
    );
    assert_eq!(run.artifact_inputs.len(), 3);
    assert_eq!(run.output_refs.len(), 5);

    let solve = solve_output(temp.path());
    assert_eq!(solve["version"], CANON_GEO_COMPOSITION_VERSION);
    assert_eq!(solve["status"], "resolved");
    assert_eq!(solve["summary"]["component_count"], 1);

    let receipt = read_node_receipt(&receipt_path(temp.path(), "geo.building.home_cells"))
        .expect("home cell receipt");
    assert!(receipt.content_hash_inputs.iter().any(|input| {
        input.ref_id == geo_run_input_hash_ref_id("geo.building.home_cells", GEO_ROWS_BINDING_ID)
    }));
}

#[test]
fn geo_run_executes_client_tile_ingest_stage_from_raw_source_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = client_tile_ingest_plan();
    let (request, source, center, neighbor) = client_tile_source_and_request();

    let run = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(temp.path()),
        client_tile_run_bindings(&request, source.as_bytes()),
    ))
    .expect("client tile stage runs through geo run");

    assert_eq!(run.status, GeoRunStatus::Completed);
    assert_eq!(run.artifact_inputs.len(), 2);
    assert!(run.artifact_inputs.iter().any(|input| {
        input.node_id == CLIENT_TILE_NODE_ID
            && input.binding_id == GEO_CLIENT_TILE_SOURCE_BINDING_ID
            && input.contract_version == CANON_GEO_CLIENT_TILE_SOURCE_VERSION
    }));
    assert_eq!(run.output_refs.len(), 1);
    assert_eq!(
        run.output_refs[0].contract_version,
        CANON_GEO_GEOMETRY_TILE_VERSION
    );

    let artifact_path = temp.path().join(CLIENT_TILE_OUTPUT_PATH);
    let bytes = fs::read(&artifact_path).expect("client tile artifact is published");
    assert_eq!(digest_bytes(&bytes), run.output_refs[0].content_digest);
    let artifact: Value = serde_json::from_slice(&bytes).expect("client tile parses");
    assert_eq!(artifact["version"], CANON_GEO_GEOMETRY_TILE_VERSION);
    assert_eq!(artifact["provider_tile"]["tile_id"], center);
    assert_eq!(
        artifact["provider_tile"]["client_ingest"]["summary"]["anchor_membership_count"],
        2
    );
    assert!(
        artifact["provider_tile"]["client_ingest"]["memberships"]
            .as_array()
            .expect("memberships")
            .iter()
            .any(|membership| {
                membership["source_feature_id"] == "client-apn-1"
                    && membership["h3_cell"] == neighbor
                    && membership["rule"] == "declared_supplemental_coverage"
            })
    );
}

#[test]
fn geo_run_client_tile_ingest_waits_for_raw_source_and_rejects_wrong_source_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = client_tile_ingest_plan();
    let (request, source, _, _) = client_tile_source_and_request();
    let request_only = vec![
        GeoRunArtifactBinding::from_json(
            CLIENT_TILE_NODE_ID,
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
            &request,
        )
        .expect("request binding"),
    ];

    let waiting = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        request_only,
    ))
    .expect("missing source waits for input");

    assert_eq!(waiting.status, GeoRunStatus::WaitingForInput);
    assert!(waiting.output_refs.is_empty());
    assert!(waiting.next_actions.iter().any(|action| {
        action.project_node_id.as_deref() == Some(CLIENT_TILE_NODE_ID)
            && action.expected_contract.as_deref() == Some(CANON_GEO_CLIENT_TILE_SOURCE_VERSION)
    }));
    assert!(!temp.path().join(CLIENT_TILE_OUTPUT_PATH).exists());

    let bad_temp = tempfile::tempdir().expect("tempdir");
    let error = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(bad_temp.path()),
        vec![
            GeoRunArtifactBinding::from_json(
                CLIENT_TILE_NODE_ID,
                GEO_REQUEST_BINDING_ID,
                CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
                &request,
            )
            .expect("request binding"),
            GeoRunArtifactBinding::from_bytes(
                CLIENT_TILE_NODE_ID,
                GEO_CLIENT_TILE_SOURCE_BINDING_ID,
                CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
                source.into_bytes(),
            ),
        ],
    ))
    .expect_err("raw source bound as a typed request refuses");

    assert_eq!(error.code, GeoRunErrorCode::OutputContractViolation);
    assert!(!bad_temp.path().join(CLIENT_TILE_OUTPUT_PATH).exists());
}

#[test]
fn geo_run_persists_acquisition_satisfaction_lineage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let satisfaction = acquisition_satisfaction_for_binding(&bindings[2]);

    let run = run_geo_plan(
        GeoRunRequest::new(plan, policy(temp.path()), bindings.clone())
            .with_acquisition_satisfactions(vec![satisfaction.clone()]),
    )
    .expect("Geo run executes with acquisition lineage");

    assert_eq!(run.acquisition_satisfactions, vec![satisfaction.clone()]);
    let run_value: Value =
        serde_json::from_slice(&canonical_geo_run_bytes(&run).expect("canonical run"))
            .expect("run json");
    let lineage = &run_value["acquisition_satisfactions"][0];
    assert_eq!(
        lineage["denominators"][0]["denominator_id"],
        "denominator.fixture.rows"
    );
    assert_eq!(
        lineage["source_digests"][0]["digest_id"],
        "source.fixture.catalog"
    );
    assert_eq!(
        lineage["result_digests"][0]["hex_digest"],
        bindings[2].content_digest.trim_start_matches("blake3:")
    );
    assert_eq!(
        lineage["run_input_refs"][0]["artifact_id"],
        bindings[2].artifact_id
    );
    assert_eq!(
        lineage["receipt_execution"]["fixture_id"],
        "fixture.acquisition.receipt"
    );

    let semantic = canonical_geo_run_semantic_bytes(&run).expect("semantic run");
    let mut executor_variant = run.clone();
    executor_variant.acquisition_satisfactions[0]
        .receipt_execution
        .fixture_id = Some("fixture.acquisition.receipt.via-other-protocol".to_string());
    assert_eq!(
        canonical_geo_run_semantic_bytes(&executor_variant).expect("variant semantic"),
        semantic,
        "operational receipt identity is durable but excluded from protocol-neutral run semantics"
    );
}

#[test]
fn geo_run_rejects_acquisition_lineage_that_drifts_from_bound_input() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let mut satisfaction = acquisition_satisfaction_for_binding(&bindings[2]);
    satisfaction.run_input_refs[0].content_hash = digest("wrong warehouse rows");

    let error = run_geo_plan(
        GeoRunRequest::new(plan, policy(temp.path()), bindings)
            .with_acquisition_satisfactions(vec![satisfaction]),
    )
    .expect_err("drifted acquisition run input refuses");

    assert_eq!(error.code, GeoRunErrorCode::InputDigestMismatch);
    assert!(
        !temp.path().join("work/receipts").exists(),
        "lineage drift must refuse before project execution creates receipts"
    );
}

#[test]
fn fresh_geo_run_resume_preloads_bounded_section_for_solve() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut partial_policy = policy(temp.path());
    partial_policy
        .cancel_before_nodes
        .insert("geo.building.solve".to_string());

    let partial = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        partial_policy,
        run_bindings(warehouse_rows()),
    ))
    .expect("partial Geo run");
    assert_eq!(partial.status, GeoRunStatus::Cancelled);
    assert!(!temp.path().join("geo/building/solve.json").exists());

    let resumed = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("fresh executor resumes solve");
    let report = resumed.project_run_report.as_ref().expect("project report");
    assert_eq!(resumed.status, GeoRunStatus::Completed);
    assert_eq!(
        report.executed_nodes,
        vec!["geo.building.solve".to_string()]
    );
    assert_eq!(report.resumed_nodes.len(), 4);
    for node_id in [
        "geo.building.home_cells",
        "geo.building.section",
        "geo.building.materialize_evidence",
        "geo.building.compile_evidence",
    ] {
        assert!(report.resumed_nodes.contains(&node_id.to_string()));
    }
    assert_eq!(solve_output(temp.path())["status"], "resolved");
}

#[test]
fn opt_in_progress_is_deterministic_and_non_semantic() {
    let baseline_temp = tempfile::tempdir().expect("baseline tempdir");
    let observed_temp = tempfile::tempdir().expect("observed tempdir");
    let repeated_temp = tempfile::tempdir().expect("repeated tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let baseline = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(baseline_temp.path()),
        bindings.clone(),
    ))
    .expect("baseline run");
    let mut progress_bytes = Vec::new();
    let observed = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(plan.clone(), policy(observed_temp.path()), bindings.clone()),
        &mut progress_bytes,
    )
    .expect("run with progress");
    let mut repeated_progress_bytes = Vec::new();
    let repeated = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(plan, policy(repeated_temp.path()), bindings),
        &mut repeated_progress_bytes,
    )
    .expect("repeated run with progress");

    assert_eq!(
        canonical_geo_run_semantic_bytes(&baseline).expect("baseline semantic bytes"),
        canonical_geo_run_semantic_bytes(&observed).expect("observed semantic bytes")
    );
    assert_eq!(baseline.semantic_hash, observed.semantic_hash);
    assert_eq!(observed.semantic_hash, repeated.semantic_hash);
    assert_eq!(progress_bytes, repeated_progress_bytes);

    let events = progress_events(&progress_bytes);
    assert_eq!(
        events.first().unwrap().kind,
        GeoRunProgressEventKind::RunStarted
    );
    assert_eq!(
        events.last().unwrap().kind,
        GeoRunProgressEventKind::RunFinished
    );
    assert_eq!(events.last().unwrap().status, Some(GeoRunStatus::Completed));
    assert_eq!(events.last().unwrap().counters.completed_nodes, 5);
    assert_eq!(events.last().unwrap().counters.executed_nodes, 5);
    assert_eq!(events.last().unwrap().counters.resumed_nodes, 0);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == GeoRunProgressEventKind::StageStarted)
            .count(),
        5
    );
    for (sequence, event) in events.iter().enumerate() {
        assert_eq!(event.version, CANON_GEO_RUN_PROGRESS_VERSION);
        assert_eq!(event.sequence, sequence as u64);
    }
    assert!(
        events
            .windows(2)
            .all(|window| window[0].phase <= window[1].phase),
        "progress phase must never regress even when DAG stages reuse an earlier phase label"
    );
    assert_eq!(
        events
            .iter()
            .find(|event| {
                event.kind == GeoRunProgressEventKind::StageStarted
                    && event.project_node_id.as_deref() == Some("geo.building.materialize_evidence")
            })
            .expect("materialize-evidence stage")
            .phase,
        run::GeoRunPhase::ReachChecked,
        "materialize evidence follows bounded-section reach checking and must not regress phase"
    );
    let rendered = String::from_utf8(progress_bytes).expect("progress utf8");
    for forbidden in ["observed_at", "workspace_path", "host_id", "process_id"] {
        assert!(!rendered.contains(forbidden));
    }
}

#[test]
fn progress_writer_failure_is_operational_and_leaves_semantic_work_resumable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let mut writer = AlwaysFailProgressWriter;
    let error = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(plan.clone(), policy(temp.path()), bindings.clone()),
        &mut writer,
    )
    .expect_err("progress delivery failure");
    assert_eq!(error.code, GeoRunErrorCode::ProgressOutput);
    assert!(error.message.contains("completed its semantic work"));

    let resumed = run_geo_plan(GeoRunRequest::new(plan, policy(temp.path()), bindings))
        .expect("semantic work remains resumable");
    assert_eq!(resumed.status, GeoRunStatus::Completed);
    let report = resumed.project_run_report.expect("project report");
    assert!(report.executed_nodes.is_empty());
    assert_eq!(report.resumed_nodes.len(), 5);
}

#[test]
fn progress_cancellation_names_last_commit_and_resume_reports_reuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let mut cancelled_policy = policy(temp.path());
    cancelled_policy
        .cancel_before_nodes
        .insert("geo.building.solve".to_string());
    let mut cancelled_progress = Vec::new();
    let cancelled = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(plan.clone(), cancelled_policy, bindings.clone()),
        &mut cancelled_progress,
    )
    .expect("cancelled run");
    assert_eq!(cancelled.status, GeoRunStatus::Cancelled);
    let cancelled_events = progress_events(&cancelled_progress);
    let terminal = cancelled_events
        .last()
        .expect("cancellation terminal event");
    assert_eq!(terminal.kind, GeoRunProgressEventKind::RunCancelled);
    assert_eq!(terminal.status, Some(GeoRunStatus::Cancelled));
    assert_eq!(
        terminal.project_node_id.as_deref(),
        Some("geo.building.solve")
    );
    assert_eq!(
        terminal.stage,
        Some(canon::geo::GeoPlanStage::FactorAndSolveExactResidual)
    );
    assert!(
        terminal
            .wait_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert_eq!(terminal.counters.completed_nodes, 4);
    assert_eq!(terminal.counters.executed_nodes, 4);
    assert_eq!(terminal.counters.cancelled_nodes, 1);
    assert_eq!(
        terminal
            .last_committed_artifact
            .as_ref()
            .expect("last committed artifact")
            .artifact_id,
        "geo.building.compile_evidence/compile_evidence"
    );

    let mut resumed_progress = Vec::new();
    let resumed = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(plan, policy(temp.path()), bindings),
        &mut resumed_progress,
    )
    .expect("resumed run");
    assert_eq!(resumed.status, GeoRunStatus::Completed);
    let resumed_events = progress_events(&resumed_progress);
    assert_eq!(
        resumed_events
            .iter()
            .filter(|event| event.kind == GeoRunProgressEventKind::ArtifactResumed)
            .count(),
        4
    );
    assert_eq!(
        resumed_events
            .iter()
            .filter(|event| event.kind == GeoRunProgressEventKind::StageStarted)
            .map(|event| event.project_node_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("geo.building.solve")]
    );
    let solve_start_index = resumed_events
        .iter()
        .position(|event| {
            event.kind == GeoRunProgressEventKind::StageStarted
                && event.project_node_id.as_deref() == Some("geo.building.solve")
        })
        .expect("solve start event");
    assert_eq!(
        resumed_events[..solve_start_index]
            .iter()
            .filter(|event| event.kind == GeoRunProgressEventKind::ArtifactResumed)
            .count(),
        4,
        "all validated reusable receipts must be visible before pending execution begins"
    );
    let solve_start = &resumed_events[solve_start_index];
    assert_eq!(solve_start.counters.completed_nodes, 4);
    assert_eq!(solve_start.counters.resumed_nodes, 4);
    assert_eq!(
        solve_start
            .last_committed_artifact
            .as_ref()
            .expect("last resumed artifact before solve")
            .artifact_id,
        "geo.building.compile_evidence/compile_evidence"
    );
    let terminal = resumed_events.last().expect("resume terminal event");
    assert_eq!(terminal.kind, GeoRunProgressEventKind::RunFinished);
    assert_eq!(terminal.status, Some(GeoRunStatus::Completed));
    assert_eq!(terminal.counters.completed_nodes, 5);
    assert_eq!(terminal.counters.executed_nodes, 1);
    assert_eq!(terminal.counters.resumed_nodes, 4);
}

#[test]
fn unchanged_geo_run_resumes_and_ignores_operational_observation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());

    let first = run_geo_plan(GeoRunRequest {
        plan: plan.clone(),
        policy: policy(temp.path()),
        input_bindings: bindings.clone(),
        acquisition_satisfactions: Vec::new(),
        observation: GeoRunObservation {
            workspace_path: Some("/tmp/first".to_string()),
            observed_at_utc: Some("2026-08-31T10:00:00Z".to_string()),
            host_id: Some("host-a".to_string()),
            process_id: Some(100),
            resource_observations: BTreeMap::from([("rss_bytes".to_string(), 100)]),
        },
    })
    .expect("first run");
    let second = run_geo_plan(GeoRunRequest {
        plan,
        policy: policy(temp.path()),
        input_bindings: bindings,
        acquisition_satisfactions: Vec::new(),
        observation: GeoRunObservation {
            workspace_path: Some("/tmp/second".to_string()),
            observed_at_utc: Some("2026-08-31T11:00:00Z".to_string()),
            host_id: Some("host-b".to_string()),
            process_id: Some(200),
            resource_observations: BTreeMap::from([("rss_bytes".to_string(), 9_999)]),
        },
    })
    .expect("resume run");

    assert!(
        second
            .project_run_report
            .as_ref()
            .unwrap()
            .executed_nodes
            .is_empty()
    );
    assert_eq!(
        canonical_geo_run_semantic_bytes(&first).expect("first semantic"),
        canonical_geo_run_semantic_bytes(&second).expect("second semantic")
    );
    assert_eq!(first.semantic_hash, second.semantic_hash);
}

#[test]
fn canonical_run_rejects_state_order_and_reference_drift() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run = run_geo_plan(GeoRunRequest::new(
        building_plan(
            "release.fixture.one",
            GeoSourceAvailability::Available,
            None,
        ),
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("valid run");

    let mut invalid_state = run.clone();
    invalid_state.status = GeoRunStatus::WaitingForInput;
    restamp_geo_run(&mut invalid_state);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_state)
            .expect_err("waiting run without a blocker or action refuses")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut invalid_order = run.clone();
    invalid_order.artifact_inputs.reverse();
    restamp_geo_run(&mut invalid_order);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_order)
            .expect_err("non-canonical artifact input order refuses")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut invalid_output = run.clone();
    invalid_output.output_refs[0].media_type = "text/plain".to_string();
    restamp_geo_run(&mut invalid_output);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_output)
            .expect_err("non-JSON output reference refuses")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut invalid_grain = run.clone();
    invalid_grain.grain_states[0].entity_level = "building-ish".to_string();
    restamp_geo_run(&mut invalid_grain);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_grain)
            .expect_err("unknown grain entity level refuses")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut invalid_blocker = run.clone();
    invalid_blocker.blockers.push(run::GeoRunBlocker {
        blocker_id: String::new(),
        kind: run::GeoRunBlockerKind::ProjectBlocked,
        project_node_id: None,
        entity_level: None,
        reason: "bounded negative fixture".to_string(),
    });
    restamp_geo_run(&mut invalid_blocker);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_blocker)
            .expect_err("empty blocker id refuses")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut invalid_action = run.clone();
    invalid_action.next_actions.push(run::GeoRunNextAction {
        action_id: "inspect.invalid".to_string(),
        kind: GeoRunNextActionKind::InspectFailure,
        project_node_id: None,
        artifact_id: None,
        expected_contract: Some("not-a-contract".to_string()),
        media_type: None,
        command: None,
        reason: "bounded negative fixture".to_string(),
    });
    restamp_geo_run(&mut invalid_action);
    assert_eq!(
        canonical_geo_run_bytes(&invalid_action)
            .expect_err("invalid expected-contract alternatives refuse")
            .code,
        GeoRunErrorCode::ArtifactContract
    );

    let mut missing_observation = serde_json::to_value(run).expect("run value");
    missing_observation
        .as_object_mut()
        .expect("run object")
        .remove("observation");
    assert!(serde_json::from_value::<GeoRun>(missing_observation).is_err());
}

#[test]
fn changed_binding_bytes_with_same_claimed_digest_refuse_before_reuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("initial run");

    let mut bindings = run_bindings(warehouse_rows());
    let home = bindings
        .iter_mut()
        .find(|binding| binding.node_id == "geo.building.home_cells")
        .expect("home binding");
    let first_quote = home.bytes.iter().position(|byte| *byte == b'a').unwrap();
    home.bytes[first_quote] = b'z';

    let error = run_geo_plan(GeoRunRequest::new(plan, policy(temp.path()), bindings))
        .expect_err("changed bytes with old digest refuse");
    assert_eq!(error.code, GeoRunErrorCode::InputDigestMismatch);
    assert_eq!(solve_output(temp.path())["status"], "resolved");
}

#[test]
fn typed_composition_status_projects_to_geo_run_status() {
    let fallback_temp = tempfile::tempdir().expect("fallback tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut fallback_rows = warehouse_rows();
    fallback_rows.max_assignments = 1;

    let fallback = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(fallback_temp.path()),
        run_bindings(fallback_rows),
    ))
    .expect("fallback run");
    assert_eq!(fallback.status, GeoRunStatus::BudgetFallback);
    assert_eq!(
        solve_output(fallback_temp.path())["status"],
        "budget_fallback"
    );

    let ambiguous_temp = tempfile::tempdir().expect("ambiguous tempdir");
    let mut ambiguous_rows = warehouse_rows();
    ambiguous_rows.contracts.clear();
    ambiguous_rows.evidence_rows.clear();
    let ambiguous = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(ambiguous_temp.path()),
        run_bindings(ambiguous_rows),
    ))
    .expect("ambiguous run");
    assert_eq!(ambiguous.status, GeoRunStatus::Abstained);
    assert_eq!(solve_output(ambiguous_temp.path())["status"], "ambiguous");
}

#[test]
fn semantic_query_as_of_changes_identity_but_failure_prose_does_not() {
    let first_temp = tempfile::tempdir().expect("first tempdir");
    let second_temp = tempfile::tempdir().expect("second tempdir");
    let first_plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let second_plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        Some("2026-09-01"),
    );

    let first = run_geo_plan(GeoRunRequest::new(
        first_plan,
        policy(first_temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("first semantic-time run");
    let second = run_geo_plan(GeoRunRequest::new(
        second_plan,
        policy(second_temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("second semantic-time run");
    assert_ne!(first.semantic_hash, second.semantic_hash);

    let fail_a_temp = tempfile::tempdir().expect("fail a tempdir");
    let fail_b_temp = tempfile::tempdir().expect("fail b tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let bindings = run_bindings(warehouse_rows());
    let mut fail_a = FailingProjectExecutor {
        message: "operator path /tmp/a failed".to_string(),
    };
    let mut fail_b = FailingProjectExecutor {
        message: "operator path /var/tmp/b failed differently".to_string(),
    };
    let failed_a = run_geo_plan_with_project_executor(
        GeoRunRequest::new(plan.clone(), policy(fail_a_temp.path()), bindings.clone()),
        &mut fail_a,
    )
    .expect("failure run a");
    let failed_b = run_geo_plan_with_project_executor(
        GeoRunRequest::new(plan, policy(fail_b_temp.path()), bindings),
        &mut fail_b,
    )
    .expect("failure run b");
    assert_eq!(failed_a.status, GeoRunStatus::Failed);
    assert_eq!(failed_b.status, GeoRunStatus::Failed);
    assert_eq!(
        canonical_geo_run_semantic_bytes(&failed_a).expect("failed a semantic"),
        canonical_geo_run_semantic_bytes(&failed_b).expect("failed b semantic")
    );
}

#[test]
fn changed_release_refuses_foreign_project_receipts_in_the_same_work_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let first_plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    run_geo_plan(GeoRunRequest::new(
        first_plan,
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("first release run");

    let second_plan = building_plan(
        "release.fixture.two",
        GeoSourceAvailability::Available,
        None,
    );
    let error = run_geo_plan(GeoRunRequest::new(
        second_plan,
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect_err("changed release must not consume foreign project receipts");
    assert_eq!(error.code, GeoRunErrorCode::ProjectRunFailed);
    assert_eq!(
        error
            .detail
            .get("project_run_error_code")
            .map(String::as_str),
        Some("ReceiptPoisoning")
    );
    assert!(
        error
            .message
            .contains("project run manifest head belongs to project_id="),
        "changed release must refuse rather than silently reuse old residuals: {error:?}"
    );
}

#[test]
fn changed_warehouse_rows_reuse_only_the_unaffected_bounded_section_prefix() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("first rows run");

    let mut changed_rows = warehouse_rows();
    changed_rows.max_assignments = 64;
    let changed = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(temp.path()),
        run_bindings(changed_rows),
    ))
    .expect("changed rows run");
    let report = changed.project_run_report.as_ref().expect("project report");
    assert!(
        report
            .resumed_nodes
            .contains(&"geo.building.home_cells".to_string())
    );
    assert!(
        report
            .resumed_nodes
            .contains(&"geo.building.section".to_string())
    );
    for node_id in [
        "geo.building.materialize_evidence",
        "geo.building.compile_evidence",
        "geo.building.solve",
    ] {
        assert!(report.executed_nodes.contains(&node_id.to_string()));
    }
}

#[test]
fn failed_candidate_reach_reference_blocks_downstream_and_refreshes_section() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect("baseline run");
    let baseline_section_digest =
        read_node_receipt(&receipt_path(temp.path(), "geo.building.section"))
            .expect("baseline section receipt")
            .outputs[0]
            .content_digest
            .clone();

    let mut bindings = run_bindings(warehouse_rows());
    replace_section_binding(
        &mut bindings,
        tile_work_request_value_with_reference(
            "reference.fixture.missing",
            vec![tile_feature_ref("building-c", outside_k1_cell())],
        ),
    );

    let failed = run_geo_plan(GeoRunRequest::new(plan, policy(temp.path()), bindings))
        .expect("reach failure is represented as a failed Geo run");
    assert_eq!(failed.status, GeoRunStatus::Failed);
    let report = failed.project_run_report.as_ref().expect("project report");
    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        report
            .executed_nodes
            .contains(&"geo.building.section".to_string()),
        "changed reference bytes must re-execute the bounded-section node"
    );
    assert!(
        !report
            .resumed_nodes
            .contains(&"geo.building.section".to_string()),
        "changed reference bytes must not reuse stale section reach"
    );
    let changed_section_digest =
        read_node_receipt(&receipt_path(temp.path(), "geo.building.section"))
            .expect("changed section receipt")
            .outputs[0]
            .content_digest
            .clone();
    assert_ne!(changed_section_digest, baseline_section_digest);

    let section = section_output(temp.path());
    assert_eq!(
        section["candidate_reach"]["status"],
        "failed_against_reference"
    );
    assert_eq!(
        section["candidate_reach"]["truth_reach_status"],
        "failed_against_reference"
    );
    assert_eq!(section["candidate_reach"]["missing_reference_count"], 1);
    assert_eq!(
        section["candidate_reach"]["missing_reference_members"][0]["source"]["source_instance_id"],
        "building"
    );
    assert_eq!(
        section["candidate_reach"]["missing_reference_members"][0]["feature_id"],
        "building-c"
    );
}

#[test]
fn blocked_plans_project_waiting_or_unsupported_next_actions() {
    let waiting = building_plan("release.fixture.one", GeoSourceAvailability::Missing, None);
    let temp = tempfile::tempdir().expect("waiting tempdir");
    let waiting_run = run_geo_plan(GeoRunRequest::new(waiting, policy(temp.path()), Vec::new()))
        .expect("waiting run");
    assert_eq!(waiting_run.status, GeoRunStatus::WaitingForInput);
    assert!(waiting_run.next_actions.iter().any(|action| {
        action.kind == GeoRunNextActionKind::SatisfyAcquisition
            && action
                .expected_contract
                .as_deref()
                .is_some_and(|contract| contract == "canon_geo_acquisition_receipt.v0")
    }));
    let local_wait_temp = tempfile::tempdir().expect("local wait tempdir");
    let mut local_wait_progress = Vec::new();
    let local_wait = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(
            building_plan(
                "release.fixture.one",
                GeoSourceAvailability::Available,
                None,
            ),
            policy(local_wait_temp.path()),
            Vec::new(),
        ),
        &mut local_wait_progress,
    )
    .expect("local input wait");
    assert_eq!(local_wait.status, GeoRunStatus::WaitingForInput);
    let terminal = progress_events(&local_wait_progress)
        .into_iter()
        .last()
        .expect("waiting terminal event");
    assert_eq!(terminal.kind, GeoRunProgressEventKind::WaitingForInput);
    assert_eq!(terminal.status, Some(GeoRunStatus::WaitingForInput));
    assert!(terminal.project_node_id.is_some());
    assert!(
        terminal.stage.is_some(),
        "known blocker node must name its Geo stage"
    );
    assert!(
        terminal
            .wait_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty())
    );

    let unsupported = parcel_only_with_building_profile_plan();
    assert_eq!(unsupported.status, GeoPlanStatus::Unsupported);
    let unsupported_temp = tempfile::tempdir().expect("unsupported tempdir");
    let unsupported_run = run_geo_plan(GeoRunRequest::new(
        unsupported,
        policy(unsupported_temp.path()),
        Vec::new(),
    ))
    .expect("unsupported run");
    assert_eq!(unsupported_run.status, GeoRunStatus::UnsupportedGrain);
    assert!(
        unsupported_run
            .next_actions
            .iter()
            .any(|action| { action.kind == GeoRunNextActionKind::UnsupportedGrain })
    );
}

#[test]
fn unknown_geo_command_refuses_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = geo_plan_with_unknown_home_cell_command();

    let error = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect_err("unknown command refuses");

    assert_eq!(error.code, GeoRunErrorCode::OutputContractViolation);
    assert!(!temp.path().join("geo/building/home_cells.json").exists());
}

#[test]
fn post_start_failure_emits_terminal_event_even_when_stage_is_unknown() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut invalid_selection_policy = policy(temp.path());
    invalid_selection_policy
        .selected_nodes
        .insert("geo.building.unknown".to_string());
    let mut progress = Vec::new();

    let error = run_geo_plan_with_progress_writer(
        GeoRunRequest::new(
            plan,
            invalid_selection_policy,
            run_bindings(warehouse_rows()),
        ),
        &mut progress,
    )
    .expect_err("unknown selected node refuses after run start");

    assert_eq!(error.code, GeoRunErrorCode::ProjectRunFailed);
    let events = progress_events(&progress);
    assert_eq!(
        events.first().unwrap().kind,
        GeoRunProgressEventKind::RunStarted
    );
    let terminal = events.last().expect("failure terminal event");
    assert_eq!(terminal.kind, GeoRunProgressEventKind::RunFailed);
    assert_eq!(terminal.status, Some(GeoRunStatus::Failed));
    assert_eq!(
        terminal.project_node_id.as_deref(),
        Some("geo.building.unknown")
    );
    assert_eq!(terminal.stage, None, "the unknown node has no Geo overlay");
    assert!(
        terminal
            .wait_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty())
    );
}

#[test]
fn wrong_solve_output_id_refuses_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = geo_plan_with_wrong_solve_output_id();

    let error = run_geo_plan(GeoRunRequest::new(
        plan,
        policy(temp.path()),
        run_bindings(warehouse_rows()),
    ))
    .expect_err("wrong solve output id refuses");

    assert_eq!(error.code, GeoRunErrorCode::ArtifactContract);
    assert!(!temp.path().join("geo/building/solve.json").exists());
}

#[test]
fn compile_and_solve_request_bindings_are_not_public_run_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let evidence_request =
        materialize_warehouse_rows(&warehouse_rows()).expect("materialized evidence request");
    let mut bindings = run_bindings(warehouse_rows());
    bindings.push(
        GeoRunArtifactBinding::from_json(
            "geo.building.compile_evidence",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_EVIDENCE_REQUEST_VERSION,
            &evidence_request,
        )
        .expect("compile override binding"),
    );

    let error = run_geo_plan(GeoRunRequest::new(
        plan.clone(),
        policy(temp.path()),
        bindings,
    ))
    .expect_err("compile override refuses");
    assert_eq!(error.code, GeoRunErrorCode::ArtifactContract);
    assert!(error.message.contains("not declared"));
    assert!(
        !temp
            .path()
            .join("geo/building/compile_evidence.json")
            .exists()
    );

    let mut bindings = run_bindings(warehouse_rows());
    bindings.push(
        GeoRunArtifactBinding::from_json(
            "geo.building.solve",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_EVIDENCE_REQUEST_VERSION,
            &evidence_request,
        )
        .expect("solve override binding"),
    );
    let error = run_geo_plan(GeoRunRequest::new(plan, policy(temp.path()), bindings))
        .expect_err("solve override refuses");
    assert_eq!(error.code, GeoRunErrorCode::ArtifactContract);
    assert!(error.message.contains("not declared"));
    assert!(!temp.path().join("geo/building/solve.json").exists());
}

#[cfg(unix)]
#[test]
fn local_input_binding_symlink_escape_is_refused() {
    use std::os::unix::fs as unix_fs;

    let temp = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside tempdir");
    let outside_file = outside.path().join("rows.json");
    fs::write(
        &outside_file,
        serde_json::to_vec(&home_cell_rows()).expect("rows json"),
    )
    .expect("outside file");
    unix_fs::symlink(&outside_file, temp.path().join("escape.json")).expect("symlink");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut binding = GeoRunArtifactBinding::from_bytes(
        "geo.building.home_cells",
        GEO_ROWS_BINDING_ID,
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        Vec::new(),
    )
    .with_local_path("escape.json");
    binding.content_digest = digest_bytes(&fs::read(&outside_file).expect("outside bytes"));
    binding.byte_count = fs::metadata(&outside_file).expect("metadata").len();

    let error = run_geo_plan(GeoRunRequest::new(plan, policy(temp.path()), vec![binding]))
        .expect_err("symlink escape refuses");

    assert_eq!(error.code, GeoRunErrorCode::InvalidInput);
    assert!(error.message.contains("workspace safety"));
}

#[test]
fn injected_executor_contract_weak_output_refuses_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut executor = ContractWeakProjectExecutor;

    let run = run_geo_plan_with_project_executor(
        GeoRunRequest::new(plan, policy(temp.path()), run_bindings(warehouse_rows())),
        &mut executor,
    )
    .expect("project failure projects into a typed Geo run");

    assert_eq!(run.status, GeoRunStatus::Failed);
    assert!(run.output_refs.is_empty());
    assert!(
        !temp.path().join("geo/building/home_cells.json").exists(),
        "contract-weak injected output must not be published"
    );
}

struct FailingProjectExecutor {
    message: String,
}

struct ContractWeakProjectExecutor;

struct AlwaysFailProgressWriter;

impl std::io::Write for AlwaysFailProgressWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("progress sink unavailable"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl ProjectNodeExecutor for ContractWeakProjectExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        _context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult> {
        let mut outputs = BTreeMap::new();
        outputs.insert(
            node.outputs[0].output_id.clone(),
            serde_json::to_vec(&serde_json::json!({
                "version": "canon_geo_home_cell_assignment.v1"
            }))
            .expect("fixture serializes"),
        );
        Ok(ProjectNodeExecutionResult::with_outputs(outputs))
    }
}

impl ProjectNodeExecutor for FailingProjectExecutor {
    fn execute(
        &mut self,
        node: &ProjectPlanNode,
        _context: &ProjectNodeExecutionContext,
    ) -> ProjectRunResult<ProjectNodeExecutionResult> {
        Err(ProjectRunError::new(
            ProjectRunErrorCode::ArtifactContract,
            Some(node.node_id.clone()),
            self.message.clone(),
        ))
    }
}

fn run_bindings(rows: GeoWarehouseRowsRequest) -> Vec<GeoRunArtifactBinding> {
    vec![
        GeoRunArtifactBinding::from_json(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        )
        .expect("home cell binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        )
        .expect("tile work binding"),
        GeoRunArtifactBinding::from_json(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        )
        .expect("warehouse rows binding"),
    ]
}

fn client_tile_run_bindings(
    request: &GeoClientTileIngestRequest,
    source: &[u8],
) -> Vec<GeoRunArtifactBinding> {
    vec![
        GeoRunArtifactBinding::from_json(
            CLIENT_TILE_NODE_ID,
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION,
            request,
        )
        .expect("client request binding"),
        GeoRunArtifactBinding::from_bytes(
            CLIENT_TILE_NODE_ID,
            GEO_CLIENT_TILE_SOURCE_BINDING_ID,
            CANON_GEO_CLIENT_TILE_SOURCE_VERSION,
            source.to_vec(),
        ),
    ]
}

fn client_tile_source_and_request() -> (GeoClientTileIngestRequest, String, String, String) {
    let center = LatLng::new(40.753000, -73.977000)
        .expect("valid client tile fixture point")
        .to_cell(Resolution::Nine)
        .to_string();
    let mut work_cells = CellIndex::from_str(&center)
        .expect("fixture center parses")
        .grid_disk::<Vec<_>>(1)
        .into_iter()
        .map(|cell| cell.to_string())
        .collect::<Vec<_>>();
    work_cells.sort();
    let neighbor = work_cells
        .iter()
        .find(|cell| cell.as_str() != center.as_str())
        .expect("fixture k1 neighbor")
        .clone();
    let source = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "id": "client-row-1",
                "properties": {
                    "apn": "client-apn-1",
                    "supplemental_cells": [neighbor]
                },
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[
                        [-73.977100, 40.752900],
                        [-73.976900, 40.752900],
                        [-73.976900, 40.753100],
                        [-73.977100, 40.753100],
                        [-73.977100, 40.752900]
                    ]]
                }
            },
            {
                "type": "Feature",
                "id": "client-row-2",
                "properties": { "apn": "client-apn-2" },
                "geometry": {
                    "type": "Point",
                    "coordinates": [-73.977000, 40.753000]
                }
            }
        ]
    })
    .to_string();
    let request = GeoClientTileIngestRequest {
        version: CANON_GEO_CLIENT_TILE_INGEST_REQUEST_VERSION.to_string(),
        tile_id: center.clone(),
        source_format: GeoClientTileSourceFormat::GeoJson,
        source_path: "client/parcels.geojson".to_string(),
        source_digest: blake3::hash(source.as_bytes()).to_hex().to_string(),
        declared_crs: "EPSG:4326".to_string(),
        frame: wgs84_client_frame(&center),
        source_instance_id: "source.client.parcels".to_string(),
        release_id: "client-parcels-2026-q3".to_string(),
        release_digest: blake3::hash(b"client-parcels-2026-q3").to_hex().to_string(),
        vendor: "county".to_string(),
        vintage: "2026-Q3".to_string(),
        vendor_identifier: GeoClientTileVendorIdentifier {
            issuer: "county".to_string(),
            role: "apn".to_string(),
            property: "apn".to_string(),
        },
        source_record_id_property: None,
        supplemental_h3_cells_property: Some("supplemental_cells".to_string()),
        license_expression: "LicenseRef-Client-Parcel-Local".to_string(),
        coverage_extent: GeoClientTileCoverageExtent {
            extent_id: "client-declared-h3-k1".to_string(),
            kind: GeoClientTileCoverageExtentKind::ClientDeclaredH3CellSet,
            h3_cells: work_cells.clone(),
        },
        mutual_exclusivity_declared: false,
        h3_resolution: 9,
        halo_k: 1,
        work_cells,
        max_features: 8,
        max_vertices_per_geometry: 64,
        max_geometry_bytes_per_tile: 100_000,
    };
    (request, source, center, neighbor)
}

fn wgs84_client_frame(tile_id: &str) -> GeoLocalFrameContract {
    GeoLocalFrameContract {
        version: CANON_GEO_LOCAL_FRAME_VERSION.to_string(),
        frame_id: format!("client:{tile_id}:wgs84-local-affine:v0"),
        tile_id: tile_id.to_string(),
        source_crs: "EPSG:4326".to_string(),
        source_axis_domain: GeoSourceAxisDomain::GeographicLongitudeLatitude,
        source_decimal_places: 6,
        source_origin: GeoSourcePointFixed {
            x: -74_000_000,
            y: 40_000_000,
        },
        affine: GeoAffineProjectionMm {
            x_from_source_x_numerator: 1,
            x_from_source_y_numerator: 0,
            y_from_source_x_numerator: 0,
            y_from_source_y_numerator: 1,
            denominator: 1,
        },
        projection: GeoProjectionProvenance {
            method_id: "fixture:wgs84-local-affine".to_string(),
            method_version: "v0".to_string(),
            parameters_blake3: blake3::hash(format!("fixture:{tile_id}").as_bytes())
                .to_hex()
                .to_string(),
            max_projection_error_micrometres: 10_000_000,
        },
        max_abs_coordinate_mm: 10_000_000,
    }
}

fn replace_section_binding(bindings: &mut [GeoRunArtifactBinding], request: Value) {
    let binding = bindings
        .iter_mut()
        .find(|binding| binding.node_id == "geo.building.section")
        .expect("section binding");
    *binding = GeoRunArtifactBinding::from_json(
        "geo.building.section",
        GEO_REQUEST_BINDING_ID,
        CANON_GEO_TILE_WORK_REQUEST_VERSION,
        &request,
    )
    .expect("tile work binding");
}

fn tile_work_request_value_with_reference(
    reference_id: &str,
    members: Vec<GeoTileFeatureRef>,
) -> Value {
    let mut value = serde_json::to_value(tile_work_request()).expect("tile request json");
    value.as_object_mut().expect("tile request object").insert(
        "candidate_reach_reference".to_string(),
        serde_json::json!({
            "reference_id": reference_id,
            "reference_kind": "complete_bounded_reference",
            "members": members,
            "max_members": 16
        }),
    );
    value
}

fn policy(workspace: &Path) -> ProjectRunPolicy {
    let mut policy = ProjectRunPolicy::new(workspace, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;
    policy
}

fn solve_output(workspace: &Path) -> Value {
    serde_json::from_slice(&fs::read(workspace.join("geo/building/solve.json")).expect("solve"))
        .expect("solve json")
}

fn section_output(workspace: &Path) -> Value {
    serde_json::from_slice(&fs::read(workspace.join("geo/building/section.json")).expect("section"))
        .expect("section json")
}

fn receipt_path(workspace: &Path, node_id: &str) -> std::path::PathBuf {
    workspace
        .join("work/receipts")
        .join(format!("{}.json", node_id_token(node_id)))
}

fn node_id_token(node_id: &str) -> String {
    node_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn progress_events(bytes: &[u8]) -> Vec<GeoRunProgressEvent> {
    std::str::from_utf8(bytes)
        .expect("progress utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("progress event"))
        .collect()
}

fn restamp_geo_run(run: &mut GeoRun) {
    run.semantic_hash = geo_run_semantic_hash(run).expect("semantic hash");
    run.run_id = format!(
        "canon_geo_run.v0:{}",
        run.semantic_hash.trim_start_matches("blake3:")
    );
}

fn building_plan(
    release_label: &str,
    availability: GeoSourceAvailability,
    query_as_of: Option<&str>,
) -> GeoPlan {
    let query_as_of = query_as_of.or(Some("2026-08-31"));
    compile_geo_plan(GeoPlanRequest {
        question: question(vec![GeoControlEntityLevel::Building], query_as_of),
        capabilities: default_geo_capabilities().expect("capabilities"),
        inventory: inventory(release_label, availability),
        profile: GeoCompositionProfile::building(),
        budget: budget(),
    })
    .expect("Geo plan compiles")
}

fn parcel_only_with_building_profile_plan() -> GeoPlan {
    compile_geo_plan(GeoPlanRequest {
        question: question(vec![GeoControlEntityLevel::Parcel], None),
        capabilities: default_geo_capabilities().expect("capabilities"),
        inventory: inventory("release.fixture.one", GeoSourceAvailability::Available),
        profile: GeoCompositionProfile::building(),
        budget: budget(),
    })
    .expect("unsupported Geo plan compiles")
}

fn geo_plan_with_unknown_home_cell_command() -> GeoPlan {
    let mut plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    plan.project_plan = project_plan_with_command(
        &plan.project_plan,
        "geo.building.home_cells",
        "canon geo unknown-leaf --request <REQUEST.json>",
    );
    plan.semantic_hash = geo_plan_semantic_hash(&plan).expect("plan hash");
    plan.plan_id = format!(
        "canon_geo_plan.v0:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    plan
}

fn geo_plan_with_wrong_solve_output_id() -> GeoPlan {
    let mut plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    plan.project_plan = project_plan_with_node_override(
        &plan.project_plan,
        "geo.building.solve",
        None,
        Some("not_solve"),
    );
    plan.semantic_hash = geo_plan_semantic_hash(&plan).expect("plan hash");
    plan.plan_id = format!(
        "canon_geo_plan.v0:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    plan
}

fn client_tile_ingest_plan() -> GeoPlan {
    let mut plan = building_plan(
        "release.fixture.one",
        GeoSourceAvailability::Available,
        None,
    );
    let mut overlay = plan
        .geo_nodes
        .iter()
        .find(|overlay| overlay.project_node_id == "geo.building.home_cells")
        .expect("home-cell overlay")
        .clone();
    overlay.project_node_id = CLIENT_TILE_NODE_ID.to_string();
    overlay.stage = GeoPlanStage::BuildBoundedSection;
    overlay.entity_level = Some(GeoControlEntityLevel::Parcel);
    overlay.evidence_classes = vec![GeoEvidenceClass::ParcelGeometry];
    overlay.claim_classes = vec![GeoClaimClass::CollateralComposition];
    overlay.expected_output_contract = CANON_GEO_GEOMETRY_TILE_VERSION.to_string();
    overlay.bounded_section_required = false;
    overlay.incidence_factorization_required = false;
    overlay.exact_solve_scope = None;

    plan.project_plan =
        compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
            "geo-client-tile-stage-fixture",
            digest_bytes(b"geo client tile stage manifest"),
            digest_bytes(b"geo client tile stage lock"),
            vec![client_tile_stage_node(&overlay.deterministic_bounds)],
        ))
        .expect("client tile project plan compiles");
    plan.geo_nodes = vec![overlay];
    let mut outcome = plan
        .grain_outcomes
        .first()
        .expect("building outcome")
        .clone();
    outcome.entity_level = GeoControlEntityLevel::Parcel;
    outcome.missing_evidence_classes = Vec::new();
    outcome.project_node_ids = vec![CLIENT_TILE_NODE_ID.to_string()];
    outcome.claim_limitation =
        "client tile ingest indexes only the declared local source bytes and coverage extent"
            .to_string();
    outcome.next_action = "execute the client tile ingest stage through geo run".to_string();
    plan.grain_outcomes = vec![outcome];
    plan.external_requests = Vec::new();
    plan.diagnostics = Vec::new();
    plan.status = GeoPlanStatus::Planned;
    plan.semantic_hash = geo_plan_semantic_hash(&plan).expect("client tile plan semantic hash");
    plan.plan_id = format!(
        "canon_geo_plan.v0:{}",
        plan.semantic_hash.trim_start_matches("blake3:")
    );
    canon::geo::validate_geo_plan(&plan).expect("client tile plan validates");
    plan
}

fn client_tile_stage_node(bounds: &[GeoNumericBound]) -> ProjectExtensionDagNode {
    ProjectExtensionDagNode {
        node_id: CLIENT_TILE_NODE_ID.to_string(),
        kind: ProjectPlanNodeKind::Index,
        class: ProjectPlanNodeClass::Computation,
        command: GEO_CLIENT_TILE_INGEST_STAGE_COMMAND.to_string(),
        dependencies: Vec::new(),
        content_hash_inputs: vec![ProjectPlanHashRef {
            ref_id: "geo.fixture.client_tile_inputs".to_string(),
            content_hash: digest_bytes(b"geo client tile fixture inputs"),
        }],
        outputs: vec![ProjectExtensionDagOutput {
            output_id: CLIENT_TILE_OUTPUT_ID.to_string(),
            path: CLIENT_TILE_OUTPUT_PATH.to_string(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: bounds
            .iter()
            .map(|bound| (bound.semantic_id.clone(), bound.value))
            .collect(),
        cache_eligible: true,
        side_effects: vec![
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::ReadsInput,
                description: "reads declared local client source bytes and request".to_string(),
            },
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::WritesArtifact,
                description: "publishes one canonical client geometry tile".to_string(),
            },
        ],
        refusal_conditions: vec![ProjectPlanRefusalCondition {
            code: ProjectPlanErrorCode::ArtifactContract,
            message: "refuse on client tile request, source, or output contract mismatch"
                .to_string(),
            next_command: None,
        }],
    }
}

fn project_plan_with_command(
    project_plan: &ProjectPlan,
    node_id: &str,
    command: &str,
) -> ProjectPlan {
    project_plan_with_node_override(project_plan, node_id, Some(command), None)
}

fn project_plan_with_node_override(
    project_plan: &ProjectPlan,
    node_id: &str,
    command: Option<&str>,
    output_id: Option<&str>,
) -> ProjectPlan {
    let nodes = project_plan
        .nodes
        .iter()
        .map(|node| {
            let dependency_refs = node
                .dependencies
                .iter()
                .flat_map(|dependency_id| {
                    project_plan
                        .nodes
                        .iter()
                        .find(|candidate| candidate.node_id == *dependency_id)
                        .into_iter()
                        .flat_map(move |dependency| {
                            dependency.outputs.iter().map(move |output| {
                                format!("node.{dependency_id}.{}", output.output_id)
                            })
                        })
                })
                .collect::<std::collections::BTreeSet<_>>();
            ProjectExtensionDagNode {
                node_id: node.node_id.clone(),
                kind: node.kind,
                class: node.class,
                command: if node.node_id == node_id {
                    command.unwrap_or(&node.command).to_string()
                } else {
                    node.command.clone()
                },
                dependencies: node.dependencies.clone(),
                content_hash_inputs: node
                    .content_hash_inputs
                    .iter()
                    .filter(|input| !dependency_refs.contains(&input.ref_id))
                    .cloned()
                    .collect(),
                outputs: node
                    .outputs
                    .iter()
                    .map(|output| ProjectExtensionDagOutput {
                        output_id: if node.node_id == node_id {
                            output_id.unwrap_or(&output.output_id).to_string()
                        } else {
                            output.output_id.clone()
                        },
                        path: output.path.clone(),
                        materialization: output.materialization,
                    })
                    .collect(),
                limits: node.limits.clone(),
                cache_eligible: node.cache.eligible,
                side_effects: node.side_effects.clone(),
                refusal_conditions: node.refusal_conditions.clone(),
            }
        })
        .collect();
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        project_plan.project_id.clone(),
        project_plan.manifest_digest.clone(),
        project_plan.lock_digest.clone(),
        nodes,
    ))
    .expect("unknown command extension plan compiles")
}

fn digest(label: &str) -> String {
    digest_bytes(label.as_bytes())
}

fn digest_hex(label: &str) -> String {
    digest(label).trim_start_matches("blake3:").to_string()
}

fn acquisition_satisfaction_for_binding(
    binding: &GeoRunArtifactBinding,
) -> GeoRunAcquisitionSatisfactionRef {
    let semantic_hash = digest("satisfaction.fixture.warehouse.semantic");
    let request_id = format!(
        "canon_geo_acquisition_request.v0:{}",
        digest_hex("request.fixture.warehouse")
    );
    let local_artifact_id = "local.artifact.warehouse_rows".to_string();
    let result_digest_id = "result.fixture.warehouse_rows".to_string();
    let result_hex = binding
        .content_digest
        .trim_start_matches("blake3:")
        .to_string();

    GeoRunAcquisitionSatisfactionRef {
        satisfaction_id: format!(
            "{CANON_GEO_ACQUISITION_SATISFACTION_VERSION}:{}",
            semantic_hash.trim_start_matches("blake3:")
        ),
        semantic_hash,
        status: GeoSatisfactionStatus::Satisfied,
        request_id: request_id.clone(),
        request_semantic_hash: digest("request.fixture.warehouse.semantic"),
        expected_receipt_contract: CANON_GEO_ACQUISITION_RECEIPT_VERSION.to_string(),
        receipt_terminal_state: GeoAcquisitionTerminalState::Complete,
        proof_class: GeoAcquisitionProofClass::Fixture,
        receipt_file: GeoSatisfactionFileAudit {
            file_id: "receipt".to_string(),
            byte_count: 128,
            digest: digest("receipt.fixture.warehouse"),
        },
        local_artifacts: vec![GeoSatisfactionFileAudit {
            file_id: local_artifact_id.clone(),
            byte_count: binding.byte_count,
            digest: binding.content_digest.clone(),
        }],
        result_files: Vec::new(),
        source_digests: vec![GeoDigest {
            digest_id: "source.fixture.catalog".to_string(),
            algorithm: GeoDigestAlgorithm::Sha256,
            hex_digest: digest_hex("source.fixture.catalog"),
        }],
        result_digests: vec![GeoDigest {
            digest_id: result_digest_id.clone(),
            algorithm: GeoDigestAlgorithm::Blake3,
            hex_digest: result_hex,
        }],
        denominators: vec![GeoAcquisitionDenominator {
            denominator_id: "denominator.fixture.rows".to_string(),
            source: GeoDenominatorSource::RequestedSubset,
            count: 2,
            unit: "row".to_string(),
            description: "fixture requested warehouse rows".to_string(),
        }],
        bindings: vec![GeoSatisfactionLocalInputBinding {
            binding_id: format!(
                "geo-local-binding:{}",
                digest_hex("local.binding.warehouse")
            ),
            request_id: request_id.clone(),
            request_semantic_hash: digest("request.fixture.warehouse.semantic"),
            receipt_terminal_state: GeoAcquisitionTerminalState::Complete,
            proof_class: GeoAcquisitionProofClass::Fixture,
            source_instance_id: "source.fixture.catalog".to_string(),
            release_id: "release.fixture.catalog".to_string(),
            release_digest: format!("sha256:{}", digest_hex("release.fixture.catalog")),
            local_artifact_id: local_artifact_id.clone(),
            media_type: binding.media_type.clone(),
            artifact_contract_version: Some(binding.contract_version.clone()),
            content_hash: binding.content_digest.clone(),
            byte_count: binding.byte_count,
            result_digest_ids: vec![result_digest_id],
        }],
        run_input_refs: vec![GeoSatisfactionRunInputRef {
            node_id: binding.node_id.clone(),
            binding_id: binding.binding_id.clone(),
            artifact_id: binding.artifact_id.clone(),
            contract_version: binding.contract_version.clone(),
            media_type: binding.media_type.clone(),
            content_hash: binding.content_digest.clone(),
            byte_count: binding.byte_count,
            local_artifact_id,
        }],
        receipt_execution: GeoSatisfactionExecutionRef {
            proof_class: GeoAcquisitionProofClass::Fixture,
            terminal_state: GeoAcquisitionTerminalState::Complete,
            fixture_id: Some("fixture.acquisition.receipt".to_string()),
            retained_receipt_id: None,
            executor_request_id: None,
            executor_query_id: None,
            executor_attempt_id: None,
        },
        findings: vec![GeoSatisfactionFinding {
            code: GeoSatisfactionFindingCode::Satisfied,
            detail: BTreeMap::new(),
        }],
    }
}

fn region() -> GeoBoundedGeography {
    GeoBoundedGeography {
        geography_id: "region.fixture.one".to_string(),
        geography_kind: "bounded_fixture".to_string(),
        description: "One explicitly bounded planning fixture".to_string(),
    }
}

fn question(
    grains: Vec<GeoControlEntityLevel>,
    query_as_of: Option<&str>,
) -> canon::geo::GeoQuestion {
    canon::geo::GeoQuestion {
        version: CANON_GEO_QUESTION_VERSION.to_string(),
        question_id: "question.fixture.run".to_string(),
        subject_bindings: vec![GeoSubjectBinding {
            role: "target".to_string(),
            binding_class: GeoSubjectBindingClass::OperatorLabel,
            value: "fixture subject".to_string(),
        }],
        bounded_geography: region(),
        requested_grains: grains
            .into_iter()
            .map(|entity_level| GeoRequestedGrain {
                entity_level,
                required_evidence_classes: vec![match entity_level {
                    GeoControlEntityLevel::Building => GeoEvidenceClass::BuildingFootprint,
                    GeoControlEntityLevel::Parcel => GeoEvidenceClass::ParcelGeometry,
                    GeoControlEntityLevel::Site
                    | GeoControlEntityLevel::Property
                    | GeoControlEntityLevel::Unit
                    | GeoControlEntityLevel::Address
                    | GeoControlEntityLevel::Poi => GeoEvidenceClass::AddressSet,
                }],
                optional_evidence_classes: Vec::new(),
            })
            .collect(),
        query_as_of: query_as_of.map(|utc_day| GeoAsOf {
            utc_day: utc_day.to_string(),
            semantic_id: "query.as_of".to_string(),
            unit: "utc_day".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
        }),
        requested_claim_classes: vec![GeoClaimClass::CollateralComposition],
        presentation_limits: vec![GeoNumericBound {
            semantic_id: "presentation.max_models".to_string(),
            counter: GeoResourceCounter::Models,
            value: 16,
            unit: "model".to_string(),
            origin: GeoValueOrigin::CallerDeclared,
            action: GeoBudgetAction::TruncatePresentationOnly,
        }],
        abstention_policy: GeoAbstentionPolicy {
            unsupported_grain: GeoAbstentionDisposition::ReportUnsupported,
            unresolved_residual: GeoAbstentionDisposition::ReportResidual,
            budget_fallback: GeoAbstentionDisposition::ReportResidual,
        },
        decision_policy: None,
        resource_budget_ref: "budget.fixture.run".to_string(),
    }
}

fn budget() -> GeoResourceBudget {
    GeoResourceBudget {
        version: CANON_GEO_RESOURCE_BUDGET_VERSION.to_string(),
        budget_id: "budget.fixture.run".to_string(),
        deterministic_bounds: vec![
            bound("budget.max_bytes", GeoResourceCounter::Bytes, 1_000_000),
            bound("budget.max_rows", GeoResourceCounter::Rows, 10_000),
            bound("budget.max_cells", GeoResourceCounter::Cells, 64),
            bound("budget.max_candidates", GeoResourceCounter::Candidates, 500),
            bound("budget.max_variables", GeoResourceCounter::Variables, 128),
            bound("budget.max_states", GeoResourceCounter::States, 100_000),
            bound("budget.max_models", GeoResourceCounter::Models, 10_000),
            bound(
                "budget.max_operations",
                GeoResourceCounter::Operations,
                1_000_000,
            ),
        ],
        telemetry: vec![GeoTelemetryDeclaration {
            metric: GeoTelemetryMetric::WallTime,
            unit: "millisecond".to_string(),
            origin: GeoValueOrigin::OperatorPolicy,
            semantic_effect: GeoTelemetrySemanticEffect::None,
        }],
    }
}

fn bound(id: &str, counter: GeoResourceCounter, value: u64) -> GeoNumericBound {
    GeoNumericBound {
        semantic_id: id.to_string(),
        counter,
        value,
        unit: format!("{counter:?}").to_lowercase(),
        origin: GeoValueOrigin::CallerDeclared,
        action: GeoBudgetAction::ReportBudgetFallback,
    }
}

fn inventory(release_label: &str, availability: GeoSourceAvailability) -> GeoRegionalInventory {
    GeoRegionalInventory {
        version: CANON_GEO_REGIONAL_INVENTORY_VERSION.to_string(),
        inventory_id: "inventory.fixture.run".to_string(),
        region: region(),
        sources: vec![GeoRegionalSourceInstance {
            source_instance_id: "arbitrary-building-source".to_string(),
            release: GeoSourceRelease {
                release_id: release_label.to_string(),
                release_digest: digest(release_label),
            },
            temporal_scope: GeoTemporalScope {
                valid_time: Some(GeoDateInterval {
                    start_utc_day: "2026-01-01".to_string(),
                    end_utc_day: "2026-12-31".to_string(),
                }),
                transaction_time: None,
                release_time: None,
            },
            lineage_ids: vec!["lineage.fixture.one".to_string()],
            native_scope: GeoNativeEntityScope::NativeEntity {
                entity_level: GeoControlEntityLevel::Building,
                identity_participation: GeoIdentityParticipation::StableAlias,
            },
            evidence_classes: vec![GeoEvidenceClass::BuildingFootprint],
            coverage: GeoCoveragePredicate {
                coverage_id: "coverage.fixture.one".to_string(),
                region: region(),
                predicate: "all declared fixture records".to_string(),
            },
            local_state: GeoLocalAcquisitionState {
                state: availability,
                local_ref: if availability == GeoSourceAvailability::Available {
                    Some(GeoLocalArtifactRef {
                        artifact_id: "artifact.building.fixture".to_string(),
                        contract_version: "canon_geo_warehouse_rows.v0".to_string(),
                        content_hash: digest("local.fixture.one"),
                        media_type: "application/json".to_string(),
                    })
                } else {
                    None
                },
            },
            geometry: Some(GeoGeometryTransformContract {
                geometry_contract_version: "geometry.fixture.v1".to_string(),
                coordinate_reference_system: "EPSG:4326".to_string(),
                transform_id: "identity.fixture".to_string(),
                transform_digest: digest("identity.fixture"),
                numeric_error_bounds: vec![GeoNumericMeasure {
                    semantic_id: "transform.error".to_string(),
                    value: 0,
                    unit: "millimeter".to_string(),
                    origin: GeoValueOrigin::AdapterContract,
                }],
            }),
            license_class: GeoLicenseClass::PublicRedistributable,
            egress_class: GeoEgressClass::Shareable,
            estimates: vec![GeoNumericMeasure {
                semantic_id: "source.rows".to_string(),
                value: 100,
                unit: "row".to_string(),
                origin: GeoValueOrigin::SourceRelease,
            }],
        }],
        discovery_gaps: Vec::new(),
    }
}

fn center_cell() -> CellIndex {
    CellIndex::from_str("892a100d62bffff").expect("valid fixture cell")
}

fn outside_k1_cell() -> CellIndex {
    let center = center_cell();
    let k1 = center.grid_disk_safe(1).collect::<BTreeSet<_>>();
    center
        .grid_disk_safe(2)
        .find(|cell| !k1.contains(cell))
        .expect("k2 contains a cell outside k1")
}

fn building_tile_source() -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: "building".to_string(),
        release: GeoSourceRelease {
            release_id: "fixture-release-2026-08-31".to_string(),
            release_digest: format!(
                "blake3:{}",
                blake3::hash(b"fixture-release-2026-08-31").to_hex()
            ),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level: GeoControlEntityLevel::Building,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture.run".to_string(),
            semantic_hash: digest("tile-inventory-semantic"),
            planning_hash: digest("tile-inventory-planning"),
        },
    }
}

fn tile_feature_ref(feature_id: &str, home_cell: CellIndex) -> GeoTileFeatureRef {
    GeoTileFeatureRef {
        source: building_tile_source(),
        feature_id: feature_id.to_string(),
        home_cell: home_cell.to_string(),
    }
}

fn tile_features() -> Vec<GeoTileFeatureRef> {
    let center = center_cell().to_string();
    vec![
        GeoTileFeatureRef {
            source: building_tile_source(),
            feature_id: "building-a".to_string(),
            home_cell: center.clone(),
        },
        GeoTileFeatureRef {
            source: building_tile_source(),
            feature_id: "building-b".to_string(),
            home_cell: center,
        },
    ]
}

fn tile_work_request() -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: center_cell().to_string(),
        halo_k: 1,
        features: tile_features(),
        candidate_reach_reference: None,
        max_features: 16,
        max_work_cells: 7,
    }
}

fn home_cell_rows() -> canon::geo::GeoHomeCellRowsRequest {
    canon::geo::GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 9,
        stability_radius_fixed: 1_000,
        rows: vec![
            home_cell_row("building-a", "rec-building-a"),
            home_cell_row("building-b", "rec-building-b"),
        ],
        max_rows: 16,
    }
}

fn home_cell_row(feature_id: &str, source_record_id: &str) -> canon::geo::GeoHomeCellRow {
    canon::geo::GeoHomeCellRow {
        source: building_tile_source(),
        feature_id: feature_id.to_string(),
        source_record_id: source_record_id.to_string(),
        geometry_sha256: "5ed87d37d872789086452c35f658f5628ba870ca36072c495bb88519592403ed"
            .to_string(),
        representative_point_method: "centroid_of_derived_wgs84_geometry".to_string(),
        longitude: "-73.977264000".to_string(),
        latitude: "40.753429000".to_string(),
        transform_execution_id: Some("fixture-transform-execution".to_string()),
        transform_definition_id: Some("fixture-transform-definition".to_string()),
        claimed_home_cell: Some(center_cell().to_string()),
    }
}

fn warehouse_rows() -> GeoWarehouseRowsRequest {
    let observation = GeoRhoObservationKind::ExactSets {
        level: canon::geo::GeoEntityLevel::Building,
        sets: vec![vec!["building-a".to_string(), "building-b".to_string()]],
    };
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::building(),
        parcel_rows: Vec::new(),
        building_parcel_rows: vec![
            GeoWarehouseBuildingParcelRow {
                building_id: "building-b".to_string(),
                parcel_id: None,
            },
            GeoWarehouseBuildingParcelRow {
                building_id: "building-a".to_string(),
                parcel_id: None,
            },
        ],
        contracts: vec![rho_contract()],
        evidence_rows: vec![
            GeoWarehouseEvidenceRow {
                observation_id: "obs.building-set".to_string(),
                contract_id: "rho.building-set".to_string(),
                source_record: record("row-b"),
                valid_time: None,
                observation: observation.clone(),
            },
            GeoWarehouseEvidenceRow {
                observation_id: "obs.building-set".to_string(),
                contract_id: "rho.building-set".to_string(),
                source_record: record("row-a"),
                valid_time: None,
                observation,
            },
        ],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn rho_contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.building-set".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "fixture.buildings".to_string(),
        source_release: "2026-08-31".to_string(),
        source_lineage_ids: vec!["fixture.buildings.release".to_string()],
        method_id: "fixture-building-candidate-set".to_string(),
        method_version: "1.0.0".to_string(),
        claim_role: GeoEvidenceClaimRole::StableIdentityAnchor,
        basis: GeoRhoBasis::LogicalRelaxation {
            invariant_id: "candidate-set-is-a-superset".to_string(),
        },
    }
}

fn record(id: &str) -> GeoEvidenceRecordRef {
    GeoEvidenceRecordRef {
        source_record_id: id.to_string(),
        source_vintage: "2026-08-31".to_string(),
        record_blake3: blake3::hash(id.as_bytes()).to_hex().to_string(),
    }
}
