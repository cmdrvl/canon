#![forbid(unsafe_code)]

//! Operator surface for the bounded Geo workbench.
//!
//! Each subcommand reads one typed JSON request, calls the library kernel, and
//! writes the canonical artifact bytes to stdout. Domain failures are typed
//! refusals, never panics: version mismatches, budget exhaustion, and invalid
//! inputs all surface through the library's own error codes.

use crate::{
    CanonOutput, RefusalCode,
    cli::{GeoCli, GeoCompileEvidenceCli, GeoEvaluateCli, GeoSolveCli, GeoSubcommand},
    refusal,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};

use super::{
    composition::{
        CANON_GEO_COMPOSITION_REQUEST_VERSION, GeoCompositionError, GeoCompositionRequest,
        canonical_composition_bytes, solve_composition,
    },
    evaluation::{
        CANON_GEO_POPULATION_REQUEST_VERSION, GeoPopulationError, GeoPopulationEvaluationRequest,
        canonical_population_evaluation_bytes, evaluate_population,
    },
    evidence::{
        CANON_GEO_EVIDENCE_REQUEST_VERSION, GeoEvidenceCompilationRequest, GeoEvidenceError,
        canonical_evidence_compilation_bytes, compile_evidence,
    },
};

pub fn run(geo: &GeoCli) -> Result<u8, Box<dyn Error>> {
    match &geo.command {
        GeoSubcommand::Solve(args) => run_solve(args),
        GeoSubcommand::CompileEvidence(args) => run_compile_evidence(args),
        GeoSubcommand::Evaluate(args) => run_evaluate(args),
    }
}

fn run_solve(args: &GeoSolveCli) -> Result<u8, Box<dyn Error>> {
    let request: GeoCompositionRequest = match read_request(
        &args.request,
        "request",
        CANON_GEO_COMPOSITION_REQUEST_VERSION,
        "canon geo solve --request <REQUEST.json>",
    ) {
        Ok(request) => request,
        Err(exit_code) => return Ok(exit_code),
    };
    let artifact = match solve_composition(&request) {
        Ok(artifact) => artifact,
        Err(error) => return emit_composition_error(error),
    };
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
