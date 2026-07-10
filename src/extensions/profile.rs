// Portable entity profile package contract.
//
// These packages let third parties ship cluster and directional-link profile
// behavior as typed configuration. Core code validates package mechanics,
// typed references, deterministic ordering, mode/capability declarations, and
// explicit project override visibility. It does not embed domain-specific
// object vocabularies or executable branching.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const CANON_ENTITY_PROFILE_PACKAGE_VERSION: &str = "canon.entity.profile.v1";
const ENTITY_PROFILE_KIND: &str = "entity-profile";

pub type ProfileResult<T> = Result<T, ProfileError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingCapability,
    WrongObjectType,
    UnknownField,
    UnknownOverride,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileError {
    pub code: ProfileErrorCode,
    pub message: String,
}

impl ProfileError {
    pub fn new(code: ProfileErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProfileError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityProfilePackageCompatibility {
    ExactDigest,
    CompatibleSameMajor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfilePackageRefKind {
    OntologyPackage,
    IdentifierPackage,
    VocabularyPackage,
    NormalizationPackage,
    EvidencePackage,
    EvidencePolicy,
    ReviewPolicy,
    PromotionPolicy,
    FrozenExecutableStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProfilePackageRef {
    pub kind: ProfilePackageRefKind,
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileModeKind {
    Cluster,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    SourceToTarget,
    TargetToSource,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileCapability {
    Prepare,
    Index,
    Block,
    Evidence,
    SolveCluster,
    SolveLink,
    Review,
    Promote,
    Apply,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfilePackage {
    pub kind: String,
    pub profile: String,
    pub version: String,
    pub entity_type: String,
    pub identity_semantics: String,
    pub canonical_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub normalized_views: BTreeMap<String, EntityNormalizedView>,
    pub evidence: EntityEvidenceLanes,
    pub patch_namespaces: EntityPatchNamespaces,
    pub evidence_policy: ProfilePackageRef,
    pub review_policy: ProfilePackageRef,
    pub promotion_policy: ProfilePackageRef,
    pub frozen_executable_strategy: ProfilePackageRef,
    pub ontology_package: ProfilePackageRef,
    pub identifier_package: ProfilePackageRef,
    pub vocabulary_package: ProfilePackageRef,
    pub evidence_package: ProfilePackageRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalization_packages: Vec<ProfilePackageRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_capabilities: Vec<ProfileCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_mappings: Vec<EntityProfileFieldMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_modes: Vec<EntityProfileMode>,
    pub limits: EntityProfileLimits,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_overrides: Vec<EntityProfileProjectOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityNormalizedView {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operators: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityEvidenceLanes {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support: Vec<EntityOperatorSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cannot_link: Vec<EntityOperatorSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_hints: Vec<EntityOperatorSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityOperatorSpec {
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityPatchNamespaces {
    pub aliases: String,
    pub distinct: String,
    pub relations: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileFieldMapping {
    pub field_path: String,
    pub object_type: String,
    pub field_role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_view: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileMode {
    pub mode: ProfileModeKind,
    pub source_object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_direction: Option<LinkDirection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<ProfileCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileLimits {
    pub max_observation_fields: u64,
    pub max_candidate_pairs: u64,
    pub max_outputs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityProfileProjectOverride {
    pub key: String,
    pub default_value: String,
    pub artifact_header_key: String,
    pub project_lock_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AppliedProjectOverride {
    pub key: String,
    pub value: String,
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileExecutionRequest {
    pub mode: ProfileModeKind,
    pub source_object_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<ProfileCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileExecutionPlan {
    pub profile: String,
    pub version: String,
    pub package_digest: String,
    pub mode: EntityProfileMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_refs: Vec<ProfilePackageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityProfileLockView {
    pub profile: String,
    pub version: String,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_modes: Vec<ProfileModeKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_outputs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defaults: Vec<ResolvedProjectOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<ResolvedProjectOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_refs: Vec<ProfilePackageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResolvedProjectOverride {
    pub key: String,
    pub value: String,
    pub artifact_header_key: String,
    pub project_lock_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

pub fn finalize_package(mut package: EntityProfilePackage) -> ProfileResult<EntityProfilePackage> {
    if package.kind.trim().is_empty() {
        package.kind = ENTITY_PROFILE_KIND.to_string();
    }
    if package.kind != ENTITY_PROFILE_KIND {
        return Err(artifact_contract_error(format!(
            "unsupported entity profile kind: {}",
            package.kind
        )));
    }

    package.profile = normalized_non_empty(&package.profile, "profile")?;
    package.version = normalized_version(&package.version, "version")?;
    package.entity_type = normalized_non_empty(&package.entity_type, "entity_type")?;
    package.identity_semantics =
        normalized_non_empty(&package.identity_semantics, "identity_semantics")?;
    package.canonical_type = normalized_non_empty(&package.canonical_type, "canonical_type")?;
    package.required_fields = normalize_unique_strings(package.required_fields, "required_fields")?;
    if package.required_fields.is_empty() {
        return Err(artifact_contract_error(
            "entity profile package must declare at least one required field",
        ));
    }

    if package.normalized_views.is_empty() {
        return Err(artifact_contract_error(
            "entity profile package must declare at least one normalized view",
        ));
    }
    for (view_name, view) in &mut package.normalized_views {
        normalize_view(view_name, view)?;
    }

    normalize_evidence_lanes(&mut package.evidence, &package.normalized_views)?;
    package.patch_namespaces =
        normalize_patch_namespaces(package.patch_namespaces, &package.profile)?;

    package.evidence_policy = normalize_profile_ref(
        package.evidence_policy,
        ProfilePackageRefKind::EvidencePolicy,
        "evidence_policy",
    )?;
    package.review_policy = normalize_profile_ref(
        package.review_policy,
        ProfilePackageRefKind::ReviewPolicy,
        "review_policy",
    )?;
    package.promotion_policy = normalize_profile_ref(
        package.promotion_policy,
        ProfilePackageRefKind::PromotionPolicy,
        "promotion_policy",
    )?;
    package.frozen_executable_strategy = normalize_profile_ref(
        package.frozen_executable_strategy,
        ProfilePackageRefKind::FrozenExecutableStrategy,
        "frozen_executable_strategy",
    )?;
    package.ontology_package = normalize_profile_ref(
        package.ontology_package,
        ProfilePackageRefKind::OntologyPackage,
        "ontology_package",
    )?;
    package.identifier_package = normalize_profile_ref(
        package.identifier_package,
        ProfilePackageRefKind::IdentifierPackage,
        "identifier_package",
    )?;
    package.vocabulary_package = normalize_profile_ref(
        package.vocabulary_package,
        ProfilePackageRefKind::VocabularyPackage,
        "vocabulary_package",
    )?;
    package.evidence_package = normalize_profile_ref(
        package.evidence_package,
        ProfilePackageRefKind::EvidencePackage,
        "evidence_package",
    )?;
    package.normalization_packages = dedupe_profile_refs(
        package
            .normalization_packages
            .into_iter()
            .map(|reference| {
                normalize_profile_ref(
                    reference,
                    ProfilePackageRefKind::NormalizationPackage,
                    "normalization_packages",
                )
            })
            .collect::<ProfileResult<Vec<_>>>()?,
        "normalization package",
    )?;
    if package.normalization_packages.is_empty() {
        return Err(artifact_contract_error(
            "entity profile package must pin at least one normalization package",
        ));
    }

    package.available_capabilities =
        normalize_capabilities(package.available_capabilities, "available_capabilities")?;
    if package.available_capabilities.is_empty() {
        return Err(missing_capability_error(
            "entity profile package must declare available capabilities",
        ));
    }

    package.field_mappings = normalize_field_mappings(package.field_mappings)?;
    if package.field_mappings.is_empty() {
        return Err(unknown_field_error(
            "entity profile package must declare at least one field mapping",
        ));
    }

    package.expected_outputs =
        normalize_unique_strings(package.expected_outputs, "expected_outputs")?;
    if package.expected_outputs.is_empty() {
        return Err(artifact_contract_error(
            "entity profile package must declare expected outputs",
        ));
    }

    package.execution_modes = normalize_execution_modes(
        package.execution_modes,
        &package.entity_type,
        &package.available_capabilities,
        &package.field_mappings,
        &package.expected_outputs,
    )?;
    if package.execution_modes.is_empty() {
        return Err(artifact_contract_error(
            "entity profile package must declare at least one execution mode",
        ));
    }

    package.limits = normalize_limits(package.limits)?;
    package.project_overrides = normalize_project_overrides(package.project_overrides)?;

    let allowed_object_types = package
        .execution_modes
        .iter()
        .flat_map(|mode| {
            std::iter::once(mode.source_object_type.clone()).chain(mode.target_object_type.clone())
        })
        .collect::<BTreeSet<_>>();

    let field_paths = package
        .field_mappings
        .iter()
        .map(|field| field.field_path.clone())
        .collect::<BTreeSet<_>>();
    for required_field in &package.required_fields {
        if !field_paths.contains(required_field) {
            return Err(unknown_field_error(format!(
                "required field {required_field} is not declared in field_mappings"
            )));
        }
    }
    for field in &package.field_mappings {
        if !allowed_object_types.contains(&field.object_type) {
            return Err(wrong_object_type_error(format!(
                "field mapping {} declares object type {} outside the profile execution modes",
                field.field_path, field.object_type
            )));
        }
        if let Some(view) = field.normalized_view.as_ref()
            && !package.normalized_views.contains_key(view)
        {
            return Err(unknown_field_error(format!(
                "field mapping {} references unknown normalized view {}",
                field.field_path, view
            )));
        }
    }

    Ok(package)
}

pub fn canonical_package_bytes(package: &EntityProfilePackage) -> ProfileResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize entity profile package: {error}"
        ))
    })
}

pub fn entity_profile_package_digest(package: &EntityProfilePackage) -> ProfileResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(hash_bytes(&bytes))
}

pub fn validate_package_for_execution(
    package: &EntityProfilePackage,
    request: &EntityProfileExecutionRequest,
) -> ProfileResult<EntityProfileExecutionPlan> {
    let package = finalize_package(package.clone())?;
    let request = normalize_execution_request(request.clone())?;
    let digest = entity_profile_package_digest(&package)?;

    let mode = package
        .execution_modes
        .iter()
        .find(|candidate| candidate.mode == request.mode)
        .cloned()
        .ok_or_else(|| {
            missing_capability_error(format!(
                "profile {} does not declare execution mode {:?}",
                package.profile, request.mode
            ))
        })?;

    if mode.source_object_type != request.source_object_type {
        return Err(wrong_object_type_error(format!(
            "execution request expected source object type {} but profile mode {:?} declares {}",
            request.source_object_type, request.mode, mode.source_object_type
        )));
    }
    if mode.target_object_type != request.target_object_type {
        return Err(wrong_object_type_error(format!(
            "execution request target object type {:?} does not match profile mode {:?} target {:?}",
            request.target_object_type, request.mode, mode.target_object_type
        )));
    }

    let declared_capabilities = mode
        .required_capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for capability in &request.required_capabilities {
        if !declared_capabilities.contains(capability) {
            return Err(missing_capability_error(format!(
                "profile {} mode {:?} does not declare capability {:?}",
                package.profile, request.mode, capability
            )));
        }
    }

    let declared_outputs = mode.outputs.iter().cloned().collect::<BTreeSet<_>>();
    for output in &request.required_outputs {
        if !declared_outputs.contains(output) {
            return Err(missing_capability_error(format!(
                "profile {} mode {:?} does not declare expected output {}",
                package.profile, request.mode, output
            )));
        }
    }

    Ok(EntityProfileExecutionPlan {
        profile: package.profile.clone(),
        version: package.version.clone(),
        package_digest: digest,
        mode,
        dependency_refs: dependency_refs(&package),
    })
}

pub fn build_project_lock_view(
    package: &EntityProfilePackage,
    applied_overrides: &[AppliedProjectOverride],
) -> ProfileResult<EntityProfileLockView> {
    let package = finalize_package(package.clone())?;
    let mut override_index = BTreeMap::new();
    for entry in &package.project_overrides {
        override_index.insert(entry.key.clone(), entry.clone());
    }

    let defaults = package
        .project_overrides
        .iter()
        .map(|entry| ResolvedProjectOverride {
            key: entry.key.clone(),
            value: entry.default_value.clone(),
            artifact_header_key: entry.artifact_header_key.clone(),
            project_lock_key: entry.project_lock_key.clone(),
            project_id: None,
        })
        .collect::<Vec<_>>();

    let mut normalized_applied = applied_overrides
        .iter()
        .cloned()
        .map(normalize_applied_override)
        .collect::<ProfileResult<Vec<_>>>()?;
    normalized_applied.sort_by(|left, right| left.key.cmp(&right.key));

    let mut seen_override_keys = BTreeSet::new();
    let mut overrides = Vec::with_capacity(normalized_applied.len());
    for applied in normalized_applied {
        if !seen_override_keys.insert(applied.key.clone()) {
            return Err(unknown_override_error(format!(
                "duplicate applied override {}",
                applied.key
            )));
        }
        let declared = override_index.get(&applied.key).ok_or_else(|| {
            unknown_override_error(format!(
                "applied override {} is not declared by profile {}",
                applied.key, package.profile
            ))
        })?;
        overrides.push(ResolvedProjectOverride {
            key: declared.key.clone(),
            value: applied.value,
            artifact_header_key: declared.artifact_header_key.clone(),
            project_lock_key: declared.project_lock_key.clone(),
            project_id: Some(applied.project_id),
        });
    }

    Ok(EntityProfileLockView {
        profile: package.profile.clone(),
        version: package.version.clone(),
        entity_type: package.entity_type.clone(),
        execution_modes: package
            .execution_modes
            .iter()
            .map(|mode| mode.mode)
            .collect(),
        expected_outputs: package.expected_outputs.clone(),
        defaults,
        overrides,
        dependency_refs: dependency_refs(&package),
    })
}

pub fn package_compatibility(
    locked: &EntityProfilePackage,
    candidate: &EntityProfilePackage,
) -> ProfileResult<EntityProfilePackageCompatibility> {
    let locked = finalize_package(locked.clone())?;
    let candidate = finalize_package(candidate.clone())?;

    if locked.profile != candidate.profile {
        return Err(compatibility_policy_error(format!(
            "entity profile ids differ: {} vs {}",
            locked.profile, candidate.profile
        )));
    }

    let locked_major = version_major(&locked.version)?;
    let candidate_major = version_major(&candidate.version)?;
    if locked_major != candidate_major {
        return Err(compatibility_policy_error(format!(
            "entity profile {} changed major version from {} to {}",
            locked.profile, locked_major, candidate_major
        )));
    }

    for (field, left, right) in [
        (
            "entity_type",
            locked.entity_type.as_str(),
            candidate.entity_type.as_str(),
        ),
        (
            "identity_semantics",
            locked.identity_semantics.as_str(),
            candidate.identity_semantics.as_str(),
        ),
        (
            "canonical_type",
            locked.canonical_type.as_str(),
            candidate.canonical_type.as_str(),
        ),
        (
            "patch_namespaces.aliases",
            locked.patch_namespaces.aliases.as_str(),
            candidate.patch_namespaces.aliases.as_str(),
        ),
        (
            "patch_namespaces.distinct",
            locked.patch_namespaces.distinct.as_str(),
            candidate.patch_namespaces.distinct.as_str(),
        ),
        (
            "patch_namespaces.relations",
            locked.patch_namespaces.relations.as_str(),
            candidate.patch_namespaces.relations.as_str(),
        ),
    ] {
        if left != right {
            return Err(compatibility_policy_error(format!(
                "entity profile {} changed {} from {} to {}",
                locked.profile, field, left, right
            )));
        }
    }

    compare_reference(
        "ontology_package",
        &locked.ontology_package,
        &candidate.ontology_package,
    )?;
    compare_reference(
        "identifier_package",
        &locked.identifier_package,
        &candidate.identifier_package,
    )?;
    compare_reference(
        "vocabulary_package",
        &locked.vocabulary_package,
        &candidate.vocabulary_package,
    )?;
    compare_reference(
        "evidence_package",
        &locked.evidence_package,
        &candidate.evidence_package,
    )?;
    compare_reference(
        "evidence_policy",
        &locked.evidence_policy,
        &candidate.evidence_policy,
    )?;
    compare_reference(
        "review_policy",
        &locked.review_policy,
        &candidate.review_policy,
    )?;
    compare_reference(
        "promotion_policy",
        &locked.promotion_policy,
        &candidate.promotion_policy,
    )?;
    compare_reference(
        "frozen_executable_strategy",
        &locked.frozen_executable_strategy,
        &candidate.frozen_executable_strategy,
    )?;
    ensure_ref_subset(
        "normalization package",
        &locked.normalization_packages,
        &candidate.normalization_packages,
    )?;

    ensure_string_subset(
        "required field",
        &locked.required_fields,
        &candidate.required_fields,
    )?;
    ensure_capability_subset(
        &locked.available_capabilities,
        &candidate.available_capabilities,
        "available capability",
    )?;
    ensure_string_subset(
        "expected output",
        &locked.expected_outputs,
        &candidate.expected_outputs,
    )?;

    let candidate_fields = candidate
        .field_mappings
        .iter()
        .map(|field| (field.field_path.clone(), field))
        .collect::<BTreeMap<_, _>>();
    for locked_field in &locked.field_mappings {
        let Some(candidate_field) = candidate_fields.get(&locked_field.field_path) else {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} no longer defines field {}",
                candidate.profile, locked_field.field_path
            )));
        };
        if *candidate_field != locked_field {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} changed field mapping {}",
                candidate.profile, locked_field.field_path
            )));
        }
    }

    let candidate_modes = candidate
        .execution_modes
        .iter()
        .map(|mode| (mode.mode, mode))
        .collect::<BTreeMap<_, _>>();
    for locked_mode in &locked.execution_modes {
        let Some(candidate_mode) = candidate_modes.get(&locked_mode.mode) else {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} no longer defines execution mode {:?}",
                candidate.profile, locked_mode.mode
            )));
        };
        if candidate_mode.source_object_type != locked_mode.source_object_type
            || candidate_mode.target_object_type != locked_mode.target_object_type
            || candidate_mode.link_direction != locked_mode.link_direction
        {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} changed execution mode {:?} object types or direction",
                candidate.profile, locked_mode.mode
            )));
        }
        ensure_capability_subset(
            &locked_mode.required_capabilities,
            &candidate_mode.required_capabilities,
            "mode capability",
        )?;
        ensure_string_subset("mode output", &locked_mode.outputs, &candidate_mode.outputs)?;
        ensure_string_subset(
            "mode field path",
            &locked_mode.field_paths,
            &candidate_mode.field_paths,
        )?;
    }

    let candidate_overrides = candidate
        .project_overrides
        .iter()
        .map(|override_spec| (&override_spec.key, override_spec))
        .collect::<BTreeMap<_, _>>();
    for locked_override in &locked.project_overrides {
        let Some(candidate_override) = candidate_overrides.get(&locked_override.key) else {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} no longer declares project override {}",
                candidate.profile, locked_override.key
            )));
        };
        if candidate_override.artifact_header_key != locked_override.artifact_header_key
            || candidate_override.project_lock_key != locked_override.project_lock_key
        {
            return Err(compatibility_policy_error(format!(
                "candidate profile {} changed override projection for {}",
                candidate.profile, locked_override.key
            )));
        }
    }

    if entity_profile_package_digest(&locked)? == entity_profile_package_digest(&candidate)? {
        Ok(EntityProfilePackageCompatibility::ExactDigest)
    } else {
        Ok(EntityProfilePackageCompatibility::CompatibleSameMajor)
    }
}

fn normalize_view(view_name: &str, view: &mut EntityNormalizedView) -> ProfileResult<()> {
    let field = format!("normalized_views.{view_name}.operators");
    view.operators = normalize_unique_strings(std::mem::take(&mut view.operators), &field)?;
    if view.operators.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must declare at least one operator"
        )));
    }
    Ok(())
}

fn normalize_evidence_lanes(
    lanes: &mut EntityEvidenceLanes,
    normalized_views: &BTreeMap<String, EntityNormalizedView>,
) -> ProfileResult<()> {
    lanes.support = normalize_operator_specs(
        std::mem::take(&mut lanes.support),
        "evidence.support",
        normalized_views,
    )?;
    lanes.cannot_link = normalize_operator_specs(
        std::mem::take(&mut lanes.cannot_link),
        "evidence.cannot_link",
        normalized_views,
    )?;
    lanes.relation_hints = normalize_operator_specs(
        std::mem::take(&mut lanes.relation_hints),
        "evidence.relation_hints",
        normalized_views,
    )?;

    for (field, operators) in [
        ("evidence.support", &lanes.support),
        ("evidence.cannot_link", &lanes.cannot_link),
        ("evidence.relation_hints", &lanes.relation_hints),
    ] {
        if operators.is_empty() {
            return Err(artifact_contract_error(format!(
                "{field} must declare at least one operator"
            )));
        }
    }
    Ok(())
}

fn normalize_operator_specs(
    mut specs: Vec<EntityOperatorSpec>,
    field: &str,
    normalized_views: &BTreeMap<String, EntityNormalizedView>,
) -> ProfileResult<Vec<EntityOperatorSpec>> {
    for spec in &mut specs {
        spec.op = normalized_non_empty(&spec.op, &format!("{field}.op"))?;
        spec.view = spec
            .view
            .take()
            .map(|value| normalized_non_empty(&value, &format!("{field}.view")))
            .transpose()?;
        if let Some(view) = spec.view.as_deref()
            && !normalized_views.contains_key(view)
        {
            return Err(unknown_field_error(format!(
                "{field} references unknown normalized view {view}"
            )));
        }
        for (key, value) in &spec.params {
            normalized_non_empty(key, &format!("{field}.params.key"))?;
            normalized_non_empty(value, &format!("{field}.params.{key}"))?;
        }
    }
    Ok(specs)
}

fn normalize_patch_namespaces(
    mut namespaces: EntityPatchNamespaces,
    profile_id: &str,
) -> ProfileResult<EntityPatchNamespaces> {
    let expected_prefix = format!("{profile_id}.");
    namespaces.aliases = normalized_non_empty(&namespaces.aliases, "patch_namespaces.aliases")?;
    namespaces.distinct = normalized_non_empty(&namespaces.distinct, "patch_namespaces.distinct")?;
    namespaces.relations =
        normalized_non_empty(&namespaces.relations, "patch_namespaces.relations")?;

    for (field, value) in [
        ("patch_namespaces.aliases", namespaces.aliases.as_str()),
        ("patch_namespaces.distinct", namespaces.distinct.as_str()),
        ("patch_namespaces.relations", namespaces.relations.as_str()),
    ] {
        if !value.starts_with(&expected_prefix) {
            return Err(artifact_contract_error(format!(
                "{field} must be scoped to profile {profile_id}"
            )));
        }
    }
    Ok(namespaces)
}

fn normalize_profile_ref(
    mut reference: ProfilePackageRef,
    expected_kind: ProfilePackageRefKind,
    field: &str,
) -> ProfileResult<ProfilePackageRef> {
    if reference.kind != expected_kind {
        return Err(artifact_contract_error(format!(
            "{field} must use kind {:?}, found {:?}",
            expected_kind, reference.kind
        )));
    }
    reference.id = normalized_non_empty(&reference.id, &format!("{field}.id"))?;
    reference.version = normalized_non_empty(&reference.version, &format!("{field}.version"))?;
    reference.content_hash =
        normalized_hash(&reference.content_hash, &format!("{field}.content_hash"))?;
    Ok(reference)
}

fn dedupe_profile_refs(
    mut references: Vec<ProfilePackageRef>,
    field: &str,
) -> ProfileResult<Vec<ProfilePackageRef>> {
    references.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.content_hash.cmp(&right.content_hash))
    });

    let mut deduped: Vec<ProfilePackageRef> = Vec::with_capacity(references.len());
    for reference in references {
        if let Some(previous) = deduped.last()
            && previous.kind == reference.kind
            && previous.id == reference.id
        {
            if previous != &reference {
                return Err(artifact_contract_error(format!(
                    "{field} {} cannot be declared with conflicting content hashes",
                    reference.id
                )));
            }
            continue;
        }
        deduped.push(reference);
    }
    Ok(deduped)
}

fn normalize_capabilities(
    mut capabilities: Vec<ProfileCapability>,
    field: &str,
) -> ProfileResult<Vec<ProfileCapability>> {
    if capabilities.is_empty() {
        return Ok(capabilities);
    }
    capabilities.sort();
    capabilities.dedup();
    if capabilities.is_empty() {
        return Err(missing_capability_error(format!(
            "{field} must declare at least one capability"
        )));
    }
    Ok(capabilities)
}

fn normalize_field_mappings(
    mut mappings: Vec<EntityProfileFieldMapping>,
) -> ProfileResult<Vec<EntityProfileFieldMapping>> {
    for mapping in &mut mappings {
        mapping.field_path =
            normalized_non_empty(&mapping.field_path, "field_mappings.field_path")?;
        mapping.object_type =
            normalized_non_empty(&mapping.object_type, "field_mappings.object_type")?;
        mapping.field_role =
            normalized_non_empty(&mapping.field_role, "field_mappings.field_role")?;
        mapping.normalized_view = mapping
            .normalized_view
            .take()
            .map(|value| normalized_non_empty(&value, "field_mappings.normalized_view"))
            .transpose()?;
    }
    mappings.sort_by(|left, right| left.field_path.cmp(&right.field_path));

    let mut deduped: Vec<EntityProfileFieldMapping> = Vec::with_capacity(mappings.len());
    for mapping in mappings {
        if let Some(previous) = deduped.last()
            && previous.field_path == mapping.field_path
        {
            if previous != &mapping {
                return Err(unknown_field_error(format!(
                    "field mapping {} cannot be declared with conflicting content",
                    mapping.field_path
                )));
            }
            continue;
        }
        deduped.push(mapping);
    }
    Ok(deduped)
}

fn normalize_execution_modes(
    mut modes: Vec<EntityProfileMode>,
    primary_entity_type: &str,
    available_capabilities: &[ProfileCapability],
    field_mappings: &[EntityProfileFieldMapping],
    expected_outputs: &[String],
) -> ProfileResult<Vec<EntityProfileMode>> {
    let field_paths = field_mappings
        .iter()
        .map(|field| field.field_path.as_str())
        .collect::<BTreeSet<_>>();
    let available_capabilities = available_capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let expected_outputs = expected_outputs
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for mode in &mut modes {
        mode.source_object_type = normalized_non_empty(
            &mode.source_object_type,
            "execution_modes.source_object_type",
        )?;
        mode.target_object_type = mode
            .target_object_type
            .take()
            .map(|value| normalized_non_empty(&value, "execution_modes.target_object_type"))
            .transpose()?;
        mode.required_capabilities = normalize_capabilities(
            std::mem::take(&mut mode.required_capabilities),
            "execution_modes.required_capabilities",
        )?;
        if mode.required_capabilities.is_empty() {
            return Err(missing_capability_error(
                "execution mode must declare required capabilities",
            ));
        }
        mode.field_paths = normalize_unique_strings(
            std::mem::take(&mut mode.field_paths),
            "execution_modes.field_paths",
        )?;
        mode.outputs =
            normalize_unique_strings(std::mem::take(&mut mode.outputs), "execution_modes.outputs")?;
        if mode.field_paths.is_empty() {
            return Err(unknown_field_error(
                "execution mode must reference at least one declared field",
            ));
        }
        if mode.outputs.is_empty() {
            return Err(artifact_contract_error(
                "execution mode must declare at least one expected output",
            ));
        }

        if mode.source_object_type != primary_entity_type {
            return Err(wrong_object_type_error(format!(
                "execution mode {:?} must use primary entity_type {} as source_object_type",
                mode.mode, primary_entity_type
            )));
        }

        match mode.mode {
            ProfileModeKind::Cluster => {
                if mode.link_direction.is_some() || mode.target_object_type.is_some() {
                    return Err(artifact_contract_error(
                        "cluster mode must not declare link_direction or target_object_type",
                    ));
                }
                ensure_mode_capability(
                    &mode.required_capabilities,
                    ProfileCapability::SolveCluster,
                )?;
                if mode
                    .required_capabilities
                    .contains(&ProfileCapability::SolveLink)
                {
                    return Err(missing_capability_error(
                        "cluster mode cannot declare solve_link capability",
                    ));
                }
            }
            ProfileModeKind::Link => {
                if mode.link_direction.is_none() {
                    return Err(artifact_contract_error(
                        "link mode must declare link_direction",
                    ));
                }
                if mode.target_object_type.is_none() {
                    return Err(wrong_object_type_error(
                        "link mode must declare target_object_type",
                    ));
                }
                ensure_mode_capability(&mode.required_capabilities, ProfileCapability::SolveLink)?;
                if mode
                    .required_capabilities
                    .contains(&ProfileCapability::SolveCluster)
                {
                    return Err(missing_capability_error(
                        "link mode cannot declare solve_cluster capability",
                    ));
                }
            }
        }

        for capability in &mode.required_capabilities {
            if !available_capabilities.contains(capability) {
                return Err(missing_capability_error(format!(
                    "execution mode {:?} requires undeclared capability {:?}",
                    mode.mode, capability
                )));
            }
        }
        for field_path in &mode.field_paths {
            if !field_paths.contains(field_path.as_str()) {
                return Err(unknown_field_error(format!(
                    "execution mode {:?} references unknown field {}",
                    mode.mode, field_path
                )));
            }
        }
        for output in &mode.outputs {
            if !expected_outputs.contains(output.as_str()) {
                return Err(artifact_contract_error(format!(
                    "execution mode {:?} references unknown expected output {}",
                    mode.mode, output
                )));
            }
        }
    }

    modes.sort_by_key(|mode| mode.mode);
    let mut deduped: Vec<EntityProfileMode> = Vec::with_capacity(modes.len());
    for mode in modes {
        if let Some(previous) = deduped.last()
            && previous.mode == mode.mode
        {
            if previous != &mode {
                return Err(artifact_contract_error(format!(
                    "execution mode {:?} cannot be declared with conflicting content",
                    mode.mode
                )));
            }
            continue;
        }
        deduped.push(mode);
    }
    Ok(deduped)
}

fn ensure_mode_capability(
    capabilities: &[ProfileCapability],
    required: ProfileCapability,
) -> ProfileResult<()> {
    if capabilities.contains(&required) {
        Ok(())
    } else {
        Err(missing_capability_error(format!(
            "execution mode must declare capability {:?}",
            required
        )))
    }
}

fn normalize_limits(limits: EntityProfileLimits) -> ProfileResult<EntityProfileLimits> {
    if limits.max_observation_fields == 0 {
        return Err(artifact_contract_error(
            "limits.max_observation_fields must be greater than zero",
        ));
    }
    if limits.max_candidate_pairs == 0 {
        return Err(artifact_contract_error(
            "limits.max_candidate_pairs must be greater than zero",
        ));
    }
    if limits.max_outputs == 0 {
        return Err(artifact_contract_error(
            "limits.max_outputs must be greater than zero",
        ));
    }
    Ok(limits)
}

fn normalize_project_overrides(
    mut overrides: Vec<EntityProfileProjectOverride>,
) -> ProfileResult<Vec<EntityProfileProjectOverride>> {
    for override_spec in &mut overrides {
        override_spec.key = normalized_non_empty(&override_spec.key, "project_overrides.key")?;
        override_spec.default_value = normalized_non_empty(
            &override_spec.default_value,
            "project_overrides.default_value",
        )?;
        override_spec.artifact_header_key = normalized_non_empty(
            &override_spec.artifact_header_key,
            "project_overrides.artifact_header_key",
        )?;
        override_spec.project_lock_key = normalized_non_empty(
            &override_spec.project_lock_key,
            "project_overrides.project_lock_key",
        )?;
    }
    overrides.sort_by(|left, right| left.key.cmp(&right.key));

    let mut deduped: Vec<EntityProfileProjectOverride> = Vec::with_capacity(overrides.len());
    for override_spec in overrides {
        if let Some(previous) = deduped.last()
            && previous.key == override_spec.key
        {
            if previous != &override_spec {
                return Err(artifact_contract_error(format!(
                    "project override {} cannot be declared with conflicting content",
                    override_spec.key
                )));
            }
            continue;
        }
        deduped.push(override_spec);
    }
    Ok(deduped)
}

fn normalize_applied_override(
    mut override_spec: AppliedProjectOverride,
) -> ProfileResult<AppliedProjectOverride> {
    override_spec.key = normalized_non_empty(&override_spec.key, "applied_overrides.key")?;
    override_spec.value = normalized_non_empty(&override_spec.value, "applied_overrides.value")?;
    override_spec.project_id =
        normalized_non_empty(&override_spec.project_id, "applied_overrides.project_id")?;
    Ok(override_spec)
}

fn normalize_execution_request(
    mut request: EntityProfileExecutionRequest,
) -> ProfileResult<EntityProfileExecutionRequest> {
    request.source_object_type =
        normalized_non_empty(&request.source_object_type, "source_object_type")?;
    request.target_object_type = request
        .target_object_type
        .take()
        .map(|value| normalized_non_empty(&value, "target_object_type"))
        .transpose()?;
    request.required_capabilities = normalize_capabilities(
        std::mem::take(&mut request.required_capabilities),
        "required_capabilities",
    )?;
    request.required_outputs = normalize_unique_strings(
        std::mem::take(&mut request.required_outputs),
        "required_outputs",
    )?;
    Ok(request)
}

fn dependency_refs(package: &EntityProfilePackage) -> Vec<ProfilePackageRef> {
    let mut references = vec![
        package.ontology_package.clone(),
        package.identifier_package.clone(),
        package.vocabulary_package.clone(),
        package.evidence_package.clone(),
        package.evidence_policy.clone(),
        package.review_policy.clone(),
        package.promotion_policy.clone(),
        package.frozen_executable_strategy.clone(),
    ];
    references.extend(package.normalization_packages.iter().cloned());
    references.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.version.cmp(&right.version))
    });
    references
}

fn compare_reference(
    field: &str,
    locked: &ProfilePackageRef,
    candidate: &ProfilePackageRef,
) -> ProfileResult<()> {
    if locked != candidate {
        return Err(compatibility_policy_error(format!(
            "{field} changed from {:?} to {:?}",
            locked, candidate
        )));
    }
    Ok(())
}

fn ensure_ref_subset(
    field: &str,
    locked: &[ProfilePackageRef],
    candidate: &[ProfilePackageRef],
) -> ProfileResult<()> {
    let candidate = candidate.iter().cloned().collect::<BTreeSet<_>>();
    for reference in locked {
        if !candidate.contains(reference) {
            return Err(compatibility_policy_error(format!(
                "candidate profile no longer declares locked {field} {}",
                reference.id
            )));
        }
    }
    Ok(())
}

fn ensure_capability_subset(
    locked: &[ProfileCapability],
    candidate: &[ProfileCapability],
    field: &str,
) -> ProfileResult<()> {
    let candidate = candidate.iter().copied().collect::<BTreeSet<_>>();
    for capability in locked {
        if !candidate.contains(capability) {
            return Err(compatibility_policy_error(format!(
                "candidate profile no longer declares locked {field} {:?}",
                capability
            )));
        }
    }
    Ok(())
}

fn ensure_string_subset(field: &str, locked: &[String], candidate: &[String]) -> ProfileResult<()> {
    let candidate = candidate
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for value in locked {
        if !candidate.contains(value.as_str()) {
            return Err(compatibility_policy_error(format!(
                "candidate profile no longer declares locked {field} {value}"
            )));
        }
    }
    Ok(())
}

fn normalize_unique_strings(values: Vec<String>, field: &str) -> ProfileResult<Vec<String>> {
    let mut normalized = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<ProfileResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalized_non_empty(value: &str, field: &str) -> ProfileResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty after trimming"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_version(value: &str, field: &str) -> ProfileResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let major = normalized
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error(format!("{field} must contain a version major")))?;
    major.parse::<u64>().map_err(|error| {
        artifact_contract_error(format!(
            "{field} must start with a numeric major version: {error}"
        ))
    })?;
    Ok(normalized)
}

fn version_major(value: &str) -> ProfileResult<u64> {
    let version = normalized_version(value, "version")?;
    version
        .split('.')
        .next()
        .ok_or_else(|| artifact_contract_error("version must contain a major component"))?
        .parse::<u64>()
        .map_err(|error| artifact_contract_error(format!("version major must be numeric: {error}")))
}

fn normalized_hash(value: &str, field: &str) -> ProfileResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if normalized.starts_with("blake3:") && normalized.len() > "blake3:".len() {
        Ok(normalized)
    } else {
        Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )))
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn artifact_contract_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::CompatibilityPolicy, message)
}

fn missing_capability_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::MissingCapability, message)
}

fn wrong_object_type_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::WrongObjectType, message)
}

fn unknown_field_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::UnknownField, message)
}

fn unknown_override_error(message: impl Into<String>) -> ProfileError {
    ProfileError::new(ProfileErrorCode::UnknownOverride, message)
}
