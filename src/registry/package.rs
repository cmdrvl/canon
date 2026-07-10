use super::{effective_entries, load_registry_definition};
use crate::RegistryDiffEntry;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, fs, path::Path};

pub const REGISTRY_PACKAGE_SCHEMA_VERSION: &str = "canon.registry.package.v1";

const REGISTRY_METADATA_KIND: &str = "registry_metadata";
const MAPPING_KIND: &str = "mapping";
const BUILD_PROVENANCE_KIND: &str = "build_provenance";
const ALLOWED_ATTACHMENT_KINDS: [&str; 8] = [
    "audit",
    "gold",
    "strategy",
    "signature",
    "relation",
    "escrow",
    "export_dbt_seed",
    "export_search_index",
];
const ALLOWED_SIDECAR_KINDS: [&str; 6] = [
    "audit",
    "gold",
    "strategy",
    "signature",
    "relation",
    "escrow",
];
const ALLOWED_PROJECTION_KINDS: [&str; 2] = ["dbt-seed", "search-index"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackage {
    pub schema_version: String,
    pub registry: RegistryPackageRegistryIdentity,
    pub content_digest: String,
    pub entry_count: usize,
    pub effective_mapping_count: usize,
    pub canonical_iri_namespace: Option<String>,
    pub file_descriptors: Vec<RegistryPackageDescriptor>,
    pub build_provenance: Option<RegistryPackageDescriptor>,
    pub attachments: Vec<RegistryPackageAttachmentDescriptor>,
    pub dependency_references: Vec<RegistryPackageDependencyReference>,
    pub allowed_sidecars: Vec<String>,
    pub deployment_projections: Vec<RegistryPackageDeploymentProjection>,
    pub lookup_entries: Vec<RegistryDiffEntry>,
    pub identity: RegistryPackageIdentityRules,
    pub layouts: RegistryPackageLayouts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageRegistryIdentity {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageDescriptor {
    pub path: String,
    pub kind: String,
    pub content_digest: String,
    pub bytes: u64,
    pub entry_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageAttachmentDescriptor {
    pub path: String,
    pub kind: String,
    pub content_digest: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageDependencyReference {
    pub id: String,
    pub version: String,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageDeploymentProjection {
    pub kind: String,
    pub first_class: bool,
    pub identity_excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageIdentityRules {
    pub hash_algorithm: String,
    pub descriptor_ordering: String,
    pub mapping_precedence: String,
    pub identity_exclusions: Vec<String>,
    pub secret_material_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageLayouts {
    pub directory_layout: String,
    pub archive_layout: String,
    pub attachment_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryPackageErrorKind {
    UnsupportedSchemaVersion,
    MissingRegistryMetadata,
    MissingMappingDescriptor,
    UnknownDescriptorKind,
    UnknownAttachmentKind,
    UnknownProjectionKind,
    DuplicateDescriptorPath,
    PathTraversalDescriptor,
    InvalidContentDigest,
    InvalidPackageDigest,
    Io,
    Parse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPackageError {
    pub kind: RegistryPackageErrorKind,
    pub message: String,
}

impl RegistryPackageError {
    fn new(kind: RegistryPackageErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for RegistryPackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RegistryPackageError {}

pub fn compile_registry_package(
    registry_dir: &Path,
) -> Result<RegistryPackage, RegistryPackageError> {
    let (registry_json, _, mapping_files) =
        load_registry_definition(registry_dir).map_err(|error| {
            RegistryPackageError::new(RegistryPackageErrorKind::Parse, error.to_string())
        })?;
    let lookup_entries = effective_entries(&mapping_files);

    let registry_bytes = fs::read(registry_dir.join("registry.json")).map_err(|error| {
        RegistryPackageError::new(
            RegistryPackageErrorKind::Io,
            format!("failed to read registry.json: {error}"),
        )
    })?;
    let mut file_descriptors = vec![RegistryPackageDescriptor {
        path: "registry.json".to_string(),
        kind: REGISTRY_METADATA_KIND.to_string(),
        content_digest: hash_bytes(&registry_bytes),
        bytes: registry_bytes.len() as u64,
        entry_count: None,
    }];

    for mapping_file in &mapping_files {
        let bytes = fs::read(&mapping_file.path).map_err(|error| {
            RegistryPackageError::new(
                RegistryPackageErrorKind::Io,
                format!("failed to read {}: {error}", mapping_file.path.display()),
            )
        })?;
        file_descriptors.push(RegistryPackageDescriptor {
            path: file_name(&mapping_file.path)?,
            kind: MAPPING_KIND.to_string(),
            content_digest: hash_bytes(&bytes),
            bytes: bytes.len() as u64,
            entry_count: Some(mapping_file.entries.len()),
        });
    }

    let build_provenance =
        load_optional_descriptor(registry_dir, "_build.json", BUILD_PROVENANCE_KIND)?;

    let package = RegistryPackage {
        schema_version: REGISTRY_PACKAGE_SCHEMA_VERSION.to_string(),
        registry: RegistryPackageRegistryIdentity {
            id: registry_json.id,
            version: registry_json.version,
        },
        content_digest: String::new(),
        entry_count: mapping_files.iter().map(|file| file.entries.len()).sum(),
        effective_mapping_count: lookup_entries.len(),
        canonical_iri_namespace: registry_json.canonical_iri_namespace,
        file_descriptors,
        build_provenance,
        attachments: Vec::new(),
        dependency_references: Vec::new(),
        allowed_sidecars: ALLOWED_SIDECAR_KINDS
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        deployment_projections: ALLOWED_PROJECTION_KINDS
            .into_iter()
            .map(|kind| RegistryPackageDeploymentProjection {
                kind: kind.to_string(),
                first_class: true,
                identity_excluded: true,
            })
            .collect(),
        lookup_entries,
        identity: RegistryPackageIdentityRules {
            hash_algorithm: "blake3".to_string(),
            descriptor_ordering: "normalized_path_lexicographic".to_string(),
            mapping_precedence: "filename_lexicographic_then_entry_order".to_string(),
            identity_exclusions: vec![
                "_index.sqlite".to_string(),
                "mtime".to_string(),
                "absolute_paths".to_string(),
                "derived_caches".to_string(),
                "provider_credentials".to_string(),
                "secrets".to_string(),
            ],
            secret_material_policy: "never_include_secrets_in_package_manifest".to_string(),
        },
        layouts: RegistryPackageLayouts {
            directory_layout: "registry-package-dir.v1".to_string(),
            archive_layout: "registry-package-archive.v1".to_string(),
            attachment_root: "_attachments/".to_string(),
        },
    };

    let package = finalized_package(package)?;
    validate_registry_package(&package)?;
    Ok(package)
}

pub fn canonical_package_bytes(package: &RegistryPackage) -> Result<Vec<u8>, RegistryPackageError> {
    let canonical = canonicalized_package(package, true)?;
    serde_json::to_vec(&canonical).map_err(|error| {
        RegistryPackageError::new(
            RegistryPackageErrorKind::Parse,
            format!("failed to serialize canonical package bytes: {error}"),
        )
    })
}

pub fn parse_registry_package(bytes: &[u8]) -> Result<RegistryPackage, RegistryPackageError> {
    let package: RegistryPackage = serde_json::from_slice(bytes).map_err(|error| {
        RegistryPackageError::new(
            RegistryPackageErrorKind::Parse,
            format!("failed to parse registry package: {error}"),
        )
    })?;
    validate_registry_package(&package)?;
    canonicalized_package(&package, true)
}

fn finalized_package(package: RegistryPackage) -> Result<RegistryPackage, RegistryPackageError> {
    let mut finalized = package;
    finalized.content_digest = package_digest(&finalized)?;
    canonicalized_package(&finalized, true)
}

pub fn validate_registry_package(package: &RegistryPackage) -> Result<(), RegistryPackageError> {
    let canonical = canonicalized_package(package, true)?;

    if canonical.schema_version != REGISTRY_PACKAGE_SCHEMA_VERSION {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::UnsupportedSchemaVersion,
            format!(
                "unsupported registry package schema_version {}",
                canonical.schema_version
            ),
        ));
    }

    let mut seen_paths = BTreeSet::new();
    let mut has_registry_metadata = false;
    let mut has_mapping = false;
    for descriptor in &canonical.file_descriptors {
        validate_descriptor_kind(&descriptor.kind)?;
        validate_digest(&descriptor.content_digest)?;
        let normalized = normalize_descriptor_path(&descriptor.path)?;
        if !seen_paths.insert(normalized) {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::DuplicateDescriptorPath,
                format!("duplicate descriptor path {}", descriptor.path),
            ));
        }
        has_registry_metadata |= descriptor.kind == REGISTRY_METADATA_KIND;
        has_mapping |= descriptor.kind == MAPPING_KIND;
    }

    if !has_registry_metadata {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::MissingRegistryMetadata,
            "registry package must include a registry_metadata descriptor",
        ));
    }
    if !has_mapping {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::MissingMappingDescriptor,
            "registry package must include at least one mapping descriptor",
        ));
    }

    if let Some(build_provenance) = &canonical.build_provenance {
        validate_descriptor_kind(&build_provenance.kind)?;
        if build_provenance.kind != BUILD_PROVENANCE_KIND {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::UnknownDescriptorKind,
                format!(
                    "build_provenance descriptor must use kind {BUILD_PROVENANCE_KIND}, found {}",
                    build_provenance.kind
                ),
            ));
        }
        validate_digest(&build_provenance.content_digest)?;
        let normalized = normalize_descriptor_path(&build_provenance.path)?;
        if !seen_paths.insert(normalized) {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::DuplicateDescriptorPath,
                format!("duplicate descriptor path {}", build_provenance.path),
            ));
        }
    }

    for attachment in &canonical.attachments {
        validate_attachment_kind(&attachment.kind)?;
        validate_digest(&attachment.content_digest)?;
        let normalized = normalize_descriptor_path(&attachment.path)?;
        if !seen_paths.insert(normalized) {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::DuplicateDescriptorPath,
                format!("duplicate descriptor path {}", attachment.path),
            ));
        }
    }

    let sidecar_set = canonical.allowed_sidecars.iter().collect::<BTreeSet<_>>();
    if sidecar_set.len() != ALLOWED_SIDECAR_KINDS.len()
        || !ALLOWED_SIDECAR_KINDS
            .into_iter()
            .all(|kind| sidecar_set.contains(&kind.to_string()))
    {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::UnknownAttachmentKind,
            "allowed_sidecars must exactly match audit, gold, strategy, signature, relation, and escrow",
        ));
    }

    for projection in &canonical.deployment_projections {
        if !ALLOWED_PROJECTION_KINDS.contains(&projection.kind.as_str()) {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::UnknownProjectionKind,
                format!("unknown deployment projection kind {}", projection.kind),
            ));
        }
    }

    for dependency in &canonical.dependency_references {
        validate_digest(&dependency.content_digest)?;
    }

    let expected_digest = package_digest(&canonical)?;
    if canonical.content_digest != expected_digest {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::InvalidPackageDigest,
            format!(
                "package digest mismatch: expected {expected_digest}, found {}",
                canonical.content_digest
            ),
        ));
    }

    Ok(())
}

fn package_digest(package: &RegistryPackage) -> Result<String, RegistryPackageError> {
    let digest_view = canonicalized_package(package, false)?;
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        RegistryPackageError::new(
            RegistryPackageErrorKind::Parse,
            format!("failed to serialize package digest view: {error}"),
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn canonicalized_package(
    package: &RegistryPackage,
    include_digest: bool,
) -> Result<RegistryPackage, RegistryPackageError> {
    let mut canonical = package.clone();
    canonical.file_descriptors = canonical
        .file_descriptors
        .into_iter()
        .map(|mut descriptor| {
            descriptor.path = normalize_descriptor_path(&descriptor.path)?;
            Ok(descriptor)
        })
        .collect::<Result<Vec<_>, RegistryPackageError>>()?;
    canonical.file_descriptors.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });

    if let Some(build_provenance) = canonical.build_provenance.as_mut() {
        build_provenance.path = normalize_descriptor_path(&build_provenance.path)?;
    }

    canonical.attachments = canonical
        .attachments
        .into_iter()
        .map(|mut attachment| {
            attachment.path = normalize_descriptor_path(&attachment.path)?;
            Ok(attachment)
        })
        .collect::<Result<Vec<_>, RegistryPackageError>>()?;
    canonical.attachments.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });

    canonical.dependency_references.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
    });
    canonical.allowed_sidecars.sort_by(|left, right| {
        sidecar_kind_rank(left)
            .cmp(&sidecar_kind_rank(right))
            .then_with(|| left.cmp(right))
    });
    canonical
        .deployment_projections
        .sort_by(|left, right| left.kind.cmp(&right.kind));
    canonical
        .lookup_entries
        .sort_by(|left, right| left.input.cmp(&right.input));
    canonical.identity.identity_exclusions.sort();
    if !include_digest {
        canonical.content_digest.clear();
    }
    Ok(canonical)
}

fn load_optional_descriptor(
    registry_dir: &Path,
    file_name: &str,
    kind: &str,
) -> Result<Option<RegistryPackageDescriptor>, RegistryPackageError> {
    let path = registry_dir.join(file_name);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| {
        RegistryPackageError::new(
            RegistryPackageErrorKind::Io,
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    Ok(Some(RegistryPackageDescriptor {
        path: file_name.to_string(),
        kind: kind.to_string(),
        content_digest: hash_bytes(&bytes),
        bytes: bytes.len() as u64,
        entry_count: None,
    }))
}

fn validate_descriptor_kind(kind: &str) -> Result<(), RegistryPackageError> {
    if [REGISTRY_METADATA_KIND, MAPPING_KIND, BUILD_PROVENANCE_KIND].contains(&kind) {
        Ok(())
    } else {
        Err(RegistryPackageError::new(
            RegistryPackageErrorKind::UnknownDescriptorKind,
            format!("unknown registry package descriptor kind {kind}"),
        ))
    }
}

fn validate_attachment_kind(kind: &str) -> Result<(), RegistryPackageError> {
    if ALLOWED_ATTACHMENT_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(RegistryPackageError::new(
            RegistryPackageErrorKind::UnknownAttachmentKind,
            format!("unknown registry package attachment kind {kind}"),
        ))
    }
}

fn sidecar_kind_rank(kind: &str) -> usize {
    ALLOWED_SIDECAR_KINDS
        .iter()
        .position(|allowed| *allowed == kind)
        .unwrap_or(usize::MAX)
}

fn validate_digest(digest: &str) -> Result<(), RegistryPackageError> {
    if digest.starts_with("blake3:") && digest.len() > "blake3:".len() {
        Ok(())
    } else {
        Err(RegistryPackageError::new(
            RegistryPackageErrorKind::InvalidContentDigest,
            format!("invalid content digest {digest}"),
        ))
    }
}

fn normalize_descriptor_path(path: &str) -> Result<String, RegistryPackageError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::PathTraversalDescriptor,
            "descriptor path must not be empty",
        ));
    }
    if normalized.starts_with('/') || has_windows_drive_prefix(&normalized) {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::PathTraversalDescriptor,
            format!("descriptor path must be relative: {path}"),
        ));
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(RegistryPackageError::new(
                RegistryPackageErrorKind::PathTraversalDescriptor,
                format!("descriptor path must not traverse directories: {path}"),
            ));
        }
        segments.push(segment);
    }

    Ok(segments.join("/"))
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic()
}

fn file_name(path: &Path) -> Result<String, RegistryPackageError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| {
            RegistryPackageError::new(
                RegistryPackageErrorKind::Parse,
                format!("failed to derive file name from {}", path.display()),
            )
        })
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegistryJson;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_registry_json(
        path: &Path,
        registry_json: &RegistryJson,
    ) -> Result<(), RegistryPackageError> {
        fs::write(
            path.join("registry.json"),
            serde_json::to_vec_pretty(registry_json).map_err(|error| {
                RegistryPackageError::new(
                    RegistryPackageErrorKind::Parse,
                    format!("failed to serialize registry.json fixture: {error}"),
                )
            })?,
        )
        .map_err(|error| {
            RegistryPackageError::new(
                RegistryPackageErrorKind::Io,
                format!("failed to write registry.json fixture: {error}"),
            )
        })
    }

    #[test]
    fn canonicalized_package_normalizes_paths_and_sorts_deterministically() {
        let package = RegistryPackage {
            schema_version: REGISTRY_PACKAGE_SCHEMA_VERSION.to_string(),
            registry: RegistryPackageRegistryIdentity {
                id: "pkg".to_string(),
                version: "1.0.0".to_string(),
            },
            content_digest: "blake3:placeholder".to_string(),
            entry_count: 1,
            effective_mapping_count: 1,
            canonical_iri_namespace: None,
            file_descriptors: vec![
                RegistryPackageDescriptor {
                    path: "z\\mapping.json".to_string(),
                    kind: MAPPING_KIND.to_string(),
                    content_digest: "blake3:z".to_string(),
                    bytes: 1,
                    entry_count: Some(1),
                },
                RegistryPackageDescriptor {
                    path: "a/registry.json".to_string(),
                    kind: REGISTRY_METADATA_KIND.to_string(),
                    content_digest: "blake3:a".to_string(),
                    bytes: 1,
                    entry_count: None,
                },
            ],
            build_provenance: None,
            attachments: vec![RegistryPackageAttachmentDescriptor {
                path: "attachments\\audit.json".to_string(),
                kind: "audit".to_string(),
                content_digest: "blake3:audit".to_string(),
                bytes: 1,
            }],
            dependency_references: Vec::new(),
            allowed_sidecars: ALLOWED_SIDECAR_KINDS
                .into_iter()
                .rev()
                .map(ToString::to_string)
                .collect(),
            deployment_projections: vec![
                RegistryPackageDeploymentProjection {
                    kind: "search-index".to_string(),
                    first_class: true,
                    identity_excluded: true,
                },
                RegistryPackageDeploymentProjection {
                    kind: "dbt-seed".to_string(),
                    first_class: true,
                    identity_excluded: true,
                },
            ],
            lookup_entries: vec![RegistryDiffEntry {
                input: "b".to_string(),
                canonical_id: "B".to_string(),
                canonical_type: "issuer".to_string(),
                rule_id: "RULE".to_string(),
            }],
            identity: RegistryPackageIdentityRules {
                hash_algorithm: "blake3".to_string(),
                descriptor_ordering: "normalized_path_lexicographic".to_string(),
                mapping_precedence: "filename_lexicographic_then_entry_order".to_string(),
                identity_exclusions: vec!["mtime".to_string(), "_index.sqlite".to_string()],
                secret_material_policy: "never".to_string(),
            },
            layouts: RegistryPackageLayouts {
                directory_layout: "registry-package-dir.v1".to_string(),
                archive_layout: "registry-package-archive.v1".to_string(),
                attachment_root: "_attachments/".to_string(),
            },
        };

        let canonical = canonicalized_package(&package, true).expect("canonical package");
        assert_eq!(canonical.file_descriptors[0].path, "a/registry.json");
        assert_eq!(canonical.file_descriptors[1].path, "z/mapping.json");
        assert_eq!(canonical.attachments[0].path, "attachments/audit.json");
        assert_eq!(
            canonical.allowed_sidecars,
            ALLOWED_SIDECAR_KINDS
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(canonical.deployment_projections[0].kind, "dbt-seed");
    }

    #[test]
    fn fixture_helpers_round_trip_registry_definition() -> Result<(), RegistryPackageError> {
        let temp = TempDir::new().map_err(|error| {
            RegistryPackageError::new(RegistryPackageErrorKind::Io, error.to_string())
        })?;
        let registry_json = RegistryJson {
            id: "fixture".to_string(),
            version: "1.0.0".to_string(),
            description: "fixture".to_string(),
            updated: "2026-07-10".to_string(),
            entry_count: 1,
            canonical_iri_namespace: Some("https://example.test/fixture/".to_string()),
            default_id_scheme: None,
        };
        write_registry_json(temp.path(), &registry_json)?;
        let file_path = temp.path().join("aliases.json");
        let mut file = fs::File::create(&file_path).map_err(|error| {
            RegistryPackageError::new(RegistryPackageErrorKind::Io, error.to_string())
        })?;
        file.write_all(
            br#"[
  {"input":"A","canonical_id":"B","canonical_type":"issuer","rule_id":"RULE"}
]"#,
        )
        .map_err(|error| {
            RegistryPackageError::new(RegistryPackageErrorKind::Io, error.to_string())
        })?;
        let compiled = compile_registry_package(temp.path())?;
        assert_eq!(compiled.registry.id, "fixture");
        assert_eq!(compiled.entry_count, 1);
        assert_eq!(compiled.effective_mapping_count, 1);
        assert!(compiled.content_digest.starts_with("blake3:"));
        Ok(())
    }
}
