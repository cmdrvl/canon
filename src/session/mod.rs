use crate::{
    Refusal, RefusalCode,
    entity::evidence_ir::{
        CANON_EVIDENCE_VERSION, EvidenceAuthorityBasis, EvidenceExtension, EvidenceExtensionValue,
        EvidenceKind, EvidenceOperatorRef, EvidencePolicyRef, EvidenceProvenanceRef,
        EvidenceRecord, EvidenceTarget, canonicalize_record,
    },
    entity::publication::{
        EntityPublicationError, EntityPublicationErrorKind, EntityPublicationFileInput,
        EntityPublicationRequest, EntityPublicationUpstreamRef, open_current_stream_generation,
        publish_stream_patch,
    },
    registry, registry_lint,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
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
pub const SESSION_ASSERT_VERSION: &str = "canon.entity.session.assert.v1";
pub const SESSION_EVIDENCE_STREAM_ID: &str = "entity-session-evidence";
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

#[derive(Debug)]
struct CanonicalObservation {
    obs_id: String,
    bytes: Vec<u8>,
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
        SESSION_ASSERT_VERSION, SESSION_INGEST_VERSION, canonical_shard_bytes,
        canonicalize_assertion_line, canonicalize_observation_line, hash_bytes,
    };
    use crate::entity::evidence_ir::EvidenceKind;
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
}
