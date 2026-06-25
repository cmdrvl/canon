use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityFixtureTier {
    NormalCi,
    OperatorStress,
}

impl EntityFixtureTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalCi => "normal_ci",
            Self::OperatorStress => "operator_stress",
        }
    }

    pub const fn runs_in_default_ci(self) -> bool {
        matches!(self, Self::NormalCi)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityCommandTranscript {
    pub command: Vec<String>,
    pub input_hash: String,
    pub output_path: PathBuf,
    pub artifact_version: String,
    pub summary_counters: BTreeMap<String, u64>,
    pub exit_code: i32,
    pub stderr_refusal: Option<Value>,
}

impl EntityCommandTranscript {
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.command.is_empty() {
            return Err("command must be logged".to_string());
        }
        if !self.input_hash.starts_with("blake3:") {
            return Err("input_hash must be a blake3: digest".to_string());
        }
        if self.output_path.as_os_str().is_empty() {
            return Err("output_path must be logged".to_string());
        }
        if !self.artifact_version.starts_with("canon_entity_") {
            return Err("artifact_version must name a canon_entity artifact".to_string());
        }
        if self.summary_counters.is_empty() {
            return Err("summary counters must be logged".to_string());
        }
        if self.exit_code == 2 && self.stderr_refusal.is_none() {
            return Err("refusal exits must log stderr refusal envelope".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSnapshot {
    files: BTreeMap<PathBuf, String>,
}

impl TreeSnapshot {
    pub fn capture(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut files = BTreeMap::new();
        capture_tree(root, root, &mut files);
        Self { files }
    }

    pub fn diff(&self, after: &Self) -> Vec<TreeDiff> {
        let mut diffs = Vec::new();
        for (path, before_hash) in &self.files {
            match after.files.get(path) {
                Some(after_hash) if after_hash == before_hash => {}
                Some(after_hash) => diffs.push(TreeDiff {
                    path: path.clone(),
                    before: Some(before_hash.clone()),
                    after: Some(after_hash.clone()),
                }),
                None => diffs.push(TreeDiff {
                    path: path.clone(),
                    before: Some(before_hash.clone()),
                    after: None,
                }),
            }
        }
        for (path, after_hash) in &after.files {
            if !self.files.contains_key(path) {
                diffs.push(TreeDiff {
                    path: path.clone(),
                    before: None,
                    after: Some(after_hash.clone()),
                });
            }
        }
        diffs
    }

    pub fn assert_unchanged(&self, after: &Self) {
        let diffs = self.diff(after);
        assert!(diffs.is_empty(), "tree changed after refusal: {diffs:?}");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeDiff {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub fn assert_refusal_envelope(value: &Value, expected_code: &str) {
    assert_eq!(value["version"], "canon.v0");
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], expected_code);
    assert!(
        value["refusal"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty()),
        "refusal message must be present"
    );
    assert!(
        value["refusal"]["detail"].is_object(),
        "refusal detail must be an object"
    );
    assert!(
        value["refusal"]["next_command"]
            .as_str()
            .is_some_and(|next| !next.trim().is_empty()),
        "refusal next_command must be present"
    );
}

pub fn assert_deterministic_runs<T, F>(mut run: F)
where
    T: std::fmt::Debug + PartialEq,
    F: FnMut() -> T,
{
    let first = run();
    let second = run();
    assert_eq!(first, second, "deterministic harness runs differed");
}

pub fn blake3_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

pub fn blake3_file(path: impl AsRef<Path>) -> String {
    let bytes = fs::read(path).expect("hash input file can be read");
    blake3_bytes(&bytes)
}

pub fn copy_fixture_file(source: impl AsRef<Path>, dest: impl AsRef<Path>) -> String {
    let source = source.as_ref();
    let dest = dest.as_ref();
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).expect("fixture destination parent can be created");
    }
    fs::copy(source, dest).expect("fixture file can be copied");
    blake3_file(dest)
}

pub fn stable_seed(label: &str) -> u64 {
    let digest = blake3::hash(label.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[0..8]);
    u64::from_le_bytes(bytes)
}

fn capture_tree(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, String>) {
    let mut entries = fs::read_dir(current)
        .unwrap_or_else(|error| panic!("read_dir {}: {error}", current.display()))
        .map(|entry| entry.expect("directory entry can be read").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            capture_tree(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("captured file is under root")
                .to_path_buf();
            files.insert(relative, blake3_file(&path));
        }
    }
}
