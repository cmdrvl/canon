#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub fn provider_manifest_schema_version() -> &'static str {
    concat!("canon.provider.manifest", ".v1")
}

pub type ProviderSdkResult<T> = Result<T, ProviderSdkError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSdkErrorCode {
    ArtifactContract,
    MissingSourceFile,
    DigestMismatch,
    OfflinePolicy,
    UndeclaredFile,
    ResourceLimitExceeded,
    DuplicateFact,
    CheckpointConflict,
    CompatibilityPolicy,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSdkError {
    pub code: ProviderSdkErrorCode,
    pub message: String,
}

impl ProviderSdkError {
    pub fn new(code: ProviderSdkErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProviderSdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for ProviderSdkError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    FrozenSourceOnly,
    StreamingParser,
    CheckpointResume,
    QuarantineRows,
    SemanticDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    DelimitedUtf8,
    JsonLinesUtf8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointUnit {
    RecordOrdinal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndeclaredFilePolicy {
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateFactPolicy {
    RejectBuild,
    QuarantineLaterDuplicates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticDiffDimension {
    Facts,
    Quarantine,
    Diagnostics,
    SourceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderParserContract {
    pub source_format: SourceFormat,
    pub streaming: bool,
    pub checkpoint_unit: CheckpointUnit,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMappingContract {
    pub fact_schema: String,
    pub fact_key_description: String,
    pub provenance_locator_kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quarantine_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBuildPolicies {
    pub acquisition_separate_from_build: bool,
    pub offline_build_only: bool,
    pub undeclared_file_policy: UndeclaredFilePolicy,
    pub duplicate_fact_policy: DuplicateFactPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBuildLimits {
    pub max_input_bytes: usize,
    pub max_rows: usize,
    pub max_facts: usize,
    pub max_quarantine_rows: usize,
    pub max_diagnostics: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderLicenseContract {
    pub source_license_expression: String,
    pub output_license_expression: String,
    pub attribution_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub schema_version: String,
    pub provider_id: String,
    pub provider_version: String,
    pub source_manifest_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<ProviderCapability>,
    pub parser: ProviderParserContract,
    pub mapping: ProviderMappingContract,
    pub policies: ProviderBuildPolicies,
    pub limits: ProviderBuildLimits,
    pub licenses: ProviderLicenseContract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_diff_dimensions: Vec<SemanticDiffDimension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredSourceFile {
    pub path: String,
    pub media_type: String,
    pub content_digest: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSourceManifest {
    pub manifest_version: String,
    pub source_id: String,
    pub source_version: String,
    pub source_revision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<DeclaredSourceFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSourceFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSourceBundle {
    pub manifest: FrozenSourceManifest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FrozenSourceFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceRecordLocator {
    pub source_path: String,
    pub record_ordinal: u64,
    pub line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFactRecord {
    pub fact_key: String,
    pub fact_schema: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    pub source_digest: String,
    pub locator: SourceRecordLocator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderQuarantineRow {
    pub quarantine_key: String,
    pub reason_code: String,
    pub raw_record_digest: String,
    pub source_digest: String,
    pub locator: SourceRecordLocator,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDiagnostic {
    pub severity: ProviderDiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<SourceRecordLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCheckpoint {
    pub source_path: String,
    pub source_digest: String,
    pub next_record_ordinal: u64,
    pub emitted_facts: usize,
    pub quarantined_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMaterializationDraft {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used_source_paths: Vec<String>,
    pub attempted_network_access: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<ProviderFactRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quarantined_rows: Vec<ProviderQuarantineRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ProviderDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<ProviderCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMaterializationPackage {
    pub package_version: String,
    pub content_digest: String,
    pub provider_id: String,
    pub provider_version: String,
    pub source_manifest_digest: String,
    pub source_revision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<ProviderFactRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quarantined_rows: Vec<ProviderQuarantineRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ProviderDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<ProviderCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSemanticDiff {
    pub left_content_digest: String,
    pub right_content_digest: String,
    pub source_manifest_digest_changed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_fact_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_fact_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_quarantine_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_quarantine_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_diagnostic_codes: Vec<String>,
}

pub trait FrozenSourceProvider {
    fn manifest(&self) -> ProviderManifest;

    fn materialize(
        &self,
        bundle: &FrozenSourceBundle,
        checkpoint: Option<&ProviderCheckpoint>,
    ) -> ProviderSdkResult<ProviderMaterializationDraft>;
}

pub fn finalize_manifest(mut manifest: ProviderManifest) -> ProviderSdkResult<ProviderManifest> {
    if manifest.schema_version.trim().is_empty() {
        manifest.schema_version = provider_manifest_schema_version().to_string();
    }
    if manifest.schema_version != provider_manifest_schema_version() {
        return Err(artifact_contract_error(format!(
            "unsupported provider manifest version: {}",
            manifest.schema_version
        )));
    }

    manifest.provider_id = normalized_package_id(&manifest.provider_id, "provider_id")?;
    manifest.provider_version = normalized_semver(&manifest.provider_version, "provider_version")?;
    manifest.source_manifest_version =
        normalized_non_empty(&manifest.source_manifest_version, "source_manifest_version")?;
    manifest.capabilities.sort();
    manifest.capabilities.dedup();
    if !manifest
        .capabilities
        .contains(&ProviderCapability::FrozenSourceOnly)
    {
        return Err(artifact_contract_error(
            "provider manifest must declare frozen_source_only capability",
        ));
    }
    manifest.semantic_diff_dimensions.sort();
    manifest.semantic_diff_dimensions.dedup();
    if manifest.semantic_diff_dimensions.is_empty() {
        return Err(artifact_contract_error(
            "provider manifest must declare at least one semantic diff dimension",
        ));
    }

    manifest.parser.required_fields =
        normalize_string_vec(manifest.parser.required_fields, "parser.required_fields")?;
    if !manifest.parser.streaming {
        return Err(artifact_contract_error(
            "provider parser contract must declare streaming=true",
        ));
    }
    if manifest.mapping.fact_schema.trim().is_empty()
        || manifest.mapping.fact_key_description.trim().is_empty()
        || manifest.mapping.provenance_locator_kind.trim().is_empty()
    {
        return Err(artifact_contract_error(
            "provider mapping contract fields must be non-empty",
        ));
    }
    manifest.mapping.quarantine_reason_codes = normalize_string_vec(
        manifest.mapping.quarantine_reason_codes,
        "mapping.quarantine_reason_codes",
    )?;
    if !manifest.policies.acquisition_separate_from_build {
        return Err(artifact_contract_error(
            "provider manifest must keep acquisition separate from deterministic offline build",
        ));
    }
    if !manifest.policies.offline_build_only {
        return Err(artifact_contract_error(
            "provider manifest must require offline_build_only=true",
        ));
    }
    validate_limits(&manifest.limits)?;
    manifest.licenses.source_license_expression = normalized_non_empty(
        &manifest.licenses.source_license_expression,
        "licenses.source_license_expression",
    )?;
    manifest.licenses.output_license_expression = normalized_non_empty(
        &manifest.licenses.output_license_expression,
        "licenses.output_license_expression",
    )?;
    Ok(manifest)
}

pub fn finalize_source_bundle(
    mut bundle: FrozenSourceBundle,
) -> ProviderSdkResult<FrozenSourceBundle> {
    bundle.manifest.manifest_version = normalized_non_empty(
        &bundle.manifest.manifest_version,
        "manifest.manifest_version",
    )?;
    bundle.manifest.source_id =
        normalized_non_empty(&bundle.manifest.source_id, "manifest.source_id")?;
    bundle.manifest.source_version =
        normalized_non_empty(&bundle.manifest.source_version, "manifest.source_version")?;
    bundle.manifest.source_revision =
        normalized_non_empty(&bundle.manifest.source_revision, "manifest.source_revision")?;

    bundle.manifest.files = dedupe_components(
        bundle
            .manifest
            .files
            .into_iter()
            .map(normalize_declared_source_file)
            .collect::<ProviderSdkResult<Vec<_>>>()?,
        |file| file.path.clone(),
        "declared source file",
    )?;

    let declared = bundle
        .manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();

    let provided = bundle
        .files
        .into_iter()
        .map(|file| {
            let path = normalized_relative_path(&file.path, "file.path")?;
            Ok(FrozenSourceFile {
                path,
                content: file.content,
            })
        })
        .collect::<ProviderSdkResult<Vec<_>>>()?;

    let mut by_path = BTreeMap::new();
    for file in provided {
        if by_path.insert(file.path.clone(), file).is_some() {
            return Err(artifact_contract_error(
                "source bundle cannot provide the same file path more than once",
            ));
        }
    }

    if by_path.len() != declared.len() {
        return Err(missing_source_file_error(format!(
            "source bundle declared {} files but provided {}",
            declared.len(),
            by_path.len()
        )));
    }

    let files = declared
        .keys()
        .map(|path| {
            let declared_file = declared
                .get(path)
                .expect("declared source file path available");
            let provided_file = by_path.get(path).ok_or_else(|| {
                missing_source_file_error(format!("source file {} was not provided", path))
            })?;
            let digest = blake3_digest(&provided_file.content);
            if digest != declared_file.content_digest {
                return Err(digest_mismatch_error(format!(
                    "source file {} digest mismatch: {} vs {}",
                    path, digest, declared_file.content_digest
                )));
            }
            if provided_file.content.len() != declared_file.bytes {
                return Err(digest_mismatch_error(format!(
                    "source file {} byte count mismatch: {} vs {}",
                    path,
                    provided_file.content.len(),
                    declared_file.bytes
                )));
            }
            Ok(provided_file.clone())
        })
        .collect::<ProviderSdkResult<Vec<_>>>()?;

    bundle.files = files;
    Ok(bundle)
}

pub fn source_manifest_digest(bundle: &FrozenSourceBundle) -> ProviderSdkResult<String> {
    let bundle = finalize_source_bundle(bundle.clone())?;
    canonical_digest(&bundle.manifest)
}

pub fn run_provider_conformance<P: FrozenSourceProvider>(
    provider: &P,
    bundle: &FrozenSourceBundle,
    checkpoint: Option<&ProviderCheckpoint>,
) -> ProviderSdkResult<ProviderMaterializationPackage> {
    let manifest = finalize_manifest(provider.manifest())?;
    let bundle = finalize_source_bundle(bundle.clone())?;
    validate_source_bundle_limits(&manifest, &bundle)?;
    let draft = provider.materialize(&bundle, checkpoint)?;
    finalize_materialization(&manifest, &bundle, draft)
}

pub fn finalize_materialization(
    manifest: &ProviderManifest,
    bundle: &FrozenSourceBundle,
    draft: ProviderMaterializationDraft,
) -> ProviderSdkResult<ProviderMaterializationPackage> {
    let manifest = finalize_manifest(manifest.clone())?;
    let bundle = finalize_source_bundle(bundle.clone())?;
    let declared_paths = bundle
        .manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let declared_digests = bundle
        .manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content_digest.clone()))
        .collect::<BTreeMap<_, _>>();

    if manifest.policies.offline_build_only && draft.attempted_network_access {
        return Err(offline_policy_error(
            "offline build attempted network access",
        ));
    }

    let used_paths = draft
        .used_source_paths
        .into_iter()
        .map(|path| normalized_relative_path(&path, "used_source_paths"))
        .collect::<ProviderSdkResult<Vec<_>>>()?;
    for path in &used_paths {
        if manifest.policies.undeclared_file_policy == UndeclaredFilePolicy::Reject
            && !declared_paths.contains(path)
        {
            return Err(undeclared_file_error(format!(
                "provider attempted to use undeclared source file {}",
                path
            )));
        }
    }

    if draft.facts.len() > manifest.limits.max_facts
        || draft.quarantined_rows.len() > manifest.limits.max_quarantine_rows
        || draft.diagnostics.len() > manifest.limits.max_diagnostics
    {
        return Err(resource_limit_error(
            "materialization output exceeded declared limits",
        ));
    }

    let mut facts = draft
        .facts
        .into_iter()
        .map(|fact| normalize_fact_record(fact, &declared_digests, &manifest.mapping.fact_schema))
        .collect::<ProviderSdkResult<Vec<_>>>()?;
    let mut quarantined_rows = draft
        .quarantined_rows
        .into_iter()
        .map(|row| normalize_quarantine_row(row, &declared_digests))
        .collect::<ProviderSdkResult<Vec<_>>>()?;
    let mut diagnostics = draft
        .diagnostics
        .into_iter()
        .map(|diagnostic| normalize_diagnostic(diagnostic, &declared_paths))
        .collect::<ProviderSdkResult<Vec<_>>>()?;
    let checkpoint = draft
        .checkpoint
        .map(|checkpoint| normalize_checkpoint(checkpoint, &declared_digests))
        .transpose()?;

    facts.sort_by(|left, right| left.fact_key.cmp(&right.fact_key));
    quarantined_rows.sort_by(|left, right| left.quarantine_key.cmp(&right.quarantine_key));
    diagnostics.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then(left.message.cmp(&right.message))
            .then(left.source_path.cmp(&right.source_path))
    });

    let mut deduped_facts: Vec<ProviderFactRecord> = Vec::with_capacity(facts.len());
    for fact in facts {
        if let Some(previous) = deduped_facts.last()
            && previous.fact_key == fact.fact_key
        {
            if manifest.policies.duplicate_fact_policy == DuplicateFactPolicy::RejectBuild {
                return Err(duplicate_fact_error(format!(
                    "duplicate fact key {} was emitted",
                    fact.fact_key
                )));
            }
            quarantined_rows.push(ProviderQuarantineRow {
                quarantine_key: format!("duplicate:{}", fact.fact_key),
                reason_code: "duplicate_fact_key".to_string(),
                raw_record_digest: canonical_digest(&fact.fields)?,
                source_digest: fact.source_digest,
                locator: fact.locator,
                message: format!("duplicate fact key {}", fact.fact_key),
            });
            continue;
        }
        deduped_facts.push(fact);
    }

    quarantined_rows.sort_by(|left, right| left.quarantine_key.cmp(&right.quarantine_key));
    quarantined_rows.dedup();

    let package_version = "provider_materialization_package.v1".to_string();
    let source_manifest_digest = source_manifest_digest(&bundle)?;
    let payload = ProviderMaterializationPackagePayload {
        package_version: package_version.clone(),
        provider_id: manifest.provider_id.clone(),
        provider_version: manifest.provider_version.clone(),
        source_manifest_digest: source_manifest_digest.clone(),
        source_revision: bundle.manifest.source_revision.clone(),
        facts: deduped_facts,
        quarantined_rows,
        diagnostics,
        checkpoint,
    };
    let content_digest = canonical_digest(&payload)?;

    Ok(ProviderMaterializationPackage {
        package_version,
        content_digest,
        provider_id: payload.provider_id,
        provider_version: payload.provider_version,
        source_manifest_digest,
        source_revision: payload.source_revision,
        facts: payload.facts,
        quarantined_rows: payload.quarantined_rows,
        diagnostics: payload.diagnostics,
        checkpoint: payload.checkpoint,
    })
}

pub fn semantic_diff(
    left: &ProviderMaterializationPackage,
    right: &ProviderMaterializationPackage,
) -> ProviderSdkResult<ProviderSemanticDiff> {
    let left_facts = left
        .facts
        .iter()
        .map(|fact| fact.fact_key.clone())
        .collect::<BTreeSet<_>>();
    let right_facts = right
        .facts
        .iter()
        .map(|fact| fact.fact_key.clone())
        .collect::<BTreeSet<_>>();
    let left_quarantine = left
        .quarantined_rows
        .iter()
        .map(|row| row.quarantine_key.clone())
        .collect::<BTreeSet<_>>();
    let right_quarantine = right
        .quarantined_rows
        .iter()
        .map(|row| row.quarantine_key.clone())
        .collect::<BTreeSet<_>>();
    let left_diagnostics = left
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect::<BTreeSet<_>>();
    let right_diagnostics = right
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect::<BTreeSet<_>>();

    Ok(ProviderSemanticDiff {
        left_content_digest: left.content_digest.clone(),
        right_content_digest: right.content_digest.clone(),
        source_manifest_digest_changed: left.source_manifest_digest != right.source_manifest_digest,
        added_fact_keys: right_facts.difference(&left_facts).cloned().collect(),
        removed_fact_keys: left_facts.difference(&right_facts).cloned().collect(),
        added_quarantine_keys: right_quarantine
            .difference(&left_quarantine)
            .cloned()
            .collect(),
        removed_quarantine_keys: left_quarantine
            .difference(&right_quarantine)
            .cloned()
            .collect(),
        changed_diagnostic_codes: left_diagnostics
            .symmetric_difference(&right_diagnostics)
            .cloned()
            .collect(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProviderMaterializationPackagePayload {
    package_version: String,
    provider_id: String,
    provider_version: String,
    source_manifest_digest: String,
    source_revision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    facts: Vec<ProviderFactRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    quarantined_rows: Vec<ProviderQuarantineRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<ProviderDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint: Option<ProviderCheckpoint>,
}

fn validate_limits(limits: &ProviderBuildLimits) -> ProviderSdkResult<()> {
    if limits.max_input_bytes == 0
        || limits.max_rows == 0
        || limits.max_facts == 0
        || limits.max_quarantine_rows == 0
        || limits.max_diagnostics == 0
    {
        return Err(artifact_contract_error(
            "all provider build limits must be greater than zero",
        ));
    }
    Ok(())
}

fn validate_source_bundle_limits(
    manifest: &ProviderManifest,
    bundle: &FrozenSourceBundle,
) -> ProviderSdkResult<()> {
    let total_input_bytes = bundle.files.iter().try_fold(0usize, |acc, file| {
        acc.checked_add(file.content.len()).ok_or_else(|| {
            resource_limit_error("source bundle byte count overflowed usize during validation")
        })
    })?;
    if total_input_bytes > manifest.limits.max_input_bytes {
        return Err(resource_limit_error(format!(
            "source bundle bytes {} exceeded limit {}",
            total_input_bytes, manifest.limits.max_input_bytes
        )));
    }

    let total_rows = bundle.files.iter().try_fold(0usize, |acc, file| {
        let text = std::str::from_utf8(&file.content).map_err(|_| {
            compatibility_policy_error(format!(
                "source file {} must be valid UTF-8 for {:?}",
                file.path, manifest.parser.source_format
            ))
        })?;
        let file_rows = match manifest.parser.source_format {
            SourceFormat::DelimitedUtf8 => text.lines().skip(1).count(),
            SourceFormat::JsonLinesUtf8 => text.lines().count(),
        };
        acc.checked_add(file_rows).ok_or_else(|| {
            resource_limit_error("source bundle row count overflowed usize during validation")
        })
    })?;
    if total_rows > manifest.limits.max_rows {
        return Err(resource_limit_error(format!(
            "source bundle rows {} exceeded limit {}",
            total_rows, manifest.limits.max_rows
        )));
    }

    Ok(())
}

fn normalize_declared_source_file(
    mut file: DeclaredSourceFile,
) -> ProviderSdkResult<DeclaredSourceFile> {
    file.path = normalized_relative_path(&file.path, "declared_source_file.path")?;
    file.media_type = normalized_non_empty(&file.media_type, "declared_source_file.media_type")?;
    file.content_digest =
        normalized_hash(&file.content_digest, "declared_source_file.content_digest")?;
    if file.bytes == 0 {
        return Err(artifact_contract_error(
            "declared source file bytes must be greater than zero",
        ));
    }
    Ok(file)
}

fn normalize_fact_record(
    mut fact: ProviderFactRecord,
    declared_digests: &BTreeMap<String, String>,
    expected_schema: &str,
) -> ProviderSdkResult<ProviderFactRecord> {
    fact.fact_key = normalized_non_empty(&fact.fact_key, "fact_key")?;
    fact.fact_schema = normalized_non_empty(&fact.fact_schema, "fact_schema")?;
    if fact.fact_schema != expected_schema {
        return Err(artifact_contract_error(format!(
            "fact {} used schema {} but manifest requires {}",
            fact.fact_key, fact.fact_schema, expected_schema
        )));
    }
    fact.source_digest = normalized_hash(&fact.source_digest, "fact.source_digest")?;
    fact.locator = normalize_locator(fact.locator, declared_digests)?;
    if declared_digests
        .get(&fact.locator.source_path)
        .is_none_or(|digest| digest != &fact.source_digest)
    {
        return Err(digest_mismatch_error(format!(
            "fact {} digest does not match declared source {}",
            fact.fact_key, fact.locator.source_path
        )));
    }
    fact.fields = normalize_field_map(fact.fields)?;
    Ok(fact)
}

fn normalize_quarantine_row(
    mut row: ProviderQuarantineRow,
    declared_digests: &BTreeMap<String, String>,
) -> ProviderSdkResult<ProviderQuarantineRow> {
    row.quarantine_key = normalized_non_empty(&row.quarantine_key, "quarantine_key")?;
    row.reason_code = normalized_component_id(&row.reason_code, "reason_code")?;
    row.raw_record_digest = normalized_hash(&row.raw_record_digest, "raw_record_digest")?;
    row.source_digest = normalized_hash(&row.source_digest, "quarantine.source_digest")?;
    row.message = normalized_non_empty(&row.message, "quarantine.message")?;
    row.locator = normalize_locator(row.locator, declared_digests)?;
    if declared_digests
        .get(&row.locator.source_path)
        .is_none_or(|digest| digest != &row.source_digest)
    {
        return Err(digest_mismatch_error(format!(
            "quarantine row {} digest does not match declared source {}",
            row.quarantine_key, row.locator.source_path
        )));
    }
    Ok(row)
}

fn normalize_diagnostic(
    mut diagnostic: ProviderDiagnostic,
    declared_paths: &BTreeSet<String>,
) -> ProviderSdkResult<ProviderDiagnostic> {
    diagnostic.code = normalized_component_id(&diagnostic.code, "diagnostic.code")?;
    diagnostic.message = normalized_non_empty(&diagnostic.message, "diagnostic.message")?;
    diagnostic.source_path = diagnostic
        .source_path
        .map(|path| normalized_relative_path(&path, "diagnostic.source_path"))
        .transpose()?;
    if let Some(source_path) = &diagnostic.source_path
        && !declared_paths.contains(source_path)
    {
        return Err(undeclared_file_error(format!(
            "diagnostic referenced undeclared source file {}",
            source_path
        )));
    }
    if let Some(locator) = diagnostic.locator.take() {
        diagnostic.locator = Some(normalize_locator_with_paths(locator, declared_paths)?);
    }
    Ok(diagnostic)
}

fn normalize_checkpoint(
    mut checkpoint: ProviderCheckpoint,
    declared_digests: &BTreeMap<String, String>,
) -> ProviderSdkResult<ProviderCheckpoint> {
    checkpoint.source_path =
        normalized_relative_path(&checkpoint.source_path, "checkpoint.source_path")?;
    checkpoint.source_digest =
        normalized_hash(&checkpoint.source_digest, "checkpoint.source_digest")?;
    if checkpoint.next_record_ordinal == 0 {
        return Err(checkpoint_conflict_error(
            "checkpoint next_record_ordinal must be greater than zero",
        ));
    }
    if declared_digests
        .get(&checkpoint.source_path)
        .is_none_or(|digest| digest != &checkpoint.source_digest)
    {
        return Err(checkpoint_conflict_error(format!(
            "checkpoint source digest does not match declared source {}",
            checkpoint.source_path
        )));
    }
    Ok(checkpoint)
}

fn normalize_locator(
    mut locator: SourceRecordLocator,
    declared_digests: &BTreeMap<String, String>,
) -> ProviderSdkResult<SourceRecordLocator> {
    locator.source_path = normalized_relative_path(&locator.source_path, "locator.source_path")?;
    if locator.record_ordinal == 0 || locator.line_number == 0 {
        return Err(artifact_contract_error(
            "locator record_ordinal and line_number must be greater than zero",
        ));
    }
    if !declared_digests.contains_key(&locator.source_path) {
        return Err(missing_source_file_error(format!(
            "locator referenced undeclared source file {}",
            locator.source_path
        )));
    }
    locator.field_path = locator
        .field_path
        .map(|field| normalized_non_empty(&field, "locator.field_path"))
        .transpose()?;
    Ok(locator)
}

fn normalize_locator_with_paths(
    mut locator: SourceRecordLocator,
    declared_paths: &BTreeSet<String>,
) -> ProviderSdkResult<SourceRecordLocator> {
    locator.source_path = normalized_relative_path(&locator.source_path, "locator.source_path")?;
    if locator.record_ordinal == 0 || locator.line_number == 0 {
        return Err(artifact_contract_error(
            "locator record_ordinal and line_number must be greater than zero",
        ));
    }
    if !declared_paths.contains(&locator.source_path) {
        return Err(undeclared_file_error(format!(
            "locator referenced undeclared source file {}",
            locator.source_path
        )));
    }
    locator.field_path = locator
        .field_path
        .map(|field| normalized_non_empty(&field, "locator.field_path"))
        .transpose()?;
    Ok(locator)
}

fn normalize_field_map(
    fields: BTreeMap<String, String>,
) -> ProviderSdkResult<BTreeMap<String, String>> {
    fields
        .into_iter()
        .map(|(key, value)| {
            Ok((
                normalized_component_id(&key, "fact.fields.key")?,
                normalized_non_empty(&value, "fact.fields.value")?,
            ))
        })
        .collect()
}

fn normalize_string_vec(values: Vec<String>, field: &str) -> ProviderSdkResult<Vec<String>> {
    let mut values = values
        .into_iter()
        .map(|value| normalized_non_empty(&value, field))
        .collect::<ProviderSdkResult<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

fn dedupe_components<T, F>(mut values: Vec<T>, key: F, label: &str) -> ProviderSdkResult<Vec<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    values.sort_by_key(|value| key(value));
    let mut deduped = Vec::with_capacity(values.len());
    for value in values {
        if let Some(previous) = deduped.last()
            && key(previous) == key(&value)
        {
            if previous != &value {
                return Err(artifact_contract_error(format!(
                    "{label} {} cannot be declared with conflicting content",
                    key(&value)
                )));
            }
            continue;
        }
        deduped.push(value);
    }
    Ok(deduped)
}

fn normalized_relative_path(value: &str, field: &str) -> ProviderSdkResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.starts_with('/') || value.contains('\\') || value.split('/').any(|part| part == "..") {
        return Err(artifact_contract_error(format!(
            "{field} must be relative and must not contain traversal segments"
        )));
    }
    Ok(value)
}

fn normalized_package_id(value: &str, field: &str) -> ProviderSdkResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use lowercase [a-z0-9._-] characters"
    )))
}

fn normalized_component_id(value: &str, field: &str) -> ProviderSdkResult<String> {
    let value = normalized_non_empty(value, field)?;
    if value.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'.' | b'_' | b'-' | b':')
    }) {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must use lowercase [a-z0-9._:-] characters"
    )))
}

fn normalized_semver(value: &str, field: &str) -> ProviderSdkResult<String> {
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
        "{field} must use MAJOR.MINOR.PATCH numeric semver"
    )))
}

fn normalized_hash(value: &str, field: &str) -> ProviderSdkResult<String> {
    let value = normalized_non_empty(value, field)?;
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must start with blake3:"
        )));
    };
    if hex.len() == 64
        && hex
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        return Ok(value);
    }
    Err(artifact_contract_error(format!(
        "{field} must match ^blake3:[0-9a-f]{{64}}$"
    )))
}

fn normalized_non_empty(value: &str, field: &str) -> ProviderSdkResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty"
        )));
    }
    Ok(value)
}

fn artifact_contract_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::ArtifactContract, message)
}

fn missing_source_file_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::MissingSourceFile, message)
}

fn digest_mismatch_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::DigestMismatch, message)
}

fn offline_policy_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::OfflinePolicy, message)
}

fn undeclared_file_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::UndeclaredFile, message)
}

fn resource_limit_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::ResourceLimitExceeded, message)
}

fn duplicate_fact_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::DuplicateFact, message)
}

fn checkpoint_conflict_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::CheckpointConflict, message)
}

fn compatibility_policy_error(message: impl Into<String>) -> ProviderSdkError {
    ProviderSdkError::new(ProviderSdkErrorCode::CompatibilityPolicy, message)
}

fn blake3_digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn canonical_digest<T: Serialize>(value: &T) -> ProviderSdkResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize value for digest computation: {error}"
        ))
    })?;
    Ok(blake3_digest(&bytes))
}
