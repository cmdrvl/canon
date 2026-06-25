//! Incumbent registry-memory loading and snapshot hashing for `canon entity`.

use super::types::{
    AliasMappingEntry, AnchorValue, CannotLinkFact, EntityError, EntityErrorCode, EntityResult,
    IncumbentMemory, PendingClusterRecord, RegistrySnapshot, RowPair, TrustedAnchorRecord,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct RegistryJson {
    id: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct RawPendingClusterRecord {
    escrow_id: String,
    profile: String,
    #[serde(default)]
    doc_ids: Vec<String>,
    #[serde(default)]
    surfaces: Vec<String>,
    #[serde(default)]
    anchors: Vec<AnchorValue>,
    #[serde(default)]
    witness_pairs: Vec<RawRowPair>,
    state: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawRowPair {
    Object {
        left_row_id: String,
        right_row_id: String,
    },
    Tuple([String; 2]),
}

impl RawRowPair {
    fn into_row_pair(self) -> RowPair {
        match self {
            Self::Object {
                left_row_id,
                right_row_id,
            } => RowPair {
                left_row_id,
                right_row_id,
            },
            Self::Tuple([left_row_id, right_row_id]) => RowPair {
                left_row_id,
                right_row_id,
            },
        }
    }
}

pub fn load_incumbent_memory(registry_dir: &Path) -> EntityResult<IncumbentMemory> {
    let metadata = load_registry_metadata(registry_dir)?;
    let alias_files = discover_alias_files(registry_dir)?;
    let anchor_files = discover_anchor_sidecars(registry_dir)?;
    let escrow_files = discover_escrow_sidecars(registry_dir);

    let alias_entries = load_alias_entries(&alias_files)?;
    let trusted_anchors = load_trusted_anchors(&anchor_files)?;
    let pending_clusters = load_pending_clusters(&escrow_files.pending)?;
    let cannot_link_facts = load_cannot_link_facts(&escrow_files.cannot_link)?;

    Ok(IncumbentMemory {
        registry: RegistrySnapshot {
            id: metadata.id,
            version: metadata.version,
            source: registry_dir.to_string_lossy().into_owned(),
            lookup_snapshot_hash: compute_manifest_hash(
                registry_dir,
                &build_lookup_manifest_paths(registry_dir, &alias_files, &anchor_files),
                "lookup snapshot",
            )?,
            escrow_snapshot_hash: compute_manifest_hash(
                registry_dir,
                &build_escrow_manifest_paths(&escrow_files),
                "escrow snapshot",
            )?,
        },
        alias_entries,
        trusted_anchors,
        pending_clusters,
        cannot_link_facts,
    })
}

pub fn lookup_snapshot_hash(registry_dir: &Path) -> EntityResult<String> {
    let alias_files = discover_alias_files(registry_dir)?;
    let anchor_files = discover_anchor_sidecars(registry_dir)?;
    compute_manifest_hash(
        registry_dir,
        &build_lookup_manifest_paths(registry_dir, &alias_files, &anchor_files),
        "lookup snapshot",
    )
}

pub fn escrow_snapshot_hash(registry_dir: &Path) -> EntityResult<String> {
    let escrow_files = discover_escrow_sidecars(registry_dir);
    compute_manifest_hash(
        registry_dir,
        &build_escrow_manifest_paths(&escrow_files),
        "escrow snapshot",
    )
}

fn load_registry_metadata(registry_dir: &Path) -> EntityResult<RegistryJson> {
    if !registry_dir.is_dir() {
        return Err(registry_error(
            format!("Registry directory not found: {}", registry_dir.display()),
            json!({ "path": registry_dir.display().to_string() }),
        ));
    }

    let registry_json_path = registry_dir.join("registry.json");
    let bytes = fs::read(&registry_json_path).map_err(|error| {
        registry_error(
            format!(
                "Failed to read registry metadata from '{}'",
                registry_json_path.display()
            ),
            json!({
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    serde_json::from_slice(&bytes).map_err(|error| {
        registry_error(
            "Invalid registry.json for org incumbent memory",
            json!({
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })
}

fn discover_alias_files(registry_dir: &Path) -> EntityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(registry_dir).map_err(|error| {
        registry_error(
            format!(
                "Failed to read registry directory '{}'",
                registry_dir.display()
            ),
            json!({
                "path": registry_dir.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            registry_error(
                format!(
                    "Failed to inspect registry directory '{}'",
                    registry_dir.display()
                ),
                json!({
                    "path": registry_dir.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
        let path = entry.path();

        if path.is_file()
            && path.extension() == Some("json".as_ref())
            && path.file_name() != Some("registry.json".as_ref())
            && path.file_name() != Some("_build.json".as_ref())
        {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

fn discover_anchor_sidecars(registry_dir: &Path) -> EntityResult<Vec<PathBuf>> {
    let anchors_dir = registry_dir.join("_anchors");
    if !anchors_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(&anchors_dir).map_err(|error| {
        registry_error(
            format!(
                "Failed to read trusted-anchor sidecar directory '{}'",
                anchors_dir.display()
            ),
            json!({
                "path": anchors_dir.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            registry_error(
                format!(
                    "Failed to inspect trusted-anchor sidecar directory '{}'",
                    anchors_dir.display()
                ),
                json!({
                    "path": anchors_dir.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
        let path = entry.path();
        if path.is_file() && path.extension() == Some("jsonl".as_ref()) {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

struct EscrowSidecars {
    pending: Option<PathBuf>,
    cannot_link: Option<PathBuf>,
}

fn discover_escrow_sidecars(registry_dir: &Path) -> EscrowSidecars {
    let escrow_dir = registry_dir.join("_escrow");
    EscrowSidecars {
        pending: file_if_exists(escrow_dir.join("pending.jsonl")),
        cannot_link: file_if_exists(escrow_dir.join("cannot_link.jsonl")),
    }
}

fn file_if_exists(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn build_lookup_manifest_paths(
    registry_dir: &Path,
    alias_files: &[PathBuf],
    anchor_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut files = Vec::with_capacity(1 + alias_files.len() + anchor_files.len());
    files.push(registry_dir.join("registry.json"));
    files.extend(alias_files.iter().cloned());
    files.extend(anchor_files.iter().cloned());
    files
}

fn build_escrow_manifest_paths(sidecars: &EscrowSidecars) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(path) = &sidecars.cannot_link {
        files.push(path.clone());
    }
    if let Some(path) = &sidecars.pending {
        files.push(path.clone());
    }
    files
}

fn load_alias_entries(paths: &[PathBuf]) -> EntityResult<Vec<AliasMappingEntry>> {
    let mut entries = Vec::new();

    for path in paths {
        let bytes = fs::read(path).map_err(|error| {
            registry_error(
                format!("Failed to read alias mapping file '{}'", path.display()),
                json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
        let file_entries: Vec<AliasMappingEntry> =
            serde_json::from_slice(&bytes).map_err(|error| {
                registry_error(
                    "Invalid alias mapping file for org incumbent memory",
                    json!({
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                    }),
                )
            })?;

        for (index, entry) in file_entries.into_iter().enumerate() {
            validate_non_empty(
                "alias mapping entry",
                "input",
                &entry.input,
                path,
                Some(index + 1),
            )?;
            validate_non_empty(
                "alias mapping entry",
                "canonical_id",
                &entry.canonical_id,
                path,
                Some(index + 1),
            )?;
            validate_non_empty(
                "alias mapping entry",
                "canonical_type",
                &entry.canonical_type,
                path,
                Some(index + 1),
            )?;
            validate_non_empty(
                "alias mapping entry",
                "rule_id",
                &entry.rule_id,
                path,
                Some(index + 1),
            )?;
            entries.push(entry);
        }
    }

    Ok(entries)
}

fn load_trusted_anchors(paths: &[PathBuf]) -> EntityResult<Vec<TrustedAnchorRecord>> {
    let mut anchors = Vec::new();

    for path in paths {
        for (line_number, record) in read_jsonl::<TrustedAnchorRecord>(path, "trusted anchor")? {
            validate_non_empty(
                "trusted anchor record",
                "canonical_id",
                &record.canonical_id,
                path,
                Some(line_number),
            )?;
            validate_non_empty(
                "trusted anchor record",
                "namespace",
                &record.namespace,
                path,
                Some(line_number),
            )?;
            validate_non_empty(
                "trusted anchor record",
                "value",
                &record.value,
                path,
                Some(line_number),
            )?;
            anchors.push(record);
        }
    }

    Ok(anchors)
}

fn load_pending_clusters(path: &Option<PathBuf>) -> EntityResult<Vec<PendingClusterRecord>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };

    let mut clusters = Vec::new();
    for (line_number, raw) in read_jsonl::<RawPendingClusterRecord>(path, "pending escrow")? {
        validate_non_empty(
            "pending escrow record",
            "escrow_id",
            &raw.escrow_id,
            path,
            Some(line_number),
        )?;
        validate_non_empty(
            "pending escrow record",
            "profile",
            &raw.profile,
            path,
            Some(line_number),
        )?;
        validate_non_empty(
            "pending escrow record",
            "state",
            &raw.state,
            path,
            Some(line_number),
        )?;

        for (index, anchor) in raw.anchors.iter().enumerate() {
            validate_non_empty(
                "pending escrow anchor",
                "namespace",
                &anchor.namespace,
                path,
                Some(line_number),
            )
            .map_err(|mut error| {
                error.detail = Some(json!({
                    "path": path.display().to_string(),
                    "line": line_number,
                    "anchor_index": index,
                    "field": "namespace",
                }));
                error
            })?;
            validate_non_empty(
                "pending escrow anchor",
                "value",
                &anchor.value,
                path,
                Some(line_number),
            )
            .map_err(|mut error| {
                error.detail = Some(json!({
                    "path": path.display().to_string(),
                    "line": line_number,
                    "anchor_index": index,
                    "field": "value",
                }));
                error
            })?;
        }

        let witness_pairs = raw
            .witness_pairs
            .into_iter()
            .map(RawRowPair::into_row_pair)
            .collect::<Vec<_>>();
        for pair in &witness_pairs {
            validate_non_empty(
                "pending escrow witness pair",
                "left_row_id",
                &pair.left_row_id,
                path,
                Some(line_number),
            )?;
            validate_non_empty(
                "pending escrow witness pair",
                "right_row_id",
                &pair.right_row_id,
                path,
                Some(line_number),
            )?;
        }

        clusters.push(PendingClusterRecord {
            escrow_id: raw.escrow_id,
            profile: raw.profile,
            doc_ids: raw.doc_ids,
            surfaces: raw.surfaces,
            anchors: raw.anchors,
            witness_pairs,
            state: raw.state,
        });
    }

    Ok(clusters)
}

fn load_cannot_link_facts(path: &Option<PathBuf>) -> EntityResult<Vec<CannotLinkFact>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };

    let mut facts = Vec::new();
    for (line_number, record) in read_jsonl::<CannotLinkFact>(path, "cannot-link")? {
        validate_non_empty(
            "cannot-link record",
            "left_key",
            &record.left_key,
            path,
            Some(line_number),
        )?;
        validate_non_empty(
            "cannot-link record",
            "right_key",
            &record.right_key,
            path,
            Some(line_number),
        )?;
        validate_non_empty(
            "cannot-link record",
            "reason",
            &record.reason,
            path,
            Some(line_number),
        )?;
        facts.push(record);
    }

    Ok(facts)
}

fn read_jsonl<T>(path: &Path, context: &str) -> EntityResult<Vec<(usize, T)>>
where
    T: DeserializeOwned,
{
    let file = File::open(path).map_err(|error| {
        registry_error(
            format!("Failed to read {context} sidecar '{}'", path.display()),
            json!({
                "path": path.display().to_string(),
                "error": error.to_string(),
            }),
        )
    })?;

    let mut records = Vec::new();
    for (line_index, line_result) in BufReader::new(file).lines().enumerate() {
        let line_number = line_index + 1;
        let line = line_result.map_err(|error| {
            registry_error(
                format!("Failed to read {context} sidecar '{}'", path.display()),
                json!({
                    "path": path.display().to_string(),
                    "line": line_number,
                    "error": error.to_string(),
                }),
            )
        })?;

        if line.trim().is_empty() {
            continue;
        }

        let record = serde_json::from_str::<T>(&line).map_err(|error| {
            registry_error(
                format!("Invalid {context} sidecar record"),
                json!({
                    "path": path.display().to_string(),
                    "line": line_number,
                    "error": error.to_string(),
                }),
            )
        })?;
        records.push((line_number, record));
    }

    Ok(records)
}

fn compute_manifest_hash(
    registry_dir: &Path,
    paths: &[PathBuf],
    context: &str,
) -> EntityResult<String> {
    let mut manifest = Vec::new();

    for path in paths {
        let bytes = fs::read(path).map_err(|error| {
            registry_error(
                format!("Failed to read file for {context}"),
                json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }),
            )
        })?;
        let relative_path = relative_path(registry_dir, path);
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        manifest.extend_from_slice(relative_path.as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(bytes.len().to_string().as_bytes());
        manifest.push(b'\t');
        manifest.extend_from_slice(content_hash.as_bytes());
        manifest.push(b'\n');
    }

    Ok(format!("blake3:{}", blake3::hash(&manifest).to_hex()))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_non_empty(
    context: &str,
    field: &str,
    value: &str,
    path: &Path,
    location: Option<usize>,
) -> EntityResult<()> {
    if value.trim().is_empty() {
        let mut detail = json!({
            "path": path.display().to_string(),
            "field": field,
        });
        if let Some(location) = location {
            detail["location"] = json!(location);
        }

        return Err(registry_error(
            format!("{context} must include a non-empty {field}"),
            detail,
        ));
    }

    Ok(())
}

fn registry_error(message: impl Into<String>, detail: serde_json::Value) -> EntityError {
    EntityError::with_detail(EntityErrorCode::Registry, message, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_registry_json(path: &Path, id: &str, version: &str) {
        fs::write(
            path.join("registry.json"),
            serde_json::to_string_pretty(&json!({
                "id": id,
                "version": version,
                "description": "test registry",
                "updated": "2026-03-23",
                "entry_count": 2
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_mapping_file(path: &Path, name: &str, entries: serde_json::Value) {
        fs::write(
            path.join(name),
            serde_json::to_string_pretty(&entries).unwrap(),
        )
        .unwrap();
    }

    fn write_jsonl(path: &Path, lines: &[serde_json::Value]) {
        let body = lines
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        fs::write(path, format!("{body}\n")).unwrap();
    }

    fn create_registry_tree() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.23");
        write_mapping_file(
            temp_dir.path(),
            "b-aliases.json",
            json!([
                {
                    "input": "Acme Corp.",
                    "canonical_id": "ORG-001",
                    "canonical_type": "org_canon_id",
                    "rule_id": "PROMOTED_ALIAS"
                }
            ]),
        );
        write_mapping_file(
            temp_dir.path(),
            "a-primary.json",
            json!([
                {
                    "input": "ACME CORPORATION",
                    "canonical_id": "ORG-001",
                    "canonical_type": "org_canon_id",
                    "rule_id": "PRIMARY_NAME"
                }
            ]),
        );

        let anchors_dir = temp_dir.path().join("_anchors");
        fs::create_dir_all(&anchors_dir).unwrap();
        write_jsonl(
            &anchors_dir.join("20260322T160000Z.anchors.jsonl"),
            &[json!({
                "canonical_id": "ORG-002",
                "namespace": "lei",
                "value": "549300BBBB"
            })],
        );
        write_jsonl(
            &anchors_dir.join("20260322T150000Z.anchors.jsonl"),
            &[json!({
                "canonical_id": "ORG-001",
                "namespace": "lei",
                "value": "549300AAAA"
            })],
        );

        let escrow_dir = temp_dir.path().join("_escrow");
        fs::create_dir_all(&escrow_dir).unwrap();
        write_jsonl(
            &escrow_dir.join("pending.jsonl"),
            &[json!({
                "escrow_id": "OE-123",
                "profile": "bdc_issuer",
                "doc_ids": ["doc-1", "doc-2"],
                "surfaces": ["Acme Corp.", "ACME CORPORATION"],
                "anchors": [{"namespace": "lei", "value": "549300AAAA"}],
                "witness_pairs": [["row-1", "row-2"]],
                "state": "pending"
            })],
        );
        write_jsonl(
            &escrow_dir.join("cannot_link.jsonl"),
            &[json!({
                "left_key": "lei:549300AAAA",
                "right_key": "lei:549300BBBB",
                "reason": "conflicting_trusted_anchor"
            })],
        );

        temp_dir
    }

    #[test]
    fn loads_incumbent_memory_in_deterministic_order() {
        let temp_dir = create_registry_tree();

        let memory = load_incumbent_memory(temp_dir.path()).unwrap();

        assert_eq!(memory.registry.id, "bdc-issuers");
        assert_eq!(memory.registry.version, "2026.03.23");
        assert_eq!(
            memory.registry.source,
            temp_dir.path().to_string_lossy().into_owned()
        );
        assert_eq!(
            memory
                .alias_entries
                .iter()
                .map(|entry| entry.input.as_str())
                .collect::<Vec<_>>(),
            vec!["ACME CORPORATION", "Acme Corp."]
        );
        assert_eq!(
            memory
                .trusted_anchors
                .iter()
                .map(|entry| entry.canonical_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ORG-001", "ORG-002"]
        );
        assert_eq!(memory.pending_clusters.len(), 1);
        assert_eq!(
            memory.pending_clusters[0].witness_pairs,
            vec![RowPair {
                left_row_id: "row-1".to_string(),
                right_row_id: "row-2".to_string(),
            }]
        );
        assert_eq!(memory.cannot_link_facts.len(), 1);
        assert!(memory.registry.lookup_snapshot_hash.starts_with("blake3:"));
        assert!(memory.registry.escrow_snapshot_hash.starts_with("blake3:"));
    }

    #[test]
    fn missing_optional_sidecars_are_tolerated() {
        let temp_dir = TempDir::new().unwrap();
        write_registry_json(temp_dir.path(), "bdc-issuers", "2026.03.23");
        write_mapping_file(
            temp_dir.path(),
            "aliases.json",
            json!([
                {
                    "input": "Acme Corp.",
                    "canonical_id": "ORG-001",
                    "canonical_type": "org_canon_id",
                    "rule_id": "PROMOTED_ALIAS"
                }
            ]),
        );

        let memory = load_incumbent_memory(temp_dir.path()).unwrap();
        assert!(memory.trusted_anchors.is_empty());
        assert!(memory.pending_clusters.is_empty());
        assert!(memory.cannot_link_facts.is_empty());
        assert_eq!(
            memory.registry.lookup_snapshot_hash,
            lookup_snapshot_hash(temp_dir.path()).unwrap()
        );
        assert_eq!(
            memory.registry.escrow_snapshot_hash,
            escrow_snapshot_hash(temp_dir.path()).unwrap()
        );
    }

    #[test]
    fn snapshot_hashes_are_stable_for_identical_trees_and_change_with_bytes() {
        let temp_dir = create_registry_tree();

        let first = load_incumbent_memory(temp_dir.path()).unwrap();
        let second = load_incumbent_memory(temp_dir.path()).unwrap();
        assert_eq!(
            first.registry.lookup_snapshot_hash,
            second.registry.lookup_snapshot_hash
        );
        assert_eq!(
            first.registry.escrow_snapshot_hash,
            second.registry.escrow_snapshot_hash
        );

        fs::write(
            temp_dir.path().join("_escrow").join("pending.jsonl"),
            "{\"escrow_id\":\"OE-123\",\"profile\":\"bdc_issuer\",\"state\":\"pending\"}\n\n",
        )
        .unwrap();
        let changed = escrow_snapshot_hash(temp_dir.path()).unwrap();
        assert_ne!(first.registry.escrow_snapshot_hash, changed);
    }

    #[test]
    fn malformed_consulted_sidecars_refuse() {
        let temp_dir = create_registry_tree();
        fs::write(
            temp_dir
                .path()
                .join("_anchors")
                .join("20260322T170000Z.anchors.jsonl"),
            "{\"canonical_id\":\n",
        )
        .unwrap();

        let error = load_incumbent_memory(temp_dir.path()).unwrap_err();
        assert_eq!(error.code, EntityErrorCode::Registry);
        assert!(error.message.contains("trusted anchor"));
    }
}
