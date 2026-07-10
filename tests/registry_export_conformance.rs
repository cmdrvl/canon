use canon::RefusalCode;
use canon::registry::{RegistryExportFormat, RegistryExportRequest, export_registry};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct SeedAliasRow {
    #[serde(rename = "source_input")]
    alias: String,
    normalized_key: String,
    canonical_id: String,
    canonical_iri: String,
    canonical_type: String,
    alias_kind: String,
    rule_id: String,
    match_source: String,
    source_file: String,
    entry_order: usize,
    registry_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchAliasRow {
    alias: String,
    normalized_key: String,
    canonical_id: String,
    canonical_iri: String,
    canonical_type: String,
    alias_kind: String,
    rule_id: String,
    match_source: String,
    source_file: String,
    entry_order: usize,
    registry_version: String,
}

fn write_registry_metadata(path: &Path, id: &str, version: &str, entry_count: usize) {
    fs::write(
        path.join("registry.json"),
        serde_json::to_string_pretty(&json!({
            "id": id,
            "version": version,
            "description": "registry export conformance fixture",
            "updated": "2026-07-10",
            "entry_count": entry_count,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn write_mapping_file(path: &Path, name: &str, entries: serde_json::Value) {
    fs::write(
        path.join(name),
        serde_json::to_string_pretty(&entries).unwrap(),
    )
    .unwrap();
}

fn fixture_request(
    registry: &Path,
    format: RegistryExportFormat,
    out: &Path,
) -> RegistryExportRequest {
    RegistryExportRequest {
        registry: registry.to_path_buf(),
        format,
        out: out.to_path_buf(),
        namespace: Some("funds".to_string()),
        source_files: vec![
            "01-shadowed.json".to_string(),
            "00-funds.json".to_string(),
            "00-funds.json".to_string(),
        ],
        canonical_types: vec!["fund".to_string(), "fund".to_string()],
        rule_id_prefixes: vec!["FUND_".to_string(), "FUND_".to_string()],
        canonical_iri_prefix: "cmdrvl:".to_string(),
        schema_out: None,
        anti_collapse_test_out: None,
    }
}

fn build_registry_fixture(path: &Path) {
    fs::create_dir_all(path).unwrap();
    write_registry_metadata(path, "funds", "2026.07.10", 6);
    write_mapping_file(
        path,
        "00-funds.json",
        json!([
            {
                "input": "Alpha Fund II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "ALPHA-II",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_TICKER"
            },
            {
                "input": "0001234567",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_KEY"
            },
            {
                "input": "Alpha Common",
                "canonical_id": "EQ-0001",
                "canonical_type": "equity",
                "rule_id": "EQUITY_NAME"
            }
        ]),
    );
    write_mapping_file(
        path,
        "01-shadowed.json",
        json!([
            {
                "input": "Alpha Fund II",
                "canonical_id": "FUND-0099",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME_SHADOWED"
            },
            {
                "input": "ALPHA SECONDARY",
                "canonical_id": "FUND-0002",
                "canonical_type": "fund",
                "rule_id": "SECONDARY_NAME"
            }
        ]),
    );
}

fn read_seed_aliases(path: &Path) -> Vec<SearchAliasRow> {
    let mut rows = csv::Reader::from_path(path)
        .unwrap()
        .deserialize::<SeedAliasRow>()
        .map(|row| {
            let row = row.unwrap();
            SearchAliasRow {
                alias: row.alias,
                normalized_key: row.normalized_key,
                canonical_id: row.canonical_id,
                canonical_iri: row.canonical_iri,
                canonical_type: row.canonical_type,
                alias_kind: row.alias_kind,
                rule_id: row.rule_id,
                match_source: row.match_source,
                source_file: row.source_file,
                entry_order: row.entry_order,
                registry_version: row.registry_version,
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        (
            left.normalized_key.as_str(),
            left.canonical_id.as_str(),
            left.source_file.as_str(),
            left.entry_order,
        )
            .cmp(&(
                right.normalized_key.as_str(),
                right.canonical_id.as_str(),
                right.source_file.as_str(),
                right.entry_order,
            ))
    });
    rows
}

fn read_search_aliases(path: &Path) -> Vec<SearchAliasRow> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT alias, normalized_key, canonical_id, canonical_iri, canonical_type, alias_kind, rule_id, match_source, source_file, entry_order, registry_version
             FROM aliases
             ORDER BY normalized_key, canonical_id, source_file, entry_order",
        )
        .unwrap();

    stmt.query_map([], |row| {
        Ok(SearchAliasRow {
            alias: row.get(0)?,
            normalized_key: row.get(1)?,
            canonical_id: row.get(2)?,
            canonical_iri: row.get(3)?,
            canonical_type: row.get(4)?,
            alias_kind: row.get(5)?,
            rule_id: row.get(6)?,
            match_source: row.get(7)?,
            source_file: row.get(8)?,
            entry_order: row.get(9)?,
            registry_version: row.get(10)?,
        })
    })
    .unwrap()
    .map(|row| row.unwrap())
    .collect()
}

#[test]
fn registry_export_backends_share_filtered_first_match_snapshot() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    build_registry_fixture(&registry_dir);

    let seed_path = temp.path().join("funds.csv");
    let sqlite_path = temp.path().join("funds.sqlite");

    let seed_output = export_registry(fixture_request(
        &registry_dir,
        RegistryExportFormat::DbtSeed,
        &seed_path,
    ))
    .unwrap();
    let search_output = export_registry(fixture_request(
        &registry_dir,
        RegistryExportFormat::SearchIndex,
        &sqlite_path,
    ))
    .unwrap();

    assert_eq!(seed_output.summary, search_output.summary);
    assert_eq!(seed_output.summary.exported_alias_count, 3);
    assert_eq!(seed_output.summary.exported_entity_count, 1);

    let seed_aliases = read_seed_aliases(&seed_path);
    let search_aliases = read_search_aliases(&sqlite_path);
    assert_eq!(seed_aliases, search_aliases);

    let conn = Connection::open(&sqlite_path).unwrap();
    let snapshot_hash: String = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'snapshot_hash'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(snapshot_hash.starts_with("blake3:"));

    let display_name: String = conn
        .query_row(
            "SELECT display_name FROM entities WHERE canonical_id = 'FUND-0001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(display_name, "Alpha Fund II");

    let alias_count: i64 = conn
        .query_row(
            "SELECT alias_count FROM entities WHERE canonical_id = 'FUND-0001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(alias_count, 3);
}

#[test]
fn registry_export_filter_order_is_canonicalized() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    build_registry_fixture(&registry_dir);

    let first_seed = temp.path().join("first.csv");
    let second_seed = temp.path().join("second.csv");

    let mut first_request =
        fixture_request(&registry_dir, RegistryExportFormat::DbtSeed, &first_seed);
    first_request.source_files = vec![
        " 00-funds.json ".to_string(),
        "01-shadowed.json".to_string(),
        "00-funds.json".to_string(),
    ];
    first_request.rule_id_prefixes = vec!["FUND_".to_string(), "FUND_".to_string()];

    let mut second_request =
        fixture_request(&registry_dir, RegistryExportFormat::DbtSeed, &second_seed);
    second_request.source_files = vec!["01-shadowed.json".to_string(), "00-funds.json".to_string()];
    second_request.rule_id_prefixes = vec!["FUND_".to_string()];

    let first_output = export_registry(first_request).unwrap();
    let second_output = export_registry(second_request).unwrap();

    assert_eq!(first_output.summary, second_output.summary);
    assert_eq!(
        first_output.filters.source_files,
        second_output.filters.source_files
    );
    assert_eq!(
        first_output.filters.rule_id_prefixes,
        second_output.filters.rule_id_prefixes
    );
    assert_eq!(first_output.content_hash, second_output.content_hash);
    assert_eq!(
        fs::read(&first_seed).unwrap(),
        fs::read(&second_seed).unwrap()
    );
}

#[test]
fn registry_export_search_index_rejects_dbt_scaffold_outputs() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    build_registry_fixture(&registry_dir);

    let request = RegistryExportRequest {
        schema_out: Some(temp.path().join("schema.yml")),
        ..fixture_request(
            &registry_dir,
            RegistryExportFormat::SearchIndex,
            &temp.path().join("funds.sqlite"),
        )
    };

    let refusal = export_registry(request).unwrap_err();
    assert_eq!(refusal.code, RefusalCode::EParse);
    assert!(refusal.message.contains("dbt scaffold outputs"));
}
