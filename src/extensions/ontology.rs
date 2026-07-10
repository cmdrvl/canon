#![forbid(unsafe_code)]

//! Domain-neutral ontology extension package contract.
//!
//! Ontology packages let operators define object-class vocabularies outside the
//! Canon binary. Core code only validates package mechanics: opaque type IDs,
//! content digests, deterministic ordering, and compatibility checks.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub const CANON_EXTENSION_ONTOLOGY_VERSION: &str = "canon.extension.ontology.v1";

pub type OntologyResult<T> = Result<T, OntologyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OntologyErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingType,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyError {
    pub code: OntologyErrorCode,
    pub message: String,
}

impl OntologyError {
    pub fn new(code: OntologyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for OntologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for OntologyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyPackageCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_classes: Vec<OntologyObjectClass>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<OntologyDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OntologyDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyObjectClass {
    pub type_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_type_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub display_groups: Vec<String>,
    pub canonical_id_policy_ref: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_identifier_namespace_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_vocabulary_refs: Vec<String>,
    pub temporal_behavior_ref: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OntologyTypeRef {
    pub package_digest: String,
    pub type_id: String,
}

pub fn finalize_package(mut package: OntologyPackage) -> OntologyResult<OntologyPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_EXTENSION_ONTOLOGY_VERSION.to_string();
    }
    if package.version != CANON_EXTENSION_ONTOLOGY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported ontology contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<OntologyResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();
    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<std::collections::BTreeSet<_>>();

    let mut classes = package
        .object_classes
        .into_iter()
        .map(|class| normalize_object_class(class, &package.package_id, &known_docs))
        .collect::<OntologyResult<Vec<_>>>()?;
    if classes.is_empty() {
        return Err(artifact_contract_error(
            "ontology package must declare at least one object class",
        ));
    }
    classes.sort_by(|left, right| left.type_id.cmp(&right.type_id));

    let mut deduped: Vec<OntologyObjectClass> = Vec::with_capacity(classes.len());
    for class in classes {
        if let Some(previous) = deduped.last()
            && previous.type_id == class.type_id
        {
            if previous != &class {
                return Err(artifact_contract_error(format!(
                    "object class {} cannot be declared with conflicting content",
                    class.type_id
                )));
            }
            continue;
        }
        deduped.push(class);
    }

    let class_index = deduped
        .iter()
        .map(|class| (class.type_id.clone(), class))
        .collect::<BTreeMap<_, _>>();

    for class in &deduped {
        for parent in &class.parent_type_ids {
            if parent == &class.type_id {
                return Err(artifact_contract_error(format!(
                    "object class {} cannot list itself as a parent",
                    class.type_id
                )));
            }
            if !class_index.contains_key(parent) {
                return Err(missing_type_error(format!(
                    "parent type {} is not present in package {}",
                    parent, package.package_id
                )));
            }
        }
    }

    package.object_classes = deduped;
    package.documentation = documentation;
    Ok(package)
}

pub fn finalize_type_ref(mut reference: OntologyTypeRef) -> OntologyResult<OntologyTypeRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.type_id = normalized_type_id(&reference.type_id, "type_id")?;
    Ok(reference)
}

pub fn canonical_package_bytes(package: &OntologyPackage) -> OntologyResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!("failed to serialize ontology package: {error}"))
    })
}

pub fn ontology_package_digest(package: &OntologyPackage) -> OntologyResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn validate_package_for_execution(
    package: &OntologyPackage,
    required_types: &[OntologyTypeRef],
) -> OntologyResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = ontology_package_digest(&package)?;
    for reference in required_types {
        let reference = finalize_type_ref(reference.clone())?;
        if reference.package_digest != digest {
            return Err(compatibility_policy_error(format!(
                "ontology type {} is pinned to {} but package resolved to {}",
                reference.type_id, reference.package_digest, digest
            )));
        }
        let _ = resolve_type_ref(&package, &reference)?;
    }
    Ok(digest)
}

pub fn resolve_type_ref(
    package: &OntologyPackage,
    reference: &OntologyTypeRef,
) -> OntologyResult<OntologyObjectClass> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_type_ref(reference.clone())?;
    let digest = ontology_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "ontology type {} is pinned to {} but package resolved to {}",
            reference.type_id, reference.package_digest, digest
        )));
    }

    package
        .object_classes
        .iter()
        .find(|class| class.type_id == reference.type_id)
        .cloned()
        .ok_or_else(|| missing_type_error(format!("unknown ontology type {}", reference.type_id)))
}

pub fn package_compatibility(
    locked: &OntologyPackage,
    candidate: &OntologyPackage,
    used_types: &[OntologyTypeRef],
) -> OntologyResult<OntologyPackageCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "ontology package ids differ: {} vs {}",
            locked.package_id, candidate.package_id
        )));
    }

    let locked_major = semver_major(&locked.package_version)?;
    let candidate_major = semver_major(&candidate.package_version)?;
    if locked_major != candidate_major {
        return Err(compatibility_policy_error(format!(
            "ontology package {} changed major version from {} to {}",
            locked.package_id, locked_major, candidate_major
        )));
    }

    let locked_digest = ontology_package_digest(&locked)?;
    let candidate_digest = ontology_package_digest(&candidate)?;

    for reference in used_types {
        let reference = finalize_type_ref(reference.clone())?;
        if reference.package_digest != locked_digest {
            return Err(compatibility_policy_error(format!(
                "ontology type {} is not pinned to locked package digest {}",
                reference.type_id, locked_digest
            )));
        }
        if candidate
            .object_classes
            .iter()
            .all(|class| class.type_id != reference.type_id)
        {
            return Err(missing_type_error(format!(
                "candidate ontology package {} no longer defines {}",
                candidate.package_id, reference.type_id
            )));
        }
    }

    if locked_digest == candidate_digest {
        Ok(OntologyPackageCompatibility::ExactDigest)
    } else {
        Ok(OntologyPackageCompatibility::CompatibleSameMajor)
    }
}

fn normalize_object_class(
    mut class: OntologyObjectClass,
    package_id: &str,
    known_docs: &std::collections::BTreeSet<String>,
) -> OntologyResult<OntologyObjectClass> {
    class.type_id = normalized_type_id(&class.type_id, "object_classes.type_id")?;
    let (namespace, _) = split_type_id(&class.type_id, "object_classes.type_id")?;
    if namespace != package_id {
        return Err(artifact_contract_error(format!(
            "object class {} must use package namespace {}",
            class.type_id, package_id
        )));
    }
    class.label = normalized_non_empty(&class.label, "object_classes.label")?;
    class.parent_type_ids = normalize_string_list(class.parent_type_ids, "parent_type_ids")?;
    for parent in &class.parent_type_ids {
        let (parent_namespace, _) = split_type_id(parent, "parent_type_ids")?;
        if parent_namespace != package_id {
            return Err(artifact_contract_error(format!(
                "parent type {} must use package namespace {}",
                parent, package_id
            )));
        }
    }
    class.display_groups = normalize_string_list(class.display_groups, "display_groups")?;
    class.canonical_id_policy_ref =
        normalized_non_empty(&class.canonical_id_policy_ref, "canonical_id_policy_ref")?;
    class.allowed_identifier_namespace_refs = normalize_string_list(
        class.allowed_identifier_namespace_refs,
        "allowed_identifier_namespace_refs",
    )?;
    class.allowed_vocabulary_refs =
        normalize_string_list(class.allowed_vocabulary_refs, "allowed_vocabulary_refs")?;
    class.temporal_behavior_ref =
        normalized_non_empty(&class.temporal_behavior_ref, "temporal_behavior_ref")?;
    class.documentation_refs =
        normalize_string_list(class.documentation_refs, "documentation_refs")?;
    for reference in &class.documentation_refs {
        if !known_docs.contains(reference) {
            return Err(artifact_contract_error(format!(
                "documentation ref {} is not declared in package documentation",
                reference
            )));
        }
    }
    Ok(class)
}

fn normalize_documentation_ref(
    mut reference: OntologyDocumentationRef,
) -> OntologyResult<OntologyDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_doc_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_string_list(values: Vec<String>, field: &str) -> OntologyResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<OntologyResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalized_package_id(value: &str, field: &str) -> OntologyResult<String> {
    let value = normalized_non_empty(value, field)?;
    ensure_token(&value, field)?;
    if value.contains(':') {
        return Err(artifact_contract_error(format!(
            "{field} must not contain ':'"
        )));
    }
    Ok(value)
}

fn normalized_type_id(value: &str, field: &str) -> OntologyResult<String> {
    let value = normalized_non_empty(value, field)?;
    let _ = split_type_id(&value, field)?;
    Ok(value)
}

fn split_type_id<'a>(value: &'a str, field: &str) -> OntologyResult<(&'a str, &'a str)> {
    let (namespace, name) = value.split_once(':').ok_or_else(|| {
        artifact_contract_error(format!(
            "{field} must be a namespaced opaque id in the form namespace:type"
        ))
    })?;
    ensure_token(namespace, field)?;
    ensure_token(name, field)?;
    Ok((namespace, name))
}

fn normalized_semver(value: &str, field: &str) -> OntologyResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(artifact_contract_error(format!(
            "{field} must be a simple semver string like 1.2.3"
        )));
    }
    for part in &parts {
        if !part.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(artifact_contract_error(format!(
                "{field} must be a simple semver string like 1.2.3"
            )));
        }
    }
    Ok(value)
}

fn semver_major(value: &str) -> OntologyResult<u64> {
    value
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("package_version must contain a major version"))?
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("invalid semver major: {error}")))
}

fn normalized_doc_uri(value: &str, field: &str) -> OntologyResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.contains('\\') || value.contains("/../") || value.starts_with("../") {
        return Err(artifact_contract_error(format!(
            "{field} must not contain path traversal"
        )));
    }
    if value.starts_with('/') {
        return Err(artifact_contract_error(format!(
            "{field} must not be absolute"
        )));
    }
    if value.contains("://") && !(value.starts_with("https://") || value.starts_with("http://")) {
        return Err(artifact_contract_error(format!(
            "{field} must use http(s) when a URI scheme is present"
        )));
    }
    Ok(value)
}

fn ensure_token(value: &str, field: &str) -> OntologyResult<()> {
    if !value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(artifact_contract_error(format!(
            "{field} must contain only lowercase ascii letters, digits, '.', '_' or '-'"
        )));
    }
    Ok(())
}

fn normalized_hash(value: &str, field: &str) -> OntologyResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must contain 64 lowercase hex characters"
        )));
    }
    Ok(value)
}

fn normalized_non_empty(value: &str, field: &str) -> OntologyResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value.to_string())
}

fn artifact_contract_error(message: impl Into<String>) -> OntologyError {
    OntologyError::new(OntologyErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> OntologyError {
    OntologyError::new(OntologyErrorCode::CompatibilityPolicy, message)
}

fn missing_type_error(message: impl Into<String>) -> OntologyError {
    OntologyError::new(OntologyErrorCode::MissingType, message)
}
