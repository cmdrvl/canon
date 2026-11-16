//! Registry hand-off identity: what a driver needs to publish a local registry
//! directory safely. Offline and deterministic; canon uploads nothing.

use super::package::compile_registry_package;
use crate::{
    Refusal, RefusalCode, entity::runtime::incumbent::registry_identity_files, witness::hash_bytes,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

pub const REGISTRY_IDENTITY_VERSION: &str = "canon_registry_identity.v0";
pub const REGISTRY_REF_CHECK_VERSION: &str = "canon_registry_ref_check.v0";
/// Rule id of a retired id that still resolves, exactly, to its survivor.
const SUPERSEDED_ID_RULE: &str = "SUPERSEDED_ID_ALIAS";
const REF_FORM: &str = "[<registry id>:]<canonical id>@<registry version>#<closure hash>";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryFileIdentity {
    pub path: String,
    pub blake3: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryIdentity {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub package_digest: String,
    pub content_hash: String,
    pub files: Vec<RegistryFileIdentity>,
}

/// The `{version, content_hash, package_digest}` a receipt records for one
/// side of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryHandoffSide {
    pub version: String,
    pub content_hash: String,
    pub package_digest: String,
}

impl RegistryIdentity {
    pub fn side(&self) -> RegistryHandoffSide {
        RegistryHandoffSide {
            version: self.registry_version.clone(),
            content_hash: self.content_hash.clone(),
            package_digest: self.package_digest.clone(),
        }
    }

    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} {} ({} files)",
            self.registry_id,
            self.registry_version,
            self.content_hash,
            self.files.len()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryChangedFile {
    pub path: String,
    /// `added`, `modified` or `removed`.
    pub change: String,
    pub blake3_before: Option<String>,
    pub blake3_after: Option<String>,
}

/// Hash of the sorted file list: path, byte length and content hash per file.
pub fn content_hash_of(files: &[RegistryFileIdentity]) -> String {
    let mut sorted = files.to_vec();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    let mut manifest = Vec::new();
    for file in &sorted {
        manifest.extend_from_slice(file.path.as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(file.bytes.to_string().as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(file.blake3.as_bytes());
        manifest.push(b'\n');
    }
    hash_bytes(&manifest)
}

pub fn registry_identity(registry_dir: &Path) -> Result<RegistryIdentity, Refusal> {
    let display = registry_dir.display().to_string();
    let package = compile_registry_package(registry_dir)
        .map_err(|error| Refusal::bad_registry(&display, &error.to_string()))?;
    let paths = registry_identity_files(registry_dir)
        .map_err(|error| Refusal::bad_registry(&display, &error.message))?;
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| {
            Refusal::bad_registry(&display, &format!("{}: {error}", path.display()))
        })?;
        let relative = path
            .strip_prefix(registry_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push(RegistryFileIdentity {
            path: relative,
            blake3: hash_bytes(&bytes),
            bytes: bytes.len() as u64,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(RegistryIdentity {
        version: REGISTRY_IDENTITY_VERSION.to_string(),
        registry_id: package.registry.id,
        registry_version: package.registry.version,
        package_digest: package.content_digest,
        content_hash: content_hash_of(&files),
        files,
    })
}

/// The files whose bytes differ between two identities, sorted by path.
pub fn changed_files(
    before: &[RegistryFileIdentity],
    after: &[RegistryFileIdentity],
) -> Vec<RegistryChangedFile> {
    let before_by_path = before
        .iter()
        .map(|file| (file.path.as_str(), file.blake3.as_str()))
        .collect::<BTreeMap<_, _>>();
    let after_by_path = after
        .iter()
        .map(|file| (file.path.as_str(), file.blake3.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut paths = before_by_path
        .keys()
        .chain(after_by_path.keys())
        .copied()
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| {
            let old = before_by_path.get(path).copied();
            let new = after_by_path.get(path).copied();
            let change = match (old, new) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(old), Some(new)) if old != new => "modified",
                _ => return None,
            };
            Some(RegistryChangedFile {
                path: path.to_string(),
                change: change.to_string(),
                blake3_before: old.map(str::to_string),
                blake3_after: new.map(str::to_string),
            })
        })
        .collect()
}

/// The argv a driver runs to produce the served search index for a version.
pub fn search_index_export_command(registry_dir: &Path, registry_version: &str) -> Vec<String> {
    vec![
        "canon".to_string(),
        "registry".to_string(),
        "export".to_string(),
        "--registry".to_string(),
        registry_dir.display().to_string(),
        "--format".to_string(),
        "search-index".to_string(),
        "--out".to_string(),
        format!("{registry_version}/search.sqlite"),
    ]
}

/// `<registry id>:<canonical id>@<registry version>#<closure hash>`.
pub fn pinned_ref(
    registry_id: &str,
    canonical_id: &str,
    registry_version: &str,
    closure_hash: &str,
) -> String {
    format!("{registry_id}:{canonical_id}@{registry_version}#{closure_hash}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedRef {
    pub registry_id: Option<String>,
    pub canonical_id: String,
    pub registry_version: String,
    pub closure_hash: String,
}

/// Split at the first `@` and the last `#`.
pub fn parse_pinned_ref(text: &str) -> Result<PinnedRef, String> {
    let (left, rest) = text
        .split_once('@')
        .ok_or_else(|| format!("'{text}' has no '@'; expected {REF_FORM}"))?;
    let (version, closure_hash) = rest
        .rsplit_once('#')
        .ok_or_else(|| format!("'{text}' has no '#' after the version; expected {REF_FORM}"))?;
    let (registry_id, canonical_id) = match left.split_once(':') {
        Some((registry_id, canonical_id)) => (Some(registry_id), canonical_id),
        None => (None, left),
    };
    if canonical_id.is_empty()
        || version.is_empty()
        || closure_hash.is_empty()
        || registry_id.is_some_and(str::is_empty)
    {
        return Err(format!("'{text}' has an empty part; expected {REF_FORM}"));
    }
    Ok(PinnedRef {
        registry_id: registry_id.map(str::to_string),
        canonical_id: canonical_id.to_string(),
        registry_version: version.to_string(),
        closure_hash: closure_hash.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryRefCheck {
    pub version: String,
    pub reference: String,
    /// `current`, `superseded` or `unknown`.
    pub id_status: String,
    pub resolves_to: Option<String>,
    pub version_match: bool,
    pub closure_found: bool,
}

/// Check a pinned reference against a registry directory.
pub fn check_pinned_ref(registry_dir: &Path, text: &str) -> Result<RegistryRefCheck, Refusal> {
    let pinned = parse_pinned_ref(text).map_err(|message| Refusal {
        code: RefusalCode::EParse,
        message: format!("Malformed pinned reference: {message}"),
        detail: json!({"reference": text}),
        next_command: Some(format!("Use {REF_FORM}")),
    })?;
    let display = registry_dir.display().to_string();
    let identity = registry_identity(registry_dir)?;
    let mut id_status = "unknown";
    let mut resolves_to = None;
    let mut closure_found = false;
    for file in &identity.files {
        if !is_mapping_file(&file.path) {
            continue;
        }
        let bytes = fs::read(registry_dir.join(&file.path))
            .map_err(|error| Refusal::bad_registry(&display, &error.to_string()))?;
        let entries = serde_json::from_slice::<Vec<Value>>(&bytes)
            .map_err(|error| Refusal::bad_registry(&display, &error.to_string()))?;
        for entry in &entries {
            let canonical_id = entry["canonical_id"].as_str().unwrap_or_default();
            if canonical_id == pinned.canonical_id {
                if id_status != "current" {
                    id_status = "current";
                    resolves_to = Some(canonical_id.to_string());
                }
                if entry["session_authority"]["closure_hash"] == pinned.closure_hash.as_str() {
                    closure_found = true;
                }
            } else if entry["input"] == pinned.canonical_id.as_str()
                && entry["rule_id"] == SUPERSEDED_ID_RULE
                && id_status == "unknown"
            {
                id_status = "superseded";
                resolves_to = Some(canonical_id.to_string());
            }
        }
    }
    if let Ok(corrections) = fs::read_to_string(registry_dir.join("corrections.jsonl")) {
        closure_found |= corrections
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|line| {
                line["closure_hash"] == pinned.closure_hash.as_str()
                    && line["canonical_id"] == pinned.canonical_id.as_str()
            });
    }
    let registry_matches = pinned
        .registry_id
        .as_deref()
        .is_none_or(|id| id == identity.registry_id);
    Ok(RegistryRefCheck {
        version: REGISTRY_REF_CHECK_VERSION.to_string(),
        reference: text.to_string(),
        id_status: id_status.to_string(),
        resolves_to,
        version_match: registry_matches && pinned.registry_version == identity.registry_version,
        closure_found: closure_found && registry_matches,
    })
}

fn is_mapping_file(path: &str) -> bool {
    !path.contains('/')
        && path.ends_with(".json")
        && path != "registry.json"
        && path != "_build.json"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, hash: &str, bytes: u64) -> RegistryFileIdentity {
        RegistryFileIdentity {
            path: path.to_string(),
            blake3: hash.to_string(),
            bytes,
        }
    }

    #[test]
    fn content_hash_is_order_independent_and_path_sensitive() {
        let a = file("aliases.json", "blake3:aa", 2);
        let b = file("_escrow/cannot_link.jsonl", "blake3:bb", 3);
        assert_eq!(
            content_hash_of(&[a.clone(), b.clone()]),
            content_hash_of(&[b.clone(), a.clone()])
        );
        let renamed = file("other.json", "blake3:aa", 2);
        assert_ne!(
            content_hash_of(&[a.clone(), b.clone()]),
            content_hash_of(&[renamed, b])
        );
    }

    #[test]
    fn one_changed_hash_changes_the_content_hash() {
        let a = file("aliases.json", "blake3:aa", 2);
        let b = file("_escrow/cannot_link.jsonl", "blake3:bb", 3);
        let b_changed = file("_escrow/cannot_link.jsonl", "blake3:bc", 3);
        assert_ne!(
            content_hash_of(&[a.clone(), b]),
            content_hash_of(&[a, b_changed])
        );
    }

    #[test]
    fn changed_files_lists_added_modified_removed_and_ignores_equal() {
        let before = [
            file("a.json", "blake3:1", 1),
            file("b.json", "blake3:2", 1),
            file("gone.json", "blake3:3", 1),
        ];
        let after = [
            file("a.json", "blake3:1", 1),
            file("b.json", "blake3:9", 1),
            file("new.json", "blake3:4", 1),
        ];
        let changes = changed_files(&before, &after);
        let summary = changes
            .iter()
            .map(|change| (change.path.as_str(), change.change.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                ("b.json", "modified"),
                ("gone.json", "removed"),
                ("new.json", "added")
            ]
        );
        assert_eq!(changes[0].blake3_before.as_deref(), Some("blake3:2"));
        assert_eq!(changes[0].blake3_after.as_deref(), Some("blake3:9"));
        assert_eq!(changes[2].blake3_before, None);
    }

    #[test]
    fn pinned_ref_splits_at_first_at_and_last_hash() {
        let parsed = parse_pinned_ref("firms:ORG-012@2026-11-16#blake3:abc").unwrap();
        assert_eq!(parsed.registry_id.as_deref(), Some("firms"));
        assert_eq!(parsed.canonical_id, "ORG-012");
        assert_eq!(parsed.registry_version, "2026-11-16");
        assert_eq!(parsed.closure_hash, "blake3:abc");
        let bare = parse_pinned_ref("ORG-012@1.0.0#blake3:abc").unwrap();
        assert_eq!(bare.registry_id, None);
        let round_trip = pinned_ref("firms", "ORG-012", "2026-11-16", "blake3:abc");
        assert_eq!(
            parse_pinned_ref(&round_trip).unwrap().canonical_id,
            "ORG-012"
        );
    }

    #[test]
    fn malformed_pinned_refs_are_refused_naming_the_form() {
        for bad in [
            "ORG-012",
            "ORG-012@1.0.0",
            "@1.0.0#h",
            "ORG-012@#h",
            "ORG-012@1.0.0#",
            ":ORG@1#h",
        ] {
            let error = parse_pinned_ref(bad).unwrap_err();
            assert!(
                error.contains("<canonical id>@<registry version>#<closure hash>"),
                "{bad}: {error}"
            );
        }
    }

    #[test]
    fn export_command_names_the_search_index_format_and_version() {
        let argv = search_index_export_command(Path::new("reg"), "1.2.3");
        assert_eq!(argv[..4], ["canon", "registry", "export", "--registry"]);
        assert!(argv.contains(&"search-index".to_string()));
        assert_eq!(argv.last().map(String::as_str), Some("1.2.3/search.sqlite"));
    }
}
