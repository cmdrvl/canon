//! Entity-link decision workbench.
//!
//! The production lookup path remains exact registry lookup. This module is the
//! bounded internal engine that turns two-row-set linkage evidence into flat
//! registry entries after explicit review or write-back gates.

use crate::{entity::run::link, registry::load_registry, witness};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveNativeEntityLinkRequest {
    pub reference_tape: PathBuf,
    pub target_tape: PathBuf,
    pub profile: String,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityLinkDecisionRequest {
    pub reference_tape: PathBuf,
    pub target_tape: PathBuf,
    pub strategy: PathBuf,
    pub registry: PathBuf,
    pub gold: Option<PathBuf>,
    pub write_back: bool,
    pub max_candidates: Option<usize>,
    pub max_rows: Option<usize>,
    pub max_bytes: Option<u64>,
}

pub fn run(request: ResolveRequest) -> ResolveResult<ResolveArtifact> {
    let production = produce_resolve_decisions(ResolveDecisionProductionRequest {
        reference_tape: &request.reference_tape,
        target_tape: &request.target_tape,
        strategy_path: &request.strategy,
        registry_path: &request.registry,
        gold_path: request.gold.as_deref(),
        max_candidates: request.max_candidates,
        load_options: TapeLoadOptions {
            max_rows: request.max_rows,
            max_bytes: request.max_bytes,
        },
    })?;
    let write_back = if request.write_back {
        Some(write_back_matches(WriteBackRequest {
            registry_dir: &request.registry,
            strategy: &production.strategy,
            matches: &production.decisions.matches,
            gold_score: production.gold_score.as_ref(),
            write_back: true,
            mapping_file_name: None,
        })?)
    } else {
        None
    };

    let artifact = build_resolve_artifact(production, write_back);
    append_resolve_witness(&request, &artifact.strategy, &artifact.registry, &artifact);
    Ok(artifact)
}

pub fn produce_entity_link_decisions(
    request: EntityLinkDecisionRequest,
) -> ResolveResult<ResolveArtifact> {
    let production = produce_resolve_decisions(ResolveDecisionProductionRequest {
        reference_tape: &request.reference_tape,
        target_tape: &request.target_tape,
        strategy_path: &request.strategy,
        registry_path: &request.registry,
        gold_path: request.gold.as_deref(),
        max_candidates: request.max_candidates,
        load_options: TapeLoadOptions {
            max_rows: request.max_rows,
            max_bytes: request.max_bytes,
        },
    })?;
    let write_back = if request.write_back {
        Some(write_back_matches(WriteBackRequest {
            registry_dir: &request.registry,
            strategy: &production.strategy,
            matches: &production.decisions.matches,
            gold_score: production.gold_score.as_ref(),
            write_back: true,
            mapping_file_name: None,
        })?)
    } else {
        None
    };

    Ok(build_resolve_artifact(production, write_back))
}

pub fn run_native_entity_link(
    request: ResolveNativeEntityLinkRequest,
) -> ResolveResult<link::EntityLinkResult> {
    link::run_entity_link(link::EntityLinkRequest {
        reference_rows: &request.reference_tape,
        target_rows: &request.target_tape,
        profile: &request.profile,
        strategy: &request.strategy,
        registry: &request.registry,
        work_dir: &request.work_dir,
    })
    .map_err(|refusal| {
        ResolveError::with_detail(
            ResolveErrorCode::InputContract,
            "Native entity link refused while running shared entity stages",
            json!({
                "refusal": refusal,
                "engine": "entity.run",
                "mode": "directional_two_tape"
            }),
        )
    })
}

struct ResolveDecisionProduction {
    strategy: ResolveStrategy,
    registry: crate::Registry,
    tapes: LoadedTapes,
    decisions: MatchDecisions,
    gold_score: Option<GoldScore>,
}

struct ResolveDecisionProductionRequest<'a> {
    reference_tape: &'a Path,
    target_tape: &'a Path,
    strategy_path: &'a Path,
    registry_path: &'a Path,
    gold_path: Option<&'a Path>,
    max_candidates: Option<usize>,
    load_options: TapeLoadOptions,
}

fn produce_resolve_decisions(
    request: ResolveDecisionProductionRequest<'_>,
) -> ResolveResult<ResolveDecisionProduction> {
    let strategy = load_strategy(request.strategy_path)?;
    let registry = load_registry_for_resolve(request.registry_path)?;
    let tapes = load_tapes(
        request.reference_tape,
        request.target_tape,
        &strategy,
        request.load_options,
    )?;
    let selection = select_candidates(&tapes, &strategy, Some(&registry), request.max_candidates)?;
    let decisions = score_candidates(&selection, &strategy, Some(&registry));
    let gold_score = request
        .gold_path
        .map(|path| score_gold_file(path, &decisions))
        .transpose()?;

    Ok(ResolveDecisionProduction {
        strategy,
        registry,
        tapes,
        decisions,
        gold_score,
    })
}

fn build_resolve_artifact(
    production: ResolveDecisionProduction,
    write_back: Option<WriteBackSummary>,
) -> ResolveArtifact {
    build_artifact(
        &production.strategy,
        &production.registry,
        &production.tapes,
        production.decisions,
        production.gold_score,
        write_back,
    )
}

fn load_registry_for_resolve(path: &Path) -> ResolveResult<crate::Registry> {
    load_registry(path).map_err(|error| {
        ResolveError::with_detail(
            ResolveErrorCode::Registry,
            format!(
                "Cannot load registry '{}' for canon entity link: {}",
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
    strategy: &StrategyReference,
    registry: &ResolveRegistrySnapshot,
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
        Value::String(registry.id.clone()),
    );
    params.insert(
        "registry_version".to_string(),
        Value::String(registry.version.clone()),
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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolveLegacyParityRequest {
    reference_tape: PathBuf,
    target_tape: PathBuf,
    strategy: PathBuf,
    registry: PathBuf,
    gold: Option<PathBuf>,
    max_candidates: Option<usize>,
    max_rows: Option<usize>,
    max_bytes: Option<u64>,
}

#[cfg(test)]
fn produce_legacy_resolve_parity(
    request: ResolveLegacyParityRequest,
) -> ResolveResult<ResolveArtifact> {
    produce_entity_link_decisions(EntityLinkDecisionRequest {
        reference_tape: request.reference_tape,
        target_tape: request.target_tape,
        strategy: request.strategy,
        registry: request.registry,
        gold: request.gold,
        write_back: false,
        max_candidates: request.max_candidates,
        max_rows: request.max_rows,
        max_bytes: request.max_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{fs, path::Path};

    const UNCHANGED_REFERENCE_TAPE: &str =
        "tests/fixtures/resolve/parity/unchanged-link/reference_loans.link.csv";
    const UNCHANGED_TARGET_TAPE: &str =
        "tests/fixtures/resolve/parity/unchanged-link/target_loans.link.csv";
    const CMBS_STRATEGY: &str = "tests/fixtures/resolve/strategies/cmbs_loans.valid.yaml";
    const LOAN_MATCH_GOLD: &str = "tests/fixtures/resolve/gold/loan_matches.jsonl";
    const UNCHANGED_MANIFEST: &str = "tests/fixtures/resolve/parity/unchanged-link/manifest.json";
    const UNCHANGED_DECISION_GOLDEN: &str =
        "tests/fixtures/resolve/golden/unchanged_input_decision_projection.json";

    #[test]
    fn legacy_parity_projection_matches_unchanged_input_golden() {
        assert_unchanged_link_fixture_bytes_match_manifest();
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let registry = temp_dir.path().join("registry");
        copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);

        let artifact = produce_legacy_resolve_parity(ResolveLegacyParityRequest {
            reference_tape: PathBuf::from(UNCHANGED_REFERENCE_TAPE),
            target_tape: PathBuf::from(UNCHANGED_TARGET_TAPE),
            strategy: PathBuf::from(CMBS_STRATEGY),
            registry,
            gold: Some(PathBuf::from(LOAN_MATCH_GOLD)),
            max_candidates: None,
            max_rows: None,
            max_bytes: None,
        })
        .expect("legacy resolve parity projection");

        let expected = read_json(UNCHANGED_DECISION_GOLDEN);
        assert_eq!(
            legacy_decision_projection(&artifact),
            expected["projection"],
            "legacy unchanged-input decision projection"
        );
        assert!(
            artifact.write_back.is_none(),
            "legacy parity witness is read-only"
        );
    }

    #[test]
    fn legacy_parity_refusal_matches_manifest_contract_for_zero_candidates() {
        assert_unchanged_link_fixture_bytes_match_manifest();
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let registry = temp_dir.path().join("registry");
        copy_json_registry_fixture("tests/fixtures/registries/resolve-servicers", &registry);

        let error = produce_legacy_resolve_parity(ResolveLegacyParityRequest {
            reference_tape: PathBuf::from(UNCHANGED_REFERENCE_TAPE),
            target_tape: PathBuf::from(UNCHANGED_TARGET_TAPE),
            strategy: PathBuf::from(CMBS_STRATEGY),
            registry,
            gold: Some(PathBuf::from(LOAN_MATCH_GOLD)),
            max_candidates: Some(0),
            max_rows: None,
            max_bytes: None,
        })
        .expect_err("max_candidates=0 refuses internally");

        let manifest = read_json(UNCHANGED_MANIFEST);
        let contract = &manifest["refusal_contract"]["max_candidates_zero"];
        assert_eq!(
            serde_json::to_value(&error.code).expect("internal refusal code"),
            contract["internal_code"]
        );
        assert_eq!(contract["public_code"], "E_TOO_MANY_CANDIDATES");
        assert_eq!(
            error.detail.as_ref().expect("refusal detail")["max_candidates"],
            0
        );
    }

    fn assert_unchanged_link_fixture_bytes_match_manifest() {
        let manifest = read_json(UNCHANGED_MANIFEST);
        assert_eq!(
            manifest["runtime_inputs"]["reference"]["blake3"],
            blake3_file(Path::new(UNCHANGED_REFERENCE_TAPE))
        );
        assert_eq!(
            manifest["runtime_inputs"]["target"]["blake3"],
            blake3_file(Path::new(UNCHANGED_TARGET_TAPE))
        );
    }

    fn legacy_decision_projection(artifact: &ResolveArtifact) -> Value {
        let artifact = serde_json::to_value(artifact).expect("legacy artifact json");
        json!({
            "strategy": artifact["strategy"],
            "registry": {
                "id": artifact["registry"]["id"],
                "version": artifact["registry"]["version"]
            },
            "reference": {
                "rows_path": artifact["reference_tape"]["path"],
                "row_count": artifact["reference_tape"]["record_count"]
            },
            "target": {
                "rows_path": artifact["target_tape"]["path"],
                "row_count": artifact["target_tape"]["record_count"]
            },
            "summary": artifact["summary"],
            "matches": compact_matches(&artifact),
            "unmatched": compact_unmatched(&artifact),
            "ambiguous": compact_ambiguous(&artifact),
            "gold_score": artifact["gold_score"],
            "read_only": {
                "write_back_present": artifact.get("write_back").is_some()
            }
        })
    }

    fn compact_matches(decisions: &Value) -> Value {
        Value::Array(
            decisions["matches"]
                .as_array()
                .expect("matches")
                .iter()
                .map(|record| {
                    json!({
                        "target_id": record["target_id"],
                        "reference_id": record["reference_id"],
                        "canonical_id": record["canonical_id"],
                        "score": record["score"]
                    })
                })
                .collect(),
        )
    }

    fn compact_unmatched(decisions: &Value) -> Value {
        Value::Array(
            decisions["unmatched"]
                .as_array()
                .expect("unmatched")
                .iter()
                .map(|record| {
                    json!({
                        "target_id": record["target_id"],
                        "reason": record["reason"]
                    })
                })
                .collect(),
        )
    }

    fn compact_ambiguous(decisions: &Value) -> Value {
        Value::Array(
            decisions["ambiguous"]
                .as_array()
                .expect("ambiguous")
                .iter()
                .map(|record| {
                    let candidate_reference_ids = record["candidates"]
                        .as_array()
                        .expect("candidate array")
                        .iter()
                        .map(|candidate| candidate["reference_id"].clone())
                        .collect::<Vec<_>>();
                    json!({
                        "target_id": record["target_id"],
                        "reason": record["reason"],
                        "candidate_reference_ids": candidate_reference_ids
                    })
                })
                .collect(),
        )
    }

    fn copy_json_registry_fixture(src_relative: &str, destination: &Path) {
        fs::create_dir_all(destination).expect("registry destination");
        for entry in fs::read_dir(fixture_path(src_relative)).expect("registry fixture readable") {
            let source = entry.expect("registry fixture entry").path();
            if source.extension().and_then(|value| value.to_str()) == Some("json") {
                fs::copy(
                    &source,
                    destination.join(source.file_name().expect("registry file name")),
                )
                .expect("copy registry json");
            }
        }
    }

    fn fixture_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    fn blake3_file(path: &Path) -> String {
        format!("blake3:{}", blake3::hash(&fs::read(path).unwrap()).to_hex())
    }

    fn read_json(path: impl AsRef<Path>) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }
}
