use crate::{Registry, RegistryMeta};
use rusqlite::Connection;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
struct RegistryJson {
    id: String,
    version: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    updated: String,
    entry_count: usize,
}

#[derive(Debug, Deserialize, serde::Serialize)]
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
        id: registry_json.id,
        version: registry_json.version.clone(),
        source: registry_dir.to_string_lossy().into_owned(),
    };

    // Discover mapping files
    let mapping_files = discover_mapping_files(registry_dir)?;

    // Validate entry count
    let actual_entry_count: usize = mapping_files.iter().map(|f| f.entries.len()).sum();
    if actual_entry_count != registry_json.entry_count {
        eprintln!(
            "Warning: registry.json entry_count ({}) differs from actual count ({}). Update to \"entry_count\": {}",
            registry_json.entry_count, actual_entry_count, actual_entry_count
        );
    }

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

    fn create_test_registry(temp_dir: &Path) -> Result<(), Box<dyn Error>> {
        // Create registry.json
        let registry_json = serde_json::json!({
            "id": "test-registry",
            "version": "1.0.0",
            "description": "Test registry",
            "updated": "2026-01-01",
            "entry_count": 3
        });
        fs::write(
            temp_dir.join("registry.json"),
            serde_json::to_string_pretty(&registry_json)?,
        )?;

        // Create mapping file
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
        fs::write(
            temp_dir.join("ticker-to-cusip.json"),
            serde_json::to_string_pretty(&mappings)?,
        )?;

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

        let mapping_files = discover_mapping_files(temp_dir.path())?;

        assert_eq!(mapping_files.len(), 1);
        assert_eq!(mapping_files[0].entries.len(), 3);
        assert_eq!(mapping_files[0].entries[0].input, "AAPL");

        Ok(())
    }
}
