use crate::{
    Refusal, RefusalCode,
    entity::evidence_ir::{
        CANON_EVIDENCE_VERSION, EvidenceAuthorityBasis, EvidenceExtension, EvidenceExtensionValue,
        EvidenceKind, EvidenceOperatorRef, EvidencePolicyRef, EvidenceProvenanceRef,
        EvidenceRecord, EvidenceTarget, canonicalize_record,
    },
    entity::publication::{
        EntityPublicationError, EntityPublicationErrorKind, EntityPublicationFileInput,
        EntityPublicationRequest, EntityPublicationSnapshot, EntityPublicationUpstreamRef,
        open_current_stream_generation, publish_stream_patch,
    },
    keys::KeyNamespace,
    registry, registry_lint,
    temporal::relation::{
        CoreEntityTypeClass, DirectedRelationFact, RelationCardinalityConstraints,
        RelationCyclePolicy, RelationEndpoint, RelationEntityTypeRef, RelationIdentityImplication,
        RelationKindRef, RelationOverlapPolicy, RelationProvenance, RelationReview,
        ReviewDisposition, TimeInterval, finalize_relation, finalize_relations,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub mod registries;
pub mod status;
pub mod intake;

pub const SESSION_MANIFEST_VERSION: &str = "canon.entity.session.v1";
pub const SESSION_MANIFEST_STREAM_ID: &str = "entity-session-manifest";
pub const SESSION_INGEST_VERSION: &str = "canon.entity.session.ingest.v1";
pub const SESSION_OBSERVATIONS_STREAM_ID: &str = "entity-session-observations";
pub const SESSION_ASSERT_VERSION: &str = "canon.entity.session.assert.v1";
pub const SESSION_EVIDENCE_STREAM_ID: &str = "entity-session-evidence";
pub const SESSION_COMMIT_VERSION: &str = "canon.entity.session.commit.v1";
pub const SESSION_COMMIT_STREAM_ID: &str = "entity-session-commit";
const SESSION_PUBLICATION_STAGE: &str = "session";
const SESSION_PUBLICATION_VERSION: &str = "canon.entity.session.v1";
const SESSION_EVIDENCE_EXTENSION_SCHEMA: &str = "canon.entity.session.assertion.v1";
const EVIDENCE_VOCABULARY: &str = r#"[{"type":"same_key","kind":"pair_support","target":"pair"},{"type":"same_slot","kind":"pair_support","target":"pair"},{"type":"attribute_agreement","kind":"pair_support","target":"pair"},{"type":"name_equivalent","kind":"pair_support","target":"pair"},{"type":"asserted_alias","kind":"pair_support","target":"pair"},{"type":"distinct","kind":"anti_merge_veto","target":"pair"},{"type":"related","kind":"context_only","target":"pair"}]"#;

#[derive(Debug, Clone)]
pub struct SessionInitRequest {
    pub goal: String,
    pub registry: PathBuf,
    pub work_dir: PathBuf,
    pub entity_kind: String,
    pub registry_id: Option<String>,
    pub prefix: Option<String>,
    pub zero_pad: Option<usize>,
}

/// Inputs for the multi-registry session initializer. The legacy
/// [`SessionInitRequest`] remains the one-kind shorthand; this request is the
/// public API used by `--registries`.
#[derive(Debug, Clone)]
pub struct SessionInitRegistriesRequest {
    pub goal: String,
    pub registries: PathBuf,
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRegistryBinding {
    pub kind: String,
    pub path: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_alias_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_mode: Option<registries::KindIdMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionManifest {
    pub version: String,
    pub session_id: String,
    pub goal: String,
    pub registries: Vec<SessionRegistryBinding>,
    pub evidence_vocabulary_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionInitOutput {
    pub version: String,
    pub status: String,
    pub session_id: String,
    pub goal: String,
    pub work_dir: String,
    pub registries: Vec<SessionRegistryBinding>,
    pub generation_id: String,
    pub registry_initialized: bool,
    pub registry_path_updated: bool,
    pub next_command: String,
}

#[derive(Debug, Clone)]
pub struct SessionIngestRequest {
    pub session: PathBuf,
    pub observations: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionIngestRejection {
    pub line: usize,
    pub obs_id: Option<String>,
    pub code: String,
    pub message: String,
    pub fix: String,
}

/// A key value that failed its namespace check at ingest. The key is dropped
/// from the stored observation; the observation itself is still ingested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionKeyWarning {
    pub line: usize,
    pub obs_id: String,
    pub namespace: String,
    pub value: String,
    pub reason: String,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionIngestOutput {
    pub version: String,
    pub accepted: usize,
    pub already_present: usize,
    pub rejected: Vec<SessionIngestRejection>,
    pub key_warnings: Vec<SessionKeyWarning>,
    pub shard: Option<String>,
    pub generation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionAssertRequest {
    pub session: PathBuf,
    pub evidence: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionAssertRejection {
    pub line: usize,
    pub code: String,
    pub message: String,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionAssertOutput {
    pub version: String,
    pub accepted: usize,
    pub already_present: usize,
    pub rejected: Vec<SessionAssertRejection>,
    pub shard: Option<String>,
    pub generation_id: Option<String>,
}

impl SessionAssertOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "accepted={} already_present={} rejected={} shard={} generation={}",
            self.accepted,
            self.already_present,
            self.rejected.len(),
            self.shard.as_deref().unwrap_or("<none>"),
            self.generation_id.as_deref().unwrap_or("<none>"),
        )
    }
}

#[derive(Debug, Clone)]
pub struct SessionCommitAlias {
    pub alias_file: String,
    pub input: String,
    pub canonical_type: String,
    pub rule_id: String,
    pub skip_if_existing: bool,
    pub session_authority: Option<Value>,
    pub closure: Option<SessionCommitClosure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitClosure {
    pub outcome_ref: String,
    pub closure_hash: String,
    pub manifest: Value,
}

#[derive(Debug, Clone)]
pub struct SessionCommitEntry {
    pub cluster_id: String,
    pub tier: String,
    pub outcome_ref: String,
    pub closure_hash: String,
    pub canonical_id: Option<String>,
    pub aliases: Vec<SessionCommitAlias>,
    pub session_authority: Option<Value>,
    pub closure: Option<SessionCommitClosure>,
}

#[derive(Debug, Clone)]
pub struct SessionCommitRegistryExpectation {
    pub id: String,
    pub version: String,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone)]
pub struct SessionCommitRelationEndpoint {
    pub canonical_id: Option<String>,
    pub cluster_id: Option<String>,
    pub external_id: Option<String>,
    pub entity_type: String,
}

#[derive(Debug, Clone)]
pub struct SessionCommitRelation {
    pub subject: SessionCommitRelationEndpoint,
    pub relation: String,
    pub object: SessionCommitRelationEndpoint,
    pub valid_time: TimeInterval,
    pub evidence_ids: Vec<String>,
}

/// Registry sidecars that a session commit should merge with the current
/// incumbent memory.  The sidecars are deliberately not part of the lookup
/// alias files: they record durable review decisions and pending questions.
#[derive(Debug, Clone, Default)]
pub struct SessionRegistryMemoryWrite {
    pub cannot_link: Vec<crate::entity::runtime::CannotLinkFact>,
    pub pending: Vec<crate::entity::runtime::PendingClusterRecord>,
    pub pending_clear: Vec<String>,
    /// Escrow ids of `pending` records that come from a review pair. Records
    /// parked only for a contradiction are not listed here.
    pub review_pending: Vec<String>,
}

/// Memory a commit must record on its own: a reviewed negative constraint, a
/// cleared pending question, or a pending question that comes from a review
/// pair. A record parked only for a contradiction is already reported by solve
/// and in `skipped[]`; on its own it never makes a commit.
pub(crate) fn memory_is_substantive(memory: &SessionRegistryMemoryWrite) -> bool {
    !memory.cannot_link.is_empty()
        || !memory.pending_clear.is_empty()
        || memory
            .pending
            .iter()
            .any(|record| memory.review_pending.contains(&record.escrow_id))
}

/// A commit has an effect only when it wrote aliases or relations, or memory
/// that stands on its own. Nothing else may be reported as `committed`.
pub(crate) fn commit_has_effect(
    aliases_written: bool,
    relations: usize,
    memory_substantive: bool,
) -> bool {
    aliases_written || relations > 0 || memory_substantive
}

#[derive(Debug, Clone)]
pub struct SessionCommitRequest {
    pub session: PathBuf,
    pub registry_kind: Option<String>,
    pub solve_hash: String,
    pub observations_generation: String,
    pub evidence_generation: Option<String>,
    pub registry: SessionCommitRegistryExpectation,
    pub entries: Vec<SessionCommitEntry>,
    pub skipped: Vec<SessionCommitSkipped>,
    pub relations: Vec<SessionCommitRelation>,
    pub memory: Option<SessionRegistryMemoryWrite>,
    pub next_version: Option<String>,
    pub bump: Option<registry::RegistryVersionBump>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitMinted {
    pub canonical_id: String,
    pub cluster_id: String,
    pub tier: String,
    pub aliases: Vec<registry::RegistryAddEntryAliasEntry>,
    pub outcome_ref: String,
    pub closure_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitAttached {
    pub canonical_id: String,
    pub cluster_id: String,
    pub tier: String,
    pub aliases: Vec<registry::RegistryAddEntryAliasEntry>,
    pub outcome_ref: String,
    pub closure_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitSkipped {
    pub cluster_id: String,
    pub reason: String,
    /// For `key_alias_collision`: the two entries that both claim the string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitRegistry {
    pub id: String,
    pub version_before: String,
    pub version_after: String,
    pub entry_count_before: usize,
    pub entry_count_after: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SessionCommitReceipt {
    version: String,
    solve_hash: String,
    registry: SessionCommitRegistry,
    #[serde(default)]
    registries: Vec<SessionCommitRegistry>,
    minted: Vec<SessionCommitMinted>,
    #[serde(default)]
    attached: Vec<SessionCommitAttached>,
    skipped: Vec<SessionCommitSkipped>,
    #[serde(default)]
    relations_written: usize,
    #[serde(default)]
    cannot_link_written: usize,
    #[serde(default)]
    pending_written: usize,
    #[serde(default)]
    pending_cleared: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    closure_set_hash: Option<String>,
    #[serde(default)]
    closures: Vec<SessionCommitClosure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCommitOutput {
    pub version: String,
    pub outcome: String,
    pub solve_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<SessionCommitRegistry>,
    #[serde(default)]
    pub registries: Vec<SessionCommitRegistry>,
    pub minted: Vec<SessionCommitMinted>,
    pub attached: Vec<SessionCommitAttached>,
    pub skipped: Vec<SessionCommitSkipped>,
    #[serde(default)]
    pub relations_written: usize,
    #[serde(default)]
    pub cannot_link_written: usize,
    #[serde(default)]
    pub pending_written: usize,
    #[serde(default)]
    pub pending_cleared: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closure_set_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

impl SessionCommitOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "outcome={} minted={} attached={} skipped={} relations_written={} cannot_link_written={} pending_written={} pending_cleared={} registry_version={} generation={}",
            self.outcome,
            self.minted.len(),
            self.attached.len(),
            self.skipped.len(),
            self.relations_written,
            self.cannot_link_written,
            self.pending_written,
            self.pending_cleared,
            self.registry
                .as_ref()
                .map(|registry| registry.version_after.as_str())
                .unwrap_or("<none>"),
            self.generation_id.as_deref().unwrap_or("<none>"),
        )
    }
}

impl SessionIngestOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "accepted={} already_present={} rejected={} key_warnings={} shard={} generation={}",
            self.accepted,
            self.already_present,
            self.rejected.len(),
            self.key_warnings.len(),
            self.shard.as_deref().unwrap_or("<none>"),
            self.generation_id.as_deref().unwrap_or("<none>"),
        )
    }
}

impl SessionInitOutput {
    pub fn render_summary(&self) -> String {
        format!(
            "{} | {} | registry={} | generation={} | next={}",
            self.status,
            self.session_id,
            self.registries
                .first()
                .map(|registry| registry.path.as_str())
                .unwrap_or("<none>"),
            self.generation_id,
            self.next_command,
        )
    }
}

pub fn init(request: SessionInitRequest) -> Result<SessionInitOutput, Refusal> {
    let goal = validate_non_empty(&request.work_dir, "--goal", &request.goal)?;
    let entity_kind = validate_non_empty(&request.work_dir, "--entity-kind", &request.entity_kind)?;
    let registry = absolute_path(&request.registry)?;
    let work_dir = absolute_path(&request.work_dir)?;

    ensure_work_dir(&work_dir)?;
    let current = read_current_manifest(&work_dir)?;
    validate_existing_session(
        current.as_ref(),
        &work_dir,
        &goal,
        &entity_kind,
        request.registry_id.as_deref(),
    )?;

    let registry_json = registry.join("registry.json");
    let registry_missing = !path_exists(&registry_json);
    let mut registry_initialized = false;
    let registry_id = if registry_missing {
        let registry_id = request.registry_id.clone().ok_or_else(|| {
            missing_registry_flag_refusal(
                &registry,
                "--registry-id",
                "Session init needs --registry-id when the registry is absent",
            )
        })?;
        let prefix = request.prefix.clone().ok_or_else(|| {
            missing_registry_flag_refusal(
                &registry,
                "--prefix",
                "Session init needs --prefix when the registry is absent",
            )
        })?;
        registry::init(registry::RegistryInitRequest {
            registry: registry.clone(),
            registry_id: registry_id.clone(),
            prefix,
            zero_pad: request.zero_pad,
            description: None,
        })?;
        registry_initialized = true;
        registry_id
    } else {
        let actual_id = read_registry_id(&registry)?;
        if let Some(requested_id) = request.registry_id.as_deref()
            && requested_id != actual_id
        {
            return Err(session_conflict_refusal(
                &work_dir,
                "registry_id",
                requested_id,
                &actual_id,
            ));
        }
        validate_existing_registry(&registry)?;
        actual_id
    };

    let binding = SessionRegistryBinding {
        kind: entity_kind.clone(),
        path: registry.to_string_lossy().into_owned(),
        id: registry_id.clone(),
        canonical_type: None,
        alias_file: None,
        key_file: None,
        id_alias_file: None,
        id_mode: None,
    };
    let session_id = session_id(&goal, &entity_kind, &registry_id)?;
    let manifest = SessionManifest {
        version: SESSION_MANIFEST_VERSION.to_string(),
        session_id: session_id.clone(),
        goal: goal.clone(),
        registries: vec![binding.clone()],
        evidence_vocabulary_hash: evidence_vocabulary_hash(),
    };

    if let Some(current) = current.as_ref() {
        if current.session_id != manifest.session_id {
            return Err(session_conflict_refusal(
                &work_dir,
                "session_id",
                &manifest.session_id,
                &current.session_id,
            ));
        }
        if current == &manifest {
            return Ok(output(
                "already_initialised",
                manifest,
                &work_dir,
                current_generation_id(&work_dir)?,
                false,
                false,
            ));
        }
    }

    let registry_path_updated = current
        .as_ref()
        .and_then(|current| current.registries.first())
        .is_some_and(|previous| previous.path != binding.path);
    let supersedes_generation_id = current_generation_id_optional(&work_dir)?;
    let manifest_bytes = pretty_manifest_bytes(&manifest)?;
    let request = EntityPublicationRequest {
        stream_id: SESSION_MANIFEST_STREAM_ID.to_string(),
        supersedes_generation_id,
        request_fingerprint: hash_bytes(b"canon.entity.session.manifest.v1"),
        cache_mode: "disabled".to_string(),
        cache_status: "cold_start".to_string(),
        cache_receipt_hash: hash_bytes(b"canon.entity.session.manifest.receipt.v1"),
        stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
        upstream_artifacts: Vec::<EntityPublicationUpstreamRef>::new(),
        files: vec![EntityPublicationFileInput::new(
            "session/session.json",
            SESSION_PUBLICATION_STAGE,
            SESSION_PUBLICATION_VERSION,
            manifest_bytes,
        )],
        omit_logical_paths: Vec::new(),
    };
    let receipt = publish_stream_patch(&work_dir, request).map_err(publication_refusal)?;
    let status = if registry_path_updated {
        "registry_path_updated"
    } else {
        "initialised"
    };
    Ok(output(
        status,
        manifest,
        &work_dir,
        receipt.generation_id,
        registry_initialized,
        registry_path_updated,
    ))
}

/// Initialize a session from a per-kind registry map. Existing registry
/// files are validated in place; only an absent registry and its configured
/// alias file are created.
pub fn init_registries(
    request: SessionInitRegistriesRequest,
) -> Result<SessionInitOutput, Refusal> {
    let goal = validate_non_empty(&request.work_dir, "--goal", &request.goal)?;
    let map_path = absolute_path(&request.registries)?;
    let work_dir = absolute_path(&request.work_dir)?;
    ensure_work_dir(&work_dir)?;
    let map_text = fs::read_to_string(&map_path).map_err(|error| io_refusal(&map_path, error))?;
    let map = registries::parse_registries_map(&map_text).map_err(|error| Refusal {
        code: RefusalCode::EParse,
        message: format!(
            "Cannot parse registries map '{}': {error}",
            map_path.display()
        ),
        detail: json!({
            "path": map_path.display().to_string(),
            "yaml_path": error.path,
            "reason": error.message,
            "writes_performed": false,
        }),
        next_command: Some(
            "Fix the registries YAML path named in the refusal, then rerun session init"
                .to_string(),
        ),
    })?;

    let mut bindings = Vec::with_capacity(map.kinds.len());
    let mut registry_initialized = false;
    for (kind, config) in &map.kinds {
        let registry_path = absolute_path(&config.registry)?;
        let registry_json = registry_path.join("registry.json");
        let initialized = if path_exists(&registry_json) {
            let actual_id = read_registry_id(&registry_path)?;
            if actual_id != config.registry_id {
                return Err(session_conflict_refusal(
                    &work_dir,
                    &format!("kinds.{kind}.registry_id"),
                    &config.registry_id,
                    &actual_id,
                ));
            }
            validate_existing_registry(&registry_path)?;
            false
        } else {
            initialize_mapped_registry(&registry_path, config)?;
            true
        };
        let alias_path = registry_path.join(&config.alias_file);
        if !path_exists(&alias_path) {
            fs::write(&alias_path, b"[]\n").map_err(|error| io_refusal(&alias_path, error))?;
        }
        if let Some(id_alias_file) = &config.id_alias_file {
            let id_alias_path = registry_path.join(id_alias_file);
            if !path_exists(&id_alias_path) {
                fs::write(&id_alias_path, b"[]\n")
                    .map_err(|error| io_refusal(&id_alias_path, error))?;
            }
        }
        if initialized {
            validate_existing_registry(&registry_path)?;
        }
        registry_initialized |= initialized;
        bindings.push(SessionRegistryBinding {
            kind: kind.clone(),
            path: registry_path.to_string_lossy().into_owned(),
            id: config.registry_id.clone(),
            canonical_type: Some(config.canonical_type.clone()),
            alias_file: Some(config.alias_file.clone()),
            key_file: config.key_file.clone(),
            id_alias_file: config.id_alias_file.clone(),
            id_mode: Some(config.id_mode.clone()),
        });
    }

    let registry_token = bindings
        .iter()
        .map(|binding| format!("{}={}", binding.kind, binding.id))
        .collect::<Vec<_>>()
        .join("|");
    let session_id = session_id(&goal, "registry-set", &registry_token)?;
    let manifest = SessionManifest {
        version: SESSION_MANIFEST_VERSION.to_string(),
        session_id: session_id.clone(),
        goal: goal.clone(),
        registries: bindings,
        evidence_vocabulary_hash: evidence_vocabulary_hash(),
    };
    let current = read_current_manifest(&work_dir)?;
    if let Some(current) = current.as_ref() {
        if current.goal != goal {
            return Err(session_conflict_refusal(
                &work_dir,
                "goal",
                &goal,
                &current.goal,
            ));
        }
        if current.session_id != manifest.session_id {
            return Err(session_conflict_refusal(
                &work_dir,
                "session_id",
                &manifest.session_id,
                &current.session_id,
            ));
        }
        if current == &manifest {
            return Ok(output(
                "already_initialised",
                manifest,
                &work_dir,
                current_generation_id(&work_dir)?,
                false,
                false,
            ));
        }
    }
    let registry_path_updated = current
        .as_ref()
        .map(|current| current.registries != manifest.registries)
        .unwrap_or(false);
    let supersedes_generation_id = current_generation_id_optional(&work_dir)?;
    let manifest_bytes = pretty_manifest_bytes(&manifest)?;
    let receipt = publish_stream_patch(
        &work_dir,
        EntityPublicationRequest {
            stream_id: SESSION_MANIFEST_STREAM_ID.to_string(),
            supersedes_generation_id,
            request_fingerprint: hash_bytes(b"canon.entity.session.manifest.v1"),
            cache_mode: "disabled".to_string(),
            cache_status: "cold_start".to_string(),
            cache_receipt_hash: hash_bytes(b"canon.entity.session.manifest.receipt.v1"),
            stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
            upstream_artifacts: Vec::<EntityPublicationUpstreamRef>::new(),
            files: vec![EntityPublicationFileInput::new(
                "session/session.json",
                SESSION_PUBLICATION_STAGE,
                SESSION_PUBLICATION_VERSION,
                manifest_bytes,
            )],
            omit_logical_paths: Vec::new(),
        },
    )
    .map_err(publication_refusal)?;
    let status = if registry_path_updated {
        "registry_path_updated"
    } else {
        "initialised"
    };
    Ok(output(
        status,
        manifest,
        &work_dir,
        receipt.generation_id,
        registry_initialized,
        registry_path_updated,
    ))
}

fn initialize_mapped_registry(
    registry_path: &Path,
    config: &registries::KindRegistry,
) -> Result<(), Refusal> {
    match fs::symlink_metadata(registry_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(path_refusal(
                registry_path,
                "Session registry init refuses a symlinked registry directory",
                "Replace the symlink with a regular directory, then rerun session init",
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(path_refusal(
                registry_path,
                "Session registry init requires a directory path",
                "Choose a directory for the registry map entry, then rerun session init",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(registry_path).map_err(|error| io_refusal(registry_path, error))?;
        }
        Err(error) => return Err(io_refusal(registry_path, error)),
    }
    let mut registry_json = json!({
        "description": format!("Session {} registry", config.kind),
        "entry_count": 0,
        "id": config.registry_id,
        "updated": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "version": "0.0.0",
    });
    if let registries::KindIdMode::Numbered {
        prefix, zero_pad, ..
    } = &config.id_mode
        && let Value::Object(object) = &mut registry_json
    {
        object.insert(
            "default_id_scheme".to_string(),
            json!({
                "prefix": prefix,
                "zero_pad": zero_pad.unwrap_or(3),
            }),
        );
    }
    let bytes = serde_json::to_vec_pretty(&registry_json).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to serialize mapped registry metadata: {error}"),
        detail: json!({"registry": registry_path.display().to_string(), "writes_performed": false}),
        next_command: Some("Repair the registries map, then rerun session init".to_string()),
    })?;
    fs::write(registry_path.join("registry.json"), bytes)
        .map_err(|error| io_refusal(&registry_path.join("registry.json"), error))?;
    Ok(())
}

pub fn ingest(request: SessionIngestRequest) -> Result<SessionIngestOutput, Refusal> {
    let work_dir = absolute_path(&request.session)?;
    let Some(manifest) = read_current_manifest(&work_dir)? else {
        return Err(session_not_initialized_refusal(&work_dir));
    };

    let current = match open_current_stream_generation(&work_dir, SESSION_OBSERVATIONS_STREAM_ID) {
        Ok(snapshot) => Some(snapshot),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => None,
        Err(error) => return Err(observation_artifact_refusal(&error.message)),
    };
    let existing = match current.as_ref() {
        Some(snapshot) => existing_observations(snapshot)?,
        None => ExistingObservations::default(),
    };
    let known_rows = existing.rows;
    let existing_shard = existing.shard;

    let input = read_observation_input(&request.observations)?;
    let mut accepted_rows = BTreeMap::new();
    let mut already_present = 0;
    let mut rejected = Vec::new();
    let mut key_warnings = Vec::new();

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let mut row = match canonicalize_observation_line(line, line_number) {
            Ok(row) => row,
            Err(error) => {
                rejected.push(error.into_rejection());
                continue;
            }
        };

        key_warnings.append(&mut row.key_warnings);

        let entity_type = serde_json::from_slice::<Value>(&row.bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("entity_type")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        if entity_type.as_deref().is_none_or(|kind| {
            !manifest
                .registries
                .iter()
                .any(|binding| binding.kind == kind)
        }) {
            let kind = entity_type.as_deref().unwrap_or("<missing>");
            rejected.push(SessionIngestRejection {
                line: line_number,
                obs_id: Some(row.obs_id),
                code: "unmapped_entity_kind".to_string(),
                message: format!(
                    "Observation entity_type '{kind}' has no session registry mapping"
                ),
                fix: "Add the entity_type under kinds in the registries YAML, then rerun ingest"
                    .to_string(),
            });
            continue;
        }

        if let Some(existing) = known_rows
            .get(&row.obs_id)
            .or(accepted_rows.get(&row.obs_id))
        {
            if existing == &row.bytes {
                already_present += 1;
            } else {
                rejected.push(SessionIngestRejection {
                    line: line_number,
                    obs_id: Some(row.obs_id),
                    code: "obs_id_conflict".to_string(),
                    message: "obs_id already exists with different observation content".to_string(),
                    fix: "obs_id or observation fields".to_string(),
                });
            }
            continue;
        }

        accepted_rows.insert(row.obs_id, row.bytes);
    }

    if accepted_rows.is_empty() {
        return Ok(SessionIngestOutput {
            version: SESSION_INGEST_VERSION.to_string(),
            accepted: 0,
            already_present,
            rejected,
            key_warnings,
            shard: existing_shard,
            generation_id: current.map(|snapshot| snapshot.generation_id),
        });
    }

    let shard_bytes = canonical_shard_bytes(&accepted_rows);
    let digest = hash_bytes(&shard_bytes);
    let digest_hex = digest.strip_prefix("blake3:").unwrap_or(&digest);
    let logical_path = format!("observations/{digest_hex}.jsonl");
    let request = EntityPublicationRequest {
        stream_id: SESSION_OBSERVATIONS_STREAM_ID.to_string(),
        supersedes_generation_id: current
            .as_ref()
            .map(|snapshot| snapshot.generation_id.clone()),
        request_fingerprint: hash_bytes(b"canon.entity.session.observations.v1"),
        cache_mode: "disabled".to_string(),
        cache_status: "cold_start".to_string(),
        cache_receipt_hash: hash_bytes(b"canon.entity.session.observations.receipt.v1"),
        stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
        upstream_artifacts: Vec::<EntityPublicationUpstreamRef>::new(),
        files: vec![EntityPublicationFileInput::new(
            logical_path.clone(),
            SESSION_PUBLICATION_STAGE,
            SESSION_PUBLICATION_VERSION,
            shard_bytes,
        )],
        omit_logical_paths: Vec::new(),
    };
    let receipt = publish_stream_patch(&work_dir, request)
        .map_err(|error| observation_publication_refusal(&error.message))?;

    Ok(SessionIngestOutput {
        version: SESSION_INGEST_VERSION.to_string(),
        accepted: accepted_rows.len(),
        already_present,
        rejected,
        key_warnings,
        shard: Some(logical_path),
        generation_id: Some(receipt.generation_id),
    })
}

pub fn assert_evidence(request: SessionAssertRequest) -> Result<SessionAssertOutput, Refusal> {
    let work_dir = absolute_path(&request.session)?;
    let Some(manifest) = read_current_manifest(&work_dir)? else {
        return Err(session_not_initialized_refusal(&work_dir));
    };

    let current = match open_current_stream_generation(&work_dir, SESSION_EVIDENCE_STREAM_ID) {
        Ok(snapshot) => Some(snapshot),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => None,
        Err(error) => return Err(evidence_artifact_refusal(&error.message)),
    };
    let existing = match current.as_ref() {
        Some(snapshot) => existing_evidence(snapshot)?,
        None => ExistingEvidence::default(),
    };
    let known_records = existing.records;
    let existing_shard = existing.shard;
    let input = read_evidence_input(&request.evidence)?;
    let mut accepted_records = BTreeMap::new();
    let mut already_present = 0;
    let mut rejected = Vec::new();

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let record = match canonicalize_assertion_line(
            line,
            line_number,
            &manifest.evidence_vocabulary_hash,
        ) {
            Ok(record) => record,
            Err(error) => {
                rejected.push(error.into_rejection());
                continue;
            }
        };

        if let Some(existing) = known_records
            .get(&record.evidence_id)
            .or(accepted_records.get(&record.evidence_id))
        {
            if existing == &record.bytes {
                already_present += 1;
            } else {
                rejected.push(SessionAssertRejection {
                    line: line_number,
                    code: "evidence_id_conflict".to_string(),
                    message: "evidence_id already exists with different record content".to_string(),
                    fix: "evidence fields or a new assertion".to_string(),
                });
            }
            continue;
        }

        accepted_records.insert(record.evidence_id, record.bytes);
    }

    if accepted_records.is_empty() {
        return Ok(SessionAssertOutput {
            version: SESSION_ASSERT_VERSION.to_string(),
            accepted: 0,
            already_present,
            rejected,
            shard: existing_shard,
            generation_id: current.map(|snapshot| snapshot.generation_id),
        });
    }

    let shard_bytes = canonical_shard_bytes(&accepted_records);
    let digest = hash_bytes(&shard_bytes);
    let digest_hex = digest.strip_prefix("blake3:").unwrap_or(&digest);
    let logical_path = format!("evidence/{digest_hex}.jsonl");
    let request = EntityPublicationRequest {
        stream_id: SESSION_EVIDENCE_STREAM_ID.to_string(),
        supersedes_generation_id: current
            .as_ref()
            .map(|snapshot| snapshot.generation_id.clone()),
        request_fingerprint: hash_bytes(b"canon.entity.session.evidence.v1"),
        cache_mode: "disabled".to_string(),
        cache_status: "cold_start".to_string(),
        cache_receipt_hash: hash_bytes(b"canon.entity.session.evidence.receipt.v1"),
        stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
        upstream_artifacts: Vec::<EntityPublicationUpstreamRef>::new(),
        files: vec![EntityPublicationFileInput::new(
            logical_path.clone(),
            SESSION_PUBLICATION_STAGE,
            SESSION_PUBLICATION_VERSION,
            shard_bytes,
        )],
        omit_logical_paths: Vec::new(),
    };
    let receipt = publish_stream_patch(&work_dir, request)
        .map_err(|error| evidence_publication_refusal(&error.message))?;

    Ok(SessionAssertOutput {
        version: SESSION_ASSERT_VERSION.to_string(),
        accepted: accepted_records.len(),
        already_present,
        rejected,
        shard: Some(logical_path),
        generation_id: Some(receipt.generation_id),
    })
}

pub fn commit(request: SessionCommitRequest) -> Result<SessionCommitOutput, Refusal> {
    let work_dir = absolute_path(&request.session)?;
    let Some(manifest) = read_current_manifest(&work_dir)? else {
        return Err(session_not_initialized_refusal(&work_dir));
    };
    let binding = match request.registry_kind.as_deref() {
        Some(kind) => manifest
            .registries
            .iter()
            .find(|binding| binding.kind == kind)
            .ok_or_else(|| Refusal {
                code: RefusalCode::EEntityArtifactContract,
                message: format!("Session manifest has no registry binding for kind '{kind}'"),
                detail: json!({"kind": kind, "writes_performed": false}),
                next_command: Some(
                    "Add the kind to the registries map, then rerun session init and commit"
                        .to_string(),
                ),
            })?,
        None => manifest.registries.first().ok_or_else(|| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Session manifest has no registry binding".to_string(),
            detail: json!({"writes_performed": false}),
            next_command: Some(
                "Initialize a session with one registry, then rerun commit".to_string(),
            ),
        })?,
    };
    let observations_generation =
        current_generation_id_for_stream(&work_dir, SESSION_OBSERVATIONS_STREAM_ID)?.ok_or_else(
            || {
                stale_solve_refusal(
                    "observations_generation",
                    &request.observations_generation,
                    None,
                )
            },
        )?;
    if observations_generation != request.observations_generation {
        return Err(stale_solve_refusal(
            "observations_generation",
            &request.observations_generation,
            Some(&observations_generation),
        ));
    }
    let evidence_generation =
        current_generation_id_for_stream(&work_dir, SESSION_EVIDENCE_STREAM_ID)?;
    if evidence_generation != request.evidence_generation {
        return Err(stale_solve_refusal(
            "evidence_generation",
            request.evidence_generation.as_deref().unwrap_or("<none>"),
            evidence_generation.as_deref(),
        ));
    }

    let registry_path = Path::new(&binding.path);
    let loaded_registry = registry::load_registry(registry_path)
        .map_err(|error| Refusal::bad_registry(&binding.path, &error.to_string()))?;
    if let Some(output) = already_committed(
        &work_dir,
        &request.solve_hash,
        &loaded_registry.meta.id,
        &loaded_registry.meta.version,
    )? {
        return Ok(output);
    }
    if loaded_registry.meta.id != request.registry.id
        || loaded_registry.meta.version != request.registry.version
    {
        return Err(stale_solve_refusal(
            "registry",
            &format!("{}@{}", request.registry.id, request.registry.version),
            Some(&format!(
                "{}@{}",
                loaded_registry.meta.id, loaded_registry.meta.version
            )),
        ));
    }

    let memory_change = request.memory.as_ref().is_some_and(|memory| {
        !memory.cannot_link.is_empty()
            || !memory.pending.is_empty()
            || !memory.pending_clear.is_empty()
    });
    if memory_change {
        let lint = registry_lint::lint(registry_path, registry_lint::RegistryLintProfile::Auto)?;
        if lint.summary.errors > 0 {
            return Err(memory_refusal(
                format!(
                    "Registry memory lint found {} error(s); no session commit was written",
                    lint.summary.errors
                ),
                false,
            ));
        }
    }
    let memory_substantive = request.memory.as_ref().is_some_and(memory_is_substantive);
    if !commit_has_effect(
        !request.entries.is_empty(),
        request.relations.len(),
        memory_substantive,
    ) {
        return Ok(SessionCommitOutput {
            version: SESSION_COMMIT_VERSION.to_string(),
            outcome: "nothing_to_commit".to_string(),
            solve_hash: request.solve_hash,
            registry: None,
            registries: Vec::new(),
            minted: Vec::new(),
            attached: Vec::new(),
            skipped: request.skipped,
            relations_written: 0,
            cannot_link_written: 0,
            pending_written: 0,
            pending_cleared: 0,
            closure_set_hash: None,
            generation_id: None,
        });
    }

    let entry_by_group = request
        .entries
        .iter()
        .map(|entry| {
            let group_key = match entry.canonical_id.as_deref() {
                Some(canonical_id) => format!("existing:{canonical_id}"),
                None => format!("new:{}", entry.cluster_id),
            };
            (group_key, entry)
        })
        .collect::<BTreeMap<_, _>>();
    let requested_skipped = request.skipped.clone();
    let (numbered_prefix, numbered_zero_pad, agent_ids) = match &binding.id_mode {
        Some(registries::KindIdMode::Numbered {
            prefix,
            zero_pad,
            agent_kind_prefix,
        }) => (
            Some(prefix.clone()),
            *zero_pad,
            agent_kind_prefix
                .as_ref()
                .map(|kind_prefix| registry::RegistryTypedAgentIds {
                    kind_prefix: format!("{kind_prefix}_"),
                    canonical: false,
                    id_alias_file: binding.id_alias_file.clone(),
                }),
        ),
        Some(registries::KindIdMode::Agent { kind_prefix }) => (
            None,
            None,
            Some(registry::RegistryTypedAgentIds {
                kind_prefix: format!("{kind_prefix}_"),
                canonical: true,
                id_alias_file: None,
            }),
        ),
        None => (None, None, None),
    };
    let typed_entries = request
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .aliases
                .iter()
                .map(|alias| registry::RegistryTypedAlias {
                    alias_file: alias.alias_file.clone(),
                    input: alias.input.clone(),
                    canonical_id: entry.canonical_id.clone(),
                    new_id_slot: entry
                        .canonical_id
                        .is_none()
                        .then(|| entry.cluster_id.clone()),
                    canonical_type: alias.canonical_type.clone(),
                    rule_id: alias.rule_id.clone(),
                    skip_if_existing: alias.skip_if_existing,
                    session_authority: alias.session_authority.clone(),
                    founding_ref: Some(entry.outcome_ref.clone()),
                })
        })
        .collect::<Vec<_>>();
    let mut closures = request
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .aliases
                .iter()
                .filter_map(|alias| alias.closure.clone())
                .chain(entry.closure.clone())
        })
        .collect::<Vec<_>>();
    closures.sort_by(|left, right| left.closure_hash.cmp(&right.closure_hash));
    closures.dedup_by(|left, right| left.closure_hash == right.closure_hash);
    for closure in &closures {
        validate_commit_closure(closure)?;
    }
    let typed = if request.entries.is_empty() {
        None
    } else {
        Some(registry::commit_typed_entries(
            registry::RegistryTypedWriteRequest {
                registry: registry_path.to_path_buf(),
                prefix: numbered_prefix,
                zero_pad: numbered_zero_pad,
                entries: typed_entries,
                bump: request.bump,
                next_version: request.next_version,
                no_lint: false,
                expected_version: Some(request.registry.version.clone()),
                agent_ids,
            },
        )?)
    };
    let typed_skipped = typed
        .as_ref()
        .map(|typed| typed.skipped.clone())
        .unwrap_or_default();
    let mut receipt_skipped = requested_skipped;
    for skipped_alias in typed_skipped {
        let group_key = skipped_alias.group_key;
        let source = entry_by_group.get(&group_key);
        receipt_skipped.push(SessionCommitSkipped {
            cluster_id: source
                .map(|entry| entry.cluster_id.clone())
                .unwrap_or(group_key),
            reason: skipped_alias.reason,
            detail: skipped_alias.detail,
        });
    }
    receipt_skipped.sort_by(|left, right| {
        left.cluster_id
            .cmp(&right.cluster_id)
            .then_with(|| left.reason.cmp(&right.reason))
    });
    let typed_minted = typed
        .as_ref()
        .map(|typed| typed.minted.clone())
        .unwrap_or_default();
    if !commit_has_effect(
        typed_minted.iter().any(|entry| !entry.aliases.is_empty()),
        request.relations.len(),
        memory_substantive,
    ) {
        return Ok(SessionCommitOutput {
            version: SESSION_COMMIT_VERSION.to_string(),
            outcome: "nothing_to_commit".to_string(),
            solve_hash: request.solve_hash,
            registry: None,
            registries: Vec::new(),
            minted: Vec::new(),
            attached: Vec::new(),
            skipped: receipt_skipped,
            relations_written: 0,
            cannot_link_written: 0,
            pending_written: 0,
            pending_cleared: 0,
            closure_set_hash: None,
            generation_id: None,
        });
    }
    let mut minted = Vec::new();
    let mut attached = Vec::new();
    let mut minted_by_cluster = BTreeMap::new();
    for entry in typed_minted {
        if let Some(cluster_id) = entry.group_key.strip_prefix("new:") {
            minted_by_cluster.insert(cluster_id.to_string(), entry.canonical_id.clone());
        }
        let Some(source) = entry_by_group.get(&entry.group_key) else {
            continue;
        };
        if entry.group_key.starts_with("existing:") {
            attached.push(SessionCommitAttached {
                canonical_id: entry.canonical_id,
                cluster_id: source.cluster_id.clone(),
                tier: source.tier.clone(),
                aliases: entry.aliases,
                outcome_ref: source.outcome_ref.clone(),
                closure_hash: source.closure_hash.clone(),
            });
        } else {
            minted.push(SessionCommitMinted {
                canonical_id: entry.canonical_id,
                cluster_id: source.cluster_id.clone(),
                tier: source.tier.clone(),
                aliases: entry.aliases,
                outcome_ref: source.outcome_ref.clone(),
                closure_hash: source.closure_hash.clone(),
            });
        }
    }
    let prior_receipt =
        read_commit_receipt(&work_dir)?.filter(|receipt| receipt.solve_hash == request.solve_hash);
    if let Some(prior) = &prior_receipt {
        closures.extend(prior.closures.iter().cloned());
        closures.sort_by(|left, right| left.closure_hash.cmp(&right.closure_hash));
        closures.dedup_by(|left, right| left.closure_hash == right.closure_hash);
    }
    let (closure_set, closure_set_hash) = if closures.is_empty() {
        (None, None)
    } else {
        let value = build_commit_closure_set(&manifest.session_id, &request.solve_hash, &closures)?;
        let bytes = serde_json::to_vec(&value).map_err(|error| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: format!("Unable to serialize session closure set: {error}"),
            detail: json!({"writes_performed": false}),
            next_command: Some("Rerun canon entity session commit".to_string()),
        })?;
        (Some(value), Some(hash_bytes(&bytes)))
    };
    let registry = Some(match typed.as_ref() {
        Some(typed) => SessionCommitRegistry {
            id: typed.registry.id.clone(),
            version_before: typed.registry.version_before.clone(),
            version_after: typed.registry.version_after.clone(),
            entry_count_before: typed.registry.entry_count_before,
            entry_count_after: typed.registry.entry_count_after,
        },
        None => current_registry_summary(
            registry_path,
            &loaded_registry.meta.id,
            &loaded_registry.meta.version,
        )?,
    });
    let current_registry = registry.clone().ok_or_else(|| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: "Typed registry commit returned no registry metadata".to_string(),
        detail: json!({"writes_performed": true}),
        next_command: Some("Reread the registry and session commit receipt".to_string()),
    })?;
    let mut registries = prior_receipt
        .as_ref()
        .map(|receipt| {
            if receipt.registries.is_empty() {
                vec![receipt.registry.clone()]
            } else {
                receipt.registries.clone()
            }
        })
        .unwrap_or_default();
    registries.retain(|item| item.id != current_registry.id);
    registries.push(current_registry.clone());
    registries.sort_by(|left, right| left.id.cmp(&right.id));
    let prior_minted = prior_receipt
        .as_ref()
        .map(|receipt| receipt.minted.clone())
        .unwrap_or_default();
    let prior_attached = prior_receipt
        .as_ref()
        .map(|receipt| receipt.attached.clone())
        .unwrap_or_default();
    let prior_skipped = prior_receipt
        .as_ref()
        .map(|receipt| receipt.skipped.clone())
        .unwrap_or_default();
    let mut all_minted = prior_minted;
    all_minted.extend(minted.clone());
    let mut all_attached = prior_attached;
    all_attached.extend(attached.clone());
    let mut all_skipped = prior_skipped;
    all_skipped.extend(receipt_skipped.clone());
    all_skipped.sort_by(|left, right| {
        left.cluster_id
            .cmp(&right.cluster_id)
            .then_with(|| left.reason.cmp(&right.reason))
    });
    let memory_counts = request
        .memory
        .as_ref()
        .map(|memory| write_registry_memory(registry_path, memory))
        .transpose()?
        .unwrap_or_default();
    let relations_written = write_relation_sidecar(
        registry_path,
        &manifest.session_id,
        &request.solve_hash,
        &request.relations,
        &minted_by_cluster,
    )?;
    let receipt = SessionCommitReceipt {
        version: SESSION_COMMIT_VERSION.to_string(),
        solve_hash: request.solve_hash.clone(),
        registry: current_registry,
        registries,
        minted: all_minted,
        attached: all_attached,
        skipped: all_skipped,
        relations_written: prior_receipt
            .as_ref()
            .map(|receipt| receipt.relations_written)
            .unwrap_or_default()
            + relations_written,
        cannot_link_written: prior_receipt
            .as_ref()
            .map(|receipt| receipt.cannot_link_written)
            .unwrap_or_default()
            + memory_counts.cannot_link_written,
        pending_written: prior_receipt
            .as_ref()
            .map(|receipt| receipt.pending_written)
            .unwrap_or_default()
            + memory_counts.pending_written,
        pending_cleared: prior_receipt
            .as_ref()
            .map(|receipt| receipt.pending_cleared)
            .unwrap_or_default()
            + memory_counts.pending_cleared,
        closure_set_hash: closure_set_hash.clone(),
        closures: closures.clone(),
    };
    let bytes = serde_json::to_vec(&receipt).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to serialize session commit receipt: {error}"),
        detail: json!({"writes_performed": false}),
        next_command: Some("Rerun canon entity session commit".to_string()),
    })?;
    let mut upstream_artifacts = vec![EntityPublicationUpstreamRef {
        version: SESSION_OBSERVATIONS_STREAM_ID.to_string(),
        content_hash: request.observations_generation,
    }];
    if let Some(generation) = request.evidence_generation {
        upstream_artifacts.push(EntityPublicationUpstreamRef {
            version: SESSION_EVIDENCE_STREAM_ID.to_string(),
            content_hash: generation,
        });
    }
    upstream_artifacts.push(EntityPublicationUpstreamRef {
        version: "entity-session-solve".to_string(),
        content_hash: request.solve_hash,
    });
    let mut publication_files = vec![EntityPublicationFileInput::new(
        "commit/commit.json",
        SESSION_PUBLICATION_STAGE,
        SESSION_COMMIT_VERSION,
        bytes,
    )];
    if let Some(closure_set) = closure_set {
        let closure_set_bytes = serde_json::to_vec(&closure_set).map_err(|error| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: format!("Unable to serialize session closure set: {error}"),
            detail: json!({"writes_performed": false}),
            next_command: Some("Rerun canon entity session commit".to_string()),
        })?;
        publication_files.push(EntityPublicationFileInput::new(
            "commit/closure-set.json",
            SESSION_PUBLICATION_STAGE,
            SESSION_COMMIT_VERSION,
            closure_set_bytes,
        ));
    }
    for closure in &closures {
        let closure_bytes = serde_json::to_vec(&closure.manifest).map_err(|error| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: format!("Unable to serialize evidence closure: {error}"),
            detail: json!({"outcome_ref": closure.outcome_ref, "writes_performed": false}),
            next_command: Some("Rerun canon entity session commit".to_string()),
        })?;
        publication_files.push(EntityPublicationFileInput::new(
            format!("commit/closures/{}.json", closure.closure_hash),
            SESSION_PUBLICATION_STAGE,
            SESSION_COMMIT_VERSION,
            closure_bytes,
        ));
    }
    let publication = publish_stream_patch(
        &work_dir,
        EntityPublicationRequest {
            stream_id: SESSION_COMMIT_STREAM_ID.to_string(),
            supersedes_generation_id: current_generation_id_for_stream(
                &work_dir,
                SESSION_COMMIT_STREAM_ID,
            )?,
            request_fingerprint: hash_bytes(SESSION_COMMIT_VERSION.as_bytes()),
            cache_mode: "disabled".to_string(),
            cache_status: "cold_start".to_string(),
            cache_receipt_hash: hash_bytes(b"canon.entity.session.commit.receipt.v1"),
            stage_order: vec![SESSION_PUBLICATION_STAGE.to_string()],
            upstream_artifacts,
            files: publication_files,
            omit_logical_paths: Vec::new(),
        },
    )
    .map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to publish session commit receipt: {error}"),
        detail: json!({"writes_performed": true, "stream": SESSION_COMMIT_STREAM_ID}),
        next_command: Some("Reread the session commit receipt, then rerun commit".to_string()),
    })?;
    Ok(SessionCommitOutput {
        version: SESSION_COMMIT_VERSION.to_string(),
        outcome: "committed".to_string(),
        solve_hash: receipt.solve_hash,
        registry,
        registries: receipt.registries,
        minted: receipt.minted,
        attached: receipt.attached,
        skipped: receipt.skipped,
        relations_written: receipt.relations_written,
        cannot_link_written: receipt.cannot_link_written,
        pending_written: receipt.pending_written,
        pending_cleared: receipt.pending_cleared,
        closure_set_hash,
        generation_id: Some(publication.generation_id),
    })
}

fn validate_commit_closure(closure: &SessionCommitClosure) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec(&closure.manifest).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to serialize evidence closure: {error}"),
        detail: json!({"outcome_ref": closure.outcome_ref, "writes_performed": false}),
        next_command: Some("Rerun canon entity session solve, then rerun commit".to_string()),
    })?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash != closure.closure_hash {
        return Err(Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Evidence closure hash does not match its manifest".to_string(),
            detail: json!({
                "outcome_ref": closure.outcome_ref,
                "expected": closure.closure_hash,
                "actual": actual_hash,
                "writes_performed": false,
            }),
            next_command: Some("Rerun canon entity session solve, then rerun commit".to_string()),
        });
    }
    if closure.manifest.get("outcome_ref").and_then(Value::as_str)
        != Some(closure.outcome_ref.as_str())
    {
        return Err(Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Evidence closure outcome_ref does not match its manifest".to_string(),
            detail: json!({"outcome_ref": closure.outcome_ref, "writes_performed": false}),
            next_command: Some("Rerun canon entity session solve, then rerun commit".to_string()),
        });
    }
    Ok(())
}

#[derive(Debug, Default)]
struct RegistryMemoryWriteCounts {
    cannot_link_written: usize,
    pending_written: usize,
    pending_cleared: usize,
}

fn write_registry_memory(
    registry_path: &Path,
    requested: &SessionRegistryMemoryWrite,
) -> Result<RegistryMemoryWriteCounts, Refusal> {
    let escrow_dir = registry_path.join("_escrow");
    let cannot_path = escrow_dir.join("cannot_link.jsonl");
    let pending_path = escrow_dir.join("pending.jsonl");
    let existing_cannot =
        read_memory_jsonl::<crate::entity::runtime::CannotLinkFact>(&cannot_path)?;
    let existing_pending =
        read_memory_jsonl::<crate::entity::runtime::PendingClusterRecord>(&pending_path)?;

    let mut cannot_by_pair = BTreeMap::new();
    for mut fact in existing_cannot {
        validate_memory_cannot_link(&fact)?;
        let (left_key, right_key) = ordered_memory_pair(&fact.left_key, &fact.right_key);
        fact.left_key = left_key.clone();
        fact.right_key = right_key.clone();
        cannot_by_pair.insert((left_key, right_key), fact);
    }
    let before_cannot = cannot_by_pair.len();
    for fact in &requested.cannot_link {
        validate_memory_cannot_link(fact)?;
        let pair = ordered_memory_pair(&fact.left_key, &fact.right_key);
        cannot_by_pair.entry(pair).or_insert_with(|| fact.clone());
    }

    let mut pending_by_id = existing_pending
        .into_iter()
        .map(|record| (record.escrow_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut pending_cleared = 0;
    for escrow_id in &requested.pending_clear {
        if pending_by_id.remove(escrow_id).is_some() {
            pending_cleared += 1;
        }
    }
    let before_pending = pending_by_id.clone();
    for record in &requested.pending {
        validate_memory_pending(record)?;
        pending_by_id.insert(record.escrow_id.clone(), record.clone());
    }

    let cannot_changed = cannot_by_pair.len() != before_cannot;
    let pending_changed = pending_by_id != before_pending;
    if !cannot_changed && !pending_changed {
        return Ok(RegistryMemoryWriteCounts::default());
    }
    fs::create_dir_all(&escrow_dir).map_err(|error| {
        memory_refusal(
            format!(
                "Unable to create registry escrow directory '{}': {error}",
                escrow_dir.display()
            ),
            false,
        )
    })?;
    if cannot_changed {
        let bytes = memory_jsonl_bytes(cannot_by_pair.values())?;
        publish_memory_sidecar(&cannot_path, bytes)?;
    }
    if pending_changed {
        let bytes = memory_jsonl_bytes(pending_by_id.values())?;
        publish_memory_sidecar(&pending_path, bytes)?;
    }
    let pending_written = requested
        .pending
        .iter()
        .filter(|record| {
            before_pending
                .get(&record.escrow_id)
                .is_none_or(|previous| previous != *record)
        })
        .count();
    Ok(RegistryMemoryWriteCounts {
        cannot_link_written: cannot_by_pair.len().saturating_sub(before_cannot),
        pending_written,
        pending_cleared,
    })
}

fn read_memory_jsonl<T>(path: &Path) -> Result<Vec<T>, Refusal>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path).map_err(|error| {
        memory_refusal(
            format!(
                "Unable to read registry memory '{}': {error}",
                path.display()
            ),
            false,
        )
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        memory_refusal(
            format!("Registry memory '{}' is not UTF-8: {error}", path.display()),
            false,
        )
    })?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line, value)| {
            serde_json::from_str(value).map_err(|error| {
                memory_refusal(
                    format!(
                        "Registry memory '{}' line {} is invalid: {error}",
                        path.display(),
                        line + 1
                    ),
                    false,
                )
            })
        })
        .collect()
}

fn memory_jsonl_bytes<'a, T>(records: impl IntoIterator<Item = &'a T>) -> Result<Vec<u8>, Refusal>
where
    T: Serialize + 'a,
{
    let mut bytes = Vec::new();
    for record in records {
        let line = serde_json::to_vec(record).map_err(|error| {
            memory_refusal(
                format!("Unable to serialize registry memory: {error}"),
                false,
            )
        })?;
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn publish_memory_sidecar(path: &Path, bytes: Vec<u8>) -> Result<(), Refusal> {
    let temporary = path.with_extension(format!("jsonl.{}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| {
        memory_refusal(
            format!(
                "Unable to stage registry memory '{}': {error}",
                temporary.display()
            ),
            false,
        )
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        memory_refusal(
            format!(
                "Unable to publish registry memory '{}': {error}",
                path.display()
            ),
            true,
        )
    })
}

fn ordered_memory_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn validate_memory_cannot_link(
    fact: &crate::entity::runtime::CannotLinkFact,
) -> Result<(), Refusal> {
    if fact.left_key.trim().is_empty()
        || fact.right_key.trim().is_empty()
        || fact.reason.trim().is_empty()
        || fact.left_key == fact.right_key
    {
        return Err(memory_refusal(
            "Registry cannot-link fact has invalid keys or reason".to_string(),
            false,
        ));
    }
    Ok(())
}

fn validate_memory_pending(
    record: &crate::entity::runtime::PendingClusterRecord,
) -> Result<(), Refusal> {
    if record.escrow_id.trim().is_empty()
        || record.profile.trim().is_empty()
        || record.state.trim().is_empty()
    {
        return Err(memory_refusal(
            "Registry pending record has an empty required field".to_string(),
            false,
        ));
    }
    Ok(())
}

fn memory_refusal(message: String, writes_performed: bool) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message,
        detail: json!({"stream": "registry/_escrow", "writes_performed": writes_performed}),
        next_command: Some(
            "Repair registry memory sidecars, then rerun canon entity session commit".to_string(),
        ),
    }
}

fn build_commit_closure_set(
    session_id: &str,
    solve_hash: &str,
    closures: &[SessionCommitClosure],
) -> Result<Value, Refusal> {
    let mut refs = closures
        .iter()
        .map(|closure| {
            json!({
                "outcome_ref": closure.outcome_ref,
                "closure_hash": closure.closure_hash,
            })
        })
        .collect::<Vec<_>>();
    refs.sort_by(|left, right| {
        let left_key = left
            .get("outcome_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_key = right
            .get("outcome_ref")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_key.cmp(right_key)
    });
    Ok(json!({
        "version": crate::entity::session_provenance::SESSION_CLOSURE_SET_VERSION,
        "session_id": session_id,
        "solve_hash": solve_hash,
        "closures": refs,
    }))
}

fn current_registry_summary(
    registry_path: &Path,
    registry_id: &str,
    registry_version: &str,
) -> Result<SessionCommitRegistry, Refusal> {
    let bytes = fs::read(registry_path.join("registry.json")).map_err(|error| {
        Refusal::bad_registry(&registry_path.display().to_string(), &error.to_string())
    })?;
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|error| {
        Refusal::bad_registry(&registry_path.display().to_string(), &error.to_string())
    })?;
    let entry_count = value
        .get("entry_count")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| {
            Refusal::bad_registry(
                &registry_path.display().to_string(),
                "registry.json entry_count is missing or invalid",
            )
        })?;
    Ok(SessionCommitRegistry {
        id: registry_id.to_string(),
        version_before: registry_version.to_string(),
        version_after: registry_version.to_string(),
        entry_count_before: entry_count,
        entry_count_after: entry_count,
    })
}

fn write_relation_sidecar(
    registry_path: &Path,
    session_id: &str,
    solve_hash: &str,
    relations: &[SessionCommitRelation],
    minted_by_cluster: &BTreeMap<String, String>,
) -> Result<usize, Refusal> {
    if relations.is_empty() {
        return Ok(0);
    }
    let path = registry_path.join("relations.jsonl");
    let existing = if path.exists() {
        let bytes = fs::read(&path).map_err(|error| {
            relation_refusal(
                format!(
                    "Unable to read registry relations '{}': {error}",
                    path.display()
                ),
                false,
            )
        })?;
        parse_relation_lines(&bytes, &path)?
    } else {
        Vec::new()
    };
    let existing_ids = existing
        .iter()
        .map(|fact| fact.relation_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut new_facts = Vec::new();
    for relation in relations {
        let Some(fact) = build_relation_fact(relation, session_id, solve_hash, minted_by_cluster)?
        else {
            continue;
        };
        new_facts.push(fact);
    }
    let facts = finalize_relations(existing.into_iter().chain(new_facts).collect::<Vec<_>>())
        .map_err(|error| {
            relation_refusal(format!("Invalid registry relation set: {error}"), true)
        })?;
    let written = facts
        .iter()
        .filter(|fact| !existing_ids.contains(&fact.relation_id))
        .count();
    if written == 0 {
        return Ok(0);
    }
    let mut bytes = Vec::new();
    for fact in facts {
        let line = serde_json::to_vec(&fact).map_err(|error| {
            relation_refusal(
                format!("Unable to serialize registry relation: {error}"),
                true,
            )
        })?;
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');
    }
    let temporary = registry_path.join(format!(".relations.jsonl.{}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| {
        relation_refusal(
            format!(
                "Unable to stage registry relations '{}': {error}",
                temporary.display()
            ),
            true,
        )
    })?;
    fs::rename(&temporary, &path).map_err(|error| {
        relation_refusal(
            format!(
                "Unable to publish registry relations '{}': {error}",
                path.display()
            ),
            true,
        )
    })?;
    Ok(written)
}

fn parse_relation_lines(bytes: &[u8], path: &Path) -> Result<Vec<DirectedRelationFact>, Refusal> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        relation_refusal(
            format!(
                "Registry relations '{}' are not UTF-8: {error}",
                path.display()
            ),
            false,
        )
    })?;
    let mut facts = Vec::new();
    for (line, value) in text.lines().enumerate() {
        if value.trim().is_empty() {
            continue;
        }
        let fact = serde_json::from_str::<DirectedRelationFact>(value).map_err(|error| {
            relation_refusal(
                format!(
                    "Registry relation {} line {} is invalid: {error}",
                    path.display(),
                    line + 1
                ),
                false,
            )
        })?;
        facts.push(finalize_relation(fact).map_err(|error| {
            relation_refusal(
                format!(
                    "Registry relation {} line {} is not canonical: {error}",
                    path.display(),
                    line + 1
                ),
                false,
            )
        })?);
    }
    Ok(facts)
}

fn build_relation_fact(
    relation: &SessionCommitRelation,
    session_id: &str,
    solve_hash: &str,
    minted_by_cluster: &BTreeMap<String, String>,
) -> Result<Option<DirectedRelationFact>, Refusal> {
    let Some(subject_id) = relation_endpoint_id(&relation.subject, minted_by_cluster) else {
        return Ok(None);
    };
    let Some(object_id) = relation_endpoint_id(&relation.object, minted_by_cluster) else {
        return Ok(None);
    };
    if relation.relation.trim().is_empty() {
        return Err(relation_refusal(
            "A committed relation must have a non-empty relation name".to_string(),
            false,
        ));
    }
    if ![
        "part_of",
        "owned_by",
        "managed_by",
        "parent_of",
        "sponsor_of",
        "guarantor_of",
        "borrower_of",
        "servicer_of",
        "trustee_of",
        "successor_of",
        "affiliate_of",
        "co_borrower_of",
        "trust_of",
    ]
    .contains(&relation.relation.as_str())
    {
        return Err(relation_refusal(
            format!("Unsupported session relation '{}'", relation.relation),
            false,
        ));
    }
    let relation_package_digest = relation_package_digest()?;
    let relation_kind = RelationKindRef::Extension {
        package_digest: relation_package_digest,
        vocabulary: "canon.entity.session.relations".to_string(),
        value: relation.relation.clone(),
    };
    let cycle_policy = if matches!(
        relation.relation.as_str(),
        "part_of" | "owned_by" | "managed_by" | "parent_of" | "successor_of"
    ) {
        RelationCyclePolicy::Disallow
    } else {
        RelationCyclePolicy::Allow
    };
    let fragment = if relation.evidence_ids.is_empty() {
        None
    } else {
        let mut evidence_ids = relation.evidence_ids.clone();
        evidence_ids.sort();
        evidence_ids.dedup();
        Some(evidence_ids.join(","))
    };
    let observed_at = relation.valid_time.start_at.clone();
    Ok(Some(DirectedRelationFact {
        version: crate::temporal::relation::relation_schema_version().to_string(),
        relation_id: String::new(),
        edge_key: String::new(),
        subject: RelationEndpoint {
            identity_id: subject_id,
            entity_type: relation_entity_type(&relation.subject.entity_type),
        },
        relation: relation_kind,
        object: RelationEndpoint {
            identity_id: object_id,
            entity_type: relation_entity_type(&relation.object.entity_type),
        },
        valid_time: relation.valid_time.clone(),
        provenance: RelationProvenance {
            source_system: "canon.entity.session".to_string(),
            locator: format!("session:{session_id}:solve:{solve_hash}"),
            fragment,
            observed_at,
        },
        policy_ref: "canon.entity.session.relations.v1".to_string(),
        constraints: RelationCardinalityConstraints {
            max_objects_per_subject: None,
            max_subjects_per_object: None,
            overlap_policy: RelationOverlapPolicy::Allow,
            cycle_policy,
        },
        identity_implication: RelationIdentityImplication::default(),
        review: RelationReview {
            disposition: ReviewDisposition::Related,
            reason_code: "related_evidence".to_string(),
        },
    }))
}

fn relation_endpoint_id(
    endpoint: &SessionCommitRelationEndpoint,
    minted_by_cluster: &BTreeMap<String, String>,
) -> Option<String> {
    endpoint
        .canonical_id
        .clone()
        .or_else(|| {
            endpoint
                .cluster_id
                .as_ref()
                .and_then(|id| minted_by_cluster.get(id).cloned())
        })
        .or_else(|| endpoint.external_id.clone())
        .filter(|id| !id.trim().is_empty())
}

fn relation_entity_type(value: &str) -> RelationEntityTypeRef {
    match value.trim().to_ascii_lowercase().as_str() {
        "org" | "organisation" | "organization" => RelationEntityTypeRef::Core {
            class: CoreEntityTypeClass::Organization,
        },
        "person" => RelationEntityTypeRef::Core {
            class: CoreEntityTypeClass::Person,
        },
        _ => RelationEntityTypeRef::Extension {
            package_digest: hash_bytes(b"canon.entity.session.entity-types.v1"),
            vocabulary: "canon.entity.session.entity-types".to_string(),
            value: value.trim().to_string(),
        },
    }
}

fn relation_package_digest() -> Result<String, Refusal> {
    let classes = BTreeMap::from([
        ("affiliate_of", "association"),
        ("borrower_of", "role"),
        ("co_borrower_of", "association"),
        ("guarantor_of", "role"),
        ("managed_by", "hierarchy"),
        ("owned_by", "hierarchy"),
        ("parent_of", "hierarchy"),
        ("part_of", "hierarchy"),
        ("servicer_of", "role"),
        ("sponsor_of", "role"),
        ("successor_of", "succession"),
        ("trust_of", "association"),
        ("trustee_of", "role"),
    ]);
    let bytes = serde_json::to_vec(&classes).map_err(|error| {
        relation_refusal(
            format!("Unable to serialize relation vocabulary: {error}"),
            false,
        )
    })?;
    Ok(hash_bytes(&bytes))
}

fn relation_refusal(message: String, writes_performed: bool) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message,
        detail: json!({"stream": "registry/relations.jsonl", "writes_performed": writes_performed}),
        next_command: Some(
            "Repair registry relations.jsonl, then rerun canon entity session commit".to_string(),
        ),
    }
}

fn read_commit_receipt(work_dir: &Path) -> Result<Option<SessionCommitReceipt>, Refusal> {
    let Some(snapshot) = open_optional_stream(work_dir, SESSION_COMMIT_STREAM_ID)? else {
        return Ok(None);
    };
    let Some(bytes) = snapshot.read_logical_file("commit/commit.json") else {
        return Err(Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Session commit generation is missing commit/commit.json".to_string(),
            detail: json!({"stream": SESSION_COMMIT_STREAM_ID, "writes_performed": false}),
            next_command: Some("Repair the session commit stream, then rerun commit".to_string()),
        });
    };
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: format!("Session commit receipt is invalid: {error}"),
            detail: json!({"stream": SESSION_COMMIT_STREAM_ID, "writes_performed": false}),
            next_command: Some("Repair the session commit receipt, then rerun commit".to_string()),
        })
}

pub(crate) fn already_committed(
    work_dir: &Path,
    solve_hash: &str,
    registry_id: &str,
    registry_version: &str,
) -> Result<Option<SessionCommitOutput>, Refusal> {
    let Some(existing) = read_commit_receipt(work_dir)? else {
        return Ok(None);
    };
    let registry_matches = existing
        .registries
        .iter()
        .find(|registry| registry.id == registry_id)
        .or_else(|| (existing.registry.id == registry_id).then_some(&existing.registry));
    if existing.solve_hash != solve_hash
        || registry_matches.is_none_or(|registry| registry.version_after != registry_version)
    {
        return Ok(None);
    }
    let registries = if existing.registries.is_empty() {
        vec![existing.registry.clone()]
    } else {
        existing.registries.clone()
    };
    let generation_id = current_generation_id_for_stream(work_dir, SESSION_COMMIT_STREAM_ID)?;
    Ok(Some(SessionCommitOutput {
        version: SESSION_COMMIT_VERSION.to_string(),
        outcome: "already_committed".to_string(),
        solve_hash: existing.solve_hash,
        registry: Some(existing.registry),
        registries,
        minted: existing.minted,
        attached: existing.attached,
        skipped: existing.skipped,
        relations_written: existing.relations_written,
        cannot_link_written: existing.cannot_link_written,
        pending_written: existing.pending_written,
        pending_cleared: existing.pending_cleared,
        closure_set_hash: existing.closure_set_hash,
        generation_id,
    }))
}

fn current_generation_id_for_stream(
    work_dir: &Path,
    stream_id: &str,
) -> Result<Option<String>, Refusal> {
    match open_current_stream_generation(work_dir, stream_id) {
        Ok(snapshot) => Ok(Some(snapshot.generation_id)),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => Ok(None),
        Err(error) => Err(publication_refusal(error)),
    }
}

fn open_optional_stream(
    work_dir: &Path,
    stream_id: &str,
) -> Result<Option<EntityPublicationSnapshot>, Refusal> {
    match open_current_stream_generation(work_dir, stream_id) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => Ok(None),
        Err(error) => Err(publication_refusal(error)),
    }
}

fn stale_solve_refusal(field: &str, expected: &str, actual: Option<&str>) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Session solve is stale for {field}"),
        detail: json!({
            "reason": "stale_solve",
            "field": field,
            "expected": expected,
            "actual": actual,
            "writes_performed": false,
        }),
        next_command: Some(
            "Run canon entity session solve, then rerun canon entity session commit".to_string(),
        ),
    }
}

#[derive(Debug)]
struct CanonicalObservation {
    obs_id: String,
    bytes: Vec<u8>,
    key_warnings: Vec<SessionKeyWarning>,
}

#[derive(Debug, Default)]
struct ExistingObservations {
    rows: BTreeMap<String, Vec<u8>>,
    shard: Option<String>,
}

#[derive(Debug, Default)]
struct ExistingEvidence {
    records: BTreeMap<String, Vec<u8>>,
    shard: Option<String>,
}

#[derive(Debug)]
struct CanonicalEvidence {
    evidence_id: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct AssertionRowError {
    line: usize,
    code: String,
    message: String,
    fix: String,
}

impl AssertionRowError {
    fn into_rejection(self) -> SessionAssertRejection {
        SessionAssertRejection {
            line: self.line,
            code: self.code,
            message: self.message,
            fix: self.fix,
        }
    }

    fn new(
        line: usize,
        code: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            line,
            code: code.into(),
            message: message.into(),
            fix: fix.into(),
        }
    }
}

fn canonicalize_assertion_line(
    line: &str,
    line_number: usize,
    policy_hash: &str,
) -> Result<CanonicalEvidence, AssertionRowError> {
    let value = serde_json::from_str::<Value>(line).map_err(|error| {
        AssertionRowError::new(
            line_number,
            "invalid_json",
            format!("evidence line is not valid JSON: {error}"),
            "valid JSON object per line",
        )
    })?;
    let is_full_record = value.get("version").is_some();
    let record = if is_full_record {
        let parsed = serde_json::from_value::<EvidenceRecord>(value).map_err(|error| {
            AssertionRowError::new(
                line_number,
                "invalid_record",
                format!("full canon.evidence.v1 record is invalid: {error}"),
                "a complete canon.evidence.v1 record",
            )
        })?;
        canonicalize_record(parsed).map_err(|error| {
            AssertionRowError::new(
                line_number,
                "invalid_evidence",
                format!("full canon.evidence.v1 record is invalid: {error}"),
                "canonical canon.evidence.v1 fields and provenance",
            )
        })?
    } else {
        let object = value.as_object().ok_or_else(|| {
            AssertionRowError::new(
                line_number,
                "not_object",
                "evidence line must be a JSON object",
                "JSON object per line",
            )
        })?;
        compile_short_assertion(object, line_number, policy_hash)?
    };
    let evidence_id = record.evidence_id.clone();
    let bytes = serde_json::to_vec(&record).map_err(|error| {
        AssertionRowError::new(
            line_number,
            "invalid_record",
            format!("failed to serialize canonical evidence: {error}"),
            "serializable canon.evidence.v1 fields",
        )
    })?;
    Ok(CanonicalEvidence { evidence_id, bytes })
}

fn compile_short_assertion(
    object: &Map<String, Value>,
    line_number: usize,
    policy_hash: &str,
) -> Result<EvidenceRecord, AssertionRowError> {
    let assertion_type = required_assertion_string(object, "type", line_number)?;
    let left_id = required_assertion_string(object, "left", line_number)?;
    let right_id = required_assertion_string(object, "right", line_number)?;
    if left_id == right_id {
        return Err(AssertionRowError::new(
            line_number,
            "same_target",
            "evidence pair must contain two distinct ids",
            "left and right",
        ));
    }
    let asserted_by = required_assertion_string(object, "asserted_by", line_number)?;
    let basis = assertion_object(object, "basis", line_number)?;
    let source = assertion_object(object, "source", line_number)?;
    let channel = required_assertion_string(source, "channel", line_number)?;
    let source_id = required_assertion_string(source, "id", line_number)?;

    let locator_supplied = supplied_assertion_string(source, "locator");
    let content_hash_supplied = supplied_assertion_string(source, "content_hash");
    if assertion_type == "asserted_alias"
        && !asserted_by.starts_with("human:")
        && !(locator_supplied && content_hash_supplied)
    {
        return Err(AssertionRowError::new(
            line_number,
            "asserted_alias_citation",
            "asserted_alias requires locator and content_hash for a non-human asserter",
            "source.locator and source.content_hash, or asserted_by human:<id>",
        ));
    }

    let short_form_hash = hash_bytes(&canonical_json_bytes(&Value::Object(object.clone())));
    let locator = optional_assertion_string(source, "locator", line_number)?
        .unwrap_or_else(|| format!("assertion:{short_form_hash}"));
    let source_content_hash =
        optional_assertion_string(source, "content_hash", line_number)?.unwrap_or(short_form_hash);
    let lineage = optional_assertion_string(source, "lineage", line_number)?;
    let ancestry = parse_assertion_ancestry(source, line_number)?;

    let (kind, reason_code, authority_basis, relation) = match assertion_type.as_str() {
        "same_key" | "same_slot" | "attribute_agreement" | "name_equivalent" | "asserted_alias" => {
            (
                EvidenceKind::PairSupport,
                assertion_type.clone(),
                None,
                None,
            )
        }
        "distinct" => {
            let authority = required_assertion_string(basis, "authority", line_number)?;
            let authority_basis = match authority.as_str() {
                "reviewed" => EvidenceAuthorityBasis::ReviewedConstraint,
                "authoritative_incompatibility" => {
                    EvidenceAuthorityBasis::AuthoritativeIncompatibility
                }
                _ => {
                    return Err(AssertionRowError::new(
                        line_number,
                        "invalid_authority",
                        "distinct authority must be reviewed or authoritative_incompatibility",
                        "basis.authority",
                    ));
                }
            };
            (
                EvidenceKind::AntiMergeVeto,
                "distinct".to_string(),
                Some(authority_basis),
                None,
            )
        }
        "related" => {
            let relation = required_assertion_string(basis, "relation", line_number)?;
            if !matches!(
                relation.as_str(),
                "part_of"
                    | "parent_of"
                    | "successor_of"
                    | "affiliate_of"
                    | "co_borrower_of"
                    | "trust_of"
            ) {
                return Err(AssertionRowError::new(
                    line_number,
                    "invalid_relation",
                    "related relation is not in the session vocabulary",
                    "basis.relation",
                ));
            }
            let subject = required_assertion_string(basis, "subject", line_number)?;
            if subject != left_id && subject != right_id {
                return Err(AssertionRowError::new(
                    line_number,
                    "invalid_subject",
                    "related basis.subject must equal left or right",
                    "basis.subject",
                ));
            }
            (
                EvidenceKind::ContextOnly,
                format!("related:{relation}"),
                None,
                Some(relation),
            )
        }
        _ => {
            return Err(AssertionRowError::new(
                line_number,
                "unknown_type",
                format!("unknown session evidence type {assertion_type}"),
                "type: same_key, same_slot, attribute_agreement, name_equivalent, asserted_alias, distinct, or related",
            ));
        }
    };

    let mut basis_payload = basis.clone();
    basis_payload.remove("authority");
    let mut extension_payload = BTreeMap::new();
    if let Some(value) = extension_value(&Value::Object(basis_payload)) {
        extension_payload.insert("basis".to_string(), value);
    }
    extension_payload.insert(
        "asserted_by".to_string(),
        EvidenceExtensionValue::String(asserted_by),
    );
    if let Some(lineage) = lineage {
        extension_payload.insert(
            "lineage".to_string(),
            EvidenceExtensionValue::String(lineage),
        );
    }
    if let Some(ancestry) = ancestry
        && let Some(value) = extension_value(&ancestry)
    {
        extension_payload.insert("ancestry".to_string(), value);
    }
    if let Some(relation) = relation {
        extension_payload.insert(
            "relation".to_string(),
            EvidenceExtensionValue::String(relation),
        );
    }

    let record = EvidenceRecord {
        version: CANON_EVIDENCE_VERSION.to_string(),
        evidence_id: String::new(),
        kind,
        target: EvidenceTarget::Pair { left_id, right_id },
        operator: EvidenceOperatorRef {
            namespace: "session".to_string(),
            operator_id: assertion_type,
            operator_version: "v1".to_string(),
            adapter_id: None,
        },
        reason_code,
        policy: EvidencePolicyRef {
            policy_id: "canon.entity.session.evidence".to_string(),
            policy_version: "v1".to_string(),
            content_hash: policy_hash.to_string(),
        },
        authority_basis,
        scope: None,
        temporal_scope: None,
        provenance: vec![EvidenceProvenanceRef {
            source_type: channel,
            source_id,
            locator,
            content_hash: source_content_hash,
            observed_at: None,
        }],
        measurements: Vec::new(),
        extensions: vec![EvidenceExtension {
            namespace: "entity-session".to_string(),
            schema_ref: SESSION_EVIDENCE_EXTENSION_SCHEMA.to_string(),
            payload: extension_payload,
        }],
    };
    canonicalize_record(record).map_err(|error| {
        AssertionRowError::new(
            line_number,
            "invalid_evidence",
            format!("short assertion does not satisfy canon.evidence.v1: {error}"),
            "canonical evidence fields, policy hash, and provenance",
        )
    })
}

fn required_assertion_string(
    object: &Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<String, AssertionRowError> {
    let Some(value) = object.get(field) else {
        return Err(AssertionRowError::new(
            line_number,
            format!("missing_field:{field}"),
            format!("evidence field {field} is required"),
            field,
        ));
    };
    let Some(value) = value.as_str() else {
        return Err(AssertionRowError::new(
            line_number,
            format!("non_scalar_value:{field}"),
            format!("evidence field {field} must be a string"),
            field,
        ));
    };
    let value = ascii_trim(value);
    if value.is_empty() {
        return Err(AssertionRowError::new(
            line_number,
            format!("missing_field:{field}"),
            format!("evidence field {field} must be non-empty"),
            field,
        ));
    }
    Ok(value.to_string())
}

fn assertion_object<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<&'a Map<String, Value>, AssertionRowError> {
    let Some(value) = object.get(field) else {
        return Err(AssertionRowError::new(
            line_number,
            format!("missing_field:{field}"),
            format!("evidence field {field} is required"),
            field,
        ));
    };
    value.as_object().ok_or_else(|| {
        AssertionRowError::new(
            line_number,
            format!("non_scalar_value:{field}"),
            format!("evidence field {field} must be an object"),
            field,
        )
    })
}

fn supplied_assertion_string(object: &Map<String, Value>, field: &str) -> bool {
    object.get(field).is_some_and(|value| {
        value
            .as_str()
            .is_some_and(|value| !ascii_trim(value).is_empty())
    })
}

fn optional_assertion_string(
    object: &Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<Option<String>, AssertionRowError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(value) = value.as_str() else {
        return Err(AssertionRowError::new(
            line_number,
            format!("non_scalar_value:{field}"),
            format!("evidence field {field} must be a string or null"),
            field,
        ));
    };
    let value = ascii_trim(value);
    if value.is_empty() {
        return Err(AssertionRowError::new(
            line_number,
            format!("missing_field:{field}"),
            format!("evidence field {field} must be non-empty when supplied"),
            field,
        ));
    }
    Ok(Some(value.to_string()))
}

fn parse_assertion_ancestry(
    source: &Map<String, Value>,
    line_number: usize,
) -> Result<Option<Value>, AssertionRowError> {
    let Some(value) = source.get("ancestry") else {
        return Ok(None);
    };
    let object = value.as_object().ok_or_else(|| {
        AssertionRowError::new(
            line_number,
            "non_scalar_value:source.ancestry",
            "source.ancestry must be an object",
            "source.ancestry.status and source.ancestry.origin_source_ids",
        )
    })?;
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .map(ascii_trim)
        .filter(|status| matches!(*status, "known" | "unknown"))
        .ok_or_else(|| {
            AssertionRowError::new(
                line_number,
                "invalid_value:source.ancestry.status",
                "source.ancestry.status must be known or unknown",
                "known or unknown",
            )
        })?;
    let ids = object
        .get("origin_source_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AssertionRowError::new(
                line_number,
                "missing_field:source.ancestry.origin_source_ids",
                "source.ancestry.origin_source_ids must be an array",
                "source.ancestry.origin_source_ids",
            )
        })?;
    let mut canonical_ids = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id
            .as_str()
            .map(ascii_trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AssertionRowError::new(
                    line_number,
                    "invalid_value:source.ancestry.origin_source_ids",
                    "every origin source id must be a non-empty string",
                    "non-empty strings",
                )
            })?;
        canonical_ids.push(Value::String(id.to_string()));
    }
    if status == "known" && canonical_ids.is_empty() {
        return Err(AssertionRowError::new(
            line_number,
            "missing_field:source.ancestry.origin_source_ids",
            "known ancestry must name at least one origin source id",
            "one or more origin source ids",
        ));
    }
    Ok(Some(json!({
        "status": status,
        "origin_source_ids": canonical_ids,
    })))
}

fn extension_value(value: &Value) -> Option<EvidenceExtensionValue> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(EvidenceExtensionValue::Bool(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Some(EvidenceExtensionValue::Int(value))
            } else if let Some(value) = value.as_u64() {
                Some(EvidenceExtensionValue::UInt(value))
            } else {
                Some(EvidenceExtensionValue::String(value.to_string()))
            }
        }
        Value::String(value) => Some(EvidenceExtensionValue::String(value.clone())),
        Value::Array(values) => {
            let mut converted = Vec::new();
            for value in values {
                if let Some(value) = extension_value(value) {
                    converted.push(value);
                }
            }
            Some(EvidenceExtensionValue::List(converted))
        }
        Value::Object(values) => {
            let mut converted = BTreeMap::new();
            for (key, value) in values {
                if let Some(value) = extension_value(value) {
                    converted.insert(key.clone(), value);
                }
            }
            if converted.is_empty() {
                None
            } else {
                Some(EvidenceExtensionValue::Object(converted))
            }
        }
    }
}

fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    fn append(value: &Value, bytes: &mut Vec<u8>) {
        match value {
            Value::Null => bytes.extend_from_slice(b"null"),
            Value::Bool(value) => bytes.extend_from_slice(if *value {
                b"true".as_slice()
            } else {
                b"false".as_slice()
            }),
            Value::Number(value) => bytes.extend_from_slice(value.to_string().as_bytes()),
            Value::String(value) => bytes.extend_from_slice(
                serde_json::to_string(value)
                    .expect("serializing a JSON string cannot fail")
                    .as_bytes(),
            ),
            Value::Array(values) => {
                bytes.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        bytes.push(b',');
                    }
                    append(value, bytes);
                }
                bytes.push(b']');
            }
            Value::Object(values) => {
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                bytes.push(b'{');
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        bytes.push(b',');
                    }
                    bytes.extend_from_slice(
                        serde_json::to_string(key)
                            .expect("serializing a JSON key cannot fail")
                            .as_bytes(),
                    );
                    bytes.push(b':');
                    append(&values[key], bytes);
                }
                bytes.push(b'}');
            }
        }
    }

    let mut bytes = Vec::new();
    append(value, &mut bytes);
    bytes
}

#[derive(Debug)]
struct ObservationRowError {
    line: usize,
    obs_id: Option<String>,
    code: String,
    message: String,
    fix: String,
}

impl ObservationRowError {
    fn into_rejection(self) -> SessionIngestRejection {
        SessionIngestRejection {
            line: self.line,
            obs_id: self.obs_id,
            code: self.code,
            message: self.message,
            fix: self.fix,
        }
    }

    fn new(
        line: usize,
        obs_id: Option<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            line,
            obs_id,
            code: code.into(),
            message: message.into(),
            fix: fix.into(),
        }
    }
}

fn canonicalize_observation_line(
    line: &str,
    line_number: usize,
) -> Result<CanonicalObservation, ObservationRowError> {
    let value = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
        ObservationRowError::new(
            line_number,
            None,
            "invalid_json",
            format!("observation line is not valid JSON: {error}"),
            "valid JSON object per line",
        )
    })?;
    let mut object = match value {
        serde_json::Value::Object(object) => object,
        _ => {
            return Err(ObservationRowError::new(
                line_number,
                None,
                "not_object",
                "observation line must be a JSON object",
                "JSON object per line",
            ));
        }
    };
    let candidate_obs_id = object
        .get("obs_id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let obs_id = required_trimmed_string(&mut object, "obs_id", line_number)?;
    required_trimmed_string(&mut object, "entity_type", line_number)?;
    let surface_value = object.get_mut("surface").ok_or_else(|| {
        ObservationRowError::new(
            line_number,
            Some(obs_id.clone()),
            "missing_field:surface",
            "observation field surface is required",
            "surface",
        )
    })?;
    let surface_text = surface_value.as_str().ok_or_else(|| {
        ObservationRowError::new(
            line_number,
            Some(obs_id.clone()),
            "non_scalar_value:surface",
            "observation field surface must be a string",
            "surface",
        )
    })?;
    let surface = ascii_trim(surface_text).to_string();
    if surface.is_empty() {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id),
            "empty_surface",
            "surface must be non-empty after ASCII-trim",
            "surface",
        ));
    }
    if surface.len() > crate::entity::evidence_ir::MAX_TEXT_BYTES {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id),
            "oversized_surface",
            format!(
                "surface exceeds the maximum text size of {} bytes",
                crate::entity::evidence_ir::MAX_TEXT_BYTES
            ),
            "surface",
        ));
    }
    *surface_value = serde_json::Value::String(surface);

    for field in ["keys", "attrs", "context"] {
        validate_scalar_object(&object, field, line_number, Some(obs_id.clone()))?;
    }
    let key_warnings = canonicalize_observation_keys(&mut object, line_number, &obs_id);

    let source = object.get_mut("source").ok_or_else(|| {
        ObservationRowError::new(
            line_number,
            Some(obs_id.clone()),
            "missing_field:source",
            "observation source is required",
            "source",
        )
    })?;
    let source_object = source.as_object_mut().ok_or_else(|| {
        ObservationRowError::new(
            line_number,
            Some(obs_id.clone()),
            "non_scalar_value:source",
            "source must be a JSON object",
            "source",
        )
    })?;
    let channel =
        required_trimmed_string(source_object, "channel", line_number).map_err(|mut error| {
            error.obs_id = Some(obs_id.clone());
            error
        })?;
    validate_observation_ancestry(source_object.get("ancestry"), line_number, &obs_id)?;
    if let Some(lineage_value) = source_object.get_mut("lineage") {
        let Some(lineage_text) = lineage_value.as_str() else {
            return Err(ObservationRowError::new(
                line_number,
                Some(obs_id),
                "non_scalar_value:source.lineage",
                "source.lineage must be a string when supplied",
                "source.lineage",
            ));
        };
        let trimmed = ascii_trim(lineage_text);
        if trimmed.is_empty() {
            return Err(ObservationRowError::new(
                line_number,
                Some(obs_id),
                "missing_field:source.lineage",
                "source.lineage must be non-empty when supplied",
                "source.lineage",
            ));
        }
        *lineage_value = serde_json::Value::String(trimmed.to_string());
    } else {
        let origin = source_object
            .get("table")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                source_object
                    .get("file")
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("<unknown>");
        source_object.insert(
            "lineage".to_string(),
            serde_json::Value::String(format!("{channel}:{origin}")),
        );
    }

    let bytes = serde_json::to_vec(&serde_json::Value::Object(object)).map_err(|error| {
        ObservationRowError::new(
            line_number,
            candidate_obs_id.or(Some(obs_id.clone())),
            "invalid_json",
            format!("failed to canonicalize observation JSON: {error}"),
            "valid JSON object per line",
        )
    })?;
    Ok(CanonicalObservation {
        obs_id,
        bytes,
        key_warnings,
    })
}

fn validate_observation_ancestry(
    value: Option<&Value>,
    line_number: usize,
    obs_id: &str,
) -> Result<(), ObservationRowError> {
    let Some(value) = value else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id.to_string()),
            "non_scalar_value:source.ancestry",
            "source.ancestry must be an object",
            "source.ancestry.status and source.ancestry.origin_source_ids",
        ));
    };
    let Some(status) = object.get("status").and_then(Value::as_str) else {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id.to_string()),
            "missing_field:source.ancestry.status",
            "source.ancestry.status must be known or unknown",
            "source.ancestry.status",
        ));
    };
    if !matches!(status, "known" | "unknown") {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id.to_string()),
            "invalid_value:source.ancestry.status",
            "source.ancestry.status must be known or unknown",
            "known or unknown",
        ));
    }
    let Some(ids) = object.get("origin_source_ids").and_then(Value::as_array) else {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id.to_string()),
            "missing_field:source.ancestry.origin_source_ids",
            "source.ancestry.origin_source_ids must be an array",
            "source.ancestry.origin_source_ids",
        ));
    };
    for id in ids {
        if id.as_str().is_none_or(|id| ascii_trim(id).is_empty()) {
            return Err(ObservationRowError::new(
                line_number,
                Some(obs_id.to_string()),
                "invalid_value:source.ancestry.origin_source_ids",
                "every origin source id must be a non-empty string",
                "non-empty strings",
            ));
        }
    }
    if status == "known" && ids.is_empty() {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id.to_string()),
            "missing_field:source.ancestry.origin_source_ids",
            "known ancestry must name at least one origin source id",
            "one or more origin source ids",
        ));
    }
    Ok(())
}

/// Validate each `keys` value against its namespace (`crate::keys`). A valid
/// key is stored in canonical form (so padded and unpadded CIKs agree); a key
/// that fails is removed from the observation and reported. Null and boolean
/// values are left untouched.
fn canonicalize_observation_keys(
    object: &mut Map<String, Value>,
    line_number: usize,
    obs_id: &str,
) -> Vec<SessionKeyWarning> {
    let mut warnings = Vec::new();
    let Some(Value::Object(keys)) = object.get_mut("keys") else {
        return warnings;
    };
    let mut dropped = Vec::new();
    for (namespace, value) in keys.iter_mut() {
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            _ => continue,
        };
        let key_namespace = KeyNamespace::from_name(namespace);
        match key_namespace.canonicalise(&text) {
            Ok(canonical) => *value = Value::String(canonical),
            Err(rejection) => {
                warnings.push(SessionKeyWarning {
                    line: line_number,
                    obs_id: obs_id.to_string(),
                    namespace: namespace.clone(),
                    value: text,
                    reason: rejection.reason().to_string(),
                    fix: format!("keys.{namespace}: {}", key_namespace.expected_form()),
                });
                dropped.push(namespace.clone());
            }
        }
    }
    for namespace in dropped {
        keys.remove(&namespace);
    }
    warnings
}

fn required_trimmed_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    line_number: usize,
) -> Result<String, ObservationRowError> {
    let Some(value) = object.get_mut(field) else {
        return Err(ObservationRowError::new(
            line_number,
            None,
            format!("missing_field:{field}"),
            format!("observation field {field} is required"),
            field,
        ));
    };
    let Some(text) = value.as_str() else {
        return Err(ObservationRowError::new(
            line_number,
            None,
            format!("non_scalar_value:{field}"),
            format!("observation field {field} must be a string"),
            field,
        ));
    };
    let trimmed = ascii_trim(text).to_string();
    if trimmed.is_empty() {
        return Err(ObservationRowError::new(
            line_number,
            None,
            format!("missing_field:{field}"),
            format!("observation field {field} must be non-empty after ASCII-trim"),
            field,
        ));
    }
    *value = serde_json::Value::String(trimmed.clone());
    Ok(trimmed)
}

fn validate_scalar_object(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    line_number: usize,
    obs_id: Option<String>,
) -> Result<(), ObservationRowError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    let Some(values) = value.as_object() else {
        return Err(ObservationRowError::new(
            line_number,
            obs_id,
            format!("non_scalar_value:{field}"),
            format!("observation field {field} must be an object of scalar values"),
            field,
        ));
    };
    for (key, value) in values {
        if matches!(
            value,
            serde_json::Value::Object(_) | serde_json::Value::Array(_)
        ) {
            return Err(ObservationRowError::new(
                line_number,
                obs_id.clone(),
                format!("non_scalar_value:{field}.{key}"),
                format!("observation value {field}.{key} must be scalar or null"),
                format!("{field}.{key}"),
            ));
        }
    }
    Ok(())
}

fn existing_observations(
    snapshot: &crate::entity::publication::EntityPublicationSnapshot,
) -> Result<ExistingObservations, Refusal> {
    let mut rows = BTreeMap::new();
    let mut shard = None;
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        if shard.is_none() {
            shard = Some(logical_path.to_string());
        }
        let bytes = snapshot
            .read_logical_file(logical_path)
            .ok_or_else(|| observation_artifact_refusal("observation shard is missing"))?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            observation_artifact_refusal(&format!("observation shard is not UTF-8: {error}"))
        })?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let value = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
                observation_artifact_refusal(&format!(
                    "observation shard contains invalid JSON: {error}"
                ))
            })?;
            let obs_id = value
                .get("obs_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| observation_artifact_refusal("observation shard row lacks obs_id"))?
                .to_string();
            let row_bytes = serde_json::to_vec(&value).map_err(|error| {
                observation_artifact_refusal(&format!(
                    "failed to canonicalize observation shard row: {error}"
                ))
            })?;
            if rows
                .insert(obs_id.clone(), row_bytes.clone())
                .is_some_and(|previous| previous != row_bytes)
            {
                return Err(observation_artifact_refusal(&format!(
                    "observation shard contains conflicting rows for obs_id {obs_id}"
                )));
            }
        }
    }
    Ok(ExistingObservations { rows, shard })
}

fn existing_evidence(
    snapshot: &crate::entity::publication::EntityPublicationSnapshot,
) -> Result<ExistingEvidence, Refusal> {
    let mut records = BTreeMap::new();
    let mut shard = None;
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("evidence/") && path.ends_with(".jsonl"))
    {
        if shard.is_none() {
            shard = Some(logical_path.to_string());
        }
        let bytes = snapshot
            .read_logical_file(logical_path)
            .ok_or_else(|| evidence_artifact_refusal("evidence shard is missing"))?;
        let text = std::str::from_utf8(bytes).map_err(|error| {
            evidence_artifact_refusal(&format!("evidence shard is not UTF-8: {error}"))
        })?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let record = serde_json::from_str::<EvidenceRecord>(line).map_err(|error| {
                evidence_artifact_refusal(&format!(
                    "evidence shard contains invalid canon.evidence.v1 JSON: {error}"
                ))
            })?;
            let canonical = canonicalize_record(record).map_err(|error| {
                evidence_artifact_refusal(&format!(
                    "evidence shard contains non-canonical canon.evidence.v1 record: {error}"
                ))
            })?;
            let evidence_id = canonical.evidence_id.clone();
            let row_bytes = serde_json::to_vec(&canonical).map_err(|error| {
                evidence_artifact_refusal(&format!(
                    "failed to canonicalize evidence shard record: {error}"
                ))
            })?;
            if records
                .insert(evidence_id.clone(), row_bytes.clone())
                .is_some_and(|previous| previous != row_bytes)
            {
                return Err(evidence_artifact_refusal(&format!(
                    "evidence shard contains conflicting records for evidence_id {evidence_id}"
                )));
            }
        }
    }
    Ok(ExistingEvidence { records, shard })
}

fn canonical_shard_bytes(rows: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut entries = rows.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    for (_, row) in entries {
        bytes.extend_from_slice(row);
        bytes.push(b'\n');
    }
    bytes
}

fn read_observation_input(path: &Path) -> Result<String, Refusal> {
    read_session_jsonl_input(path, "Observation")
}

fn read_evidence_input(path: &Path) -> Result<String, Refusal> {
    read_session_jsonl_input(path, "Evidence")
}

fn read_session_jsonl_input(path: &Path, label: &str) -> Result<String, Refusal> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        let stdin = io::stdin();
        let mut locked = stdin.lock();
        locked
            .read_to_end(&mut bytes)
            .map_err(|error| io_refusal(path, error))?;
    } else {
        bytes = fs::read(path).map_err(|error| io_refusal(path, error))?;
    }
    String::from_utf8(bytes).map_err(|error| Refusal {
        code: RefusalCode::EEncoding,
        message: format!("{label} JSONL '{}' is not valid UTF-8", path.display()),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false,
        }),
        next_command: Some(format!(
            "Convert the {label} JSONL to UTF-8, then rerun the same command"
        )),
    })
}

fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_ascii_whitespace())
}

fn session_not_initialized_refusal(work_dir: &Path) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: format!(
            "No initialized evidence session exists at '{}'",
            work_dir.display()
        ),
        detail: json!({
            "session": work_dir.display().to_string(),
            "writes_performed": false,
        }),
        next_command: Some(format!(
            "Run canon entity session init --work-dir {} before ingest",
            work_dir.display()
        )),
    }
}

fn observation_artifact_refusal(message: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.to_string(),
        detail: json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
        next_command: Some(
            "Repair or choose a new session, then rerun canon entity session ingest".to_string(),
        ),
    }
}

fn observation_publication_refusal(message: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to publish session observations: {message}"),
        detail: json!({"stream": SESSION_OBSERVATIONS_STREAM_ID, "writes_performed": false}),
        next_command: Some("Rerun the same canon entity session ingest command".to_string()),
    }
}

fn evidence_artifact_refusal(message: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: message.to_string(),
        detail: json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
        next_command: Some(
            "Repair or choose a new session, then rerun canon entity session assert".to_string(),
        ),
    }
}

fn evidence_publication_refusal(message: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to publish session evidence: {message}"),
        detail: json!({"stream": SESSION_EVIDENCE_STREAM_ID, "writes_performed": false}),
        next_command: Some("Rerun the same canon entity session assert command".to_string()),
    }
}

fn output(
    status: &str,
    manifest: SessionManifest,
    work_dir: &Path,
    generation_id: String,
    registry_initialized: bool,
    registry_path_updated: bool,
) -> SessionInitOutput {
    let next_command = format!(
        "canon entity session ingest {} <OBSERVATIONS.jsonl|-> --emit json",
        work_dir.display()
    );
    SessionInitOutput {
        version: "canon.entity.session.init.v1".to_string(),
        status: status.to_string(),
        session_id: manifest.session_id,
        goal: manifest.goal,
        work_dir: work_dir.display().to_string(),
        registries: manifest.registries,
        generation_id,
        registry_initialized,
        registry_path_updated,
        next_command,
    }
}

fn ensure_work_dir(work_dir: &Path) -> Result<(), Refusal> {
    match fs::symlink_metadata(work_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(path_refusal(
            work_dir,
            "Session work directory must not be a symlink",
            "Choose a regular directory for --work-dir, then rerun",
        )),
        Ok(metadata) if !metadata.is_dir() => Err(path_refusal(
            work_dir,
            "Session work directory must be a directory",
            "Choose a directory for --work-dir, then rerun",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(work_dir).map_err(|error| io_refusal(work_dir, error))
        }
        Err(error) => Err(io_refusal(work_dir, error)),
    }
}

fn validate_existing_session(
    current: Option<&SessionManifest>,
    work_dir: &Path,
    goal: &str,
    entity_kind: &str,
    requested_registry_id: Option<&str>,
) -> Result<(), Refusal> {
    let Some(current) = current else {
        return Ok(());
    };
    if current.goal != goal {
        return Err(session_conflict_refusal(
            work_dir,
            "goal",
            goal,
            &current.goal,
        ));
    }
    let current_kind = current
        .registries
        .first()
        .map(|registry| registry.kind.as_str());
    if current_kind != Some(entity_kind) {
        return Err(session_conflict_refusal(
            work_dir,
            "entity_kind",
            entity_kind,
            current_kind.unwrap_or("<missing>"),
        ));
    }
    if let Some(requested_registry_id) = requested_registry_id
        && current
            .registries
            .first()
            .is_some_and(|registry| registry.id != requested_registry_id)
    {
        let current_id = current
            .registries
            .first()
            .map(|registry| registry.id.as_str())
            .unwrap_or("<missing>");
        return Err(session_conflict_refusal(
            work_dir,
            "registry_id",
            requested_registry_id,
            current_id,
        ));
    }
    Ok(())
}

fn validate_existing_registry(registry: &Path) -> Result<(), Refusal> {
    let lint = registry_lint::lint(registry, registry_lint::RegistryLintProfile::Standard)?;
    if lint.summary.errors == 0 {
        return Ok(());
    }
    Err(Refusal {
        code: RefusalCode::EBadRegistry,
        message: format!("Registry '{}' is invalid", registry.display()),
        detail: json!({"registry": registry.display().to_string(), "lint": lint}),
        next_command: Some(format!(
            "Fix the registry, then rerun canon entity session init --registry {}",
            registry.display()
        )),
    })
}

fn read_registry_id(registry: &Path) -> Result<String, Refusal> {
    let path = registry.join("registry.json");
    let bytes = fs::read(&path).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to read registry.json: {error}"),
        )
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        Refusal::bad_registry(
            &registry.display().to_string(),
            &format!("Failed to parse registry.json: {error}"),
        )
    })?;
    let Some(id) = value.get("id").and_then(|value| value.as_str()) else {
        return Err(Refusal::bad_registry(
            &registry.display().to_string(),
            "registry.json must contain a non-empty id",
        ));
    };
    if id.trim().is_empty() {
        return Err(Refusal::bad_registry(
            &registry.display().to_string(),
            "registry.json id must not be empty",
        ));
    }
    Ok(id.to_string())
}

fn read_current_manifest(work_dir: &Path) -> Result<Option<SessionManifest>, Refusal> {
    let snapshot = match open_current_stream_generation(work_dir, SESSION_MANIFEST_STREAM_ID) {
        Ok(snapshot) => snapshot,
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => {
            return Ok(None);
        }
        Err(error) => return Err(publication_refusal(error)),
    };
    let bytes = snapshot
        .read_logical_file("session/session.json")
        .ok_or_else(|| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Session manifest stream has no session/session.json".to_string(),
            detail: json!({"work_dir": work_dir.display().to_string(), "writes_performed": false}),
            next_command: Some(
                "Choose a new --work-dir or repair the session publication".to_string(),
            ),
        })?;
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| Refusal {
            code: RefusalCode::EEntityArtifactContract,
            message: "Session manifest is not valid JSON".to_string(),
            detail: json!({
                "work_dir": work_dir.display().to_string(),
                "error": error.to_string(),
                "writes_performed": false,
            }),
            next_command: Some(
                "Choose a new --work-dir or repair the session publication".to_string(),
            ),
        })
}

fn current_generation_id(work_dir: &Path) -> Result<String, Refusal> {
    current_generation_id_optional(work_dir)?.ok_or_else(|| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: "Session manifest generation is missing".to_string(),
        detail: json!({"work_dir": work_dir.display().to_string(), "writes_performed": false}),
        next_command: Some("Choose a new --work-dir or repair the session publication".to_string()),
    })
}

fn current_generation_id_optional(work_dir: &Path) -> Result<Option<String>, Refusal> {
    match open_current_stream_generation(work_dir, SESSION_MANIFEST_STREAM_ID) {
        Ok(snapshot) => Ok(Some(snapshot.generation_id)),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => Ok(None),
        Err(error) => Err(publication_refusal(error)),
    }
}

fn session_id(goal: &str, entity_kind: &str, registry_id: &str) -> Result<String, Refusal> {
    let identity = json!({
        "entity_kind": entity_kind,
        "evidence_vocabulary_hash": evidence_vocabulary_hash(),
        "goal": goal,
        "registry_id": registry_id,
        "version": SESSION_MANIFEST_VERSION,
    });
    let bytes = serde_json::to_vec(&identity).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: "Failed to serialize session identity".to_string(),
        detail: json!({"error": error.to_string(), "writes_performed": false}),
        next_command: Some("Rerun canon entity session init".to_string()),
    })?;
    Ok(format!("session:blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn evidence_vocabulary_hash() -> String {
    hash_bytes(EVIDENCE_VOCABULARY.as_bytes())
}

fn pretty_manifest_bytes(manifest: &SessionManifest) -> Result<Vec<u8>, Refusal> {
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: "Failed to serialize session manifest".to_string(),
        detail: json!({"error": error.to_string(), "writes_performed": false}),
        next_command: Some("Rerun canon entity session init".to_string()),
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_non_empty(path: &Path, field: &str, value: &str) -> Result<String, Refusal> {
    let trimmed = value.trim();
    if !trimmed.is_empty() && trimmed == value {
        return Ok(trimmed.to_string());
    }
    Err(Refusal {
        code: RefusalCode::EParse,
        message: format!("{field} must be non-empty and ASCII-trimmed"),
        detail: json!({"field": field, "value": value, "path": path.display().to_string(), "writes_performed": false}),
        next_command: Some(format!(
            "Provide a valid {field}, then rerun canon entity session init"
        )),
    })
}

fn missing_registry_flag_refusal(registry: &Path, flag: &str, message: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.to_string(),
        detail: json!({
            "registry": registry.display().to_string(),
            "flag": flag,
            "writes_performed": false,
        }),
        next_command: Some(format!(
            "Rerun canon entity session init with {flag} <VALUE>"
        )),
    }
}

fn session_conflict_refusal(
    work_dir: &Path,
    field: &str,
    requested: &str,
    existing: &str,
) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: format!("Session init arguments differ for {field}"),
        detail: json!({
            "work_dir": work_dir.display().to_string(),
            "field": field,
            "requested": requested,
            "existing": existing,
            "writes_performed": false,
        }),
        next_command: Some(
            "Use the original session arguments or choose a new --work-dir".to_string(),
        ),
    }
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn absolute_path(path: &Path) -> Result<PathBuf, Refusal> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| io_refusal(path, error))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn path_refusal(path: &Path, message: &str, next: &str) -> Refusal {
    Refusal {
        code: RefusalCode::EParse,
        message: message.to_string(),
        detail: json!({"path": path.display().to_string(), "writes_performed": false}),
        next_command: Some(next.to_string()),
    }
}

fn io_refusal(path: &Path, error: std::io::Error) -> Refusal {
    Refusal {
        code: RefusalCode::EIo,
        message: format!("Unable to access '{}': {error}", path.display()),
        detail: json!({"path": path.display().to_string(), "error": error.to_string(), "writes_performed": false}),
        next_command: Some("Check paths and permissions, then rerun the same command".to_string()),
    }
}

fn publication_refusal(error: EntityPublicationError) -> Refusal {
    Refusal {
        code: RefusalCode::EEntityArtifactContract,
        message: format!("Unable to publish the session manifest: {}", error.message),
        detail: json!({
            "kind": format!("{:?}", error.kind),
            "writes_performed": error.writes_performed,
            "committed": error.committed,
            "generation_id": error.generation_id,
        }),
        next_command: Some("Rerun the same canon entity session init command".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SESSION_ASSERT_VERSION, SESSION_INGEST_VERSION, SessionCommitRelation,
        SessionCommitRelationEndpoint, build_relation_fact, canonical_shard_bytes,
        canonicalize_assertion_line, canonicalize_observation_line, hash_bytes,
    };
    use crate::entity::evidence_ir::EvidenceKind;
    use crate::temporal::relation::{
        IntervalBoundary, RelationIdentityImplicationMode, RelationKindRef, ReviewDisposition,
        TimeInterval, relation_schema_version,
    };
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[test]
    fn observation_rows_trim_surface_and_default_lineage_without_normalising_backticks() {
        let row = canonicalize_observation_line(
            r#"{"obs_id":"obs-1","entity_type":"organisation","surface":"  `  ","keys":{"cik":null},"source":{"channel":"file","file":"rows.jsonl"}}"#,
            1,
        )
        .expect("valid observation row");
        let value: Value = serde_json::from_slice(&row.bytes).expect("canonical row JSON");
        assert_eq!(row.obs_id, "obs-1");
        assert_eq!(value["surface"], "`");
        assert_eq!(value["source"]["lineage"], "file:rows.jsonl");
        assert_eq!(value["keys"]["cik"], Value::Null);
        assert_eq!(SESSION_INGEST_VERSION, "canon.entity.session.ingest.v1");
    }

    #[test]
    fn observation_row_classification_keeps_planted_errors_individual() {
        let invalid = canonicalize_observation_line("{not-json", 2).expect_err("invalid JSON");
        assert_eq!(invalid.code, "invalid_json");
        let empty = canonicalize_observation_line(
            r#"{"obs_id":"obs-2","entity_type":"organisation","surface":"  ","source":{"channel":"file"}}"#,
            7,
        )
        .expect_err("empty surface");
        assert_eq!(empty.code, "empty_surface");
        assert_eq!(empty.line, 7);
        assert_eq!(empty.fix, "surface");
    }

    #[test]
    fn canonical_shard_bytes_are_independent_of_input_order() {
        let first = canonicalize_observation_line(
            r#"{"obs_id":"obs-b","entity_type":"organisation","surface":"B","source":{"channel":"file"}}"#,
            1,
        )
        .expect("row b");
        let second = canonicalize_observation_line(
            r#"{"obs_id":"obs-a","entity_type":"organisation","surface":"A","source":{"channel":"file"}}"#,
            2,
        )
        .expect("row a");
        let mut forward = BTreeMap::new();
        forward.insert(first.obs_id.clone(), first.bytes.clone());
        forward.insert(second.obs_id.clone(), second.bytes.clone());
        let mut reverse = BTreeMap::new();
        reverse.insert(second.obs_id, second.bytes);
        reverse.insert(first.obs_id, first.bytes);
        assert_eq!(
            canonical_shard_bytes(&forward),
            canonical_shard_bytes(&reverse)
        );
    }

    #[test]
    fn short_assertion_types_compile_to_typed_evidence_records() {
        let policy_hash = hash_bytes(b"session-policy");
        let cases = [
            ("same_key", EvidenceKind::PairSupport, "same_key"),
            ("same_slot", EvidenceKind::PairSupport, "same_slot"),
            (
                "attribute_agreement",
                EvidenceKind::PairSupport,
                "attribute_agreement",
            ),
            (
                "name_equivalent",
                EvidenceKind::PairSupport,
                "name_equivalent",
            ),
            (
                "asserted_alias",
                EvidenceKind::PairSupport,
                "asserted_alias",
            ),
            ("distinct", EvidenceKind::AntiMergeVeto, "distinct"),
            ("related", EvidenceKind::ContextOnly, "related:affiliate_of"),
        ];
        for (assertion_type, expected_kind, expected_reason) in cases {
            let basis = match assertion_type {
                "distinct" => r#"{"authority":"reviewed"}"#,
                "related" => r#"{"relation":"affiliate_of","subject":"left"}"#,
                _ => r#"{"rule":"declared"}"#,
            };
            let source = if assertion_type == "asserted_alias" {
                r#"{"channel":"script","id":"gold","locator":"row:1","content_hash":"blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
            } else {
                r#"{"channel":"agent","id":"agent-1"}"#
            };
            let line = format!(
                r#"{{"type":"{assertion_type}","left":"left","right":"right","basis":{basis},"source":{source},"asserted_by":"agent:runner"}}"#
            );
            let record = canonicalize_assertion_line(&line, 1, &policy_hash)
                .expect("short assertion compiles")
                .bytes;
            let value: Value = serde_json::from_slice(&record).expect("canonical evidence JSON");
            assert_eq!(value["version"], "canon.evidence.v1");
            assert_eq!(value["kind"], serde_json::to_value(expected_kind).unwrap());
            assert_eq!(value["reason_code"], expected_reason);
            assert_eq!(value["operator"]["namespace"], "session");
            assert_eq!(value["target"]["left_id"], "left");
            assert_eq!(value["target"]["right_id"], "right");
        }
        assert_eq!(SESSION_ASSERT_VERSION, "canon.entity.session.assert.v1");
    }

    #[test]
    fn short_assertion_evidence_id_is_stable_under_key_reordering() {
        let policy_hash = hash_bytes(b"session-policy");
        let first = r#"{"type":"name_equivalent","left":"left","right":"right","basis":{"rule":"casefold","left_value":"LEFT","right_value":"right"},"source":{"channel":"script","id":"gold"},"asserted_by":"script:runner"}"#;
        let second = r#"{"asserted_by":"script:runner","source":{"id":"gold","channel":"script"},"right":"right","basis":{"right_value":"right","left_value":"LEFT","rule":"casefold"},"left":"left","type":"name_equivalent"}"#;
        let first = canonicalize_assertion_line(first, 1, &policy_hash).expect("first assertion");
        let second =
            canonicalize_assertion_line(second, 1, &policy_hash).expect("reordered assertion");
        assert_eq!(first.evidence_id, second.evidence_id);
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn short_assertion_rejects_missing_distinct_authority() {
        let error = canonicalize_assertion_line(
            r#"{"type":"distinct","left":"left","right":"right","basis":{"rule":"different"},"source":{"channel":"agent","id":"agent-1"},"asserted_by":"agent:runner"}"#,
            4,
            &hash_bytes(b"session-policy"),
        )
        .expect_err("distinct without authority must be rejected");
        assert_eq!(error.line, 4);
        assert_eq!(error.code, "missing_field:authority");
    }

    #[test]
    fn session_relation_fact_is_typed_non_identity_and_canonical() {
        let relation = SessionCommitRelation {
            subject: SessionCommitRelationEndpoint {
                canonical_id: Some("ORG-001".to_string()),
                cluster_id: None,
                external_id: None,
                entity_type: "organisation".to_string(),
            },
            relation: "part_of".to_string(),
            object: SessionCommitRelationEndpoint {
                canonical_id: None,
                cluster_id: None,
                external_id: Some("deal-7".to_string()),
                entity_type: "deal".to_string(),
            },
            valid_time: TimeInterval {
                start_at: Some("2024-01-01T00:00:00Z".to_string()),
                start_bound: IntervalBoundary::Inclusive,
                end_at: Some("2024-12-31T00:00:00Z".to_string()),
                end_bound: IntervalBoundary::Inclusive,
            },
            evidence_ids: vec!["evidence-1".to_string()],
        };
        let fact = build_relation_fact(&relation, "session-1", "solve-1", &BTreeMap::new())
            .expect("valid relation")
            .expect("committed endpoints");

        assert_eq!(fact.version, relation_schema_version());
        assert_eq!(fact.subject.identity_id, "ORG-001");
        assert_eq!(fact.object.identity_id, "deal-7");
        assert!(matches!(
            fact.relation,
            RelationKindRef::Extension { value, .. } if value == "part_of"
        ));
        assert_eq!(fact.provenance.source_system, "canon.entity.session");
        assert_eq!(fact.provenance.locator, "session:session-1:solve:solve-1");
        assert_eq!(fact.provenance.fragment.as_deref(), Some("evidence-1"));
        assert!(matches!(
            fact.identity_implication.mode,
            RelationIdentityImplicationMode::None
        ));
        assert_eq!(fact.review.disposition, ReviewDisposition::Related);
    }

    #[test]
    fn session_relation_fact_rejects_unknown_relation_names() {
        let relation = SessionCommitRelation {
            subject: SessionCommitRelationEndpoint {
                canonical_id: Some("ORG-001".to_string()),
                cluster_id: None,
                external_id: None,
                entity_type: "organisation".to_string(),
            },
            relation: "same_as".to_string(),
            object: SessionCommitRelationEndpoint {
                canonical_id: Some("ORG-002".to_string()),
                cluster_id: None,
                external_id: None,
                entity_type: "organisation".to_string(),
            },
            valid_time: TimeInterval::default(),
            evidence_ids: Vec::new(),
        };

        assert!(build_relation_fact(&relation, "session-1", "solve-1", &BTreeMap::new()).is_err());
    }
}

#[cfg(test)]
mod commit_effect_tests {
    use super::*;
    use crate::entity::runtime::{CannotLinkFact, PendingClusterRecord};

    fn pending(id: &str) -> PendingClusterRecord {
        PendingClusterRecord {
            escrow_id: id.to_string(),
            profile: "p".to_string(),
            doc_ids: Vec::new(),
            surfaces: vec!["A".to_string(), "B".to_string()],
            anchors: Vec::new(),
            witness_pairs: Vec::new(),
            state: "pending".to_string(),
        }
    }

    #[test]
    fn nothing_written_is_not_a_commit() {
        assert!(!commit_has_effect(false, 0, false));
    }

    #[test]
    fn aliases_relations_or_substantive_memory_each_make_a_commit() {
        assert!(commit_has_effect(true, 0, false));
        assert!(commit_has_effect(false, 1, false));
        assert!(commit_has_effect(false, 0, true));
    }

    #[test]
    fn a_pending_record_parked_for_a_contradiction_alone_is_not_substantive() {
        let memory = SessionRegistryMemoryWrite {
            pending: vec![pending("contradiction-only")],
            ..SessionRegistryMemoryWrite::default()
        };
        assert!(!memory_is_substantive(&memory));
    }

    #[test]
    fn review_pending_cleared_pending_and_cannot_link_are_substantive() {
        let review = SessionRegistryMemoryWrite {
            pending: vec![pending("from-review")],
            review_pending: vec!["from-review".to_string()],
            ..SessionRegistryMemoryWrite::default()
        };
        assert!(memory_is_substantive(&review));
        let cleared = SessionRegistryMemoryWrite {
            pending_clear: vec!["old".to_string()],
            ..SessionRegistryMemoryWrite::default()
        };
        assert!(memory_is_substantive(&cleared));
        let cannot = SessionRegistryMemoryWrite {
            cannot_link: vec![CannotLinkFact {
                left_key: "a".to_string(),
                right_key: "b".to_string(),
                reason: "distinct".to_string(),
            }],
            ..SessionRegistryMemoryWrite::default()
        };
        assert!(memory_is_substantive(&cannot));
        assert!(!memory_is_substantive(
            &SessionRegistryMemoryWrite::default()
        ));
    }
}
