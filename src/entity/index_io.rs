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
            EntityIndexArtifact, EntityIndexCacheMode, EntityIndexCachePolicy,
            EntityIndexCacheStatus, validate_index_artifact_contract, validate_index_cache_policy,
        },
        postings::{EntityPostingIndex, PostingLayoutError},
    },
    witness,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

pub const CANON_ENTITY_INDEX_DISK_BUNDLE_VERSION: &str = "canon_entity_index_disk_bundle.v0";
pub const CANON_ENTITY_INDEX_DIAGNOSTIC_VERSION: &str = "canon_entity_index_diagnostic.v0";
pub const CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION: &str = "canon_entity_index_cache_receipt.v0";
pub const INDEX_ARTIFACT_FILE: &str = "index.json";
pub const INDEX_CACHE_KEY_FILE: &str = "index/cache_key.json";
pub const INDEX_CACHE_RECEIPT_FILE: &str = "index/cache_receipt.json";

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
    pub receipt: EntityIndexCacheReceiptRef,
    pub paths: EntityIndexDiskPaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexDiskPaths {
    pub artifact_path: PathBuf,
    pub cache_key_path: PathBuf,
    pub receipt_path: PathBuf,
    pub postings_path: PathBuf,
    pub diagnostics_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityIndexCacheReceipt {
    pub version: String,
    pub mode: EntityIndexCacheMode,
    pub status: EntityIndexCacheStatus,
    pub reusable: bool,
    pub bundle_hash: String,
    pub files: Vec<EntityIndexCacheReceiptFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityIndexCacheReceiptFile {
    pub role: String,
    pub path: String,
    pub content_hash: String,
    pub byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexCacheReceiptRef {
    pub receipt: EntityIndexCacheReceipt,
    pub path: PathBuf,
    pub content_hash: String,
}

pub fn write_index_disk_bundle(
    work_dir: impl AsRef<Path>,
    request: EntityIndexPersistRequest,
) -> Result<EntityIndexDiskPaths, Refusal> {
    write_index_disk_bundle_with_cache_receipt(
        work_dir,
        request,
        EntityIndexCacheMode::Enabled,
        EntityIndexCacheStatus::Rebuilt,
        true,
    )
}

pub fn write_index_disk_bundle_with_cache_receipt(
    work_dir: impl AsRef<Path>,
    request: EntityIndexPersistRequest,
    mode: EntityIndexCacheMode,
    status: EntityIndexCacheStatus,
    reusable: bool,
) -> Result<EntityIndexDiskPaths, Refusal> {
    validate_index_artifact_contract(&request.artifact)?;
    request.postings.validate_reload()?;
    validate_diagnostics(&request.diagnostics)?;

    let paths = disk_paths(work_dir.as_ref(), &request.artifact)?;
    reject_symlinked_bundle_components(work_dir.as_ref(), &paths)?;
    let artifact_bytes = to_json_bytes(&request.artifact, "index artifact")?;
    let cache_key_bytes = to_json_bytes(&request.cache_key, "index cache key")?;
    let postings_bytes = to_json_bytes(&request.postings, "index postings")?;
    let diagnostics_bytes = to_jsonl_bytes(&request.diagnostics)?;
    let receipt = cache_receipt_from_bytes(
        &request.artifact,
        &paths,
        mode,
        status,
        reusable,
        &artifact_bytes,
        &cache_key_bytes,
        &postings_bytes,
        &diagnostics_bytes,
    )?;
    let receipt_bytes = to_json_bytes(&receipt, "index cache receipt")?;

    enforce_write_budget(
        request.max_artifact_bytes,
        [
            artifact_bytes.len(),
            cache_key_bytes.len(),
            postings_bytes.len(),
            diagnostics_bytes.len(),
            receipt_bytes.len(),
        ],
    )?;

    write_bytes(&paths.artifact_path, &artifact_bytes)?;
    write_bytes(&paths.cache_key_path, &cache_key_bytes)?;
    write_bytes(&paths.postings_path, &postings_bytes)?;
    write_bytes(&paths.diagnostics_path, &diagnostics_bytes)?;
    write_bytes(&paths.receipt_path, &receipt_bytes)?;

    Ok(paths)
}

pub fn read_index_disk_bundle(
    work_dir: impl AsRef<Path>,
    expected_artifact: &EntityIndexArtifact,
    current_cache_key: &EntityCacheKey,
    max_artifact_bytes: Option<u64>,
) -> Result<EntityIndexDiskBundle, Refusal> {
    let paths = disk_paths(work_dir.as_ref(), expected_artifact)?;
    reject_symlinked_bundle_components(work_dir.as_ref(), &paths)?;
    enforce_read_budget(max_artifact_bytes, cache_bundle_paths(&paths))?;

    let receipt =
        read_index_cache_receipt(work_dir.as_ref(), expected_artifact, max_artifact_bytes)?;
    if receipt.receipt.mode != EntityIndexCacheMode::Enabled
        || !receipt.receipt.reusable
        || matches!(
            receipt.receipt.status,
            EntityIndexCacheStatus::Miss | EntityIndexCacheStatus::Bypassed
        )
    {
        return Err(artifact_contract_refusal(
            "Entity index cache receipt is not reusable for enabled cache hit",
            json!({
                "stage": "index",
                "mode": receipt.receipt.mode.as_str(),
                "status": receipt.receipt.status.as_str(),
                "reusable": receipt.receipt.reusable,
                "writes_performed": false
            }),
        ));
    }

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
        receipt,
        paths,
    })
}

pub(crate) fn index_cache_file_exists(
    work_dir: &Path,
    path: &Path,
    label: &str,
) -> Result<bool, Refusal> {
    reject_symlinked_path_components(work_dir, path, label)?;
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(read_io_refusal(path, label, error)),
    }
}

pub(crate) fn read_index_artifact_for_cache(
    work_dir: impl AsRef<Path>,
    max_artifact_bytes: Option<u64>,
) -> Result<EntityIndexArtifact, Refusal> {
    let work_dir = work_dir.as_ref();
    let artifact_path = work_dir.join(INDEX_ARTIFACT_FILE);
    reject_symlinked_path_components(work_dir, &artifact_path, "index artifact")?;
    enforce_read_budget(max_artifact_bytes, [&artifact_path])?;
    let bytes = read_file_bytes_reject_symlink(&artifact_path, "index artifact")?;
    let artifact: EntityIndexArtifact = serde_json::from_slice(&bytes).map_err(|error| {
        artifact_contract_refusal(
            "Failed to parse entity index cache artifact",
            json!({
                "stage": "index",
                "artifact": "index artifact",
                "path": artifact_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
        )
    })?;
    validate_index_artifact_contract(&artifact)?;
    Ok(artifact)
}

pub fn read_index_cache_receipt(
    work_dir: impl AsRef<Path>,
    artifact: &EntityIndexArtifact,
    max_artifact_bytes: Option<u64>,
) -> Result<EntityIndexCacheReceiptRef, Refusal> {
    let paths = disk_paths(work_dir.as_ref(), artifact)?;
    reject_symlinked_bundle_components(work_dir.as_ref(), &paths)?;
    enforce_read_budget(max_artifact_bytes, cache_bundle_paths(&paths))?;
    let receipt_bytes = read_file_bytes_reject_symlink(&paths.receipt_path, "index cache receipt")?;
    let receipt: EntityIndexCacheReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(|error| {
            artifact_contract_refusal(
                "Failed to parse entity index cache receipt",
                json!({
                    "stage": "index",
                    "artifact": "index cache receipt",
                    "path": paths.receipt_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
            )
        })?;
    validate_index_cache_receipt(&paths, artifact, &receipt)?;
    Ok(EntityIndexCacheReceiptRef {
        receipt,
        path: paths.receipt_path,
        content_hash: witness::hash_bytes(&receipt_bytes),
    })
}

pub fn write_index_cache_receipt(
    work_dir: impl AsRef<Path>,
    artifact: &EntityIndexArtifact,
    mode: EntityIndexCacheMode,
    status: EntityIndexCacheStatus,
    reusable: bool,
    max_artifact_bytes: Option<u64>,
) -> Result<EntityIndexCacheReceiptRef, Refusal> {
    let paths = disk_paths(work_dir.as_ref(), artifact)?;
    reject_symlinked_bundle_components(work_dir.as_ref(), &paths)?;
    let artifact_bytes = read_file_bytes_reject_symlink(&paths.artifact_path, "index artifact")?;
    let cache_key_bytes = read_file_bytes_reject_symlink(&paths.cache_key_path, "index cache key")?;
    let postings_bytes = read_file_bytes_reject_symlink(&paths.postings_path, "index postings")?;
    let diagnostics_bytes =
        read_file_bytes_reject_symlink(&paths.diagnostics_path, "index diagnostics")?;
    let receipt = cache_receipt_from_bytes(
        artifact,
        &paths,
        mode,
        status,
        reusable,
        &artifact_bytes,
        &cache_key_bytes,
        &postings_bytes,
        &diagnostics_bytes,
    )?;
    let receipt_bytes = to_json_bytes(&receipt, "index cache receipt")?;
    enforce_write_budget(
        max_artifact_bytes,
        [
            artifact_bytes.len(),
            cache_key_bytes.len(),
            postings_bytes.len(),
            diagnostics_bytes.len(),
            receipt_bytes.len(),
        ],
    )?;
    write_bytes(&paths.receipt_path, &receipt_bytes)?;
    Ok(EntityIndexCacheReceiptRef {
        receipt,
        path: paths.receipt_path,
        content_hash: witness::hash_bytes(&receipt_bytes),
    })
}

fn disk_paths(
    work_dir: &Path,
    artifact: &EntityIndexArtifact,
) -> Result<EntityIndexDiskPaths, Refusal> {
    Ok(EntityIndexDiskPaths {
        artifact_path: work_dir.join(INDEX_ARTIFACT_FILE),
        cache_key_path: resolve_relative_path(work_dir, INDEX_CACHE_KEY_FILE, "cache_key_path")?,
        receipt_path: resolve_relative_path(
            work_dir,
            INDEX_CACHE_RECEIPT_FILE,
            "cache_receipt_path",
        )?,
        postings_path: resolve_relative_path(work_dir, &artifact.postings_path, "postings_path")?,
        diagnostics_path: resolve_relative_path(
            work_dir,
            &artifact.diagnostics_path,
            "diagnostics_path",
        )?,
    })
}

fn cache_bundle_paths(paths: &EntityIndexDiskPaths) -> [&PathBuf; 5] {
    [
        &paths.artifact_path,
        &paths.cache_key_path,
        &paths.postings_path,
        &paths.diagnostics_path,
        &paths.receipt_path,
    ]
}

#[allow(clippy::too_many_arguments)]
fn cache_receipt_from_bytes(
    artifact: &EntityIndexArtifact,
    paths: &EntityIndexDiskPaths,
    mode: EntityIndexCacheMode,
    status: EntityIndexCacheStatus,
    reusable: bool,
    artifact_bytes: &[u8],
    cache_key_bytes: &[u8],
    postings_bytes: &[u8],
    diagnostics_bytes: &[u8],
) -> Result<EntityIndexCacheReceipt, Refusal> {
    validate_receipt_paths_do_not_collide(paths)?;
    validate_cache_receipt_mode_status(mode, status, reusable)?;
    let file_inputs = [
        (
            "artifact",
            INDEX_ARTIFACT_FILE,
            paths.artifact_path.as_path(),
            artifact_bytes,
        ),
        (
            "cache_key",
            INDEX_CACHE_KEY_FILE,
            paths.cache_key_path.as_path(),
            cache_key_bytes,
        ),
        (
            "postings",
            artifact.postings_path.as_str(),
            paths.postings_path.as_path(),
            postings_bytes,
        ),
        (
            "diagnostics",
            artifact.diagnostics_path.as_str(),
            paths.diagnostics_path.as_path(),
            diagnostics_bytes,
        ),
    ];
    let files = file_inputs
        .iter()
        .map(
            |(role, relative_path, _absolute_path, bytes)| EntityIndexCacheReceiptFile {
                role: (*role).to_string(),
                path: (*relative_path).to_string(),
                content_hash: witness::hash_bytes(bytes),
                byte_count: bytes.len() as u64,
            },
        )
        .collect::<Vec<_>>();
    Ok(EntityIndexCacheReceipt {
        version: CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION.to_string(),
        mode,
        status,
        reusable,
        bundle_hash: cache_bundle_hash(&file_inputs),
        files,
    })
}

fn cache_bundle_hash(file_inputs: &[(&str, &str, &Path, &[u8])]) -> String {
    let mut material = Vec::new();
    for (role, relative_path, _absolute_path, bytes) in file_inputs {
        material.extend_from_slice(role.as_bytes());
        material.push(0);
        material.extend_from_slice(relative_path.as_bytes());
        material.push(0);
        material.extend_from_slice(bytes.len().to_string().as_bytes());
        material.push(0);
        material.extend_from_slice(bytes);
        material.push(0);
    }
    witness::hash_bytes(&material)
}

fn validate_index_cache_receipt(
    paths: &EntityIndexDiskPaths,
    artifact: &EntityIndexArtifact,
    receipt: &EntityIndexCacheReceipt,
) -> Result<(), Refusal> {
    if receipt.version != CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION {
        return Err(artifact_contract_refusal(
            "Entity index cache receipt version mismatch",
            json!({
                "stage": "index",
                "field": "cache_receipt.version",
                "expected": CANON_ENTITY_INDEX_CACHE_RECEIPT_VERSION,
                "actual": receipt.version,
                "writes_performed": false
            }),
        ));
    }
    validate_cache_receipt_mode_status(receipt.mode, receipt.status, receipt.reusable)?;
    validate_receipt_paths_do_not_collide(paths)?;
    let artifact_bytes = read_file_bytes_reject_symlink(&paths.artifact_path, "index artifact")?;
    let cache_key_bytes = read_file_bytes_reject_symlink(&paths.cache_key_path, "index cache key")?;
    let postings_bytes = read_file_bytes_reject_symlink(&paths.postings_path, "index postings")?;
    let diagnostics_bytes =
        read_file_bytes_reject_symlink(&paths.diagnostics_path, "index diagnostics")?;
    let expected = cache_receipt_from_bytes(
        artifact,
        paths,
        receipt.mode,
        receipt.status,
        receipt.reusable,
        &artifact_bytes,
        &cache_key_bytes,
        &postings_bytes,
        &diagnostics_bytes,
    )?;
    if receipt.files != expected.files || receipt.bundle_hash != expected.bundle_hash {
        return Err(artifact_contract_refusal(
            "Entity index cache receipt does not match bundle bytes",
            json!({
                "stage": "index",
                "field": "cache_receipt",
                "writes_performed": false
            }),
        ));
    }
    Ok(())
}

fn validate_cache_receipt_mode_status(
    mode: EntityIndexCacheMode,
    status: EntityIndexCacheStatus,
    reusable: bool,
) -> Result<(), Refusal> {
    let allowed = match mode {
        EntityIndexCacheMode::Enabled => {
            reusable
                && matches!(
                    status,
                    EntityIndexCacheStatus::Hit | EntityIndexCacheStatus::Rebuilt
                )
        }
        EntityIndexCacheMode::Disabled => !reusable && status == EntityIndexCacheStatus::Bypassed,
    };
    if allowed {
        return Ok(());
    }
    Err(artifact_contract_refusal(
        "Entity index cache receipt mode/status/reusable combination is invalid",
        json!({
            "stage": "index",
            "mode": mode.as_str(),
            "status": status.as_str(),
            "reusable": reusable,
            "writes_performed": false
        }),
    ))
}

fn validate_receipt_paths_do_not_collide(paths: &EntityIndexDiskPaths) -> Result<(), Refusal> {
    let mut seen = BTreeMap::new();
    for (role, path) in [
        ("artifact", &paths.artifact_path),
        ("cache_key", &paths.cache_key_path),
        ("postings", &paths.postings_path),
        ("diagnostics", &paths.diagnostics_path),
        ("receipt", &paths.receipt_path),
    ] {
        if let Some(previous) = seen.insert(path.clone(), role) {
            return Err(artifact_contract_refusal(
                "Entity index cache bundle paths must not collide",
                json!({
                    "stage": "index",
                    "path": path.display().to_string(),
                    "first_role": previous,
                    "second_role": role,
                    "writes_performed": false
                }),
            ));
        }
    }
    Ok(())
}

fn reject_symlinked_bundle_components(
    work_dir: &Path,
    paths: &EntityIndexDiskPaths,
) -> Result<(), Refusal> {
    for (label, path) in [
        ("index artifact", &paths.artifact_path),
        ("index cache key", &paths.cache_key_path),
        ("index postings", &paths.postings_path),
        ("index diagnostics", &paths.diagnostics_path),
        ("index cache receipt", &paths.receipt_path),
    ] {
        reject_symlinked_path_components(work_dir, path, label)?;
    }
    Ok(())
}

fn reject_symlinked_path_components(
    work_dir: &Path,
    path: &Path,
    label: &str,
) -> Result<(), Refusal> {
    reject_existing_symlink_component(work_dir, label)?;
    let relative = path.strip_prefix(work_dir).map_err(|_| {
        artifact_contract_refusal(
            "Entity index cache path must remain under work_dir",
            json!({
                "stage": "index",
                "artifact": label,
                "work_dir": work_dir.display().to_string(),
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        )
    })?;
    let mut current = work_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(artifact_contract_refusal(
                "Entity index cache path must be a safe work_dir-relative path",
                json!({
                    "stage": "index",
                    "artifact": label,
                    "path": path.display().to_string(),
                    "writes_performed": false
                }),
            ));
        };
        current.push(name);
        if !reject_existing_symlink_component(&current, label)? {
            break;
        }
    }
    Ok(())
}

fn reject_existing_symlink_component(path: &Path, label: &str) -> Result<bool, Refusal> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(artifact_contract_refusal(
                    "Entity index cache path component must not be a symlink",
                    json!({
                        "stage": "index",
                        "artifact": label,
                        "path": path.display().to_string(),
                        "writes_performed": false
                    }),
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(read_io_refusal(path, label, error)),
    }
}

pub(crate) fn preflight_index_cache_entry_paths(work_dir: &Path) -> Result<(), Refusal> {
    let artifact_path = work_dir.join(INDEX_ARTIFACT_FILE);
    let cache_key_path = work_dir.join(INDEX_CACHE_KEY_FILE);
    let receipt_path = work_dir.join(INDEX_CACHE_RECEIPT_FILE);
    index_cache_file_exists(work_dir, &artifact_path, "entity index artifact")?;
    index_cache_file_exists(work_dir, &cache_key_path, "index cache key")?;
    index_cache_file_exists(work_dir, &receipt_path, "index cache receipt")?;
    Ok(())
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

fn read_file_bytes_reject_symlink(path: &Path, label: &str) -> Result<Vec<u8>, Refusal> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| read_io_refusal(path, label, error))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(artifact_contract_refusal(
            "Entity index cache file must be a regular non-symlink file",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    let bytes = fs::read(path).map_err(|error| read_io_refusal(path, label, error))?;
    if metadata.len() != bytes.len() as u64 {
        return Err(artifact_contract_refusal(
            "Entity index cache file changed while being verified",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "expected_byte_count": metadata.len(),
                "actual_byte_count": bytes.len(),
                "writes_performed": false
            }),
        ));
    }
    Ok(bytes)
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
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return Err(artifact_contract_refusal(
            "Entity index cache file must not overwrite a symlink",
            json!({
                "stage": "index",
                "path": path.display().to_string(),
                "writes_performed": false
            }),
        ));
    }
    let tmp_path = temporary_sibling_path(path);
    fs::write(&tmp_path, bytes).map_err(|error| write_io_refusal(&tmp_path, error))?;
    fs::rename(&tmp_path, path).map_err(|error| write_io_refusal(path, error))
}

fn temporary_sibling_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()))
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
