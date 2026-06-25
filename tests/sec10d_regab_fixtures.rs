use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entity/regab")
}

fn public_slice_root() -> PathBuf {
    fixture_root().join("sec10d_baseline_public")
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("json fixture opens"))
        .expect("json fixture parses")
}

fn strip_u8(value: &str) -> &str {
    value.strip_prefix("u8:").unwrap_or(value)
}

#[test]
fn sec10d_regab_manifest_and_public_slice_parse() {
    let manifest = read_json(fixture_root().join("sec10d_regab_benchmark_manifest.json"));
    assert_eq!(manifest["profile"], "regab_firm_identity");
    assert_eq!(manifest["expected_counts"]["mention_rows"], 127_991);
    assert_eq!(manifest["expected_counts"]["unique_surfaces"], 46);
    assert_eq!(manifest["expected_counts"]["unique_canonical_ids"], 31);
    assert_eq!(manifest["expected_counts"]["unresolved_mentions"], 0);
    assert_eq!(manifest["source"]["registry"]["id"], "firms");
    assert_eq!(manifest["source"]["registry"]["version"], "1.0.12");

    let required = manifest["required_benchmarks"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "REGAB-OBS-002"));
    assert!(required.iter().any(|v| v == "REGAB-HIER-001"));
    assert!(required.iter().any(|v| v == "REGAB-ENRICH-001"));

    let slice = read_json(public_slice_root().join("fixture_slice.json"));
    assert_eq!(slice["selected_surface_rows"], 46);
    assert_eq!(slice["selected_unique_surfaces"], 46);
    assert_eq!(slice["selected_unique_canonical_ids"], 31);
    assert_eq!(
        slice["source_zip_sha256"], manifest["source"]["zip_sha256"],
        "slice metadata tracks the same source artifact"
    );
}

#[test]
fn sec10d_regab_selected_mentions_cover_every_surface_and_canonical_id() {
    let manifest = read_json(fixture_root().join("sec10d_regab_benchmark_manifest.json"));
    let expected_columns: Vec<String> = manifest["input_contract"]["org_mentions_columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    let root = public_slice_root();
    let mut input_reader =
        csv::Reader::from_path(root.join("org_mentions_selected.csv")).expect("input csv");
    let input_headers: Vec<String> = input_reader
        .headers()
        .unwrap()
        .iter()
        .map(str::to_string)
        .collect();
    assert_eq!(input_headers, expected_columns);

    let input_rows = input_reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("input rows parse");
    assert_eq!(input_rows.len(), 46);

    let mut canon_reader =
        csv::Reader::from_path(root.join("org_mentions_selected.canon.csv")).expect("canon csv");
    let mut expected_canon_headers = expected_columns.clone();
    expected_canon_headers.push("org_canon_id".to_string());
    let canon_headers: Vec<String> = canon_reader
        .headers()
        .unwrap()
        .iter()
        .map(str::to_string)
        .collect();
    assert_eq!(canon_headers, expected_canon_headers);

    let canon_rows = canon_reader
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("canon rows parse");
    assert_eq!(canon_rows.len(), 46);

    let surfaces: BTreeSet<_> = canon_rows
        .iter()
        .map(|row| row["org_name"].to_string())
        .collect();
    let canonical_ids: BTreeSet<_> = canon_rows
        .iter()
        .map(|row| row["org_canon_id"].to_string())
        .collect();

    assert_eq!(surfaces.len(), 46);
    assert_eq!(canonical_ids.len(), 31);
    assert!(!canonical_ids.contains(""));
}

#[test]
fn sec10d_regab_lookup_fixture_matches_selected_canon_rows() {
    let root = public_slice_root();
    let lookup = read_json(root.join("org_lookup_expected.map.json"));
    assert_eq!(lookup["registry"]["id"], "firms");
    assert_eq!(lookup["registry"]["version"], "1.0.12");
    assert_eq!(lookup["summary"]["total"], 46);
    assert_eq!(lookup["summary"]["resolved"], 46);
    assert_eq!(lookup["summary"]["unresolved"], 0);
    assert_eq!(lookup["mappings"].as_array().unwrap().len(), 46);

    let mut expected_by_surface = BTreeMap::new();
    for mapping in lookup["mappings"].as_array().unwrap() {
        expected_by_surface.insert(
            strip_u8(mapping["input"].as_str().unwrap()).to_string(),
            strip_u8(mapping["canonical_id"].as_str().unwrap()).to_string(),
        );
    }

    let mut canon_reader =
        csv::Reader::from_path(root.join("org_mentions_selected.canon.csv")).expect("canon csv");
    for row in canon_reader.deserialize::<BTreeMap<String, String>>() {
        let row = row.expect("canon row");
        assert_eq!(
            expected_by_surface.get(&row["org_name"]),
            Some(&row["org_canon_id"]),
            "selected row matches lookup fixture for {}",
            row["org_name"]
        );
    }
}

#[test]
fn sec10d_regab_hierarchy_antimerge_cases_are_encoded() {
    let root = public_slice_root();
    let mut canon_by_surface = BTreeMap::new();
    let mut reader =
        csv::Reader::from_path(root.join("org_mentions_selected.canon.csv")).expect("canon csv");
    for row in reader.deserialize::<BTreeMap<String, String>>() {
        let row = row.expect("canon row");
        canon_by_surface.insert(row["org_name"].clone(), row["org_canon_id"].clone());
    }

    assert_eq!(
        canon_by_surface["PNC Bank, National Association"],
        "ORG-034"
    );
    assert_eq!(
        canon_by_surface["Midland Loan Services, a division of PNC Bank, National Association"],
        "ORG-035"
    );
    assert_ne!(
        canon_by_surface["PNC Bank, National Association"],
        canon_by_surface["Midland Loan Services, a division of PNC Bank, National Association"]
    );

    assert_eq!(
        canon_by_surface["Wells Fargo Bank, National Association"],
        "ORG-012"
    );
    assert_eq!(
        canon_by_surface["Wells Fargo Commercial Mortgage Servicing, a division of Wells Fargo Bank, National Association"],
        "ORG-053"
    );
    assert_ne!(
        canon_by_surface["Wells Fargo Bank, National Association"],
        canon_by_surface["Wells Fargo Commercial Mortgage Servicing, a division of Wells Fargo Bank, National Association"]
    );
}

#[test]
fn sec10d_regab_enriched_samples_are_append_only_shape_fixtures() {
    let root = public_slice_root();
    let slice = read_json(root.join("fixture_slice.json"));
    let expected_counts = slice["enriched_sample_record_counts"].as_object().unwrap();
    let allowed_suffixes = [
        "_org_canon_id",
        "_org_canonical_name",
        "_org_resolution_status",
        "_org_registry_id",
        "_org_registry_version",
        "_org_rule_id",
    ];

    for (dataset, expected_count) in expected_counts {
        let path = root
            .join("enriched_samples")
            .join(format!("{dataset}.selected.jsonl"));
        let content = fs::read_to_string(&path).expect("enriched jsonl opens");
        let mut count = 0usize;
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let value: Value = serde_json::from_str(line).expect("enriched line parses");
            let object = value.as_object().expect("enriched line object");
            assert!(object.contains_key("record_id"));
            let org_fields: Vec<_> = object.keys().filter(|key| key.contains("_org_")).collect();
            assert!(
                !org_fields.is_empty(),
                "{dataset} sample exposes canonical org fields"
            );
            for key in org_fields {
                assert!(
                    allowed_suffixes.iter().any(|suffix| key.ends_with(suffix)),
                    "{dataset} has unexpected canonical field {key}"
                );
            }
            count += 1;
        }
        assert_eq!(count as u64, expected_count.as_u64().unwrap(), "{dataset}");
    }
}

#[test]
fn sec10d_regab_registry_snapshot_fixture_matches_baseline_metadata() {
    let registry = read_json(public_slice_root().join("registry_snapshot/firms/registry.json"));
    assert_eq!(registry["id"], "firms");
    assert_eq!(registry["version"], "1.0.12");
    assert_eq!(registry["entry_count"], 110);

    let seed =
        read_json(public_slice_root().join("registry_snapshot/firms/regab-org-seed-20260623.json"));
    assert_eq!(seed.as_array().unwrap().len(), 42);
}
