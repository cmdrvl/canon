use super::provider::{
    ProviderConfig, RegistryMaterializedEntry, available_sources, provider_for_source,
};
use crate::{
    RegistryBuildFailure, RegistryBuildOutput, RegistryBuildSpecialReason, RegistryBuildSummary,
    RegistryBuildUnresolvedEntry, RegistryMeta,
};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct RegistryBuildRequest {
    pub source: String,
    pub seed_path: PathBuf,
    pub seed_column: String,
    pub output_dir: PathBuf,
    pub version: String,
    pub incremental: bool,
    pub identifiers: Vec<String>,
    pub seed_hash: String,
    pub special_reasons: Vec<(String, usize)>,
    pub batch_size: Option<usize>,
    pub rate_limit_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryBuildErrorKind {
    Io,
    BadRegistry,
    Parse,
}

#[derive(Debug, Clone)]
pub struct RegistryBuildError {
    pub kind: RegistryBuildErrorKind,
    pub message: String,
    pub detail: serde_json::Value,
}

impl RegistryBuildError {
    fn io(message: impl Into<String>, detail: serde_json::Value) -> Self {
        Self {
            kind: RegistryBuildErrorKind::Io,
            message: message.into(),
            detail,
        }
    }

    fn bad_registry(message: impl Into<String>, detail: serde_json::Value) -> Self {
        Self {
            kind: RegistryBuildErrorKind::BadRegistry,
            message: message.into(),
            detail,
        }
    }

    fn parse(message: impl Into<String>, detail: serde_json::Value) -> Self {
        Self {
            kind: RegistryBuildErrorKind::Parse,
            message: message.into(),
            detail,
        }
    }
}

pub fn build_registry(
    request: &RegistryBuildRequest,
) -> Result<RegistryBuildOutput, RegistryBuildError> {
    let provider = provider_for_source(&request.source).ok_or_else(|| {
        RegistryBuildError::parse(
            format!("Unknown registry build source '{}'", request.source),
            json!({
                "source": request.source,
                "available_sources": available_sources(),
            }),
        )
    })?;

    if request.output_dir.exists() && !request.output_dir.is_dir() {
        return Err(RegistryBuildError::io(
            format!(
                "Output path '{}' exists and is not a directory",
                request.output_dir.display()
            ),
            json!({ "output": request.output_dir.display().to_string() }),
        ));
    }

    let expected_registry_id = provider.registry_id(&request.seed_column);
    let output_has_entries = dir_has_entries(&request.output_dir)?;
    let mut existing_files = BTreeMap::new();
    let mut carried_forward_inputs = BTreeSet::new();

    if request.incremental {
        if request.output_dir.join("registry.json").exists() {
            let (_, registry_meta, mapping_files) =
                super::load_registry_definition(&request.output_dir).map_err(|error| {
                    RegistryBuildError::bad_registry(
                        format!(
                            "Cannot incrementally build into '{}': {}",
                            request.output_dir.display(),
                            error
                        ),
                        json!({ "output": request.output_dir.display().to_string() }),
                    )
                })?;

            if registry_meta.id != expected_registry_id {
                return Err(RegistryBuildError::bad_registry(
                    format!(
                        "Existing registry id '{}' does not match provider output '{}'",
                        registry_meta.id, expected_registry_id
                    ),
                    json!({
                        "output": request.output_dir.display().to_string(),
                        "existing_registry_id": registry_meta.id,
                        "expected_registry_id": expected_registry_id,
                    }),
                ));
            }

            existing_files = mapping_files_to_entries(&mapping_files);
            carried_forward_inputs = super::effective_entries(&mapping_files)
                .into_iter()
                .map(|entry| entry.input)
                .collect();
        } else if output_has_entries {
            return Err(RegistryBuildError::bad_registry(
                format!(
                    "Incremental build output '{}' must already be a valid registry directory",
                    request.output_dir.display()
                ),
                json!({ "output": request.output_dir.display().to_string() }),
            ));
        }
    } else if output_has_entries {
        return Err(RegistryBuildError::io(
            format!(
                "Output directory '{}' already exists and is not empty; refuse to overwrite in place",
                request.output_dir.display()
            ),
            json!({ "output": request.output_dir.display().to_string() }),
        ));
    }

    let batch_size = request
        .batch_size
        .unwrap_or_else(|| provider.default_batch_size());
    if batch_size == 0 {
        return Err(RegistryBuildError::parse(
            "--batch-size must be greater than zero".to_string(),
            json!({ "batch_size": batch_size }),
        ));
    }

    let config = ProviderConfig {
        seed_column: request.seed_column.clone(),
        version: request.version.clone(),
        batch_size,
        rate_limit_ms: request.rate_limit_ms,
    };
    let rate_limit_ms = provider.rate_limit(&config).map(|limit| limit.delay_ms);

    let identifiers_to_fetch = request
        .identifiers
        .iter()
        .filter(|identifier| !carried_forward_inputs.contains(*identifier))
        .cloned()
        .collect::<Vec<_>>();

    let mut new_files = BTreeMap::new();
    let mut unresolved = BTreeSet::new();
    let mut failures = Vec::new();
    let mut api_calls = 0usize;
    let batch_count = if identifiers_to_fetch.is_empty() {
        0
    } else {
        identifiers_to_fetch.len().div_ceil(batch_size)
    };

    for (batch_index, batch) in identifiers_to_fetch.chunks(batch_size).enumerate() {
        let result = provider.fetch(batch, &config).map_err(|error| {
            RegistryBuildError::io(
                format!("Provider '{}' fetch failed: {}", provider.name(), error),
                json!({
                    "source": request.source,
                    "batch_size": batch.len(),
                }),
            )
        })?;

        for (file_name, entries) in result.files {
            new_files
                .entry(file_name)
                .or_insert_with(Vec::new)
                .extend(entries);
        }
        unresolved.extend(result.unresolved);
        failures.extend(result.failures);
        api_calls += result.api_calls;

        if batch_index + 1 < batch_count
            && let Some(delay_ms) = rate_limit_ms
        {
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    let mut final_files = if request.incremental {
        existing_files
    } else {
        BTreeMap::new()
    };
    for (file_name, mut entries) in new_files {
        final_files
            .entry(file_name)
            .or_insert_with(Vec::new)
            .append(&mut entries);
    }

    for entries in final_files.values_mut() {
        sort_entries(entries);
    }

    fs::create_dir_all(&request.output_dir).map_err(|error| {
        RegistryBuildError::io(
            format!(
                "Failed to create output directory '{}': {}",
                request.output_dir.display(),
                error
            ),
            json!({ "output": request.output_dir.display().to_string() }),
        )
    })?;

    let mapping_files = final_files_to_mapping_files(&final_files);
    let effective_inputs = super::effective_entries(&mapping_files)
        .into_iter()
        .map(|entry| entry.input)
        .collect::<BTreeSet<_>>();
    let failed_inputs = failures
        .iter()
        .map(|failure| failure.input.clone())
        .collect::<BTreeSet<_>>();
    let unresolved_entries = unresolved
        .into_iter()
        .filter(|identifier| {
            !effective_inputs.contains(identifier) && !failed_inputs.contains(identifier)
        })
        .map(|input| RegistryBuildUnresolvedEntry {
            input,
            reason: "provider_no_match".to_string(),
        })
        .collect::<Vec<_>>();
    let failure_entries = failures
        .into_iter()
        .map(|failure| RegistryBuildFailure {
            input: failure.input,
            message: failure.message,
        })
        .collect::<Vec<_>>();
    let carried_forward_count = if request.incremental {
        request
            .identifiers
            .iter()
            .filter(|identifier| carried_forward_inputs.contains(*identifier))
            .count()
    } else {
        0
    };
    let resolved_count = request
        .identifiers
        .iter()
        .filter(|identifier| effective_inputs.contains(*identifier))
        .count();
    let special_reasons = request
        .special_reasons
        .iter()
        .map(|(reason, count)| RegistryBuildSpecialReason {
            reason: reason.clone(),
            count: *count,
        })
        .collect::<Vec<_>>();
    let skipped_special_reason_rows = special_reasons.iter().map(|reason| reason.count).sum();
    let registry_files = final_files.keys().cloned().collect::<Vec<_>>();
    let materialized_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let updated = Utc::now().format("%Y-%m-%d").to_string();
    let registry_description = provider.description(&request.seed_column);
    let entry_count = final_files.values().map(Vec::len).sum::<usize>();

    for (file_name, entries) in &final_files {
        let path = request.output_dir.join(file_name);
        let mapping_entries = entries
            .iter()
            .map(|entry| super::MappingEntry {
                input: entry.input.clone(),
                canonical_id: entry.canonical_id.clone(),
                canonical_type: entry.canonical_type.clone(),
                rule_id: entry.rule_id.clone(),
            })
            .collect::<Vec<_>>();
        let content = serde_json::to_string_pretty(&mapping_entries).map_err(|error| {
            RegistryBuildError::io(
                format!(
                    "Failed to serialize mapping file '{}': {}",
                    file_name, error
                ),
                json!({ "file": file_name }),
            )
        })?;
        fs::write(&path, content).map_err(|error| {
            RegistryBuildError::io(
                format!(
                    "Failed to write mapping file '{}': {}",
                    path.display(),
                    error
                ),
                json!({ "file": path.display().to_string() }),
            )
        })?;
    }

    let registry_json = json!({
        "id": expected_registry_id,
        "version": request.version,
        "description": registry_description,
        "updated": updated,
        "entry_count": entry_count,
        "source": request.source,
        "materialized_at": materialized_at,
        "seed_count": request.identifiers.len(),
        "resolved_count": resolved_count,
    });
    fs::write(
        request.output_dir.join("registry.json"),
        serde_json::to_string_pretty(&registry_json).map_err(|error| {
            RegistryBuildError::io(
                format!("Failed to serialize registry.json: {}", error),
                json!({ "output": request.output_dir.display().to_string() }),
            )
        })?,
    )
    .map_err(|error| {
        RegistryBuildError::io(
            format!(
                "Failed to write registry.json in '{}': {}",
                request.output_dir.display(),
                error
            ),
            json!({ "output": request.output_dir.display().to_string() }),
        )
    })?;

    let build_json = json!({
        "version": "canon_registry_build.v0",
        "source": request.source,
        "seed": {
            "path": request.seed_path.display().to_string(),
            "column": request.seed_column,
            "hash": request.seed_hash,
            "count": request.identifiers.len(),
            "special_reasons": special_reasons,
        },
        "registry": {
            "id": provider.registry_id(&request.seed_column),
            "version": request.version,
            "output": request.output_dir.display().to_string(),
        },
        "summary": {
            "seed_count": request.identifiers.len(),
            "queried_count": identifiers_to_fetch.len(),
            "carried_forward_count": carried_forward_count,
            "resolved_count": resolved_count,
            "unresolved_count": unresolved_entries.len(),
            "failure_count": failure_entries.len(),
            "skipped_special_reason_rows": skipped_special_reason_rows,
            "mapping_files": registry_files.len(),
            "api_calls": api_calls,
        },
        "incremental": request.incremental,
        "provider": {
            "name": provider.name(),
            "id_types": provider.id_types(),
            "batch_size": batch_size,
            "rate_limit_ms": rate_limit_ms,
        },
        "files": registry_files,
        "unresolved": unresolved_entries,
        "failures": failure_entries,
    });
    fs::write(
        request.output_dir.join("_build.json"),
        serde_json::to_string_pretty(&build_json).map_err(|error| {
            RegistryBuildError::io(
                format!("Failed to serialize _build.json: {}", error),
                json!({ "output": request.output_dir.display().to_string() }),
            )
        })?,
    )
    .map_err(|error| {
        RegistryBuildError::io(
            format!(
                "Failed to write _build.json in '{}': {}",
                request.output_dir.display(),
                error
            ),
            json!({ "output": request.output_dir.display().to_string() }),
        )
    })?;

    Ok(RegistryBuildOutput {
        version: "canon_registry_build.v0".to_string(),
        source: request.source.clone(),
        registry: RegistryMeta {
            id: provider.registry_id(&request.seed_column),
            version: request.version.clone(),
            source: request.output_dir.display().to_string(),
        },
        output_path: request.output_dir.display().to_string(),
        summary: RegistryBuildSummary {
            seed_count: request.identifiers.len(),
            queried_count: identifiers_to_fetch.len(),
            carried_forward_count,
            resolved_count,
            unresolved_count: unresolved_entries.len(),
            failure_count: failure_entries.len(),
            skipped_special_reason_rows,
            mapping_files: registry_files.len(),
            api_calls,
        },
        files: registry_files,
        unresolved: unresolved_entries,
        failures: failure_entries,
        special_reasons,
        incremental: request.incremental,
    })
}

fn dir_has_entries(path: &Path) -> Result<bool, RegistryBuildError> {
    if !path.exists() {
        return Ok(false);
    }

    let mut entries = fs::read_dir(path).map_err(|error| {
        RegistryBuildError::io(
            format!(
                "Failed to read output directory '{}': {}",
                path.display(),
                error
            ),
            json!({ "output": path.display().to_string() }),
        )
    })?;

    Ok(entries.next().is_some())
}

fn mapping_files_to_entries(
    mapping_files: &[super::MappingFile],
) -> BTreeMap<String, Vec<RegistryMaterializedEntry>> {
    let mut files = BTreeMap::new();

    for mapping_file in mapping_files {
        let file_name = mapping_file
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown.json")
            .to_string();
        let entries = mapping_file
            .entries
            .iter()
            .map(|entry| RegistryMaterializedEntry {
                input: entry.input.clone(),
                canonical_id: entry.canonical_id.clone(),
                canonical_type: entry.canonical_type.clone(),
                rule_id: entry.rule_id.clone(),
            })
            .collect::<Vec<_>>();
        files.insert(file_name, entries);
    }

    files
}

fn final_files_to_mapping_files(
    files: &BTreeMap<String, Vec<RegistryMaterializedEntry>>,
) -> Vec<super::MappingFile> {
    files
        .iter()
        .map(|(file_name, entries)| super::MappingFile {
            path: PathBuf::from(file_name),
            entries: entries
                .iter()
                .map(|entry| super::MappingEntry {
                    input: entry.input.clone(),
                    canonical_id: entry.canonical_id.clone(),
                    canonical_type: entry.canonical_type.clone(),
                    rule_id: entry.rule_id.clone(),
                })
                .collect(),
        })
        .collect()
}

fn sort_entries(entries: &mut [RegistryMaterializedEntry]) {
    entries.sort_by(|left, right| {
        left.input
            .cmp(&right.input)
            .then_with(|| left.canonical_type.cmp(&right.canonical_type))
            .then_with(|| left.canonical_id.cmp(&right.canonical_id))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
    });
}
