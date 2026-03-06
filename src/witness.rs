use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessSummary {
    pub total: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WitnessRecord {
    pub tool: String,
    pub version: String,
    pub timestamp: String,
    pub input_path: String,
    pub input_hash: String,
    pub output_hash: String,
    pub registry_id: String,
    pub registry_version: String,
    pub outcome: String,
    pub summary: WitnessSummary,
}

impl WitnessRecord {
    pub fn new(
        input_path: &str,
        input_hash: &str,
        output_hash: &str,
        registry_id: &str,
        registry_version: &str,
        outcome: &str,
        summary: WitnessSummary,
    ) -> Self {
        Self {
            tool: "canon".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: utc_timestamp_millis(),
            input_path: input_path.to_string(),
            input_hash: input_hash.to_string(),
            output_hash: output_hash.to_string(),
            registry_id: registry_id.to_string(),
            registry_version: registry_version.to_string(),
            outcome: outcome.to_string(),
            summary,
        }
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
        "<unknown>",
        "blake3:unknown",
        &output_hash,
        "<unknown>",
        "<unknown>",
        "PARTIAL",
        WitnessSummary {
            total: 0,
            resolved: 0,
            unresolved: 0,
        },
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

    let line = serde_json::to_string(record)?;
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

fn utc_timestamp_millis() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    format!("unix:{}.{:03}Z", secs, millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn witness_record_serializes_expected_structure() {
        let temp_dir = TempDir::new().unwrap();
        let witness_path = temp_dir.path().join("nested").join("witness.jsonl");

        let record = WitnessRecord::new(
            "tape.csv",
            "blake3:inputhash",
            "blake3:outputhash",
            "cusip-isin",
            "1.0.0",
            "PARTIAL",
            WitnessSummary {
                total: 100,
                resolved: 95,
                unresolved: 5,
            },
        );

        append_witness_record_to_path(&record, false, &witness_path).unwrap();

        let contents = std::fs::read_to_string(&witness_path).unwrap();
        let line = contents.lines().next().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();

        assert_eq!(parsed["tool"], "canon");
        assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(parsed["input_path"], "tape.csv");
        assert_eq!(parsed["input_hash"], "blake3:inputhash");
        assert_eq!(parsed["output_hash"], "blake3:outputhash");
        assert_eq!(parsed["registry_id"], "cusip-isin");
        assert_eq!(parsed["registry_version"], "1.0.0");
        assert_eq!(parsed["outcome"], "PARTIAL");
        assert_eq!(parsed["summary"]["total"], 100);
        assert_eq!(parsed["summary"]["resolved"], 95);
        assert_eq!(parsed["summary"]["unresolved"], 5);
        assert!(parsed["timestamp"].as_str().unwrap().starts_with("unix:"));
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
        let record = WitnessRecord::new(
            "tape.csv",
            "blake3:in",
            "blake3:out",
            "registry",
            "1.0.0",
            "RESOLVED",
            WitnessSummary {
                total: 1,
                resolved: 1,
                unresolved: 0,
            },
        );

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

        let record = WitnessRecord::new(
            "tape.csv",
            "blake3:in",
            "blake3:out",
            "registry",
            "1.0.0",
            "UNRESOLVED",
            WitnessSummary {
                total: 1,
                resolved: 0,
                unresolved: 1,
            },
        );

        let result = append_witness_record_to_path(&record, false, &invalid_path);
        assert!(result.is_ok());
    }
}
