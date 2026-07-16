use crate::witness::hash_bytes;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const CANON_ENTITY_STAGE_PUBLICATION_VERSION: &str = "canon_entity_stage_publication.v1";
pub const ENTITY_PUBLICATION_ROOT: &str = ".canon/entity/publications/v1";

static ATTEMPT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPublicationFileInput {
    pub logical_path: String,
    pub stage: String,
    pub version: String,
    pub bytes: Vec<u8>,
}

impl EntityPublicationFileInput {
    pub fn new(
        logical_path: impl Into<String>,
        stage: impl Into<String>,
        version: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            logical_path: logical_path.into(),
            stage: stage.into(),
            version: version.into(),
            bytes: bytes.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPublicationUpstreamRef {
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPublicationRequest {
    pub stream_id: String,
    pub supersedes_generation_id: Option<String>,
    pub request_fingerprint: String,
    pub cache_mode: String,
    pub cache_status: String,
    pub cache_receipt_hash: String,
    pub stage_order: Vec<String>,
    pub upstream_artifacts: Vec<EntityPublicationUpstreamRef>,
    pub files: Vec<EntityPublicationFileInput>,
    pub omit_logical_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPublicationFileRecord {
    pub logical_path: String,
    pub stage: String,
    pub version: String,
    pub byte_len: u64,
    pub content_hash: String,
    pub object_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPublicationManifest {
    pub version: String,
    pub generation_id: String,
    pub manifest_content_hash: String,
    pub stream_id: String,
    pub supersedes_generation_id: Option<String>,
    pub request_fingerprint: String,
    pub cache_mode: String,
    pub cache_status: String,
    pub cache_receipt_hash: String,
    pub stage_order: Vec<String>,
    pub upstream_artifacts: Vec<EntityPublicationUpstreamRef>,
    pub files: Vec<EntityPublicationFileRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityPublicationOutcome {
    Committed,
    AlreadyCommitted,
    CommitUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityPublicationReceipt {
    pub version: String,
    pub generation_id: String,
    pub outcome: EntityPublicationOutcome,
    pub writes_performed: bool,
    pub committed: Option<bool>,
    pub manifest_path: String,
    pub commit_marker_path: String,
    pub object_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityPublicationOptions {
    pub failpoint: EntityPublicationFailpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EntityPublicationFailpoint {
    #[default]
    None,
    BeforeObjectDirectoryCreate,
    AfterObjectDirectoryCreate,
    BeforeObjectAttemptCreate,
    AfterObjectAttemptCreate,
    AfterObjectWrite,
    AfterObjectFlush,
    AfterObjectSync,
    BeforeObjectRename,
    AfterObjectRenameBeforeDirectorySync,
    BeforeCommitDirectoryCreate,
    AfterCommitDirectoryCreate,
    BeforeCommitMarker,
    AfterCommitMarkerBeforeParentSync,
    BeforeClaimDirectoryCreate,
    AfterClaimDirectoryCreate,
    BeforeClaimCreate,
    AfterClaimCreate,
    AfterClaimWrite,
    AfterClaimFlush,
    AfterClaimSync,
    BeforeClaimPublish,
    AfterClaimPublishBeforeParentSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPublicationSnapshot {
    pub generation_id: String,
    pub manifest: EntityPublicationManifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl EntityPublicationSnapshot {
    pub fn read_logical_file(&self, logical_path: &str) -> Option<&[u8]> {
        self.files.get(logical_path).map(Vec::as_slice)
    }

    pub fn logical_paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityPublicationErrorKind {
    InvalidRequest,
    InvalidPath,
    Io,
    Parse,
    HashMismatch,
    UncommittedGeneration,
    CorruptGeneration,
    ForkedGeneration,
    CommitUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPublicationError {
    pub kind: EntityPublicationErrorKind,
    pub message: String,
    pub writes_performed: bool,
    pub committed: Option<bool>,
    pub generation_id: Option<String>,
}

impl EntityPublicationError {
    fn new(
        kind: EntityPublicationErrorKind,
        message: impl Into<String>,
        writes_performed: bool,
        committed: Option<bool>,
        generation_id: Option<String>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            writes_performed,
            committed,
            generation_id,
        }
    }
}

impl fmt::Display for EntityPublicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for EntityPublicationError {}

pub fn publish_generation(
    work_dir: &Path,
    request: EntityPublicationRequest,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    publish_generation_with_options(work_dir, request, EntityPublicationOptions::default())
}

pub fn publish_generation_with_options(
    work_dir: &Path,
    request: EntityPublicationRequest,
    options: EntityPublicationOptions,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    let publication = PlannedPublication::new(request)?;
    publish_planned_generation(work_dir, publication, options)
}

pub fn publish_stream_patch(
    work_dir: &Path,
    request: EntityPublicationRequest,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    publish_stream_patch_with_options(work_dir, request, EntityPublicationOptions::default())
}

pub fn publish_stream_patch_with_options(
    work_dir: &Path,
    mut request: EntityPublicationRequest,
    options: EntityPublicationOptions,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    validate_publication_token(&request.stream_id, "stream_id")?;
    let current = match open_current_stream_generation(work_dir, &request.stream_id) {
        Ok(snapshot) => Some(snapshot),
        Err(err) if err.kind == EntityPublicationErrorKind::UncommittedGeneration => None,
        Err(err) => return Err(err),
    };

    let Some(current) = current else {
        if let Some(supersedes_generation_id) = &request.supersedes_generation_id {
            validate_hash(supersedes_generation_id)?;
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                "publication stream patch cannot supersede a missing current head",
                false,
                Some(false),
                Some(supersedes_generation_id.clone()),
            ));
        }
        if let Some(logical_path) = request.omit_logical_paths.first() {
            validate_logical_path(logical_path)?;
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                format!(
                    "publication stream patch cannot omit logical path {logical_path} without a current head"
                ),
                false,
                Some(false),
                None,
            ));
        }
        let publication = PlannedPublication::new(request)?;
        let claim = claim_stream_lineage(
            work_dir,
            &publication.manifest.stream_id,
            None,
            &publication.manifest.generation_id,
            options.failpoint,
        )?;
        let mut receipt = publish_planned_generation(work_dir, publication, options)
            .map_err(|err| claim.apply_to_error(err))?;
        claim.apply_to_receipt(&mut receipt);
        return validate_stream_receipt_lineage(work_dir, receipt);
    };

    if let Some(supersedes_generation_id) = &request.supersedes_generation_id {
        validate_hash(supersedes_generation_id)?;
        if supersedes_generation_id != &current.generation_id {
            let requested_parent = supersedes_generation_id.clone();
            let claim_path =
                lineage_claim_path(work_dir, &request.stream_id, Some(&requested_parent))?;
            if claim_path.is_dir() {
                let publication = PlannedPublication::new(request)?;
                let claim = claim_stream_lineage(
                    work_dir,
                    &publication.manifest.stream_id,
                    Some(&requested_parent),
                    &publication.manifest.generation_id,
                    options.failpoint,
                )?;
                let mut err = EntityPublicationError::new(
                    EntityPublicationErrorKind::InvalidRequest,
                    format!(
                        "publication stream patch expected current head {requested_parent} but found {}",
                        current.generation_id
                    ),
                    false,
                    Some(true),
                    Some(current.generation_id.clone()),
                );
                err = claim.apply_to_error(err);
                return Err(err);
            }
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                format!(
                    "publication stream patch expected current head {requested_parent} but found {}",
                    current.generation_id
                ),
                false,
                Some(true),
                Some(current.generation_id.clone()),
            ));
        }
    }
    request.supersedes_generation_id = Some(current.generation_id.clone());
    let request = request_with_carried_forward_files(request, &current)?;
    let publication = PlannedPublication::new(request)?;
    if stream_patch_matches_current_head(&publication, &current) {
        return Ok(receipt_for_manifest(
            work_dir,
            &current.manifest,
            EntityPublicationOutcome::AlreadyCommitted,
            false,
            Some(true),
        ));
    }
    let claim = claim_stream_lineage(
        work_dir,
        &publication.manifest.stream_id,
        Some(&current.generation_id),
        &publication.manifest.generation_id,
        options.failpoint,
    )?;
    let mut receipt = publish_planned_generation(work_dir, publication, options)
        .map_err(|err| claim.apply_to_error(err))?;
    claim.apply_to_receipt(&mut receipt);
    validate_stream_receipt_lineage(work_dir, receipt)
}

fn publish_planned_generation(
    work_dir: &Path,
    publication: PlannedPublication,
    options: EntityPublicationOptions,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    let generation_id = publication.manifest.generation_id.clone();

    if commit_marker_path(work_dir, &generation_id).is_dir() {
        open_committed_generation(work_dir, &generation_id)?;
        return Ok(publication.receipt(
            work_dir,
            EntityPublicationOutcome::AlreadyCommitted,
            false,
            Some(true),
        ));
    }

    let mut writes_performed = false;
    for file in &publication.input_files {
        let write = write_content_object(
            work_dir,
            &generation_id,
            &hash_bytes(&file.bytes),
            &file.bytes,
            options.failpoint,
        )?;
        writes_performed |= write.writes_performed;
    }

    let manifest_bytes = publication.manifest_bytes()?;
    let write = write_content_object(
        work_dir,
        &generation_id,
        &generation_id,
        &manifest_bytes,
        options.failpoint,
    )?;
    writes_performed |= write.writes_performed;
    validate_manifest_bundle(work_dir, &publication.manifest)?;

    if options.failpoint == EntityPublicationFailpoint::BeforeCommitMarker {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before commit marker",
            writes_performed,
            Some(false),
            Some(generation_id),
        ));
    }

    let marker_parent = commit_marker_parent(work_dir);
    if options.failpoint == EntityPublicationFailpoint::BeforeCommitDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before commit marker directory creation",
            writes_performed,
            Some(false),
            Some(generation_id),
        ));
    }
    ensure_directory_synced(&marker_parent).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create commit marker parent: {err}"),
            writes_performed,
            Some(false),
            Some(generation_id.clone()),
        )
    })?;
    writes_performed = true;
    if options.failpoint == EntityPublicationFailpoint::AfterCommitDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after commit marker directory creation",
            writes_performed,
            Some(false),
            Some(generation_id),
        ));
    }

    match fs::create_dir(commit_marker_path(work_dir, &generation_id)) {
        Ok(()) => {
            if options.failpoint == EntityPublicationFailpoint::AfterCommitMarkerBeforeParentSync {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::CommitUnknown,
                    "publication commit marker exists but parent sync was not confirmed",
                    writes_performed,
                    None,
                    Some(generation_id),
                ));
            }
            sync_directory(&marker_parent).map_err(|err| {
                EntityPublicationError::new(
                    EntityPublicationErrorKind::CommitUnknown,
                    format!("failed to sync commit marker parent: {err}"),
                    writes_performed,
                    None,
                    Some(generation_id.clone()),
                )
            })?;
            open_committed_generation(work_dir, &generation_id)?;
            Ok(publication.receipt(
                work_dir,
                EntityPublicationOutcome::Committed,
                writes_performed,
                Some(true),
            ))
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            open_committed_generation(work_dir, &generation_id)?;
            Ok(publication.receipt(
                work_dir,
                EntityPublicationOutcome::AlreadyCommitted,
                writes_performed,
                Some(true),
            ))
        }
        Err(err) => Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create commit marker: {err}"),
            writes_performed,
            Some(false),
            Some(generation_id),
        )),
    }
}

pub fn open_current_stream_generation(
    work_dir: &Path,
    stream_id: &str,
) -> Result<EntityPublicationSnapshot, EntityPublicationError> {
    validate_publication_token(stream_id, "stream_id")?;
    let root_claim_path = lineage_claim_path(work_dir, stream_id, None)?;
    if !root_claim_path.is_dir() {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::UncommittedGeneration,
            format!("publication stream {stream_id} has no committed generation"),
            false,
            Some(false),
            None,
        ));
    }

    let root_generation_id = read_lineage_claim_child(&root_claim_path)?;
    if !commit_marker_path(work_dir, &root_generation_id).is_dir() {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::UncommittedGeneration,
            format!("publication stream {stream_id} root generation is not committed"),
            false,
            Some(false),
            Some(root_generation_id),
        ));
    }
    let mut current = open_committed_generation(work_dir, &root_generation_id)?;
    validate_stream_snapshot_link(stream_id, None, &current)?;

    let mut seen = BTreeSet::new();
    seen.insert(current.generation_id.clone());
    loop {
        let child_claim_path =
            lineage_claim_path(work_dir, stream_id, Some(&current.generation_id))?;
        if !child_claim_path.is_dir() {
            return Ok(current);
        }
        let child_generation_id = read_lineage_claim_child(&child_claim_path)?;
        if !commit_marker_path(work_dir, &child_generation_id).is_dir() {
            return Ok(current);
        }
        if !seen.insert(child_generation_id.clone()) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::CorruptGeneration,
                format!("publication stream {stream_id} lineage contains a cycle"),
                false,
                Some(true),
                Some(child_generation_id),
            ));
        }
        let child = open_committed_generation(work_dir, &child_generation_id)?;
        validate_stream_snapshot_link(stream_id, Some(&current.generation_id), &child)?;
        current = child;
    }
}

pub fn open_committed_generation(
    work_dir: &Path,
    generation_id: &str,
) -> Result<EntityPublicationSnapshot, EntityPublicationError> {
    validate_hash(generation_id)?;
    let marker_path = commit_marker_path(work_dir, generation_id);
    if !marker_path.is_dir() {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::UncommittedGeneration,
            format!("generation {generation_id} is not committed"),
            false,
            Some(false),
            Some(generation_id.to_string()),
        ));
    }

    let manifest_bytes = read_object(work_dir, generation_id)?;
    let mut manifest: EntityPublicationManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|err| {
            EntityPublicationError::new(
                EntityPublicationErrorKind::Parse,
                format!("failed to parse publication manifest: {err}"),
                false,
                Some(true),
                Some(generation_id.to_string()),
            )
        })?;
    manifest.generation_id = generation_id.to_string();
    manifest.manifest_content_hash = generation_id.to_string();
    validate_manifest_self_hash(&manifest)?;
    if manifest.generation_id != generation_id {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            "publication manifest generation id does not match commit marker",
            false,
            Some(true),
            Some(generation_id.to_string()),
        ));
    }

    let mut seen = BTreeSet::new();
    let mut files = BTreeMap::new();
    for record in &manifest.files {
        validate_manifest_file_record(record)?;
        validate_logical_path(&record.logical_path)?;
        if !seen.insert(record.logical_path.clone()) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::CorruptGeneration,
                format!(
                    "publication manifest contains duplicate logical path {}",
                    record.logical_path
                ),
                false,
                Some(true),
                Some(generation_id.to_string()),
            ));
        }
        let bytes = read_object(work_dir, &record.content_hash)?;
        let actual_hash = hash_bytes(&bytes);
        if actual_hash != record.content_hash {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::HashMismatch,
                format!(
                    "publication object hash mismatch for {}",
                    record.logical_path
                ),
                false,
                Some(true),
                Some(generation_id.to_string()),
            ));
        }
        if bytes.len() as u64 != record.byte_len {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::CorruptGeneration,
                format!(
                    "publication object length mismatch for {}",
                    record.logical_path
                ),
                false,
                Some(true),
                Some(generation_id.to_string()),
            ));
        }
        files.insert(record.logical_path.clone(), bytes);
    }

    Ok(EntityPublicationSnapshot {
        generation_id: generation_id.to_string(),
        manifest,
        files,
    })
}

pub fn publication_object_path(
    work_dir: &Path,
    content_hash: &str,
) -> Result<PathBuf, EntityPublicationError> {
    Ok(object_path(work_dir, content_hash)?.absolute)
}

pub fn publication_commit_marker_path(work_dir: &Path, generation_id: &str) -> PathBuf {
    commit_marker_path(work_dir, generation_id)
}

fn validate_stream_snapshot_link(
    stream_id: &str,
    expected_parent: Option<&str>,
    snapshot: &EntityPublicationSnapshot,
) -> Result<(), EntityPublicationError> {
    if snapshot.manifest.stream_id != stream_id {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            format!(
                "publication stream {stream_id} lineage points to generation {} in stream {}",
                snapshot.generation_id, snapshot.manifest.stream_id
            ),
            false,
            Some(true),
            Some(snapshot.generation_id.clone()),
        ));
    }
    if snapshot.manifest.supersedes_generation_id.as_deref() != expected_parent {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            format!(
                "publication stream {stream_id} generation {} has invalid supersession",
                snapshot.generation_id
            ),
            false,
            Some(true),
            Some(snapshot.generation_id.clone()),
        ));
    }
    Ok(())
}

struct PlannedPublication {
    manifest: EntityPublicationManifest,
    input_files: Vec<EntityPublicationFileInput>,
}

impl PlannedPublication {
    fn new(mut request: EntityPublicationRequest) -> Result<Self, EntityPublicationError> {
        validate_publication_token(&request.stream_id, "stream_id")?;
        if let Some(supersedes_generation_id) = &request.supersedes_generation_id {
            validate_hash(supersedes_generation_id)?;
        }
        validate_hash(&request.request_fingerprint)?;
        validate_hash(&request.cache_receipt_hash)?;
        validate_cache_mode(&request.cache_mode)?;
        validate_publication_token(&request.cache_status, "cache_status")?;
        validate_stage_order(&request.stage_order)?;

        request.files.sort_by(|left, right| {
            left.logical_path
                .cmp(&right.logical_path)
                .then_with(|| left.stage.cmp(&right.stage))
                .then_with(|| left.version.cmp(&right.version))
        });
        request.upstream_artifacts.sort_by(|left, right| {
            left.version
                .cmp(&right.version)
                .then_with(|| left.content_hash.cmp(&right.content_hash))
        });

        let mut seen = BTreeSet::new();
        let mut records = Vec::with_capacity(request.files.len());
        for file in &request.files {
            validate_logical_path(&file.logical_path)?;
            validate_publication_token(&file.stage, "file.stage")?;
            validate_publication_token(&file.version, "file.version")?;
            validate_file_stage_in_order(&file.stage, &request.stage_order)?;
            if !seen.insert(file.logical_path.clone()) {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::InvalidRequest,
                    format!("duplicate publication logical path {}", file.logical_path),
                    false,
                    Some(false),
                    None,
                ));
            }
            let content_hash = hash_bytes(&file.bytes);
            records.push(EntityPublicationFileRecord {
                logical_path: file.logical_path.clone(),
                stage: file.stage.clone(),
                version: file.version.clone(),
                byte_len: file.bytes.len() as u64,
                object_path: object_relative_path(&content_hash)?,
                content_hash,
            });
        }

        for upstream in &request.upstream_artifacts {
            validate_publication_token(&upstream.version, "upstream.version")?;
            validate_hash(&upstream.content_hash)?;
        }

        let mut manifest = EntityPublicationManifest {
            version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
            generation_id: String::new(),
            manifest_content_hash: String::new(),
            stream_id: request.stream_id,
            supersedes_generation_id: request.supersedes_generation_id,
            request_fingerprint: request.request_fingerprint,
            cache_mode: request.cache_mode,
            cache_status: request.cache_status,
            cache_receipt_hash: request.cache_receipt_hash,
            stage_order: request.stage_order,
            upstream_artifacts: request.upstream_artifacts,
            files: records,
        };
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(parse_error)?;
        let manifest_hash = hash_bytes(&manifest_bytes);
        manifest.generation_id = manifest_hash.clone();
        manifest.manifest_content_hash = manifest_hash;

        Ok(Self {
            manifest,
            input_files: request.files,
        })
    }

    fn manifest_bytes(&self) -> Result<Vec<u8>, EntityPublicationError> {
        let mut hashable = self.manifest.clone();
        hashable.generation_id.clear();
        hashable.manifest_content_hash.clear();
        serde_json::to_vec(&hashable).map_err(parse_error)
    }

    fn receipt(
        &self,
        work_dir: &Path,
        outcome: EntityPublicationOutcome,
        writes_performed: bool,
        committed: Option<bool>,
    ) -> EntityPublicationReceipt {
        receipt_for_manifest(
            work_dir,
            &self.manifest,
            outcome,
            writes_performed,
            committed,
        )
    }
}

fn request_with_carried_forward_files(
    mut request: EntityPublicationRequest,
    current: &EntityPublicationSnapshot,
) -> Result<EntityPublicationRequest, EntityPublicationError> {
    let mut omitted = BTreeSet::new();
    for logical_path in &request.omit_logical_paths {
        validate_logical_path(logical_path)?;
        omitted.insert(logical_path.clone());
    }
    for file in &request.files {
        if omitted.contains(&file.logical_path) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                format!(
                    "publication request cannot both omit and replace logical path {}",
                    file.logical_path
                ),
                false,
                Some(false),
                Some(current.generation_id.clone()),
            ));
        }
    }
    let mut files = BTreeMap::new();
    let mut found_omitted = BTreeSet::new();
    for record in &current.manifest.files {
        if omitted.contains(&record.logical_path) {
            found_omitted.insert(record.logical_path.clone());
            continue;
        }
        let bytes = current
            .files
            .get(&record.logical_path)
            .ok_or_else(|| {
                EntityPublicationError::new(
                    EntityPublicationErrorKind::CorruptGeneration,
                    format!(
                        "publication stream head is missing logical file {}",
                        record.logical_path
                    ),
                    false,
                    Some(true),
                    Some(current.generation_id.clone()),
                )
            })?
            .clone();
        files.insert(
            record.logical_path.clone(),
            EntityPublicationFileInput {
                logical_path: record.logical_path.clone(),
                stage: record.stage.clone(),
                version: record.version.clone(),
                bytes,
            },
        );
    }
    for file in request.files.drain(..) {
        files.insert(file.logical_path.clone(), file);
    }
    for logical_path in &omitted {
        if !found_omitted.contains(logical_path) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                format!("publication stream patch cannot omit missing logical path {logical_path}"),
                false,
                Some(false),
                Some(current.generation_id.clone()),
            ));
        }
    }
    request.files = files.into_values().collect();
    Ok(request)
}

fn stream_patch_matches_current_head(
    publication: &PlannedPublication,
    current: &EntityPublicationSnapshot,
) -> bool {
    let manifest = &publication.manifest;
    manifest.stream_id == current.manifest.stream_id
        && manifest.request_fingerprint == current.manifest.request_fingerprint
        && manifest.cache_mode == current.manifest.cache_mode
        && manifest.cache_status == current.manifest.cache_status
        && manifest.cache_receipt_hash == current.manifest.cache_receipt_hash
        && manifest.stage_order == current.manifest.stage_order
        && manifest.upstream_artifacts == current.manifest.upstream_artifacts
        && manifest.files == current.manifest.files
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamLineageClaim {
    writes_performed: bool,
}

impl StreamLineageClaim {
    fn apply_to_error(self, mut err: EntityPublicationError) -> EntityPublicationError {
        if self.writes_performed {
            err.writes_performed = true;
        }
        err
    }

    fn apply_to_receipt(self, receipt: &mut EntityPublicationReceipt) {
        if self.writes_performed {
            receipt.writes_performed = true;
        }
    }
}

fn claim_stream_lineage(
    work_dir: &Path,
    stream_id: &str,
    parent_generation_id: Option<&str>,
    child_generation_id: &str,
    failpoint: EntityPublicationFailpoint,
) -> Result<StreamLineageClaim, EntityPublicationError> {
    validate_publication_token(stream_id, "stream_id")?;
    validate_hash(child_generation_id)?;
    if let Some(parent_generation_id) = parent_generation_id {
        validate_hash(parent_generation_id)?;
    }
    let claim_path = lineage_claim_path(work_dir, stream_id, parent_generation_id)?;

    let claim_parent = claim_path.parent().ok_or_else(|| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidPath,
            "publication lineage claim path has no parent",
            false,
            Some(false),
            None,
        )
    })?;
    if failpoint == EntityPublicationFailpoint::BeforeClaimDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before lineage claim directory creation",
            false,
            Some(false),
            parent_generation_id.map(str::to_string),
        ));
    }
    ensure_directory_synced(claim_parent).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create publication lineage claim directory: {err}"),
            false,
            Some(false),
            parent_generation_id.map(str::to_string),
        )
    })?;
    if failpoint == EntityPublicationFailpoint::AfterClaimDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after lineage claim directory creation",
            true,
            Some(false),
            parent_generation_id.map(str::to_string),
        ));
    }

    if claim_path.is_dir() {
        return validate_existing_lineage_claim(
            work_dir,
            parent_generation_id,
            child_generation_id,
            &claim_path,
            true,
        );
    }

    if failpoint == EntityPublicationFailpoint::BeforeClaimCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before lineage claim staging directory creation",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }
    let staging_dir = create_lineage_claim_staging_dir(work_dir, child_generation_id)?;
    if failpoint == EntityPublicationFailpoint::AfterClaimCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after lineage claim staging directory creation",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }

    let claim_child_path = staging_dir.join("child");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(claim_child_path.as_path())
        .map_err(|err| {
            EntityPublicationError::new(
                EntityPublicationErrorKind::Io,
                format!("failed to create publication lineage claim child: {err}"),
                true,
                Some(false),
                Some(child_generation_id.to_string()),
            )
        })?;
    file.write_all(child_generation_id.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|err| {
            EntityPublicationError::new(
                EntityPublicationErrorKind::Io,
                format!("failed to write publication lineage claim child: {err}"),
                true,
                Some(false),
                Some(child_generation_id.to_string()),
            )
        })?;
    if failpoint == EntityPublicationFailpoint::AfterClaimWrite {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after lineage claim write",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }
    file.flush().map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to flush publication lineage claim child: {err}"),
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        )
    })?;
    if failpoint == EntityPublicationFailpoint::AfterClaimFlush {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after lineage claim flush",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }
    file.sync_all().map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to sync publication lineage claim child: {err}"),
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        )
    })?;
    drop(file);
    if failpoint == EntityPublicationFailpoint::AfterClaimSync {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after lineage claim sync",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }
    sync_directory(&staging_dir).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to sync publication lineage claim staging directory: {err}"),
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        )
    })?;
    if failpoint == EntityPublicationFailpoint::BeforeClaimPublish {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before lineage claim publish",
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        ));
    }

    match fs::rename(staging_dir.as_path(), claim_path.as_path()) {
        Ok(()) => {
            if failpoint == EntityPublicationFailpoint::AfterClaimPublishBeforeParentSync {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    "publication lineage claim was published but directory sync was not confirmed",
                    true,
                    Some(false),
                    Some(child_generation_id.to_string()),
                ));
            }
            sync_directory(claim_parent).map_err(|err| {
                EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    format!("failed to sync publication lineage claim directory: {err}"),
                    true,
                    Some(false),
                    Some(child_generation_id.to_string()),
                )
            })?;
            Ok(StreamLineageClaim {
                writes_performed: true,
            })
        }
        Err(err) if claim_path.is_dir() || err.kind() == io::ErrorKind::AlreadyExists => {
            validate_existing_lineage_claim(
                work_dir,
                parent_generation_id,
                child_generation_id,
                &claim_path,
                true,
            )
        }
        Err(err) => Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create publication lineage claim: {err}"),
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        )),
    }
}

fn validate_existing_lineage_claim(
    work_dir: &Path,
    parent_generation_id: Option<&str>,
    child_generation_id: &str,
    claim_path: &Path,
    writes_performed: bool,
) -> Result<StreamLineageClaim, EntityPublicationError> {
    let claimed_child = read_lineage_claim_child(claim_path)?;
    if claimed_child == child_generation_id {
        return Ok(StreamLineageClaim { writes_performed });
    }
    let committed = commit_marker_path(work_dir, &claimed_child).is_dir();
    if committed {
        open_committed_generation(work_dir, &claimed_child)?;
    }
    Err(EntityPublicationError::new(
        EntityPublicationErrorKind::ForkedGeneration,
        format!("publication stream lineage is already claimed by generation {claimed_child}"),
        writes_performed,
        Some(committed),
        parent_generation_id
            .map(str::to_string)
            .or(Some(claimed_child)),
    ))
}

fn create_lineage_claim_staging_dir(
    work_dir: &Path,
    child_generation_id: &str,
) -> Result<PathBuf, EntityPublicationError> {
    let staging_root = publication_root(work_dir).join("claims").join("staging");
    ensure_directory_synced(&staging_root).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create publication lineage claim staging root: {err}"),
            true,
            Some(false),
            Some(child_generation_id.to_string()),
        )
    })?;
    let hex = hash_hex(child_generation_id)?;
    loop {
        let counter = ATTEMPT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let staging_dir =
            staging_root.join(format!("{}.{}.{}.claim", std::process::id(), counter, hex));
        match fs::create_dir(staging_dir.as_path()) {
            Ok(()) => {
                sync_directory(&staging_root).map_err(|err| {
                    EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        format!("failed to sync publication lineage claim staging root: {err}"),
                        true,
                        Some(false),
                        Some(child_generation_id.to_string()),
                    )
                })?;
                return Ok(staging_dir);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    format!("failed to create publication lineage claim staging directory: {err}"),
                    true,
                    Some(false),
                    Some(child_generation_id.to_string()),
                ));
            }
        }
    }
}

fn read_lineage_claim_child(path: &Path) -> Result<String, EntityPublicationError> {
    let child_path = path.join("child");
    let value = fs::read_to_string(&child_path).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to read publication lineage claim child: {err}"),
            false,
            Some(false),
            None,
        )
    })?;
    let value = value.trim();
    validate_hash(value).map_err(|_| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            format!("publication lineage claim child is corrupt: {value:?}"),
            false,
            Some(false),
            None,
        )
    })?;
    Ok(value.to_string())
}

fn validate_stream_receipt_lineage(
    work_dir: &Path,
    receipt: EntityPublicationReceipt,
) -> Result<EntityPublicationReceipt, EntityPublicationError> {
    if receipt.committed != Some(true) {
        return Ok(receipt);
    }
    let snapshot = open_committed_generation(work_dir, &receipt.generation_id)?;
    let current = open_current_stream_generation(work_dir, &snapshot.manifest.stream_id)?;
    if current.generation_id != receipt.generation_id {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::ForkedGeneration,
            format!(
                "publication stream {} current head is {} after committing {}",
                snapshot.manifest.stream_id, current.generation_id, receipt.generation_id
            ),
            receipt.writes_performed,
            Some(true),
            Some(receipt.generation_id),
        ));
    }
    Ok(receipt)
}

fn receipt_for_manifest(
    work_dir: &Path,
    manifest: &EntityPublicationManifest,
    outcome: EntityPublicationOutcome,
    writes_performed: bool,
    committed: Option<bool>,
) -> EntityPublicationReceipt {
    EntityPublicationReceipt {
        version: CANON_ENTITY_STAGE_PUBLICATION_VERSION.to_string(),
        generation_id: manifest.generation_id.clone(),
        outcome,
        writes_performed,
        committed,
        manifest_path: object_relative_path(&manifest.generation_id)
            .expect("validated publication hash"),
        commit_marker_path: path_relative_to_work_dir(
            work_dir,
            &commit_marker_path(work_dir, &manifest.generation_id),
        ),
        object_count: manifest.files.len() + 1,
    }
}

struct ObjectPath {
    absolute: PathBuf,
}

struct ObjectWrite {
    writes_performed: bool,
}

fn write_content_object(
    work_dir: &Path,
    generation_id: &str,
    content_hash: &str,
    bytes: &[u8],
    failpoint: EntityPublicationFailpoint,
) -> Result<ObjectWrite, EntityPublicationError> {
    validate_hash(content_hash)?;
    let target = object_path(work_dir, content_hash)?.absolute;
    if target.exists() {
        validate_existing_object(&target, content_hash)?;
        return Ok(ObjectWrite {
            writes_performed: false,
        });
    }

    let object_parent = target.parent().ok_or_else(|| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidPath,
            "publication object path has no parent",
            false,
            Some(false),
            None,
        )
    })?;
    if failpoint == EntityPublicationFailpoint::BeforeObjectDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before object directory creation",
            false,
            Some(false),
            Some(generation_id.to_string()),
        ));
    }
    ensure_directory_synced(object_parent).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create publication object directory: {err}"),
            false,
            Some(false),
            Some(generation_id.to_string()),
        )
    })?;
    if failpoint == EntityPublicationFailpoint::AfterObjectDirectoryCreate {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed after object directory creation",
            true,
            Some(false),
            Some(generation_id.to_string()),
        ));
    }
    let attempts_dir = publication_root(work_dir).join("attempts");
    ensure_directory_synced(&attempts_dir).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to create publication attempts directory: {err}"),
            true,
            Some(false),
            Some(generation_id.to_string()),
        )
    })?;
    let attempt =
        create_attempt_file(&attempts_dir, generation_id, content_hash, bytes, failpoint)?;

    if target.exists() {
        validate_existing_object(&target, content_hash)?;
        return Ok(ObjectWrite {
            writes_performed: true,
        });
    }
    if failpoint == EntityPublicationFailpoint::BeforeObjectRename {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            "publication failed before object rename",
            true,
            Some(false),
            Some(generation_id.to_string()),
        ));
    }
    match fs::rename(attempt.as_path(), target.as_path()) {
        Ok(()) => {
            if failpoint == EntityPublicationFailpoint::AfterObjectRenameBeforeDirectorySync {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    "publication object was renamed but directory sync was not confirmed",
                    true,
                    Some(false),
                    Some(generation_id.to_string()),
                ));
            }
            sync_directory(object_parent).map_err(|err| {
                EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    format!("failed to sync publication object directory: {err}"),
                    true,
                    Some(false),
                    Some(generation_id.to_string()),
                )
            })?;
            validate_existing_object(&target, content_hash)?;
            Ok(ObjectWrite {
                writes_performed: true,
            })
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            validate_existing_object(&target, content_hash)?;
            Ok(ObjectWrite {
                writes_performed: true,
            })
        }
        Err(err) => Err(EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to publish content object: {err}"),
            true,
            Some(false),
            Some(generation_id.to_string()),
        )),
    }
}

fn create_attempt_file(
    attempts_dir: &Path,
    generation_id: &str,
    content_hash: &str,
    bytes: &[u8],
    failpoint: EntityPublicationFailpoint,
) -> Result<PathBuf, EntityPublicationError> {
    let hex = hash_hex(content_hash)?;
    loop {
        let counter = ATTEMPT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let attempt = attempts_dir.join(format!(
            "{}.{}.{}.attempt",
            std::process::id(),
            counter,
            hex
        ));
        if failpoint == EntityPublicationFailpoint::BeforeObjectAttemptCreate {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::Io,
                "publication failed before object attempt creation",
                true,
                Some(false),
                Some(generation_id.to_string()),
            ));
        }
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(attempt.as_path())
        {
            Ok(mut file) => {
                if failpoint == EntityPublicationFailpoint::AfterObjectAttemptCreate {
                    return Err(EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        "publication failed after object attempt creation",
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    ));
                }
                file.write_all(bytes).map_err(|err| {
                    EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        format!("failed to write publication attempt object: {err}"),
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    )
                })?;
                if failpoint == EntityPublicationFailpoint::AfterObjectWrite {
                    return Err(EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        "publication failed after object write",
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    ));
                }
                file.flush().map_err(|err| {
                    EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        format!("failed to flush publication attempt object: {err}"),
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    )
                })?;
                if failpoint == EntityPublicationFailpoint::AfterObjectFlush {
                    return Err(EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        "publication failed after object flush",
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    ));
                }
                file.sync_all().map_err(|err| {
                    EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        format!("failed to sync publication attempt object: {err}"),
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    )
                })?;
                if failpoint == EntityPublicationFailpoint::AfterObjectSync {
                    return Err(EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        "publication failed after object sync",
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    ));
                }
                sync_directory(attempts_dir).map_err(|err| {
                    EntityPublicationError::new(
                        EntityPublicationErrorKind::Io,
                        format!("failed to sync publication attempts directory: {err}"),
                        true,
                        Some(false),
                        Some(generation_id.to_string()),
                    )
                })?;
                return Ok(attempt);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(EntityPublicationError::new(
                    EntityPublicationErrorKind::Io,
                    format!("failed to create publication attempt object: {err}"),
                    true,
                    Some(false),
                    Some(generation_id.to_string()),
                ));
            }
        }
    }
}

fn validate_existing_object(
    path: &Path,
    expected_hash: &str,
) -> Result<(), EntityPublicationError> {
    let bytes = fs::read(path).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to read publication object: {err}"),
            false,
            Some(true),
            None,
        )
    })?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash != expected_hash {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::HashMismatch,
            "publication object hash mismatch",
            false,
            Some(true),
            None,
        ));
    }
    Ok(())
}

fn validate_manifest_bundle(
    work_dir: &Path,
    manifest: &EntityPublicationManifest,
) -> Result<(), EntityPublicationError> {
    validate_manifest_self_hash(manifest)?;
    let stored_manifest = read_object(work_dir, &manifest.generation_id)?;
    let stored_hash = hash_bytes(&stored_manifest);
    if stored_hash != manifest.generation_id {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::HashMismatch,
            "stored publication manifest hash mismatch",
            false,
            Some(false),
            Some(manifest.generation_id.clone()),
        ));
    }
    for record in &manifest.files {
        validate_manifest_file_record(record)?;
        let bytes = read_object(work_dir, &record.content_hash)?;
        if hash_bytes(&bytes) != record.content_hash {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::HashMismatch,
                format!(
                    "stored publication object hash mismatch for {}",
                    record.logical_path
                ),
                false,
                Some(false),
                Some(manifest.generation_id.clone()),
            ));
        }
        if bytes.len() as u64 != record.byte_len {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::CorruptGeneration,
                format!(
                    "stored publication object length mismatch for {}",
                    record.logical_path
                ),
                false,
                Some(false),
                Some(manifest.generation_id.clone()),
            ));
        }
    }
    Ok(())
}

fn validate_manifest_self_hash(
    manifest: &EntityPublicationManifest,
) -> Result<(), EntityPublicationError> {
    if manifest.version != CANON_ENTITY_STAGE_PUBLICATION_VERSION {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            format!(
                "unsupported publication manifest version {}",
                manifest.version
            ),
            false,
            Some(true),
            Some(manifest.generation_id.clone()),
        ));
    }
    validate_publication_token(&manifest.stream_id, "stream_id")?;
    if let Some(supersedes_generation_id) = &manifest.supersedes_generation_id {
        validate_hash(supersedes_generation_id)?;
    }
    validate_hash(&manifest.request_fingerprint)?;
    validate_hash(&manifest.cache_receipt_hash)?;
    validate_cache_mode(&manifest.cache_mode)?;
    validate_publication_token(&manifest.cache_status, "cache_status")?;
    validate_stage_order(&manifest.stage_order)?;
    for upstream in &manifest.upstream_artifacts {
        validate_publication_token(&upstream.version, "upstream.version")?;
        validate_hash(&upstream.content_hash)?;
    }
    for record in &manifest.files {
        validate_manifest_file_record(record)?;
        validate_file_stage_in_order(&record.stage, &manifest.stage_order)?;
    }
    validate_hash(&manifest.generation_id)?;
    validate_hash(&manifest.manifest_content_hash)?;
    if manifest.generation_id != manifest.manifest_content_hash {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            "publication manifest self hash fields disagree",
            false,
            Some(true),
            Some(manifest.generation_id.clone()),
        ));
    }
    let mut hashable = manifest.clone();
    hashable.generation_id.clear();
    hashable.manifest_content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(parse_error)?;
    let actual_hash = hash_bytes(&bytes);
    if actual_hash != manifest.generation_id {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::HashMismatch,
            "publication manifest self hash mismatch",
            false,
            Some(true),
            Some(manifest.generation_id.clone()),
        ));
    }
    Ok(())
}

fn validate_manifest_file_record(
    record: &EntityPublicationFileRecord,
) -> Result<(), EntityPublicationError> {
    validate_logical_path(&record.logical_path)?;
    validate_publication_token(&record.stage, "file.stage")?;
    validate_publication_token(&record.version, "file.version")?;
    validate_hash(&record.content_hash)?;
    let expected = object_relative_path(&record.content_hash)?;
    if record.object_path != expected {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::CorruptGeneration,
            format!(
                "publication manifest object_path does not match content_hash for {}",
                record.logical_path
            ),
            false,
            Some(true),
            None,
        ));
    }
    Ok(())
}

fn read_object(work_dir: &Path, content_hash: &str) -> Result<Vec<u8>, EntityPublicationError> {
    validate_hash(content_hash)?;
    let path = object_path(work_dir, content_hash)?.absolute;
    let mut file = File::open(path.as_path()).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to open publication object: {err}"),
            false,
            Some(true),
            Some(content_hash.to_string()),
        )
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|err| {
        EntityPublicationError::new(
            EntityPublicationErrorKind::Io,
            format!("failed to read publication object: {err}"),
            false,
            Some(true),
            Some(content_hash.to_string()),
        )
    })?;
    if hash_bytes(&bytes) != content_hash {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::HashMismatch,
            "publication object hash mismatch",
            false,
            Some(true),
            Some(content_hash.to_string()),
        ));
    }
    Ok(bytes)
}

fn object_path(work_dir: &Path, content_hash: &str) -> Result<ObjectPath, EntityPublicationError> {
    let hex = hash_hex(content_hash)?;
    Ok(ObjectPath {
        absolute: publication_root(work_dir)
            .join("objects")
            .join("blake3")
            .join(hex),
    })
}

fn object_relative_path(content_hash: &str) -> Result<String, EntityPublicationError> {
    let hex = hash_hex(content_hash)?;
    Ok(format!("{ENTITY_PUBLICATION_ROOT}/objects/blake3/{hex}"))
}

fn publication_root(work_dir: &Path) -> PathBuf {
    work_dir.join(ENTITY_PUBLICATION_ROOT)
}

fn commit_marker_parent(work_dir: &Path) -> PathBuf {
    publication_root(work_dir).join("commits")
}

fn commit_marker_path(work_dir: &Path, generation_id: &str) -> PathBuf {
    let hex = hash_hex(generation_id).expect("validated publication generation id");
    commit_marker_parent(work_dir).join(hex)
}

fn lineage_claim_path(
    work_dir: &Path,
    stream_id: &str,
    parent_generation_id: Option<&str>,
) -> Result<PathBuf, EntityPublicationError> {
    validate_publication_token(stream_id, "stream_id")?;
    let claims = publication_root(work_dir).join("claims");
    match parent_generation_id {
        Some(parent_generation_id) => Ok(claims
            .join("children")
            .join(hash_hex(parent_generation_id)?)),
        None => Ok(claims.join("roots").join(stream_id)),
    }
}

fn path_relative_to_work_dir(work_dir: &Path, path: &Path) -> String {
    path.strip_prefix(work_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_logical_path(value: &str) -> Result<(), EntityPublicationError> {
    if value.is_empty() || value.contains('\\') {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidPath,
            format!("invalid publication logical path {value:?}"),
            false,
            Some(false),
            None,
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidPath,
            format!("publication logical path must be relative: {value}"),
            false,
            Some(false),
            None,
        ));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidPath,
                format!("publication logical path must not traverse directories: {value}"),
                false,
                Some(false),
                None,
            ));
        }
    }
    Ok(())
}

fn validate_cache_mode(value: &str) -> Result<(), EntityPublicationError> {
    match value {
        "enabled" | "disabled" => Ok(()),
        _ => Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidRequest,
            format!("publication cache_mode must be enabled or disabled: {value}"),
            false,
            Some(false),
            None,
        )),
    }
}

fn validate_stage_order(values: &[String]) -> Result<(), EntityPublicationError> {
    if values.is_empty() {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidRequest,
            "publication stage_order must not be empty",
            false,
            Some(false),
            None,
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_publication_token(value, "stage_order")?;
        if !seen.insert(value.as_str()) {
            return Err(EntityPublicationError::new(
                EntityPublicationErrorKind::InvalidRequest,
                format!("publication stage_order contains duplicate stage {value}"),
                false,
                Some(false),
                None,
            ));
        }
    }
    Ok(())
}

fn validate_file_stage_in_order(
    stage: &str,
    stage_order: &[String],
) -> Result<(), EntityPublicationError> {
    if stage_order.iter().any(|candidate| candidate == stage) {
        return Ok(());
    }
    Err(EntityPublicationError::new(
        EntityPublicationErrorKind::InvalidRequest,
        format!("publication file stage {stage} is not declared in stage_order"),
        false,
        Some(false),
        None,
    ))
}

fn validate_publication_token(value: &str, field: &str) -> Result<(), EntityPublicationError> {
    if value.is_empty()
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidRequest,
            format!("publication {field} is not a safe token: {value:?}"),
            false,
            Some(false),
            None,
        ));
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), EntityPublicationError> {
    let _ = hash_hex(value)?;
    Ok(())
}

fn hash_hex(value: &str) -> Result<&str, EntityPublicationError> {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidRequest,
            format!("publication hash must use blake3 prefix: {value}"),
            false,
            Some(false),
            None,
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EntityPublicationError::new(
            EntityPublicationErrorKind::InvalidRequest,
            format!("publication hash is not a 64-byte hex digest: {value}"),
            false,
            Some(false),
            None,
        ));
    }
    Ok(hex)
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn ensure_directory_synced(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    sync_directory(path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn parse_error(err: serde_json::Error) -> EntityPublicationError {
    EntityPublicationError::new(
        EntityPublicationErrorKind::Parse,
        format!("failed to serialize publication manifest: {err}"),
        false,
        Some(false),
        None,
    )
}
