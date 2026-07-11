use super::{effective_entries, load_registry_definition};
use crate::RegistryDiffEntry;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
};

pub const REGISTRY_PACKAGE_SCHEMA_VERSION: &str = "canon.registry.package.v1";
pub const REGISTRY_PACKAGE_VERIFY_SCHEMA_VERSION: &str = "canon.registry.package.verify.v1";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryPackageFindingSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryPackageVerificationFinding {
    pub severity: RegistryPackageFindingSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub detail: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackageVerificationSummary {
    pub checked_files: usize,
    pub checked_bytes: u64,
    pub entry_count: usize,
    pub effective_mapping_count: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryPackageVerificationReport {
    pub version: String,
    pub registry: RegistryPackageRegistryIdentity,
    pub package_digest: String,
    pub verified: bool,
    pub summary: RegistryPackageVerificationSummary,
    pub findings: Vec<RegistryPackageVerificationFinding>,
}

impl RegistryPackageVerificationReport {
    pub fn render_summary(&self) -> String {
        format!(
            "{}@{} package verify | verified={} findings={} errors={} warnings={} info={}",
            self.registry.id,
            self.registry.version,
            self.verified,
            self.findings.len(),
            self.summary.errors,
            self.summary.warnings,
            self.summary.info,
        )
    }
}

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

pub fn verify_registry_package(
    registry_dir: &Path,
    package: &RegistryPackage,
) -> Result<RegistryPackageVerificationReport, RegistryPackageError> {
    let mut findings = Vec::new();
    let canonical = match canonicalized_package(package, true) {
        Ok(canonical) => canonical,
        Err(error) => {
            push_package_finding(
                &mut findings,
                RegistryPackageFindingSeverity::Error,
                "package_canonicalization_failed",
                format!("Registry package cannot be canonicalized: {error}"),
                None,
                json!({
                    "error_kind": format!("{:?}", error.kind),
                    "error": error.message,
                }),
            );
            package.clone()
        }
    };

    if let Err(error) = validate_registry_package(package) {
        push_package_finding(
            &mut findings,
            RegistryPackageFindingSeverity::Error,
            "package_structural_validation_failed",
            format!("Registry package structure is invalid: {error}"),
            None,
            json!({
                "error_kind": format!("{:?}", error.kind),
                "error": error.message,
            }),
        );
    }

    verify_package_contract(&canonical, &mut findings);
    verify_declared_files(registry_dir, &canonical, &mut findings);

    match load_registry_definition(registry_dir) {
        Ok((registry_json, _, mapping_files)) => {
            let actual_entry_count = mapping_files
                .iter()
                .map(|file| file.entries.len())
                .sum::<usize>();
            if registry_json.entry_count != actual_entry_count {
                push_package_finding(
                    &mut findings,
                    RegistryPackageFindingSeverity::Error,
                    "registry_entry_count_mismatch",
                    format!(
                        "registry.json entry_count ({}) differs from actual mapping entries ({actual_entry_count})",
                        registry_json.entry_count
                    ),
                    Some("registry.json".to_string()),
                    json!({
                        "registry_entry_count": registry_json.entry_count,
                        "actual_entry_count": actual_entry_count,
                    }),
                );
            }
            verify_mapping_descriptor_counts(&canonical, &mapping_files, &mut findings);
            verify_duplicate_mapping_inputs(&mapping_files, &mut findings);
        }
        Err(error) => push_package_finding(
            &mut findings,
            RegistryPackageFindingSeverity::Error,
            "local_registry_unreadable",
            format!("Local registry could not be loaded for package verification: {error}"),
            None,
            json!({ "error": error.to_string() }),
        ),
    }

    match compile_registry_package(registry_dir) {
        Ok(local_package) => {
            compare_against_local_package(&canonical, &local_package, &mut findings)
        }
        Err(error) => push_package_finding(
            &mut findings,
            RegistryPackageFindingSeverity::Error,
            "local_package_recompute_failed",
            format!("Local registry package could not be recomputed: {error}"),
            None,
            json!({
                "error_kind": format!("{:?}", error.kind),
                "error": error.message,
            }),
        ),
    }

    findings.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.message.cmp(&right.message))
    });

    let summary = summarize_verification(&canonical, &findings);
    Ok(RegistryPackageVerificationReport {
        version: REGISTRY_PACKAGE_VERIFY_SCHEMA_VERSION.to_string(),
        registry: canonical.registry.clone(),
        package_digest: canonical.content_digest.clone(),
        verified: summary.errors == 0,
        summary,
        findings,
    })
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
    validate_digest(&canonical.content_digest)?;

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

fn verify_package_contract(
    package: &RegistryPackage,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    for (field, actual, expected) in [
        (
            "identity.hash_algorithm",
            package.identity.hash_algorithm.as_str(),
            "blake3",
        ),
        (
            "identity.descriptor_ordering",
            package.identity.descriptor_ordering.as_str(),
            "normalized_path_lexicographic",
        ),
        (
            "identity.mapping_precedence",
            package.identity.mapping_precedence.as_str(),
            "filename_lexicographic_then_entry_order",
        ),
        (
            "identity.secret_material_policy",
            package.identity.secret_material_policy.as_str(),
            "never_include_secrets_in_package_manifest",
        ),
    ] {
        if actual != expected {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "package_contract_mismatch",
                format!("{field} must be {expected}, found {actual}"),
                None,
                json!({ "field": field, "expected": expected, "actual": actual }),
            );
        }
    }

    if package.layouts.attachment_root.trim().is_empty() {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "attachment_root_empty",
            "Package attachment_root must be non-empty",
            None,
            Value::Null,
        );
    }

    let attachment_root = package.layouts.attachment_root.replace('\\', "/");
    for attachment in &package.attachments {
        if !attachment.path.starts_with(&attachment_root) {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "attachment_scope_invalid",
                format!(
                    "Attachment '{}' is outside declared attachment root '{}'",
                    attachment.path, package.layouts.attachment_root
                ),
                Some(attachment.path.clone()),
                json!({
                    "attachment_kind": attachment.kind,
                    "attachment_root": package.layouts.attachment_root,
                }),
            );
        }
        if attachment.kind == "signature" && attachment.bytes == 0 {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "signature_reference_empty",
                format!(
                    "Signature attachment '{}' declares zero bytes",
                    attachment.path
                ),
                Some(attachment.path.clone()),
                Value::Null,
            );
        }
    }

    let mut seen_dependencies = BTreeSet::new();
    for dependency in &package.dependency_references {
        if dependency.id.trim().is_empty() || dependency.version.trim().is_empty() {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "dependency_pin_incomplete",
                "Dependency references must pin non-empty id and version",
                None,
                json!({
                    "id": dependency.id,
                    "version": dependency.version,
                }),
            );
        }
        let key = (&dependency.id, &dependency.version);
        if !seen_dependencies.insert(key) {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Warning,
                "dependency_pin_duplicate",
                format!(
                    "Dependency pin '{}@{}' is declared more than once",
                    dependency.id, dependency.version
                ),
                None,
                json!({
                    "id": dependency.id,
                    "version": dependency.version,
                }),
            );
        }
    }

    for projection in &package.deployment_projections {
        if !projection.first_class || !projection.identity_excluded {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Warning,
                "deployment_projection_capability_mismatch",
                format!(
                    "Deployment projection '{}' should be first-class and identity-excluded",
                    projection.kind
                ),
                None,
                json!({
                    "kind": projection.kind,
                    "first_class": projection.first_class,
                    "identity_excluded": projection.identity_excluded,
                }),
            );
        }
    }
}

fn verify_declared_files(
    registry_dir: &Path,
    package: &RegistryPackage,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    for descriptor in &package.file_descriptors {
        verify_descriptor_file(registry_dir, descriptor, findings);
    }
    if let Some(build_provenance) = &package.build_provenance {
        verify_descriptor_file(registry_dir, build_provenance, findings);
        verify_build_provenance_json(registry_dir, build_provenance, findings);
    }
    for attachment in &package.attachments {
        verify_attachment_file(registry_dir, attachment, findings);
    }
}

fn verify_descriptor_file(
    registry_dir: &Path,
    descriptor: &RegistryPackageDescriptor,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    let Some((normalized, bytes)) =
        read_declared_file(registry_dir, &descriptor.path, "descriptor", findings)
    else {
        return;
    };

    let actual_digest = hash_bytes(&bytes);
    if descriptor.content_digest != actual_digest {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "descriptor_digest_mismatch",
            format!(
                "Descriptor '{}' digest does not match local bytes",
                descriptor.path
            ),
            Some(normalized.clone()),
            json!({
                "expected": descriptor.content_digest,
                "actual": actual_digest,
            }),
        );
    }
    if descriptor.bytes != bytes.len() as u64 {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "descriptor_size_mismatch",
            format!(
                "Descriptor '{}' byte length does not match local bytes",
                descriptor.path
            ),
            Some(normalized.clone()),
            json!({
                "expected": descriptor.bytes,
                "actual": bytes.len(),
            }),
        );
    }

    if descriptor.kind == MAPPING_KIND {
        match serde_json::from_slice::<Vec<super::MappingEntry>>(&bytes) {
            Ok(entries) => {
                if descriptor.entry_count != Some(entries.len()) {
                    push_package_finding(
                        findings,
                        RegistryPackageFindingSeverity::Error,
                        "mapping_descriptor_entry_count_mismatch",
                        format!(
                            "Mapping descriptor '{}' entry_count does not match local mapping entries",
                            descriptor.path
                        ),
                        Some(normalized),
                        json!({
                            "descriptor_entry_count": descriptor.entry_count,
                            "actual_entry_count": entries.len(),
                        }),
                    );
                }
            }
            Err(error) => push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "mapping_descriptor_malformed",
                format!(
                    "Mapping descriptor '{}' is not valid mapping JSON: {error}",
                    descriptor.path
                ),
                Some(normalized),
                json!({ "error": error.to_string() }),
            ),
        }
    }
}

fn verify_attachment_file(
    registry_dir: &Path,
    attachment: &RegistryPackageAttachmentDescriptor,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    let Some((normalized, bytes)) =
        read_declared_file(registry_dir, &attachment.path, "attachment", findings)
    else {
        return;
    };
    let actual_digest = hash_bytes(&bytes);
    if attachment.content_digest != actual_digest {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "attachment_digest_mismatch",
            format!(
                "Attachment '{}' digest does not match local bytes",
                attachment.path
            ),
            Some(normalized.clone()),
            json!({
                "attachment_kind": attachment.kind,
                "expected": attachment.content_digest,
                "actual": actual_digest,
            }),
        );
    }
    if attachment.bytes != bytes.len() as u64 {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "attachment_size_mismatch",
            format!(
                "Attachment '{}' byte length does not match local bytes",
                attachment.path
            ),
            Some(normalized),
            json!({
                "attachment_kind": attachment.kind,
                "expected": attachment.bytes,
                "actual": bytes.len(),
            }),
        );
    }
}

fn verify_build_provenance_json(
    registry_dir: &Path,
    descriptor: &RegistryPackageDescriptor,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    let Ok(normalized) = normalize_descriptor_path(&descriptor.path) else {
        return;
    };
    let path = registry_dir.join(&normalized);
    let Ok(bytes) = fs::read(&path) else {
        return;
    };
    if !serde_json::from_slice::<Value>(&bytes).is_ok_and(|value| value.is_object()) {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "build_provenance_malformed",
            "Build provenance descriptor must point to a JSON object",
            Some(normalized),
            Value::Null,
        );
    }
}

fn read_declared_file(
    registry_dir: &Path,
    package_path: &str,
    label: &str,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) -> Option<(String, Vec<u8>)> {
    let normalized = match normalize_descriptor_path(package_path) {
        Ok(normalized) => normalized,
        Err(error) => {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "declared_path_invalid",
                format!("{label} path '{package_path}' is invalid: {error}"),
                Some(package_path.to_string()),
                json!({
                    "error_kind": format!("{:?}", error.kind),
                    "error": error.message,
                }),
            );
            return None;
        }
    };
    let path = registry_dir.join(&normalized);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "declared_file_missing",
                format!("{label} '{}' is absent from the local registry", normalized),
                Some(normalized),
                json!({ "error": error.to_string() }),
            );
            return None;
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "declared_file_symlink",
            format!(
                "{label} '{}' must be a regular file, not a symlink",
                normalized
            ),
            Some(normalized),
            Value::Null,
        );
        return None;
    }
    if !file_type.is_file() {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "declared_file_not_regular",
            format!("{label} '{}' must be a regular file", normalized),
            Some(normalized),
            Value::Null,
        );
        return None;
    }
    match fs::read(&path) {
        Ok(bytes) => Some((normalized, bytes)),
        Err(error) => {
            push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "declared_file_unreadable",
                format!("{label} '{}' could not be read: {error}", normalized),
                Some(normalized),
                json!({ "error": error.to_string() }),
            );
            None
        }
    }
}

fn verify_mapping_descriptor_counts(
    package: &RegistryPackage,
    mapping_files: &[super::MappingFile],
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    let mut actual_counts = BTreeMap::new();
    for mapping_file in mapping_files {
        if let Ok(name) = file_name(&mapping_file.path) {
            actual_counts.insert(name, mapping_file.entries.len());
        }
    }

    for descriptor in package
        .file_descriptors
        .iter()
        .filter(|descriptor| descriptor.kind == MAPPING_KIND)
    {
        match actual_counts.get(&descriptor.path) {
            Some(actual) if descriptor.entry_count != Some(*actual) => push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "mapping_descriptor_entry_count_mismatch",
                format!(
                    "Mapping descriptor '{}' entry_count does not match local registry material",
                    descriptor.path
                ),
                Some(descriptor.path.clone()),
                json!({
                    "descriptor_entry_count": descriptor.entry_count,
                    "actual_entry_count": actual,
                }),
            ),
            None => push_package_finding(
                findings,
                RegistryPackageFindingSeverity::Error,
                "mapping_descriptor_not_discovered",
                format!(
                    "Mapping descriptor '{}' is not discovered as a root mapping file",
                    descriptor.path
                ),
                Some(descriptor.path.clone()),
                Value::Null,
            ),
            _ => {}
        }
    }
}

fn verify_duplicate_mapping_inputs(
    mapping_files: &[super::MappingFile],
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    let mut first_by_input = BTreeMap::<&str, (&Path, usize, &super::MappingEntry)>::new();
    for mapping_file in mapping_files {
        for (entry_order, entry) in mapping_file.entries.iter().enumerate() {
            if let Some((first_file, first_order, first_entry)) =
                first_by_input.get(entry.input.as_str())
            {
                let exact_duplicate = first_entry.canonical_id == entry.canonical_id
                    && first_entry.canonical_type == entry.canonical_type
                    && first_entry.rule_id == entry.rule_id;
                push_package_finding(
                    findings,
                    RegistryPackageFindingSeverity::Error,
                    if exact_duplicate {
                        "duplicate_mapping_input"
                    } else {
                        "shadowed_mapping_input"
                    },
                    format!(
                        "Mapping input '{}' is duplicated and shadowed by precedence",
                        entry.input
                    ),
                    Some(
                        file_name(&mapping_file.path)
                            .unwrap_or_else(|_| mapping_file.path.display().to_string()),
                    ),
                    json!({
                        "input": entry.input,
                        "first": {
                            "source_file": file_name(first_file).unwrap_or_else(|_| first_file.display().to_string()),
                            "entry_order": first_order,
                            "canonical_id": first_entry.canonical_id,
                            "canonical_type": first_entry.canonical_type,
                            "rule_id": first_entry.rule_id,
                        },
                        "shadowed": {
                            "source_file": file_name(&mapping_file.path).unwrap_or_else(|_| mapping_file.path.display().to_string()),
                            "entry_order": entry_order,
                            "canonical_id": entry.canonical_id,
                            "canonical_type": entry.canonical_type,
                            "rule_id": entry.rule_id,
                        }
                    }),
                );
            } else {
                first_by_input.insert(
                    entry.input.as_str(),
                    (&mapping_file.path, entry_order, entry),
                );
            }
        }
    }
}

fn compare_against_local_package(
    package: &RegistryPackage,
    local_package: &RegistryPackage,
    findings: &mut Vec<RegistryPackageVerificationFinding>,
) {
    if package.registry != local_package.registry {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "registry_identity_mismatch",
            "Package registry identity does not match local registry metadata",
            Some("registry.json".to_string()),
            json!({
                "package": package.registry,
                "local": local_package.registry,
            }),
        );
    }
    if package.entry_count != local_package.entry_count {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "entry_count_mismatch",
            "Package entry_count does not match recomputed local package",
            None,
            json!({
                "package": package.entry_count,
                "local": local_package.entry_count,
            }),
        );
    }
    if package.effective_mapping_count != local_package.effective_mapping_count {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "effective_mapping_count_mismatch",
            "Package effective_mapping_count does not match recomputed local package",
            None,
            json!({
                "package": package.effective_mapping_count,
                "local": local_package.effective_mapping_count,
            }),
        );
    }
    if package.file_descriptors != local_package.file_descriptors {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "descriptor_inventory_mismatch",
            "Package file descriptor inventory does not match recomputed local package",
            None,
            json!({
                "package": package.file_descriptors,
                "local": local_package.file_descriptors,
            }),
        );
    }
    if package.build_provenance != local_package.build_provenance {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "build_provenance_mismatch",
            "Package build provenance descriptor does not match recomputed local package",
            None,
            json!({
                "package": package.build_provenance,
                "local": local_package.build_provenance,
            }),
        );
    }
    if package.lookup_entries != local_package.lookup_entries {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "effective_mappings_mismatch",
            "Package lookup_entries do not match recomputed effective mappings",
            None,
            json!({
                "package": package.lookup_entries,
                "local": local_package.lookup_entries,
            }),
        );
    }
    if package.content_digest != local_package.content_digest {
        push_package_finding(
            findings,
            RegistryPackageFindingSeverity::Error,
            "package_digest_mismatch",
            "Package content_digest does not match recomputed local package digest",
            None,
            json!({
                "package": package.content_digest,
                "local": local_package.content_digest,
            }),
        );
    }
}

fn summarize_verification(
    package: &RegistryPackage,
    findings: &[RegistryPackageVerificationFinding],
) -> RegistryPackageVerificationSummary {
    let checked_files = package.file_descriptors.len()
        + usize::from(package.build_provenance.is_some())
        + package.attachments.len();
    let checked_bytes = package
        .file_descriptors
        .iter()
        .map(|descriptor| descriptor.bytes)
        .chain(
            package
                .build_provenance
                .iter()
                .map(|descriptor| descriptor.bytes),
        )
        .chain(
            package
                .attachments
                .iter()
                .map(|attachment| attachment.bytes),
        )
        .sum();
    RegistryPackageVerificationSummary {
        checked_files,
        checked_bytes,
        entry_count: package.entry_count,
        effective_mapping_count: package.effective_mapping_count,
        errors: findings
            .iter()
            .filter(|finding| finding.severity == RegistryPackageFindingSeverity::Error)
            .count(),
        warnings: findings
            .iter()
            .filter(|finding| finding.severity == RegistryPackageFindingSeverity::Warning)
            .count(),
        info: findings
            .iter()
            .filter(|finding| finding.severity == RegistryPackageFindingSeverity::Info)
            .count(),
    }
}

fn push_package_finding(
    findings: &mut Vec<RegistryPackageVerificationFinding>,
    severity: RegistryPackageFindingSeverity,
    code: &str,
    message: impl Into<String>,
    path: Option<String>,
    detail: Value,
) {
    findings.push(RegistryPackageVerificationFinding {
        severity,
        code: code.to_string(),
        message: message.into(),
        path,
        detail,
    });
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
    let Some(hex) = digest.strip_prefix("blake3:") else {
        return Err(RegistryPackageError::new(
            RegistryPackageErrorKind::InvalidContentDigest,
            format!("invalid content digest {digest}"),
        ));
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
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
            content_digest: hash_bytes(b"package"),
            entry_count: 1,
            effective_mapping_count: 1,
            canonical_iri_namespace: None,
            file_descriptors: vec![
                RegistryPackageDescriptor {
                    path: "z\\mapping.json".to_string(),
                    kind: MAPPING_KIND.to_string(),
                    content_digest: hash_bytes(b"z"),
                    bytes: 1,
                    entry_count: Some(1),
                },
                RegistryPackageDescriptor {
                    path: "a/registry.json".to_string(),
                    kind: REGISTRY_METADATA_KIND.to_string(),
                    content_digest: hash_bytes(b"a"),
                    bytes: 1,
                    entry_count: None,
                },
            ],
            build_provenance: None,
            attachments: vec![RegistryPackageAttachmentDescriptor {
                path: "attachments\\audit.json".to_string(),
                kind: "audit".to_string(),
                content_digest: hash_bytes(b"audit"),
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
        assert_eq!(compiled.content_digest.len(), "blake3:".len() + 64);
        assert!(
            compiled
                .content_digest
                .strip_prefix("blake3:")
                .expect("compiled content digest prefix")
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        Ok(())
    }
}
