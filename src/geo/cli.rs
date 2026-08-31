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
        GeoCli, GeoCompileEvidenceCli, GeoEvaluateCli, GeoLinkSourcesCli,
        GeoMaterializeEvidenceCli, GeoMaterializeGeometryCli, GeoMaterializeH7PopulationCli,
        GeoMaterializeH7StagingBatchCli, GeoMaterializeHomeCellsCli,
        GeoMaterializeWarehouseGeometryCli, GeoReconcileTilesCli, GeoSolveCli, GeoSubcommand,
        GeoTileWorkCli,
    },
    refusal,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};

use super::{
    composition::{
        GeoCompositionError, GeoCompositionRequest, GeoEvidenceCompilationReference,
        canonical_composition_bytes, solve_composition,
    },
    evaluation::{
        CANON_GEO_POPULATION_REQUEST_VERSION, GeoPopulationError, GeoPopulationEvaluationRequest,
        canonical_population_evaluation_bytes, evaluate_population,
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
        CANON_GEO_H7_POPULATION_ROWS_VERSION, CANON_GEO_H7_POPULATION_VERSION,
        CANON_GEO_H7_STAGING_SOURCE_RECORD_BYTES_BATCH_VERSION, CANON_GEO_WAREHOUSE_ROWS_VERSION,
        GeoH7PopulationRowsRequest, GeoH7StagingSourceRecordBytesBatchRequest,
        GeoMaterializationError, GeoWarehouseRowsRequest, canonical_h7_population_bytes,
        canonical_materialized_evidence_request_bytes, materialize_h7_population_rows,
        materialize_h7_staging_source_record_bytes_batch, materialize_warehouse_rows,
    },
    multisource::{
        CANON_GEO_MULTISOURCE_REQUEST_VERSION, GeoMultisourceRequest,
        canonical_multisource_artifact_bytes, materialize_geo_multisource,
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

pub fn run(geo: &GeoCli) -> Result<u8, Box<dyn Error>> {
    match &geo.command {
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
        GeoSubcommand::MaterializeH7Population(args) => run_materialize_h7_population(args),
        GeoSubcommand::MaterializeH7StagingBatch(args) => run_materialize_h7_staging_batch(args),
        GeoSubcommand::CompileEvidence(args) => run_compile_evidence(args),
        GeoSubcommand::Evaluate(args) => run_evaluate(args),
    }
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
    let request: GeoPopulationEvaluationRequest = match read_request(
        &args.population,
        "population",
        CANON_GEO_POPULATION_REQUEST_VERSION,
        "canon geo evaluate --population <POPULATION.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match evaluate_population(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_population_error(error),
    };
    match canonical_population_evaluation_bytes(&artifact) {
        Ok(bytes) => write_canonical(&bytes),
        Err(error) => emit_serialization_refusal("canon_geo_population_evaluation.v0", &error),
    }
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
            "repair the rows against canon_geo_home_cell_rows.v0, then rerun canon geo materialize-home-cells"
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
