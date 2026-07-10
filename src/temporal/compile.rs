use super::alias::{
    AliasClaim, AliasScope, LookupVisibility, compile_alias_snapshot, finalize_alias_claims,
};
use super::conflict::{
    ConflictDisposition, ConflictPolicy, ConflictRecord, compile_conflict_artifact,
};
use super::fact::{
    RecordedTime, SourceLocator, TemporalError, TemporalErrorCode, TemporalResult, TimeInterval,
};
use crate::RegistryDiffEntry;
use crate::registry::{
    REGISTRY_PACKAGE_SCHEMA_VERSION, RegistryPackage, RegistryPackageDependencyReference,
    RegistryPackageDeploymentProjection, RegistryPackageDescriptor, RegistryPackageIdentityRules,
    RegistryPackageLayouts, RegistryPackageRegistryIdentity, validate_registry_package,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CANON_TEMPORAL_COMPILE_VERSION: &str = "canon.temporal.compile.v1";

const REGISTRY_METADATA_KIND: &str = "registry_metadata";
const MAPPING_KIND: &str = "mapping";
const BUILD_PROVENANCE_KIND: &str = "build_provenance";
const CANONICAL_TYPE_ENTITY: &str = "entity";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalCompileRequest {
    pub version: String,
    pub registry_id: String,
    pub registry_version: String,
    pub valid_at: String,
    pub known_as_of: String,
    pub policy: ConflictPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<AliasClaim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_filter: Option<CompileScopeFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_snapshot: Option<RegistryPackageDependencyReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_sidecars: Vec<TemporalRelationSidecar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_iri_namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompileScopeFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_systems: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalRelationSidecar {
    pub schema_version: String,
    pub path: String,
    pub content_digest: String,
    pub relation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalCompileArtifact {
    pub version: String,
    pub valid_at: String,
    pub known_as_of: String,
    pub policy_id: String,
    pub registry_package: RegistryPackage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_snapshot: Option<RegistryPackageDependencyReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_sidecars: Vec<TemporalRelationSidecar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mapping_proofs: Vec<TemporalCompileMappingProof>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omissions: Vec<TemporalCompileOmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalCompileMappingProof {
    pub input: String,
    pub canonical_id: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub claim_id: String,
    pub valid_time: TimeInterval,
    pub recorded_time: RecordedTime,
    pub source_locator: SourceLocator,
    pub scope: AliasScope,
    pub trust_policy_ref: String,
    pub materialization_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_clause_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalCompileOmissionReason {
    ConflictAbstained,
    ScopeExcluded,
    NonGlobalVisibility,
    RelationSidecarOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalCompileOmission {
    pub reason: TemporalCompileOmissionReason,
    pub subject_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claim_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_clause_ids: Vec<String>,
    pub message: String,
}

pub fn compile_exact_lookup_snapshot(
    request: TemporalCompileRequest,
) -> TemporalResult<TemporalCompileArtifact> {
    let request = finalize_compile_request(request)?;
    let claims = finalize_alias_claims(request.claims.clone())?;
    let (included_claims, mut omissions) = partition_claims(&claims, request.scope_filter.as_ref());

    let snapshot =
        compile_alias_snapshot(&included_claims, &request.valid_at, &request.known_as_of)?;
    let conflict_artifact = compile_conflict_artifact(
        &included_claims,
        request.policy.clone(),
        &request.valid_at,
        &request.known_as_of,
    )?;

    let claims_by_id = conflict_artifact
        .claims
        .iter()
        .cloned()
        .map(|claim| (claim.claim_id.clone(), claim))
        .collect::<BTreeMap<_, _>>();
    let policy_clause_ids_by_claim = winning_policy_clause_ids(&conflict_artifact.conflicts);

    let mut lookup_entries = Vec::new();
    let mut mapping_proofs = Vec::new();
    let rule_id = format!("TEMPORAL_ALIAS_SNAPSHOT:{}", request.policy.policy_id);
    for claim_id in &conflict_artifact.global_exact_claim_ids {
        let claim = claims_by_id.get(claim_id).ok_or_else(|| {
            artifact_contract_error(format!(
                "global exact claim_id {claim_id} was not present in conflict artifact history"
            ))
        })?;

        let mut policy_clause_ids = policy_clause_ids_by_claim
            .get(claim_id)
            .cloned()
            .unwrap_or_default();
        if let Some(promotion) = &claim.promoted_to_global_by {
            push_unique(&mut policy_clause_ids, promotion.policy_clause_id.clone());
        }

        lookup_entries.push(RegistryDiffEntry {
            input: claim.alias_value.clone(),
            canonical_id: claim.entity_id.clone(),
            canonical_type: CANONICAL_TYPE_ENTITY.to_string(),
            rule_id: rule_id.clone(),
        });
        mapping_proofs.push(TemporalCompileMappingProof {
            input: claim.alias_value.clone(),
            canonical_id: claim.entity_id.clone(),
            canonical_type: CANONICAL_TYPE_ENTITY.to_string(),
            rule_id: rule_id.clone(),
            claim_id: claim.claim_id.clone(),
            valid_time: claim.valid_time.clone(),
            recorded_time: claim.recorded_time.clone(),
            source_locator: claim.source_locator.clone(),
            scope: claim.scope.clone(),
            trust_policy_ref: claim.trust_policy_ref.clone(),
            materialization_digest: claim.materialization_digest.clone(),
            policy_clause_ids,
        });
    }

    omissions.extend(conflict_omissions(&conflict_artifact.conflicts));
    omissions.extend(non_global_omissions(
        &snapshot.active_claims,
        &conflict_artifact.global_exact_claim_ids,
        &conflict_artifact.conflicts,
    ));
    omissions.extend(relation_sidecar_omissions(&request.relation_sidecars));

    mapping_proofs.sort_by(|left, right| {
        left.input
            .cmp(&right.input)
            .then_with(|| left.claim_id.cmp(&right.claim_id))
    });
    omissions.sort_by(|left, right| {
        left.reason
            .cmp(&right.reason)
            .then_with(|| left.subject_key.cmp(&right.subject_key))
    });

    let registry_package = build_registry_package(
        &request,
        &rule_id,
        lookup_entries,
        &mapping_proofs,
        &omissions,
    )?;

    Ok(TemporalCompileArtifact {
        version: CANON_TEMPORAL_COMPILE_VERSION.to_string(),
        valid_at: request.valid_at,
        known_as_of: request.known_as_of,
        policy_id: request.policy.policy_id,
        registry_package,
        parent_snapshot: request.parent_snapshot,
        relation_sidecars: request.relation_sidecars,
        mapping_proofs,
        omissions,
    })
}

pub fn canonical_compile_bytes(artifact: &TemporalCompileArtifact) -> TemporalResult<Vec<u8>> {
    validate_registry_package(&artifact.registry_package)
        .map_err(|error| artifact_contract_error(error.to_string()))?;

    let mut canonical = artifact.clone();
    canonical
        .relation_sidecars
        .sort_by(|left, right| left.path.cmp(&right.path));
    canonical.mapping_proofs.sort_by(|left, right| {
        left.input
            .cmp(&right.input)
            .then_with(|| left.claim_id.cmp(&right.claim_id))
    });
    canonical.omissions.sort_by(|left, right| {
        left.reason
            .cmp(&right.reason)
            .then_with(|| left.subject_key.cmp(&right.subject_key))
    });
    serde_json::to_vec(&canonical).map_err(|error| {
        artifact_contract_error(format!("failed to serialize compile artifact: {error}"))
    })
}

fn finalize_compile_request(
    mut request: TemporalCompileRequest,
) -> TemporalResult<TemporalCompileRequest> {
    if request.version.trim().is_empty() {
        request.version = CANON_TEMPORAL_COMPILE_VERSION.to_string();
    }
    if request.version != CANON_TEMPORAL_COMPILE_VERSION {
        return Err(artifact_contract_error(format!(
            "unsupported temporal compile version: {}",
            request.version
        )));
    }

    request.registry_id = normalized_non_empty(&request.registry_id, "registry_id")?;
    request.registry_version = normalized_non_empty(&request.registry_version, "registry_version")?;
    request.valid_at = canonical_timestamp(&request.valid_at, "valid_at")?;
    request.known_as_of = canonical_timestamp(&request.known_as_of, "known_as_of")?;
    request.scope_filter = request
        .scope_filter
        .map(normalize_scope_filter)
        .transpose()?;
    request.parent_snapshot = request
        .parent_snapshot
        .map(normalize_dependency_reference)
        .transpose()?;
    request.relation_sidecars = request
        .relation_sidecars
        .into_iter()
        .map(normalize_relation_sidecar)
        .collect::<TemporalResult<Vec<_>>>()?;
    request
        .relation_sidecars
        .sort_by(|left, right| left.path.cmp(&right.path));
    request.canonical_iri_namespace = request
        .canonical_iri_namespace
        .map(|value| normalized_non_empty(&value, "canonical_iri_namespace"))
        .transpose()?;
    Ok(request)
}

fn normalize_scope_filter(mut filter: CompileScopeFilter) -> TemporalResult<CompileScopeFilter> {
    if filter.scope_type.is_some() ^ filter.scope_id.is_some() {
        return Err(artifact_contract_error(
            "scope_filter.scope_type and scope_filter.scope_id must both be set or both be omitted",
        ));
    }

    filter.scope_type = filter
        .scope_type
        .map(|value| normalized_non_empty(&value, "scope_filter.scope_type"))
        .transpose()?;
    filter.scope_id = filter
        .scope_id
        .map(|value| normalized_non_empty(&value, "scope_filter.scope_id"))
        .transpose()?;

    let mut normalized = filter
        .source_systems
        .into_iter()
        .map(|value| normalized_non_empty(&value, "scope_filter.source_systems"))
        .collect::<TemporalResult<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    filter.source_systems = normalized;
    Ok(filter)
}

fn normalize_dependency_reference(
    mut dependency: RegistryPackageDependencyReference,
) -> TemporalResult<RegistryPackageDependencyReference> {
    dependency.id = normalized_non_empty(&dependency.id, "parent_snapshot.id")?;
    dependency.version = normalized_non_empty(&dependency.version, "parent_snapshot.version")?;
    dependency.content_digest =
        normalized_hash(&dependency.content_digest, "parent_snapshot.content_digest")?;
    Ok(dependency)
}

fn normalize_relation_sidecar(
    mut sidecar: TemporalRelationSidecar,
) -> TemporalResult<TemporalRelationSidecar> {
    sidecar.schema_version =
        normalized_non_empty(&sidecar.schema_version, "relation_sidecars.schema_version")?;
    sidecar.path = normalize_relative_path(&sidecar.path, "relation_sidecars.path")?;
    sidecar.content_digest =
        normalized_hash(&sidecar.content_digest, "relation_sidecars.content_digest")?;
    Ok(sidecar)
}

fn partition_claims(
    claims: &[AliasClaim],
    scope_filter: Option<&CompileScopeFilter>,
) -> (Vec<AliasClaim>, Vec<TemporalCompileOmission>) {
    let Some(filter) = scope_filter else {
        return (claims.to_vec(), Vec::new());
    };

    let mut included = Vec::new();
    let mut omissions = Vec::new();
    for claim in claims {
        if claim_matches_scope_filter(claim, filter) {
            included.push(claim.clone());
            continue;
        }
        omissions.push(TemporalCompileOmission {
            reason: TemporalCompileOmissionReason::ScopeExcluded,
            subject_key: claim.claim_id.clone(),
            claim_ids: vec![claim.claim_id.clone()],
            policy_clause_ids: Vec::new(),
            message: "claim was excluded by the temporal compile scope filter".to_string(),
        });
    }
    (included, omissions)
}

fn claim_matches_scope_filter(claim: &AliasClaim, filter: &CompileScopeFilter) -> bool {
    if !filter.source_systems.is_empty() {
        let matches_source = filter.source_systems.iter().any(|source_system| {
            claim.source_locator.source_system == *source_system
                || claim.scope.source_system.as_deref() == Some(source_system.as_str())
        });
        if !matches_source {
            return false;
        }
    }

    if filter
        .scope_type
        .as_deref()
        .is_some_and(|scope_type| claim.scope.scope_type.as_deref() != Some(scope_type))
    {
        return false;
    }
    if filter
        .scope_id
        .as_deref()
        .is_some_and(|scope_id| claim.scope.scope_id.as_deref() != Some(scope_id))
    {
        return false;
    }
    true
}

fn winning_policy_clause_ids(conflicts: &[ConflictRecord]) -> BTreeMap<String, Vec<String>> {
    let mut by_claim = BTreeMap::<String, Vec<String>>::new();
    for record in conflicts {
        if let Some(winning_claim_id) = &record.winning_claim_id {
            let entry = by_claim.entry(winning_claim_id.clone()).or_default();
            for clause_id in &record.policy_clause_ids_used {
                push_unique(entry, clause_id.clone());
            }
        }
    }
    by_claim
}

fn conflict_omissions(conflicts: &[ConflictRecord]) -> Vec<TemporalCompileOmission> {
    conflicts
        .iter()
        .filter(|record| matches!(record.disposition, ConflictDisposition::Abstain))
        .map(|record| TemporalCompileOmission {
            reason: TemporalCompileOmissionReason::ConflictAbstained,
            subject_key: record.subject_key.clone(),
            claim_ids: record.claim_ids.clone(),
            policy_clause_ids: record.policy_clause_ids_used.clone(),
            message: record.message.clone(),
        })
        .collect()
}

fn non_global_omissions(
    active_claims: &[AliasClaim],
    winning_claim_ids: &[String],
    conflicts: &[ConflictRecord],
) -> Vec<TemporalCompileOmission> {
    let winning_claim_ids = winning_claim_ids.iter().cloned().collect::<BTreeSet<_>>();
    let conflict_claim_ids = conflicts
        .iter()
        .filter(|record| matches!(record.disposition, ConflictDisposition::Abstain))
        .flat_map(|record| record.claim_ids.iter().cloned())
        .collect::<BTreeSet<_>>();

    active_claims
        .iter()
        .filter(|claim| !winning_claim_ids.contains(&claim.claim_id))
        .filter(|claim| !conflict_claim_ids.contains(&claim.claim_id))
        .filter(|claim| !matches!(claim.lookup_visibility, LookupVisibility::Global))
        .map(|claim| TemporalCompileOmission {
            reason: TemporalCompileOmissionReason::NonGlobalVisibility,
            subject_key: claim.claim_id.clone(),
            claim_ids: vec![claim.claim_id.clone()],
            policy_clause_ids: Vec::new(),
            message:
                "source-scoped alias remained local and did not enter the exact lookup snapshot"
                    .to_string(),
        })
        .collect()
}

fn relation_sidecar_omissions(
    relation_sidecars: &[TemporalRelationSidecar],
) -> Vec<TemporalCompileOmission> {
    relation_sidecars
        .iter()
        .map(|sidecar| TemporalCompileOmission {
            reason: TemporalCompileOmissionReason::RelationSidecarOnly,
            subject_key: sidecar.path.clone(),
            claim_ids: Vec::new(),
            policy_clause_ids: Vec::new(),
            message:
                "relationship sidecars are preserved for projection only and never create exact lookup mappings"
                    .to_string(),
        })
        .collect()
}

fn build_registry_package(
    request: &TemporalCompileRequest,
    rule_id: &str,
    lookup_entries: Vec<RegistryDiffEntry>,
    mapping_proofs: &[TemporalCompileMappingProof],
    omissions: &[TemporalCompileOmission],
) -> TemporalResult<RegistryPackage> {
    let metadata_bytes = canonical_json_bytes(
        &serde_json::json!({
            "id": request.registry_id,
            "version": request.registry_version,
            "compiler": CANON_TEMPORAL_COMPILE_VERSION,
            "valid_at": request.valid_at,
            "known_as_of": request.known_as_of,
            "entry_count": lookup_entries.len(),
            "canonical_iri_namespace": request.canonical_iri_namespace,
        }),
        "temporal compile registry metadata",
    )?;
    let mapping_bytes = canonical_json_bytes(&lookup_entries, "temporal compile lookup entries")?;
    let build_provenance_bytes = canonical_json_bytes(
        &serde_json::json!({
            "version": CANON_TEMPORAL_COMPILE_VERSION,
            "policy_id": request.policy.policy_id,
            "rule_id": rule_id,
            "scope_filter": request.scope_filter,
            "parent_snapshot": request.parent_snapshot,
            "relation_sidecars": request.relation_sidecars,
            "mapping_proofs": mapping_proofs,
            "omissions": omissions,
        }),
        "temporal compile build provenance",
    )?;

    let mut package = RegistryPackage {
        schema_version: REGISTRY_PACKAGE_SCHEMA_VERSION.to_string(),
        registry: RegistryPackageRegistryIdentity {
            id: request.registry_id.clone(),
            version: request.registry_version.clone(),
        },
        content_digest: String::new(),
        entry_count: lookup_entries.len(),
        effective_mapping_count: lookup_entries.len(),
        canonical_iri_namespace: request.canonical_iri_namespace.clone(),
        file_descriptors: vec![
            descriptor_for(
                "registry.json",
                REGISTRY_METADATA_KIND,
                &metadata_bytes,
                None,
            ),
            descriptor_for(
                "mappings/exact_lookup.json",
                MAPPING_KIND,
                &mapping_bytes,
                Some(lookup_entries.len()),
            ),
        ],
        build_provenance: Some(descriptor_for(
            "_build/temporal_compile.json",
            BUILD_PROVENANCE_KIND,
            &build_provenance_bytes,
            None,
        )),
        attachments: Vec::new(),
        dependency_references: request.parent_snapshot.clone().into_iter().collect(),
        allowed_sidecars: vec![
            "audit".to_string(),
            "gold".to_string(),
            "strategy".to_string(),
            "signature".to_string(),
            "relation".to_string(),
            "escrow".to_string(),
        ],
        deployment_projections: vec![
            RegistryPackageDeploymentProjection {
                kind: "dbt-seed".to_string(),
                first_class: true,
                identity_excluded: true,
            },
            RegistryPackageDeploymentProjection {
                kind: "search-index".to_string(),
                first_class: true,
                identity_excluded: true,
            },
        ],
        lookup_entries,
        identity: RegistryPackageIdentityRules {
            hash_algorithm: "blake3".to_string(),
            descriptor_ordering: "normalized_path_lexicographic".to_string(),
            mapping_precedence: "filename_lexicographic_then_entry_order".to_string(),
            identity_exclusions: vec![
                "_index.sqlite".to_string(),
                "absolute_paths".to_string(),
                "derived_caches".to_string(),
                "mtime".to_string(),
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

    canonicalize_package_fields(&mut package);
    package.content_digest = registry_package_digest(&package)?;
    validate_registry_package(&package)
        .map_err(|error| artifact_contract_error(error.to_string()))?;
    Ok(package)
}

fn descriptor_for(
    path: &str,
    kind: &str,
    bytes: &[u8],
    entry_count: Option<usize>,
) -> RegistryPackageDescriptor {
    RegistryPackageDescriptor {
        path: path.to_string(),
        kind: kind.to_string(),
        content_digest: hash_bytes(bytes),
        bytes: bytes.len() as u64,
        entry_count,
    }
}

fn registry_package_digest(package: &RegistryPackage) -> TemporalResult<String> {
    let mut digest_view = package.clone();
    canonicalize_package_fields(&mut digest_view);
    digest_view.content_digest.clear();
    let bytes = serde_json::to_vec(&digest_view).map_err(|error| {
        artifact_contract_error(format!(
            "failed to serialize registry package digest view: {error}"
        ))
    })?;
    Ok(hash_bytes(&bytes))
}

fn canonicalize_package_fields(package: &mut RegistryPackage) {
    package.file_descriptors.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    package.attachments.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    package.dependency_references.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
    });
    package
        .allowed_sidecars
        .sort_by_key(|kind| match kind.as_str() {
            "audit" => 0,
            "gold" => 1,
            "strategy" => 2,
            "signature" => 3,
            "relation" => 4,
            "escrow" => 5,
            _ => usize::MAX,
        });
    package
        .deployment_projections
        .sort_by(|left, right| left.kind.cmp(&right.kind));
    package
        .lookup_entries
        .sort_by(|left, right| left.input.cmp(&right.input));
    package.identity.identity_exclusions.sort();
}

fn canonical_json_bytes<T: Serialize>(value: &T, context: &str) -> TemporalResult<Vec<u8>> {
    serde_json::to_vec(value)
        .map_err(|error| artifact_contract_error(format!("failed to serialize {context}: {error}")))
}

fn canonical_timestamp(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    let parsed = DateTime::parse_from_rfc3339(&normalized).map_err(|error| {
        artifact_contract_error(format!("{field} must be RFC3339 timestamp: {error}"))
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn normalized_non_empty(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(artifact_contract_error(format!(
            "{field} must be non-empty after trimming"
        )));
    }
    Ok(normalized.to_string())
}

fn normalized_hash(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = normalized_non_empty(value, field)?;
    if normalized.starts_with("blake3:") && normalized.len() == "blake3:".len() + 64 {
        return Ok(normalized);
    }
    Err(artifact_contract_error(format!(
        "{field} must be a blake3: digest with 64 lowercase hex characters"
    )))
}

fn normalize_relative_path(value: &str, field: &str) -> TemporalResult<String> {
    let normalized = normalized_non_empty(value, field)?.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(artifact_contract_error(format!(
            "{field} must be a relative path"
        )));
    }
    let segments = normalized.split('/').map(str::trim).collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(artifact_contract_error(format!(
            "{field} must not traverse directories"
        )));
    }
    Ok(segments.join("/"))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn artifact_contract_error(message: impl Into<String>) -> TemporalError {
    TemporalError {
        code: TemporalErrorCode::ArtifactContract,
        message: message.into(),
    }
}
