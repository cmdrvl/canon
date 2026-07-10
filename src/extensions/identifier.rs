#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn identifier_extension_schema_version() -> &'static str {
    "canon.extension.identifier.v1"
}

pub type IdentifierResult<T> = Result<T, IdentifierError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierErrorCode {
    ArtifactContract,
    NamespaceDeclarationRequired,
    MissingNamespace,
    MissingValidator,
    MissingTrustPolicy,
    NamespaceNotApplicable,
    ValidationFailed,
    ResourceLimitExceeded,
    DigestMismatch,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierError {
    pub code: IdentifierErrorCode,
    pub message: String,
}

impl IdentifierError {
    pub fn new(code: IdentifierErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for IdentifierError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierExtensionPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<IdentifierNamespaceDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validators: Vec<IdentifierValidatorDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trust_policies: Vec<IdentifierTrustPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IdentifierNormalizationMode {
    #[serde(rename = "ascii_trim")]
    Trim,
    #[serde(rename = "ascii_trim_upper")]
    Upper,
    #[serde(rename = "ascii_trim_upper_alnum")]
    UpperAlnum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierTemporalScope {
    Persistent,
    ValidTimeOptional,
    HistoricalReuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierScopeKind {
    Global,
    ScopedByDeclaredKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierDisplayPolicy {
    Full,
    Last4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierRedactionPolicy {
    CleartextAllowed,
    MaskAllButLast4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierNamespaceDefinition {
    pub namespace_id: String,
    pub namespace_uri: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applicable_object_types: Vec<String>,
    pub normalization: IdentifierNormalizationMode,
    pub temporal_scope: IdentifierTemporalScope,
    pub scope_kind: IdentifierScopeKind,
    pub display_policy: IdentifierDisplayPolicy,
    pub redaction_policy: IdentifierRedactionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierValidatorDefinition {
    pub validator_id: String,
    pub max_input_bytes: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<IdentifierValidatorPrimitive>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierScopedCharset {
    UpperAlnum,
    Digits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdentifierValidatorPrimitive {
    Digits {
        min_length: usize,
        max_length: usize,
    },
    AsciiAlphanumeric {
        min_length: usize,
        max_length: usize,
    },
    LuhnChecksum {
        min_length: usize,
        max_length: usize,
    },
    ScopedSegments {
        separator: char,
        segment_count: usize,
        segment_min_length: usize,
        segment_max_length: usize,
        charset: IdentifierScopedCharset,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierReusePolicy {
    Never,
    AllowHistoricalNonOverlapping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierTrustPolicy {
    pub policy_id: String,
    pub single_value_per_object: bool,
    pub single_object_per_value: bool,
    pub reuse_policy: IdentifierReusePolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_trust_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentifierNamespaceRef {
    pub package_id: String,
    pub namespace_id: String,
    pub namespace_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentifierValidatorRef {
    pub package_id: String,
    pub validator_id: String,
    pub validator_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentifierTrustPolicyRef {
    pub package_id: String,
    pub policy_id: String,
    pub policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierFieldBinding {
    pub field_path: String,
    pub object_type: String,
    pub namespace: IdentifierNamespaceRef,
    pub validator: IdentifierValidatorRef,
    pub trust_policy: IdentifierTrustPolicyRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifierObservationInput {
    pub object_key: String,
    pub raw_value: String,
    pub scope_key: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedIdentifierObservation {
    pub object_key: String,
    pub field_path: String,
    pub object_type: String,
    pub namespace_id: String,
    pub namespace_uri: String,
    pub validator_id: String,
    pub policy_id: String,
    pub normalized_value: String,
    pub rendered_value: String,
    pub stable_fingerprint: String,
    pub scope_key: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_trust_hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierConflictClass {
    ExclusiveIdentifierConflict,
    RecycledIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierConflictDisposition {
    AntiMerge,
    HistoricalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentifierConflictEvidence {
    pub class: IdentifierConflictClass,
    pub disposition: IdentifierConflictDisposition,
    pub namespace_id: String,
    pub namespace_uri: String,
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_trust_hints: Vec<String>,
    pub reason: String,
}

pub fn finalize_package(
    mut package: IdentifierExtensionPackage,
) -> IdentifierResult<IdentifierExtensionPackage> {
    if package.version.trim().is_empty() {
        package.version = identifier_extension_schema_version().to_string();
    }
    if package.version != identifier_extension_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported identifier contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;
    package.namespaces = dedupe_components(
        package
            .namespaces
            .into_iter()
            .map(normalize_namespace)
            .collect::<IdentifierResult<Vec<_>>>()?,
        |namespace| namespace.namespace_id.clone(),
        "namespace",
    )?;
    package.validators = dedupe_components(
        package
            .validators
            .into_iter()
            .map(normalize_validator)
            .collect::<IdentifierResult<Vec<_>>>()?,
        |validator| validator.validator_id.clone(),
        "validator",
    )?;
    package.trust_policies = dedupe_components(
        package
            .trust_policies
            .into_iter()
            .map(normalize_trust_policy)
            .collect::<IdentifierResult<Vec<_>>>()?,
        |policy| policy.policy_id.clone(),
        "trust policy",
    )?;

    if package.namespaces.is_empty() {
        return Err(artifact_contract_error(
            "identifier package must declare at least one namespace",
        ));
    }
    if package.validators.is_empty() {
        return Err(artifact_contract_error(
            "identifier package must declare at least one validator",
        ));
    }
    if package.trust_policies.is_empty() {
        return Err(artifact_contract_error(
            "identifier package must declare at least one trust policy",
        ));
    }

    Ok(package)
}

pub fn finalize_field_binding(
    package: &IdentifierExtensionPackage,
    mut binding: IdentifierFieldBinding,
) -> IdentifierResult<IdentifierFieldBinding> {
    let package = finalize_package(package.clone())?;
    binding.field_path = normalized_non_empty(&binding.field_path, "field_path")?;
    binding.object_type = normalized_component_id(&binding.object_type, "object_type")?;
    binding.namespace = normalize_namespace_ref(binding.namespace)?;
    binding.validator = normalize_validator_ref(binding.validator)?;
    binding.trust_policy = normalize_trust_policy_ref(binding.trust_policy)?;

    if binding.namespace.package_id != package.package_id
        || binding.validator.package_id != package.package_id
        || binding.trust_policy.package_id != package.package_id
    {
        return Err(digest_mismatch_error(format!(
            "binding package ids must all match package {}",
            package.package_id
        )));
    }

    let namespace = resolve_namespace(&package, &binding.namespace)?;
    if !namespace
        .applicable_object_types
        .iter()
        .any(|object_type| object_type == &binding.object_type)
    {
        return Err(namespace_not_applicable_error(format!(
            "namespace {} is not declared for object type {}",
            namespace.namespace_id, binding.object_type
        )));
    }

    let _ = resolve_validator(&package, &binding.validator)?;
    let _ = resolve_trust_policy(&package, &binding.trust_policy)?;
    Ok(binding)
}

pub fn interpret_identifier(
    package: &IdentifierExtensionPackage,
    binding: Option<&IdentifierFieldBinding>,
    input: &IdentifierObservationInput,
) -> IdentifierResult<ValidatedIdentifierObservation> {
    let Some(binding) = binding else {
        return Err(IdentifierError::new(
            IdentifierErrorCode::NamespaceDeclarationRequired,
            "bare identifier strings are not assigned to a namespace without an explicit field binding",
        ));
    };
    validate_identifier_observation(package, binding, input)
}

pub fn validate_identifier_observation(
    package: &IdentifierExtensionPackage,
    binding: &IdentifierFieldBinding,
    input: &IdentifierObservationInput,
) -> IdentifierResult<ValidatedIdentifierObservation> {
    let package = finalize_package(package.clone())?;
    let binding = finalize_field_binding(&package, binding.clone())?;
    let namespace = resolve_namespace(&package, &binding.namespace)?;
    let validator = resolve_validator(&package, &binding.validator)?;
    let trust_policy = resolve_trust_policy(&package, &binding.trust_policy)?;

    let object_key = normalized_non_empty(&input.object_key, "object_key")?;
    let source_ref = normalized_non_empty(&input.source_ref, "source_ref")?;
    let scope_key = normalize_optional_scope_key(input.scope_key.clone(), namespace.scope_kind)?;
    let valid_from = normalize_optional_timestamp(input.valid_from.clone(), "valid_from")?;
    let valid_to = normalize_optional_timestamp(input.valid_to.clone(), "valid_to")?;
    validate_time_window(valid_from.as_deref(), valid_to.as_deref())?;

    let normalized_value = normalize_identifier_value(namespace.normalization, &input.raw_value)?;
    if normalized_value.len() > validator.max_input_bytes {
        return Err(resource_limit_error(format!(
            "identifier for validator {} exceeded max_input_bytes {}",
            validator.validator_id, validator.max_input_bytes
        )));
    }
    validate_with_declared_primitives(&normalized_value, &validator)?;

    Ok(ValidatedIdentifierObservation {
        object_key,
        field_path: binding.field_path,
        object_type: binding.object_type,
        namespace_id: namespace.namespace_id.clone(),
        namespace_uri: namespace.namespace_uri.clone(),
        validator_id: validator.validator_id.clone(),
        policy_id: trust_policy.policy_id.clone(),
        normalized_value: normalized_value.clone(),
        rendered_value: render_identifier(
            &normalized_value,
            namespace.display_policy,
            namespace.redaction_policy,
        ),
        stable_fingerprint: stable_identifier_fingerprint(
            &namespace.namespace_uri,
            &normalized_value,
            scope_key.as_deref(),
        ),
        scope_key,
        valid_from,
        valid_to,
        source_ref,
        source_trust_hints: trust_policy.source_trust_hints,
    })
}

pub fn collect_conflicts(
    package: &IdentifierExtensionPackage,
    binding: &IdentifierFieldBinding,
    inputs: &[IdentifierObservationInput],
) -> IdentifierResult<Vec<IdentifierConflictEvidence>> {
    let package = finalize_package(package.clone())?;
    let binding = finalize_field_binding(&package, binding.clone())?;
    let trust_policy = resolve_trust_policy(&package, &binding.trust_policy)?;
    let validated = inputs
        .iter()
        .map(|input| validate_identifier_observation(&package, &binding, input))
        .collect::<IdentifierResult<Vec<_>>>()?;

    let mut conflicts = Vec::new();

    if trust_policy.single_value_per_object {
        let mut by_object: BTreeMap<
            (String, Option<String>),
            Vec<&ValidatedIdentifierObservation>,
        > = BTreeMap::new();
        for observation in &validated {
            by_object
                .entry((
                    observation.object_key.clone(),
                    observation.scope_key.clone(),
                ))
                .or_default()
                .push(observation);
        }

        for (((object_key, scope_key), entries), values) in
            by_object.into_iter().map(|(group_key, entries)| {
                let values = entries
                    .iter()
                    .map(|entry| entry.normalized_value.clone())
                    .collect::<BTreeSet<_>>();
                ((group_key, entries), values)
            })
        {
            if values.len() > 1 {
                let first = entries[0];
                let mut object_keys = vec![object_key];
                if let Some(scope) = scope_key {
                    object_keys.push(format!("scope:{scope}"));
                }
                conflicts.push(IdentifierConflictEvidence {
                    class: IdentifierConflictClass::ExclusiveIdentifierConflict,
                    disposition: IdentifierConflictDisposition::AntiMerge,
                    namespace_id: first.namespace_id.clone(),
                    namespace_uri: first.namespace_uri.clone(),
                    policy_id: first.policy_id.clone(),
                    object_keys,
                    values: values.into_iter().collect(),
                    source_refs: entries
                        .iter()
                        .map(|entry| entry.source_ref.clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                    source_trust_hints: first.source_trust_hints.clone(),
                    reason:
                        "exclusive namespace observed multiple current values for the same object"
                            .to_string(),
                });
            }
        }
    }

    if trust_policy.single_object_per_value {
        let mut by_value: BTreeMap<(String, Option<String>), Vec<&ValidatedIdentifierObservation>> =
            BTreeMap::new();
        for observation in &validated {
            by_value
                .entry((
                    observation.normalized_value.clone(),
                    observation.scope_key.clone(),
                ))
                .or_default()
                .push(observation);
        }

        for (((value, scope_key), entries), object_keys) in
            by_value.into_iter().map(|(group_key, entries)| {
                let object_keys = entries
                    .iter()
                    .map(|entry| entry.object_key.clone())
                    .collect::<BTreeSet<_>>();
                ((group_key, entries), object_keys)
            })
        {
            if object_keys.len() > 1 {
                let first = entries[0];
                let reuse_is_historical_only = trust_policy.reuse_policy
                    == IdentifierReusePolicy::AllowHistoricalNonOverlapping
                    && observations_do_not_overlap(&entries);
                let class = if reuse_is_historical_only {
                    IdentifierConflictClass::RecycledIdentifier
                } else {
                    IdentifierConflictClass::ExclusiveIdentifierConflict
                };
                let disposition = if reuse_is_historical_only {
                    IdentifierConflictDisposition::HistoricalOnly
                } else {
                    IdentifierConflictDisposition::AntiMerge
                };
                let reason = if reuse_is_historical_only {
                    "identifier was reused across objects only after non-overlapping historical intervals"
                } else {
                    "exclusive identifier points to multiple objects in overlapping or open time ranges"
                };

                let mut values = vec![value];
                if let Some(scope) = scope_key {
                    values.push(format!("scope:{scope}"));
                }

                conflicts.push(IdentifierConflictEvidence {
                    class,
                    disposition,
                    namespace_id: first.namespace_id.clone(),
                    namespace_uri: first.namespace_uri.clone(),
                    policy_id: first.policy_id.clone(),
                    object_keys: object_keys.into_iter().collect(),
                    values,
                    source_refs: entries
                        .iter()
                        .map(|entry| entry.source_ref.clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                    source_trust_hints: first.source_trust_hints.clone(),
                    reason: reason.to_string(),
                });
            }
        }
    }

    conflicts.sort();
    conflicts.dedup();
    Ok(conflicts)
}

pub fn canonical_package_bytes(package: &IdentifierExtensionPackage) -> IdentifierResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!("failed to serialize identifier package: {error}"))
    })
}

pub fn namespace_digest(namespace: &IdentifierNamespaceDefinition) -> IdentifierResult<String> {
    let namespace = normalize_namespace(namespace.clone())?;
    component_digest(&namespace, "namespace")
}

pub fn validator_digest(validator: &IdentifierValidatorDefinition) -> IdentifierResult<String> {
    let validator = normalize_validator(validator.clone())?;
    component_digest(&validator, "validator")
}

pub fn trust_policy_digest(policy: &IdentifierTrustPolicy) -> IdentifierResult<String> {
    let policy = normalize_trust_policy(policy.clone())?;
    component_digest(&policy, "trust policy")
}

fn normalize_namespace(
    mut namespace: IdentifierNamespaceDefinition,
) -> IdentifierResult<IdentifierNamespaceDefinition> {
    namespace.namespace_id = normalized_namespaced_id(&namespace.namespace_id, "namespace_id")?;
    namespace.namespace_uri = normalized_non_empty(&namespace.namespace_uri, "namespace_uri")?;
    if namespace.namespace_uri.chars().any(char::is_whitespace) {
        return Err(artifact_contract_error(
            "namespace_uri must not contain whitespace",
        ));
    }
    namespace.applicable_object_types =
        normalize_string_vec(namespace.applicable_object_types, "applicable_object_types")?;
    if namespace.applicable_object_types.is_empty() {
        return Err(artifact_contract_error(
            "namespace must declare at least one applicable object type",
        ));
    }
    Ok(namespace)
}

fn normalize_validator(
    mut validator: IdentifierValidatorDefinition,
) -> IdentifierResult<IdentifierValidatorDefinition> {
    validator.validator_id = normalized_component_id(&validator.validator_id, "validator_id")?;
    if validator.max_input_bytes == 0 {
        return Err(artifact_contract_error(
            "validator max_input_bytes must be greater than zero",
        ));
    }
    if validator.checks.is_empty() {
        return Err(artifact_contract_error(
            "validator must declare at least one safe primitive check",
        ));
    }
    for check in &validator.checks {
        match check {
            IdentifierValidatorPrimitive::Digits {
                min_length,
                max_length,
            }
            | IdentifierValidatorPrimitive::AsciiAlphanumeric {
                min_length,
                max_length,
            }
            | IdentifierValidatorPrimitive::LuhnChecksum {
                min_length,
                max_length,
            } => validate_length_bounds(*min_length, *max_length)?,
            IdentifierValidatorPrimitive::ScopedSegments {
                segment_count,
                segment_min_length,
                segment_max_length,
                ..
            } => {
                if *segment_count == 0 {
                    return Err(artifact_contract_error(
                        "scoped validator must declare at least one segment",
                    ));
                }
                validate_length_bounds(*segment_min_length, *segment_max_length)?;
            }
        }
    }
    Ok(validator)
}

fn normalize_trust_policy(
    mut policy: IdentifierTrustPolicy,
) -> IdentifierResult<IdentifierTrustPolicy> {
    policy.policy_id = normalized_component_id(&policy.policy_id, "policy_id")?;
    policy.source_trust_hints =
        normalize_string_vec(policy.source_trust_hints, "source_trust_hints")?;
    Ok(policy)
}

fn normalize_namespace_ref(
    mut reference: IdentifierNamespaceRef,
) -> IdentifierResult<IdentifierNamespaceRef> {
    reference.package_id = normalized_package_id(&reference.package_id, "namespace.package_id")?;
    reference.namespace_id =
        normalized_namespaced_id(&reference.namespace_id, "namespace.namespace_id")?;
    reference.namespace_digest =
        normalized_hash(&reference.namespace_digest, "namespace.namespace_digest")?;
    Ok(reference)
}

fn normalize_validator_ref(
    mut reference: IdentifierValidatorRef,
) -> IdentifierResult<IdentifierValidatorRef> {
    reference.package_id = normalized_package_id(&reference.package_id, "validator.package_id")?;
    reference.validator_id =
        normalized_component_id(&reference.validator_id, "validator.validator_id")?;
    reference.validator_digest =
        normalized_hash(&reference.validator_digest, "validator.validator_digest")?;
    Ok(reference)
}

fn normalize_trust_policy_ref(
    mut reference: IdentifierTrustPolicyRef,
) -> IdentifierResult<IdentifierTrustPolicyRef> {
    reference.package_id = normalized_package_id(&reference.package_id, "trust_policy.package_id")?;
    reference.policy_id = normalized_component_id(&reference.policy_id, "trust_policy.policy_id")?;
    reference.policy_digest =
        normalized_hash(&reference.policy_digest, "trust_policy.policy_digest")?;
    Ok(reference)
}

fn resolve_namespace(
    package: &IdentifierExtensionPackage,
    reference: &IdentifierNamespaceRef,
) -> IdentifierResult<IdentifierNamespaceDefinition> {
    let namespace = package
        .namespaces
        .iter()
        .find(|namespace| namespace.namespace_id == reference.namespace_id)
        .cloned()
        .ok_or_else(|| {
            IdentifierError::new(
                IdentifierErrorCode::MissingNamespace,
                format!("unknown namespace {}", reference.namespace_id),
            )
        })?;
    let digest = namespace_digest(&namespace)?;
    if digest != reference.namespace_digest {
        return Err(digest_mismatch_error(format!(
            "namespace {} is pinned to {} but resolved to {}",
            reference.namespace_id, reference.namespace_digest, digest
        )));
    }
    Ok(namespace)
}

fn resolve_validator(
    package: &IdentifierExtensionPackage,
    reference: &IdentifierValidatorRef,
) -> IdentifierResult<IdentifierValidatorDefinition> {
    let validator = package
        .validators
        .iter()
        .find(|validator| validator.validator_id == reference.validator_id)
        .cloned()
        .ok_or_else(|| {
            IdentifierError::new(
                IdentifierErrorCode::MissingValidator,
                format!("unknown validator {}", reference.validator_id),
            )
        })?;
    let digest = validator_digest(&validator)?;
    if digest != reference.validator_digest {
        return Err(digest_mismatch_error(format!(
            "validator {} is pinned to {} but resolved to {}",
            reference.validator_id, reference.validator_digest, digest
        )));
    }
    Ok(validator)
}

fn resolve_trust_policy(
    package: &IdentifierExtensionPackage,
    reference: &IdentifierTrustPolicyRef,
) -> IdentifierResult<IdentifierTrustPolicy> {
    let policy = package
        .trust_policies
        .iter()
        .find(|policy| policy.policy_id == reference.policy_id)
        .cloned()
        .ok_or_else(|| {
            IdentifierError::new(
                IdentifierErrorCode::MissingTrustPolicy,
                format!("unknown trust policy {}", reference.policy_id),
            )
        })?;
    let digest = trust_policy_digest(&policy)?;
    if digest != reference.policy_digest {
        return Err(digest_mismatch_error(format!(
            "trust policy {} is pinned to {} but resolved to {}",
            reference.policy_id, reference.policy_digest, digest
        )));
    }
    Ok(policy)
}

fn validate_with_declared_primitives(
    normalized_value: &str,
    validator: &IdentifierValidatorDefinition,
) -> IdentifierResult<()> {
    for check in &validator.checks {
        match check {
            IdentifierValidatorPrimitive::Digits {
                min_length,
                max_length,
            } => {
                validate_ascii_length(normalized_value, *min_length, *max_length, "digits")?;
                if !normalized_value.chars().all(|ch| ch.is_ascii_digit()) {
                    return Err(validation_failed_error(format!(
                        "validator {} requires ASCII digits only",
                        validator.validator_id
                    )));
                }
            }
            IdentifierValidatorPrimitive::AsciiAlphanumeric {
                min_length,
                max_length,
            } => {
                validate_ascii_length(normalized_value, *min_length, *max_length, "alphanumeric")?;
                if !normalized_value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric())
                {
                    return Err(validation_failed_error(format!(
                        "validator {} requires ASCII alphanumeric characters only",
                        validator.validator_id
                    )));
                }
            }
            IdentifierValidatorPrimitive::LuhnChecksum {
                min_length,
                max_length,
            } => {
                validate_ascii_length(normalized_value, *min_length, *max_length, "luhn_checksum")?;
                if !normalized_value.chars().all(|ch| ch.is_ascii_digit()) {
                    return Err(validation_failed_error(format!(
                        "validator {} requires ASCII digits before Luhn verification",
                        validator.validator_id
                    )));
                }
                if !passes_luhn(normalized_value) {
                    return Err(validation_failed_error(format!(
                        "validator {} rejected identifier with invalid Luhn checksum",
                        validator.validator_id
                    )));
                }
            }
            IdentifierValidatorPrimitive::ScopedSegments {
                separator,
                segment_count,
                segment_min_length,
                segment_max_length,
                charset,
            } => {
                let parts = normalized_value.split(*separator).collect::<Vec<_>>();
                if parts.len() != *segment_count {
                    return Err(validation_failed_error(format!(
                        "validator {} expected {} scoped segments separated by {}",
                        validator.validator_id, segment_count, separator
                    )));
                }
                for part in parts {
                    validate_ascii_length(
                        part,
                        *segment_min_length,
                        *segment_max_length,
                        "scoped_segment",
                    )?;
                    let valid = match charset {
                        IdentifierScopedCharset::UpperAlnum => part
                            .chars()
                            .all(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase()),
                        IdentifierScopedCharset::Digits => {
                            part.chars().all(|ch| ch.is_ascii_digit())
                        }
                    };
                    if !valid {
                        return Err(validation_failed_error(format!(
                            "validator {} rejected scoped segment {}",
                            validator.validator_id, part
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn observations_do_not_overlap(observations: &[&ValidatedIdentifierObservation]) -> bool {
    for (index, left) in observations.iter().enumerate() {
        for right in observations.iter().skip(index + 1) {
            if intervals_overlap(
                left.valid_from.as_deref(),
                left.valid_to.as_deref(),
                right.valid_from.as_deref(),
                right.valid_to.as_deref(),
            ) {
                return false;
            }
        }
    }
    true
}

fn intervals_overlap(
    left_start: Option<&str>,
    left_end: Option<&str>,
    right_start: Option<&str>,
    right_end: Option<&str>,
) -> bool {
    let Some(left_start) = left_start else {
        return true;
    };
    let Some(left_end) = left_end else {
        return true;
    };
    let Some(right_start) = right_start else {
        return true;
    };
    let Some(right_end) = right_end else {
        return true;
    };
    !(left_end < right_start || right_end < left_start)
}

fn validate_length_bounds(min_length: usize, max_length: usize) -> IdentifierResult<()> {
    if min_length == 0 || max_length == 0 || min_length > max_length {
        return Err(artifact_contract_error(format!(
            "invalid length bounds: min_length={} max_length={}",
            min_length, max_length
        )));
    }
    Ok(())
}

fn validate_ascii_length(
    value: &str,
    min_length: usize,
    max_length: usize,
    label: &str,
) -> IdentifierResult<()> {
    validate_length_bounds(min_length, max_length)?;
    let length = value.len();
    if length < min_length || length > max_length {
        return Err(validation_failed_error(format!(
            "{} length {} is outside {}..={}",
            label, length, min_length, max_length
        )));
    }
    Ok(())
}

fn render_identifier(
    normalized_value: &str,
    display_policy: IdentifierDisplayPolicy,
    redaction_policy: IdentifierRedactionPolicy,
) -> String {
    match redaction_policy {
        IdentifierRedactionPolicy::MaskAllButLast4 => mask_all_but_last4(normalized_value),
        IdentifierRedactionPolicy::CleartextAllowed => match display_policy {
            IdentifierDisplayPolicy::Full => normalized_value.to_string(),
            IdentifierDisplayPolicy::Last4 => tail4(normalized_value),
        },
    }
}

fn stable_identifier_fingerprint(
    namespace_uri: &str,
    normalized_value: &str,
    scope_key: Option<&str>,
) -> String {
    let mut material = format!("{namespace_uri}\u{0}{normalized_value}");
    if let Some(scope_key) = scope_key {
        material.push('\u{0}');
        material.push_str(scope_key);
    }
    format!("blake3:{}", blake3::hash(material.as_bytes()).to_hex())
}

fn tail4(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

fn mask_all_but_last4(value: &str) -> String {
    let suffix = tail4(value);
    if value.chars().count() <= 4 {
        return suffix;
    }
    let masked = "*".repeat(value.chars().count().saturating_sub(4));
    format!("{masked}{suffix}")
}

fn normalize_identifier_value(
    mode: IdentifierNormalizationMode,
    raw_value: &str,
) -> IdentifierResult<String> {
    let trimmed = raw_value.trim_matches(|ch: char| ch.is_ascii_whitespace());
    if trimmed.is_empty() {
        return Err(validation_failed_error(
            "identifier value is empty after ASCII trim",
        ));
    }
    let normalized = match mode {
        IdentifierNormalizationMode::Trim => trimmed.to_string(),
        IdentifierNormalizationMode::Upper => trimmed.to_ascii_uppercase(),
        IdentifierNormalizationMode::UpperAlnum => trimmed
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase(),
    };
    if normalized.is_empty() {
        return Err(validation_failed_error(
            "identifier value is empty after normalization",
        ));
    }
    Ok(normalized)
}

fn normalize_optional_scope_key(
    scope_key: Option<String>,
    scope_kind: IdentifierScopeKind,
) -> IdentifierResult<Option<String>> {
    match scope_kind {
        IdentifierScopeKind::Global => Ok(scope_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())),
        IdentifierScopeKind::ScopedByDeclaredKey => scope_key
            .map(|value| normalized_non_empty(&value, "scope_key"))
            .transpose()
            .and_then(|value| {
                value.ok_or_else(|| {
                    validation_failed_error(
                        "scoped namespace requires a declared scope_key in the field mapping input",
                    )
                })
            })
            .map(Some),
    }
}

fn normalize_optional_timestamp(
    value: Option<String>,
    field: &str,
) -> IdentifierResult<Option<String>> {
    value
        .map(|value| {
            let value = normalized_non_empty(&value, field)?;
            if !(value.ends_with('Z') && value.contains('T')) {
                return Err(validation_failed_error(format!(
                    "{field} must be an RFC3339 UTC timestamp ending with Z"
                )));
            }
            Ok(value)
        })
        .transpose()
}

fn validate_time_window(valid_from: Option<&str>, valid_to: Option<&str>) -> IdentifierResult<()> {
    if let (Some(valid_from), Some(valid_to)) = (valid_from, valid_to)
        && valid_from > valid_to
    {
        return Err(validation_failed_error(
            "valid_from must not be greater than valid_to",
        ));
    }
    Ok(())
}

fn passes_luhn(value: &str) -> bool {
    let mut sum = 0u32;
    let mut double = false;
    for digit in value.chars().rev() {
        let mut value = digit.to_digit(10).unwrap_or(10);
        if value > 9 {
            return false;
        }
        if double {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
        double = !double;
    }
    sum.is_multiple_of(10)
}

fn dedupe_components<T, F>(mut components: Vec<T>, key: F, label: &str) -> IdentifierResult<Vec<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    components.sort_by_key(|component| key(component));
    let mut deduped = Vec::with_capacity(components.len());
    for component in components {
        if let Some(previous) = deduped.last()
            && key(previous) == key(&component)
        {
            if previous != &component {
                return Err(artifact_contract_error(format!(
                    "{label} {} cannot be declared with conflicting content",
                    key(&component)
                )));
            }
            continue;
        }
        deduped.push(component);
    }
    Ok(deduped)
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> IdentifierResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<IdentifierResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalized_package_id(value: &str, field: &str) -> IdentifierResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.chars().all(|ch| {
        ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_' || ch == '-'
    }) && value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^[a-z0-9][a-z0-9._-]*$"
    )))
}

fn normalized_component_id(value: &str, field: &str) -> IdentifierResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.chars().all(|ch| {
        ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || ch == '.'
            || ch == '_'
            || ch == '-'
            || ch == ':'
    }) && value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use opaque lowercase ASCII component IDs"
    )))
}

fn normalized_namespaced_id(value: &str, field: &str) -> IdentifierResult<String> {
    let value = normalized_component_id(value, field)?;
    if !value.contains(':') {
        return Err(artifact_contract_error(format!(
            "{field} must contain at least one namespace separator ':'"
        )));
    }
    Ok(value)
}

fn normalized_semver(value: &str, field: &str) -> IdentifierResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^[0-9]+\\.[0-9]+\\.[0-9]+$"
    )))
}

fn normalized_hash(value: &str, field: &str) -> IdentifierResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64
        && hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^blake3:[0-9a-f]{{64}}$"
    )))
}

fn normalized_non_empty(value: &str, field: &str) -> IdentifierResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn component_digest<T: Serialize>(value: &T, label: &str) -> IdentifierResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize {label} for digest computation: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn artifact_contract_error(message: impl Into<String>) -> IdentifierError {
    IdentifierError::new(IdentifierErrorCode::ArtifactContract, message)
}

fn namespace_not_applicable_error(message: impl Into<String>) -> IdentifierError {
    IdentifierError::new(IdentifierErrorCode::NamespaceNotApplicable, message)
}

fn validation_failed_error(message: impl Into<String>) -> IdentifierError {
    IdentifierError::new(IdentifierErrorCode::ValidationFailed, message)
}

fn resource_limit_error(message: impl Into<String>) -> IdentifierError {
    IdentifierError::new(IdentifierErrorCode::ResourceLimitExceeded, message)
}

fn digest_mismatch_error(message: impl Into<String>) -> IdentifierError {
    IdentifierError::new(IdentifierErrorCode::DigestMismatch, message)
}
