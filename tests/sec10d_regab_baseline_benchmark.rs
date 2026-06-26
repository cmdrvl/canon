#![forbid(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const REQUIRED_BENCHMARKS: &[&str] = &[
    "REGAB-SRC-001",
    "REGAB-OBS-001",
    "REGAB-OBS-002",
    "REGAB-LOOKUP-001",
    "REGAB-APPLY-001",
    "REGAB-ENRICH-001",
    "REGAB-HIER-001",
    "REGAB-HIER-002",
    "REGAB-ALIAS-001",
    "REGAB-REVIEW-001",
    "REGAB-PREP-001",
    "REGAB-PERF-001",
    "REGAB-FIREWALL-001",
];

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
fn sec10d_regab_baseline_benchmark_manifest_names_behavioral_assertions() {
    let manifest = manifest();

    assert_eq!(
        manifest["schema_version"],
        "canon.entity.benchmark_manifest.v0"
    );
    assert_eq!(manifest["profile"], "regab_firm_identity");
    assert_eq!(
        manifest["identity_semantics"],
        "same_firm_or_reviewed_alias"
    );
    assert_eq!(
        manifest["source"]["zip_sha256"],
        "5766b83bb2e1bad3736b1d78fa7ea1433d929d1f3d936762fdfbdba7cc9bdf3b"
    );
    assert_eq!(manifest["source"]["artifact_root"], "org_canon_baseline/");
    assert_eq!(manifest["source"]["registry"]["id"], "firms");
    assert_eq!(manifest["source"]["registry"]["version"], "1.0.12");
    assert_eq!(manifest["expected_counts"]["mention_rows"], 127_991);
    assert_eq!(manifest["expected_counts"]["unique_surfaces"], 46);
    assert_eq!(manifest["expected_counts"]["unique_canonical_ids"], 31);
    assert_eq!(manifest["expected_counts"]["unresolved_mentions"], 0);

    let required = string_set(&manifest["required_benchmarks"]);
    assert_eq!(required, REQUIRED_BENCHMARKS.iter().copied().collect());

    let assertions = manifest["benchmark_assertions"]
        .as_object()
        .expect("benchmark assertions object");
    assert_eq!(
        assertions
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        required
    );
    for id in REQUIRED_BENCHMARKS {
        let entry = &manifest["benchmark_assertions"][*id];
        assert!(
            !entry["tier"].as_str().unwrap_or_default().trim().is_empty(),
            "{id} must name a tier"
        );
        assert!(
            !entry["failure_meaning"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .is_empty(),
            "{id} must explain failure meaning"
        );
        let behavior = entry["assertions"].as_array().expect("assertions array");
        assert!(!behavior.is_empty(), "{id} must assert behavior");
        assert!(
            behavior.iter().all(|assertion| {
                let text = assertion.as_str().unwrap_or_default();
                !text.trim().is_empty() && !text.contains("artifact exists")
            }),
            "{id} cannot pass on artifact existence alone"
        );
    }

    let source_files = manifest["source"]["files"]
        .as_array()
        .expect("source files");
    assert!(source_files.len() >= 9);
    for file in source_files {
        assert!(file["path"].as_str().is_some_and(|path| {
            path.starts_with("org_canon_baseline/") && !path.trim().is_empty()
        }));
        assert!(file["sha256"].as_str().is_some_and(|hash| hash.len() == 64));
        assert!(file["size_bytes"].as_u64().is_some_and(|size| size > 0));
    }
}

#[test]
fn sec10d_regab_baseline_benchmark_public_slice_covers_surfaces_and_ids() {
    let manifest = manifest();
    let root = public_slice_root();
    let slice = read_json(root.join("fixture_slice.json"));

    assert_eq!(slice["schema_version"], "canon.entity.fixture_slice.v0");
    assert_eq!(slice["source_zip_sha256"], manifest["source"]["zip_sha256"]);
    assert_eq!(
        slice["source_artifact_root"],
        manifest["source"]["artifact_root"]
    );
    assert_eq!(slice["selected_row_count"], 46);
    assert_eq!(slice["unique_surface_count"], 46);
    assert_eq!(slice["unique_canonical_id_count"], 31);
    assert_eq!(slice["registry"], manifest["source"]["registry"]);
    for id in [
        "REGAB-OBS-002",
        "REGAB-LOOKUP-001",
        "REGAB-APPLY-001",
        "REGAB-ENRICH-001",
        "REGAB-HIER-001",
        "REGAB-HIER-002",
    ] {
        assert!(string_set(&slice["benchmark_ids"]).contains(id));
    }

    for file in manifest["committed_fixture_slice"]["files"]
        .as_array()
        .expect("committed fixture file list")
    {
        let relative = file.as_str().expect("fixture file path");
        assert!(
            root.join(relative).exists(),
            "manifest-listed fixture file missing: {relative}"
        );
    }
    for file in slice["expected_files"].as_array().expect("slice file list") {
        let relative = file.as_str().expect("slice file path");
        assert!(
            root.join(relative).exists(),
            "slice-listed fixture file missing: {relative}"
        );
    }

    let expected_columns = string_vec(&manifest["input_contract"]["org_mentions_columns"]);
    let (input_headers, input_records) = csv_records(root.join("org_mentions_selected.csv"));
    assert_eq!(input_headers, expected_columns);
    assert_eq!(input_records.len(), 46);

    let (canon_headers, canon_records) = csv_records(root.join("org_mentions_selected.canon.csv"));
    let mut expected_canon_headers = expected_columns.clone();
    expected_canon_headers.push("org_canon_id".to_string());
    assert_eq!(canon_headers, expected_canon_headers);
    assert_eq!(canon_records.len(), input_records.len());

    let canon_rows = csv_maps(root.join("org_mentions_selected.canon.csv"));
    let surfaces = canon_rows
        .iter()
        .map(|row| row["org_name"].as_str())
        .collect::<BTreeSet<_>>();
    let canonical_ids = canon_rows
        .iter()
        .map(|row| row["org_canon_id"].as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(surfaces.len(), 46);
    assert_eq!(canonical_ids.len(), 31);
    assert!(!canonical_ids.contains(""));

    for row in csv_maps(root.join("org_mentions_selected.csv")) {
        let alias_surfaces: Value =
            serde_json::from_str(&row["alias_surfaces_json"]).expect("alias JSON parses");
        let mention_surfaces: Value =
            serde_json::from_str(&row["mention_surfaces_json"]).expect("mention JSON parses");
        assert!(alias_surfaces.as_array().is_some());
        assert!(mention_surfaces.as_array().is_some());
    }
}

#[test]
fn sec10d_regab_baseline_benchmark_lookup_rules_match_manifest() {
    let manifest = manifest();
    let root = public_slice_root();
    let lookup = read_json(root.join("org_lookup_expected.map.json"));

    assert_eq!(lookup["registry"]["id"], "firms");
    assert_eq!(lookup["registry"]["version"], "1.0.12");
    assert_eq!(lookup["summary"]["total"], 46);
    assert_eq!(lookup["summary"]["resolved"], 46);
    assert_eq!(lookup["summary"]["unresolved"], 0);
    assert!(lookup["unresolved"].as_array().unwrap().is_empty());

    let expected_by_surface = manifest_surface_mappings(&manifest);
    let lookup_by_surface = lookup_surface_mappings(&lookup);
    assert_eq!(lookup_by_surface.len(), 46);
    assert_eq!(
        lookup_by_surface.keys().collect::<BTreeSet<_>>(),
        expected_by_surface.keys().collect::<BTreeSet<_>>()
    );

    let mut observed_rule_counts = BTreeMap::<String, u64>::new();
    for (surface, observed) in &lookup_by_surface {
        let expected = expected_by_surface
            .get(surface)
            .unwrap_or_else(|| panic!("missing manifest surface {surface}"));
        assert_eq!(
            observed.canonical_id, expected.canonical_id,
            "canonical id for {surface}"
        );
        assert_eq!(observed.rule_id, expected.rule_id, "rule id for {surface}");
        *observed_rule_counts
            .entry(observed.rule_id.clone())
            .or_default() += 1;
    }
    assert_eq!(
        observed_rule_counts,
        object_u64_map(&manifest["rule_counts"])
    );

    let canon_by_surface = canon_by_surface(root.join("org_mentions_selected.canon.csv"));
    for (surface, observed) in &lookup_by_surface {
        assert_eq!(
            canon_by_surface.get(surface),
            Some(&observed.canonical_id),
            "selected canon CSV agrees with lookup map for {surface}"
        );
    }

    let full_distribution_total: u64 = manifest["canonical_id_counts"]
        .as_array()
        .expect("canonical id counts")
        .iter()
        .map(|entry| entry["mention_count"].as_u64().expect("mention count"))
        .sum();
    assert_eq!(
        full_distribution_total,
        manifest["expected_counts"]["mention_rows"]
            .as_u64()
            .expect("mention rows")
    );
}

#[test]
fn sec10d_regab_baseline_benchmark_outputs_are_append_only() {
    let manifest = manifest();
    let root = public_slice_root();
    let expected_columns = string_vec(&manifest["input_contract"]["org_mentions_columns"]);
    let (input_headers, input_records) = csv_records(root.join("org_mentions_selected.csv"));
    let (canon_headers, canon_records) = csv_records(root.join("org_mentions_selected.canon.csv"));

    assert_eq!(input_headers, expected_columns);
    assert_eq!(
        &canon_headers[..input_headers.len()],
        input_headers.as_slice()
    );
    assert_eq!(
        &canon_headers[input_headers.len()..],
        ["org_canon_id".to_string()]
    );
    assert_eq!(canon_records.len(), input_records.len());
    for (input, canon) in input_records.iter().zip(&canon_records) {
        assert_eq!(canon.len(), input.len() + 1);
        for index in 0..input.len() {
            assert_eq!(canon.get(index), input.get(index));
        }
        assert!(!canon.get(input.len()).unwrap_or_default().is_empty());
    }

    let approved_suffixes = string_vec(&manifest["input_contract"]["approved_enriched_suffixes"]);
    let slice = read_json(root.join("fixture_slice.json"));
    let expected_sample_counts = slice["enriched_sample_record_counts"]
        .as_object()
        .expect("sample counts");
    for (dataset, config) in manifest["enriched_datasets"]
        .as_object()
        .expect("enriched datasets")
    {
        let path = root
            .join("enriched_samples")
            .join(format!("{dataset}.selected.jsonl"));
        let lines = fs::read_to_string(&path).expect("enriched sample opens");
        let mut row_count = 0_u64;
        for line in lines.lines().filter(|line| !line.trim().is_empty()) {
            row_count += 1;
            let value: Value = serde_json::from_str(line).expect("enriched sample line parses");
            let object = value.as_object().expect("enriched sample object");
            for raw_key in ["record_id", "evidence", "filing", "version"] {
                assert!(
                    object.contains_key(raw_key),
                    "{dataset} sample missing parser field {raw_key}"
                );
            }

            let mut observed_prefixes = BTreeSet::new();
            for key in object.keys().filter(|key| key.contains("_org_")) {
                assert!(
                    approved_suffixes
                        .iter()
                        .any(|suffix| key.ends_with(suffix.as_str())),
                    "{dataset} has unexpected canonical field {key}"
                );
                if let Some(prefix) = key.strip_suffix("_org_canon_id") {
                    observed_prefixes.insert(prefix.to_string());
                }
            }
            assert!(
                !observed_prefixes.is_empty(),
                "{dataset} sample must include canonical firm fields"
            );
            for prefix in observed_prefixes {
                for suffix in &approved_suffixes {
                    assert!(
                        object.contains_key(&format!("{prefix}{suffix}")),
                        "{dataset} missing {prefix}{suffix}"
                    );
                }
            }
        }
        assert_eq!(
            row_count,
            expected_sample_counts[dataset]
                .as_u64()
                .expect("expected sample count"),
            "{dataset} sample count"
        );
        assert!(
            !config["canonical_field_prefixes"]
                .as_array()
                .expect("canonical field prefixes")
                .is_empty(),
            "{dataset} must name canonical field prefixes"
        );
    }
}

#[test]
fn sec10d_regab_baseline_benchmark_hierarchy_review_and_perf_guards_are_explicit() {
    let manifest = manifest();
    let root = public_slice_root();
    let surface_mappings = manifest_surface_mappings(&manifest);

    for pair in manifest["anti_collapse_pairs"]
        .as_array()
        .expect("anti collapse pairs")
    {
        let left_surface = pair["left_surface"].as_str().expect("left surface");
        let right_surface = pair["right_surface"].as_str().expect("right surface");
        let left = surface_mappings
            .get(left_surface)
            .unwrap_or_else(|| panic!("missing left guard surface {left_surface}"));
        let right = surface_mappings
            .get(right_surface)
            .unwrap_or_else(|| panic!("missing right guard surface {right_surface}"));
        assert_eq!(left.canonical_id, pair["left_canonical_id"]);
        assert_eq!(right.canonical_id, pair["right_canonical_id"]);
        match pair["expected"].as_str().expect("expected guard behavior") {
            "must_remain_distinct_or_relation_hint" => {
                assert_ne!(left.canonical_id, right.canonical_id);
                assert!(
                    pair["reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains("division")
                            || reason.contains("affiliate")
                            || reason.contains("bank"))
                );
            }
            "same_only_with_reviewed_policy_rule" => {
                assert_eq!(left.canonical_id, right.canonical_id);
                assert!(
                    right.rule_id.contains("REVIEWED")
                        || right.rule_id.contains("SUBSIDIARIES")
                        || right.rule_id.contains("DIVISION")
                );
            }
            other => panic!("unsupported guard behavior {other}"),
        }
    }

    let review_queue =
        fs::read_to_string(root.join("org_review_queue.csv")).expect("review queue fixture opens");
    let review_lines = review_queue.lines().collect::<Vec<_>>();
    assert_eq!(
        review_lines.len(),
        1,
        "resolved baseline review queue is header-only"
    );
    assert!(review_lines[0].contains("org_name"));
    assert!(review_lines[0].contains("review_decision"));

    let rows = manifest["expected_counts"]["mention_rows"]
        .as_u64()
        .expect("mention rows");
    let unique_surfaces = manifest["expected_counts"]["unique_surfaces"]
        .as_u64()
        .expect("unique surfaces");
    let raw_all_pairs = u128::from(rows) * u128::from(rows - 1) / 2;
    let unique_all_pairs = u128::from(unique_surfaces) * u128::from(unique_surfaces - 1) / 2;
    assert_eq!(unique_surfaces, 46);
    assert!(
        raw_all_pairs > unique_all_pairs * 1_000_000,
        "benchmark must prove dedupe-first structure, not row-level all-pairs work"
    );
    assert_eq!(
        manifest["benchmark_assertions"]["REGAB-PERF-001"]["assertions"][0],
        "no all-pairs generation over 127991 mention rows"
    );

    let non_goals = string_set(&manifest["non_goals"]);
    for required in [
        "do not introduce frontier model calls",
        "do not introduce network access",
        "do not introduce Python or general ML framework runtime dependencies",
        "do not silently collapse parent/subsidiary/division relationships",
        "do not mutate raw sec10d parser evidence",
    ] {
        assert!(non_goals.contains(required));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SurfaceMapping {
    canonical_id: String,
    rule_id: String,
}

fn manifest() -> Value {
    read_json(fixture_root().join("sec10d_regab_benchmark_manifest.json"))
}

fn csv_records(path: impl AsRef<Path>) -> (Vec<String>, Vec<csv::StringRecord>) {
    let mut reader = csv::Reader::from_path(path).expect("csv opens");
    let headers = reader
        .headers()
        .expect("csv headers")
        .iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv records parse");
    (headers, records)
}

fn csv_maps(path: impl AsRef<Path>) -> Vec<BTreeMap<String, String>> {
    csv::Reader::from_path(path)
        .expect("csv opens")
        .deserialize::<BTreeMap<String, String>>()
        .collect::<Result<Vec<_>, _>>()
        .expect("csv rows deserialize")
}

fn string_vec(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item").to_string())
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string item"))
        .collect()
}

fn object_u64_map(value: &Value) -> BTreeMap<String, u64> {
    value
        .as_object()
        .expect("u64 object")
        .iter()
        .map(|(key, value)| (key.clone(), value.as_u64().expect("u64 value")))
        .collect()
}

fn manifest_surface_mappings(manifest: &Value) -> BTreeMap<String, SurfaceMapping> {
    manifest["surface_mappings"]
        .as_array()
        .expect("surface mappings")
        .iter()
        .map(|mapping| {
            (
                mapping["surface"].as_str().expect("surface").to_string(),
                SurfaceMapping {
                    canonical_id: mapping["canonical_id"]
                        .as_str()
                        .expect("canonical id")
                        .to_string(),
                    rule_id: mapping["rule_id"].as_str().expect("rule id").to_string(),
                },
            )
        })
        .collect()
}

fn lookup_surface_mappings(lookup: &Value) -> BTreeMap<String, SurfaceMapping> {
    lookup["mappings"]
        .as_array()
        .expect("lookup mappings")
        .iter()
        .map(|mapping| {
            (
                strip_u8(mapping["input"].as_str().expect("input")).to_string(),
                SurfaceMapping {
                    canonical_id: strip_u8(mapping["canonical_id"].as_str().expect("canonical id"))
                        .to_string(),
                    rule_id: mapping["rule_id"].as_str().expect("rule id").to_string(),
                },
            )
        })
        .collect()
}

fn canon_by_surface(path: impl AsRef<Path>) -> BTreeMap<String, String> {
    csv_maps(path)
        .into_iter()
        .map(|row| (row["org_name"].clone(), row["org_canon_id"].clone()))
        .collect()
}
