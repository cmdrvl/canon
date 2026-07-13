#![forbid(unsafe_code)]

//! Entity index artifact and cache-key contract.
//!
//! The index stage is an accelerator over prepared unique surfaces. This module
//! defines only the persisted artifact shell and cache validation contract; it
//! does not build postings or make identity decisions.

use crate::{
    Refusal,
    entity::{
        CANON_ENTITY_INDEX_VERSION, CANON_ENTITY_PREPARE_VERSION, EntityArtifactMetadata,
        EntityArtifactReference, EntityCacheKeyMaterial, EntityDeterministicSummary,
        EntityProfileDocument, EntityStrategyReference,
        artifact_chain::{EntityCacheDecision, EntityHashField},
        cache::{EntityCacheInvalidation, EntityCacheKey, EntityCacheLayer, compare_cache_keys},
        contracts::EntityArtifactHeader,
        error::EntityRefusalKind,
        index::ngram_index::{EntityNgramBuildConfig, EntityNgramIndex, EntityNgramSurface},
        index_io::{
            EntityIndexDiagnosticRecord, EntityIndexDiskBundle, EntityIndexDiskPaths,
            EntityIndexPersistRequest, EntityIndexPostingsBundle, INDEX_ARTIFACT_FILE,
            INDEX_CACHE_KEY_FILE, index_cache_file_exists, read_index_artifact_for_cache,
            read_index_disk_bundle, write_index_disk_bundle,
        },
        postings::{EntityPostingBuildConfig, EntityPostingIndex, EntityPostingSurface},
        prepare::{PrepareRegistrySnapshot, PrepareRunArtifact, PreparedSurfaceRecord},
    },
    namekit::ngram::NgramConfig,
    witness,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::BufRead,
    path::{Component, Path, PathBuf},
};

const BUILTIN_CMBS_TENANT_LABEL_PROFILE: &str =
    include_str!("../../tests/fixtures/entity/profiles/cmbs_tenant_label.yaml");
const BUILTIN_REGAB_FIRM_IDENTITY_PROFILE: &str =
    include_str!("../../tests/fixtures/entity/profiles/regab_firm_identity.yaml");

pub const DEFAULT_INDEX_COMMON_POSTING_LIMIT: usize = 100;
pub const DEFAULT_INDEX_NGRAM_WIDTH: usize = 3;
pub const DEFAULT_INDEX_POSTINGS_PATH: &str = "index/postings.json";
pub const DEFAULT_INDEX_DIAGNOSTICS_PATH: &str = "index/diagnostics.jsonl";
pub const PREPARE_ARTIFACT_PATH: &str = "prepare/prepare.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityIndexCacheMode {
    #[default]
    Enabled,
    Disabled,
}

impl EntityIndexCacheMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityIndexCacheStatus {
    Hit,
    Miss,
    Rebuilt,
    Bypassed,
}

impl EntityIndexCacheStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Rebuilt => "rebuilt",
            Self::Bypassed => "bypassed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityIndexCachePolicy {
    RebuildOnMiss,
    RefuseOnMiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIndexArtifact {
    pub version: String,
    pub artifact_content_hash: String,
    pub metadata: EntityArtifactMetadata,
    pub prepare_hash: String,
    pub summary: EntityDeterministicSummary,
    pub postings_path: String,
    pub diagnostics_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexArtifactRequest {
    pub prepare: EntityArtifactHeader,
    pub strategy: EntityStrategyReference,
    pub cache_status: EntityIndexCacheStatus,
    pub postings_path: String,
    pub diagnostics_path: String,
    pub counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityIndexBuildRequest<'a> {
    pub rows: &'a Path,
    pub profile: &'a str,
    pub strategy: &'a Path,
    pub registry: &'a Path,
    pub work_dir: &'a Path,
    pub max_artifact_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityIndexBuildReport {
    pub version: String,
    pub artifact: EntityIndexArtifact,
    pub cache_status: EntityIndexCacheStatus,
    pub cache: EntityIndexBuildCacheReport,
    pub paths: EntityIndexBuildPaths,
    pub next_command: String,
    pub next_commands: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityIndexBuildCacheReport {
    pub decision: EntityCacheDecision,
    pub layer: EntityCacheLayer,
    pub changed_fields: Vec<String>,
    pub invalidated_layers: Vec<EntityCacheLayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityIndexBuildPaths {
    pub artifact: String,
    pub cache_key: String,
    pub postings: String,
    pub diagnostics: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityIndexBuildResult {
    pub artifact: EntityIndexArtifact,
    pub cache_key: EntityCacheKey,
    pub cache_status: EntityIndexCacheStatus,
    pub cache_invalidation: EntityCacheInvalidation,
    pub diagnostics: Vec<EntityIndexDiagnosticRecord>,
    pub paths: EntityIndexDiskPaths,
    pub next_commands: BTreeMap<String, String>,
}

pub fn build_index_artifact_contract(
    request: EntityIndexArtifactRequest,
) -> Result<EntityIndexArtifact, Refusal> {
    validate_prepare_header(&request.prepare)?;
    let prepare_hash = request.prepare.metadata.artifact_content_hash.clone();
    let metadata = index_metadata_from_prepare(&request.prepare, request.strategy)?;
    let mut labels = BTreeMap::from([(
        "cache_status".to_string(),
        request.cache_status.as_str().to_string(),
    )]);
    labels.insert(
        "upstream_version".to_string(),
        request.prepare.version.clone(),
    );

    let mut artifact = EntityIndexArtifact {
        version: CANON_ENTITY_INDEX_VERSION.to_string(),
        artifact_content_hash: String::new(),
        metadata,
        prepare_hash,
        summary: EntityDeterministicSummary {
            counts: request.counts,
            labels,
        },
        postings_path: request.postings_path,
        diagnostics_path: request.diagnostics_path,
    };
    artifact.artifact_content_hash = hash_index_artifact_without_self(&artifact)?;
    artifact.metadata.artifact_content_hash = artifact.artifact_content_hash.clone();
    Ok(artifact)
}

pub fn run_index_build(
    request: EntityIndexBuildRequest<'_>,
) -> Result<EntityIndexBuildResult, Refusal> {
    let prepare = load_prepare_artifact(request.work_dir)?;
    validate_prepare_artifact_hash(&prepare)?;
    validate_prepare_context(&prepare, request)?;

    let prepare_header = prepare_header(&prepare);
    let strategy = load_index_strategy_reference(request.profile, request.strategy)?;
    let current_cache_key = index_cache_key_from_prepare_header(
        EntityCacheLayer::NgramPostings,
        &prepare_header,
        &strategy,
    )?;

    if let Some(bundle) = read_verified_cache_if_present(
        request.work_dir,
        &current_cache_key,
        request.max_artifact_bytes,
    )? {
        let cache_invalidation = compare_cache_keys(&bundle.cache_key, &current_cache_key);
        return Ok(EntityIndexBuildResult {
            artifact: bundle.artifact,
            cache_key: bundle.cache_key,
            cache_status: EntityIndexCacheStatus::Hit,
            cache_invalidation,
            diagnostics: bundle.diagnostics,
            paths: bundle.paths,
            next_commands: next_commands(request),
        });
    }

    let surfaces = read_prepare_surfaces(request.work_dir, &prepare)?;
    validate_prepare_surfaces(&surfaces, &prepare)?;
    let postings = EntityPostingIndex::build(
        &posting_surfaces(&surfaces),
        EntityPostingBuildConfig {
            common_posting_limit: DEFAULT_INDEX_COMMON_POSTING_LIMIT,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build entity postings index",
            json!({
                "stage": "index",
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
            Some(next_index_command(request)),
        )
    })?;
    let ngrams = EntityNgramIndex::build(
        &ngram_surfaces(&surfaces),
        EntityNgramBuildConfig {
            ngram: NgramConfig::new(DEFAULT_INDEX_NGRAM_WIDTH)
                .expect("default ngram width is valid"),
            common_posting_limit: DEFAULT_INDEX_COMMON_POSTING_LIMIT,
        },
    )
    .map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to build entity ngram index",
            json!({
                "stage": "index",
                "error": format!("{error:?}"),
                "writes_performed": false
            }),
            Some(next_index_command(request)),
        )
    })?;
    let posting_diagnostics = postings.diagnostics.clone();
    let ngram_diagnostics = ngrams.diagnostics.clone();
    let artifact = build_index_artifact_contract(EntityIndexArtifactRequest {
        prepare: prepare_header,
        strategy,
        cache_status: EntityIndexCacheStatus::Rebuilt,
        postings_path: DEFAULT_INDEX_POSTINGS_PATH.to_string(),
        diagnostics_path: DEFAULT_INDEX_DIAGNOSTICS_PATH.to_string(),
        counts: index_summary_counts(
            u64::from(posting_diagnostics.surface_count),
            posting_diagnostics.token_count as u64,
            ngram_diagnostics.ngram_count as u64,
            (posting_diagnostics.large_exact_view_bucket_count
                + posting_diagnostics.common_token_count
                + ngram_diagnostics.common_ngram_count) as u64,
        ),
    })?;
    let diagnostics = index_diagnostics(&artifact, &posting_diagnostics, &ngram_diagnostics);
    let paths = write_index_disk_bundle(
        request.work_dir,
        EntityIndexPersistRequest {
            artifact: artifact.clone(),
            cache_key: current_cache_key.clone(),
            postings: EntityIndexPostingsBundle::new(postings, Some(ngrams)),
            diagnostics: diagnostics.clone(),
            max_artifact_bytes: request.max_artifact_bytes,
        },
    )?;

    Ok(EntityIndexBuildResult {
        artifact,
        cache_key: current_cache_key,
        cache_status: EntityIndexCacheStatus::Rebuilt,
        cache_invalidation: EntityCacheInvalidation {
            layer: EntityCacheLayer::NgramPostings,
            decision: EntityCacheDecision::Miss,
            changed_fields: Vec::new(),
            invalidated_layers: Vec::new(),
        },
        diagnostics,
        paths,
        next_commands: next_commands(request),
    })
}

pub fn index_build_report(result: &EntityIndexBuildResult) -> EntityIndexBuildReport {
    let next_command = result
        .next_commands
        .get("block")
        .cloned()
        .unwrap_or_default();
    EntityIndexBuildReport {
        version: "canon_entity_index_build.v0".to_string(),
        artifact: result.artifact.clone(),
        cache_status: result.cache_status,
        cache: EntityIndexBuildCacheReport {
            decision: result.cache_invalidation.decision,
            layer: result.cache_invalidation.layer,
            changed_fields: result
                .cache_invalidation
                .changed_fields
                .iter()
                .map(|field| field.as_str().to_string())
                .collect(),
            invalidated_layers: result.cache_invalidation.invalidated_layers.clone(),
        },
        paths: EntityIndexBuildPaths {
            artifact: result.paths.artifact_path.display().to_string(),
            cache_key: result.paths.cache_key_path.display().to_string(),
            postings: result.paths.postings_path.display().to_string(),
            diagnostics: result.paths.diagnostics_path.display().to_string(),
        },
        next_command,
        next_commands: result.next_commands.clone(),
    }
}

pub fn validate_index_artifact_hash(artifact: &EntityIndexArtifact) -> Result<(), Refusal> {
    let expected = hash_index_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index artifact content hash does not match its payload",
            json!({
                "stage": "index",
                "field": "artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Reload the matching index artifact or rebuild canon entity index".to_string()),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index metadata hash does not match artifact hash",
            json!({
                "stage": "index",
                "field": "metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Reload the matching index artifact or rebuild canon entity index".to_string()),
        ));
    }
    Ok(())
}

pub fn index_cache_key_from_prepare_header(
    layer: EntityCacheLayer,
    prepare: &EntityArtifactHeader,
    index_strategy: &EntityStrategyReference,
) -> Result<EntityCacheKey, Refusal> {
    validate_prepare_header(prepare)?;
    let metadata = &prepare.metadata;
    let profile_hash = metadata
        .profile
        .content_hash
        .clone()
        .ok_or_else(|| missing_prepare_metadata("metadata.profile.content_hash"))?;
    let input = metadata
        .input
        .as_ref()
        .ok_or_else(|| missing_prepare_metadata("metadata.input"))?;
    let patch_hash = metadata
        .patch_set
        .as_ref()
        .map(|patch| patch.content_hash.clone());
    let namekit = metadata
        .namekit
        .as_ref()
        .ok_or_else(|| missing_prepare_metadata("metadata.namekit"))?;

    Ok(EntityCacheKey::from_i21_material(
        layer,
        EntityCacheKeyMaterial {
            input_hash: input.content_hash.clone(),
            profile_hash,
            strategy_hash: index_strategy.content_hash.clone(),
            registry_snapshot_hash: metadata.registry_snapshot.lookup_snapshot_hash.clone(),
            patch_hash,
            namekit_version: namekit.version.clone(),
            namekit_hash: Some(namekit.content_hash.clone()),
        },
    )
    .with_upstream_artifact_hash(metadata.artifact_content_hash.clone()))
}

pub fn validate_index_cache_policy(
    cached: &EntityCacheKey,
    current: &EntityCacheKey,
    policy: EntityIndexCachePolicy,
) -> Result<EntityCacheInvalidation, Refusal> {
    let invalidation = compare_cache_keys(cached, current);
    if invalidation.decision == EntityCacheDecision::Miss
        && policy == EntityIndexCachePolicy::RefuseOnMiss
    {
        return Err(cache_mismatch_refusal(&invalidation));
    }
    Ok(invalidation)
}

pub fn validate_index_artifact_contract(artifact: &EntityIndexArtifact) -> Result<(), Refusal> {
    if artifact.version != CANON_ENTITY_INDEX_VERSION {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index artifact version mismatch",
            json!({
                "expected": CANON_ENTITY_INDEX_VERSION,
                "actual": artifact.version,
                "stage": "index",
                "writes_performed": false
            }),
            Some("Use a matching index artifact or rerun canon entity index build".to_string()),
        ));
    }
    if artifact.artifact_content_hash.trim().is_empty() {
        return Err(missing_prepare_metadata("artifact_content_hash"));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index artifact metadata hash does not match artifact hash",
            json!({
                "stage": "index",
                "artifact_content_hash": artifact.artifact_content_hash,
                "metadata_artifact_content_hash": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Use a complete index artifact or rerun canon entity index build".to_string()),
        ));
    }
    let expected = hash_index_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index artifact content hash mismatch",
            json!({
                "stage": "index",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Discard the stale index artifact and rerun canon entity index build".to_string()),
        ));
    }
    Ok(())
}

fn index_metadata_from_prepare(
    prepare: &EntityArtifactHeader,
    strategy: EntityStrategyReference,
) -> Result<EntityArtifactMetadata, Refusal> {
    validate_prepare_header(prepare)?;
    let metadata = &prepare.metadata;
    Ok(EntityArtifactMetadata {
        profile: metadata.profile.clone(),
        strategy,
        registry_snapshot: metadata.registry_snapshot.clone(),
        patch_namespace: metadata.patch_namespace.clone(),
        input: metadata.input.clone(),
        upstream_artifacts: vec![EntityArtifactReference {
            version: prepare.version.clone(),
            content_hash: metadata.artifact_content_hash.clone(),
        }],
        patch_set: metadata.patch_set.clone(),
        namekit: metadata.namekit.clone(),
        artifact_content_hash: String::new(),
    })
}

fn load_prepare_artifact(work_dir: &Path) -> Result<PrepareRunArtifact, Refusal> {
    read_json_file(
        &work_dir.join(PREPARE_ARTIFACT_PATH),
        "entity prepare artifact",
    )
}

pub(crate) fn read_verified_cache_if_present(
    work_dir: &Path,
    current_cache_key: &EntityCacheKey,
    max_artifact_bytes: Option<u64>,
) -> Result<Option<EntityIndexDiskBundle>, Refusal> {
    let artifact_path = work_dir.join(INDEX_ARTIFACT_FILE);
    let cache_key_path = work_dir.join(INDEX_CACHE_KEY_FILE);
    let receipt_path = work_dir.join(crate::entity::index_io::INDEX_CACHE_RECEIPT_FILE);
    let artifact_exists =
        index_cache_file_exists(work_dir, &artifact_path, "entity index artifact")?;
    let cache_key_exists = index_cache_file_exists(work_dir, &cache_key_path, "index cache key")?;
    let receipt_exists = index_cache_file_exists(work_dir, &receipt_path, "index cache receipt")?;

    if !artifact_exists && !cache_key_exists && !receipt_exists {
        return Ok(None);
    }
    if !(artifact_exists && cache_key_exists && receipt_exists) {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index cache is incomplete",
            json!({
                "stage": "index",
                "artifact_path": artifact_path.display().to_string(),
                "cache_key_path": cache_key_path.display().to_string(),
                "receipt_path": receipt_path.display().to_string(),
                "artifact_exists": artifact_exists,
                "cache_key_exists": cache_key_exists,
                "receipt_exists": receipt_exists,
                "writes_performed": false
            }),
            Some(
                "Remove the incomplete cache outside canon or use a fresh work directory"
                    .to_string(),
            ),
        ));
    }

    let existing_artifact = read_index_artifact_for_cache(work_dir, max_artifact_bytes)?;
    let receipt = crate::entity::index_io::read_index_cache_receipt(
        work_dir,
        &existing_artifact,
        max_artifact_bytes,
    )?;
    if receipt.receipt.mode != EntityIndexCacheMode::Enabled || !receipt.receipt.reusable {
        return Ok(None);
    }
    let mut bundle = read_index_disk_bundle(
        work_dir,
        &existing_artifact,
        current_cache_key,
        max_artifact_bytes,
    )?;
    bundle.receipt = crate::entity::index_io::write_index_cache_receipt(
        work_dir,
        &bundle.artifact,
        EntityIndexCacheMode::Enabled,
        EntityIndexCacheStatus::Hit,
        true,
        max_artifact_bytes,
    )?;
    Ok(Some(bundle))
}

fn validate_prepare_artifact_hash(artifact: &PrepareRunArtifact) -> Result<(), Refusal> {
    if artifact.version != CANON_ENTITY_PREPARE_VERSION {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index requires a canon_entity_prepare.v0 artifact",
            json!({
                "stage": "index",
                "field": "prepare.version",
                "expected": CANON_ENTITY_PREPARE_VERSION,
                "actual": artifact.version,
                "writes_performed": false
            }),
            Some("Rerun canon entity prepare before building the index".to_string()),
        ));
    }
    if artifact.artifact_content_hash.trim().is_empty() {
        return Err(missing_prepare_metadata("artifact_content_hash"));
    }
    let expected = hash_prepare_artifact_without_self(artifact)?;
    if artifact.artifact_content_hash != expected {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity prepare artifact content hash does not match its payload",
            json!({
                "stage": "index",
                "field": "prepare.artifact_content_hash",
                "expected": expected,
                "actual": artifact.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Reload the matching prepare artifact or rerun canon entity prepare".to_string()),
        ));
    }
    if artifact.metadata.artifact_content_hash != artifact.artifact_content_hash {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity prepare metadata hash does not match artifact hash",
            json!({
                "stage": "index",
                "field": "prepare.metadata.artifact_content_hash",
                "expected": artifact.artifact_content_hash,
                "actual": artifact.metadata.artifact_content_hash,
                "writes_performed": false
            }),
            Some("Reload the matching prepare artifact or rerun canon entity prepare".to_string()),
        ));
    }
    Ok(())
}

fn validate_prepare_context(
    prepare: &PrepareRunArtifact,
    request: EntityIndexBuildRequest<'_>,
) -> Result<(), Refusal> {
    let current_profile = load_profile_reference(request.profile)?;
    require_equal(
        EntityRefusalKind::Profile,
        "Prepared artifact profile does not match current profile",
        "metadata.profile.id",
        &prepare.metadata.profile.id,
        &current_profile.id,
    )?;
    require_equal(
        EntityRefusalKind::Profile,
        "Prepared artifact profile version does not match current profile",
        "metadata.profile.version",
        &prepare.metadata.profile.version,
        &current_profile.version,
    )?;
    require_equal_optional(
        EntityRefusalKind::Profile,
        "Prepared artifact profile hash does not match current profile",
        "metadata.profile.content_hash",
        prepare.metadata.profile.content_hash.as_deref(),
        current_profile.content_hash.as_deref(),
    )?;

    let expected_patch_hash =
        witness::hash_bytes(current_profile.patch_namespaces.aliases.as_bytes());
    let actual_patch_hash = prepare
        .metadata
        .patch_set
        .as_ref()
        .map(|patch| patch.content_hash.as_str());
    require_equal_optional(
        EntityRefusalKind::PatchConflict,
        "Prepared artifact patch namespace hash does not match current profile",
        "metadata.patch_set.content_hash",
        actual_patch_hash,
        Some(expected_patch_hash.as_str()),
    )?;

    let input = prepare
        .metadata
        .input
        .as_ref()
        .ok_or_else(|| missing_prepare_metadata("metadata.input"))?;
    let current_input_hash = witness::hash_file(request.rows).map_err(|error| {
        EntityRefusalKind::IoBudget.to_refusal(
            "Failed to hash entity index input rows",
            json!({
                "stage": "index",
                "path": request.rows.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some(next_index_command(request)),
        )
    })?;
    require_equal(
        EntityRefusalKind::InputContract,
        "Prepared artifact input hash does not match current rows",
        "metadata.input.content_hash",
        &input.content_hash,
        &current_input_hash,
    )?;

    let current_registry = load_registry_snapshot(request.registry)?;
    require_equal(
        EntityRefusalKind::RegistrySnapshot,
        "Prepared artifact registry id does not match current registry",
        "metadata.registry_snapshot.id",
        &prepare.metadata.registry_snapshot.id,
        &current_registry.id,
    )?;
    require_equal(
        EntityRefusalKind::RegistrySnapshot,
        "Prepared artifact registry version does not match current registry",
        "metadata.registry_snapshot.version",
        &prepare.metadata.registry_snapshot.version,
        &current_registry.version,
    )?;
    require_equal(
        EntityRefusalKind::RegistrySnapshot,
        "Prepared artifact registry hash does not match current registry",
        "metadata.registry_snapshot.lookup_snapshot_hash",
        &prepare.metadata.registry_snapshot.lookup_snapshot_hash,
        &current_registry.lookup_snapshot_hash,
    )
}

fn load_profile_reference(profile: &str) -> Result<crate::entity::EntityProfileReference, Refusal> {
    let profile_source = if Path::new(profile).exists() {
        fs::read_to_string(profile).map_err(|error| {
            EntityRefusalKind::Profile.to_refusal(
                "Failed to read entity index profile",
                json!({
                    "stage": "index",
                    "profile": profile,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?
    } else {
        match profile {
            "cmbs_tenant_label" => BUILTIN_CMBS_TENANT_LABEL_PROFILE.to_string(),
            "regab_firm_identity" => BUILTIN_REGAB_FIRM_IDENTITY_PROFILE.to_string(),
            _ => {
                return Err(EntityRefusalKind::Profile.to_refusal(
                    "Unknown entity index profile",
                    json!({
                        "stage": "index",
                        "profile": profile,
                        "available_profiles": ["cmbs_tenant_label", "regab_firm_identity"],
                        "writes_performed": false
                    }),
                    None,
                ));
            }
        }
    };
    let document = EntityProfileDocument::from_yaml_str(&profile_source)
        .map_err(|error| error.to_refusal())?;
    let mut reference = document.to_reference();
    reference.content_hash = Some(witness::hash_bytes(profile_source.as_bytes()));
    Ok(reference)
}

fn load_registry_snapshot(registry_dir: &Path) -> Result<PrepareRegistrySnapshot, Refusal> {
    #[derive(Deserialize)]
    struct RegistryJsonLite {
        id: String,
        version: String,
    }

    let registry_json_path = registry_dir.join("registry.json");
    let registry_json_bytes = fs::read(&registry_json_path).map_err(|error| {
        EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Failed to read entity registry snapshot metadata",
            json!({
                "stage": "index",
                "registry": registry_dir.display().to_string(),
                "path": registry_json_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    let registry: RegistryJsonLite =
        serde_json::from_slice(&registry_json_bytes).map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to parse entity registry snapshot metadata",
                json!({
                    "stage": "index",
                    "registry": registry_dir.display().to_string(),
                    "path": registry_json_path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?;

    Ok(PrepareRegistrySnapshot {
        id: registry.id,
        version: registry.version,
        source: registry_dir.display().to_string(),
        lookup_snapshot_hash: hash_registry_json_files(registry_dir)?,
    })
}

fn hash_registry_json_files(registry_dir: &Path) -> Result<String, Refusal> {
    let mut files = Vec::new();
    for entry in fs::read_dir(registry_dir).map_err(|error| {
        EntityRefusalKind::RegistrySnapshot.to_refusal(
            "Failed to read entity registry directory",
            json!({
                "stage": "index",
                "registry": registry_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })? {
        let entry = entry.map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to inspect entity registry directory entry",
                json!({
                    "stage": "index",
                    "registry": registry_dir.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?;
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            files.push(path);
        }
    }
    files.sort();

    let mut hasher = blake3::Hasher::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        let bytes = fs::read(&path).map_err(|error| {
            EntityRefusalKind::RegistrySnapshot.to_refusal(
                "Failed to hash entity registry snapshot file",
                json!({
                    "stage": "index",
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                None,
            )
        })?;
        hasher.update(&bytes);
        hasher.update(&[0]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn load_index_strategy_reference(
    profile: &str,
    strategy_path: &Path,
) -> Result<EntityStrategyReference, Refusal> {
    let bytes = fs::read(strategy_path).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Failed to read entity index strategy",
            json!({
                "stage": "index",
                "path": strategy_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    let content_hash = witness::hash_bytes(&bytes);
    let value = serde_yaml::from_slice::<serde_yaml::Value>(&bytes).map_err(|error| {
        EntityRefusalKind::Strategy.to_refusal(
            "Invalid entity index strategy YAML",
            json!({
                "stage": "index",
                "path": strategy_path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    let id = yaml_string(&value, "strategy_id")
        .or_else(|| yaml_string(&value, "profile"))
        .unwrap_or_else(|| profile.to_string());
    let version = yaml_string(&value, "strategy_version")
        .or_else(|| yaml_string(&value, "version"))
        .unwrap_or_else(|| "0.0.0".to_string());
    Ok(EntityStrategyReference {
        id: format!("{id}.index"),
        version,
        content_hash: witness::hash_bytes(format!("{content_hash}:index").as_bytes()),
    })
}

fn yaml_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String(key.to_string())))
        .and_then(serde_yaml::Value::as_str)
        .map(ToOwned::to_owned)
}

fn read_prepare_surfaces(
    work_dir: &Path,
    prepare: &PrepareRunArtifact,
) -> Result<Vec<PreparedSurfaceRecord>, Refusal> {
    let path = resolve_safe_relative_path(work_dir, &prepare.surfaces_path, "surfaces_path")?;
    read_jsonl_file(&path, "prepared surfaces")
}

fn validate_prepare_surfaces(
    surfaces: &[PreparedSurfaceRecord],
    prepare: &PrepareRunArtifact,
) -> Result<(), Refusal> {
    let expected_count = prepare.summary.get("prepared_surfaces").copied();
    if expected_count.is_some_and(|count| count != surfaces.len() as u64) {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Prepared surface stream count does not match prepare artifact summary",
            json!({
                "stage": "index",
                "field": "prepared_surfaces",
                "expected": expected_count,
                "actual": surfaces.len(),
                "writes_performed": false
            }),
            Some("Rerun canon entity prepare before building the index".to_string()),
        ));
    }
    for (ordinal, surface) in surfaces.iter().enumerate() {
        if surface.profile_id != prepare.profile.id {
            return Err(EntityRefusalKind::ArtifactContract.to_refusal(
                "Prepared surface profile does not match prepare artifact profile",
                json!({
                    "stage": "index",
                    "field": "surface.profile_id",
                    "surface_ordinal": ordinal,
                    "surface_id": &surface.surface_id,
                    "expected": &prepare.profile.id,
                    "actual": &surface.profile_id,
                    "writes_performed": false
                }),
                Some("Rerun canon entity prepare before building the index".to_string()),
            ));
        }
    }
    Ok(())
}

fn prepare_header(artifact: &PrepareRunArtifact) -> EntityArtifactHeader {
    EntityArtifactHeader {
        version: artifact.version.clone(),
        metadata: artifact.metadata.clone(),
        summary: EntityDeterministicSummary {
            counts: artifact.summary.clone(),
            labels: BTreeMap::from([("stage".to_string(), "prepare".to_string())]),
        },
    }
}

fn posting_surfaces(surfaces: &[PreparedSurfaceRecord]) -> Vec<EntityPostingSurface> {
    surfaces
        .iter()
        .map(|surface| {
            let mut posting = EntityPostingSurface::new(surface.surface_id.clone());
            for (view_name, view) in &surface.normalized_views {
                if !view.value.trim().is_empty() {
                    posting = posting.with_exact_view(view_name.clone(), view.value.clone());
                }
            }
            posting.with_tokens(tokens_for_surface(surface))
        })
        .collect()
}

fn ngram_surfaces(surfaces: &[PreparedSurfaceRecord]) -> Vec<EntityNgramSurface> {
    surfaces
        .iter()
        .map(|surface| {
            EntityNgramSurface::new(
                surface.surface_id.clone(),
                core_view_value(&surface.profile_id, surface),
            )
        })
        .collect()
}

fn tokens_for_surface(surface: &PreparedSurfaceRecord) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for value in surface
        .normalized_views
        .values()
        .map(|view| view.value.as_str())
        .chain(std::iter::once(surface.primary_surface.as_str()))
    {
        for token in value.split_whitespace() {
            let token = token.trim();
            if !token.is_empty() {
                tokens.insert(token.to_string());
            }
        }
    }
    tokens.into_iter().collect()
}

fn core_view_value(profile_id: &str, surface: &PreparedSurfaceRecord) -> String {
    surface
        .normalized_views
        .get(core_view_name(profile_id))
        .or_else(|| surface.normalized_views.values().next())
        .map(|view| view.value.clone())
        .unwrap_or_else(|| surface.primary_surface.trim().to_string())
}

fn core_view_name(profile_id: &str) -> &'static str {
    match profile_id {
        "cmbs_tenant_label" => "tenant_core",
        "regab_firm_identity" => "firm_core",
        _ => "core",
    }
}

fn index_diagnostics(
    artifact: &EntityIndexArtifact,
    postings: &crate::entity::postings::EntityPostingDiagnostics,
    ngrams: &crate::entity::index::ngram_index::EntityNgramDiagnostics,
) -> Vec<EntityIndexDiagnosticRecord> {
    let mut summary = EntityIndexDiagnosticRecord::new("artifact_summary");
    summary.counts = artifact.summary.counts.clone();
    summary.labels = artifact.summary.labels.clone();

    let mut posting = EntityIndexDiagnosticRecord::new("posting_summary");
    posting.counts = BTreeMap::from([
        (
            "surface_count".to_string(),
            u64::from(postings.surface_count),
        ),
        ("token_count".to_string(), postings.token_count as u64),
        (
            "common_token_count".to_string(),
            postings.common_token_count as u64,
        ),
        (
            "large_exact_view_bucket_count".to_string(),
            postings.large_exact_view_bucket_count as u64,
        ),
    ]);

    let mut ngram = EntityIndexDiagnosticRecord::new("ngram_summary");
    ngram.counts = BTreeMap::from([
        ("ngram_count".to_string(), ngrams.ngram_count as u64),
        (
            "common_ngram_count".to_string(),
            ngrams.common_ngram_count as u64,
        ),
    ]);

    vec![summary, posting, ngram]
}

fn validate_prepare_header(prepare: &EntityArtifactHeader) -> Result<(), Refusal> {
    if prepare.version != CANON_ENTITY_PREPARE_VERSION {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity index requires a canon_entity_prepare.v0 artifact",
            json!({
                "expected": CANON_ENTITY_PREPARE_VERSION,
                "actual": prepare.version,
                "stage": "index",
                "writes_performed": false
            }),
            Some("Use a matching prepare artifact or rerun canon entity prepare".to_string()),
        ));
    }
    if prepare.metadata.artifact_content_hash.trim().is_empty() {
        return Err(missing_prepare_metadata("metadata.artifact_content_hash"));
    }
    Ok(())
}

fn missing_prepare_metadata(field: &str) -> Refusal {
    EntityRefusalKind::ArtifactContract.to_refusal(
        "Entity prepare artifact is missing index cache metadata",
        json!({
            "field": field,
            "stage": "index",
            "writes_performed": false
        }),
        Some("Use a complete prepare artifact or rerun canon entity prepare".to_string()),
    )
}

fn cache_mismatch_refusal(invalidation: &EntityCacheInvalidation) -> Refusal {
    EntityRefusalKind::CacheMismatch.to_refusal(
        "Entity index cache key does not match current artifact inputs",
        json!({
            "stage": "index",
            "layer": invalidation.layer,
            "decision": invalidation.decision,
            "changed_fields": invalidation
                .changed_fields
                .iter()
                .map(|field| field.as_str())
                .collect::<Vec<_>>(),
            "invalidated_layers": invalidation.invalidated_layers,
            "writes_performed": false
        }),
        Some("Rebuild the entity index cache or use the matching prepare artifact".to_string()),
    )
}

fn read_json_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, Refusal> {
    let bytes = fs::read(path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to read entity index input artifact",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Run the prerequisite entity stage before building the index".to_string()),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to parse entity index input artifact",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Run the prerequisite entity stage before building the index".to_string()),
        )
    })
}

fn read_jsonl_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<Vec<T>, Refusal> {
    let file = fs::File::open(path).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to read entity index JSONL artifact",
            json!({
                "stage": "index",
                "artifact": label,
                "path": path.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false
            }),
            Some("Run the prerequisite entity stage before building the index".to_string()),
        )
    })?;
    let reader = std::io::BufReader::new(file);
    let mut rows = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to read entity index JSONL artifact line",
                json!({
                    "stage": "index",
                    "artifact": label,
                    "path": path.display().to_string(),
                    "line": line_index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some("Run the prerequisite entity stage before building the index".to_string()),
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(serde_json::from_str(&line).map_err(|error| {
            EntityRefusalKind::ArtifactContract.to_refusal(
                "Failed to parse entity index JSONL artifact record",
                json!({
                    "stage": "index",
                    "artifact": label,
                    "path": path.display().to_string(),
                    "line": line_index + 1,
                    "error": error.to_string(),
                    "writes_performed": false
                }),
                Some("Run the prerequisite entity stage before building the index".to_string()),
            )
        })?);
    }
    Ok(rows)
}

fn resolve_safe_relative_path(
    work_dir: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, Refusal> {
    let path = Path::new(relative);
    if relative.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(EntityRefusalKind::ArtifactContract.to_refusal(
            "Entity prepare artifact path must be a safe relative path",
            json!({
                "stage": "index",
                "field": field,
                "path": relative,
                "writes_performed": false
            }),
            Some("Rerun canon entity prepare before building the index".to_string()),
        ));
    }
    Ok(work_dir.join(path))
}

fn hash_prepare_artifact_without_self(artifact: &PrepareRunArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity prepare artifact",
            json!({
                "stage": "index",
                "error": error.to_string(),
                "writes_performed": false
            }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

fn require_equal(
    kind: EntityRefusalKind,
    message: &str,
    field: &str,
    expected: &str,
    actual: &str,
) -> Result<(), Refusal> {
    if expected == actual {
        return Ok(());
    }
    Err(context_mismatch_refusal(
        kind,
        message,
        field,
        Value::String(expected.to_string()),
        Value::String(actual.to_string()),
    ))
}

fn require_equal_optional(
    kind: EntityRefusalKind,
    message: &str,
    field: &str,
    expected: Option<&str>,
    actual: Option<&str>,
) -> Result<(), Refusal> {
    if expected == actual {
        return Ok(());
    }
    Err(context_mismatch_refusal(
        kind,
        message,
        field,
        expected
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        actual
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    ))
}

fn context_mismatch_refusal(
    kind: EntityRefusalKind,
    message: &str,
    field: &str,
    expected: Value,
    actual: Value,
) -> Refusal {
    kind.to_refusal(
        message,
        json!({
            "stage": "index",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false
        }),
        Some(
            "Rerun canon entity prepare with the current inputs before building the index"
                .to_string(),
        ),
    )
}

fn next_commands(request: EntityIndexBuildRequest<'_>) -> BTreeMap<String, String> {
    BTreeMap::from([("block".to_string(), next_block_command(request))])
}

fn next_index_command(request: EntityIndexBuildRequest<'_>) -> String {
    format!(
        "canon entity index build {} --profile {} --strategy {} --registry {} --work-dir {}",
        request.rows.display(),
        request.profile,
        request.strategy.display(),
        request.registry.display(),
        request.work_dir.display()
    )
}

fn next_block_command(request: EntityIndexBuildRequest<'_>) -> String {
    format!(
        "canon entity block {} --profile {} --strategy {} --registry {} --work-dir {}",
        request.rows.display(),
        request.profile,
        request.strategy.display(),
        request.registry.display(),
        request.work_dir.display()
    )
}

fn hash_index_artifact_without_self(artifact: &EntityIndexArtifact) -> Result<String, Refusal> {
    let mut hashable = artifact.clone();
    hashable.artifact_content_hash.clear();
    hashable.metadata.artifact_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EntityRefusalKind::ArtifactContract.to_refusal(
            "Failed to hash entity index artifact",
            json!({ "error": error.to_string(), "stage": "index" }),
            None,
        )
    })?;
    Ok(witness::hash_bytes(&bytes))
}

pub fn index_summary_counts(
    surface_count: u64,
    token_count: u64,
    ngram_count: u64,
    large_bucket_count: u64,
) -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("surface_count".to_string(), surface_count),
        ("token_count".to_string(), token_count),
        ("ngram_count".to_string(), ngram_count),
        ("large_bucket_count".to_string(), large_bucket_count),
    ])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityNativeIndexScaleReport {
    pub surface_count: u64,
    pub token_count: u64,
    pub ngram_count: u64,
    pub tfidf_term_count: u64,
    pub total_ngram_posting_count: u64,
    pub large_bucket_count: u64,
    pub largest_exact_view_bucket_size: u64,
    pub largest_token_posting_size: u64,
    pub largest_ngram_posting_size: u64,
    pub exact_bucket_pair_expansion_count: u64,
    pub suppressed_exact_view_pair_count: u64,
    pub cache_status: EntityIndexCacheStatus,
    pub cache_reusable: bool,
    pub artifact_bytes: u64,
    pub cache_key_content_hash: String,
}

pub fn native_index_scale_report(
    postings: &crate::entity::postings::EntityPostingDiagnostics,
    ngrams: &crate::entity::index::ngram_index::EntityNgramDiagnostics,
    cache_status: EntityIndexCacheStatus,
    artifact_bytes: u64,
    cache_key_material: &str,
) -> EntityNativeIndexScaleReport {
    EntityNativeIndexScaleReport {
        surface_count: u64::from(postings.surface_count),
        token_count: postings.token_count as u64,
        ngram_count: ngrams.ngram_count as u64,
        tfidf_term_count: postings.tfidf_term_count as u64,
        total_ngram_posting_count: ngrams.total_posting_count as u64,
        large_bucket_count: (postings.large_exact_view_bucket_count
            + postings.common_token_count
            + ngrams.common_ngram_count) as u64,
        largest_exact_view_bucket_size: postings.largest_exact_view_bucket_size as u64,
        largest_token_posting_size: postings.largest_token_posting_size as u64,
        largest_ngram_posting_size: ngrams.largest_ngram_posting_size as u64,
        exact_bucket_pair_expansion_count: postings.exact_bucket_pair_expansion_count,
        suppressed_exact_view_pair_count: postings.suppressed_exact_view_pair_count,
        cache_status,
        cache_reusable: matches!(
            cache_status,
            EntityIndexCacheStatus::Hit | EntityIndexCacheStatus::Rebuilt
        ),
        artifact_bytes,
        cache_key_content_hash: witness::hash_bytes(cache_key_material.as_bytes()),
    }
}

pub fn required_index_hash_fields() -> &'static [EntityHashField] {
    &[
        EntityHashField::InputHash,
        EntityHashField::ProfileHash,
        EntityHashField::StrategyHash,
        EntityHashField::RegistrySnapshotHash,
        EntityHashField::UpstreamArtifactHash,
        EntityHashField::PatchHash,
        EntityHashField::NamekitVersion,
        EntityHashField::NamekitHash,
    ]
}

#[cfg(test)]
mod cache_preflight_tests {
    use super::*;

    fn test_cache_key() -> EntityCacheKey {
        EntityCacheKey {
            contract_version: "canon.entity.cache_key.v0".to_string(),
            layer: EntityCacheLayer::NgramPostings,
            input_hash: "blake3:input".to_string(),
            profile_hash: "blake3:profile".to_string(),
            strategy_hash: "blake3:strategy".to_string(),
            registry_snapshot_hash: "blake3:registry".to_string(),
            upstream_artifact_hash: Some("blake3:prepare".to_string()),
            patch_hash: None,
            namekit_version: "test".to_string(),
            namekit_hash: Some("blake3:namekit".to_string()),
        }
    }

    #[cfg(unix)]
    #[test]
    fn standalone_cache_probe_refuses_symlinked_work_dir_before_read() {
        let temp = tempfile::tempdir().expect("tempdir");
        let real_work = temp.path().join("real-work");
        let work_dir = temp.path().join("work-link");
        std::fs::create_dir_all(&real_work).expect("real work dir");
        std::os::unix::fs::symlink(&real_work, &work_dir).expect("work dir symlink");

        let refusal = read_verified_cache_if_present(&work_dir, &test_cache_key(), None)
            .expect_err("symlinked work_dir refuses before cache read");
        assert!(
            refusal.message.contains("symlink") || refusal.detail.to_string().contains("symlink"),
            "unexpected refusal: {refusal:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn standalone_cache_probe_refuses_symlinked_index_artifact_before_read() {
        let temp = tempfile::tempdir().expect("tempdir");
        let work_dir = temp.path().join("work");
        let outside_artifact = temp.path().join("outside-index.json");
        std::fs::create_dir_all(&work_dir).expect("work dir");
        std::fs::write(&outside_artifact, b"{}").expect("outside artifact");
        std::os::unix::fs::symlink(&outside_artifact, work_dir.join(INDEX_ARTIFACT_FILE))
            .expect("index artifact symlink");

        let refusal = read_verified_cache_if_present(&work_dir, &test_cache_key(), None)
            .expect_err("symlinked index artifact refuses before cache read");
        assert!(
            refusal.message.contains("symlink") || refusal.detail.to_string().contains("symlink"),
            "unexpected refusal: {refusal:?}"
        );
    }
}

#[path = "ngram_index.rs"]
pub mod ngram_index;
