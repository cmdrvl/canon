#![forbid(unsafe_code)]

//! Qualified identity scope semantics for exact lookup and snapshot headers.
//!
//! Canonical IDs are only reusable when their object class, identifier
//! namespace, and resolved scope agree. Unknown or extension vocabulary values
//! must be carried explicitly so identical strings do not collide across
//! datasets, jurisdictions, source systems, or profile-local namespaces.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_IDENTITY_SCOPE_VERSION: &str = "canon.identity.scope.v1";

pub type IdentityScopeResult<T> = Result<T, IdentityScopeError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentityScopeErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityScopeError {
    pub code: IdentityScopeErrorCode,
    pub message: String,
}

impl IdentityScopeError {
    pub fn new(code: IdentityScopeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for IdentityScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for IdentityScopeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreCanonicalTypeClass {
    Account,
    Asset,
    Document,
    Event,
    Instrument,
    Location,
    Organization,
    Person,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreIdentifierNamespaceClass {
    CanonicalId,
    ExternalGlobalId,
    SourceLocalId,
    DatasetLocalId,
    AliasSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreScopeDimension {
    Dataset,
    Jurisdiction,
    SourceSystem,
    Profile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityCompatibility {
    Equal,
    Compatible,
    RequiresExplicitEvidence,
    Incompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactLookupQualification {
    QualifiedMatch,
    RequiresExplicitEvidence,
    Incompatible,
    DifferentValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossScopeAliasPolicy {
    SameScopeOnly,
    RequireExplicitEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalTypeRef {
    Core {
        class: CoreCanonicalTypeClass,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdentifierNamespaceRef {
    Core {
        class: CoreIdentifierNamespaceClass,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopeDimensionRef {
    Core {
        dimension: CoreScopeDimension,
    },
    Extension {
        package_digest: String,
        vocabulary: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "binding", rename_all = "snake_case")]
pub enum ScopeBinding {
    Exact { value: String },
    Unknown,
    Inherit,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeDimensionBinding {
    pub dimension: ScopeDimensionRef,
    pub binding: ScopeBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IdentityScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<ScopeDimensionBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedIdentityRef {
    pub version: String,
    pub identifier_value: String,
    pub canonical_type: CanonicalTypeRef,
    pub namespace: IdentifierNamespaceRef,
    #[serde(default)]
    pub scope: IdentityScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityFactHeader {
    pub canonical_type: CanonicalTypeRef,
    pub namespace: IdentifierNamespaceRef,
    #[serde(default)]
    pub scope: IdentityScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySnapshotHeader {
    pub canonical_type: CanonicalTypeRef,
    pub namespace: IdentifierNamespaceRef,
    #[serde(default)]
    pub scope: IdentityScope,
}

pub fn finalize_qualified_identity(
    mut identity: QualifiedIdentityRef,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<QualifiedIdentityRef> {
    if identity.version.trim().is_empty() {
        identity.version = CANON_IDENTITY_SCOPE_VERSION.to_string();
    }
    if identity.version != CANON_IDENTITY_SCOPE_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported identity scope version: {}",
            identity.version
        )));
    }

    identity.identifier_value =
        normalized_non_empty(&identity.identifier_value, "identifier_value")?;
    identity.canonical_type = normalize_canonical_type(identity.canonical_type)?;
    identity.namespace = normalize_namespace(identity.namespace)?;
    identity.scope = finalize_scope(identity.scope, parent_scope)?;
    validate_namespace_requirements(&identity.namespace, &identity.scope)?;
    Ok(identity)
}

pub fn finalize_fact_header(
    mut header: IdentityFactHeader,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<IdentityFactHeader> {
    header.canonical_type = normalize_canonical_type(header.canonical_type)?;
    header.namespace = normalize_namespace(header.namespace)?;
    header.scope = finalize_scope(header.scope, parent_scope)?;
    validate_namespace_requirements(&header.namespace, &header.scope)?;
    Ok(header)
}

pub fn finalize_snapshot_header(
    mut header: IdentitySnapshotHeader,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<IdentitySnapshotHeader> {
    header.canonical_type = normalize_canonical_type(header.canonical_type)?;
    header.namespace = normalize_namespace(header.namespace)?;
    header.scope = finalize_scope(header.scope, parent_scope)?;
    validate_namespace_requirements(&header.namespace, &header.scope)?;
    Ok(header)
}

pub fn finalize_scope(
    scope: IdentityScope,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<IdentityScope> {
    let parent = parent_scope
        .cloned()
        .map(|scope| finalize_scope(scope, None))
        .transpose()?;

    let mut normalized = scope
        .dimensions
        .into_iter()
        .map(|binding| normalize_scope_dimension_binding(binding, parent.as_ref()))
        .collect::<IdentityScopeResult<Vec<_>>>()?;
    normalized.sort_by(|left, right| left.dimension.cmp(&right.dimension));

    let mut deduped: Vec<ScopeDimensionBinding> = Vec::with_capacity(normalized.len());
    for binding in normalized {
        if let Some(previous) = deduped.last()
            && previous.dimension == binding.dimension
        {
            if previous.binding != binding.binding {
                return Err(artifact_contract_error(format!(
                    "scope dimension {} cannot carry multiple bindings",
                    describe_scope_dimension(&binding.dimension)
                )));
            }
            continue;
        }
        deduped.push(binding);
    }

    Ok(IdentityScope {
        dimensions: deduped,
    })
}

pub fn identity_compatibility(
    left: &QualifiedIdentityRef,
    right: &QualifiedIdentityRef,
) -> IdentityScopeResult<IdentityCompatibility> {
    let left = finalize_qualified_identity(left.clone(), None)?;
    let right = finalize_qualified_identity(right.clone(), None)?;
    Ok(descriptor_compatibility(
        &left.canonical_type,
        &left.namespace,
        &left.scope,
        &right.canonical_type,
        &right.namespace,
        &right.scope,
    ))
}

pub fn qualify_exact_lookup(
    query: &QualifiedIdentityRef,
    candidate: &QualifiedIdentityRef,
) -> IdentityScopeResult<ExactLookupQualification> {
    let query = finalize_qualified_identity(query.clone(), None)?;
    let candidate = finalize_qualified_identity(candidate.clone(), None)?;
    if query.identifier_value != candidate.identifier_value {
        return Ok(ExactLookupQualification::DifferentValue);
    }

    Ok(
        match descriptor_compatibility(
            &query.canonical_type,
            &query.namespace,
            &query.scope,
            &candidate.canonical_type,
            &candidate.namespace,
            &candidate.scope,
        ) {
            IdentityCompatibility::Equal => ExactLookupQualification::QualifiedMatch,
            IdentityCompatibility::Compatible | IdentityCompatibility::RequiresExplicitEvidence => {
                ExactLookupQualification::RequiresExplicitEvidence
            }
            IdentityCompatibility::Incompatible => ExactLookupQualification::Incompatible,
        },
    )
}

pub fn authorize_cross_scope_alias(
    left: &QualifiedIdentityRef,
    right: &QualifiedIdentityRef,
    policy: CrossScopeAliasPolicy,
    evidence_ref: Option<&str>,
) -> IdentityScopeResult<IdentityCompatibility> {
    let compatibility = identity_compatibility(left, right)?;
    match compatibility {
        IdentityCompatibility::Incompatible => Err(compatibility_policy_error(
            "cross-scope alias is incompatible by canonical type, namespace, or scope",
        )),
        IdentityCompatibility::Equal => Ok(compatibility),
        IdentityCompatibility::Compatible | IdentityCompatibility::RequiresExplicitEvidence => {
            if matches!(policy, CrossScopeAliasPolicy::SameScopeOnly) {
                return Err(compatibility_policy_error(
                    "cross-scope aliases require explicit policy and evidence",
                ));
            }
            let evidence_ref = evidence_ref
                .map(|value| normalized_non_empty(value, "evidence_ref"))
                .transpose()?;
            if evidence_ref.is_none() {
                return Err(compatibility_policy_error(
                    "cross-scope aliases require a non-empty evidence_ref",
                ));
            }
            Ok(compatibility)
        }
    }
}

pub fn canonical_qualified_identity_bytes(
    identity: &QualifiedIdentityRef,
) -> IdentityScopeResult<Vec<u8>> {
    let identity = finalize_qualified_identity(identity.clone(), None)?;
    serde_json::to_vec(&identity)
        .map_err(|error| artifact_contract_error(format!("failed to serialize identity: {error}")))
}

fn normalize_scope_dimension_binding(
    mut binding: ScopeDimensionBinding,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<ScopeDimensionBinding> {
    binding.dimension = normalize_scope_dimension(binding.dimension)?;
    binding.binding = match binding.binding {
        ScopeBinding::Exact { value } => ScopeBinding::Exact {
            value: normalized_non_empty(&value, "scope.binding.value")?,
        },
        ScopeBinding::Unknown => ScopeBinding::Unknown,
        ScopeBinding::Inherit => inherited_scope_binding(&binding.dimension, parent_scope)?,
    };
    Ok(binding)
}

fn inherited_scope_binding(
    dimension: &ScopeDimensionRef,
    parent_scope: Option<&IdentityScope>,
) -> IdentityScopeResult<ScopeBinding> {
    let Some(parent_scope) = parent_scope else {
        return Err(artifact_contract_error(format!(
            "scope dimension {} cannot inherit without a parent scope",
            describe_scope_dimension(dimension)
        )));
    };
    parent_scope
        .dimensions
        .iter()
        .find(|binding| &binding.dimension == dimension)
        .map(|binding| binding.binding.clone())
        .ok_or_else(|| {
            artifact_contract_error(format!(
                "scope dimension {} cannot inherit because the parent scope does not define it",
                describe_scope_dimension(dimension)
            ))
        })
}

fn validate_namespace_requirements(
    namespace: &IdentifierNamespaceRef,
    scope: &IdentityScope,
) -> IdentityScopeResult<()> {
    match namespace {
        IdentifierNamespaceRef::Core {
            class: CoreIdentifierNamespaceClass::SourceLocalId,
        } => {
            if !scope_has_exact_dimension(scope, CoreScopeDimension::SourceSystem) {
                return Err(artifact_contract_error(
                    "source_local_id namespaces require an exact source_system scope dimension",
                ));
            }
        }
        IdentifierNamespaceRef::Core {
            class: CoreIdentifierNamespaceClass::DatasetLocalId,
        } if !scope_has_exact_dimension(scope, CoreScopeDimension::Dataset) => {
            return Err(artifact_contract_error(
                "dataset_local_id namespaces require an exact dataset scope dimension",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn scope_has_exact_dimension(scope: &IdentityScope, target: CoreScopeDimension) -> bool {
    scope.dimensions.iter().any(|binding| {
        matches!(
            (&binding.dimension, &binding.binding),
            (
                ScopeDimensionRef::Core { dimension },
                ScopeBinding::Exact { .. }
            ) if *dimension == target
        )
    })
}

fn descriptor_compatibility(
    left_type: &CanonicalTypeRef,
    left_namespace: &IdentifierNamespaceRef,
    left_scope: &IdentityScope,
    right_type: &CanonicalTypeRef,
    right_namespace: &IdentifierNamespaceRef,
    right_scope: &IdentityScope,
) -> IdentityCompatibility {
    if left_type != right_type || left_namespace != right_namespace {
        return IdentityCompatibility::Incompatible;
    }

    let left_scope = scope_binding_map(left_scope);
    let right_scope = scope_binding_map(right_scope);
    let dimensions = left_scope
        .keys()
        .chain(right_scope.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut saw_specialization = false;
    let mut saw_evidence_boundary = false;
    for dimension in dimensions {
        match (left_scope.get(&dimension), right_scope.get(&dimension)) {
            (
                Some(ScopeBinding::Exact { value: left }),
                Some(ScopeBinding::Exact { value: right }),
            ) => {
                if left != right {
                    return IdentityCompatibility::Incompatible;
                }
            }
            (Some(ScopeBinding::Exact { .. }), None) | (None, Some(ScopeBinding::Exact { .. })) => {
                saw_specialization = true;
            }
            (Some(ScopeBinding::Unknown), Some(ScopeBinding::Unknown))
            | (Some(ScopeBinding::Unknown), None)
            | (None, Some(ScopeBinding::Unknown))
            | (Some(ScopeBinding::Unknown), Some(ScopeBinding::Exact { .. }))
            | (Some(ScopeBinding::Exact { .. }), Some(ScopeBinding::Unknown)) => {
                saw_evidence_boundary = true;
            }
            (Some(ScopeBinding::Inherit), _) | (_, Some(ScopeBinding::Inherit)) => {
                saw_evidence_boundary = true;
            }
            (None, None) => {}
        }
    }

    if saw_evidence_boundary {
        IdentityCompatibility::RequiresExplicitEvidence
    } else if saw_specialization {
        IdentityCompatibility::Compatible
    } else {
        IdentityCompatibility::Equal
    }
}

fn scope_binding_map(scope: &IdentityScope) -> BTreeMap<ScopeDimensionRef, ScopeBinding> {
    scope
        .dimensions
        .iter()
        .map(|binding| (binding.dimension.clone(), binding.binding.clone()))
        .collect()
}

fn normalize_canonical_type(value: CanonicalTypeRef) -> IdentityScopeResult<CanonicalTypeRef> {
    match value {
        CanonicalTypeRef::Core { class } => Ok(CanonicalTypeRef::Core { class }),
        CanonicalTypeRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(CanonicalTypeRef::Extension {
            package_digest: normalized_hash(&package_digest, "canonical_type.package_digest")?,
            vocabulary: normalized_non_empty(&vocabulary, "canonical_type.vocabulary")?,
            value: normalized_non_empty(&value, "canonical_type.value")?,
        }),
    }
}

fn normalize_namespace(
    value: IdentifierNamespaceRef,
) -> IdentityScopeResult<IdentifierNamespaceRef> {
    match value {
        IdentifierNamespaceRef::Core { class } => Ok(IdentifierNamespaceRef::Core { class }),
        IdentifierNamespaceRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(IdentifierNamespaceRef::Extension {
            package_digest: normalized_hash(&package_digest, "namespace.package_digest")?,
            vocabulary: normalized_non_empty(&vocabulary, "namespace.vocabulary")?,
            value: normalized_non_empty(&value, "namespace.value")?,
        }),
    }
}

fn normalize_scope_dimension(value: ScopeDimensionRef) -> IdentityScopeResult<ScopeDimensionRef> {
    match value {
        ScopeDimensionRef::Core { dimension } => Ok(ScopeDimensionRef::Core { dimension }),
        ScopeDimensionRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => Ok(ScopeDimensionRef::Extension {
            package_digest: normalized_hash(&package_digest, "scope.dimension.package_digest")?,
            vocabulary: normalized_non_empty(&vocabulary, "scope.dimension.vocabulary")?,
            value: normalized_non_empty(&value, "scope.dimension.value")?,
        }),
    }
}

fn normalized_non_empty(value: &str, field: &str) -> IdentityScopeResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty after trimming"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> IdentityScopeResult<String> {
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

fn describe_scope_dimension(dimension: &ScopeDimensionRef) -> String {
    match dimension {
        ScopeDimensionRef::Core { dimension } => format!("{dimension:?}").to_ascii_lowercase(),
        ScopeDimensionRef::Extension {
            package_digest,
            vocabulary,
            value,
        } => format!("{package_digest}:{vocabulary}:{value}"),
    }
}

fn artifact_contract_error(message: impl Into<String>) -> IdentityScopeError {
    IdentityScopeError::new(IdentityScopeErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> IdentityScopeError {
    IdentityScopeError::new(IdentityScopeErrorCode::CompatibilityPolicy, message)
}
