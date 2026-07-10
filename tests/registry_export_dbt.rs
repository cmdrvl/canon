use canon::registry::{
    RegistryExportFormat, RegistryExportRequest, compile_registry_package, export_registry,
};
use serde_json::json;
use std::{fs, path::Path};
use tempfile::tempdir;

const EXPECTED_SEED: &str = include_str!("fixtures/registry_export/dbt/expected_seed.csv");
const EXPECTED_SCHEMA: &str = include_str!("fixtures/registry_export/dbt/expected_schema.yml");
const EXPECTED_TEST: &str = include_str!("fixtures/registry_export/dbt/expected_tests.sql");

fn write_json(path: &Path, value: serde_json::Value) {
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn build_dbt_registry_fixture(registry_dir: &Path) {
    fs::create_dir_all(registry_dir).unwrap();
    write_json(
        &registry_dir.join("registry.json"),
        json!({
            "id": "funds",
            "version": "2026.07.10",
            "description": "dbt export golden fixture",
            "updated": "2026-07-10",
            "entry_count": 7,
            "canonical_iri_namespace": "https://canon.example/registry/funds/"
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
                "input": "ALPHA, II \"Quoted\"",
                "canonical_id": "FUND-0001",
                "canonical_type": "fund",
                "rule_id": "FUND_ALIAS"
            },
            {
                "input": "São Paulo Growth",
                "canonical_id": "FUND-0002",
                "canonical_type": "fund",
                "rule_id": "FUND_NAME"
            },
            {
                "input": "  CUSIP-123  ",
                "canonical_id": "FUND-0003",
                "canonical_type": "fund",
                "rule_id": "FUND_KEY"
            },
            {
                "input": "Beta Note",
                "canonical_id": "BOND-0001",
                "canonical_type": "bond",
                "rule_id": "BOND_NAME"
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
                "input": "Gamma Fund",
                "canonical_id": "FUND-0004",
                "canonical_type": "fund",
                "rule_id": "MANUAL_NAME"
            }
        ]),
    );
}

fn dbt_request(
    registry_dir: &Path,
    seed_path: &Path,
    schema_path: Option<&Path>,
    test_path: Option<&Path>,
) -> RegistryExportRequest {
    RegistryExportRequest {
        registry: registry_dir.to_path_buf(),
        format: RegistryExportFormat::DbtSeed,
        out: seed_path.to_path_buf(),
        namespace: Some("warehouse_funds".to_string()),
        source_files: vec![
            " 01-shadowed.json ".to_string(),
            "00-funds.json".to_string(),
            "00-funds.json".to_string(),
        ],
        canonical_types: vec!["fund".to_string(), "fund".to_string()],
        rule_id_prefixes: vec!["FUND_".to_string(), "FUND_".to_string()],
        canonical_iri_prefix: "https://canon.example/id/".to_string(),
        schema_out: schema_path.map(Path::to_path_buf),
        anti_collapse_test_out: test_path.map(Path::to_path_buf),
    }
}

#[test]
fn dbt_seed_export_matches_golden_scaffolds_and_package_trace() {
    let temp = tempdir().unwrap();
    let registry_dir = temp.path().join("registry");
    build_dbt_registry_fixture(&registry_dir);

    let seed_path = temp.path().join("canon_registry_seed.csv");
    let schema_path = temp.path().join("schema.yml");
    let test_path = temp
        .path()
        .join("assert_canon_registry_seed_no_collapse.sql");
    let output = export_registry(dbt_request(
        &registry_dir,
        &seed_path,
        Some(&schema_path),
        Some(&test_path),
    ))
    .unwrap();

    assert_eq!(output.summary.source_entry_count, 7);
    assert_eq!(output.summary.filtered_entry_count, 5);
    assert_eq!(output.summary.exported_alias_count, 4);
    assert_eq!(output.summary.exported_entity_count, 3);
    assert_eq!(output.summary.skipped_filter_count, 2);
    assert_eq!(output.summary.skipped_shadowed_count, 1);
    assert_eq!(
        output.filters.source_files,
        vec!["00-funds.json", "01-shadowed.json"]
    );
    assert_eq!(output.filters.canonical_types, vec!["fund"]);
    assert_eq!(output.filters.rule_id_prefixes, vec!["FUND_"]);
    assert!(output.content_hash.starts_with("blake3:"));

    let package = compile_registry_package(&registry_dir).unwrap();
    let seed = fs::read_to_string(&seed_path).unwrap();
    assert!(seed.contains(&package.content_digest));
    assert_eq!(seed, EXPECTED_SEED);
    assert_eq!(fs::read_to_string(&schema_path).unwrap(), EXPECTED_SCHEMA);
    assert_eq!(fs::read_to_string(&test_path).unwrap(), EXPECTED_TEST);
}

#[test]
fn dbt_seed_export_is_stable_across_paths_and_index_caches() {
    let left = tempdir().unwrap();
    let right = tempdir().unwrap();
    let left_registry = left.path().join("registry");
    let right_registry = right.path().join("registry");
    build_dbt_registry_fixture(&left_registry);
    build_dbt_registry_fixture(&right_registry);
    fs::write(right_registry.join("_index.sqlite"), b"derived cache").unwrap();

    let left_seed = left.path().join("canon_registry_seed.csv");
    let right_seed = right.path().join("canon_registry_seed.csv");
    let left_output = export_registry(dbt_request(&left_registry, &left_seed, None, None)).unwrap();
    let right_output =
        export_registry(dbt_request(&right_registry, &right_seed, None, None)).unwrap();

    assert_eq!(
        fs::read(&left_seed).unwrap(),
        fs::read(&right_seed).unwrap()
    );
    assert_eq!(left_output.content_hash, right_output.content_hash);
}
