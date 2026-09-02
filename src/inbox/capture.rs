use super::{
    CANON_UNRESOLVED_INBOX_VERSION, CandidateStatus, CandidateSummary, InboxError, InboxErrorCode,
    InboxEventKind, InboxExportMode, InboxFieldRole, InboxOccurrenceRef, InboxPrivacyPolicy,
    InboxReasonCode, InboxResult, NamespaceHint, NormalizedSurfaceFingerprint, PrivacyClass,
    ProfileFieldRef, UnresolvedInboxArtifact, UnresolvedInboxItem, canonical_json_bytes,
    finalize_artifact, merge_artifacts,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const DEFAULT_CAPTURE_NORMALIZER_ID: &str = "canon.inbox.capture.surface.v1";
pub const SOURCE_ARTIFACT_HASH_NAMESPACE: &str = "source_artifact_hash";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRequest {
    pub policy: InboxPrivacyPolicy,
    pub view: InboxExportMode,
    pub context: CaptureContext,
}

impl CaptureRequest {
    pub fn new(policy: InboxPrivacyPolicy, view: InboxExportMode, context: CaptureContext) -> Self {
        Self {
            policy,
            view,
            context,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureContext {
    pub project_ref: String,
    pub run_ref: String,
    pub source_ref: String,
    pub source_artifact_hash: String,
    pub seen_at: String,
    pub field_name: String,
    pub field_role: InboxFieldRole,
    pub profile_ref: Option<ProfileFieldRef>,
    pub namespace_hints: Vec<NamespaceHint>,
    pub privacy_class: Option<PrivacyClass>,
    pub normalizer_id: String,
}

impl CaptureContext {
    pub fn new(
        project_ref: impl Into<String>,
        run_ref: impl Into<String>,
        source_ref: impl Into<String>,
        source_artifact_hash: impl Into<String>,
        seen_at: impl Into<String>,
        field_name: impl Into<String>,
        field_role: InboxFieldRole,
    ) -> Self {
        Self {
            project_ref: project_ref.into(),
            run_ref: run_ref.into(),
            source_ref: source_ref.into(),
            source_artifact_hash: source_artifact_hash.into(),
            seen_at: seen_at.into(),
            field_name: field_name.into(),
            field_role,
            profile_ref: None,
            namespace_hints: Vec::new(),
            privacy_class: None,
            normalizer_id: DEFAULT_CAPTURE_NORMALIZER_ID.to_string(),
        }
    }
}

struct CaptureItemSeed {
    event_kind: InboxEventKind,
    reason_code: InboxReasonCode,
    field_name: String,
    field_role: InboxFieldRole,
    surface_role: String,
    surface_seed: Value,
    record_ref: Option<String>,
    candidate_summary: CandidateSummary,
    namespace_hints: Vec<NamespaceHint>,
    profile_ref: Option<ProfileFieldRef>,
    privacy_class: Option<PrivacyClass>,
}

#[derive(Debug, Clone, Copy)]
struct CaptureBucketPolicy {
    event_kind: InboxEventKind,
    default_reason_code: InboxReasonCode,
    default_status: CandidateStatus,
    default_candidate_count: u32,
}

pub fn capture_artifact(
    artifact: &Value,
    request: &CaptureRequest,
) -> InboxResult<UnresolvedInboxArtifact> {
    if artifact.get("outcome").is_some() {
        return capture_exact_mapping_artifact(artifact, request);
    }

    if string_field(artifact, &["schema_version"]).as_deref() == Some("canon.project.run.v2") {
        return capture_project_run_receipt(artifact, request);
    }

    match string_field(artifact, &["version"]).as_deref() {
        Some("canon_registry_build.v0") => {
            capture_provider_materialization_artifact(artifact, request)
        }
        Some(version) if is_entity_v1_version(version) => {
            capture_entity_v1_artifact(artifact, request)
        }
        Some(version) => Err(artifact_contract_error(format!(
            "unsupported upstream artifact version for inbox capture: {version}"
        ))),
        None => Err(artifact_contract_error(
            "upstream artifact requires version, schema_version, or outcome for inbox capture",
        )),
    }
}

pub fn capture_exact_mapping_artifact(
    artifact: &Value,
    request: &CaptureRequest,
) -> InboxResult<UnresolvedInboxArtifact> {
    require_version(artifact, "canon.v0")?;
    let outcome = required_string(artifact, "outcome")?;
    if matches!(outcome, "RESOLVED" | "REFUSAL") {
        return finalized_capture_artifact(request, Vec::new());
    }
    if !matches!(outcome, "PARTIAL" | "UNRESOLVED") {
        return Err(artifact_contract_error(format!(
            "unsupported exact mapping outcome for inbox capture: {outcome}"
        )));
    }

    let mut items = Vec::new();
    for (index, entry) in optional_array(artifact, "unresolved")?.iter().enumerate() {
        let reason = required_string(entry, "reason")?;
        let reason_code = exact_reason_code(reason)?;
        let input = string_field(entry, &["input"]);
        let field_name = request.context.field_name.clone();
        let surface_seed = json!({
            "producer": "canon.exact_lookup",
            "field_name": field_name,
            "reason": reason,
            "input": input,
        });

        items.push(capture_item(
            request,
            CaptureItemSeed {
                event_kind: InboxEventKind::ExactLookup,
                reason_code,
                field_name,
                field_role: InboxFieldRole::LookupInput,
                surface_role: "lookup_input".to_string(),
                surface_seed,
                record_ref: record_ref(entry, "exact_unresolved", index),
                candidate_summary: CandidateSummary {
                    status: CandidateStatus::None,
                    candidate_count: 0,
                    ..CandidateSummary::default()
                },
                namespace_hints: namespace_hints_from_entry(entry),
                profile_ref: request.context.profile_ref.clone(),
                privacy_class: request.context.privacy_class,
            },
        )?);
    }

    finalized_capture_artifact(request, items)
}

pub fn capture_entity_v1_artifact(
    artifact: &Value,
    request: &CaptureRequest,
) -> InboxResult<UnresolvedInboxArtifact> {
    let version = required_string(artifact, "version")?;
    if !is_entity_v1_version(version) {
        return Err(artifact_contract_error(format!(
            "unsupported entity artifact version for inbox capture: {version}"
        )));
    }

    let default_kind = if version.contains("_link") {
        InboxEventKind::LinkAbstention
    } else {
        InboxEventKind::ClusterAbstention
    };
    let mut items = Vec::new();

    capture_bucket(
        artifact,
        "unresolved",
        request,
        CaptureBucketPolicy {
            event_kind: default_kind,
            default_reason_code: InboxReasonCode::NoMatchingRule,
            default_status: CandidateStatus::None,
            default_candidate_count: 0,
        },
        &mut items,
    )?;
    capture_bucket(
        artifact,
        "ambiguous",
        request,
        CaptureBucketPolicy {
            event_kind: default_kind,
            default_reason_code: InboxReasonCode::AmbiguousCandidates,
            default_status: CandidateStatus::Ambiguous,
            default_candidate_count: 2,
        },
        &mut items,
    )?;
    capture_bucket(
        artifact,
        "review_deferred",
        request,
        CaptureBucketPolicy {
            event_kind: default_kind,
            default_reason_code: InboxReasonCode::ScoreBelowThreshold,
            default_status: CandidateStatus::None,
            default_candidate_count: 0,
        },
        &mut items,
    )?;
    capture_bucket(
        artifact,
        "escrow",
        request,
        CaptureBucketPolicy {
            event_kind: default_kind,
            default_reason_code: InboxReasonCode::ScoreBelowThreshold,
            default_status: CandidateStatus::None,
            default_candidate_count: 0,
        },
        &mut items,
    )?;
    capture_bucket(
        artifact,
        "contradictions",
        request,
        CaptureBucketPolicy {
            event_kind: InboxEventKind::CandidateRejected,
            default_reason_code: InboxReasonCode::CannotLink,
            default_status: CandidateStatus::Rejected,
            default_candidate_count: 0,
        },
        &mut items,
    )?;
    capture_review_items(artifact, request, default_kind, &mut items)?;

    finalized_capture_artifact(request, items)
}

pub fn capture_provider_materialization_artifact(
    artifact: &Value,
    request: &CaptureRequest,
) -> InboxResult<UnresolvedInboxArtifact> {
    require_version(artifact, "canon_registry_build.v0")?;
    let mut items = Vec::new();

    capture_bucket(
        artifact,
        "unresolved",
        request,
        CaptureBucketPolicy {
            event_kind: InboxEventKind::CandidateRejected,
            default_reason_code: InboxReasonCode::NoMatchingRule,
            default_status: CandidateStatus::None,
            default_candidate_count: 0,
        },
        &mut items,
    )?;

    for (index, entry) in optional_array(artifact, "failures")?.iter().enumerate() {
        let reason_text = reason_text(entry);
        let reason_code = outcome_reason_code(&reason_text, InboxReasonCode::CannotLink);
        let status = match reason_code {
            InboxReasonCode::AmbiguousCandidates => CandidateStatus::Ambiguous,
            InboxReasonCode::BudgetExceeded => CandidateStatus::BudgetLimited,
            InboxReasonCode::CannotLink => CandidateStatus::Rejected,
            _ => CandidateStatus::None,
        };
        items.push(capture_item(
            request,
            CaptureItemSeed {
                event_kind: InboxEventKind::CandidateRejected,
                reason_code,
                field_name: field_name(entry, request),
                field_role: request.context.field_role,
                surface_role: surface_role(request.context.field_role),
                surface_seed: surface_seed(entry, "provider_failure", index),
                record_ref: record_ref(entry, "provider_failure", index),
                candidate_summary: CandidateSummary {
                    status,
                    candidate_count: candidate_count(entry, status),
                    rejection_reasons: rejection_reasons(entry, &reason_text),
                    ..CandidateSummary::default()
                },
                namespace_hints: namespace_hints_from_entry(entry),
                profile_ref: request.context.profile_ref.clone(),
                privacy_class: request.context.privacy_class,
            },
        )?);
    }

    finalized_capture_artifact(request, items)
}

pub fn capture_project_run_receipt(
    artifact: &Value,
    request: &CaptureRequest,
) -> InboxResult<UnresolvedInboxArtifact> {
    if string_field(artifact, &["schema_version"]).as_deref() != Some("canon.project.run.v2") {
        return Err(artifact_contract_error(
            "project run receipt requires schema_version canon.project.run.v2",
        ));
    }

    let mut items = Vec::new();
    for (index, receipt) in optional_array(artifact, "node_receipts")?
        .iter()
        .enumerate()
    {
        if string_field(receipt, &["outcome"]).as_deref() != Some("failed") {
            continue;
        }
        let failure_code = string_field(receipt, &["failure_code", "code"]).unwrap_or_default();
        let Some((reason_code, status)) = project_failure_reason(&failure_code) else {
            continue;
        };
        items.push(capture_item(
            request,
            CaptureItemSeed {
                event_kind: match status {
                    CandidateStatus::Rejected => InboxEventKind::CandidateRejected,
                    _ => InboxEventKind::ClusterAbstention,
                },
                reason_code,
                field_name: field_name(receipt, request),
                field_role: request.context.field_role,
                surface_role: surface_role(request.context.field_role),
                surface_seed: surface_seed(receipt, "project_node_failure", index),
                record_ref: record_ref(receipt, "project_node_failure", index),
                candidate_summary: CandidateSummary {
                    status,
                    candidate_count: candidate_count(receipt, status),
                    rejection_reasons: rejection_reasons(receipt, &failure_code),
                    ..CandidateSummary::default()
                },
                namespace_hints: namespace_hints_from_entry(receipt),
                profile_ref: request.context.profile_ref.clone(),
                privacy_class: request.context.privacy_class,
            },
        )?);
    }

    finalized_capture_artifact(request, items)
}

pub fn merge_capture_shards(
    shards: impl IntoIterator<Item = UnresolvedInboxArtifact>,
) -> InboxResult<UnresolvedInboxArtifact> {
    merge_artifacts(shards)
}

pub fn canonical_capture_bytes(artifact: &UnresolvedInboxArtifact) -> InboxResult<Vec<u8>> {
    canonical_json_bytes(artifact)
}

pub fn write_capture_artifact(path: &Path, artifact: &UnresolvedInboxArtifact) -> InboxResult<()> {
    if path.exists() {
        return Err(artifact_contract_error(format!(
            "inbox capture output already exists: {}",
            path.display()
        )));
    }
    let bytes = canonical_capture_bytes(artifact)?;
    let tmp_path = temp_path(path, &bytes)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|error| io_artifact_error("create temporary inbox output", &tmp_path, error))?;
    file.write_all(&bytes)
        .map_err(|error| io_artifact_error("write temporary inbox output", &tmp_path, error))?;
    file.sync_all()
        .map_err(|error| io_artifact_error("sync temporary inbox output", &tmp_path, error))?;
    drop(file);
    fs::rename(&tmp_path, path)
        .map_err(|error| io_artifact_error("rename inbox output", path, error))?;
    Ok(())
}

fn finalized_capture_artifact(
    request: &CaptureRequest,
    items: Vec<UnresolvedInboxItem>,
) -> InboxResult<UnresolvedInboxArtifact> {
    validate_request(request)?;
    finalize_artifact(UnresolvedInboxArtifact {
        version: CANON_UNRESOLVED_INBOX_VERSION.to_string(),
        view: request.view,
        artifact_content_hash: String::new(),
        policy: request.policy.clone(),
        summary: Default::default(),
        items,
    })
}

fn capture_bucket(
    artifact: &Value,
    bucket: &str,
    request: &CaptureRequest,
    policy: CaptureBucketPolicy,
    items: &mut Vec<UnresolvedInboxItem>,
) -> InboxResult<()> {
    for (index, entry) in optional_array(artifact, bucket)?.iter().enumerate() {
        let reason = reason_text(entry);
        let reason_code = if reason.is_empty() {
            policy.default_reason_code
        } else {
            outcome_reason_code(&reason, policy.default_reason_code)
        };
        let status = match reason_code {
            InboxReasonCode::AmbiguousCandidates => CandidateStatus::Ambiguous,
            InboxReasonCode::BudgetExceeded => CandidateStatus::BudgetLimited,
            InboxReasonCode::CannotLink => CandidateStatus::Rejected,
            _ => policy.default_status,
        };

        items.push(capture_item(
            request,
            CaptureItemSeed {
                event_kind: policy.event_kind,
                reason_code,
                field_name: field_name(entry, request),
                field_role: request.context.field_role,
                surface_role: surface_role(request.context.field_role),
                surface_seed: surface_seed(entry, bucket, index),
                record_ref: record_ref(entry, bucket, index),
                candidate_summary: CandidateSummary {
                    status,
                    candidate_count: candidate_count(entry, status)
                        .max(policy.default_candidate_count),
                    rejection_reasons: rejection_reasons(entry, &reason),
                    ..CandidateSummary::default()
                },
                namespace_hints: namespace_hints_from_entry(entry),
                profile_ref: request.context.profile_ref.clone(),
                privacy_class: request.context.privacy_class,
            },
        )?);
    }
    Ok(())
}

fn capture_review_items(
    artifact: &Value,
    request: &CaptureRequest,
    default_kind: InboxEventKind,
    items: &mut Vec<UnresolvedInboxItem>,
) -> InboxResult<()> {
    for (index, entry) in optional_array(artifact, "items")?.iter().enumerate() {
        let state = string_field(entry, &["state", "status", "outcome"]).unwrap_or_default();
        let reason = reason_text(entry);
        let combined = format!("{state} {reason}");
        let Some((reason_code, status)) = review_reason(&combined) else {
            continue;
        };
        let event_kind = if status == CandidateStatus::Rejected {
            InboxEventKind::CandidateRejected
        } else {
            default_kind
        };

        items.push(capture_item(
            request,
            CaptureItemSeed {
                event_kind,
                reason_code,
                field_name: field_name(entry, request),
                field_role: request.context.field_role,
                surface_role: surface_role(request.context.field_role),
                surface_seed: surface_seed(entry, "review_item", index),
                record_ref: record_ref(entry, "review_item", index),
                candidate_summary: CandidateSummary {
                    status,
                    candidate_count: candidate_count(entry, status),
                    rejection_reasons: rejection_reasons(entry, &combined),
                    ..CandidateSummary::default()
                },
                namespace_hints: namespace_hints_from_entry(entry),
                profile_ref: request.context.profile_ref.clone(),
                privacy_class: request.context.privacy_class,
            },
        )?);
    }
    Ok(())
}

fn capture_item(
    request: &CaptureRequest,
    seed: CaptureItemSeed,
) -> InboxResult<UnresolvedInboxItem> {
    let context = &request.context;
    let source_artifact_hash =
        normalized_hash(&context.source_artifact_hash, "source_artifact_hash")?;
    let mut namespace_hints = context.namespace_hints.clone();
    namespace_hints.extend(seed.namespace_hints);
    namespace_hints.push(NamespaceHint {
        namespace: SOURCE_ARTIFACT_HASH_NAMESPACE.to_string(),
        source: source_artifact_hash,
    });

    let fingerprint_seed = json!({
        "field_name": seed.field_name,
        "field_role": enum_name(seed.field_role)?,
        "normalizer_id": context.normalizer_id,
        "surface_role": seed.surface_role,
        "surface": seed.surface_seed,
    });

    Ok(UnresolvedInboxItem {
        event_key: String::new(),
        event_kind: seed.event_kind,
        reason_code: seed.reason_code,
        field_name: seed.field_name,
        field_role: seed.field_role,
        profile_ref: seed.profile_ref,
        surface_fingerprints: vec![NormalizedSurfaceFingerprint {
            normalizer_id: context.normalizer_id.clone(),
            surface_role: seed.surface_role,
            fingerprint: hash_json(&fingerprint_seed)?,
        }],
        namespace_hints,
        candidate_summary: seed.candidate_summary,
        temporal_scope: None,
        first_seen_at: String::new(),
        last_seen_at: String::new(),
        occurrence_summary: Default::default(),
        occurrences: vec![InboxOccurrenceRef {
            project_ref: context.project_ref.clone(),
            run_ref: context.run_ref.clone(),
            source_ref: context.source_ref.clone(),
            record_ref: seed.record_ref,
            seen_at: context.seen_at.clone(),
        }],
        privacy_class: seed.privacy_class,
        raw_values_redacted: false,
        raw_values: Vec::new(),
    })
}

fn validate_request(request: &CaptureRequest) -> InboxResult<()> {
    let context = &request.context;
    for (field, value) in [
        ("project_ref", context.project_ref.as_str()),
        ("run_ref", context.run_ref.as_str()),
        ("source_ref", context.source_ref.as_str()),
        ("seen_at", context.seen_at.as_str()),
        ("field_name", context.field_name.as_str()),
        ("normalizer_id", context.normalizer_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(artifact_contract_error(format!(
                "capture context requires non-empty {field}"
            )));
        }
    }
    normalized_hash(&context.source_artifact_hash, "source_artifact_hash")?;
    Ok(())
}

fn require_version(artifact: &Value, expected: &str) -> InboxResult<()> {
    let version = required_string(artifact, "version")?;
    if version == expected {
        Ok(())
    } else {
        Err(artifact_contract_error(format!(
            "expected upstream artifact version {expected}, got {version}"
        )))
    }
}

fn required_string<'a>(value: &'a Value, field: &str) -> InboxResult<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| artifact_contract_error(format!("upstream artifact requires {field}")))
}

fn optional_array<'a>(value: &'a Value, field: &str) -> InboxResult<&'a [Value]> {
    match value.get(field) {
        Some(Value::Array(values)) => Ok(values.as_slice()),
        Some(_) => Err(artifact_contract_error(format!(
            "upstream artifact field {field} must be an array"
        ))),
        None => Ok(&[]),
    }
}

fn exact_reason_code(reason: &str) -> InboxResult<InboxReasonCode> {
    match reason {
        "no_matching_rule" => Ok(InboxReasonCode::NoMatchingRule),
        "empty_value" => Ok(InboxReasonCode::EmptyValue),
        "missing_field" => Ok(InboxReasonCode::MissingField),
        "null_value" => Ok(InboxReasonCode::NullValue),
        "non_scalar_value" => Ok(InboxReasonCode::NonScalarValue),
        _ => Err(artifact_contract_error(format!(
            "unsupported exact unresolved reason for inbox capture: {reason}"
        ))),
    }
}

fn outcome_reason_code(reason: &str, fallback: InboxReasonCode) -> InboxReasonCode {
    let normalized = reason.to_ascii_lowercase();
    if normalized.contains("ambiguous") {
        InboxReasonCode::AmbiguousCandidates
    } else if normalized.contains("budget") {
        InboxReasonCode::BudgetExceeded
    } else if normalized.contains("cannot_link")
        || normalized.contains("cannot link")
        || normalized.contains("conflict")
        || normalized.contains("contradict")
    {
        InboxReasonCode::CannotLink
    } else if normalized.contains("score")
        || normalized.contains("threshold")
        || normalized.contains("review")
        || normalized.contains("deferred")
        || normalized.contains("escrow")
    {
        InboxReasonCode::ScoreBelowThreshold
    } else if normalized.contains("empty_value") {
        InboxReasonCode::EmptyValue
    } else if normalized.contains("missing_field") {
        InboxReasonCode::MissingField
    } else if normalized.contains("null_value") {
        InboxReasonCode::NullValue
    } else if normalized.contains("non_scalar_value") {
        InboxReasonCode::NonScalarValue
    } else if normalized.contains("no_matching_rule")
        || normalized.contains("no match")
        || normalized.contains("unresolved")
    {
        InboxReasonCode::NoMatchingRule
    } else {
        fallback
    }
}

fn project_failure_reason(failure_code: &str) -> Option<(InboxReasonCode, CandidateStatus)> {
    let normalized = failure_code.to_ascii_uppercase();
    if normalized.contains("BUDGET") {
        Some((
            InboxReasonCode::BudgetExceeded,
            CandidateStatus::BudgetLimited,
        ))
    } else if normalized.contains("AMBIGUOUS") {
        Some((
            InboxReasonCode::AmbiguousCandidates,
            CandidateStatus::Ambiguous,
        ))
    } else if normalized.contains("CANNOT_LINK")
        || normalized.contains("CONFLICT")
        || normalized.contains("CONTRADICTION")
    {
        Some((InboxReasonCode::CannotLink, CandidateStatus::Rejected))
    } else if normalized.contains("APPLY_UNRESOLVED")
        || normalized.contains("NO_MATCH")
        || normalized == "UNRESOLVED"
    {
        Some((InboxReasonCode::NoMatchingRule, CandidateStatus::None))
    } else {
        None
    }
}

fn review_reason(value: &str) -> Option<(InboxReasonCode, CandidateStatus)> {
    let reason_code = outcome_reason_code(value, InboxReasonCode::NoMatchingRule);
    match reason_code {
        InboxReasonCode::AmbiguousCandidates => Some((reason_code, CandidateStatus::Ambiguous)),
        InboxReasonCode::BudgetExceeded => Some((reason_code, CandidateStatus::BudgetLimited)),
        InboxReasonCode::CannotLink => Some((reason_code, CandidateStatus::Rejected)),
        InboxReasonCode::ScoreBelowThreshold => Some((reason_code, CandidateStatus::None)),
        InboxReasonCode::NoMatchingRule
            if value.contains("unresolved") || value.contains("no_match") =>
        {
            Some((reason_code, CandidateStatus::None))
        }
        _ => None,
    }
}

fn is_entity_v1_version(version: &str) -> bool {
    version.starts_with("canon_entity_") && version.ends_with(".v1")
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        value
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn reason_text(value: &Value) -> String {
    string_field(
        value,
        &[
            "reason",
            "reason_code",
            "failure_code",
            "failure_message",
            "message",
            "error",
            "status",
            "state",
        ],
    )
    .unwrap_or_default()
}

fn field_name(value: &Value, request: &CaptureRequest) -> String {
    string_field(
        value,
        &[
            "field_name",
            "field",
            "left_field",
            "right_field",
            "target_field",
        ],
    )
    .unwrap_or_else(|| request.context.field_name.clone())
}

fn record_ref(value: &Value, bucket: &str, index: usize) -> Option<String> {
    Some(
        string_field(
            value,
            &[
                "record_ref",
                "record_id",
                "row_id",
                "node_id",
                "input_id",
                "surface_id",
                "id",
            ],
        )
        .unwrap_or_else(|| format!("{bucket}:{index}")),
    )
}

fn surface_seed(value: &Value, bucket: &str, index: usize) -> Value {
    json!({
        "bucket": bucket,
        "index": index,
        "surface": string_field(
            value,
            &[
                "input",
                "value",
                "surface",
                "name",
                "observed_name",
                "identifier",
                "node_id",
                "id",
            ],
        ),
    })
}

fn namespace_hints_from_entry(value: &Value) -> Vec<NamespaceHint> {
    let mut hints = Vec::new();
    for (field, namespace) in [
        ("namespace", "namespace"),
        ("namespace_hint", "namespace_hint"),
        ("provider", "provider"),
        ("source_namespace", "source_namespace"),
    ] {
        if let Some(source) = string_field(value, &[field]) {
            hints.push(NamespaceHint {
                namespace: namespace.to_string(),
                source,
            });
        }
    }
    hints
}

fn candidate_count(value: &Value, status: CandidateStatus) -> u32 {
    if let Some(count) = value
        .get("candidate_count")
        .or_else(|| value.get("candidates_count"))
        .and_then(Value::as_u64)
    {
        return count.min(u32::MAX as u64) as u32;
    }
    for field in ["candidates", "candidate_ids", "matches"] {
        if let Some(Value::Array(values)) = value.get(field) {
            return values.len().min(u32::MAX as usize) as u32;
        }
    }
    match status {
        CandidateStatus::Ambiguous => 2,
        _ => 0,
    }
}

fn rejection_reasons(value: &Value, fallback: &str) -> Vec<String> {
    let mut reasons = Vec::new();
    if let Some(Value::Array(values)) = value.get("rejection_reasons") {
        reasons.extend(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        );
    }
    if !fallback.trim().is_empty() {
        reasons.push(fallback.trim().to_string());
    }
    reasons
}

fn surface_role(role: InboxFieldRole) -> String {
    match role {
        InboxFieldRole::LookupInput => "lookup_input",
        InboxFieldRole::NameField => "name_field",
        InboxFieldRole::AnchorField => "anchor_field",
        InboxFieldRole::ContextField => "context_field",
        InboxFieldRole::CandidatePair => "candidate_pair",
    }
    .to_string()
}

fn enum_name(value: impl Serialize) -> InboxResult<String> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|error| artifact_contract_error(format!("failed to encode enum name: {error}")))
}

fn hash_json(value: &Value) -> InboxResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .map_err(|error| {
            artifact_contract_error(format!("failed to hash capture surface: {error}"))
        })
}

fn normalized_hash(value: &str, field: &str) -> InboxResult<String> {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("blake3:") else {
        return Err(artifact_contract_error(format!(
            "{field} must be a blake3: hex digest"
        )));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(artifact_contract_error(format!(
            "{field} must be a 64-character blake3: hex digest"
        )));
    }
    Ok(format!("blake3:{}", hex.to_ascii_lowercase()))
}

fn temp_path(path: &Path, bytes: &[u8]) -> InboxResult<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            artifact_contract_error(format!(
                "inbox capture output path requires a file name: {}",
                path.display()
            ))
        })?;
    let digest = blake3::hash(bytes).to_hex();
    Ok(path.with_file_name(format!(".{file_name}.{}.tmp", &digest[..16])))
}

fn artifact_contract_error(message: impl Into<String>) -> InboxError {
    InboxError::new(InboxErrorCode::ArtifactContract, message)
}

fn io_artifact_error(action: &str, path: &Path, error: impl std::fmt::Display) -> InboxError {
    artifact_contract_error(format!("{action} at {} failed: {error}", path.display()))
}
