use crate::{
    Registry, RegistryDiffChangeType, RegistryDiffChangedEntry, RegistryDiffEntry,
    RegistryDiffOutput, RegistryDiffRemovedEntry, RegistryDiffSummary, RegistryDiffValue,
    RegistryDiffVersion, RegistryMeta,
    paths::{self, RegistryIndexCacheMode},
};
pub use build::{RegistryBuildError, RegistryBuildErrorKind, RegistryBuildRequest, build_registry};
pub use export::{
    RegistryExportFormat, RegistryExportOutput, RegistryExportRequest, export_registry,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod add_entry;
mod build;
mod export;
mod id_scheme;
mod mint;
mod next_id;
pub mod package;
mod provider;

pub use add_entry::{
    RegistryAddEntryOutput, RegistryAddEntryPlan, RegistryAddEntryRequest, RegistryVersionBump,
    add_entry, plan_add_entry,
};
pub use id_scheme::{
    RegistryDefaultIdSchemeOutput, RegistryDefaultIdSchemeRequest, set_default_id_scheme,
};
pub use mint::{RegistryMintOutput, RegistryMintRequest, mint};
pub use next_id::{RegistryNextIdOutput, RegistryNextIdRequest, next_id};
pub use package::{
    REGISTRY_PACKAGE_SCHEMA_VERSION, RegistryPackage, RegistryPackageAttachmentDescriptor,
    RegistryPackageDependencyReference, RegistryPackageDeploymentProjection,
    RegistryPackageDescriptor, RegistryPackageError, RegistryPackageErrorKind,
    RegistryPackageIdentityRules, RegistryPackageLayouts, RegistryPackageRegistryIdentity,
    canonical_package_bytes, compile_registry_package, parse_registry_package,
    validate_registry_package,
};
pub use provider::{
    ProviderCatalogEntry, ProviderExample, ProviderOption, ProviderSchema, provider_catalog,
    provider_schema,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RegistryJson {
    id: String,
    version: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    updated: String,
    entry_count: usize,
    #[serde(default)]
    canonical_iri_namespace: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    default_id_scheme: Option<DefaultIdScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DefaultIdScheme {
    pub prefix: String,
    pub zero_pad: usize,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct MappingEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug)]
struct MappingFile {
    path: PathBuf,
    entries: Vec<MappingEntry>,
}

#[derive(Debug)]
struct RegistrySnapshot {
    meta: RegistryMeta,
    entries: Vec<RegistryDiffEntry>,
}

#[derive(Debug)]
pub struct RegistryDiffError {
    pub source: Box<dyn Error>,
    pub is_mismatched_id: bool,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub old_id: String,
    pub new_id: String,
}

impl RegistryDiffError {
    fn other(source: Box<dyn Error>, old_path: &Path, new_path: &Path) -> Self {
        Self {
            source,
            is_mismatched_id: false,
            old_path: old_path.to_path_buf(),
            new_path: new_path.to_path_buf(),
            old_id: String::new(),
            new_id: String::new(),
        }
    }

    fn mismatched_id(old_path: &Path, old_id: &str, new_path: &Path, new_id: &str) -> Self {
        Self {
            source: std::io::Error::other(format!(
                "Cannot diff registries with different ids: '{}' ({}) != '{}' ({})",
                old_path.display(),
                old_id,
                new_path.display(),
                new_id,
            ))
            .into(),
            is_mismatched_id: true,
            old_path: old_path.to_path_buf(),
            new_path: new_path.to_path_buf(),
            old_id: old_id.to_string(),
            new_id: new_id.to_string(),
        }
    }
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
    input TEXT NOT NULL,
    canonical_id TEXT NOT NULL,
    canonical_type TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    source_file TEXT NOT NULL,
    entry_order INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_input ON entries(input);
"#;

const INDEX_SCHEMA_VERSION: &str = "canon.registry_index.v1";

#[derive(Debug)]
struct IndexTarget {
    cache_db_path: PathBuf,
    db_path: PathBuf,
    force_rebuild: bool,
}

pub fn load_registry(registry_dir: &Path) -> Result<Registry, Box<dyn Error>> {
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(registry_dir)?;
    let source_digest = compute_registry_source_digest(registry_dir)?;
    let index_target = resolve_index_target(registry_dir, &source_digest)?;

    let needs_rebuild = index_target.force_rebuild
        || should_rebuild_index(&index_target.cache_db_path, &source_digest)?;

    if needs_rebuild {
        eprintln!("Building registry index for {}", registry_meta.id);
        build_index(
            &index_target.cache_db_path,
            &registry_json.version,
            &source_digest,
            &mapping_files,
        )?;
    }

    if index_target.db_path != index_target.cache_db_path {
        materialize_working_index(
            &index_target.cache_db_path,
            &index_target.db_path,
            &source_digest,
        )?;
    }

    Ok(Registry {
        meta: registry_meta,
        db_path: index_target.db_path,
    })
}

pub fn diff_registries(
    old_dir: &Path,
    new_dir: &Path,
) -> Result<RegistryDiffOutput, RegistryDiffError> {
    let old_registry = load_registry_snapshot(old_dir)
        .map_err(|error| RegistryDiffError::other(error, old_dir, new_dir))?;
    let new_registry = load_registry_snapshot(new_dir)
        .map_err(|error| RegistryDiffError::other(error, old_dir, new_dir))?;

    if old_registry.meta.id != new_registry.meta.id {
        return Err(RegistryDiffError::mismatched_id(
            old_dir,
            &old_registry.meta.id,
            new_dir,
            &new_registry.meta.id,
        ));
    }

    let old_entries = old_registry
        .entries
        .into_iter()
        .map(|entry| (entry.input.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let new_entries = new_registry
        .entries
        .into_iter()
        .map(|entry| (entry.input.clone(), entry))
        .collect::<BTreeMap<_, _>>();

    let mut inputs = BTreeSet::new();
    inputs.extend(old_entries.keys().cloned());
    inputs.extend(new_entries.keys().cloned());

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0;

    for input in inputs {
        match (old_entries.get(&input), new_entries.get(&input)) {
            (None, Some(new_entry)) => added.push(new_entry.clone()),
            (Some(old_entry), None) => removed.push(RegistryDiffRemovedEntry {
                input: old_entry.input.clone(),
                canonical_id: old_entry.canonical_id.clone(),
                canonical_type: old_entry.canonical_type.clone(),
                rule_id: old_entry.rule_id.clone(),
                reason: "not_in_new_registry".to_string(),
            }),
            (Some(old_entry), Some(new_entry)) => {
                if let Some(change_type) = classify_change(old_entry, new_entry) {
                    changed.push(RegistryDiffChangedEntry {
                        input: input.clone(),
                        old: RegistryDiffValue {
                            canonical_id: old_entry.canonical_id.clone(),
                            canonical_type: old_entry.canonical_type.clone(),
                            rule_id: old_entry.rule_id.clone(),
                        },
                        new: RegistryDiffValue {
                            canonical_id: new_entry.canonical_id.clone(),
                            canonical_type: new_entry.canonical_type.clone(),
                            rule_id: new_entry.rule_id.clone(),
                        },
                        change_type,
                    });
                } else {
                    unchanged += 1;
                }
            }
            (None, None) => {}
        }
    }

    Ok(RegistryDiffOutput {
        version: "canon_registry_diff.v0".to_string(),
        old: RegistryDiffVersion {
            id: old_registry.meta.id,
            version: old_registry.meta.version,
        },
        new: RegistryDiffVersion {
            id: new_registry.meta.id,
            version: new_registry.meta.version,
        },
        summary: RegistryDiffSummary {
            total_old: old_entries.len(),
            total_new: new_entries.len(),
            added: added.len(),
            removed: removed.len(),
            changed: changed.len(),
            unchanged,
        },
        added,
        removed,
        changed,
    })
}

fn load_registry_snapshot(registry_dir: &Path) -> Result<RegistrySnapshot, Box<dyn Error>> {
    let (_, registry_meta, mapping_files) = load_registry_definition(registry_dir)?;
    Ok(RegistrySnapshot {
        meta: registry_meta,
        entries: effective_entries(&mapping_files),
    })
}

fn load_registry_definition(
    registry_dir: &Path,
) -> Result<(RegistryJson, RegistryMeta, Vec<MappingFile>), Box<dyn Error>> {
    // Check if registry directory exists
    if !registry_dir.exists() || !registry_dir.is_dir() {
        return Err(format!("Registry directory not found: {}", registry_dir.display()).into());
    }

    // Read and parse registry.json
    let registry_json_path = registry_dir.join("registry.json");
    if !registry_json_path.exists() {
        return Err("Missing registry.json in registry directory".into());
    }

    let registry_json_content = fs::read_to_string(&registry_json_path)
        .map_err(|e| format!("Failed to read registry.json: {}", e))?;

    let registry_json: RegistryJson = serde_json::from_str(&registry_json_content)
        .map_err(|e| format!("Failed to parse registry.json: {}", e))?;

    let registry_meta = RegistryMeta {
        id: registry_json.id.clone(),
        version: registry_json.version.clone(),
        source: registry_dir.to_string_lossy().into_owned(),
    };

    let mapping_files = discover_mapping_files(registry_dir)?;
    warn_if_entry_count_mismatch(&registry_json, &mapping_files);

    Ok((registry_json, registry_meta, mapping_files))
}

fn warn_if_entry_count_mismatch(registry_json: &RegistryJson, mapping_files: &[MappingFile]) {
    let actual_entry_count: usize = mapping_files.iter().map(|file| file.entries.len()).sum();
    if actual_entry_count != registry_json.entry_count {
        eprintln!(
            "Warning: registry.json entry_count ({}) differs from actual count ({}). Update to \"entry_count\": {}",
            registry_json.entry_count, actual_entry_count, actual_entry_count
        );
    }
}

fn effective_entries(mapping_files: &[MappingFile]) -> Vec<RegistryDiffEntry> {
    let mut entries = BTreeMap::new();

    for mapping_file in mapping_files {
        for entry in &mapping_file.entries {
            entries
                .entry(entry.input.clone())
                .or_insert_with(|| RegistryDiffEntry {
                    input: entry.input.clone(),
                    canonical_id: entry.canonical_id.clone(),
                    canonical_type: entry.canonical_type.clone(),
                    rule_id: entry.rule_id.clone(),
                });
        }
    }

    entries.into_values().collect()
}

fn classify_change(
    old_entry: &RegistryDiffEntry,
    new_entry: &RegistryDiffEntry,
) -> Option<RegistryDiffChangeType> {
    let canonical_id_changed = old_entry.canonical_id != new_entry.canonical_id;
    let canonical_type_changed = old_entry.canonical_type != new_entry.canonical_type;
    let rule_id_changed = old_entry.rule_id != new_entry.rule_id;

    match (
        canonical_id_changed,
        canonical_type_changed,
        rule_id_changed,
    ) {
        (false, false, false) => None,
        (true, false, false) => Some(RegistryDiffChangeType::CanonicalIdChange),
        (false, true, false) => Some(RegistryDiffChangeType::CanonicalTypeChange),
        (false, false, true) => Some(RegistryDiffChangeType::RuleIdChange),
        _ => Some(RegistryDiffChangeType::MultipleFieldsChanged),
    }
}

fn discover_mapping_files(registry_dir: &Path) -> Result<Vec<MappingFile>, Box<dyn Error>> {
    let json_files = discover_mapping_file_paths(registry_dir)?;
    let mut mapping_files = Vec::with_capacity(json_files.len());

    for path in json_files {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read mapping file {:?}: {}", path, e))?;

        let entries: Vec<MappingEntry> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse mapping file {:?}: {}", path, e))?;

        // Validate required fields
        for (i, entry) in entries.iter().enumerate() {
            if entry.input.is_empty()
                || entry.canonical_id.is_empty()
                || entry.canonical_type.is_empty()
                || entry.rule_id.is_empty()
            {
                return Err(
                    format!("Invalid entry {} in {:?}: missing required fields", i, path).into(),
                );
            }
        }

        mapping_files.push(MappingFile { path, entries });
    }

    Ok(mapping_files)
}

fn discover_mapping_file_paths(registry_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut mapping_files = Vec::new();

    let entries = fs::read_dir(registry_dir)
        .map_err(|e| format!("Failed to read registry directory: {}", e))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && path.extension() == Some("json".as_ref())
            && path.file_name() != Some("registry.json".as_ref())
            && path.file_name() != Some("_build.json".as_ref())
        {
            mapping_files.push(path);
        }
    }

    // Sort files by filename for deterministic precedence
    mapping_files.sort();
    Ok(mapping_files)
}

fn should_rebuild_index(db_path: &Path, source_digest: &str) -> Result<bool, Box<dyn Error>> {
    if !db_path.exists() {
        return Ok(true);
    }

    // Try to connect to existing database
    let conn = match Connection::open(db_path) {
        Ok(conn) => conn,
        Err(_) => return Ok(true), // Database corrupted, rebuild
    };

    // Check if metadata table exists and has correct version
    let stored_schema_version: Result<String, _> = conn.query_row(
        "SELECT value FROM metadata WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    );

    let stored_schema_version = match stored_schema_version {
        Ok(v) => v,
        Err(_) => return Ok(true), // No version metadata, rebuild
    };

    if stored_schema_version != INDEX_SCHEMA_VERSION {
        return Ok(true);
    }

    let stored_source_digest: Result<String, _> = conn.query_row(
        "SELECT value FROM metadata WHERE key = 'source_digest'",
        [],
        |row| row.get(0),
    );

    let stored_source_digest = match stored_source_digest {
        Ok(value) => value,
        Err(_) => return Ok(true),
    };

    let stored_entry_count: Result<i64, _> = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'entry_count'",
            [],
            |row| row.get::<_, String>(0),
        )
        .and_then(|value| {
            value
                .parse::<i64>()
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))
        });

    let stored_entry_count = match stored_entry_count {
        Ok(value) => value,
        Err(_) => return Ok(true),
    };

    let actual_entry_count: i64 =
        match conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0)) {
            Ok(value) => value,
            Err(_) => return Ok(true),
        };

    if actual_entry_count != stored_entry_count {
        return Ok(true);
    }

    Ok(stored_source_digest != source_digest)
}

fn compute_registry_source_digest(registry_dir: &Path) -> Result<String, Box<dyn Error>> {
    let mut hasher = blake3::Hasher::new();
    let registry_json_path = registry_dir.join("registry.json");
    let mut files = vec![registry_json_path];
    files.extend(discover_mapping_file_paths(registry_dir)?);

    for path in files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Invalid UTF-8 registry path: {}", path.display()))?;
        hasher.update(file_name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(&path)?);
        hasher.update(&[0xff]);
    }

    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn build_index(
    db_path: &Path,
    version: &str,
    source_digest: &str,
    mapping_files: &[MappingFile],
) -> Result<(), Box<dyn Error>> {
    let parent = db_path.parent().ok_or_else(|| {
        format!(
            "Registry index path does not have a parent directory: {}",
            db_path.display()
        )
    })?;
    fs::create_dir_all(parent)?;

    let temp_path = temporary_index_path(parent, db_path);
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }

    let build_result = (|| -> Result<(), Box<dyn Error>> {
        let conn = Connection::open(&temp_path)
            .map_err(|e| format!("Failed to create SQLite index: {}", e))?;

        conn.execute_batch(SCHEMA_SQL)?;
        conn.execute("DELETE FROM metadata", [])?;
        conn.execute("DELETE FROM entries", [])?;
        conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('schema_version', ?)",
            [INDEX_SCHEMA_VERSION],
        )?;
        conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('registry_version', ?)",
            [version],
        )?;
        conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('source_digest', ?)",
            [source_digest],
        )?;
        conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('entry_count', ?)",
            [mapping_files
                .iter()
                .map(|mapping_file| mapping_file.entries.len())
                .sum::<usize>()
                .to_string()],
        )?;

        let mut stmt = conn.prepare(
            "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order) VALUES (?, ?, ?, ?, ?, ?)"
        )?;

        for mapping_file in mapping_files {
            let source_file = mapping_file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            for (entry_order, entry) in mapping_file.entries.iter().enumerate() {
                stmt.execute([
                    &entry.input,
                    &entry.canonical_id,
                    &entry.canonical_type,
                    &entry.rule_id,
                    source_file,
                    &entry_order.to_string(),
                ])?;
            }
        }

        drop(stmt);
        drop(conn);
        Ok(())
    })();

    if let Err(error) = build_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    install_built_index(&temp_path, db_path, source_digest)
}

fn resolve_index_target(
    registry_dir: &Path,
    source_digest: &str,
) -> Result<IndexTarget, Box<dyn Error>> {
    match paths::registry_index_cache_mode() {
        RegistryIndexCacheMode::Managed => match paths::prepare_registry_index_cache_dir() {
            Ok(cache_dir) => {
                let cache_db_path = cache_dir.join(index_file_name(source_digest));
                let db_path = if registry_is_read_only(registry_dir)? {
                    cache_db_path.clone()
                } else {
                    temporary_cache_db_path(registry_dir, source_digest)?
                };

                Ok(IndexTarget {
                    cache_db_path,
                    db_path,
                    force_rebuild: false,
                })
            }
            Err(error) => {
                eprintln!(
                    "Warning: managed registry cache unavailable ({}); using temporary external index",
                    error
                );
                let db_path = temporary_cache_db_path(registry_dir, source_digest)?;
                Ok(IndexTarget {
                    cache_db_path: db_path.clone(),
                    db_path,
                    force_rebuild: true,
                })
            }
        },
        RegistryIndexCacheMode::NoCache => {
            let db_path = temporary_cache_db_path(registry_dir, source_digest)?;
            Ok(IndexTarget {
                cache_db_path: db_path.clone(),
                db_path,
                force_rebuild: true,
            })
        }
    }
}

fn index_file_name(source_digest: &str) -> String {
    let digest = source_digest
        .strip_prefix("blake3:")
        .unwrap_or(source_digest);
    format!("{digest}.sqlite")
}

fn registry_is_read_only(registry_dir: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(fs::metadata(registry_dir)?.permissions().readonly())
}

fn temporary_cache_db_path(
    registry_dir: &Path,
    source_digest: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir()
        .join("canon")
        .join("registry-indexes")
        .join(std::process::id().to_string());
    fs::create_dir_all(&dir)?;
    Ok(dir.join(working_index_file_name(registry_dir, source_digest)))
}

fn working_index_file_name(registry_dir: &Path, source_digest: &str) -> String {
    let digest = source_digest
        .strip_prefix("blake3:")
        .unwrap_or(source_digest);
    let mut hasher = blake3::Hasher::new();
    hasher.update(registry_dir.to_string_lossy().as_bytes());
    format!("{digest}-{}.sqlite", hasher.finalize().to_hex())
}

fn temporary_index_path(parent: &Path, db_path: &Path) -> PathBuf {
    let stem = db_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("registry-index");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{stem}.{unique}.tmp"))
}

fn install_built_index(
    temp_path: &Path,
    db_path: &Path,
    source_digest: &str,
) -> Result<(), Box<dyn Error>> {
    match fs::rename(temp_path, db_path) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            if db_path.exists() && !should_rebuild_index(db_path, source_digest)? {
                let _ = fs::remove_file(temp_path);
                return Ok(());
            }

            if db_path.exists() {
                fs::remove_file(db_path).map_err(|remove_error| {
                    format!(
                        "Failed to replace registry index {} after rename error {}: {}",
                        db_path.display(),
                        rename_error,
                        remove_error
                    )
                })?;
                fs::rename(temp_path, db_path).map_err(|retry_error| {
                    format!(
                        "Failed to install rebuilt registry index {}: {}",
                        db_path.display(),
                        retry_error
                    )
                })?;
                return Ok(());
            }

            let _ = fs::remove_file(temp_path);
            Err(format!(
                "Failed to install registry index {}: {}",
                db_path.display(),
                rename_error
            )
            .into())
        }
    }
}

fn materialize_working_index(
    cache_db_path: &Path,
    working_db_path: &Path,
    source_digest: &str,
) -> Result<(), Box<dyn Error>> {
    if !should_rebuild_index(working_db_path, source_digest)? {
        return Ok(());
    }

    let parent = working_db_path.parent().ok_or_else(|| {
        format!(
            "Working registry index path does not have a parent directory: {}",
            working_db_path.display()
        )
    })?;
    fs::create_dir_all(parent)?;

    let temp_path = temporary_index_path(parent, working_db_path);
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }

    fs::copy(cache_db_path, &temp_path).map_err(|error| {
        format!(
            "Failed to copy registry cache {} to working index {}: {}",
            cache_db_path.display(),
            working_db_path.display(),
            error
        )
    })?;

    install_copied_index(&temp_path, working_db_path, source_digest)
}

fn install_copied_index(
    temp_path: &Path,
    working_db_path: &Path,
    source_digest: &str,
) -> Result<(), Box<dyn Error>> {
    match fs::rename(temp_path, working_db_path) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            if working_db_path.exists() && !should_rebuild_index(working_db_path, source_digest)? {
                let _ = fs::remove_file(temp_path);
                return Ok(());
            }

            if working_db_path.exists() {
                fs::remove_file(working_db_path).map_err(|remove_error| {
                    format!(
                        "Failed to replace working registry index {} after rename error {}: {}",
                        working_db_path.display(),
                        rename_error,
                        remove_error
                    )
                })?;
                fs::rename(temp_path, working_db_path).map_err(|retry_error| {
                    format!(
                        "Failed to install working registry index {}: {}",
                        working_db_path.display(),
                        retry_error
                    )
                })?;
                return Ok(());
            }

            let _ = fs::remove_file(temp_path);
            Err(format!(
                "Failed to install working registry index {}: {}",
                working_db_path.display(),
                rename_error
            )
            .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_registry_metadata(
        temp_dir: &Path,
        id: &str,
        version: &str,
        entry_count: usize,
    ) -> Result<(), Box<dyn Error>> {
        let registry_json = serde_json::json!({
            "id": id,
            "version": version,
            "description": "Test registry",
            "updated": "2026-01-01",
            "entry_count": entry_count
        });
        fs::write(
            temp_dir.join("registry.json"),
            serde_json::to_string_pretty(&registry_json)?,
        )?;
        Ok(())
    }

    fn write_mapping_file(
        temp_dir: &Path,
        name: &str,
        entries: &[MappingEntry],
    ) -> Result<(), Box<dyn Error>> {
        fs::write(temp_dir.join(name), serde_json::to_string_pretty(entries)?)?;
        Ok(())
    }

    fn create_test_registry(temp_dir: &Path) -> Result<(), Box<dyn Error>> {
        write_registry_metadata(temp_dir, "test-registry", "1.0.0", 3)?;

        let mappings = vec![
            MappingEntry {
                input: "AAPL".to_string(),
                canonical_id: "037833100".to_string(),
                canonical_type: "cusip".to_string(),
                rule_id: "TICKER_TO_CUSIP".to_string(),
            },
            MappingEntry {
                input: "MSFT".to_string(),
                canonical_id: "594918104".to_string(),
                canonical_type: "cusip".to_string(),
                rule_id: "TICKER_TO_CUSIP".to_string(),
            },
            MappingEntry {
                input: "GOOGL".to_string(),
                canonical_id: "02079K305".to_string(),
                canonical_type: "cusip".to_string(),
                rule_id: "TICKER_TO_CUSIP".to_string(),
            },
        ];
        write_mapping_file(temp_dir, "ticker-to-cusip.json", &mappings)?;

        Ok(())
    }

    #[test]
    fn test_load_registry_success() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        create_test_registry(temp_dir.path())?;

        let registry = load_registry(temp_dir.path())?;

        assert_eq!(registry.meta.id, "test-registry");
        assert_eq!(registry.meta.version, "1.0.0");
        assert!(registry.db_path.exists());
        assert!(!temp_dir.path().join("_index.sqlite").exists());
        assert!(!registry.db_path.starts_with(temp_dir.path()));

        Ok(())
    }

    #[test]
    fn test_load_registry_missing_directory() {
        let result = load_registry(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Registry directory not found")
        );
    }

    #[test]
    fn test_load_registry_missing_registry_json() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;

        let result = load_registry(temp_dir.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing registry.json")
        );

        Ok(())
    }

    #[test]
    fn test_discover_mapping_files() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        create_test_registry(temp_dir.path())?;
        fs::write(
            temp_dir.path().join("_build.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "canon_registry_build.v0",
                "summary": { "seed_count": 3 }
            }))?,
        )?;

        let mapping_files = discover_mapping_files(temp_dir.path())?;

        assert_eq!(mapping_files.len(), 1);
        assert_eq!(mapping_files[0].entries.len(), 3);
        assert_eq!(mapping_files[0].entries[0].input, "AAPL");

        Ok(())
    }

    #[test]
    fn test_effective_entries_follow_sorted_file_precedence() -> Result<(), Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        write_registry_metadata(temp_dir.path(), "test-registry", "1.0.0", 4)?;
        write_mapping_file(
            temp_dir.path(),
            "z-secondary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "SECOND".to_string(),
                    canonical_type: "ticker".to_string(),
                    rule_id: "SECONDARY".to_string(),
                },
                MappingEntry {
                    input: "NVDA".to_string(),
                    canonical_id: "67066G104".to_string(),
                    canonical_type: "cusip".to_string(),
                    rule_id: "SECONDARY".to_string(),
                },
            ],
        )?;
        write_mapping_file(
            temp_dir.path(),
            "a-primary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "FIRST".to_string(),
                    canonical_type: "ticker".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
                MappingEntry {
                    input: "MSFT".to_string(),
                    canonical_id: "594918104".to_string(),
                    canonical_type: "cusip".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
            ],
        )?;

        let snapshot = load_registry_snapshot(temp_dir.path())?;

        assert_eq!(snapshot.entries.len(), 3);
        assert_eq!(snapshot.entries[0].input, "AAPL");
        assert_eq!(snapshot.entries[0].canonical_id, "FIRST");
        assert_eq!(snapshot.entries[0].rule_id, "PRIMARY");
        assert_eq!(snapshot.entries[2].input, "NVDA");

        Ok(())
    }

    #[test]
    fn test_diff_registries_reports_add_remove_change_and_unchanged() -> Result<(), Box<dyn Error>>
    {
        let old_dir = TempDir::new()?;
        write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 3)?;
        write_mapping_file(
            old_dir.path(),
            "a-primary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "BBG000B9XRY4".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
                MappingEntry {
                    input: "MSFT".to_string(),
                    canonical_id: "BBG000BPH459".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
                MappingEntry {
                    input: "TSLA".to_string(),
                    canonical_id: "BBG000N9MNX3".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
            ],
        )?;

        let new_dir = TempDir::new()?;
        write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 3)?;
        write_mapping_file(
            new_dir.path(),
            "a-primary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "BBG000B9XRY4".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
                MappingEntry {
                    input: "MSFT".to_string(),
                    canonical_id: "BBG000BPH45Z".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
                MappingEntry {
                    input: "NVDA".to_string(),
                    canonical_id: "BBG000BBJQV0".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "OPENFIGI".to_string(),
                },
            ],
        )?;

        let diff = diff_registries(old_dir.path(), new_dir.path()).unwrap();

        assert_eq!(
            diff.summary,
            RegistryDiffSummary {
                total_old: 3,
                total_new: 3,
                added: 1,
                removed: 1,
                changed: 1,
                unchanged: 1,
            }
        );
        assert_eq!(diff.added[0].input, "NVDA");
        assert_eq!(diff.removed[0].input, "TSLA");
        assert_eq!(diff.removed[0].reason, "not_in_new_registry");
        assert_eq!(diff.changed[0].input, "MSFT");
        assert_eq!(
            diff.changed[0].change_type,
            RegistryDiffChangeType::CanonicalIdChange
        );

        Ok(())
    }

    #[test]
    fn test_diff_registries_ignores_shadowed_entries_in_new_mapping_files()
    -> Result<(), Box<dyn Error>> {
        let old_dir = TempDir::new()?;
        write_registry_metadata(old_dir.path(), "openfigi-cusip", "2026.02.28", 2)?;
        write_mapping_file(
            old_dir.path(),
            "a-primary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "BBG000B9XRY4".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
                MappingEntry {
                    input: "MSFT".to_string(),
                    canonical_id: "BBG000BPH459".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
            ],
        )?;

        let new_dir = TempDir::new()?;
        write_registry_metadata(new_dir.path(), "openfigi-cusip", "2026.03.05", 4)?;
        write_mapping_file(
            new_dir.path(),
            "a-primary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "BBG000B9XRY4".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
                MappingEntry {
                    input: "MSFT".to_string(),
                    canonical_id: "BBG000BPH459".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "PRIMARY".to_string(),
                },
            ],
        )?;
        write_mapping_file(
            new_dir.path(),
            "z-secondary.json",
            &[
                MappingEntry {
                    input: "AAPL".to_string(),
                    canonical_id: "SHADOWED".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "SECONDARY".to_string(),
                },
                MappingEntry {
                    input: "NVDA".to_string(),
                    canonical_id: "BBG000BBJQV0".to_string(),
                    canonical_type: "composite_figi".to_string(),
                    rule_id: "SECONDARY".to_string(),
                },
            ],
        )?;

        let diff = diff_registries(old_dir.path(), new_dir.path()).unwrap();

        assert_eq!(diff.summary.added, 1);
        assert_eq!(diff.summary.changed, 0);
        assert_eq!(diff.summary.unchanged, 2);
        assert_eq!(diff.added[0].input, "NVDA");

        Ok(())
    }

    #[test]
    fn test_diff_registries_detects_mismatched_ids() -> Result<(), Box<dyn Error>> {
        let old_dir = TempDir::new()?;
        write_registry_metadata(old_dir.path(), "old-registry", "1.0.0", 0)?;

        let new_dir = TempDir::new()?;
        write_registry_metadata(new_dir.path(), "new-registry", "1.1.0", 0)?;

        let error = diff_registries(old_dir.path(), new_dir.path()).unwrap_err();

        assert!(error.is_mismatched_id);
        assert_eq!(error.old_id, "old-registry");
        assert_eq!(error.new_id, "new-registry");

        Ok(())
    }
}
