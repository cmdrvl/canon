#![forbid(unsafe_code)]

use canon::{
    InputFormat, InputValues, SpecialReason,
    lookup::resolve_values,
    registry::{compile_registry_package, load_registry},
    registry_lint::{RegistryLintProfile, lint},
};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn source_local_namespace() -> serde_json::Value {
    serde_json::json!({
        "kind": "core",
        "class": "source_local_id"
    })
}

fn source_scoped_scope() -> serde_json::Value {
    serde_json::json!({
        "dimensions": [
            {
                "dimension": {
                    "kind": "core",
                    "dimension": "source_system"
                },
                "binding": {
                    "binding": "exact",
                    "value": "sec_abs_ee"
                }
            },
            {
                "dimension": {
                    "kind": "core",
                    "dimension": "dataset"
                },
                "binding": {
                    "binding": "exact",
                    "value": "deal:COMM-2014-UBS4"
                }
            }
        ]
    })
}

fn write_registry_metadata(
    dir: &Path,
    id: &str,
    version: &str,
    entry_count: usize,
) -> Result<(), Box<dyn Error>> {
    fs::write(
        dir.join("registry.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": id,
            "version": version,
            "description": "registry cache contract fixture",
            "updated": "2026-07-10",
            "entry_count": entry_count
        }))?,
    )?;
    Ok(())
}

fn write_mapping_file(
    dir: &Path,
    name: &str,
    entries: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    fs::write(dir.join(name), serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

fn create_registry(dir: &Path, label: &str, aapl: &str, msft: &str) -> Result<(), Box<dyn Error>> {
    write_registry_metadata(dir, &format!("registry-{label}"), "1.0.0", 2)?;
    write_mapping_file(
        dir,
        "ticker-to-cusip.json",
        &[
            serde_json::json!({
                "input": "AAPL",
                "canonical_id": aapl,
                "canonical_type": "cusip",
                "rule_id": format!("rule-{label}")
            }),
            serde_json::json!({
                "input": "MSFT",
                "canonical_id": msft,
                "canonical_type": "cusip",
                "rule_id": format!("rule-{label}")
            }),
        ],
    )?;
    Ok(())
}

fn input_values(values: &[&str]) -> InputValues {
    let mut deduped = HashMap::new();
    for value in values {
        deduped.insert((*value).to_string(), ());
    }

    InputValues {
        values: deduped,
        special: HashMap::<SpecialReason, usize>::new(),
        format: InputFormat::Csv,
        delimiter: Some(b','),
        source_hash: None,
        source_bytes: None,
    }
}

fn resolve_aapl(registry_dir: &Path) -> Result<(PathBuf, String), Box<dyn Error>> {
    let registry = load_registry(registry_dir)?;
    let result = resolve_values(&registry, &input_values(&["AAPL"]))?;
    Ok((registry.db_path, result.mappings[0].canonical_id.clone()))
}

fn index_schema_version(db_path: &Path) -> Result<String, Box<dyn Error>> {
    let connection = Connection::open(db_path)?;
    Ok(connection.query_row(
        "SELECT value FROM metadata WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )?)
}

fn finding_codes(output: &canon::registry_lint::RegistryLintOutput) -> Vec<String> {
    output
        .findings
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

fn cleanup_cache_file(path: &Path) {
    let _ = fs::remove_file(path);
}

#[test]
fn load_registry_uses_external_cache_without_sidecar_writes() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "external", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;

    assert!(registry.db_path.exists());
    assert!(!temp.path().join("_index.sqlite").exists());
    assert!(!registry.db_path.starts_with(temp.path()));

    cleanup_cache_file(&registry.db_path);
    Ok(())
}

#[test]
fn scoped_mapping_metadata_is_carried_in_index_and_lints_clean() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    write_registry_metadata(temp.path(), "scoped", "1.0.0", 1)?;
    write_mapping_file(
        temp.path(),
        "asset-number.json",
        &[serde_json::json!({
            "input": "41-001",
            "canonical_id": "PROPERTY-0001",
            "canonical_type": "property",
            "rule_id": "ABS_EE_ASSET_NUMBER",
            "namespace": source_local_namespace(),
            "scope": source_scoped_scope()
        })],
    )?;

    let registry = load_registry(temp.path())?;
    let result = resolve_values(&registry, &input_values(&["41-001"]))?;

    assert_eq!(result.mappings.len(), 1);
    assert_eq!(result.mappings[0].canonical_id, "PROPERTY-0001");

    let connection = Connection::open(&registry.db_path)?;
    let (namespace_json, scope_json): (Option<String>, Option<String>) = connection.query_row(
        "SELECT namespace, scope FROM entries WHERE input = ?1",
        params!["41-001"],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&namespace_json.expect("namespace stored"))?,
        source_local_namespace()
    );
    let scope = serde_json::from_str::<serde_json::Value>(&scope_json.expect("scope stored"))?;
    let dimensions = scope["dimensions"].as_array().expect("scope dimensions");
    assert_eq!(dimensions.len(), 2);
    assert!(dimensions.iter().any(|dimension| {
        dimension["dimension"] == serde_json::json!({"kind":"core","dimension":"source_system"})
            && dimension["binding"] == serde_json::json!({"binding":"exact","value":"sec_abs_ee"})
    }));

    let lint_output = lint(temp.path(), RegistryLintProfile::Standard).expect("lint standard");
    let codes = finding_codes(&lint_output);
    assert_eq!(lint_output.summary.errors, 0);
    assert!(!codes.contains(&"mapping_scope_metadata_invalid".to_string()));

    let package = compile_registry_package(temp.path()).expect("scoped registry packages");
    assert_eq!(package.entry_count, 1);
    assert_eq!(package.effective_mapping_count, 1);

    cleanup_cache_file(&registry.db_path);
    Ok(())
}

#[test]
fn malformed_scoped_mapping_metadata_is_bad_registry_and_lint_error() -> Result<(), Box<dyn Error>>
{
    let temp = TempDir::new()?;
    write_registry_metadata(temp.path(), "scoped", "1.0.0", 1)?;
    write_mapping_file(
        temp.path(),
        "asset-number.json",
        &[serde_json::json!({
            "input": "41-001",
            "canonical_id": "PROPERTY-0001",
            "canonical_type": "property",
            "rule_id": "ABS_EE_ASSET_NUMBER",
            "namespace": source_local_namespace(),
            "scope": {
                "dimensions": [
                    {
                        "dimension": {
                            "kind": "core",
                            "dimension": "dataset"
                        },
                        "binding": {
                            "binding": "exact",
                            "value": "deal:COMM-2014-UBS4"
                        }
                    }
                ]
            }
        })],
    )?;

    let error = load_registry(temp.path()).expect_err("source local id without source scope fails");
    assert!(error.to_string().contains("source_local_id"));

    let lint_output = lint(temp.path(), RegistryLintProfile::Standard).expect("lint standard");
    assert!(lint_output.summary.errors > 0);
    assert!(finding_codes(&lint_output).contains(&"mapping_scope_metadata_invalid".to_string()));
    Ok(())
}

#[test]
fn identical_bytes_reuse_same_cache_without_mtime_invalidation() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "mtime", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;
    let before_path = registry.db_path.clone();
    let before_modified = fs::metadata(&before_path)?.modified()?;
    let original_bytes = fs::read(temp.path().join("ticker-to-cusip.json"))?;

    thread::sleep(Duration::from_secs(1));
    fs::write(temp.path().join("ticker-to-cusip.json"), &original_bytes)?;

    let reloaded = load_registry(temp.path())?;
    let after_modified = fs::metadata(&reloaded.db_path)?.modified()?;

    assert_eq!(reloaded.db_path, before_path);
    assert_eq!(before_modified, after_modified);

    cleanup_cache_file(&reloaded.db_path);
    Ok(())
}

#[test]
fn content_changes_invalidate_cache_key() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "digest", "037833100", "594918104")?;

    let (before_path, before_value) = resolve_aapl(temp.path())?;
    assert_eq!(before_value, "037833100");

    create_registry(temp.path(), "digest", "CHANGED-CUSIP", "594918104")?;
    let (after_path, after_value) = resolve_aapl(temp.path())?;

    assert_ne!(before_path, after_path);
    assert_eq!(after_value, "CHANGED-CUSIP");

    cleanup_cache_file(&before_path);
    cleanup_cache_file(&after_path);
    Ok(())
}

#[test]
fn legacy_v1_registry_index_is_rebuilt_not_reused() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "legacy-index", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;
    let db_path = registry.db_path.clone();
    cleanup_cache_file(&db_path);

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        r#"
        CREATE TABLE metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE entries (
            input TEXT NOT NULL,
            canonical_id TEXT NOT NULL,
            canonical_type TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            source_file TEXT NOT NULL,
            entry_order INTEGER NOT NULL
        );

        CREATE INDEX idx_input ON entries(input);
        "#,
    )?;
    connection.execute(
        "INSERT INTO metadata (key, value) VALUES ('schema_version', 'canon.registry_index.v1')",
        [],
    )?;
    connection.execute(
        "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!["TSLA", "STALE-CUSIP", "cusip", "STALE_RULE", "stale.json", 0_i64],
    )?;
    drop(connection);

    let reloaded = load_registry(temp.path())?;
    let result = resolve_values(&reloaded, &input_values(&["AAPL", "TSLA"]))?;

    assert_eq!(reloaded.db_path, db_path);
    assert_eq!(
        index_schema_version(&reloaded.db_path)?,
        "canon.registry_index.v2"
    );
    assert_eq!(result.mappings.len(), 1);
    assert_eq!(result.mappings[0].input, "AAPL");
    assert_eq!(result.mappings[0].canonical_id, "037833100");
    assert_eq!(result.unresolved.len(), 1);
    assert_eq!(result.unresolved[0].input.as_deref(), Some("TSLA"));

    cleanup_cache_file(&reloaded.db_path);
    Ok(())
}

#[test]
fn corrupt_cache_rebuilds_safely() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "corrupt", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;
    fs::write(&registry.db_path, b"not a sqlite database")?;

    let reloaded = load_registry(temp.path())?;
    let result = resolve_values(&reloaded, &input_values(&["AAPL"]))?;
    assert_eq!(result.mappings[0].canonical_id, "037833100");

    cleanup_cache_file(&reloaded.db_path);
    Ok(())
}

#[test]
fn mutated_cache_with_stale_extra_rows_rebuilds_safely() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "mutated", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;
    let connection = Connection::open(&registry.db_path)?;
    connection.execute(
        "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!["TSLA", "88160R101", "cusip", "MANUAL", "manual.json", 0_i64],
    )?;

    let reloaded = load_registry(temp.path())?;
    let result = resolve_values(&reloaded, &input_values(&["TSLA"]))?;
    assert!(result.mappings.is_empty());
    assert_eq!(result.unresolved.len(), 1);

    cleanup_cache_file(&reloaded.db_path);
    Ok(())
}

#[test]
fn cold_and_warm_cache_outputs_are_identical() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "determinism", "037833100", "594918104")?;

    let registry = load_registry(temp.path())?;
    let cold = resolve_values(&registry, &input_values(&["AAPL", "UNKNOWN"]))?;
    let db_path = registry.db_path.clone();
    cleanup_cache_file(&db_path);

    let warm_registry = load_registry(temp.path())?;
    let warm = resolve_values(&warm_registry, &input_values(&["AAPL", "UNKNOWN"]))?;

    assert_eq!(cold.mappings, warm.mappings);
    assert_eq!(cold.unresolved, warm.unresolved);
    assert_eq!(cold.summary, warm.summary);

    cleanup_cache_file(&warm_registry.db_path);
    Ok(())
}

#[test]
fn concurrent_loads_share_the_same_external_cache_safely() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    create_registry(temp.path(), "concurrent", "037833100", "594918104")?;
    let registry_dir = temp.path().to_path_buf();

    let handles = (0..2)
        .map(|_| {
            let registry_dir = registry_dir.clone();
            thread::spawn(move || {
                let registry = load_registry(&registry_dir).expect("registry loads");
                let result =
                    resolve_values(&registry, &input_values(&["AAPL"])).expect("lookup succeeds");
                (registry.db_path, result.mappings[0].canonical_id.clone())
            })
        })
        .collect::<Vec<_>>();

    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread completes"))
        .collect::<Vec<_>>();

    assert_eq!(results[0].0, results[1].0);
    assert_eq!(results[0].1, "037833100");
    assert_eq!(results[1].1, "037833100");

    cleanup_cache_file(&results[0].0);
    Ok(())
}

#[test]
fn identical_content_in_distinct_writable_registries_use_isolated_working_indexes()
-> Result<(), Box<dyn Error>> {
    let first = TempDir::new()?;
    let second = TempDir::new()?;
    create_registry(first.path(), "shared", "037833100", "594918104")?;
    create_registry(second.path(), "shared", "037833100", "594918104")?;

    let first_registry = load_registry(first.path())?;
    let second_registry = load_registry(second.path())?;

    assert_ne!(first_registry.db_path, second_registry.db_path);

    let connection = Connection::open(&first_registry.db_path)?;
    connection.execute(
        "INSERT INTO entries (input, canonical_id, canonical_type, rule_id, source_file, entry_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!["TSLA", "88160R101", "cusip", "MANUAL", "manual.json", 0_i64],
    )?;

    let first_result = resolve_values(&first_registry, &input_values(&["TSLA"]))?;
    assert_eq!(first_result.mappings[0].canonical_id, "88160R101");

    let second_result = resolve_values(&second_registry, &input_values(&["TSLA"]))?;
    assert!(second_result.mappings.is_empty());
    assert_eq!(second_result.unresolved.len(), 1);

    cleanup_cache_file(&first_registry.db_path);
    cleanup_cache_file(&second_registry.db_path);
    Ok(())
}

#[cfg(unix)]
#[test]
fn read_only_registry_mount_works_without_local_index_writes() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new()?;
    create_registry(temp.path(), "readonly", "037833100", "594918104")?;

    fs::set_permissions(
        temp.path().join("registry.json"),
        fs::Permissions::from_mode(0o444),
    )?;
    fs::set_permissions(
        temp.path().join("ticker-to-cusip.json"),
        fs::Permissions::from_mode(0o444),
    )?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o555))?;

    let registry = load_registry(temp.path())?;
    let result = resolve_values(&registry, &input_values(&["AAPL"]))?;

    assert_eq!(result.mappings[0].canonical_id, "037833100");
    assert!(!temp.path().join("_index.sqlite").exists());
    assert!(!registry.db_path.starts_with(temp.path()));

    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755))?;
    fs::set_permissions(
        temp.path().join("registry.json"),
        fs::Permissions::from_mode(0o644),
    )?;
    fs::set_permissions(
        temp.path().join("ticker-to-cusip.json"),
        fs::Permissions::from_mode(0o644),
    )?;
    cleanup_cache_file(&registry.db_path);
    Ok(())
}
