#![forbid(unsafe_code)]

mod geo {
    pub use canon::geo::*;
}

mod project {
    pub use canon::project::*;
}

#[allow(dead_code)]
#[path = "../src/geo/executor.rs"]
mod executor;

use canon::{
    geo::{
        CANON_GEO_COLLATERAL_LEDGER_SEED_VERSION, CANON_GEO_COLLATERAL_LEDGER_VERSION,
        CANON_GEO_COMPOSITION_VERSION, CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        CANON_GEO_EVIDENCE_REQUEST_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION, DEFAULT_MAX_MATERIALIZED_MODELS, GeoCandidateReachStatus,
        GeoCollateralLedgerProofClass, GeoCollateralLedgerSeed, GeoCollateralLedgerSeedRow,
        GeoCompositionProfile, GeoControlEntityLevel, GeoEntityLevel, GeoEvidenceClaimRole,
        GeoEvidenceCompilationArtifact, GeoEvidenceRecordRef, GeoHomeCellRow,
        GeoHomeCellRowsRequest, GeoIdentityParticipation, GeoLedgerPropertyRef,
        GeoNativeEntityScope, GeoPlanComponentScope, GeoPlanExactSolveScope, GeoPlanInventoryRef,
        GeoPlanProducedArtifactRef, GeoPropagationBudget, GeoRhoBasis, GeoRhoContract,
        GeoRhoObservationKind, GeoSourceRelease, GeoSourceReleasePin, GeoTileFeatureRef,
        GeoTileSourceBinding, GeoTileWorkRequest, GeoTruthPlane, GeoValidTimeInterval,
        GeoWarehouseBuildingParcelRow, GeoWarehouseEvidenceRow, GeoWarehouseParcelRow,
        GeoWarehouseRowsRequest, canonical_collateral_ledger_seed_bytes,
        canonical_evidence_compilation_bytes, canonical_propagation_bytes,
        canonical_tile_work_unit_bytes, compile_evidence, materialize_tile_work_unit,
        materialize_warehouse_rows, propagate,
    },
    project::{
        ProjectDependencyOutput, ProjectExtensionDagNode, ProjectExtensionDagOutput,
        ProjectExtensionDagRequest, ProjectNodeExecutionContext, ProjectNodeExecutor, ProjectPlan,
        ProjectPlanErrorCode, ProjectPlanHashRef, ProjectPlanNodeClass, ProjectPlanNodeKind,
        ProjectPlanOutputMaterialization, ProjectPlanRefusalCondition, ProjectPlanSideEffect,
        ProjectPlanSideEffectKind, ProjectRunFailurePolicy, ProjectRunNodeOutcome,
        ProjectRunPolicy, compile_extension_project_plan, digest_bytes, read_node_receipt,
        run_project_plan,
    },
};
use executor::{
    GEO_COLLATERAL_LEDGER_OUTPUT_ID, GEO_COMPILE_EVIDENCE_COMMAND, GEO_LEDGER_STAGE_COMMAND,
    GEO_MATERIALIZE_EVIDENCE_COMMAND, GEO_MATERIALIZE_HOME_CELLS_COMMAND, GEO_PROPAGATE_OUTPUT_ID,
    GEO_PROPAGATE_STAGE_COMMAND, GEO_REQUEST_BINDING_ID, GEO_ROWS_BINDING_ID, GEO_SOLVE_COMMAND,
    GEO_TILE_WORK_COMMAND, GeoExecutorDependencyOutput, GeoExecutorInputBinding,
    GeoProjectNodeExecutor,
};
use h3o::CellIndex;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, str::FromStr};

#[test]
fn geo_project_node_executor_runs_the_six_planner_leaf_chain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let mut executor = executor_with_fixture_inputs();

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("geo chain run");

    assert_eq!(
        report.executed_nodes,
        vec![
            "geo.building.compile_evidence".to_string(),
            "geo.building.home_cells".to_string(),
            "geo.building.materialize_evidence".to_string(),
            "geo.building.propagate".to_string(),
            "geo.building.section".to_string(),
            "geo.building.solve".to_string(),
        ]
    );
    assert!(report.failed_nodes.is_empty());
    assert!(report.blocked_nodes.is_empty());
    assert!(report.next_actions.is_empty());

    let solve_bytes = fs::read(temp.path().join("geo/building/solve.json")).expect("solve output");
    let solve: Value = serde_json::from_slice(&solve_bytes).expect("solve json");
    assert_eq!(solve["version"], CANON_GEO_COMPOSITION_VERSION);
    assert_eq!(solve["status"], "resolved");
    assert_eq!(solve["summary"]["residual_model_count"], 1);
    assert_eq!(solve["summary"]["component_count"], 1);
    assert_eq!(solve["factorization"][0]["key"], "building:building-a");
    assert_eq!(
        solve["evidence_compilation"]["version"],
        "canon_geo_evidence_compilation.v0"
    );

    let solve_receipt =
        read_node_receipt(&receipt_path(temp.path(), "geo.building.solve")).expect("receipt");
    assert_eq!(solve_receipt.outcome, ProjectRunNodeOutcome::Completed);
    assert_eq!(
        solve_receipt
            .dependency_semantic_hashes
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "geo.building.compile_evidence".to_string(),
            "geo.building.propagate".to_string(),
            "geo.building.section".to_string()
        ]
    );
    assert_eq!(
        solve_receipt.deterministic_usage["bounded_section_work_cells"],
        7
    );
    assert_eq!(
        solve_receipt.deterministic_usage["composition_components"],
        1
    );
}

#[test]
fn geo_project_node_executor_resumes_with_verified_dependency_outputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let mut partial_policy = policy(temp.path());
    partial_policy
        .cancel_before_nodes
        .insert("geo.building.solve".to_string());
    let mut first_executor = executor_with_fixture_inputs();

    let partial =
        run_project_plan(&plan, &partial_policy, &mut first_executor).expect("partial run");

    assert_eq!(
        partial.cancelled_nodes,
        vec!["geo.building.solve".to_string()]
    );
    assert!(
        !temp.path().join("geo/building/solve.json").exists(),
        "cancelled solve must not publish output"
    );

    let mut resume_executor = executor_with_scope();
    preload_dependency_output(
        &mut resume_executor,
        temp.path(),
        "geo.building.section",
        "section",
        CANON_GEO_TILE_WORK_UNIT_VERSION,
    );

    let resumed =
        run_project_plan(&plan, &policy(temp.path()), &mut resume_executor).expect("resume run");

    assert_eq!(
        resumed.executed_nodes,
        vec!["geo.building.solve".to_string()]
    );
    assert!(resumed.failed_nodes.is_empty());
    assert!(
        resumed
            .resumed_nodes
            .contains(&"geo.building.home_cells".to_string())
    );
    assert!(
        resumed
            .resumed_nodes
            .contains(&"geo.building.compile_evidence".to_string())
    );
    assert!(
        resumed
            .resumed_nodes
            .contains(&"geo.building.propagate".to_string())
    );
    assert!(
        resumed
            .resumed_nodes
            .contains(&"geo.building.section".to_string())
    );
    let solve_receipt =
        read_node_receipt(&receipt_path(temp.path(), "geo.building.solve")).expect("receipt");
    assert_eq!(
        solve_receipt
            .dependency_semantic_hashes
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "geo.building.compile_evidence".to_string(),
            "geo.building.propagate".to_string(),
            "geo.building.section".to_string()
        ]
    );
    assert_eq!(solve_receipt.deterministic_usage["input_binding_count"], 0);
    assert_eq!(
        solve_receipt.deterministic_usage["bounded_section_work_cells"],
        7
    );
}

#[test]
fn geo_executor_refuses_to_infer_a_section_outside_exact_solve_scope() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let mut wrong_scope = exact_solve_scope();
    wrong_scope.bounded_section.producer_node_id = "geo.other.section".to_string();
    let mut executor =
        executor_with_fixture_inputs().with_exact_solve_scope("geo.building.solve", wrong_scope);

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(report.failed_nodes, vec!["geo.building.solve".to_string()]);
    assert!(
        node_report_reason(&report, "geo.building.solve").contains("geo.other.section:section")
    );
    assert!(
        !temp.path().join("geo/building/solve.json").exists(),
        "scope mismatch must refuse before publishing a solve artifact"
    );
}

#[test]
fn geo_executor_refuses_bad_input_digest_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let mut bad = binding(
        "geo.building.home_cells",
        GEO_ROWS_BINDING_ID,
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        &home_cell_rows(),
    );
    bad.content_hash = digest_bytes(b"different bytes");
    let mut executor = GeoProjectNodeExecutor::new().with_input_binding(bad);

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.home_cells".to_string()]
    );
    assert!(
        !temp.path().join("geo/building/home_cells.json").exists(),
        "failed node must not publish artifact bytes"
    );
    assert!(node_report_reason(&report, "geo.building.home_cells").contains("digest mismatch"));
}

#[test]
fn geo_executor_refuses_missing_binding_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = single_home_cell_plan();
    let mut executor = GeoProjectNodeExecutor::new();

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.home_cells".to_string()]
    );
    assert!(
        !temp.path().join("geo/building/home_cells.json").exists(),
        "missing binding must not publish artifact bytes"
    );
    assert!(
        node_report_reason(&report, "geo.building.home_cells").contains("requires binding rows")
    );
}

#[test]
fn geo_executor_refuses_unknown_command_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut plan = single_home_cell_plan();
    plan.nodes[0].command = "canon geo solve --request <REQUEST.json> --ambient".to_string();
    plan.nodes[0].cache.cache_key = digest_bytes(b"intentionally wrong for shape validation");
    plan.graph_hash = digest_bytes(b"unknown-command-plan");

    let mut executor = GeoProjectNodeExecutor::new();
    let error = run_project_plan(&plan, &policy(temp.path()), &mut executor)
        .expect_err("stale cache key refuses before executor");
    assert!(error.message.contains("node cache key"));

    let plan = single_unknown_command_plan();
    let mut executor = GeoProjectNodeExecutor::new();
    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");
    assert_eq!(report.failed_nodes, vec!["geo.bad.home_cells".to_string()]);
    assert!(
        !temp.path().join("geo/bad/home_cells.json").exists(),
        "unknown command must not publish an artifact"
    );
    assert!(
        node_report_reason(&report, "geo.bad.home_cells").contains("does not implement command")
    );
}

#[test]
fn geo_executor_refuses_wrong_binding_contract_and_unsafe_effects() {
    let mut plan = single_home_cell_plan();
    let node = plan.nodes.remove(0);
    let context = ProjectNodeExecutionContext {
        node_id: node.node_id.clone(),
        dependency_semantic_hashes: BTreeMap::new(),
        dependency_outputs: BTreeMap::new(),
    };
    let mut wrong_contract = GeoProjectNodeExecutor::new().with_input_binding(binding(
        "geo.building.home_cells",
        GEO_ROWS_BINDING_ID,
        CANON_GEO_EVIDENCE_REQUEST_VERSION,
        &home_cell_rows(),
    ));
    let error = wrong_contract
        .execute(&node, &context)
        .expect_err("wrong input contract refuses");
    assert!(error.message.contains("contract mismatch"));

    let mut unsafe_node = node.clone();
    unsafe_node.side_effects.push(ProjectPlanSideEffect {
        kind: ProjectPlanSideEffectKind::MayUseNetwork,
        description: "would call a vendor client".to_string(),
    });
    let mut executor = GeoProjectNodeExecutor::new().with_input_binding(binding(
        "geo.building.home_cells",
        GEO_ROWS_BINDING_ID,
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        &home_cell_rows(),
    ));
    let error = executor
        .execute(&unsafe_node, &context)
        .expect_err("unsafe effects refuse");
    assert!(error.message.contains("offline"));
}

#[test]
fn geo_executor_refuses_tile_request_not_backed_by_home_cell_dependency() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut request = tile_work_request();
    request.features.push(GeoTileFeatureRef {
        source: building_source(),
        feature_id: "building-outside".to_string(),
        home_cell: center_cell().to_string(),
    });
    let plan = five_node_plan();
    let mut executor = GeoProjectNodeExecutor::new()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &request,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.section".to_string()]
    );
    assert!(
        !temp.path().join("geo/building/section.json").exists(),
        "bad tile request must not publish section bytes"
    );
    assert!(node_report_reason(&report, "geo.building.section").contains("home-cell dependency"));
}

#[test]
fn geo_executor_rejects_evidence_universe_outside_bounded_section() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rows = warehouse_rows();
    rows.building_parcel_rows
        .push(GeoWarehouseBuildingParcelRow {
            building_id: "building-outside".to_string(),
            parcel_id: None,
        });
    let plan = five_node_plan();
    let mut executor = GeoProjectNodeExecutor::new()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        !temp
            .path()
            .join("geo/building/materialize_evidence.json")
            .exists(),
        "out-of-section evidence must not publish an evidence request"
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("bounded tile section feature_ids")
    );
}

#[test]
fn geo_executor_rejects_evidence_universe_that_omits_a_bounded_candidate() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rows = warehouse_rows();
    rows.building_parcel_rows
        .retain(|row| row.building_id != "building-b");
    let plan = five_node_plan();
    let mut executor = GeoProjectNodeExecutor::new()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("candidate universe must equal")
    );
}

#[test]
fn geo_executor_rejects_cross_level_same_text_ids_before_materializing_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let parcel_source = source_at_level(GeoControlEntityLevel::Parcel);
    let mut executor = executor_with_scope()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows_for(parcel_source.clone(), &["building-a", "building-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request_for(parcel_source, &["building-a", "building-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &warehouse_rows(),
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        !temp
            .path()
            .join("geo/building/materialize_evidence.json")
            .exists(),
        "cross-level same-text ids must not publish evidence request bytes"
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("native entity level")
    );
}

#[test]
fn geo_executor_rejects_parcel_profile_auxiliary_building_without_section_incidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rows = parcel_warehouse_rows();
    rows.building_parcel_rows
        .push(GeoWarehouseBuildingParcelRow {
            building_id: "building-unbound".to_string(),
            parcel_id: None,
        });
    let plan = five_node_plan();
    let parcel_source = source_at_level(GeoControlEntityLevel::Parcel);
    let mut executor = executor_with_scope()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows_for(parcel_source.clone(), &["parcel-a", "parcel-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request_for(parcel_source, &["parcel-a", "parcel-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("source-member incidence")
    );
}

#[test]
fn geo_executor_rejects_parcel_profile_auxiliary_incidence_outside_section() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rows = parcel_warehouse_rows();
    rows.building_parcel_rows
        .push(GeoWarehouseBuildingParcelRow {
            building_id: "building-outside".to_string(),
            parcel_id: Some("parcel-outside".to_string()),
        });
    let plan = five_node_plan();
    let parcel_source = source_at_level(GeoControlEntityLevel::Parcel);
    let mut executor = executor_with_scope()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows_for(parcel_source.clone(), &["parcel-a", "parcel-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request_for(parcel_source, &["parcel-a", "parcel-b"]),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("selected bounded section parcels")
    );
}

#[test]
fn geo_executor_rejects_building_profile_unbound_auxiliary_parcel_candidates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut rows = warehouse_rows();
    rows.parcel_rows.push(GeoWarehouseParcelRow {
        parcel_id: "parcel-unbound".to_string(),
    });
    let plan = five_node_plan();
    let mut executor = GeoProjectNodeExecutor::new()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &rows,
        ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.materialize_evidence".to_string()]
    );
    assert!(
        node_report_reason(&report, "geo.building.materialize_evidence")
            .contains("unbound auxiliary parcel candidates")
    );
}

#[test]
fn geo_executor_refuses_direct_compile_request_binding_before_stale_reuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let declared_request =
        materialize_warehouse_rows(&warehouse_rows()).expect("declared compile request is typed");
    let mut changed_rows = warehouse_rows();
    changed_rows.max_assignments -= 1;
    let changed_request =
        materialize_warehouse_rows(&changed_rows).expect("changed compile request is typed");
    assert_ne!(
        serde_json::to_vec(&declared_request).expect("declared request serializes"),
        serde_json::to_vec(&changed_request).expect("changed request serializes"),
        "the planted direct binding must differ from the dependency output"
    );
    let plan = five_node_plan();
    let mut executor = executor_with_fixture_inputs().with_input_binding(binding(
        "geo.building.compile_evidence",
        GEO_REQUEST_BINDING_ID,
        CANON_GEO_EVIDENCE_REQUEST_VERSION,
        &changed_request,
    ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(
        report.failed_nodes,
        vec!["geo.building.compile_evidence".to_string()]
    );
    assert!(
        !temp
            .path()
            .join("geo/building/compile_evidence.json")
            .exists(),
        "direct compile input must not publish compiled evidence"
    );
    assert!(
        !temp.path().join("geo/building/solve.json").exists(),
        "failed compile must not publish a solve artifact"
    );
    assert!(
        node_report_reason(&report, "geo.building.compile_evidence")
            .contains("direct input bindings are forbidden")
    );
}

#[test]
fn geo_executor_refuses_direct_solve_binding_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = five_node_plan();
    let mut executor = executor_with_fixture_inputs().with_input_binding(binding(
        "geo.building.solve",
        GEO_REQUEST_BINDING_ID,
        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
        &serde_json::json!({
            "version": CANON_GEO_EVIDENCE_COMPILATION_VERSION,
            "tampered": true
        }),
    ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(report.failed_nodes, vec!["geo.building.solve".to_string()]);
    assert!(
        temp.path()
            .join("geo/building/compile_evidence.json")
            .exists(),
        "the solve negative should reach a declared compiled-evidence dependency first"
    );
    assert!(
        !temp.path().join("geo/building/solve.json").exists(),
        "direct solve input must not publish a solve artifact"
    );
    assert!(
        node_report_reason(&report, "geo.building.solve")
            .contains("direct input bindings are forbidden")
    );
}

#[test]
fn geo_executor_refuses_stale_preloaded_section_without_current_direct_dependency() {
    let plan = five_node_plan();
    let mut solve_node = plan
        .nodes
        .iter()
        .find(|node| node.node_id == "geo.building.solve")
        .expect("solve node")
        .clone();
    solve_node.dependencies = vec!["geo.building.compile_evidence".to_string()];
    let compile_bytes = compile_evidence_bytes();
    let mut executor = executor_with_scope();
    executor
        .insert_dependency_output(executor_dependency_output(
            "geo.building.section",
            "section",
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            stale_same_id_section_bytes(),
        ))
        .expect("stale preloaded section is typed");

    let error = executor
        .execute(
            &solve_node,
            &ProjectNodeExecutionContext {
                node_id: solve_node.node_id.clone(),
                dependency_semantic_hashes: BTreeMap::from([(
                    "geo.building.compile_evidence".to_string(),
                    digest_bytes(&compile_bytes),
                )]),
                dependency_outputs: BTreeMap::from([(
                    "geo.building.compile_evidence".to_string(),
                    vec![project_dependency_output("compile_evidence", compile_bytes)],
                )]),
            },
        )
        .expect_err("stale scoped section not declared as a direct dependency refuses");

    assert!(error.message.contains("direct dependency"));
    assert!(error.message.contains("geo.building.section:section"));
}

#[test]
fn geo_executor_refuses_empty_current_section_output_vector_over_stale_preload() {
    let plan = five_node_plan();
    let solve_node = plan
        .nodes
        .iter()
        .find(|node| node.node_id == "geo.building.solve")
        .expect("solve node")
        .clone();
    let compile_bytes = compile_evidence_bytes();
    let propagation_bytes = propagation_bytes();
    let section_bytes = section_bytes(&tile_work_request());
    let mut executor = executor_with_scope();
    executor
        .insert_dependency_output(executor_dependency_output(
            "geo.building.section",
            "section",
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            stale_same_id_section_bytes(),
        ))
        .expect("stale preloaded section is typed");

    let error = executor
        .execute(
            &solve_node,
            &ProjectNodeExecutionContext {
                node_id: solve_node.node_id.clone(),
                dependency_semantic_hashes: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        digest_bytes(&compile_bytes),
                    ),
                    (
                        "geo.building.propagate".to_string(),
                        digest_bytes(&propagation_bytes),
                    ),
                    (
                        "geo.building.section".to_string(),
                        digest_bytes(&section_bytes),
                    ),
                ]),
                dependency_outputs: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        vec![project_dependency_output("compile_evidence", compile_bytes)],
                    ),
                    (
                        "geo.building.propagate".to_string(),
                        vec![project_dependency_output("propagation", propagation_bytes)],
                    ),
                    ("geo.building.section".to_string(), Vec::new()),
                ]),
            },
        )
        .expect_err("empty current section output vector must fail closed");

    assert!(
        error
            .message
            .contains("requires dependency output geo.building.section:section"),
        "{}",
        error.message
    );
}

#[test]
fn geo_executor_uses_fresh_direct_section_dependency_over_preloaded_stale_state() {
    let plan = five_node_plan();
    let solve_node = plan
        .nodes
        .iter()
        .find(|node| node.node_id == "geo.building.solve")
        .expect("solve node")
        .clone();
    let compile_bytes = compile_evidence_bytes();
    let propagation_bytes = propagation_bytes();
    let section_bytes = section_bytes(&tile_work_request());
    let mut executor = executor_with_scope();
    executor
        .insert_dependency_output(executor_dependency_output(
            "geo.building.section",
            "section",
            CANON_GEO_TILE_WORK_UNIT_VERSION,
            stale_same_id_section_bytes(),
        ))
        .expect("stale preloaded section is typed");

    let result = executor
        .execute(
            &solve_node,
            &ProjectNodeExecutionContext {
                node_id: solve_node.node_id.clone(),
                dependency_semantic_hashes: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        digest_bytes(&compile_bytes),
                    ),
                    (
                        "geo.building.propagate".to_string(),
                        digest_bytes(&propagation_bytes),
                    ),
                    (
                        "geo.building.section".to_string(),
                        digest_bytes(&section_bytes),
                    ),
                ]),
                dependency_outputs: BTreeMap::from([
                    (
                        "geo.building.compile_evidence".to_string(),
                        vec![project_dependency_output("compile_evidence", compile_bytes)],
                    ),
                    (
                        "geo.building.propagate".to_string(),
                        vec![project_dependency_output("propagation", propagation_bytes)],
                    ),
                    (
                        "geo.building.section".to_string(),
                        vec![project_dependency_output("section", section_bytes)],
                    ),
                ]),
            },
        )
        .expect("fresh direct section dependency overwrites stale preloaded state");

    assert_eq!(result.deterministic_usage["bounded_section_work_cells"], 7);
    let solve_bytes = result.outputs.get("solve").expect("solve output exists");
    let solve: Value = serde_json::from_slice(solve_bytes).expect("solve output parses");
    assert_eq!(solve["status"], "resolved");
}

#[test]
fn geo_executor_runs_collateral_ledger_stage_from_seed_and_declared_outputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = ledger_stage_plan();
    let mut executor = executor_with_fixture_inputs().with_input_binding(ledger_seed_binding(
        "geo.building.ledger",
        &ledger_stage_seed(),
    ));

    let report = run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("ledger run");

    assert!(report.failed_nodes.is_empty());
    assert!(
        report
            .executed_nodes
            .contains(&"geo.building.ledger".to_string()),
        "ledger stage should execute through project run"
    );

    let ledger_path = temp.path().join("geo/building/collateral_ledger.json");
    let ledger_bytes = fs::read(&ledger_path).expect("ledger output");
    let ledger: Value = serde_json::from_slice(&ledger_bytes).expect("ledger json");
    assert_eq!(ledger["version"], CANON_GEO_COLLATERAL_LEDGER_VERSION);
    assert_eq!(ledger["proof_class"], "fixture");
    assert_eq!(ledger["rows"].as_array().expect("rows").len(), 2);

    let rows = ledger["rows"].as_array().expect("rows array");
    let solved = rows
        .iter()
        .find(|row| row["loan_id"] == "loan-ledger-stage")
        .expect("solved row");
    assert_eq!(solved["reach"], "full");
    assert_eq!(
        solved["building_set"],
        serde_json::json!(["building-a", "building-b"])
    );
    assert_eq!(
        solved["property_refs"][0]["property_id"],
        "property:building-set"
    );
    assert_eq!(solved["last_observed_present"]["start_day"], 20_970);
    assert_eq!(solved["last_observed_present"]["end_day"], 20_970);
    assert!(
        solved["composition_blake3"]
            .as_str()
            .expect("composition digest")
            .starts_with("blake3:")
    );
    assert!(
        solved["evidence_blake3"]
            .as_str()
            .expect("evidence digest")
            .starts_with("blake3:")
    );

    let no_reach = rows
        .iter()
        .find(|row| row["loan_id"] == "loan-ledger-no-reach")
        .expect("no-reach row");
    assert_eq!(no_reach["reach"], "none");
    assert_eq!(no_reach["reach_none_reason"], "no_candidate_parcels");
    assert!(no_reach.get("parcel_set").is_none());
    assert!(no_reach.get("building_set").is_none());

    assert_eq!(ledger["rollups"][0]["rows"], 2);
    assert_eq!(
        ledger["rollups"][0]["truth_planes"]["gate_v2_historical"]["resolved"],
        1
    );
    assert_eq!(
        ledger["rollups"][0]["truth_planes"]["gate_v2_historical"]["reach_none"],
        1
    );

    let receipt =
        read_node_receipt(&receipt_path(temp.path(), "geo.building.ledger")).expect("receipt");
    assert_eq!(receipt.outcome, ProjectRunNodeOutcome::Completed);
    assert_eq!(receipt.deterministic_usage["ledger_rows"], 2);
    assert_eq!(receipt.deterministic_usage["ledger_reach_none_rows"], 1);
    assert_eq!(
        receipt
            .outputs
            .iter()
            .map(|output| output.output_id.as_str())
            .collect::<Vec<_>>(),
        vec![GEO_COLLATERAL_LEDGER_OUTPUT_ID]
    );
}

#[test]
fn geo_executor_refuses_ledger_stage_without_solve_dependency_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let plan = ledger_stage_missing_solve_plan();
    let mut executor = executor_with_fixture_inputs().with_input_binding(ledger_seed_binding(
        "geo.building.ledger",
        &ledger_stage_seed(),
    ));

    let report =
        run_project_plan(&plan, &policy(temp.path()), &mut executor).expect("failure report");

    assert_eq!(report.failed_nodes, vec!["geo.building.ledger".to_string()]);
    assert!(
        !temp
            .path()
            .join("geo/building/collateral_ledger.json")
            .exists(),
        "missing solve dependency must not publish a collateral ledger"
    );
    assert!(
        node_report_reason(&report, "geo.building.ledger")
            .contains("requires exactly one direct dependency output solve")
    );
}

fn binding<T: serde::Serialize>(
    node_id: &str,
    binding_id: &str,
    contract: &str,
    value: &T,
) -> GeoExecutorInputBinding {
    GeoExecutorInputBinding::from_json(node_id, binding_id, contract, value)
        .expect("binding serializes")
}

fn ledger_seed_binding(node_id: &str, seed: &GeoCollateralLedgerSeed) -> GeoExecutorInputBinding {
    GeoExecutorInputBinding::from_bytes(
        node_id,
        GEO_REQUEST_BINDING_ID,
        CANON_GEO_COLLATERAL_LEDGER_SEED_VERSION,
        canonical_collateral_ledger_seed_bytes(seed).expect("ledger seed canonicalizes"),
    )
}

fn ledger_stage_seed() -> GeoCollateralLedgerSeed {
    GeoCollateralLedgerSeed {
        version: CANON_GEO_COLLATERAL_LEDGER_SEED_VERSION.to_string(),
        proof_class: GeoCollateralLedgerProofClass::Fixture,
        rows: vec![
            GeoCollateralLedgerSeedRow {
                accession: "0000000000-26-000777".to_string(),
                deal_id: "fixture-executor-ledger-deal".to_string(),
                loan_id: "loan-ledger-stage".to_string(),
                reach: GeoCandidateReachStatus::Full,
                reach_none_reason: None,
                deed_ids: vec!["deed:fixture:executor:1".to_string()],
                truth_plane: Some(GeoTruthPlane::GateV2Historical),
                source_release_pins: vec![ledger_stage_pin()],
                composition_artifact_ref: Some("solve".to_string()),
                evidence_artifact_ref: Some("compile_evidence".to_string()),
                property_refs: vec![GeoLedgerPropertyRef {
                    property_id: "property:building-set".to_string(),
                    parcel_ids: Vec::new(),
                    building_ids: vec!["building-a".to_string(), "building-b".to_string()],
                }],
                last_observed_present: Some(GeoValidTimeInterval {
                    start_day: 20_970,
                    end_day: 20_970,
                }),
            },
            GeoCollateralLedgerSeedRow {
                accession: "0000000000-26-000777".to_string(),
                deal_id: "fixture-executor-ledger-deal".to_string(),
                loan_id: "loan-ledger-no-reach".to_string(),
                reach: GeoCandidateReachStatus::None,
                reach_none_reason: Some("no_candidate_parcels".to_string()),
                deed_ids: Vec::new(),
                truth_plane: Some(GeoTruthPlane::GateV2Historical),
                source_release_pins: vec![ledger_stage_pin()],
                composition_artifact_ref: None,
                evidence_artifact_ref: None,
                property_refs: Vec::new(),
                last_observed_present: None,
            },
        ],
    }
}

fn ledger_stage_pin() -> GeoSourceReleasePin {
    GeoSourceReleasePin {
        source_dataset: "fixture.buildings".to_string(),
        source_release: "2026-08-31".to_string(),
        blake3: format!(
            "blake3:{}",
            blake3::hash(b"fixture-executor-geo-ledger-stage-buildings").to_hex()
        ),
    }
}

fn executor_with_fixture_inputs() -> GeoProjectNodeExecutor {
    executor_with_scope()
        .with_input_binding(binding(
            "geo.building.home_cells",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_HOME_CELL_ROWS_VERSION,
            &home_cell_rows(),
        ))
        .with_input_binding(binding(
            "geo.building.section",
            GEO_REQUEST_BINDING_ID,
            CANON_GEO_TILE_WORK_REQUEST_VERSION,
            &tile_work_request(),
        ))
        .with_input_binding(binding(
            "geo.building.materialize_evidence",
            GEO_ROWS_BINDING_ID,
            CANON_GEO_WAREHOUSE_ROWS_VERSION,
            &warehouse_rows(),
        ))
}

fn executor_with_scope() -> GeoProjectNodeExecutor {
    GeoProjectNodeExecutor::new().with_exact_solve_scope("geo.building.solve", exact_solve_scope())
}

fn exact_solve_scope() -> GeoPlanExactSolveScope {
    GeoPlanExactSolveScope {
        bounded_section: GeoPlanProducedArtifactRef {
            producer_node_id: "geo.building.section".to_string(),
            output_id: "section".to_string(),
            output_contract: CANON_GEO_TILE_WORK_UNIT_VERSION.to_string(),
        },
        evidence_compilation: GeoPlanProducedArtifactRef {
            producer_node_id: "geo.building.compile_evidence".to_string(),
            output_id: "compile_evidence".to_string(),
            output_contract: CANON_GEO_EVIDENCE_COMPILATION_VERSION.to_string(),
        },
        component_scope:
            GeoPlanComponentScope::ActualConnectedComponentsOfCompiledConstraintIncidence,
        component_key_field: "canon_geo_composition.v0.factorization[].key".to_string(),
    }
}

fn preload_dependency_output(
    executor: &mut GeoProjectNodeExecutor,
    workspace: &Path,
    node_id: &str,
    output_id: &str,
    contract: &str,
) {
    let receipt = read_node_receipt(&receipt_path(workspace, node_id)).expect("dependency receipt");
    let output = receipt
        .outputs
        .iter()
        .find(|output| output.output_id == output_id)
        .expect("dependency output receipt");
    let bytes = fs::read(workspace.join(&output.path)).expect("dependency output bytes");
    assert_eq!(digest_bytes(&bytes), output.content_digest);
    assert_eq!(bytes.len() as u64, output.byte_count);
    executor
        .insert_dependency_output(GeoExecutorDependencyOutput {
            producer_node_id: node_id.to_string(),
            output_id: output_id.to_string(),
            contract: contract.to_string(),
            content_hash: output.content_digest.clone(),
            bytes,
        })
        .expect("dependency output validates before resume");
}

fn compile_evidence_bytes() -> Vec<u8> {
    let request = materialize_warehouse_rows(&warehouse_rows()).expect("warehouse rows compile");
    let artifact = compile_evidence(&request).expect("evidence compiles");
    canonical_evidence_compilation_bytes(&artifact).expect("compile artifact serializes")
}

fn propagation_bytes() -> Vec<u8> {
    let compile_bytes = compile_evidence_bytes();
    let compilation: GeoEvidenceCompilationArtifact =
        serde_json::from_slice(&compile_bytes).expect("compile artifact parses");
    let artifact = propagate(
        &compilation.composition_request,
        Some(&compilation),
        &GeoPropagationBudget::default(),
    )
    .expect("propagation succeeds");
    canonical_propagation_bytes(&artifact).expect("propagation artifact serializes")
}

fn section_bytes(request: &GeoTileWorkRequest) -> Vec<u8> {
    let artifact = materialize_tile_work_unit(request).expect("tile work materializes");
    canonical_tile_work_unit_bytes(&artifact).expect("section artifact serializes")
}

fn stale_same_id_section_bytes() -> Vec<u8> {
    let mut source = building_source();
    source.release.release_id = "stale-fixture-release-2026-08-31".to_string();
    source.release.release_digest = format!("blake3:{}", blake3::hash(b"stale-release").to_hex());
    source.inventory_ref.semantic_hash =
        format!("blake3:{}", blake3::hash(b"stale-semantic").to_hex());
    source.inventory_ref.planning_hash =
        format!("blake3:{}", blake3::hash(b"stale-planning").to_hex());
    let mut request = tile_work_request_for(source, &["building-a", "building-b"]);
    request.halo_k = 0;
    request.max_work_cells = 1;
    section_bytes(&request)
}

fn project_dependency_output(output_id: &str, bytes: Vec<u8>) -> ProjectDependencyOutput {
    ProjectDependencyOutput {
        output_id: output_id.to_string(),
        content_digest: digest_bytes(&bytes),
        byte_count: bytes.len() as u64,
        bytes,
    }
}

fn executor_dependency_output(
    producer_node_id: &str,
    output_id: &str,
    contract: &str,
    bytes: Vec<u8>,
) -> GeoExecutorDependencyOutput {
    GeoExecutorDependencyOutput {
        producer_node_id: producer_node_id.to_string(),
        output_id: output_id.to_string(),
        contract: contract.to_string(),
        content_hash: digest_bytes(&bytes),
        bytes,
    }
}

fn policy(workspace: &Path) -> ProjectRunPolicy {
    let mut policy = ProjectRunPolicy::new(workspace, "work");
    policy.failure_policy = ProjectRunFailurePolicy::FailFast;
    policy
}

fn five_node_plan() -> ProjectPlan {
    let nodes = vec![
        extension_node(
            "geo.building.home_cells",
            ProjectPlanNodeKind::Normalize,
            GEO_MATERIALIZE_HOME_CELLS_COMMAND,
            Vec::new(),
            "home_cells",
            "geo/building/home_cells.json",
            vec![ProjectPlanHashRef {
                ref_id: "geo.fixture.inputs".to_string(),
                content_hash: digest_bytes(b"geo executor fixture inputs"),
            }],
        ),
        extension_node(
            "geo.building.section",
            ProjectPlanNodeKind::Block,
            GEO_TILE_WORK_COMMAND,
            vec!["geo.building.home_cells".to_string()],
            "section",
            "geo/building/section.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.materialize_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_MATERIALIZE_EVIDENCE_COMMAND,
            vec!["geo.building.section".to_string()],
            "materialize_evidence",
            "geo/building/materialize_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.compile_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_COMPILE_EVIDENCE_COMMAND,
            vec!["geo.building.materialize_evidence".to_string()],
            "compile_evidence",
            "geo/building/compile_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.propagate",
            ProjectPlanNodeKind::Solve,
            GEO_PROPAGATE_STAGE_COMMAND,
            vec!["geo.building.compile_evidence".to_string()],
            "propagation",
            "geo/building/propagation.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.solve",
            ProjectPlanNodeKind::Solve,
            GEO_SOLVE_COMMAND,
            vec![
                "geo.building.compile_evidence".to_string(),
                "geo.building.propagate".to_string(),
                "geo.building.section".to_string(),
            ],
            "solve",
            "geo/building/solve.json",
            Vec::new(),
        ),
    ];
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "geo-executor-fixture",
        digest_bytes(b"geo executor manifest"),
        digest_bytes(b"geo executor lock"),
        nodes,
    ))
    .expect("extension project plan compiles")
}

fn ledger_stage_plan() -> ProjectPlan {
    let nodes = vec![
        extension_node(
            "geo.building.home_cells",
            ProjectPlanNodeKind::Normalize,
            GEO_MATERIALIZE_HOME_CELLS_COMMAND,
            Vec::new(),
            "home_cells",
            "geo/building/home_cells.json",
            vec![ProjectPlanHashRef {
                ref_id: "geo.fixture.inputs".to_string(),
                content_hash: digest_bytes(b"geo executor ledger fixture inputs"),
            }],
        ),
        extension_node(
            "geo.building.section",
            ProjectPlanNodeKind::Block,
            GEO_TILE_WORK_COMMAND,
            vec!["geo.building.home_cells".to_string()],
            "section",
            "geo/building/section.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.materialize_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_MATERIALIZE_EVIDENCE_COMMAND,
            vec!["geo.building.section".to_string()],
            "materialize_evidence",
            "geo/building/materialize_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.compile_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_COMPILE_EVIDENCE_COMMAND,
            vec!["geo.building.materialize_evidence".to_string()],
            "compile_evidence",
            "geo/building/compile_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.propagate",
            ProjectPlanNodeKind::Solve,
            GEO_PROPAGATE_STAGE_COMMAND,
            vec!["geo.building.compile_evidence".to_string()],
            GEO_PROPAGATE_OUTPUT_ID,
            "geo/building/propagation.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.solve",
            ProjectPlanNodeKind::Solve,
            GEO_SOLVE_COMMAND,
            vec![
                "geo.building.compile_evidence".to_string(),
                "geo.building.propagate".to_string(),
                "geo.building.section".to_string(),
            ],
            "solve",
            "geo/building/solve.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.ledger",
            ProjectPlanNodeKind::Evidence,
            GEO_LEDGER_STAGE_COMMAND,
            vec![
                "geo.building.compile_evidence".to_string(),
                "geo.building.propagate".to_string(),
                "geo.building.solve".to_string(),
            ],
            GEO_COLLATERAL_LEDGER_OUTPUT_ID,
            "geo/building/collateral_ledger.json",
            Vec::new(),
        ),
    ];
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "geo-executor-ledger-fixture",
        digest_bytes(b"geo executor ledger manifest"),
        digest_bytes(b"geo executor ledger lock"),
        nodes,
    ))
    .expect("ledger extension project plan compiles")
}

fn ledger_stage_missing_solve_plan() -> ProjectPlan {
    let nodes = vec![
        extension_node(
            "geo.building.home_cells",
            ProjectPlanNodeKind::Normalize,
            GEO_MATERIALIZE_HOME_CELLS_COMMAND,
            Vec::new(),
            "home_cells",
            "geo/building/home_cells.json",
            vec![ProjectPlanHashRef {
                ref_id: "geo.fixture.inputs".to_string(),
                content_hash: digest_bytes(b"geo executor ledger missing solve inputs"),
            }],
        ),
        extension_node(
            "geo.building.section",
            ProjectPlanNodeKind::Block,
            GEO_TILE_WORK_COMMAND,
            vec!["geo.building.home_cells".to_string()],
            "section",
            "geo/building/section.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.materialize_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_MATERIALIZE_EVIDENCE_COMMAND,
            vec!["geo.building.section".to_string()],
            "materialize_evidence",
            "geo/building/materialize_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.compile_evidence",
            ProjectPlanNodeKind::Evidence,
            GEO_COMPILE_EVIDENCE_COMMAND,
            vec!["geo.building.materialize_evidence".to_string()],
            "compile_evidence",
            "geo/building/compile_evidence.json",
            Vec::new(),
        ),
        extension_node(
            "geo.building.ledger",
            ProjectPlanNodeKind::Evidence,
            GEO_LEDGER_STAGE_COMMAND,
            vec!["geo.building.compile_evidence".to_string()],
            GEO_COLLATERAL_LEDGER_OUTPUT_ID,
            "geo/building/collateral_ledger.json",
            Vec::new(),
        ),
    ];
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "geo-executor-ledger-missing-solve",
        digest_bytes(b"geo executor ledger missing solve manifest"),
        digest_bytes(b"geo executor ledger missing solve lock"),
        nodes,
    ))
    .expect("ledger missing-solve extension project plan compiles")
}

fn single_home_cell_plan() -> ProjectPlan {
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "geo-executor-single",
        digest_bytes(b"single manifest"),
        digest_bytes(b"single lock"),
        vec![extension_node(
            "geo.building.home_cells",
            ProjectPlanNodeKind::Normalize,
            GEO_MATERIALIZE_HOME_CELLS_COMMAND,
            Vec::new(),
            "home_cells",
            "geo/building/home_cells.json",
            vec![ProjectPlanHashRef {
                ref_id: "geo.fixture.inputs".to_string(),
                content_hash: digest_bytes(b"single inputs"),
            }],
        )],
    ))
    .expect("single plan compiles")
}

fn single_unknown_command_plan() -> ProjectPlan {
    compile_extension_project_plan(ProjectExtensionDagRequest::offline_read_only(
        "geo-executor-unknown",
        digest_bytes(b"unknown manifest"),
        digest_bytes(b"unknown lock"),
        vec![extension_node(
            "geo.bad.home_cells",
            ProjectPlanNodeKind::Normalize,
            "canon geo unknown-leaf --request <REQUEST.json>",
            Vec::new(),
            "home_cells",
            "geo/bad/home_cells.json",
            Vec::new(),
        )],
    ))
    .expect("unknown-command plan still compiles as an extension node")
}

fn extension_node(
    node_id: &str,
    kind: ProjectPlanNodeKind,
    command: &str,
    dependencies: Vec<String>,
    output_id: &str,
    path: &str,
    content_hash_inputs: Vec<ProjectPlanHashRef>,
) -> ProjectExtensionDagNode {
    ProjectExtensionDagNode {
        node_id: node_id.to_string(),
        kind,
        class: ProjectPlanNodeClass::Computation,
        command: command.to_string(),
        dependencies,
        content_hash_inputs,
        outputs: vec![ProjectExtensionDagOutput {
            output_id: output_id.to_string(),
            path: path.to_string(),
            materialization: ProjectPlanOutputMaterialization::PlannedArtifact,
        }],
        limits: BTreeMap::from([
            ("budget.rows".to_string(), 100),
            ("budget.bytes".to_string(), 100_000),
            ("budget.cells".to_string(), 64),
            ("budget.candidates".to_string(), 128),
            ("budget.variables".to_string(), 128),
            ("budget.states".to_string(), 1_024),
            ("budget.models".to_string(), 64),
            ("budget.operations".to_string(), 10_000),
        ]),
        cache_eligible: true,
        side_effects: vec![
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::ReadsInput,
                description: "reads only declared local typed inputs".to_string(),
            },
            ProjectPlanSideEffect {
                kind: ProjectPlanSideEffectKind::WritesArtifact,
                description: "publishes one declared artifact".to_string(),
            },
        ],
        refusal_conditions: vec![ProjectPlanRefusalCondition {
            code: ProjectPlanErrorCode::ArtifactContract,
            message: "refuse on input or output contract mismatch".to_string(),
            next_command: None,
        }],
    }
}

fn center_cell() -> CellIndex {
    CellIndex::from_str("892a100d62bffff").expect("valid fixture cell")
}

fn building_source() -> GeoTileSourceBinding {
    source_at_level(GeoControlEntityLevel::Building)
}

fn source_at_level(entity_level: GeoControlEntityLevel) -> GeoTileSourceBinding {
    GeoTileSourceBinding {
        source_instance_id: format!("{entity_level:?}").to_ascii_lowercase(),
        release: GeoSourceRelease {
            release_id: "fixture-release-2026-08-31".to_string(),
            release_digest: format!(
                "blake3:{}",
                blake3::hash(b"fixture-release-2026-08-31").to_hex()
            ),
        },
        native_scope: GeoNativeEntityScope::NativeEntity {
            entity_level,
            identity_participation: GeoIdentityParticipation::StableAlias,
        },
        inventory_ref: GeoPlanInventoryRef {
            inventory_id: "inventory.fixture.executor".to_string(),
            semantic_hash: format!("blake3:{}", blake3::hash(b"executor-semantic").to_hex()),
            planning_hash: format!("blake3:{}", blake3::hash(b"executor-planning").to_hex()),
        },
    }
}

fn tile_features() -> Vec<GeoTileFeatureRef> {
    tile_features_for(building_source(), &["building-a", "building-b"])
}

fn tile_features_for(source: GeoTileSourceBinding, feature_ids: &[&str]) -> Vec<GeoTileFeatureRef> {
    let center = center_cell().to_string();
    feature_ids
        .iter()
        .map(|feature_id| GeoTileFeatureRef {
            source: source.clone(),
            feature_id: (*feature_id).to_string(),
            home_cell: center.clone(),
        })
        .collect()
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

fn tile_work_request_for(source: GeoTileSourceBinding, feature_ids: &[&str]) -> GeoTileWorkRequest {
    GeoTileWorkRequest {
        version: CANON_GEO_TILE_WORK_REQUEST_VERSION.to_string(),
        center_cell: center_cell().to_string(),
        halo_k: 1,
        features: tile_features_for(source, feature_ids),
        candidate_reach_reference: None,
        max_features: 16,
        max_work_cells: 7,
    }
}

fn home_cell_rows() -> GeoHomeCellRowsRequest {
    GeoHomeCellRowsRequest {
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

fn home_cell_rows_for(
    source: GeoTileSourceBinding,
    feature_ids: &[&str],
) -> GeoHomeCellRowsRequest {
    GeoHomeCellRowsRequest {
        version: CANON_GEO_HOME_CELL_ROWS_VERSION.to_string(),
        coordinate_crs: "EPSG:4326".to_string(),
        coordinate_decimal_places: 9,
        h3_resolution: 9,
        stability_radius_fixed: 1_000,
        rows: feature_ids
            .iter()
            .map(|feature_id| {
                let feature_id = *feature_id;
                home_cell_row_with_source(source.clone(), feature_id, &format!("rec-{feature_id}"))
            })
            .collect(),
        max_rows: 16,
    }
}

fn home_cell_row(feature_id: &str, source_record_id: &str) -> GeoHomeCellRow {
    home_cell_row_with_source(building_source(), feature_id, source_record_id)
}

fn home_cell_row_with_source(
    source: GeoTileSourceBinding,
    feature_id: &str,
    source_record_id: &str,
) -> GeoHomeCellRow {
    GeoHomeCellRow {
        source,
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

fn parcel_warehouse_rows() -> GeoWarehouseRowsRequest {
    let observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Parcel,
        sets: vec![vec!["parcel-a".to_string(), "parcel-b".to_string()]],
    };
    GeoWarehouseRowsRequest {
        version: CANON_GEO_WAREHOUSE_ROWS_VERSION.to_string(),
        profile: GeoCompositionProfile::parcel(),
        parcel_rows: vec![
            GeoWarehouseParcelRow {
                parcel_id: "parcel-b".to_string(),
            },
            GeoWarehouseParcelRow {
                parcel_id: "parcel-a".to_string(),
            },
        ],
        building_parcel_rows: vec![
            GeoWarehouseBuildingParcelRow {
                building_id: "building-b".to_string(),
                parcel_id: Some("parcel-b".to_string()),
            },
            GeoWarehouseBuildingParcelRow {
                building_id: "building-a".to_string(),
                parcel_id: Some("parcel-a".to_string()),
            },
        ],
        contracts: vec![parcel_rho_contract()],
        evidence_rows: vec![GeoWarehouseEvidenceRow {
            observation_id: "obs.parcel-set".to_string(),
            contract_id: "rho.parcel-set".to_string(),
            source_record: record("parcel-row-a"),
            valid_time: None,
            observation,
        }],
        max_assignments: 128,
        max_materialized_models: DEFAULT_MAX_MATERIALIZED_MODELS,
    }
}

fn warehouse_rows() -> GeoWarehouseRowsRequest {
    let observation = GeoRhoObservationKind::ExactSets {
        level: GeoEntityLevel::Building,
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

fn parcel_rho_contract() -> GeoRhoContract {
    GeoRhoContract {
        id: "rho.parcel-set".to_string(),
        version: "1.0.0".to_string(),
        source_dataset: "fixture.parcels".to_string(),
        source_release: "2026-08-31".to_string(),
        source_lineage_ids: vec!["fixture.parcels.release".to_string()],
        method_id: "fixture-parcel-candidate-set".to_string(),
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

fn node_report_reason<'a>(report: &'a canon::project::ProjectRunReport, node_id: &str) -> &'a str {
    report
        .node_reports
        .iter()
        .find(|node| node.node_id == node_id)
        .and_then(|node| node.reason.as_deref())
        .expect("node failure reason")
}
