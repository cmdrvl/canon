#![forbid(unsafe_code)]

//! Domain-neutral source-mapping package contract.
//!
//! Source-mapping packages let generic record readers project arbitrary source
//! schemas into separate observation, typed assignment, and relationship
//! artifacts. Parsing preserves source-locator and mapping-digest provenance
//! and never infers canonical identity during intake.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    error::Error,
    fmt,
};

pub const CANON_SOURCE_MAPPING_VERSION: &str = "canon.source.mapping.v1";
pub const CANON_ENTITY_RECORD_LINK_INPUT_VERSION: &str = "canon.entity.record_link_input.v1";

pub type SourceMappingResult<T> = Result<T, SourceMappingError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMappingErrorCode {
    ArtifactContract,
    CompatibilityPolicy,
    MissingProfile,
    MissingField,
    InputShape,
    PolicyConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMappingError {
    pub code: SourceMappingErrorCode,
    pub message: String,
}

impl SourceMappingError {
    pub fn new(code: SourceMappingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for SourceMappingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Csv,
    Tsv,
    #[default]
    Jsonl,
    Parquet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapturePolicy {
    Preserve,
    Quarantine,
    #[default]
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellDispositionReason {
    UnknownField,
    AmbiguousCell,
    MissingRequired,
    NullValue,
    UnknownRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMappingPackage {
    pub version: String,
    pub package_id: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<SourceMappingProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<SourceMappingDocumentationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceMappingDocumentationRef {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMappingProfile {
    pub profile_id: String,
    pub source_system: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_formats: Vec<SourceFormat>,
    pub object_id_path: String,
    pub locator_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<ObservationMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<AssignmentMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<RelationshipMapping>,
    #[serde(default)]
    pub policies: SourceMappingPolicies,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMappingPolicies {
    pub unknown_field: CapturePolicy,
    pub ambiguous_cell: CapturePolicy,
    pub missing_required: CapturePolicy,
    pub null_value: CapturePolicy,
    pub unknown_role: CapturePolicy,
}

impl Default for SourceMappingPolicies {
    fn default() -> Self {
        Self {
            unknown_field: CapturePolicy::Preserve,
            ambiguous_cell: CapturePolicy::Quarantine,
            missing_required: CapturePolicy::Reject,
            null_value: CapturePolicy::Quarantine,
            unknown_role: CapturePolicy::Preserve,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnchorMapping {
    pub namespace: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationMapping {
    pub mapping_id: String,
    pub subject_type_id: String,
    pub surface_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor_mappings: Vec<AnchorMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentMapping {
    pub mapping_id: String,
    pub subject_type_id: String,
    pub assignee_type_id: String,
    pub role_binding: RoleBinding,
    pub assignee_surface_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignee_anchor_mappings: Vec<AnchorMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipMapping {
    pub mapping_id: String,
    pub subject_type_id: String,
    pub relation_type_id: String,
    pub object_type_id: String,
    pub object_surface_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_anchor_mappings: Vec<AnchorMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoleBinding {
    Literal {
        role_id: String,
    },
    Field {
        path: String,
        namespace: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        allowed_values: BTreeMap<String, String>,
        #[serde(default)]
        allow_verbatim_values: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceMappingProfileRef {
    pub package_digest: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    pub format: SourceFormat,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct SourceLocator {
    pub source_system: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TemporalContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingProvenance {
    pub profile_id: String,
    pub mapping_digest: String,
    pub source_locator: SourceLocator,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub raw_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NamespacedAnchor {
    pub namespace: String,
    pub value: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedCell {
    pub path: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationArtifact {
    pub observation_id: String,
    pub mapping_id: String,
    pub object_id: String,
    pub subject_type_id: String,
    pub surface: MappedCell,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<NamespacedAnchor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, Value>,
    pub temporal: TemporalContext,
    pub provenance: MappingProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentArtifact {
    pub assignment_id: String,
    pub mapping_id: String,
    pub subject_object_id: String,
    pub subject_type_id: String,
    pub role_id: String,
    pub assignee_type_id: String,
    pub assignee_surface: MappedCell,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignee_anchors: Vec<NamespacedAnchor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, Value>,
    pub temporal: TemporalContext,
    pub provenance: MappingProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipArtifact {
    pub relationship_id: String,
    pub mapping_id: String,
    pub subject_object_id: String,
    pub subject_type_id: String,
    pub relation_type_id: String,
    pub object_type_id: String,
    pub object_surface: MappedCell,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_anchors: Vec<NamespacedAnchor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, Value>,
    pub temporal: TemporalContext,
    pub provenance: MappingProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservedCell {
    pub reason: CellDispositionReason,
    pub path: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantinedCell {
    pub reason: CellDispositionReason,
    pub path: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MappedSourceArtifacts {
    pub mapping_digest: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_locator: Option<SourceLocator>,
    pub temporal: TemporalContext,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<ObservationArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<AssignmentArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<RelationshipArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved_cells: Vec<PreservedCell>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quarantined_cells: Vec<QuarantinedCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkComparisonKind {
    Numeric,
    Date,
    #[default]
    Categorical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkComparisonSource {
    #[default]
    AssignmentContext,
    ObservationContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkComparisonMapping {
    pub feature_id: String,
    pub source: RecordLinkComparisonSource,
    pub path: String,
    pub value_kind: RecordLinkComparisonKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    #[serde(default)]
    pub policies: RecordLinkComparisonPolicies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkComparisonPolicies {
    pub missing: CapturePolicy,
    pub malformed: CapturePolicy,
    pub overflow: CapturePolicy,
    pub incomparable: CapturePolicy,
}

impl Default for RecordLinkComparisonPolicies {
    fn default() -> Self {
        Self {
            missing: CapturePolicy::Reject,
            malformed: CapturePolicy::Reject,
            overflow: CapturePolicy::Reject,
            incomparable: CapturePolicy::Reject,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkInputBuildRequest {
    pub source_id: String,
    pub scope_id: String,
    pub profile_id: String,
    pub profile_digest: String,
    pub input_digest: String,
    pub source_mapping_digest: String,
    pub subject_observation_mapping_id: String,
    #[serde(default)]
    pub assignment_mapping_ids: Vec<String>,
    #[serde(default)]
    pub missing_assignment_policy: CapturePolicy,
    #[serde(default)]
    pub comparison_mappings: Vec<RecordLinkComparisonMapping>,
    #[serde(default)]
    pub duplicate_record_policy: CapturePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkInputSidecar {
    pub version: String,
    pub artifact_content_hash: String,
    pub source_id: String,
    pub scope_id: String,
    pub profile_id: String,
    pub profile_digest: String,
    pub input_digest: String,
    pub source_mapping_digest: String,
    pub summary: BTreeMap<String, u64>,
    pub records: Vec<RecordLinkInputRecord>,
    #[serde(default)]
    pub source_cell_dispositions: Vec<RecordLinkSourceCellDisposition>,
    #[serde(default)]
    pub quarantined_records: Vec<RecordLinkQuarantinedRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkInputRecord {
    pub record_id: String,
    pub source_ref: RecordLinkSourceRef,
    pub subject_observation_ref: RecordLinkObservationRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<RecordLinkAssignmentRef>,
    pub comparison_views: Vec<RecordLinkComparisonView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quarantined_fields: Vec<RecordLinkQuarantinedField>,
    pub provenance: RecordLinkInputProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkSourceRef {
    pub source_id: String,
    pub scope_id: String,
    pub source_object_id: String,
    pub source_locator: SourceLocator,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkObservationRef {
    pub observation_id: String,
    pub mapping_id: String,
    pub subject_type_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkAssignmentRef {
    pub assignment_id: String,
    pub mapping_id: String,
    pub role_id: String,
    pub assignee_type_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordLinkComparisonView {
    Numeric {
        feature_id: String,
        source: RecordLinkComparisonSource,
        source_path: String,
        units: String,
        scaled_value: i64,
        scale: u32,
    },
    Date {
        feature_id: String,
        source: RecordLinkComparisonSource,
        source_path: String,
        value: String,
    },
    Categorical {
        feature_id: String,
        source: RecordLinkComparisonSource,
        source_path: String,
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkFieldDispositionReason {
    MissingField,
    MalformedField,
    Overflow,
    IncomparableField,
    DuplicateRecordId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkQuarantinedField {
    pub reason: RecordLinkFieldDispositionReason,
    pub feature_id: String,
    pub source: RecordLinkComparisonSource,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkQuarantinedRecord {
    pub reason: RecordLinkFieldDispositionReason,
    pub record_id: String,
    pub source_ref: RecordLinkSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_ref: Option<RecordLinkAssignmentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_assignment_mapping_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordLinkSourceCellDispositionKind {
    Preserved,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordLinkSourceCellDisposition {
    pub disposition: RecordLinkSourceCellDispositionKind,
    pub reason: CellDispositionReason,
    pub source_ref: RecordLinkSourceRef,
    pub path: String,
    pub value_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLinkInputProvenance {
    pub profile_digest: String,
    pub input_digest: String,
    pub source_mapping_digest: String,
    pub source_locator: SourceLocator,
    pub raw_field_hashes: BTreeMap<String, String>,
}

pub fn source_mapping_schema_version() -> &'static str {
    CANON_SOURCE_MAPPING_VERSION
}

pub fn record_link_input_schema_version() -> &'static str {
    CANON_ENTITY_RECORD_LINK_INPUT_VERSION
}

pub fn finalize_package(
    mut package: SourceMappingPackage,
) -> SourceMappingResult<SourceMappingPackage> {
    if package.version.trim().is_empty() {
        package.version = CANON_SOURCE_MAPPING_VERSION.to_string();
    }
    if package.version != CANON_SOURCE_MAPPING_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported source-mapping contract version: {}",
            package.version
        )));
    }

    package.package_id = normalized_package_id(&package.package_id, "package_id")?;
    package.package_version = normalized_semver(&package.package_version, "package_version")?;

    let mut documentation = package
        .documentation
        .into_iter()
        .map(normalize_documentation_ref)
        .collect::<SourceMappingResult<Vec<_>>>()?;
    documentation.sort();
    documentation.dedup();

    let known_docs = documentation
        .iter()
        .map(|entry| entry.uri.clone())
        .collect::<BTreeSet<_>>();

    let mut profiles = package
        .profiles
        .into_iter()
        .map(|profile| normalize_profile(profile, &known_docs))
        .collect::<SourceMappingResult<Vec<_>>>()?;
    if profiles.is_empty() {
        return Err(artifact_contract_error(
            "source-mapping package must declare at least one profile",
        ));
    }
    profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));

    let mut deduped: Vec<SourceMappingProfile> = Vec::with_capacity(profiles.len());
    for profile in profiles {
        if let Some(previous) = deduped.last()
            && previous.profile_id == profile.profile_id
        {
            if previous != &profile {
                return Err(artifact_contract_error(format!(
                    "profile {} cannot be declared with conflicting content",
                    profile.profile_id
                )));
            }
            continue;
        }
        deduped.push(profile);
    }

    package.documentation = documentation;
    package.profiles = deduped;
    Ok(package)
}

pub fn finalize_profile_ref(
    mut reference: SourceMappingProfileRef,
) -> SourceMappingResult<SourceMappingProfileRef> {
    reference.package_digest = normalized_hash(&reference.package_digest, "package_digest")?;
    reference.profile_id = normalized_opaque_ref(&reference.profile_id, "profile_id")?;
    Ok(reference)
}

pub fn canonical_package_bytes(package: &SourceMappingPackage) -> SourceMappingResult<Vec<u8>> {
    let package = finalize_package(package.clone())?;
    serde_json::to_vec(&package).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize source-mapping package: {error}"
        ))
    })
}

pub fn source_mapping_package_digest(
    package: &SourceMappingPackage,
) -> SourceMappingResult<String> {
    let bytes = canonical_package_bytes(package)?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn resolve_profile_ref(
    package: &SourceMappingPackage,
    reference: &SourceMappingProfileRef,
) -> SourceMappingResult<SourceMappingProfile> {
    let package = finalize_package(package.clone())?;
    let reference = finalize_profile_ref(reference.clone())?;
    let digest = source_mapping_package_digest(&package)?;
    if reference.package_digest != digest {
        return Err(compatibility_policy_error(format!(
            "source-mapping profile {} is pinned to {} but package resolved to {}",
            reference.profile_id, reference.package_digest, digest
        )));
    }

    package
        .profiles
        .iter()
        .find(|profile| profile.profile_id == reference.profile_id)
        .cloned()
        .ok_or_else(|| {
            missing_profile_error(format!(
                "unknown source-mapping profile {}",
                reference.profile_id
            ))
        })
}

pub fn validate_package_for_execution(
    package: &SourceMappingPackage,
    references: &[SourceMappingProfileRef],
) -> SourceMappingResult<String> {
    let package = finalize_package(package.clone())?;
    let digest = source_mapping_package_digest(&package)?;
    for reference in references {
        let reference = finalize_profile_ref(reference.clone())?;
        if reference.package_digest != digest {
            return Err(compatibility_policy_error(format!(
                "source-mapping profile {} is pinned to {} but package resolved to {}",
                reference.profile_id, reference.package_digest, digest
            )));
        }
        let _ = resolve_profile_ref(&package, &reference)?;
    }
    Ok(digest)
}

pub fn map_record(
    package: &SourceMappingPackage,
    reference: &SourceMappingProfileRef,
    record: &SourceRecord,
) -> SourceMappingResult<MappedSourceArtifacts> {
    let package = finalize_package(package.clone())?;
    let profile = resolve_profile_ref(&package, reference)?;
    if !profile.source_formats.contains(&record.format) {
        return Err(input_shape_error(format!(
            "profile {} does not support record format {:?}",
            profile.profile_id, record.format
        )));
    }

    let payload = record
        .payload
        .as_object()
        .ok_or_else(|| input_shape_error("source record payload must be a JSON object"))?;
    let mapping_digest = source_mapping_package_digest(&package)?;

    let mut preserved_cells = Vec::new();
    let mut quarantined_cells = Vec::new();

    let object_id = required_scalar(
        payload,
        &profile.object_id_path,
        &profile.policies,
        &mut preserved_cells,
        &mut quarantined_cells,
    )?;
    let locator = required_scalar(
        payload,
        &profile.locator_path,
        &profile.policies,
        &mut preserved_cells,
        &mut quarantined_cells,
    )?;
    let fragment = optional_scalar(
        payload,
        profile.fragment_path.as_deref(),
        &profile.policies,
        &mut preserved_cells,
        &mut quarantined_cells,
    )?;
    let temporal = TemporalContext {
        as_of: optional_scalar(
            payload,
            profile.as_of_path.as_deref(),
            &profile.policies,
            &mut preserved_cells,
            &mut quarantined_cells,
        )?,
        valid_from: optional_scalar(
            payload,
            profile.valid_from_path.as_deref(),
            &profile.policies,
            &mut preserved_cells,
            &mut quarantined_cells,
        )?,
        valid_to: optional_scalar(
            payload,
            profile.valid_to_path.as_deref(),
            &profile.policies,
            &mut preserved_cells,
            &mut quarantined_cells,
        )?,
    };

    let source_locator = locator.map(|locator| SourceLocator {
        source_system: profile.source_system.clone(),
        locator,
        fragment,
    });

    let mut bundle = MappedSourceArtifacts {
        mapping_digest: mapping_digest.clone(),
        profile_id: profile.profile_id.clone(),
        object_id: object_id.clone(),
        source_locator: source_locator.clone(),
        temporal: temporal.clone(),
        observations: Vec::new(),
        assignments: Vec::new(),
        relationships: Vec::new(),
        preserved_cells,
        quarantined_cells,
    };

    if let (Some(object_id), Some(source_locator)) = (object_id.clone(), source_locator.clone()) {
        let mut execution = MappingExecution {
            payload,
            profile: &profile,
            mapping_digest: &mapping_digest,
            object_id: &object_id,
            source_locator: &source_locator,
            temporal: &temporal,
            preserved_cells: &mut bundle.preserved_cells,
            quarantined_cells: &mut bundle.quarantined_cells,
        };

        for observation in &profile.observations {
            if let Some(mapped) = render_observation(&mut execution, observation)? {
                bundle.observations.push(mapped);
            }
        }

        for assignment in &profile.assignments {
            if let Some(mapped) = render_assignment(&mut execution, assignment)? {
                bundle.assignments.push(mapped);
            }
        }

        for relationship in &profile.relationships {
            if let Some(mapped) = render_relationship(&mut execution, relationship)? {
                bundle.relationships.push(mapped);
            }
        }
    }

    capture_unknown_fields(payload, &profile, &mut bundle)?;
    bundle.preserved_cells.sort_by(cell_cmp_preserved);
    bundle.quarantined_cells.sort_by(cell_cmp_quarantined);
    Ok(bundle)
}

pub fn build_record_link_input_sidecar(
    request: &RecordLinkInputBuildRequest,
    bundles: &[MappedSourceArtifacts],
) -> SourceMappingResult<RecordLinkInputSidecar> {
    let request = normalize_record_link_request(request.clone())?;
    let requested_assignments = request
        .assignment_mapping_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut records = Vec::new();
    let mut source_cell_dispositions = Vec::new();
    let mut quarantined_records = Vec::new();
    let mut seen_record_bytes = BTreeMap::new();

    for bundle in bundles {
        validate_record_link_bundle_binding(bundle, &request)?;
        let source_locator = bundle.source_locator.clone().ok_or_else(|| {
            missing_field_error("record-link input requires a source locator on every mapped row")
        })?;
        let source_object_id = bundle.object_id.clone().ok_or_else(|| {
            missing_field_error("record-link input requires an object id on every mapped row")
        })?;
        let observation = bundle
            .observations
            .iter()
            .find(|observation| observation.mapping_id == request.subject_observation_mapping_id)
            .ok_or_else(|| {
                missing_field_error(format!(
                    "record-link input row {} is missing subject observation mapping {}",
                    source_locator.locator, request.subject_observation_mapping_id
                ))
            })?;

        let source_ref = RecordLinkSourceRef {
            source_id: request.source_id.clone(),
            scope_id: request.scope_id.clone(),
            source_object_id,
            source_locator: source_locator.clone(),
        };
        let observation_ref = RecordLinkObservationRef {
            observation_id: observation.observation_id.clone(),
            mapping_id: observation.mapping_id.clone(),
            subject_type_id: observation.subject_type_id.clone(),
        };
        source_cell_dispositions.extend(record_link_source_cell_dispositions(
            &source_ref,
            &bundle.preserved_cells,
            &bundle.quarantined_cells,
        )?);

        if request.assignment_mapping_ids.is_empty() {
            let record = build_record_link_record(
                &request,
                &source_ref,
                &observation_ref,
                observation,
                None,
            )?;
            insert_record_link_record(
                record,
                request.duplicate_record_policy,
                &mut seen_record_bytes,
                &mut records,
                &mut quarantined_records,
            )?;
        } else {
            let mut assignments = bundle
                .assignments
                .iter()
                .filter(|assignment| requested_assignments.contains(assignment.mapping_id.as_str()))
                .collect::<Vec<_>>();
            assignments.sort_by(|left, right| {
                left.mapping_id
                    .cmp(&right.mapping_id)
                    .then_with(|| left.assignment_id.cmp(&right.assignment_id))
            });
            let row_assignment_ids = assignments
                .iter()
                .map(|assignment| assignment.mapping_id.as_str())
                .collect::<BTreeSet<_>>();
            for mapping_id in &request.assignment_mapping_ids {
                if !row_assignment_ids.contains(mapping_id.as_str()) {
                    apply_record_link_missing_assignment_policy(
                        request.missing_assignment_policy,
                        mapping_id,
                        &source_ref,
                        &mut quarantined_records,
                    )?;
                }
            }
            for assignment in assignments {
                let record = build_record_link_record(
                    &request,
                    &source_ref,
                    &observation_ref,
                    observation,
                    Some(assignment),
                )?;
                insert_record_link_record(
                    record,
                    request.duplicate_record_policy,
                    &mut seen_record_bytes,
                    &mut records,
                    &mut quarantined_records,
                )?;
            }
        }
    }

    records.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    source_cell_dispositions.sort();
    quarantined_records.sort();
    quarantined_records.dedup();
    let mut sidecar = RecordLinkInputSidecar {
        version: CANON_ENTITY_RECORD_LINK_INPUT_VERSION.to_string(),
        artifact_content_hash: String::new(),
        source_id: request.source_id,
        scope_id: request.scope_id,
        profile_id: request.profile_id,
        profile_digest: request.profile_digest,
        input_digest: request.input_digest,
        source_mapping_digest: request.source_mapping_digest,
        summary: BTreeMap::new(),
        records,
        source_cell_dispositions,
        quarantined_records,
    };
    sidecar.summary = record_link_summary(&sidecar);
    sidecar.artifact_content_hash = record_link_input_hash(&sidecar)?;
    validate_record_link_input_sidecar(&sidecar)?;
    Ok(sidecar)
}

pub fn canonical_record_link_input_bytes(
    sidecar: &RecordLinkInputSidecar,
) -> SourceMappingResult<Vec<u8>> {
    validate_record_link_input_sidecar(sidecar)?;
    serde_json::to_vec(sidecar).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize record-link input sidecar: {error}"
        ))
    })
}

pub fn validate_record_link_input_sidecar(
    sidecar: &RecordLinkInputSidecar,
) -> SourceMappingResult<()> {
    if sidecar.version != CANON_ENTITY_RECORD_LINK_INPUT_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported record-link input version: {}",
            sidecar.version
        )));
    }
    normalized_non_empty(&sidecar.source_id, "source_id")?;
    normalized_non_empty(&sidecar.scope_id, "scope_id")?;
    normalized_opaque_ref(&sidecar.profile_id, "profile_id")?;
    normalized_hash(&sidecar.profile_digest, "profile_digest")?;
    normalized_hash(&sidecar.input_digest, "input_digest")?;
    normalized_hash(&sidecar.source_mapping_digest, "source_mapping_digest")?;
    normalized_hash(&sidecar.artifact_content_hash, "artifact_content_hash")?;
    let expected_summary = record_link_summary(sidecar);
    if sidecar.summary != expected_summary {
        return Err(artifact_contract_error(
            "record-link input summary does not match records",
        ));
    }
    let expected_hash = record_link_input_hash(sidecar)?;
    if sidecar.artifact_content_hash != expected_hash {
        return Err(artifact_contract_error(format!(
            "record-link input self hash mismatch: expected {expected_hash}, got {}",
            sidecar.artifact_content_hash
        )));
    }

    let mut seen_record_ids = BTreeSet::new();
    for record in &sidecar.records {
        if !seen_record_ids.insert(record.record_id.as_str()) {
            return Err(artifact_contract_error(format!(
                "record-link input repeats record_id {}",
                record.record_id
            )));
        }
        let expected_record_id = derive_record_link_record_id(record)?;
        if record.record_id != expected_record_id {
            return Err(artifact_contract_error(format!(
                "record-link input record_id mismatch: expected {expected_record_id}, got {}",
                record.record_id
            )));
        }
        if record.source_ref.source_id != sidecar.source_id
            || record.source_ref.scope_id != sidecar.scope_id
            || record.provenance.profile_digest != sidecar.profile_digest
            || record.provenance.input_digest != sidecar.input_digest
            || record.provenance.source_mapping_digest != sidecar.source_mapping_digest
        {
            return Err(artifact_contract_error(
                "record-link input record provenance does not match sidecar binding",
            ));
        }
        if record
            .comparison_views
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(artifact_contract_error(
                "record-link comparison views must be sorted and unique",
            ));
        }
        for view in &record.comparison_views {
            validate_record_link_comparison_view(view)?;
        }
        if record
            .quarantined_fields
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(artifact_contract_error(
                "record-link quarantined fields must be sorted and unique",
            ));
        }
        for field in &record.quarantined_fields {
            normalized_opaque_ref(&field.feature_id, "quarantined_fields.feature_id")?;
            normalized_path(&field.source_path, "quarantined_fields.source_path")?;
            if let Some(hash) = &field.value_hash {
                normalized_hash(hash, "quarantined_fields.value_hash")?;
            }
        }
    }
    if sidecar
        .records
        .windows(2)
        .any(|pair| pair[0].record_id >= pair[1].record_id)
    {
        return Err(artifact_contract_error(
            "record-link records must be sorted by record_id",
        ));
    }
    if sidecar
        .source_cell_dispositions
        .windows(2)
        .any(|pair| pair[0] > pair[1])
    {
        return Err(artifact_contract_error(
            "record-link source cell dispositions must be sorted",
        ));
    }
    for disposition in &sidecar.source_cell_dispositions {
        if disposition.source_ref.source_id != sidecar.source_id
            || disposition.source_ref.scope_id != sidecar.scope_id
        {
            return Err(artifact_contract_error(
                "record-link source cell disposition binding does not match sidecar",
            ));
        }
        normalized_path(&disposition.path, "source_cell_dispositions.path")?;
        normalized_hash(
            &disposition.value_hash,
            "source_cell_dispositions.value_hash",
        )?;
    }
    if sidecar
        .quarantined_records
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(artifact_contract_error(
            "record-link quarantined records must be sorted and unique",
        ));
    }
    for quarantined in &sidecar.quarantined_records {
        if quarantined.source_ref.source_id != sidecar.source_id
            || quarantined.source_ref.scope_id != sidecar.scope_id
        {
            return Err(artifact_contract_error(
                "record-link quarantined record binding does not match sidecar",
            ));
        }
        match quarantined.reason {
            RecordLinkFieldDispositionReason::MissingField => {
                let Some(mapping_id) = &quarantined.missing_assignment_mapping_id else {
                    return Err(artifact_contract_error(
                        "missing-assignment quarantines must name the assignment mapping",
                    ));
                };
                normalized_opaque_ref(
                    mapping_id,
                    "quarantined_records.missing_assignment_mapping_id",
                )?;
                if quarantined.assignment_ref.is_some() {
                    return Err(artifact_contract_error(
                        "missing-assignment quarantines must not include an assignment ref",
                    ));
                }
            }
            RecordLinkFieldDispositionReason::DuplicateRecordId => {
                if quarantined.missing_assignment_mapping_id.is_some() {
                    return Err(artifact_contract_error(
                        "duplicate record quarantines must not name a missing assignment mapping",
                    ));
                }
            }
            RecordLinkFieldDispositionReason::MalformedField
            | RecordLinkFieldDispositionReason::Overflow
            | RecordLinkFieldDispositionReason::IncomparableField => {
                return Err(artifact_contract_error(
                    "record-level quarantines only support missing assignments or duplicate record IDs",
                ));
            }
        }
    }
    Ok(())
}

fn validate_record_link_comparison_view(
    view: &RecordLinkComparisonView,
) -> SourceMappingResult<()> {
    match view {
        RecordLinkComparisonView::Numeric {
            feature_id,
            source_path,
            units,
            scale,
            ..
        } => {
            normalized_opaque_ref(feature_id, "comparison_views.feature_id")?;
            normalized_path(source_path, "comparison_views.source_path")?;
            normalized_non_empty(units, "comparison_views.units")?;
            if *scale > 18 {
                return Err(artifact_contract_error(
                    "record-link numeric comparison scale must be <= 18",
                ));
            }
        }
        RecordLinkComparisonView::Date {
            feature_id,
            source_path,
            value,
            ..
        } => {
            normalized_opaque_ref(feature_id, "comparison_views.feature_id")?;
            normalized_path(source_path, "comparison_views.source_path")?;
            if !is_iso_day_date(value) {
                return Err(artifact_contract_error(
                    "record-link date comparison value must be YYYY-MM-DD",
                ));
            }
        }
        RecordLinkComparisonView::Categorical {
            feature_id,
            source_path,
            value,
            ..
        } => {
            normalized_opaque_ref(feature_id, "comparison_views.feature_id")?;
            normalized_path(source_path, "comparison_views.source_path")?;
            normalized_non_empty(value, "comparison_views.value")?;
        }
    }
    Ok(())
}

fn normalize_record_link_request(
    mut request: RecordLinkInputBuildRequest,
) -> SourceMappingResult<RecordLinkInputBuildRequest> {
    request.source_id = normalized_non_empty(&request.source_id, "source_id")?;
    request.scope_id = normalized_non_empty(&request.scope_id, "scope_id")?;
    request.profile_id = normalized_opaque_ref(&request.profile_id, "profile_id")?;
    request.profile_digest = normalized_hash(&request.profile_digest, "profile_digest")?;
    request.input_digest = normalized_hash(&request.input_digest, "input_digest")?;
    request.source_mapping_digest =
        normalized_hash(&request.source_mapping_digest, "source_mapping_digest")?;
    request.subject_observation_mapping_id = normalized_opaque_ref(
        &request.subject_observation_mapping_id,
        "subject_observation_mapping_id",
    )?;

    request.assignment_mapping_ids = request
        .assignment_mapping_ids
        .into_iter()
        .map(|value| normalized_opaque_ref(&value, "assignment_mapping_ids"))
        .collect::<SourceMappingResult<Vec<_>>>()?;
    request.assignment_mapping_ids.sort();
    if let Some(pair) = request
        .assignment_mapping_ids
        .windows(2)
        .find(|pair| pair[0] == pair[1])
    {
        return Err(artifact_contract_error(format!(
            "record-link request repeats assignment mapping {}",
            pair[0]
        )));
    }

    request.comparison_mappings = request
        .comparison_mappings
        .into_iter()
        .map(normalize_record_link_comparison_mapping)
        .collect::<SourceMappingResult<Vec<_>>>()?;
    request.comparison_mappings.sort_by(|left, right| {
        left.feature_id
            .cmp(&right.feature_id)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.path.cmp(&right.path))
    });
    if let Some(pair) = request.comparison_mappings.windows(2).find(|pair| {
        pair[0].feature_id == pair[1].feature_id
            && pair[0].source == pair[1].source
            && pair[0].path == pair[1].path
    }) {
        return Err(artifact_contract_error(format!(
            "record-link request repeats comparison mapping {} at {}",
            pair[0].feature_id, pair[0].path
        )));
    }
    Ok(request)
}

fn normalize_record_link_comparison_mapping(
    mut mapping: RecordLinkComparisonMapping,
) -> SourceMappingResult<RecordLinkComparisonMapping> {
    mapping.feature_id = normalized_opaque_ref(&mapping.feature_id, "comparison.feature_id")?;
    mapping.path = normalized_path(&mapping.path, "comparison.path")?;
    match mapping.value_kind {
        RecordLinkComparisonKind::Numeric => {
            let units = mapping.units.as_deref().ok_or_else(|| {
                artifact_contract_error("numeric comparison fields must declare units")
            })?;
            mapping.units = Some(normalized_non_empty(units, "comparison.units")?);
            let scale = mapping.scale.ok_or_else(|| {
                artifact_contract_error("numeric comparison fields must declare scale")
            })?;
            if scale > 18 {
                return Err(artifact_contract_error(
                    "numeric comparison scale must be <= 18",
                ));
            }
        }
        RecordLinkComparisonKind::Date | RecordLinkComparisonKind::Categorical => {
            if mapping.units.is_some() {
                return Err(artifact_contract_error(
                    "non-numeric comparison fields must not declare units",
                ));
            }
            if mapping.scale.is_some() {
                return Err(artifact_contract_error(
                    "non-numeric comparison fields must not declare scale",
                ));
            }
        }
    }
    Ok(mapping)
}

fn validate_record_link_bundle_binding(
    bundle: &MappedSourceArtifacts,
    request: &RecordLinkInputBuildRequest,
) -> SourceMappingResult<()> {
    if bundle.profile_id != request.profile_id {
        return Err(compatibility_policy_error(format!(
            "record-link input profile mismatch: expected {}, got {}",
            request.profile_id, bundle.profile_id
        )));
    }
    if bundle.mapping_digest != request.source_mapping_digest {
        return Err(compatibility_policy_error(format!(
            "record-link input source-mapping digest mismatch: expected {}, got {}",
            request.source_mapping_digest, bundle.mapping_digest
        )));
    }
    Ok(())
}

fn build_record_link_record(
    request: &RecordLinkInputBuildRequest,
    source_ref: &RecordLinkSourceRef,
    observation_ref: &RecordLinkObservationRef,
    observation: &ObservationArtifact,
    assignment: Option<&AssignmentArtifact>,
) -> SourceMappingResult<RecordLinkInputRecord> {
    let assignment_ref = assignment.map(|assignment| RecordLinkAssignmentRef {
        assignment_id: assignment.assignment_id.clone(),
        mapping_id: assignment.mapping_id.clone(),
        role_id: assignment.role_id.clone(),
        assignee_type_id: assignment.assignee_type_id.clone(),
    });
    let mut comparison_views = Vec::new();
    let mut quarantined_fields = Vec::new();
    for mapping in &request.comparison_mappings {
        if let Some(view) = render_record_link_comparison_view(
            mapping,
            observation,
            assignment,
            &mut quarantined_fields,
        )? {
            comparison_views.push(view);
        }
    }
    comparison_views.sort();
    comparison_views.dedup();
    quarantined_fields.sort();
    quarantined_fields.dedup();

    let raw_field_hashes = record_link_raw_field_hashes(observation, assignment)?;
    let mut record = RecordLinkInputRecord {
        record_id: String::new(),
        source_ref: source_ref.clone(),
        subject_observation_ref: observation_ref.clone(),
        assignment_ref,
        comparison_views,
        quarantined_fields,
        provenance: RecordLinkInputProvenance {
            profile_digest: request.profile_digest.clone(),
            input_digest: request.input_digest.clone(),
            source_mapping_digest: request.source_mapping_digest.clone(),
            source_locator: source_ref.source_locator.clone(),
            raw_field_hashes,
        },
    };
    record.record_id = derive_record_link_record_id(&record)?;
    Ok(record)
}

fn record_link_source_cell_dispositions(
    source_ref: &RecordLinkSourceRef,
    preserved_cells: &[PreservedCell],
    quarantined_cells: &[QuarantinedCell],
) -> SourceMappingResult<Vec<RecordLinkSourceCellDisposition>> {
    let mut dispositions = Vec::with_capacity(preserved_cells.len() + quarantined_cells.len());
    for cell in preserved_cells {
        dispositions.push(RecordLinkSourceCellDisposition {
            disposition: RecordLinkSourceCellDispositionKind::Preserved,
            reason: cell.reason,
            source_ref: source_ref.clone(),
            path: normalized_path(&cell.path, "source_cell_dispositions.path")?,
            value_hash: hash_json_value(&cell.value)?,
        });
    }
    for cell in quarantined_cells {
        dispositions.push(RecordLinkSourceCellDisposition {
            disposition: RecordLinkSourceCellDispositionKind::Quarantined,
            reason: cell.reason,
            source_ref: source_ref.clone(),
            path: normalized_path(&cell.path, "source_cell_dispositions.path")?,
            value_hash: hash_json_value(&cell.value)?,
        });
    }
    dispositions.sort();
    Ok(dispositions)
}

fn insert_record_link_record(
    record: RecordLinkInputRecord,
    duplicate_policy: CapturePolicy,
    seen_record_bytes: &mut BTreeMap<String, Vec<u8>>,
    records: &mut Vec<RecordLinkInputRecord>,
    quarantined_records: &mut Vec<RecordLinkQuarantinedRecord>,
) -> SourceMappingResult<()> {
    let bytes = serde_json::to_vec(&record).map_err(|error| {
        artifact_contract_error(format!("failed to serialize record-link record: {error}"))
    })?;
    if seen_record_bytes
        .insert(record.record_id.clone(), bytes)
        .is_some()
    {
        match duplicate_policy {
            CapturePolicy::Reject => {
                return Err(policy_constraint_error(format!(
                    "duplicate record-link record_id {} violates sidecar policy",
                    record.record_id
                )));
            }
            CapturePolicy::Preserve | CapturePolicy::Quarantine => {
                quarantined_records.push(RecordLinkQuarantinedRecord {
                    reason: RecordLinkFieldDispositionReason::DuplicateRecordId,
                    record_id: record.record_id,
                    source_ref: record.source_ref,
                    assignment_ref: record.assignment_ref,
                    missing_assignment_mapping_id: None,
                });
                return Ok(());
            }
        }
    }
    records.push(record);
    Ok(())
}

fn apply_record_link_missing_assignment_policy(
    policy: CapturePolicy,
    mapping_id: &str,
    source_ref: &RecordLinkSourceRef,
    quarantined_records: &mut Vec<RecordLinkQuarantinedRecord>,
) -> SourceMappingResult<()> {
    match policy {
        CapturePolicy::Reject => Err(missing_field_error(format!(
            "record-link input row {} is missing requested assignment mapping {mapping_id}",
            source_ref.source_locator.locator
        ))),
        CapturePolicy::Preserve | CapturePolicy::Quarantine => {
            quarantined_records.push(RecordLinkQuarantinedRecord {
                reason: RecordLinkFieldDispositionReason::MissingField,
                record_id: derive_record_link_missing_assignment_id(source_ref, mapping_id)?,
                source_ref: source_ref.clone(),
                assignment_ref: None,
                missing_assignment_mapping_id: Some(mapping_id.to_string()),
            });
            Ok(())
        }
    }
}

fn render_record_link_comparison_view(
    mapping: &RecordLinkComparisonMapping,
    observation: &ObservationArtifact,
    assignment: Option<&AssignmentArtifact>,
    quarantined_fields: &mut Vec<RecordLinkQuarantinedField>,
) -> SourceMappingResult<Option<RecordLinkComparisonView>> {
    let value = record_link_source_value(mapping.source, &mapping.path, observation, assignment);
    let Some(value) = value else {
        return apply_record_link_field_policy(
            mapping.policies.missing,
            RecordLinkFieldDispositionReason::MissingField,
            mapping,
            None,
            quarantined_fields,
        )
        .map(|()| None);
    };
    if value.is_array() || value.is_object() {
        return apply_record_link_field_policy(
            mapping.policies.incomparable,
            RecordLinkFieldDispositionReason::IncomparableField,
            mapping,
            Some(value),
            quarantined_fields,
        )
        .map(|()| None);
    }
    match mapping.value_kind {
        RecordLinkComparisonKind::Numeric => match render_numeric_comparison(mapping, value) {
            Ok(view) => Ok(view),
            Err(reason) => {
                let policy = match reason {
                    RecordLinkFieldDispositionReason::Overflow => mapping.policies.overflow,
                    RecordLinkFieldDispositionReason::IncomparableField => {
                        mapping.policies.incomparable
                    }
                    _ => mapping.policies.malformed,
                };
                apply_record_link_field_policy(
                    policy,
                    reason,
                    mapping,
                    Some(value),
                    quarantined_fields,
                )?;
                Ok(None)
            }
        },
        RecordLinkComparisonKind::Date => match render_date_comparison(mapping, value) {
            Ok(view) => Ok(view),
            Err(reason) => {
                apply_record_link_field_policy(
                    mapping.policies.malformed,
                    reason,
                    mapping,
                    Some(value),
                    quarantined_fields,
                )?;
                Ok(None)
            }
        },
        RecordLinkComparisonKind::Categorical => {
            match render_categorical_comparison(mapping, value) {
                Ok(view) => Ok(view),
                Err(reason) => {
                    apply_record_link_field_policy(
                        mapping.policies.malformed,
                        reason,
                        mapping,
                        Some(value),
                        quarantined_fields,
                    )?;
                    Ok(None)
                }
            }
        }
    }
}

fn record_link_source_value<'a>(
    source: RecordLinkComparisonSource,
    path: &str,
    observation: &'a ObservationArtifact,
    assignment: Option<&'a AssignmentArtifact>,
) -> Option<&'a Value> {
    match source {
        RecordLinkComparisonSource::AssignmentContext => assignment?.context.get(path),
        RecordLinkComparisonSource::ObservationContext => observation.context.get(path),
    }
}

fn render_numeric_comparison(
    mapping: &RecordLinkComparisonMapping,
    value: &Value,
) -> Result<Option<RecordLinkComparisonView>, RecordLinkFieldDispositionReason> {
    let raw =
        scalar_record_link_string(value).ok_or(RecordLinkFieldDispositionReason::MalformedField)?;
    let scale = mapping
        .scale
        .ok_or(RecordLinkFieldDispositionReason::MalformedField)?;
    let scaled_value = parse_scaled_decimal(&raw, scale)?;
    Ok(Some(RecordLinkComparisonView::Numeric {
        feature_id: mapping.feature_id.clone(),
        source: mapping.source,
        source_path: mapping.path.clone(),
        units: mapping.units.clone().unwrap_or_default(),
        scaled_value,
        scale,
    }))
}

fn render_date_comparison(
    mapping: &RecordLinkComparisonMapping,
    value: &Value,
) -> Result<Option<RecordLinkComparisonView>, RecordLinkFieldDispositionReason> {
    let raw =
        scalar_record_link_string(value).ok_or(RecordLinkFieldDispositionReason::MalformedField)?;
    if !is_iso_day_date(&raw) {
        return Err(RecordLinkFieldDispositionReason::MalformedField);
    }
    Ok(Some(RecordLinkComparisonView::Date {
        feature_id: mapping.feature_id.clone(),
        source: mapping.source,
        source_path: mapping.path.clone(),
        value: raw,
    }))
}

fn render_categorical_comparison(
    mapping: &RecordLinkComparisonMapping,
    value: &Value,
) -> Result<Option<RecordLinkComparisonView>, RecordLinkFieldDispositionReason> {
    let raw =
        scalar_record_link_string(value).ok_or(RecordLinkFieldDispositionReason::MalformedField)?;
    Ok(Some(RecordLinkComparisonView::Categorical {
        feature_id: mapping.feature_id.clone(),
        source: mapping.source,
        source_path: mapping.path.clone(),
        value: raw,
    }))
}

fn apply_record_link_field_policy(
    policy: CapturePolicy,
    reason: RecordLinkFieldDispositionReason,
    mapping: &RecordLinkComparisonMapping,
    value: Option<&Value>,
    quarantined_fields: &mut Vec<RecordLinkQuarantinedField>,
) -> SourceMappingResult<()> {
    match policy {
        CapturePolicy::Reject => Err(policy_constraint_error(format!(
            "{reason:?} at {} violates record-link comparison policy",
            mapping.path
        ))),
        CapturePolicy::Preserve | CapturePolicy::Quarantine => {
            quarantined_fields.push(RecordLinkQuarantinedField {
                reason,
                feature_id: mapping.feature_id.clone(),
                source: mapping.source,
                source_path: mapping.path.clone(),
                value_hash: value.map(hash_json_value).transpose()?,
            });
            Ok(())
        }
    }
}

fn scalar_record_link_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn parse_scaled_decimal(raw: &str, scale: u32) -> Result<i64, RecordLinkFieldDispositionReason> {
    if raw.contains(['e', 'E']) {
        return Err(RecordLinkFieldDispositionReason::MalformedField);
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RecordLinkFieldDispositionReason::MalformedField);
    }
    let (sign, unsigned) = match trimmed.as_bytes()[0] {
        b'-' => (-1i128, &trimmed[1..]),
        b'+' => (1i128, &trimmed[1..]),
        _ => (1i128, trimmed),
    };
    let mut parts = unsigned.split('.');
    let int_part = parts
        .next()
        .ok_or(RecordLinkFieldDispositionReason::MalformedField)?;
    let frac_part = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || int_part.is_empty()
        || !int_part.chars().all(|ch| ch.is_ascii_digit())
        || !frac_part.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(RecordLinkFieldDispositionReason::MalformedField);
    }
    let scale_usize =
        usize::try_from(scale).map_err(|_| RecordLinkFieldDispositionReason::Overflow)?;
    if frac_part.len() > scale_usize {
        return Err(RecordLinkFieldDispositionReason::IncomparableField);
    }
    let mut digits = String::with_capacity(int_part.len() + scale_usize);
    digits.push_str(int_part);
    digits.push_str(frac_part);
    for _ in 0..scale_usize.saturating_sub(frac_part.len()) {
        digits.push('0');
    }
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| RecordLinkFieldDispositionReason::Overflow)?;
    let value = magnitude
        .checked_mul(sign)
        .ok_or(RecordLinkFieldDispositionReason::Overflow)?;
    if value < i128::from(i64::MIN) || value > i128::from(i64::MAX) {
        return Err(RecordLinkFieldDispositionReason::Overflow);
    }
    Ok(value as i64)
}

fn is_iso_day_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let year = parse_fixed_u32(&value[0..4]);
    let month = parse_fixed_u32(&value[5..7]);
    let day = parse_fixed_u32(&value[8..10]);
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) {
        return false;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn parse_fixed_u32(value: &str) -> Option<u32> {
    value
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
}

fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

fn record_link_raw_field_hashes(
    observation: &ObservationArtifact,
    assignment: Option<&AssignmentArtifact>,
) -> SourceMappingResult<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for (path, value) in &observation.provenance.raw_fields {
        hashes.insert(format!("observation:{path}"), hash_json_value(value)?);
    }
    if let Some(assignment) = assignment {
        for (path, value) in &assignment.provenance.raw_fields {
            hashes.insert(format!("assignment:{path}"), hash_json_value(value)?);
        }
    }
    Ok(hashes)
}

fn derive_record_link_record_id(record: &RecordLinkInputRecord) -> SourceMappingResult<String> {
    let seed = serde_json::json!({
        "source_ref": record.source_ref,
        "subject_observation_ref": record.subject_observation_ref,
        "assignment_ref": record.assignment_ref,
    });
    stable_artifact_id("record_link", &seed)
}

fn derive_record_link_missing_assignment_id(
    source_ref: &RecordLinkSourceRef,
    mapping_id: &str,
) -> SourceMappingResult<String> {
    let seed = serde_json::json!({
        "source_ref": source_ref,
        "missing_assignment_mapping_id": mapping_id,
    });
    stable_artifact_id("record_link_quarantine", &seed)
}

fn record_link_input_hash(sidecar: &RecordLinkInputSidecar) -> SourceMappingResult<String> {
    let mut hashable = sidecar.clone();
    hashable.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize record-link input sidecar for hashing: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn record_link_summary(sidecar: &RecordLinkInputSidecar) -> BTreeMap<String, u64> {
    let record_count = u64::try_from(sidecar.records.len()).expect("record count fits u64");
    let comparison_view_count = sidecar
        .records
        .iter()
        .map(|record| {
            u64::try_from(record.comparison_views.len()).expect("comparison view count fits u64")
        })
        .sum();
    let quarantined_field_count = sidecar
        .records
        .iter()
        .map(|record| u64::try_from(record.quarantined_fields.len()).expect("field count fits u64"))
        .sum();
    BTreeMap::from([
        ("record_count".to_string(), record_count),
        ("comparison_view_count".to_string(), comparison_view_count),
        (
            "quarantined_field_count".to_string(),
            quarantined_field_count,
        ),
        (
            "quarantined_record_count".to_string(),
            u64::try_from(sidecar.quarantined_records.len()).expect("record count fits u64"),
        ),
        (
            "preserved_source_cell_count".to_string(),
            u64::try_from(
                sidecar
                    .source_cell_dispositions
                    .iter()
                    .filter(|cell| {
                        cell.disposition == RecordLinkSourceCellDispositionKind::Preserved
                    })
                    .count(),
            )
            .expect("source cell count fits u64"),
        ),
        (
            "quarantined_source_cell_count".to_string(),
            u64::try_from(
                sidecar
                    .source_cell_dispositions
                    .iter()
                    .filter(|cell| {
                        cell.disposition == RecordLinkSourceCellDispositionKind::Quarantined
                    })
                    .count(),
            )
            .expect("source cell count fits u64"),
        ),
    ])
}

fn hash_json_value(value: &Value) -> SourceMappingResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize source field for hashing: {error}"
        ))
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

struct MappingExecution<'a> {
    payload: &'a serde_json::Map<String, Value>,
    profile: &'a SourceMappingProfile,
    mapping_digest: &'a str,
    object_id: &'a str,
    source_locator: &'a SourceLocator,
    temporal: &'a TemporalContext,
    preserved_cells: &'a mut Vec<PreservedCell>,
    quarantined_cells: &'a mut Vec<QuarantinedCell>,
}

fn render_observation(
    execution: &mut MappingExecution<'_>,
    mapping: &ObservationMapping,
) -> SourceMappingResult<Option<ObservationArtifact>> {
    let policies = &execution.profile.policies;
    let Some(surface_value) = required_scalar(
        execution.payload,
        &mapping.surface_path,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?
    else {
        return Ok(None);
    };

    let anchors = render_anchors(
        execution.payload,
        &mapping.anchor_mappings,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?;
    let context = collect_context(execution.payload, &mapping.context_paths);
    let provenance = build_provenance(
        execution.payload,
        &execution.profile.profile_id,
        execution.mapping_digest,
        execution.source_locator,
        observation_paths(execution.profile, mapping),
    );

    let observation_id = stable_artifact_id(
        "observation",
        &serde_json::json!({
            "mapping_id": mapping.mapping_id,
            "object_id": execution.object_id,
            "surface": surface_value,
            "source_locator": execution.source_locator,
            "temporal": execution.temporal,
            "mapping_digest": execution.mapping_digest,
        }),
    )?;

    Ok(Some(ObservationArtifact {
        observation_id,
        mapping_id: mapping.mapping_id.clone(),
        object_id: execution.object_id.to_string(),
        subject_type_id: mapping.subject_type_id.clone(),
        surface: MappedCell {
            path: mapping.surface_path.clone(),
            value: surface_value,
        },
        anchors,
        context,
        temporal: execution.temporal.clone(),
        provenance,
    }))
}

fn render_assignment(
    execution: &mut MappingExecution<'_>,
    mapping: &AssignmentMapping,
) -> SourceMappingResult<Option<AssignmentArtifact>> {
    let policies = &execution.profile.policies;
    let role_id = resolve_role_binding(
        execution.payload,
        &mapping.role_binding,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?;
    let assignee_surface = required_scalar(
        execution.payload,
        &mapping.assignee_surface_path,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?;
    let (Some(role_id), Some(assignee_surface)) = (role_id, assignee_surface) else {
        return Ok(None);
    };

    let assignee_anchors = render_anchors(
        execution.payload,
        &mapping.assignee_anchor_mappings,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?;
    let context = collect_context(execution.payload, &mapping.context_paths);
    let provenance = build_provenance(
        execution.payload,
        &execution.profile.profile_id,
        execution.mapping_digest,
        execution.source_locator,
        assignment_paths(execution.profile, mapping),
    );
    let assignment_id = stable_artifact_id(
        "assignment",
        &serde_json::json!({
            "mapping_id": mapping.mapping_id,
            "object_id": execution.object_id,
            "role_id": role_id,
            "assignee_surface": assignee_surface,
            "source_locator": execution.source_locator,
            "temporal": execution.temporal,
            "mapping_digest": execution.mapping_digest,
        }),
    )?;

    Ok(Some(AssignmentArtifact {
        assignment_id,
        mapping_id: mapping.mapping_id.clone(),
        subject_object_id: execution.object_id.to_string(),
        subject_type_id: mapping.subject_type_id.clone(),
        role_id,
        assignee_type_id: mapping.assignee_type_id.clone(),
        assignee_surface: MappedCell {
            path: mapping.assignee_surface_path.clone(),
            value: assignee_surface,
        },
        assignee_anchors,
        context,
        temporal: execution.temporal.clone(),
        provenance,
    }))
}

fn render_relationship(
    execution: &mut MappingExecution<'_>,
    mapping: &RelationshipMapping,
) -> SourceMappingResult<Option<RelationshipArtifact>> {
    let policies = &execution.profile.policies;
    let Some(object_surface) = required_scalar(
        execution.payload,
        &mapping.object_surface_path,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?
    else {
        return Ok(None);
    };

    let object_anchors = render_anchors(
        execution.payload,
        &mapping.object_anchor_mappings,
        policies,
        execution.preserved_cells,
        execution.quarantined_cells,
    )?;
    let context = collect_context(execution.payload, &mapping.context_paths);
    let provenance = build_provenance(
        execution.payload,
        &execution.profile.profile_id,
        execution.mapping_digest,
        execution.source_locator,
        relationship_paths(execution.profile, mapping),
    );
    let relationship_id = stable_artifact_id(
        "relationship",
        &serde_json::json!({
            "mapping_id": mapping.mapping_id,
            "object_id": execution.object_id,
            "relation_type_id": mapping.relation_type_id,
            "object_surface": object_surface,
            "source_locator": execution.source_locator,
            "temporal": execution.temporal,
            "mapping_digest": execution.mapping_digest,
        }),
    )?;

    Ok(Some(RelationshipArtifact {
        relationship_id,
        mapping_id: mapping.mapping_id.clone(),
        subject_object_id: execution.object_id.to_string(),
        subject_type_id: mapping.subject_type_id.clone(),
        relation_type_id: mapping.relation_type_id.clone(),
        object_type_id: mapping.object_type_id.clone(),
        object_surface: MappedCell {
            path: mapping.object_surface_path.clone(),
            value: object_surface,
        },
        object_anchors,
        context,
        temporal: execution.temporal.clone(),
        provenance,
    }))
}

fn render_anchors(
    payload: &serde_json::Map<String, Value>,
    mappings: &[AnchorMapping],
    policies: &SourceMappingPolicies,
    preserved_cells: &mut Vec<PreservedCell>,
    quarantined_cells: &mut Vec<QuarantinedCell>,
) -> SourceMappingResult<Vec<NamespacedAnchor>> {
    let mut anchors = Vec::new();
    for mapping in mappings {
        if let Some(value) = optional_scalar(
            payload,
            Some(mapping.path.as_str()),
            policies,
            preserved_cells,
            quarantined_cells,
        )? {
            anchors.push(NamespacedAnchor {
                namespace: mapping.namespace.clone(),
                value,
                path: mapping.path.clone(),
            });
        }
    }
    anchors.sort();
    anchors.dedup();
    Ok(anchors)
}

fn resolve_role_binding(
    payload: &serde_json::Map<String, Value>,
    binding: &RoleBinding,
    policies: &SourceMappingPolicies,
    preserved_cells: &mut Vec<PreservedCell>,
    quarantined_cells: &mut Vec<QuarantinedCell>,
) -> SourceMappingResult<Option<String>> {
    match binding {
        RoleBinding::Literal { role_id } => Ok(Some(role_id.clone())),
        RoleBinding::Field {
            path,
            namespace,
            allowed_values,
            allow_verbatim_values,
        } => {
            let Some(raw_value) =
                required_scalar(payload, path, policies, preserved_cells, quarantined_cells)?
            else {
                return Ok(None);
            };
            if let Some(role_id) = allowed_values.get(raw_value.as_str()) {
                return Ok(Some(role_id.clone()));
            }
            if *allow_verbatim_values {
                let token = normalized_fragment_token(&raw_value, path)?;
                return Ok(Some(format!("{namespace}:{token}")));
            }

            apply_capture_policy(
                policies.unknown_role,
                CellDispositionReason::UnknownRole,
                path,
                Value::String(raw_value),
                preserved_cells,
                quarantined_cells,
            )?;
            Ok(None)
        }
    }
}

fn capture_unknown_fields(
    payload: &serde_json::Map<String, Value>,
    profile: &SourceMappingProfile,
    bundle: &mut MappedSourceArtifacts,
) -> SourceMappingResult<()> {
    let configured = configured_paths(profile);
    let mut leaves = BTreeMap::new();
    for (key, value) in payload {
        collect_leaf_values(key, value, &mut leaves);
    }

    for (path, value) in leaves {
        if configured.contains(path.as_str()) {
            continue;
        }
        apply_capture_policy(
            profile.policies.unknown_field,
            CellDispositionReason::UnknownField,
            &path,
            value,
            &mut bundle.preserved_cells,
            &mut bundle.quarantined_cells,
        )?;
    }
    Ok(())
}

fn required_scalar(
    payload: &serde_json::Map<String, Value>,
    path: &str,
    policies: &SourceMappingPolicies,
    preserved_cells: &mut Vec<PreservedCell>,
    quarantined_cells: &mut Vec<QuarantinedCell>,
) -> SourceMappingResult<Option<String>> {
    match lookup_path(payload, path) {
        None => {
            if matches!(policies.missing_required, CapturePolicy::Reject) {
                return Err(missing_field_error(format!(
                    "required source field {path} is missing"
                )));
            }
            apply_capture_policy(
                policies.missing_required,
                CellDispositionReason::MissingRequired,
                path,
                Value::Null,
                preserved_cells,
                quarantined_cells,
            )?;
            Ok(None)
        }
        Some(Value::Null) => {
            if matches!(policies.null_value, CapturePolicy::Reject) {
                return Err(policy_constraint_error(format!(
                    "required source field {path} is null"
                )));
            }
            apply_capture_policy(
                policies.null_value,
                CellDispositionReason::NullValue,
                path,
                Value::Null,
                preserved_cells,
                quarantined_cells,
            )?;
            Ok(None)
        }
        Some(value) if value.is_array() || value.is_object() => {
            if matches!(policies.ambiguous_cell, CapturePolicy::Reject) {
                return Err(policy_constraint_error(format!(
                    "required source field {path} is not a scalar"
                )));
            }
            apply_capture_policy(
                policies.ambiguous_cell,
                CellDispositionReason::AmbiguousCell,
                path,
                value.clone(),
                preserved_cells,
                quarantined_cells,
            )?;
            Ok(None)
        }
        Some(value) => Ok(Some(scalar_to_string(value, path)?)),
    }
}

fn optional_scalar(
    payload: &serde_json::Map<String, Value>,
    path: Option<&str>,
    policies: &SourceMappingPolicies,
    preserved_cells: &mut Vec<PreservedCell>,
    quarantined_cells: &mut Vec<QuarantinedCell>,
) -> SourceMappingResult<Option<String>> {
    let Some(path) = path else {
        return Ok(None);
    };
    match lookup_path(payload, path) {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_array() || value.is_object() => {
            if matches!(policies.ambiguous_cell, CapturePolicy::Reject) {
                return Err(policy_constraint_error(format!(
                    "optional source field {path} is not a scalar"
                )));
            }
            apply_capture_policy(
                policies.ambiguous_cell,
                CellDispositionReason::AmbiguousCell,
                path,
                value.clone(),
                preserved_cells,
                quarantined_cells,
            )?;
            Ok(None)
        }
        Some(value) => Ok(Some(scalar_to_string(value, path)?)),
    }
}

fn build_provenance(
    payload: &serde_json::Map<String, Value>,
    profile_id: &str,
    mapping_digest: &str,
    source_locator: &SourceLocator,
    paths: BTreeSet<String>,
) -> MappingProvenance {
    let raw_fields = paths
        .into_iter()
        .filter_map(|path| {
            lookup_path(payload, &path)
                .cloned()
                .map(|value| (path, value))
        })
        .collect::<BTreeMap<_, _>>();

    MappingProvenance {
        profile_id: profile_id.to_string(),
        mapping_digest: mapping_digest.to_string(),
        source_locator: source_locator.clone(),
        raw_fields,
    }
}

fn collect_context(
    payload: &serde_json::Map<String, Value>,
    context_paths: &[String],
) -> BTreeMap<String, Value> {
    context_paths
        .iter()
        .filter_map(|path| {
            lookup_path(payload, path)
                .cloned()
                .map(|value| (path.clone(), value))
        })
        .collect()
}

fn lookup_path<'a>(payload: &'a serde_json::Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut current = payload.get(path.split('.').next()?)?;
    for segment in path.split('.').skip(1) {
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}

fn collect_leaf_values(path: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (segment, child) in map {
                let child_path = format!("{path}.{segment}");
                collect_leaf_values(&child_path, child, out);
            }
        }
        _ => {
            out.insert(path.to_string(), value.clone());
        }
    }
}

fn configured_paths(profile: &SourceMappingProfile) -> BTreeSet<&str> {
    let mut paths = BTreeSet::new();
    paths.insert(profile.object_id_path.as_str());
    paths.insert(profile.locator_path.as_str());
    if let Some(path) = profile.fragment_path.as_deref() {
        paths.insert(path);
    }
    if let Some(path) = profile.as_of_path.as_deref() {
        paths.insert(path);
    }
    if let Some(path) = profile.valid_from_path.as_deref() {
        paths.insert(path);
    }
    if let Some(path) = profile.valid_to_path.as_deref() {
        paths.insert(path);
    }
    for mapping in &profile.observations {
        paths.insert(mapping.surface_path.as_str());
        for anchor in &mapping.anchor_mappings {
            paths.insert(anchor.path.as_str());
        }
        for path in &mapping.context_paths {
            paths.insert(path.as_str());
        }
    }
    for mapping in &profile.assignments {
        paths.insert(mapping.assignee_surface_path.as_str());
        if let RoleBinding::Field { path, .. } = &mapping.role_binding {
            paths.insert(path.as_str());
        }
        for anchor in &mapping.assignee_anchor_mappings {
            paths.insert(anchor.path.as_str());
        }
        for path in &mapping.context_paths {
            paths.insert(path.as_str());
        }
    }
    for mapping in &profile.relationships {
        paths.insert(mapping.object_surface_path.as_str());
        for anchor in &mapping.object_anchor_mappings {
            paths.insert(anchor.path.as_str());
        }
        for path in &mapping.context_paths {
            paths.insert(path.as_str());
        }
    }
    paths
}

fn observation_paths(
    profile: &SourceMappingProfile,
    mapping: &ObservationMapping,
) -> BTreeSet<String> {
    let mut paths = profile_base_paths(profile);
    paths.insert(mapping.surface_path.clone());
    paths.extend(
        mapping
            .anchor_mappings
            .iter()
            .map(|anchor| anchor.path.clone()),
    );
    paths.extend(mapping.context_paths.iter().cloned());
    paths
}

fn assignment_paths(
    profile: &SourceMappingProfile,
    mapping: &AssignmentMapping,
) -> BTreeSet<String> {
    let mut paths = profile_base_paths(profile);
    paths.insert(mapping.assignee_surface_path.clone());
    if let RoleBinding::Field { path, .. } = &mapping.role_binding {
        paths.insert(path.clone());
    }
    paths.extend(
        mapping
            .assignee_anchor_mappings
            .iter()
            .map(|anchor| anchor.path.clone()),
    );
    paths.extend(mapping.context_paths.iter().cloned());
    paths
}

fn relationship_paths(
    profile: &SourceMappingProfile,
    mapping: &RelationshipMapping,
) -> BTreeSet<String> {
    let mut paths = profile_base_paths(profile);
    paths.insert(mapping.object_surface_path.clone());
    paths.extend(
        mapping
            .object_anchor_mappings
            .iter()
            .map(|anchor| anchor.path.clone()),
    );
    paths.extend(mapping.context_paths.iter().cloned());
    paths
}

fn profile_base_paths(profile: &SourceMappingProfile) -> BTreeSet<String> {
    let mut paths = BTreeSet::from([profile.object_id_path.clone(), profile.locator_path.clone()]);
    if let Some(path) = &profile.fragment_path {
        paths.insert(path.clone());
    }
    if let Some(path) = &profile.as_of_path {
        paths.insert(path.clone());
    }
    if let Some(path) = &profile.valid_from_path {
        paths.insert(path.clone());
    }
    if let Some(path) = &profile.valid_to_path {
        paths.insert(path.clone());
    }
    paths
}

fn stable_artifact_id(kind: &str, seed: &Value) -> SourceMappingResult<String> {
    let bytes = serde_json::to_vec(seed).map_err(|error| {
        artifact_contract_error(format!("failed to serialize {kind} identity seed: {error}"))
    })?;
    Ok(format!("{kind}:blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn normalize_profile(
    mut profile: SourceMappingProfile,
    known_docs: &BTreeSet<String>,
) -> SourceMappingResult<SourceMappingProfile> {
    profile.profile_id = normalized_opaque_ref(&profile.profile_id, "profile_id")?;
    profile.source_system = normalized_non_empty(&profile.source_system, "source_system")?;
    profile.object_id_path = normalized_path(&profile.object_id_path, "object_id_path")?;
    profile.locator_path = normalized_path(&profile.locator_path, "locator_path")?;
    profile.fragment_path = normalize_optional_path(profile.fragment_path, "fragment_path")?;
    profile.as_of_path = normalize_optional_path(profile.as_of_path, "as_of_path")?;
    profile.valid_from_path = normalize_optional_path(profile.valid_from_path, "valid_from_path")?;
    profile.valid_to_path = normalize_optional_path(profile.valid_to_path, "valid_to_path")?;

    profile.source_formats.sort();
    profile.source_formats.dedup();
    if profile.source_formats.is_empty() {
        return Err(artifact_contract_error(format!(
            "profile {} must declare at least one source format",
            profile.profile_id
        )));
    }

    profile.documentation_refs =
        normalize_documentation_refs(profile.documentation_refs, known_docs, "documentation_refs")?;

    profile.observations = profile
        .observations
        .into_iter()
        .map(normalize_observation_mapping)
        .collect::<SourceMappingResult<Vec<_>>>()?;
    profile
        .observations
        .sort_by(|left, right| left.mapping_id.cmp(&right.mapping_id));

    profile.assignments = profile
        .assignments
        .into_iter()
        .map(normalize_assignment_mapping)
        .collect::<SourceMappingResult<Vec<_>>>()?;
    profile
        .assignments
        .sort_by(|left, right| left.mapping_id.cmp(&right.mapping_id));

    profile.relationships = profile
        .relationships
        .into_iter()
        .map(normalize_relationship_mapping)
        .collect::<SourceMappingResult<Vec<_>>>()?;
    profile
        .relationships
        .sort_by(|left, right| left.mapping_id.cmp(&right.mapping_id));

    if profile.observations.is_empty()
        && profile.assignments.is_empty()
        && profile.relationships.is_empty()
    {
        return Err(artifact_contract_error(format!(
            "profile {} must emit at least one artifact kind",
            profile.profile_id
        )));
    }

    Ok(profile)
}

fn normalize_observation_mapping(
    mut mapping: ObservationMapping,
) -> SourceMappingResult<ObservationMapping> {
    mapping.mapping_id = normalized_opaque_ref(&mapping.mapping_id, "mapping_id")?;
    mapping.subject_type_id = normalized_opaque_ref(&mapping.subject_type_id, "subject_type_id")?;
    mapping.surface_path = normalized_path(&mapping.surface_path, "surface_path")?;
    mapping.anchor_mappings = normalize_anchor_mappings(mapping.anchor_mappings)?;
    mapping.context_paths = normalize_paths(mapping.context_paths, "context_paths")?;
    Ok(mapping)
}

fn normalize_assignment_mapping(
    mut mapping: AssignmentMapping,
) -> SourceMappingResult<AssignmentMapping> {
    mapping.mapping_id = normalized_opaque_ref(&mapping.mapping_id, "mapping_id")?;
    mapping.subject_type_id = normalized_opaque_ref(&mapping.subject_type_id, "subject_type_id")?;
    mapping.assignee_type_id =
        normalized_opaque_ref(&mapping.assignee_type_id, "assignee_type_id")?;
    mapping.assignee_surface_path =
        normalized_path(&mapping.assignee_surface_path, "assignee_surface_path")?;
    mapping.assignee_anchor_mappings = normalize_anchor_mappings(mapping.assignee_anchor_mappings)?;
    mapping.context_paths = normalize_paths(mapping.context_paths, "context_paths")?;
    mapping.role_binding = normalize_role_binding(mapping.role_binding)?;
    Ok(mapping)
}

fn normalize_relationship_mapping(
    mut mapping: RelationshipMapping,
) -> SourceMappingResult<RelationshipMapping> {
    mapping.mapping_id = normalized_opaque_ref(&mapping.mapping_id, "mapping_id")?;
    mapping.subject_type_id = normalized_opaque_ref(&mapping.subject_type_id, "subject_type_id")?;
    mapping.relation_type_id =
        normalized_opaque_ref(&mapping.relation_type_id, "relation_type_id")?;
    mapping.object_type_id = normalized_opaque_ref(&mapping.object_type_id, "object_type_id")?;
    mapping.object_surface_path =
        normalized_path(&mapping.object_surface_path, "object_surface_path")?;
    mapping.object_anchor_mappings = normalize_anchor_mappings(mapping.object_anchor_mappings)?;
    mapping.context_paths = normalize_paths(mapping.context_paths, "context_paths")?;
    Ok(mapping)
}

fn normalize_anchor_mappings(
    mappings: Vec<AnchorMapping>,
) -> SourceMappingResult<Vec<AnchorMapping>> {
    let mut mappings = mappings
        .into_iter()
        .map(|mut mapping| {
            mapping.namespace = normalized_package_id(&mapping.namespace, "namespace")?;
            mapping.path = normalized_path(&mapping.path, "path")?;
            Ok(mapping)
        })
        .collect::<SourceMappingResult<Vec<_>>>()?;
    mappings.sort();
    mappings.dedup();
    Ok(mappings)
}

fn normalize_role_binding(binding: RoleBinding) -> SourceMappingResult<RoleBinding> {
    match binding {
        RoleBinding::Literal { role_id } => Ok(RoleBinding::Literal {
            role_id: normalized_opaque_ref(&role_id, "role_id")?,
        }),
        RoleBinding::Field {
            path,
            namespace,
            allowed_values,
            allow_verbatim_values,
        } => {
            let path = normalized_path(&path, "path")?;
            let namespace = normalized_package_id(&namespace, "namespace")?;
            let mut normalized = BTreeMap::new();
            for (key, value) in allowed_values {
                let key = normalized_non_empty(&key, "allowed_values.key")?;
                let value = normalized_opaque_ref(&value, "allowed_values.value")?;
                normalized.insert(key, value);
            }
            Ok(RoleBinding::Field {
                path,
                namespace,
                allowed_values: normalized,
                allow_verbatim_values,
            })
        }
    }
}

fn normalize_documentation_ref(
    mut reference: SourceMappingDocumentationRef,
) -> SourceMappingResult<SourceMappingDocumentationRef> {
    reference.label = normalized_non_empty(&reference.label, "documentation.label")?;
    reference.uri = normalized_documentation_uri(&reference.uri, "documentation.uri")?;
    Ok(reference)
}

fn normalize_documentation_refs(
    values: Vec<String>,
    known_docs: &BTreeSet<String>,
    field: &str,
) -> SourceMappingResult<Vec<String>> {
    let mut refs = values
        .into_iter()
        .map(|value| normalized_documentation_uri(&value, field))
        .collect::<SourceMappingResult<Vec<_>>>()?;
    refs.sort();
    refs.dedup();
    for reference in &refs {
        if !known_docs.contains(reference) {
            return Err(artifact_contract_error(format!(
                "{field} references unknown documentation uri {reference}"
            )));
        }
    }
    Ok(refs)
}

fn normalize_paths(values: Vec<String>, field: &str) -> SourceMappingResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_path(&value, field))
        .collect::<SourceMappingResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalize_optional_path(
    value: Option<String>,
    field: &str,
) -> SourceMappingResult<Option<String>> {
    value
        .map(|value| normalized_path(&value, field))
        .transpose()
}

fn normalized_hash(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hash) = value.strip_prefix("blake3:") else {
        return Err(compatibility_policy_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hash.len() != 64
        || !hash
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        return Err(compatibility_policy_error(format!(
            "{field} must be a lowercase blake3 hex digest"
        )));
    }
    Ok(value)
}

fn normalized_package_id(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    if !is_valid_package_token(&value) {
        return Err(artifact_contract_error(format!(
            "{field} must match ^[a-z0-9][a-z0-9._-]*$"
        )));
    }
    Ok(value)
}

fn normalized_opaque_ref(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some((namespace, suffix)) = value.split_once(':') else {
        return Err(artifact_contract_error(format!(
            "{field} must match ^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
        )));
    };
    if !is_valid_package_token(namespace) || !is_valid_package_token(suffix) {
        return Err(artifact_contract_error(format!(
            "{field} must match ^[a-z0-9][a-z0-9._-]*:[a-z0-9][a-z0-9._-]*$"
        )));
    }
    Ok(value)
}

fn normalized_semver(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Err(artifact_contract_error(format!(
            "{field} must match ^[0-9]+\\.[0-9]+\\.[0-9]+$"
        )));
    }
    Ok(value)
}

fn normalized_documentation_uri(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.starts_with('/') || value.contains('\\') || value.contains("../") || value == ".." {
        return Err(artifact_contract_error(format!(
            "{field} cannot be absolute or traverse parents"
        )));
    }
    if value.contains(':') && !value.starts_with("http://") && !value.starts_with("https://") {
        return Err(artifact_contract_error(format!(
            "{field} must be http(s) or a relative doc path"
        )));
    }
    Ok(value)
}

fn normalized_path(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err(artifact_contract_error(format!(
            "{field} must be a dot-separated path without empty segments"
        )));
    }
    Ok(value)
}

fn normalized_fragment_token(value: &str, field: &str) -> SourceMappingResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(policy_constraint_error(format!(
            "{field} cannot normalize an empty role token"
        )));
    }
    let mut normalized = String::new();
    let mut last_was_sep = false;
    for ch in trimmed.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            normalized.push('_');
            last_was_sep = true;
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized.starts_with('_') {
        normalized.remove(0);
    }
    if normalized.is_empty() || !is_valid_package_token(&normalized) {
        return Err(policy_constraint_error(format!(
            "{field} cannot normalize into an opaque role token"
        )));
    }
    Ok(normalized)
}

fn normalized_non_empty(value: &str, field: &str) -> SourceMappingResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn scalar_to_string(value: &Value, field: &str) -> SourceMappingResult<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err(input_shape_error(format!(
            "{field} must be a scalar string, number, or bool"
        ))),
    }
}

fn is_valid_package_token(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}

fn apply_capture_policy(
    policy: CapturePolicy,
    reason: CellDispositionReason,
    path: &str,
    value: Value,
    preserved_cells: &mut Vec<PreservedCell>,
    quarantined_cells: &mut Vec<QuarantinedCell>,
) -> SourceMappingResult<()> {
    match policy {
        CapturePolicy::Preserve => {
            preserved_cells.push(PreservedCell {
                reason,
                path: path.to_string(),
                value,
            });
            Ok(())
        }
        CapturePolicy::Quarantine => {
            quarantined_cells.push(QuarantinedCell {
                reason,
                path: path.to_string(),
                value,
            });
            Ok(())
        }
        CapturePolicy::Reject => Err(policy_constraint_error(format!(
            "{reason:?} at {path} violates source-mapping policy"
        ))),
    }
}

fn cell_cmp_preserved(left: &PreservedCell, right: &PreservedCell) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.reason.cmp(&right.reason))
        .then_with(|| left.value.to_string().cmp(&right.value.to_string()))
}

fn cell_cmp_quarantined(left: &QuarantinedCell, right: &QuarantinedCell) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.reason.cmp(&right.reason))
        .then_with(|| left.value.to_string().cmp(&right.value.to_string()))
}

fn artifact_contract_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::ArtifactContract, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::CompatibilityPolicy, message)
}

fn missing_profile_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::MissingProfile, message)
}

fn missing_field_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::MissingField, message)
}

fn input_shape_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::InputShape, message)
}

fn policy_constraint_error(message: impl Into<String>) -> SourceMappingError {
    SourceMappingError::new(SourceMappingErrorCode::PolicyConstraint, message)
}
