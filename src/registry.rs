use crate::{
    Registry, RegistryDiffChangeType, RegistryDiffChangedEntry, RegistryDiffEntry,
    RegistryDiffOutput, RegistryDiffRemovedEntry, RegistryDiffSummary, RegistryDiffValue,
    RegistryDiffVersion, RegistryMeta,
};
pub use build::{RegistryBuildError, RegistryBuildErrorKind, RegistryBuildRequest, build_registry};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

mod build;
mod next_id;
mod provider;

pub use next_id::{RegistryNextIdOutput, RegistryNextIdRequest, next_id};

#[derive(Debug, Clone, Deserialize)]
struct RegistryJson {
    id: String,
    version: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    updated: String,
    entry_count: usize,
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

pub fn load_registry(registry_dir: &Path) -> Result<Registry, Box<dyn Error>> {
    let (registry_json, registry_meta, mapping_files) = load_registry_definition(registry_dir)?;

    // Build or validate SQLite index
    let db_path = registry_dir.join("_index.sqlite");
    let needs_rebuild = should_rebuild_index(&db_path, registry_dir, &registry_json.version)?;

    if needs_rebuild {
        eprintln!("Building registry index for {}", registry_meta.id);
        build_index(
            &db_path,
            &registry_json.version,
            &mapping_files,
            registry_dir,
        )?;
    }

    Ok(Registry {
        meta: registry_meta,
        db_path,
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
    let mut mapping_files = Vec::new();

    let entries = fs::read_dir(registry_dir)
        .map_err(|e| format!("Failed to read registry directory: {}", e))?;

    let mut json_files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file()
            && path.extension() == Some("json".as_ref())
            && path.file_name() != Some("registry.json".as_ref())
            && path.file_name() != Some("_build.json".as_ref())
        {
            json_files.push(path);
        }
    }

    // Sort files by filename for deterministic precedence
    json_files.sort();

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

fn should_rebuild_index(
    db_path: &Path,
    registry_dir: &Path,
    version: &str,
) -> Result<bool, Box<dyn Error>> {
    if !db_path.exists() {
        return Ok(true);
    }

    // Try to connect to existing database
    let conn = match Connection::open(db_path) {
        Ok(conn) => conn,
        Err(_) => return Ok(true), // Database corrupted, rebuild
    };

    // Check if metadata table exists and has correct version
    let stored_version: Result<String, _> = conn.query_row(
        "SELECT value FROM metadata WHERE key = 'version'",
        [],
        |row| row.get(0),
    );

    let stored_version = match stored_version {
        Ok(v) => v,
        Err(_) => return Ok(true), // No version metadata, rebuild
    };

    if stored_version != version {
        return Ok(true); // Version changed, rebuild
    }

    // Check file modification times
    let stored_max_mtime: Result<u64, _> = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'max_mtime'",
            [],
            |row| row.get::<_, String>(0),
        )
        .and_then(|s| {
            s.parse::<u64>()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))
        });

    let stored_max_mtime = match stored_max_mtime {
        Ok(t) => t,
        Err(_) => return Ok(true), // No mtime metadata, rebuild
    };

    let current_max_mtime = get_max_mtime(registry_dir)?;
    Ok(current_max_mtime > stored_max_mtime)
}

fn get_max_mtime(registry_dir: &Path) -> Result<u64, Box<dyn Error>> {
    let mut max_mtime = 0;

    let entries = fs::read_dir(registry_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension() == Some("json".as_ref()) {
            let metadata = fs::metadata(&path)?;
            let mtime = metadata
                .modified()?
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs();
            max_mtime = max_mtime.max(mtime);
        }
    }

    Ok(max_mtime)
}

fn build_index(
    db_path: &Path,
    version: &str,
    mapping_files: &[MappingFile],
    registry_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let use_memory_db = !can_write_to_dir(registry_dir);

    if use_memory_db {
        eprintln!("Warning: Registry directory not writable, using in-memory index");
        // For now, we'll still try to write to disk but handle the error gracefully
    }

    let conn =
        Connection::open(db_path).map_err(|e| format!("Failed to create SQLite index: {}", e))?;

    // Create schema
    conn.execute_batch(SCHEMA_SQL)?;

    // Clear existing data
    conn.execute("DELETE FROM metadata", [])?;
    conn.execute("DELETE FROM entries", [])?;

    // Insert metadata
    let max_mtime = get_max_mtime(registry_dir)?;
    conn.execute(
        "INSERT INTO metadata (key, value) VALUES ('version', ?)",
        [version],
    )?;
    conn.execute(
        "INSERT INTO metadata (key, value) VALUES ('max_mtime', ?)",
        [&max_mtime.to_string()],
    )?;

    // Insert entries
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

    Ok(())
}

fn can_write_to_dir(dir: &Path) -> bool {
    // Try to create a temporary file to test writability
    let test_file = dir.join(".write_test_tmp");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
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
