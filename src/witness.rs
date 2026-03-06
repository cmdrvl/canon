use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessSummary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessInput {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessRecord {
    #[serde(default)]
    pub id: String,
    pub tool: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub binary_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<WitnessInput>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    pub output_hash: String,
    pub outcome: String,
    pub exit_code: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    pub ts: String,
}

impl WitnessRecord {
    pub fn new(
        inputs: Vec<WitnessInput>,
        params: Map<String, Value>,
        output_hash: &str,
        outcome: &str,
        exit_code: u8,
    ) -> Self {
        Self {
            id: String::new(),
            tool: "canon".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            binary_hash: hash_binary()
                .map(|hash| format!("blake3:{hash}"))
                .unwrap_or_default(),
            inputs,
            params,
            output_hash: output_hash.to_string(),
            outcome: outcome.to_string(),
            exit_code,
            prev: None,
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        }
    }

    pub fn compute_id(&mut self) {
        self.id.clear();
        self.id = format!(
            "blake3:{}",
            blake3::hash(canonical_json(self).as_bytes()).to_hex()
        );
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

pub fn hash_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}

pub fn append_witness_record(
    record: &WitnessRecord,
    no_witness: bool,
) -> Result<(), Box<dyn Error>> {
    let ledger_path = resolve_ledger_path()?;
    append_witness_record_to_path(record, no_witness, &ledger_path)
}

pub fn append_witness(_output_hash: &[u8], no_witness: bool) -> Result<(), Box<dyn Error>> {
    if no_witness {
        return Ok(());
    }

    let output_hash = hash_bytes(_output_hash);
    let record = WitnessRecord::new(
        vec![WitnessInput {
            path: "<unknown>".to_string(),
            hash: Some("blake3:unknown".to_string()),
            bytes: None,
        }],
        Map::new(),
        &output_hash,
        "PARTIAL",
        1,
    );
    append_witness_record(&record, false)
}

fn append_witness_record_to_path(
    record: &WitnessRecord,
    no_witness: bool,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    if no_witness {
        return Ok(());
    }

    let mut record = record.clone();
    if record.prev.is_none() {
        record.prev = last_record_id(path);
    }
    record.compute_id();
    let line = canonical_json(&record);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("Warning: witness append skipped: {}", error);
        return Ok(());
    }

    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => {
            if let Err(error) = writeln!(file, "{}", line) {
                eprintln!("Warning: witness write failed: {}", error);
            }
        }
        Err(error) => {
            eprintln!("Warning: witness append skipped: {}", error);
        }
    }
    Ok(())
}

fn canonical_json(record: &WitnessRecord) -> String {
    let value = serde_json::to_value(record).expect("witness record should serialize");
    serde_json::to_string(&value).expect("witness record should encode")
}

fn hash_binary() -> io::Result<String> {
    let path = std::env::current_exe()?;
    let bytes = std::fs::read(path)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn last_record_id(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut last_non_empty = None;

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            last_non_empty = Some(trimmed.to_owned());
        }
    }

    let last = last_non_empty?;
    let value: Value = serde_json::from_str(&last).ok()?;
    value.get("id")?.as_str().map(ToOwned::to_owned)
}

fn resolve_ledger_path() -> io::Result<PathBuf> {
    resolve_ledger_path_from_env(|key| std::env::var(key).ok())
}

fn resolve_ledger_path_from_env<F>(get_env: F) -> io::Result<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = get_env("EPISTEMIC_WITNESS")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path));
    }

    let home = get_env("HOME")
        .or_else(|| get_env("USERPROFILE"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not determine home directory; set EPISTEMIC_WITNESS",
            )
        })?;

    Ok(PathBuf::from(home).join(".epistemic").join("witness.jsonl"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_record() -> WitnessRecord {
        let mut params = Map::new();
        params.insert(
            "input_path".to_string(),
            Value::String("tape.csv".to_string()),
        );
        params.insert(
            "registry_id".to_string(),
            Value::String("cusip-isin".to_string()),
        );
        params.insert(
            "registry_version".to_string(),
            Value::String("1.0.0".to_string()),
        );
        params.insert(
            "summary".to_string(),
            serde_json::json!({
                "total": 100,
                "resolved": 95,
                "unresolved": 5
            }),
        );

        WitnessRecord::new(
            vec![WitnessInput {
                path: "tape.csv".to_string(),
                hash: Some("blake3:inputhash".to_string()),
                bytes: Some(123),
            }],
            params,
            "blake3:outputhash",
            "PARTIAL",
            1,
        )
    }

    #[test]
    fn witness_record_serializes_expected_structure() {
        let temp_dir = TempDir::new().unwrap();
        let witness_path = temp_dir.path().join("nested").join("witness.jsonl");

        let record = sample_record();

        append_witness_record_to_path(&record, false, &witness_path).unwrap();

        let contents = std::fs::read_to_string(&witness_path).unwrap();
        let line = contents.lines().next().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();

        assert_eq!(parsed["tool"], "canon");
        assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
        assert!(parsed["id"].as_str().unwrap().starts_with("blake3:"));
        assert!(
            parsed["binary_hash"]
                .as_str()
                .unwrap()
                .starts_with("blake3:")
        );
        assert_eq!(parsed["inputs"][0]["path"], "tape.csv");
        assert_eq!(parsed["inputs"][0]["hash"], "blake3:inputhash");
        assert_eq!(parsed["inputs"][0]["bytes"], 123);
        assert_eq!(parsed["output_hash"], "blake3:outputhash");
        assert_eq!(parsed["params"]["registry_id"], "cusip-isin");
        assert_eq!(parsed["params"]["registry_version"], "1.0.0");
        assert_eq!(parsed["outcome"], "PARTIAL");
        assert_eq!(parsed["exit_code"], 1);
        assert_eq!(parsed["params"]["summary"]["total"], 100);
        assert_eq!(parsed["params"]["summary"]["resolved"], 95);
        assert_eq!(parsed["params"]["summary"]["unresolved"], 5);
        assert!(parsed["ts"].as_str().unwrap().contains('T'));
        assert!(parsed["prev"].is_null());
    }

    #[test]
    fn blake3_hash_helpers_match_reference_digest() {
        let bytes = b"canon";
        let expected = format!("blake3:{}", blake3::hash(bytes).to_hex());
        assert_eq!(hash_bytes(bytes), expected);

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("input.txt");
        std::fs::write(&file_path, bytes).unwrap();
        assert_eq!(hash_file(&file_path).unwrap(), expected);
    }

    #[test]
    fn no_witness_mode_skips_writing() {
        let temp_dir = TempDir::new().unwrap();
        let witness_path = temp_dir.path().join("nested").join("witness.jsonl");
        let record = sample_record();

        append_witness_record_to_path(&record, true, &witness_path).unwrap();
        assert!(!witness_path.exists());
    }

    #[test]
    fn ledger_path_prefers_epistemic_witness_env_var() {
        let path = resolve_ledger_path_from_env(|key| match key {
            "EPISTEMIC_WITNESS" => Some("/tmp/custom-witness.jsonl".to_owned()),
            "HOME" => Some("/tmp/home".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(path, PathBuf::from("/tmp/custom-witness.jsonl"));
    }

    #[test]
    fn ledger_path_defaults_to_home_epistemic_path() {
        let path = resolve_ledger_path_from_env(|key| match key {
            "EPISTEMIC_WITNESS" => None,
            "HOME" => Some("/tmp/home".to_owned()),
            _ => None,
        })
        .unwrap();

        assert_eq!(path, PathBuf::from("/tmp/home/.epistemic/witness.jsonl"));
    }

    #[test]
    fn ledger_path_errors_without_home_or_override() {
        let error = resolve_ledger_path_from_env(|_| None).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn witness_failure_does_not_fail_callers() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_path = temp_dir.path().join("not_a_file");
        std::fs::create_dir_all(&invalid_path).unwrap();

        let mut record = sample_record();
        record.outcome = "UNRESOLVED".to_string();

        let result = append_witness_record_to_path(&record, false, &invalid_path);
        assert!(result.is_ok());
    }

    #[test]
    fn witness_records_chain_prev_ids() {
        let temp_dir = TempDir::new().unwrap();
        let witness_path = temp_dir.path().join("nested").join("witness.jsonl");

        let first = sample_record();
        let mut second = sample_record();
        second.output_hash = "blake3:other".to_string();

        append_witness_record_to_path(&first, false, &witness_path).unwrap();
        append_witness_record_to_path(&second, false, &witness_path).unwrap();

        let contents = std::fs::read_to_string(&witness_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Value = serde_json::from_str(lines[0]).unwrap();
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["prev"], first["id"]);
    }
}
