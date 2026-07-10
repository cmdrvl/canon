#![forbid(unsafe_code)]

//! Domain-neutral role and relationship vocabulary extension package contract.
//!
//! Vocabulary packages let operators define namespaced roles, predicates, and
//! intake synonyms outside the Canon binary. Core code only validates package
//! mechanics, opaque IDs, deterministic synonym normalization, and declared
//! relation constraints. Relation facts keep party IDs opaque and never imply
//! identity on their own.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fmt};

pub const CANON_EXTENSION_VOCABULARY_VERSION: &str = "canon.extension.vocabulary.v1";

pub type VocabularyResult<T> = Result<T, VocabularyError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingTerm,
    AmbiguousSynonym,
    ConstraintViolation,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabularyError {
    pub code: VocabularyErrorCode,
    pub message: String,
}

impl VocabularyError {
    pub fn new(code: VocabularyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for VocabularyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for VocabularyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyPackageCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyTermKind {
    Role,
    Relationship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationDirection {
    Directed,
    Undirected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardinalityHint {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalRequirement {
    Required,
    Optional,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabularyPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<VocabularyTerm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<VocabularyDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VocabularyDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabularyTerm {
    pub term_id: String,
    pub kind: VocabularyTermKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intake_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_type_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_type_refs: Vec<String>,
    pub direction: RelationDirection,
    pub cardinality_hint: CardinalityHint,
    pub interval_requirement: IntervalRequirement,
    #[serde(default)]
    pub identity_implication: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VocabularyTermRef {
    pub package_digest: String,
    pub term_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationInterval {
    pub start_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationFact {
    pub package_digest: String,
    pub term_id: String,
    pub subject_id: String,
    pub subject_type_ref: String,
    pub object_id: String,
    pub object_type_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<RelationInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRelationFact {
    pub relation: RelationFact,
    pub term: VocabularyTerm,
}

pub fn finalize_package(mut package: VocabularyPackage) -> VocabularyResult<VocabularyPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_EXTENSION_VOCABULARY_VERSION.to_string();
    }
    if package.version != CANON_EXTENSION_VOCABULARY_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported vocabulary contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<VocabularyResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();
    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<BTreeSet<_>>();

    let mut terms = package
        .terms
        .into_iter()
        .map(|term| normalize_term(term, &package.package_id, &known_docs))
        .collect::<VocabularyResult<Vec<_>>>()?;
    if terms.is_empty() {
        return Err(artifact_contract_error(
            "vocabulary package must declare at least one term",
        ));
    }
    terms.sort_by(|left, right| left.term_id.cmp(&right.term_id));

    let mut deduped: Vec<VocabularyTerm> = Vec::with_capacity(terms.len());
    for term in terms {
        if let Some(previous) = deduped.last()
            && previous.term_id == term.term_id
        {
            if previous != &term {
                return Err(artifact_contract_error(format!(
                    "term {} cannot be declared with conflicting content",
                    term.term_id
                )));
            }
            continue;
        }
        deduped.push(term);
    }

    package.documentation = documentation;
    package.terms = deduped;
    Ok(package)
}

pub fn canonical_package_bytes(package: &VocabularyPackage) -> VocabularyResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!("failed to serialize vocabulary package: {error}"))
    })
}

pub fn vocabulary_package_digest(package: &VocabularyPackage) -> VocabularyResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn finalize_term_ref(mut reference: VocabularyTermRef) -> VocabularyResult<VocabularyTermRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.term_id = normalized_opaque_ref(&reference.term_id, "term_id")?;
    Ok(reference)
}

pub fn resolve_term_ref(
    package: &VocabularyPackage,
    reference: &VocabularyTermRef,
) -> VocabularyResult<VocabularyTerm> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_term_ref(reference.clone())?;
    let digest = vocabulary_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "vocabulary term {} is pinned to {} but package resolved to {}",
            reference.term_id, reference.package_digest, digest
        )));
    }

    package
        .terms
        .iter()
        .find(|term| term.term_id == reference.term_id)
        .cloned()
        .ok_or_else(|| missing_term_error(format!("unknown vocabulary term {}", reference.term_id)))
}

pub fn validate_package_for_execution(
    package: &VocabularyPackage,
    required_terms: &[VocabularyTermRef],
) -> VocabularyResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = vocabulary_package_digest(&package)?;
    for reference in required_terms {
        let reference = finalize_term_ref(reference.clone())?;
        if reference.package_digest != digest {
            return Err(compatibility_policy_error(format!(
                "vocabulary term {} is pinned to {} but package resolved to {}",
                reference.term_id, reference.package_digest, digest
            )));
        }
        let _ = resolve_term_ref(&package, &reference)?;
    }
    Ok(digest)
}

pub fn package_compatibility(
    locked: &VocabularyPackage,
    candidate: &VocabularyPackage,
    used_terms: &[VocabularyTermRef],
) -> VocabularyResult<VocabularyPackageCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    if locked.package_id != candidate.package_id {
        return Err(compatibility_policy_error(format!(
            "vocabulary package ids differ: {} vs {}",
            locked.package_id, candidate.package_id
        )));
    }

    let locked_major = semver_major(&locked.package_version)?;
    let candidate_major = semver_major(&candidate.package_version)?;
    if locked_major != candidate_major {
        return Err(compatibility_policy_error(format!(
            "vocabulary package {} changed major version from {} to {}",
            locked.package_id, locked_major, candidate_major
        )));
    }

    let locked_digest = vocabulary_package_digest(&locked)?;
    let candidate_digest = vocabulary_package_digest(&candidate)?;
    for reference in used_terms {
        let reference = finalize_term_ref(reference.clone())?;
        if reference.package_digest != locked_digest {
            return Err(compatibility_policy_error(format!(
                "vocabulary term {} is not pinned to locked package digest {}",
                reference.term_id, locked_digest
            )));
        }
        if candidate
            .terms
            .iter()
            .all(|term| term.term_id != reference.term_id)
        {
            return Err(missing_term_error(format!(
                "candidate vocabulary package {} no longer defines {}",
                candidate.package_id, reference.term_id
            )));
        }
    }

    Ok(if locked_digest == candidate_digest {
        VocabularyPackageCompatibility::ExactDigest
    } else {
        VocabularyPackageCompatibility::CompatibleSameMajor
    })
}

pub fn normalize_term_name(
    package: &VocabularyPackage,
    intake_name: &str,
) -> VocabularyResult<VocabularyTermRef> {
    let package = finalize_package(package.clone())?;
    let digest = vocabulary_package_digest(&package)?;
    let lookup_name = normalized_lookup_name(intake_name, "intake_name")?;

    let mut matches = package
        .terms
        .iter()
        .filter(|term| term_lookup_names(term).contains(&lookup_name))
        .map(|term| term.term_id.clone())
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();

    match matches.as_slice() {
        [term_id] => Ok(VocabularyTermRef {
            package_digest: digest,
            term_id: term_id.clone(),
        }),
        [] => Err(missing_term_error(format!(
            "unknown vocabulary intake name {lookup_name}"
        ))),
        _ => Err(ambiguous_synonym_error(format!(
            "intake name {lookup_name} matches multiple terms: {}",
            matches.join(", ")
        ))),
    }
}

pub fn validate_relation_fact(
    package: &VocabularyPackage,
    relation: &RelationFact,
) -> VocabularyResult<ValidatedRelationFact> {
    let package = finalize_package(package.clone())?;
    let relation = finalize_relation_fact(relation.clone())?;
    let term = resolve_term_ref(
        &package,
        &VocabularyTermRef {
            package_digest: relation.package_digest.clone(),
            term_id: relation.term_id.clone(),
        },
    )?;
    validate_relation_against_term(&relation, &term)?;
    Ok(ValidatedRelationFact { relation, term })
}

fn validate_relation_against_term(
    relation: &RelationFact,
    term: &VocabularyTerm,
) -> VocabularyResult<()> {
    if term.identity_implication {
        return Err(artifact_contract_error(format!(
            "term {} cannot imply identity",
            term.term_id
        )));
    }

    match term.interval_requirement {
        IntervalRequirement::Required if relation.interval.is_none() => {
            return Err(constraint_violation_error(format!(
                "term {} requires an interval",
                term.term_id
            )));
        }
        IntervalRequirement::Forbidden if relation.interval.is_some() => {
            return Err(constraint_violation_error(format!(
                "term {} forbids intervals",
                term.term_id
            )));
        }
        _ => {}
    }

    let direct_match = type_constraints_match(
        &relation.subject_type_ref,
        &term.subject_type_refs,
        "subject_type_ref",
    ) && type_constraints_match(
        &relation.object_type_ref,
        &term.object_type_refs,
        "object_type_ref",
    );

    if direct_match {
        return Ok(());
    }

    if matches!(term.direction, RelationDirection::Undirected)
        && type_constraints_match(
            &relation.subject_type_ref,
            &term.object_type_refs,
            "subject_type_ref",
        )
        && type_constraints_match(
            &relation.object_type_ref,
            &term.subject_type_refs,
            "object_type_ref",
        )
    {
        return Ok(());
    }

    Err(constraint_violation_error(format!(
        "relation fact does not satisfy subject/object constraints for term {}",
        term.term_id
    )))
}

fn type_constraints_match(value: &str, allowed: &[String], _field: &str) -> bool {
    allowed.is_empty() || allowed.iter().any(|candidate| candidate == value)
}

fn finalize_relation_fact(mut relation: RelationFact) -> VocabularyResult<RelationFact> {
    relation.package_digest = normalized_hash(&relation.package_digest, "package_digest")?;
    relation.term_id = normalized_opaque_ref(&relation.term_id, "term_id")?;
    relation.subject_id = normalized_non_empty(&relation.subject_id, "subject_id")?;
    relation.subject_type_ref =
        normalized_opaque_ref(&relation.subject_type_ref, "subject_type_ref")?;
    relation.object_id = normalized_non_empty(&relation.object_id, "object_id")?;
    relation.object_type_ref = normalized_opaque_ref(&relation.object_type_ref, "object_type_ref")?;
    relation.interval = relation
        .interval
        .map(normalize_relation_interval)
        .transpose()?;
    Ok(relation)
}

fn normalize_relation_interval(
    mut interval: RelationInterval,
) -> VocabularyResult<RelationInterval> {
    interval.start_at = normalized_non_empty(&interval.start_at, "interval.start_at")?;
    interval.end_at = interval
        .end_at
        .take()
        .map(|value| normalized_non_empty(&value, "interval.end_at"))
        .transpose()?;
    if let Some(end_at) = interval.end_at.as_deref()
        && interval.start_at.as_str() > end_at
    {
        return Err(artifact_contract_error(
            "interval.start_at must be <= interval.end_at",
        ));
    }
    Ok(interval)
}

fn normalize_documentation_ref(
    mut reference: VocabularyDocumentationRef,
) -> VocabularyResult<VocabularyDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_documentation_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_term(
    mut term: VocabularyTerm,
    package_id: &str,
    known_docs: &BTreeSet<String>,
) -> VocabularyResult<VocabularyTerm> {
    term.term_id = normalized_opaque_ref(&term.term_id, "term_id")?;
    if !term.term_id.starts_with(&format!("{package_id}:")) {
        return Err(artifact_contract_error(format!(
            "term {} must be namespaced under package {}",
            term.term_id, package_id
        )));
    }
    term.label = normalized_non_empty(&term.label, "label")?;
    term.intake_names =
        normalize_lookup_name_list(term.intake_names, "intake_names", Some(&term.label))?;
    term.subject_type_refs =
        normalize_opaque_ref_list(term.subject_type_refs, "subject_type_refs")?;
    term.object_type_refs = normalize_opaque_ref_list(term.object_type_refs, "object_type_refs")?;
    term.documentation_refs = normalize_documentation_ref_list(
        term.documentation_refs,
        known_docs,
        "documentation_refs",
    )?;
    if term.identity_implication {
        return Err(artifact_contract_error(format!(
            "term {} cannot set identity_implication=true",
            term.term_id
        )));
    }
    Ok(term)
}

fn normalize_lookup_name_list(
    values: Vec<String>,
    field: &str,
    label: Option<&str>,
) -> VocabularyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_lookup_name(&value, field))
        .collect::<VocabularyResult<Vec<_>>>()?;
    if let Some(label) = label {
        normalized.push(normalized_lookup_name(label, "label")?);
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_opaque_ref_list(values: Vec<String>, field: &str) -> VocabularyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_opaque_ref(&value, field))
        .collect::<VocabularyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_documentation_ref_list(
    values: Vec<String>,
    known_docs: &BTreeSet<String>,
    field: &str,
) -> VocabularyResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_documentation_uri(&value, field))
        .collect::<VocabularyResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    for value in &normalized {
        if !known_docs.contains(value) {
            return Err(artifact_contract_error(format!(
                "documentation ref {} is not present in package documentation",
                value
            )));
        }
    }
    Ok(normalized)
}

fn term_lookup_names(term: &VocabularyTerm) -> BTreeSet<String> {
    term.intake_names.iter().cloned().collect()
}

fn normalized_package_id(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if !normalized.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(artifact_contract_error(format!(
            "{field} must use lowercase [a-z0-9._-] characters"
        )));
    }
    Ok(normalized)
}

fn normalized_semver(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let parts = normalized.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(artifact_contract_error(format!(
            "{field} must use MAJOR.MINOR.PATCH numeric semver"
        )));
    }
    Ok(normalized)
}

fn semver_major(value: &str) -> VocabularyResult<u64> {
    value
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("semver missing major version"))?
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("invalid semver major version: {error}")))
}

fn normalized_lookup_name(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    Ok(normalized.to_ascii_lowercase())
}

fn normalized_opaque_ref(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some((namespace, local)) = normalized.split_once(':') else {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    };
    if namespace.is_empty()
        || local.is_empty()
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        || !local.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(artifact_contract_error(format!(
            "{field} must use namespaced <namespace>:<local> format"
        )));
    }
    Ok(normalized)
}

fn normalized_documentation_uri(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if normalized.starts_with('/')
        || normalized.contains('\\')
        || normalized.split('/').any(|part| part == "..")
    {
        return Err(artifact_contract_error(format!(
            "{field} cannot be absolute or contain traversal segments"
        )));
    }
    if normalized.contains(':')
        && !normalized.starts_with("http://")
        && !normalized.starts_with("https://")
    {
        return Err(artifact_contract_error(format!(
            "{field} must be relative or http(s)"
        )));
    }
    Ok(normalized)
}

fn normalized_non_empty(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty after trimming"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> VocabularyResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let Some(hex) = normalized.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(artifact_contract_error(format!(
            "{field} must use blake3:<lower-hex-64> format"
        )));
    }
    Ok(normalized)
}

fn artifact_contract_error(message: impl Into<String>) -> VocabularyError {
    VocabularyError::new(VocabularyErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> VocabularyError {
    VocabularyError::new(VocabularyErrorCode::CompatibilityPolicy, message)
}

fn missing_term_error(message: impl Into<String>) -> VocabularyError {
    VocabularyError::new(VocabularyErrorCode::MissingTerm, message)
}

fn ambiguous_synonym_error(message: impl Into<String>) -> VocabularyError {
    VocabularyError::new(VocabularyErrorCode::AmbiguousSynonym, message)
}

fn constraint_violation_error(message: impl Into<String>) -> VocabularyError {
    VocabularyError::new(VocabularyErrorCode::ConstraintViolation, message)
}
