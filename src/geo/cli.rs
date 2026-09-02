#![forbid(unsafe_code)]

//! Operator surface for the bounded Geo workbench.
//!
//! Each subcommand reads one typed JSON request, calls the library kernel, and
//! writes the canonical artifact bytes to stdout. Domain failures are typed
//! refusals, never panics: version mismatches, budget exhaustion, and invalid
//! inputs all surface through the library's own error codes.

use crate::{
    CanonOutput, Refusal, RefusalCode,
    cli::{
        GeoCapabilitiesCli, GeoCapabilitiesEmitMode, GeoCli, GeoCompileEvidenceCli, GeoEvaluateCli,
        GeoLinkSourcesCli, GeoMaterializeAddressEvidenceCli, GeoMaterializeEvidenceCli,
        GeoMaterializeGeometryCli, GeoMaterializeH7PipBlockBatchCli, GeoMaterializeH7PopulationCli,
        GeoMaterializeH7StagingBatchCli, GeoMaterializeHomeCellsCli,
        GeoMaterializeWarehouseGeometryCli, GeoPlanCli, GeoReconcileTilesCli,
        GeoReplanFromAcquisitionCli, GeoRunCli, GeoSolveCli, GeoStackEvidenceCli, GeoSubcommand,
        GeoTileWorkCli,
    },
    project::ProjectRunPolicy,
    refusal,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use super::{
    address::{
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION, GeoAddressError,
        GeoAddressParcelEvidenceRequest, build_address_parcel_evidence,
        canonical_address_parcel_evidence_bundle_bytes,
    },
    composition::{
        CANON_GEO_COMPOSITION_PROFILE_VERSION, GeoCompositionError, GeoCompositionProfile,
        GeoCompositionRequest, GeoEvidenceCompilationReference, canonical_composition_bytes,
        solve_composition,
    },
    control::{
        CANON_GEO_CAPABILITIES_VERSION, CANON_GEO_QUESTION_VERSION,
        CANON_GEO_REGIONAL_INVENTORY_VERSION, CANON_GEO_RESOURCE_BUDGET_VERSION, GeoCapabilities,
        GeoControlError, GeoQuestion, GeoRegionalInventory, GeoResourceBudget,
        canonical_capabilities_bytes, default_geo_capabilities,
    },
    discovery::{CANON_GEO_ACQUISITION_RECEIPT_VERSION, GeoAcquisitionReceipt, GeoDigestAlgorithm},
    evaluation::{
        CANON_GEO_POPULATION_REQUEST_VERSION, GeoPopulationCaseArtifacts, GeoPopulationError,
        GeoPopulationEvaluationRequest, canonical_population_evaluation_bytes,
        evaluate_population_with_artifacts,
    },
    evidence::{
        CANON_GEO_EVIDENCE_COMPILATION_VERSION, CANON_GEO_EVIDENCE_REQUEST_VERSION,
        GeoEvidenceCompilationArtifact, GeoEvidenceCompilationRequest, GeoEvidenceError,
        canonical_evidence_compilation_bytes, compile_evidence,
        validate_evidence_compilation_artifact,
    },
    geometry_value::{
        CANON_GEO_GEOMETRY_REQUEST_VERSION, CANON_GEO_GEOMETRY_TILE_VERSION,
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION, CANON_GEO_WAREHOUSE_GEOMETRY_VERSION,
        GeoGeometryError, GeoGeometryTileRequest, GeoWarehouseGeometryRowsRequest,
        canonical_geometry_tile_bytes, canonical_warehouse_geometry_bytes,
        materialize_geometry_tile, materialize_warehouse_geometry,
    },
    materialize::{
        CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION, CANON_GEO_H7_POPULATION_ROWS_VERSION,
        CANON_GEO_H7_POPULATION_VERSION, CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
        CANON_GEO_WAREHOUSE_ROWS_VERSION, GeoH7PipBlockPopulationBatchRequest,
        GeoH7PopulationRowsRequest, GeoH7StagingSourceRecordBytesBatchRequest,
        GeoMaterializationError, GeoWarehouseRowsRequest, canonical_h7_population_bytes,
        canonical_materialized_evidence_request_bytes, materialize_h7_pip_block_population_batch,
        materialize_h7_population_rows, materialize_h7_staging_source_record_bytes_batch,
        materialize_warehouse_rows,
    },
    multisource::{
        CANON_GEO_MULTISOURCE_REQUEST_VERSION, GeoMultisourceRequest,
        canonical_multisource_artifact_bytes, materialize_geo_multisource,
    },
    plan::{
        CANON_GEO_PLAN_VERSION, GeoPlan, GeoPlanError, GeoPlanReplanRequest, GeoPlanRequest,
        canonical_geo_plan_bytes, compile_geo_plan, replan_geo_plan_from_inventory_advancement,
    },
    run::{
        CANON_GEO_RUN_VERSION, GeoRunError, GeoRunInputBinding, GeoRunRequest,
        canonical_geo_run_bytes, run_geo_plan,
    },
    satisfy::{
        GeoSatisfactionAssignment, GeoSatisfactionFileBinding, GeoSatisfactionInput,
        GeoSatisfactionRunInput, GeoSatisfactionRunInputFileBinding, GeoSatisfactionStatus,
        GeoSatisfyError, canonical_geo_regional_inventory_advancement_bytes,
        parse_geo_satisfaction_assignment, satisfy_geo_acquisition,
        satisfy_geo_acquisition_for_run,
    },
    stack::{
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
        CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION, GeoEvidenceStackError,
        GeoPopulationEvidenceStackArtifact, GeoPopulationEvidenceStackRequest,
        canonical_population_evidence_stack_bytes, stack_population_evidence,
        validate_population_evidence_stack_artifact,
    },
    tile::{
        CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, CANON_GEO_HOME_CELL_ROWS_VERSION,
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION, CANON_GEO_TILE_RECONCILIATION_VERSION,
        CANON_GEO_TILE_WORK_REQUEST_VERSION, CANON_GEO_TILE_WORK_UNIT_VERSION,
        GeoHomeCellRowsRequest, GeoTileError, GeoTileReconciliationRequest, GeoTileWorkRequest,
        canonical_home_cell_assignment_bytes, canonical_tile_reconciliation_bytes,
        canonical_tile_work_unit_bytes, materialize_home_cells, materialize_tile_work_unit,
        reconcile_tile_decisions,
    },
};

const GEO_PLAN_NEXT_COMMAND: &str = "canon geo plan --question <QUESTION.json> --capabilities <CAPABILITIES.json> --inventory <INVENTORY.json> --profile <PROFILE.json> --budget <BUDGET.json>";
const GEO_RUN_NEXT_COMMAND: &str =
    "canon geo run --plan <PLAN.json> --work-dir <DIR> --input <NODE_ID:BINDING_ID=PATH>";
const GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND: &str = "canon geo replan-from-acquisition --base-plan <PLAN.json> --base-inventory <INVENTORY.json> --question <QUESTION.json> --capabilities <CAPABILITIES.json> --profile <PROFILE.json> --budget <BUDGET.json> --satisfy <REQUEST_ID=RECEIPT.json> --local-artifact <LOCAL_ARTIFACT_ID=PATH> --advancement-out <ADVANCEMENT.json>";

pub fn run(geo: &GeoCli) -> Result<u8, Box<dyn Error>> {
    match &geo.command {
        GeoSubcommand::Capabilities(args) => run_capabilities(args),
        GeoSubcommand::Plan(args) => run_plan(args),
        GeoSubcommand::Run(args) => run_geo_run(args),
        GeoSubcommand::ReplanFromAcquisition(args) => run_replan_from_acquisition(args),
        GeoSubcommand::Inspect => run_unavailable_primary("geo inspect"),
        GeoSubcommand::Ledger => run_unavailable_primary("geo ledger"),
        GeoSubcommand::LinkSources(args) => run_link_sources(args),
        GeoSubcommand::MaterializeHomeCells(args) => run_materialize_home_cells(args),
        GeoSubcommand::TileWork(args) => run_tile_work(args),
        GeoSubcommand::ReconcileTiles(args) => run_reconcile_tiles(args),
        GeoSubcommand::Solve(args) => run_solve(args),
        GeoSubcommand::MaterializeGeometry(args) => run_materialize_geometry(args),
        GeoSubcommand::MaterializeWarehouseGeometry(args) => {
            run_materialize_warehouse_geometry(args)
        }
        GeoSubcommand::MaterializeEvidence(args) => run_materialize_evidence(args),
        GeoSubcommand::MaterializeAddressEvidence(args) => run_materialize_address_evidence(args),
        GeoSubcommand::MaterializeH7Population(args) => run_materialize_h7_population(args),
        GeoSubcommand::MaterializeH7StagingBatch(args) => run_materialize_h7_staging_batch(args),
        GeoSubcommand::MaterializeH7PipBlockBatch(args) => run_materialize_h7_pip_block_batch(args),
        GeoSubcommand::CompileEvidence(args) => run_compile_evidence(args),
        GeoSubcommand::StackEvidence(args) => run_stack_evidence(args),
        GeoSubcommand::Evaluate(args) => run_evaluate(args),
    }
}

fn run_unavailable_primary(command: &str) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo primary command is planned but not implemented in this build",
        json!({
            "command": format!("canon {command}"),
            "status": "planned_not_implemented",
            "implemented_primary_commands": [
                "canon geo capabilities --emit json",
                GEO_PLAN_NEXT_COMMAND,
                "canon geo run --plan <PLAN.json> --work-dir <DIR> --input <NODE_ID:BINDING_ID=PATH>",
                GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
                "canon geo evaluate --population <POPULATION.json>"
            ]
        }),
        Some("canon geo capabilities --emit json".to_string()),
    )
}

fn run_capabilities(args: &GeoCapabilitiesCli) -> Result<u8, Box<dyn Error>> {
    match args.emit {
        GeoCapabilitiesEmitMode::Json => {
            let capabilities = match default_geo_capabilities() {
                Ok(capabilities) => capabilities,
                Err(error) => return emit_control_error(error),
            };
            match canonical_capabilities_bytes(&capabilities) {
                Ok(bytes) => write_canonical(&bytes),
                Err(error) => emit_control_error(error),
            }
        }
    }
}

fn run_plan(args: &GeoPlanCli) -> Result<u8, Box<dyn Error>> {
    let question: GeoQuestion = match read_request(
        &args.question,
        "question",
        CANON_GEO_QUESTION_VERSION,
        GEO_PLAN_NEXT_COMMAND,
    ) {
        Ok(question) => question,
        Err(exit_code) => return Ok(exit_code),
    };
    let capabilities: GeoCapabilities = match read_request(
        &args.capabilities,
        "capabilities",
        CANON_GEO_CAPABILITIES_VERSION,
        GEO_PLAN_NEXT_COMMAND,
    ) {
        Ok(capabilities) => capabilities,
        Err(exit_code) => return Ok(exit_code),
    };
    let inventory: GeoRegionalInventory = match read_request(
        &args.inventory,
        "inventory",
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        GEO_PLAN_NEXT_COMMAND,
    ) {
        Ok(inventory) => inventory,
        Err(exit_code) => return Ok(exit_code),
    };
    let profile: GeoCompositionProfile = match read_request(
        &args.profile,
        "profile",
        CANON_GEO_COMPOSITION_PROFILE_VERSION,
        GEO_PLAN_NEXT_COMMAND,
    ) {
        Ok(profile) => profile,
        Err(exit_code) => return Ok(exit_code),
    };
    let budget: GeoResourceBudget = match read_request(
        &args.budget,
        "budget",
        CANON_GEO_RESOURCE_BUDGET_VERSION,
        GEO_PLAN_NEXT_COMMAND,
    ) {
        Ok(budget) => budget,
        Err(exit_code) => return Ok(exit_code),
    };

    let plan = match compile_geo_plan(GeoPlanRequest {
        question,
        capabilities,
        inventory,
        profile,
        budget,
    }) {
        Ok(plan) => plan,
        Err(error) => return emit_plan_error(error),
    };
    match canonical_geo_plan_bytes(&plan) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_plan_error(error),
    }
}

fn run_geo_run(args: &GeoRunCli) -> Result<u8, Box<dyn Error>> {
    let plan: GeoPlan = match read_request(
        &args.plan,
        "plan",
        CANON_GEO_PLAN_VERSION,
        GEO_RUN_NEXT_COMMAND,
    ) {
        Ok(plan) => plan,
        Err(exit_code) => return Ok(exit_code),
    };
    let input_files = match read_geo_run_inputs(&args.input) {
        Ok(inputs) => inputs,
        Err(exit_code) => return Ok(exit_code),
    };
    let satisfaction_bindings = match validate_geo_satisfactions(&plan, &args.satisfy, &input_files)
    {
        Ok(bindings) => bindings,
        Err(exit_code) => return Ok(exit_code),
    };

    let input_bindings = input_files
        .into_iter()
        .map(|input| {
            let binding = GeoRunInputBinding::from_bytes(
                input.node_id,
                input.binding_id,
                input.contract_version,
                input.bytes,
            );
            (
                (binding.node_id.clone(), binding.binding_id.clone()),
                binding,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for binding in satisfaction_bindings {
        let key = (binding.node_id.clone(), binding.binding_id.clone());
        match input_bindings.get(&key) {
            Some(explicit) if explicit == &binding => {}
            Some(_) => {
                return emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo satisfaction binding disagrees with its explicit --input bytes",
                    json!({
                        "node_id": key.0,
                        "binding_id": key.1,
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                );
            }
            None => {
                return emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo satisfaction binding has no matching explicit --input",
                    json!({
                        "node_id": key.0,
                        "binding_id": key.1,
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                );
            }
        }
    }
    let request = GeoRunRequest::new(
        plan,
        ProjectRunPolicy::new(&args.work_dir, ".canon/geo-run"),
        input_bindings.into_values().collect(),
    );
    let run = match run_geo_plan(request) {
        Ok(run) => run,
        Err(error) => return emit_run_error(error),
    };
    match canonical_geo_run_bytes(&run) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_run_error(error),
    }
}

fn run_replan_from_acquisition(args: &GeoReplanFromAcquisitionCli) -> Result<u8, Box<dyn Error>> {
    let base_plan: GeoPlan = match read_request(
        &args.base_plan,
        "base_plan",
        CANON_GEO_PLAN_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(plan) => plan,
        Err(exit_code) => return Ok(exit_code),
    };
    let base_inventory: GeoRegionalInventory = match read_request(
        &args.base_inventory,
        "base_inventory",
        CANON_GEO_REGIONAL_INVENTORY_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(inventory) => inventory,
        Err(exit_code) => return Ok(exit_code),
    };
    let question: GeoQuestion = match read_request(
        &args.question,
        "question",
        CANON_GEO_QUESTION_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(question) => question,
        Err(exit_code) => return Ok(exit_code),
    };
    let capabilities: GeoCapabilities = match read_request(
        &args.capabilities,
        "capabilities",
        CANON_GEO_CAPABILITIES_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(capabilities) => capabilities,
        Err(exit_code) => return Ok(exit_code),
    };
    let profile: GeoCompositionProfile = match read_request(
        &args.profile,
        "profile",
        CANON_GEO_COMPOSITION_PROFILE_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(profile) => profile,
        Err(exit_code) => return Ok(exit_code),
    };
    let budget: GeoResourceBudget = match read_request(
        &args.budget,
        "budget",
        CANON_GEO_RESOURCE_BUDGET_VERSION,
        GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND,
    ) {
        Ok(budget) => budget,
        Err(exit_code) => return Ok(exit_code),
    };
    let assignment = match parse_geo_satisfaction_assignment(&args.satisfy) {
        Ok(assignment) => assignment,
        Err(error) => return emit_replan_satisfy_error(error),
    };
    let local_artifact_files = match read_geo_satisfaction_file_bindings(
        &args.local_artifact,
        "local-artifact",
        "LOCAL_ARTIFACT_ID=PATH",
    ) {
        Ok(bindings) => bindings,
        Err(exit_code) => return Ok(exit_code),
    };
    let result_digest_files =
        match read_geo_satisfaction_file_bindings(&args.result, "result", "DIGEST_ID=PATH") {
            Ok(bindings) => bindings,
            Err(exit_code) => return Ok(exit_code),
        };

    let satisfaction = match satisfy_geo_acquisition(GeoSatisfactionInput {
        plan: &base_plan,
        inventory: Some(&base_inventory),
        assignment,
        local_artifact_files,
        result_digest_files,
    }) {
        Ok(satisfaction) => satisfaction,
        Err(error) => return emit_replan_satisfy_error(error),
    };
    let Some(advancement) = satisfaction.inventory_advancement.clone() else {
        return emit_refusal(
            RefusalCode::EEntityArtifactContract,
            "Geo replan acquisition receipt did not produce a live complete inventory advancement",
            json!({
                "request_id": satisfaction.request_id,
                "status": code_name(&satisfaction.status),
                "proof_class": code_name(&satisfaction.receipt_execution.proof_class),
                "receipt_terminal_state": code_name(&satisfaction.receipt_execution.terminal_state),
                "findings": satisfaction.findings,
            }),
            Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
        );
    };
    let advancement_bytes = match canonical_geo_regional_inventory_advancement_bytes(&advancement) {
        Ok(bytes) => bytes,
        Err(error) => return emit_replan_satisfy_error(error),
    };
    let replanned = match replan_geo_plan_from_inventory_advancement(GeoPlanReplanRequest {
        base_plan,
        base_inventory,
        question,
        capabilities,
        profile,
        budget,
        inventory_advancement: advancement,
    }) {
        Ok(plan) => plan,
        Err(error) => return emit_replan_plan_error(error),
    };
    let plan_bytes = match canonical_geo_plan_bytes(&replanned) {
        Ok(bytes) => bytes,
        Err(error) => return emit_replan_plan_error(error),
    };
    if let Err(error) =
        publish_replan_advancement_sidecar(&args.advancement_out, &advancement_bytes)
    {
        return emit_refusal(
            RefusalCode::EIo,
            "Geo replan could not publish the inventory advancement sidecar",
            json!({
                "advancement_out": error.target,
                "temp_path": error.temp_path,
                "error": error.message,
            }),
            Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
        );
    }
    write_canonical(&plan_bytes)
}

fn run_materialize_home_cells(args: &GeoMaterializeHomeCellsCli) -> Result<u8, Box<dyn Error>> {
    let rows: GeoHomeCellRowsRequest = match read_request(
        &args.rows,
        "rows",
        CANON_GEO_HOME_CELL_ROWS_VERSION,
        "canon geo materialize-home-cells --rows <ROWS.json>",
    ) {
        Ok(rows) => rows,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_home_cells(&rows) {
        Ok(artifact) => artifact,
        Err(error) => return emit_home_cell_error(error),
    };
    match canonical_home_cell_assignment_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_HOME_CELL_ASSIGNMENT_VERSION, &error),
    }
}

fn run_materialize_warehouse_geometry(
    args: &GeoMaterializeWarehouseGeometryCli,
) -> Result<u8, Box<dyn Error>> {
    let rows: GeoWarehouseGeometryRowsRequest = match read_request(
        &args.rows,
        "rows",
        CANON_GEO_WAREHOUSE_GEOMETRY_ROWS_VERSION,
        "canon geo materialize-warehouse-geometry --rows <ROWS.json>",
    ) {
        Ok(rows) => rows,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_warehouse_geometry(&rows) {
        Ok(artifact) => artifact,
        Err(error) => return emit_geometry_error(error),
    };
    match canonical_warehouse_geometry_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_WAREHOUSE_GEOMETRY_VERSION, &error),
    }
}

fn run_tile_work(args: &GeoTileWorkCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoTileWorkRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_TILE_WORK_REQUEST_VERSION,
        "canon geo tile-work --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_tile_work_unit(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_tile_error(error),
    };
    match canonical_tile_work_unit_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_TILE_WORK_UNIT_VERSION, &error),
    }
}

fn run_reconcile_tiles(args: &GeoReconcileTilesCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoTileReconciliationRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_TILE_RECONCILIATION_REQUEST_VERSION,
        "canon geo reconcile-tiles --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match reconcile_tile_decisions(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_tile_error(error),
    };
    match canonical_tile_reconciliation_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_TILE_RECONCILIATION_VERSION, &error),
    }
}

fn run_link_sources(args: &GeoLinkSourcesCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoMultisourceRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_MULTISOURCE_REQUEST_VERSION,
        "canon geo link-sources --request <REQUEST.json> --rows-out <ROWS.csv>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_geo_multisource(&request, &args.rows_out) {
        Ok(artifact) => artifact,
        Err(refusal) => return emit_library_refusal(refusal),
    };
    match canonical_multisource_artifact_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(refusal) => emit_library_refusal(refusal),
    }
}

fn run_materialize_geometry(args: &GeoMaterializeGeometryCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoGeometryTileRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_GEOMETRY_REQUEST_VERSION,
        "canon geo materialize-geometry --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_geometry_tile(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_geometry_error(error),
    };
    match canonical_geometry_tile_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_GEOMETRY_TILE_VERSION, &error),
    }
}

fn run_materialize_evidence(args: &GeoMaterializeEvidenceCli) -> Result<u8, Box<dyn Error>> {
    let rows: GeoWarehouseRowsRequest = match read_request(
        &args.rows,
        "rows",
        CANON_GEO_WAREHOUSE_ROWS_VERSION,
        "canon geo materialize-evidence --rows <ROWS.json>",
    ) {
        Ok(rows) => rows,
        Err(exit_code) => return Ok(exit_code),
    };
    let request = match materialize_warehouse_rows(&rows) {
        Ok(request) => request,
        Err(error) => return emit_materialization_error(error),
    };
    match canonical_materialized_evidence_request_bytes(&request) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_EVIDENCE_REQUEST_VERSION, &error),
    }
}

fn run_materialize_address_evidence(
    args: &GeoMaterializeAddressEvidenceCli,
) -> Result<u8, Box<dyn Error>> {
    let request: GeoAddressParcelEvidenceRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_ADDRESS_PARCEL_EVIDENCE_REQUEST_VERSION,
        "canon geo materialize-address-evidence --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let bundle = match build_address_parcel_evidence(&request) {
        Ok(bundle) => bundle,
        Err(error) => return emit_address_error(error),
    };
    match canonical_address_parcel_evidence_bundle_bytes(&bundle) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_address_error(error),
    }
}

fn run_materialize_h7_population(
    args: &GeoMaterializeH7PopulationCli,
) -> Result<u8, Box<dyn Error>> {
    let rows: GeoH7PopulationRowsRequest = match read_request(
        &args.rows,
        "rows",
        CANON_GEO_H7_POPULATION_ROWS_VERSION,
        "canon geo materialize-h7-population --rows <ROWS.json>",
    ) {
        Ok(rows) => rows,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_h7_population_rows(&rows) {
        Ok(artifact) => artifact,
        Err(error) => return emit_h7_population_materialization_error(error),
    };
    match canonical_h7_population_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_H7_POPULATION_VERSION, &error),
    }
}

fn run_materialize_h7_staging_batch(
    args: &GeoMaterializeH7StagingBatchCli,
) -> Result<u8, Box<dyn Error>> {
    let batch: GeoH7StagingSourceRecordBytesBatchRequest = match read_request(
        &args.batch,
        "batch",
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION,
        "canon geo materialize-h7-staging-batch --batch <BATCH.json>",
    ) {
        Ok(batch) => batch,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_h7_staging_source_record_bytes_batch(&batch) {
        Ok(artifact) => artifact,
        Err(error) => return emit_h7_population_materialization_error(error),
    };
    match canonical_h7_population_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_H7_POPULATION_VERSION, &error),
    }
}

fn run_materialize_h7_pip_block_batch(
    args: &GeoMaterializeH7PipBlockBatchCli,
) -> Result<u8, Box<dyn Error>> {
    let batch: GeoH7PipBlockPopulationBatchRequest = match read_request(
        &args.batch,
        "batch",
        CANON_GEO_H7_PIP_BLOCK_POPULATION_BATCH_VERSION,
        "canon geo materialize-h7-pip-block-batch --batch <BATCH.json>",
    ) {
        Ok(batch) => batch,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match materialize_h7_pip_block_population_batch(&batch) {
        Ok(artifact) => artifact,
        Err(error) => return emit_h7_population_materialization_error(error),
    };
    match canonical_h7_population_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal(CANON_GEO_H7_POPULATION_VERSION, &error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GeoSolveInput {
    CompositionRequest(GeoCompositionRequest),
    EvidenceCompilation(GeoEvidenceCompilationArtifact),
}

fn run_solve(args: &GeoSolveCli) -> Result<u8, Box<dyn Error>> {
    let input: GeoSolveInput = match read_request(
        &args.request,
        "request",
        "canon_geo_composition_request.v0 or canon_geo_evidence_compilation.v0",
        "canon geo solve --request <REQUEST.json>",
    ) {
        Ok(input) => input,
        Err(exit_code) => return Ok(exit_code),
    };
    let (request, evidence_compilation) = match input {
        GeoSolveInput::CompositionRequest(request) => (request, None),
        GeoSolveInput::EvidenceCompilation(compilation) => {
            if let Err(error) = validate_evidence_compilation_artifact(&compilation) {
                return emit_evidence_error(error);
            }
            let bytes = match canonical_evidence_compilation_bytes(&compilation) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return emit_serialization_refusal(
                        CANON_GEO_EVIDENCE_COMPILATION_VERSION,
                        &error,
                    );
                }
            };
            let reference = GeoEvidenceCompilationReference {
                version: compilation.version.clone(),
                request_version: compilation.request_version.clone(),
                blake3: blake3::hash(&bytes).to_hex().to_string(),
            };
            (compilation.composition_request, Some(reference))
        }
    };
    let mut artifact = match solve_composition(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_composition_error(error),
    };
    artifact.evidence_compilation = evidence_compilation;
    match canonical_composition_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal("canon_geo_composition.v0", &error),
    }
}

fn run_compile_evidence(args: &GeoCompileEvidenceCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoEvidenceCompilationRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_EVIDENCE_REQUEST_VERSION,
        "canon geo compile-evidence --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match compile_evidence(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_evidence_error(error),
    };
    match canonical_evidence_compilation_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal("canon_geo_evidence_compilation.v0", &error),
    }
}

fn run_evaluate(args: &GeoEvaluateCli) -> Result<u8, Box<dyn Error>> {
    let request = match read_population_or_stack(
        &args.population,
        "canon geo evaluate --population <POPULATION.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let evaluated = match evaluate_population_with_artifacts(&request) {
        Ok(evaluated) => evaluated,
        Err(error) => return emit_population_error(error),
    };
    if let Some(artifact_dir) = &args.artifact_dir
        && let Err(exit_code) = write_evaluate_artifact_dir(artifact_dir, &evaluated.case_artifacts)
    {
        return Ok(exit_code);
    }
    match canonical_population_evaluation_bytes(&evaluated.evaluation) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal("canon_geo_population_evaluation.v0", &error),
    }
}

#[derive(Debug, Serialize)]
struct GeoEvaluateArtifactIndex {
    cases: Vec<GeoEvaluateArtifactIndexEntry>,
}

#[derive(Debug, Serialize)]
struct GeoEvaluateArtifactIndexEntry {
    case_id: String,
    truth_plane: super::evaluation::GeoTruthPlane,
    solve_file: String,
    evidence_file: String,
    solver_digest: String,
    compilation_digest: String,
}

fn write_evaluate_artifact_dir(
    artifact_dir: &Path,
    cases: &[GeoPopulationCaseArtifacts],
) -> Result<(), u8> {
    if let Err(error) = fs::create_dir_all(artifact_dir) {
        return Err(emit_refusal(
            RefusalCode::EIo,
            "Could not create the Geo evaluate artifact directory",
            json!({
                "artifact_dir": path_string(artifact_dir),
                "error": error.to_string(),
            }),
            Some("choose a writable --artifact-dir and rerun canon geo evaluate".to_string()),
        )
        .unwrap_or(2));
    }

    let mut index = GeoEvaluateArtifactIndex { cases: Vec::new() };
    for case in cases {
        let stem = match safe_case_artifact_stem(&case.case_id) {
            Ok(stem) => stem,
            Err(error) => return Err(emit_population_error(error).unwrap_or(2)),
        };
        let evidence_file = format!("{stem}.evidence.json");
        let solve_file = format!("{stem}.solve.json");
        let evidence_bytes = match canonical_evidence_compilation_bytes(&case.evidence) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(emit_serialization_refusal(
                    CANON_GEO_EVIDENCE_COMPILATION_VERSION,
                    &error,
                )
                .unwrap_or(2));
            }
        };
        let evidence_digest = blake3::hash(&evidence_bytes).to_hex().to_string();
        if evidence_digest != case.compilation_digest {
            return Err(emit_population_error(GeoPopulationError::invalid_input(
                "Geo evaluate artifact-dir evidence bytes do not match the case digest",
                [
                    ("case_id", case.case_id.clone()),
                    ("expected_blake3", case.compilation_digest.clone()),
                    ("actual_blake3", evidence_digest),
                ],
            ))
            .unwrap_or(2));
        }
        write_artifact_file(
            artifact_dir,
            &evidence_file,
            &evidence_bytes,
            &case.case_id,
            "evidence",
        )?;

        let solve = match &case.solve {
            Some(solve) => solve,
            None => {
                return Err(emit_population_error(GeoPopulationError::invalid_input(
                    "Geo evaluate artifact-dir requires a solve artifact for every case",
                    [
                        ("case_id", case.case_id.clone()),
                        ("artifact_kind", "solve".to_string()),
                    ],
                ))
                .unwrap_or(2));
            }
        };
        let solve_bytes = match canonical_composition_bytes(solve) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(
                    emit_serialization_refusal("canon_geo_composition.v0", &error).unwrap_or(2),
                );
            }
        };
        let solve_digest = blake3::hash(&solve_bytes).to_hex().to_string();
        let expected_solver_digest = match &case.solver_digest {
            Some(digest) => digest,
            None => {
                return Err(emit_population_error(GeoPopulationError::invalid_input(
                    "Geo evaluate artifact-dir case is missing a solver digest",
                    [
                        ("case_id", case.case_id.clone()),
                        ("artifact_kind", "solve".to_string()),
                    ],
                ))
                .unwrap_or(2));
            }
        };
        if solve_digest != *expected_solver_digest {
            return Err(emit_population_error(GeoPopulationError::invalid_input(
                "Geo evaluate artifact-dir solve bytes do not match the case digest",
                [
                    ("case_id", case.case_id.clone()),
                    ("expected_blake3", expected_solver_digest.clone()),
                    ("actual_blake3", solve_digest.clone()),
                ],
            ))
            .unwrap_or(2));
        }
        write_artifact_file(
            artifact_dir,
            &solve_file,
            &solve_bytes,
            &case.case_id,
            "solve",
        )?;

        index.cases.push(GeoEvaluateArtifactIndexEntry {
            case_id: case.case_id.clone(),
            truth_plane: case.truth_plane,
            solve_file,
            evidence_file,
            solver_digest: solve_digest,
            compilation_digest: case.compilation_digest.clone(),
        });
    }

    let index_bytes = match serde_json::to_vec(&index) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EEntityArtifactContract,
                "Geo evaluate artifact index could not be serialized",
                json!({
                    "artifact_dir": path_string(artifact_dir),
                    "error": error.to_string(),
                }),
                Some("rerun canon geo evaluate with a smaller population".to_string()),
            )
            .unwrap_or(2));
        }
    };
    write_artifact_file(artifact_dir, "index.json", &index_bytes, "index", "index")
}

fn safe_case_artifact_stem(case_id: &str) -> Result<&str, GeoPopulationError> {
    if case_id.is_empty()
        || case_id == "."
        || case_id == ".."
        || case_id.contains('/')
        || case_id.contains('\\')
    {
        return Err(GeoPopulationError::invalid_input(
            "Geo evaluate artifact-dir case identifiers must be safe file stems",
            [("case_id", case_id.to_string())],
        ));
    }
    Ok(case_id)
}

fn write_artifact_file(
    artifact_dir: &Path,
    relative_file: &str,
    bytes: &[u8],
    case_id: &str,
    artifact_kind: &str,
) -> Result<(), u8> {
    let path = artifact_dir.join(relative_file);
    match fs::read(&path) {
        Ok(existing) if existing == bytes => return Ok(()),
        Ok(existing) => {
            return Err(emit_population_error(GeoPopulationError::invalid_input(
                "Geo evaluate artifact-dir target already exists with different bytes",
                [
                    ("case_id", case_id.to_string()),
                    ("artifact_kind", artifact_kind.to_string()),
                    ("path", path_string(&path)),
                    ("expected_blake3", blake3::hash(bytes).to_hex().to_string()),
                    (
                        "actual_blake3",
                        blake3::hash(&existing).to_hex().to_string(),
                    ),
                ],
            ))
            .unwrap_or(2));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EIo,
                "Could not read an existing Geo evaluate artifact-dir target",
                json!({
                    "case_id": case_id,
                    "artifact_kind": artifact_kind,
                    "path": path_string(&path),
                    "error": error.to_string(),
                }),
                Some("choose a readable --artifact-dir and rerun canon geo evaluate".to_string()),
            )
            .unwrap_or(2));
        }
    }
    if let Err(error) = fs::write(&path, bytes) {
        return Err(emit_refusal(
            RefusalCode::EIo,
            "Could not write a Geo evaluate artifact-dir target",
            json!({
                "case_id": case_id,
                "artifact_kind": artifact_kind,
                "path": path_string(&path),
                "error": error.to_string(),
            }),
            Some("choose a writable --artifact-dir and rerun canon geo evaluate".to_string()),
        )
        .unwrap_or(2));
    }
    Ok(())
}

fn run_stack_evidence(args: &GeoStackEvidenceCli) -> Result<u8, Box<dyn Error>> {
    let next_command =
        "canon geo stack-evidence --population <POPULATION.json> --overlay <OVERLAY.json>";
    let population = match read_population_or_stack(&args.population, next_command) {
        Ok(population) => population,
        Err(exit_code) => return Ok(exit_code),
    };
    let request: GeoPopulationEvidenceStackRequest = match read_request(
        &args.overlay,
        "overlay",
        CANON_GEO_POPULATION_EVIDENCE_STACK_REQUEST_VERSION,
        next_command,
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match stack_population_evidence(&population, &request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_evidence_stack_error(error),
    };
    match canonical_population_evidence_stack_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_evidence_stack_error(error),
    }
}

fn read_population_or_stack(
    path: &Path,
    next_command: &str,
) -> Result<GeoPopulationEvaluationRequest, u8> {
    let value: Value = read_request(
        path,
        "population",
        "canon_geo_population_request.v0 or canon_geo_population_evidence_stack.v0",
        next_command,
    )?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match version {
        CANON_GEO_POPULATION_REQUEST_VERSION => serde_json::from_value(value).map_err(|error| {
            emit_refusal(
                RefusalCode::EParse,
                "Could not parse the Geo --population request",
                json!({
                    "population": path_string(path),
                    "expected_version": CANON_GEO_POPULATION_REQUEST_VERSION,
                    "error": error.to_string(),
                }),
                Some(next_command.to_string()),
            )
            .unwrap_or(2)
        }),
        CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION => {
            let artifact: GeoPopulationEvidenceStackArtifact = serde_json::from_value(value)
                .map_err(|error| {
                    emit_refusal(
                        RefusalCode::EParse,
                        "Could not parse the Geo --population evidence-stack artifact",
                        json!({
                            "population": path_string(path),
                            "expected_version": CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
                            "error": error.to_string(),
                        }),
                        Some(next_command.to_string()),
                    )
                    .unwrap_or(2)
                })?;
            validate_population_evidence_stack_artifact(&artifact)
                .map_err(|error| emit_evidence_stack_error(error).unwrap_or(2))?;
            Ok(artifact.population)
        }
        _ => Err(emit_refusal(
            RefusalCode::EEntityArtifactContract,
            "Geo --population has an unsupported contract version",
            json!({
                "population": path_string(path),
                "actual_version": version,
                "supported_versions": [
                    CANON_GEO_POPULATION_REQUEST_VERSION,
                    CANON_GEO_POPULATION_EVIDENCE_STACK_VERSION,
                ],
            }),
            Some(next_command.to_string()),
        )
        .unwrap_or(2)),
    }
}

#[derive(Debug, Clone)]
struct GeoRunInputFile {
    node_id: String,
    binding_id: String,
    path: PathBuf,
    contract_version: String,
    content_digest: String,
    byte_count: u64,
    bytes: Vec<u8>,
}

fn read_geo_run_inputs(values: &[String]) -> Result<Vec<GeoRunInputFile>, u8> {
    let mut seen = BTreeSet::new();
    let mut inputs = Vec::with_capacity(values.len());
    for value in values {
        let assignment = match parse_geo_run_input_assignment(value) {
            Ok(assignment) => assignment,
            Err(message) => {
                return Err(emit_refusal(
                    RefusalCode::EParse,
                    "Geo run --input must be NODE_ID:BINDING_ID=PATH",
                    json!({
                        "input": value,
                        "error": message,
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        };
        let key = (assignment.node_id.clone(), assignment.binding_id.clone());
        if !seen.insert(key.clone()) {
            return Err(emit_refusal(
                RefusalCode::EParse,
                "Geo run --input declares the same node binding more than once",
                json!({
                    "node_id": key.0,
                    "binding_id": key.1,
                }),
                Some(GEO_RUN_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
        let bytes = match fs::read(&assignment.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(emit_refusal(
                    RefusalCode::EIo,
                    "Could not read the Geo --input file",
                    json!({
                        "input": value,
                        "path": path_string(&assignment.path),
                        "error": error.to_string(),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        };
        let contract_version = json_contract_version(&bytes, &assignment.path)?;
        let byte_count = match u64::try_from(bytes.len()) {
            Ok(byte_count) => byte_count,
            Err(_) => {
                return Err(emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo run --input file byte count overflowed u64",
                    json!({
                        "path": path_string(&assignment.path),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        };
        inputs.push(GeoRunInputFile {
            node_id: assignment.node_id,
            binding_id: assignment.binding_id,
            path: assignment.path,
            contract_version,
            content_digest: blake3_prefixed(&bytes),
            byte_count,
            bytes,
        });
    }
    inputs.sort_by(|left, right| {
        (&left.node_id, &left.binding_id, &left.path).cmp(&(
            &right.node_id,
            &right.binding_id,
            &right.path,
        ))
    });
    Ok(inputs)
}

fn read_geo_satisfaction_file_bindings(
    values: &[String],
    flag: &str,
    expected_shape: &str,
) -> Result<Vec<GeoSatisfactionFileBinding>, u8> {
    let mut seen = BTreeSet::new();
    let mut bindings = Vec::with_capacity(values.len());
    for value in values {
        let (binding_id, path) = match value.split_once('=') {
            Some(parts) => parts,
            None => {
                return Err(emit_refusal(
                    RefusalCode::EParse,
                    format!("Geo replan --{flag} must be {expected_shape}"),
                    json!({
                        "input": value,
                        "error": "missing '=' separator",
                    }),
                    Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        };
        if binding_id.is_empty()
            || path.is_empty()
            || binding_id.trim() != binding_id
            || path.trim() != path
        {
            return Err(emit_refusal(
                RefusalCode::EParse,
                format!("Geo replan --{flag} must contain a trimmed id and path"),
                json!({
                    "input": value,
                    "expected": expected_shape,
                }),
                Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
        if !seen.insert(binding_id.to_string()) {
            return Err(emit_refusal(
                RefusalCode::EParse,
                format!("Geo replan --{flag} declares the same id more than once"),
                json!({
                    "binding_id": binding_id,
                }),
                Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
        bindings.push(GeoSatisfactionFileBinding {
            binding_id: binding_id.to_string(),
            path: PathBuf::from(path),
        });
    }
    bindings.sort_by(|left, right| {
        (&left.binding_id, &left.path).cmp(&(&right.binding_id, &right.path))
    });
    Ok(bindings)
}

#[derive(Debug)]
struct GeoSidecarPublishError {
    target: String,
    temp_path: Option<String>,
    message: String,
}

fn publish_replan_advancement_sidecar(
    path: &Path,
    bytes: &[u8],
) -> Result<(), GeoSidecarPublishError> {
    match existing_replan_sidecar_matches(path, bytes)? {
        Some(true) => return Ok(()),
        Some(false) => {
            return Err(GeoSidecarPublishError {
                target: path_string(path),
                temp_path: None,
                message: "target already exists with different bytes".to_string(),
            });
        }
        None => {}
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(GeoSidecarPublishError {
            target: path_string(path),
            temp_path: None,
            message: "parent directory does not exist".to_string(),
        });
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| GeoSidecarPublishError {
            target: path_string(path),
            temp_path: None,
            message: "advancement output path must name a file".to_string(),
        })?;
    let digest = blake3::hash(bytes).to_hex().to_string();
    for attempt in 0_u8..64 {
        let temp_path = parent.join(format!(".{file_name}.tmp.{}.{}", &digest[..16], attempt));
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(GeoSidecarPublishError {
                    target: path_string(path),
                    temp_path: Some(path_string(&temp_path)),
                    message: error.to_string(),
                });
            }
        };
        if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&temp_path);
            return Err(GeoSidecarPublishError {
                target: path_string(path),
                temp_path: Some(path_string(&temp_path)),
                message: error.to_string(),
            });
        }
        drop(file);
        match fs::hard_link(&temp_path, path) {
            Ok(()) => {
                let _ = fs::remove_file(&temp_path);
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let existing = existing_replan_sidecar_matches(path, bytes);
                let _ = fs::remove_file(&temp_path);
                match existing? {
                    Some(true) => return Ok(()),
                    Some(false) => {
                        return Err(GeoSidecarPublishError {
                            target: path_string(path),
                            temp_path: None,
                            message: "target was concurrently created with different bytes"
                                .to_string(),
                        });
                    }
                    None => continue,
                }
            }
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                return Err(GeoSidecarPublishError {
                    target: path_string(path),
                    temp_path: Some(path_string(&temp_path)),
                    message: error.to_string(),
                });
            }
        }
    }
    Err(GeoSidecarPublishError {
        target: path_string(path),
        temp_path: None,
        message: "could not allocate a collision-free sibling temp path".to_string(),
    })
}

fn existing_replan_sidecar_matches(
    path: &Path,
    bytes: &[u8],
) -> Result<Option<bool>, GeoSidecarPublishError> {
    let mut file = match open_replan_sidecar_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(GeoSidecarPublishError {
                target: path_string(path),
                temp_path: None,
                message: error.to_string(),
            });
        }
    };
    let metadata = file.metadata().map_err(|error| GeoSidecarPublishError {
        target: path_string(path),
        temp_path: None,
        message: error.to_string(),
    })?;
    if !metadata.file_type().is_file() {
        return Err(GeoSidecarPublishError {
            target: path_string(path),
            temp_path: None,
            message: "target already exists and is not a regular file".to_string(),
        });
    }
    let mut existing = Vec::new();
    file.read_to_end(&mut existing)
        .map(|_| Some(existing == bytes))
        .map_err(|error| GeoSidecarPublishError {
            target: path_string(path),
            temp_path: None,
            message: error.to_string(),
        })
}

#[cfg(unix)]
fn open_replan_sidecar_no_follow(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_replan_sidecar_no_follow(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_OPEN_REPARSE_POINT makes the handle refer to the link entry
    // itself instead of following it to an arbitrary target.
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_replan_sidecar_no_follow(path: &Path) -> io::Result<fs::File> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(error),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "idempotent sidecar comparison requires no-follow file handles",
        )),
    }
}

#[derive(Debug, Clone)]
struct GeoRunInputAssignment {
    node_id: String,
    binding_id: String,
    path: PathBuf,
}

fn parse_geo_run_input_assignment(value: &str) -> Result<GeoRunInputAssignment, String> {
    let (target, path) = value
        .split_once('=')
        .ok_or_else(|| "missing '=' separator".to_string())?;
    let mut target_parts = target.split(':');
    let node_id = target_parts
        .next()
        .ok_or_else(|| "missing node id".to_string())?;
    let binding_id = target_parts
        .next()
        .ok_or_else(|| "missing binding id".to_string())?;
    if target_parts.next().is_some() {
        return Err("left side must be exactly NODE_ID:BINDING_ID".to_string());
    }
    if node_id.is_empty()
        || binding_id.is_empty()
        || path.is_empty()
        || node_id.trim() != node_id
        || binding_id.trim() != binding_id
        || path.trim() != path
    {
        return Err("node id, binding id, and path must be non-empty and trimmed".to_string());
    }
    Ok(GeoRunInputAssignment {
        node_id: node_id.to_string(),
        binding_id: binding_id.to_string(),
        path: PathBuf::from(path),
    })
}

fn json_contract_version(bytes: &[u8], path: &Path) -> Result<String, u8> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        emit_refusal(
            RefusalCode::EParse,
            "Could not parse the Geo --input file",
            json!({
                "path": path_string(path),
                "expected_version": "typed Geo JSON artifact",
                "error": error.to_string(),
            }),
            Some(GEO_RUN_NEXT_COMMAND.to_string()),
        )
        .unwrap_or(2)
    })?;
    value
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.is_empty() && version.trim() == *version)
        .map(str::to_string)
        .ok_or_else(|| {
            emit_refusal(
                RefusalCode::EEntityArtifactContract,
                "Geo --input file must carry a non-empty string version",
                json!({
                    "path": path_string(path),
                }),
                Some(GEO_RUN_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2)
        })
}

fn validate_geo_satisfactions(
    plan: &GeoPlan,
    values: &[String],
    inputs: &[GeoRunInputFile],
) -> Result<Vec<GeoRunInputBinding>, u8> {
    let mut seen = BTreeSet::new();
    let mut assignments = Vec::with_capacity(values.len());
    for value in values {
        let assignment = match parse_geo_satisfaction_assignment(value) {
            Ok(assignment) => assignment,
            Err(error) => return Err(emit_satisfy_error(error).unwrap_or(2)),
        };
        if !seen.insert(assignment.request_id.clone()) {
            return Err(emit_refusal(
                RefusalCode::EParse,
                "Geo run --satisfy declares the same request more than once",
                json!({
                    "request_id": assignment.request_id,
                }),
                Some(GEO_RUN_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
        assignments.push(assignment);
    }
    let mut verified_run_bindings = Vec::new();
    for assignment in assignments {
        let receipt = read_geo_acquisition_receipt(&assignment)?;
        let run_input_files =
            pair_receipt_run_input_targets(&assignment.request_id, &receipt, inputs)?;
        let result_digest_files =
            pair_receipt_result_digests(&assignment.request_id, &receipt, inputs)?;
        let satisfaction = match satisfy_geo_acquisition_for_run(GeoSatisfactionRunInput {
            plan,
            inventory: None,
            assignment,
            run_input_files,
            result_digest_files,
        }) {
            Ok(satisfaction) => satisfaction,
            Err(error) => return Err(emit_satisfy_error(error).unwrap_or(2)),
        };
        if satisfaction.satisfaction.status != GeoSatisfactionStatus::Satisfied {
            return Err(emit_refusal(
                RefusalCode::EEntityArtifactContract,
                "Geo run --satisfy receipt did not meet its positive acquisition gate",
                json!({
                    "request_id": satisfaction.satisfaction.request_id,
                    "status": code_name(&satisfaction.satisfaction.status),
                    "findings": satisfaction.satisfaction.findings,
                }),
                Some(GEO_RUN_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
        verified_run_bindings.extend(satisfaction.run_input_bindings);
    }
    verified_run_bindings.sort_by(|left, right| {
        (&left.node_id, &left.binding_id).cmp(&(&right.node_id, &right.binding_id))
    });
    Ok(verified_run_bindings)
}

fn read_geo_acquisition_receipt(
    assignment: &GeoSatisfactionAssignment,
) -> Result<GeoAcquisitionReceipt, u8> {
    let bytes = match fs::read(&assignment.receipt_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EIo,
                "Could not read the Geo --satisfy receipt file",
                json!({
                    "request_id": assignment.request_id.as_str(),
                    "receipt": path_string(&assignment.receipt_path),
                    "error": error.to_string(),
                }),
                Some(GEO_RUN_NEXT_COMMAND.to_string()),
            )
            .unwrap_or(2));
        }
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        emit_refusal(
            RefusalCode::EParse,
            "Could not parse the Geo --satisfy receipt file",
            json!({
                "request_id": assignment.request_id.as_str(),
                "receipt": path_string(&assignment.receipt_path),
                "expected_version": CANON_GEO_ACQUISITION_RECEIPT_VERSION,
                "error": error.to_string(),
            }),
            Some(GEO_RUN_NEXT_COMMAND.to_string()),
        )
        .unwrap_or(2)
    })
}

fn pair_receipt_run_input_targets(
    request_id: &str,
    receipt: &GeoAcquisitionReceipt,
    inputs: &[GeoRunInputFile],
) -> Result<Vec<GeoSatisfactionRunInputFileBinding>, u8> {
    let mut bindings = Vec::with_capacity(receipt.local_artifacts.len());
    for artifact in &receipt.local_artifacts {
        let expected_digest =
            match blake3_digest_string(artifact.digest.algorithm, &artifact.digest.hex_digest) {
                Some(digest) => digest,
                None => {
                    return Err(emit_unsupported_receipt_digest_refusal(
                        request_id,
                        artifact.artifact_id.as_str(),
                        artifact.digest.algorithm,
                    ));
                }
            };
        let candidates = matching_inputs(inputs, &expected_digest, artifact.byte_count);
        match candidates.as_slice() {
            [input] => bindings.push(GeoSatisfactionRunInputFileBinding {
                local_artifact_id: artifact.artifact_id.clone(),
                node_id: input.node_id.clone(),
                binding_id: input.binding_id.clone(),
                contract_version: input.contract_version.clone(),
                path: input.path.clone(),
            }),
            [] => {
                return Err(emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo run could not match a receipt local artifact to any explicit --input file",
                    json!({
                        "request_id": request_id,
                        "artifact_id": artifact.artifact_id.as_str(),
                        "expected_digest": expected_digest,
                        "expected_byte_count": artifact.byte_count,
                        "input_count": inputs.len(),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
            _ => {
                return Err(emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo run receipt local artifact matches more than one explicit --input file",
                    json!({
                        "request_id": request_id,
                        "artifact_id": artifact.artifact_id.as_str(),
                        "expected_digest": expected_digest,
                        "expected_byte_count": artifact.byte_count,
                        "candidates": candidates.iter().map(|input| input.describe()).collect::<Vec<_>>(),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        }
    }
    Ok(bindings)
}

fn emit_unsupported_receipt_digest_refusal(
    request_id: &str,
    artifact_id: &str,
    algorithm: GeoDigestAlgorithm,
) -> u8 {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo run can pair only BLAKE3 local artifact digests to explicit --input files",
        json!({
            "request_id": request_id,
            "artifact_id": artifact_id,
            "digest_algorithm": format!("{:?}", algorithm),
        }),
        Some(GEO_RUN_NEXT_COMMAND.to_string()),
    )
    .unwrap_or(2)
}

fn pair_receipt_result_digests(
    request_id: &str,
    receipt: &GeoAcquisitionReceipt,
    inputs: &[GeoRunInputFile],
) -> Result<Vec<GeoSatisfactionFileBinding>, u8> {
    let local_digests = receipt
        .local_artifacts
        .iter()
        .filter_map(|artifact| {
            blake3_digest_string(artifact.digest.algorithm, &artifact.digest.hex_digest)
        })
        .collect::<BTreeSet<_>>();
    let mut bindings = Vec::new();
    for digest in &receipt.result_digests {
        let Some(expected_digest) = blake3_digest_string(digest.algorithm, &digest.hex_digest)
        else {
            continue;
        };
        if local_digests.contains(&expected_digest) {
            continue;
        }
        let candidates = matching_inputs(inputs, &expected_digest, receipt.counts.bytes);
        match candidates.as_slice() {
            [input] => bindings.push(GeoSatisfactionFileBinding {
                binding_id: digest.digest_id.clone(),
                path: input.path.clone(),
            }),
            [] => {
                return Err(emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo run could not match a receipt result digest to any explicit --input file",
                    json!({
                        "request_id": request_id,
                        "digest_id": digest.digest_id.as_str(),
                        "expected_digest": expected_digest,
                        "expected_byte_count": receipt.counts.bytes,
                        "input_count": inputs.len(),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
            _ => {
                return Err(emit_refusal(
                    RefusalCode::EEntityArtifactContract,
                    "Geo run receipt result digest matches more than one explicit --input file",
                    json!({
                        "request_id": request_id,
                        "digest_id": digest.digest_id.as_str(),
                        "expected_digest": expected_digest,
                        "expected_byte_count": receipt.counts.bytes,
                        "candidates": candidates.iter().map(|input| input.describe()).collect::<Vec<_>>(),
                    }),
                    Some(GEO_RUN_NEXT_COMMAND.to_string()),
                )
                .unwrap_or(2));
            }
        }
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(bindings)
}

fn matching_inputs<'a>(
    inputs: &'a [GeoRunInputFile],
    expected_digest: &str,
    expected_byte_count: u64,
) -> Vec<&'a GeoRunInputFile> {
    let mut candidates = inputs
        .iter()
        .filter(|input| {
            input.content_digest == expected_digest && input.byte_count == expected_byte_count
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.describe());
    candidates
}

impl GeoRunInputFile {
    fn describe(&self) -> String {
        format!(
            "{}:{}={}",
            self.node_id,
            self.binding_id,
            path_string(&self.path)
        )
    }
}

fn blake3_digest_string(algorithm: GeoDigestAlgorithm, hex_digest: &str) -> Option<String> {
    match algorithm {
        GeoDigestAlgorithm::Blake3 => Some(format!("blake3:{hex_digest}")),
        _ => None,
    }
}

fn blake3_prefixed(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// Read and parse one typed Geo request file.
///
/// IO and parse failures are distinct refusals so an operator can tell a
/// missing file from malformed JSON. Version checking stays in the library so
/// that a mismatch is reported by the kernel's own typed error code.
fn read_request<T: DeserializeOwned>(
    path: &Path,
    flag: &str,
    expected_version: &str,
    next_command: &str,
) -> Result<T, u8> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(emit_refusal(
                RefusalCode::EIo,
                format!("Could not read the Geo --{flag} file"),
                json!({
                    flag: path_string(path),
                    "error": error.to_string(),
                }),
                Some(next_command.to_string()),
            )
            .unwrap_or(2));
        }
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        emit_refusal(
            RefusalCode::EParse,
            format!("Could not parse the Geo --{flag} file"),
            json!({
                flag: path_string(path),
                "expected_version": expected_version,
                "error": error.to_string(),
            }),
            Some(next_command.to_string()),
        )
        .unwrap_or(2)
    })
}

fn emit_composition_error(error: GeoCompositionError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo composition request could not be solved",
        json!({
            "geo_composition_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the composition request against canon_geo_composition_request.v0, then rerun canon geo solve"
                .to_string(),
        ),
    )
}

fn emit_evidence_error(error: GeoEvidenceError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo evidence request could not be compiled",
        json!({
            "geo_evidence_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the evidence request against canon_geo_evidence_request.v0, then rerun canon geo compile-evidence"
                .to_string(),
        ),
    )
}

fn emit_materialization_error(error: GeoMaterializationError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo warehouse rows could not be materialized",
        json!({
            "geo_materialization_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the rows against canon_geo_warehouse_rows.v0, then rerun canon geo materialize-evidence"
                .to_string(),
        ),
    )
}

fn emit_address_error(error: GeoAddressError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo address/PAD evidence request could not be materialized",
        json!({
            "geo_address_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the request against canon_geo_address_parcel_evidence_request.v0, then rerun canon geo materialize-address-evidence"
                .to_string(),
        ),
    )
}

fn emit_h7_population_materialization_error(
    error: GeoMaterializationError,
) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo H.7 population rows could not be materialized",
        json!({
            "geo_materialization_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the rows against canon_geo_h7_population_rows.v0, then rerun canon geo materialize-h7-population"
                .to_string(),
        ),
    )
}

fn emit_geometry_error(error: GeoGeometryError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo geometry request could not be materialized",
        json!({
            "geo_geometry_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
            "budget": error.budget,
        }),
        Some(
            "repair the request against canon_geo_geometry_request.v0, then rerun canon geo materialize-geometry"
                .to_string(),
        ),
    )
}

fn emit_tile_error(error: GeoTileError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo tile request could not be materialized or reconciled",
        json!({
            "geo_tile_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the request against its canon_geo_tile_*.v0 contract, then rerun the same canon geo command"
                .to_string(),
        ),
    )
}

fn emit_home_cell_error(error: GeoTileError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo home-cell rows could not be materialized",
        json!({
            "geo_tile_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the rows against canon_geo_home_cell_rows.v1, then rerun canon geo materialize-home-cells"
                .to_string(),
        ),
    )
}

fn emit_population_error(error: GeoPopulationError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo population request could not be evaluated",
        json!({
            "geo_population_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the population request against canon_geo_population_request.v0, then rerun canon geo evaluate"
                .to_string(),
        ),
    )
}

fn emit_evidence_stack_error(error: GeoEvidenceStackError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo population evidence could not be stacked",
        json!({
            "geo_evidence_stack_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the inputs against canon_geo_population_request.v0 and canon_geo_population_evidence_stack_request.v0, then rerun canon geo stack-evidence"
                .to_string(),
        ),
    )
}

fn emit_plan_error(error: GeoPlanError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo plan request could not be compiled",
        json!({
            "geo_plan_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the plan inputs against canon_geo_question.v0, canon_geo_capabilities.v0, canon_geo_regional_inventory.v1, canon_geo_composition_profile.v0, and canon_geo_resource_budget.v0, then rerun canon geo plan"
                .to_string(),
        ),
    )
}

fn emit_run_error(error: GeoRunError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo run request could not be executed",
        json!({
            "geo_run_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(format!(
            "repair the plan and explicit inputs against {CANON_GEO_RUN_VERSION}, then rerun canon geo run"
        )),
    )
}

fn emit_replan_plan_error(error: GeoPlanError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo acquisition replan could not be compiled",
        json!({
            "geo_plan_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
    )
}

fn emit_satisfy_error(error: GeoSatisfyError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo acquisition satisfaction could not be validated",
        json!({
            "geo_satisfy_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(
            "repair the REQUEST_ID=RECEIPT.json handoff and explicit --input bytes, then rerun canon geo run"
                .to_string(),
        ),
    )
}

fn emit_replan_satisfy_error(error: GeoSatisfyError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo acquisition satisfaction could not be validated for replan",
        json!({
            "geo_satisfy_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some(GEO_REPLAN_FROM_ACQUISITION_NEXT_COMMAND.to_string()),
    )
}

fn emit_control_error(error: GeoControlError) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo control contract could not be emitted",
        json!({
            "geo_control_error_code": code_name(&error.code),
            "message": error.message,
            "detail": error.detail,
        }),
        Some("canon geo capabilities --emit json".to_string()),
    )
}

fn emit_serialization_refusal(
    contract: &str,
    error: &serde_json::Error,
) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        RefusalCode::EEntityArtifactContract,
        "Geo artifact could not be serialized",
        json!({
            "contract": contract,
            "error": error.to_string(),
        }),
        Some("rerun the same canon geo command with a smaller declared budget".to_string()),
    )
}

fn emit_refusal(
    code: RefusalCode,
    message: impl Into<String>,
    detail: Value,
    next_command: Option<String>,
) -> Result<u8, Box<dyn Error>> {
    let output: CanonOutput = refusal::create_refusal(code, message.into(), detail, next_command);
    println!("{}", serde_json::to_string(&output)?);
    Ok(2)
}

fn emit_library_refusal(refusal: Refusal) -> Result<u8, Box<dyn Error>> {
    emit_refusal(
        refusal.code,
        refusal.message,
        refusal.detail,
        refusal.next_command,
    )
}

fn write_canonical(bytes: &[u8]) -> Result<u8, Box<dyn Error>> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(0)
}

/// Render a typed library error code through its own serde contract so the
/// refusal detail names the same token the artifact schemas use.
fn code_name(code: &impl Serialize) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
