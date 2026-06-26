#![forbid(unsafe_code)]

//! Disk persistence and reload validation for entity index artifacts.
//!
//! The persisted bundle is an accelerator cache. It records enough metadata to
//! reject stale cache hits before a later block stage can consume postings.

use crate::{
    Refusal,
    entity::{
        cache::EntityCacheKey,
        error::EntityRefusalKind,
        index::ngram_index::EntityNgramIndex,
        index::{
            EntityIndexArtifact, EntityIndexCachePolicy, validate_index_artifact_contract,
            validate_index_cache_policy,
        },
        postings::{EntityPostingIndex, PostingLayoutError},
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

pub const CANON_ENTITY_INDEX_DISK_BUNDLE_VERSION: &str = "canon_entity_index_disk_bundle.v0";
pub const CANON_ENTITY_INDEX_DIAGNOSTIC_VERSION: &str = "canon_entity_index_diagnostic.v0";
pub const INDEX_ARTIFACT_FILE: &str = "index.json";
pub const INDEX_CACHE_KEY_FILE: &str = "index/cache_key.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIndexPostingsBundle {
    pub version: String,
    pub posting_index: EntityPostingIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ngram_index: Option<EntityNgramIndex>,
}

impl EntityIndexPostingsBundle {
    pub fn new(posting_index: EntityPostingIndex, ngram_index: Option<EntityNgramIndex>) -> Self {
        Self {
            version: CANON_ENTITY_INDEX_DISK_BUNDLE_VERSION.to_string(),
            posting_index,
            ngram_index,
        }
    }

    pub fn validate_reload(&self) -> Result<(), Refusal> {
        if self.version != CANON_ENTITY_INDEX_DISK_BUNDLE_VERSION {
            return Err(artifact_contract_refusal(
                "Entity index postings bundle version mismatch",
                json!({
                    "stage": "index",
                    "field": "postings.version",
                    "expected": CANON_ENTITY_INDEX_DISK_BUNDLE_VERSION,
                    "actual": self.version,
                    "writes_performed": false
                }),
            ));
        }

        self.posting_index
            .exact_view_layout
            .validate_reload()
            .map_err(posting_layout_refusal)?;
        self.posting_index
            .token_layout
            .validate_reload()
            .map_err(posting_layout_refusal)?;
        self.posting_index
            .tfidf_layout
            .validate_reload()
            .map_err(posting_layout_refusal)?;

        if let Some(ngram_index) = &self.ngram_index {
            ngram_index
                .ngram_layout
                .validate_reload()
                .map_err(posting_layout_refusal)?;
            if ngram_index.surface_ids != self.posting_index.surface_ids {
                return Err(artifact_contract_refusal(
                    "Entity index postings bundle has mismatched surface order",
                    json!({
                        "stage": "index",
                        "field": "surface_ids",
                        "posting_surface_count": self.posting_index.surface_ids.len(),
                        "ngram_surface_count": ngram_index.surface_ids.len(),
                        "writes_performed": false
                    }),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIndexDiagnosticRecord {
    pub version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counts: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl EntityIndexDiagnosticRecord {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            version: CANON_ENTITY_INDEX_DIAGNOSTIC_VERSION.to_string(),
            kind: kind.into(),
            counts: BTreeMap::new(),
            labels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexPersistRequest {
    pub artifact: EntityIndexArtifact,
    pub cache_key: EntityCacheKey,
    pub postings: EntityIndexPostingsBundle,
    pub diagnostics: Vec<EntityIndexDiagnosticRecord>,
    pub max_artifact_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexDiskBundle {
    pub artifact: EntityIndexArtifact,
    pub cache_key: EntityCacheKey,
    pub postings: EntityIndexPostingsBundle,
    pub diagnostics: Vec<EntityIndexDiagnosticRecord>,
    pub paths: EntityIndexDiskPaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexDiskPaths {
    pub artifact_path: PathBuf,
    pub cache_key_path: PathBuf,
    pub postings_path: PathBuf,
    pub diagnostics_path: PathBuf,
}

pub fn write_index_disk_bundle(
    work_dir: impl AsRef<Path>,
    request: EntityIndexPersistRequest,
) -> Result<EntityIndexDiskPaths, Refusal> {
    validate_index_artifact_contract(&request.artifact)?;
    request.postings.validate_reload()?;
    validate_diagnostics(&request.diagnostics)?;

    let paths = disk_paths(work_dir.as_ref(), &request.artifact)?;
    let artifact_bytes = to_json_bytes(&request.artifact, "index artifact")?;
    let cache_key_bytes = to_json_bytes(&request.cache_key, "index cache key")?;
    let postings_bytes = to_json_bytes(&request.postings, "index postings")?;
    let diagnostics_bytes = to_jsonl_bytes(&request.diagnostics)?;

    enforce_write_budget(
        request.max_artifact_bytes,
        [
            artifact_bytes.len(),
            cache_key_bytes.len(),
            postings_bytes.len(),
            diagnostics_bytes.len(),
        ],
    )?;

    write_bytes(&paths.artifact_path, &artifact_bytes)?;
    write_bytes(&paths.cache_key_path, &cache_key_bytes)?;
    write_bytes(&paths.postings_path, &postings_bytes)?;
    write_bytes(&paths.diagnostics_path, &diagnostics_bytes)?;

    Ok(paths)
}

pub fn read_index_disk_bundle(
    work_dir: impl AsRef<Path>,
    expected_artifact: &EntityIndexArtifact,
    current_cache_key: &EntityCacheKey,
    max_artifact_bytes: Option<u64>,
) -> Result<EntityIndexDiskBundle, Refusal> {
    let paths = disk_paths(work_dir.as_ref(), expected_artifact)?;
    enforce_read_budget(
        max_artifact_bytes,
        [
            &paths.artifact_path,
            &paths.cache_key_path,
            &paths.postings_path,
            &paths.diagnostics_path,
        ],
    )?;

    let artifact: EntityIndexArtifact = read_json_file(&paths.artifact_path, "index artifact")?;
    validate_index_artifact_contract(&artifact)?;
    if artifact != *expected_artifact {
        return Err(artifact_contract_refusal(
            "Entity index artifact does not match expected artifact contract",
            json!({
                "stage": "index",
                "field": "artifact_content_hash",
                "expected": expected_artifact.artifact_content_hash,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
        ));
    }

    let cache_key: EntityCacheKey = read_json_file(&paths.cache_key_path, "index cache key")?;
    validate_index_cache_policy(
        &cache_key,
        current_cache_key,
        EntityIndexCachePolicy::RefuseOnMiss,
    )?;

    let postings: EntityIndexPostingsBundle =
        read_json_file(&paths.postings_path, "index postings")?;
    postings.validate_reload()?;
    let diagnostics = read_diagnostics_jsonl(&paths.diagnostics_path)?;
    validate_diagnostics(&diagnostics)?;

    Ok(EntityIndexDiskBundle {
        artifact,
        cache_key,
        postings,
        diagnostics,
        paths,
    })
}

fn disk_paths(
    work_dir: &Path,
    artifact: &EntityIndexArtifact,
) -> Result<EntityIndexDiskPaths, Refusal> {
    Ok(EntityIndexDiskPaths {
        artifact_path: work_dir.join(INDEX_ARTIFACT_FILE),
        cache_key_path: resolve_relative_path(work_dir, INDEX_CACHE_KEY_FILE, "cache_key_path")?,
        postings_path: resolve_relative_path(work_dir, &artifact.postings_path, "postings_path")?,
        diagnostics_path: resolve_relative_path(
            work_dir,
            &artifact.diagnostics_path,
            "diagnostics_path",
        )?,
    })
}

fn resolve_relative_path(work_dir: &Path, relative: &str, field: &str) -> Result<PathBuf, Refusal> {
    let path = Path::new(relative);
    if relative.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(artifact_contract_refusal(
            "Entity index artifact path must be a safe relative path",
            json!({
                "stage": "index",
                "field": field,
                "path": relative,
                "writes_performed": false
            }),
        ));
    }
    Ok(work_dir.join(path))
}

fn validate_diagnostics(records: &[EntityIndexDiagnosticRecord]) -> Result<(), Refusal> {
    for (ordinal, record) in records.iter().enumerate() {
        if record.version != CANON_ENTITY_INDEX_DIAGNOSTIC_VERSION {
            return Err(artifact_contract_refusal(
                "Entity index diagnostic record version mismatch",
                json!({
                    "stage": "index",
                    "field": "diagnostics.version",
                    "record_ordinal": ordinal,
                    "expected": CANON_ENTITY_INDEX_DIAGNOSTIC_VERSION,
                    "actual": record.version,
                    "writes_performed": false
                }),
            ));
        }
        if record.kind.trim().is_empty() {
            return Err(artifact_contract_refusal(
                "Entity index diagnostic record kind is empty",
                json!({
                    "stage": "index",
                    "field": "diagnostics.kind",
                    "record_ordinal": ordinal,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn to_json_bytes<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec_pretty(value).map_err(|error| {
        artifact_contract_refusal(
            "Failed to serialize entity index artifact",
            json!({
                "stage": "index",
                "artifact": label,
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn to_jsonl_bytes(records: &[EntityIndexDiagnosticRecord]) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for record in records {
        let mut line = serde_json::to_vec(record).map_err(|error| {
            artifact_contract_refusal(
                "Failed to serialize entity index diagnostics",
                json!({
                    "stage": "index",
                    "artifact": "index diagnostics",
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
        bytes.append(&mut line);
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn read_json_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, Refusal> {
    let bytes = fs::read(path).map_err(|error| read_io_refusal(path, label, error))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        artifact_contract_refusal(
            "Failed to parse entity index JSON artifact",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })
}

fn read_diagnostics_jsonl(path: &Path) -> Result<Vec<EntityIndexDiagnosticRecord>, Refusal> {
    let bytes =
        fs::read(path).map_err(|error| read_io_refusal(path, "index diagnostics", error))?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        artifact_contract_refusal(
            "Entity index diagnostics are not UTF-8 JSONL",
            json!({
                "stage": "index",
                "artifact": "index diagnostics",
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;

    let mut records = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        records.push(serde_json::from_str(line).map_err(|error| {
            artifact_contract_refusal(
                "Failed to parse entity index diagnostic JSONL record",
                json!({
                    "stage": "index",
                    "artifact": "index diagnostics",
                    "path": path.display().to_string(),
                    "line": line_index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?);
    }
    Ok(records)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| write_io_refusal(path, error))?;
    }
    fs::write(path, bytes).map_err(|error| write_io_refusal(path, error))
}

fn enforce_write_budget<I>(max_artifact_bytes: Option<u64>, byte_lengths: I) -> Result<(), Refusal>
where
    I: IntoIterator<Item = usize>,
{
    let Some(max_artifact_bytes) = max_artifact_bytes else {
        return Ok(());
    };
    let observed = byte_lengths.into_iter().fold(0_u64, |total, length| {
        total.saturating_add(u64::try_from(length).unwrap_or(u64::MAX))
    });
    if observed > max_artifact_bytes {
        return Err(io_budget_refusal(
            "max_artifact_bytes",
            observed,
            max_artifact_bytes,
        ));
    }
    Ok(())
}

fn enforce_read_budget<'a, I>(max_artifact_bytes: Option<u64>, paths: I) -> Result<(), Refusal>
where
    I: IntoIterator<Item = &'a PathBuf>,
{
    let Some(max_artifact_bytes) = max_artifact_bytes else {
        return Ok(());
    };
    let mut observed = 0_u64;
    for path in paths {
        let metadata =
            fs::metadata(path).map_err(|error| read_io_refusal(path, "index artifact", error))?;
        observed = observed.saturating_add(metadata.len());
    }
    if observed > max_artifact_bytes {
        return Err(io_budget_refusal(
            "max_artifact_bytes",
            observed,
            max_artifact_bytes,
        ));
    }
    Ok(())
}

fn io_budget_refusal(limit: &str, observed: u64, configured: u64) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        "Entity index IO budget exceeded before artifact emission",
        json!({
            "stage": "index",
            "limit": limit,
            "observed": observed,
            "configured": configured,
            "writes_performed": false
        }),
        Some(
            "Increase the entity index IO budget or rebuild in smaller physical batches"
                .to_string(),
        ),
    )
}

fn posting_layout_refusal(error: PostingLayoutError) -> Refusal {
    artifact_contract_refusal(
        "Entity index postings failed reload validation",
        json!({
            "stage": "index",
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn read_io_refusal(path: &Path, label: &str, error: std::io::Error) -> Refusal {
    artifact_contract_refusal(
        "Failed to read entity index artifact",
        json!({
            "stage": "index",
            "artifact": label,
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
    )
}

fn write_io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    EntityRefusalKind::IoBudget.to_refusal(
        "Failed to write entity index artifact",
        json!({
            "stage": "index",
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false
        }),
        Some("Use a writable work directory or reduce the entity index artifact size".to_string()),
    )
}

fn artifact_contract_refusal(message: &str, detail: serde_json::Value) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        message,
        detail,
        Some("Reload the matching index artifact or rebuild canon entity index".to_string()),
    )
}
