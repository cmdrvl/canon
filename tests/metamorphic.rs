#![forbid(unsafe_code)]

use assert_cmd::Command;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

const SEEDS: [u64; 3] = [7, 11, 19];
const COLUMN: &str = "cusip";

#[derive(Clone, Debug, Serialize)]
struct RegistryEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug)]
struct MetamorphicCase {
    _tempdir: TempDir,
    home_dir: PathBuf,
    registry_path: PathBuf,
    csv_path: PathBuf,
    jsonl_path: PathBuf,
    delimiter: u8,
    csv_rows: Vec<String>,
    jsonl_rows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedOutput {
    mappings: BTreeMap<String, (String, String, String)>,
    unresolved_values: BTreeSet<String>,
    unresolved_special: BTreeSet<String>,
}

#[test]
fn csv_permutations_preserve_exact_lookup_semantics() {
    for seed in SEEDS {
        let case = build_case(seed);
        let (_, original, original_status) =
            run_json_mode(&case.csv_path, &case.registry_path, &case.home_dir, None);

        let rotated = permute_rows(&case.csv_rows, seed);
        let rotated_path = case
            ._tempdir
            .path()
            .join(format!("input-permuted-{seed}.csv"));
        write_csv_input(&rotated_path, case.delimiter, &rotated);
        let (_, permuted, permuted_status) =
            run_json_mode(&rotated_path, &case.registry_path, &case.home_dir, None);

        assert_eq!(original_status, permuted_status, "seed {}", seed);
        assert_eq!(
            normalize_output(&original),
            normalize_output(&permuted),
            "seed {}",
            seed
        );
        assert_eq!(original["summary"], permuted["summary"], "seed {}", seed);
    }
}

#[test]
fn jsonl_permutations_preserve_exact_lookup_semantics() {
    for seed in SEEDS {
        let case = build_case(seed);
        let (_, original, original_status) =
            run_json_mode(&case.jsonl_path, &case.registry_path, &case.home_dir, None);

        let rotated = permute_rows(&case.jsonl_rows, seed);
        let rotated_path = case
            ._tempdir
            .path()
            .join(format!("input-permuted-{seed}.jsonl"));
        write_jsonl_input(&rotated_path, &rotated);
        let (_, permuted, permuted_status) =
            run_json_mode(&rotated_path, &case.registry_path, &case.home_dir, None);

        assert_eq!(original_status, permuted_status, "seed {}", seed);
        assert_eq!(
            normalize_output(&original),
            normalize_output(&permuted),
            "seed {}",
            seed
        );
        assert_eq!(original["summary"], permuted["summary"], "seed {}", seed);
    }
}

#[test]
fn csv_shard_merge_matches_whole_input() {
    for seed in SEEDS {
        let case = build_case(seed);
        let (_, full, _) = run_json_mode(&case.csv_path, &case.registry_path, &case.home_dir, None);

        let (left_rows, right_rows) = split_rows(&case.csv_rows);
        let left_path = case._tempdir.path().join(format!("input-left-{seed}.csv"));
        let right_path = case._tempdir.path().join(format!("input-right-{seed}.csv"));
        write_csv_input(&left_path, case.delimiter, &left_rows);
        write_csv_input(&right_path, case.delimiter, &right_rows);

        let (_, left, _) = run_json_mode(&left_path, &case.registry_path, &case.home_dir, None);
        let (_, right, _) = run_json_mode(&right_path, &case.registry_path, &case.home_dir, None);

        assert_eq!(
            normalize_output(&full),
            merge_outputs(&[left, right]),
            "seed {}",
            seed
        );
    }
}

#[test]
fn jsonl_shard_merge_matches_whole_input() {
    for seed in SEEDS {
        let case = build_case(seed);
        let (_, full, _) =
            run_json_mode(&case.jsonl_path, &case.registry_path, &case.home_dir, None);

        let (left_rows, right_rows) = split_rows(&case.jsonl_rows);
        let left_path = case
            ._tempdir
            .path()
            .join(format!("input-left-{seed}.jsonl"));
        let right_path = case
            ._tempdir
            .path()
            .join(format!("input-right-{seed}.jsonl"));
        write_jsonl_input(&left_path, &left_rows);
        write_jsonl_input(&right_path, &right_rows);

        let (_, left, _) = run_json_mode(&left_path, &case.registry_path, &case.home_dir, None);
        let (_, right, _) = run_json_mode(&right_path, &case.registry_path, &case.home_dir, None);

        assert_eq!(
            normalize_output(&full),
            merge_outputs(&[left, right]),
            "seed {}",
            seed
        );
    }
}

#[test]
fn exact_lookup_backends_agree_between_managed_and_no_cache_modes() {
    for seed in SEEDS {
        let case = build_case(seed);
        let (managed_bytes, managed_json, managed_status) =
            run_json_mode(&case.csv_path, &case.registry_path, &case.home_dir, None);
        let (no_cache_bytes, no_cache_json, no_cache_status) = run_json_mode(
            &case.csv_path,
            &case.registry_path,
            &case.home_dir,
            Some("no-cache"),
        );

        assert_eq!(managed_status, no_cache_status, "seed {}", seed);
        assert_eq!(managed_bytes, no_cache_bytes, "seed {}", seed);
        assert_eq!(managed_json, no_cache_json, "seed {}", seed);
        assert!(
            !case.registry_path.join("_index.sqlite").exists(),
            "seed {}",
            seed
        );
    }
}

fn build_case(seed: u64) -> MetamorphicCase {
    let tempdir = TempDir::new().expect("tempdir");
    let home_dir = tempdir.path().join("home");
    fs::create_dir_all(&home_dir).expect("home dir");

    let registry_path = tempdir.path().join("registry");
    fs::create_dir_all(&registry_path).expect("registry dir");

    let entries = vec![
        RegistryEntry {
            input: format!("ID{seed}_A"),
            canonical_id: format!("CANON{seed}_A"),
            canonical_type: "ticker".to_string(),
            rule_id: "RULE_A".to_string(),
        },
        RegistryEntry {
            input: format!("ID{seed}_B"),
            canonical_id: format!("CANON{seed}_B"),
            canonical_type: "isin".to_string(),
            rule_id: "RULE_B".to_string(),
        },
        RegistryEntry {
            input: format!("ID{seed}_C"),
            canonical_id: format!("CANON{seed}_C"),
            canonical_type: "lei".to_string(),
            rule_id: "RULE_C".to_string(),
        },
    ];
    let shadow = vec![
        RegistryEntry {
            input: entries[0].input.clone(),
            canonical_id: format!("SHADOW{seed}"),
            canonical_type: "ticker".to_string(),
            rule_id: "SHADOW_RULE".to_string(),
        },
        RegistryEntry {
            input: format!("ALIAS{seed}"),
            canonical_id: format!("ALIAS_CANON{seed}"),
            canonical_type: "ticker".to_string(),
            rule_id: "ALIAS_RULE".to_string(),
        },
    ];
    write_registry(&registry_path, &entries, &shadow);

    let delimiter = match seed % 3 {
        0 => b',',
        1 => b'\t',
        _ => b'|',
    };
    let csv_rows = vec![
        format!("{}{}resolved-a", entries[0].input, delimiter as char),
        format!("UNKNOWN_{seed}{}unknown", delimiter as char),
        format!("  {}  {}trimmed", entries[1].input, delimiter as char),
        format!("{}empty-first-field", delimiter as char),
        format!("{}{}", delimiter as char, ""),
        format!("{}{}duplicate", entries[0].input, delimiter as char),
        format!("ALIAS{seed}{}alias-hit", delimiter as char),
    ];
    let csv_path = tempdir.path().join(match delimiter {
        b'\t' => "input.tsv",
        _ => "input.csv",
    });
    write_csv_input(&csv_path, delimiter, &csv_rows);

    let jsonl_rows = vec![
        format!(r#"{{"{COLUMN}":"{}","idx":1}}"#, entries[0].input),
        format!(r#"{{"{COLUMN}":"UNKNOWN_JSONL_{seed}"}}"#),
        format!(r#"{{"{COLUMN}":"  {}  "}}"#, entries[2].input),
        r#"{"other":"missing"}"#.to_string(),
        format!(r#"{{"{COLUMN}":null}}"#),
        format!(r#"{{"{COLUMN}":{{"nested":true}}}}"#),
        format!(r#"{{"{COLUMN}":[1,2,3]}}"#),
        format!(r#"{{"{COLUMN}":true}}"#),
        format!(r#"{{"{COLUMN}":"ALIAS{seed}"}}"#),
    ];
    let jsonl_path = tempdir.path().join("input.jsonl");
    write_jsonl_input(&jsonl_path, &jsonl_rows);

    MetamorphicCase {
        _tempdir: tempdir,
        home_dir,
        registry_path,
        csv_path,
        jsonl_path,
        delimiter,
        csv_rows,
        jsonl_rows,
    }
}

fn write_registry(registry_path: &Path, primary: &[RegistryEntry], secondary: &[RegistryEntry]) {
    let registry_json = serde_json::json!({
        "id": "metamorphic-registry",
        "version": "1.0.0",
        "description": "metamorphic test registry",
        "updated": "2026-07-10",
        "entry_count": primary.len() + secondary.len()
    });
    fs::write(
        registry_path.join("registry.json"),
        serde_json::to_vec_pretty(&registry_json).expect("registry json"),
    )
    .expect("write registry");
    fs::write(
        registry_path.join("a-primary.json"),
        serde_json::to_vec_pretty(primary).expect("primary json"),
    )
    .expect("write primary");
    fs::write(
        registry_path.join("z-secondary.json"),
        serde_json::to_vec_pretty(secondary).expect("secondary json"),
    )
    .expect("write secondary");
}

fn write_csv_input(path: &Path, delimiter: u8, rows: &[String]) {
    let sep = delimiter as char;
    let mut lines = vec![format!("{COLUMN}{sep}note")];
    lines.extend(rows.iter().cloned());
    fs::write(path, lines.join("\n") + "\n").expect("write csv");
}

fn write_jsonl_input(path: &Path, rows: &[String]) {
    fs::write(path, rows.join("\n") + "\n").expect("write jsonl");
}

fn run_json_mode(
    input_path: &Path,
    registry_path: &Path,
    home_dir: &Path,
    index_mode: Option<&str>,
) -> (Vec<u8>, Value, i32) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_canon"));
    command.args([
        input_path.to_str().expect("input path"),
        "--registry",
        registry_path.to_str().expect("registry path"),
        "--column",
        COLUMN,
        "--emit",
        "json",
        "--explicit",
        "--no-witness",
    ]);
    command.env("HOME", home_dir);
    command.env("USERPROFILE", home_dir);
    if let Some(index_mode) = index_mode {
        command.env("CANON_REGISTRY_INDEX_MODE", index_mode);
    }
    let output = command.output().expect("canon output");
    let status = output.status.code().unwrap_or(2);
    assert_ne!(
        status,
        2,
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json payload");
    (output.stdout, payload, status)
}

fn permute_rows(rows: &[String], seed: u64) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let offset = (seed as usize) % rows.len();
    let mut rotated = rows[offset..].to_vec();
    rotated.extend_from_slice(&rows[..offset]);
    if seed.is_multiple_of(2) {
        rotated.reverse();
    }
    rotated
}

fn split_rows(rows: &[String]) -> (Vec<String>, Vec<String>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index % 2 == 0 {
            left.push(row.clone());
        } else {
            right.push(row.clone());
        }
    }
    (left, right)
}

fn normalize_output(payload: &Value) -> NormalizedOutput {
    let mappings = payload["mappings"]
        .as_array()
        .expect("mappings array")
        .iter()
        .map(|entry| {
            (
                decode_identifier(entry["input"].as_str().expect("mapping input")),
                (
                    decode_identifier(
                        entry["canonical_id"]
                            .as_str()
                            .expect("mapping canonical id"),
                    ),
                    entry["canonical_type"]
                        .as_str()
                        .expect("mapping canonical type")
                        .to_string(),
                    entry["rule_id"]
                        .as_str()
                        .expect("mapping rule id")
                        .to_string(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let unresolved = payload["unresolved"].as_array().expect("unresolved array");
    let unresolved_values = unresolved
        .iter()
        .filter_map(|entry| entry["input"].as_str().map(decode_identifier))
        .collect::<BTreeSet<_>>();
    let unresolved_special = unresolved
        .iter()
        .filter(|entry| entry["input"].is_null())
        .map(|entry| {
            entry["reason"]
                .as_str()
                .expect("special reason")
                .to_string()
        })
        .collect::<BTreeSet<_>>();

    NormalizedOutput {
        mappings,
        unresolved_values,
        unresolved_special,
    }
}

fn merge_outputs(payloads: &[Value]) -> NormalizedOutput {
    let mut merged = NormalizedOutput {
        mappings: BTreeMap::new(),
        unresolved_values: BTreeSet::new(),
        unresolved_special: BTreeSet::new(),
    };

    for payload in payloads {
        let normalized = normalize_output(payload);
        for (input, mapping) in normalized.mappings {
            merged.mappings.entry(input).or_insert(mapping);
        }
        merged
            .unresolved_values
            .extend(normalized.unresolved_values);
        merged
            .unresolved_special
            .extend(normalized.unresolved_special);
    }

    merged
}

fn decode_identifier(encoded: &str) -> String {
    if let Some(value) = encoded.strip_prefix("u8:") {
        return value.to_string();
    }
    if let Some(hex) = encoded.strip_prefix("hex:") {
        return hex_to_string_lossy(hex);
    }
    encoded.to_string()
}

fn hex_to_string_lossy(hex: &str) -> String {
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut index = 0usize;
    while index + 1 < hex.len() {
        let value = u8::from_str_radix(&hex[index..index + 2], 16).expect("hex byte");
        bytes.push(value);
        index += 2;
    }
    String::from_utf8_lossy(&bytes).to_string()
}
