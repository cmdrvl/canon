//! Cross-tape structural resolution workbench scaffold.
//!
//! The production lookup path remains exact registry lookup. This module is the
//! bounded workbench that will turn two-tape structural evidence into flat
//! registry entries once the phase-1/phase-2 implementation beads land.

use crate::{registry::load_registry, witness};
use serde_json::{Map, Value, json};
use std::{fs, path::Path};

pub mod assertions;
pub mod gold;
pub mod graph;
pub mod output;
pub mod scoring;
pub mod strategy;
pub mod tape;
pub mod types;
pub mod writeback;

pub use assertions::*;
pub use gold::*;
pub use graph::*;
pub use output::*;
pub use scoring::*;
pub use strategy::*;
pub use tape::*;
pub use types::*;
pub use writeback::*;

pub fn run(request: ResolveRequest) -> ResolveResult<ResolveArtifact> {
    let strategy = load_strategy(&request.strategy)?;
    let registry = load_registry_for_resolve(&request.registry)?;
    let tapes = load_tapes(
        &request.reference_tape,
        &request.target_tape,
        &strategy,
        TapeLoadOptions {
            max_rows: request.max_rows,
            max_bytes: request.max_bytes,
        },
    )?;
    let selection = select_candidates(&tapes, &strategy, Some(&registry), request.max_candidates)?;
    let decisions = score_candidates(&selection, &strategy, Some(&registry));
    let gold_score = request
        .gold
        .as_deref()
        .map(|path| score_gold_file(path, &decisions))
        .transpose()?;
    let write_back = if request.write_back {
        Some(write_back_matches(WriteBackRequest {
            registry_dir: &request.registry,
            strategy: &strategy,
            matches: &decisions.matches,
            gold_score: gold_score.as_ref(),
            write_back: true,
            mapping_file_name: None,
        })?)
    } else {
        None
    };

    let artifact = build_artifact(
        &strategy, &registry, &tapes, decisions, gold_score, write_back,
    );
    append_resolve_witness(&request, &strategy, &registry, &artifact);
    Ok(artifact)
}

fn load_registry_for_resolve(path: &Path) -> ResolveResult<crate::Registry> {
    load_registry(path).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Registry,
            format!(
                "Cannot load registry '{}' for canon resolve: {}",
                path.display(),
                error
            ),
            json!({
                "registry": path.display().to_string(),
                "error": error.to_string()
            }),
        )
    })
}

fn append_resolve_witness(
    request: &ResolveRequest,
    strategy: &ResolveStrategy,
    registry: &crate::Registry,
    artifact: &ResolveArtifact,
) {
    if request.no_witness {
        return;
    }

    let output_bytes = match serde_json::to_vec(artifact) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("Warning: resolve witness skipped: {}", error);
            return;
        }
    };
    let output_hash = witness::hash_bytes(&output_bytes);
    let exit_code = artifact.exit_code();
    let outcome = if exit_code == 0 {
        "RESOLVED"
    } else {
        "PARTIAL"
    };
    let mut params = Map::new();
    params.insert("command".to_string(), Value::String("resolve".to_string()));
    params.insert(
        "registry_id".to_string(),
        Value::String(registry.meta.id.clone()),
    );
    params.insert(
        "registry_version".to_string(),
        Value::String(registry.meta.version.clone()),
    );
    params.insert(
        "strategy_id".to_string(),
        Value::String(strategy.id.clone()),
    );
    params.insert(
        "strategy_version".to_string(),
        Value::String(strategy.version.clone()),
    );
    params.insert(
        "strategy_hash".to_string(),
        Value::String(strategy.content_hash.clone()),
    );
    params.insert("write_back".to_string(), Value::Bool(request.write_back));
    params.insert(
        "summary".to_string(),
        json!({
            "target_records": artifact.summary.target_records,
            "matched": artifact.summary.matched,
            "unmatched": artifact.summary.unmatched,
            "ambiguous": artifact.summary.ambiguous,
            "match_rate": artifact.summary.match_rate
        }),
    );
    if let Some(max_candidates) = request.max_candidates {
        params.insert(
            "max_candidates".to_string(),
            Value::from(max_candidates as u64),
        );
    }
    if let Some(max_rows) = request.max_rows {
        params.insert("max_rows".to_string(), Value::from(max_rows as u64));
    }
    if let Some(max_bytes) = request.max_bytes {
        params.insert("max_bytes".to_string(), Value::from(max_bytes));
    }

    let mut inputs = vec![
        witness_input(&request.reference_tape),
        witness_input(&request.target_tape),
        witness_input(&request.strategy),
        witness::WitnessInput {
            path: request.registry.display().to_string(),
            hash: None,
            bytes: None,
        },
    ];
    if let Some(gold) = &request.gold {
        inputs.push(witness_input(gold));
    }

    let record = witness::WitnessRecord::new(inputs, params, &output_hash, outcome, exit_code);
    if let Err(error) = witness::append_witness_record(&record, request.no_witness) {
        eprintln!("Warning: failed to append resolve witness: {}", error);
    }
}

fn witness_input(path: &Path) -> witness::WitnessInput {
    witness::WitnessInput {
        path: path.display().to_string(),
        hash: witness::hash_file(path).ok(),
        bytes: fs::metadata(path).ok().map(|metadata| metadata.len()),
    }
}
