#![forbid(unsafe_code)]

pub mod types;

pub use types::*;

use serde::Serialize;
use std::{collections::BTreeMap, error::Error, fmt};

pub type EvidenceResult<T> = Result<T, EvidenceError>;

pub const MAX_TEXT_BYTES: usize = 4 * 1024;
pub const MAX_MEASUREMENTS_PER_RECORD: usize = 32;
pub const MAX_EXTENSIONS_PER_RECORD: usize = 16;
pub const MAX_EXTENSION_NODES: usize = 128;
pub const MAX_PROVENANCE_REFS_PER_RECORD: usize = 16;

const RESERVED_EXTENSION_KEYS: &[&str] = &[
    "authority",
    "authority_basis",
    "decision",
    "promotion",
    "promotion_authority",
    "solver_verdict",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceErrorCode {
    ArtifactContract,
    UnsupportedTarget,
    DuplicateRecord,
    OversizedPayload,
    InvalidExtension,
    #[default]
    Unimplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceError {
    pub code: EvidenceErrorCode,
    pub message: String,
}

impl EvidenceError {
    pub fn new(code: EvidenceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for EvidenceError {}

pub fn canonicalize_record(mut record: EvidenceRecord) -> EvidenceResult<EvidenceRecord> {
    if record.version.trim().is_empty() {
        record.version = CANON_EVIDENCE_VERSION.to_string();
    }
    require_eq(
        &record.version,
        CANON_EVIDENCE_VERSION,
        "record.version",
        "evidence records must declare canon.evidence.v1",
    )?;

    canonicalize_target(&mut record.target)?;
    canonicalize_operator(&record.operator)?;
    canonicalize_policy(&record.policy)?;
    canonicalize_reason_code(&record.reason_code)?;
    canonicalize_scope(record.scope.as_ref())?;
    canonicalize_temporal_scope(record.temporal_scope.as_ref())?;
    canonicalize_provenance(&mut record.provenance)?;
    canonicalize_measurements(&mut record.measurements)?;
    canonicalize_extensions(&mut record.extensions)?;
    validate_kind_target_contract(
        record.kind.clone(),
        &record.target,
        record.authority_basis.as_ref(),
    )?;

    let computed = derive_evidence_id(&record)?;
    if record.evidence_id.trim().is_empty() {
        record.evidence_id = computed;
    } else if record.evidence_id != computed {
        return Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!(
                "record evidence_id does not match canonical bytes: expected {}, got {}",
                computed, record.evidence_id
            ),
        ));
    }

    Ok(record)
}

pub fn derive_evidence_id(record: &EvidenceRecord) -> EvidenceResult<String> {
    let mut hashable = record.clone();
    hashable.evidence_id.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("failed to serialize evidence record for hashing: {error}"),
        )
    })?;
    Ok(format!("evidence:blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn merge_shards<I>(shards: I) -> EvidenceResult<EvidenceBundle>
where
    I: IntoIterator<Item = Vec<EvidenceRecord>>,
{
    canonicalize_bundle(shards.into_iter().flatten())
}

pub fn canonicalize_bundle<I>(records: I) -> EvidenceResult<EvidenceBundle>
where
    I: IntoIterator<Item = EvidenceRecord>,
{
    let mut canonical_records = records
        .into_iter()
        .map(canonicalize_record)
        .collect::<EvidenceResult<Vec<_>>>()?;
    canonical_records.sort_by_key(|record| record.evidence_id.clone());

    let mut deduped = Vec::new();
    let mut seen = BTreeMap::new();
    for record in canonical_records {
        let bytes = serde_json::to_vec(&record).map_err(|error| {
            EvidenceError::new(
                EvidenceErrorCode::ArtifactContract,
                format!("failed to serialize canonical evidence record: {error}"),
            )
        })?;
        match seen.entry(record.evidence_id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(bytes);
                deduped.push(record);
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                if entry.get() != &bytes {
                    return Err(EvidenceError::new(
                        EvidenceErrorCode::DuplicateRecord,
                        format!(
                            "duplicate evidence_id {} has non-identical bytes",
                            record.evidence_id
                        ),
                    ));
                }
            }
        }
    }

    let mut bundle = EvidenceBundle {
        version: CANON_EVIDENCE_VERSION.to_string(),
        record_count: deduped.len() as u64,
        content_hash: String::new(),
        records: deduped,
    };
    bundle.content_hash = compute_bundle_hash(&bundle)?;
    Ok(bundle)
}

pub fn canonical_bundle_bytes(bundle: &EvidenceBundle) -> EvidenceResult<Vec<u8>> {
    let canonical = bundle.clone();
    require_eq(
        &canonical.version,
        CANON_EVIDENCE_VERSION,
        "bundle.version",
        "evidence bundles must declare canon.evidence.v1",
    )?;
    if canonical.record_count != canonical.records.len() as u64 {
        return Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!(
                "bundle.record_count must match records.len(): expected {}, got {}",
                canonical.records.len(),
                canonical.record_count
            ),
        ));
    }
    for window in canonical.records.windows(2) {
        if window[0].evidence_id >= window[1].evidence_id {
            return Err(EvidenceError::new(
                EvidenceErrorCode::ArtifactContract,
                "bundle.records must be in strict evidence_id order",
            ));
        }
    }
    let expected_hash = compute_bundle_hash(&canonical)?;
    require_eq(
        &canonical.content_hash,
        &expected_hash,
        "bundle.content_hash",
        "bundle.content_hash must match canonical bytes",
    )?;
    serde_json::to_vec(&canonical).map_err(|error| {
        EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("failed to serialize evidence bundle: {error}"),
        )
    })
}

fn compute_bundle_hash(bundle: &EvidenceBundle) -> EvidenceResult<String> {
    let mut hashable = bundle.clone();
    hashable.content_hash.clear();
    let bytes = serde_json::to_vec(&hashable).map_err(|error| {
        EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("failed to serialize evidence bundle for hashing: {error}"),
        )
    })?;
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

fn canonicalize_target(target: &mut EvidenceTarget) -> EvidenceResult<()> {
    match target {
        EvidenceTarget::Observation {
            observation_id,
            surface,
            subject_hint,
        } => {
            require_non_empty(observation_id, "target.observation_id")?;
            require_non_empty(surface, "target.surface")?;
            if let Some(subject_hint) = subject_hint {
                require_max_text(subject_hint, "target.subject_hint")?;
            }
        }
        EvidenceTarget::CandidateScope {
            scope_id,
            candidate_ids,
        } => {
            require_non_empty(scope_id, "target.scope_id")?;
            sort_and_dedup_strings(candidate_ids);
            require_non_empty_slice(candidate_ids, "target.candidate_ids")?;
        }
        EvidenceTarget::Pair { left_id, right_id } => {
            require_non_empty(left_id, "target.left_id")?;
            require_non_empty(right_id, "target.right_id")?;
            if left_id > right_id {
                std::mem::swap(left_id, right_id);
            }
            if left_id == right_id {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::UnsupportedTarget,
                    "pair target requires two distinct ids",
                ));
            }
        }
        EvidenceTarget::Hyperedge { member_ids } => {
            sort_and_dedup_strings(member_ids);
            require_non_empty_slice(member_ids, "target.member_ids")?;
            if member_ids.len() < 2 {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::UnsupportedTarget,
                    "hyperedge target requires at least two member ids",
                ));
            }
        }
        EvidenceTarget::RecordLink {
            left_source,
            left_record_id,
            right_source,
            right_record_id,
        } => {
            require_non_empty(left_source, "target.left_source")?;
            require_non_empty(left_record_id, "target.left_record_id")?;
            require_non_empty(right_source, "target.right_source")?;
            require_non_empty(right_record_id, "target.right_record_id")?;
            let left = (left_source.clone(), left_record_id.clone());
            let right = (right_source.clone(), right_record_id.clone());
            if left > right {
                std::mem::swap(left_source, right_source);
                std::mem::swap(left_record_id, right_record_id);
            }
            if left_source == right_source && left_record_id == right_record_id {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::UnsupportedTarget,
                    "record_link target requires two distinct record refs",
                ));
            }
        }
    }
    Ok(())
}

fn canonicalize_operator(operator: &EvidenceOperatorRef) -> EvidenceResult<()> {
    require_non_empty(&operator.namespace, "operator.namespace")?;
    require_non_empty(&operator.operator_id, "operator.operator_id")?;
    require_non_empty(&operator.operator_version, "operator.operator_version")?;
    if let Some(adapter_id) = &operator.adapter_id {
        require_max_text(adapter_id, "operator.adapter_id")?;
    }
    Ok(())
}

fn canonicalize_policy(policy: &EvidencePolicyRef) -> EvidenceResult<()> {
    require_non_empty(&policy.policy_id, "policy.policy_id")?;
    require_non_empty(&policy.policy_version, "policy.policy_version")?;
    require_blake3_hash(&policy.content_hash, "policy.content_hash")
}

fn canonicalize_reason_code(reason_code: &str) -> EvidenceResult<()> {
    require_non_empty(reason_code, "reason_code")?;
    require_max_text(reason_code, "reason_code")
}

fn canonicalize_scope(scope: Option<&EvidenceScope>) -> EvidenceResult<()> {
    if let Some(scope) = scope {
        require_non_empty(&scope.scope_type, "scope.scope_type")?;
        require_non_empty(&scope.scope_id, "scope.scope_id")?;
        if let Some(namespace) = &scope.namespace {
            require_max_text(namespace, "scope.namespace")?;
        }
    }
    Ok(())
}

fn canonicalize_temporal_scope(scope: Option<&EvidenceTemporalScope>) -> EvidenceResult<()> {
    if let Some(scope) = scope {
        for (value, field) in [
            (scope.as_of.as_deref(), "temporal_scope.as_of"),
            (scope.start_at.as_deref(), "temporal_scope.start_at"),
            (scope.end_at.as_deref(), "temporal_scope.end_at"),
        ] {
            if let Some(value) = value {
                require_max_text(value, field)?;
            }
        }
    }
    Ok(())
}

fn canonicalize_provenance(provenance: &mut Vec<EvidenceProvenanceRef>) -> EvidenceResult<()> {
    if provenance.is_empty() {
        return Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            "every evidence record requires at least one provenance reference",
        ));
    }
    if provenance.len() > MAX_PROVENANCE_REFS_PER_RECORD {
        return Err(EvidenceError::new(
            EvidenceErrorCode::OversizedPayload,
            format!(
                "record provenance exceeds max refs: {} > {}",
                provenance.len(),
                MAX_PROVENANCE_REFS_PER_RECORD
            ),
        ));
    }
    provenance.sort_by_key(provenance_sort_key);
    for entry in provenance {
        require_non_empty(&entry.source_type, "provenance.source_type")?;
        require_non_empty(&entry.source_id, "provenance.source_id")?;
        require_non_empty(&entry.locator, "provenance.locator")?;
        require_blake3_hash(&entry.content_hash, "provenance.content_hash")?;
        if let Some(observed_at) = &entry.observed_at {
            require_max_text(observed_at, "provenance.observed_at")?;
        }
    }
    Ok(())
}

fn canonicalize_measurements(measurements: &mut Vec<EvidenceMeasurement>) -> EvidenceResult<()> {
    if measurements.len() > MAX_MEASUREMENTS_PER_RECORD {
        return Err(EvidenceError::new(
            EvidenceErrorCode::OversizedPayload,
            format!(
                "record measurements exceed max count: {} > {}",
                measurements.len(),
                MAX_MEASUREMENTS_PER_RECORD
            ),
        ));
    }
    measurements.sort_by_key(measurement_sort_key);

    let mut feature_ids = BTreeMap::new();
    for measurement in measurements {
        match measurement {
            EvidenceMeasurement::Numeric(measurement) => {
                require_non_empty(&measurement.feature_id, "measurement.feature_id")?;
                require_non_empty(&measurement.units, "measurement.units")?;
                if feature_ids
                    .insert(measurement.feature_id.clone(), ())
                    .is_some()
                {
                    return Err(EvidenceError::new(
                        EvidenceErrorCode::ArtifactContract,
                        format!(
                            "duplicate measurement feature_id {}",
                            measurement.feature_id
                        ),
                    ));
                }
            }
            EvidenceMeasurement::Categorical(measurement) => {
                require_non_empty(&measurement.feature_id, "measurement.feature_id")?;
                require_non_empty(&measurement.value, "measurement.value")?;
                if feature_ids
                    .insert(measurement.feature_id.clone(), ())
                    .is_some()
                {
                    return Err(EvidenceError::new(
                        EvidenceErrorCode::ArtifactContract,
                        format!(
                            "duplicate measurement feature_id {}",
                            measurement.feature_id
                        ),
                    ));
                }
            }
            EvidenceMeasurement::Boolean(measurement) => {
                require_non_empty(&measurement.feature_id, "measurement.feature_id")?;
                if feature_ids
                    .insert(measurement.feature_id.clone(), ())
                    .is_some()
                {
                    return Err(EvidenceError::new(
                        EvidenceErrorCode::ArtifactContract,
                        format!(
                            "duplicate measurement feature_id {}",
                            measurement.feature_id
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn canonicalize_extensions(extensions: &mut Vec<EvidenceExtension>) -> EvidenceResult<()> {
    if extensions.len() > MAX_EXTENSIONS_PER_RECORD {
        return Err(EvidenceError::new(
            EvidenceErrorCode::OversizedPayload,
            format!(
                "record extensions exceed max count: {} > {}",
                extensions.len(),
                MAX_EXTENSIONS_PER_RECORD
            ),
        ));
    }
    extensions.sort_by_key(extension_sort_key);

    let mut seen = BTreeMap::new();
    for extension in extensions {
        require_non_empty(&extension.namespace, "extension.namespace")?;
        require_non_empty(&extension.schema_ref, "extension.schema_ref")?;
        if seen
            .insert(
                (extension.namespace.clone(), extension.schema_ref.clone()),
                (),
            )
            .is_some()
        {
            return Err(EvidenceError::new(
                EvidenceErrorCode::InvalidExtension,
                format!(
                    "duplicate extension {} {}",
                    extension.namespace, extension.schema_ref
                ),
            ));
        }
        if extension.payload.is_empty() {
            return Err(EvidenceError::new(
                EvidenceErrorCode::InvalidExtension,
                format!(
                    "extension {} {} must carry at least one payload field",
                    extension.namespace, extension.schema_ref
                ),
            ));
        }
        let mut nodes = 0usize;
        for (key, value) in &extension.payload {
            require_non_empty(key, "extension.payload.key")?;
            if RESERVED_EXTENSION_KEYS.contains(&key.as_str()) {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::InvalidExtension,
                    format!("extension payload key {key} is reserved for authority or decisions"),
                ));
            }
            nodes += count_extension_nodes(value)?;
        }
        if nodes > MAX_EXTENSION_NODES {
            return Err(EvidenceError::new(
                EvidenceErrorCode::OversizedPayload,
                format!(
                    "extension {} {} exceeds node limit: {} > {}",
                    extension.namespace, extension.schema_ref, nodes, MAX_EXTENSION_NODES
                ),
            ));
        }
    }
    Ok(())
}

fn validate_kind_target_contract(
    kind: EvidenceKind,
    target: &EvidenceTarget,
    authority_basis: Option<&EvidenceAuthorityBasis>,
) -> EvidenceResult<()> {
    match kind {
        EvidenceKind::Observation => require_target(target, "observation")?,
        EvidenceKind::CandidateScope => require_target(target, "candidate_scope")?,
        EvidenceKind::PairSupport => require_target(target, "pair")?,
        EvidenceKind::HyperedgeSupport => require_target(target, "hyperedge")?,
        EvidenceKind::RecordLinkSupport => require_target(target, "record_link")?,
        EvidenceKind::AntiMergeVeto => {
            if authority_basis.is_none() {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::ArtifactContract,
                    "anti_merge_veto evidence requires authority_basis",
                ));
            }
        }
        EvidenceKind::ContextOnly
        | EvidenceKind::ContextualNegative
        | EvidenceKind::Missingness => {
            if authority_basis.is_some() {
                return Err(EvidenceError::new(
                    EvidenceErrorCode::ArtifactContract,
                    "only anti_merge_veto evidence may carry authority_basis",
                ));
            }
        }
    }

    if kind != EvidenceKind::AntiMergeVeto && authority_basis.is_some() {
        return Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            "only anti_merge_veto evidence may carry authority_basis",
        ));
    }
    Ok(())
}

fn require_target(target: &EvidenceTarget, expected: &str) -> EvidenceResult<()> {
    let actual = match target {
        EvidenceTarget::Observation { .. } => "observation",
        EvidenceTarget::CandidateScope { .. } => "candidate_scope",
        EvidenceTarget::Pair { .. } => "pair",
        EvidenceTarget::Hyperedge { .. } => "hyperedge",
        EvidenceTarget::RecordLink { .. } => "record_link",
    };
    if actual == expected {
        Ok(())
    } else {
        Err(EvidenceError::new(
            EvidenceErrorCode::UnsupportedTarget,
            format!("evidence kind requires target {expected}, got {actual}"),
        ))
    }
}

fn provenance_sort_key(provenance: &EvidenceProvenanceRef) -> (String, String, String, String) {
    (
        provenance.source_type.clone(),
        provenance.source_id.clone(),
        provenance.locator.clone(),
        provenance.content_hash.clone(),
    )
}

fn measurement_sort_key(measurement: &EvidenceMeasurement) -> (u8, String) {
    match measurement {
        EvidenceMeasurement::Boolean(measurement) => (0, measurement.feature_id.clone()),
        EvidenceMeasurement::Categorical(measurement) => (1, measurement.feature_id.clone()),
        EvidenceMeasurement::Numeric(measurement) => (2, measurement.feature_id.clone()),
    }
}

fn extension_sort_key(extension: &EvidenceExtension) -> (String, String) {
    (extension.namespace.clone(), extension.schema_ref.clone())
}

fn count_extension_nodes(value: &EvidenceExtensionValue) -> EvidenceResult<usize> {
    match value {
        EvidenceExtensionValue::Bool(_) => Ok(1),
        EvidenceExtensionValue::Int(_) => Ok(1),
        EvidenceExtensionValue::UInt(_) => Ok(1),
        EvidenceExtensionValue::String(value) => {
            require_max_text(value, "extension.payload.value")?;
            Ok(1)
        }
        EvidenceExtensionValue::List(values) => values.iter().try_fold(1usize, |count, value| {
            Ok(count + count_extension_nodes(value)?)
        }),
        EvidenceExtensionValue::Object(values) => {
            values.iter().try_fold(1usize, |count, (key, value)| {
                require_non_empty(key, "extension.payload.object.key")?;
                if RESERVED_EXTENSION_KEYS.contains(&key.as_str()) {
                    return Err(EvidenceError::new(
                        EvidenceErrorCode::InvalidExtension,
                        format!(
                            "extension payload key {key} is reserved for authority or decisions"
                        ),
                    ));
                }
                Ok(count + count_extension_nodes(value)?)
            })
        }
    }
}

fn sort_and_dedup_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn require_non_empty(value: &str, field: &str) -> EvidenceResult<()> {
    if value.trim().is_empty() {
        Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("{field} must be non-empty"),
        ))
    } else {
        require_max_text(value, field)
    }
}

fn require_non_empty_slice<T>(slice: &[T], field: &str) -> EvidenceResult<()> {
    if slice.is_empty() {
        Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("{field} must be non-empty"),
        ))
    } else {
        Ok(())
    }
}

fn require_eq(actual: &str, expected: &str, field: &str, message: &str) -> EvidenceResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("{message}: {field} expected {expected}, got {actual}"),
        ))
    }
}

fn require_blake3_hash(value: &str, field: &str) -> EvidenceResult<()> {
    require_non_empty(value, field)?;
    if value.starts_with("blake3:") && value.len() > "blake3:".len() {
        Ok(())
    } else {
        Err(EvidenceError::new(
            EvidenceErrorCode::ArtifactContract,
            format!("{field} must use a blake3: digest"),
        ))
    }
}

fn require_max_text(value: &str, field: &str) -> EvidenceResult<()> {
    if value.len() > MAX_TEXT_BYTES {
        Err(EvidenceError::new(
            EvidenceErrorCode::OversizedPayload,
            format!(
                "{field} exceeds max text bytes: {} > {}",
                value.len(),
                MAX_TEXT_BYTES
            ),
        ))
    } else {
        Ok(())
    }
}
