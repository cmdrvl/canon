use canon::lookup::resolve_values;
use canon::registry::{
    RegistryExportFormat, RegistryExportRequest, compile_registry_package, export_registry,
    load_registry,
};
use canon::{InputFormat, InputValues};
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};
use tempfile::tempdir;

const EXPECTED_SCHEMA: &str =
    include_str!("fixtures/registry_export/search_index/expected_schema.sql");
const QUERY_GOLDEN: &str = include_str!("fixtures/registry_export/search_index/queries.sql");

fn write_json(path: &Path, value: serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn build_search_registry_fixture(registry_dir: &Path) {
    fs::create_dir_all(registry_dir).unwrap();
    write_json(
        &registry_dir.join("registry.json"),
        json!({
            "id": "search-fixture",
            "version": "2026.07.11",
            "description": "search index export golden fixture",
            "updated": "2026-07-11",
            "entry_count": 8,
            "canonical_iri_namespace": "https://canon.example/registry/search-fixture/"
        }),
    );
    write_json(
        &registry_dir.join("00-funds.json"),
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
                "input": "São Paulo Growth",
                "canonical_id": "FUND-0002",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "ACME",
                "canonical_id": "FUND-0003",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "A-CME",
                "canonical_id": "FUND-0004",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            }
        ]),
    );
    write_json(
        &registry_dir.join("01-shadowed.json"),
        json!([
            {
                "input": "Alpha Fund II",
                "canonical_id": "FUND-0099",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME_SHADOWED"
            },
            {
                "input": "Manual Only",
                "canonical_id": "FUND-0005",
                "canonical_type": "fund",
                "rule_id": "MANUAL_NAME"
            }
        ]),
    );
}

fn search_index_request(registry_dir: &Path, sqlite_path: &Path) -> RegistryExportRequest {
    RegistryExportRequest {
        registry: registry_dir.to_path_buf(),
        format: RegistryExportFormat::SearchIndex,
        out: sqlite_path.to_path_buf(),
        namespace: Some("serving_funds".to_string()),
        source_files: Vec::new(),
        canonical_types: Vec::new(),
        rule_id_prefixes: Vec::new(),
        canonical_iri_prefix: "https://canon.example/id/".to_string(),
        schema_out: None,
        anti_collapse_test_out: None,
    }
}

fn export_fixture(registry_dir: &Path, sqlite_path: &Path) {
    let output = export_registry(search_index_request(registry_dir, sqlite_path)).unwrap();
    assert_eq!(output.summary.source_entry_count, 8);
    assert_eq!(output.summary.filtered_entry_count, 8);
    assert_eq!(output.summary.exported_alias_count, 7);
    assert_eq!(output.summary.exported_entity_count, 5);
    assert_eq!(output.summary.skipped_shadowed_count, 1);
}

fn open_readonly(path: &Path) -> Connection {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap()
}

fn logical_schema(conn: &Connection) -> String {
    let mut stmt = conn
        .prepare(
            "SELECT type, name, tbl_name, sql
             FROM sqlite_schema
             WHERE sql IS NOT NULL
               AND name NOT LIKE 'sqlite_%'
               AND name NOT GLOB 'aliases_fts_*'
             ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, name",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .map(|row| {
            let (kind, name, table, sql) = row.unwrap();
            format!("-- {kind} {name} on {table}\n{sql};\n")
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{rows}\n")
}

fn query_sections() -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    let mut current_name = None::<String>;
    let mut current_sql = Vec::new();

    for line in QUERY_GOLDEN.lines() {
        if let Some(name) = line.strip_prefix("-- query: ") {
            if let Some(previous_name) = current_name.replace(name.to_string()) {
                sections.insert(previous_name, current_sql.join("\n").trim().to_string());
                current_sql.clear();
            }
        } else if current_name.is_some() {
            current_sql.push(line);
        }
    }
    if let Some(name) = current_name {
        sections.insert(name, current_sql.join("\n").trim().to_string());
    }

    sections
}

fn query_rows(conn: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let column_count = stmt.column_count();
    stmt.query_map([], |row| {
        let mut values = Vec::with_capacity(column_count);
        for index in 0..column_count {
            values.push(row.get::<_, String>(index)?);
        }
        Ok(values)
    })
    .unwrap()
    .map(|row| row.unwrap())
    .collect()
}

fn alias_rows(conn: &Connection) -> BTreeMap<String, (String, String, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT alias, canonical_id, canonical_type, rule_id
             FROM aliases
             ORDER BY alias",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            (
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ),
        ))
    })
    .unwrap()
    .map(|row| row.unwrap())
    .collect()
}

fn metadata_map(conn: &Connection) -> BTreeMap<String, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM metadata ORDER BY key")
        .unwrap();
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .unwrap()
    .map(|row| row.unwrap())
    .filter(|(key, _)| key != "generated_at" && key != "registry_source")
    .collect()
}

#[test]
fn search_index_schema_queries_and_metadata_match_goldens() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    let sqlite_path = temp.path().join("search.sqlite");
    build_search_registry_fixture(&registry_dir);
    export_fixture(&registry_dir, &sqlite_path);

    let conn = open_readonly(&sqlite_path);
    assert_eq!(
        logical_schema(&conn).trim_end_matches('\n'),
        EXPECTED_SCHEMA.trim_end_matches('\n')
    );

    let queries = query_sections();
    assert_eq!(
        queries.keys().cloned().collect::<Vec<_>>(),
        vec![
            "capabilities",
            "exact_alias_lookup",
            "package_metadata",
            "serving_normalization_collision"
        ]
    );
    assert_eq!(
        query_rows(&conn, &queries["exact_alias_lookup"]),
        vec![vec![
            "FUND-0001".to_string(),
            "https://canon.example/id/FUND-0001".to_string(),
            "FUND_NAME".to_string(),
            "00-funds.json".to_string(),
        ]]
    );
    assert_eq!(
        query_rows(&conn, &queries["serving_normalization_collision"]),
        vec![vec!["ACME".to_string(), "2".to_string(), "2".to_string()]]
    );

    let package = compile_registry_package(&registry_dir).unwrap();
    assert_eq!(
        query_rows(&conn, &queries["package_metadata"]),
        vec![
            vec![
                "cache_policy".to_string(),
                "standalone_export_not_internal_cache".to_string()
            ],
            vec![
                "registry_package_digest".to_string(),
                package.content_digest.clone()
            ],
            vec![
                "registry_package_id".to_string(),
                "search-fixture".to_string()
            ],
            vec![
                "registry_package_schema_version".to_string(),
                "canon.registry.package.v1".to_string()
            ],
            vec![
                "registry_package_version".to_string(),
                "2026.07.11".to_string()
            ],
        ]
    );
    assert_eq!(
        query_rows(&conn, &queries["capabilities"]),
        vec![
            vec!["exact_alias_lookup".to_string(), "1".to_string()],
            vec!["mutable_internal_cache".to_string(), "0".to_string()],
            vec!["registry_package_trace".to_string(), "1".to_string()],
            vec!["standalone_export".to_string(), "1".to_string()],
        ]
    );
}

#[test]
fn search_index_opens_read_only_and_agrees_with_core_lookup_for_aliases() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    let sqlite_path = temp.path().join("search.sqlite");
    build_search_registry_fixture(&registry_dir);
    export_fixture(&registry_dir, &sqlite_path);

    let conn = open_readonly(&sqlite_path);
    let write_error = conn
        .execute(
            "INSERT INTO metadata (key, value) VALUES ('mutated', 'no')",
            [],
        )
        .unwrap_err();
    assert!(write_error.to_string().contains("readonly"));

    let registry = load_registry(&registry_dir).unwrap();
    assert_ne!(registry.db_path, sqlite_path);

    let search_aliases = alias_rows(&conn);
    let input_values = InputValues {
        values: search_aliases
            .keys()
            .cloned()
            .map(|alias| (alias, ()))
            .collect(),
        special: HashMap::new(),
        format: InputFormat::Csv,
        delimiter: Some(b','),
        source_hash: None,
        source_bytes: None,
    };
    let resolved = resolve_values(&registry, &input_values).unwrap();
    assert!(resolved.unresolved.is_empty());

    let core_aliases = resolved
        .mappings
        .into_iter()
        .map(|mapping| {
            (
                mapping.input,
                (
                    mapping.canonical_id,
                    mapping.canonical_type,
                    mapping.rule_id,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(core_aliases, search_aliases);
}

#[test]
fn search_index_logical_contract_is_stable_across_paths_and_internal_caches() {
    let left = tempdir().unwrap();
    let right = tempdir().unwrap();
    let left_registry = left.path().join("registry");
    let right_registry = right.path().join("registry");
    let left_sqlite = left.path().join("search.sqlite");
    let right_sqlite = right.path().join("search.sqlite");
    build_search_registry_fixture(&left_registry);
    build_search_registry_fixture(&right_registry);
    fs::write(
        right_registry.join("_index.sqlite"),
        b"internal cache bytes",
    )
    .unwrap();

    let left_output = export_registry(search_index_request(&left_registry, &left_sqlite)).unwrap();
    let right_output =
        export_registry(search_index_request(&right_registry, &right_sqlite)).unwrap();
    assert_eq!(left_output.content_hash, right_output.content_hash);

    let left_conn = open_readonly(&left_sqlite);
    let right_conn = open_readonly(&right_sqlite);
    assert_eq!(logical_schema(&left_conn), logical_schema(&right_conn));
    assert_eq!(alias_rows(&left_conn), alias_rows(&right_conn));
    assert_eq!(metadata_map(&left_conn), metadata_map(&right_conn));
}
