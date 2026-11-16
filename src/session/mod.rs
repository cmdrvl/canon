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
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub const SESSION_MANIFEST_VERSION: &str = "canon.entity.session.v1";
pub const SESSION_MANIFEST_STREAM_ID: &str = "entity-session-manifest";
pub const SESSION_INGEST_VERSION: &str = "canon.entity.session.ingest.v1";
pub const SESSION_OBSERVATIONS_STREAM_ID: &str = "entity-session-observations";
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionIngestOutput {
    pub version: String,
    pub accepted: usize,
    pub already_present: usize,
    pub rejected: Vec<SessionIngestRejection>,
    pub shard: Option<String>,
    pub generation_id: Option<String>,
}

impl SessionIngestOutput {
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

pub fn ingest(request: SessionIngestRequest) -> Result<SessionIngestOutput, Refusal> {
    let work_dir = absolute_path(&request.session)?;
    if read_current_manifest(&work_dir)?.is_none() {
        return Err(session_not_initialized_refusal(&work_dir));
    }

    let current = match open_current_stream_generation(&work_dir, SESSION_OBSERVATIONS_STREAM_ID) {
        Ok(snapshot) => Some(snapshot),
        Err(error) if error.kind == EntityPublicationErrorKind::UncommittedGeneration => None,
        Err(error) => return Err(observation_artifact_refusal(&error.message)),
    };
    let (mut known_rows, existing_shard) = match current.as_ref() {
        Some(snapshot) => existing_observations(snapshot)?,
        None => (BTreeMap::new(), None),
    };

    let input = read_observation_input(&request.observations)?;
    let mut accepted_rows = BTreeMap::new();
    let mut already_present = 0;
    let mut rejected = Vec::new();

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let row = match canonicalize_observation_line(line, line_number) {
            Ok(row) => row,
            Err(error) => {
                rejected.push(error.into_rejection());
                continue;
            }
        };

        if let Some(existing) = known_rows.get(&row.obs_id) {
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

        known_rows.insert(row.obs_id.clone(), row.bytes.clone());
        accepted_rows.insert(row.obs_id, row.bytes);
    }

    if accepted_rows.is_empty() {
        return Ok(SessionIngestOutput {
            version: SESSION_INGEST_VERSION.to_string(),
            accepted: 0,
            already_present,
            rejected,
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
        shard: Some(logical_path),
        generation_id: Some(receipt.generation_id),
    })
}

#[derive(Debug)]
struct CanonicalObservation {
    obs_id: String,
    bytes: Vec<u8>,
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
    *surface_value = serde_json::Value::String(surface.clone());
    if surface.is_empty() {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id),
            "empty_surface",
            "surface must be non-empty after ASCII-trim",
            "surface",
        ));
    }
    if surface.len() > crate::evidence::MAX_TEXT_BYTES {
        return Err(ObservationRowError::new(
            line_number,
            Some(obs_id),
            "oversized_surface",
            format!(
                "surface exceeds the maximum text size of {} bytes",
                crate::evidence::MAX_TEXT_BYTES
            ),
            "surface",
        ));
    }

    for field in ["keys", "attrs", "context"] {
        validate_scalar_object(&object, field, line_number, Some(obs_id.clone()))?;
    }

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
    Ok(CanonicalObservation { obs_id, bytes })
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
) -> Result<(BTreeMap<String, Vec<u8>>, Option<String>), Refusal> {
    let mut rows = BTreeMap::new();
    let mut shards = Vec::new();
    for logical_path in snapshot
        .logical_paths()
        .filter(|path| path.starts_with("observations/") && path.ends_with(".jsonl"))
    {
        shards.push(logical_path.to_string());
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
    Ok((rows, shards.into_iter().next()))
}

fn canonical_shard_bytes(rows: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in rows.values() {
        bytes.extend_from_slice(row);
        bytes.push(b'\n');
    }
    bytes
}

fn read_observation_input(path: &Path) -> Result<String, Refusal> {
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
        message: format!("Observation JSONL '{}' is not valid UTF-8", path.display()),
        detail: json!({
            "path": path.display().to_string(),
            "error": error.to_string(),
            "writes_performed": false,
        }),
        next_command: Some(
            "Convert the observation JSONL to UTF-8, then rerun the same command".to_string(),
        ),
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
    use super::{SESSION_INGEST_VERSION, canonical_shard_bytes, canonicalize_observation_line};
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
}
