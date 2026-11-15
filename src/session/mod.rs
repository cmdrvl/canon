use crate::{
    Refusal, RefusalCode,
    entity::publication::{
        EntityPublicationError, EntityPublicationErrorKind, EntityPublicationFileInput,
        EntityPublicationRequest, EntityPublicationUpstreamRef, open_current_stream_generation,
        publish_stream_patch,
    },
    registry, registry_lint,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const SESSION_MANIFEST_VERSION: &str = "canon.entity.session.v1";
pub const SESSION_MANIFEST_STREAM_ID: &str = "entity-session-manifest";
const SESSION_PUBLICATION_STAGE: &str = "session";
const SESSION_PUBLICATION_VERSION: &str = "canon.entity.session.v1";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRegistryBinding {
    pub kind: String,
    pub path: String,
    pub id: String,
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

fn output(
    status: &str,
    manifest: SessionManifest,
    work_dir: &Path,
    generation_id: String,
    registry_initialized: bool,
    registry_path_updated: bool,
) -> SessionInitOutput {
    let next_command = format!(
        "canon entity session ingest {} <OBSERVATIONS.jsonl> --emit json",
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
