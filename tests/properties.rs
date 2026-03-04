use assert_cmd::Command;
use csv::{ReaderBuilder, StringRecord};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 6];
const COLUMN: &str = "cusip";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct RegistryEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug)]
struct CsvCase {
    _tempdir: TempDir,
    registry_path: PathBuf,
    input_path: PathBuf,
    delimiter: u8,
    all_entries: Vec<RegistryEntry>,
}

#[derive(Debug)]
struct JsonlCase {
    _tempdir: TempDir,
    registry_path: PathBuf,
    input_path: PathBuf,
}

#[derive(Debug, Default)]
struct ObservedInputs {
    values: HashSet<String>,
    special_reasons: HashSet<String>,
}

#[test]
fn invariant_completeness() {
    for seed in SEEDS {
        let case = build_csv_case(seed);
        let (_bytes, json) = run_json_mode(&case.input_path, &case.registry_path);

        let summary = &json["summary"];
        let total = summary["total"].as_u64().unwrap();
        let resolved = summary["resolved"].as_u64().unwrap();
        let unresolved = summary["unresolved"].as_u64().unwrap();

        assert_eq!(total, resolved + unresolved, "seed {}", seed);
    }
}

#[test]
fn invariant_determinism() {
    for seed in SEEDS {
        let case = build_csv_case(seed);
        let (run1, _) = run_json_mode(&case.input_path, &case.registry_path);
        let (run2, _) = run_json_mode(&case.input_path, &case.registry_path);

        assert_eq!(run1, run2, "seed {}", seed);
    }
}

#[test]
fn invariant_partition() {
    for seed in SEEDS {
        let case = build_csv_case(seed);
        let (_bytes, json) = run_json_mode(&case.input_path, &case.registry_path);
        let observed = observe_csv_inputs(&case.input_path, case.delimiter);

        let mapped_inputs: HashSet<String> = json["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| decode_identifier(entry["input"].as_str().unwrap()))
            .collect();

        let unresolved_non_null_inputs: HashSet<String> = json["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["input"].as_str().map(decode_identifier))
            .collect();

        assert!(
            mapped_inputs.is_disjoint(&unresolved_non_null_inputs),
            "seed {}",
            seed
        );
        let union: HashSet<String> = mapped_inputs
            .union(&unresolved_non_null_inputs)
            .cloned()
            .collect();
        assert_eq!(union, observed.values, "seed {}", seed);

        let unresolved_special: HashSet<String> = json["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["input"].is_null())
            .map(|entry| entry["reason"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            unresolved_special, observed.special_reasons,
            "seed {}",
            seed
        );

        let summary_total = json["summary"]["total"].as_u64().unwrap() as usize;
        let expected_total = observed.values.len() + observed.special_reasons.len();
        assert_eq!(summary_total, expected_total, "seed {}", seed);
    }
}

#[test]
fn invariant_traceability() {
    for seed in SEEDS {
        let case = build_csv_case(seed);
        let (_bytes, json) = run_json_mode(&case.input_path, &case.registry_path);

        for mapping in json["mappings"].as_array().unwrap() {
            let raw_input = decode_identifier(mapping["input"].as_str().unwrap());
            let raw_canonical = decode_identifier(mapping["canonical_id"].as_str().unwrap());
            let canonical_type = mapping["canonical_type"].as_str().unwrap();
            let rule_id = mapping["rule_id"].as_str().unwrap();

            assert!(
                case.all_entries.iter().any(|entry| {
                    entry.input == raw_input
                        && entry.canonical_id == raw_canonical
                        && entry.canonical_type == canonical_type
                        && entry.rule_id == rule_id
                }),
                "seed {} mapping not traceable",
                seed
            );
        }
    }
}

#[test]
fn invariant_cross_mode_consistency() {
    for seed in SEEDS {
        let case = build_csv_case(seed);
        let (_json_bytes, json_payload) = run_json_mode(&case.input_path, &case.registry_path);
        let (csv_stdout, csv_status) = run_csv_mode(&case.input_path, &case.registry_path);

        let json_map: HashMap<String, String> = json_payload["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    decode_identifier(entry["input"].as_str().unwrap()),
                    decode_identifier(entry["canonical_id"].as_str().unwrap()),
                )
            })
            .collect();

        let mut reader = ReaderBuilder::new()
            .delimiter(case.delimiter)
            .from_reader(csv_stdout.as_slice());
        let headers = reader.headers().unwrap().clone();
        let canon_idx = headers
            .iter()
            .position(|name| name == "cusip__canon")
            .unwrap();
        let input_idx = headers.iter().position(|name| name == COLUMN).unwrap();

        for row in reader.records() {
            let record = row.unwrap();
            let input = record.get(input_idx).unwrap_or("").trim().to_string();
            let output_canonical = record.get(canon_idx).unwrap_or("");

            if is_blank_record(&record) || input.is_empty() {
                assert_eq!(output_canonical, "", "seed {}", seed);
                continue;
            }

            match json_map.get(&input) {
                Some(expected) => assert_eq!(output_canonical, expected, "seed {}", seed),
                None => assert_eq!(output_canonical, "", "seed {}", seed),
            }
        }

        let unresolved = json_payload["summary"]["unresolved"].as_u64().unwrap();
        if unresolved == 0 {
            assert_eq!(csv_status, 0, "seed {}", seed);
        } else {
            assert_eq!(csv_status, 1, "seed {}", seed);
        }
    }
}

#[test]
fn invariant_jsonl_completeness() {
    for seed in SEEDS {
        let case = build_jsonl_case(seed);
        let (_bytes, json) = run_json_mode(&case.input_path, &case.registry_path);

        let summary = &json["summary"];
        let total = summary["total"].as_u64().unwrap();
        let resolved = summary["resolved"].as_u64().unwrap();
        let unresolved = summary["unresolved"].as_u64().unwrap();

        assert_eq!(total, resolved + unresolved, "seed {}", seed);
    }
}

#[test]
fn invariant_jsonl_partition() {
    for seed in SEEDS {
        let case = build_jsonl_case(seed);
        let (_bytes, json) = run_json_mode(&case.input_path, &case.registry_path);
        let observed = observe_jsonl_inputs(&case.input_path);

        let mapped_inputs: HashSet<String> = json["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| decode_identifier(entry["input"].as_str().unwrap()))
            .collect();

        let unresolved_non_null_inputs: HashSet<String> = json["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["input"].as_str().map(decode_identifier))
            .collect();

        assert!(
            mapped_inputs.is_disjoint(&unresolved_non_null_inputs),
            "seed {}",
            seed
        );
        let union: HashSet<String> = mapped_inputs
            .union(&unresolved_non_null_inputs)
            .cloned()
            .collect();
        assert_eq!(union, observed.values, "seed {}", seed);

        let unresolved_special: HashSet<String> = json["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["input"].is_null())
            .map(|entry| entry["reason"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            unresolved_special, observed.special_reasons,
            "seed {}",
            seed
        );

        let summary_total = json["summary"]["total"].as_u64().unwrap() as usize;
        let expected_total = observed.values.len() + observed.special_reasons.len();
        assert_eq!(summary_total, expected_total, "seed {}", seed);
    }
}

fn build_csv_case(seed: u64) -> CsvCase {
    let tempdir = TempDir::new().unwrap();
    let registry_path = tempdir.path().join("registry");
    fs::create_dir_all(&registry_path).unwrap();

    let delimiter = match seed % 3 {
        0 => b',',
        1 => b'\t',
        _ => b'|',
    };
    let sep = delimiter as char;
    let row_count = 28usize;

    let entry_count = ((seed % 5) + 3) as usize;
    let mut primary_entries = Vec::new();
    for index in 0..entry_count {
        primary_entries.push(RegistryEntry {
            input: format!("ID{}_{}", seed, index),
            canonical_id: format!("CANON{}_{}", seed, index),
            canonical_type: match index % 4 {
                0 => "ticker".to_string(),
                1 => "isin".to_string(),
                2 => "lei".to_string(),
                _ => "counterparty_id".to_string(),
            },
            rule_id: format!("RULE_{}", index % 3),
        });
    }

    let mut secondary_entries = Vec::new();
    if let Some(first) = primary_entries.first() {
        secondary_entries.push(RegistryEntry {
            input: first.input.clone(),
            canonical_id: format!("SHADOW{}", seed),
            canonical_type: "ticker".to_string(),
            rule_id: "SHADOW_RULE".to_string(),
        });
    }
    secondary_entries.push(RegistryEntry {
        input: format!("ALIAS{}", seed),
        canonical_id: format!("ALIAS_CANON{}", seed),
        canonical_type: "ticker".to_string(),
        rule_id: "ALIAS_RULE".to_string(),
    });

    let all_entries: Vec<RegistryEntry> = primary_entries
        .iter()
        .chain(secondary_entries.iter())
        .cloned()
        .collect();

    write_registry_files(
        &registry_path,
        &primary_entries,
        &secondary_entries,
        all_entries.len(),
    );

    let extension = if delimiter == b'\t' { "tsv" } else { "csv" };
    let input_path = tempdir.path().join(format!("input.{}", extension));
    let mut lines = Vec::new();
    lines.push(format!("{}{}note", COLUMN, sep));
    for row in 0..row_count {
        match (seed as usize + row) % 6 {
            0 => {
                let value = &primary_entries[row % primary_entries.len()].input;
                lines.push(format!("{}{}resolved-{}", value, sep, row));
            }
            1 => {
                lines.push(format!("UNK{}_{}{}unknown", seed, row, sep));
            }
            2 => {
                let value = &primary_entries[row % primary_entries.len()].input;
                lines.push(format!("  {}  {}trimmed", value, sep));
            }
            3 => {
                lines.push(format!("{}empty-value", sep));
            }
            4 => {
                lines.push(format!("{}{}", sep, ""));
            }
            _ => {
                let value = &primary_entries[0].input;
                lines.push(format!("{}{}duplicate", value, sep));
            }
        }
    }
    fs::write(&input_path, lines.join("\n") + "\n").unwrap();

    CsvCase {
        _tempdir: tempdir,
        registry_path,
        input_path,
        delimiter,
        all_entries,
    }
}

fn build_jsonl_case(seed: u64) -> JsonlCase {
    let tempdir = TempDir::new().unwrap();
    let registry_path = tempdir.path().join("registry");
    fs::create_dir_all(&registry_path).unwrap();

    let entry_count = ((seed % 4) + 3) as usize;
    let mut primary_entries = Vec::new();
    for index in 0..entry_count {
        primary_entries.push(RegistryEntry {
            input: format!("JSON{}_{}", seed, index),
            canonical_id: format!("JCANON{}_{}", seed, index),
            canonical_type: "ticker".to_string(),
            rule_id: format!("JRULE_{}", index % 2),
        });
    }
    let secondary_entries = vec![RegistryEntry {
        input: primary_entries[0].input.clone(),
        canonical_id: format!("JSHADOW{}", seed),
        canonical_type: "ticker".to_string(),
        rule_id: "JSHADOW_RULE".to_string(),
    }];

    let all_entries: Vec<RegistryEntry> = primary_entries
        .iter()
        .chain(secondary_entries.iter())
        .cloned()
        .collect();
    write_registry_files(
        &registry_path,
        &primary_entries,
        &secondary_entries,
        all_entries.len(),
    );

    let input_path = tempdir.path().join("input.jsonl");
    let mut lines = Vec::new();
    for row in 0..24usize {
        let line = match (seed as usize + row) % 8 {
            0 => format!(
                r#"{{"{}":"{}","idx":{}}}"#,
                COLUMN, primary_entries[0].input, row
            ),
            1 => format!(r#"{{"{}":"UNKNOWN_{}_{}"}}"#, COLUMN, seed, row),
            2 => format!(
                r#"{{"{}":"  {}  "}}"#,
                COLUMN,
                primary_entries[row % primary_entries.len()].input
            ),
            3 => r#"{"other":"missing"}"#.to_string(),
            4 => format!(r#"{{"{}":null}}"#, COLUMN),
            5 => format!(r#"{{"{}":{{"nested":true}}}}"#, COLUMN),
            6 => format!(r#"{{"{}":[1,2,3]}}"#, COLUMN),
            _ => format!(r#"{{"{}":{}}}"#, COLUMN, row),
        };
        lines.push(line);
    }
    fs::write(&input_path, lines.join("\n") + "\n").unwrap();

    JsonlCase {
        _tempdir: tempdir,
        registry_path,
        input_path,
    }
}

fn write_registry_files(
    registry_path: &Path,
    primary_entries: &[RegistryEntry],
    secondary_entries: &[RegistryEntry],
    entry_count: usize,
) {
    let registry_json = serde_json::json!({
        "id": "prop-registry",
        "version": "1.0.0",
        "description": "property-test registry",
        "updated": "2026-01-01",
        "entry_count": entry_count
    });
    fs::write(
        registry_path.join("registry.json"),
        serde_json::to_string_pretty(&registry_json).unwrap(),
    )
    .unwrap();
    fs::write(
        registry_path.join("a-primary.json"),
        serde_json::to_string_pretty(primary_entries).unwrap(),
    )
    .unwrap();
    fs::write(
        registry_path.join("z-secondary.json"),
        serde_json::to_string_pretty(secondary_entries).unwrap(),
    )
    .unwrap();
}

fn run_json_mode(input_path: &Path, registry_path: &Path) -> (Vec<u8>, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            input_path.to_str().unwrap(),
            "--registry",
            registry_path.to_str().unwrap(),
            "--column",
            COLUMN,
            "--emit",
            "json",
            "--explicit",
        ])
        .output()
        .unwrap();

    let status = output.status.code().unwrap_or(2);
    assert_ne!(
        status,
        2,
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    (output.stdout, payload)
}

fn run_csv_mode(input_path: &Path, registry_path: &Path) -> (Vec<u8>, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .args([
            input_path.to_str().unwrap(),
            "--registry",
            registry_path.to_str().unwrap(),
            "--column",
            COLUMN,
            "--emit",
            "csv",
        ])
        .output()
        .unwrap();

    let status = output.status.code().unwrap_or(2);
    assert_ne!(
        status,
        2,
        "unexpected refusal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output.stdout, status)
}

fn observe_csv_inputs(path: &Path, delimiter: u8) -> ObservedInputs {
    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .from_path(path)
        .unwrap();
    let headers = reader.headers().unwrap().clone();
    let index = headers.iter().position(|name| name == COLUMN).unwrap();

    let mut observed = ObservedInputs::default();
    for row in reader.records() {
        let record = row.unwrap();
        if is_blank_record(&record) {
            continue;
        }
        let value = record.get(index).unwrap_or("").trim();
        if value.is_empty() {
            observed.special_reasons.insert("empty_value".to_string());
        } else {
            observed.values.insert(value.to_string());
        }
    }
    observed
}

fn observe_jsonl_inputs(path: &Path) -> ObservedInputs {
    let content = fs::read_to_string(path).unwrap();
    let mut observed = ObservedInputs::default();

    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line).unwrap();
        let object = value.as_object().unwrap();
        match object.get(COLUMN) {
            None => {
                observed.special_reasons.insert("missing_field".to_string());
            }
            Some(Value::Null) => {
                observed.special_reasons.insert("null_value".to_string());
            }
            Some(Value::Object(_)) | Some(Value::Array(_)) => {
                observed
                    .special_reasons
                    .insert("non_scalar_value".to_string());
            }
            Some(Value::String(text)) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    observed.special_reasons.insert("empty_value".to_string());
                } else {
                    observed.values.insert(trimmed.to_string());
                }
            }
            Some(Value::Number(number)) => {
                observed.values.insert(number.to_string());
            }
            Some(Value::Bool(boolean)) => {
                observed.values.insert(boolean.to_string());
            }
        }
    }

    observed
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
        let value = u8::from_str_radix(&hex[index..index + 2], 16).unwrap();
        bytes.push(value);
        index += 2;
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn is_blank_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}
